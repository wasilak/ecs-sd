# Phase 11: Rich Health Endpoint & k8s Probes — Research

**Researched:** 2026-07-07
**Domain:** Rust — axum handler enrichment, AppState extension, k8s/ALB probe patterns
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HEALTH-01 | `GET /health` returns structured JSON: status, version, uptime_seconds, cache sub-object, cluster sub-object, last_refresh sub-object | Handler rewrite documented below; all data sources inventoried |
| HEALTH-02 | `GET /health` returns HTTP 503 when cache state is empty AND last refresh failed | Status determination function pattern documented below |
| HEALTH-03 | `GET /health/live` always returns HTTP 200 with `{"status":"alive"}` | Stateless handler — no AppState needed |
| HEALTH-04 | `GET /health/ready` returns HTTP 200 when cache has >= 1 target, HTTP 503 when empty | Single snapshot read; same pattern as health handler but simpler |
</phase_requirements>

---

## Project Constraints (from CLAUDE.md)

- Use `rg` (ripgrep) not `grep`; use `fd` not `find`
- No git hooks bypass; no AI attribution in commit messages
- Surgical changes: touch only files required by the phase requirements
- Do not vendor Rust dependencies
- Karpathy principle: simplicity first — no speculative abstractions, no new crates unless necessary
- TaskMaster workflow: update subtask notes during implementation

---

## Summary

Phase 11 enriches the existing `/health` endpoint and adds two new k8s-safe probe routes (`/health/live`, `/health/ready`). The current `health_handler` is stateless — it returns a hardcoded JSON body with no AppState access. The new handler must read cache state, cluster topology, and a per-refresh outcome record to build the structured response.

The central design challenge is tracking whether the last refresh attempt succeeded or failed. The `CacheSnapshot` is only written on success (stale-on-failure guarantee from QUAL-02/Phase 9). This means the last successful refresh timestamp lives in `CacheSnapshot.last_refresh`, but there is no existing record of failed attempts. A new `last_refresh_outcome: Arc<RwLock<Option<RefreshOutcome>>>` field must be added to `AppState` and updated from both success and error paths in the background refresh loop in `main.rs`.

No new crates are needed. All required capabilities — JSON serialization (`serde`/`serde_json`), status codes (`axum`/`http`), async locks (`tokio::sync::RwLock`), time (`std::time`) — are already in `Cargo.toml`. The 161 existing tests must remain green after the changes.

**Primary recommendation:** Two sequential plans. Plan 1 expands AppState (add `started_at` + `last_refresh_outcome`, update refresh loop in `main.rs`). Plan 2 rewrites the health handler and adds the two new routes — it depends on Plan 1's AppState additions.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Refresh outcome tracking | State (`src/state/app_state.rs`) | Entrypoint (`src/main.rs`) | AppState owns all shared mutable state; refresh loop writes the outcome |
| Process uptime | State (`src/state/app_state.rs`) | — | `started_at: Instant` set at construction, immutable thereafter |
| Health status computation | Handler (`src/handlers/health.rs`) | — | Pure logic consuming state snapshot — no side effects |
| Liveness probe route | Handler (`src/handlers/health.rs`) | Routes (`src/routes/health.rs`) | Stateless handler; no AppState required |
| Readiness probe route | Handler (`src/handlers/health.rs`) | Routes (`src/routes/health.rs`) | Reads only `snapshot.cache` — single read lock |
| Cluster node topology | Cluster (`src/cluster/`) | Handler | Handler calls `chitchat.live_nodes().count()` and `is_leader()`, same pattern as `metrics_handler` |

---

## Standard Stack

### Core (already in Cargo.toml — no new deps)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` | `0.8` | Routing, state extraction, response types | Already in project; `State<AppState>`, `(StatusCode, Json<T>)` return pattern |
| `serde` | `1.0` | `#[derive(Serialize)]` on response structs | Already in project; typed structs beat `serde_json::json!` macro for rich responses |
| `serde_json` | `1.0` | JSON serialization | Already in project |
| `tokio::sync::RwLock` | `1.52` | `last_refresh_outcome` shared state | Already in project; matches existing `snapshot` lock type |
| `std::time::Instant` | std | Process start time (`started_at`) | No dep needed; `Instant` is Copy + Clone |
| `std::time::SystemTime` | std | Unix timestamp for `last_refresh.timestamp` | `duration_since(UNIX_EPOCH).as_secs()` gives u64 |
| `http::StatusCode` | re-exported via `axum` | HTTP 200 / 503 return codes | Already used in `metrics_handler` |

