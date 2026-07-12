use axum::{routing::get, Router};

use crate::handlers::proxy;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/proxy/{id}/{*path}", get(proxy::proxy_handler))
}
