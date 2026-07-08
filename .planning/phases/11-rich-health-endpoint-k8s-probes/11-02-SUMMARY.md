---
phase: 11-rich-health-endpoint-k8s-probes
plan: "02"
subsystem: handlers/routes
tags: [health, k8s-probes, axum, serde, tdd]
dependency_graph:
  requires: [RefreshOutcome, AppState.started_at, AppState.last_refresh_outcome]
  provides: [health_handler, health_live_handler, health_ready_handler, determine_health_status, determine_readiness_status, /health/live, /health/ready]
  affects: [src/handlers/health.rs, src/routes/health.rs]
tech_stack:
  added: []
  patterns: [TDD RED/GREEN, pure status fn extracted for testability, chitchat drop-before-is_leader, age_seconds zero when empty]
key_files:
  created: []
  modified:
    - src/handlers/health.rs
    - src/routes/health.rs
decisions:
  - "determine_health_status is a pure free fn (not method) to enable inline unit tests without AppState construction"
  - "age_seconds forced to 0 when target_count == 0 to avoid UNIX_EPOCH overflow (~56 year report on empty cache)"
  - "chitchat lock dropped before is_leader() call — is_leader re-acquires the same mutex, holding it deadlocks"
  - "ClusterMode::Standalone maps to nodes=1/is_leader=true (same as metrics_handler None branch)"
  - "503 returned only when cache empty AND last refresh failed — populated cache with failed refresh reports degraded 200"
metrics:
  duration: "~10 minutes"
  completed: "2026-07-08T20:03:05Z"
  tasks_completed: 2
  files_modified: 2
---

# Phase 11 Plan 02: Rich Health Handlers & Routes Summary

**One-liner:** Rewrote health.rs with typed response structs, two pure testable status functions, three handlers (/health /health/live /health/ready), and registered the two new k8s probe routes — all TDD with 9 inline unit tests.

## Tasks Completed

| # | Name | Commit | Files |
|---|------|--------|-------|
| RED | Add failing tests for health handlers and pure status functions | 24ae70f | src/handlers/health.rs |
| 1 (GREEN) | Rewrite health.rs — typed structs, pure status fns, three handlers | 4b30498 | src/handlers/health.rs |
| 2 | Register /health/live and /health/ready routes | 038b440 | src/routes/health.rs |

## TDD Gate Compliance

- RED gate (test commit): 24ae70f — 9 behavioral tests written, stubs return wrong values, all 9 fail
- GREEN gate (feat commit): 4b30498 — correct implementations, all 10 tests pass (9 behavioral + 1 serialization)
- REFACTOR: not needed — implementation was clean on first pass

## What Was Built

### Task 1 — health.rs rewrite

**Typed response structs** (`#[derive(Serialize)]`):
- `HealthResponse { status, version, uptime_seconds, cache: CacheHealth, cluster: ClusterHealth, last_refresh: LastRefreshHealth }`
- `CacheHealth { targets, age_seconds, state }`
- `ClusterHealth { mode, nodes, is_leader }`
- `LastRefreshHealth { status, timestamp: Option<u64> }`

**Pure status functions**:
- `determine_health_status(target_count, &Option<RefreshOutcome>) -> (&'static str, StatusCode)`:
  - populated + success → "healthy" 200
  - populated + failed/none → "degraded" 200
  - empty + failed → "starting" 503 (only 503 case, HEALTH-02)
  - empty + success/none → "starting" 200
- `determine_readiness_status(target_count) -> (&'static str, StatusCode)`:
  - > 0 → "ready" 200
  - == 0 → "not_ready" 503 (HEALTH-04)

**Handlers**:
- `health_handler`: reads snapshot (target_count, age_seconds), last_refresh_outcome, started_at, chitchat cluster topology; calls `determine_health_status`; returns structured HealthResponse
- `health_live_handler`: no State parameter, always returns `{"status":"alive"}` (HEALTH-03)
- `health_ready_handler`: single snapshot read, calls `determine_readiness_status`

**Inline tests (9 + 1 = 10 total)**:
- 6 branches of `determine_health_status`
- 2 branches of `determine_readiness_status`
- 1 `health_live_handler` body assertion
- 1 `HealthResponse` serialization shape test (HEALTH-01)

### Task 2 — routes/health.rs extension

Added two `.route()` calls to `routes()`:
- `/health/live` → `health::health_live_handler`
- `/health/ready` → `health::health_ready_handler`

Original `/health → health::health_handler` preserved unchanged.

## Verification Results

- `cargo test determine_health_status`: 6/6 passed (all status branches)
- `cargo test determine_readiness_status`: 2/2 passed
- `cargo test health_live`: 1/1 passed (body == `{"status":"alive"}`)
- `cargo test health`: 10/10 passed (all inline tests)
- `cargo build`: exit 0, no errors
- `cargo test`: 170/170 passed (full suite)
- Structural gates:
  - `rg -q 'fn health_live_handler' src/handlers/health.rs` = match
  - `rg -q 'fn health_ready_handler' src/handlers/health.rs` = match
  - `rg -q 'fn determine_health_status' src/handlers/health.rs` = match
  - `! rg -q 'health_response_contains_app_and_version' src/handlers/health.rs` = match (old test removed)
  - `rg -q '"/health/live"' src/routes/health.rs` = match
  - `rg -q '"/health/ready"' src/routes/health.rs` = match

## Deviations from Plan

None — plan executed exactly as written.

## Threat Surface Scan

| Flag | File | Description |
|------|------|-------------|
| T-11-02 (handled) | src/handlers/health.rs | Response serializes only counts, booleans, package version, and Unix-second timestamps. No ARNs, account IDs, cluster names, refresh tokens, or env values in HealthResponse or sub-objects — mitigation implemented as specified. |

No new threat surface introduced beyond what the plan's threat model documents.

## Known Stubs

None — all handlers are fully implemented.

## Self-Check: PASSED

Files verified:
- `src/handlers/health.rs` — FOUND (all three handlers + structs + pure fns + tests present)
- `src/routes/health.rs` — FOUND (/health/live and /health/ready routes present)

Commits verified:
- `24ae70f` — test(11-02): add failing tests (RED gate)
- `4b30498` — feat(11-02): rewrite health.rs (GREEN gate)
- `038b440` — feat(11-02): register /health/live and /health/ready routes
