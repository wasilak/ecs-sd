use axum::{
    extract::{Query, State},
    Json,
};
use crate::state::AppState;
use crate::models::{Target, FilterParams};
use serde_json::json;
use tracing::info;

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Json<Vec<Target>> {
    let targets = state.cache.read().await.clone();
    let filtered = filter_targets(targets, params);
    Json(filtered)
}

pub async fn refresh_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let clusters = state.config.clusters.clone();

    info!("Manual discovery refresh triggered");

    let targets = state.discovery.discover_all_clusters(&clusters).await;
    let count = targets.len();

    // Update cache
    {
        let mut cache = state.cache.write().await;
        *cache = targets;
    }

    info!("Discovery refresh complete: {} targets", count);

    Json(json!({
        "status": "ok",
        "targets_discovered": count
    }))
}

fn filter_targets(targets: Vec<Target>, params: FilterParams) -> Vec<Target> {
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

        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };

        let filtered = filter_targets(targets, params);
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

        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };

        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 0); // Case-sensitive: Prod != prod
    }

    #[test]
    fn test_filter_and_logic() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("prod", "web", "web-task"),
            create_test_target("dev", "api", "api-task"),
        ];

        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: Some("api".to_string()),
            family: None,
        };

        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_no_params() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "web", "web-task"),
        ];

        let params = FilterParams {
            cluster: None,
            service: None,
            family: None,
        };

        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 2); // No filtering returns all
    }
}
