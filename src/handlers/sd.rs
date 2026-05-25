use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use crate::state::AppState;
use crate::models::{MetadataLevel, SdQueryParams, Target};
use serde_json::json;
use tracing::info;
use std::collections::HashMap;
use std::time::SystemTime;
use tracing::debug;

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<SdQueryParams>,
) -> Response {
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

    // Discover at full Aws level
    let targets_aws = state.discovery.discover_all_clusters(&clusters).await;
    let count = targets_aws.len();

    // Derive all cache tiers from Aws-level targets
    let targets_cluster: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
        .collect();
    let targets_service: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Service))
        .collect();
    let targets_task: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Task))
        .collect();
    let targets_container: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Container))
        .collect();

    // Update all cache tiers atomically
    {
        let mut cache = state.cache.write().await;
        cache.insert(MetadataLevel::Aws, targets_aws);
        cache.insert(MetadataLevel::Cluster, targets_cluster);
        cache.insert(MetadataLevel::Service, targets_service);
        cache.insert(MetadataLevel::Task, targets_task);
        cache.insert(MetadataLevel::Container, targets_container);
    }

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
                let target_cluster = target.labels.get("__meta_ecs_cluster_name");
                if target_cluster.map(|s| s.as_str()) != Some(cluster.as_str()) {
                    return false;
                }
            }

            // Check service filter
            if let Some(ref service) = params.service {
                let target_service = target.labels.get("__meta_ecs_service_name");
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
}
