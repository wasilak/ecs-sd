use axum::{
    extract::{Query, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use crate::config::Mode;
use crate::models::ProxyTarget;
use crate::state::AppState;
use crate::models::{filter_labels_by_level, FilterMode, MetadataLevel, SdQueryParams, Target};
use serde_json::json;
use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};
use std::time::SystemTime;
use uuid::Uuid;

/// Build a single SD target entry for proxy mode: the public_address becomes the
/// scrape target and __metrics_path__ points through the proxy at the given UUID.
fn build_proxy_sd_target(
    uuid: &Uuid,
    proxy_target: &ProxyTarget,
    public_address: &str,
    public_address_scheme: &str,
) -> Target {
    let mut labels = proxy_target.labels.clone();
    labels.insert(
        "__metrics_path__".to_string(),
        format!("/proxy/{}/metrics", uuid),
    );
    labels.insert("__scheme__".to_string(), public_address_scheme.to_string());
    Target {
        targets: vec![public_address.to_string()],
        labels,
    }
}

/// List discovery targets with optional filtering
///
/// Returns ECS service discovery targets in Prometheus SD format.
/// In proxy mode, targets point to this service's public address with
/// `__metrics_path__` routing through the proxy endpoint.
#[utoipa::path(
    get,
    path = "/sd",
    tag = "discovery",
    params(
        ("cluster" = Vec<String>, Query, description = "Filter by cluster name(s); repeatable: ?cluster=a&cluster=b"),
        ("service" = Vec<String>, Query, description = "Filter by ECS service name(s); repeatable"),
        ("family" = Vec<String>, Query, description = "Filter by task definition family; repeatable"),
        ("level" = Option<MetadataLevel>, Query, description = "Metadata level override: container, task, service, cluster, aws"),
        ("filter_mode" = Option<FilterMode>, Query, description = "AND (default) or OR filter logic for combined filters"),
        ("tag_{name}" = Option<String>, Query, description = "Filter by ECS tag; e.g., tag_env=prod"),
    ),
    responses(
        (status = 200, description = "Discovery targets", body = Vec<Target>,
         headers(
             ("X-Cache-Age" = String, description = "Cache age in seconds"),
             ("X-Cache-State" = String, description = "Cache freshness: 'fresh' or 'stale'")
         ))
    )
)]
pub async fn sd_handler(
    State(state): State<AppState>,
    Query(mut params): Query<SdQueryParams>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let raw_pairs = parse_raw_query(raw_query.as_deref());
    params.clusters = extract_values(&raw_pairs, "cluster");
    params.services = extract_values(&raw_pairs, "service");
    params.families = extract_values(&raw_pairs, "family");
    params.tag_filters = extract_tag_filters_from_pairs(&raw_pairs);

    let level = params.level.unwrap_or(state.config.metadata_level);

    // In proxy mode, return routing-table targets with public_address as target
    // and __metrics_path__ pointing through the proxy.
    if state.config.mode == Mode::Proxy {
        let public_address = state.config.public_address.as_deref().unwrap_or_default();
        let public_address_scheme = state
            .config
            .public_address_scheme
            .as_deref()
            .unwrap_or("http");

        let (proxy_targets, last_refresh) = {
            let snap = state.snapshot.read().await;
            let last_refresh = snap.last_refresh;
            let targets: Vec<Target> = snap.routing_table
                .iter()
                .map(|(uuid, proxy_target)| {
                    build_proxy_sd_target(uuid, proxy_target, public_address, public_address_scheme)
                })
                .map(|target| filter_labels_by_level(&target, level))
                .collect();
            (targets, last_refresh)
            // snap lock released here
        };

        let filtered = filter_targets(proxy_targets, &params);

        let cache_age_seconds = calculate_cache_age_seconds(last_refresh, SystemTime::now());
        let cache_state = if cache_age_seconds > state.cache_ttl_seconds {
            "stale"
        } else {
            "fresh"
        };

        return build_sd_response_with_cache_age(filtered, cache_age_seconds, cache_state);
    }

    let (targets, last_refresh) = {
        let snap = state.snapshot.read().await;
        let targets = snap.cache.get(&level).cloned().unwrap_or_default();
        let last_refresh = snap.last_refresh;
        (targets, last_refresh)
        // snap lock released here
    };

    if !targets.is_empty() {
        debug!("cache hit: {} targets served", targets.len());
    } else {
        debug!("cache miss: 0 targets served for level={}", level);
    }

    let filtered = filter_targets(targets, &params);

    let cache_age_seconds = calculate_cache_age_seconds(last_refresh, SystemTime::now());
    let cache_state = if cache_age_seconds > state.cache_ttl_seconds {
        "stale"
    } else {
        "fresh"
    };

    build_sd_response_with_cache_age(filtered, cache_age_seconds, cache_state)
}

