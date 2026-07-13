# ecs-sd: ECS Prometheus Service Discovery

## What This Is

A Rust HTTP server that provides **Prometheus/VictoriaMetrics-compatible HTTP service discovery** for AWS ECS (Elastic Container Service). It runs as a web service that exposes endpoints (`/sd`, `/health`, `/metrics`, `/config`, `/openapi.json`) which return scrape targets in the format expected by `http_sd_configs`. Built for teams running Prometheus or VictoriaMetrics who want automatic discovery of ECS container metrics endpoints without manual configuration.

## Core Value

**Zero-config metrics discovery for ECS containers** — automatically discover and expose scrape targets for containers with metrics endpoints, using configurable metadata levels and serving cached results to prevent AWS API throttling.

## Requirements

### Validated

- **v1.0 (Phases 1–5)**
  - CONF-01..06, CACHE-01..06: Configuration, caching, CLI — v1.0

- **v0.2.0 Network (Phases 6–8)**
  - PROX-01..07: Proxy mode with reverse proxy — v0.2.0
  - FARG-01..03: Fargate task discovery via ENI — v0.2.0
  - CLUS-01..09: Horizontal clustering with gossip — v0.2.0
  - MET-01..07: Prometheus metrics endpoint — v0.2.0

- **v0.3.0 Operational Excellence (Phases 9–15)**
  - QUAL-01..08: CacheSnapshot atomicity, error hardening, dependency pinning — v0.3.0
  - HEALTH-01..04: Rich health endpoint with K8s probe support — v0.3.0
  - MET-08..14: 7 new Prometheus metrics (HTTP, discovery, churn, AWS, startup) — v0.3.0
  - CONF-07: Config endpoint with secret masking — v0.3.0
  - CHURN-01: Target churn protection guard — v0.3.0
  - API-01..04: OpenAPI/Swagger self-documentation — v0.3.0
  - TEST-01..02: Handler integration + mocked AWS failure tests — v0.3.0

### Active

_No active requirements — define via `/gsd-new-milestone` for next milestone._

### Out of Scope

| Feature | Reason |
|---------|--------|
| Fargate support in discovery mode | ✅ Implemented in v0.2.0 via proxy mode |
| Multiple containers per task | One target per task with metrics_port label |
| Write operations to AWS | Read-only discovery only |
| Metrics scraping | This is discovery, not scraping |
| Alerting or monitoring | Discovery service only |
| Kubernetes support | ECS only |
| File-based service discovery | HTTP SD only |
| TLS/HTTPS termination | Run behind reverse proxy if needed |
| Authentication/authorization | Use network-level controls |
| `axum-prometheus` crate | Uses different Prometheus registry ecosystem — incompatible |
| SIGHUP hot-reload | Deferred — restart is acceptable for config changes |

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

- **Launch type**: EC2 only (no Fargate — Fargate supported via proxy mode)
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
| CacheSnapshot atomicity | Single `Arc<RwLock<CacheSnapshot>>` replaces 3 locks — no torn reads | ✓ Correct — prerequisite for health/metrics enrichment |
| Custom HTTP metrics middleware | `from_fn_with_state` Tower middleware instead of `axum-prometheus` (incompatible registry) | ✓ Correct — preserves 9 existing metrics |
| `utoipa` for OpenAPI | 3 new crates, simpler `merge(SwaggerUi::new(...))` pattern | ✓ Correct — minimal deps, good axum support |
| `/health/live` for ALB | Must point at always-200 endpoint, not `/health` | ✓ Correct — prevents eviction loop during AWS outage |
| `last_manual_refresh_request` as AtomicU64 | Must NOT fold into CacheSnapshot — different write path | ✓ Correct — rate-limiting concern |
| `discover_all_clusters` returns Result | Caller skips cache replacement on `Err` | ✓ Correct — stale-while-revalidate guarantee |
| Churn guard as pure function | `churn_guard_should_discard()` testable without mocks | ✓ Correct — clean separation |
| ConfigResponse separate from Config | Avoids serializing `refresh_token` secret | ✓ Correct — security by design |

## Current State

**v0.3.0 Operational Excellence shipped 2026-07-13.**

- 215 unit tests passing (`cargo test`)
- **Three operating modes:** Discovery, Proxy, Cluster
- **7,748 LOC Rust** across routes/, handlers/, models/, aws/, cluster/, metrics/
- **HTTP endpoints:** /sd, /sd/refresh, /health, /health/live, /health/ready, /metrics, /config, /openapi.json, /swagger-ui, /proxy/:id/*path

**v0.3.0 deliverables:**
- CacheSnapshot atomicity — single `Arc<RwLock<CacheSnapshot>>`, no torn reads
- Zero panics in HTTP paths — `unwrap_or_else` fallbacks in proxy/metrics handlers
- reqwest connect_timeout(5s) + tcp_keepalive(10s)
- Exact SDK pins — aws-sdk-ec2=1.236.0, aws-sdk-ecs=1.133.1
- Hard startup failure on missing AWS region
- Rich `/health` JSON — status, version, uptime, cache state, cluster role, last refresh
- `/health/live` (always 200) for ALB/K8s liveness probes
- `/health/ready` (200/503) for readiness gating
- 7 new Prometheus metrics — HTTP requests/latency, per-cluster targets, churn, AWS calls, startup
- `GET /config` with secret masking
- Target churn protection — configurable drop ratio threshold
- OpenAPI 3.0 spec + Swagger UI
- 215 tests: handler integration + mocked AWS failure paths

**Known gaps carried forward:**
- PKG-03: GHCR auto-push / GitHub Actions release not fully wired
- WR-03: `publish_cache_to_gossip` holds snapshot lock across async gossip awaits — should clone then release
- AWS credential modes: E2E testing incomplete

---

## Evolution

<details>
<summary>v0.3.0 → next milestone transition</summary>

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
*Last updated: 2026-07-13 after v0.3.0 milestone*
