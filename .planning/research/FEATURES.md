# Feature Landscape: v0.3.0 Operational Excellence

**Domain:** Prometheus HTTP service discovery daemon (Rust/axum, cache-backed, ECS)
**Researched:** 2026-07-04
**Scope:** Five features added to an existing production service — not greenfield

---

## Table Stakes

Features that operators expect. Missing any of these makes the service feel unfinished for a
production k8s/ECS deployment.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `/health/live` liveness probe | Every k8s Deployment expects it; ALB health checks need it | Low | Must always 200 while process is alive — never check cache or AWS |
| `/health/ready` readiness probe | k8s uses this to gate traffic; ELB Target Group needs 200 to route | Low-Med | 503 when cache is empty AND last refresh failed |
| Rich `/health` JSON | Ops/dashboards need operational state at a glance | Medium | Status, uptime, cache state, cluster role; 200 healthy / 200 degraded / 503 |
| HTTP request metrics | Every Prometheus-instrumented service exports request counts + latency | Medium | Tower middleware is cleanest integration point |
| OpenAPI spec at `/openapi.json` | Self-describing APIs are expected by any team deploying a new service | Low-Med | Machine-readable; consumed by clients, CI linting, API gateways |
| Swagger UI at `/swagger-ui` | Visual API explorer shipped with the binary is standard for internal tools | Low | utoipa-swagger-ui handles serving; just wire to Router |

---

## Differentiators

Features that go beyond baseline expectations and add operational value.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Per-cluster target count metric | Lets teams alert on "cluster X lost all targets" without joining scrape config | Low | GaugeVec with `cluster` label |
| Target churn protection | Prevents Prometheus losing all targets during transient AWS API failures | Medium | Configurable percentage threshold; default 50%; skip cache replacement if exceeded |
| AWS API call instrumentation | Pinpoints throttling and quota exhaustion before it causes discovery failures | Medium | CounterVec per operation; surfaces in Prometheus before errors cascade |
| Cache hit/miss metric for followers | In cluster mode, quantifies how often followers serve stale vs fresh data | Low | CounterVec `result=hit|miss`; meaningful only in cluster mode |
| Startup duration metric | Captures cold-start time for capacity planning and incident triage | Low | Gauge set once after first successful cache population |
| `GET /config` endpoint | Eliminates "what config is this instance running with?" support questions | Low | Return sanitized effective config JSON; mask `refresh_token` |

---

## Anti-Features

Features to explicitly NOT build in v0.3.0.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Authentication on `/config` | Scope creep; PROJECT.md decision: use network-level controls | Firewall `/config` at the load balancer or security group level |
| Spring Boot Actuator-style `/health` sub-endpoints | Sprawl; operators end up confused about which URL to use | Three endpoints only: `/health`, `/health/live`, `/health/ready` |
| `degraded` as a third HTTP status code (e.g., 207) | Non-standard; breaks k8s probe expectations (probes expect 2xx=ok / non-2xx=fail) | Use `status` field in JSON body; HTTP code is only 200 vs 503 |
| Per-request label on latency histogram with `status_code` | High-cardinality on histogram is expensive; status codes fragment the distribution | Use `status_code` on the counter; use only `endpoint`+`method` on the histogram |
| Separate metric per AWS operation type (one metric per API call) | Rigid; adding new AWS calls requires new metrics | Single CounterVec `ecs_sd_aws_api_calls_total{operation, result}` |
| `/health/startup` probe endpoint | Startup probe in k8s can reuse `/health/live`; a separate endpoint adds confusion | Point `startupProbe` at `/health/live` in k8s spec |

---

## Prometheus Metric Naming Conventions

