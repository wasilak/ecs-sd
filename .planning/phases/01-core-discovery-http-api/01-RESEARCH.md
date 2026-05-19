# Phase 1 Research: Core Discovery & HTTP API

**Phase:** 1 — Core Discovery & HTTP API  
**Researched:** 2026-05-19  
**Purpose:** Technical foundation for ECS discovery HTTP server

---

## RESEARCH COMPLETE

---

## 1. AWS SDK for Rust Patterns

### 1.1 ECS API Chain

The discovery flow requires sequential AWS API calls with proper pagination:

```rust
// 1. DescribeClusters — validate configured clusters
let clusters = client
    .describe_clusters()
    .set_clusters(Some(cluster_names))
    .send()
    .await?;

// 2. ListServices — paginated, 10 per call
let services = client
    .list_services()
    .cluster(cluster_arn)
    .max_results(10)
    .send()
    .await?;

// 3. DescribeServices — batch up to 10 ARNs
let service_details = client
    .describe_services()
    .cluster(cluster_arn)
    .set_services(Some(service_arns))
    .send()
    .await?;

// 4. ListTasks — paginated
let tasks = client
    .list_tasks()
    .cluster(cluster_arn)
    .service_name(service_name)
    .max_results(100)
    .send()
    .await?;

// 5. DescribeTasks — batch up to 100 ARNs
let task_details = client
    .describe_tasks()
    .cluster(cluster_arn)
    .set_tasks(Some(task_arns))
    .send()
    .await?;

// 6. DescribeContainerInstances — get EC2 instance IDs
let container_instances = client
    .describe_container_instances()
    .cluster(cluster_arn)
    .set_container_instances(Some(container_instance_arns))
    .send()
    .await?;
```

### 1.2 EC2 API for Private IP Resolution

Cross-service call required (ECS → EC2):

```rust
use aws_sdk_ec2::Client as Ec2Client;

// Container instance → EC2 instance ID → Private IP
let ec2_client = Ec2Client::new(&config);

let instances = ec2_client
    .describe_instances()
    .set_instance_ids(Some(ec2_instance_ids))
    .send()
    .await?;

// Extract private IP from response
let private_ip = instances
    .reservations()
    .first()
    .and_then(|r| r.instances().first())
    .and_then(|i| i.private_ip_address());
```

### 1.3 Pagination Handling

AWS SDK returns paginators for list operations:

```rust
// ListServices with pagination
let mut services = Vec::new();
let mut next_token = None;

loop {
    let resp = client
        .list_services()
        .cluster(cluster_arn)
        .set_next_token(next_token)
        .max_results(10)
        .send()
        .await?;
    
    services.extend(resp.service_arns().to_vec());
    
    match resp.next_token() {
        Some(token) => next_token = Some(token.to_string()),
        None => break,
    }
}
```

---

## 2. Axum Web Framework Patterns

### 2.1 Basic Server Setup

```rust
use axum::{
    routing::get,
    Router,
    Json,
    extract::Query,
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/sd", get(sd_handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### 2.2 Graceful Shutdown

```rust
use tokio::signal;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

// Usage
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
```

### 2.3 State Sharing

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
struct AppState {
    cache: Arc<RwLock<Vec<Target>>>,
    config: Arc<Config>,
}

let state = AppState {
    cache: Arc::new(RwLock::new(Vec::new())),
    config: Arc::new(config),
};

let app = Router::new()
    .route("/sd", get(sd_handler))
    .with_state(state);

async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Json<Vec<Target>> {
    let targets = state.cache.read().await.clone();
    // Filter and return
    Json(filter_targets(targets, params))
}
```

### 2.4 Query Parameter Extraction

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FilterParams {
    cluster: Option<String>,
    service: Option<String>,
    family: Option<String>,
}

async fn sd_handler(
    Query(params): Query<FilterParams>,
) -> Json<Vec<Target>> {
    // params.cluster, params.service, params.family are Option<String>
    // Exact match, case-sensitive filtering
}
```

---

## 3. Prometheus http_sd_configs Format

### 3.1 Response Schema

```json
[
  {
    "targets": ["10.0.1.5:8080"],
    "labels": {
      "__meta_ecs_cluster_name": "prod",
      "__meta_ecs_service_name": "api",
      "__meta_ecs_task_family": "api-task"
    }
  }
]
```

### 3.2 Rust Data Model

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Target {
    targets: Vec<String>,
    labels: std::collections::HashMap<String, String>,
}

impl Target {
    fn new(address: String) -> Self {
        Self {
            targets: vec![address],
            labels: HashMap::new(),
        }
    }
    
    fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}
```

### 3.3 Content-Type Header

Axum's `Json` response automatically sets:
- `Content-Type: application/json`

This is what Prometheus expects.

---

## 4. Error Handling Patterns

