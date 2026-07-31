use crate::handlers::health;
use crate::state::AppState;
use axum::{Router, routing::get};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_handler))
        .route("/health/live", get(health::health_live_handler))
        .route("/health/ready", get(health::health_ready_handler))
}
