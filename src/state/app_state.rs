use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::error::DiscoveryError;
use crate::models::{MetadataLevel, Target};
use crate::aws::DiscoveryService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
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
            config: Arc::new(config),
            discovery,
        })
    }
}
