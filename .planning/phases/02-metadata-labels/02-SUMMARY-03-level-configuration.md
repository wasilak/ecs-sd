---
phase: 02-metadata-labels
plan: 03
subsystem: handlers
tags: [cache, configuration, query-params, multi-tier]
requires: [02-PLAN-02-label-implementation.md]
provides: [Multi-tier cache, SdQueryParams, level-based filtering]
affects: [src/config.rs, src/models/mod.rs, src/state/app_state.rs, src/handlers/sd.rs, src/main.rs]
tech-stack:
  added: []
  removed: []
patterns:
  - Multi-tier caching
  - Query parameter parsing with defaults
  - Label filtering by level
key-files:
  created:
  modified:
    - src/config.rs
    - src/models/mod.rs
    - src/state/app_state.rs
    - src/handlers/sd.rs
    - src/main.rs
key-decisions:
  - HashMap<MetadataLevel, Vec<Target>> for multi-tier cache
  - Single discovery at Aws level, derive lower tiers by filtering
  - Query param ?level= overrides config default
  - Default level is Task (MetadataLevel::default())
requirements-completed: [META-15, META-16]
duration: 15 min
completed: 2026-05-19T21:37:00Z
---

# Phase 2 Plan 3: Level Configuration Summary

**Implemented metadata level configuration with CLI flag support, query parameter override, and multi-tier caching.**

## What Was Built

1. **Config with MetadataLevel** (`src/config.rs`)
   - `metadata_level` field changed from `String` to `MetadataLevel` enum
   - Default is `MetadataLevel::default()` (Task level)
   - Clean type-safe configuration

2. **SdQueryParams** (`src/models/mod.rs`)
   - New struct with `cluster`, `service`, `family`, `level` fields
   - `#[serde(default)]` on level field for optional query param
   - Level parses via MetadataLevel::from_str (case-insensitive)
   - FilterParams kept for backward compatibility

3. **Multi-Tier Cache** (`src/state/app_state.rs`)
   - Cache type: `Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>`
   - Stores targets for all 5 levels (Container, Task, Service, Cluster, Aws)
   - HashMap keyed by MetadataLevel for O(1) lookup

4. **Level-Based Handler** (`src/handlers/sd.rs`)
   - `sd_handler` uses SdQueryParams with level field
   - Reads from cache using `params.level` as key
   - Returns empty vec if level not yet populated
   - Maintains cluster/service/family filtering

5. **Multi-Tier Refresh** (`src/handlers/sd.rs`)
   - `refresh_handler` discovers once at Aws level
   - Derives all 5 tiers using `filter_labels_by_level()`
   - Single write lock for atomic cache update
   - Each tier contains only labels appropriate for that level

6. **Label Filtering** (`filter_labels_by_level`)
   - Determines label level by prefix matching:
     - `__meta_ecs_container_*` or `__meta_ecs_metrics_port` → Container
     - `__meta_ecs_task_*` → Task
     - `__meta_ecs_service_*` → Service
     - `__meta_ecs_cluster_*` → Cluster
     - `__meta_ecs_*` → Aws
   - Uses `MetadataLevel::includes()` for hierarchy

7. **Startup Population** (`src/main.rs`)
   - Initial discovery populates all 5 cache tiers
   - Same filtering logic as refresh_handler
   - Server starts with warm cache at all levels

## Deviations from Plan

None — plan executed exactly as written.

## Verification Results

```
✓ cargo build passes (0 errors, 6 warnings)
✓ cargo test passes (11 tests)
✓ Config.metadata_level is MetadataLevel type
✓ SdQueryParams has level field with serde(default)
✓ AppState.cache is HashMap<MetadataLevel, Vec<Target>>
✓ sd_handler reads from cache by level
✓ refresh_handler populates all 5 cache tiers
✓ filter_labels_by_level correctly filters by prefix
```

## API Usage

```bash
# Get targets with default level (Task)
curl "http://localhost:8080/sd"

# Get targets with specific level
curl "http://localhost:8080/sd?level=container"
curl "http://localhost:8080/sd?level=task"
curl "http://localhost:8080/sd?level=service"
curl "http://localhost:8080/sd?level=cluster"
curl "http://localhost:8080/sd?level=aws"

# Combined with filters
curl "http://localhost:8080/sd?level=aws&cluster=prod"

# Refresh all cache tiers
curl -X POST "http://localhost:8080/refresh"
```

## Cache Levels

| Level | Labels Included |
|-------|-----------------|
| Container | __meta_ecs_container_name, __meta_ecs_container_image, __meta_ecs_metrics_port |
| Task | + __meta_ecs_task_arn, __meta_ecs_task_family, __meta_ecs_task_version |
| Service | + __meta_ecs_service_name, __meta_ecs_desired_count, __meta_ecs_running_count |
| Cluster | + __meta_ecs_cluster_name, __meta_ecs_cluster_arn |
| Aws | + __meta_ecs_region, __meta_ecs_account_id, __meta_ecs_availability_zone |

## Performance

- **Discovery**: Single AWS API call at Aws level
- **Cache lookup**: O(1) HashMap access by MetadataLevel
- **Memory**: 5x storage for 5x query flexibility
- **Refresh**: Atomic update of all tiers in single write lock

## Security Considerations

- Invalid level strings return 400 Bad Request (via serde validation)
- Empty cache returns 200 OK with `[]` (legitimate "no targets" state)
- Cache isolation by level prevents cross-tier data leakage

## Next Steps

Phase 2 is complete. Phase 3 (Caching & Configuration) will add:
- Stale-while-revalidate cache refresh
- Background refresh interval
- Cache metrics and health checks

---

**Commits:**
- `feat(phase-2-plan-03): implement metadata level configuration with multi-tier cache` — All 7 tasks
