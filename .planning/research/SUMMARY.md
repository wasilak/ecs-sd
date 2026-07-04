# Project Research Summary

**Project:** ecs-sd
**Domain:** Rust/axum HTTP service — operational hardening of an existing production service
**Researched:** 2026-07-04
**Confidence:** HIGH

## Executive Summary

ecs-sd v0.3.0 adds operational observability, API documentation, and code-quality hardening to an existing production service. This is not a greenfield build — every change is additive or refactors existing code. The recommended approach follows well-documented Rust/axum patterns: custom Tower middleware for HTTP metrics (not `axum-prometheus`, which is ecosystem-incompatible), `utoipa` for OpenAPI annotation via `#[utoipa::path]` on handlers, and a `CacheSnapshot` struct to consolidate three sequential `RwLock` acquisitions into one atomic write. Only 3 new crates are required (`utoipa`, `utoipa-axum`, `utoipa-swagger-ui`), all for OpenAPI/Swagger; everything else uses existing dependencies.

The single most important design constraint is the registry conflict: `axum-prometheus 0.10` uses the `metrics-exporter-prometheus` ecosystem, which is entirely separate from the `prometheus 0.14` crate this service already uses. Adding it would silently drop all 9 existing operational metrics from `/metrics`. The correct approach is ~60 lines of custom `from_fn_with_state` middleware that writes into the existing `MetricsState`. The second critical risk is that `discover_all_clusters` currently returns an empty `Vec<Target>` on total AWS failure, silently wiping the cache — this must be fixed to return `Result` so the caller can preserve stale data.

The architectural refactor (CacheSnapshot) is the blocking dependency for health enrichment, churn protection, and any code that reads consistent cache state. It must land first. Everything else — config endpoint, HTTP metrics middleware, OpenAPI annotations — is independent and can proceed in parallel once the snapshot refactor is stable.

## Key Findings

### Recommended Stack

The existing axum 0.8 / prometheus 0.14 / serde / tokio stack requires exactly 3 new crates, all for OpenAPI. Everything else (HTTP metrics, health enrichment, config endpoint, churn protection) is implemented with what is already in `Cargo.toml`. Compatibility has been verified against upstream `Cargo.toml` files: `utoipa-axum 0.2.0` and `utoipa-swagger-ui 9.0.2` both declare `axum = "^0.8.4"`, resolving cleanly against the project's `axum = "0.8"`.

**New crates only (Cargo.toml additions):**
- `utoipa = { version = "5", features = ["axum_extras"] }` — OpenAPI macro derive with axum extractor inference
- `utoipa-axum = "0.2"` — optional; only needed if using `OpenApiRouter` (FEATURES.md recommends the simpler `merge(SwaggerUi::new(...))` pattern instead, making this crate unnecessary)
- `utoipa-swagger-ui = { version = "9", features = ["axum"] }` — Swagger UI HTML + bundled assets, axum router integration

**Explicitly rejected:**
- `axum-prometheus 0.10` — uses `metrics-exporter-prometheus`, incompatible registry, would silently hide all existing metrics

### Expected Features

**Must have (table stakes):**
- `/health/live` liveness probe — always 200, no state check, prevents restart loops
- `/health/ready` readiness probe — 503 when cache empty AND last refresh failed
- Rich `/health` JSON — status, uptime, cache age/count, cluster role, last error
- HTTP request metrics — `ecs_sd_http_requests_total` + `ecs_sd_http_request_duration_seconds` via custom middleware
- `GET /config` — sanitized effective config, `refresh_token_set: bool` not the value
- `/openapi.json` + `/swagger-ui` — machine-readable spec and visual explorer

**Should have (differentiators):**
- Target churn protection — configurable threshold (default 50%) blocks cache wipe on AWS glitch
- Per-cluster target gauge (`ecs_sd_cluster_targets{cluster}`) — enables cluster-level alerting
- AWS API call instrumentation (`ecs_sd_aws_api_calls_total{operation,result}`)
- Cache hit/miss metric (`ecs_sd_cache_requests_total{result}`) — meaningful in follower mode
- Startup duration gauge (`ecs_sd_startup_duration_seconds`) — cold-start visibility

**Explicitly defer (anti-features for v0.3.0):**
- Auth on `/config` — use network-level controls per PROJECT.md decision
- `degraded` as a non-standard HTTP status (207) — use JSON body field only
- `status_code` label on latency histogram — high cardinality, use only on counter
- `/health/startup` endpoint — reuse `/health/live` for k8s `startupProbe`

**Definitive new metric names (8 total):**

