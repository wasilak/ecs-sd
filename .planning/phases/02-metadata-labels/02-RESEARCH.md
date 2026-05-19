# Phase 2: Metadata Labels - Research

**Researched:** 2026-05-19  
**Domain:** AWS ECS Discovery with Rust, Metadata Label System  
**Confidence:** HIGH

## Summary

Phase 2 implements a complete metadata label system with 5 hierarchical levels (container → task → service → cluster → aws) and 14 label types. The key architectural challenge is designing a `LabelBuilder` that constructs labels based on the requested metadata level while efficiently extracting AWS metadata (account ID via STS, region from SDK config, AZ from EC2). 

**Primary recommendation:** Implement a `LabelBuilder` struct with level-aware construction in `src/models/label_builder.rs`, add `aws-sdk-sts` for account ID retrieval, create `MetadataLevel` enum with `FromStr`/`Display` implementations, and use a multi-tier cache strategy where each level has its own cache entry. Filter labels at response time based on the requested level rather than discovering separately per level.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Label construction | Backend (DiscoveryService) | — | LabelBuilder owned by DiscoveryService, builds labels from AWS SDK objects |
| Level filtering | Backend (Handler) | — | Handler filters cached targets by level at response time |
| AWS metadata extraction | Backend (DiscoveryService) | — | STS/EC2 calls happen during discovery, cached in DiscoveryService |
| Query param parsing | Handler (Axum extractors) | — | Axum's `Query<T>` extractor with custom deserializer |
| Multi-tier cache | Backend (AppState) | — | HashMap<MetadataLevel, Vec<Target>> in cache structure |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| aws-sdk-sts | 1.103.0 | GetCallerIdentity for account ID | Official AWS SDK for account lookup [VERIFIED: cargo search] |
| aws-sdk-ecs | 1.124.0 | ECS API calls | Already in use, battle-tested |
| aws-sdk-ec2 | 1.0.x | EC2 DescribeInstances for AZ | Already in use, extracts availability_zone from placement |
| aws-config | 1.8.16 | SDK configuration | Provides `region()` method for region extraction |
| axum | 0.7 | HTTP framework | Already in use, Query extractor for params |
| serde | 1.0 | Serialization | Already in use, derive macros for deserializing query params |
| strum | 0.28.0 | Enum utilities | `Display` and `FromStr` derives for MetadataLevel [VERIFIED: cargo search] |
| thiserror | 1.0 | Error types | Already in use, derive Error for custom types |
| tracing | 0.1 | Logging | Already in use, debug! for missing metadata |

### Installation
```bash
cargo add aws-sdk-sts@1.103.0
cargo add strum@0.28.0 --features derive
```

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        HTTP Request                              │
│              GET /sd?level=service&cluster=prod                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Axum Query Extractor                          │
│            Parse ?level= into MetadataLevel enum                │
│              (invalid → 400 Bad Request)                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       SD Handler                                 │
│  1. Extract LevelParam from query (default: config.metadata_level)│
│  2. Read targets from cache[level]                               │
│  3. Apply cluster/service/family filters                         │
│  4. Return filtered targets as JSON                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Multi-Tier Cache                              │
│   HashMap<MetadataLevel, Vec<Target>>                           │
│   - container: targets with container labels only               │
│   - task: targets with container + task labels                  │
│   - service: targets with container + task + service labels     │
│   - cluster: targets with all above + cluster labels            │
│   - aws: targets with all 14 labels (full metadata)             │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │
┌─────────────────────────────────────────────────────────────────┐
│                  DiscoveryService (Background)                   │
│  1. Get region from aws_config::SdkConfig.region()              │
│  2. Get account_id from STS GetCallerIdentity (cached once)     │
│  3. For each task:                                              │
│     - Build labels via LabelBuilder                               │
│     - EC2 DescribeInstances → availability_zone                 │
│  4. Populate all 5 cache tiers                                    │
└─────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure
```
src/
├── models/
│   ├── mod.rs
│   ├── target.rs           # Existing Target struct
│   ├── label_builder.rs    # NEW: LabelBuilder with level-aware construction
│   └── metadata_level.rs   # NEW: MetadataLevel enum + FromStr
├── aws/
│   ├── mod.rs
│   ├── discovery.rs        # Extend with STS client, account_id caching
│   └── client.rs           # Add STS client creation
├── handlers/
│   ├── sd.rs               # Add LevelParam, filter by level
│   └── ...
└── state/
    └── app_state.rs        # Change cache to HashMap<MetadataLevel, Vec<Target>>
```

