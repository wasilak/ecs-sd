use axum::{
    routing::{get, post},
    Router,
};
use crate::state::AppState;
use crate::handlers::sd;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sd", get(sd::sd_handler))
        .route("/sd/refresh", post(sd::refresh_handler))
}
