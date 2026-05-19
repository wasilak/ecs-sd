use crate::error::DiscoveryError;
use crate::models::Target;
use aws_sdk_ecs::types::LaunchType;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
}

impl DiscoveryService {
    pub fn new(ecs_client: aws_sdk_ecs::Client, ec2_client: aws_sdk_ec2::Client) -> Self {
        Self {
            ecs_client,
            ec2_client,
        }
    }

    pub async fn discover_all_clusters(
        &self,
        cluster_names: &[String],
    ) -> Vec<Target> {
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
                    // Continue with other clusters (partial results strategy)
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

        // 1. Validate cluster exists
        let clusters = self
            .ecs_client
            .describe_clusters()
            .set_clusters(Some(vec![cluster_name.to_string()]))
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
        let cluster_name = cluster
            .cluster_name()
            .unwrap_or(cluster_name);

        // 2. List all services in cluster (paginated)
        let service_arns = self.list_all_services(cluster_arn).await?;
        debug!("Found {} services in cluster {}", service_arns.len(), cluster_name);

        // 3. Get service details (batch in groups of 10)
        for service_arn_chunk in service_arns.chunks(10) {
            let services = self
                .ecs_client
                .describe_services()
                .cluster(cluster_arn)
                .set_services(Some(service_arn_chunk.to_vec()))
                .send()
                .await
                .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

            for service in services.services() {
                let service_name = service.service_name().unwrap_or("unknown");

                // 4. List tasks for this service (paginated)
                let task_arns = self.list_service_tasks(cluster_arn, service_name).await?;

                if task_arns.is_empty() {
                    continue;
                }

                // 5. Describe tasks (batch in groups of 100)
                for task_chunk in task_arns.chunks(100) {
                    let tasks = self
                        .ecs_client
                        .describe_tasks()
                        .cluster(cluster_arn)
                        .set_tasks(Some(task_chunk.to_vec()))
                        .send()
                        .await
                        .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

                    for task in tasks.tasks() {
                        // Skip non-EC2 launch type
                        if task.launch_type() != Some(&LaunchType::Ec2) {
                            debug!("Skipping non-EC2 task: {:?}", task.task_arn());
                            continue;
                        }

                        // Skip STOPPED/STOPPING tasks
                        if let Some(status) = task.last_status() {
                            if status == "STOPPED" || status == "STOPPING" {
                                continue;
                            }
                        }

                        // 6. Get task definition to check docker labels
                        let task_def_arn = task
                            .task_definition_arn()
                            .ok_or(DiscoveryError::NoContainerInstance)?;

                        let task_def = self
                            .ecs_client
                            .describe_task_definition()
                            .task_definition(task_def_arn)
                            .send()
                            .await
                            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

                        if let Some(task_def) = task_def.task_definition() {
                            for container_def in task_def.container_definitions() {
                                // Check for prometheus.io/scrape label
                                let should_scrape = container_def
                                    .docker_labels()
                                    .and_then(|labels| labels.get("prometheus.io/scrape"))
                                    .map(|v| v == "true")
                                    .unwrap_or(false);

                                if !should_scrape {
                                    continue;
                                }

                                // Get the port from prometheus.io/port label
                                let port = container_def
                                    .docker_labels()
                                    .and_then(|labels| labels.get("prometheus.io/port"))
                                    .and_then(|p| p.parse::<u16>().ok())
                                    .ok_or_else(|| {
                                        warn!(
                                            "Container {} has prometheus.io/scrape=true but no valid prometheus.io/port",
                                            container_def.name().unwrap_or("unknown")
                                        );
                                        DiscoveryError::NoContainerInstance
                                    })?;

                                // 7. Get container instance for EC2 resolution
                                let container_instance_arn = match task.container_instance_arn() {
                                    Some(arn) => arn,
                                    None => {
                                        warn!("Task has no container instance");
                                        continue;
                                    }
                                };

                                // 8. Resolve target address
                                match self.resolve_target_address(container_instance_arn, port).await {
                                    Ok(address) => {
                                        let target = Target::new(address)
                                            .with_label("__meta_ecs_cluster_name", cluster_name)
                                            .with_label("__meta_ecs_service_name", service_name)
                                            .with_label(
                                                "__meta_ecs_task_family",
                                                task_def.family().unwrap_or("unknown"),
                                            );

                                        targets.push(target);
                                    }
                                    Err(e) => {
                                        warn!("Failed to resolve target address: {}", e);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(targets)
    }

    async fn list_all_services(
        &self,
        cluster_arn: &str,
    ) -> Result<Vec<String>, DiscoveryError> {
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

            let resp = req.send()
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

            let resp = req.send()
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

    async fn resolve_target_address(
        &self,
        container_instance_arn: &str,
        port: u16,
    ) -> Result<String, DiscoveryError> {
        // Extract cluster from container instance ARN
        // ARN format: arn:aws:ecs:region:account:container-instance/cluster-name/container-instance-id
        let cluster_name = container_instance_arn
            .split("/")
            .nth(1)
            .ok_or(DiscoveryError::NoContainerInstance)?;

        // Get EC2 instance ID from container instance
        let container_instances = self
            .ecs_client
            .describe_container_instances()
            .cluster(cluster_name)
            .set_container_instances(Some(vec![container_instance_arn.to_string()]))
            .send()
            .await
            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

        let ec2_instance_id = container_instances
            .container_instances()
            .first()
            .and_then(|ci| ci.ec2_instance_id())
            .ok_or(DiscoveryError::NoContainerInstance)?;

        // Get private IP from EC2
        let instances = self
            .ec2_client
            .describe_instances()
            .set_instance_ids(Some(vec![ec2_instance_id.to_string()]))
            .send()
            .await
            .map_err(|e| DiscoveryError::Ec2Error(e.to_string()))?;

        let private_ip = instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.private_ip_address())
            .ok_or(DiscoveryError::NoPrivateIp)?;

        Ok(format!("{}:{}", private_ip, port))
    }
}
