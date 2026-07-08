use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::state::{AppState, RefreshOutcome};

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub cache: CacheHealth,
    pub cluster: ClusterHealth,
    pub last_refresh: LastRefreshHealth,
}

#[derive(Serialize)]
pub struct CacheHealth {
    pub targets: usize,
    pub age_seconds: u64,
    pub state: &'static str,
}

#[derive(Serialize)]
pub struct ClusterHealth {
    pub mode: &'static str,
    pub nodes: usize,
    pub is_leader: bool,
}

#[derive(Serialize)]
pub struct LastRefreshHealth {
    pub status: &'static str,
    pub timestamp: Option<u64>,
}

// RED phase stubs — return wrong values so behavioral tests fail
fn determine_health_status(
    _target_count: usize,
    _last_outcome: &Option<RefreshOutcome>,
) -> (&'static str, StatusCode) {
    ("wrong", StatusCode::OK)
}

fn determine_readiness_status(_target_count: usize) -> (&'static str, StatusCode) {
    ("wrong", StatusCode::OK)
}

pub async fn health_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthResponse>) {
    let (target_count, age_seconds) = {
        let snap = state.snapshot.read().await;
        let target_count = snap
            .cache
            .get(&crate::models::MetadataLevel::Aws)
            .map(|v| v.len())
            .unwrap_or(0);
        let age_seconds = if target_count > 0 {
            std::time::SystemTime::now()
                .duration_since(snap.last_refresh)
                .unwrap_or_default()
                .as_secs()
        } else {
            0
        };
        (target_count, age_seconds)
    };

    let last_outcome = state.last_refresh_outcome.read().await.clone();
    let uptime_seconds = state.started_at.elapsed().as_secs();

    let (cluster_nodes, is_leader) = match state.cluster.as_ref() {
        None => (1usize, true),
        Some(cluster) => {
            let chitchat = cluster.handle.chitchat();
            let cc = chitchat.lock().await;
            let count = cc.live_nodes().count();
            drop(cc);
            let leader = cluster.is_leader().await;
            (count, leader)
        }
    };

    let mode = match state.config.cluster_mode {
        crate::config::ClusterMode::Standalone => "standalone",
        crate::config::ClusterMode::Cluster => "cluster",
    };

    let cache_state = if target_count > 0 { "populated" } else { "empty" };

    let (last_refresh_status, last_refresh_timestamp) = match &last_outcome {
        Some(o) if o.success => ("ok", Some(o.timestamp_unix)),
        Some(o) => ("failed", Some(o.timestamp_unix)),
        None => ("never", None),
    };

    let (status_str, http_status) = determine_health_status(target_count, &last_outcome);

    let response = HealthResponse {
        status: status_str,
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds,
        cache: CacheHealth {
            targets: target_count,
            age_seconds,
            state: cache_state,
        },
        cluster: ClusterHealth {
            mode,
            nodes: cluster_nodes,
            is_leader,
        },
        last_refresh: LastRefreshHealth {
            status: last_refresh_status,
            timestamp: last_refresh_timestamp,
        },
    };

    (http_status, Json(response))
}

pub async fn health_live_handler() -> Json<serde_json::Value> {
    // RED phase stub — wrong value so test fails
    Json(serde_json::json!({"status": "wrong"}))
}

