---
phase: 02-metadata-labels
plan: 03
type: execute
wave: 2
depends_on:
  - 02-PLAN-02-label-implementation.md
files_modified:
  - src/config.rs
  - src/models/mod.rs
  - src/handlers/sd.rs
  - src/state/app_state.rs
autonomous: true
requirements:
  - META-15
  - META-16
must_haves:
  truths:
    - Config.metadata_level is MetadataLevel enum (not String)
    - SdQueryParams includes level field with MetadataLevel type
    - Handler reads from multi-tier cache by level
    - Invalid level returns 400 Bad Request with clear message
    - Query param ?level= overrides config default
  artifacts:
    - path: src/config.rs
      provides: Config with MetadataLevel metadata_level field
      exports: [Config]
    - path: src/models/mod.rs
      provides: SdQueryParams with level field
      exports: [SdQueryParams]
    - path: src/handlers/sd.rs
      provides: Handler with level filtering
      exports: [sd_handler]
    - path: src/state/app_state.rs
      provides: Multi-tier cache HashMap<MetadataLevel, Vec<Target>>
      exports: [AppState]
  key_links:
    - from: sd_handler
      to: cache
      via: state.cache.read().await.get(&params.level)
    - from: SdQueryParams
      to: MetadataLevel
      via: #[serde(default)] level: MetadataLevel
---

<objective>
Implement metadata level configuration: CLI flag, query parameter, and multi-tier cache.

Purpose: Allow users to configure which metadata labels are returned via global default (--metadata-level flag) and per-request override (?level= query parameter), with efficient multi-tier caching.

Output:
- Config.metadata_level as MetadataLevel enum
- SdQueryParams with level field for query param parsing
- Multi-tier cache: HashMap<MetadataLevel, Vec<Target>>
- Updated handler that filters by requested level
- Per-request level override support
</objective>

<execution_context>
@/Users/piotrek/.config/opencode/get-shit-done/workflows/execute-plan.md
</execution_context>

<context>
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-CONTEXT.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-RESEARCH.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md
@/Users/piotrek/git/ecs-sd/src/config.rs — current Config
@/Users/piotrek/git/ecs-sd/src/handlers/sd.rs — current handler
@/Users/piotrek/git/ecs-sd/src/state/app_state.rs — current AppState

## Interface Context

### From PATTERNS.md - Config Change
```rust
use crate::models::MetadataLevel;

#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: MetadataLevel,  // Changed from String
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: MetadataLevel::default(),  // Task level
        }
    }
}
```

### From PATTERNS.md - SdQueryParams
```rust
#[derive(Debug, Deserialize)]
pub struct SdQueryParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub level: MetadataLevel,  // Uses MetadataLevel::default() = Task
}
```

### From PATTERNS.md - AppState Cache
```rust
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
}
```

### From PATTERNS.md - Handler Cache Access
```rust
let cache = state.cache.read().await;
let targets = cache
    .get(&params.level)
    .cloned()
    .unwrap_or_default();
```
</context>

<tasks>

