use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::aws::DiscoveryService;
use crate::config::{Config, Mode};
use crate::error::DiscoveryError;
use crate::models::{build_routing_table, MetadataLevel, ProxyTarget, Target};

use crate::handlers::sd::filter_labels_by_level;

fn migrate_target_label_schema(target: &mut Target) {
    if let Some(cluster) = target.labels.remove("__meta_ecs_cluster") {
        target
            .labels
            .entry("__meta_ecs_cluster_name".to_string())
            .or_insert(cluster);
    }

    if let Some(service) = target.labels.remove("__meta_ecs_service") {
        target
            .labels
            .entry("__meta_ecs_service_name".to_string())
            .or_insert(service);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub last_refresh: Arc<RwLock<SystemTime>>,
    pub cache_ttl_seconds: u64,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
    pub routing_table: Arc<RwLock<HashMap<Uuid, ProxyTarget>>>,
    pub http_client: reqwest::Client,
    pub cluster: Option<Arc<crate::cluster::ClusterState>>,
    pub metrics: Arc<crate::metrics::MetricsState>,
    pub last_manual_refresh_request: Arc<RwLock<SystemTime>>,
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
        let discovery = DiscoveryService::new(ecs_client, ec2_client, sts_client, region).await?;
        let http_client = reqwest::Client::builder()
            // No client-level timeout: timeout set per-request from X-Prometheus-Scrape-Timeout-Seconds
            .build()
            .expect("failed to build reqwest client");

        Ok(Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_refresh: Arc::new(RwLock::new(SystemTime::now())),
            cache_ttl_seconds: config.refresh_interval.max(1),
            config: Arc::new(config),
            discovery,
            routing_table: Arc::new(RwLock::new(HashMap::new())),
            http_client,
            cluster,
            metrics,
            last_manual_refresh_request: Arc::new(RwLock::new(SystemTime::UNIX_EPOCH)),
        })
    }

    /// Atomically replace all cache tiers and update last_refresh. In proxy mode,
    /// also rebuilds the routing table. Called from both the background refresh loop
    /// (main.rs) and the manual POST /refresh handler (sd.rs), ensuring PROX-06.
    pub async fn replace_cache_and_routing(&self, targets_aws: Vec<Target>) {
        let mut targets_aws = targets_aws;
        for target in &mut targets_aws {
            migrate_target_label_schema(target);
        }

        let targets_cluster: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
            .collect();
        let targets_service: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Service))
            .collect();
        let targets_task: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Task))
            .collect();
        let targets_container: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Container))
            .collect();

        {
            let mut cache = self.cache.write().await;
            cache.insert(MetadataLevel::Aws, targets_aws.clone());
            cache.insert(MetadataLevel::Cluster, targets_cluster);
            cache.insert(MetadataLevel::Service, targets_service);
            cache.insert(MetadataLevel::Task, targets_task);
            cache.insert(MetadataLevel::Container, targets_container);
        }

        {
            let mut last_refresh = self.last_refresh.write().await;
            *last_refresh = SystemTime::now();
        }

        if self.config.mode == Mode::Proxy {
            let routing = build_routing_table(&targets_aws);
            let mut rt = self.routing_table.write().await;
            *rt = routing;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_target_label_schema_maps_legacy_keys_to_canonical_names() {
        let mut target = Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels: HashMap::from([
                ("__meta_ecs_cluster".to_string(), "prod".to_string()),
                ("__meta_ecs_service".to_string(), "api".to_string()),
            ]),
        };

        migrate_target_label_schema(&mut target);

        assert_eq!(
            target.labels.get("__meta_ecs_cluster_name").map(String::as_str),
            Some("prod")
        );
        assert_eq!(
            target.labels.get("__meta_ecs_service_name").map(String::as_str),
            Some("api")
        );
        assert!(!target.labels.contains_key("__meta_ecs_cluster"));
        assert!(!target.labels.contains_key("__meta_ecs_service"));
    }

    #[test]
    fn migrate_target_label_schema_keeps_existing_canonical_values() {
        let mut target = Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels: HashMap::from([
                ("__meta_ecs_cluster".to_string(), "legacy-prod".to_string()),
                (
                    "__meta_ecs_cluster_name".to_string(),
                    "canonical-prod".to_string(),
                ),
            ]),
        };

        migrate_target_label_schema(&mut target);

        assert_eq!(
            target.labels.get("__meta_ecs_cluster_name").map(String::as_str),
            Some("canonical-prod")
        );
        assert!(!target.labels.contains_key("__meta_ecs_cluster"));
    }
}
