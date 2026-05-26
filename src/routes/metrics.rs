use axum::{
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::handlers::metrics;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(metrics::metrics_handler))
}