Source: [prometheus.io/docs/practices/naming](https://prometheus.io/docs/practices/naming/)

All new metrics must follow these rules (consistent with the 9 existing `ecs_sd_*` metrics):

**Format:** `{namespace}_{subsystem}_{name}_{unit}` in snake_case

| Rule | Applied Example |
|------|-----------------|
| Application prefix (`ecs_sd_`) on all metrics | `ecs_sd_http_requests_total` |
| Counters end in `_total` | `ecs_sd_http_requests_total`, `ecs_sd_aws_api_calls_total` |
| Duration histograms end in `_seconds` | `ecs_sd_http_request_duration_seconds` |
| Base units only (seconds, bytes, not ms/KB) | `ecs_sd_startup_duration_seconds` not `_ms` |
| Labels in snake_case | `status_code`, not `statusCode` |
| Do not include label names in the metric name | `ecs_sd_cluster_targets` with `cluster=prod`, not `ecs_sd_prod_targets` |
| Gauges use no suffix or a meaningful noun | `ecs_sd_cluster_targets`, `ecs_sd_startup_duration_seconds` |

**High-cardinality label warning:** `endpoint` labels must use normalized route patterns
(`/sd`, `/health`, `/proxy/{uuid}/metrics`) not raw URLs. Never include UUIDs or user IDs in label
values.

---

## Definitive Metric Names for v0.3.0

The 7 new metrics, with full names and label sets:

| # | Name | Type | Labels | Notes |
|---|------|------|--------|-------|
| 1 | `ecs_sd_http_requests_total` | CounterVec | `endpoint`, `method`, `status_code` | Tower middleware; normalized path template not raw URL |
| 2 | `ecs_sd_http_request_duration_seconds` | HistogramVec | `endpoint`, `method` | No `status_code` on histogram (see anti-features); buckets: exponential 0.001s×2, 15 buckets |
| 3 | `ecs_sd_cluster_targets` | GaugeVec | `cluster` | Per ECS cluster name; set after each successful refresh |
| 4 | `ecs_sd_targets_added_total` | Counter | — | Targets that appeared in latest refresh vs previous |
| 5 | `ecs_sd_targets_removed_total` | Counter | — | Targets that disappeared in latest refresh vs previous |
| 6 | `ecs_sd_aws_api_calls_total` | CounterVec | `operation`, `result` | `operation`: ListTasks, DescribeTasks, DescribeContainerInstances, DescribeInstances, GetCallerIdentity; `result`: success\|error |
| 7 | `ecs_sd_cache_requests_total` | CounterVec | `result` | `result`: hit\|miss; meaningful in cluster follower mode |
| 8 | `ecs_sd_startup_duration_seconds` | Gauge | — | Set once after first successful cache population; never updated again |

Note: `ecs_sd_startup_duration_seconds` is item 8 but counts as the 7th NEW metric because it
replaces the need for a separate startup-time counter. The churn protection feature feeds
`ecs_sd_targets_added_total` and `ecs_sd_targets_removed_total` (items 4 and 5).

---

## Health Endpoint Shapes

### `/health` — Rich operational status

**HTTP 200** when status is `healthy` or `degraded` (informational).
**HTTP 503** when status is `degraded` — only when `cache.targets_count == 0 AND cache.last_refresh_ok == false`.

```json
{
  "status": "healthy",
  "app": "ecs-sd",
  "version": "0.5.0",
  "uptime_seconds": 3612,
  "cluster_role": "leader",
  "cache": {
    "age_seconds": 45,
    "targets_count": 42,
    "last_refresh_ok": true,
    "last_refresh_error": null
  }
}
```

**Degraded example (HTTP 503):**
```json
{
  "status": "degraded",
  "app": "ecs-sd",
  "version": "0.5.0",
  "uptime_seconds": 600,
  "cluster_role": "standalone",
  "cache": {
    "age_seconds": 600,
    "targets_count": 0,
    "last_refresh_ok": false,
    "last_refresh_error": "RequestTimeout: AWS API call timed out"
  }
}
```

**Field definitions:**

| Field | Type | Source |
|-------|------|--------|
| `status` | `"healthy"` \| `"degraded"` | Derived: healthy = targets > 0 OR last_refresh_ok; degraded = targets == 0 AND !last_refresh_ok |
| `app` | string | `env!("CARGO_PKG_NAME")` |
| `version` | string | `env!("CARGO_PKG_VERSION")` |
| `uptime_seconds` | u64 | `SystemTime::now() - startup_time` stored in AppState at init |
| `cluster_role` | `"leader"` \| `"follower"` \| `"standalone"` | From `ClusterState` if cluster mode enabled; otherwise `"standalone"` |
| `cache.age_seconds` | f64 | `SystemTime::now() - last_refresh` |
| `cache.targets_count` | usize | `cache.get(config.metadata_level).map(len).unwrap_or(0)` |
| `cache.last_refresh_ok` | bool | New field needed in AppState: `last_refresh_succeeded: Arc<AtomicBool>` |
| `cache.last_refresh_error` | string \| null | New field: `last_refresh_error: Arc<RwLock<Option<String>>>` |

### `/health/live` — Liveness probe

**HTTP 200 always** (as long as the process responds). No cache check, no AWS check.
Kubernetes semantics: failure → restart. Cache being empty is not a reason to restart.

```json
{"status": "alive"}
```

### `/health/ready` — Readiness probe

**HTTP 200** when the service can serve meaningful discovery data.
**HTTP 503** when `targets_count == 0 AND last_refresh_ok == false` (same condition as `/health` degraded).

Kubernetes semantics: failure → removes pod from Service endpoints (no restart). This is safe to
fail on AWS outages.

```json
{"status": "ready"}
```
or on 503:
```json
{"status": "not_ready", "reason": "cache empty and last refresh failed"}
```

**Critical design choice:** liveness and readiness use DIFFERENT conditions.
- Liveness: always 200 — prevents restart loops during AWS outages
- Readiness: 503 when degraded — prevents bad traffic routing

---

## `GET /config` Response Shape

Return the effective runtime configuration after all defaults are applied. Derive `serde::Serialize`
on a sanitized view struct — do NOT expose the raw `Config` struct (which contains `refresh_token`
as `Option<String>`).

```json
{
  "clusters": ["prod", "staging"],
  "listen": "0.0.0.0:8080",
  "refresh_interval_seconds": 60,
  "metadata_level": "task",
  "mode": "discovery",
  "public_address": null,
  "cluster_mode": "standalone",
  "cluster_seeds": [],
  "gossip_port": 8081,
  "node_id": "hostname:8081",
  "metrics_port": null,
  "refresh_token_set": true,
  "refresh_min_interval_seconds": 30,
  "proxy_forward_sensitive_headers": false
}
```

Note `refresh_token_set: bool` not the actual value — shows whether it is configured without
leaking it. This is the standard pattern (used by HashiCorp Vault, GitHub Actions, etc.).

**HTTP:** Always 200. No authentication required (rely on network controls per PROJECT.md).
**Content-Type:** `application/json`.

---

## Target Churn Protection Design

**Problem:** AWS API glitch returns 0 tasks. Cache gets replaced with empty. Prometheus loses all
targets. Next scrape cycle everything appears down. This is a false alarm.

**Solution:** Before calling `replace_cache_and_routing()`, compare new target count to existing.
If `removed / old_total > threshold` (default 0.50), skip the replacement and log a warning.

**Threshold logic:**

```
old_count = current cache targets_count
new_count = len(new_targets)
removed = max(old_count - new_count, 0)

if old_count > 0 AND (removed as f64 / old_count as f64) > threshold:
    log WARN "churn protection: skipping cache update (removed X of Y targets, threshold Z%)"
    increment ecs_sd_targets_removed_total by removed  // still count what would have been removed
    return  // keep stale cache
else:
    proceed with cache replacement
    increment ecs_sd_targets_added_total by max(new_count - old_count, 0)
    increment ecs_sd_targets_removed_total by removed
```

**Config parameter:**
- `--churn-threshold <f64>` / `ECS_SD_CHURN_THRESHOLD` (default: `0.5`)
- Range: 0.0 = always protect (useless), 1.0 = never protect (disable feature)
- A value of 0.0 effectively blocks all cache updates if any target disappears; set default to 0.5

**Edge cases:**
- `old_count == 0` → always accept (initial population; can't compute percentage)
- New count > old count → always accept (growth is never suspicious)
- Feature disabled by setting threshold to `1.0`

**Complexity:** Medium. The logic is simple, but it requires reading the current cache count before
the write, which means coordinating with the existing RwLock in `AppState.cache`.

---

## OpenAPI / Swagger Integration Pattern

**Crates needed:**
```toml
utoipa = "5"
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

**Pattern (axum-native approach):**
1. Annotate each handler: `#[utoipa::path(get, path = "/sd", responses((status = 200, ...)))]`
2. Annotate response types: `#[derive(ToSchema)]` on serializable structs (e.g., `HealthResponse`)
3. Collect in root doc: `#[derive(OpenApi)] #[openapi(paths(...), components(schemas(...)))] struct ApiDoc;`
4. Mount in Router:
   ```rust
   router.merge(
       SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi())
   )
   ```

**Do not use `OpenApiRouter`** (the utoipa-axum binding alternative). It requires restructuring all
existing routes. The `merge(SwaggerUi::new(...))` pattern integrates with no changes to existing
route structure.

**Spec URL:** Expose at `/openapi.json` (not `/api-docs/openapi.json`). This service does not use
API versioning prefixes — the shorter path is cleaner and consistent with `/metrics`, `/health`.

---

## Feature Dependencies

```
Target churn protection
  → needs ecs_sd_targets_added_total + ecs_sd_targets_removed_total metrics (items 4, 5)

Rich /health endpoint
  → needs startup_time in AppState (new field)
  → needs last_refresh_succeeded in AppState (new AtomicBool field)
  → needs last_refresh_error in AppState (new RwLock<Option<String>> field)
  → /health/ready depends on same fields

ecs_sd_cluster_targets GaugeVec (item 3)
  → needs per-cluster breakdown of targets, not just total
  → requires iterating targets after refresh and grouping by cluster label

HTTP request metrics (items 1, 2)
  → need Tower middleware layer applied to the router
  → must run after routing (to know which endpoint matched) — use MatchedPath extractor

ecs_sd_startup_duration_seconds (item 8)
  → needs startup_time in AppState (shared with rich health endpoint)

OpenAPI annotations
  → need serde-serializable structs for all request/response bodies
  → HealthResponse, ConfigResponse, SdResponse need #[derive(ToSchema)]
```

---

## MVP Recommendation

Build in this order (lowest-risk to highest-risk):

1. **`GET /config`** — simplest feature; one new handler + sanitized struct; no AppState changes
2. **`/health/live` + `/health/ready`** — requires small AppState additions (`startup_time`, `last_refresh_succeeded`, `last_refresh_error`); enables k8s probes
3. **Rich `/health` JSON** — builds on the same AppState fields added in step 2
4. **7 new metrics** — Tower middleware for HTTP metrics is additive; per-cluster gauge and startup gauge are straightforward; AWS API call instrumentation requires touching discovery code
5. **OpenAPI/Swagger** — annotation pass across all handlers; mostly mechanical boilerplate; no runtime risk
6. **Target churn protection** — requires the most careful logic; should come last so it can be tested in isolation

**Defer:** None of these features are large enough to defer from v0.3.0.

---

## k8s Probe Configuration Reference

For operators deploying ecs-sd in Kubernetes:

```yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 15
  timeoutSeconds: 3
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 3
  failureThreshold: 3

startupProbe:
  httpGet:
    path: /health/live
    port: 8080
  failureThreshold: 30
  periodSeconds: 5
```

**Why these settings:**
- `livenessProbe` uses `/health/live` (never fails on AWS issues → no restart loops)
- `readinessProbe` uses `/health/ready` (503 when cache degraded → removed from LB until recovered)
- `startupProbe` reuses `/health/live` endpoint (no need for a third endpoint)
- `initialDelaySeconds: 10` on readiness gives ECS discovery time to populate cache before traffic is sent

---

## Sources

- [Prometheus metric naming conventions](https://prometheus.io/docs/practices/naming/) — HIGH confidence (official)
- [Kubernetes probe semantics](https://kubernetes.io/docs/concepts/workloads/pods/probes/) — HIGH confidence (official)
- [utoipa-swagger-ui axum integration](https://docs.rs/utoipa-swagger-ui/latest/utoipa_swagger_ui/) — HIGH confidence (official crate docs)
- [utoipa todo-axum example](https://github.com/juhaku/utoipa/blob/master/examples/todo-axum/src/main.rs) — HIGH confidence (official examples)
- [axum-prometheus middleware](https://github.com/Ptrskay3/axum-prometheus) — MEDIUM confidence (community crate showing industry-standard label names)
- Kubernetes liveness/readiness best practices: [beefed.ai](https://beefed.ai/en/kubernetes-liveness-readiness-probes-best-practices), [fairwinds.com](https://www.fairwinds.com/blog/a-guide-to-understanding-kubernetes-liveness-probes-best-practices) — MEDIUM confidence (community guides corroborating official docs)
