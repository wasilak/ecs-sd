use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::error::DiscoveryError;
use crate::models::Target;
use crate::aws::DiscoveryService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<Vec<Target>>>,
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
            cache: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
            discovery,
        })
    }
}
