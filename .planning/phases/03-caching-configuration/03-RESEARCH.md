# Phase 3: Caching & Configuration - Research

**Researched:** 2026-05-20  
**Domain:** Rust async caching (stale-while-revalidate) + CLI/env configuration  
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

### D-01: Cache Refresh Strategy

**Failure handling:** Log error, continue serving stale data
- On any AWS API failure during refresh, log at WARN level and keep serving existing cache
- Next refresh attempt happens at the normal interval
- No exponential backoff or circuit breaker for v1 (simplest approach)

**Jitter:** Add ±10% random jitter to refresh intervals
- Prevents thundering herd when multiple ecs-sd instances restart simultaneously
- Applied to each interval calculation: `actual_interval = refresh_interval ± 10%`
- Helps naturally spread out AWS API calls across instances

### D-02: CLI Framework

**Framework:** clap with derive macros (`#[derive(Parser)]`)
- Declarative, less boilerplate than builder API
- Sufficient for straightforward flag-based configuration

**Structure:** Flat flags (no subcommands)
- Single binary behavior: `ecs-sd --clusters foo,bar --listen 0.0.0.0:8080`
- Subcommands can be added later without breaking changes (with default behavior)

**Help text:** Auto-generated from derive attributes
- Use `#[arg(help = "...")]` on struct fields
- clap generates `--help` automatically

### D-03: Configuration Precedence

**Order:** CLI args > environment variables > defaults

1. **CLI args** — Highest priority (e.g., `--clusters foo`)
2. **Environment variables** — Second priority (e.g., `ECS_SD_CLUSTERS=foo`)
3. **Defaults** — Built-in defaults for optional flags

**No config file support for v1** — Can be added in v2 if needed

**Required flags:**
- Only `--clusters` (or `ECS_SD_CLUSTERS` env var) is required
- Everything else has sensible defaults:
  - `--listen=0.0.0.0:8080`
  - `--refresh-interval=60s`
  - `--metadata-level=task`

**Validation:** Immediate on startup, exit on error
- Parse CLI and validate all configuration before starting server
- Exit with non-zero status and error message if invalid
- Examples of validation:
  - `--clusters` must not be empty
  - `--listen` must be valid socket address
  - `--refresh-interval` must be positive duration

### D-04: Background Task Lifecycle

**Timing mechanism:** `tokio::interval` (drift-free)
- Automatically schedules next tick based on interval duration
- Accounts for execution time of refresh operation
- Handles missed ticks (bursts) appropriately

**First refresh:** Immediate on startup, then interval
- Populate cache during server startup (current behavior)
- Then start interval timer for subsequent refreshes
- Ensures cache is warm before first HTTP request

**Cancellation:** Let refresh complete, then shutdown
- On SIGTERM, set shutdown flag but allow current refresh to finish
- Cooperative shutdown — refresh task checks flag between cluster discoveries
- No forced abort mid-AWS-call (cleaner, no orphaned connections)

### D-05: Cache Visibility

**HTTP header:** `X-Cache-Age` (seconds since last refresh)
- Add to all `/sd` responses
- Value is integer seconds: `X-Cache-Age: 45`
- Helps operators/debuggers understand cache freshness

**Cache hit/miss logging:** DEBUG level only
- Log each cache access at DEBUG level (not INFO — too noisy)
- Format: `cache hit: 150 targets served` or `cache miss: triggering refresh`
- Off by default, enable with `RUST_LOG=debug`

**Discovery logging:** Keep current minimal style
- INFO: `discovery refresh started`
- INFO: `discovery refresh complete: 150 targets in 1.2s`
- WARN: `discovery refresh failed: <error>`
- No per-cluster or per-service breakdown at INFO level

### the agent's Discretion
None explicitly listed in CONTEXT.md. `[VERIFIED: .planning/phases/03-caching-configuration/03-CONTEXT.md]`

### Deferred Ideas (OUT OF SCOPE)
- Config file support (YAML/JSON)
- Circuit breaker for AWS failures
- Cache hit/miss metrics endpoint
- Prometheus metrics for cache
- Dynamic config reload
- Admin endpoints (/refresh, /config)
</user_constraints>

