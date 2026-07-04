# Architecture Patterns — v0.3.0 Operational Excellence

**Project:** ecs-sd
**Researched:** 2026-07-04
**Confidence:** HIGH (all findings verified against source code + Context7 docs)

---

## Questions Answered

This document addresses four integration architecture questions for v0.3.0:

1. Where should utoipa schema annotations live?
2. How does the health handler get AppState without changing the signature pattern?
3. What is the cleanest HTTP metrics middleware approach in axum 0.8?
4. How does CacheSnapshot replace three sequential locks, and what handler code changes?

---

## Q1: utoipa Annotation Placement

**Recommendation: `#[utoipa::path]` lives on handler functions; `#[derive(ToSchema)]` goes on model structs in `src/models/`.**

### Why not separate schema structs

Separate "documentation-only" schema structs duplicate the actual response shapes and diverge silently over time. The utoipa model is that annotations on handler functions reference structs that are already in the response path. That is both the documented pattern and the maintainable one.

### Concrete placement rules

| Annotation | Where |
|---|---|
| `#[utoipa::path(...)]` | On each `async fn` in `src/handlers/` |
| `#[derive(ToSchema)]` | On `Target`, `SdQueryParams`, new typed response structs |
| `#[derive(OpenApi)]` | Single `ApiDoc` struct in new `src/openapi.rs` |

### Handlers returning `serde_json::Value` need typed response structs

`health_handler`, `refresh_handler`, and `metrics_handler` currently return ad-hoc JSON or `serde_json::Value`. utoipa cannot derive schemas from these. The fix is a thin response struct per handler — not a "documentation-only" struct, but one that is the actual return type:

```rust
// src/handlers/health.rs
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub app: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub cache_age_seconds: u64,
    pub target_count: usize,
    pub mode: String,
    pub cluster_role: Option<&'static str>,
}
```

`sd_handler` already returns `Vec<Target>` — just add `#[derive(ToSchema)]` to `Target` in `src/models/target.rs`. The only addition is `utoipa` in the derive list alongside the existing `serde` derives.

`metrics_handler` returns plain text (`text/plain; version=0.0.4`). Annotate it with `content_type = "text/plain"` in the `#[utoipa::path]` response, no schema struct needed.

### The `ApiDoc` struct

Create `src/openapi.rs`:

```rust
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::health::health_handler,
        crate::handlers::health::liveness_handler,
        crate::handlers::health::readiness_handler,
        crate::handlers::sd::sd_handler,
        crate::handlers::sd::refresh_handler,
        crate::handlers::metrics::metrics_handler,
        crate::handlers::config::config_handler,
        crate::handlers::proxy::proxy_handler,
    ),
    components(schemas(
        crate::models::Target,
        crate::handlers::health::HealthResponse,
        crate::handlers::config::ConfigResponse,
    )),
    tags(
        (name = "discovery", description = "Prometheus service discovery"),
        (name = "operational", description = "Health, config, and metrics"),
    )
)]
pub struct ApiDoc;
```

The swagger-ui and `/openapi.json` routes attach to a separate sub-router that bypasses the main `AppState`-typed router:

```rust
// src/routes/openapi.rs
use utoipa_swagger_ui::SwaggerUi;
use crate::openapi::ApiDoc;

pub fn routes() -> Router {
    SwaggerUi::new("/swagger-ui")
        .url("/openapi.json", ApiDoc::openapi())
        .into()
}
```

This sub-router is merged into the app in `main.rs` _after_ `routes::create_routes()`. It does not take `AppState` as a type parameter because `SwaggerUi::into::<Router>()` produces a stateless router.

### New Cargo.toml additions

```toml
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "8", features = ["axum"] }
```

The `axum_extras` feature enables automatic `parameter_in` inference from `Path<...>` and `Query<...>` handler arguments, which avoids manually repeating parameter locations in the `params(...)` annotation.

---

## Q2: Health Handler — AppState Access

**Recommendation: Add `State(state): State<AppState>` as a parameter. No routing change needed.**

### Why this is a non-issue for the router

`routes/health.rs` registers `health::health_handler` on a `Router<AppState>`. Axum's router is parameterized by the state type and will inject it into any handler that declares `State<AppState>` as an extractor. Adding the extractor to the handler function is the only required change — the route registration line is untouched.

```rust
// Before
pub async fn health_handler() -> Json<serde_json::Value>

// After
pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse>
```

