---
plan: 09-03
phase: 09-cachesnapshot-refactor-module-cleanup
status: complete
wave: 3
completed: 2026-07-06
requirements: [QUAL-02]
key-files:
  created: []
  modified:
    - src/error.rs
    - src/aws/discovery.rs
    - src/handlers/sd.rs
    - src/main.rs
---

## What Was Built

Changed `discover_all_clusters` to return `Result<Vec<Target>, DiscoveryError>` so callers can distinguish total AWS failure from partial success. On total failure, stale cache is preserved — transient AWS API failures can never blank the in-memory target set.

## Changes Made

**src/error.rs:**
- Added `AllClustersFailed` variant to `DiscoveryError` for the case where every configured cluster fails

**src/aws/discovery.rs:**
- Added pure `fn aggregate_cluster_results(per_cluster: Vec<Result<...>>) -> Result<Vec<Target>, DiscoveryError>` helper
  - Partial success (≥1 cluster OK) → `Ok(combined_targets)`
  - Total failure (all clusters error) → `Err(AllClustersFailed)`
- Changed `discover_all_clusters` signature from `-> Vec<Target>` to `-> Result<Vec<Target>, DiscoveryError>`
- Added 2 unit tests: `discover_all_clusters_returns_err_when_all_clusters_fail` and `discover_all_clusters_returns_partial_ok_when_some_clusters_fail`

**src/handlers/sd.rs:**
- `refresh_handler` now matches on the `Result`: returns HTTP 503 with `{"error": "all clusters failed"}` on `Err(AllClustersFailed)`; existing success path unchanged
- Added `warn` to tracing import

**src/main.rs:**
- Initial discovery block matches on `Result`: skips `replace_cache_and_routing` on error (starts with empty cache, warns)
- `refresh_cache_once` uses `.map_err(...)?.` to propagate `AllClustersFailed` as a `String` error (background TTL loop already logs and continues)

## Verification

- `cargo test`: 159/159 passed (157 + 2 new aggregate tests)
- Zero build warnings
- `rg "discover_all_clusters.*await" src/ --type rust | grep -v discovery.rs` → all callers handle `Result`

## Deviations

The executor ran from the wrong worktree base (pre-wave-2 code), causing merge conflicts with wave 2's snapshot consolidation. The implementation was applied inline to main after discarding the conflicting worktree branch. Logic is identical to the executor's implementation.

## Self-Check: PASSED
