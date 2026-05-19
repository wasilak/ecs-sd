use crate::models::metadata_level::MetadataLevel;
use aws_sdk_ecs::types::{ContainerDefinition, Service, Cluster};
use aws_sdk_ecs::types::{Task, TaskDefinition};
use std::collections::HashMap;

pub struct LabelBuilder {
    level: MetadataLevel,
    container_data: Option<ContainerData>,
    task_data: Option<TaskData>,
    service_data: Option<ServiceData>,
    cluster_data: Option<ClusterData>,
    aws_data: Option<AwsData>,
}

struct ContainerData {
    name: String,
    image: String,
    port: u16,
}

struct TaskData {
    arn: String,
    family: String,
    version: String,
}

struct ServiceData {
    name: String,
    desired_count: i32,
    running_count: i32,
}

struct ClusterData {
    name: String,
    arn: String,
}

struct AwsData {
    region: String,
    account_id: String,
    availability_zone: Option<String>,
}

impl LabelBuilder {
    pub fn new(level: MetadataLevel) -> Self {
        Self {
            level,
            container_data: None,
            task_data: None,
            service_data: None,
            cluster_data: None,
            aws_data: None,
        }
    }

    pub fn with_container(mut self, def: &ContainerDefinition, port: u16) -> Self {
        self.container_data = Some(ContainerData {
            name: def.name().unwrap_or("unknown").to_string(),
            image: def.image().unwrap_or("unknown").to_string(),
            port,
        });
        self
    }

    pub fn with_task(mut self, task: &Task, task_def: &TaskDefinition) -> Self {
        let version = task_def
            .task_definition_arn()
            .and_then(|arn| arn.split(':').last())
            .unwrap_or("unknown")
            .to_string();

        self.task_data = Some(TaskData {
            arn: task.task_arn().unwrap_or("unknown").to_string(),
            family: task_def.family().unwrap_or("unknown").to_string(),
            version,
        });
        self
    }

    pub fn with_service(mut self, service: &Service) -> Self {
        self.service_data = Some(ServiceData {
            name: service.service_name().unwrap_or("unknown").to_string(),
            desired_count: service.desired_count(),
            running_count: service.running_count(),
        });
        self
    }

    pub fn with_cluster(mut self, cluster: &Cluster) -> Self {
        self.cluster_data = Some(ClusterData {
            name: cluster.cluster_name().unwrap_or("unknown").to_string(),
            arn: cluster.cluster_arn().unwrap_or("unknown").to_string(),
        });
        self
    }

    pub fn with_aws(
        mut self,
        region: &str,
        account_id: &str,
        az: Option<&str>,
    ) -> Self {
        self.aws_data = Some(AwsData {
            region: region.to_string(),
            account_id: account_id.to_string(),
            availability_zone: az.map(|s| s.to_string()),
        });
        self
    }

    pub fn build(self) -> HashMap<String, String> {
        let mut labels = HashMap::new();

        // Container level labels (META-01, META-02, META-03)
        if self.level.includes(MetadataLevel::Container) {
            if let Some(data) = self.container_data {
                labels.insert("__meta_ecs_container_name".to_string(), data.name);
                labels.insert("__meta_ecs_container_image".to_string(), data.image);
                labels.insert("__meta_ecs_metrics_port".to_string(), data.port.to_string());
            }
        }

        // Task level labels (META-04, META-05, META-06)
        if self.level.includes(MetadataLevel::Task) {
            if let Some(data) = self.task_data {
                labels.insert("__meta_ecs_task_arn".to_string(), data.arn);
                labels.insert("__meta_ecs_task_family".to_string(), data.family);
                labels.insert("__meta_ecs_task_version".to_string(), data.version);
            }
        }

        // Service level labels (META-07, META-08, META-09)
        if self.level.includes(MetadataLevel::Service) {
            if let Some(data) = self.service_data {
                labels.insert("__meta_ecs_service_name".to_string(), data.name);
                labels.insert(
                    "__meta_ecs_desired_count".to_string(),
                    data.desired_count.to_string(),
                );
                labels.insert(
                    "__meta_ecs_running_count".to_string(),
                    data.running_count.to_string(),
                );
            }
        }

        // Cluster level labels (META-10, META-11)
        if self.level.includes(MetadataLevel::Cluster) {
            if let Some(data) = self.cluster_data {
                labels.insert("__meta_ecs_cluster_name".to_string(), data.name);
                labels.insert("__meta_ecs_cluster_arn".to_string(), data.arn);
            }
        }

        // AWS level labels (META-12, META-13, META-14)
        if self.level.includes(MetadataLevel::Aws) {
            if let Some(data) = self.aws_data {
                labels.insert("__meta_ecs_region".to_string(), data.region);
                labels.insert("__meta_ecs_account_id".to_string(), data.account_id);
                if let Some(az) = data.availability_zone {
                    labels.insert("__meta_ecs_availability_zone".to_string(), az);
                }
            }
        }

        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_builder_container_level() {
        let labels = LabelBuilder::new(MetadataLevel::Container).build();
        assert!(labels.is_empty()); // No data added, so no labels
    }

    #[test]
    fn test_label_builder_includes_hierarchy() {
        // Container level only includes container labels
        let builder = LabelBuilder::new(MetadataLevel::Container);
        assert!(builder.level.includes(MetadataLevel::Container));
        assert!(!builder.level.includes(MetadataLevel::Task));

        // Aws level includes all
        let builder = LabelBuilder::new(MetadataLevel::Aws);
        assert!(builder.level.includes(MetadataLevel::Container));
        assert!(builder.level.includes(MetadataLevel::Task));
        assert!(builder.level.includes(MetadataLevel::Service));
        assert!(builder.level.includes(MetadataLevel::Cluster));
        assert!(builder.level.includes(MetadataLevel::Aws));
    }
}
