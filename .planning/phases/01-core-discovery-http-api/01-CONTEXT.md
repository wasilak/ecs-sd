# Phase 1 Context: Core Discovery & HTTP API

**Phase:** 1 — Core Discovery & HTTP API  
**Goal:** ECS task discovery working and HTTP endpoints serving valid Prometheus SD format  
**Discussed:** 2026-05-19  

---

## Domain

This phase delivers an HTTP server that provides Prometheus-compatible service discovery for AWS ECS tasks running on EC2 launch type. The server exposes `/health` and `/sd` endpoints, discovers ECS tasks with `prometheus.io/scrape: true` labels, and returns them as scrape targets.

---

## Canonical References

| File | Purpose |
|------|---------|
| `.planning/PROJECT.md` | Project overview and key decisions |
| `.planning/REQUIREMENTS.md` | Full requirements specification |
| `.planning/ROADMAP.md` | Phase boundaries and dependencies |
| `.planning/codebase/ARCHITECTURE.md` | Current codebase structure |
| `.planning/codebase/CONVENTIONS.md` | Rust code patterns in use |
| `.planning/codebase/STACK.md` | Technology stack (tokio, aws-sdk, etc.) |
| `src/main.rs` | Existing discovery prototype (100 lines) |

---

## Codebase Context

### Reusable Assets
- **AWS SDK pattern** — `aws_config::defaults()`, `Client::new(&config)`
- **Async runtime** — Tokio with full features already configured
- **Cluster discovery** — `describe_clusters()` call pattern exists
- **Task enumeration** — `list_tasks()` → `describe_tasks()` → `describe_task_definition()` chain

### Current Limitations
- Single-file architecture (`src/main.rs` only)
- Extensive `.unwrap()` usage (not production-ready)
- Hardcoded cluster list
- No HTTP server (CLI tool only)

### Integration Points
- Axum web framework (new dependency needed)
- Prometheus http_sd_configs format (JSON array)
- AWS credential chain (already configured)

---

## Decisions

### Architecture

**Decision:** Use full modular structure from Phase 1

```
src/
├── main.rs           # Entry point, server startup
├── routes/           # Axum route definitions
│   ├── mod.rs
│   ├── health.rs
│   └── sd.rs
├── handlers/         # Route handlers (business logic)
│   ├── mod.rs
│   ├── health.rs
│   └── sd.rs
├── models/           # Data structures
│   ├── mod.rs
│   ├── target.rs
│   └── discovery.rs
└── aws/              # AWS SDK wrappers
    ├── mod.rs
    ├── client.rs
    └── discovery.rs
```

**Rationale:** 4 more phases will add significant features. Modular structure prevents refactoring later.

---

### AWS Error Handling

**Decision:** Partial results with logging

- If AWS API fails for one cluster, log the error and continue
- Return targets from successfully queried clusters
- Never fail entire discovery due to one cluster error

**Rationale:** Prometheus treats any non-200 as "no targets." Partial results keep scrapers working during partial AWS outages.

---

### Target Resolution Edge Cases

| Scenario | Decision |
|----------|----------|
| Task has no container instance | Skip silently |
| EC2 DescribeInstances returns no private IP | Skip silently |
| Container has no `prometheus.io/scrape` label | Skip silently |
| **Multiple containers have scrape labels** | **Create multiple targets** (one per container) |
| Task status is STOPPED/STOPPING | Skip silently |

**Rationale:** Include everything scrapeable, exclude everything questionable. Multiple containers per task is valid for sidecar patterns.

---

### Graceful Shutdown

**Decision:** Drain-then-close

- On SIGTERM, stop accepting new connections
- Wait for in-flight requests to complete (unbounded)
- Then shut down

**Implementation:** Use Axum's `Server::with_graceful_shutdown()` with `tokio::signal::ctrl_c()`

---

### Query Parameter Filtering

**Decision:** Case-sensitive exact match

Format: `GET /sd?cluster=prod&service=api`

