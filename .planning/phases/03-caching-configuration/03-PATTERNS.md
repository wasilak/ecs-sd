# Phase 03: caching-configuration - Pattern Map

**Mapped:** 2026-05-20
**Files analyzed:** 6
**Analogs found:** 5 / 6

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `src/config.rs` | config | transform | `src/config.rs` | exact |
| `src/main.rs` | service (runtime orchestrator) | event-driven | `src/main.rs` | exact |
| `src/state/app_state.rs` | store/state | request-response | `src/state/app_state.rs` | exact |
| `src/handlers/sd.rs` | controller/handler | request-response | `src/handlers/sd.rs` | exact |
| `Cargo.toml` | config | transform | `Cargo.toml` | exact |
| `src/config.rs` tests + runtime refresh tests | test | event-driven | `src/handlers/sd.rs` (existing test module) | partial |

## Pattern Assignments

### `src/config.rs` (config, transform)

**Analog:** `src/config.rs`

**Struct + defaults pattern** (lines 1-19):
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
```

**Constructor pattern** (lines 22-29):
```rust
impl Config {
    pub fn new(clusters: Vec<String>) -> Self {
        Self {
            clusters,
            ..Default::default()
        }
    }
}
```

Use this shape, but migrate to clap derive input + conversion into runtime `Config`.

---

### `src/main.rs` (runtime orchestrator, event-driven)

**Analog:** `src/main.rs`

**Imports + startup wiring pattern** (lines 9-18):
```rust
use axum::Router;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;

use crate::config::Config;
use crate::models::{MetadataLevel, Target};
use crate::state::AppState;
```

**Initialization + fail-fast startup pattern** (lines 31-55):
```rust
let (ecs_client, ec2_client) = aws::client::create_clients().await?;
let sts_client = aws::client::create_sts_client().await;

let state = AppState::new(
    config.clone(),
    ecs_client,
    ec2_client,
    sts_client,
    region,
)
.await
.map_err(|e| {
    eprintln!("Failed to initialize discovery service: {}", e);
    std::process::exit(1);
})?;
```

**Cache warm-up + atomic write pattern** (lines 56-86):
```rust
let targets_aws = state.discovery.discover_all_clusters(&config.clusters).await;
// derive tiers...
{
    let mut cache = state.cache.write().await;
    cache.insert(MetadataLevel::Aws, targets_aws);
    cache.insert(MetadataLevel::Cluster, targets_cluster);
    cache.insert(MetadataLevel::Service, targets_service);
    cache.insert(MetadataLevel::Task, targets_task);
    cache.insert(MetadataLevel::Container, targets_container);
}
```

**Graceful shutdown select pattern** (lines 141-163):
```rust
tokio::select! {
    _ = ctrl_c => info!("Received Ctrl+C, shutting down"),
    _ = terminate => info!("Received SIGTERM, shutting down"),
}
```

Use this as baseline for spawning refresh loop and coordinating shutdown.

---

### `src/state/app_state.rs` (store/state, request-response)

**Analog:** `src/state/app_state.rs`

**Shared state layout pattern** (lines 1-14):
```rust
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
}
```

**Async constructor + propagated domain error** (lines 17-31):
```rust
pub async fn new(...) -> Result<Self, DiscoveryError> {
    let discovery = DiscoveryService::new(ecs_client, ec2_client, sts_client, region).await?;
    Ok(Self {
        cache: Arc::new(RwLock::new(HashMap::new())),
        config: Arc::new(config),
        discovery,
    })
}
```

Add `last_refresh` in this same Arc/RwLock style.

---

### `src/handlers/sd.rs` (controller/handler, request-response)

**Analog:** `src/handlers/sd.rs`

**Handler signature pattern** (lines 11-24):
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
    drop(cache);

    let filtered = filter_targets(targets, &params);
    Json(filtered)
}
```

**Refresh behavior pattern (stale-safe structure)** (lines 33-71):
```rust
let targets_aws = state.discovery.discover_all_clusters(&clusters).await;
let count = targets_aws.len();
// derive tiers...
{
    let mut cache = state.cache.write().await;
    cache.insert(MetadataLevel::Aws, targets_aws);
    cache.insert(MetadataLevel::Cluster, targets_cluster);
    cache.insert(MetadataLevel::Service, targets_service);
    cache.insert(MetadataLevel::Task, targets_task);
    cache.insert(MetadataLevel::Container, targets_container);
}
info!("Discovery refresh complete: {} targets", count);
```

