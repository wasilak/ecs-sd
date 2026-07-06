---
phase: 09-cachesnapshot-refactor-module-cleanup
reviewed: 2026-07-06T17:04:12Z
depth: standard
files_reviewed: 10
files_reviewed_list:
  - src/aws/discovery.rs
  - src/error.rs
  - src/handlers/metrics.rs
  - src/handlers/proxy.rs
  - src/handlers/sd.rs
  - src/main.rs
  - src/models/label_filter.rs
  - src/models/mod.rs
  - src/state/app_state.rs
  - src/state/mod.rs
findings:
  critical: 1
  warning: 6
  info: 2
  total: 9
status: issues_found
---

# Phase 09: Code Review Report

**Reviewed:** 2026-07-06T17:04:12Z
**Depth:** standard
**Files Reviewed:** 10
**Status:** issues_found

## Summary

This phase refactored `CacheSnapshot` into `state/app_state.rs`, added proxy-mode target building and label-level filtering to `sd.rs`, and made structural cleanups across the model and state modules. The core cache/snapshot logic is sound — `build_snapshot` is atomic, `replace_cache_and_routing` correctly builds all tiers outside the write lock before swapping, and routing table UUIDs are deterministic via `Uuid::new_v5`, so follower sync is correct.

One BLOCKER was found: `gossip_advertise_addr` is hardcoded to `0.0.0.0:{port}` in main.rs — other cluster nodes cannot reach a peer advertised at the unspecified address, making multi-node cluster mode completely non-functional.

Six warnings cover a doubled background-refresh interval, hop-by-hop response header forwarding, a read lock held across async gossip calls, a missing legacy label case in the filter, a semantically wrong error variant, and dead gossip code that publishes routing state no follower ever reads.

---

## Critical Issues

### CR-01: `gossip_advertise_addr` hardcoded to `0.0.0.0` breaks multi-node cluster

**File:** `src/main.rs:66`
**Issue:** `gossip_advertise_addr` is the address advertised to peers so they know how to reach this node over UDP gossip. Hardcoding `0.0.0.0` (the "unspecified" address) means every other cluster node will attempt to gossip with `0.0.0.0:{port}`, which is not routable from any remote machine. Cluster-mode gossip will silently fail to establish peer connections; each node will believe itself to be standalone, discover independently, and diverge.

`listen_addr` correctly uses `0.0.0.0` to bind on all interfaces, but `gossip_advertise_addr` must be a routable address that peers can actually reach. The `node_id` field (`HOSTNAME:port`) already encodes the correct hostname — that source should be used here.

**Fix:**
```rust
// In main.rs, inside the ClusterMode::Cluster arm:

// Derive routable advertise address from node_id (hostname:gossip_port)
let gossip_advertise_addr = config
    .node_id  // e.g. "ip-10-0-1-5.eu-west-1.compute.internal:8081"
    .parse::<SocketAddr>()
    .unwrap_or_else(|_| {
        format!("{}:{}", std::env::var("HOSTNAME").unwrap_or_default(), config.gossip_port)
            .parse()
            .expect("gossip_port is validated at config parse time")
    });

let chitchat_id = ChitchatId {
    node_id: config.node_id.clone().into(),
    generation_id: rand::random::<u64>(),
    gossip_advertise_addr,  // routable, not 0.0.0.0
};
let cc_config = ChitchatConfig {
    chitchat_id,
    cluster_id: "ecs-sd".to_string(),
    gossip_interval: std::time::Duration::from_secs(1),
    listen_addr: format!("0.0.0.0:{}", config.gossip_port).parse()
        .expect("gossip_port is validated at config parse time"),
    // ...
};
```

Alternatively, add an `ECS_SD_GOSSIP_ADVERTISE_ADDR` config field so operators can set it explicitly, which is the most robust approach.

---

## Warnings

### WR-01: Background refresh fires at ~2× the configured interval due to post-tick jitter sleep

**File:** `src/main.rs:204-262`
**Issue:** `spawn_background_refresh` creates an interval of `base_interval` seconds, then after each tick it sleeps an **additional** `base_interval * (1 ± 10%)` before doing the actual refresh. Total elapsed between refreshes:

- tick fires at `t = base_interval`
- additional sleep: `base_interval * 0.90..1.10`
- refresh happens at: `t ≈ 1.9 × base_interval`
- with `MissedTickBehavior::Skip`, the next tick schedules at `last_tick + base_interval`, which is already in the past → fires immediately after the sleep → next refresh at `≈ 3 × base_interval`

For a configured `refresh_interval = 60s`, actual refresh cadence is ~180s. `cache_ttl_seconds` equals `refresh_interval` (60s), so the `X-Cache-State` header will report `stale` for the vast majority of the inter-refresh window — undermining the freshness signal and making SLA reasoning unreliable.

The jitter was clearly intended to spread out refreshes across cluster nodes (±10% randomisation is correct for that purpose), but it should replace the ticker delay, not be added to it.

