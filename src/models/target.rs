use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
    /// Construct a Target from an IP, port, and label set.
    /// Produces a single-entry `targets` vector formatted as `"{ip}:{port}"`.
    pub fn new(ip: &str, port: u16, labels: HashMap<String, String>) -> Self {
        Self {
            targets: vec![format!("{}:{}", ip, port)],
            labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_address_format() {
        let t = Target::new("10.0.1.5", 9090, HashMap::new());
        assert_eq!(t.targets, vec!["10.0.1.5:9090".to_string()]);
    }

    #[test]
    fn test_target_address_low_port() {
        let t = Target::new("192.168.0.1", 80, HashMap::new());
        assert_eq!(t.targets, vec!["192.168.0.1:80".to_string()]);
    }

    #[test]
    fn test_target_labels_passed_through() {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster".to_string(), "prod".to_string());
        labels.insert("__meta_ecs_service".to_string(), "api".to_string());

        let t = Target::new("10.0.0.1", 8080, labels.clone());
        assert_eq!(t.labels, labels);
        assert_eq!(t.targets, vec!["10.0.0.1:8080".to_string()]);
    }
}
