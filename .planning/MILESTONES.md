# Milestones: ecs-sd

## v1.0 — Release

**Shipped:** 2026-05-26
**Phases:** 1–5 | **Plans:** 13 | **Commits:** 81

### Delivered

1. Full AWS ECS discovery chain — 8 chained API calls building Prometheus-compatible targets
2. 14 metadata labels across 5 configurable levels with per-request `?level=` override
3. Stale-while-revalidate cache with ±10% jitter, cooperative shutdown, cache-state headers
4. JSON structured logging via tracing-subscriber with `RUST_LOG` env-filter
5. Distroless production image (`gcr.io/distroless/cc-debian12`) with dep-layer caching
6. 12 unit tests covering label_builder, Target::new, and cache/filter behavior

### Known Deferred Items

- PKG-03: Full GitHub Actions release automation (GHCR push)
- QUAL-02/03: Idiomatic error handling (no-unwrap, thiserror)
- Phase 3 human UAT: AWS credential modes E2E testing incomplete

**Archive:** `.planning/milestones/v1.0-ROADMAP.md`

---

---

## v0.2.0 — Network

**Shipped:** 2026-05-26
**Phases:** 6–8 | **Plans:** 12 | **Commits:** 50+

### Delivered

1. **Proxy Mode** — Reverse proxy for Fargate targets, enabling network segmentation support
2. **Fargate Discovery** — ENI IP extraction for Fargate tasks via AWS SDK attachments API
3. **Horizontal Clustering** — Gossip-based membership with `chitchat`, deterministic leader election
4. **Cache Propagation** — Leader discovery results propagate to followers via gossip protocol
5. **Prometheus Metrics** — `/metrics` endpoint with 9 operational metrics (discovery, cache, proxy, cluster)
6. **Self-Registration** — ecs-sd discovers itself via standard docker labels (emergent behavior)
7. **Separate Metrics Port** — Optional `ECS_SD_METRICS_PORT` for security separation
8. **Terraform Module** — Production-ready Fargate deployment with Cloud Map integration

### Technical Highlights

- ~3,489 LOC Rust with 103 unit tests passing
- Cluster failover within ~10s via gossip failure detection
- All metrics in Prometheus text exposition format
- Proxy mode routing table rebuilds on each cache refresh (PROX-06)

**Archive:** `.planning/milestones/v0.2.0-ROADMAP.md`

---

---

## v0.3.0 — Operational Excellence

**Shipped:** 2026-07-13
**Phases:** 9–15 | **Plans:** 21 | **Tests:** 215 (was 103)

### Delivered

1. **CacheSnapshot Atomicity** — Single `Arc<RwLock<CacheSnapshot>>` replaces 3 separate locks, eliminating torn reads
2. **Production Hardening** — Zero panics in HTTP paths, reqwest timeouts, exact SDK pins, region validation
3. **Rich Health Probes** — `/health` (structured JSON), `/health/live` (always 200), `/health/ready` (readiness gating)
4. **7 New Prometheus Metrics** — HTTP requests/latency, per-cluster targets, target churn, AWS API calls, startup duration
5. **Config Endpoint + Churn Protection** — Runtime config introspection, stale-cache preservation on AWS glitches
6. **OpenAPI/Swagger** — Machine-readable spec + visual explorer for all 8 endpoints
7. **Test Coverage** — 215 tests: handler integration + mocked AWS failure paths

### Technical Highlights

- 7,748 LOC Rust across routes/, handlers/, models/, aws/, cluster/, metrics/
- 23 feat commits over 6 days (2026-07-07 → 2026-07-13)
- 27/27 requirements satisfied
- Custom Tower middleware for HTTP metrics (avoided incompatible axum-prometheus)

### Known Deferred Items

- PKG-03: GHCR auto-push / GitHub Actions release not fully wired
- WR-03: `publish_cache_to_gossip` holds snapshot lock across async gossip awaits
- AWS credential modes: E2E testing incomplete

**Archive:** `.planning/milestones/v0.3.0-ROADMAP.md`

---

*See .planning/PROJECT.md for current state*
