use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ProxyTarget {
    pub address: String,
    pub route_id: Uuid,
}

pub fn route_id(task_arn: &str, container_name: &str, container_id: &str) -> Uuid {
    let input = format!("{}:{}:{}", task_arn, container_name, container_id);
    Uuid::new_v5(&Uuid::NAMESPACE_URL, input.as_bytes())
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
}