**Fix:**
```rust
// Option A: remove the interval; use jittered sleep directly as the cadence
fn spawn_background_refresh(state: AppState, mut shutdown_rx: watch::Receiver<bool>)
    -> tokio::task::JoinHandle<()>
{
    tokio::spawn(async move {
        let base_interval = Duration::from_secs(state.config.refresh_interval.max(1));
        loop {
            let jitter_factor = rand::rng().random_range(-0.10..=0.10);
            let sleep_duration = calculate_jittered_delay(base_interval, jitter_factor);
            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {}
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow() { break; }
                }
            }
            if *shutdown_rx.borrow() { break; }
            // ... leader check, refresh, metrics
        }
    })
}

// Option B: keep the interval but remove the inner tokio::time::sleep call
// (the interval already fires at base_interval; apply jitter only as a
// small additive nudge of, say, ±5s regardless of base_interval)
```

---

### WR-02: Proxy handler forwards upstream hop-by-hop response headers verbatim

**File:** `src/handlers/proxy.rs:164-169`
**Issue:** The request path strips hop-by-hop headers before forwarding to upstream (via `filter_hop_by_hop_headers`), but the response path copies all upstream headers to the client without any stripping:

```rust
let upstream_headers = upstream_resp.headers().clone();
*resp_builder.headers_mut().unwrap() = upstream_headers;
```

Hop-by-hop headers such as `Transfer-Encoding`, `Connection`, `Keep-Alive`, and `Trailer` are connection-scoped and must not be forwarded end-to-end (RFC 7230 §6.1). Forwarding `Transfer-Encoding: chunked` from an HTTP/1.1 upstream to an HTTP/2 client is explicitly forbidden by RFC 7540 §8.1.2.2 and will cause compliant clients to reject the response. Axum serves both HTTP/1.1 and HTTP/2 depending on the listener, so this is a real risk.

**Fix:**
```rust
// Apply the same hop-by-hop filter to the upstream response headers
use crate::handlers::proxy::filter_hop_by_hop_headers;

let upstream_headers = filter_hop_by_hop_headers(upstream_resp.headers().clone());
let mut resp_builder = Response::builder().status(status);
*resp_builder.headers_mut().unwrap() = upstream_headers;
resp_builder
    .body(Body::from_stream(upstream_resp.bytes_stream()))
    .unwrap()
```

---

### WR-03: `publish_cache_to_gossip` holds a read lock across two async gossip publishes

**File:** `src/main.rs:324-342`
**Issue:** The `snap` read-lock guard is kept alive across both `cluster.publish_cache(&json).await` and `cluster.publish_routing(&json).await`:

```rust
let snap = state.snapshot.read().await;   // read lock acquired
if let Some(targets) = snap.cache.get(...) {
    if let Ok(json) = serde_json::to_string(targets) {
        cluster.publish_cache(&json).await;   // .await with lock held
    }
}
if state.config.mode == Mode::Proxy {
    let gossip_rt = snap.routing_table.values().map(...).collect();
    if let Ok(json) = serde_json::to_string(&gossip_rt) {
        cluster.publish_routing(&json).await;  // .await with lock held
    }
}
// lock drops here
```

Any `replace_cache_and_routing` call (background refresh or manual `/sd/refresh`) acquires the write lock and will block for the entire duration of both gossip publish calls. Under high Chitchat write latency, this delays cache updates.

**Fix:** Clone the needed data, release the lock, then serialize and publish:
```rust
async fn publish_cache_to_gossip(state: &AppState) {
    let Some(ref cluster) = state.cluster else { return };

    let (aws_targets, routing_entries) = {
        let snap = state.snapshot.read().await;
        let aws = snap.cache.get(&MetadataLevel::Aws).cloned();
        let rt = if state.config.mode == Mode::Proxy {
            Some(snap.routing_table.values().map(|pt| GossipProxyTarget {
                route_id: pt.route_id.to_string(),
                address: pt.address.clone(),
                labels: pt.labels.clone(),
            }).collect::<Vec<_>>())
        } else {
            None
        };
        (aws, rt)
        // lock released here
    };

    if let Some(targets) = aws_targets {
        if let Ok(json) = serde_json::to_string(&targets) {
            cluster.publish_cache(&json).await;
        }
    }
    if let Some(entries) = routing_entries {
        if let Ok(json) = serde_json::to_string(&entries) {
            cluster.publish_routing(&json).await;
        }
    }
}
```

---

### WR-04: Legacy `__meta_ecs_cluster` label misclassified as Aws-level in `filter_labels_by_level`

**File:** `src/models/label_filter.rs:15-20`
**Issue:** The label classifier handles the legacy `__meta_ecs_service` label explicitly:

```rust
} else if key.starts_with("__meta_ecs_service_") || *key == "__meta_ecs_service" {
    MetadataLevel::Service
```

But the analogous legacy label `__meta_ecs_cluster` (without a trailing underscore) is not handled. It falls through to the generic `__meta_ecs_` prefix case and is classified as `MetadataLevel::Aws` instead of `MetadataLevel::Cluster`. This means at `MetadataLevel::Cluster`, `MetadataLevel::Service`, `MetadataLevel::Task`, and `MetadataLevel::Container` response levels, the legacy cluster label is stripped when it should be retained. Targets with the legacy label schema will have incomplete cluster context at all levels below Aws.

