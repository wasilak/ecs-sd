# ecs-sd: ECS Service Discovery Tool

## What This Is

A Rust-based CLI tool for discovering and inspecting AWS ECS (Elastic Container Service) resources. It queries ECS clusters, lists tasks running in those clusters, and displays detailed container configurations including environment variables. Built for DevOps engineers who need quick visibility into containerized infrastructure without navigating the AWS Console.

## Core Value

**Instant visibility into ECS infrastructure** — provide complete cluster-to-container introspection in a single command, without context switching to AWS Console or writing custom scripts.

## Requirements

### Validated

(Existing codebase capabilities — mapped and functional)

- ✓ **ECS-01**: Tool can authenticate with AWS using default credential chain
- ✓ **ECS-02**: Tool can discover and display ECS cluster information
- ✓ **ECS-03**: Tool can list tasks running in specified clusters
- ✓ **ECS-04**: Tool can retrieve and display container definitions
- ✓ **ECS-05**: Tool can display environment variables from container configurations

### Active

(Current scope being built toward)

- [ ] **SD-01**: Implement service discovery — automatically discover all ECS services across clusters
- [ ] **SD-02**: Add structured output formats (JSON, YAML) for programmatic consumption
- [ ] **SD-03**: Support filtering by service name, task status, or container image
- [ ] **SD-04**: Add health check endpoint discovery for load balancer integration
- [ ] **SD-05**: Cache discovery results to reduce AWS API calls
- [ ] **SD-06**: Add support for ECS service connect proxy configuration discovery

### Out of Scope

- Real-time monitoring or metrics collection — use CloudWatch or Prometheus
- Write operations (scaling, updating services) — read-only introspection only
- Multi-cloud support — AWS ECS only
- Web UI or API server — CLI tool only

## Context

**Technical Environment:**
- AWS infrastructure with ECS clusters
- Standard AWS credential chain (env vars, profiles, IAM roles)
- Rust 2024 edition with modern async patterns
- AWS SDK v1.x for Rust

**Prior Work:**
- Basic CLI structure already functional
- Can list clusters and tasks with container details
- Uses sequential async/await pattern with Tokio

**Known Issues to Address:**
- Hardcoded cluster list ("blog", "dev") needs to be dynamic
- `unwrap()` usage for prototyping needs proper error handling
- No output formatting options (only human-readable stdout)
- No caching — every run hits AWS APIs

## Constraints

- **Tech stack**: Rust (must stay idiomatic Rust for performance and safety)
- **AWS SDK**: Must use official AWS SDK for Rust (aws-sdk-ecs)
- **Async runtime**: Tokio (already established, don't change)
- **Read-only**: Tool must remain read-only (no state changes to AWS)
- **CLI-only**: No web server, no daemon, no API — command-line tool only

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Keep read-only scope | Safety first — no risk of accidental infrastructure changes | — Pending validation |
| JSON output as v1 priority | Enables integration with other tools (jq, scripts, CI/CD) | — Pending |
| Support service discovery natively | Core value is service discovery — can't rely on external tools | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-19 after initialization*
