---
title: Architecture
created: 2026-05-19
codebase: ecs-sd
---

# Architecture

## System Pattern
This is a **simple CLI tool** with a linear execution flow. It follows a straightforward procedural pattern with async/await for AWS API calls.

## Architecture Overview

```
┌─────────────────┐
│   main()        │  Entry point with tokio::main
│   (async)       │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  AWS Config     │  Configure region, credentials
│  Setup          │  Default provider chain
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ECS Client     │  Create SDK client
│  Creation       │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ show_clusters() │────▶│ DescribeClusters│
│ (async fn)      │     │ AWS API call    │
└────────┬────────┘     └─────────────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│list_tasks_in_   │────▶│ ListTasks       │
│cluster()         │     │ DescribeTasks   │
│(async fn)       │     │ DescribeTaskDef │
└─────────────────┘     └─────────────────┘
```

## Entry Points

### Main Entry Point
**File:** `src/main.rs`
**Function:** `main()` (lines 5-25)

**Flow:**
1. Configure AWS region provider (default → us-east-1 fallback)
2. Load AWS configuration
3. Create ECS client
4. Call `show_clusters()` with hardcoded cluster list
5. Call `list_tasks_in_cluster()` with results

## Core Functions

### `show_clusters()`
**Location:** `src/main.rs:27-40`
**Purpose:** Retrieve cluster information from ECS

**Signature:**
```rust
async fn show_clusters(
    client: &aws_sdk_ecs::Client,
    clusters_list: &[String],
) -> Result<Option<Vec<aws_sdk_ecs::types::Cluster>>, aws_sdk_ecs::Error>
```

**Behavior:**
- Accepts a list of cluster identifiers (names or ARNs)
- Calls `describe_clusters` API
- Returns cluster details or error

### `list_tasks_in_cluster()`
**Location:** `src/main.rs:42-100`
**Purpose:** List and describe tasks with container details

**Signature:**
```rust
async fn list_tasks_in_cluster(
    client: &aws_sdk_ecs::Client,
    cluster: Option<Vec<aws_sdk_ecs::types::Cluster>>,
)
```

**Behavior:**
- Iterates through provided clusters
- For each cluster:
  1. Lists tasks using `list_tasks`
  2. Describes tasks using `describe_tasks`
  3. For each task, describes task definition
  4. Prints container definitions and environment variables

## Data Flow

```
Cluster Names/ARNs
       │
       ▼
┌──────────────┐
│describe_clusters│
└──────────────┘
       │
       ▼
Cluster Details ──────► Print cluster info
       │
       ▼
┌──────────────┐
│  list_tasks  │
└──────────────┘
       │
       ▼
Task ARNs
       │
       ▼
┌──────────────┐
│describe_tasks│
└──────────────┘
       │
       ▼
Task Details ─────────► Print task info
       │
       ▼
┌──────────────┐
│describe_task_│
│  definition  │
└──────────────┘
       │
       ▼
Container Definitions ─► Print containers + env vars
```

## Error Handling
- Uses `Result<T, aws_sdk_ecs::Error>` for AWS operations
- `unwrap()` used in `list_tasks_in_cluster()` for quick prototyping
- Main function propagates errors with `?` operator

## Async Model
- **Runtime:** Tokio with full features
- **Pattern:** Sequential async/await
- No parallel processing or concurrency currently implemented

---

*Document generated: 2026-05-19*