| Metric | Type | Labels |
|--------|------|--------|
| `ecs_sd_http_requests_total` | CounterVec | `endpoint`, `method`, `status_code` |
| `ecs_sd_http_request_duration_seconds` | HistogramVec | `endpoint`, `method` |
| `ecs_sd_cluster_targets` | GaugeVec | `cluster` |
| `ecs_sd_targets_added_total` | Counter | — |
| `ecs_sd_targets_removed_total` | Counter | — |
| `ecs_sd_aws_api_calls_total` | CounterVec | `operation`, `result` |
| `ecs_sd_cache_requests_total` | CounterVec | `result` |
| `ecs_sd_startup_duration_seconds` | Gauge | — |

### Architecture Approach

The central refactor is replacing three sequential `Arc<RwLock<...>>` fields (`cache`, `last_refresh`, `routing_table`) with a single `Arc<RwLock<CacheSnapshot>>`. This makes `replace_cache_and_routing()` atomic — readers either see the old snapshot or the new one, never a mix. It also makes churn protection trivially safe (check-then-swap under the same lock). The cost is a slightly wider write critical section, which is acceptable given ~60s refresh intervals and sub-millisecond write durations.

**Modified/new files:**

| File | Change |
|------|--------|
| `src/state/cache_snapshot.rs` | NEW — `CacheSnapshot` struct |
| `src/state/app_state.rs` | Remove 3 lock fields, add `snapshot` + `start_time` |
| `src/middleware/http_metrics.rs` | NEW — `from_fn_with_state` Tower middleware |
| `src/handlers/config.rs` | NEW — `GET /config`, `ConfigResponse` |
| `src/openapi.rs` | NEW — `ApiDoc` struct with all handler paths |
| `src/handlers/health.rs` | Add `State` extractor, `HealthResponse`, `/live`+`/ready` |
| `src/metrics/mod.rs` | Add 2 HTTP metric families |
| `src/models/target.rs` | Add `#[derive(ToSchema)]` |

Files that do NOT change: `src/aws/`, `src/cluster/`, `src/error.rs`, `src/models/proxy_target.rs`.

### Critical Pitfalls

1. **axum-prometheus registry conflict** — Do not add `axum-prometheus`. Write custom `from_fn_with_state` middleware (~60 lines) that writes into `MetricsState` via the existing `prometheus` registry. Detection: integration test asserting HTTP counters appear in `GET /metrics` output.

2. **`discover_all_clusters` silently wipes cache on total failure** — Current signature returns `Vec<Target>` and returns empty on all-cluster failure. Fix: return `Result<Vec<Target>, DiscoveryError>`; caller skips `replace_cache_and_routing` on `Err`. This is the stale-while-revalidate guarantee the design promises.

3. **ALB eviction loop during AWS outage** — If `/health` returns 503 for stale cache and ALB health check points at `/health`, all instances get evicted during the outage. Fix: point ALB health check at `/health/live` (always 200). Reserve `/health/ready` for k8s readiness probes only.

4. **utoipa generic schema name collision** — `ToSchema` derive ignores type parameters; `Foo<()>` and `Foo<i32>` both produce schema name `"Foo"`. Use concrete types in `responses(...)` annotations, not bare generics.

5. **`last_manual_refresh_request` must not fold into CacheSnapshot** — It is written from `refresh_handler` (rate-limiting, not cache consistency). Keep as a separate `AtomicU64`. Folding it into the snapshot would widen the write lock unnecessarily.

## Implications for Roadmap

### Phase 1: Module Cleanup + CacheSnapshot Refactor
**Rationale:** `CacheSnapshot` is a hard dependency for health enrichment (cache age), churn protection (check-then-swap), and any consistent read of cache state. Moving `filter_labels_by_level` out of handlers into models also fixes an inverted dependency (state layer importing from handler layer). Both changes are pure refactors — no new behavior, cargo test should pass throughout.
**Delivers:** Atomic cache updates, correct dependency layering, `start_time` field in AppState
**Addresses:** Pitfall 2 prerequisite, ARCHITECTURE Q4
**Avoids:** Phantom staleness in health endpoint, inconsistent cache/routing_table reads

### Phase 2: Stale-Cache Error Handling + reqwest Timeouts
**Rationale:** Independent of Phase 1, low risk, enables the stale-while-revalidate guarantee the design promises but currently doesn't deliver.
**Delivers:** `discover_all_clusters` returns `Result`; caller preserves stale cache on total failure; reqwest connect timeout (5s) + TCP keepalive (10s) configured
**Addresses:** Pitfall 2 (silent cache wipe on AWS outage)

### Phase 3: Rich Health Endpoint + k8s Probes
**Rationale:** Requires Phase 1 (needs `snapshot` + `start_time`). Adds two new AppState fields: `last_refresh_succeeded: Arc<AtomicBool>` and `last_refresh_error: Arc<RwLock<Option<String>>>`.
**Delivers:** `/health` (rich JSON, 503 on degraded), `/health/live` (always 200), `/health/ready` (503 on degraded)
**Addresses:** Pitfall 3 — document that ALB must point at `/health/live`, not `/health`

