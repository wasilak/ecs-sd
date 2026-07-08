---
phase: 11
slug: rich-health-endpoint-k8s-probes
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-07
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo test health` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test health`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 11-01-01 | 01 | 1 | HEALTH-01 | — | N/A | unit | `cargo test determine_health_status` | ❌ W0 | ⬜ pending |
| 11-01-02 | 01 | 1 | HEALTH-02 | — | 503 on empty+failed only | unit | `cargo test health_503` | ❌ W0 | ⬜ pending |
| 11-02-01 | 02 | 2 | HEALTH-03 | — | N/A | unit | `cargo test health_live` | ❌ W0 | ⬜ pending |
| 11-02-02 | 02 | 2 | HEALTH-04 | — | N/A | unit | `cargo test health_ready` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/handlers/health.rs` — unit test stubs for `determine_health_status()` covering all status branches
- [ ] Tests for HEALTH-03 (`/health/live` always 200) and HEALTH-04 (`/health/ready` 200/503)

*Existing infrastructure: `cargo test` covers all phases.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live/ready endpoint smoke test | HEALTH-03, HEALTH-04 | Requires running instance | `curl http://localhost:PORT/health/live` returns 200; `curl http://localhost:PORT/health/ready` returns 200 when cache populated |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