## Summary

Phase 3 should be planned as a **runtime behavior phase** (background refresh + cache serving semantics) plus a **startup configuration phase** (CLI/env parsing and validation), not as a discovery-logic rewrite. Current code already has: in-memory cache guarded by `Arc<RwLock<...>>`, initial discovery warm-up, and graceful shutdown wiring. The plan should extend this with one spawned refresh loop, cache freshness metadata, and clap-based parsing for all CONF requirements. `[VERIFIED: src/main.rs][VERIFIED: src/state/app_state.rs][VERIFIED: src/handlers/sd.rs]`

For locked decisions, the recommended implementation is: `tokio::time::interval` + `MissedTickBehavior::Skip` for periodic scheduling, jitter per cycle, write-lock only during atomic cache swap, and stale-serving on refresh failure. This aligns with CACHE-03/04/05 and avoids reader blocking except for short critical sections. `[CITED: https://docs.rs/tokio/latest/tokio/time/fn.interval.html][CITED: https://docs.rs/tokio/latest/src/tokio/time/interval.rs.html][VERIFIED: .planning/REQUIREMENTS.md]`

For configuration, use `clap` derive + `env` feature and `humantime::parse_duration` for `--refresh-interval`. Clap env behavior and defaults support the required precedence model when flags are present (`CLI > env > default`). `[CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/][CITED: https://docs.rs/clap/latest/clap/builder/struct.Arg.html][CITED: https://docs.rs/humantime/latest/humantime/fn.parse_duration.html]`