The compiler enforces that handler extractors are compatible with the router's state type. If the handler asks for something the router does not provide, the code does not compile. This is the mechanism that guarantees the change is safe.

### Test breakage is intentional and small

The existing test calls `health_handler().await` directly with no arguments. After the signature change it must construct a minimal `AppState`. This is a one-time pain that creates the integration test skeleton for the rich health endpoint, so it is worth doing.

### What data health needs and where it comes from

| Field | Source | Lock required |
|---|---|---|
| `status` | static string | none |
| `app`, `version` | `env!()` macros | none |
| `uptime_seconds` | `state.start_time` (new Instant field) | none |
| `cache_age_seconds` | `snapshot.read().last_refresh` | read on CacheSnapshot |
| `target_count` | `snapshot.read().cache[MetadataLevel::Aws].len()` | read on CacheSnapshot |
| `mode` | `state.config.mode` | none (Arc<Config>) |
| `cluster_role` | `state.cluster.as_ref()?.is_leader().await` | internal chitchat lock |

### `start_time` field addition to AppState

`AppState` gains one new field: `pub start_time: std::time::Instant`. It is set once in `AppState::new()` and never locked:

```rust
Ok(Self {
    snapshot: Arc::new(RwLock::new(CacheSnapshot::empty())),
    start_time: std::time::Instant::now(),
    // ... rest unchanged
})
```

### Kubernetes probe routes

`/health/live` and `/health/ready` are added in `routes/health.rs`. Both are trivially thin:

- `/health/live` — returns `200 OK` if the process is up. No state access. Static.
- `/health/ready` — returns `200 OK` if the cache has been populated at least once. Reads `snapshot.last_refresh != SystemTime::UNIX_EPOCH`.

Both get their own handler functions to keep utoipa path annotations separate and clean.

---

## Q3: HTTP Metrics Middleware in axum 0.8

**Recommendation: `axum::middleware::from_fn_with_state`. Zero per-handler changes.**

### Why not tower-http TraceLayer

`TraceLayer` is tracing-based. Its callbacks produce `tracing::Span` events, not Prometheus metric increments. You can technically add side effects in the callbacks, but it conflates tracing infrastructure with metrics infrastructure and creates an awkward closure capture pattern to get `Arc<MetricsState>` into the callbacks. Keep them separate.

### Why not a custom `tower::Layer`

A hand-written `tower::Layer` + `tower::Service` implementation is 40-60 lines of boilerplate for a use case that `from_fn_with_state` handles in 20 lines. Use the boilerplate only when you need multiple layers to compose or need to transform the body. Latency + counter recording does not need that.

### The middleware

New file: `src/middleware/http_metrics.rs`

```rust
use axum::{extract::{MatchedPath, State}, middleware::Next, response::Response};
use axum::http::Request;
use std::time::Instant;

use crate::state::AppState;

pub async fn track_http_metrics(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_string());

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();

    state.metrics.http_requests_total
        .with_label_values(&[&method, &path, &status])
        .inc();
    state.metrics.http_request_duration_seconds
        .with_label_values(&[&method, &path])
        .observe(elapsed);

    response
}
```

### MatchedPath is critical for proxy cardinality

Without `MatchedPath`, the path `/proxy/550e8400-e29b-41d4-a716-446655440000/metrics` becomes a unique label value per UUID, creating unbounded cardinality in Prometheus. `MatchedPath` normalizes this to `/proxy/:id/*path`, which is the axum route template string.

`MatchedPath` is populated by axum's router before middleware runs when the layer is attached via `.layer()` _before_ `.with_state()`. The registration order in `main.rs` is:

```rust
let app = Router::new()
    .merge(routes::create_routes())
    .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::http_metrics::track_http_metrics,
    ))
    .with_state(state.clone());
```

`.layer()` wraps the already-routed request pipeline, so `MatchedPath` is set before the middleware function runs.

### Exclusion of `/metrics` and `/swagger-ui`

The metrics endpoint and swagger-ui should not self-instrument (Prometheus already tracks scrape counts externally; swagger-ui adds noise). Exclude them in the middleware by path prefix:

```rust
if path.starts_with("/metrics") || path.starts_with("/swagger-ui") || path == "/openapi.json" {
    return next.run(req).await;
}
```

### New MetricsState fields

`src/metrics/mod.rs` gains two fields:

