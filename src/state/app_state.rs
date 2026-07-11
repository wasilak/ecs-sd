use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime};

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::aws::DiscoveryService;
use crate::config::{Config, Mode};
use crate::error::DiscoveryError;
use crate::models::{build_routing_table, filter_labels_by_level, MetadataLevel, ProxyTarget, Target};

#[derive(Clone)]
pub struct CacheSnapshot {
    pub cache: HashMap<MetadataLevel, Vec<Target>>,
    pub last_refresh: SystemTime,
    pub routing_table: HashMap<Uuid, ProxyTarget>,
}

impl Default for CacheSnapshot {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            last_refresh: SystemTime::UNIX_EPOCH,
            routing_table: HashMap::new(),
        }
    }
}

fn build_snapshot(targets_aws: Vec<Target>, mode: Mode) -> CacheSnapshot {
    let mut cache = HashMap::new();
    cache.insert(MetadataLevel::Aws, targets_aws.clone());
    cache.insert(
        MetadataLevel::Cluster,
        targets_aws.iter().map(|t| filter_labels_by_level(t, MetadataLevel::Cluster)).collect(),
    );
    cache.insert(
        MetadataLevel::Service,
        targets_aws.iter().map(|t| filter_labels_by_level(t, MetadataLevel::Service)).collect(),
    );
    cache.insert(
        MetadataLevel::Task,
        targets_aws.iter().map(|t| filter_labels_by_level(t, MetadataLevel::Task)).collect(),
    );
    cache.insert(
        MetadataLevel::Container,
        targets_aws.iter().map(|t| filter_labels_by_level(t, MetadataLevel::Container)).collect(),
    );

    let routing_table = if mode == Mode::Proxy {
        build_routing_table(&targets_aws)
    } else {
        HashMap::new()
    };

    CacheSnapshot {
        cache,
        last_refresh: SystemTime::now(),
        routing_table,
    }
}

pub(crate) fn per_cluster_target_counts(targets: &[Target]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for target in targets {
        let cluster = target
            .labels
            .get("__meta_ecs_cluster_name")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        *counts.entry(cluster).or_insert(0) += target.targets.len();
    }
    counts
}

pub(crate) fn record_per_cluster_target_counts(
    metrics: &crate::metrics::MetricsState,
    configured_clusters: &[String],
    old_targets: &[Target],
    new_targets: &[Target],
) {
    let old_counts = per_cluster_target_counts(old_targets);
    let new_counts = per_cluster_target_counts(new_targets);
    let clusters_to_reset: HashSet<&str> = old_counts
        .keys()
        .map(String::as_str)
        .chain(configured_clusters.iter().map(String::as_str))
        .collect();

    for cluster in clusters_to_reset {
        metrics
            .discovery_targets_per_cluster
            .with_label_values(&[cluster])
            .set(0.0);
    }

    for (cluster, count) in new_counts {
        metrics
            .discovery_targets_per_cluster
            .with_label_values(&[&cluster])
            .set(count as f64);
    }
}

pub(crate) fn target_address_churn(old: &HashSet<String>, new: &HashSet<String>) -> (usize, usize) {
    (new.difference(old).count(), old.difference(new).count())
}

#[derive(Clone)]
pub struct RefreshOutcome {
    pub success: bool,
    pub timestamp_unix: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub snapshot: Arc<RwLock<CacheSnapshot>>,
    pub cache_ttl_seconds: u64,
    pub started_at: std::time::Instant,
    pub last_refresh_outcome: Arc<RwLock<Option<RefreshOutcome>>>,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
    pub http_client: reqwest::Client,
    pub cluster: Option<Arc<crate::cluster::ClusterState>>,
    pub metrics: Arc<crate::metrics::MetricsState>,
    pub last_manual_refresh_request: Arc<AtomicU64>,
}

