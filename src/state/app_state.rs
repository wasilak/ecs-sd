use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::models::Target;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<Vec<Target>>>,
    pub config: Arc<Config>,
    pub ecs_client: aws_sdk_ecs::Client,
    pub ec2_client: aws_sdk_ec2::Client,
}

impl AppState {
    pub fn new(
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
            ecs_client,
            ec2_client,
        }
    }
}
