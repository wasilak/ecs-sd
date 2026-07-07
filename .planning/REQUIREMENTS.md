# Requirements: ecs-sd v0.3.0 Operational Excellence

**Milestone:** v0.3.0
**Status:** Active

---

## v0.3.0 Requirements

### Code Quality — Concurrency & State

- [x] **QUAL-01**: The cache, routing table, and last_refresh timestamp are wrapped in a single `Arc<RwLock<CacheSnapshot>>` so cache replacement is one atomic write with no intermediate inconsistent state visible to concurrent handlers
- [x] **QUAL-02**: When all clusters fail during a discovery refresh, the existing stale cache is preserved and an error is logged — the cache is never replaced with an empty target list due to transient AWS API failures
- [x] **QUAL-03**: `Response::builder()` calls in `proxy_handler` and `metrics_handler` are handled with proper error propagation (no `unwrap()`) — failed response construction returns a 500 to the caller instead of panicking the Tokio task
- [x] **QUAL-04**: The `reqwest::Client` in `AppState` is configured with `connect_timeout(5s)` and `tcp_keepalive(10s)` to prevent TCP black-hole connections from exhausting file descriptors under concurrent scrapes

### Code Quality — Architecture & Dependencies

- [x] **QUAL-05**: `filter_labels_by_level` lives in `src/models/` (not `src/handlers/sd.rs`) so the state layer does not import from the handler layer
- [x] **QUAL-06**: `migrate_target_label_schema` is removed from the cache refresh hot path; `src/aws/discovery.rs` emits canonical label keys (`__meta_ecs_cluster_name`, `__meta_ecs_service_name`) directly
- [x] **QUAL-07**: `aws-sdk-ec2` in `Cargo.toml` is pinned to an exact patch version matching the `aws-sdk-ecs` release series (e.g. `1.124.x`) so `cargo update` cannot silently pull a mismatched EC2 SDK
- [x] **QUAL-08**: If no AWS region can be resolved at startup (no env var, no instance metadata), the service exits with a clear error message instead of silently defaulting to `us-east-1`

### Health Endpoint

- [ ] **HEALTH-01**: `GET /health` returns a JSON body with: `status` (`healthy`/`degraded`/`starting`), `version`, `uptime_seconds`, cache sub-object (`targets`, `age_seconds`, `state`), cluster sub-object (`mode`, `nodes`, `is_leader`), and `last_refresh` sub-object (`status`, `timestamp`)
- [ ] **HEALTH-02**: `GET /health` returns HTTP 503 when cache state is `empty` AND the last refresh failed — indicating the service cannot serve useful discovery data
- [ ] **HEALTH-03**: `GET /health/live` always returns HTTP 200 with `{"status":"alive"}` — never checks cache or AWS state, safe for liveness probes and ALB health checks
- [ ] **HEALTH-04**: `GET /health/ready` returns HTTP 200 when the cache contains at least one target, HTTP 503 otherwise — safe for readiness probes that gate traffic

### Operational Metrics

- [ ] **MET-08**: `ecs_sd_http_requests_total{endpoint, method, status}` counter tracks request count per route (using matched path pattern, not raw URI, to avoid UUID cardinality explosion)
- [ ] **MET-09**: `ecs_sd_http_request_duration_seconds{endpoint, method}` histogram tracks request latency per route — status code label omitted from histogram to control cardinality
- [ ] **MET-10**: `ecs_sd_discovery_targets_per_cluster{cluster}` gauge tracks target count per ECS cluster name after each successful refresh
- [ ] **MET-11**: `ecs_sd_discovery_target_churn_total{change}` counter (labels: `added`, `removed`) increments after each refresh where the target set changes
- [ ] **MET-12**: `ecs_sd_aws_api_calls_total{operation}` counter tracks calls per AWS SDK operation (`list_tasks`, `describe_tasks`, `describe_task_definition`, `describe_container_instances`, `describe_instances`, `get_caller_identity`)
- [ ] **MET-13**: `ecs_sd_cache_follower_syncs_total{result}` counter (labels: `success`, `error`, `skipped_leader`) tracks follower cache sync outcomes in cluster mode
- [ ] **MET-14**: `ecs_sd_startup_duration_seconds` gauge records time from process start to first successful cache population

### OpenAPI / Swagger

