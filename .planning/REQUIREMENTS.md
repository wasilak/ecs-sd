# Requirements: ecs-sd (ECS Service Discovery)

**Defined:** 2026-05-19
**Core Value:** Instant visibility into ECS infrastructure — provide complete cluster-to-container introspection in a single command

## v1 Requirements

### Service Discovery

- [ ] **SD-01**: Tool automatically discovers all ECS services across specified clusters
- [ ] **SD-02**: Tool displays service-to-task mapping (which tasks belong to which service)
- [ ] **SD-03**: Tool discovers service endpoints (target groups, load balancers)

### Output Formats

- [ ] **OUT-01**: Tool supports JSON output format for programmatic consumption
- [ ] **OUT-02**: Tool supports YAML output format for configuration-style output
- [ ] **OUT-03**: Human-readable format remains default (current behavior preserved)

### Filtering & Search

- [ ] **FILT-01**: User can filter by service name (exact match and partial match)
- [ ] **FILT-02**: User can filter by task status (RUNNING, PENDING, STOPPED)
- [ ] **FILT-03**: User can filter by container image name/tag
- [ ] **FILT-04**: User can discover services across all clusters (not just specified list)

### Performance

- [ ] **PERF-01**: Tool caches AWS API responses for 60 seconds to reduce API calls
- [ ] **PERF-02**: Tool supports parallel AWS API calls where safe (clusters queried in parallel)

### Health & Networking

- [ ] **HEALTH-01**: Tool displays health check configuration for services
- [ ] **HEALTH-02**: Tool displays service connect proxy configuration if enabled

## v2 Requirements

### Extended Discovery

- **EXT-01**: Discover and display CloudWatch log groups for services
- **EXT-02**: Discover task networking details (ENI, security groups)
- **EXT-03**: Display capacity provider information

### Configuration Export

- **EXP-01**: Export discovered configuration as Terraform data source format
- **EXP-02**: Export discovered configuration as Kubernetes service definition hints

### Interactive Mode

- **INT-01**: Interactive TUI mode for browsing clusters/services
- **INT-02**: Live refresh mode (watch for changes)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Real-time metrics/monitoring | Use CloudWatch, Prometheus, or DataDog — outside discovery scope |
| Write operations (scale/update) | Safety constraint — tool is read-only introspection only |
| Multi-cloud (K8s, GKE, AKS) | AWS ECS focus keeps scope manageable |
| Web UI or API server | CLI tool only — avoids complexity of hosting |
| Cost analysis | Requires CloudWatch/ billing data access — defer |
| Container image scanning | Security scanning is separate concern — defer |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| SD-01 | Phase 1 | Pending |
| SD-02 | Phase 1 | Pending |
| SD-03 | Phase 1 | Pending |
| OUT-01 | Phase 2 | Pending |
| OUT-02 | Phase 2 | Pending |
| OUT-03 | Phase 2 | Pending |
| FILT-01 | Phase 2 | Pending |
| FILT-02 | Phase 2 | Pending |
| FILT-03 | Phase 3 | Pending |
| FILT-04 | Phase 3 | Pending |
| PERF-01 | Phase 3 | Pending |
| PERF-02 | Phase 3 | Pending |
| HEALTH-01 | Phase 4 | Pending |
| HEALTH-02 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 13 total
- Mapped to phases: 13
- Unmapped: 0 ✓

---
*Requirements defined: 2026-05-19*
*Last updated: 2026-05-19 after initialization*