### 4.1 Custom Error Types with thiserror

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("AWS ECS API error: {0}")]
    EcsError(#[from] aws_sdk_ecs::Error),
    
    #[error("AWS EC2 API error: {0}")]
    Ec2Error(#[from] aws_sdk_ec2::Error),
    
    #[error("Cluster not found: {0}")]
    ClusterNotFound(String),
    
    #[error("Task has no container instance")]
    NoContainerInstance,
    
    #[error("EC2 instance has no private IP")]
    NoPrivateIp,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    MissingConfig(&'static str),
    
    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
}
```

### 4.2 HTTP Error Responses

```rust
use axum::{
    response::IntoResponse,
    http::StatusCode,
    Json,
};

impl IntoResponse for DiscoveryError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            DiscoveryError::ClusterNotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            _ => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
        };
        
        let body = Json(serde_json::json!({
            "error": message
        }));
        
        (status, body).into_response()
    }
}
```

### 4.3 Partial Results Strategy

```rust
async fn discover_targets(
    client: &EcsClient,
    clusters: &[String],
) -> Vec<Target> {
    let mut all_targets = Vec::new();
    
    for cluster in clusters {
        match discover_cluster_targets(client, cluster).await {
            Ok(targets) => all_targets.extend(targets),
            Err(e) => {
                // Log error but continue with other clusters
                tracing::error!("Failed to discover cluster {}: {}", cluster, e);
            }
        }
    }
    
    all_targets
}
```

---

## 5. Project Structure

### 5.1 Recommended Layout

```
src/
├── main.rs              # Entry point
├── lib.rs               # Library exports (optional)
├── config.rs            # Configuration types and parsing
├── error.rs             # Error types
├── routes/
│   ├── mod.rs           # Route aggregation
│   ├── health.rs        # /health endpoint
│   └── sd.rs            # /sd endpoint
├── handlers/
│   ├── mod.rs
│   ├── health.rs        # Health check logic
│   └── sd.rs            # Discovery handler
├── models/
│   ├── mod.rs
│   ├── target.rs        # Target and label types
│   └── discovery.rs     # Discovery result types
├── aws/
│   ├── mod.rs
│   ├── client.rs        # AWS client setup
│   └── discovery.rs     # ECS/EC2 discovery logic
└── state/
    ├── mod.rs
    └── app_state.rs     # Shared application state
```

### 5.2 Module Exports Pattern

```rust
// src/routes/mod.rs
pub mod health;
pub mod sd;

use axum::Router;
use crate::state::AppState;

pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(sd::routes())
}
```

---

## 6. Cargo Dependencies

### 6.1 Required Additions

```toml
[dependencies]
# HTTP Framework
axum = "0.7"
tower = "0.4"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# AWS SDK (additions)
aws-sdk-ec2 = "1.0"  # For private IP resolution

# Error Handling
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }

# Existing (keep)
aws-config = "1.8"
aws-sdk-ecs = "1.124"
tokio = { version = "1.52", features = ["full"] }
```

---

## 7. Key Technical Challenges & Solutions

### 7.1 Challenge: AWS API Pagination

**Solution:** Wrap list operations in pagination loops with max_results tuning.

```rust
pub async fn list_all_services(
    client: &EcsClient,
    cluster: &str,
) -> Result<Vec<String>, DiscoveryError> {
    let mut services = Vec::new();
    let mut next_token = None;
    
    loop {
        let mut req = client.list_services().cluster(cluster);
        
        if let Some(token) = &next_token {
            req = req.next_token(token);
        }
        
        let resp = req.max_results(10).send().await?;
        services.extend(resp.service_arns().to_vec());
        
        next_token = resp.next_token().map(|s| s.to_string());
        if next_token.is_none() {
            break;
        }
    }
    
    Ok(services)
}
```

### 7.2 Challenge: Cross-Service Dependencies

**Solution:** Compose ECS and EC2 clients, handle missing data gracefully.

```rust
pub struct DiscoveryService {
    ecs: aws_sdk_ecs::Client,
    ec2: aws_sdk_ec2::Client,
}

impl DiscoveryService {
    pub async fn resolve_target_address(
        &self,
        container_instance_arn: &str,
    ) -> Result<String, DiscoveryError> {
        // 1. Get EC2 instance ID from container instance
        let container_instances = self.ecs
            .describe_container_instances()
            .set_container_instances(Some(vec![container_instance_arn.to_string()]))
            .send()
            .await?;
        
        let ec2_instance_id = container_instances
            .container_instances()
            .first()
            .and_then(|ci| ci.ec2_instance_id())
            .ok_or(DiscoveryError::NoContainerInstance)?;
        
        // 2. Get private IP from EC2
        let instances = self.ec2
            .describe_instances()
            .set_instance_ids(Some(vec![ec2_instance_id.to_string()]))
            .send()
            .await?;
        
        instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.private_ip_address())
            .map(|ip| ip.to_string())
            .ok_or(DiscoveryError::NoPrivateIp)
    }
}
```

### 7.3 Challenge: Graceful Shutdown with Active Connections

**Solution:** Use Axum's built-in graceful shutdown with signal handlers.

### 7.4 Challenge: State Management

**Solution:** Use `Arc<RwLock<T>>` for cache, clone cheaply for each handler.

---

## 8. Validation Architecture

### 8.1 Test Strategy

| Test Type | Target | Approach |
|-----------|--------|----------|
| Unit | Target building | Mock inputs, assert JSON output |
| Unit | Label generation | Assert label keys/values |
| Unit | Config parsing | Test env var → struct mapping |
| Integration | Route handlers | Use axum::TestClient |
| Mock | AWS calls | Stub AWS SDK responses |

### 8.2 Mock AWS Client Pattern

```rust
#[cfg(test)]
mod tests {
    use aws_sdk_ecs::Client as EcsClient;
    
    fn create_mock_ecs_client() -> EcsClient {
        // Use aws-smithy-client test utils or dependency injection
        // Alternative: trait-based abstraction for testing
    }
}
```

---

## 9. Security Considerations

- **AWS Credentials:** Use default provider chain (IAM role → profile → env vars)
- **Network:** Bind to `0.0.0.0:8080` for containerized deployment
- **No TLS:** Run behind reverse proxy per PROJECT.md
- **No Auth:** Network-level controls only per scope

---

## 10. Performance Notes

- **AWS API Calls:** 7 sequential calls per cluster → optimize with parallelization later if needed
- **Cache:** Will be implemented in Phase 3; Phase 1 makes direct AWS calls per HTTP request
- **Memory:** Target list expected < 1000 items; HashMap for labels is acceptable
- **CPU:** JSON serialization is primary CPU cost; negligible for expected scale

---

*Research completed: Ready for planning phase.*
