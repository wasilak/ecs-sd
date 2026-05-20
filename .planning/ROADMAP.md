# Roadmap: ecs-sd (ECS Prometheus Service Discovery)

**Project:** ecs-sd — AWS ECS HTTP Service Discovery for Prometheus/VictoriaMetrics  
**Created:** 2026-05-19  
**Mode:** Standard (Vertical Slices)  

---

## Overview

| Phase | Name | Goal | Requirements | Est. Effort |
|-------|------|------|--------------|-------------|
| 1 | Core Discovery & HTTP API | Implement ECS discovery and basic HTTP endpoints | DISC-01..06, HTTP-01..04 | Large |
| 2 | Metadata Labels | Implement all metadata levels with configuration | META-01..16 | Large |
| 3 | Caching & Configuration | 1/2 | In Progress|  |
| 4 | Observability & Logging | Add structured JSON logging and instrumentation | OBS-01..04 | Small |
| 5 | Packaging & CI/CD | Docker, GHCR, GitHub Actions workflow | PKG-01..03, QUAL-01..05 | Medium |

---

## Phase 1: Core Discovery & HTTP API

**Status:** ✓ Complete (2026-05-19)

**Goal:** ECS task discovery working and HTTP endpoints serving valid Prometheus SD format

**Requirements:**

- DISC-01: Discover ECS clusters from configured list
- DISC-02: Discover services within clusters  
- DISC-03: List tasks with EC2 launch type
- DISC-04: Filter tasks by docker label `metrics_port`
- DISC-05: Resolve EC2 instance IP for tasks
- DISC-06: Build Prometheus-compatible targets
- HTTP-01: `/health` endpoint
- HTTP-02: `/sd` endpoint with proper JSON format
- HTTP-03: Query param filtering support
- HTTP-04: Graceful shutdown

**Success Criteria:**

1. `GET /health` returns 200 OK
2. `GET /sd` returns valid Prometheus http_sd_configs JSON:
   ```json
   [
     {
       "targets": ["10.0.1.5:9999"],
       "labels": {
         "__meta_ecs_cluster_name": "prod",
         "__meta_ecs_service_name": "api",
         "__meta_ecs_task_family": "api-task"
       }
     }
   ]
   ```

3. Targets include only tasks with `metrics_port` docker label
4. Address format is `EC2_IP:metrics_port`
5. Graceful shutdown handles in-flight requests

**Dependencies:**

- tokio (async runtime)
- axum (HTTP framework)
- aws-config + aws-sdk-ecs
- tracing (logging)
- serde (JSON serialization)

**Key Technical Challenges:**

- AWS API pagination for clusters, services, tasks
- Mapping task → container instance → EC2 instance → private IP
- Filtering containers by docker label
- Error handling for AWS API failures

**Notes:**

- Hardcode minimal labels initially (just cluster, service, task)
- Full metadata levels come in Phase 2
- No caching yet — direct AWS calls per request

---

## Phase 2: Metadata Labels

**Status:** ✓ Complete (2026-05-19)

**Goal:** Complete metadata label system with all 5 levels and configurable output

**Requirements:**

- META-01..14: All label types implemented
- META-15..16: Global and per-request level configuration

**Success Criteria:**

1. `--metadata-level container` includes only container labels
2. `--metadata-level aws` includes all labels (container + task + service + cluster + aws)
3. `GET /sd?level=service` overrides global default for that request
4. All AWS metadata extracted correctly (region, account, AZ)

**Dependencies:**

- Phase 1 complete
- clap (CLI parsing)

**Key Technical Challenges:**

- Parsing ARNs to extract account ID
- Getting EC2 instance details (DescribeInstances) for AZ
- Efficient label building without excessive cloning

**Notes:**

- Default level: `task` (includes container + task labels)
- Level hierarchy: container < task < service < cluster < aws
- Higher levels include all lower level labels

---

## Phase 3: Caching & Configuration

**Goal:** Background refresh with stale-while-revalidate and full CLI configuration

**Plans:** 1/2 plans executed

Plans:
**Wave 1**