### Pattern 1: LabelBuilder with Level-Aware Construction
**What:** Builder pattern that accumulates AWS SDK objects and builds labels based on requested level

**When to use:** When constructing Target labels during discovery

**Example:**
```rust
// src/models/label_builder.rs
use std::collections::HashMap;
use aws_sdk_ecs::types::{ContainerDefinition, Task, TaskDefinition, Service, Cluster};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataLevel {
    Container,
    Task,
    Service,
    Cluster,
    Aws,
}

impl MetadataLevel {
    /// Returns true if self includes the given level
    /// e.g., Aws.includes(Task) == true, Task.includes(Aws) == false
    pub fn includes(&self, other: MetadataLevel) -> bool {
        use MetadataLevel::*;
        match (self, other) {
            (Aws, _) => true,
            (Cluster, Container) | (Cluster, Task) | (Cluster, Service) | (Cluster, Cluster) => true,
            (Service, Container) | (Service, Task) | (Service, Service) => true,
            (Task, Container) | (Task, Task) => true,
            (Container, Container) => true,
            _ => false,
        }
    }
}

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
    availability_zone: String,
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
        // Extract revision from task definition ARN
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

    pub fn with_aws(mut self, region: &str, account_id: &str, az: &str) -> Self {
        self.aws_data = Some(AwsData {
            region: region.to_string(),
            account_id: account_id.to_string(),
            availability_zone: az.to_string(),
        });
        self
    }

    pub fn build(self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        
        use MetadataLevel::*;
        
        // Container level (always included if level >= Container)
        if self.level.includes(Container) {
            if let Some(data) = self.container_data {
                labels.insert("__meta_ecs_container_name".to_string(), data.name);
                labels.insert("__meta_ecs_container_image".to_string(), data.image);
                labels.insert("__meta_ecs_metrics_port".to_string(), data.port.to_string());
            }
        }
        
        // Task level
        if self.level.includes(Task) {
            if let Some(data) = self.task_data {
                labels.insert("__meta_ecs_task_arn".to_string(), data.arn);
                labels.insert("__meta_ecs_task_family".to_string(), data.family);
                labels.insert("__meta_ecs_task_version".to_string(), data.version);
            }
        }
        
        // Service level
        if self.level.includes(Service) {
            if let Some(data) = self.service_data {
                labels.insert("__meta_ecs_service_name".to_string(), data.name);
                labels.insert("__meta_ecs_desired_count".to_string(), data.desired_count.to_string());
                labels.insert("__meta_ecs_running_count".to_string(), data.running_count.to_string());
            }
        }
        
        // Cluster level
        if self.level.includes(Cluster) {
            if let Some(data) = self.cluster_data {
                labels.insert("__meta_ecs_cluster_name".to_string(), data.name);
                labels.insert("__meta_ecs_cluster_arn".to_string(), data.arn);
            }
        }
        
        // AWS level
        if self.level.includes(Aws) {
            if let Some(data) = self.aws_data {
                labels.insert("__meta_ecs_region".to_string(), data.region);
                labels.insert("__meta_ecs_account_id".to_string(), data.account_id);
                labels.insert("__meta_ecs_availability_zone".to_string(), data.availability_zone);
            }
        }
        
        labels
    }
}
```
**Source:** Builder pattern adapted from Phase 1 Target::with_label pattern

### Pattern 2: STS GetCallerIdentity for Account ID
**What:** Use aws-sdk-sts to get AWS account ID once at startup

**When to use:** During DiscoveryService initialization