[VERIFIED: Cargo.toml — all crates confirmed present, no new additions required]

### No New Packages

This phase is a pure code addition. Zero new entries in `Cargo.toml`.

---

## Package Legitimacy Audit

> Not applicable — no new packages are installed in this phase.

---

## Architecture Patterns

### System Architecture Diagram

```
GET /health
    │
    ▼
health_handler(State<AppState>)
    │
    ├─► snapshot.read() ──► target_count, age_seconds, cache.state
    │
    ├─► last_refresh_outcome.read() ──► RefreshOutcome { success, timestamp_unix }
    │
    ├─► started_at ──► uptime_seconds = Instant::now() - started_at
    │
    └─► cluster (Option<Arc<ClusterState>>)
            ├─ None ──► mode="standalone", nodes=1, is_leader=true
            └─ Some(c) ──► chitchat.lock() ──► live_nodes().count()
                           c.is_leader().await

determine_health_status(target_count, last_outcome)
    ├─ populated + success    ──► "healthy", HTTP 200
    ├─ populated + failed     ──► "degraded", HTTP 200
    ├─ populated + never      ──► "degraded", HTTP 200  (data present, no record of success)
    ├─ empty + success/never  ──► "starting", HTTP 200
    └─ empty + failed         ──► "starting", HTTP 503  (HEALTH-02)

GET /health/live
    │
    └─► health_live_handler() — no State needed
            └─► always 200 {"status":"alive"}

GET /health/ready
    │
    └─► health_ready_handler(State<AppState>)
            └─► snapshot.read() ──► target_count
                    ├─ > 0 ──► 200 {"status":"ready"}
                    └─ == 0 ──► 503 {"status":"not_ready"}
```

### Recommended File Changes

```
src/
├── state/
│   └── app_state.rs     # Add RefreshOutcome struct, started_at field, last_refresh_outcome field
├── main.rs              # Update refresh loop to write last_refresh_outcome
├── handlers/
│   └── health.rs        # Complete rewrite — 3 handlers, typed response structs
└── routes/
    └── health.rs        # Add /health/live and /health/ready routes
```

### Pattern 1: AppState Extension — Immutable Field

The `started_at` field follows the same pattern as `cache_ttl_seconds: u64` — a bare value set at construction that is never mutated. It does not need `Arc` wrapping.

```rust
// Source: codebase analogy — src/state/app_state.rs cache_ttl_seconds pattern [VERIFIED: codebase]
#[derive(Clone)]
pub struct AppState {
    pub snapshot: Arc<RwLock<CacheSnapshot>>,
    pub cache_ttl_seconds: u64,        // existing bare field
    pub started_at: std::time::Instant, // new — same pattern, immutable after construction
    // ...
}
```

### Pattern 2: AppState Extension — Shared Mutable Outcome

`last_refresh_outcome` follows the same `Arc<RwLock<Option<...>>>` pattern as `snapshot`. The write lock is held briefly to replace the outcome record.

```rust
// Source: codebase analogy — Arc<RwLock<CacheSnapshot>> pattern [VERIFIED: codebase]
pub struct RefreshOutcome {
    pub success: bool,
    pub timestamp_unix: u64,  // seconds since UNIX_EPOCH
}

pub struct AppState {
    // ... existing fields ...
    pub started_at: std::time::Instant,
    pub last_refresh_outcome: Arc<RwLock<Option<RefreshOutcome>>>,
}
```

Construction in `AppState::new`:
```rust
Ok(Self {
    // ... existing fields ...
    started_at: std::time::Instant::now(),
    last_refresh_outcome: Arc::new(RwLock::new(None)),
})
```