impl AppState {
    pub async fn new(
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,
        region: String,
        cluster: Option<Arc<crate::cluster::ClusterState>>,
        metrics: Arc<crate::metrics::MetricsState>,
    ) -> Result<Self, DiscoveryError> {
        let discovery = DiscoveryService::new(
            ecs_client,
            ec2_client,
            sts_client,
            region,
            Arc::clone(&metrics),
        )
        .await?;
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .tcp_keepalive(Duration::from_secs(10))
            // No client-level timeout: timeout set per-request from X-Prometheus-Scrape-Timeout-Seconds
            .build()
            .expect("failed to build reqwest client");

        Ok(Self {
            snapshot: Arc::new(RwLock::new(CacheSnapshot::default())),
            cache_ttl_seconds: config.refresh_interval.max(1),
            started_at: std::time::Instant::now(),
            last_refresh_outcome: Arc::new(RwLock::new(None)),
            config: Arc::new(config),
            discovery,
            http_client,
            cluster,
            metrics,
            last_manual_refresh_request: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Atomically replace all cache tiers and update last_refresh. In proxy mode,
    /// also rebuilds the routing table. Called from both the background refresh loop
    /// (main.rs) and the manual POST /refresh handler (sd.rs), ensuring PROX-06.
    pub async fn replace_cache_and_routing(&self, targets_aws: Vec<Target>) {
        // Build all values BEFORE acquiring the write lock (avoids deadlock — Pitfall 1)
        let new_snapshot = build_snapshot(targets_aws, self.config.mode.clone());
        // Single atomic write
        let mut snap = self.snapshot.write().await;
        *snap = new_snapshot;
    }

    pub async fn replace_cache_and_record_metrics(&self, targets_aws: Vec<Target>) {
        let (old_addresses, old_targets): (HashSet<String>, Vec<Target>) = {
            let snap = self.snapshot.read().await;
            let old_targets = snap.cache
                .get(&MetadataLevel::Aws)
                .cloned()
                .unwrap_or_default();
            let old_addresses = old_targets
                .iter()
                .flat_map(|target| target.targets.iter().cloned())
                .collect();
            (old_addresses, old_targets)
        };

        record_per_cluster_target_counts(
            &self.metrics,
            &self.config.clusters,
            &old_targets,
            &targets_aws,
        );

        let new_addresses: HashSet<String> = targets_aws
            .iter()
            .flat_map(|target| target.targets.iter().cloned())
            .collect();

        self.replace_cache_and_routing(targets_aws).await;

        let (added, removed) = target_address_churn(&old_addresses, &new_addresses);
        if added > 0 {
            self.metrics
                .discovery_target_churn_total
                .with_label_values(&["added"])
                .inc_by(added as f64);
        }
        if removed > 0 {
            self.metrics
                .discovery_target_churn_total
                .with_label_values(&["removed"])
                .inc_by(removed as f64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_snapshot_produces_consistent_tiers() {
        let targets = vec![
            Target {
                targets: vec!["10.0.0.1:8080".to_string()],
                labels: {
                    let mut m = HashMap::new();
                    m.insert("__meta_ecs_task_arn".to_string(), "arn:aws:ecs:us-east-1:123:task/cluster/task1".to_string());
                    m.insert("__meta_ecs_container_name".to_string(), "app1".to_string());
                    m.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
                    m.insert("__meta_ecs_service_name".to_string(), "api".to_string());
                    m.insert("__meta_ecs_task_family".to_string(), "api-task".to_string());
                    m
                },
            },
            Target {
                targets: vec!["10.0.0.2:8080".to_string()],
                labels: {
                    let mut m = HashMap::new();
                    m.insert("__meta_ecs_task_arn".to_string(), "arn:aws:ecs:us-east-1:123:task/cluster/task2".to_string());
                    m.insert("__meta_ecs_container_name".to_string(), "app2".to_string());
                    m.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
                    m.insert("__meta_ecs_service_name".to_string(), "worker".to_string());
                    m.insert("__meta_ecs_task_family".to_string(), "worker-task".to_string());
                    m
                },
            },
        ];

        let snap = build_snapshot(targets.clone(), Mode::Proxy);

        // Aws tier has all input targets
        assert_eq!(
            snap.cache.get(&MetadataLevel::Aws).map(Vec::len),
            Some(targets.len()),
            "Aws tier must contain all input targets"
        );

        // All tiers are present
        for level in [
            MetadataLevel::Aws,
            MetadataLevel::Cluster,
            MetadataLevel::Service,
            MetadataLevel::Task,
            MetadataLevel::Container,
        ] {
            assert!(snap.cache.contains_key(&level), "tier {:?} must be present", level);
        }

        // Proxy mode: routing_table is non-empty and size matches input
        assert!(!snap.routing_table.is_empty(), "routing_table must be non-empty in proxy mode");
        assert_eq!(
            snap.routing_table.len(),
            targets.len(),
            "routing_table size must match input count"
        );
    }

    #[test]
    fn per_cluster_target_counts_groups_by_cluster_and_counts_addresses() {
        let targets = vec![
            Target {
                targets: vec!["10.0.0.1:8080".to_string()],
                labels: {
                    let mut m = HashMap::new();
                    m.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
                    m
                },
            },
            Target {
                targets: vec!["10.0.0.2:8080".to_string()],
                labels: {
                    let mut m = HashMap::new();
                    m.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
                    m
                },
            },
        ];

        let counts = per_cluster_target_counts(&targets);

        assert_eq!(counts.get("prod"), Some(&2));
    }

    #[test]
    fn per_cluster_target_counts_buckets_missing_cluster_as_unknown() {
        let targets = vec![Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels: HashMap::new(),
        }];

        let counts = per_cluster_target_counts(&targets);

        assert_eq!(counts.get("unknown"), Some(&1));
    }

    #[test]
    fn discovery_targets_per_cluster_resets_dropped_cluster_to_zero() {
        let metrics = crate::metrics::MetricsState::new().unwrap();
        let configured_clusters = vec!["prod".to_string()];
        let old_targets = vec![Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels: {
                let mut m = HashMap::new();
                m.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
                m
            },
        }];
        let new_targets = vec![Target {
            targets: vec!["10.0.0.2:8080".to_string()],
            labels: {
                let mut m = HashMap::new();
                m.insert("__meta_ecs_cluster_name".to_string(), "dev".to_string());
                m
            },
        }];

        record_per_cluster_target_counts(
            &metrics,
            &configured_clusters,
            &old_targets,
            &new_targets,
        );

        let families = metrics.registry.gather();
        let family = families
            .iter()
            .find(|f| f.name() == "ecs_sd_discovery_targets_per_cluster")
            .unwrap();
        let prod_metric = family
            .get_metric()
            .iter()
            .find(|metric| {
                metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "cluster" && label.value() == "prod")
            })
            .unwrap();

        assert_eq!(prod_metric.get_gauge().value(), 0.0);
    }

    #[test]
    fn target_address_churn_counts_added_and_removed_addresses() {
        let old = std::collections::HashSet::from(["a".to_string(), "b".to_string()]);
        let new = std::collections::HashSet::from(["b".to_string(), "c".to_string()]);

        assert_eq!(target_address_churn(&old, &new), (1, 1));
    }

    #[test]
    fn target_address_churn_counts_initial_population_and_no_change() {
        let empty = std::collections::HashSet::new();
        let populated = std::collections::HashSet::from(["a".to_string(), "b".to_string()]);

        assert_eq!(target_address_churn(&empty, &populated), (2, 0));
        assert_eq!(target_address_churn(&populated, &populated), (0, 0));
    }
}