fn authorize_refresh(
    expected_token: Option<&str>,
    provided_token: Option<&str>,
) -> Result<(), (StatusCode, serde_json::Value)> {
    let Some(expected_token) = expected_token else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "error": "refresh endpoint disabled",
                "reason": "refresh token not configured"
            }),
        ));
    };

    if provided_token != Some(expected_token) {
        return Err((StatusCode::UNAUTHORIZED, json!({ "error": "unauthorized" })));
    }

    Ok(())
}

fn refresh_retry_after_seconds(last_request_secs: u64, now_secs: u64, min_interval: u64) -> Option<u64> {
    let elapsed = now_secs.saturating_sub(last_request_secs);
    if elapsed < min_interval {
        Some(min_interval - elapsed)
    } else {
        None
    }
}

/// Trigger a manual discovery refresh
///
/// Forces an immediate ECS API call to re-discover all targets.
/// Requires a valid `X-Refresh-Token` header. Rate limited by `refresh_min_interval`.
#[utoipa::path(
    post,
    path = "/sd/refresh",
    tag = "operations",
    responses(
        (status = 200, description = "Refresh succeeded", body = serde_json::Value),
        (status = 401, description = "Missing or invalid refresh token", body = serde_json::Value),
        (status = 429, description = "Rate limited", body = serde_json::Value),
        (status = 503, description = "All clusters unreachable", body = serde_json::Value),
    )
)]
pub async fn refresh_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let provided_token = headers
        .get("X-Refresh-Token")
        .and_then(|v| v.to_str().ok());

    if let Err((status, body)) = authorize_refresh(state.config.refresh_token.as_deref(), provided_token) {
        return (status, Json(body)).into_response();
    }

    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last_secs = state.last_manual_refresh_request.load(Ordering::SeqCst);
    if let Some(retry_after_seconds) =
        refresh_retry_after_seconds(last_secs, now_secs, state.config.refresh_min_interval)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "rate limited",
                "retry_after_seconds": retry_after_seconds
            })),
        )
            .into_response();
    }
    state.last_manual_refresh_request.store(now_secs, Ordering::SeqCst);

    let clusters = state.config.clusters.clone();

    info!("Manual discovery refresh triggered");

    match state.discovery.discover_all_clusters(&clusters, state.config.mode.clone()).await {
        Ok(targets_aws) => {
            let count = targets_aws.len();
            state.replace_cache_and_record_metrics(targets_aws).await;
            state.record_startup_duration_once();
            info!("Discovery refresh complete: {} targets", count);
            Json(json!({
                "status": "ok",
                "targets_discovered": count
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Manual refresh failed — all clusters unreachable: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "all clusters failed",
                    "detail": e.to_string()
                })),
            )
            .into_response()
        }
    }
}