### Pattern 3: Writing Outcome After Each Refresh Attempt

The background refresh loop in `main.rs` already has a `match refresh_cache_once(&state)` branch for success and error. Both branches get a write to `last_refresh_outcome`.

```rust
// Source: codebase analogy — last_manual_refresh_request AtomicU64 pattern [VERIFIED: codebase]
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

match refresh_cache_once(&state).await {
    Ok(target_count) => {
        // ... existing metrics updates ...
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: true,
            timestamp_unix: unix_now(),
        });
    }
    Err(error) => {
        // ... existing metrics updates ...
        *state.last_refresh_outcome.write().await = Some(RefreshOutcome {
            success: false,
            timestamp_unix: unix_now(),
        });
    }
}
```

Apply the same pattern after initial discovery at startup.

### Pattern 4: Typed Response Structs

Replace the current `serde_json::json!{}` macro call with `#[derive(Serialize)]` structs. This enables compile-time field checking and is consistent with how `Target` and `ProxyTarget` are defined.

```rust
// Source: codebase analogy — src/models/target.rs #[derive(Serialize)] pattern [VERIFIED: codebase]
use serde::Serialize;

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
    pub timestamp: Option<u64>,        // Unix seconds; None when never attempted
}
```

### Pattern 5: Handler Return Type for Variable Status Codes

The `metrics_handler` returns `Response` for flexibility. For the health handler, `(StatusCode, Json<T>)` is simpler and sufficient — it implements `IntoResponse` in axum.

```rust
// Source: codebase analogy — axum (StatusCode, Json<T>) IntoResponse pattern [ASSUMED: well-known axum pattern]
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};

pub async fn health_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthResponse>) {
    // ... build response ...
    (status_code, Json(response))
}
```

For `/health/live` — no state needed, always returns 200:
```rust
pub async fn health_live_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "alive"}))
}
```

For `/health/ready` — minimal response, simpler return type:
```rust
pub async fn health_ready_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let snap = state.snapshot.read().await;
    let target_count = snap.cache
        .get(&crate::models::MetadataLevel::Aws)
        .map(|v| v.len())
        .unwrap_or(0);
    drop(snap);

    if target_count > 0 {
        (StatusCode::OK, Json(serde_json::json!({"status": "ready"})))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status": "not_ready"})))
    }
}
```

### Pattern 6: Chitchat Node Count (from metrics_handler)

The `metrics_handler` already demonstrates the correct lock-ordering pattern for reading chitchat state. The health handler must follow the same sequence: lock → count → drop → is_leader (is_leader re-acquires the lock internally).

```rust
// Source: src/handlers/metrics.rs lines 18-26 [VERIFIED: codebase]
if let Some(ref cluster) = state.cluster {
    let chitchat = cluster.handle.chitchat();
    let cc = chitchat.lock().await;
    let node_count = cc.live_nodes().count();
    drop(cc);  // REQUIRED: release before calling is_leader (it re-acquires)

    let is_leader = cluster.is_leader().await;
    // use node_count and is_leader
}
```

### Pattern 7: Status Determination (pure, testable)

Extract the status/HTTP-code logic into a pure function to enable unit testing without AppState:

```rust
// Place inside health.rs — free function, no async
fn determine_health_status(
    target_count: usize,
    last_outcome: &Option<RefreshOutcome>,
) -> (&'static str, StatusCode) {
    match (target_count > 0, last_outcome) {
        (true, Some(RefreshOutcome { success: true, .. })) => ("healthy", StatusCode::OK),
        (true, _) => ("degraded", StatusCode::OK),   // has data, last refresh failed or unrecorded
        (_, Some(RefreshOutcome { success: false, .. })) => ("starting", StatusCode::SERVICE_UNAVAILABLE), // HEALTH-02
        (_, _) => ("starting", StatusCode::OK),       // no data, but no failure on record
    }
}
```

### Pattern 8: Route Registration

Follows the existing pattern in `src/routes/health.rs`. The live handler takes no state so it can be registered without `State`, but in axum 0.8 all handlers on a state-ful Router must accept the state — registering a stateless handler on a stateful Router is fine; it simply won't extract the state.

