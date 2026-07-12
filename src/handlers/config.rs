use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: crate::models::MetadataLevel,
    pub mode: crate::config::Mode,
    pub cluster_mode: crate::config::ClusterMode,
    pub gossip_port: u16,
    pub node_id: String,
    pub metrics_port: Option<u16>,
    pub refresh_token_set: bool,
    pub refresh_min_interval: u64,
    pub proxy_forward_sensitive_headers: bool,
    pub max_target_drop_ratio: f64,
}

impl From<&crate::config::Config> for ConfigResponse {
    fn from(config: &crate::config::Config) -> Self {
        Self {
            clusters: config.clusters.clone(),
            listen: config.listen.clone(),
            refresh_interval: config.refresh_interval,
            metadata_level: config.metadata_level.clone(),
            mode: config.mode.clone(),
            cluster_mode: config.cluster_mode.clone(),
            gossip_port: config.gossip_port,
            node_id: config.node_id.clone(),
            metrics_port: config.metrics_port,
            refresh_token_set: config.refresh_token.is_some(),
            refresh_min_interval: config.refresh_min_interval,
            proxy_forward_sensitive_headers: config.proxy_forward_sensitive_headers,
            max_target_drop_ratio: config.max_target_drop_ratio,
        }
    }
}

pub async fn config_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<ConfigResponse>) {
    (StatusCode::OK, Json(ConfigResponse::from(state.config.as_ref())))
}
