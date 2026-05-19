---
title: Technology Stack
created: 2026-05-19
codebase: ecs-sd
---

# Technology Stack

## Overview
This project is a Rust-based AWS ECS service discovery tool that interacts with Amazon Elastic Container Service (ECS) to list clusters, tasks, and container definitions.

## Languages & Runtime

| Component | Technology | Version |
|-----------|------------|---------|
| Language | Rust | 2024 Edition |
| Async Runtime | Tokio | 1.52.2 |

## Core Dependencies

### AWS SDK
- **`aws-config`** (v1.8.16) - AWS SDK configuration and credential management
- **`aws-sdk-ecs`** (v1.124.0) - AWS ECS service client for cluster and task operations

### Async Runtime
- **`tokio`** (v1.52.2, features: "full") - Async runtime for Rust with full feature set including:
  - Multi-threaded runtime
  - All I/O drivers (TCP, UDP, Unix sockets)
  - All runtime components (process, signal, time)
  - Macros for async main and testing

## Build System
- **Cargo** - Rust's build system and package manager
- Edition: 2024 (latest Rust edition)

## Configuration Files

| File | Purpose |
|------|---------|
| `Cargo.toml` | Package manifest with dependencies and metadata |
| `Cargo.lock` | Dependency version lockfile |
| `.gitignore` | Git ignore patterns (`/target`, `.serena`) |

## Package Information
- **Name**: `ecs-sd`
- **Version**: `0.1.0`
- **Edition**: 2024

## Dependencies Summary
```toml
[dependencies]
aws-config = "1.8.16"
aws-sdk-ecs = "1.124.0"
tokio = { version = "1.52.2", features = ["full"] }
```

## Key Technical Decisions
1. **AWS SDK v1** - Uses the latest generation AWS SDK for Rust
2. **Tokio Full Features** - Enables all async capabilities for flexibility
3. **2024 Edition** - Uses the most recent Rust edition for latest language features

---

*Document generated: 2026-05-19*