fn filter_targets(targets: Vec<Target>, params: &SdQueryParams) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|target| {
            let mut checks = Vec::new();

            if !params.clusters.is_empty() {
                let target_cluster = target
                    .labels
                    .get("__meta_ecs_cluster_name")
                    .or_else(|| target.labels.get("__meta_ecs_cluster"));
                checks.push(
                    target_cluster
                        .map(|s| params.clusters.contains(s))
                        .unwrap_or(false),
                );
            }

            if !params.services.is_empty() {
                let target_service = target
                    .labels
                    .get("__meta_ecs_service_name")
                    .or_else(|| target.labels.get("__meta_ecs_service"));
                checks.push(
                    target_service
                        .map(|s| params.services.contains(s))
                        .unwrap_or(false),
                );
            }

            if !params.families.is_empty() {
                let target_family = target.labels.get("__meta_ecs_task_family");
                checks.push(
                    target_family
                        .map(|s| params.families.contains(s))
                        .unwrap_or(false),
                );
            }

            // Group tag filters by key: same key = OR (any value matches),
            // different keys each add an AND check.
            let mut tag_groups: Vec<(&str, Vec<&str>)> = Vec::new();
            for (tag_name, tag_value) in &params.tag_filters {
                if let Some(group) = tag_groups.iter_mut().find(|(k, _)| *k == tag_name.as_str()) {
                    group.1.push(tag_value.as_str());
                } else {
                    tag_groups.push((tag_name.as_str(), vec![tag_value.as_str()]));
                }
            }
            for (tag_name, tag_values) in &tag_groups {
                let label_key = format!("__meta_ecs_tag_{}", tag_name);
                let target_tag = target.labels.get(&label_key);
                checks.push(
                    target_tag
                        .map(|s| tag_values.contains(&s.as_str()))
                        .unwrap_or(false),
                );
            }

            if checks.is_empty() {
                return true;
            }

            match params.filter_mode {
                FilterMode::And => checks.into_iter().all(|m| m),
                FilterMode::Or => checks.into_iter().any(|m| m),
            }
        })
        .collect()
}

fn parse_raw_query(raw_query: Option<&str>) -> Vec<(String, String)> {
    raw_query
        .and_then(|q| serde_urlencoded::from_str(q).ok())
        .unwrap_or_default()
}

fn extract_values(pairs: &[(String, String)], key: &str) -> Vec<String> {
    pairs
        .iter()
        .filter_map(|(k, v)| if k == key { Some(v.clone()) } else { None })
        .collect()
}

fn extract_tag_filters_from_pairs(pairs: &[(String, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("tag_")
                .map(|tag_name| (tag_name.to_string(), v.clone()))
        })
        .collect()
}

fn calculate_cache_age_seconds(last_refresh: SystemTime, now: SystemTime) -> u64 {
    now.duration_since(last_refresh)
        .unwrap_or_default()
        .as_secs()
}

