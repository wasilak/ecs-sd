# Technology Stack — v0.3.0 Operational Excellence

**Project:** ecs-sd
**Researched:** 2026-07-04
**Scope:** Additive changes only — existing axum 0.8 / prometheus 0.14 stack is not replaced

---

## New Crates Required

Only 3 new crates are needed, all for the OpenAPI/Swagger feature. Everything else
(HTTP metrics, health enrichment, config endpoint) is implemented with existing crates.

### OpenAPI / Swagger UI

| Crate | Version | Feature flags | Why |
|-------|---------|---------------|-----|
| `utoipa` | `"5"` (latest: 5.5.0) | `axum_extras` | Core macro crate; `axum_extras` simplifies `IntoParams` for axum path/query extractors |
| `utoipa-axum` | `"0.2"` (latest: 0.2.0) | none | Provides `OpenApiRouter` wrapper that collects path specs while building the router |
| `utoipa-swagger-ui` | `"9"` (latest: 9.0.2) | `axum` | Serves Swagger UI HTML and bundles the JS assets; `axum` feature gates the axum integration |

**Cargo.toml additions:**

```toml
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-axum = "0.2"
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

**Compatibility confirmed (HIGH confidence):**
- `utoipa-axum 0.2.0` declares `axum = { version = "0.8.4", default-features = false }` in its
  own Cargo.toml. The project's `axum = "0.8"` satisfies this constraint — Cargo resolves to
  the same compatible minor.
- `utoipa-swagger-ui 9.0.2` with feature `axum` declares `axum = { version = "0.8.4",
  default-features = false, features = ["json"] }`. Same resolution path.
- Both require `utoipa ^5.0.0`, consistent with adding `utoipa = "5"`.
- Source: GitHub juhaku/utoipa repo `utoipa-axum/Cargo.toml` and
  `utoipa-swagger-ui/Cargo.toml` (verified via raw file fetch, 2026-07-04).

---

## HTTP Metrics Middleware — No New Crate

**Decision: manual axum middleware, NOT `axum-prometheus`.**

`axum-prometheus 0.10.0` uses the `metrics` / `metrics-exporter-prometheus` crate ecosystem,
which is entirely separate from the `prometheus = "0.14"` crate this project already uses.
Adding it would result in two independent Prometheus registries — one for the 9 existing
operational metrics and one for HTTP request metrics. The `/metrics` endpoint would only
expose whichever registry it reads; the other silently disappears. This is a data loss bug,
not a minor inconvenience.

**The correct approach for this codebase:** write a custom `axum::middleware::from_fn` function
that instruments HTTP requests into `MetricsState` via the existing `prometheus::Registry`.

What the middleware needs to do:
1. Clone the `Arc<MetricsState>` from axum's `Extension` or pass via closure capture.
2. Record `Instant::now()` before calling `next.run(req)`.
3. Extract `MatchedPath` (axum provides this) for label-safe route patterns like `/sd`
   instead of raw URLs that would create unbounded cardinality.
4. After the response, observe elapsed time and increment counters into two new fields
   in `MetricsState`:
   - `http_requests_total: CounterVec` — labels: `[method, endpoint, status]`
   - `http_request_duration_seconds: HistogramVec` — labels: `[method, endpoint, status]`

No additional crates required. `axum::middleware`, `std::time::Instant`, and the existing
`prometheus` types (`CounterVec`, `HistogramVec`) cover everything.

**Paths to exclude from HTTP metrics** (to avoid noise and self-referential loops):
- `/metrics` itself
- `/health/live` and `/health/ready` (high-frequency k8s probes)

---

## Health Endpoint Enrichment — No New Crate

The enriched `/health` needs cache age, cluster role (leader/follower), and uptime. All
of these are computable from existing `AppState` fields:

| Field needed | Source |
|---|---|
| Cache age (seconds) | `SystemTime::now().duration_since(*state.last_refresh.read().await)` |
| Is cache stale | compare cache age against `state.cache_ttl_seconds` |
| Cluster role | `state.cluster.as_ref().map(|c| c.is_leader().await)` |
| Uptime | new field `startup_time: Arc<Instant>` in `AppState`, set once at `AppState::new()` |
| Target count | read `state.cache.read().await.get(&MetadataLevel::Aws).map(|v| v.len())` |

`/health/live` returns `200 OK` unconditionally (process is alive).
`/health/ready` returns `200 OK` when the cache has been populated at least once,
`503 Service Unavailable` otherwise.

No new crates needed.

---

## Config Endpoint — No New Crate

`serde` is already a dependency with `features = ["derive"]`. The `GET /config` handler
needs `Config` to be serializable.

Required changes (no new crates):
1. Add `#[derive(serde::Serialize)]` to `Config`, `Mode`, `ClusterMode`.
2. `MetadataLevel` already has `strum` — add `#[derive(serde::Serialize)]` or use
   `strum`'s `Display` + serialize as string via `#[serde(rename_all = "snake_case")]`.
