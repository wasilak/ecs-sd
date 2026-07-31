use crate::handlers::config;
use crate::state::AppState;
use axum::{Router, routing::get};

pub fn routes() -> Router<AppState> {
    Router::new().route("/config", get(config::config_handler))
}
