---
plan_id: 03-discovery-logic
phase: 1
wave: 2
depends_on:
  - 01-core-infrastructure
  - 02-routes-handlers
autonomous: true
requirements_addressed:
  - DISC-01
  - DISC-02
  - DISC-03
  - DISC-04
  - DISC-05
  - DISC-06
files_modified:
  - src/aws/discovery.rs
  - src/aws/mod.rs
  - src/state/app_state.rs
  - src/handlers/sd.rs
---

# Plan 03: Discovery Logic

**Objective:** Implement AWS ECS discovery that queries clusters, services, tasks, and builds Prometheus targets.

## must_haves

truths:
  - "DescribeClusters validates configured clusters and returns cluster details"
  - "ListServices paginates through all services per cluster"
  - "DescribeServices gets service details including service name"
  - "ListTasks paginates through all tasks per service"
  - "DescribeTasks gets task details including container instance ARN"
  - "DescribeTaskDefinition extracts container definitions and docker labels"
  - "DescribeContainerInstances maps container instance to EC2 instance ID"
  - "DescribeInstances (EC2) resolves private IP address"
  - "Targets are built with format EC2_IP:prometheus.io/port for containers with prometheus.io/scrape=true"
  - "Partial results: errors for one cluster don't fail entire discovery"

## tasks

### Task 1: Implement Discovery Service

<read_first>
- src/aws/mod.rs (current content)
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 1, 1.1, 1.2)
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (AWS API Flow section)
</read_first>

<acceptance_criteria>
- src/aws/discovery.rs exists with DiscoveryService struct
- DiscoveryService has methods: discover_all_clusters(), discover_cluster_targets()
- Uses ECS and EC2 clients for cross-service calls
- Implements pagination for ListServices and ListTasks
- Returns Result<Vec<Target>, DiscoveryError>
</acceptance_criteria>

<action>
Create src/aws/discovery.rs with:

```rust
use crate::error::DiscoveryError;
use crate::models::Target;
use aws_sdk_ecs::types::LaunchType;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

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
            .await?;

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
                .await?;

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
                        .await?;

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
                            .await?;

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

            let resp = req.send().await?;
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

            let resp = req.send().await?;
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
            .await?;

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
            .map_err(DiscoveryError::Ec2Error)?;

        let private_ip = instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.private_ip_address())
            .ok_or(DiscoveryError::NoPrivateIp)?;

        Ok(format!("{}:{}", private_ip, port))
    }
}
```
</action>

---

### Task 2: Update AWS Module Exports

<read_first>
- src/aws/mod.rs (current content)
- src/aws/discovery.rs (created above)
</read_first>

<acceptance_criteria>
- src/aws/mod.rs exports discovery module and DiscoveryService
</acceptance_criteria>

<action>
Replace src/aws/mod.rs with:

```rust
pub mod client;
pub mod discovery;

pub use discovery::DiscoveryService;
```
</action>

---

### Task 3: Update State to Include DiscoveryService

<read_first>
- src/state/app_state.rs (current content)
- src/aws/discovery.rs (DiscoveryService)
</read_first>

<acceptance_criteria>
- AppState includes DiscoveryService
- AppState::new() creates DiscoveryService from ecs/ec2 clients
</acceptance_criteria>

<action>
Replace src/state/app_state.rs with:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::models::Target;
use crate::aws::DiscoveryService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<Vec<Target>>>,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
}

impl AppState {
    pub fn new(
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
    ) -> Self {
        let discovery = DiscoveryService::new(ecs_client, ec2_client);
        
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
            discovery,
        }
    }
}
```
</action>

---

### Task 4: Create Initial Discovery on Startup

<read_first>
- src/main.rs (current content)
- src/state/app_state.rs (updated)
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (Discovery Flow)
</read_first>

<acceptance_criteria>
- src/main.rs calls discovery.discover_all_clusters() on startup
- Results are written to state.cache
- Any errors during initial discovery are logged but don't prevent startup
</acceptance_criteria>

<action>
Update src/main.rs to perform initial discovery:

```rust
mod error;
mod config;
mod state;
mod aws;
mod models;
mod routes;
mod handlers;