3. Annotate `refresh_token: Option<String>` with `#[serde(skip)]` so it never appears
   in output — it is the only secret field in `Config`.
4. The handler is: `Json(&*state.config)` — trivial once Config is Serialize.

---

## Integration Pattern for utoipa-axum

The `OpenApiRouter` wrapper collects path specs at compile time via macros. The integration
point in `main.rs` / `routes/mod.rs` changes from `axum::Router::new().merge(...)` to
`OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(...))`. At the end,
`.split_for_parts()` returns the router and the collected OpenAPI document separately.
The document is then served at `/openapi.json` and wired into `SwaggerUi::new("/swagger-ui")`.

Handlers are annotated with `#[utoipa::path(...)]` above their `async fn`. Each route module
(health, sd, metrics, proxy) gets its handler annotated; the root `ApiDoc` struct lists them
all in `#[openapi(paths(...))]`.

**Caveat:** `utoipa-axum` currently bundles the swagger-ui JS/CSS assets at compile time
when using `utoipa-swagger-ui`. If the docker build layer is thin, the compiled binary will
be slightly larger (~2 MB for swagger assets). This is acceptable for an ops tool. The
vendored variant (`utoipa-swagger-ui-vendored`) embeds assets at compile time with no network
calls — use this for air-gapped builds. For normal builds, the standard `utoipa-swagger-ui`
pulls assets from the bundled copy.

---

## Alternatives Considered and Rejected

| Category | Rejected option | Reason |
|---|---|---|
| HTTP metrics | `axum-prometheus 0.10` | Uses `metrics-exporter-prometheus` — separate registry from `prometheus 0.14`. Would silently drop 9 existing metrics from `/metrics` output. |
| HTTP metrics | `prometheus-axum-middleware 0.4` | Lower adoption, less maintained. Custom middleware is simpler and uses fewer deps. |
| HTTP metrics | `tower-http` TraceLayer | Tracing-based, not prometheus-native. Bridging tracing spans to prometheus requires significant adapter code. |
| OpenAPI | `utoipa-rapidoc` / `utoipa-scalar` | Alternative UI renderers. SwaggerUI is the most widely recognised; no reason to deviate. |
| OpenAPI | Manual OpenAPI JSON | More work, no compile-time correctness, inconsistent with existing code-first patterns. |

---

## Final Cargo.toml Changes Summary

```diff
+utoipa = { version = "5", features = ["axum_extras"] }
+utoipa-axum = "0.2"
+utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

No existing crates change version or gain/lose features.
`serde` already has `features = ["derive"]` — no change needed.

---

## Sources

- utoipa-axum Cargo.toml (GitHub juhaku/utoipa, master): axum `^0.8.4` requirement confirmed
- utoipa-swagger-ui Cargo.toml (GitHub juhaku/utoipa, master): axum feature and version confirmed
- Context7 /juhaku/utoipa: SwaggerUi::new("/swagger-ui").url(...) integration pattern
- Context7 /ptrskay3/axum-prometheus: confirmed `metrics-exporter-prometheus` dependency (incompatible ecosystem)
- axum-prometheus Cargo.toml (GitHub API, v0.10.0): `metrics-exporter-prometheus = "0.18"`, no `prometheus` crate
- `cargo search` (2026-07-04): utoipa 5.5.0, utoipa-axum 0.2.0, utoipa-swagger-ui 9.0.2, axum-prometheus 0.10.0