- [ ] **API-01**: All public HTTP endpoints (`/sd`, `/sd/refresh`, `/health`, `/health/live`, `/health/ready`, `/metrics`, `/config`, `/proxy/{id}/metrics`) have `#[utoipa::path]` annotations with documented query parameters, request/response schemas, and HTTP status codes
- [ ] **API-02**: All response and request model structs (`Target`, `ProxyTarget`, `SdQueryParams`, `MetadataLevel`, health response, config response) have `#[derive(utoipa::ToSchema)]`
- [ ] **API-03**: `GET /openapi.json` returns the machine-readable OpenAPI 3.0 specification
- [ ] **API-04**: `GET /swagger-ui` serves the Swagger UI browser interface pointing at `/openapi.json`

### Config Endpoint

- [ ] **CONF-07**: `GET /config` returns the effective runtime configuration as JSON — all fields from `Config` are included; `refresh_token` is replaced with `refresh_token_set: bool` (presence indicator only, value never exposed)

### Target Churn Protection

- [ ] **CHURN-01**: A configurable `ECS_SD_MAX_TARGET_DROP_RATIO` (float 0.0–1.0, default `0.0` = disabled) controls the maximum fraction of targets that may be removed in a single refresh cycle; when the drop exceeds the threshold the new result is discarded, the stale cache is kept, and a warning is logged — the threshold check is skipped when the previous target count was zero (initial population)

### Test Coverage

- [ ] **TEST-01**: HTTP handler integration tests use axum's `TestClient` (or `tower::ServiceExt`) to exercise full request/response cycles for `/health`, `/health/live`, `/health/ready`, `/sd` (with filter params), and `/config` — no mocking of AppState, real state constructed in test setup
- [ ] **TEST-02**: `discover_all_clusters` has tests covering the total-failure path (all clusters return errors) and the partial-failure path (some clusters succeed) using hand-constructed AWS SDK builder types — verifying that stale-cache-on-total-failure (QUAL-02) behaves correctly

---

## Future Requirements (v0.4.0+)

- Multi-region support (`cluster@region` syntax in `ECS_SD_CLUSTERS`)
- Active target health checking (probe metrics endpoints before including in SD output)
- TLS/HTTPS support for the HTTP API
- `SIGHUP` hot-reload of cluster list without restart
- Structured OpenTelemetry spans on discovery and proxy paths
- Split `src/handlers/sd.rs` (1132 lines) and `src/aws/discovery.rs` (968 lines) into sub-modules

---

## Out of Scope

| Feature | Reason |
|---------|--------|
| `axum-prometheus` crate | Uses different Prometheus registry ecosystem — incompatible with existing `prometheus 0.14` |
| Kubernetes support | ECS only by design |
| Write operations to AWS | Read-only is a feature |
| Metrics scraping | This is discovery, not scraping |
| TLS termination | Run behind reverse proxy |
| Authentication/authorization | Use network-level controls |

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| QUAL-01 | Phase 9 | Complete |
| QUAL-02 | Phase 9 | Complete |
| QUAL-05 | Phase 9 | Complete |
| QUAL-06 | Phase 9 | Complete |
| QUAL-03 | Phase 10 | Complete |
| QUAL-04 | Phase 10 | Complete |
| QUAL-07 | Phase 10 | Complete |
| QUAL-08 | Phase 10 | Complete |
| HEALTH-01 | Phase 11 | Pending |
| HEALTH-02 | Phase 11 | Pending |
| HEALTH-03 | Phase 11 | Pending |
| HEALTH-04 | Phase 11 | Pending |
| MET-08 | Phase 12 | Pending |
| MET-09 | Phase 12 | Pending |
| MET-10 | Phase 12 | Pending |
| MET-11 | Phase 12 | Pending |
| MET-12 | Phase 12 | Pending |
| MET-13 | Phase 12 | Pending |
| MET-14 | Phase 12 | Pending |
| CONF-07 | Phase 13 | Pending |
| CHURN-01 | Phase 13 | Pending |
| API-01 | Phase 14 | Pending |
| API-02 | Phase 14 | Pending |
| API-03 | Phase 14 | Pending |
| API-04 | Phase 14 | Pending |
| TEST-01 | Phase 15 | Pending |
| TEST-02 | Phase 15 | Pending |

---

*Last updated: 2026-07-04 — v0.3.0 roadmap created, traceability populated*
