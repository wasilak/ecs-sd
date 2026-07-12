---
gsd_state_version: 1.0
milestone: v0.3.0
milestone_name: Operational Excellence
current_phase: 12
status: completed
last_updated: "2026-07-12T15:53:39.023Z"
last_activity: 2026-07-12 -- Completed Phase 13 Plan 01 config + churn protection
progress:
  total_phases: 7
  completed_phases: 5
  total_plans: 15
  completed_plans: 15
  percent: 71
---

# Project State: ecs-sd

**Project:** ecs-sd — AWS ECS HTTP Service Discovery for Prometheus/VictoriaMetrics
**Current Milestone:** v0.3.0 Operational Excellence
**Current Phase:** 12
**Last Updated:** 2026-07-08

---

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-26)

**Core value:** Zero-config metrics discovery for ECS containers — automatic discovery of metrics endpoints with configurable metadata

**Current focus:** Phase 12 — http-metrics-middleware-new-metric-families

---

## Phase Status

| Phase | Name | Status | Progress |
|-------|------|--------|----------|
| 1 | Core Discovery & HTTP API | ✓ Complete | 100% |
| 2 | Metadata Labels | ✓ Complete | 100% |
| 3 | Caching & Configuration | ✓ Complete | 100% |
| 4 | Observability & Logging | ✓ Complete | 100% |
| 5 | Packaging & CI/CD | ✓ Complete | 100% |
| 6 | Proxy Mode & Fargate | ✓ Complete | 100% |
| 7 | Horizontal Clustering | ✓ Complete | 100% |
| 8 | Internal Metrics & Self-Registration | ✓ Complete | 100% |
| 9 | CacheSnapshot Refactor & Module Cleanup | ✓ Complete | 100% |
| 10 | Error Hardening & Dependency Pinning | ✓ Complete | 100% |
| 11 | Rich Health Endpoint | ✓ Complete | 100% |
| 12 | HTTP Metrics Middleware & New Metric Families | ✓ Complete | 100% |
| 13 | Config Endpoint & Churn Protection | In progress | 33% |
| 14 | OpenAPI/Swagger | Not started | 0% |
| 15 | Test Coverage | Not started | 0% |

---

## Active Work

**Phase 13 — Plan 01 complete, ready for Plan 02**

Phase 13 (Config + Churn) Plan 01 done: max_target_drop_ratio config field + churn guard. Plan 02 (config endpoint handler) next.

---

## Accumulated Context

### v0.2.0 Completion (carried forward)

- Phase 8: Internal Metrics & Self-Registration — COMPLETE ✓
  - Plan 01: Core metrics infrastructure — prometheus crate, MetricsState, 9 metrics registered
  - Plan 02: Discovery & cache instrumentation
  - Plan 03: Conditional metrics & self-registration — separate metrics port
  - Test suite: 103 tests passing

### v0.3.0 Decisions

1. **CacheSnapshot**: Replace three separate `Arc<RwLock<...>>` fields (cache, last_refresh, routing_table) with a single `Arc<RwLock<CacheSnapshot>>` — atomic replacement, prerequisite for health enrichment and churn protection
2. **HTTP Metrics**: Custom `from_fn_with_state` Tower middleware (~60 lines) writing into existing MetricsState — do NOT add `axum-prometheus` (incompatible registry ecosystem, would silently drop 9 existing metrics)
3. **OpenAPI**: `utoipa = "5"` + `utoipa-swagger-ui = { version = "9", features = ["axum"] }` — 3 new crates total; `utoipa-axum` not needed if using simpler `merge(SwaggerUi::new(...))` pattern
4. **ALB health check**: Must point at `/health/live` (always 200), not `/health` — pointing at `/health` causes ALB eviction loop during AWS outage when `/health` returns 503
5. **`last_manual_refresh_request`**: Must NOT fold into CacheSnapshot — it is written from refresh_handler (rate-limiting concern), keep as separate `AtomicU64`
6. **`discover_all_clusters` return type**: Must change from `Vec<Target>` to `Result<Vec<Target>, DiscoveryError>` — caller skips `replace_cache_and_routing` on `Err` (stale-while-revalidate guarantee)

### Architecture Snapshot (v0.2.0)

- ~3,489 LOC Rust across `routes/`, `handlers/`, `models/`, `aws/`, `cluster/`, `metrics/`
- 103 unit tests passing
- Three operating modes: discovery, proxy, cluster
- Gossip-based clustering via chitchat crate

---

## Blockers

_None_

---

## Next Actions

1. Run `/gsd-plan-phase 9` — CacheSnapshot Refactor & Module Cleanup
2. Phase 10 (Error Hardening) can be planned in parallel — independent of Phase 9
3. After Phase 9 completes: phases 11, 12, 13 unlock (can be planned independently)
4. Phase 14 planned only after phases 11–13 are complete
5. Phase 15 planned after Phase 14 completes

---

*State updated: 2026-07-04 — v0.3.0 roadmap created, 7 phases planned (9–15)*

## Current Position

Phase: 13 (config-endpoint-churn-protection) — IN PROGRESS
Plan: 2 of 2
Status: Plan 01 complete
Last activity: 2026-07-12 -- Completed Phase 13 Plan 01 config + churn protection

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 12 P01 | 5 min | 2 tasks | 2 files |
| Phase 12 P02 | 2 min | 2 tasks | 5 files |
| Phase 12 P03 | 9 min | 2 tasks | 2 files |
| Phase 12 P04 | 2 min | 2 tasks | 2 files |
| Phase 12 P05 | 6 min | 3 tasks | 5 files |
| Phase 13 P01 | 8 min | 2 tasks | 2 files |

## Decisions

- [Phase 12 Plan 01] Registered all new Phase 12 metrics in the existing custom `MetricsState::new()` Registry instead of using the global default registry.
- [Phase 12 Plan 01] Kept HTTP duration labels to `endpoint` and `method` only; `status_code` is only on `http_requests_total` as planned.
- [Phase 12 Plan 02] Attached HTTP metrics with `Router::route_layer` instead of `Router::layer` so axum `MatchedPath` is populated before labels are recorded.
- [Phase 12 Plan 02] Left the optional separate metrics server uninstrumented; Plan 12-02 scopes HTTP request metrics to the primary merged application router.
- [Phase 12 Plan 03] Counted AWS SDK pagination loop `.send()` calls per network round-trip, matching actual AWS API request volume.
- [Phase 12 Plan 03] Scoped the old-address snapshot read before `replace_cache_and_routing`, avoiding a RwLock read/write deadlock in cache metrics recording.
- [Phase 12 Plan 05] Kept HTTP request metric value order unchanged while renaming the public third label from `status_code` to `status`.
- [Phase 12 Plan 05] Reset old and configured cluster gauge labels to 0.0 before writing current counts instead of deleting Prometheus series.
- [Phase 12 Plan 05] Captured process startup timing at the beginning of `main` and injected it into `AppState` rather than starting the timer in the constructor.
- [Phase 13 Plan 01] Extracted churn guard logic into pure `churn_guard_should_discard` helper function — testable without constructing AppState or mocks.