- [x] 03-01-PLAN.md — CLI/env configuration parsing, validation, and startup wiring

**Wave 2** *(blocked on Wave 1 completion)*

- [ ] 03-02-PLAN.md — Background cache refresh loop, jitter, stale serving, and cache-age visibility

**Requirements:**

- CACHE-01..06: In-memory caching with background refresh
- CONF-01..06: CLI flags with env var support

**Success Criteria:**

1. `--refresh-interval 30s` sets cache refresh to 30 seconds
2. Background task refreshes cache without blocking requests
3. Requests always serve from cache immediately
4. Failed refresh logs error, continues serving stale data
5. All config options work via env vars (e.g., `ECS_SD_REFRESH_INTERVAL=30s`)
6. AWS credentials work with IAM role, profile, or env vars

**Dependencies:**

- Phase 2 complete
- tokio::time for intervals
- dashmap or RwLock for concurrent cache access

**Key Technical Challenges:**

- Thread-safe cache updates without blocking readers
- Background task lifecycle management
- Graceful handling of AWS throttling during refresh

**Notes:**

- Cache structure: `HashMap<ClusterName, Vec<Target>>`
- Use `Arc<RwLock<_>>` or `dashmap` for concurrent access
- Refresh task spawned on startup, runs forever

---

## Phase 4: Observability & Logging

**Goal:** Structured JSON logging and operational visibility

**Requirements:**

- OBS-01..04: JSON logging, discovery events, target counts

**Success Criteria:**

1. Logs are valid JSON with `timestamp`, `level`, `message`, `fields`
2. Log includes discovery start: `{"message":"discovery refresh started","clusters":["prod"]}`
3. Log includes completion: `{"message":"discovery refresh complete","targets":42,"duration_ms":1500}`
4. Log includes failures: `{"message":"discovery refresh failed","error":"..."}`

**Dependencies:**

- Phase 3 complete
- tracing + tracing-subscriber with json feature

**Key Technical Challenges:**

- Structured fields in tracing spans
- Proper error context propagation

**Notes:**

- Use `tracing` macros: `info!`, `warn!`, `error!`
- Configure subscriber at startup based on RUST_LOG

---

## Phase 5: Packaging & CI/CD

**Goal:** Production-ready container image and automated release pipeline

**Requirements:**

- PKG-01..03: Dockerfile, GHCR, GitHub Actions
- QUAL-01..05: Unit tests, error handling, idiomatic Rust

**Success Criteria:**

1. `docker build -t ecs-sd .` produces working image
2. Image runs as non-root user
3. GitHub Actions workflow runs on push to main:
   - Run `cargo test`
   - Build and push to `ghcr.io/wasilak/ecs-sd:latest`
   - Create release with binary artifact
4. All unit tests pass
5. No unwrap/expect in production code paths
6. Proper AWS pagination handling

**Dependencies:**

- All previous phases complete
- GitHub repository configured with GHCR access

**Key Technical Challenges:**

- Multi-stage Dockerfile for minimal image size
- GitHub Actions secrets for GHCR push
- Release automation with proper tagging

**Notes:**

- Use distroless or alpine for final image
- Cache cargo dependencies in Docker build
- Release binary should be statically linked or include required libs

---

## Milestone: v1.0 Release

**Trigger:** All 5 phases complete

**Definition of Done:**

- All v1 requirements (38 total) implemented
- All unit tests passing
- Container image published to GHCR
- README with usage examples and configuration reference
- Example Prometheus scrape config showing http_sd_configs usage

**Post-Milestone:**

- Move v2 requirements into Active
- Consider Fargate support, custom filtering, metrics exposition

---

## State Tracking

| Phase | Status | Requirements Complete | Plans Created |
|-------|--------|----------------------|---------------|
| 1 | ✓ Complete | 10/10 | 3/3 |
| 2 | ○ Not Started | 0/16 | 0/? |
| 3 | ○ Not Started | 0/12 | 0/? |
| 4 | ○ Not Started | 0/4 | 0/? |
| 5 | ○ Not Started | 0/8 | 0/? |

**Last updated:** 2026-05-19

---

*See STATE.md for current execution state*
