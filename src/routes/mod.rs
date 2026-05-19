pub mod health;
pub mod sd;

use axum::Router;
use crate::state::AppState;

pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(sd::routes())
}
