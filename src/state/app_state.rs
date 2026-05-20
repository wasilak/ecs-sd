use std::sync::Arc;
use std::collections::HashMap;
use std::time::SystemTime;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::error::DiscoveryError;
use crate::models::{MetadataLevel, Target};
use crate::aws::DiscoveryService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub last_refresh: Arc<RwLock<SystemTime>>,
    pub cache_ttl_seconds: u64,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
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

        Ok(Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_refresh: Arc::new(RwLock::new(SystemTime::now())),
            cache_ttl_seconds: config.refresh_interval.max(1),
            config: Arc::new(config),
            discovery,
        })
    }
}
