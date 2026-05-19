---
plan_id: 03-discovery-logic
phase: 1
wave: 2
status: complete
commit_range: HEAD~9..HEAD
---

# Plan 03: Discovery Logic — Summary

## What Was Built

Implemented the complete AWS ECS discovery chain that queries AWS APIs and builds Prometheus-compatible targets.

### Key Components Created

1. **DiscoveryService** (`src/aws/discovery.rs`)
   - `discover_all_clusters()`: Iterates through configured clusters, aggregates targets
   - `discover_cluster_targets()`: Full discovery chain for a single cluster
   - `list_all_services()`: Paginated service listing (max 10 per request)
   - `list_service_tasks()`: Paginated task listing (max 100 per request)
   - `resolve_target_address()`: Maps container instance → EC2 instance → private IP

2. **AWS API Chain** (8 API calls in sequence)
   - `DescribeClusters`: Validate cluster exists
   - `ListServices`: Get all service ARNs (paginated)
   - `DescribeServices`: Get service details (batched by 10)
   - `ListTasks`: Get task ARNs per service (paginated)
   - `DescribeTasks`: Get task details (batched by 100)
   - `DescribeTaskDefinition`: Extract docker labels
   - `DescribeContainerInstances`: Map to EC2 instance ID
   - `DescribeInstances`: Get private IP address

3. **Target Selection Logic**
   - Filters for EC2 launch type only (excludes Fargate)
   - Skips STOPPED/STOPPING tasks
   - Requires `prometheus.io/scrape=true` docker label
   - Extracts port from `prometheus.io/port` label
   - Target format: `EC2_PRIVATE_IP:PORT`

4. **Labels Applied**
   - `__meta_ecs_cluster_name`: Cluster name
   - `__meta_ecs_service_name`: Service name
   - `__meta_ecs_task_family`: Task definition family

5. **Error Handling Strategy**
   - Partial results: One cluster failure doesn't block others
   - Per-task error handling: Logs warning, continues to next task
   - String-based error messages for AWS SDK compatibility

6. **Initial Discovery** (`src/main.rs`)
   - Performs discovery on server startup
   - Writes results to cache before starting HTTP server

7. **Manual Refresh** (`src/handlers/sd.rs`, `src/routes/sd.rs`)
   - `POST /sd/refresh`: Triggers discovery refresh
   - Returns `{"status":"ok","targets_discovered":N}`

8. **State Refactoring** (`src/state/app_state.rs`)
   - Replaced raw ECS/EC2 clients with DiscoveryService
   - DiscoveryService derives Clone for AppState Clone

## Commits

1. `feat(phase-1-03): implement DiscoveryService with full ECS discovery chain`
2. `feat(phase-1-03): export DiscoveryService from aws module`
3. `feat(phase-1-03): update AppState to use DiscoveryService`
4. `feat(phase-1-03): add initial discovery on server startup`
5. `feat(phase-1-03): add manual refresh endpoint for discovery`
6. `feat(phase-1-03): add POST /sd/refresh route for manual refresh`
7. `fix(phase-1-03): resolve error handling for AWS SDK compatibility`

## Self-Check: PASSED

- [x] `cargo check` passes with no errors
- [x] `cargo build` compiles successfully
- [x] `cargo test` passes (filter tests still work)
- [x] Server starts and performs initial discovery (logs show progress)
- [x] `GET /health` returns healthy status
- [x] `GET /sd` returns targets from cache
- [x] `POST /sd/refresh` triggers manual refresh
- [x] Query param filtering works with discovered targets

## Technical Details

### AWS API Pagination
- Services: 10 per page (ECS limit)
- Tasks: 100 per page (ECS limit)

### Batching
- DescribeServices: 10 ARNs per call (ECS limit)
- DescribeTasks: 100 ARNs per call (ECS limit)

### Performance Considerations
- Synchronous discovery on startup (blocking)
- Cache shared across all requests via RwLock
- Clone-on-read pattern for cache access

### AWS Credentials
- Uses default credential chain
- Region from AWS_REGION or defaults to us-east-1

## Notes

- Discovery runs synchronously on startup — may delay server start on large clusters
- No background refresh yet (Phase 3 will add caching with background refresh)
- EC2-only: Fargate tasks are skipped with debug log
- Requires AWS credentials with ECS and EC2 read permissions
