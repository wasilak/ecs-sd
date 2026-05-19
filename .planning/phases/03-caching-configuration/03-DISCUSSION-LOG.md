# Phase 3: Caching & Configuration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-19
**Phase:** 3-Caching & Configuration
**Areas discussed:** Cache Refresh Strategy, CLI Framework, Configuration Precedence, Background Task Lifecycle, Cache Visibility

---

## Area 1: Cache Refresh Strategy — Failure handling & backoff

| Option | Description | Selected |
|--------|-------------|----------|
| Log error, continue with stale data | On any failure, log the error at WARN level and keep serving stale cache. Next refresh attempt happens at the normal interval. No special backoff logic. | ✓ |
| Exponential backoff on consecutive failures | Track consecutive failures. After 1st failure: normal interval. After 2nd: 2x interval. After 3rd: 4x interval. Cap at some max. | |
| Circuit breaker pattern | After N consecutive failures, stop trying for a cooldown period. Serve stale data only. More complex but prevents hammering AWS during outages. | |

**User's choice:** Log error, continue with stale data (simplest)
**Notes:** User wanted the simplest approach for v1. Complexity can be added if needed later.

### AWS Throttling

| Option | Description | Selected |
|--------|-------------|----------|
| Same as any error — log and continue | Treat ThrottlingException the same as any other AWS error. Log it, serve stale cache, retry at next interval. | |
| Add jitter to refresh intervals | Prevent thundering herd across multiple instances by adding ±10% random jitter to the refresh interval. Helps naturally spread out AWS calls. | ✓ |
| Explicit backoff on ThrottlingException | If we get a throttling error, immediately back off (e.g., skip next refresh, double interval temporarily). | |

**User's choice:** Add jitter to refresh intervals
**Notes:** Jitter is a good middle ground — simple to implement but provides throttling protection.

---

## Area 2: CLI Framework — clap derive vs builder API

| Option | Description | Selected |
|--------|-------------|----------|
| Derive macros | Uses `#[derive(Parser)]` with struct field attributes. Less code, declarative, widely used. Good for straightforward CLIs. | ✓ |
| Builder API | Programmatic construction with `Command::new()`, `Arg::new()`, etc. More verbose but offers dynamic argument generation. | |

**User's choice:** Derive macros (recommended for this)

### Subcommand Structure

| Option | Description | Selected |
|--------|-------------|----------|
| No subcommands — flat flags only | Simple: `ecs-sd --clusters foo,bar --listen 0.0.0.0:8080`. Single `serve` behavior. | ✓ |
| Add `serve` subcommand now | Structure: `ecs-sd serve --clusters foo,bar`. Allows adding subcommands later without breaking changes. | |

**User's choice:** No subcommands — flat flags only

### Help Text

| Option | Description | Selected |
|--------|-------------|----------|
| Auto-generated from clap derive attributes | Use `#[arg(help = "...")]` attributes. clap generates `--help` automatically. | ✓ |
| Custom help template with examples | Override clap's default help with a custom template including usage examples. | |

**User's choice:** Auto-generated from clap derive attributes

### Environment Variable Naming

| Option | Description | Selected |
|--------|-------------|----------|
| ECS_SD_<FLAG_NAME> uppercase with underscores | Examples: `ECS_SD_CLUSTERS`, `ECS_SD_LISTEN`, `ECS_SD_REFRESH_INTERVAL`. Clear prefix, follows Unix convention. | ✓ |
| Just uppercase flag names (no prefix) | Examples: `CLUSTERS`, `LISTEN`, `REFRESH_INTERVAL`. Shorter but risks collision. | |

**User's choice:** ECS_SD_<FLAG_NAME> uppercase with underscores

---

## Area 3: Configuration Precedence — env vars vs CLI args

| Option | Description | Selected |
|--------|-------------|----------|
| CLI args > env vars > defaults | Standard pattern. `ecs-sd --clusters foo` overrides `ECS_SD_CLUSTERS=bar`. | ✓ |
| CLI args > env vars > config file > defaults | Add config file support now (YAML/JSON). Allows complex configurations. | |
| 12-factor style: env vars are primary | Env vars override CLI args. Less common but matches "config in environment" philosophy. | |

**User's choice:** CLI args > env vars > defaults
**Notes:** Simple precedence, no config file for v1.

### Required Flags

