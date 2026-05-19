# Requirements: ecs-sd (ECS Prometheus Service Discovery)

**Defined:** 2026-05-19  
**Core Value:** Zero-config metrics discovery for ECS containers — automatic discovery of metrics endpoints with configurable metadata

## v1 Requirements

### Discovery (DISC)

- [ ] **DISC-01**: Discover ECS clusters from configured list
- [ ] **DISC-02**: Discover all services within each cluster
- [ ] **DISC-03**: List all tasks with EC2 launch type
- [ ] **DISC-04**: Filter tasks to those with container docker label `metrics_port`
- [ ] **DISC-05**: Resolve EC2 instance IP for each task's container instance
- [ ] **DISC-06**: Build target with address: `EC2_IP:metrics_port`

### HTTP API (HTTP)

- [ ] **HTTP-01**: `GET /health` returns `200 OK` with body `{"status":"healthy"}`
- [ ] **HTTP-02**: `GET /sd` returns JSON array in Prometheus http_sd_configs format
- [ ] **HTTP-03**: `/sd` supports query params for filtering (per VictoriaMetrics http_sd_configs)
- [ ] **HTTP-04**: Graceful shutdown on SIGTERM

### Metadata Labels (META)

**Container Level:**
- [ ] **META-01**: `__meta_ecs_container_name` — container name
- [ ] **META-02**: `__meta_ecs_container_image` — container image URI
- [ ] **META-03**: `__meta_ecs_metrics_port` — port from docker label

**Task Level:**
- [ ] **META-04**: `__meta_ecs_task_arn` — full task ARN
- [ ] **META-05**: `__meta_ecs_task_family` — task definition family
- [ ] **META-06**: `__meta_ecs_task_version` — task definition revision

**Service Level:**
- [ ] **META-07**: `__meta_ecs_service_name` — service name
- [ ] **META-08**: `__meta_ecs_desired_count` — service desired count
- [ ] **META-09**: `__meta_ecs_running_count` — service running count

**Cluster Level:**
- [ ] **META-10**: `__meta_ecs_cluster_name` — cluster name
- [ ] **META-11**: `__meta_ecs_cluster_arn` — cluster ARN

**AWS Level:**
- [ ] **META-12**: `__meta_ecs_region` — AWS region
- [ ] **META-13**: `__meta_ecs_account_id` — AWS account ID from ARN
- [ ] **META-14**: `__meta_ecs_availability_zone` — EC2 instance AZ

**Configuration:**
- [ ] **META-15**: `--metadata-level` flag sets global default (`container`, `task`, `service`, `cluster`, `aws`)
- [ ] **META-16**: `?level=<level>` query param overrides per-request

### Caching (CACHE)

- [ ] **CACHE-01**: In-memory cache stores discovery results
- [ ] **CACHE-02**: `--refresh-interval` flag configures background refresh (default: 60s)
- [ ] **CACHE-03**: Background task refreshes cache at interval without blocking requests
- [ ] **CACHE-04**: All HTTP requests serve from cache immediately
- [ ] **CACHE-05**: Failed refreshes log error and keep serving stale data
- [ ] **CACHE-06**: Cache TTL same as refresh interval

### Configuration (CONF)

- [ ] **CONF-01**: `--clusters` comma-separated list of cluster names/ARNs (required)
- [ ] **CONF-02**: `--listen` address:port to bind (default: `0.0.0.0:8080`)
- [ ] **CONF-03**: `--refresh-interval` duration (default: `60s`)
- [ ] **CONF-04**: `--metadata-level` default level (default: `task`)
- [ ] **CONF-05**: All flags support env vars (e.g., `ECS_SD_CLUSTERS`, `ECS_SD_LISTEN`)
- [ ] **CONF-06**: AWS credentials loaded via aws-config default provider chain

### Observability (OBS)

- [ ] **OBS-01**: JSON structured logging via tracing + tracing-subscriber
- [ ] **OBS-02**: Log discovery refresh start/completion/failure
- [ ] **OBS-03**: Log target count per discovery run
- [ ] **OBS-04**: Log cache hits (optional, debug level)

### Packaging (PKG)

- [ ] **PKG-01**: Dockerfile with multi-stage build
- [ ] **PKG-02**: Final image based on distroless or `gcr.io/distroless/cc` or alpine
- [ ] **PKG-03**: GitHub Actions workflow on push to main:
  1. Run tests (`cargo test`)
  2. Build image
  3. Push to GHCR (`ghcr.io/wasilak/ecs-sd`)
  4. Create GitHub release with binary artifact

### Quality (QUAL)

- [ ] **QUAL-01**: Unit tests for:
  - Label building logic
  - Target address resolution
  - Cache operations
  - Config parsing
- [ ] **QUAL-02**: No unwrap/expect in production paths (proper error handling with `?`)
- [ ] **QUAL-03**: Use `thiserror` or `anyhow` for error types
- [ ] **QUAL-04**: Handle AWS API pagination (describe_clusters, list_services, list_tasks)
- [ ] **QUAL-05**: Proper async error propagation with tokio

## v2 Requirements (Future)

| ID | Feature |
|----|---------|
| V2-DISC-01 | Discover all clusters (no explicit list needed) |
| V2-DISC-02 | Fargate launch type support |
| V2-FILT-01 | Custom label-based filtering (e.g., only services with label `metrics: enabled`) |
| V2-META-01 | Task tags as labels |
| V2-OBS-01 | Prometheus metrics exposition (discovered targets count, cache age, AWS API calls) |
| V2-CONF-01 | Configuration file support (YAML/JSON) |

## Out of Scope

| Feature | Reason |
|---------|--------|
| Fargate (v1) | EC2-only per scope, Fargate in v2 |
| Multiple ports per container | One metrics port per task per scope |
| File-based SD | HTTP SD only |
| Direct metrics scraping | Discovery service, not scraper |
| Kubernetes/EKS | ECS only |
| TLS/mTLS termination | Run behind reverse proxy |
| AuthN/AuthZ | Network-level controls only |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DISC-01..06 | Phase 1 | Pending |
| HTTP-01..04 | Phase 1 | Pending |
| META-01..14 | Phase 2 | Pending |
| META-15..16 | Phase 2 | Pending |
| CACHE-01..06 | Phase 3 | Pending |
| CONF-01..06 | Phase 3 | Pending |
| OBS-01..04 | Phase 4 | Pending |
| PKG-01..03 | Phase 4 | Pending |
| QUAL-01..05 | Phase 5 | Pending |

**Coverage:**
- v1 requirements: 38 total
- Mapped to phases: 38
- Unmapped: 0 ✓

---
*Requirements defined: 2026-05-19*
*Last updated: 2026-05-19 after scope clarification*