<task type="auto">
  <name>Task 2-03-01: Update Config to use MetadataLevel enum</name>
  <files>src/config.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/config.rs` — current Config struct
    - `/Users/piotrek/git/ecs-sd/src/models/metadata_level.rs` — MetadataLevel enum
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — Config pattern
  </read_first>
  <acceptance_criteria>
    - Config.metadata_level field type is MetadataLevel (not String)
    - Default is MetadataLevel::default() (which is Task)
    - Import for MetadataLevel present
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/config.rs:
    
    ```rust
    use crate::models::MetadataLevel;

    #[derive(Debug, Clone)]
    pub struct Config {
        pub clusters: Vec<String>,
        pub listen: String,
        pub refresh_interval: u64,
        pub metadata_level: MetadataLevel,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                clusters: Vec::new(),
                listen: "0.0.0.0:8080".to_string(),
                refresh_interval: 60,
                metadata_level: MetadataLevel::default(),
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
    
    Per META-15: --metadata-level flag uses MetadataLevel.
    Per D-02: Default level is Task.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Config updated with MetadataLevel</done>
</task>

<task type="auto">
  <name>Task 2-03-02: Create SdQueryParams with level field</name>
  <files>src/models/mod.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/models/mod.rs` — current FilterParams
    - `/Users/piotrek/git/ecs-sd/src/models/metadata_level.rs` — MetadataLevel
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — SdQueryParams pattern
  </read_first>
  <acceptance_criteria>
    - SdQueryParams struct exists with level: MetadataLevel field
    - #[serde(default)] on level field
    - Also includes cluster, service, family as Option<String>
    - FilterParams can be kept for backward compatibility or replaced
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/models/mod.rs:
    
    ```rust
    pub mod target;
    pub use target::Target;

    pub mod metadata_level;
    pub use metadata_level::MetadataLevel;

    pub mod label_builder;
    pub use label_builder::LabelBuilder;

    use serde::Deserialize;

    /// Query parameters for the /sd endpoint
    #[derive(Debug, Deserialize)]
    pub struct SdQueryParams {
        pub cluster: Option<String>,
        pub service: Option<String>,
        pub family: Option<String>,
        /// Metadata level to return (default: from config, typically "task")
        #[serde(default)]
        pub level: MetadataLevel,
    }

    /// Legacy filter params - kept for compatibility
    #[derive(Debug, Deserialize)]
    pub struct FilterParams {
        pub cluster: Option<String>,
        pub service: Option<String>,
        pub family: Option<String>,
    }
    ```
    
    Per META-16: ?level= query parameter parsed via serde.
    Per D-02: Level field has default for when param omitted.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>SdQueryParams created with level field</done>
</task>

<task type="auto">
  <name>Task 2-03-03: Update AppState cache to multi-tier HashMap</name>
  <files>src/state/app_state.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/state/app_state.rs` — current cache definition
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — AppState pattern
  </read_first>
  <acceptance_criteria>
    - cache field type is Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>>
    - HashMap imported from std::collections
    - MetadataLevel imported
    - cache initialized as HashMap::new()
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/state/app_state.rs:
    
    ```rust
    use std::sync::Arc;
    use std::collections::HashMap;
    use tokio::sync::RwLock;
    use crate::config::Config;
    use crate::error::DiscoveryError;
    use crate::models::{MetadataLevel, Target};
    use crate::aws::DiscoveryService;

    #[derive(Clone)]
    pub struct AppState {
        pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
        pub config: Arc<Config>,
        pub discovery: DiscoveryService,
    }

    impl AppState {
        pub async fn new(
            config: Config,
            ecs_client: aws_sdk_ecs::Client,
            ec2_client: aws_sdk_ec2::Client,
            sts_client: aws_sdk_sts::Client,
            region: String,
        ) -> Result<Self, DiscoveryError> {
            let discovery = DiscoveryService::new(ecs_client, ec2_client, sts_client, region).await?;

            Ok(Self {
                cache: Arc::new(RwLock::new(HashMap::new())),
                config: Arc::new(config),
                discovery,
            })
        }
    }
    ```
    
    Per D-04: Multi-tier cache stores targets per level.
    Per D-02: Allows per-request level override with efficient lookup.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>AppState cache updated to HashMap</done>
</task>

<task type="auto">
  <name>Task 2-03-04: Update refresh_handler to populate all cache tiers</name>
  <files>src/handlers/sd.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/handlers/sd.rs` — current refresh_handler
    - `/Users/piotrek/git/ecs-sd/src/models/label_builder.rs` — LabelBuilder and MetadataLevel
  </read_first>
  <acceptance_criteria>
    - refresh_handler calls discovery once at Aws level
    - Derives all 5 cache tiers from Aws-level targets
    - Populates cache for Container, Task, Service, Cluster, Aws levels
    - Uses single write lock for all updates
    - Returns count of targets in response
  </acceptance_criteria>
  <action>
    Modify src/handlers/sd.rs refresh_handler:
    
    Add helper function to filter labels by level:
    ```rust
    /// Filter target labels to only include those for the specified level
    fn filter_labels_by_level(target: &Target, level: MetadataLevel) -> Target {
        let filtered_labels: std::collections::HashMap<String, String> = target
            .labels
            .iter()
            .filter(|(key, _)| {
                // Determine which level this label belongs to based on prefix
                let label_level = if key.starts_with("__meta_ecs_container_") || key == "__meta_ecs_metrics_port" {
                    MetadataLevel::Container
                } else if key.starts_with("__meta_ecs_task_") {
                    MetadataLevel::Task
                } else if key.starts_with("__meta_ecs_service_") {
                    MetadataLevel::Service
                } else if key.starts_with("__meta_ecs_cluster_") {
                    MetadataLevel::Cluster
                } else if key.starts_with("__meta_ecs_") {
                    MetadataLevel::Aws
                } else {
                    MetadataLevel::Container // Default for unknown labels
                };
                
                level.includes(label_level)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        Target {
            targets: target.targets.clone(),
            labels: filtered_labels,
        }
    }
    ```
    
    Update refresh_handler to populate all tiers:
    ```rust
    pub async fn refresh_handler(
        State(state): State<AppState>,
    ) -> Json<serde_json::Value> {
        let clusters = state.config.clusters.clone();

        info!("Manual discovery refresh triggered");

        // Discover at full Aws level
        let targets_aws = state.discovery.discover_all_clusters(&clusters).await;
        let count = targets_aws.len();

        // Derive all cache tiers from Aws-level targets
        let targets_cluster: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
            .collect();
        let targets_service: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Service))
            .collect();
        let targets_task: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Task))
            .collect();
        let targets_container: Vec<Target> = targets_aws
            .iter()
            .map(|t| filter_labels_by_level(t, MetadataLevel::Container))
            .collect();

        // Update all cache tiers atomically
        {
            let mut cache = state.cache.write().await;
            cache.insert(MetadataLevel::Aws, targets_aws);
            cache.insert(MetadataLevel::Cluster, targets_cluster);
            cache.insert(MetadataLevel::Service, targets_service);
            cache.insert(MetadataLevel::Task, targets_task);
            cache.insert(MetadataLevel::Container, targets_container);
        }

        info!("Discovery refresh complete: {} targets", count);

        Json(json!({
            "status": "ok",
            "targets_discovered": count
        }))
    }
    ```
    
    Per D-04: All 5 cache tiers populated from single discovery run.
    Per D-02: Handler filters by level at response time.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>refresh_handler populates all cache tiers</done>
</task>

<task type="auto">
  <name>Task 2-03-05: Update sd_handler for level-based cache access</name>
  <files>src/handlers/sd.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/handlers/sd.rs` — current sd_handler
    - `/Users/piotrek/git/ecs-sd/src/models/mod.rs` — SdQueryParams
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — handler pattern
  </read_first>
  <acceptance_criteria>
    - sd_handler uses SdQueryParams instead of FilterParams
    - Handler reads from cache using params.level
    - Returns empty vec if level not in cache
    - Maintains existing cluster/service/family filtering
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/handlers/sd.rs:
    
    1. Update imports:
    ```rust
    use axum::{
        extract::{Query, State},
        Json,
    };
    use crate::state::AppState;
    use crate::models::{MetadataLevel, SdQueryParams, Target};
    use serde_json::json;
    use tracing::info;
    ```
    
    2. Update handler signature and implementation:
    ```rust
    pub async fn sd_handler(
        State(state): State<AppState>,
        Query(params): Query<SdQueryParams>,
    ) -> Json<Vec<Target>> {
        let cache = state.cache.read().await;
        let targets = cache
            .get(&params.level)
            .cloned()
            .unwrap_or_default();
        drop(cache); // Release read lock before filtering
        
        let filtered = filter_targets(targets, &params);
        Json(filtered)
    }
    ```
    
    3. Update filter_targets to take reference:
    ```rust
    fn filter_targets(targets: Vec<Target>, params: &SdQueryParams) -> Vec<Target> {
        // Same implementation as before, just use reference
        targets
            .into_iter()
            .filter(|target| {
                // ... existing filter logic ...
            })
            .collect()
    }
    ```
    
    Per META-16: ?level= query param determines which cache tier to read.
    Per D-02: Level from query param overrides default.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>sd_handler uses level-based cache access</done>
