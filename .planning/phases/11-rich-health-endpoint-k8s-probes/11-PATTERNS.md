# Phase 11: Rich Health Endpoint & k8s Probes — Pattern Map

**Mapped:** 2026-07-07
**Files analyzed:** 4
**Analogs found:** 4 / 4

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `src/state/app_state.rs` | state/model | CRUD (add fields) | self — existing `AppState` struct | exact (modify existing) |
| `src/main.rs` | entrypoint | event-driven (refresh loop) | self — existing `spawn_background_refresh` | exact (modify existing) |
| `src/handlers/health.rs` | handler | request-response | `src/handlers/metrics.rs` | role-match + data flow match |
| `src/routes/health.rs` | route config | request-response | self — existing `routes()` fn | exact (extend existing) |

---

## Pattern Assignments

### `src/state/app_state.rs` — add `started_at` + `RefreshOutcome` + `last_refresh_outcome`

**Analog:** self (existing file at lines 1–116)

**Existing bare-value field pattern** (lines 66–74) — `started_at` follows this exactly:
```rust
#[derive(Clone)]
pub struct AppState {
    pub snapshot: Arc<RwLock<CacheSnapshot>>,
    pub cache_ttl_seconds: u64,           // bare value, immutable after construction
    pub config: Arc<Config>,
    pub discovery: DiscoveryService,
    pub http_client: reqwest::Client,
    pub cluster: Option<Arc<crate::cluster::ClusterState>>,
    pub metrics: Arc<crate::metrics::MetricsState>,
    pub last_manual_refresh_request: Arc<AtomicU64>,
}
```

**Existing `Arc<RwLock<Option<...>>>` pattern** — `last_refresh_outcome` follows `snapshot`:
```rust
// snapshot field (line 66) — the prototype pattern for last_refresh_outcome
pub snapshot: Arc<RwLock<CacheSnapshot>>,
// new field to add — same wrapper, wraps Option<RefreshOutcome>
pub last_refresh_outcome: Arc<RwLock<Option<RefreshOutcome>>>,
```

**Construction pattern** (lines 94–103) — two fields to add in `Ok(Self { ... })`:
```rust
Ok(Self {
    snapshot: Arc::new(RwLock::new(CacheSnapshot::default())),
    cache_ttl_seconds: config.refresh_interval.max(1),
    // ADD:
    started_at: std::time::Instant::now(),
    last_refresh_outcome: Arc::new(RwLock::new(None)),
    // ... existing fields unchanged
    last_manual_refresh_request: Arc::new(AtomicU64::new(0)),
})
```

**New struct to define** (place above `AppState` struct, after existing `CacheSnapshot`):
```rust
pub struct RefreshOutcome {
    pub success: bool,
    pub timestamp_unix: u64,  // seconds since UNIX_EPOCH
}
```

**No new imports needed** — `Arc`, `RwLock` already imported at lines 2, 6.

---

### `src/main.rs` — record `RefreshOutcome` in refresh loop and initial discovery

**Analog:** self (existing file, lines 213–304)

**Success/error branch pattern to extend** (lines 246–264) — add `last_refresh_outcome` writes to both arms:
```rust
// existing match (lines 246–264) — add outcome writes inside each arm
match refresh_cache_once(&state).await {
    Ok(target_count) => {
        timer.observe_duration();
        state.metrics.discovery_targets.set(target_count as f64);
        state.metrics.cache_refreshes.with_label_values(&["success"]).inc();
        info!("discovery refresh complete: {} targets", target_count);
        publish_cache_to_gossip(&state).await;
        // ADD:
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: true,
            timestamp_unix: unix_now(),
        });
    }
    Err(error) => {
        timer.observe_duration();
        state.metrics.discovery_errors.inc();
        state.metrics.cache_refreshes.with_label_values(&["error"]).inc();
        warn!("discovery refresh failed: {}", error);
        // ADD:
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: false,
            timestamp_unix: unix_now(),
        });
    }
}
```

**Helper function to add** (free function, same style as `calculate_jittered_delay` at line 306):
```rust
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

**Initial discovery block** (lines 128–141) — same outcome recording pattern, same two arms:
```rust
match state.discovery.discover_all_clusters(&config.clusters, config.mode.clone()).await {
    Ok(targets_aws) => {
        state.replace_cache_and_routing(targets_aws).await;
        info!("Initial discovery complete");
        publish_cache_to_gossip(&state).await;
        // ADD:
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: true,
            timestamp_unix: unix_now(),
        });
    }
    Err(e) => {
        warn!("Initial discovery failed — starting with empty cache: {}", e);
        // ADD:
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: false,
            timestamp_unix: unix_now(),
        });
    }
}
```

**Import to add at top of `main.rs`**:
```rust
use crate::state::RefreshOutcome;
```

---

### `src/handlers/health.rs` — complete rewrite: 3 handlers + typed structs + pure status fn

**Analog:** `src/handlers/metrics.rs`

**Imports pattern** (metrics.rs lines 1–4 — adapt for health):
```rust
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use crate::state::{AppState, RefreshOutcome};
```

**State extraction + single snapshot read pattern** (metrics.rs lines 10–15):
```rust
// Read snapshot under one lock acquisition; release before next lock
let (target_count, age_seconds) = {
    let snap = state.snapshot.read().await;
    let target_count = snap.cache
        .get(&crate::models::MetadataLevel::Aws)
        .map(|v| v.len())
        .unwrap_or(0);
    let age_seconds = std::time::SystemTime::now()
        .duration_since(snap.last_refresh)
        .unwrap_or_default()
        .as_secs();
    (target_count, age_seconds)
    // snap lock released here
};
```

**Chitchat lock-ordering pattern** (metrics.rs lines 18–27 — CRITICAL: drop before is_leader):
```rust
let (cluster_nodes, is_leader) = match state.cluster.as_ref() {
    None => (1usize, true),
    Some(cluster) => {
        let chitchat = cluster.handle.chitchat();
        let cc = chitchat.lock().await;
        let count = cc.live_nodes().count();
        drop(cc);  // REQUIRED: release before is_leader — it re-acquires the same lock
        let leader = cluster.is_leader().await;
        (count, leader)
    }
};
```

**Typed response structs** (model pattern from `src/models/target.rs` — use `#[derive(Serialize)]`):
```rust
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub cache: CacheHealth,
    pub cluster: ClusterHealth,
    pub last_refresh: LastRefreshHealth,
}

#[derive(Serialize)]
pub struct CacheHealth {
    pub targets: usize,
    pub age_seconds: u64,
    pub state: &'static str,  // "empty" | "populated"
}

#[derive(Serialize)]
pub struct ClusterHealth {
    pub mode: &'static str,   // "standalone" | "cluster"
    pub nodes: usize,
    pub is_leader: bool,
}

#[derive(Serialize)]
pub struct LastRefreshHealth {
    pub status: &'static str,          // "ok" | "failed" | "never"
    pub timestamp: Option<u64>,
}
```

