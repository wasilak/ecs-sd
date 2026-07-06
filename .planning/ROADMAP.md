# Roadmap: ecs-sd (ECS Prometheus Service Discovery)

**Project:** ecs-sd — AWS ECS HTTP Service Discovery for Prometheus/VictoriaMetrics

---

## Milestones

- ✅ **v1.0 Release** — Phases 1–5 (shipped 2026-05-26) — [Archive](milestones/v1.0-ROADMAP.md)
- ✅ **v0.2.0 Network** — Phases 6–8 (shipped 2026-05-26) — [Archive](milestones/v0.2.0-ROADMAP.md)
- 📋 **v0.3.0 Operational Excellence** — Phases 9–15 (planned)

---

## Phases

<details>
<summary>✅ v1.0 Release (Phases 1–5) — SHIPPED 2026-05-26</summary>

- [x] Phase 1: Core Discovery & HTTP API (3/3 plans) — completed 2026-05-19
- [x] Phase 2: Metadata Labels (3/3 plans) — completed 2026-05-19
- [x] Phase 3: Caching & Configuration (3/3 plans) — completed 2026-05-20
- [x] Phase 4: Observability & Logging (1/1 plan) — completed 2026-05-25
- [x] Phase 5: Packaging & CI/CD (3/3 plans) — completed 2026-05-25

Full archive: `.planning/milestones/v1.0-ROADMAP.md`

</details>

<details>
<summary>✅ v0.2.0 Network (Phases 6–8) — SHIPPED 2026-05-26</summary>

- [x] Phase 6: Proxy Mode & Fargate (3/3 plans) — completed 2026-05-26
- [x] Phase 7: Horizontal Clustering (6/6 plans) — completed 2026-05-26
- [x] Phase 8: Internal Metrics & Self-Registration (3/3 plans) — completed 2026-05-26

Full archive: `.planning/milestones/v0.2.0-ROADMAP.md`

</details>

### 📋 v0.3.0 Operational Excellence (Phases 9–15)

- [x] **Phase 9: CacheSnapshot Refactor & Module Cleanup** - Atomic cache replacement and correct module layering (hard prerequisite for phases 11, 12, 13) (completed 2026-07-06)
- [ ] **Phase 10: Error Hardening & Dependency Pinning** - Remove panics from production paths, harden outbound connections, pin SDK versions
- [ ] **Phase 11: Rich Health Endpoint & k8s Probes** - Structured /health JSON, /health/live always-200, /health/ready for readiness gating
- [ ] **Phase 12: HTTP Metrics Middleware & New Metric Families** - 7 new Prometheus metrics covering HTTP traffic, discovery, churn, AWS calls, startup
- [ ] **Phase 13: Config Endpoint & Churn Protection** - Runtime config introspection and stale-cache preservation on AWS glitch
- [ ] **Phase 14: OpenAPI/Swagger** - Machine-readable spec at /openapi.json, visual explorer at /swagger-ui
- [ ] **Phase 15: Test Coverage** - HTTP handler integration tests and mocked AWS discovery tests

---

## Phase Details

### Phase 9: CacheSnapshot Refactor & Module Cleanup

**Goal**: All concurrent state access is atomic through a single CacheSnapshot, and the module dependency graph has no handler-to-state import violations
**Depends on**: Nothing (first phase of v0.3.0)
**Requirements**: QUAL-01, QUAL-02, QUAL-05, QUAL-06
**Success Criteria** (what must be TRUE):

  1. `cargo test` passes before and after the refactor — no behavior changes visible to callers
  2. A concurrent handler can never observe targets from a new cache snapshot paired with a routing table from the old one (one atomic write replaces both)
  3. `filter_labels_by_level` lives in `src/models/`, not `src/handlers/sd.rs` — the state layer no longer imports from the handler layer
  4. `migrate_target_label_schema` is absent from the cache refresh hot path; discovery emits canonical label keys directly
  5. AppState holds a single `Arc<RwLock<CacheSnapshot>>` field instead of three separate lock fields

**Plans**: TBD

### Phase 10: Error Hardening & Dependency Pinning

**Goal**: No unwrap panics in production HTTP paths, outbound connections have explicit timeouts, and SDK dependency versions are deterministic
**Depends on**: Nothing (independent of Phase 9)
**Requirements**: QUAL-03, QUAL-04, QUAL-07, QUAL-08
**Success Criteria** (what must be TRUE):

  1. A malformed response construction in proxy_handler or metrics_handler returns HTTP 500 to the caller instead of panicking the Tokio task
  2. The reqwest client in AppState enforces a 5s connect timeout and 10s TCP keepalive (verifiable via config inspection or integration test)
  3. `cargo update` cannot silently upgrade aws-sdk-ec2 to a version mismatched with the aws-sdk-ecs release series
  4. Starting the binary without a resolvable AWS region prints a human-readable error message and exits with a non-zero code

**Plans**: 2 plans (wave 1, both parallel — no file overlap)
- [ ] 10-01-PLAN.md — Runtime error hardening: remove unwrap panics from proxy/metrics handlers (QUAL-03), add reqwest connect_timeout + tcp_keepalive (QUAL-04)
- [ ] 10-02-PLAN.md — Dependency pinning + startup region validation: exact-pin aws-sdk-ec2/ecs (QUAL-07), hard-fail on missing AWS region (QUAL-08)

### Phase 11: Rich Health Endpoint & k8s Probes