**Example:**
```rust
// src/aws/discovery.rs - extend DiscoveryService
use aws_sdk_sts::Client as StsClient;

pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    sts_client: aws_sdk_sts::Client,
    account_id: String,
    region: String,
}

impl DiscoveryService {
    pub async fn new(
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: aws_sdk_sts::Client,
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
        
        // Get region from SDK config (passed in or extracted from client)
        let region = "us-east-1".to_string(); // Extract from aws_config::SdkConfig
        
        Ok(Self {
            ecs_client,
            ec2_client,
            sts_client,
            account_id,
            region,
        })
    }
}
```
**Source:** AWS SDK for Rust STS patterns [CITED: Context7 aws-sdk-rust]

### Pattern 3: MetadataLevel FromStr for Query Params
**What:** Implement `FromStr` for MetadataLevel to enable Axum Query parsing

**When to use:** For `?level=<level>` query parameter support

**Example:**
```rust
// src/models/metadata_level.rs
use std::str::FromStr;
use serde::Deserialize;
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Deserialize)]
#[strum(ascii_case_insensitive, serialize_all = "lowercase")]
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

// Usage in handler
#[derive(Debug, Deserialize)]
pub struct SdQueryParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub level: MetadataLevel,
}

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<SdQueryParams>,
) -> Result<Json<Vec<Target>>, StatusCode> {
    let level = params.level;
    let cache = state.cache.read().await;
    let targets = cache.get(&level).cloned().unwrap_or_default();
    // ... filter and return
}
```
**Source:** Axum Query extractor + strum derive macros [ASSUMED: common Rust patterns]

### Pattern 4: Multi-Tier Cache with RwLock
**What:** Cache targets per metadata level using HashMap<MetadataLevel, Vec<Target>>

**When to use:** When serving requests with different level requirements

**Example:**
```rust
// src/state/app_state.rs
use std::collections::HashMap;
use crate::models::metadata_level::MetadataLevel;

pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
}

// During discovery, populate all 5 levels
async fn refresh_cache(&self) {
    let targets_aws = self.discovery.discover_all_clusters_aws_level().await;
    
    let mut cache = self.cache.write().await;
    cache.insert(MetadataLevel::Aws, targets_aws.clone());
    
    // Derive lower levels by filtering labels
    cache.insert(MetadataLevel::Cluster, 
        targets_aws.iter().map(|t| filter_labels(t, MetadataLevel::Cluster)).collect());
    cache.insert(MetadataLevel::Service,
        targets_aws.iter().map(|t| filter_labels(t, MetadataLevel::Service)).collect());
    cache.insert(MetadataLevel::Task,
        targets_aws.iter().map(|t| filter_labels(t, MetadataLevel::Task)).collect());
    cache.insert(MetadataLevel::Container,
        targets_aws.iter().map(|t| filter_labels(t, MetadataLevel::Container)).collect());
}
```

### Anti-Patterns to Avoid
- **Discovering separately per level:** Don't make 5 separate AWS discovery calls — discover once at aws level, then derive lower levels by filtering labels
- **Storing level in Target:** Don't add `level: MetadataLevel` to Target struct — the same target can be viewed at different levels
- **Panicking on missing metadata:** Don't use `expect()` or `unwrap()` for optional metadata — omit the label instead
- **Parsing ARNs manually:** Don't split ARNs with string operations to get account ID — use STS GetCallerIdentity

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Enum string conversion | Manual match statements | `strum::EnumString` + `strum::Display` | Handles case-insensitive parsing, serialization, derives Debug/Clone/Copy |
| Query param parsing | Manual string extraction | Axum `Query<T>` extractor | Type-safe, automatic validation, integrates with serde |
| AWS account lookup | Parsing cluster ARN strings | STS `GetCallerIdentity` | Official API, handles all credential types, no ARN format assumptions |
| Region detection | Environment variable reads | `aws_config::SdkConfig.region()` | Uses standard AWS region provider chain |
| Error type boilerplate | Manual Error impl | `thiserror::Error` derive | Concise, integrates with std::error::Error |
| HashMap for labels | BTreeMap or custom struct | `HashMap<String, String>` | Standard Prometheus format, O(1) lookups, serde compatible |

## Common Pitfalls

