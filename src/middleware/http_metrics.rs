use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

use crate::state::AppState;

pub async fn http_metrics_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|mp| mp.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());
    let method = req.method().as_str().to_owned();

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    state
        .metrics
        .http_requests_total
        .with_label_values(&[&path, &method, &status])
        .inc();
    state
        .metrics
        .http_request_duration_seconds
        .with_label_values(&[&path, &method])
        .observe(elapsed);

    response
}
