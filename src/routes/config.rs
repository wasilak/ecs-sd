use axum::{
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::handlers::config;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/config", get(config::config_handler))
}