use axum::Router;
use std::net::SocketAddr;
use tokio::signal;
use tracing::{info, error};

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    info!("Starting ecs-sd server");
    
    // Create config (hardcoded for Phase 1)
    let config = Config::new(vec![
        "service-platform-default".to_string(),
    ]);
    
    // Create AWS clients
    let (ecs_client, ec2_client) = aws::client::create_clients().await?;
    
    // Create shared state
    let state = AppState::new(config.clone(), ecs_client, ec2_client);
    
    // Perform initial discovery
    info!("Performing initial discovery...");
    let targets = state.discovery.discover_all_clusters(&config.clusters).await;
    
    // Write to cache
    {
        let mut cache = state.cache.write().await;
        *cache = targets;
    }
    
    info!("Initial discovery complete");
    
    // Build router
    let app = Router::new()
        .merge(routes::create_routes())
        .with_state(state);
    
    // Parse bind address
    let addr: SocketAddr = config.listen.parse()?;
    info!("Listening on {}", addr);
    
    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    
    info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down"),
        _ = terminate => info!("Received SIGTERM, shutting down"),
    }
}
```
</action>

---

### Task 5: Add Discovery Refresh Endpoint (Optional)

<read_first>
- src/handlers/sd.rs (current content)
- src/routes/sd.rs (current content)
</read_first>

<acceptance_criteria>
- POST /sd/refresh endpoint triggers manual discovery refresh
- Returns 200 OK with count of discovered targets
- Updates the cache
</acceptance_criteria>

<action>
Update src/handlers/sd.rs to add refresh handler:

```rust
use axum::{
    extract::{Query, State},
    Json,
};
use crate::state::AppState;
use crate::models::{Target, FilterParams};
use serde_json::json;
use tracing::info;

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Json<Vec<Target>> {
    let targets = state.cache.read().await.clone();
    let filtered = filter_targets(targets, params);
    Json(filtered)
}

pub async fn refresh_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let clusters = state.config.clusters.clone();
    
    info!("Manual discovery refresh triggered");
    
    let targets = state.discovery.discover_all_clusters(&clusters).await;
    let count = targets.len();
    
    // Update cache
    {
        let mut cache = state.cache.write().await;
        *cache = targets;
    }
    
    info!("Discovery refresh complete: {} targets", count);
    
    Json(json!({
        "status": "ok",
        "targets_discovered": count
    }))
}

fn filter_targets(targets: Vec<Target>, params: FilterParams) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|target| {
            // Check cluster filter
            if let Some(ref cluster) = params.cluster {
                let target_cluster = target.labels.get("__meta_ecs_cluster_name");
                if target_cluster.map(|s| s.as_str()) != Some(cluster.as_str()) {
                    return false;
                }
            }
            
            // Check service filter
            if let Some(ref service) = params.service {
                let target_service = target.labels.get("__meta_ecs_service_name");
                if target_service.map(|s| s.as_str()) != Some(service.as_str()) {
                    return false;
                }
            }
            
            // Check family filter
            if let Some(ref family) = params.family {
                let target_family = target.labels.get("__meta_ecs_task_family");
                if target_family.map(|s| s.as_str()) != Some(family.as_str()) {
                    return false;
                }
            }
            
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_target(cluster: &str, service: &str, family: &str) -> Target {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster_name".to_string(), cluster.to_string());
        labels.insert("__meta_ecs_service_name".to_string(), service.to_string());
        labels.insert("__meta_ecs_task_family".to_string(), family.to_string());
        
        Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }
    }

    #[test]
    fn test_filter_by_cluster() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_cluster_name"),
            Some(&"prod".to_string())
        );
    }

    #[test]
    fn test_filter_case_sensitive() {
        let targets = vec![
            create_test_target("Prod", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 0); // Case-sensitive: Prod != prod
    }

    #[test]
    fn test_filter_and_logic() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("prod", "web", "web-task"),
            create_test_target("dev", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: Some("api".to_string()),
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_no_params() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "web", "web-task"),
        ];
        
        let params = FilterParams {
            cluster: None,
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 2); // No filtering returns all
    }
}
```
</action>

---

### Task 6: Update SD Route for Refresh Endpoint

<read_first>
- src/routes/sd.rs (current content)
- src/handlers/sd.rs (updated with refresh_handler)
</read_first>

<acceptance_criteria>
- src/routes/sd.rs includes POST /sd/refresh route
</acceptance_criteria>

<action>
Replace src/routes/sd.rs with:

```rust
use axum::{
    routing::{get, post},
    Router,
};
use crate::state::AppState;
use crate::handlers::sd;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sd", get(sd::sd_handler))
        .route("/sd/refresh", post(sd::refresh_handler))
}
```
</action>

---

## verification

- [ ] `cargo check` passes with no errors
- [ ] `cargo build` compiles successfully
- [ ] `cargo test` passes (filter tests still work)
- [ ] Server starts and performs initial discovery (check logs)
- [ ] `curl http://localhost:8080/health` returns healthy status
- [ ] `curl http://localhost:8080/sd` returns targets (if AWS resources exist)
- [ ] `curl -X POST http://localhost:8080/sd/refresh` triggers refresh
- [ ] Query param filtering works: `curl "http://localhost:8080/sd?cluster=service-platform-default"`