**Goal**: /health returns structured operational state; /health/live and /health/ready are safe for ALB and k8s probe wiring
**Depends on**: Phase 9
**Requirements**: HEALTH-01, HEALTH-02, HEALTH-03, HEALTH-04
**Success Criteria** (what must be TRUE):

  1. `GET /health` returns a JSON body containing status, version, uptime_seconds, a cache sub-object (targets count, age_seconds, state), a cluster sub-object (mode, nodes, is_leader), and a last_refresh sub-object (status, timestamp)
  2. `GET /health` returns HTTP 503 when the cache state is empty AND the last refresh failed
  3. `GET /health/live` always returns HTTP 200 with `{"status":"alive"}` — no cache or AWS state is checked
  4. `GET /health/ready` returns HTTP 200 when the cache contains at least one target, HTTP 503 when the cache is empty

**Plans**: TBD

### Phase 12: HTTP Metrics Middleware & New Metric Families

**Goal**: All 7 new Prometheus metrics are present in /metrics output with correct labels after normal operation
**Depends on**: Phase 9
**Requirements**: MET-08, MET-09, MET-10, MET-11, MET-12, MET-13, MET-14
**Success Criteria** (what must be TRUE):

  1. After any HTTP request, `ecs_sd_http_requests_total` counter appears in `GET /metrics` output with endpoint, method, and status_code labels — proxy route labels use the matched path pattern, not the raw URI
  2. `ecs_sd_http_request_duration_seconds` histogram appears in `GET /metrics` with endpoint and method labels (no status_code label)
  3. After a successful discovery refresh, `ecs_sd_discovery_targets_per_cluster` gauge reflects per-cluster target counts
  4. When the target set changes between refreshes, `ecs_sd_discovery_target_churn_total` with added/removed labels increments by the delta
  5. `ecs_sd_startup_duration_seconds` gauge records the time from process start to first successful cache population

**Plans**: TBD

### Phase 13: Config Endpoint & Churn Protection

**Goal**: Runtime config is inspectable over HTTP without exposing secrets, and an AWS transient failure returning zero tasks cannot wipe the in-memory target cache
**Depends on**: Phase 9
**Requirements**: CONF-07, CHURN-01
**Success Criteria** (what must be TRUE):

  1. `GET /config` returns effective runtime config as JSON; if a refresh_token is configured it appears only as `refresh_token_set: true`, never as its value
  2. When all ECS clusters return errors in a single refresh cycle, the cache retains the previous stale targets — it is never replaced with an empty list
  3. With a churn threshold configured (e.g. 0.5), a refresh that would drop more than 50% of known targets is discarded and a warning is logged
  4. The churn threshold check is skipped when the previous cache was empty, allowing the initial target population to proceed

**Plans**: TBD

### Phase 14: OpenAPI/Swagger

**Goal**: All HTTP endpoints are self-documenting via a machine-readable OpenAPI 3.0 spec and a visual browser UI
**Depends on**: Phases 11, 12, 13 (all handlers must be stable before annotating)
**Requirements**: API-01, API-02, API-03, API-04
**Success Criteria** (what must be TRUE):

  1. `GET /openapi.json` returns a valid OpenAPI 3.0 JSON document listing all public endpoints (/sd, /sd/refresh, /health, /health/live, /health/ready, /metrics, /config, /proxy/{id}/metrics)
  2. `GET /swagger-ui` renders the Swagger UI in a browser and successfully loads the spec from /openapi.json
  3. Each endpoint entry in the spec documents its query parameters, response schemas, and HTTP status codes
  4. All response model structs (Target, ProxyTarget, health response, config response) appear in the spec's components/schemas section

**Plans**: TBD

### Phase 15: Test Coverage

**Goal**: HTTP handler integration tests and mocked AWS discovery tests provide a regression safety net for all v0.3.0 behavior
**Depends on**: Phase 14 (all handlers finalized)
**Requirements**: TEST-01, TEST-02
**Success Criteria** (what must be TRUE):

  1. Integration tests using axum's test client exercise /health, /health/live, /health/ready, /sd (with filter params), and /config with real AppState constructed in test setup — no mock at the AppState boundary
  2. A test for `discover_all_clusters` total-failure path verifies the stale cache is preserved when all clusters return errors
  3. A test for `discover_all_clusters` partial-failure path verifies successful clusters' results are returned while failed clusters are logged and skipped

**Plans**: TBD

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Core Discovery & HTTP API | v1.0 | 3/3 | ✓ Complete | 2026-05-19 |
| 2. Metadata Labels | v1.0 | 3/3 | ✓ Complete | 2026-05-19 |
| 3. Caching & Configuration | v1.0 | 3/3 | ✓ Complete | 2026-05-20 |
| 4. Observability & Logging | v1.0 | 1/1 | ✓ Complete | 2026-05-25 |
| 5. Packaging & CI/CD | v1.0 | 3/3 | ✓ Complete | 2026-05-25 |
| 6. Proxy Mode & Fargate | v0.2.0 | 3/3 | ✓ Complete | 2026-05-26 |
| 7. Horizontal Clustering | v0.2.0 | 6/6 | ✓ Complete | 2026-05-26 |
| 8. Internal Metrics & Self-Registration | v0.2.0 | 3/3 | ✓ Complete | 2026-05-26 |
| 9. CacheSnapshot Refactor & Module Cleanup | v0.3.0 | 3/3 | Complete    | 2026-07-06 |
| 10. Error Hardening & Dependency Pinning | v0.3.0 | 0/2 | Not started | - |
| 11. Rich Health Endpoint & k8s Probes | v0.3.0 | 0/? | Not started | - |
| 12. HTTP Metrics Middleware & New Metric Families | v0.3.0 | 0/? | Not started | - |
| 13. Config Endpoint & Churn Protection | v0.3.0 | 0/? | Not started | - |
| 14. OpenAPI/Swagger | v0.3.0 | 0/? | Not started | - |
| 15. Test Coverage | v0.3.0 | 0/? | Not started | - |

---

*See STATE.md for current execution state*
*Last updated: 2026-07-06 — Phase 10 planned (2 plans, wave 1)*
