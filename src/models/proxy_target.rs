use std::collections::HashMap;

use uuid::Uuid;

use crate::models::Target;

#[derive(Debug, Clone)]
pub struct ProxyTarget {
    pub address: String,
    pub route_id: Uuid,
    pub labels: HashMap<String, String>,
}

pub fn route_id(task_arn: &str, container_name: &str, container_id: &str) -> Uuid {
    let input = format!("{}:{}:{}", task_arn, container_name, container_id);
    Uuid::new_v5(&Uuid::NAMESPACE_URL, input.as_bytes())
}

/// Build a routing table from a slice of AWS-level targets.
/// Each target that has a non-empty address and ECS ARN/name/id labels gets one entry.
pub fn build_routing_table(targets: &[Target]) -> HashMap<Uuid, ProxyTarget> {
    let mut table = HashMap::new();
    for target in targets {
        let address = match target.targets.first() {
            Some(a) => a.clone(),
            None => continue,
        };
        let task_arn = target
            .labels
            .get("__meta_ecs_task_arn")
            .map(|s| s.as_str())
            .unwrap_or("");
        let container_name = target
            .labels
            .get("__meta_ecs_container_name")
            .map(|s| s.as_str())
            .unwrap_or("");
        let container_id = target
            .labels
            .get("__meta_ecs_container_id")
            .map(|s| s.as_str())
            .unwrap_or("");
        let uuid = route_id(task_arn, container_name, container_id);
        table.insert(
            uuid,
            ProxyTarget {
                address,
                route_id: uuid,
                labels: target.labels.clone(),
            },
        );
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_id_is_deterministic() {
        let a = route_id("arn:task", "web", "abc123");
        let b = route_id("arn:task", "web", "abc123");
        assert_eq!(a, b);
    }

    #[test]
    fn route_id_differs_for_different_container_id() {
        let a = route_id("arn", "web", "c1");
        let b = route_id("arn", "web", "c2");
        assert_ne!(a, b);
    }

    #[test]
    fn route_id_separator_prevents_prefix_collision() {
        let a = route_id("a", "bc", "x");
        let b = route_id("ab", "c", "x");
        assert_ne!(a, b);
    }

    fn make_target(address: &str, task_arn: &str, container_name: &str, container_id: &str) -> Target {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_task_arn".to_string(), task_arn.to_string());
        labels.insert("__meta_ecs_container_name".to_string(), container_name.to_string());
        labels.insert("__meta_ecs_container_id".to_string(), container_id.to_string());
        Target {
            targets: if address.is_empty() { vec![] } else { vec![address.to_string()] },
            labels,
        }
    }

    #[test]
    fn build_routing_table_produces_entry_per_target() {
        let targets = vec![
            make_target("10.1.0.1:9090", "arn:task1", "web", "c1"),
            make_target("10.1.0.2:9090", "arn:task2", "api", "c2"),
        ];
        let table = build_routing_table(&targets);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn build_routing_table_entry_has_correct_address() {
        let targets = vec![make_target("10.1.2.3:9090", "arn:t", "web", "c1")];
        let table = build_routing_table(&targets);
        let entry = table.values().next().expect("should have one entry");
        assert_eq!(entry.address, "10.1.2.3:9090");
    }

    #[test]
    fn build_routing_table_copies_labels() {
        let targets = vec![make_target("10.1.2.3:9090", "arn:t", "web", "c1")];
        let table = build_routing_table(&targets);
        let entry = table.values().next().expect("should have one entry");
        assert_eq!(
            entry.labels.get("__meta_ecs_task_arn"),
            Some(&"arn:t".to_string())
        );
        assert_eq!(
            entry.labels.get("__meta_ecs_container_name"),
            Some(&"web".to_string())
        );
    }

    #[test]
    fn build_routing_table_uuid_is_deterministic() {
        let targets = vec![make_target("10.0.0.1:8080", "arn:t", "web", "c1")];
        let table1 = build_routing_table(&targets);
        let table2 = build_routing_table(&targets);
        let key1: Vec<Uuid> = table1.keys().cloned().collect();
        let key2: Vec<Uuid> = table2.keys().cloned().collect();
        assert_eq!(key1, key2);
    }

    #[test]
    fn build_routing_table_skips_target_with_no_address() {
        let targets = vec![make_target("", "arn:t", "web", "c1")];
        let table = build_routing_table(&targets);
        assert_eq!(table.len(), 0);
    }
}
