use crate::handlers::sd;
use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sd", get(sd::sd_handler))
        .route("/sd/refresh", post(sd::refresh_handler))
}
