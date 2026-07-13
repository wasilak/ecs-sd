# Retrospective: ecs-sd

---

## Milestone: v1.0 — Release

**Shipped:** 2026-05-26
**Phases:** 5 | **Plans:** 13 | **Commits:** 81 | **Timeline:** 7 days

---

### What Was Built

1. Full AWS ECS discovery chain via 8 chained API calls → Prometheus-compatible `http_sd_configs` targets
2. 14 metadata labels across 5 configurable levels (container → task → service → cluster → aws) with per-request `?level=` override
3. Stale-while-revalidate cache with ±10% jitter refresh, cooperative shutdown, `X-Cache-Age`/`X-Cache-State` headers
4. JSON structured logging via tracing-subscriber with `RUST_LOG` env-filter
5. Distroless production image with dep-layer caching
6. 12 unit tests for label_builder helpers, Target::new, and cache/filter behavior

---

### What Worked

- **Wave-based execution** — dependency ordering within phases (e.g., Phase 3 Wave 1 config before Wave 2 refresh logic) prevented integration surprises
- **TDD cadence** in Phase 3 (failing tests committed before implementation) caught the TTL enforcement gap that would have shipped silently
- **Distroless migration** was low-friction once the glibc compat issue (alpine → bookworm) was identified upfront in planning
- **Phase 4 minimal scope** (1 plan, 1 commit) — observability was a clean one-line change because tracing was already wired; the small scope was correct
- **clap derive macros** — `env()` attribute made CLI ↔ env var parity zero-cost to implement

---

### What Was Inefficient

- **REQUIREMENTS.md never updated during execution** — all items stayed `[ ]` throughout, making traceability impossible without reading SUMMARY files
- **STATE.md drifted** — Phase 3 showed 20% / "context gathered" even after all 3 plans executed; caught only at milestone close
- **Phase 3 UAT left at `status: testing`** with 5/6 tests pending — this was never resolved and carried over as debt
- **ROADMAP.md content rot** — the overview table had raw phase completion data (`3/3 | Complete | 2026-05-20`) mixed with original planning text, making it structurally inconsistent

---

### Patterns Established

- **stub-src dep caching** in Dockerfile — build empty binary to cache deps, then real build
- **`MissedTickBehavior::Skip`** as the standard tokio interval approach for background workers
- **Cooperative shutdown via `watch` channel** — signal between refresh iterations rather than hard kill
- **Labels omitted when data unavailable** (not empty string) — cleaner Prometheus label set

---

### Key Lessons

1. **Update REQUIREMENTS.md in-flight, not just at milestone close.** One-line traceability update per phase saves significant archaeology at archive time.
2. **UAT sessions need a hard close gate.** 03-UAT.md reached `status: testing` but never flipped to `complete` or `passed`. Enforce a close step before moving to the next phase.
3. **STATE.md phase status should be a computed fact, not a manual update.** The discrepancy between "Phase 3 at 20%" and "3/3 SUMMARY.md files exist" is a systemic failure of manual state management.
4. **Distroless requires glibc.** `rust:alpine` → static binary needed for alpine, or use `rust:bookworm` → glibc → distroless/cc. Never mix without checking.

---

### Cost Observations

- Sessions: multiple short sessions across 7 days
- Notable: Phase 5 (packaging + tests) was the most efficient — 3 plans in one tight session with clear objectives; Phase 3 (caching) was the most complex with 3 waves and inter-wave dependencies

---

---

## Milestone: v0.3.0 — Operational Excellence

**Shipped:** 2026-07-13
**Phases:** 7 | **Plans:** 21 | **Tests:** 215 (was 103)

---

### What Was Built

1. CacheSnapshot atomicity — single `Arc<RwLock<CacheSnapshot>>` replaces 3 separate locks
2. Zero panics in HTTP paths — `unwrap_or_else` fallbacks, reqwest timeouts, exact SDK pins
3. Rich health endpoint — `/health` (structured JSON), `/health/live` (always 200), `/health/ready` (readiness gating)
4. 7 new Prometheus metrics — HTTP requests/latency, per-cluster targets, churn, AWS calls, startup duration
5. Config endpoint with secret masking + target churn protection guard
6. OpenAPI/Swagger — machine-readable spec + visual explorer for all 8 endpoints
7. 215 tests — handler integration + mocked AWS failure paths

