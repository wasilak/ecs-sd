# Phase 2: Metadata Labels - Pattern Map

**Mapped:** 2026-05-19
**Files analyzed:** 2 new, 8 existing source files
**Analogs found:** 7 / 7

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/models/metadata_level.rs` | model | enum configuration | `src/models/target.rs` (enum pattern from serde derives) | exact |
| `src/models/label_builder.rs` | model | builder construction | `src/models/target.rs` (builder pattern with_label) | exact |
| `src/models/mod.rs` | module | export coordination | existing mod.rs (self-referential) | exact |
| `src/aws/discovery.rs` | service | AWS API orchestration | self (modify existing) | exact |
| `src/aws/client.rs` | service | client factory | `src/aws/discovery.rs` (client usage pattern) | exact |
| `src/handlers/sd.rs` | handler | HTTP request-response | self (modify existing) | exact |
| `src/state/app_state.rs` | state | shared state management | self (modify existing) | exact |
| `src/config.rs` | config | configuration | self (modify existing) | exact |
| `src/error.rs` | error | error type definition | self (extend existing) | exact |
| `Cargo.toml` | config | dependency management | self (extend existing) | exact |

---

## New File Patterns

### `src/models/metadata_level.rs` (model, enum configuration)

**Analog:** `src/models/target.rs` (lines 1-8 for derive pattern)

**Imports pattern:**
```rust
// From target.rs pattern (lines 1-2)
use serde::{Serialize, Deserialize};
// Additional for strum derives
```

**Core enum pattern:**
```rust
// Based on derive pattern from target.rs lines 4-5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
```

**Trait implementations:**
```rust
// FromStr for query param parsing (pattern similar to FilterParams deserialization)
use std::str::FromStr;

impl FromStr for MetadataLevel {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "container" => Ok(MetadataLevel::Container),
            "task" => Ok(MetadataLevel::Task),
            "service" => Ok(MetadataLevel::Service),
            "cluster" => Ok(MetadataLevel::Cluster),
            "aws" => Ok(MetadataLevel::Aws),
            _ => Err(format!("Invalid level: {}. Valid: container, task, service, cluster, aws", s)),
        }
    }
}

// Display for logging (standard Rust pattern)
use std::fmt;

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

// Includes method for level hierarchy
impl MetadataLevel {
    /// Returns true if self includes the given level
    pub fn includes(&self, other: MetadataLevel) -> bool {
        use MetadataLevel::*;
        match (*self, other) {
            (Aws, _) => true,
            (Cluster, Container) | (Cluster, Task) | (Cluster, Service) | (Cluster, Cluster) => true,
            (Service, Container) | (Service, Task) | (Service, Service) => true,
            (Task, Container) | (Task, Task) => true,
            (Container, Container) => true,
            _ => false,
        }
    }
}
```

---

### `src/models/label_builder.rs` (model, builder construction)

**Analog:** `src/models/target.rs` (builder pattern from `with_label` method, lines 18-21)

**Imports pattern:**
```rust
// From target.rs pattern
use std::collections::HashMap;
// AWS SDK types (from discovery.rs usage)
use aws_sdk_ecs::types::{ContainerDefinition, Task, TaskDefinition, Service, Cluster};
// Local module
use crate::models::metadata_level::MetadataLevel;
```

**Builder struct pattern:**
```rust
// Pattern: Builder with level-aware accumulation (inspired by Target::with_label)
pub struct LabelBuilder {
    level: MetadataLevel,
    container_data: Option<ContainerData>,
    task_data: Option<TaskData>,
    service_data: Option<ServiceData>,
    cluster_data: Option<ClusterData>,
    aws_data: Option<AwsData>,
}