### Pitfall 1: STS Permission Errors on Startup
**What goes wrong:** DiscoveryService::new fails if AWS credentials lack STS permissions
**Why it happens:** Default ECS/EC2 policies may not include STS:GetCallerIdentity
**How to avoid:** 
- Document required IAM permissions including `sts:GetCallerIdentity`
- Consider making account_id optional (omit `__meta_ecs_account_id` label if STS fails)
- Cache account_id lazily on first successful discovery instead of at startup

**Warning signs:** Server fails to start with "STS API error" in logs

### Pitfall 2: EC2 DescribeInstances Missing Placement Data
**What goes wrong:** Availability zone is missing from EC2 response
**Why it happens:** Instance may be in a state where placement info isn't populated
**How to avoid:**
- Use `instances.reservations().first().and_then(|r| r.instances().first()).and_then(|i| i.placement().and_then(|p| p.availability_zone()))`
- Omit `__meta_ecs_availability_zone` label if AZ unavailable

**Warning signs:** Debug logs show "placement unavailable" for some tasks

### Pitfall 3: Invalid Level Query Param Returns 500
**What goes wrong:** Unknown level value (e.g., `?level=foo`) causes panic or 500 error
**Why it happens:** FromStr returns Err, Axum doesn't know how to handle it
**How to avoid:**
- Implement custom rejection handler for invalid level values
- Return 400 Bad Request with clear message: "Invalid level: foo. Valid: container, task, service, cluster, aws"

**Warning signs:** Integration tests with invalid params return 500 instead of 400

### Pitfall 4: Standalone Tasks Missing Service Labels
**What goes wrong:** Tasks not in a service cause LabelBuilder::with_service to fail
**Why it happens:** Standalone tasks have no service data
**How to avoid:**
- Pass `Option<&Service>` to with_service
- Skip service-level labels if service is None
- Log at debug level: "Task X has no service, omitting service labels"

**Warning signs:** Missing service labels for tasks that should have them

### Pitfall 5: Cache Inconsistency Across Levels
**What goes wrong:** Different levels show different targets (e.g., aws level shows 10 targets, task level shows 9)
**Why it happens:** Separate cache updates or filtering logic errors
**How to avoid:**
- Populate all 5 cache tiers atomically from a single discovery run
- Derive lower levels from aws level by label filtering (not separate discoveries)
- Use single write lock when updating all tiers

**Warning signs:** Target counts differ across level requests

## Code Examples

### STS Client Creation and Account ID Retrieval
```rust
use aws_config::BehaviorVersion;
use aws_sdk_sts::Client as StsClient;

async fn create_sts_client() -> StsClient {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    StsClient::new(&config)
}

async fn get_account_id(sts_client: &StsClient) -> Result<String, Box<dyn std::error::Error>> {
    let response = sts_client
        .get_caller_identity()
        .send()
        .await?;
    
    Ok(response
        .account()
        .ok_or("Account ID not found in STS response")?
        .to_string())
}
```
**Source:** AWS SDK for Rust [CITED: Context7 aws-sdk-rust]

### Extracting Region from SDK Config
```rust
use aws_config::SdkConfig;

fn get_region(config: &SdkConfig) -> String {
    config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "us-east-1".to_string())
}
```
**Source:** AWS SDK for Rust [CITED: Context7 aws-sdk-rust]

### Availability Zone from EC2 DescribeInstances
```rust
// Extracted from existing discovery.rs resolve_target_address
let az = instances
    .reservations()
    .first()
    .and_then(|r| r.instances().first())
    .and_then(|i| i.placement())
    .and_then(|p| p.availability_zone())
    .map(|s| s.to_string());
```

