use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
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

/// Get service configuration
#[utoipa::path(
    get,
    path = "/config",
    tag = "operations",
    responses(
        (status = 200, description = "Service configuration", body = ConfigResponse)
    )
)]
pub async fn config_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<ConfigResponse>) {
    (StatusCode::OK, Json(ConfigResponse::from(state.config.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ClusterMode, Mode};
    use crate::models::MetadataLevel;

    fn sample_config_response() -> ConfigResponse {
        ConfigResponse {
            clusters: vec!["prod".to_string()],
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: MetadataLevel::Task,
            mode: Mode::Discovery,
            cluster_mode: ClusterMode::Standalone,
            gossip_port: 8081,
            node_id: "localhost:8081".to_string(),
            metrics_port: None,
            refresh_token_set: false,
            refresh_min_interval: 30,
            proxy_forward_sensitive_headers: false,
            max_target_drop_ratio: 0.0,
        }
    }

    #[test]
    fn config_response_serializes_all_expected_keys() {
        let response = sample_config_response();
        let json = serde_json::to_value(&response).unwrap();

        let expected_keys = [
            "clusters",
            "listen",
            "refresh_interval",
            "metadata_level",
            "mode",
            "cluster_mode",
            "gossip_port",
            "node_id",
            "metrics_port",
            "refresh_token_set",
            "refresh_min_interval",
            "proxy_forward_sensitive_headers",
            "max_target_drop_ratio",
        ];

        for key in &expected_keys {
            assert!(
                json.get(*key).is_some(),
                "missing expected key: {}",
                key
            );
        }
    }

    #[test]
    fn config_response_hides_refresh_token() {
        let response = sample_config_response();
        let json = serde_json::to_value(&response).unwrap();

        assert!(
            json.get("refresh_token").is_none(),
            "refresh_token must NOT appear in serialized output"
        );
        assert!(
            json.get("refresh_token_set").is_some(),
            "refresh_token_set must appear in serialized output"
        );
    }

    #[test]
    fn config_response_from_config_masks_secret() {
        let config = crate::config::Config {
            refresh_token: Some("super-secret".to_string()),
            ..crate::config::Config::default()
        };
        let response = ConfigResponse::from(&config);
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json.get("refresh_token_set").and_then(|v| v.as_bool()), Some(true));
        assert!(
            json.get("refresh_token").is_none(),
            "refresh_token must NOT appear in serialized output"
        );
    }

    #[test]
    fn config_response_from_config_no_token() {
        let config = crate::config::Config {
            refresh_token: None,
            ..crate::config::Config::default()
        };
        let response = ConfigResponse::from(&config);

        assert_eq!(response.refresh_token_set, false);
    }

    #[test]
    fn config_response_includes_max_target_drop_ratio() {
        let response = ConfigResponse {
            max_target_drop_ratio: 0.75,
            ..sample_config_response()
        };
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(
            json.get("max_target_drop_ratio").and_then(|v| v.as_f64()),
            Some(0.75)
        );
    }

    // --- Integration tests through full router ---

    #[tokio::test]
    async fn config_integration_returns_200() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = crate::test_helpers::build_test_state();
        let app = crate::routes::create_routes(state.clone()).with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_integration_returns_expected_keys() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let state = crate::test_helpers::build_test_state();
        let app = crate::routes::create_routes(state.clone()).with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("clusters").is_some(), "missing 'clusters'");
        assert!(json.get("listen").is_some(), "missing 'listen'");
        assert!(json.get("refresh_interval").is_some(), "missing 'refresh_interval'");
        assert!(json.get("metadata_level").is_some(), "missing 'metadata_level'");
        assert!(json.get("mode").is_some(), "missing 'mode'");
        assert!(json.get("node_id").is_some(), "missing 'node_id'");
        assert!(json.get("refresh_token_set").is_some(), "missing 'refresh_token_set'");
    }

    #[tokio::test]
    async fn config_integration_hides_refresh_token() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let state = crate::test_helpers::build_test_state();
        let app = crate::routes::create_routes(state.clone()).with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("refresh_token").is_none(), "refresh_token must NOT appear in response");
    }
}
