# ecs-sd: ECS Prometheus Service Discovery

## What This Is

A Rust HTTP server that provides **Prometheus/VictoriaMetrics-compatible HTTP service discovery** for AWS ECS (Elastic Container Service). It runs as a web service that exposes endpoints (`/sd` and `/health`) which return scrape targets in the format expected by `http_sd_configs`. Built for teams running Prometheus or VictoriaMetrics who want automatic discovery of ECS container metrics endpoints without manual configuration.

## Core Value

**Zero-config metrics discovery for ECS containers** — automatically discover and expose scrape targets for containers with metrics endpoints, using configurable metadata levels and serving cached results to prevent AWS API throttling.

## Requirements

### Validated

- **Phase 03 (caching-configuration)**
  - **CONF-01**: Use clap for CLI with full env var support
  - **CONF-02**: Support cluster list configuration
  - **CONF-03**: Support metadata level configuration
  - **CONF-04**: Support refresh interval configuration
  - **CONF-05**: Support listen address/port configuration
  - **CONF-06**: AWS credentials via standard chain (live human UAT still pending)
  - **CACHE-01**: Cache AWS discovery results in memory
  - **CACHE-02**: Configurable refresh interval (default: 60s)
  - **CACHE-03**: Background refresh on interval (non-blocking)
  - **CACHE-04**: Always serve cached data — stale data acceptable until refresh succeeds
  - **CACHE-05**: Prevent thundering herd/request flood during cache refresh
  - **CACHE-06**: TTL explicitly enforced against refresh interval

### Active

**Core Discovery**
- [ ] **DISC-01**: Discover ECS clusters (configurable list or all)
- [ ] **DISC-02**: Discover services within clusters
- [ ] **DISC-03**: Discover tasks running on EC2 launch type (Fargate excluded per scope)
- [ ] **DISC-04**: Filter tasks to only those with container docker label `metrics_port`
- [ ] **DISC-05**: Resolve target address as EC2 node IP + metrics_port from label
- [ ] **DISC-06**: Build Prometheus-compatible target response with labels

**HTTP Endpoints**
- [ ] **HTTP-01**: `/health` endpoint returns 200 OK when healthy
- [ ] **HTTP-02**: `/sd` endpoint returns JSON in Prometheus http_sd_configs format
- [ ] **HTTP-03**: Support query parameter filtering per VictoriaMetrics examples

**Metadata Labels**
- [ ] **META-01**: Level: container — container name, image, port
- [ ] **META-02**: Level: task — task ARN, version, family
- [ ] **META-03**: Level: service — service name, desired/running count
- [ ] **META-04**: Level: cluster — cluster name, ARN
- [ ] **META-05**: Level: aws — region, account ID, availability zone
- [ ] **META-06**: Configurable via startup flag (global default)
- [ ] **META-07**: Overridable per-request via query param

**Caching & Performance**

**Configuration**

**Observability**
- [ ] **OBS-01**: Structured logging in JSON format
- [ ] **OBS-02**: Log cache hits/misses and refresh operations
- [ ] **OBS-03**: Log discovered target counts per level

**Packaging**
- [ ] **PKG-01**: Multi-stage Dockerfile with distroless or minimal base
- [ ] **PKG-02**: GitHub Actions workflow: test → build image → push to GHCR → release
- [ ] **PKG-03**: Published to `ghcr.io/wasilak/ecs-sd`

**Quality**
- [ ] **QUAL-01**: Unit tests for core logic (target building, label generation)
- [ ] **QUAL-02**: Idiomatic Rust with proper error handling (no unwrap in production paths)
- [ ] **QUAL-03**: Async/await with Tokio
- [ ] **QUAL-04**: Use aws-sdk-ecs with proper pagination handling

### Out of Scope

| Feature | Reason |
|---------|--------|
| Fargate support | EC2 launch type only per scope |
| Multiple containers per task | One target per task with metrics_port label |
| Write operations to AWS | Read-only discovery only |
| Metrics scraping | This is discovery, not scraping |
| Alerting or monitoring | Discovery service only |
| Kubernetes support | ECS only |
| File-based service discovery | HTTP SD only |
| TLS/HTTPS termination | Run behind reverse proxy if needed |
| Authentication/authorization | Use network-level controls |

## Context

**Technical Environment:**
- AWS ECS clusters with EC2 launch type
- Containers expose metrics endpoints via docker label `metrics_port`
- Prometheus or VictoriaMetrics as scrapers
- Standard AWS credential chain (IAM roles preferred)

**Architecture Pattern:**
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Prometheus/    │────▶│  ecs-sd         │────▶│  AWS ECS API    │
│  VictoriaMetrics│     │  HTTP Server    │     │  (background)   │
│  (scraper)      │     │                 │     │                 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌─────────────────┐
                        │  In-Memory      │
                        │  Cache          │
                        └─────────────────┘
```

**Discovery Flow:**
1. On startup: initial AWS discovery → populate cache
2. Background task: periodic refresh at configured interval
3. HTTP requests: serve from cache immediately (never block on AWS)
4. Refresh failures: log error, keep serving stale data

## Constraints

- **Launch type**: EC2 only (no Fargate)
- **Target criteria**: Container must have docker label `metrics_port`
- **Address format**: EC2 instance private IP + metrics_port value
- **Cache behavior**: Always serve cached, refresh in background
- **Logging**: JSON format only
- **AWS auth**: Standard credential chain only

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| HTTP server (not CLI) | Prometheus http_sd_configs requires HTTP endpoint | — Pending |
| In-memory cache only | Simplicity, no external dependencies | — Pending |
| Stale-while-revalidate | Prevents scraper failures during AWS issues | — Pending |
| One target per task | Simplifies model, metrics_port identifies metrics container | — Pending |
| EC2 launch type only | Fargate networking complexity deferred to v2 | — Pending |
| Axum web framework | Modern, tokio-native, widely adopted in Rust | — Pending |

## Evolution

## Current State

Phase 03 is complete: startup configuration parsing, cache lifecycle, and TTL enforcement are implemented and verified in code/tests. Human verification remains for live AWS credential modes (IAM role, profile, env vars).

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-20 after Phase 03 completion*
