---
phase: 12
slug: http-metrics-middleware-new-metric-families
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-07-11
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo build 2>&1 | tail -5` |
| **Full suite command** | `cargo test 2>&1 | tail -20` |
| **Estimated runtime** | ~12 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build 2>&1 | tail -5`
- **After every plan wave:** Run `cargo test 2>&1 | tail -20`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 12 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 12-01-T1 | 01 | 1 | MET-08..14 | — | N/A | build + rg | `cargo build && rg -c 'registry.register' src/metrics/mod.rs` | ✅ | ⬜ pending |
| 12-01-T2 | 01 | 1 | MET-08..14 | — | N/A | unit | `cargo test metrics` | ✅ | ⬜ pending |
| 12-02-T1 | 02 | 2 | MET-08, MET-09 | T-12-02 | No raw URI label values (MatchedPath only) | build | `cargo build` | ❌ new file | ⬜ pending |
| 12-02-T2 | 02 | 2 | MET-08, MET-09 | T-12-02 | route_layer prevents pre-routing middleware | build + rg | `cargo test && rg 'route_layer' src/routes/mod.rs` | ✅ | ⬜ pending |
| 12-03-T1 | 03 | 2 | MET-12 | T-12-05 | Read-lock released before write-lock | build + rg | `cargo build && rg -c '\.send\(\)' src/aws/discovery.rs` | ✅ | ⬜ pending |
| 12-03-T2 | 03 | 2 | MET-10, MET-11, MET-12 | T-12-05 | Deadlock-safe: read scope ends before write begins | unit | `cargo test app_state && cargo test` | ✅ | ⬜ pending |
| 12-04-T1 | 04 | 3 | MET-10, MET-11, MET-13, MET-14 | — | startup_duration set exactly once, not in refresh loop | build + rg | `cargo build && rg -c 'startup_duration_seconds' src/main.rs` | ✅ | ⬜ pending |
| 12-04-T2 | 04 | 3 | MET-10, MET-11 | — | N/A | build + rg | `cargo test && rg 'replace_cache_and_record_metrics' src/handlers/sd.rs` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — no new test framework or stubs needed.
Rust's `cargo test` is the test harness. No Wave 0 setup required.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (none — existing infra)
- [x] No watch-mode flags
- [x] Feedback latency < 12s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
