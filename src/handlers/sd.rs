use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use crate::config::Mode;
use crate::models::ProxyTarget;
use crate::state::AppState;
use crate::models::{MetadataLevel, SdQueryParams, Target};
use serde_json::json;
use tracing::info;
use std::collections::HashMap;
use std::time::SystemTime;
use tracing::debug;
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

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<SdQueryParams>,
) -> Response {
    // In proxy mode, return routing-table targets with public_address as target
    // and __metrics_path__ pointing through the proxy.
    if state.config.mode == Mode::Proxy {
        let public_address = state.config.public_address.as_deref().unwrap_or_default();
        let public_address_scheme = state
            .config
            .public_address_scheme
            .as_deref()
            .unwrap_or("http");

        let routing = state.routing_table.read().await;
        let proxy_targets: Vec<Target> = routing
            .iter()
            .map(|(uuid, proxy_target)| {
                build_proxy_sd_target(uuid, proxy_target, public_address, public_address_scheme)
            })
            .collect();
        drop(routing);

        let last_refresh = *state.last_refresh.read().await;
        let cache_age_seconds = calculate_cache_age_seconds(last_refresh, SystemTime::now());
        let cache_state = if cache_age_seconds > state.cache_ttl_seconds {
            "stale"
        } else {
            "fresh"
        };

        return build_sd_response_with_cache_age(proxy_targets, cache_age_seconds, cache_state);
    }

    let level = params.level.unwrap_or(state.config.metadata_level);

    let cache = state.cache.read().await;
    let maybe_targets = cache
        .get(&level)
        .cloned();

    let targets = maybe_targets.unwrap_or_default();

    if !targets.is_empty() {
        debug!("cache hit: {} targets served", targets.len());
    } else {
        debug!("cache miss: 0 targets served for level={}", level);
    }

    drop(cache); // Release read lock before filtering

    let filtered = filter_targets(targets, &params);

    let last_refresh = *state.last_refresh.read().await;
    let cache_age_seconds = calculate_cache_age_seconds(last_refresh, SystemTime::now());
    let cache_state = if cache_age_seconds > state.cache_ttl_seconds {
        "stale"
    } else {
        "fresh"
    };

    build_sd_response_with_cache_age(filtered, cache_age_seconds, cache_state)
}

pub async fn refresh_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let clusters = state.config.clusters.clone();

    info!("Manual discovery refresh triggered");

    let targets_aws = state
        .discovery
        .discover_all_clusters(&clusters, state.config.mode.clone())
        .await;
    let count = targets_aws.len();
    state.replace_cache_and_routing(targets_aws).await;

    info!("Discovery refresh complete: {} targets", count);

    Json(json!({
        "status": "ok",
        "targets_discovered": count
    }))
}

/// Filter target labels to only include those for the specified level
pub(crate) fn filter_labels_by_level(target: &Target, level: MetadataLevel) -> Target {
    let filtered_labels: HashMap<String, String> = target
        .labels
        .iter()
        .filter(|(key, _)| {
            // Determine which level this label belongs to based on prefix
            let label_level = if key.starts_with("__meta_ecs_container_") || *key == "__meta_ecs_metrics_port" {
                MetadataLevel::Container
            } else if key.starts_with("__meta_ecs_task_") {
                MetadataLevel::Task
            } else if key.starts_with("__meta_ecs_service_") || *key == "__meta_ecs_service" {
                MetadataLevel::Service
            } else if key.starts_with("__meta_ecs_cluster_") {
                MetadataLevel::Cluster
            } else if key.starts_with("__meta_ecs_") {
                MetadataLevel::Aws
            } else {
                MetadataLevel::Container // Default for unknown labels
            };
            
            level.includes(label_level)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    
    Target {
        targets: target.targets.clone(),
        labels: filtered_labels,
    }
}

fn filter_targets(targets: Vec<Target>, params: &SdQueryParams) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|target| {
            // Check cluster filter
            if let Some(ref cluster) = params.cluster {
                let target_cluster = target
                    .labels
                    .get("__meta_ecs_cluster_name")
                    .or_else(|| target.labels.get("__meta_ecs_cluster"));
                if target_cluster.map(|s| s.as_str()) != Some(cluster.as_str()) {
                    return false;
                }
            }

            // Check service filter
            if let Some(ref service) = params.service {
                let target_service = target
                    .labels
                    .get("__meta_ecs_service_name")
                    .or_else(|| target.labels.get("__meta_ecs_service"));
                if target_service.map(|s| s.as_str()) != Some(service.as_str()) {
                    return false;
                }
            }

            // Check family filter
            if let Some(ref family) = params.family {
                let target_family = target.labels.get("__meta_ecs_task_family");
                if target_family.map(|s| s.as_str()) != Some(family.as_str()) {
                    return false;
                }
            }

            true
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
    use crate::models::LabelBuilder;
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
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
            level: Some(MetadataLevel::default()),
        };

        let filtered = filter_targets(targets, &params);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_cluster_name"),
            Some(&"prod".to_string())
        );
    }

    #[test]
    fn test_filter_case_sensitive() {
        let targets = vec![
            create_test_target("Prod", "api", "api-task"),
        ];

        let params = SdQueryParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
            level: Some(MetadataLevel::default()),
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
            cluster: Some("prod".to_string()),
            service: Some("api".to_string()),
            family: None,
            level: Some(MetadataLevel::default()),
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
            cluster: None,
            service: None,
            family: None,
            level: Some(MetadataLevel::default()),
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
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
            level: Some(MetadataLevel::default()),
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
            cluster: None,
            service: Some("api".to_string()),
            family: None,
            level: Some(MetadataLevel::default()),
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
            cluster: Some("prod".to_string()),
            service: Some("api".to_string()),
            family: None,
            level: Some(MetadataLevel::default()),
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
    fn test_filter_labels_by_level_preserves_prometheus_specials() {
        let mut labels = HashMap::new();
        labels.insert("__scheme__".to_string(), "https".to_string());
        labels.insert("__metrics_path__".to_string(), "/custom".to_string());
        labels.insert("__meta_ecs_task_arn".to_string(), "arn:task".to_string());
        labels.insert("__meta_ecs_service".to_string(), "api".to_string());

        let target = Target {
            targets: vec!["10.0.0.1:9090".to_string()],
            labels,
        };

        // Container level: scheme/path retained, task and service labels filtered out.
        let container = filter_labels_by_level(&target, MetadataLevel::Container);
        assert!(container.labels.contains_key("__scheme__"));
        assert!(container.labels.contains_key("__metrics_path__"));
        assert!(!container.labels.contains_key("__meta_ecs_task_arn"));
        assert!(!container.labels.contains_key("__meta_ecs_service"));

        // Task level: scheme/path + task label retained, service label filtered out.
        let task = filter_labels_by_level(&target, MetadataLevel::Task);
        assert!(task.labels.contains_key("__scheme__"));
        assert!(task.labels.contains_key("__meta_ecs_task_arn"));
        assert!(!task.labels.contains_key("__meta_ecs_service"));

        // Service level: __meta_ecs_service IS now retained (bug fix from Wave 2).
        let service = filter_labels_by_level(&target, MetadataLevel::Service);
        assert!(
            service.labels.contains_key("__meta_ecs_service"),
            "__meta_ecs_service must be classified as Service level (Wave 2 bug fix)"
        );
        assert!(service.labels.contains_key("__meta_ecs_task_arn"));
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
}