---

### What Worked

- **Wave-based parallelization** — Phase 12's 6 plans executed across 5 waves with parallel waves 2 (no file overlap) — efficient use of context
- **Gap closure waves** — Phase 12 plans 05/06 were gap closure after verification failures; the pattern of verify → close → re-verify worked cleanly
- **Pure function extraction** — `churn_guard_should_discard()` as pure function (Phase 13) made testing trivial without mocks
- **`utoipa` integration** — utoipa 5 + utoipa-swagger-ui 9 with axum feature was straightforward; proc macros reduced boilerplate significantly
- **Custom Tower middleware** — `from_fn_with_state` approach (~60 lines) avoided `axum-prometheus` ecosystem incompatibility while keeping all 9 existing metrics

---

### What Was Inefficient

- **REQUIREMENTS.md traceability drifted** — HEALTH-01..04 stayed `[ ]` even after Phase 11 completion; MET-08/10/14 showed "Gap closure planned" instead of updating to "Complete"
- **ROADMAP.md not updated during execution** — Phase 12 plan 12-06 and Phase 15 plans stayed unchecked despite SUMMARY.md files existing; required manual correction at archive time
- **STATE.md stale** — Phase 14 showed "Not started" and Phase 15 showed "0%" even after completion; the `progress.percent` (86%) didn't match actual completion (100%)
- **Phase 12 scope creep** — 6 plans with 5 waves (including 2 gap closure waves) suggests the initial plan underestimated the metrics wiring complexity

---

### Patterns Established

- **CacheSnapshot as single atomic unit** — replacing multiple locks with one `Arc<RwLock<Snapshot>>` pattern
- **`record_*_once()` helpers** — AtomicBool one-shot pattern for metrics that should only fire once (startup duration)
- **`ConfigResponse` separate from `Config`** — security-by-design pattern for endpoints that expose config
- **`tower::ServiceExt::oneshot` for handler tests** — axum 0.8 requires `create_routes(state).with_state(state)` pattern for stateful router testing
- **aws-smithy-mocks** — `mock!(Client::method).then_error(|| ...)` pattern for AWS SDK testing

---

### Key Lessons

1. **Update REQUIREMENTS.md in-flight.** Traceability table should be updated per-phase, not deferred to milestone close. The drift between "what was built" and "what was tracked" created confusion.
2. **ROADMAP.md checkboxes need maintenance.** When SUMMARY.md exists for a plan, the ROADMAP checkbox should be updated immediately. This is a 5-second action that prevents 10-minute archaeology later.
3. **Gap closure waves are a planning signal.** Phase 12 needing 2 gap closure waves (plans 05/06) suggests the initial success criteria were incomplete. Future phases should include gap closure budget in the plan.
4. **Test infrastructure pays off fast.** The `new_for_test` + `test_helpers` pattern (Phase 15 plan 01) made adding 11 integration tests trivial. Invest in test scaffolding early.
5. **Custom middleware > ecosystem lock-in.** The 60-line custom Tower middleware preserved all existing metrics. Adding `axum-prometheus` would have required dropping 9 metrics. Always check ecosystem compatibility before adopting.

---

### Cost Observations

- Model mix: balanced (default profile)
- Sessions: ~6 days of execution (2026-07-07 → 2026-07-13)
- Notable: Phase 12 (6 plans, 5 waves) was the most complex; Phase 14 (2 plans, OpenAPI) was the most efficient per-plan

---

## Cross-Milestone Trends

| Metric | v1.0 | v0.2.0 | v0.3.0 |
|--------|------|--------|--------|
| Days | 7 | 1 | 6 |
| Phases | 5 | 3 | 7 |
| Plans | 13 | 12 | 21 |
| LOC (Rust) | ~2,070 | ~3,489 | ~7,748 |
| Test count | 41 | 103 | 215 |
| Requirements | 18 | 22 | 27 |