```rust
pub http_requests_total: CounterVec,       // labels: ["method", "path", "status"]
pub http_request_duration_seconds: HistogramVec,  // labels: ["method", "path"]
```

Histogram buckets for HTTP latency: `[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]` (sub-second focus; this is a discovery service, not a scraper).

---

## Q4: CacheSnapshot — Three Locks to One

**Recommendation: Introduce `CacheSnapshot` struct, replace three `Arc<RwLock<...>>` fields with one.**

### Current problem

`replace_cache_and_routing()` holds and releases three sequential write locks:
1. `cache.write()` — inserts all five MetadataLevel variants, then drops
2. `last_refresh.write()` — updates timestamp, then drops
3. `routing_table.write()` (proxy mode only) — rebuilds table, then drops

A reader acquires `cache.read()` between steps 1 and 2 will see new target data but old `last_refresh`. The `sd_handler` then calculates `cache_age_seconds` from the old time. This is a minor inconsistency (cache looks older than it is), but it also means `publish_cache_to_gossip` and the health endpoint can observe phantom staleness. More critically: if `replace_cache_and_routing` is called during a follower sync and a /sd request is in flight, the routing_table (step 3) can be read in a state inconsistent with the just-written cache (step 1).

### The fix

```rust
// src/state/cache_snapshot.rs  (new file)
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;
use crate::models::{MetadataLevel, ProxyTarget, Target};

pub struct CacheSnapshot {
    pub cache: HashMap<MetadataLevel, Vec<Target>>,
    pub last_refresh: SystemTime,
    pub routing_table: HashMap<Uuid, ProxyTarget>,
}

impl CacheSnapshot {
    pub fn empty() -> Self {
        Self {
            cache: HashMap::new(),
            last_refresh: SystemTime::UNIX_EPOCH,
            routing_table: HashMap::new(),
        }
    }
}
```

`AppState` replaces three fields with one:

```rust
// Remove:
pub cache: Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>,
pub last_refresh: Arc<RwLock<SystemTime>>,
pub routing_table: Arc<RwLock<HashMap<Uuid, ProxyTarget>>>,

// Add:
pub snapshot: Arc<RwLock<CacheSnapshot>>,
```

`replace_cache_and_routing()` becomes one lock:

```rust
pub async fn replace_cache_and_routing(&self, targets_aws: Vec<Target>) {
    // ... existing migration and level-building logic unchanged ...

    let mut snap = self.snapshot.write().await;
    snap.cache.insert(MetadataLevel::Aws, targets_aws.clone());
    snap.cache.insert(MetadataLevel::Cluster, targets_cluster);
    snap.cache.insert(MetadataLevel::Service, targets_service);
    snap.cache.insert(MetadataLevel::Task, targets_task);
    snap.cache.insert(MetadataLevel::Container, targets_container);
    snap.last_refresh = SystemTime::now();
    if self.config.mode == Mode::Proxy {
        snap.routing_table = build_routing_table(&targets_aws);
    }
    // lock drops here — all fields updated atomically
}
```

### Handler-by-handler impact

**`sd_handler` (handlers/sd.rs)**

Before:
```rust
let cache = state.cache.read().await;
let maybe_targets = cache.get(&level).cloned();
drop(cache);
let last_refresh = *state.last_refresh.read().await;
```

After:
```rust
let (targets, cache_age_seconds, cache_state) = {
    let snap = state.snapshot.read().await;
    let targets = snap.cache.get(&level).cloned().unwrap_or_default();
    let cache_age_seconds = calculate_cache_age_seconds(snap.last_refresh, SystemTime::now());
    let cache_state = if cache_age_seconds > state.cache_ttl_seconds { "stale" } else { "fresh" };
    (targets, cache_age_seconds, cache_state)
};
// filter targets outside the lock — same as today
```

In proxy mode, `state.routing_table.read().await` becomes `state.snapshot.read().await` with `snap.routing_table.iter()`.

**`metrics_handler` (handlers/metrics.rs)**

Before:
```rust
let last_refresh = *state.last_refresh.read().await;
```

After:
```rust
let last_refresh = state.snapshot.read().await.last_refresh;
```

**`publish_cache_to_gossip` (main.rs)**

Before: two separate read locks (`cache.read()`, then `routing_table.read()`).

