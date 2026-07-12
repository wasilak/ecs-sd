pub mod config;
pub mod health;
pub mod metrics;
pub mod proxy;
pub mod sd;

use axum::{middleware, Router};

use crate::state::AppState;

pub fn create_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(metrics::routes())
        .merge(sd::routes())
        .merge(proxy::routes())
        .merge(config::routes())
        .route_layer(middleware::from_fn_with_state(
            state,
            crate::middleware::http_metrics::http_metrics_middleware,
        ))
}
