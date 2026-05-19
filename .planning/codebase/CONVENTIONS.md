---
title: Code Conventions
created: 2026-05-19
codebase: ecs-sd
---

# Code Conventions

## Code Style

### Formatting
- Standard Rust formatting (`rustfmt` defaults)
- 4-space indentation
- No custom rustfmt configuration detected

### Line Length
- No strict limit observed
- Code is reasonably compact (~100 lines total)

## Naming Patterns

### Functions
- **Snake case:** `show_clusters`, `list_tasks_in_cluster`
- **Descriptive:** Function names describe behavior

### Variables
- **Snake case:** `region_provider`, `clusters_list`, `client`
- **Clear intent:** Names indicate purpose

### Types
Uses AWS SDK types directly:
- `aws_sdk_ecs::Client`
- `aws_sdk_ecs::types::Cluster`
- `aws_sdk_ecs::Error`

## Code Patterns

### Async/Await
```rust
#[tokio::main]
async fn main() -> Result<(), Error> {
    // async code with .await
}
```

### AWS Client Pattern
```rust
let config = aws_config::defaults(BehaviorVersion::latest())
    .region(region_provider)
    .load()
    .await;
let client = Client::new(&config);
```

### Builder Pattern (AWS SDK)
```rust
client
    .describe_clusters()
    .set_clusters(Some(clusters_list.into()))
    .send()
    .await?;
```

### Option Handling
Mix of patterns observed:
- `Some(...)` for wrapping values
- `.map(|s| s.to_string())` for conversion
- `.unwrap()` for quick unwrapping (not production-ready)
- `if let Some(...)` for conditional processing

## Error Handling

### Current Approach
- **Propagating errors:** Main uses `Result` with `?` operator
- **Unwrapping:** Task listing uses `.unwrap()` extensively

**Example - Propagation (preferred):**
```rust
async fn show_clusters(...) -> Result<..., aws_sdk_ecs::Error> {
    let clusters = client.describe_clusters().send().await?;
    Ok(Some(clusters.clusters().to_vec()))
}
```

**Example - Unwrapping (quick but risky):**
```rust
let tasks_list = client.list_tasks().send().await.unwrap();
```

### Error Types
- `aws_sdk_ecs::Error` - AWS SDK errors
- Standard Rust error handling via `Result<T, E>`

## Code Organization

### Single File Structure
```rust
// 1. Imports
use aws_config::...;
use aws_sdk_ecs::...;

// 2. Main entry
#[tokio::main]
async fn main() -> Result<(), Error> { ... }

// 3. Helper functions
async fn show_clusters(...) { ... }
async fn list_tasks_in_cluster(...) { ... }
```

### No Modules
- No `mod` declarations
- No separate files for functions
- All code in `main.rs`

## Comments
- **Minimal commenting** - Code is mostly self-documenting
- No doc comments (`///`) found
- No module-level documentation (`//!`)

## Hardcoded Values
Several hardcoded values present:

```rust
// Region fallback
.or_else("us-east-1")

// Cluster list (lines 15-18)
vec![
    "service-platform-default".to_string(),
    "arn:aws:ecs:eu-west-1:723255075185:cluster/service-platform-default".to_string(),
]
```

## Potential Improvements
1. Replace `.unwrap()` calls with proper error handling
2. Extract hardcoded values to configuration
3. Add doc comments for public functions
4. Consider splitting into modules as codebase grows
5. Add input validation for cluster ARNs

---

*Document generated: 2026-05-19*