```rust
// Source: codebase analogy — src/routes/sd.rs pattern [VERIFIED: codebase]
use axum::{routing::get, Router};
use crate::state::AppState;
use crate::handlers::health;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_handler))
        .route("/health/live", get(health::health_live_handler))
        .route("/health/ready", get(health::health_ready_handler))
}
```

### Anti-Patterns to Avoid

- **Don't infer refresh failure from `CacheSnapshot.last_refresh == UNIX_EPOCH`**: This value is only updated on success; using it to detect failure is incorrect. Use `last_refresh_outcome` instead.
- **Don't read `CacheSnapshot.last_refresh` for the `last_refresh.timestamp` field**: `CacheSnapshot.last_refresh` is the time of the last *successful* refresh, not the last *attempted* refresh. The `last_refresh.timestamp` in the health response should come from `last_refresh_outcome.timestamp_unix`, which is written after both success and failure.
- **Don't hold the chitchat mutex while calling `is_leader()`**: `is_leader()` acquires the chitchat mutex internally. Holding it while calling will deadlock. Always `drop(cc)` before calling `is_leader()`.
- **Don't fold `last_refresh_outcome` into `CacheSnapshot`**: CacheSnapshot is only replaced on success. A field inside it would always show the last success, never failures. This is a locked decision from v0.3.0 planning (same rationale as `last_manual_refresh_request`).
- **Don't use `serde_json::json!{}` macro for the full health response**: Use typed structs with `#[derive(Serialize)]` — compiler-checked fields, consistent with existing model patterns.
- **Don't write `cache.age_seconds` using `CacheSnapshot.last_refresh`** for the case where cache is empty: `last_refresh == UNIX_EPOCH` when no refresh has run. `duration_since(UNIX_EPOCH)` would give a huge number. Use `unwrap_or_default()` to clamp to 0 in this case.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization | Manual field string building | `#[derive(Serialize)]` on structs | Compiler-checked, no escaping bugs |
| Unix timestamp | Manual epoch calculation | `SystemTime::duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()` | Standard library handles leap seconds, platform differences |
| Async shared state | Custom spinlock or Cell | `tokio::sync::RwLock` | Already in project; re-entrant safe |
| HTTP status variants | Hard-coded integer literals | `axum::http::StatusCode::OK` / `SERVICE_UNAVAILABLE` | Named constants, compile-time safe |

**Key insight:** This phase adds zero new crates. Everything is composition of existing primitives.

---

## Common Pitfalls

### Pitfall 1: Chitchat Deadlock on is_leader

**What goes wrong:** Calling `cluster.is_leader().await` while holding the chitchat mutex lock (`cc`) causes a deadlock. `is_leader()` acquires the same lock internally.

**Why it happens:** The chitchat mutex is a `tokio::sync::Mutex`, not re-entrant. Holding `cc = chitchat.lock().await` and then calling `is_leader()` blocks forever.

**How to avoid:** Always `drop(cc)` explicitly before calling `is_leader()`. This exact pattern is used in `src/handlers/metrics.rs` line 23.

**Warning signs:** Test hangs indefinitely; liveness probe times out.

### Pitfall 2: cache.age_seconds Overflow When Cache Is Empty

**What goes wrong:** `CacheSnapshot.last_refresh` defaults to `SystemTime::UNIX_EPOCH`. `SystemTime::now().duration_since(UNIX_EPOCH)` gives ~56 years of seconds — nonsensical for the health response.

**Why it happens:** The default snapshot has never been refreshed; `last_refresh` is a sentinel value, not a real timestamp.

**How to avoid:** Use `SystemTime::now().duration_since(snap.last_refresh).unwrap_or_default().as_secs()`. `duration_since` returns `Err` when the argument is *after* now (impossible in practice) and `Ok(0)` is fine. But more precisely: when `last_refresh == UNIX_EPOCH` AND cache is empty, report `age_seconds: 0` and `state: "empty"`. Clamp with `unwrap_or_default()`.

**Warning signs:** `/health` returns `age_seconds: 1782547200` (or similar large number).

