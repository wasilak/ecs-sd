use crate::error::DiscoveryError;
use crate::models::{LabelBuilder, MetadataLevel, Target};
use aws_sdk_ecs::types::{ClusterField, LaunchType, ServiceField, TaskField};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    account_id: String,
    region: String,
}

struct Ec2InstanceInfo {
    private_ip: String,
    public_ip: Option<String>,
    availability_zone: Option<String>,
    subnet_id: Option<String>,
    container_instance_arn: String,
    ec2_instance_id: String,
    ec2_instance_type: Option<String>,
    ec2_tags: HashMap<String, String>,
}

impl DiscoveryService {
    pub async fn new(
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,
        region: String,
    ) -> Result<Self, DiscoveryError> {
        let caller_identity = sts_client
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| DiscoveryError::StsError(e.to_string()))?;

        let account_id = caller_identity
            .account()
            .ok_or_else(|| DiscoveryError::StsError("No account ID in response".to_string()))?
            .to_string();

        Ok(Self {
            ecs_client,
            ec2_client,
            account_id,
            region,
        })
    }

    pub async fn discover_all_clusters(&self, cluster_names: &[String]) -> Vec<Target> {
        let mut all_targets = Vec::new();

        for cluster_name in cluster_names {
            info!("Discovering cluster: {}", cluster_name);

            match self.discover_cluster_targets(cluster_name).await {
                Ok(targets) => {
                    info!(
                        "Cluster {}: discovered {} targets",
                        cluster_name,
                        targets.len()
                    );
                    all_targets.extend(targets);
                }
                Err(e) => {
                    error!("Failed to discover cluster {}: {}", cluster_name, e);
                }
            }
        }

        info!("Total targets discovered: {}", all_targets.len());
        all_targets
    }

    async fn discover_cluster_targets(
        &self,
        cluster_name: &str,
    ) -> Result<Vec<Target>, DiscoveryError> {
        let mut targets = Vec::new();

        let clusters = self
            .ecs_client
            .describe_clusters()
            .set_clusters(Some(vec![cluster_name.to_string()]))
            .include(ClusterField::Tags)
            .send()
            .await
            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

        if clusters.clusters().is_empty() {
            return Err(DiscoveryError::ClusterNotFound(cluster_name.to_string()));
        }

        let cluster = &clusters.clusters()[0];
        let cluster_arn = cluster
            .cluster_arn()
            .ok_or_else(|| DiscoveryError::ClusterNotFound(cluster_name.to_string()))?;
        let cluster_name = cluster.cluster_name().unwrap_or(cluster_name);

        let service_arns = self.list_all_services(cluster_arn).await?;
        debug!("Found {} services in cluster {}", service_arns.len(), cluster_name);

        for service_arn_chunk in service_arns.chunks(10) {
            let services = self
                .ecs_client
                .describe_services()
                .cluster(cluster_arn)
                .set_services(Some(service_arn_chunk.to_vec()))
                .include(ServiceField::Tags)
                .send()
                .await
                .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

            for service in services.services() {
                let service_name = service.service_name().unwrap_or("unknown");

                let task_arns = self.list_service_tasks(cluster_arn, service_name).await?;
                if task_arns.is_empty() {
                    continue;
                }

                for task_chunk in task_arns.chunks(100) {
                    let tasks = self
                        .ecs_client
                        .describe_tasks()
                        .cluster(cluster_arn)
                        .set_tasks(Some(task_chunk.to_vec()))
                        .include(TaskField::Tags)
                        .send()
                        .await
                        .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

                    for task in tasks.tasks() {
                        if task.launch_type() != Some(&LaunchType::Ec2) {
                            debug!("Skipping non-EC2 task: {:?}", task.task_arn());
                            continue;
                        }

                        if let Some(status) = task.last_status() {
                            if status == "STOPPED" || status == "STOPPING" {
                                continue;
                            }
                        }

                        let task_def_arn = task
                            .task_definition_arn()
                            .ok_or(DiscoveryError::NoContainerInstance)?;

                        let task_def_resp = self
                            .ecs_client
                            .describe_task_definition()
                            .task_definition(task_def_arn)
                            .send()
                            .await
                            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

                        let Some(task_def) = task_def_resp.task_definition() else {
                            continue;
                        };

                        let network_mode = task_def
                            .network_mode()
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_else(|| "bridge".to_string());

                        for container_def in task_def.container_definitions() {
                            let should_scrape = container_def
                                .docker_labels()
                                .and_then(|l| l.get("prometheus.io/scrape"))
                                .map(|v| v == "true")
                                .unwrap_or(false);

                            if !should_scrape {
                                continue;
                            }

                            let port = match container_def
                                .docker_labels()
                                .and_then(|l| l.get("prometheus.io/port"))
                                .and_then(|p| p.parse::<u16>().ok())
                            {
                                Some(p) => p,
                                None => {
                                    warn!(
                                        "Container {} has prometheus.io/scrape=true but no valid prometheus.io/port",
                                        container_def.name().unwrap_or("unknown")
                                    );
                                    continue;
                                }
                            };

                            let container_instance_arn = match task.container_instance_arn() {
                                Some(arn) => arn,
                                None => {
                                    warn!("Task has no container instance");
                                    continue;
                                }
                            };

                            match self
                                .resolve_ec2_instance(cluster_arn, container_instance_arn)
                                .await
                            {
                                Ok(ec2) => {
                                    let labels = LabelBuilder::new(MetadataLevel::Aws)
                                        .with_container(container_def, port)
                                        .with_network(
                                            &ec2.private_ip,
                                            &network_mode,
                                            ec2.subnet_id.as_deref(),
                                        )
                                        .with_task(task, task_def)
                                        .with_service(service)
                                        .with_cluster(cluster)
                                        .with_aws(
                                            &self.region,
                                            &self.account_id,
                                            ec2.availability_zone.as_deref(),
                                        )
                                        .with_ec2_instance(
                                            &ec2.container_instance_arn,
                                            &ec2.ec2_instance_id,
                                            ec2.ec2_instance_type.as_deref(),
                                            &ec2.private_ip,
                                            ec2.public_ip.as_deref(),
                                            ec2.ec2_tags,
                                        )
                                        .build();

                                    targets.push(Target::new(&ec2.private_ip, port, labels));
                                }
                                Err(e) => {
                                    warn!("Failed to resolve EC2 instance for task: {}", e);
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(targets)
    }

    async fn list_all_services(&self, cluster_arn: &str) -> Result<Vec<String>, DiscoveryError> {
        let mut services = Vec::new();
        let mut next_token: Option<String> = None;

        loop {
            let mut req = self
                .ecs_client
                .list_services()
                .cluster(cluster_arn)
                .max_results(10);

            if let Some(token) = &next_token {
                req = req.next_token(token);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;
            services.extend(resp.service_arns().to_vec());

            next_token = resp.next_token().map(|s| s.to_string());
            if next_token.is_none() {
                break;
            }
        }

        Ok(services)
    }

    async fn list_service_tasks(
        &self,
        cluster_arn: &str,
        service_name: &str,
    ) -> Result<Vec<String>, DiscoveryError> {
        let mut tasks = Vec::new();
        let mut next_token: Option<String> = None;

        loop {
            let mut req = self
                .ecs_client
                .list_tasks()
                .cluster(cluster_arn)
                .service_name(service_name)
                .max_results(100);

            if let Some(token) = &next_token {
                req = req.next_token(token);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;
            tasks.extend(resp.task_arns().to_vec());

            next_token = resp.next_token().map(|s| s.to_string());
            if next_token.is_none() {
                break;
            }
        }

        Ok(tasks)
    }

    async fn resolve_ec2_instance(
        &self,
        cluster_arn: &str,
        container_instance_arn: &str,
    ) -> Result<Ec2InstanceInfo, DiscoveryError> {
        let container_instances = self
            .ecs_client
            .describe_container_instances()
            .cluster(cluster_arn)
            .set_container_instances(Some(vec![container_instance_arn.to_string()]))
            .send()
            .await
            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

        let ec2_instance_id = container_instances
            .container_instances()
            .first()
            .and_then(|ci| ci.ec2_instance_id())
            .ok_or(DiscoveryError::NoContainerInstance)?
            .to_string();

        let instances = self
            .ec2_client
            .describe_instances()
            .set_instance_ids(Some(vec![ec2_instance_id.clone()]))
            .send()
            .await
            .map_err(|e| DiscoveryError::Ec2Error(e.to_string()))?;

        let instance = instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .ok_or(DiscoveryError::NoPrivateIp)?;

        let private_ip = instance
            .private_ip_address()
            .ok_or(DiscoveryError::NoPrivateIp)?
            .to_string();

        let public_ip = instance.public_ip_address().map(|s| s.to_string());

        let availability_zone = instance
            .placement()
            .and_then(|p| p.availability_zone())
            .map(|s| s.to_string());

        let subnet_id = instance.subnet_id().map(|s| s.to_string());

        let ec2_instance_type = instance
            .instance_type()
            .map(|t| t.as_str().to_string());

        let ec2_tags: HashMap<String, String> = instance
            .tags()
            .iter()
            .filter_map(|t| {
                t.key()
                    .zip(t.value())
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();

        Ok(Ec2InstanceInfo {
            private_ip,
            public_ip,
            availability_zone,
            subnet_id,
            container_instance_arn: container_instance_arn.to_string(),
            ec2_instance_id,
            ec2_instance_type,
            ec2_tags,
        })
    }
}