- Only exact string matches
- Case-sensitive: `prod` ≠ `Prod` ≠ `PROD`
- Multiple params combined with AND logic (all must match)
- Unknown params ignored

**Deferred:** Regex matching, Prometheus-style `match[]` arrays, negation operators — these can be added later if needed.

---

### Scrape Labels (⚠️ Deviation from Requirements)

**Decision:** Use standard Prometheus labels (NOT `metrics_port`)

| Docker Label | Value | Meaning |
|--------------|-------|---------|
| `prometheus.io/scrape` | `true` | Include this container in discovery |
| `prometheus.io/port` | `8080` | Port to scrape (required if scrape=true) |

**Why:** Industry-standard convention recognized by Prometheus operators. Two-label system is explicit and self-documenting.

**Impact on Requirements:**
- DISC-04 changes from "docker label `metrics_port`" to "docker labels `prometheus.io/scrape: true` and `prometheus.io/port`"
- DISC-06 address format becomes `EC2_IP:prometheus.io/port`

---

## HTTP Endpoints

### `GET /health`

**Response:**
```json
{"status":"healthy"}
```

**Status:** 200 OK

**Purpose:** Kubernetes/liveness probe compatibility

---

### `GET /sd`

**Response format:** Prometheus http_sd_configs

```json
[
  {
    "targets": ["10.0.1.5:8080"],
    "labels": {
      "__meta_ecs_cluster_name": "prod",
      "__meta_ecs_service_name": "api",
      "__meta_ecs_task_family": "api-task"
    }
  }
]
```

**Query params (exact match, case-sensitive):**
- `?cluster=<name>` — Filter by cluster name
- `?service=<name>` — Filter by service name
- `?family=<name>` — Filter by task definition family

---

## Deferred Ideas

| Idea | Reason Deferred |
|------|-----------------|
| `match[]` Prometheus-style label matchers | Can be added later without breaking changes |
| Regex filtering | Complex, most use cases covered by exact match |
| Public IP fallback for EC2 | VPC networking should always have private IP |
| Fargate support | Out of scope per PROJECT.md v1 |
| Custom scrape paths (prometheus.io/path) | Discovery only, paths handled by scraper |
| TLS termination | Run behind reverse proxy per PROJECT.md |

---

## Implementation Notes

### New Dependencies

Add to `Cargo.toml`:
```toml
[dependencies]
axum = "0.7"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tower = "0.4"
```

### AWS API Flow

1. **DescribeClusters** — Get cluster details for configured clusters
2. **ListServices** — Get service ARNs per cluster (paginated)
3. **DescribeServices** — Get service details
4. **ListTasks** — Get task ARNs per service (paginated)
5. **DescribeTasks** — Get task details (includes container instance ARN)
6. **DescribeContainerInstances** — Get EC2 instance ID
7. **DescribeInstances** (EC2 API) — Get private IP

**Note:** Steps 6-7 require cross-service calls (ECS → EC2). May need `aws-sdk-ec2`.

### Error Types

Use `thiserror` for custom error types:
- `DiscoveryError` — AWS API failures
- `ConfigError` — Missing/invalid configuration
- `ServerError` — HTTP server startup failures

### Testing Strategy

- Unit tests for label building logic
- Unit tests for target address resolution
- Integration test with mock AWS responses
- No AWS live calls in CI tests

---

## Success Criteria

From ROADMAP.md, verified against decisions:

| Criteria | Status |
|----------|--------|
| `GET /health` returns 200 OK | ✓ Defined |
| `GET /sd` returns valid Prometheus JSON | ✓ Format specified |
| Targets include only containers with `prometheus.io/scrape: true` | ✓ Label requirement defined |
| Address format is `EC2_IP:prometheus.io/port` | ✓ Port label defined |
| Graceful shutdown handles in-flight requests | ✓ Drain-then-close strategy |
| Query param filtering supported | ✓ Case-sensitive exact match |

---

*Context captured for researcher and planner use.*
