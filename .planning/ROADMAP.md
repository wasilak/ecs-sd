# Roadmap: ecs-sd (ECS Service Discovery)

**Project:** ecs-sd — AWS ECS Service Discovery CLI Tool  
**Created:** 2026-05-19  
**Mode:** Standard (Horizontal Layers)  

---

## Overview

| Phase | Name | Goal | Requirements | Est. Effort |
|-------|------|------|--------------|-------------|
| 1 | Core Service Discovery | Discover ECS services and their relationships | SD-01, SD-02, SD-03 | Medium |
| 2 | Output Formats & Basic Filtering | Enable programmatic use with JSON/YAML and simple filters | OUT-01, OUT-02, OUT-03, FILT-01, FILT-02 | Medium |
| 3 | Advanced Filtering & Performance | Image filtering, cluster auto-discovery, caching, parallelization | FILT-03, FILT-04, PERF-01, PERF-02 | Medium |
| 4 | Health & Networking Discovery | Service health checks and Service Connect proxy config | HEALTH-01, HEALTH-02 | Small |

---

## Phase 1: Core Service Discovery

**Goal:** Implement foundational service discovery — tool can discover ECS services and map them to tasks

**Requirements:**
- SD-01: Automatically discover all ECS services across clusters
- SD-02: Display service-to-task mapping
- SD-03: Discover service endpoints (target groups, load balancers)

**Success Criteria:**
1. User can run `ecs-sd services` and see all services across clusters
2. Output shows service name, cluster, desired/running count, and associated tasks
3. Service endpoints (load balancer DNS, target group ARNs) are displayed
4. Works with existing cluster discovery (replaces hardcoded list with dynamic discovery)

**Dependencies:**
- AWS SDK ECS client (already exists)
- ListServices, DescribeServices API calls
- ELBv2 client for load balancer discovery (DescribeTargetGroups, DescribeLoadBalancers)

**Notes:**
- Keep existing cluster/task functionality intact
- Service discovery is new functionality, not replacement

---

## Phase 2: Output Formats & Basic Filtering

**Goal:** Make output machine-readable and filterable for integration with other tools

**Requirements:**
- OUT-01: JSON output format
- OUT-02: YAML output format
- OUT-03: Human-readable default preserved
- FILT-01: Filter by service name (exact/partial)
- FILT-02: Filter by task status

**Success Criteria:**
1. `--format json` produces valid JSON output
2. `--format yaml` produces valid YAML output
3. `--service my-service` filters to matching services
4. `--status RUNNING` filters tasks by status
5. Default (no flags) maintains current human-readable output

**Dependencies:**
- Phase 1 complete (service discovery working)
- serde for serialization
- clap for argument parsing (if not already present)

**Notes:**
- Consider structured output schemas for stability
- JSON output should be pipeable to jq

---

## Phase 3: Advanced Filtering & Performance

**Goal:** Enable flexible discovery patterns and reduce AWS API overhead

**Requirements:**
- FILT-03: Filter by container image name/tag
- FILT-04: Discover services across all clusters (no explicit list)
- PERF-01: Cache AWS API responses (60s TTL)
- PERF-02: Parallel API calls for clusters

**Success Criteria:**
1. `--image nginx:latest` filters services by container image
2. `--all-clusters` discovers across all accessible ECS clusters
3. Second run within 60 seconds uses cached data (no AWS calls)
4. Multiple clusters queried in parallel (improved latency)

**Dependencies:**
- Phase 2 complete
- Caching library (e.g., cached crate)
- Tokio for parallel execution (already available)

**Notes:**
- Cache location: temp dir or XDG cache
- Parallelism must respect AWS API rate limits
- Consider exponential backoff for throttling

---

## Phase 4: Health & Networking Discovery

**Goal:** Surface health check and service mesh configuration

**Requirements:**
- HEALTH-01: Display health check configuration
- HEALTH-02: Display Service Connect proxy configuration

**Success Criteria:**
1. Health check path, interval, timeout visible per service
2. Service Connect namespace and DNS name displayed if enabled
3. Proxy configuration (App Mesh or ECS Service Connect) shown

**Dependencies:**
- Phase 3 complete
- Health check data from target groups
- Service Connect API understanding

**Notes:**
- Service Connect is newer ECS feature — verify API availability
- Health checks may require ELB + ECS API coordination

---

## Milestone: v1.0 Release

**Trigger:** All 4 phases complete

**Definition of Done:**
- All v1 requirements implemented and tested
- CLI help documentation complete
- README with usage examples
- Published to crates.io (optional but recommended)

**Post-Milestone:**
- Move v2 requirements into Active
- Define v2 phases based on user feedback

---

## State Tracking

| Phase | Status | Requirements Complete | Plans Created |
|-------|--------|----------------------|---------------|
| 1 | ○ Not Started | 0/3 | 0/? |
| 2 | ○ Not Started | 0/5 | 0/? |
| 3 | ○ Not Started | 0/4 | 0/? |
| 4 | ○ Not Started | 0/2 | 0/? |

**Last updated:** 2026-05-19

---

*See STATE.md for current execution state*