The `filter_targets` function in `sd.rs` correctly falls back to `__meta_ecs_cluster` for filtering (line 211), so filtering still works — but the filtered result will have the cluster label stripped when requested at non-Aws levels.

**Fix:**
```rust
} else if key.starts_with("__meta_ecs_cluster_") || *key == "__meta_ecs_cluster" {
    MetadataLevel::Cluster
```

Add a test that mirrors the existing `__meta_ecs_service` legacy label test in `label_filter.rs`.

---

### WR-05: `DiscoveryError::NoContainerInstance` returned for a missing task definition ARN

**File:** `src/aws/discovery.rs:354-355`
**Issue:**
```rust
let task_def_arn = task
    .task_definition_arn()
    .ok_or(DiscoveryError::NoContainerInstance)?;
```

`DiscoveryError::NoContainerInstance` carries the message "Task has no container instance". It is returned here when the EC2 task is missing its `task_definition_arn` — an entirely different field. A task that has no container instance and a task that has no task definition ARN are distinct failure modes. Operators who encounter this error message in logs will look for a missing container instance mapping, not a missing task definition, wasting debugging time.

**Fix:**
```rust
// Add a new error variant to error.rs:
#[error("EC2 task has no task definition ARN")]
NoTaskDefinitionArn,

// Use it in discovery.rs:
let task_def_arn = task
    .task_definition_arn()
    .ok_or(DiscoveryError::NoTaskDefinitionArn)?;
```

---

### WR-06: Routing-table gossip is published but never consumed by followers

**File:** `src/main.rs:332-341`
**Issue:** `publish_cache_to_gossip` publishes the routing table via `cluster.publish_routing(&json).await`. However, `spawn_follower_sync` only reads `cluster.read_leader_cache()` (the SD target cache) and never calls a `read_leader_routing()` equivalent. Because routing table UUIDs are deterministic (`Uuid::new_v5` over task ARN + container name + container ID), followers correctly rebuild an identical routing table themselves by calling `replace_cache_and_routing(targets)` with the gossip-provided Aws-level targets.

The consequence is that `publish_routing` performs serialization and gossip I/O whose result is silently discarded. This wastes CPU and gossip bandwidth, and more importantly creates a misleading implication that followers rely on this data. Any future developer might remove the `replace_cache_and_routing` call from `spawn_follower_sync` thinking "followers get the routing table via gossip" — which would break proxy mode on followers.

**Fix (two options):**
1. Remove `publish_routing` from `publish_cache_to_gossip` entirely and add a comment on `build_routing_table` documenting that UUIDs are deterministic and followers rebuild locally.
2. If redundancy is desired, implement `read_leader_routing()` in the follower sync and use it instead of rebuilding from targets — and remove the rebuild path from `replace_cache_and_routing` when called on followers.

Option 1 is simpler and correct given the current deterministic UUID design.

---

## Info

### IN-01: `unwrap()` on `Response::builder()` in production paths

**File:** `src/handlers/proxy.rs:166,168`, `src/handlers/metrics.rs:44`
**Issue:** Several production response-building paths use `unwrap()`:
```rust
*resp_builder.headers_mut().unwrap() = upstream_headers;  // proxy.rs:166
resp_builder.body(...).unwrap()                            // proxy.rs:168
.body(Body::from(buffer)).unwrap()                         // metrics.rs:44
```
These are safe in the current code (the builder starts from a valid state in all cases), but `unwrap()` on `Option`/`Result` in production-facing code paths is a maintenance hazard — any future change to the builder initialization (e.g., adding an invalid header) would produce a panic rather than a graceful error response.

**Fix:** Use `.expect("invariant: ...")` with a descriptive message, or propagate via `?` after converting to an appropriate error response.

---

### IN-02: Config validation gives inconsistent error messages for sub-second interval values

**File:** `src/config.rs:219-243`
**Issue:** Both `refresh_interval` and `refresh_min_interval` validation perform two sequential checks:
1. `if value <= Duration::ZERO` → "must be greater than 0"
2. `if value.as_secs() == 0` → "must be at least 1 second"

A value like `500ms` passes check 1 (it is > zero) but fails check 2 (truncated to 0 seconds). The resulting error message "must be at least 1 second" is accurate, but check 1's message "must be greater than 0" is misleading for the `0s` case where it fires — the actual minimum is 1 second, not "any positive value". The two checks are redundant; check 1 is only needed to catch exactly `0s`, which check 2 already covers.

**Fix:** Remove check 1 and rely solely on the `as_secs() == 0` guard:
```rust
let refresh_interval = args.refresh_interval.as_secs();
if refresh_interval == 0 {
    return Err(ConfigError::InvalidValue(
        "refresh_interval must be at least 1 second".to_string(),
    ));
}
```

---

_Reviewed: 2026-07-06T17:04:12Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