### Pitfall 3: last_refresh_outcome Not Updated After Initial Discovery

**What goes wrong:** The initial discovery in `main.rs` runs before the background refresh loop starts. If the initial discovery fails and the background loop later succeeds, the outcome records won't reflect the initial failure. Conversely, if the handler is called before the background loop has run, `last_refresh_outcome` is `None` (never set).

**Why it happens:** The initial discovery block in `main.rs` (`if should_discover { ... }`) is a separate code path from the background refresh loop.

**How to avoid:** Record `last_refresh_outcome` in BOTH the initial discovery block AND the background refresh loop. Both success and error branches.

**Warning signs:** `/health` shows `last_refresh.status: "never"` even after a successful initial discovery.

### Pitfall 4: "degraded" vs "starting" Status Ambiguity

**What goes wrong:** When cache has targets but `last_refresh_outcome` is None (no refresh yet — impossible in practice if initial discovery ran, but possible in tests), the status should be "degraded" not "starting". "starting" means no data available.

**Why it happens:** Confusing "cache is empty" with "cache state unknown".

**How to avoid:** The status determination function must check `target_count > 0` first:
- `target_count > 0` → "healthy" (last OK) or "degraded" (last failed or unknown)
- `target_count == 0` → "starting" (with HTTP 503 if last failed)

### Pitfall 5: Route Registration Ordering

**What goes wrong:** Registering `/health` after `/health/live` and `/health/ready` may work but looks inconsistent; more importantly, forgetting to merge the health routes change into `create_routes()` means the new endpoints return 404.

**How to avoid:** `src/routes/health.rs` `routes()` function already returns a Router that is merged in `src/routes/mod.rs`. Add the two new routes to the existing `health::routes()` function — no change to `src/routes/mod.rs` is needed.

---

## Code Examples

### Full health_handler skeleton

```rust
// Source: derives from metrics_handler pattern in src/handlers/metrics.rs [VERIFIED: codebase]
pub async fn health_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthResponse>) {
    // 1. Read snapshot (one lock acquisition)
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
    };

    // 2. Read last refresh outcome (separate lock)
    let last_outcome = state.last_refresh_outcome.read().await.clone();

    // 3. Compute uptime
    let uptime_seconds = state.started_at.elapsed().as_secs();

    // 4. Read cluster info (matching metrics_handler lock pattern)
    let (cluster_nodes, is_leader) = match state.cluster.as_ref() {
        None => (1usize, true),
        Some(cluster) => {
            let chitchat = cluster.handle.chitchat();
            let cc = chitchat.lock().await;
            let count = cc.live_nodes().count();
            drop(cc);
            let leader = cluster.is_leader().await;
            (count, leader)
        }
    };

    // 5. Determine overall status and HTTP code
    let (status_str, http_status) = determine_health_status(target_count, &last_outcome);

    let response = HealthResponse {
        status: status_str,
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds,
        cache: CacheHealth {
            targets: target_count,
            age_seconds,
            state: if target_count > 0 { "populated" } else { "empty" },
        },
        cluster: ClusterHealth {
            mode: match state.config.cluster_mode {
                crate::config::ClusterMode::Standalone => "standalone",
                crate::config::ClusterMode::Cluster => "cluster",
            },
            nodes: cluster_nodes,
            is_leader,
        },
        last_refresh: LastRefreshHealth {
            status: match &last_outcome {
                None => "never",
                Some(o) if o.success => "ok",
                Some(_) => "failed",
            },
            timestamp: last_outcome.as_ref().map(|o| o.timestamp_unix),
        },
    };

    (http_status, Json(response))
}
```

### Example /health response (healthy)

```json
{
  "status": "healthy",
  "version": "0.5.0",
  "uptime_seconds": 3623,
  "cache": {
    "targets": 42,
    "age_seconds": 47,
    "state": "populated"
  },
  "cluster": {
    "mode": "standalone",
    "nodes": 1,
    "is_leader": true
  },
  "last_refresh": {
    "status": "ok",
    "timestamp": 1751890800
  }
}
```

### Example /health response (503 — empty cache, refresh failed)