</task>

<task type="auto">
  <name>Task 2-03-06: Update initial discovery in main.rs to populate cache</name>
  <files>src/main.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/main.rs` — initial discovery code
  </read_first>
  <acceptance_criteria>
    - Initial discovery populates all 5 cache tiers
    - Uses same filter_labels_by_level logic as refresh_handler
    - Discovery happens before starting HTTP server
    - Any errors logged but don't prevent server startup
  </acceptance_criteria>
  <action>
    Modify src/main.rs initial discovery section (around where AppState is created and discovery happens):
    
    ```rust
    // After creating app_state:
    
    // Perform initial discovery to populate cache
    let clusters = app_state.config.clusters.clone();
    let targets_aws = app_state.discovery.discover_all_clusters(&clusters).await;
    
    // Derive all cache tiers from Aws-level targets
    let targets_cluster: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
        .collect();
    let targets_service: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Service))
        .collect();
    let targets_task: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Task))
        .collect();
    let targets_container: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Container))
        .collect();

    // Populate cache
    {
        let mut cache = app_state.cache.write().await;
        cache.insert(MetadataLevel::Aws, targets_aws);
        cache.insert(MetadataLevel::Cluster, targets_cluster);
        cache.insert(MetadataLevel::Service, targets_service);
        cache.insert(MetadataLevel::Task, targets_task);
        cache.insert(MetadataLevel::Container, targets_container);
    }
    
    info!("Initial discovery complete");
    ```
    
    Add filter_labels_by_level function to main.rs or import from handlers::sd.
    
    Per D-04: Multi-tier cache populated at startup.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Initial discovery populates all cache tiers</done>
</task>

<task type="auto">
  <name>Task 2-03-07: Build and verify</name>
  <files></files>
  <read_first>
  </read_first>
  <acceptance_criteria>
    - `cargo build` passes with no errors
    - `cargo test` passes
    - All integration points verified
  </acceptance_criteria>
  <action>
    Run full build and tests:
    
    ```bash
    cargo build
    cargo test
    ```
    
    Address any compilation errors.
  </action>
  <verify>
    <automated>cargo build && cargo test</automated>
  </verify>
  <done>Full build and tests pass</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Query Param → Handler | Level validation via MetadataLevel::from_str |
| Handler → Cache | Level-based cache lookup |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-06 | Tampering | Invalid level query param | mitigate | FromStr validates, returns 400 for invalid values |
| T-02-07 | Information Disclosure | Cache cross-level leakage | mitigate | HashMap keyed by MetadataLevel ensures isolation |
| T-02-08 | Denial of Service | Empty cache returns 200 OK with [] | accept | Valid behavior, no targets found is legitimate response |
</threat_model>

<verification>
## Wave 2 Verification

### Automated Tests
```bash
cargo build              # Full build passes
cargo test               # All tests pass
```

### Manual Verification (if needed)
```bash
# Start server
cargo run

# Test different levels (requires populated cache)
curl "http://localhost:8080/sd?level=container"
curl "http://localhost:8080/sd?level=task"
curl "http://localhost:8080/sd?level=aws"
```

### Coverage Check
- [ ] Config.metadata_level is MetadataLevel type
- [ ] SdQueryParams has level field with serde(default)
- [ ] AppState.cache is HashMap<MetadataLevel, Vec<Target>>
- [ ] sd_handler reads from cache by level
- [ ] refresh_handler populates all 5 cache tiers
- [ ] filter_labels_by_level filters labels correctly
- [ ] Build passes
</verification>

<success_criteria>
1. `cargo build` passes with no errors
2. `cargo test` passes
3. Config uses MetadataLevel enum for metadata_level
4. SdQueryParams includes level field for query param parsing
5. Multi-tier cache stores targets for all 5 levels
6. Handler reads from appropriate cache tier based on requested level
7. Initial discovery populates all cache tiers at startup
</success_criteria>

<output>
After completion, create `.planning/phases/02-metadata-labels/02-SUMMARY-03-level-configuration.md`
</output>
