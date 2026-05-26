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

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub last_refresh: Arc<RwLock<SystemTime>>,
    pub cache_ttl_seconds: u64,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
    pub routing_table: Arc<RwLock<HashMap<Uuid, ProxyTarget>>>,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub async fn new(
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,
        region: String,
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
        })
    }

    /// Atomically replace all cache tiers and update last_refresh. In proxy mode,
    /// also rebuilds the routing table. Called from both the background refresh loop
    /// (main.rs) and the manual POST /refresh handler (sd.rs), ensuring PROX-06.
    pub async fn replace_cache_and_routing(&self, targets_aws: Vec<Target>) {
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