fn build_sd_response_with_cache_age(
    targets: Vec<Target>,
    cache_age_seconds: u64,
    cache_state: &'static str,
) -> Response {
    let mut headers = HeaderMap::new();
    let header_value = HeaderValue::from_str(&cache_age_seconds.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("0"));
    headers.insert("X-Cache-Age", header_value);
    let state_header_value = HeaderValue::from_static(cache_state);
    headers.insert("X-Cache-State", state_header_value);

    (headers, Json(targets)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LabelBuilder, MetadataLevel};
    use aws_sdk_ecs::types::{Cluster, Service};
    use std::collections::HashMap;

    fn create_test_target(cluster: &str, service: &str, family: &str) -> Target {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster_name".to_string(), cluster.to_string());
        labels.insert("__meta_ecs_service_name".to_string(), service.to_string());
        labels.insert("__meta_ecs_task_family".to_string(), family.to_string());

        Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }
    }

    #[test]
    fn test_filter_by_cluster() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "api", "api-task"),
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_cluster_name"),
            Some(&"prod".to_string())
        );
    }

    #[test]
    fn test_filter_by_multiple_clusters() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("staging", "api", "api-task"),
            create_test_target("dev", "api", "api-task"),
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string(), "staging".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_multiple_families() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("prod", "worker", "worker-task"),
            create_test_target("prod", "web", "web-task"),
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: vec!["api-task".to_string(), "worker-task".to_string()],
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_case_sensitive() {
        let targets = vec![
            create_test_target("Prod", "api", "api-task"),
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 0); // Case-sensitive: Prod != prod
    }

    #[test]
    fn test_filter_and_logic() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("prod", "web", "web-task"),
            create_test_target("dev", "api", "api-task"),
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: vec!["api".to_string()],
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_no_params() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "web", "web-task"),
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2); // No filtering returns all
    }

    #[test]
    fn test_filter_by_cluster_with_legacy_label_schema() {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster".to_string(), "prod".to_string());
        labels.insert("__meta_ecs_service".to_string(), "api".to_string());
        labels.insert("__meta_ecs_task_family".to_string(), "api-task".to_string());

        let targets = vec![Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_by_service_with_legacy_label_schema() {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster".to_string(), "prod".to_string());
        labels.insert("__meta_ecs_service".to_string(), "api".to_string());
        labels.insert("__meta_ecs_task_family".to_string(), "api-task".to_string());

        let targets = vec![Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: vec!["api".to_string()],
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_targets_with_labels_from_label_builder() {
        let service = Service::builder()
            .service_name("api")
            .service_arn("arn:aws:ecs:eu-west-1:123456789012:service/prod/api")
            .status("ACTIVE")
            .desired_count(2)
            .running_count(2)
            .build();
        let cluster = Cluster::builder()
            .cluster_name("prod")
            .cluster_arn("arn:aws:ecs:eu-west-1:123456789012:cluster/prod")
            .build();

        let labels = LabelBuilder::new(MetadataLevel::Cluster)
            .with_service(&service)
            .with_cluster(&cluster)
            .build();

        let targets = vec![Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: vec!["api".to_string()],
            families: Vec::new(),
            level: Some(MetadataLevel::default()),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_cache_age_seconds_from_system_times() {
        let now = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let refreshed = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(88);

        let age = calculate_cache_age_seconds(refreshed, now);
        assert_eq!(age, 12);
    }

    #[test]
    fn test_sd_response_includes_cache_age_header() {
        let response = build_sd_response_with_cache_age(vec![], 7, "fresh");
        let header = response
            .headers()
            .get("X-Cache-Age")
            .expect("X-Cache-Age header must be present");

        assert_eq!(header, "7");
    }

    #[test]
    fn ttl_within_interval_marks_fresh() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let last_refresh = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(75);
        let cache_age_seconds = calculate_cache_age_seconds(last_refresh, now);

        let cache_state = if cache_age_seconds > 30 { "stale" } else { "fresh" };

        let response = build_sd_response_with_cache_age(vec![], cache_age_seconds, cache_state);
        let cache_state = response
            .headers()
            .get("X-Cache-State")
            .expect("X-Cache-State header must be present");

        assert_eq!(cache_state, "fresh");
    }

    #[test]
    fn ttl_beyond_interval_marks_stale() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let last_refresh = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let cache_age_seconds = calculate_cache_age_seconds(last_refresh, now);

        let cache_state = if cache_age_seconds > 30 { "stale" } else { "fresh" };

        let response = build_sd_response_with_cache_age(vec![], cache_age_seconds, cache_state);
        let cache_state = response
            .headers()
            .get("X-Cache-State")
            .expect("X-Cache-State header must be present");

        assert_eq!(cache_state, "stale");
    }

    #[test]
    fn test_cache_age_exactly_at_ttl_is_fresh() {
        // The handler uses `cache_age > ttl` (strict >), so age == ttl is fresh.
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let last_refresh = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(70);
        let age = calculate_cache_age_seconds(last_refresh, now);
        assert_eq!(age, 30);

        let ttl = 30u64;
        let state = if age > ttl { "stale" } else { "fresh" };
        assert_eq!(state, "fresh", "age == ttl must be classified as fresh");
    }

    #[test]
    fn test_cache_age_one_second_past_ttl_is_stale() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let last_refresh = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(69);
        let age = calculate_cache_age_seconds(last_refresh, now);
        assert_eq!(age, 31);

        let ttl = 30u64;
        let state = if age > ttl { "stale" } else { "fresh" };
        assert_eq!(state, "stale");
    }

    #[test]
    fn test_cache_age_clock_skew_returns_zero() {
        // now < last_refresh: duration_since returns Err, .unwrap_or_default() yields 0.
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(50);
        let last_refresh = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let age = calculate_cache_age_seconds(last_refresh, now);
        assert_eq!(age, 0);
    }

    #[test]
    fn authorize_refresh_returns_service_unavailable_when_token_missing() {
        let result = authorize_refresh(None, Some("provided"));
        assert!(result.is_err());
        let (status, body) = result.err().unwrap();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "refresh endpoint disabled");
    }

    #[test]
    fn authorize_refresh_rejects_missing_or_invalid_token() {
        let missing = authorize_refresh(Some("secret"), None);
        assert!(missing.is_err());
        assert_eq!(missing.err().unwrap().0, StatusCode::UNAUTHORIZED);

        let wrong = authorize_refresh(Some("secret"), Some("wrong"));
        assert!(wrong.is_err());
        assert_eq!(wrong.err().unwrap().0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn authorize_refresh_accepts_matching_token() {
        let result = authorize_refresh(Some("secret"), Some("secret"));
        assert!(result.is_ok());
    }

    #[test]
    fn refresh_retry_after_seconds_returns_none_when_interval_elapsed() {
        // now_secs=100, last_request_secs=60: elapsed=40 >= 30
        let retry = refresh_retry_after_seconds(60, 100, 30);
        assert_eq!(retry, None);
    }

    #[test]
    fn refresh_retry_after_seconds_returns_remaining_when_rate_limited() {
        // now_secs=100, last_request_secs=95: elapsed=5 < 30, remaining=25
        let retry = refresh_retry_after_seconds(95, 100, 30);
        assert_eq!(retry, Some(25));
    }

    #[test]
    fn refresh_retry_after_seconds_handles_clock_skew() {
        // now_secs=50, last_request_secs=100: saturating_sub gives 0 < 30, remaining=30
        let retry = refresh_retry_after_seconds(100, 50, 30);
        assert_eq!(retry, Some(30));
    }

    #[test]
    fn api_docs_refresh_contract_matches_handler_response_shape() {
        let docs = include_str!("../../docs/api.md");
        assert!(
            docs.contains("`POST /sd/refresh`"),
            "API docs must include /sd/refresh section"
        );
        assert!(
            docs.contains("\"status\": \"ok\"")
                && docs.contains("\"targets_discovered\""),
            "API docs must describe refresh JSON with status and targets_discovered"
        );
        assert!(
            !docs.contains("Returns same format as `GET /sd`"),
            "API docs must not claim /sd/refresh returns /sd target array"
        );
    }

    #[test]
    fn api_docs_sd_filtering_mentions_legacy_label_compatibility() {
        let docs = include_str!("../../docs/api.md");
        assert!(
            docs.contains("legacy")
                && docs.contains("__meta_ecs_cluster")
                && docs.contains("__meta_ecs_service"),
            "API docs should mention temporary legacy label compatibility"
        );
    }

    // ---- proxy-mode sd target tests ----

    #[test]
    fn sd_proxy_mode_returns_public_address_as_target() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"test-target");
        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels: HashMap::new(),
        };
        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "https");
        assert_eq!(target.targets, vec!["ecs-sd.internal:8080"]);
    }

    #[test]
    fn sd_proxy_mode_sets_metrics_path_label() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"test-target");
        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels: HashMap::new(),
        };
        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "https");
        let expected_path = format!("/proxy/{}/metrics", uuid);
        assert_eq!(
            target.labels.get("__metrics_path__"),
            Some(&expected_path)
        );
    }

    #[test]
    fn sd_proxy_mode_target_has_no_original_address() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"test-target");
        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels: HashMap::new(),
        };
        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "https");
        assert!(!target.targets.contains(&"10.0.0.5:8080".to_string()));
    }

    #[test]
    fn sd_proxy_mode_preserves_meta_labels() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"test-target");
        let mut labels = HashMap::new();
        labels.insert(
            "__meta_ecs_task_family".to_string(),
            "o11y-bot".to_string(),
        );
        labels.insert(
            "__meta_ecs_container_name".to_string(),
            "ingestion".to_string(),
        );
        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels,
        };

        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "https");

        assert_eq!(
            target.labels.get("__meta_ecs_task_family"),
            Some(&"o11y-bot".to_string())
        );
        assert_eq!(
            target.labels.get("__meta_ecs_container_name"),
            Some(&"ingestion".to_string())
        );
        assert!(target.labels.contains_key("__metrics_path__"));
    }

    #[test]
    fn sd_proxy_mode_sets_scheme_from_public_address() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"test-target");
        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels: HashMap::new(),
        };
        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:443", "https");

        assert_eq!(target.labels.get("__scheme__"), Some(&"https".to_string()));
    }

    #[test]
    fn sd_proxy_mode_filters_by_cluster_service_and_family() {
        let mk_target = |cluster: &str, service: &str, family: &str| {
            let uuid = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("{}:{}:{}", cluster, service, family).as_bytes(),
            );
            let mut labels = HashMap::new();
            labels.insert("__meta_ecs_cluster_name".to_string(), cluster.to_string());
            labels.insert("__meta_ecs_service_name".to_string(), service.to_string());
            labels.insert("__meta_ecs_task_family".to_string(), family.to_string());

            let proxy_target = ProxyTarget {
                address: "10.0.0.1:8080".to_string(),
                route_id: uuid,
                labels,
            };

            build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "http")
        };

        let targets = vec![
            mk_target("prod", "api", "api-task"),
            mk_target("prod", "worker", "worker-task"),
            mk_target("dev", "api", "api-task"),
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: vec!["api".to_string()],
            families: vec!["api-task".to_string()],
            level: Some(MetadataLevel::Aws),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(targets, &params);

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_cluster_name"),
            Some(&"prod".to_string())
        );
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_service_name"),
            Some(&"api".to_string())
        );
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_task_family"),
            Some(&"api-task".to_string())
        );
    }

    #[test]
    fn sd_proxy_mode_level_filtering_can_remove_filter_labels() {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"proxy-level-filter");
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
        labels.insert("__meta_ecs_service_name".to_string(), "api".to_string());
        labels.insert("__meta_ecs_task_family".to_string(), "api-task".to_string());

        let proxy_target = ProxyTarget {
            address: "10.0.0.1:8080".to_string(),
            route_id: uuid,
            labels,
        };

        let target = build_proxy_sd_target(&uuid, &proxy_target, "ecs-sd.internal:8080", "http");
        let container_level_target = filter_labels_by_level(&target, MetadataLevel::Container);

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::Container),
            filter_mode: FilterMode::And,
            tag_filters: Vec::new(),
        };

        let filtered = filter_targets(vec![container_level_target], &params);
        assert_eq!(
            filtered.len(), 0,
            "cluster filter cannot match after container-level label filtering"
        );
    }

    #[test]
    fn test_extract_tag_filters_allows_multiple_same_tag() {
        let pairs = parse_raw_query(Some("tag_task_env=prod&tag_task_env=staging&cluster=prod"));
        let filters = extract_tag_filters_from_pairs(&pairs);
        assert_eq!(
            filters,
            vec![
                ("task_env".to_string(), "prod".to_string()),
                ("task_env".to_string(), "staging".to_string())
            ]
        );
    }

    #[test]
    fn test_filter_same_tag_key_is_or() {
        // tag_env=prod&tag_env=staging → matches targets with env=prod OR env=staging
        let mut labels_prod = HashMap::new();
        labels_prod.insert("__meta_ecs_tag_env".to_string(), "prod".to_string());

        let mut labels_staging = HashMap::new();
        labels_staging.insert("__meta_ecs_tag_env".to_string(), "staging".to_string());

        let mut labels_dev = HashMap::new();
        labels_dev.insert("__meta_ecs_tag_env".to_string(), "dev".to_string());

        let targets = vec![
            Target { targets: vec!["10.0.0.1:8080".to_string()], labels: labels_prod },
            Target { targets: vec!["10.0.0.2:8080".to_string()], labels: labels_staging },
            Target { targets: vec!["10.0.0.3:8080".to_string()], labels: labels_dev },
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: None,
            filter_mode: FilterMode::And,
            tag_filters: vec![
                ("env".to_string(), "prod".to_string()),
                ("env".to_string(), "staging".to_string()),
            ],
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2, "prod and staging should both match");
    }

    #[test]
    fn test_filter_different_tag_keys_are_and() {
        // tag_env=prod&tag_team=obs → must have env=prod AND team=obs
        let mut labels_both = HashMap::new();
        labels_both.insert("__meta_ecs_tag_env".to_string(), "prod".to_string());
        labels_both.insert("__meta_ecs_tag_team".to_string(), "obs".to_string());

        let mut labels_only_env = HashMap::new();
        labels_only_env.insert("__meta_ecs_tag_env".to_string(), "prod".to_string());

        let targets = vec![
            Target { targets: vec!["10.0.0.1:8080".to_string()], labels: labels_both },
            Target { targets: vec!["10.0.0.2:8080".to_string()], labels: labels_only_env },
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: None,
            filter_mode: FilterMode::And,
            tag_filters: vec![
                ("env".to_string(), "prod".to_string()),
                ("team".to_string(), "obs".to_string()),
            ],
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1, "only target with both tags should match");
    }

    #[test]
    fn test_filter_mixed_tag_or_and() {
        // tag_env=prod&tag_env=staging&tag_team=obs → (env=prod OR staging) AND team=obs
        let mut labels_prod_obs = HashMap::new();
        labels_prod_obs.insert("__meta_ecs_tag_env".to_string(), "prod".to_string());
        labels_prod_obs.insert("__meta_ecs_tag_team".to_string(), "obs".to_string());

        let mut labels_staging_obs = HashMap::new();
        labels_staging_obs.insert("__meta_ecs_tag_env".to_string(), "staging".to_string());
        labels_staging_obs.insert("__meta_ecs_tag_team".to_string(), "obs".to_string());

        let mut labels_prod_platform = HashMap::new();
        labels_prod_platform.insert("__meta_ecs_tag_env".to_string(), "prod".to_string());
        labels_prod_platform.insert("__meta_ecs_tag_team".to_string(), "platform".to_string());

        let targets = vec![
            Target { targets: vec!["10.0.0.1:8080".to_string()], labels: labels_prod_obs },
            Target { targets: vec!["10.0.0.2:8080".to_string()], labels: labels_staging_obs },
            Target { targets: vec!["10.0.0.3:8080".to_string()], labels: labels_prod_platform },
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: None,
            filter_mode: FilterMode::And,
            tag_filters: vec![
                ("env".to_string(), "prod".to_string()),
                ("env".to_string(), "staging".to_string()),
                ("team".to_string(), "obs".to_string()),
            ],
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2, "prod+obs and staging+obs should match; prod+platform should not");
    }

    #[test]
    fn test_filter_tag_filters_default_and() {
        let mut labels1 = HashMap::new();
        labels1.insert("__meta_ecs_tag_task_env".to_string(), "prod".to_string());
        labels1.insert("__meta_ecs_tag_task_team".to_string(), "obs".to_string());

        let mut labels2 = HashMap::new();
        labels2.insert("__meta_ecs_tag_task_env".to_string(), "prod".to_string());

        let targets = vec![
            Target {
                targets: vec!["10.0.0.1:8080".to_string()],
                labels: labels1,
            },
            Target {
                targets: vec!["10.0.0.2:8080".to_string()],
                labels: labels2,
            },
        ];

        let params = SdQueryParams {
            clusters: Vec::new(),
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::Aws),
            filter_mode: FilterMode::And,
            tag_filters: vec![
                ("task_env".to_string(), "prod".to_string()),
                ("task_team".to_string(), "obs".to_string()),
            ],
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].targets, vec!["10.0.0.1:8080".to_string()]);
    }

    #[test]
    fn test_filter_or_mode_with_mixed_filters() {
        let mut labels1 = HashMap::new();
        labels1.insert("__meta_ecs_cluster_name".to_string(), "prod".to_string());
        labels1.insert("__meta_ecs_tag_task_team".to_string(), "obs".to_string());

        let mut labels2 = HashMap::new();
        labels2.insert("__meta_ecs_cluster_name".to_string(), "dev".to_string());
        labels2.insert("__meta_ecs_tag_task_team".to_string(), "platform".to_string());

        let targets = vec![
            Target {
                targets: vec!["10.0.0.1:8080".to_string()],
                labels: labels1,
            },
            Target {
                targets: vec!["10.0.0.2:8080".to_string()],
                labels: labels2,
            },
        ];

        let params = SdQueryParams {
            clusters: vec!["prod".to_string()],
            services: Vec::new(),
            families: Vec::new(),
            level: Some(MetadataLevel::Aws),
            filter_mode: FilterMode::Or,
            tag_filters: vec![("task_team".to_string(), "platform".to_string())],
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 2);
    }
}
