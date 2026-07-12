use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MetadataLevel {
    Container,
    Task,
    Service,
    Cluster,
    Aws,
}

impl Default for MetadataLevel {
    fn default() -> Self {
        MetadataLevel::Task
    }
}

impl FromStr for MetadataLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "container" => Ok(MetadataLevel::Container),
            "task" => Ok(MetadataLevel::Task),
            "service" => Ok(MetadataLevel::Service),
            "cluster" => Ok(MetadataLevel::Cluster),
            "aws" => Ok(MetadataLevel::Aws),
            _ => Err(format!(
                "Invalid level: {}. Valid: container, task, service, cluster, aws",
                s
            )),
        }
    }
}

impl fmt::Display for MetadataLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataLevel::Container => write!(f, "container"),
            MetadataLevel::Task => write!(f, "task"),
            MetadataLevel::Service => write!(f, "service"),
            MetadataLevel::Cluster => write!(f, "cluster"),
            MetadataLevel::Aws => write!(f, "aws"),
        }
    }
}

impl MetadataLevel {
    /// Returns true if self includes the given level
    /// e.g., Aws.includes(Task) == true, Task.includes(Aws) == false
    pub fn includes(&self, other: MetadataLevel) -> bool {
        use MetadataLevel::*;
        match (*self, other) {
            (Aws, _) => true,
            (Cluster, Container)
            | (Cluster, Task)
            | (Cluster, Service)
            | (Cluster, Cluster) => true,
            (Service, Container) | (Service, Task) | (Service, Service) => true,
            (Task, Container) | (Task, Task) => true,
            (Container, Container) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_task() {
        assert_eq!(MetadataLevel::default(), MetadataLevel::Task);
    }

    #[test]
    fn test_from_str_case_insensitive() {
        assert_eq!("container".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
        assert_eq!("CONTAINER".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
        assert_eq!("Container".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
    }

    #[test]
    fn test_from_str_invalid() {
        assert!("invalid".parse::<MetadataLevel>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(MetadataLevel::Container.to_string(), "container");
        assert_eq!(MetadataLevel::Task.to_string(), "task");
        assert_eq!(MetadataLevel::Service.to_string(), "service");
        assert_eq!(MetadataLevel::Cluster.to_string(), "cluster");
        assert_eq!(MetadataLevel::Aws.to_string(), "aws");
    }

    #[test]
    fn test_includes_hierarchy() {
        // Aws includes all
        assert!(MetadataLevel::Aws.includes(MetadataLevel::Container));
        assert!(MetadataLevel::Aws.includes(MetadataLevel::Aws));

        // Container includes only itself
        assert!(MetadataLevel::Container.includes(MetadataLevel::Container));
        assert!(!MetadataLevel::Container.includes(MetadataLevel::Task));

        // Task includes container and task
        assert!(MetadataLevel::Task.includes(MetadataLevel::Container));
        assert!(MetadataLevel::Task.includes(MetadataLevel::Task));
        assert!(!MetadataLevel::Task.includes(MetadataLevel::Service));

        // Service includes container, task, service
        assert!(MetadataLevel::Service.includes(MetadataLevel::Container));
        assert!(MetadataLevel::Service.includes(MetadataLevel::Task));
        assert!(MetadataLevel::Service.includes(MetadataLevel::Service));
        assert!(!MetadataLevel::Service.includes(MetadataLevel::Cluster));
    }
}
