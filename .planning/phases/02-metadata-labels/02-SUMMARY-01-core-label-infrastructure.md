---
phase: 02-metadata-labels
plan: 01
subsystem: models
tags: [metadata, labels, infrastructure]
requires: []
provides: [MetadataLevel, LabelBuilder]
affects: [src/models/metadata_level.rs, src/models/label_builder.rs, Cargo.toml, src/error.rs]
tech-stack:
  added: [aws-sdk-sts, strum]
  removed: []
patterns:
  - Consuming builder pattern
  - Level-aware filtering
key-files:
  created:
    - src/models/metadata_level.rs
    - src/models/label_builder.rs
  modified:
    - Cargo.toml
    - src/error.rs
    - src/models/mod.rs
key-decisions:
  - MetadataLevel::Task as default (balanced visibility)
  - Consuming builder pattern for clean API
  - includes() method for hierarchy checking
  - All 14 label names defined centrally in build()
requirements-completed: [META-15]
duration: 8 min
completed: 2026-05-19T21:10:00Z
---

# Phase 2 Plan 1: Core Label Infrastructure Summary

**Infrastructure for level-aware metadata label system with 5 hierarchical levels.**

## What Was Built

Created the foundational types enabling configurable metadata discovery:

1. **MetadataLevel enum** (`src/models/metadata_level.rs`)
   - 5 variants: Container, Task, Service, Cluster, Aws
   - Implements Default (Task), FromStr (case-insensitive), Display
   - `includes()` method implements proper hierarchy (Aws includes all, Container includes only itself)
   - Comprehensive test coverage (6 tests)

2. **LabelBuilder struct** (`src/models/label_builder.rs`)
   - Consuming builder pattern: `new(level) -> with_*() -> build()`
   - 5 data structs for each metadata level
   - `with_container()`, `with_task()`, `with_service()`, `with_cluster()`, `with_aws()` methods
   - `build()` returns HashMap with all 14 label names (META-01..14)
   - Level-aware filtering: only includes labels where `level.includes(target_level)`

3. **Dependencies** (`Cargo.toml`)
   - `aws-sdk-sts = "1.103"` — for account ID lookup
   - `strum = { version = "0.28", features = ["derive"] }` — for EnumString/Display derives

4. **Error handling** (`src/error.rs`)
   - Added `StsError(String)` variant for STS API errors

## Deviations from Plan

None — plan executed exactly as written.

## Verification Results

```
✓ cargo check passes (0 errors)
✓ MetadataLevel enum with all 5 variants
✓ Default level is Task
✓ FromStr parses case-insensitively
✓ includes() hierarchy correct
✓ LabelBuilder has all 5 with_* methods
✓ build() defines all 14 label names
✓ Module exports allow importing MetadataLevel and LabelBuilder
```

## Implementation Notes

- **Hierarchy logic**: Hardcoded match arms for clarity and performance
- **Builder pattern**: Consuming (takes `mut self` -> `Self`) for clean chaining
- **Label naming**: All 14 Prometheus-style labels defined in one place (`build()` method)
- **Missing data handling**: Labels omitted entirely when data unavailable (Option checks)

## Next Steps

Ready for Plan 02: Label Implementation — uses LabelBuilder in DiscoveryService to populate all 14 labels from AWS SDK objects.

---

**Commits:**
- `feat(phase-2-plan-01): add core label infrastructure` — All 5 tasks
