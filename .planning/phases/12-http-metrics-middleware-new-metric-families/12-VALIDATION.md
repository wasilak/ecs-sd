---
phase: 12
slug: http-metrics-middleware-new-metric-families
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-11
---

# Phase 12 - Validation Strategy

> Per-phase validation contract for feedback sampling during gap-closure execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test app_state` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~2 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task-specific automated command listed below.
- **After every plan wave:** Run `cargo test`.
- **Before `/gsd-verify-work`:** `cargo build` and `cargo test` must be green.
- **Max feedback latency:** 10 seconds for targeted checks, 30 seconds for full suite.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 12-05-01 | 05 | 4 | MET-08 | T-12-05-01 / T-12-05-03 | HTTP metric labels remain bounded by matched path, method, and status; raw URI and `status_code` contract drift are avoided. | unit/contract | `cargo test metrics::tests::http_requests_total_uses_required_label_names && cargo test metrics::tests::http_requests_total_countervec_works` | yes | pending |
| 12-05-02 | 05 | 4 | MET-10 | T-12-05-02 | Cluster label set remains bounded while stale non-zero per-cluster gauges are reset to zero after successful refresh. | unit/regression | `cargo test app_state` | yes | pending |
| 12-05-03 | 05 | 4 | MET-14 | T-12-05-04 | Startup timing exposes only intended coarse process timing and no secrets or request data. | unit/regression | `cargo test require_region_errors_when_none && cargo test ttl_refresh_loop_uses_skip_missed_tick_behavior && cargo test app_state` | yes | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all Phase 12 gap-closure requirements. No Wave 0 setup is required.

---

## Manual-Only Verifications

All Phase 12 gap-closure behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references; none are present.
- [x] No watch-mode flags.
- [x] Feedback latency is below 30 seconds for the current suite.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-07-11