### Phase 4: HTTP Metrics Middleware + New Metric Families
**Rationale:** Requires Phase 1 (clean AppState). Custom `from_fn_with_state` middleware avoids registry conflict. Must use `MatchedPath` extractor for proxy route cardinality.
**Delivers:** All 8 new metrics: HTTP request counter/histogram, per-cluster gauge, churn counters, AWS API calls, cache hit/miss, startup duration
**Addresses:** Pitfall 1 (registry conflict — custom middleware, not axum-prometheus)

### Phase 5: GET /config Endpoint
**Rationale:** Fully independent, simplest feature. Can be developed in parallel with Phase 4.
**Delivers:** `GET /config` returning sanitized effective config, `refresh_token_set: bool`
**Addresses:** FEATURES config endpoint shape, anti-feature (no auth needed)

### Phase 6: Target Churn Protection
**Rationale:** Requires Phase 1 (check-then-swap atomic under single `CacheSnapshot` lock). `--churn-threshold` / `ECS_SD_CHURN_THRESHOLD` (default 0.5).
**Delivers:** Protection against AWS API glitch returning 0 tasks wiping cache

### Phase 7: OpenAPI / Swagger UI
**Rationale:** Last, because all handlers must be stable before annotating. Annotation pass is mechanical — no runtime behavior changes.
**Delivers:** `/openapi.json` (machine-readable spec), `/swagger-ui` (visual explorer)
**Addresses:** Pitfall 4 (generic schema collision — use concrete types), Pitfall 5 (axum feature flag)
**Stack:** `utoipa = "5"`, `utoipa-swagger-ui = { version = "9", features = ["axum"] }`

### Phase 8: Test Coverage
**Rationale:** Integration tests for all handlers and mocked AWS discovery tests. Ensures detection criteria from PITFALLS.md are exercised.
**Delivers:** HTTP handler integration tests, mock for `discover_all_clusters` total-failure scenario

### Phase Ordering Rationale

- Phase 1 is the only hard blocker — Phases 3, 4, 6 all depend on `CacheSnapshot`
- Phase 2 is independent and low-risk; can land before or after Phase 1
- Phase 5 can run in parallel with Phase 4 (different files, no conflicts)
- Phase 7 must come after Phases 3-6 (all handlers must be final before annotating)

### Research Flags

Phases with well-documented patterns (skip research-phase):
- **Phase 1:** Exact code specified in ARCHITECTURE.md
- **Phase 3:** Exact JSON shapes specified in FEATURES.md
- **Phase 4:** Exact middleware code specified in ARCHITECTURE.md
- **Phase 5:** Trivial — one handler, one struct
- **Phase 7:** `ApiDoc` struct fully specified in ARCHITECTURE.md

Phases that may benefit from targeted research during planning:
- **Phase 6:** Churn threshold behavior in cluster/follower mode needs thought
- **Phase 8:** axum test harness patterns for `State<AppState>` mock construction

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Crate versions verified via GitHub raw Cargo.toml files + cargo search |
| Features | HIGH | Official Prometheus naming + k8s probe semantics verified against official docs |
| Architecture | HIGH | Verified against actual source code in `src/` |
| Pitfalls | HIGH | Registry conflict confirmed via crates.io; ALB behavior from AWS docs |

**Overall confidence:** HIGH

### Gaps to Address

- **reqwest timeout values:** Chosen as reasonable defaults (5s connect, 10s keepalive); validate against observed AWS API latency in production
- **Churn threshold default (0.5):** Teams with volatile ECS clusters may need a lower default — document as tunable, validate in staging
- **`utoipa-axum` necessity:** STACK.md adds it; FEATURES.md recommends the simpler `merge(SwaggerUi::new(...))` pattern that does not require it. Resolve before Phase 7 — if `OpenApiRouter` is not used, drop it from Cargo.toml

## Sources

### Primary (HIGH confidence)
- GitHub juhaku/utoipa — `utoipa-axum/Cargo.toml`, `utoipa-swagger-ui/Cargo.toml` (axum version constraints verified)
- `prometheus.io/docs/practices/naming` — metric naming conventions
- `kubernetes.io/docs/concepts/workloads/pods/probes` — liveness/readiness semantics
- `docs.rs/axum` — `from_fn_with_state`, `MatchedPath` extractor
- `docs.rs/utoipa/5.5.0` — `ToSchema::name()` generic collision behavior
- `/Users/piotrek/git/ecs-sd/src/` — full codebase read (architecture findings)

### Secondary (MEDIUM confidence)
- AWS ALB health check eviction docs — 2-failure threshold, 30s interval behavior
- Community k8s probe guides (beefed.ai, fairwinds.com) — corroborating official semantics
- `axum-prometheus` GitHub — confirmed `metrics-exporter-prometheus` dependency (registry incompatibility)

---
*Research completed: 2026-07-04*
*Ready for roadmap: yes*