// Inner data structs (pattern: private data holders)
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
    availability_zone: Option<String>,  // Optional as per D-03
}
```

**Builder API pattern (consuming self style like Target::with_label):**
```rust
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

    // Consuming builder pattern (matches Target::with_label style)
    pub fn with_container(mut self, def: &ContainerDefinition, port: u16) -> Self {
        self.container_data = Some(ContainerData {
            name: def.name().unwrap_or("unknown").to_string(),
            image: def.image().unwrap_or("unknown").to_string(),
            port,
        });
        self
    }

    pub fn with_task(mut self, task: &Task, task_def: &TaskDefinition) -> Self {
        let version = task_def.task_definition_arn()
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
            desired_count: service.desired_count().unwrap_or(0),
            running_count: service.running_count().unwrap_or(0),
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

    pub fn with_aws(mut self, region: &str, account_id: &str, az: Option<&str>) -> Self {
        self.aws_data = Some(AwsData {
            region: region.to_string(),
            account_id: account_id.to_string(),
            availability_zone: az.map(|s| s.to_string()),
        });
        self
    }

    // Build method - returns HashMap like Target.labels
    pub fn build(self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        
        // Container level labels
        if self.level.includes(MetadataLevel::Container) {
            if let Some(data) = self.container_data {
                labels.insert("__meta_ecs_container_name".to_string(), data.name);
                labels.insert("__meta_ecs_container_image".to_string(), data.image);
                labels.insert("__meta_ecs_metrics_port".to_string(), data.port.to_string());
            }
        }
        
        // Task level labels
        if self.level.includes(MetadataLevel::Task) {
            if let Some(data) = self.task_data {
                labels.insert("__meta_ecs_task_arn".to_string(), data.arn);
                labels.insert("__meta_ecs_task_family".to_string(), data.family);
                labels.insert("__meta_ecs_task_version".to_string(), data.version);
            }
        }
        
        // Service level labels
        if self.level.includes(MetadataLevel::Service) {
            if let Some(data) = self.service_data {
                labels.insert("__meta_ecs_service_name".to_string(), data.name);
                labels.insert("__meta_ecs_desired_count".to_string(), data.desired_count.to_string());
                labels.insert("__meta_ecs_running_count".to_string(), data.running_count.to_string());
            }
        }
        
        // Cluster level labels
        if self.level.includes(MetadataLevel::Cluster) {
            if let Some(data) = self.cluster_data {
                labels.insert("__meta_ecs_cluster_name".to_string(), data.name);
                labels.insert("__meta_ecs_cluster_arn".to_string(), data.arn);
            }
        }
        
        // AWS level labels
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
```

---

## Modified File Patterns

### `src/models/mod.rs` (module export)

**Current State (lines 1-11):**
```rust
pub mod target;
pub use target::Target;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
```

**Expected Changes:**
```rust
// Add after line 2
pub mod metadata_level;
pub use metadata_level::MetadataLevel;

pub mod label_builder;
pub use label_builder::LabelBuilder;
```

---

### `src/aws/discovery.rs` (service, AWS API orchestration)

**Current State - Constructor (lines 7-19):**
```rust
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
```

**Expected Changes - Add STS client and cached metadata:**
```rust
// After line 5, add import
use crate::models::{MetadataLevel, LabelBuilder};

// Modify struct (lines 7-11)
#[derive(Clone)]
pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    sts_client: aws_sdk_sts::Client,
    account_id: String,
    region: String,
}

// Modify constructor (lines 13-28)
impl DiscoveryService {
    pub async fn new(
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,
        region: String,
    ) -> Result<Self, DiscoveryError> {
        // Get account ID from STS
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
            sts_client,
            account_id,
            region,
        })
    }
```

**Current State - Label building (lines 177-184):**
```rust
let target = Target::new(address)
    .with_label("__meta_ecs_cluster_name", cluster_name)
    .with_label("__meta_ecs_service_name", service_name)
    .with_label(
        "__meta_ecs_task_family",
        task_def.family().unwrap_or("unknown"),
    );
```

**Expected Changes - Use LabelBuilder:**
```rust
// Get availability zone from EC2 response (already available in resolve_target_address)
let availability_zone = instances
    .reservations()
    .first()
    .and_then(|r| r.instances().first())
    .and_then(|i| i.placement())
    .and_then(|p| p.availability_zone())
    .map(|s| s.to_string());

// Build labels using LabelBuilder
let labels = LabelBuilder::new(MetadataLevel::Aws)  // Always build at max level
    .with_container(container_def, port)
    .with_task(task, &task_def)
    .with_service(service)
    .with_cluster(cluster)
    .with_aws(&self.region, &self.account_id, availability_zone.as_deref())
    .build();

let target = Target {
    targets: vec![address],
    labels,
};
```

---

### `src/aws/client.rs` (service, client factory) - MAY NOT EXIST YET

**Analog:** `src/aws/discovery.rs` client usage pattern

**Pattern for STS client creation:**
```rust
use aws_config::BehaviorVersion;

pub async fn create_sts_client() -> aws_sdk_sts::Client {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    aws_sdk_sts::Client::new(&config)
}
```

---

### `src/handlers/sd.rs` (handler, HTTP request-response)

**Current State - FilterParams (from models/mod.rs):**
```rust
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
```

**Expected Changes - Add level param:**
```rust
// Extend FilterParams or create SdQueryParams
use crate::models::MetadataLevel;

#[derive(Debug, Deserialize)]
pub struct SdQueryParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub level: MetadataLevel,  // Uses MetadataLevel::default() = Task
}

// Change handler signature (line 10-13)
pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<SdQueryParams>,  // Changed from FilterParams
) -> Result<Json<Vec<Target>>, (StatusCode, String)> {  // Changed to support errors
    let cache = state.cache.read().await;
    let targets = cache
        .get(&params.level)
        .cloned()
        .unwrap_or_default();
    
    let filtered = filter_targets(targets, &params);
    Ok(Json(filtered))
}
```

**Current State - Cache access (line 14):**
```rust
let targets = state.cache.read().await.clone();
```

**Expected Changes:**
```rust
// Access multi-tier cache by level
let cache = state.cache.read().await;
let targets = cache
    .get(&params.level)
    .cloned()
    .unwrap_or_default();
```

---

### `src/state/app_state.rs` (state, shared state management)

**Current State (lines 1-28):**
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

**Expected Changes - Multi-tier cache:**
```rust
use std::sync::Arc;
use std::collections::HashMap;  // ADD
use tokio::sync::RwLock;
use crate::config::Config;
use crate::models::{Target, MetadataLevel};  // ADD MetadataLevel
use crate::aws::DiscoveryService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,  // CHANGED
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
}

