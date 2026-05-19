---
phase: 2
slug: metadata-labels
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-19
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in Rust testing) |
| **Config file** | none — uses Cargo.toml defaults |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~5-10 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 1 | META-15 | — | CLI parses --metadata-level correctly | unit | `cargo test --lib config::` | ⬜ W0 | ⬜ pending |
| 2-01-02 | 01 | 1 | META-01..03 | — | Container labels built correctly | unit | `cargo test --lib label_builder::container` | ⬜ W0 | ⬜ pending |
| 2-01-03 | 01 | 1 | META-04..06 | — | Task labels built correctly | unit | `cargo test --lib label_builder::task` | ⬜ W0 | ⬜ pending |
| 2-02-01 | 02 | 1 | META-07..09 | — | Service labels built correctly | unit | `cargo test --lib label_builder::service` | ⬜ W0 | ⬜ pending |
| 2-02-02 | 02 | 1 | META-10..11 | — | Cluster labels built correctly | unit | `cargo test --lib label_builder::cluster` | ⬜ W0 | ⬜ pending |
| 2-02-03 | 02 | 1 | META-12..14 | — | AWS metadata extracted correctly | unit | `cargo test --lib label_builder::aws` | ⬜ W0 | ⬜ pending |
| 2-03-01 | 03 | 2 | META-16 | — | Query param ?level= parsed and validated | unit | `cargo test --lib handlers::level_param` | ⬜ W0 | ⬜ pending |
| 2-03-02 | 03 | 2 | META-16 | — | Per-request level override filters labels | unit | `cargo test --lib handlers::level_filter` | ⬜ W0 | ⬜ pending |
| 2-03-03 | 03 | 2 | META-15,16 | — | Multi-tier cache stores all levels | integration | `cargo test --test integration cache_levels` | ⬜ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/models/label_builder.rs` — created with MetadataLevel enum
- [ ] `src/models/metadata_level.rs` — enum definition with FromStr/Display
- [ ] `tests/label_builder_tests.rs` — unit test stubs for all label types
- [ ] `aws-sdk-sts` dependency added to Cargo.toml
- [ ] `strum` dependency added to Cargo.toml for enum derives

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| STS GetCallerIdentity IAM permissions | META-13 | Requires AWS credentials | 1. Deploy to ECS with task role<br>2. Verify account_id label populated |
| AZ extraction from EC2 | META-14 | Requires live EC2 instance | 1. Run against real ECS cluster<br>2. Verify availability_zone label matches EC2 console |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

---

## Phase 2 Specific Validation Notes

### Label Building Tests
Each label type (META-01..14) needs unit tests verifying:
- Correct label name (e.g., `__meta_ecs_container_name`)
- Correct value extraction from AWS SDK objects
- Proper handling of missing/optional data

### Level Filtering Tests
- MetadataLevel::includes() correctly determines hierarchy
- Lower levels are subsets of higher levels
- Invalid level strings return proper error

### Integration Tests
- Full discovery run produces targets with all expected labels
- Query param ?level=aws returns all 14 labels
- Query param ?level=container returns only 3 labels

---

*Validation strategy created: 2026-05-19*
