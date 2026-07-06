---
phase: 09-cachesnapshot-refactor-module-cleanup
plan: "01"
subsystem: models/state
tags: [refactor, module-layering, QUAL-05]
dependency_graph:
  requires: []
  provides: [models/label_filter.rs, filter_labels_by_level-in-models]
  affects: [src/handlers/sd.rs, src/state/app_state.rs, src/models/mod.rs]
tech_stack:
  added: []
  patterns: [move-pure-function-to-models-layer, re-export-from-mod.rs]
key_files:
  created:
    - src/models/label_filter.rs
  modified:
    - src/models/mod.rs
    - src/handlers/sd.rs
    - src/state/app_state.rs
decisions:
  - "Changed visibility from pub(crate) to pub so state layer can call filter_labels_by_level"
  - "Co-located the test with the function in label_filter.rs (test follows code)"
  - "Moved MetadataLevel import to test module in sd.rs to eliminate unused-import warning"
metrics:
  duration_minutes: 8
  completed_date: "2026-07-06"
  tasks_completed: 2
  files_modified: 4
---

# Phase 09 Plan 01: filter_labels_by_level Module Move Summary

Move `filter_labels_by_level` from the handler layer to the models layer, eliminating the inverted dependency where `src/state/app_state.rs` imported from `src/handlers/sd.rs`.

## What Was Built

Relocated `filter_labels_by_level` (a pure label-filtering function) from `src/handlers/sd.rs` into a new `src/models/label_filter.rs` module. Changed visibility from `pub(crate)` to `pub`, re-exported from `src/models/mod.rs`, and updated all import sites. The state layer now imports the function from `crate::models`, satisfying QUAL-05.

## Tasks Completed

| Task | Description | Commit | Files |
|------|-------------|--------|-------|
| 1 | Create src/models/label_filter.rs + re-export | 45f1700 | src/models/label_filter.rs, src/models/mod.rs |
| 2 | Rewire consumers to import from models | 673cd89 | src/handlers/sd.rs, src/state/app_state.rs |
| 2-fix | Remove unused imports introduced by move | 63bc051 | src/handlers/sd.rs |

## Verification Results

- `cargo build` exits 0 with zero warnings after all changes
- `cargo test` passes all 158 tests (no regressions)
- `rg "use crate::handlers::sd" src/state/` returns no matches (QUAL-05 satisfied)
- `rg "fn filter_labels_by_level" src/handlers/sd.rs` returns no matches
- `rg "pub fn filter_labels_by_level" src/models/label_filter.rs` matches

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused imports in sd.rs introduced by function deletion**
- **Found during:** Task 2 verification
- **Issue:** Removing `filter_labels_by_level` from `src/handlers/sd.rs` left `MetadataLevel` and `std::collections::HashMap` as unused imports in the outer module scope. Both were used by the deleted function body.
- **Fix:** Removed both from the outer-scope imports; moved `MetadataLevel` to the test module import (`use crate::models::{LabelBuilder, MetadataLevel};`). `HashMap` was already imported inside the test module.
- **Files modified:** src/handlers/sd.rs
- **Commit:** 63bc051

## Known Stubs

None.

## Threat Flags

None — pure internal refactor, no new HTTP surface, no external I/O.

## Self-Check: PASSED

- [x] src/models/label_filter.rs exists
- [x] src/models/mod.rs contains pub use label_filter::filter_labels_by_level
- [x] src/handlers/sd.rs no longer defines filter_labels_by_level
- [x] src/state/app_state.rs imports from crate::models only
- [x] All 3 task commits exist (45f1700, 673cd89, 63bc051)
- [x] 158 tests pass, 0 warnings
