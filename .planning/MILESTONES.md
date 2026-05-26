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

*See .planning/PROJECT.md for current state*
