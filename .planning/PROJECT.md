# ecs-sd: ECS Prometheus Service Discovery

## What This Is

A Rust HTTP server that provides **Prometheus/VictoriaMetrics-compatible HTTP service discovery** for AWS ECS (Elastic Container Service). It runs as a web service that exposes endpoints (`/sd` and `/health`) which return scrape targets in the format expected by `http_sd_configs`. Built for teams running Prometheus or VictoriaMetrics who want automatic discovery of ECS container metrics endpoints without manual configuration.

## Core Value

**Zero-config metrics discovery for ECS containers** — automatically discover and expose scrape targets for containers with metrics endpoints, using configurable metadata levels and serving cached results to prevent AWS API throttling.

## Requirements

### Validated

- **Phase 03 (caching-configuration)**
  - **CONF-01**: Use clap for CLI with full env var support — v1.0
  - **CONF-02**: Support cluster list configuration — v1.0
  - **CONF-03**: Support metadata level configuration — v1.0
  - **CONF-04**: Support refresh interval configuration — v1.0
  - **CONF-05**: Support listen address/port configuration — v1.0
  - **CONF-06**: AWS credentials via standard chain (live human UAT still pending) — v1.0
  - **CACHE-01**: Cache AWS discovery results in memory — v1.0
  - **CACHE-02**: Configurable refresh interval (default: 60s) — v1.0
  - **CACHE-03**: Background refresh on interval (non-blocking) — v1.0
  - **CACHE-04**: Always serve cached data — stale data acceptable until refresh succeeds — v1.0
  - **CACHE-05**: Prevent thundering herd/request flood during cache refresh — v1.0
  - **CACHE-06**: TTL explicitly enforced against refresh interval — v1.0

- **v0.2.0 Network (Phases 6-8)**
  - **PROX-01** to **PROX-07**: Proxy mode with reverse proxy support — v0.2.0
  - **FARG-01** to **FARG-03**: Fargate task discovery via ENI extraction — v0.2.0
  - **CLUS-01** to **CLUS-09**: Horizontal clustering with gossip and leader election — v0.2.0
  - **MET-01** to **MET-07**: Prometheus metrics endpoint with 9 operational metrics — v0.2.0

### Active

*All v1.0 and v0.2.0 requirements shipped. Planning v0.3.0...*

**Deferred from v1.0/v0.2.0**
- **PKG-03**: Full GitHub Actions release automation (GHCR push) — infrastructure task
- **QUAL-02/03**: Idiomatic error handling audit (`thiserror`, remove unwrap) — refactoring
- **CONF-06**: Complete AWS credential modes E2E testing — pending access to all auth methods

### Out of Scope

| Feature | Reason |
|---------|--------|
| Fargate support in discovery mode | ✅ Implemented in v0.2.0 via proxy mode — Fargate tasks work via `/proxy/:id/*path` routing |
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
| HTTP server (not CLI) | Prometheus http_sd_configs requires HTTP endpoint | ✓ Correct — standard pattern |
| In-memory cache only | Simplicity, no external dependencies | ✓ Correct |
| Stale-while-revalidate | Prevents scraper failures during AWS issues | ✓ Correct |
| One target per task | Simplifies model, prometheus.io/port identifies metrics container | ✓ Correct |
| EC2 launch type only | Fargate needs proxy to bridge network gap | ✓ Correct — Fargate supported via proxy mode v0.2.0 |
| Axum web framework | Modern, tokio-native, widely adopted in Rust | ✓ Correct |
| clap derive macros | Auto-generates help, env-var binding via `env()` attribute | ✓ Correct |
| MissedTickBehavior::Skip | Prevents refresh pile-up on slow AWS API | ✓ Correct |
| rust:bookworm base | glibc compatibility with distroless/cc-debian12 | ✓ Correct |
| Proxy mode for Fargate | Bridges network gap without complex VPC networking | ✓ Correct — clean separation of concerns |
| chitchat for clustering | Established gossip library, tokio-native, minimal config | ✓ Correct — simple, effective |
| Min NodeId leader election | Simple deterministic algorithm, no additional consensus | ✓ Correct — works with gossip failure detection |
| Optional separate metrics port | Security separation, allows firewall rules | ✓ Correct — follows Prometheus conventions |
| Self-registration as emergent behavior | No special code needed, just standard discovery | ✓ Correct — elegant solution |

## Current State

**v0.2.0 Network shipped 2026-05-26.** All 8 phases complete, 25 plans total, ~50 commits.

- ~3,489 LOC Rust across modular architecture (`routes/`, `handlers/`, `models/`, `aws/`, `cluster/`, `metrics/`)
- 103 unit tests passing (`cargo test`)
- **Three operating modes:**
  - Discovery mode (v0.1.0): Direct target exposure for EC2 launch type
  - Proxy mode (v0.2.0): Reverse proxy for Fargate support in segmented networks
  - Cluster mode (v0.2.0): Gossip-based HA with leader election
- **Observability:** Prometheus `/metrics` endpoint with 9 operational metrics
- **Deployment:** Terraform module for Fargate with Cloud Map service discovery

**Architecture highlights:**
- Gossip-based cluster membership via `chitchat` crate
- Deterministic leader election (min NodeId) with ~10s failover
- Stale-while-revalidate cache with jitter and cooperative shutdown
- Prometheus-compatible HTTP service discovery with 14 metadata labels
- Self-registration via standard docker labels (emergent behavior)

**Known gaps carried forward:**
- PKG-03: GHCR auto-push / GitHub Actions release not fully wired
- QUAL-02/03: Some `unwrap`/`expect` in production paths; `thiserror` not added
- AWS credential modes: E2E testing incomplete (needs all auth method access)

**Next milestone: v0.3.0 (TBD)**
- Potential areas: Performance optimization, extended metrics, operational tooling
- Pending requirements review and prioritization

## Evolution

<details>
<summary>Instructions for future milestone transitions</summary>

**After each phase transition:**
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Current State

</details>

---
*Last updated: 2026-05-26 after v0.2.0 milestone — all requirements validated, planning v0.3.0*
