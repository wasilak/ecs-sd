---
plan: 09-02
phase: 09-cachesnapshot-refactor-module-cleanup
status: complete
wave: 2
completed: 2026-07-06
requirements: [QUAL-01, QUAL-06]
key-files:
  created:
    - src/state/app_state.rs (CacheSnapshot struct + build_snapshot + consolidated AppState)
  modified:
    - src/handlers/sd.rs
    - src/handlers/metrics.rs
    - src/handlers/proxy.rs
    - src/main.rs
    - src/state/mod.rs
---

## What Was Built

Consolidated the three independently-locked `AppState` fields (`cache`, `last_refresh`, `routing_table`) into a single `Arc<RwLock<CacheSnapshot>>`. Every read site now acquires one lock and reads all three fields from the same snapshot generation, eliminating torn reads.

## Changes Made

**Task 1 — CacheSnapshot + build_snapshot + AppState consolidation (src/state/app_state.rs):**
- Added `pub struct CacheSnapshot { cache, last_refresh, routing_table }` with `Clone` and `Default`
- Added pure `fn build_snapshot(targets_aws, mode) -> CacheSnapshot` that builds all 5 cache tiers atomically before lock acquisition (Pitfall 1 compliance)
- Replaced three `Arc<RwLock<…>>` fields with `pub snapshot: Arc<RwLock<CacheSnapshot>>`
- Changed `last_manual_refresh_request` from `Arc<RwLock<SystemTime>>` to `Arc<AtomicU64>` epoch seconds (STATE.md Decision #5)
- Removed `migrate_target_label_schema` and its two tests (QUAL-06)
- Added `build_snapshot_produces_consistent_tiers` test
- Re-exported `CacheSnapshot` from `src/state/mod.rs`

**Task 2 — sd.rs single snapshot read + AtomicU64 rate limit (src/handlers/sd.rs):**
- Proxy path: `state.routing_table.read()` + `state.last_refresh.read()` → single `state.snapshot.read()`
- Non-proxy path: `state.cache.read()` + `state.last_refresh.read()` → single `state.snapshot.read()`
- `refresh_retry_after_seconds` signature changed to `(last_request_secs: u64, now_secs: u64, min_interval: u64)`
- Rate-limit check now reads `state.last_manual_refresh_request.load(Ordering::SeqCst)` and writes `.store()`

**Task 3 — remaining read sites (src/handlers/metrics.rs, proxy.rs, src/main.rs):**
- `metrics.rs`: `state.last_refresh.read()` → `state.snapshot.read()` with immediate drop
- `proxy.rs`: `state.routing_table.read()` → `state.snapshot.read()` (lock still released before HTTP call)
- `main.rs` `publish_cache_to_gossip`: two separate reads merged into single `state.snapshot.read()` (Pitfall 2 — gossip now publishes cache and routing_table from same generation)

## Verification

- `rg "state\.cache\.read|state\.routing_table\.read|state\.last_refresh\.read" src/` → no matches (QUAL-01 ✓)
- `rg "migrate_target_label_schema" src/` → no matches (QUAL-06 ✓)
- `cargo test`: 157/157 passed (158 baseline − 2 deleted migrate tests + 1 new consistency test)

## Deviations

None. Note: the quota-exceeded interruption mid-execution required the orchestrator to complete Tasks 2 and 3 inline after merging the Task 1 commit from the worktree.

## Self-Check: PASSED
