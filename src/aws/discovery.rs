use crate::config::Mode;
use crate::error::DiscoveryError;
use crate::models::{LabelBuilder, MetadataLevel, Target};
use aws_sdk_ecs::types::{ClusterField, LaunchType, ServiceField, TaskField};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

fn extract_fargate_private_ip(task: &aws_sdk_ecs::types::Task) -> Option<&str> {
    task.attachments()
        .iter()
        .find(|a| a.r#type() == Some("ElasticNetworkInterface"))
        .and_then(|a| {
            a.details()
                .iter()
                .find(|d| d.name() == Some("privateIPv4Address"))
        })
        .and_then(|d| d.value())
}

fn task_definition_from_cache(
    cache: &HashMap<String, aws_sdk_ecs::types::TaskDefinition>,
    task_definition_arn: &str,
) -> Option<aws_sdk_ecs::types::TaskDefinition> {
    cache.get(task_definition_arn).cloned()
}

fn ec2_instance_from_cache(
    container_instance_to_ec2_id: &HashMap<String, String>,
    ec2_cache: &HashMap<String, Ec2InstanceInfo>,
    container_instance_arn: &str,
) -> Option<Ec2InstanceInfo> {
    let ec2_instance_id = container_instance_to_ec2_id.get(container_instance_arn)?;
    let mut info = ec2_cache.get(ec2_instance_id)?.clone();
    info.container_instance_arn = container_instance_arn.to_string();
    info.ec2_instance_id = ec2_instance_id.clone();
    Some(info)
}

fn missing_container_instance_arns(
    container_instance_to_ec2_id: &HashMap<String, String>,
    container_instance_arns: &[String],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut missing = Vec::new();
    for arn in container_instance_arns {
        if container_instance_to_ec2_id.contains_key(arn) || !seen.insert(arn.clone()) {
            continue;
        }
        missing.push(arn.clone());
    }
    missing
}

fn missing_ec2_instance_ids(
    container_instance_to_ec2_id: &HashMap<String, String>,
    ec2_cache: &HashMap<String, Ec2InstanceInfo>,
    container_instance_arns: &[String],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut missing = Vec::new();
    for arn in container_instance_arns {
        let Some(ec2_instance_id) = container_instance_to_ec2_id.get(arn) else {
            continue;
        };
        if ec2_cache.contains_key(ec2_instance_id) || !seen.insert(ec2_instance_id.clone()) {
            continue;
        }
        missing.push(ec2_instance_id.clone());
    }
    missing
}

fn apply_page(items: &mut Vec<String>, page_items: &[String], next_token: Option<&str>) -> Option<String> {
    items.extend(page_items.iter().cloned());
    next_token.map(|s| s.to_string())
}

fn aggregate_cluster_results(
    per_cluster: Vec<Result<Vec<Target>, DiscoveryError>>,
) -> Result<Vec<Target>, DiscoveryError> {
    let attempted = per_cluster.len();
    let mut all_targets = Vec::new();
    let mut any_succeeded = false;

    for result in per_cluster {
        match result {
            Ok(targets) => {
                any_succeeded = true;
                all_targets.extend(targets);
            }
            Err(_) => {}
        }
    }

    if attempted > 0 && !any_succeeded {
        return Err(DiscoveryError::AllClustersFailed);
    }

    Ok(all_targets)
}

#[derive(Clone)]
pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    account_id: String,
    region: String,
}