**Handler return type** (variable status code — `(StatusCode, Json<T>)` pattern):
```rust
pub async fn health_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthResponse>) {
    // ... build response ...
    (http_status, Json(response))
}
```

**Pure status function** (same file, extract for testability — no async):
```rust
fn determine_health_status(
    target_count: usize,
    last_outcome: &Option<RefreshOutcome>,
) -> (&'static str, StatusCode) {
    match (target_count > 0, last_outcome) {
        (true, Some(RefreshOutcome { success: true, .. })) => ("healthy", StatusCode::OK),
        (true, _) => ("degraded", StatusCode::OK),
        (_, Some(RefreshOutcome { success: false, .. })) => ("starting", StatusCode::SERVICE_UNAVAILABLE),
        (_, _) => ("starting", StatusCode::OK),
    }
}
```

**Stateless liveness handler** (same shape as existing `health_handler` before this phase):
```rust
pub async fn health_live_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "alive"}))
}
```

**Readiness handler** (minimal state read — same single snapshot read pattern):
```rust
pub async fn health_ready_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let target_count = {
        let snap = state.snapshot.read().await;
        snap.cache
            .get(&crate::models::MetadataLevel::Aws)
            .map(|v| v.len())
            .unwrap_or(0)
    };

    if target_count > 0 {
        (StatusCode::OK, Json(serde_json::json!({"status": "ready"})))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status": "not_ready"})))
    }
}
```

**Test module pattern** (existing `health.rs` lines 12–27 — inline `#[cfg(test)]`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determine_health_status_healthy_when_populated_and_success() {
        let outcome = Some(RefreshOutcome { success: true, timestamp_unix: 100 });
        let (status, code) = determine_health_status(5, &outcome);
        assert_eq!(status, "healthy");
        assert_eq!(code, StatusCode::OK);
    }

    // Additional tests: degraded, starting+200, starting+503, live handler call
}
```

---

### `src/routes/health.rs` — add `/health/live` and `/health/ready` routes

**Analog:** self (existing file, lines 1–11)

**Existing route registration pattern** (lines 1–11 — extend, do not replace):
```rust
use axum::{
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::handlers::health;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_handler))
        .route("/health/live", get(health::health_live_handler))    // ADD
        .route("/health/ready", get(health::health_ready_handler))  // ADD
}
```

No change to `src/routes/mod.rs` required — `health::routes()` is already merged there.

---

## Shared Patterns

### Snapshot Read Pattern
**Source:** `src/handlers/metrics.rs` line 10; `src/handlers/sd.rs` lines 60–72
**Apply to:** All three new handlers that read snapshot

```rust
// Minimal lock scope: read only what you need, release before next await
let value = {
    let snap = state.snapshot.read().await;
    let result = snap.some_field;
    result
    // snap dropped here
};
```

### Chitchat Lock Ordering
**Source:** `src/handlers/metrics.rs` lines 18–27
**Apply to:** `health_handler` cluster section only

```rust
let chitchat = cluster.handle.chitchat();
let cc = chitchat.lock().await;
let node_count = cc.live_nodes().count();
drop(cc);  // MUST drop before is_leader() — it re-acquires the same mutex
let is_leader = cluster.is_leader().await;
```

### RwLock Write Pattern
**Source:** `src/state/app_state.rs` lines 109–115 (`replace_cache_and_routing`)
**Apply to:** `last_refresh_outcome` writes in `main.rs`

```rust
// Build value BEFORE acquiring write lock; hold lock only for assignment
*state.last_refresh_outcome.write().await = Some(RefreshOutcome { ... });
```

### Cargo.toml — Environment Macro
**Source:** `src/handlers/health.rs` (existing, line 7)
**Apply to:** `health_handler` response struct

```rust
version: env!("CARGO_PKG_VERSION"),
```

---

## No Analog Found

All files have direct analogs in the codebase. No RESEARCH.md fallback needed.

---

## Metadata

**Analog search scope:** `src/handlers/`, `src/routes/`, `src/state/`, `src/main.rs`
**Files read:** 6 (`app_state.rs`, `health.rs`, `sd.rs`, `metrics.rs`, `main.rs`, `routes/health.rs`)
**Pattern extraction date:** 2026-07-07