**Primary recommendation:** Keep `Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>`, implement one background refresher task in `main.rs`, add `last_refresh` timestamp to shared state, and migrate config loading to clap derive with ECS_SD_* env support. `[VERIFIED: src/state/app_state.rs][VERIFIED: src/config.rs][HIGH]`

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| CLI/env parsing and startup validation | API/Backend | — | Happens before server bind; process-level config concern. `[VERIFIED: src/main.rs]` |
| Stale-while-revalidate cache policy | API/Backend | — | Request handlers + background task are server internals. `[VERIFIED: src/handlers/sd.rs]` |
| Periodic refresh scheduling | API/Backend | — | Tokio runtime task orchestration belongs in backend runtime. `[VERIFIED: src/main.rs]` |
| Concurrent cache access | API/Backend | — | Shared server state (`AppState`) and synchronization primitives. `[VERIFIED: src/state/app_state.rs]` |
| AWS credential resolution | API/Backend | AWS platform IAM | SDK credential chain resolves from env/profile/role at runtime. `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |
| Cache freshness visibility (`X-Cache-Age`) | API/Backend | — | Response decoration in HTTP handler. `[ASSUMED]` |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.52.3 | async runtime + interval scheduling | Native stack already used; interval APIs match locked lifecycle design. `[VERIFIED: cargo info tokio][CITED: https://docs.rs/tokio/latest/tokio/time/fn.interval.html]` |
| clap | 4.6.1 | CLI flags, validation, help, env var mapping | Derive API + env support directly map to CONF-01..05. `[VERIFIED: cargo info clap][CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/][CITED: https://docs.rs/clap/latest/clap/builder/struct.Arg.html]` |
| humantime | 2.3.0 | parse `30s`, `5m` duration strings | Aligns with required duration UX for refresh interval. `[VERIFIED: cargo info humantime][CITED: https://docs.rs/humantime/latest/humantime/fn.parse_duration.html]` |
| aws-config | 1.8.16 | default provider chain for credentials | Required by CONF-06; supports env/profile/ECS/EC2 chain. `[VERIFIED: cargo info aws-config][CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rand | 0.8.6 | ±10% jitter per refresh cycle | Required by locked D-01 jitter strategy. `[VERIFIED: crates.io api]` |
| std::sync + tokio::sync::RwLock | std/tokio | shared cache + non-blocking read path | Keep existing structure; minimizes refactor risk in Phase 3. `[VERIFIED: src/state/app_state.rs]` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `Arc<RwLock<HashMap<...>>>` | `DashMap` (6.2.1 stable docs) | DashMap reduces explicit lock handling, but existing code already centered on `RwLock`; switching adds migration risk with little phase value. `[VERIFIED: src/state/app_state.rs][CITED: https://docs.rs/dashmap/latest/dashmap/]` |

**Installation:**
```bash
cargo add clap --features derive,env
cargo add humantime
cargo add rand
```

**Version verification (latest + release date):**
- clap 4.6.1 (2026-04-15) `[VERIFIED: crates.io api]`
- tokio 1.52.3 (2026-05-08) `[VERIFIED: crates.io api]`
- humantime 2.3.0 (2025-09-11) `[VERIFIED: crates.io api]`
- dashmap 6.2.1 (2026-05-17) `[VERIFIED: crates.io api]`
- rand 0.8.6 (2026-04-17) `[VERIFIED: crates.io api]`

## Architecture Patterns

### System Architecture Diagram

```text
Process start
  |
  v
Parse config (clap derive + env + defaults) ----> validate (clusters/listen/interval)
  |                                                        |
  | invalid                                                | valid
  v                                                        v
Exit non-zero                                        Build AppState + AWS clients
                                                          |
                                                          v
                                            Initial discovery + populate cache tiers
                                                          |
                        +---------------------------------+--------------------------------+
                        |                                                                  |
                        v                                                                  v
              Spawn background refresh loop                                      Start Axum HTTP server
      (interval + jitter + skip missed ticks + shutdown select)                       (/sd, /health)
                        |                                                                  |
                        v                                                                  v
             Refresh success: atomic cache swap                                 Read cache immediately
             Refresh failure: WARN, keep stale                                  Add X-Cache-Age header
```

### Recommended Project Structure
```text
src/
├── config.rs            # clap Args + conversion into runtime Config
├── main.rs              # startup sequence + spawn refresh task + shutdown orchestration
├── state/
│   └── app_state.rs     # cache + last_refresh timestamp + config
├── handlers/
│   └── sd.rs            # cache read path + X-Cache-Age header
└── aws/
    └── discovery.rs     # refresh source of truth (already implemented)
```

### Pattern 1: clap derive with env and duration parser
**What:** Strongly typed startup arguments with env fallback and human-readable durations. `[CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/][CITED: https://docs.rs/clap/latest/clap/builder/struct.Arg.html][CITED: https://docs.rs/humantime/latest/humantime/fn.parse_duration.html]`

**When to use:** Replace hardcoded config in `main.rs` and satisfy CONF-01..05. `[VERIFIED: src/main.rs][VERIFIED: .planning/REQUIREMENTS.md]`

**Example:**
```rust
use clap::Parser;
use std::time::Duration;

#[derive(Parser, Debug, Clone)]
#[command(name = "ecs-sd", about = "ECS Prometheus Service Discovery")]
pub struct Args {
    #[arg(long, env = "ECS_SD_CLUSTERS", required = true)]
    pub clusters: String,

    #[arg(long, env = "ECS_SD_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: String,

    #[arg(long, env = "ECS_SD_REFRESH_INTERVAL", default_value = "60s", value_parser = humantime::parse_duration)]
    pub refresh_interval: Duration,
}
```

### Pattern 2: background refresh loop with cooperative shutdown
**What:** Dedicated spawned task using `tokio::interval` and `tokio::select!` on interval tick + shutdown signal. `[CITED: https://docs.rs/tokio/latest/tokio/time/fn.interval.html][CITED: https://docs.rs/tokio/latest/src/tokio/time/interval.rs.html]`

**When to use:** Implement CACHE-03/05 and D-04 lifecycle behavior. `[VERIFIED: .planning/REQUIREMENTS.md][VERIFIED: .planning/phases/03-caching-configuration/03-CONTEXT.md]`

**Example:**
```rust
use tokio::time::{self, MissedTickBehavior};

let mut interval = time::interval(refresh_every);
interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

loop {
    tokio::select! {
        _ = interval.tick() => {
            let delay = with_jitter(refresh_every);
            time::sleep(delay).await;
            if let Err(e) = refresh_cache_once().await {
                tracing::warn!(error = %e, "cache refresh failed; serving stale cache");
            }
        }
        _ = shutdown_rx.recv() => break,
    }
}
```

### Anti-Patterns to Avoid
- **Lock held across AWS calls:** never keep write lock while calling ECS/EC2/STS; build refreshed data first, swap under short write lock. `[VERIFIED: src/aws/discovery.rs][HIGH]`
- **Blocking request path on refresh:** `/sd` must only read cached state and never trigger synchronous discovery. `[VERIFIED: .planning/REQUIREMENTS.md]`
- **Refreshing each metadata tier separately:** use one AWS-level discovery result and derive lower tiers (existing pattern). `[VERIFIED: src/main.rs][VERIFIED: src/handlers/sd.rs]`

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CLI parser and help | custom arg parsing | `clap` derive | Mature validation/help/env support. `[CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/]` |
| Human duration grammar | manual `30s/5m` parser | `humantime::parse_duration` | Handles full unit grammar and errors consistently. `[CITED: https://docs.rs/humantime/latest/humantime/fn.parse_duration.html]` |
| Periodic scheduler | ad-hoc sleep loop | `tokio::time::interval` | Correct periodic semantics and missed tick policy. `[CITED: https://docs.rs/tokio/latest/tokio/time/fn.interval.html]` |
| AWS credential logic | custom env/profile/IAM selection | aws-config default provider chain | Correct precedence and refresh behavior across environments. `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |

**Key insight:** Most Phase 3 complexity is in edge-case runtime behavior (timing, lock scope, stale serving), not in business logic. Reusing battle-tested crates is the fastest low-risk path. `[HIGH]`

## Common Pitfalls

### Pitfall 1: Refresh loop causes synchronized API spikes
**What goes wrong:** Multiple instances call ECS APIs at same second. `[ASSUMED]`
**Why it happens:** Deterministic intervals with identical start time. `[ASSUMED]`
**How to avoid:** Apply ±10% jitter each cycle (locked D-01). `[VERIFIED: 03-CONTEXT.md]`
**Warning signs:** Burst of throttling warnings aligned by timestamps. `[ASSUMED]`

### Pitfall 2: Reader latency spikes during cache replacement
**What goes wrong:** Requests block while refresh mutates cache. `[ASSUMED]`
**Why it happens:** Lock scope too large (including derivation/work). `[ASSUMED]`
**How to avoid:** Build refreshed map off-lock, write-lock only for final swap. `[HIGH]`
**Warning signs:** p95 response time rises during refresh windows. `[ASSUMED]`

### Pitfall 3: Wrong precedence implementation
**What goes wrong:** Env/default override explicit CLI unexpectedly. `[ASSUMED]`
**Why it happens:** Manual merge logic after clap parse. `[ASSUMED]`
**How to avoid:** Let clap own precedence and parse once from process env/args. `[CITED: https://docs.rs/clap/latest/clap/builder/struct.Arg.html]`
**Warning signs:** `--refresh-interval` ignored when env var is set. `[ASSUMED]`

### Pitfall 4: STS failures crash startup
**What goes wrong:** App exits when STS identity lookup fails in `DiscoveryService::new`. `[VERIFIED: src/aws/discovery.rs]`
**Why it happens:** Current constructor returns error on STS failure. `[VERIFIED: src/aws/discovery.rs]`
**How to avoid:** Keep current behavior for now or explicitly decide fallback behavior in planning; document IAM requirement clearly. `[VERIFIED: src/aws/discovery.rs][CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]`
**Warning signs:** startup fails before listener bind with STS error. `[VERIFIED: src/error.rs]`

## Code Examples

### Env-backed clap option with default
```rust
#[arg(long, env = "ECS_SD_REFRESH_INTERVAL", default_value = "60s")]
refresh_interval: String
```
Source: `[CITED: https://docs.rs/clap/latest/clap/builder/struct.Arg.html]`

### Interval with missed tick behavior
```rust
let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
```
Source: `[CITED: https://docs.rs/tokio/latest/src/tokio/time/interval.rs.html]`

### Credential chain default config
```rust
let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
    .load()
    .await;
```
Source: `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]`

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hardcoded config in `main.rs` | clap derive + env-backed args | Pending in this phase | Enables CONF-01..05 and production-friendly deploys. `[VERIFIED: src/main.rs][VERIFIED: REQUIREMENTS.md]` |
| One-time startup discovery only | periodic stale-while-revalidate | Pending in this phase | Meets CACHE-03..06 and smooth runtime behavior. `[VERIFIED: src/main.rs][VERIFIED: REQUIREMENTS.md]` |
| Implicit cache freshness | explicit `X-Cache-Age` header | Locked decision D-05 | Better operability/troubleshooting. `[VERIFIED: 03-CONTEXT.md]` |

**Deprecated/outdated for v1 scope:**
- Config-file-first approach (YAML/JSON) is out of scope for this phase. `[VERIFIED: 03-CONTEXT.md]`

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `X-Cache-Age` should be derived from a shared `last_refresh` timestamp in state | Architecture Patterns | **Resolved in Open Questions #2** |
| A2 | Reader latency impact from `RwLock` will be acceptable at expected traffic | Common Pitfalls | Might require DashMap/other strategy sooner |
| A3 | Jitter via `rand` crate is acceptable dependency-wise for this repo | Standard Stack | Team may prefer deterministic/no-new-deps approach |

## Open Questions (RESOLVED)

1. **Should failed STS lookup block startup in Phase 3?**
   - **Status: RESOLVED**
   - **Resolution:** Yes — failed STS lookup **blocks startup** in Phase 3.
   - **Rationale:** Current constructor already fails on STS error, and Phase 3 scope does not introduce credential fallback logic; this preserves explicit fail-fast behavior for IAM misconfiguration.
   - **Implementation note:** Keep constructor fail-fast behavior as-is and document IAM requirement in PLAN/summary.
   - Evidence: current code fails constructor on STS error. `[VERIFIED: src/aws/discovery.rs]`

2. **Where to keep `last_refresh` timestamp?**
   - **Status: RESOLVED**
   - **Resolution:** Store `last_refresh` as a **global timestamp in `AppState`** (shared across metadata tiers).
   - **Rationale:** Phase 3 performs a single discovery run that updates all tiers together, so one shared freshness value is correct for `X-Cache-Age` and avoids unnecessary per-tier complexity.
   - **Implementation note:** Add shared timestamp field to `AppState`, update only on successful cache swap.
   - Evidence: no timestamp field exists yet in `AppState`. `[VERIFIED: src/state/app_state.rs]`

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo | Build/test/dep management | ✓ | 1.95.0 | — |
| Rust toolchain | Compile + tokio runtime | ✓ | via cargo toolchain | — |
| crates.io access | Add clap/humantime/rand | ✓ | reachable | vendor deps manually (not recommended) |

**Missing dependencies with no fallback:**
- None identified for planning stage.

**Missing dependencies with fallback:**
- None identified.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`) |
| Config file | none |
| Quick run command | `cargo test` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CACHE-01 | in-memory cache stores discovery results | unit/integration | `cargo test handlers::sd::tests` | ✅ |
| CACHE-02 | `--refresh-interval` config | unit | `cargo test config::tests::test_refresh_interval_parse` | ❌ Wave 0 |
| CACHE-03 | background refresh non-blocking | integration | `cargo test main::tests::test_background_refresh_non_blocking` | ❌ Wave 0 |
| CACHE-04 | requests always serve from cache | integration | `cargo test handlers::sd::tests::test_serves_cached_targets` | ❌ Wave 0 |
| CACHE-05 | failed refresh keeps stale data | integration | `cargo test main::tests::test_refresh_failure_keeps_stale` | ❌ Wave 0 |
| CACHE-06 | TTL equals refresh interval semantics | integration | `cargo test main::tests::test_cache_age_matches_refresh_policy` | ❌ Wave 0 |
| CONF-01 | required clusters argument/env | unit | `cargo test config::tests::test_clusters_required` | ❌ Wave 0 |
| CONF-02 | listen bind parsing/default | unit | `cargo test config::tests::test_listen_default_and_parse` | ❌ Wave 0 |
| CONF-03 | refresh interval parsing/default | unit | `cargo test config::tests::test_refresh_interval_default_and_parse` | ❌ Wave 0 |
| CONF-04 | metadata-level default | unit | `cargo test config::tests::test_metadata_level_default` | ❌ Wave 0 |
| CONF-05 | env var support for all flags | unit | `cargo test config::tests::test_env_var_resolution` | ❌ Wave 0 |
| CONF-06 | credential provider chain compatibility | integration | `cargo test aws::client::tests::test_default_chain_config_loads` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test`
- **Per wave merge:** `cargo test`
- **Phase gate:** full `cargo test` green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Add `src/config.rs` tests for clap/env precedence and validation
- [ ] Add refresh-loop tests (possibly with `tokio::time::pause` and manual advance) `[ASSUMED]`
- [ ] Add handler tests for `X-Cache-Age` header and stale-serving behavior

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Service has no end-user auth in scope. `[VERIFIED: REQUIREMENTS.md Out of Scope]` |
| V3 Session Management | no | Stateless HTTP service. `[VERIFIED: src/routes/sd.rs]` |
| V4 Access Control | yes | IAM role/profile/env-driven AWS permissions. `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |
| V5 Input Validation | yes | clap validation for CLI and typed parsing for values. `[CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/]` |
| V6 Cryptography | no | TLS/signing delegated to AWS SDK, no custom crypto. `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| AWS API throttling during synchronized refresh | Denial of Service | jitter + stale-while-revalidate + warning logs. `[VERIFIED: 03-CONTEXT.md]` |
| Misconfigured credentials in runtime env | Denial of Service | rely on default provider chain + explicit startup errors. `[CITED: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html]` |
| Invalid operator input (flags/env) | Tampering | clap parser + value parsers + startup fail-fast. `[CITED: https://docs.rs/clap/latest/clap/_derive/_tutorial/]` |

## Sources

### Primary (HIGH confidence)
- `.planning/phases/03-caching-configuration/03-CONTEXT.md` (locked decisions)
- `.planning/REQUIREMENTS.md` (CACHE-01..06, CONF-01..06)
- `src/main.rs`, `src/state/app_state.rs`, `src/handlers/sd.rs`, `src/config.rs`, `src/aws/discovery.rs` (actual implementation baseline)
- Tokio docs (`interval`, `MissedTickBehavior`): https://docs.rs/tokio/latest/tokio/time/fn.interval.html
- clap derive + Arg env docs: https://docs.rs/clap/latest/clap/_derive/_tutorial/ and https://docs.rs/clap/latest/clap/builder/struct.Arg.html
- AWS SDK Rust credential provider chain: https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html

### Secondary (MEDIUM confidence)
- humantime docs: https://docs.rs/humantime/latest/humantime/fn.parse_duration.html
- dashmap docs for alternative assessment: https://docs.rs/dashmap/latest/dashmap/

### Tertiary (LOW confidence)
- Operational warning-sign heuristics in Common Pitfalls (explicitly marked `[ASSUMED]`)

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** — versions verified from registry + official docs.
- Architecture: **HIGH** — grounded in current code paths and locked context decisions.
- Pitfalls: **MEDIUM** — some production-behavior claims are assumed and flagged.

**Research date:** 2026-05-20  
**Valid until:** 2026-06-19

## Project Constraints (from AGENTS.md)

No project-local `./AGENTS.md` found in repository root, so no additional project-specific constraints beyond global agent policy were extracted. `[VERIFIED: glob AGENTS.md in repo root]`