```json
{
  "status": "starting",
  "version": "0.5.0",
  "uptime_seconds": 12,
  "cache": {
    "targets": 0,
    "age_seconds": 0,
    "state": "empty"
  },
  "cluster": {
    "mode": "standalone",
    "nodes": 1,
    "is_leader": true
  },
  "last_refresh": {
    "status": "failed",
    "timestamp": 1751890710
  }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single `/health` with static response | Split into `/health` (rich), `/health/live` (always-200), `/health/ready` (cache-gated) | Phase 11 | ALB/k8s can use `/health/live` for liveness without evicting during AWS outage |
| `serde_json::json!{}` macro for health | Typed `#[derive(Serialize)]` structs | Phase 11 | Compile-time field checking; required for Phase 14 utoipa schema generation |

**Deprecated/outdated:**
- The existing `health_handler` function signature (`async fn health_handler() -> Json<serde_json::Value>`) — must change to `State<AppState>` parameter

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `/health/live` handler does not need `State<AppState>` — registering a stateless handler on a state-ful axum Router is valid | Architecture Patterns, Pattern 8 | If wrong, must add `State<AppState>` parameter even though it's unused; trivial fix |
| A2 | `(StatusCode, Json<T>)` implements `IntoResponse` in axum 0.8 | Architecture Patterns, Pattern 5 | If wrong, must use `Response` return type like metrics_handler; trivial fix |
| A3 | `ClusterMode::Standalone` node count should be reported as `nodes: 1, is_leader: true` | Architecture Patterns | If wrong, could report `nodes: 0` which is misleading; clarify in plan |
| A4 | `last_refresh.timestamp` as Unix seconds (u64) is acceptable format | Architecture Patterns | If ISO 8601 string is required, must add formatting code (no new crates — format manually or use `time` crate) |

**Assumptions A1 and A2 are well-known axum 0.8 patterns [ASSUMED] — confirm with `cargo check` after implementation.**

---

## Open Questions (RESOLVED)

1. **Standalone node count: 1 or 0?**
   - What we know: `cluster: None` in AppState when standalone mode; chitchat is not running
   - What's unclear: Should `nodes: 1` (self counts) or `nodes: 0` (no cluster nodes) be reported?
   - RESOLVED: `nodes: 1, is_leader: true` — the local instance IS a node serving traffic; 0 would imply no service

2. **`last_refresh.timestamp` format**
   - What we know: Requirements say "timestamp" without specifying format
   - What's unclear: Unix seconds (u64) vs ISO 8601 string
   - RESOLVED: Unix seconds (u64) — no new deps, unambiguous, trivially parseable by callers

3. **`degraded` status when `last_refresh_outcome` is None but cache is populated**
   - What we know: This can only happen if initial discovery was never recorded but somehow cache is populated (e.g., follower sync). In practice, if a leader runs initial discovery and records the outcome, followers sync via gossip and their `last_refresh_outcome` stays None.
   - What's unclear: Should followers show `status: "degraded"` (no record) or `status: "healthy"` (cache is populated)?
   - RESOLVED: Report "degraded" — better to be conservative; followers can check their own /health/ready for readiness

---

## Environment Availability

> Skip: This phase has no external dependencies — pure code changes within the existing project.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`) |
| Config file | none — uses `#[cfg(test)]` modules inline |
| Quick run command | `cargo test health` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HEALTH-01 | `/health` JSON contains all required fields with correct types | unit | `cargo test health::tests::health_response_has_all_required_fields` | No — Wave 0 |
| HEALTH-02 | `/health` returns 503 when cache empty AND last refresh failed | unit | `cargo test health::tests::health_503_when_empty_and_refresh_failed` | No — Wave 0 |
| HEALTH-02 | `/health` returns 200 when cache empty but no failure on record | unit | `cargo test health::tests::health_200_when_empty_and_no_failure` | No — Wave 0 |
| HEALTH-02 | `/health` returns 200 when cache populated even if last refresh failed | unit | `cargo test health::tests::health_200_when_populated_even_if_refresh_failed` | No — Wave 0 |
| HEALTH-03 | `/health/live` always returns 200 | unit | `cargo test health::tests::health_live_returns_200` | No — Wave 0 |
| HEALTH-04 | `/health/ready` returns 503 when cache empty | unit | `cargo test health::tests::health_ready_503_when_empty` | No — Wave 0 |
| HEALTH-04 | `/health/ready` returns 200 when cache has targets | unit | `cargo test health::tests::health_ready_200_when_populated` | No — Wave 0 |

