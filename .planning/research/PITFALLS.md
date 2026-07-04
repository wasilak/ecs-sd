# Domain Pitfalls: ecs-sd v0.3.0 Operational Excellence

**Domain:** Adding operational features (OpenAPI, metrics middleware, health 503, lock refactor, module move, error handling) to an existing Rust/axum 0.8 service
**Researched:** 2026-07-04
**Confidence:** HIGH (most pitfalls verified against codebase + library docs; registry conflict confirmed via crates.io documentation)

---

## Critical Pitfalls

### Pitfall 1: axum-prometheus Registry Conflict with Existing prometheus Crate

**What goes wrong:** `axum-prometheus` is built on the `metrics` crate ecosystem (specifically `metrics-exporter-prometheus`). This project already uses the `prometheus` crate with a custom `MetricsState` struct and a scoped `Registry`. These are two independent global registries — one owned by the `prometheus` crate, one installed by `metrics-exporter-prometheus`. If you add `axum-prometheus` directly, HTTP middleware metrics end up in the `metrics` registry while application metrics stay in the `prometheus` registry. The existing `/metrics` handler renders only the `prometheus` registry, so HTTP metrics are silently absent.

**Why it happens:** Library authors made different choices. `axum-prometheus` uses `metrics-exporter-prometheus::PrometheusBuilder::install_recorder()` which installs a process-global recorder. The existing code uses `prometheus::Registry::new()` (a separate, isolated registry). There is no bridge between these two systems without an explicit adapter crate (`metrics-prometheus`).

**Consequences:** HTTP request metrics (count, duration, pending) don't appear in `/metrics`. No compiler warning. Tests pass. The bug is only visible at runtime by inspecting the output of `GET /metrics`.

**Prevention:** Do not add `axum-prometheus` as a dependency. Instead, implement the HTTP metrics as a custom Tower `Layer` that records into the existing `MetricsState` prometheus registry using the same `CounterVec` / `Histogram` types already in use. This is ~60 lines of code and keeps a single registry. Alternatively, use `prometheus-axum-middleware` crate, which explicitly targets the native `prometheus` crate registry — verify its axum 0.8 compatibility before adopting.

**Detection:** After adding any middleware metrics crate, run `cargo test` with an integration test that calls `GET /metrics` and asserts HTTP request counters are present in the response body.

---

### Pitfall 2: discover_all_clusters Total-Failure Silently Wipes Cache

**What goes wrong:** `discover_all_clusters` in `src/aws/discovery.rs` returns `Vec<Target>` (never returns an error). It logs per-cluster errors and continues, returning an empty Vec if all clusters fail. The background refresh loop in `main.rs` then calls `replace_cache_and_routing(empty_vec)`, which overwrites the existing cache with zero targets. Prometheus subsequently discovers nothing and stops scraping all targets.

**Why it happens:** The function signature hides the distinction between "discovered 0 targets" (valid) and "all AWS API calls failed" (error requiring cache preservation). The caller cannot distinguish these two cases.

**Consequences:** During an AWS API outage, ecs-sd correctly stays up but begins serving an empty target list. Prometheus stops scraping all ECS services within one refresh cycle. This is the exact scenario the stale-while-revalidate cache design was meant to prevent — but the current implementation silently defeats it.

**Prevention:** Change `discover_all_clusters` to return `Result<Vec<Target>, DiscoveryError>` and return `Err` when every cluster discovery call fails (partial success — some clusters failed, some succeeded — is still a `Vec<Target>` with partial results). In `refresh_cache_once`, treat `Err` as a soft failure: log it, increment the error counter, and skip calling `replace_cache_and_routing`. This preserves the stale cache.

**Detection:** Write a test where the mock AWS client returns errors for all clusters and assert that the cache is non-empty after a failed refresh.

---

### Pitfall 3: Health 503 Causes ALB to Evict All Instances During AWS Outages

**What goes wrong:** When `/health` returns 503 for degraded state (stale cache), the AWS ALB marks the target unhealthy after N consecutive failures (default: 2 failures with 30s interval = 60s to eviction). If all ecs-sd instances simultaneously have stale caches — e.g., during an AWS API outage — the ALB will mark all targets unhealthy and begin returning 503 to its clients (Prometheus). The background refresh loop continues running, but if ecs-sd is behind the ALB, Prometheus scraping fails completely.

**Why it happens:** ALBs don't distinguish between "service is crashing" and "service is operational but data is stale". Both return 503. ALB eviction logic is binary: expected status code or not.

**Secondary effect:** The very condition that causes cache staleness (AWS outage) is also the condition that triggers 503, which causes ALB eviction, which prevents recovery — a negative feedback loop.

**Consequences:** Complete loss of Prometheus service discovery during AWS API degradation, precisely when observability matters most.

