---
plan_id: 02-routes-handlers
phase: 1
wave: 1
depends_on:
  - 01-core-infrastructure
autonomous: true
requirements_addressed:
  - HTTP-01
  - HTTP-02
  - HTTP-03
  - DISC-06
files_modified:
  - src/routes/health.rs
  - src/routes/sd.rs
  - src/routes/mod.rs
  - src/handlers/health.rs
  - src/handlers/sd.rs
  - src/handlers/mod.rs
  - src/models/target.rs
---

# Plan 02: Routes and Handlers

**Objective:** Implement /health and /sd HTTP endpoints with proper JSON responses.

## must_haves

truths:
  - "GET /health returns 200 OK with JSON body {\"status\":\"healthy\"}"
  - "GET /sd returns valid Prometheus http_sd_configs JSON array format"
  - "Query parameters ?cluster= &service= &family= filter results with exact case-sensitive match"
  - "Response Content-Type is application/json"

## tasks

### Task 1: Create Health Route

<read_first>
- src/routes/mod.rs
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (GET /health section)
</read_first>

<acceptance_criteria>
- src/routes/health.rs exists with routes() function
- Returns Router with GET /health route
- Uses crate::handlers::health::health_handler
</acceptance_criteria>

<action>
Create src/routes/health.rs with:

```rust
use axum::{
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::handlers::health;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_handler))
}
```
</action>

---

### Task 2: Create Health Handler

<read_first>
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (GET /health Response section)
</read_first>

<acceptance_criteria>
- src/handlers/health.rs exists with health_handler function
- Returns Json<serde_json::Value> with {"status":"healthy"}
- HTTP status is 200 OK
</acceptance_criteria>

<action>
Create src/handlers/health.rs with:

```rust
use axum::Json;
use serde_json::json;

pub async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy"
    }))
}
```
</action>

---

### Task 3: Create SD Route

<read_first>
- src/routes/mod.rs (needs update)
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (GET /sd section)
</read_first>

<acceptance_criteria>
- src/routes/sd.rs exists with routes() function
- Returns Router with GET /sd route
- Uses crate::handlers::sd::sd_handler
</acceptance_criteria>

<action>
Create src/routes/sd.rs with:

```rust
use axum::{
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::handlers::sd;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sd", get(sd::sd_handler))
}
```
</action>

---

### Task 4: Update Routes Module

<read_first>
- src/routes/mod.rs (current content)
- src/routes/sd.rs (created above)
</read_first>

<acceptance_criteria>
- src/routes/mod.rs includes both health and sd modules
- create_routes() merges both route sets
</acceptance_criteria>

<action>
Replace src/routes/mod.rs with:

```rust
pub mod health;
pub mod sd;

use axum::Router;
use crate::state::AppState;

pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(sd::routes())
}
```
</action>

---

### Task 5: Create FilterParams Model

<read_first>
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 2.4 Query Parameter Extraction)
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (Query Parameter Filtering section)
</read_first>

<acceptance_criteria>
- src/models/mod.rs exports FilterParams
- FilterParams struct exists with cluster, service, family as Option<String>
- Derives Debug and Deserialize
</acceptance_criteria>

<action>
Update src/models/mod.rs to include FilterParams:

```rust
pub mod target;
pub use target::Target;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub family: Option<String>,
}
```
</action>

---

### Task 6: Create SD Handler

<read_first>
- src/state/app_state.rs
- src/models/target.rs
- src/models/mod.rs (FilterParams)
- .planning/phases/01-core-discovery-http-api/01-CONTEXT.md (Query param filtering rules)
</read_first>

<acceptance_criteria>
- src/handlers/sd.rs exists with sd_handler function
- Extracts Query<FilterParams> from request
- Reads cache from AppState
- Filters targets based on query params (exact match, case-sensitive, AND logic)
- Returns Json<Vec<Target>>
</acceptance_criteria>

<action>
Create src/handlers/sd.rs with:

```rust
use axum::{
    extract::{Query, State},
    Json,
};
use crate::state::AppState;
use crate::models::{Target, FilterParams};

pub async fn sd_handler(
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Json<Vec<Target>> {
    let targets = state.cache.read().await.clone();
    let filtered = filter_targets(targets, params);
    Json(filtered)
}

fn filter_targets(targets: Vec<Target>, params: FilterParams) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|target| {
            // Check cluster filter
            if let Some(ref cluster) = params.cluster {
                let target_cluster = target.labels.get("__meta_ecs_cluster_name");
                if target_cluster.map(|s| s.as_str()) != Some(cluster.as_str()) {
                    return false;
                }
            }
            
            // Check service filter
            if let Some(ref service) = params.service {
                let target_service = target.labels.get("__meta_ecs_service_name");
                if target_service.map(|s| s.as_str()) != Some(service.as_str()) {
                    return false;
                }
            }
            
            // Check family filter
            if let Some(ref family) = params.family {
                let target_family = target.labels.get("__meta_ecs_task_family");
                if target_family.map(|s| s.as_str()) != Some(family.as_str()) {
                    return false;
                }
            }
            
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_target(cluster: &str, service: &str, family: &str) -> Target {
        let mut labels = HashMap::new();
        labels.insert("__meta_ecs_cluster_name".to_string(), cluster.to_string());
        labels.insert("__meta_ecs_service_name".to_string(), service.to_string());
        labels.insert("__meta_ecs_task_family".to_string(), family.to_string());
        
        Target {
            targets: vec!["10.0.0.1:8080".to_string()],
            labels,
        }
    }

    #[test]
    fn test_filter_by_cluster() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].labels.get("__meta_ecs_cluster_name"),
            Some(&"prod".to_string())
        );
    }

    #[test]
    fn test_filter_case_sensitive() {
        let targets = vec![
            create_test_target("Prod", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 0); // Case-sensitive: Prod != prod
    }

    #[test]
    fn test_filter_and_logic() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("prod", "web", "web-task"),
            create_test_target("dev", "api", "api-task"),
        ];
        
        let params = FilterParams {
            cluster: Some("prod".to_string()),
            service: Some("api".to_string()),
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_no_params() {
        let targets = vec![
            create_test_target("prod", "api", "api-task"),
            create_test_target("dev", "web", "web-task"),
        ];
        
        let params = FilterParams {
            cluster: None,
            service: None,
            family: None,
        };
        
        let filtered = filter_targets(targets, params);
        assert_eq!(filtered.len(), 2); // No filtering returns all
    }
}
```
</action>

---

### Task 7: Update Handlers Module

<read_first>
- src/handlers/mod.rs (current content)
- src/handlers/sd.rs (created above)
</read_first>

<acceptance_criteria>
- src/handlers/mod.rs includes both health and sd modules
</acceptance_criteria>

<action>
Replace src/handlers/mod.rs with:

```rust
pub mod health;
pub mod sd;
```
</action>

---

### Task 8: Enhance Target Model

<read_first>
- src/models/target.rs (current stub)
- .planning/phases/01-core-discovery-http-api/01-RESEARCH.md (Section 3.2)
</read_first>

<acceptance_criteria>
- Target struct remains unchanged (already correct)
- impl Target provides new() and with_label() methods
- Target derives Serialize, Deserialize, Debug, Clone
</acceptance_criteria>

<action>
Verify src/models/target.rs contains:

```rust
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub targets: Vec<String>,
    pub labels: HashMap<String, String>,
}

impl Target {
    pub fn new(address: String) -> Self {
        Self {
            targets: vec![address],
            labels: HashMap::new(),
        }
    }
    
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}
```
</action>

---

## verification

- [ ] `cargo test` passes (especially filter_targets tests)
- [ ] `cargo build` compiles successfully
- [ ] Server starts and responds:
  - `curl http://localhost:8080/health` returns `{"status":"healthy"}`
  - `curl http://localhost:8080/sd` returns `[]` (empty cache is valid JSON)
  - `curl "http://localhost:8080/sd?cluster=prod"` returns `[]`
- [ ] Content-Type header is `application/json`