impl AppState {
    pub async fn new(  // CHANGED to async
        config: Config,
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,  // ADD
        region: String,  // ADD
    ) -> Result<Self, DiscoveryError> {  // CHANGED to Result
        let discovery = DiscoveryService::new(ecs_client, ec2_client, sts_client, region).await?;  // CHANGED

        Ok(Self {  // CHANGED
            cache: Arc::new(RwLock::new(HashMap::new())),  // CHANGED
            config: Arc::new(config),
            discovery,
        })
    }
}
```

---

### `src/config.rs` (config, configuration)

**Current State (lines 1-27):**
```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: String,  // Currently unused
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: "task".to_string(),  // String, not enum
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

**Expected Changes:**
```rust
use crate::models::MetadataLevel;  // ADD

#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: MetadataLevel,  // CHANGED from String
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: MetadataLevel::default(),  // CHANGED
        }
    }
}
```

---

### `src/error.rs` (error, error type definition)

**Current State (lines 1-28):**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("AWS ECS API error: {0}")]
    EcsError(String),

    #[error("AWS EC2 API error: {0}")]
    Ec2Error(String),

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

**Expected Changes:**
```rust
#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("AWS ECS API error: {0}")]
    EcsError(String),

    #[error("AWS EC2 API error: {0}")]
    Ec2Error(String),

    #[error("AWS STS API error: {0}")]  // ADD
    StsError(String),  // ADD

    #[error("Cluster not found: {0}")]
    ClusterNotFound(String),

    #[error("Task has no container instance")]
    NoContainerInstance,

    #[error("EC2 instance has no private IP")]
    NoPrivateIp,
}
```

---

### `Cargo.toml` (config, dependency management)

**Current State (lines 1-17):**
```toml
[package]
name = "ecs-sd"
version = "0.1.0"
edition = "2024"

[dependencies]
aws-config = "1.8.16"
aws-sdk-ecs = "1.124.0"
aws-sdk-ec2 = "1.0"
tokio = { version = "1.52.2", features = ["full"] }
axum = "0.7"
tower = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
```

**Expected Changes:**
```toml
[dependencies]
aws-config = "1.8.16"
aws-sdk-ecs = "1.124.0"
aws-sdk-ec2 = "1.0"
aws-sdk-sts = "1.103.0"  # ADD for account ID retrieval
strum = { version = "0.28.0", features = ["derive"] }  # ADD for enum derives (optional)
tokio = { version = "1.52.2", features = ["full"] }
axum = "0.7"
tower = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
```

---

## Shared Patterns

### Error Handling Pattern

**Source:** `src/error.rs`
**Apply to:** All service and AWS API files

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("AWS {service} API error: {message}")]
    AwsError { service: String, message: String },
    // ... variants
}
```

### AWS SDK Client Pattern

**Source:** `src/aws/discovery.rs` (lines 8-11, 57-63)
**Apply to:** DiscoveryService, STS client usage

```rust
// Client storage
pub struct Service {
    client: aws_sdk_X::Client,
}

// API call with error mapping
let response = self
    .client
    .operation_name()
    .param(value)
    .send()
    .await
    .map_err(|e| DiscoveryError::ServiceError(e.to_string()))?;
```

### Tracing Pattern

**Source:** `src/aws/discovery.rs` (lines 28, 32-36, 40, etc.)
**Apply to:** All async operations

```rust
use tracing::{debug, error, info, warn};

info!("Operation starting: {}", value);
debug!("Detailed state: {:?}", obj);
warn!("Non-fatal issue: {}", err);
error!("Critical failure: {}", err);
```

### Cache Access Pattern

**Source:** `src/handlers/sd.rs` (lines 14, 30-33)
**Apply to:** All cache reads/writes

```rust
// Read
let data = state.cache.read().await;

// Write
{
    let mut cache = state.cache.write().await;
    *cache = new_data;
}
```

---

## No Analog Found

None - all files have clear analogs in the existing codebase.

---

## Metadata

**Analog search scope:** src/{models,aws,handlers,state,config,error}.rs, Cargo.toml
**Files scanned:** 10
**Pattern extraction date:** 2026-05-19

**Key conventions identified:**
1. **Error handling:** `thiserror::Error` derive with descriptive messages
2. **Builder pattern:** Consuming `self` returning `Self` (Target::with_label style)
3. **AWS SDK:** Map errors to DiscoveryError with `.map_err(|e| DiscoveryError::X(e.to_string()))`
4. **Async/await:** Tokio runtime, `async fn` for I/O operations
5. **Derive macros:** `Debug, Clone` minimum; add `Serialize, Deserialize` for API types
6. **Logging:** `tracing` crate with `info!`, `debug!`, `warn!`, `error!` macros
7. **Cache:** `Arc<RwLock<T>>` pattern for shared mutable state
8. **Option handling:** Prefer `and_then()` chains over `match` for deep nesting (see discovery.rs lines 306-311)
