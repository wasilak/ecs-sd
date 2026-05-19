use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("AWS ECS API error: {0}")]
    EcsError(String),

    #[error("AWS EC2 API error: {0}")]
    Ec2Error(String),

    #[error("AWS STS API error: {0}")]
    StsError(String),

    #[error("Cluster not found: {0}")]
    ClusterNotFound(String),

    #[error("Task has no container instance")]
    NoContainerInstance,

    #[error("EC2 instance has no private IP")]
    NoPrivateIp,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    MissingConfig(&'static str),

    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
}
