# Phase 1 Discussion Log

**Phase:** 1 — Core Discovery & HTTP API  
**Date:** 2026-05-19  
**Purpose:** Extract implementation decisions for downstream agents  

---

## Areas Discussed

### 1. HTTP Server Structure

**Question:** Modular or single-file? Current code is single-file (~100 lines).

**Options presented:**
1. Single-file — Keep everything in `main.rs`
2. Minimal modules — Split into `main.rs`, `routes.rs`, `discovery.rs`
3. Full modular — `src/routes/`, `src/handlers/`, `src/models/`, `src/aws/`

**Decision:** Full modular structure (option 3)

**Rationale:** Phase 1 is foundational; 4 more phases will add features. Modular structure prevents refactoring later.

---

### 2. AWS Error Handling Strategy

**Question:** What happens when AWS API fails mid-discovery?

**Options presented:**
1. Fail fast — Abort entire discovery, return 500
2. Partial results — Log error, return targets from successful clusters
3. Staleness tolerance — Keep serving cached data
4. Per-cluster error tracking — Include failed clusters in metadata

**Decision:** Partial results (option 2)

**Rationale:** Prometheus treats non-200 as "no targets." Partial results keep scrapers working during partial outages.

---

### 3. Target Resolution Edge Cases

**Question:** How to handle edge cases in Task → EC2 → IP resolution chain?

| Scenario | Options | Decision |
|----------|---------|----------|
| Task has no container instance | A) Skip silently, B) Log warning | **A) Skip silently** |
| EC2 has no private IP | A) Skip, B) Use public IP | **A) Skip silently** |
| Container has no metrics label | A) Skip, B) Default port | **A) Skip silently** |
| Multiple containers with labels | A) Use first, B) Multiple targets | **B) Multiple targets** |
| STOPPED/STOPPING tasks | A) Skip, B) Include | **A) Skip silently** |

**Rationale:** Include everything scrapeable, exclude everything questionable. Multiple containers supports sidecar patterns.

---

### 4. Graceful Shutdown Behavior

**Question:** SIGTERM handling — how graceful?

**Options presented:**
1. Immediate — Close listener, drop in-flight requests
2. Timeout-based — Wait up to N seconds
3. Drain-then-close — Wait for in-flight to complete (unbounded)

**Decision:** Drain-then-close (option 3)

**Rationale:** Most graceful approach. Axum supports this natively.

---

### 5. Query Parameter Filtering Scope

**Question:** What level of filtering for Phase 1?

**Options presented:**
1. No filtering — defer to later phase
2. Exact match only — `?cluster=prod`
3. Single match parameter — `?match={label="value"}`
4. Full Prometheus matchers — `match[]` array support

**Decision:** Exact match, case-sensitive (option 2)

**Constraint:** Case-sensitive matching: `prod` ≠ `Prod` ≠ `PROD`

**Rationale:** Simple, explicit, covers common use cases. Complex matching can be added later.

---

### 6. Label Convention (User-initiated)

**User input:** "Use established Prometheus labels: `prometheus.io/scrape: true` and `prometheus.io/port: 8080`"

**Decision:** Use standard Prometheus labels instead of `metrics_port`

**Impact:**
- Changes DISC-04 from `metrics_port` label to `prometheus.io/scrape` + `prometheus.io/port`
- Aligns with industry standard
- Immediately recognizable to Prometheus operators

**Note:** This is a deviation from original requirements, documented in CONTEXT.md.

---

## Key Decisions Summary

| Area | Decision |
|------|----------|
| Code structure | Full modular: `routes/`, `handlers/`, `models/`, `aws/` |
| AWS errors | Partial results, log errors |
| Edge cases | Skip silently; multiple targets for multiple containers |
| Graceful shutdown | Drain-then-close |
| Query filtering | Case-sensitive exact match |
| Scrape labels | `prometheus.io/scrape` + `prometheus.io/port` |

---

## Deferred Ideas

- `match[]` Prometheus-style label matchers
- Regex filtering
- Public IP fallback
- Fargate support (v2 scope)
- Custom scrape paths
- TLS termination

---

## Next Steps

1. Researcher: Investigate Axum patterns, AWS SDK pagination, EC2 IP resolution
2. Planner: Create PLAN.md with modular structure, route handlers, AWS integration
3. Executor: Implement Phase 1 per plan

---

*Discussion log for audit trail. Decisions captured in 01-CONTEXT.md.*