**Prevention:**
- Map ALB health check to `/health/live` — a liveness probe that returns 200 if the process is running, regardless of cache state. This endpoint never returns 503.
- `/health/ready` can return 503 for stale cache — use this for Kubernetes readiness probes only (where pod replacement is the intended response).
- `/health` returns the rich JSON response with cache state information — for human dashboards and alerting, not for automated probes.
- Document which path serves which purpose explicitly.

**Detection:** Before deploying, verify the ALB target group health check is pointed at `/health/live`, not `/health` or `/health/ready`.

---

## Moderate Pitfalls

### Pitfall 4: utoipa Generic Type Name Collision in OpenAPI Spec

**What goes wrong:** `ToSchema` derive for generic types defaults to using only the outer type name. `Foo<()>` and `Foo<i32>` both produce the schema name `"Foo"`. In this codebase, if any handler uses `Json<Vec<Target>>` or similar generic wrappers as documented response bodies, only the last schema registered under that name survives in the OpenAPI output. The earlier schema is silently overwritten.

**Why it happens:** From the official docs: `assert_eq!(Foo::<()>::name(), Foo::<i32>::name())` — the `ToSchema::name()` default implementation ignores type parameters.

**Consequences:** Missing or wrong schema definitions in the generated OpenAPI spec. Clients that generate code from the spec get incorrect type information.

**Prevention:** For any type that is generic and used in handler annotations, override `ToSchema::name()` to include the type parameter, or use the `#[schema(as = MyFooInt)]` attribute to assign an explicit name in the derive macro. Keep response body types concrete in `#[utoipa::path]` annotations (use `Target` directly rather than wrapping in a generic).

---

### Pitfall 5: utoipa Swagger UI — Missing Axum Feature Flag

**What goes wrong:** `utoipa-swagger-ui` must be added with `features = ["axum"]` to serve the UI through an axum `Router`. Without this feature flag, the axum integration types (`SwaggerUi::into_router()` or the `Router::merge()` support) are not compiled in. The compile error message is cryptic — it mentions a missing method rather than a feature flag.

**Secondary issue:** `utoipa-swagger-ui` must be version-compatible with `utoipa`. A major version mismatch causes compilation errors or empty spec rendering.

**Prevention:** Add both dependencies explicitly:
```toml
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```
Check that the major versions are compatible before running `cargo build`.

---

### Pitfall 6: utoipa Path Macro Does Not Auto-Detect Non-200 Status Codes

**What goes wrong:** The `#[utoipa::path]` macro derives documentation from explicit `responses(...)` annotations, not from the handler's return type at compile time. If a handler conditionally returns `StatusCode::SERVICE_UNAVAILABLE` (503) for stale cache, that response variant is invisible in the spec unless added to the macro annotation. The compiler accepts the code, the handler works correctly at runtime, but the OpenAPI spec misleadingly claims the endpoint always returns 200.

**Prevention:** For every handler that has multiple status code branches, enumerate them all in the `responses(...)` block:
```rust
responses(
    (status = 200, description = "Healthy", body = HealthResponse),
    (status = 503, description = "Cache stale or service degraded", body = HealthResponse),
)
```
Treat the macro annotation as a contract that must stay in sync with the handler logic manually.

---

### Pitfall 7: CacheSnapshot Refactor Creates Larger Critical Section

**What goes wrong:** The current `replace_cache_and_routing` acquires three locks in sequence: `cache`, then `last_refresh`, then `routing_table` (in proxy mode). These are separate lock acquisitions, meaning readers of `last_refresh` are only blocked when the write to `last_refresh` is in progress, not during the longer `cache` write.

When consolidated into a single `CacheSnapshot` struct under one `RwLock`, the entire update — including the routing table rebuild — is serialized under one write guard. The write takes longer, and all readers (health handler, sd handler, gossip publisher) are blocked for the full duration.

**Why it happens:** Compound locks trade atomicity for throughput. The write phase takes longer but the result is always consistent.

**Consequences:** Increased tail latency on `/sd` and `/health` requests during the 60-second background refresh. Under normal conditions this is invisible (~few hundred microseconds). Under a slow AWS API (which makes `discover_all_clusters` take seconds), the write guard is not held during discovery — only during `replace_cache_and_routing` — so the actual critical section remains short.

**Prevention:** Verify the compound lock only wraps the immutable snapshot data: `cache` content, `last_refresh` timestamp, and routing table. Do not fold `last_manual_refresh_request` into the snapshot — it is written from a separate handler (`refresh_handler`) and its only purpose is rate-limiting, which does not require cache-level consistency. Keep it as a separate `Arc<RwLock<SystemTime>>` or convert to `AtomicU64` storing a Unix timestamp.

---

### Pitfall 8: Moving filter_labels_by_level — Import Chain Must Invert Cleanly

**What goes wrong:** `src/state/app_state.rs` currently imports `crate::handlers::sd::filter_labels_by_level`. This is an inverted dependency: the state layer imports from the handler layer. The fix is to move the function to `models/`. The risk during the move: if `models/` imports anything from `handlers/` or `state/`, the circular dependency reasserts itself in the new location.