After: one read lock gives both:
```rust
let snap = state.snapshot.read().await;
if let Some(targets) = snap.cache.get(&MetadataLevel::Aws) {
    // ... serialize targets to gossip
}
if state.config.mode == Mode::Proxy {
    // snap.routing_table is already available — no second lock
}
```

**`health_handler` (handlers/health.rs)**

After CacheSnapshot, the health handler reads cleanly:
```rust
let snap = state.snapshot.read().await;
let cache_age_seconds = calculate_cache_age_seconds(snap.last_refresh, SystemTime::now());
let target_count = snap.cache.get(&MetadataLevel::Aws).map(|v| v.len()).unwrap_or(0);
drop(snap);
```

### Churn protection fits here

The churn protection logic checks before committing:

```rust
pub async fn replace_cache_and_routing(&self, new_targets: Vec<Target>) {
    // ... build all level vectors ...

    let mut snap = self.snapshot.write().await;

    // Churn protection: refuse swap if count drops by more than threshold
    let old_count = snap.cache.get(&MetadataLevel::Aws).map(|v| v.len()).unwrap_or(0);
    let new_count = new_targets.len();
    if old_count > 0 && new_count == 0 {
        warn!("churn protection: refusing cache swap ({} → 0 targets)", old_count);
        return;
    }
    // threshold variant: if new_count < old_count * threshold, return stale

    snap.cache.insert(...);
    snap.last_refresh = SystemTime::now();
    // ...
}
```

Having a single lock makes this check-then-swap an atomic operation. With three sequential locks, the check (on `cache`) and the swap (also on `cache`) would need the same lock across the whole operation — which the current code does not do.

---

## Component Boundaries After v0.3.0

```
src/
├── main.rs             MODIFIED  — middleware registration, openapi routes, reqwest timeouts
├── openapi.rs          NEW       — ApiDoc struct, #[derive(OpenApi)]
├── state/
│   ├── app_state.rs    MODIFIED  — remove 3 locks, add snapshot + start_time
│   └── cache_snapshot.rs  NEW   — CacheSnapshot struct
├── models/
│   ├── metadata_level.rs  MODIFIED  — add filter_labels_by_level (moved from sd.rs)
│   └── target.rs       MODIFIED  — add #[derive(ToSchema)]
├── middleware/
│   ├── mod.rs          NEW
│   └── http_metrics.rs NEW       — from_fn_with_state middleware
├── handlers/
│   ├── health.rs       MODIFIED  — State extractor, HealthResponse struct, ToSchema
│   ├── sd.rs           MODIFIED  — remove filter_labels_by_level, read snapshot
│   ├── metrics.rs      MODIFIED  — read snapshot.last_refresh
│   ├── proxy.rs        MODIFIED  — read snapshot.routing_table, remove unwrap()
│   └── config.rs       NEW       — GET /config handler, ConfigResponse + ToSchema
├── routes/
│   ├── health.rs       MODIFIED  — add /health/live and /health/ready
│   ├── mod.rs          MODIFIED  — add config, openapi routes
│   └── config.rs       NEW
│   └── openapi.rs      NEW
└── metrics/
    └── mod.rs          MODIFIED  — http_requests_total, http_request_duration_seconds
```

### Files that do NOT change

- `src/aws/` — discovery logic changes are isolated to the `discover_all_clusters` return behavior, not structure
- `src/cluster/` — no changes
- `src/config.rs` — only read via `Arc<Config>`, no structural changes (churn threshold could be added here)
- `src/models/proxy_target.rs`, `label_builder.rs` — no changes
- `src/error.rs` — no changes

---

## Build Order

The dependency chain is hard in two places: CacheSnapshot must precede health enrichment and churn protection. Everything else is independent.