| Option | Description | Selected |
|--------|-------------|----------|
| Only `--clusters` is required | Everything else has sensible defaults. Minimal friction to get started. | ✓ |
| All flags required | Force user to specify everything. No hidden defaults. | |

**User's choice:** Only `--clusters` is required
**Clarification:** Either `--clusters` or `ECS_SD_CLUSTERS` env var must be provided.

### Validation Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Validate immediately, exit on error | Parse CLI, validate config, exit with error code if invalid. Fail fast. | ✓ |
| Validate lazily, fail on first request | Start server even with potentially invalid config. Fail when discovery runs. | |

**User's choice:** Validate immediately, exit on error

---

## Area 4: Background Task Lifecycle — Cancellation & shutdown

| Option | Description | Selected |
|--------|-------------|----------|
| Use tokio::select! with cancellation token | Periodically check `token.cancelled()` during discovery. Fast shutdown (aborts mid-refresh). | |
| Use AbortHandle for coarse cancellation | Spawn task with `tokio::spawn`, store `AbortHandle`. Task stops at next `.await` point. | |
| Let refresh complete, then shutdown | On SIGTERM, set shutdown flag. Let current refresh finish before shutting down. | ✓ |

**User's choice:** Let refresh complete, then shutdown
**Notes:** Simplest approach. Shutdown might be delayed but no risk of partial/corrupted state.

### Refresh Timing Mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| tokio::interval (drift-free) | `interval.tick().await` automatically schedules next tick. Accounts for execution time. | ✓ |
| tokio::time::sleep in a loop | `sleep(refresh_interval).await` then run refresh. Simpler but can drift. | |

**User's choice:** tokio::interval (drift-free)

### First Refresh

| Option | Description | Selected |
|--------|-------------|----------|
| Immediate on startup, then interval | Populate cache immediately on startup, then start interval timer. | ✓ |
| Wait for first interval tick | Don't populate on startup. First request serves empty cache until first interval fires. | |

**User's choice:** Immediate on startup, then interval
**Notes:** Matches current behavior. Cache is warm before first HTTP request.

---

## Area 5: Cache Visibility — Metrics & debugging

| Option | Description | Selected |
|--------|-------------|----------|
| Add X-Cache-Age response header | Response header with seconds since last refresh. Helpful for debugging. | ✓ |
| Log cache age periodically | Include `cache_age_seconds` in discovery completion log. No HTTP header. | |
| Both header and log | Maximum visibility. | |
| Neither — keep it simple | No cache age tracking. | |

**User's choice:** Add X-Cache-Age response header

### Cache Hit/Miss Logging

| Option | Description | Selected |
|--------|-------------|----------|
| Don't log — too noisy | Every request would generate a log line. Could be thousands of lines. | |
| Log at DEBUG level only | Log cache hits/misses at DEBUG level. Off by default, can be enabled. | ✓ |
| Periodic summary stats | Log aggregate stats every N minutes instead of per-request. | |

**User's choice:** Log at DEBUG level only

### Discovery Operation Logging

| Option | Description | Selected |
|--------|-------------|----------|
| Start/complete/failure only | Log at INFO: 'discovery started', 'discovery complete: N targets', 'discovery failed'. | ✓ |
| Add per-cluster breakdown | INFO: 'cluster prod: 15 targets', 'cluster staging: 3 targets'. | |
| Full detail with timing (DEBUG) | DEBUG: per-cluster, per-service breakdown with timing. | |

**User's choice:** Start/complete/failure only (current)
**Notes:** Keep current minimal logging style. Don't add noise.

---

## Agent's Discretion

No areas deferred to agent discretion — user made explicit choices for all questions.

---

## Deferred Ideas

| Idea | Reason Deferred |
|------|-----------------|
| Config file support (YAML/JSON) | Not needed for v1 — flags and env vars sufficient. Can add in v2. |
| Circuit breaker for AWS failures | Simple logging approach sufficient for v1. |
| Cache hit/miss metrics endpoint | Requires metrics exposition framework (Phase 4+). |
| Prometheus metrics for cache | Out of scope — belongs in observability phase. |
| Dynamic config reload | Complex, requires file watching. Restart server for config changes. |
| Admin endpoints (/refresh, /config) | REST API for admin operations — new capability for v2. |

---

*Discussion completed: 2026-05-19*
