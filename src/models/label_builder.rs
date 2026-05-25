use crate::models::metadata_level::MetadataLevel;
use aws_sdk_ecs::types::{Cluster, ContainerDefinition, Service, Tag as EcsTag};
use aws_sdk_ecs::types::{Task, TaskDefinition};
use std::collections::HashMap;

pub struct LabelBuilder {
    level: MetadataLevel,
    container_data: Option<ContainerData>,
    network_data: Option<NetworkData>,
    task_data: Option<TaskData>,
    service_data: Option<ServiceData>,
    cluster_data: Option<ClusterData>,
    aws_data: Option<AwsData>,
    ec2_data: Option<Ec2Data>,
}

struct ContainerData {
    name: String,
    image: String,
    port: u16,
    scheme: String,
    metrics_path: String,
}

struct NetworkData {
    ip_address: String,
    network_mode: String,
    subnet_id: Option<String>,
}

struct TaskData {
    arn: String,
    definition_arn: String,
    family: String,
    version: String,
    group: Option<String>,
    launch_type: String,
    desired_status: Option<String>,
    last_status: Option<String>,
    health_status: Option<String>,
    platform_family: Option<String>,
    platform_version: Option<String>,
    tags: HashMap<String, String>,
}

struct ServiceData {
    name: String,
    arn: String,
    status: String,
    desired_count: i32,
    running_count: i32,
    tags: HashMap<String, String>,
}

struct ClusterData {
    name: String,
    arn: String,
    tags: HashMap<String, String>,
}

struct AwsData {
    region: String,
    account_id: String,
    availability_zone: Option<String>,
}

struct Ec2Data {
    container_instance_arn: String,
    instance_id: String,
    instance_type: Option<String>,
    private_ip: String,
    public_ip: Option<String>,
    tags: HashMap<String, String>,
}