### Query Param Handler with Level Validation
```rust
use axum::{extract::Query, http::StatusCode, Json};

#[derive(Debug, Deserialize)]
struct SdParams {
    cluster: Option<String>,
    service: Option<String>,
    family: Option<String>,
    #[serde(default = "default_level")]
    level: MetadataLevel,
}

fn default_level() -> MetadataLevel {
    MetadataLevel::Task
}

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<SdParams>,
) -> Result<Json<Vec<Target>>, (StatusCode, String)> {
    let cache = state.cache.read().await;
    let targets = cache
        .get(&params.level)
        .cloned()
        .unwrap_or_default();
    
    let filtered = filter_targets(targets, params);
    Ok(Json(filtered))
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual string parsing for account ID | STS GetCallerIdentity | v2 AWS SDK | Official API, no ARN assumptions |
| String-based level passing | Enum with FromStr/Display | Rust 2021 | Type-safe, compile-time checking |
| Single global cache | Multi-tier per-level cache | Phase 2 design | Supports per-request level overrides efficiently |
| Discovery-time level filtering | Response-time filtering | Phase 2 design | More flexible, single discovery run |

**Deprecated/outdated:**
- Parsing ARNs to extract account ID: Use STS API instead
- Environment variable for region: Use SdkConfig.region() instead
- Manual enum string conversion: Use strum derive macros instead

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `aws-sdk-sts` v1.103.0 is compatible with existing `aws-config` v1.8.16 | Standard Stack | Version mismatch could cause compilation errors |
| A2 | `strum` v0.28.0 supports `#[serde(rename_all = "lowercase")]` equivalent via `serialize_all` | Architecture Patterns | May need manual serde implementation |
| A3 | STS GetCallerIdentity returns account ID for all valid AWS credentials | Pattern 2 | Will fail for credentials without STS permissions |
| A4 | EC2 DescribeInstances placement.availability_zone is always present for running EC2 instances | Pitfall 2 | May be missing for terminated/launching instances |
| A5 | Memory overhead of storing 5 cache tiers is acceptable for typical ECS scale (hundreds of tasks) | Pattern 4 | Large clusters could cause memory pressure |

## Open Questions

1. **IAM Permissions Documentation**
   - What we know: STS:GetCallerIdentity is needed for account ID
   - What's unclear: Should we document minimal IAM policy or provide CFN/Terraform examples?
   - Recommendation: Add IAM permissions section to README

2. **Standalone Task Support**
   - What we know: Tasks can exist without services (standalone)
   - What's unclear: Should we discover standalone tasks or only service-backed tasks?
   - Recommendation: Include standalone tasks, omit service-level labels as documented in D-05

3. **AZ Missing Handling**
   - What we know: EC2 placement may not always have AZ
   - What's unclear: Is this a real scenario for running EC2 instances in ECS?
   - Recommendation: Omit label if AZ unavailable, log at debug level

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| aws-sdk-sts | META-13 (account_id) | ✓ | 1.103.0 | None — required for account ID |
| strum | MetadataLevel derives | ✓ | 0.28.0 | Manual FromStr/Display impl |
| Rust 2024 edition | Project | ✓ | — | — |
| cargo | Build | ✓ | — | — |

**Missing dependencies with no fallback:**
- None — all dependencies are available via crates.io

**Missing dependencies with fallback:**
- None

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Built-in Rust test (`cargo test`) |
| Config file | None — inline `#[cfg(test)]` modules |
| Quick run command | `cargo test` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| META-01 | Container name label | Unit | `cargo test label_builder::tests::test_container_labels` | ❌ Wave 0 |
| META-04 | Task ARN label | Unit | `cargo test label_builder::tests::test_task_labels` | ❌ Wave 0 |
| META-07 | Service name label | Unit | `cargo test label_builder::tests::test_service_labels` | ❌ Wave 0 |
| META-10 | Cluster name label | Unit | `cargo test label_builder::tests::test_cluster_labels` | ❌ Wave 0 |
| META-12 | Region label | Unit | `cargo test label_builder::tests::test_aws_labels` | ❌ Wave 0 |
| META-15 | --metadata-level flag | Integration | `cargo test config::tests` | ❌ Wave 0 |
| META-16 | ?level= query param | Unit | `cargo test handlers::sd::tests::test_level_filtering` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test` (fast feedback)
- **Per wave merge:** `cargo test` (full suite)
- **Phase gate:** All tests pass before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/models/label_builder.rs` — new file for LabelBuilder
- [ ] `src/models/metadata_level.rs` — new file for MetadataLevel enum
- [ ] `src/models/label_builder.rs` tests — unit tests for all label levels
- [ ] `src/handlers/sd.rs` tests — add level filtering tests
- [ ] STS client integration tests — mock STS responses
- [ ] Update `src/aws/client.rs` — add STS client creation

