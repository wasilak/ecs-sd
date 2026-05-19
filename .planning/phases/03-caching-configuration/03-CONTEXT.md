# Phase 3: Caching & Configuration - Context

**Gathered:** 2026-05-19
**Status:** Ready for planning

---

## Phase Boundary

This phase delivers a background cache refresh system with stale-while-revalidate semantics and full CLI configuration support. The cache will refresh periodically without blocking HTTP requests, and all configuration will be controllable via CLI flags and environment variables.

**Key outcomes:**
1. Background refresh task running at configured interval
2. CLI parsing with clap derive macros
3. Environment variable support (ECS_SD_* prefix)
4. Stale-while-revalidate behavior (serve cached, refresh in background)
5. Graceful shutdown handling for background tasks

---

## Implementation Decisions

### D-01: Cache Refresh Strategy

**Failure handling:** Log error, continue serving stale data
- On any AWS API failure during refresh, log at WARN level and keep serving existing cache
- Next refresh attempt happens at the normal interval
- No exponential backoff or circuit breaker for v1 (simplest approach)

**Jitter:** Add ±10% random jitter to refresh intervals
- Prevents thundering herd when multiple ecs-sd instances restart simultaneously
- Applied to each interval calculation: `actual_interval = refresh_interval ± 10%`
- Helps naturally spread out AWS API calls across instances

**Example:**
```rust
let jitter = rand::random::<f64>() * 0.2 - 0.1; // ±10%
let interval = refresh_interval + (refresh_interval as f64 * jitter) as u64;
```

---

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

**Example struct:**
```rust
#[derive(Parser, Debug)]
#[command(name = "ecs-sd")]
#[command(about = "ECS Prometheus Service Discovery")]
pub struct Args {
    #[arg(long, env = "ECS_SD_CLUSTERS", help = "Comma-separated cluster names", required = true)]
    pub clusters: String,
    
    #[arg(long, env = "ECS_SD_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: String,
    
    #[arg(long, env = "ECS_SD_REFRESH_INTERVAL", default_value = "60s", value_parser = humantime::parse_duration)]
    pub refresh_interval: Duration,
    
    #[arg(long, env = "ECS_SD_METADATA_LEVEL", default_value = "task", value_enum)]
    pub metadata_level: MetadataLevel,
}
```

---

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

---

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

**Implementation sketch:**
```rust
let mut interval = tokio::time::interval(refresh_interval);
interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

loop {
    tokio::select! {
        _ = interval.tick() => {
            if shutdown_signal_received {
                break;
            }
            perform_refresh().await;
        }
        _ = shutdown_rx.recv() => {
            // Wait for current refresh to complete, then exit
            info!("Shutdown requested, waiting for refresh to complete...");
            shutdown_signal_received = true;
        }
    }
}
```

---

### D-05: Cache Visibility

**HTTP header:** `X-Cache-Age` (seconds since last refresh)
- Add to all `/sd` responses
- Value is integer seconds: `X-Cache-Age: 45`
- Helps operators/debuggers understand cache freshness

**Implementation:**
```rust
let cache_age = SystemTime::now()
    .duration_since(last_refresh_time)
    .unwrap_or_default()
    .as_secs();
    
Response::builder()
    .header("X-Cache-Age", cache_age.to_string())
    .json(&targets)
```

**Cache hit/miss logging:** DEBUG level only
- Log each cache access at DEBUG level (not INFO — too noisy)
- Format: `cache hit: 150 targets served` or `cache miss: triggering refresh`
- Off by default, enable with `RUST_LOG=debug`

**Discovery logging:** Keep current minimal style
- INFO: `discovery refresh started`
- INFO: `discovery refresh complete: 150 targets in 1.2s`
- WARN: `discovery refresh failed: <error>`
- No per-cluster or per-service breakdown at INFO level

---

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §CACHE-01..06 — Caching requirements
- `.planning/REQUIREMENTS.md` §CONF-01..06 — Configuration requirements
- `.planning/ROADMAP.md` — Phase 3 scope and success criteria
- `.planning/PROJECT.md` — Key decisions and constraints

### Prior Phase Context
- `.planning/phases/01-core-discovery-http-api/01-CONTEXT.md` — Core discovery decisions
- `.planning/phases/02-metadata-labels/02-CONTEXT.md` — Multi-tier cache, label filtering decisions

### Codebase Context
- `src/config.rs` — Config struct with placeholder fields
- `src/state/app_state.rs` — AppState with cache and discovery service
- `src/main.rs` — Current startup flow, initial discovery
- `src/aws/discovery.rs` — DiscoveryService implementation
- `Cargo.toml` — Current dependencies (need to add `clap` and `humantime`)

### Dependencies to Add
- `clap = { version = "4", features = ["derive", "env"] }` — CLI parsing
- `humantime = "2"` — Duration parsing (optional, for human-readable intervals like "60s")

---

## Existing Code Insights

### Reusable Assets
- **AppState.cache** — `Arc<RwLock<HashMap<MetadataLevel, Vec<Target>>>>` already exists
- **Config struct** — Has clusters, listen, refresh_interval, metadata_level fields (currently populated manually)
- **DiscoveryService** — Already has `discover_all_clusters()` method
- **Graceful shutdown** — SIGTERM/SIGINT handler exists in main.rs

### Established Patterns
- **Error handling** — `DiscoveryError` enum with `thiserror`
- **Partial results** — Log errors, continue with other clusters
- **Async/await** — Tokio runtime throughout
- **Tracing** — `info!`, `debug!`, `warn!` macros in use
- **State management** — Axum state pattern with `with_state()`

### Integration Points
- **Main startup** — Replace hardcoded config with clap-parsed config
- **Background task** — Spawn in main.rs after initial discovery
- **Cache updates** — Background task writes to `state.cache.write().await`
- **Handler reads** — Handlers use `state.cache.read().await` (no changes needed)
- **Shutdown** — Extend existing graceful shutdown to signal background task

### Current Limitations
- Config is hardcoded in main.rs (no CLI parsing)
- No background refresh (only initial discovery)
- `refresh_interval` field exists but unused
- No env var support

---

## Specific Ideas

### CLI Usage Examples

**Minimal:**
```bash
ecs-sd --clusters prod,staging
```

**Full explicit:**
```bash
ecs-sd \
  --clusters prod,staging \
  --listen 0.0.0.0:8080 \
  --refresh-interval 60s \
  --metadata-level task
```

**Environment variables:**
```bash
export ECS_SD_CLUSTERS="prod,staging"
export ECS_SD_REFRESH_INTERVAL="120s"
ecs-sd
```

### Duration Format
Use `humantime` crate for human-friendly duration parsing:
- `60s` — 60 seconds
- `5m` — 5 minutes
- `1h` — 1 hour

---

## Deferred Ideas

| Idea | Reason Deferred |
|------|-----------------|
| Config file support (YAML/JSON) | Not needed for v1 — flags and env vars sufficient. Add in v2. |
| Circuit breaker for AWS failures | Simple logging approach sufficient for v1. Can add if needed. |
| Cache hit/miss metrics endpoint | Requires metrics exposition framework (Phase 4+). |
| Prometheus metrics for cache | Out of scope — belongs in observability phase. |
| Dynamic config reload | Complex, requires file watching. Restart server for config changes. |
| Admin endpoints (/refresh, /config) | REST API for admin operations — new capability for v2. |

---

*Phase: 3-Caching & Configuration*
*Context gathered: 2026-05-19*