```
Step 1: filter_labels_by_level migration
   src/models/metadata_level.rs  ← add function
   src/handlers/sd.rs            ← remove function, update import
   src/state/app_state.rs        ← update import
   verify: cargo test passes

Step 2: CacheSnapshot + start_time  [BLOCKING for steps 4, 5]
   src/state/cache_snapshot.rs   ← new struct
   src/state/app_state.rs        ← swap 3 fields for 1, add start_time
   src/handlers/sd.rs            ← read from snapshot
   src/handlers/metrics.rs       ← read from snapshot
   src/main.rs                   ← publish_cache_to_gossip
   verify: cargo test passes

Step 3: reqwest timeouts + discover_all_clusters stale-on-error  [independent]
   src/state/app_state.rs        ← connect_timeout(5s), tcp_keepalive(10s)
   src/aws/discovery.rs          ← return stale on total failure
   verify: cargo test passes

Step 4: Rich health endpoint  [requires step 2]
   src/handlers/health.rs        ← State extractor, HealthResponse, ToSchema
   src/routes/health.rs          ← /health/live, /health/ready routes
   verify: cargo test passes

Step 5: MetricsState new fields + HTTP middleware  [requires step 2 for clean AppState]
   src/metrics/mod.rs            ← 2 new metric families
   src/middleware/http_metrics.rs← new file
   src/middleware/mod.rs         ← new file
   src/main.rs                   ← register layer
   verify: cargo test passes, /metrics shows new metrics

Step 6: GET /config endpoint  [independent, can run in parallel with step 5]
   src/handlers/config.rs        ← new handler, ConfigResponse
   src/routes/config.rs          ← new routes file
   src/routes/mod.rs             ← merge config routes
   verify: cargo test passes

Step 7: utoipa integration  [requires steps 4, 5, 6 complete — all handlers stable]
   Cargo.toml                    ← add utoipa + utoipa-swagger-ui
   src/models/target.rs          ← #[derive(ToSchema)]
   src/openapi.rs                ← new ApiDoc struct
   src/routes/openapi.rs         ← swagger-ui + /openapi.json routes
   All handlers                  ← #[utoipa::path] annotations
   src/main.rs                   ← merge openapi routes
   verify: GET /openapi.json returns valid spec

Step 8: Churn protection  [requires step 2]
   src/config.rs                 ← optional churn_threshold field
   src/state/app_state.rs        ← check-before-swap in replace_cache_and_routing
   verify: unit test for threshold behavior
```

**Parallel opportunities:**
- Step 3 and steps 4-6 can run in parallel tracks after step 2 completes.
- Step 6 can be done during step 5 without conflict.
- Step 8 can be done last without blocking anything.

---

## Data Flow Changes Summary

### Read path (HTTP request handling)

Before: up to three separate read locks acquired and released by different code paths.

After: one `state.snapshot.read().await` per handler, yielding a consistent view of `cache`, `last_refresh`, and `routing_table` together. Lock is dropped before any CPU-heavy work (filtering, serialization).

### Write path (cache refresh)

Before: `cache.write()` → drop → `last_refresh.write()` → drop → (proxy) `routing_table.write()` → drop.

After: `snapshot.write()` → update all fields atomically → drop.

Readers that previously could observe `(new_cache, old_last_refresh)` now always see `(new_cache, new_last_refresh)` or `(old_cache, old_last_refresh)`.

### Gossip publish path

Before: two separate locks, brief window where routing_table lock is held after cache lock is released.

After: one snapshot read gives both, no window.

### HTTP metrics path (new)

Every request passes through `track_http_metrics` middleware before reaching a handler. The middleware extracts `MatchedPath` (the route template, not the concrete URL), measures elapsed time, increments `http_requests_total{method, path, status}`, and observes `http_request_duration_seconds{method, path}`. No handler has visibility into this — it is entirely in the middleware.

---

## Concurrency Safety Notes

- `CacheSnapshot` under a single `Arc<RwLock<>>` provides serializable reads. Multiple concurrent readers share the read lock simultaneously. The write lock is held only during `replace_cache_and_routing()`, which is called from the background refresh loop (every N seconds) and the manual `/sd/refresh` handler (rate-limited). Lock contention is minimal.
- `start_time: Instant` is immutable after construction — no lock needed, no `Arc<RwLock<>>`.
- `Arc<Config>` is already immutable — no change.
- `Arc<MetricsState>` metrics are already thread-safe (prometheus crate counters/histograms are internally synchronized).
- The HTTP middleware does not take a write lock on anything in `AppState` — it only calls `with_label_values(...).inc()` and `.observe()` on prometheus metric types.

---

## Sources

- utoipa 5.5.0 docs: https://docs.rs/utoipa/5.5.0/utoipa/
- axum middleware docs (from_fn_with_state): https://docs.rs/axum/latest/axum/middleware/fn.from_fn_with_state.html
- tower-http TraceLayer: https://docs.rs/tower-http/latest/tower_http/trace/struct.TraceLayer.html
- Source: `/Users/piotrek/git/ecs-sd/src/` — full codebase read, confidence HIGH