## Security Domain

**Security enforcement:** Not explicitly configured; assumed enabled for risk assessment.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | No user auth in this service |
| V3 Session Management | No | Stateless service, no sessions |
| V4 Access Control | Yes | IAM roles for AWS API access |
| V5 Input Validation | Yes | Query param validation for `?level=` |
| V6 Cryptography | No | No custom crypto, AWS SDK handles TLS |

### Known Threat Patterns for AWS SDK

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| STS credential exposure | Information Disclosure | Use IAM roles, never hardcode credentials |
| Query param injection | Tampering | Strict enum validation for level param |
| Cache poisoning | Tampering | Immutable cache entries, background refresh only |
| Excessive AWS API calls | Denial of Service | Discovery interval minimum (30s), cache-first serving |

## Sources

### Primary (HIGH confidence)
- Phase 2 Context: `.planning/phases/02-metadata-labels/02-CONTEXT.md` — User decisions from discuss-phase
- Phase 1 Context: `.planning/phases/01-core-discovery-http-api/01-CONTEXT.md` — Prior decisions to carry forward
- Requirements: `.planning/REQUIREMENTS.md` — META-01..16 specifications
- Existing code: `src/aws/discovery.rs`, `src/models/target.rs`, `src/handlers/sd.rs` — Established patterns

### Secondary (MEDIUM confidence)
- AWS SDK for Rust: Context7 `/awslabs/aws-sdk-rust` — STS initialization, region extraction patterns
- Cargo registry: `aws-sdk-sts` v1.103.0, `strum` v0.28.0 — Version verification

### Tertiary (LOW confidence)
- Rust builder pattern conventions — Assumed based on Phase 1 Target::with_label pattern
- Axum Query extractor patterns — Assumed based on existing FilterParams usage

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Versions verified via cargo search
- Architecture: HIGH — Based on Phase 1 patterns and Context decisions
- Pitfalls: MEDIUM — Inferred from AWS SDK behavior, not all verified

**Research date:** 2026-05-19  
**Valid until:** 30 days (AWS SDK versions change slowly)

---

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| META-01 | `__meta_ecs_container_name` — container name | LabelBuilder.with_container extracts from ContainerDefinition.name() |
| META-02 | `__meta_ecs_container_image` — container image URI | LabelBuilder.with_container extracts from ContainerDefinition.image() |
| META-03 | `__meta_ecs_metrics_port` — port from docker label | LabelBuilder.with_container receives port parameter from discovery |
| META-04 | `__meta_ecs_task_arn` — full task ARN | LabelBuilder.with_task extracts from Task.task_arn() |
| META-05 | `__meta_ecs_task_family` — task definition family | LabelBuilder.with_task extracts from TaskDefinition.family() |
| META-06 | `__meta_ecs_task_version` — task definition revision | LabelBuilder.with_task parses from TaskDefinition.task_definition_arn() |
| META-07 | `__meta_ecs_service_name` — service name | LabelBuilder.with_service extracts from Service.service_name() |
| META-08 | `__meta_ecs_desired_count` — service desired count | LabelBuilder.with_service extracts from Service.desired_count() |
| META-09 | `__meta_ecs_running_count` — service running count | LabelBuilder.with_service extracts from Service.running_count() |
| META-10 | `__meta_ecs_cluster_name` — cluster name | LabelBuilder.with_cluster extracts from Cluster.cluster_name() |
| META-11 | `__meta_ecs_cluster_arn` — cluster ARN | LabelBuilder.with_cluster extracts from Cluster.cluster_arn() |
| META-12 | `__meta_ecs_region` — AWS region | Cached in DiscoveryService from SdkConfig.region() |
| META-13 | `__meta_ecs_account_id` — AWS account ID from ARN | STS GetCallerIdentity once at DiscoveryService startup |
| META-14 | `__meta_ecs_availability_zone` — EC2 instance AZ | EC2 DescribeInstances.placement.availability_zone |
| META-15 | `--metadata-level` flag sets global default | Config.metadata_level connects to DiscoveryService |
| META-16 | `?level=<level>` query param overrides per-request | LevelParam in SdQueryParams with serde default |