#[derive(Clone)]
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

    pub async fn discover_all_clusters(
        &self,
        cluster_names: &[String],
        mode: Mode,
    ) -> Result<Vec<Target>, DiscoveryError> {
        let mut per_cluster: Vec<Result<Vec<Target>, DiscoveryError>> = Vec::new();

        for cluster_name in cluster_names {
            info!("Discovering cluster: {}", cluster_name);
            let result = self.discover_cluster_targets(cluster_name, mode.clone()).await;
            match &result {
                Ok(targets) => {
                    info!(
                        "Cluster {}: discovered {} targets",
                        cluster_name,
                        targets.len()
                    );
                }
                Err(e) => {
                    error!("Failed to discover cluster {}: {}", cluster_name, e);
                }
            }
            per_cluster.push(result);
        }

        let result = aggregate_cluster_results(per_cluster);
        if let Ok(ref targets) = result {
            info!("Total targets discovered: {}", targets.len());
        }
        result
    }

    async fn discover_cluster_targets(
        &self,
        cluster_name: &str,
        mode: Mode,
    ) -> Result<Vec<Target>, DiscoveryError> {
        let mut targets = Vec::new();
        let mut task_definition_cache: HashMap<String, aws_sdk_ecs::types::TaskDefinition> =
            HashMap::new();
        let mut container_instance_to_ec2_id_cache: HashMap<String, String> = HashMap::new();
        let mut ec2_instance_cache: HashMap<String, Ec2InstanceInfo> = HashMap::new();

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

                    let container_instance_arns: Vec<String> = tasks
                        .tasks()
                        .iter()
                        .filter(|task| task.launch_type() == Some(&LaunchType::Ec2))
                        .filter(|task| {
                            !matches!(task.last_status(), Some("STOPPED") | Some("STOPPING"))
                        })
                        .filter_map(|task| task.container_instance_arn().map(|s| s.to_string()))
                        .collect();

                    self
                        .populate_container_instance_to_ec2_id_cache(
                            cluster_arn,
                            &container_instance_arns,
                            &mut container_instance_to_ec2_id_cache,
                        )
                        .await?;

                    let missing_ids = missing_ec2_instance_ids(
                        &container_instance_to_ec2_id_cache,
                        &ec2_instance_cache,
                        &container_instance_arns,
                    );

                    self
                        .populate_ec2_instance_cache(&missing_ids, &mut ec2_instance_cache)
                        .await?;

                    for task in tasks.tasks() {
                        if task.launch_type() != Some(&LaunchType::Ec2) {
                            if mode == Mode::Proxy && task.launch_type() == Some(&LaunchType::Fargate) {
                                // Fargate: extract private IP from ENI, build target without EC2 metadata
                                if let Some(private_ip) = extract_fargate_private_ip(task) {
                                    // Fargate tasks always use awsvpc network mode
                                    let network_mode = "awsvpc".to_string();

                                    let task_def_arn = match task.task_definition_arn() {
                                        Some(a) => a,
                                        None => {
                                            warn!("Fargate task {:?} has no task_definition_arn", task.task_arn());
                                            continue;
                                        }
                                    };
                                    let task_def = match self
                                        .get_task_definition_cached(
                                            &mut task_definition_cache,
                                            task_def_arn,
                                        )
                                        .await
                                    {
                                        Ok(Some(td)) => td,
                                        Ok(None) => continue,
                                        Err(e) => {
                                            warn!("Failed to describe Fargate task def: {}", e);
                                            continue;
                                        }
                                    };

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
                                                    "Fargate container {} has scrape=true but no valid port label",
                                                    container_def.name().unwrap_or("unknown")
                                                );
                                                continue;
                                            }
                                        };

                                        let labels = LabelBuilder::new(MetadataLevel::Aws)
                                            .with_container(container_def, port)
                                            .with_network(private_ip, &network_mode, None)
                                            .with_task(task, &task_def)
                                            .with_service(service)
                                            .with_cluster(cluster)
                                            .with_aws(&self.region, &self.account_id, None)
                                            // with_ec2_instance intentionally omitted: Fargate has no container instance
                                            .build();

                                        targets.push(Target::new(private_ip, port, labels));
                                    }
                                } else {
                                    warn!("Fargate task {:?} has no ENI private IP, skipping", task.task_arn());
                                }
                            } else {
                                debug!("Skipping non-EC2/non-Fargate task: {:?}", task.task_arn());
                            }
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

                        let Some(task_def) = self
                            .get_task_definition_cached(&mut task_definition_cache, task_def_arn)
                            .await?
                        else {
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
                                .resolve_ec2_instance_cached(
                                    cluster_arn,
                                    container_instance_arn,
                                    &mut container_instance_to_ec2_id_cache,
                                    &mut ec2_instance_cache,
                                )
                                .await
                            {
                                Ok(ec2) => {
                                    let target_ip = if network_mode == "awsvpc" {
                                        match extract_fargate_private_ip(task) {
                                            Some(ip) => ip,
                                            None => {
                                                warn!(
                                                    "EC2 awsvpc task {:?} has no ENI private IP, skipping",
                                                    task.task_arn()
                                                );
                                                continue;
                                            }
                                        }
                                    } else {
                                        ec2.private_ip.as_str()
                                    };

                                    let labels = LabelBuilder::new(MetadataLevel::Aws)
                                        .with_container(container_def, port)
                                        .with_network(target_ip, &network_mode, ec2.subnet_id.as_deref())
                                        .with_task(task, &task_def)
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

                                    targets.push(Target::new(target_ip, port, labels));
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

    async fn get_task_definition_cached(
        &self,
        cache: &mut HashMap<String, aws_sdk_ecs::types::TaskDefinition>,
        task_definition_arn: &str,
    ) -> Result<Option<aws_sdk_ecs::types::TaskDefinition>, DiscoveryError> {
        if let Some(task_definition) = task_definition_from_cache(cache, task_definition_arn) {
            return Ok(Some(task_definition));
        }

        let task_def_resp = self
            .ecs_client
            .describe_task_definition()
            .task_definition(task_definition_arn)
            .send()
            .await
            .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

        let Some(task_definition) = task_def_resp.task_definition() else {
            return Ok(None);
        };

        let task_definition = task_definition.clone();
        cache.insert(task_definition_arn.to_string(), task_definition.clone());
        Ok(Some(task_definition))
    }

    async fn resolve_ec2_instance_cached(
        &self,
        cluster_arn: &str,
        container_instance_arn: &str,
        container_instance_to_ec2_id_cache: &mut HashMap<String, String>,
        ec2_instance_cache: &mut HashMap<String, Ec2InstanceInfo>,
    ) -> Result<Ec2InstanceInfo, DiscoveryError> {
        if let Some(ec2) = ec2_instance_from_cache(
            container_instance_to_ec2_id_cache,
            ec2_instance_cache,
            container_instance_arn,
        ) {
            return Ok(ec2);
        }

        let ec2 = self.resolve_ec2_instance(cluster_arn, container_instance_arn).await?;
        container_instance_to_ec2_id_cache
            .insert(container_instance_arn.to_string(), ec2.ec2_instance_id.clone());
        ec2_instance_cache.insert(ec2.ec2_instance_id.clone(), ec2.clone());
        Ok(ec2)
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
            next_token = apply_page(&mut services, resp.service_arns(), resp.next_token());
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
            next_token = apply_page(&mut tasks, resp.task_arns(), resp.next_token());
            if next_token.is_none() {
                break;
            }
        }

        Ok(tasks)
    }

    async fn populate_container_instance_to_ec2_id_cache(
        &self,
        cluster_arn: &str,
        container_instance_arns: &[String],
        cache: &mut HashMap<String, String>,
    ) -> Result<(), DiscoveryError> {
        let missing = missing_container_instance_arns(cache, container_instance_arns);
        for chunk in missing.chunks(100) {
            let container_instances = self
                .ecs_client
                .describe_container_instances()
                .cluster(cluster_arn)
                .set_container_instances(Some(chunk.to_vec()))
                .send()
                .await
                .map_err(|e| DiscoveryError::EcsError(e.to_string()))?;

            for ci in container_instances.container_instances() {
                if let (Some(container_instance_arn), Some(ec2_instance_id)) =
                    (ci.container_instance_arn(), ci.ec2_instance_id())
                {
                    cache.insert(container_instance_arn.to_string(), ec2_instance_id.to_string());
                }
            }
        }
        Ok(())
    }

    async fn populate_ec2_instance_cache(
        &self,
        ec2_instance_ids: &[String],
        cache: &mut HashMap<String, Ec2InstanceInfo>,
    ) -> Result<(), DiscoveryError> {
        for chunk in ec2_instance_ids.chunks(100) {
            let instances = self
                .ec2_client
                .describe_instances()
                .set_instance_ids(Some(chunk.to_vec()))
                .send()
                .await
                .map_err(|e| DiscoveryError::Ec2Error(e.to_string()))?;

            for reservation in instances.reservations() {
                for instance in reservation.instances() {
                    let Some(ec2_instance_id) = instance.instance_id() else {
                        continue;
                    };
                    let Some(private_ip) = instance.private_ip_address() else {
                        continue;
                    };

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

                    cache.insert(
                        ec2_instance_id.to_string(),
                        Ec2InstanceInfo {
                            private_ip: private_ip.to_string(),
                            public_ip,
                            availability_zone,
                            subnet_id,
                            container_instance_arn: String::new(),
                            ec2_instance_id: ec2_instance_id.to_string(),
                            ec2_instance_type,
                            ec2_tags,
                        },
                    );
                }
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ecs::types::{Attachment, KeyValuePair};

    #[test]
    fn task_definition_from_cache_returns_none_on_miss() {
        let cache: HashMap<String, aws_sdk_ecs::types::TaskDefinition> = HashMap::new();
        let result = task_definition_from_cache(&cache, "arn:missing");
        assert!(result.is_none());
    }

    #[test]
    fn task_definition_from_cache_returns_cloned_value_on_hit() {
        let mut cache: HashMap<String, aws_sdk_ecs::types::TaskDefinition> = HashMap::new();
        let task_def = aws_sdk_ecs::types::TaskDefinition::builder()
            .task_definition_arn("arn:task-def:1")
            .family("svc")
            .build();
        cache.insert("arn:task-def:1".to_string(), task_def);

        let result = task_definition_from_cache(&cache, "arn:task-def:1");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().task_definition_arn(),
            Some("arn:task-def:1")
        );
    }

    #[test]
    fn missing_container_instance_arns_returns_unique_uncached_values() {
        let cache = HashMap::from([("ci:1".to_string(), "i-1".to_string())]);
        let input = vec![
            "ci:1".to_string(),
            "ci:2".to_string(),
            "ci:2".to_string(),
            "ci:3".to_string(),
        ];
        let missing = missing_container_instance_arns(&cache, &input);
        assert_eq!(missing, vec!["ci:2".to_string(), "ci:3".to_string()]);
    }

    #[test]
    fn missing_ec2_instance_ids_returns_unique_uncached_values() {
        let container_to_ec2 = HashMap::from([
            ("ci:1".to_string(), "i-1".to_string()),
            ("ci:2".to_string(), "i-2".to_string()),
            ("ci:3".to_string(), "i-2".to_string()),
            ("ci:4".to_string(), "i-4".to_string()),
        ]);
        let mut ec2_cache = HashMap::new();
        ec2_cache.insert(
            "i-1".to_string(),
            Ec2InstanceInfo {
                private_ip: "10.0.0.1".to_string(),
                public_ip: None,
                availability_zone: None,
                subnet_id: None,
                container_instance_arn: "ci:1".to_string(),
                ec2_instance_id: "i-1".to_string(),
                ec2_instance_type: None,
                ec2_tags: HashMap::new(),
            },
        );

        let input = vec![
            "ci:1".to_string(),
            "ci:2".to_string(),
            "ci:3".to_string(),
            "ci:4".to_string(),
            "ci:unknown".to_string(),
        ];
        let missing = missing_ec2_instance_ids(&container_to_ec2, &ec2_cache, &input);
        assert_eq!(missing, vec!["i-2".to_string(), "i-4".to_string()]);
    }

    #[test]
    fn ec2_instance_from_cache_rehydrates_container_instance_context() {
        let container_to_ec2 = HashMap::from([("ci:1".to_string(), "i-1".to_string())]);
        let mut ec2_cache = HashMap::new();
        ec2_cache.insert(
            "i-1".to_string(),
            Ec2InstanceInfo {
                private_ip: "10.0.0.1".to_string(),
                public_ip: Some("1.2.3.4".to_string()),
                availability_zone: Some("eu-west-1a".to_string()),
                subnet_id: Some("subnet-1".to_string()),
                container_instance_arn: String::new(),
                ec2_instance_id: "i-1".to_string(),
                ec2_instance_type: Some("t3.medium".to_string()),
                ec2_tags: HashMap::from([("Name".to_string(), "node-1".to_string())]),
            },
        );

        let result = ec2_instance_from_cache(&container_to_ec2, &ec2_cache, "ci:1")
            .expect("cache hit expected");
        assert_eq!(result.container_instance_arn, "ci:1");
        assert_eq!(result.ec2_instance_id, "i-1");
        assert_eq!(result.private_ip, "10.0.0.1");
    }

    #[test]
    fn apply_page_accumulates_items_and_token_across_pages() {
        let mut items = Vec::new();

        let token = apply_page(
            &mut items,
            &["svc:1".to_string(), "svc:2".to_string()],
            Some("t1"),
        );
        assert_eq!(items, vec!["svc:1".to_string(), "svc:2".to_string()]);
        assert_eq!(token.as_deref(), Some("t1"));

        let token = apply_page(&mut items, &["svc:3".to_string()], None);
        assert_eq!(items, vec!["svc:1", "svc:2", "svc:3"]);
        assert_eq!(token, None);
    }

    #[test]
    fn missing_container_instance_arns_scales_with_large_paginated_input() {
        let cache: HashMap<String, String> = (0..50)
            .map(|i| (format!("ci:{}", i), format!("i-{}", i)))
            .collect();

        let mut input = Vec::new();
        for page in 0..4 {
            for i in 0..80 {
                let value = format!("ci:{}", page * 80 + i);
                input.push(value.clone());
                if i % 3 == 0 {
                    input.push(value);
                }
            }
        }

        let missing = missing_container_instance_arns(&cache, &input);
        assert_eq!(missing.len(), 270);
        assert_eq!(missing.first().map(|s| s.as_str()), Some("ci:50"));
        assert_eq!(missing.last().map(|s| s.as_str()), Some("ci:319"));
    }

    #[test]
    fn missing_ec2_instance_ids_scales_and_preserves_first_seen_order() {
        let container_to_ec2 = HashMap::from([
            ("ci:1".to_string(), "i-1".to_string()),
            ("ci:2".to_string(), "i-2".to_string()),
            ("ci:3".to_string(), "i-3".to_string()),
            ("ci:4".to_string(), "i-2".to_string()),
            ("ci:5".to_string(), "i-4".to_string()),
        ]);

        let mut ec2_cache = HashMap::new();
        ec2_cache.insert(
            "i-1".to_string(),
            Ec2InstanceInfo {
                private_ip: "10.0.0.1".to_string(),
                public_ip: None,
                availability_zone: None,
                subnet_id: None,
                container_instance_arn: "ci:1".to_string(),
                ec2_instance_id: "i-1".to_string(),
                ec2_instance_type: None,
                ec2_tags: HashMap::new(),
            },
        );

        let input = vec![
            "ci:1".to_string(),
            "ci:2".to_string(),
            "ci:3".to_string(),
            "ci:4".to_string(),
            "ci:5".to_string(),
            "ci:unknown".to_string(),
            "ci:2".to_string(),
            "ci:5".to_string(),
        ];

        let missing = missing_ec2_instance_ids(&container_to_ec2, &ec2_cache, &input);
        assert_eq!(missing, vec!["i-2", "i-3", "i-4"]);
    }

    #[test]
    fn ec2_instance_from_cache_returns_none_when_container_mapping_missing() {
        let container_to_ec2: HashMap<String, String> = HashMap::new();
        let ec2_cache: HashMap<String, Ec2InstanceInfo> = HashMap::new();

        let result = ec2_instance_from_cache(&container_to_ec2, &ec2_cache, "ci:missing");
        assert!(result.is_none());
    }

    #[test]
    fn extract_fargate_ip_from_valid_eni() {
        let kv = KeyValuePair::builder()
            .name("privateIPv4Address")
            .value("10.0.1.42")
            .build();
        let attachment = Attachment::builder()
            .r#type("ElasticNetworkInterface")
            .details(kv)
            .build();
        let task = aws_sdk_ecs::types::Task::builder()
            .attachments(attachment)
            .build();
        assert_eq!(extract_fargate_private_ip(&task), Some("10.0.1.42"));
    }

    #[test]
    fn extract_fargate_ip_returns_none_when_no_eni() {
        let task = aws_sdk_ecs::types::Task::builder().build();
        assert_eq!(extract_fargate_private_ip(&task), None);
    }

    #[test]
    fn extract_fargate_ip_returns_none_when_wrong_attachment_type() {
        let kv = KeyValuePair::builder()
            .name("privateIPv4Address")
            .value("10.0.1.42")
            .build();
        let attachment = Attachment::builder()
            .r#type("other")
            .details(kv)
            .build();
        let task = aws_sdk_ecs::types::Task::builder()
            .attachments(attachment)
            .build();
        assert_eq!(extract_fargate_private_ip(&task), None);
    }

    #[test]
    fn extract_fargate_ip_returns_none_when_ip_key_missing() {
        let kv = KeyValuePair::builder()
            .name("someOtherKey")
            .value("10.0.1.42")
            .build();
        let attachment = Attachment::builder()
            .r#type("ElasticNetworkInterface")
            .details(kv)
            .build();
        let task = aws_sdk_ecs::types::Task::builder()
            .attachments(attachment)
            .build();
        assert_eq!(extract_fargate_private_ip(&task), None);
    }

    #[test]
    fn extract_fargate_ip_from_eni_attachment_for_ec2_awsvpc_shape() {
        let eni_ip = KeyValuePair::builder()
            .name("privateIPv4Address")
            .value("10.157.8.169")
            .build();
        let attachment = Attachment::builder()
            .r#type("ElasticNetworkInterface")
            .details(eni_ip)
            .build();
        let task = aws_sdk_ecs::types::Task::builder()
            .attachments(attachment)
            .build();

        assert_eq!(extract_fargate_private_ip(&task), Some("10.157.8.169"));
    }

    #[test]
    fn discover_all_clusters_returns_err_when_all_clusters_fail() {
        let per_cluster: Vec<Result<Vec<Target>, DiscoveryError>> = vec![
            Err(DiscoveryError::EcsError("simulated".to_string())),
            Err(DiscoveryError::EcsError("simulated".to_string())),
        ];
        let result = aggregate_cluster_results(per_cluster);
        assert!(matches!(result, Err(DiscoveryError::AllClustersFailed)));
    }

    #[test]
    fn discover_all_clusters_returns_partial_ok_when_some_clusters_fail() {
        let target = Target {
            targets: vec!["10.0.0.1:9090".to_string()],
            labels: HashMap::new(),
        };
        let per_cluster: Vec<Result<Vec<Target>, DiscoveryError>> = vec![
            Ok(vec![target]),
            Err(DiscoveryError::EcsError("simulated".to_string())),
        ];
        let result = aggregate_cluster_results(per_cluster);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