**Key testing strategy:** The `determine_health_status(target_count, last_outcome)` pure function is the primary test target. It takes no async dependencies and can be exercised with simple `assert_eq!` calls. Handler-level tests can call `health_live_handler()` directly (no state needed) and invoke `determine_health_status` for the logic tests without constructing AppState.

Full handler integration tests with AppState construction are scoped to Phase 15 (TEST-01) per the roadmap.

### Sampling Rate

- **Per task commit:** `cargo test health`
- **Per wave merge:** `cargo test`
- **Phase gate:** `cargo test` full suite green before `/gsd-verify-work`

### Wave 0 Gaps

All test functions listed above must be written alongside the new handler code (inline `#[cfg(test)]` module inside `src/handlers/health.rs`):

- [ ] `determine_health_status` unit tests (7 cases covering all branches of HEALTH-01/02/03/04)
- [ ] `health_live_handler` direct call test (no AppState needed — call the function directly)

*(Existing test infrastructure `cargo test` coverage: 161 tests passing — no new test files needed, tests live in the same file as the handler)*

---

## Security Domain

> `security_enforcement` not explicitly set to false — section included.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Health endpoints are unauthenticated by design (liveness/readiness probes require no auth) |
| V3 Session Management | No | Stateless endpoints |
| V4 Access Control | Low | Health endpoints expose operational state — no secrets, no AWS credentials, no refresh tokens |
| V5 Input Validation | No | `/health`, `/health/live`, `/health/ready` take no query parameters or request bodies |
| V6 Cryptography | No | No crypto operations |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Information disclosure via /health | Information Disclosure | Ensure `last_refresh.timestamp` is Unix seconds (not wall clock with timezone that reveals infra location); no AWS account IDs, ARNs, or cluster names in response |
| DoS via high-frequency health polling | Denial of Service | Health handler acquires read locks briefly — acceptable; no rate limiting needed for probe traffic |

**Info-sec note:** The `cluster` sub-object exposes `nodes` count and `is_leader` flag — this is operational metadata, not sensitive. `version` is already exposed in the current handler. No ARNs, account IDs, or environment variables are included in the response.

---

## Sources

### Primary (HIGH confidence)

- Codebase: `src/handlers/metrics.rs` — chitchat lock ordering pattern (lock → count → drop → is_leader)
- Codebase: `src/state/app_state.rs` — AppState field patterns, CacheSnapshot layout, RefreshOutcome placement rationale
- Codebase: `src/routes/health.rs` — existing route registration pattern
- Codebase: `src/main.rs` — background refresh loop, where to inject outcome recording
- Codebase: `Cargo.toml` — verified all needed crates already present

### Secondary (MEDIUM confidence)

- `.planning/STATE.md` v0.3.0 decisions — "last_manual_refresh_request Must NOT fold into CacheSnapshot" — same rationale applies to `last_refresh_outcome`
- `.planning/REQUIREMENTS.md` — HEALTH-01 through HEALTH-04 field specifications

### Tertiary (LOW confidence — verify with `cargo check`)

- axum 0.8 `(StatusCode, Json<T>)` implements `IntoResponse` [ASSUMED — well-known pattern]
- axum 0.8 stateless handler on state-ful Router is valid [ASSUMED — well-known pattern]

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new crates, all confirmed in Cargo.toml
- Architecture: HIGH — directly derived from existing handler patterns in the same codebase
- Pitfalls: HIGH — chitchat deadlock and UNIX_EPOCH sentinel documented from actual code inspection
- Test strategy: HIGH — pure function extraction is directly testable

**Research date:** 2026-07-07
**Valid until:** 2026-08-07 (stable Rust/axum stack; no fast-moving dependencies involved)
