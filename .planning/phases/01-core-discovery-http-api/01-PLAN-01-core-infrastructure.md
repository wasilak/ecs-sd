---
plan_id: 01-core-infrastructure
phase: 1
wave: 1
depends_on: []
autonomous: true
requirements_addressed:
  - HTTP-01
  - HTTP-04
  - QUAL-02
files_modified:
  - Cargo.toml
  - src/main.rs
  - src/error.rs
  - src/config.rs
  - src/state/mod.rs
  - src/state/app_state.rs
  - src/aws/mod.rs
  - src/aws/client.rs
---

# Plan 01: Core Infrastructure

**Objective:** Set up project structure, dependencies, error handling, and shared state.

## must_haves

truths:
  - "Modular structure established in src/ with routes/, handlers/, models/, aws/, state/"
  - "Error types use thiserror with DiscoveryError and ConfigError variants"
  - "Axum server starts and responds to requests"
  - "Graceful shutdown handles SIGTERM/SIGINT"

## tasks

### Task 1: Update Cargo.toml Dependencies

<read_first>
- Cargo.toml
</read_first>

<acceptance_criteria>
- Cargo.toml contains: axum = "0.7", tower = "0.4", serde = { version = "1.0", features = ["derive"] }, serde_json = "1.0", aws-sdk-ec2 = "1.0", thiserror = "1.0", tracing = "0.1", tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
- Existing dependencies (aws-config, aws-sdk-ecs, tokio) are preserved
</acceptance_criteria>

<action>
Add the following dependencies to Cargo.toml [dependencies] section:

```toml
axum = "0.7"
tower = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
aws-sdk-ec2 = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
```
</action>

---

### Task 2: Create Error Types Module

<read_first>
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 4: Error Handling Patterns)
</read_first>

<acceptance_criteria>
- src/error.rs exists with DiscoveryError and ConfigError enums
- DiscoveryError includes: EcsError, Ec2Error, ClusterNotFound, NoContainerInstance, NoPrivateIp
- Both enums derive Debug and use #[derive(Error)] from thiserror
- Error messages match the research document exactly
</acceptance_criteria>

<action>
Create src/error.rs with:

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
</action>

---

### Task 3: Create Configuration Module

<read_first>
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (Implementation Notes section)
</read_first>

<acceptance_criteria>
- src/config.rs exists with Config struct
- Config contains: clusters (Vec<String>), listen (String), refresh_interval (u64), metadata_level (String)
- Default values: listen = "0.0.0.0:8080", refresh_interval = 60, metadata_level = "task"
- Derive Clone for Config
</acceptance_criteria>

<action>
Create src/config.rs with:

```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: "task".to_string(),
        }
    }
}

impl Config {
    pub fn new(clusters: Vec<String>) -> Self {
        Self {
            clusters,
            ..Default::default()
        }
    }
}
```
</action>

---

### Task 4: Create State Module

<read_first>
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 2.3 State Sharing)
- src/models/target.rs (will be created in Plan 02)
</read_first>

<acceptance_criteria>
- src/state/mod.rs exists and exports AppState
- src/state/app_state.rs exists with AppState struct
- AppState contains: cache (Arc<RwLock<Vec<Target>>>), config (Arc<Config>), ecs_client, ec2_client
- AppState derives Clone
</acceptance_criteria>

<action>
Create src/state/app_state.rs with:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::models::target::Target;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<Vec<Target>>>,
    pub config: Arc<Config>,
    pub ecs_client: aws_sdk_ecs::Client,
    pub ec2_client: aws_sdk_ec2::Client,
}

impl AppState {
    pub fn new(
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
            ecs_client,
            ec2_client,
        }
    }
}
```

Create src/state/mod.rs with:

```rust
pub mod app_state;
pub use app_state::AppState;
```
</action>

---

### Task 5: Create AWS Client Module

<read_first>
- src/main.rs (existing AWS config setup)
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 1.1 ECS API Chain)
</read_first>

<acceptance_criteria>
- src/aws/mod.rs exists
- src/aws/client.rs exists with create_clients() function
- create_clients() returns Result<(aws_sdk_ecs::Client, aws_sdk_ec2::Client), aws_sdk_ecs::Error>
- Uses aws_config::BehaviorVersion::latest() and default region provider
</acceptance_criteria>

<action>
Create src/aws/client.rs with:

```rust
use aws_config::BehaviorVersion;
use aws_config::meta::region::RegionProviderChain;

pub async fn create_clients() -> Result<(aws_sdk_ecs::Client, aws_sdk_ec2::Client), aws_sdk_ecs::Error> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;

    let ecs_client = aws_sdk_ecs::Client::new(&config);
    let ec2_client = aws_sdk_ec2::Client::new(&config);

    Ok((ecs_client, ec2_client))
}
```

Create src/aws/mod.rs with:

```rust
pub mod client;
pub mod discovery;
```
</action>

---

### Task 6: Update Main Entry Point

<read_first>
- src/main.rs (current implementation)
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 2.1, 2.2)
</read_first>

<acceptance_criteria>
- src/main.rs contains: mod declarations (error, config, state, aws, models, routes, handlers)
- Server starts with Axum on config.listen address
- Graceful shutdown signal handler installed
- Basic router with /health endpoint stub
- Returns OK on server start
</acceptance_criteria>

<action>
Replace src/main.rs with:

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

### Task 7: Create Models Module

<read_first>
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 3.2 Rust Data Model)
</read_first>

<acceptance_criteria>
- src/models/mod.rs exists
- src/models/target.rs exists with Target struct stub (full implementation in Plan 02)
- Target struct has targets: Vec<String> and labels: HashMap<String, String>
</acceptance_criteria>

<action>
Create src/models/mod.rs with:

```rust
pub mod target;
pub use target::Target;
```

Create src/models/target.rs with stub:

```rust
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
    pub fn new(address: String) -> Self {
        Self {
            targets: vec![address],
            labels: HashMap::new(),
        }
    }
    
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}
```
</action>

---

### Task 8: Create Routes Module

<read_first>
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 2.1)
- src/routes/health.rs (will be created in next task)
</read_first>

<acceptance_criteria>
- src/routes/mod.rs exists with create_routes() function
- Returns Router<AppState> with /health route mounted
</acceptance_criteria>

<action>
Create src/routes/mod.rs with:

```rust
pub mod health;

use axum::Router;
use crate::state::AppState;

pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
}
```
</action>

---

### Task 9: Create Handlers Module

<read_first>
- src/handlers/health.rs (will be created in Plan 02)
</read_first>

<acceptance_criteria>
- src/handlers/mod.rs exists and exports health module
</acceptance_criteria>

<action>
Create src/handlers/mod.rs with:

```rust
pub mod health;
```
</action>

---

## verification

- [ ] `cargo check` passes with no errors
- [ ] `cargo build` compiles successfully
- [ ] `cargo run` starts server on 0.0.0.0:8080
- [ ] `curl http://localhost:8080/health` returns 200 OK
- [ ] Server responds to Ctrl+C with graceful shutdown message