fn ecs_tags_to_map(tags: &[EcsTag]) -> HashMap<String, String> {
    tags.iter()
        .filter_map(|t| {
            t.key()
                .zip(t.value())
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn sanitize_tag_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

impl LabelBuilder {
    pub fn new(level: MetadataLevel) -> Self {
        Self {
            level,
            container_data: None,
            network_data: None,
            task_data: None,
            service_data: None,
            cluster_data: None,
            aws_data: None,
            ec2_data: None,
        }
    }

    pub fn with_container(mut self, def: &ContainerDefinition, port: u16) -> Self {
        let scheme = def
            .docker_labels()
            .and_then(|l| l.get("prometheus.io/scheme"))
            .map(|s| s.clone())
            .unwrap_or_else(|| "http".to_string());

        let metrics_path = def
            .docker_labels()
            .and_then(|l| l.get("prometheus.io/path"))
            .map(|s| s.clone())
            .unwrap_or_else(|| "/metrics".to_string());

        self.container_data = Some(ContainerData {
            name: def.name().unwrap_or("unknown").to_string(),
            image: def.image().unwrap_or("unknown").to_string(),
            port,
            scheme,
            metrics_path,
        });
        self
    }

    pub fn with_network(mut self, ip_address: &str, network_mode: &str, subnet_id: Option<&str>) -> Self {
        self.network_data = Some(NetworkData {
            ip_address: ip_address.to_string(),
            network_mode: network_mode.to_string(),
            subnet_id: subnet_id.map(|s| s.to_string()),
        });
        self
    }

    pub fn with_task(mut self, task: &Task, task_def: &TaskDefinition) -> Self {
        let definition_arn = task_def
            .task_definition_arn()
            .unwrap_or("unknown")
            .to_string();

        let version = definition_arn
            .split(':')
            .last()
            .unwrap_or("unknown")
            .to_string();

        self.task_data = Some(TaskData {
            arn: task.task_arn().unwrap_or("unknown").to_string(),
            definition_arn,
            family: task_def.family().unwrap_or("unknown").to_string(),
            version,
            group: task.group().map(|s| s.to_string()),
            launch_type: task
                .launch_type()
                .map(|lt| lt.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            desired_status: task.desired_status().map(|s| s.to_string()),
            last_status: task.last_status().map(|s| s.to_string()),
            health_status: task
                .health_status()
                .map(|hs| hs.as_str().to_string()),
            platform_family: task.platform_family().map(|s| s.to_string()),
            platform_version: task.platform_version().map(|s| s.to_string()),
            tags: ecs_tags_to_map(task.tags()),
        });
        self
    }

    pub fn with_service(mut self, service: &Service) -> Self {
        self.service_data = Some(ServiceData {
            name: service.service_name().unwrap_or("unknown").to_string(),
            arn: service.service_arn().unwrap_or("unknown").to_string(),
            status: service.status().unwrap_or("unknown").to_string(),
            desired_count: service.desired_count(),
            running_count: service.running_count(),
            tags: ecs_tags_to_map(service.tags()),
        });
        self
    }

    pub fn with_cluster(mut self, cluster: &Cluster) -> Self {
        self.cluster_data = Some(ClusterData {
            name: cluster.cluster_name().unwrap_or("unknown").to_string(),
            arn: cluster.cluster_arn().unwrap_or("unknown").to_string(),
            tags: ecs_tags_to_map(cluster.tags()),
        });
        self
    }

    pub fn with_aws(mut self, region: &str, account_id: &str, az: Option<&str>) -> Self {
        self.aws_data = Some(AwsData {
            region: region.to_string(),
            account_id: account_id.to_string(),
            availability_zone: az.map(|s| s.to_string()),
        });
        self
    }

    pub fn with_ec2_instance(
        mut self,
        container_instance_arn: &str,
        instance_id: &str,
        instance_type: Option<&str>,
        private_ip: &str,
        public_ip: Option<&str>,
        tags: HashMap<String, String>,
    ) -> Self {
        self.ec2_data = Some(Ec2Data {
            container_instance_arn: container_instance_arn.to_string(),
            instance_id: instance_id.to_string(),
            instance_type: instance_type.map(|s| s.to_string()),
            private_ip: private_ip.to_string(),
            public_ip: public_ip.map(|s| s.to_string()),
            tags,
        });
        self
    }

    pub fn build(self) -> HashMap<String, String> {
        let mut labels = HashMap::new();

        // Container level
        if self.level.includes(MetadataLevel::Container) {
            if let Some(data) = self.container_data {
                labels.insert("__meta_ecs_container_name".to_string(), data.name);
                labels.insert("__meta_ecs_container_image".to_string(), data.image);
                labels.insert("__meta_ecs_metrics_port".to_string(), data.port.to_string());
                labels.insert("__scheme__".to_string(), data.scheme);
                labels.insert("__metrics_path__".to_string(), data.metrics_path);
            }
            if let Some(data) = self.network_data {
                labels.insert("__meta_ecs_ip_address".to_string(), data.ip_address);
                labels.insert("__meta_ecs_network_mode".to_string(), data.network_mode);
                if let Some(subnet) = data.subnet_id {
                    labels.insert("__meta_ecs_subnet_id".to_string(), subnet);
                }
            }
        }

        // Task level
        if self.level.includes(MetadataLevel::Task) {
            if let Some(data) = self.task_data {
                labels.insert("__meta_ecs_task_arn".to_string(), data.arn);
                labels.insert("__meta_ecs_task_definition".to_string(), data.definition_arn);
                labels.insert("__meta_ecs_task_family".to_string(), data.family);
                labels.insert("__meta_ecs_task_version".to_string(), data.version);
                labels.insert("__meta_ecs_launch_type".to_string(), data.launch_type);
                if let Some(group) = data.group {
                    labels.insert("__meta_ecs_task_group".to_string(), group);
                }
                if let Some(v) = data.desired_status {
                    labels.insert("__meta_ecs_desired_status".to_string(), v);
                }
                if let Some(v) = data.last_status {
                    labels.insert("__meta_ecs_last_status".to_string(), v);
                }
                if let Some(v) = data.health_status {
                    labels.insert("__meta_ecs_health_status".to_string(), v);
                }
                if let Some(v) = data.platform_family {
                    labels.insert("__meta_ecs_platform_family".to_string(), v);
                }
                if let Some(v) = data.platform_version {
                    labels.insert("__meta_ecs_platform_version".to_string(), v);
                }
                for (k, v) in data.tags {
                    labels.insert(
                        format!("__meta_ecs_tag_task_{}", sanitize_tag_key(&k)),
                        v,
                    );
                }
            }
        }

        // Service level
        if self.level.includes(MetadataLevel::Service) {
            if let Some(data) = self.service_data {
                labels.insert("__meta_ecs_service".to_string(), data.name);
                labels.insert("__meta_ecs_service_arn".to_string(), data.arn);
                labels.insert("__meta_ecs_service_status".to_string(), data.status);
                labels.insert("__meta_ecs_desired_count".to_string(), data.desired_count.to_string());
                labels.insert("__meta_ecs_running_count".to_string(), data.running_count.to_string());
                for (k, v) in data.tags {
                    labels.insert(
                        format!("__meta_ecs_tag_service_{}", sanitize_tag_key(&k)),
                        v,
                    );
                }
            }
        }

        // Cluster level
        if self.level.includes(MetadataLevel::Cluster) {
            if let Some(data) = self.cluster_data {
                labels.insert("__meta_ecs_cluster".to_string(), data.name);
                labels.insert("__meta_ecs_cluster_arn".to_string(), data.arn);
                for (k, v) in data.tags {
                    labels.insert(
                        format!("__meta_ecs_tag_cluster_{}", sanitize_tag_key(&k)),
                        v,
                    );
                }
            }
        }

        // AWS level
        if self.level.includes(MetadataLevel::Aws) {
            if let Some(data) = self.aws_data {
                labels.insert("__meta_ecs_region".to_string(), data.region);
                labels.insert("__meta_ecs_account_id".to_string(), data.account_id);
                if let Some(az) = data.availability_zone {
                    labels.insert("__meta_ecs_availability_zone".to_string(), az);
                }
            }
            if let Some(data) = self.ec2_data {
                labels.insert("__meta_ecs_container_instance_arn".to_string(), data.container_instance_arn);
                labels.insert("__meta_ecs_ec2_instance_id".to_string(), data.instance_id);
                labels.insert("__meta_ecs_ec2_instance_private_ip".to_string(), data.private_ip.clone());
                // __meta_ecs_public_ip = EC2 public IP for bridge/host mode
                if let Some(ref ip) = data.public_ip {
                    labels.insert("__meta_ecs_public_ip".to_string(), ip.clone());
                    labels.insert("__meta_ecs_ec2_instance_public_ip".to_string(), ip.clone());
                }
                if let Some(v) = data.instance_type {
                    labels.insert("__meta_ecs_ec2_instance_type".to_string(), v);
                }
                for (k, v) in data.tags {
                    labels.insert(
                        format!("__meta_ecs_tag_ec2_{}", sanitize_tag_key(&k)),
                        v,
                    );
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
    fn test_label_builder_empty() {
        let labels = LabelBuilder::new(MetadataLevel::Container).build();
        assert!(labels.is_empty());
    }

    #[test]
    fn test_label_builder_includes_hierarchy() {
        let builder = LabelBuilder::new(MetadataLevel::Container);
        assert!(builder.level.includes(MetadataLevel::Container));
        assert!(!builder.level.includes(MetadataLevel::Task));

        let builder = LabelBuilder::new(MetadataLevel::Aws);
        assert!(builder.level.includes(MetadataLevel::Container));
        assert!(builder.level.includes(MetadataLevel::Task));
        assert!(builder.level.includes(MetadataLevel::Service));
        assert!(builder.level.includes(MetadataLevel::Cluster));
        assert!(builder.level.includes(MetadataLevel::Aws));
    }

    #[test]
    fn test_sanitize_tag_key() {
        assert_eq!(sanitize_tag_key("my-tag"), "my_tag");
        assert_eq!(sanitize_tag_key("My.Tag.Key"), "my_tag_key");
        assert_eq!(sanitize_tag_key("already_valid"), "already_valid");
        assert_eq!(sanitize_tag_key("Tag:Name"), "tag_name");
    }

    #[test]
    fn test_network_labels() {
        let builder = LabelBuilder::new(MetadataLevel::Container)
            .with_network("10.0.1.5", "bridge", Some("subnet-abc123"));
        let labels = builder.build();
        assert_eq!(labels.get("__meta_ecs_ip_address").map(String::as_str), Some("10.0.1.5"));
        assert_eq!(labels.get("__meta_ecs_network_mode").map(String::as_str), Some("bridge"));
        assert_eq!(labels.get("__meta_ecs_subnet_id").map(String::as_str), Some("subnet-abc123"));
    }

    #[test]
    fn test_ec2_tags_emitted_at_aws_level() {
        let mut ec2_tags = HashMap::new();
        ec2_tags.insert("Name".to_string(), "my-node".to_string());
        ec2_tags.insert("env".to_string(), "prod".to_string());

        let builder = LabelBuilder::new(MetadataLevel::Aws).with_ec2_instance(
            "arn:aws:ecs:us-east-1:123:container-instance/abc",
            "i-0123456789abcdef0",
            Some("t3.medium"),
            "10.0.1.5",
            Some("1.2.3.4"),
            ec2_tags,
        );
        let labels = builder.build();
        assert_eq!(labels.get("__meta_ecs_ec2_instance_type").map(String::as_str), Some("t3.medium"));
        assert_eq!(labels.get("__meta_ecs_tag_ec2_name").map(String::as_str), Some("my-node"));
        assert_eq!(labels.get("__meta_ecs_tag_ec2_env").map(String::as_str), Some("prod"));
        assert_eq!(labels.get("__meta_ecs_public_ip").map(String::as_str), Some("1.2.3.4"));
        assert_eq!(labels.get("__meta_ecs_ec2_instance_public_ip").map(String::as_str), Some("1.2.3.4"));
    }

    #[test]
    fn test_aws_level_excludes_ec2_at_cluster_level() {
        let mut ec2_tags = HashMap::new();
        ec2_tags.insert("Name".to_string(), "my-node".to_string());

        let builder = LabelBuilder::new(MetadataLevel::Cluster).with_ec2_instance(
            "arn:instance",
            "i-abc",
            None,
            "10.0.0.1",
            None,
            ec2_tags,
        );
        let labels = builder.build();
        assert!(!labels.contains_key("__meta_ecs_ec2_instance_id"));
        assert!(!labels.contains_key("__meta_ecs_tag_ec2_name"));
    }
}
