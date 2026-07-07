use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::config::Mode;
use crate::state::AppState;

/// Strip hop-by-hop headers that must not be forwarded to upstream.
pub(crate) fn filter_hop_by_hop_headers(mut headers: HeaderMap) -> HeaderMap {
    const HOP_BY_HOP: &[&str] = &[
        "connection",
        "keep-alive",
        "transfer-encoding",
        "te",
        "trailer",
        "upgrade",
        "proxy-authorization",
        "proxy-authenticate",
        "host",
    ];
    for name in HOP_BY_HOP {
        // HeaderName::from_static panics on invalid static strings, but all of these are valid.
        headers.remove(HeaderName::from_static(name));
    }
    headers
}

/// Strip sensitive headers unless explicitly allowed by configuration.
pub(crate) fn filter_sensitive_headers(mut headers: HeaderMap, allow_sensitive: bool) -> HeaderMap {
    if allow_sensitive {
        return headers;
    }

    const SENSITIVE_HEADERS: &[&str] = &[
        "authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
    ];

    for name in SENSITIVE_HEADERS {
        headers.remove(HeaderName::from_static(name));
    }

    headers
}

/// Parse the X-Prometheus-Scrape-Timeout-Seconds header into a Duration.
/// Accepts values > 0.0 and <= 300.0. All other values (absent, unparseable, zero,
/// negative, or > 300) fall back to the 30-second default.
pub(crate) fn parse_scrape_timeout(headers: &HeaderMap) -> Duration {
    const DEFAULT: Duration = Duration::from_secs(30);
    headers
        .get("x-prometheus-scrape-timeout-seconds")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|&secs| secs > 0.0 && secs <= 300.0)
        .map(Duration::from_secs_f64)
        .unwrap_or(DEFAULT)
}

pub async fn proxy_handler(
    State(state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    headers: HeaderMap,
    req: axum::extract::Request,
) -> Response {
    // Proxy is only available in proxy mode.
    if state.config.mode != Mode::Proxy {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "proxy mode not enabled"})),
        )
            .into_response();
    }

    // Parse the UUID from the path segment.
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid route id"})),
            )
                .into_response()
        }
    };

    // Look up routing table — release read lock before HTTP call (Risk 3 / T-06-03-05).
    let proxy_target = {
        let snap = state.snapshot.read().await;
        snap.routing_table.get(&uuid).cloned()
    };
    let proxy_target = match proxy_target {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "route not found"})),
            )
                .into_response()
        }
    };

    // Build upstream URL; wildcard path from Axum has NO leading slash.
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let upstream_url = format!("http://{}/{}{}", proxy_target.address, path, query);

    debug!(uuid = %uuid, upstream_url = %upstream_url, "proxy route resolved");

    // Parse per-request timeout from Prometheus scrape header.
    let timeout = parse_scrape_timeout(&headers);

    // Strip hop-by-hop headers before forwarding.
    let forwarded = filter_hop_by_hop_headers(headers);
    let forwarded = filter_sensitive_headers(forwarded, state.config.proxy_forward_sensitive_headers);

    // Start timer for proxy metrics
    let timer = state.metrics.proxy_duration.start_timer();

    // Send upstream request.
    let upstream_resp = match state
        .http_client
        .get(&upstream_url)
        .headers(forwarded)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            timer.observe_duration();
            state.metrics.proxy_requests
                .with_label_values(&["502"])
                .inc();
            warn!(upstream_url = %upstream_url, error = %e, "upstream request failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream unreachable"})),
            )
                .into_response();
        }
    };

    // Stream response back: copy status + headers from upstream.
    let status = upstream_resp.status();
    timer.observe_duration();
    state.metrics.proxy_requests
        .with_label_values(&[&status.as_u16().to_string()])
        .inc();
    let upstream_headers = upstream_resp.headers().clone();
    let mut builder = Response::builder().status(status);
    for (key, value) in upstream_headers.iter() {
        builder = builder.header(key, value);
    }
    builder
        .body(Body::from_stream(upstream_resp.bytes_stream()))
        .unwrap_or_else(|e| {
            warn!(error = %e, "failed to construct proxy response");
            (StatusCode::INTERNAL_SERVER_ERROR, "response construction failed").into_response()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    // ---- parse_scrape_timeout tests ----

    #[test]
    fn parse_scrape_timeout_float_string() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-prometheus-scrape-timeout-seconds",
            HeaderValue::from_static("10.5"),
        );
        assert_eq!(parse_scrape_timeout(&headers), Duration::from_secs_f64(10.5));
    }

    #[test]
    fn parse_scrape_timeout_integer_string() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-prometheus-scrape-timeout-seconds",
            HeaderValue::from_static("30"),
        );
        assert_eq!(parse_scrape_timeout(&headers), Duration::from_secs(30));
    }

    #[test]
    fn parse_scrape_timeout_absent_header() {
        let headers = HeaderMap::new();
        assert_eq!(parse_scrape_timeout(&headers), Duration::from_secs(30));
    }

    #[test]
    fn parse_scrape_timeout_over_300s_falls_back_to_default() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-prometheus-scrape-timeout-seconds",
            HeaderValue::from_static("999"),
        );
        // Values > 300 fall back to 30s (not capped at 300s).
        assert_eq!(parse_scrape_timeout(&headers), Duration::from_secs(30));
    }

    #[test]
    fn parse_scrape_timeout_zero_falls_back() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-prometheus-scrape-timeout-seconds",
            HeaderValue::from_static("0"),
        );
        assert_eq!(parse_scrape_timeout(&headers), Duration::from_secs(30));
    }

    // ---- filter_hop_by_hop_headers tests ----

    #[test]
    fn filter_hop_by_hop_removes_connection() {
        let mut headers = HeaderMap::new();
        headers.insert("connection", HeaderValue::from_static("keep-alive"));
        let result = filter_hop_by_hop_headers(headers);
        assert!(!result.contains_key("connection"));
    }

    #[test]
    fn filter_hop_by_hop_removes_host() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("example.com"));
        let result = filter_hop_by_hop_headers(headers);
        assert!(!result.contains_key("host"));
    }

    #[test]
    fn filter_hop_by_hop_preserves_accept() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("text/plain"));
        headers.insert("connection", HeaderValue::from_static("close"));
        let result = filter_hop_by_hop_headers(headers);
        assert!(result.contains_key("accept"));
        assert!(!result.contains_key("connection"));
    }

    #[test]
    fn filter_hop_by_hop_preserves_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer tok"),
        );
        let result = filter_hop_by_hop_headers(headers);
        assert!(result.contains_key("authorization"));
    }

    #[test]
    fn filter_sensitive_headers_strips_authorization_and_cookie_by_default() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer tok"));
        headers.insert("cookie", HeaderValue::from_static("session=abc"));
        headers.insert("accept", HeaderValue::from_static("application/json"));

        let result = filter_sensitive_headers(headers, false);

        assert!(!result.contains_key("authorization"));
        assert!(!result.contains_key("cookie"));
        assert!(result.contains_key("accept"));
    }

    #[test]
    fn filter_sensitive_headers_keeps_sensitive_when_enabled() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer tok"));
        headers.insert("cookie", HeaderValue::from_static("session=abc"));

        let result = filter_sensitive_headers(headers, true);

        assert!(result.contains_key("authorization"));
        assert!(result.contains_key("cookie"));
    }
}