pub async fn health_ready_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let target_count = {
        let snap = state.snapshot.read().await;
        snap.cache
            .get(&crate::models::MetadataLevel::Aws)
            .map(|v| v.len())
            .unwrap_or(0)
    };
    let (status_str, code) = determine_readiness_status(target_count);
    (code, Json(serde_json::json!({"status": status_str})))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- determine_health_status: all six branches ---

    #[test]
    fn determine_health_status_healthy_when_populated_and_success() {
        let outcome = Some(RefreshOutcome { success: true, timestamp_unix: 100 });
        let (status, code) = determine_health_status(5, &outcome);
        assert_eq!(status, "healthy");
        assert_eq!(code, StatusCode::OK);
    }

    #[test]
    fn determine_health_status_degraded_when_populated_and_failed() {
        let outcome = Some(RefreshOutcome { success: false, timestamp_unix: 100 });
        let (status, code) = determine_health_status(5, &outcome);
        assert_eq!(status, "degraded");
        assert_eq!(code, StatusCode::OK);
    }

    #[test]
    fn determine_health_status_degraded_when_populated_and_no_outcome() {
        let (status, code) = determine_health_status(5, &None);
        assert_eq!(status, "degraded");
        assert_eq!(code, StatusCode::OK);
    }

    #[test]
    fn determine_health_status_starting_503_when_empty_and_failed() {
        let outcome = Some(RefreshOutcome { success: false, timestamp_unix: 100 });
        let (status, code) = determine_health_status(0, &outcome);
        assert_eq!(status, "starting");
        assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn determine_health_status_starting_200_when_empty_and_success() {
        let outcome = Some(RefreshOutcome { success: true, timestamp_unix: 100 });
        let (status, code) = determine_health_status(0, &outcome);
        assert_eq!(status, "starting");
        assert_eq!(code, StatusCode::OK);
    }

    #[test]
    fn determine_health_status_starting_200_when_empty_and_no_outcome() {
        let (status, code) = determine_health_status(0, &None);
        assert_eq!(status, "starting");
        assert_eq!(code, StatusCode::OK);
    }

    // --- determine_readiness_status ---

    #[test]
    fn determine_readiness_status_ready_when_targets_present() {
        let (status, code) = determine_readiness_status(3);
        assert_eq!(status, "ready");
        assert_eq!(code, StatusCode::OK);
    }

    #[test]
    fn determine_readiness_status_not_ready_when_empty() {
        let (status, code) = determine_readiness_status(0);
        assert_eq!(status, "not_ready");
        assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    }

    // --- health_live_handler ---

    #[tokio::test]
    async fn health_live_handler_returns_alive_status() {
        let Json(body) = health_live_handler().await;
        assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("alive"));
    }

    // --- HealthResponse serialization (HEALTH-01) ---

    #[test]
    fn health_response_serializes_all_expected_keys() {
        let response = HealthResponse {
            status: "healthy",
            version: "0.5.0",
            uptime_seconds: 42,
            cache: CacheHealth {
                targets: 5,
                age_seconds: 10,
                state: "populated",
            },
            cluster: ClusterHealth {
                mode: "standalone",
                nodes: 1,
                is_leader: true,
            },
            last_refresh: LastRefreshHealth {
                status: "ok",
                timestamp: Some(1_234_567_890),
            },
        };

        let json = serde_json::to_value(&response).unwrap();

        assert!(json.get("status").is_some(), "missing top-level 'status'");
        assert!(json.get("version").is_some(), "missing top-level 'version'");
        assert!(json.get("uptime_seconds").is_some(), "missing top-level 'uptime_seconds'");

        let cache = json.get("cache").expect("missing 'cache'");
        assert!(cache.get("targets").is_some(), "missing cache.targets");
        assert!(cache.get("age_seconds").is_some(), "missing cache.age_seconds");
        assert!(cache.get("state").is_some(), "missing cache.state");

        let cluster = json.get("cluster").expect("missing 'cluster'");
        assert!(cluster.get("mode").is_some(), "missing cluster.mode");
        assert!(cluster.get("nodes").is_some(), "missing cluster.nodes");
        assert!(cluster.get("is_leader").is_some(), "missing cluster.is_leader");

        let last_refresh = json.get("last_refresh").expect("missing 'last_refresh'");
        assert!(last_refresh.get("status").is_some(), "missing last_refresh.status");
        assert!(last_refresh.get("timestamp").is_some(), "missing last_refresh.timestamp");
    }
}
