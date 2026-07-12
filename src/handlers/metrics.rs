use axum::{extract::State, response::{IntoResponse, Response}, http::StatusCode, body::Body};
use prometheus::{TextEncoder, Encoder};

use crate::state::AppState;

/// Get Prometheus metrics in text exposition format
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "operations",
    responses(
        (status = 200, description = "Prometheus metrics",
         content_type = "text/plain; version=0.0.4")
    )
)]
pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    // Update cache age gauge
    let last_refresh = { let snap = state.snapshot.read().await; snap.last_refresh };
    let age_secs = std::time::SystemTime::now()
        .duration_since(last_refresh)
        .unwrap_or_default()
        .as_secs_f64();
    state.metrics.cache_age.set(age_secs);

    // Update cluster metrics if cluster mode is active
    if let Some(ref cluster) = state.cluster {
        let chitchat = cluster.handle.chitchat();
        let cc = chitchat.lock().await;
        let node_count = cc.live_nodes().count();
        state.metrics.cluster_nodes.set(node_count as f64);
        drop(cc); // Release lock before calling is_leader

        let is_leader = cluster.is_leader().await;
        state.metrics.cluster_is_leader.set(if is_leader { 1.0 } else { 0.0 });
    }

    // Gather and encode metrics
    let encoder = TextEncoder::new();
    let metric_families = state.metrics.registry.gather();
    let mut buffer = vec![];
    
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::warn!(error = %e, "metrics encoding error");
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("metrics encoding error"))
            .unwrap_or_else(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, "metrics encoding error").into_response()
            });
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", encoder.format_type())
        .body(Body::from(buffer))
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to build metrics response").into_response()
        })
}