**Filtering helper pattern** (lines 105-136):
```rust
fn filter_targets(targets: Vec<Target>, params: &SdQueryParams) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|target| {
            if let Some(ref cluster) = params.cluster {
                let target_cluster = target.labels.get("__meta_ecs_cluster_name");
                if target_cluster.map(|s| s.as_str()) != Some(cluster.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect()
}
```

Reuse this style for adding `X-Cache-Age` response metadata and debug cache logs.

---

### `Cargo.toml` (config, transform)

**Analog:** `Cargo.toml`

**Dependency declaration pattern** (lines 6-19):
```toml
[dependencies]
aws-config = "1.8.16"
aws-sdk-ecs = "1.124.0"
aws-sdk-ec2 = "1.0"
aws-sdk-sts = "1.103"
tokio = { version = "1.52.2", features = ["full"] }
axum = "0.7"
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
```

Follow same inline-table style when adding:
- `clap = { version = "4", features = ["derive", "env"] }`
- `humantime = "2"`
- `rand = "0.8"` (for jitter)

---

### `src/config.rs` and runtime tests (test, event-driven)

**Analog:** `src/handlers/sd.rs` test module (lines 138-230)

**In-file test module pattern**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_by_cluster() {
        // arrange / act / assert in one focused test
    }
}
```

Use this structure for:
- clap/env precedence tests in `src/config.rs`
- cache-age/header behavior tests in `src/handlers/sd.rs`
- refresh loop semantics tests (if added in-file in `main.rs`)

## Shared Patterns

### Cache tier derivation + atomic swap
**Source:** `src/main.rs` lines 60-86; `src/handlers/sd.rs` lines 37-63
**Apply to:** background refresh path in `main.rs`
```rust
let targets_cluster: Vec<Target> = targets_aws
    .iter()
    .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
    .collect();
// ...other tiers...
let mut cache = state.cache.write().await;
cache.insert(MetadataLevel::Aws, targets_aws);
cache.insert(MetadataLevel::Cluster, targets_cluster);
```

### Error handling and partial-results resilience
**Source:** `src/aws/discovery.rs` lines 54-67
**Apply to:** background refresh failures
```rust
match self.discover_cluster_targets(cluster_name).await {
    Ok(targets) => { all_targets.extend(targets); }
    Err(e) => {
        error!("Failed to discover cluster {}: {}", cluster_name, e);
        // Continue with other clusters
    }
}
```

### Graceful shutdown signal handling
**Source:** `src/main.rs` lines 141-163
**Apply to:** refresh-task cancellation choreography
```rust
let ctrl_c = async { signal::ctrl_c().await.expect("failed to install Ctrl+C handler"); };
tokio::select! {
    _ = ctrl_c => info!("Received Ctrl+C, shutting down"),
    _ = terminate => info!("Received SIGTERM, shutting down"),
}
```

### Read-path lock minimization
**Source:** `src/handlers/sd.rs` lines 15-21
**Apply to:** any handler reading cache and then doing extra work
```rust
let cache = state.cache.read().await;
let targets = cache.get(&params.level).cloned().unwrap_or_default();
drop(cache); // release before post-processing
```

## No Analog Found

| File/Change Area | Role | Data Flow | Reason |
|---|---|---|---|
| clap derive with `#[arg(env = ...)]` in `src/config.rs` | config | transform | No clap usage exists in repo yet |
| periodic scheduler with `tokio::time::interval` + `MissedTickBehavior::Skip` in `main.rs` | service | event-driven | No interval-based background worker exists yet |
| jitter calculation (`rand`) for refresh cadence | utility/runtime | event-driven | No jitter pattern currently in codebase |
| `X-Cache-Age` response header in `/sd` | handler | request-response | Current handler returns plain `Json<Vec<Target>>` without custom headers |

## Metadata

**Analog search scope:** `src/main.rs`, `src/config.rs`, `src/state/app_state.rs`, `src/handlers/*.rs`, `src/routes/*.rs`, `src/aws/*.rs`, `Cargo.toml`
**Files scanned:** 10
**Pattern extraction date:** 2026-05-20
