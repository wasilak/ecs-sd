use std::collections::HashMap;

use crate::models::{MetadataLevel, Target};

/// Filter target labels to only include those for the specified level
pub fn filter_labels_by_level(target: &Target, level: MetadataLevel) -> Target {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
