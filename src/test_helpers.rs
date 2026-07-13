use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::aws::DiscoveryService;
use crate::config::Config;
use crate::metrics::MetricsState;
use crate::models::Target;
use crate::state::{AppState, app_state::CacheSnapshot};

/// Build a minimal AppState for handler integration tests.
/// Uses dummy AWS clients — handler tests never call discovery methods.
pub(crate) fn build_test_state() -> AppState {
    let metrics = Arc::new(MetricsState::new().unwrap());

    let ecs_client = aws_sdk_ecs::Client::from_conf(
        aws_sdk_ecs::config::Builder::new()
            .behavior_version_latest()
            .region(aws_sdk_ecs::config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_ecs::config::Credentials::new(
                "test", "test", None, None, "test",
            ))
            .build(),
    );
    let ec2_client = aws_sdk_ec2::Client::from_conf(
        aws_sdk_ec2::config::Builder::new()
            .behavior_version_latest()
            .region(aws_sdk_ec2::config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_ec2::config::Credentials::new(
                "test", "test", None, None, "test",
            ))
            .build(),
    );

    let discovery = DiscoveryService::new_for_test(
        ecs_client, ec2_client, "123456789012", "us-east-1", Arc::clone(&metrics),
    );

    let config = Config::default();

    AppState {
        snapshot: Arc::new(RwLock::new(CacheSnapshot::default())),
        cache_ttl_seconds: config.refresh_interval,
        started_at: std::time::Instant::now(),
        last_refresh_outcome: Arc::new(RwLock::new(None)),
        config: Arc::new(config),
        discovery,
        http_client: reqwest::Client::builder().build().unwrap(),
        cluster: None,
        metrics,
        last_manual_refresh_request: Arc::new(AtomicU64::new(0)),
        startup_duration_recorded: Arc::new(AtomicBool::new(false)),
    }
}

/// Build a test AppState with pre-populated cache targets.
pub(crate) async fn build_test_state_with_targets(targets: Vec<Target>) -> AppState {
    let state = build_test_state();
    state.replace_cache_and_routing(targets).await;
    state
}
