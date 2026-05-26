pub mod health;
pub mod proxy;
pub mod sd;

use axum::Router;

use crate::state::AppState;

pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(sd::routes())
        .merge(proxy::routes())
}