`filter_labels_by_level` currently uses only `Target`, `MetadataLevel`, and `HashMap` — all pure model types. The function body is safe to move.

**Secondary risk:** The function is declared `pub(crate)` in `handlers::sd`. After the move, any test in `handlers/sd.rs` that calls `filter_labels_by_level` by its short name (unqualified, within the same module) will need the import path updated to `crate::models::filter_labels_by_level`. Failing to update test imports produces a compile error, not a logic bug — it will be caught immediately.

**Prevention:**
- Move to `src/models/target.rs` or a new `src/models/label_filter.rs` — not to `src/models/mod.rs` (keep mod.rs as a re-export hub).
- Keep visibility as `pub(crate)` — no need to widen scope.
- Update `src/models/mod.rs` to re-export: `pub(crate) use label_filter::filter_labels_by_level;`
- Remove the `use crate::handlers::sd::filter_labels_by_level;` import from `app_state.rs`.
- Update `handlers/sd.rs` import to `use crate::models::filter_labels_by_level;`.
- Run `cargo test` immediately after — any missed import shows up as a compile error.

---

## Minor Pitfalls

### Pitfall 9: tokio Write-Preferring RwLock — Starvation Under Compound Lock

**What goes wrong:** tokio's `RwLock` is write-preferring: when a writer is waiting, new read acquisitions queue behind it. With a compound lock, the single pending background refresh write blocks all concurrent reads (health, sd, gossip) until the write completes. With separate locks, only the specific lock being written was blocked.

**Prevention:** Under normal operations (refresh every 60s, write lasting <1ms), this is not observable. Starvation would require a continuous stream of concurrent writes, which doesn't exist in this architecture. No special mitigation needed, but worth noting if lock contention metrics are added later.

---

### Pitfall 10: Swagger UI Served on Same Port Competes with /metrics Path

**What goes wrong:** The `/swagger-ui` and `/openapi.json` routes are added to the main router. If `metrics_port` is configured (separate port for metrics), the OpenAPI spec won't be accessible from the metrics port. This is probably correct, but documentation should make it explicit to avoid confusion for operators who expect all operational endpoints on one port.

**Prevention:** Document that `/swagger-ui` is only available on the main listener port. If needed in future, add the routes to both routers explicitly.

---

### Pitfall 11: IntoResponse Return Type Mismatch After Adding AppState to Health Handler

**What goes wrong:** The current `health_handler` has no state parameter and returns `Json<serde_json::Value>` directly. Adding `State(state): State<AppState>` to support rich health response (cache age, uptime) is straightforward, but the return type must change to `impl IntoResponse` or `Response` to support the 503 branch. If the return type stays as `Json<serde_json::Value>`, the compiler rejects the `(StatusCode, Json<...>).into_response()` branch.

**Prevention:** Change the return type to `Response` (from `axum::response::Response`) or `impl IntoResponse` at the same time as adding the state parameter. This is a one-line change that the compiler will enforce.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| HTTP middleware metrics | Registry conflict (axum-prometheus vs prometheus crate) | Build custom Tower layer; do not add axum-prometheus |
| Health 503 | ALB eviction loop during AWS outage | Split /health/live from /health/ready before wiring 503 |
| Health 503 | Missing 503 in OpenAPI spec | Add `status = 503` to utoipa path macro annotation |
| OpenAPI integration | Generic schema name collision | Use concrete types in responses, avoid bare generic wrappers |
| OpenAPI swagger-ui | Missing axum feature flag | Add `features = ["axum"]` to utoipa-swagger-ui in Cargo.toml |
| CacheSnapshot refactor | last_manual_refresh_request folded in | Keep it as separate AtomicU64, not inside snapshot |
| CacheSnapshot refactor | Write critical section widening | Measure before/after; keep routing table rebuild outside the lock if possible |
| Module move (filter_labels_by_level) | Stale import path in tests | Run cargo test immediately after move to surface compile errors |
| Stale cache on total failure | discover_all_clusters returns empty Vec silently | Return Result; skip replace_cache_and_routing on total error |

## Sources

- `prometheus-axum-middleware` registry architecture distinction: https://crates.io/crates/prometheus-axum-middleware
- axum-prometheus documentation: https://github.com/Ptrskay3/axum-prometheus
- utoipa `ToSchema::name()` default behavior for generics: https://docs.rs/utoipa/5.5.0/utoipa/trait.ToSchema.html
- utoipa axum integration guide: https://docs.rs/utoipa/5.5.0/utoipa/index.html
- tokio RwLock write-preferring semantics: https://docs.rs/tokio/latest/tokio/sync/struct.RwLock.html
- AWS ALB health check eviction: https://docs.aws.amazon.com/elasticloadbalancing/latest/application/target-group-health-checks.html
- Rust visibility and circular dependency guidance: https://users.rust-lang.org/t/how-to-resolve-cyclic-dependency/51387
