use crate::handlers::metrics;
use crate::state::AppState;
use axum::{Router, routing::get};

pub fn routes() -> Router<AppState> {
    Router::new().route("/metrics", get(metrics::metrics_handler))
}
