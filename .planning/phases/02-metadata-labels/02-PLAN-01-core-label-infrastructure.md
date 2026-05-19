---
phase: 02-metadata-labels
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - src/models/metadata_level.rs
  - src/models/label_builder.rs
  - src/models/mod.rs
  - src/error.rs
autonomous: true
requirements:
  - META-15
must_haves:
  truths:
    - MetadataLevel enum exists with 5 variants (Container, Task, Service, Cluster, Aws)
    - MetadataLevel implements Default (Task), FromStr, Display
    - LabelBuilder struct exists with level-aware construction
    - LabelBuilder has builder methods for each data type
    - aws-sdk-sts and strum dependencies are added
  artifacts:
    - path: src/models/metadata_level.rs
      provides: MetadataLevel enum with all traits
      exports: [MetadataLevel]
    - path: src/models/label_builder.rs
      provides: LabelBuilder struct and builder API
      exports: [LabelBuilder]
  key_links:
    - from: LabelBuilder
      to: MetadataLevel
      via: level field and includes() method
---

<objective>
Create the foundational label building infrastructure: MetadataLevel enum and LabelBuilder struct.

Purpose: This infrastructure enables the level-aware metadata label system that allows users to request different amounts of metadata (from container-only to full AWS metadata) based on their needs.

Output:
- MetadataLevel enum with 5 hierarchical levels (container < task < service < cluster < aws)
- LabelBuilder struct with consuming builder pattern
- Updated module exports
- New dependencies (aws-sdk-sts, strum)
</objective>

<execution_context>
@/Users/piotrek/.config/opencode/get-shit-done/workflows/execute-plan.md
</execution_context>

<context>
@/Users/piotrek/git/ecs-sd/.planning/ROADMAP.md
@/Users/piotrek/git/ecs-sd/.planning/REQUIREMENTS.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-CONTEXT.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-RESEARCH.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md

## Interface Context

### From PATTERNS.md - MetadataLevel Pattern
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataLevel {
    Container,
    Task,
    Service,
    Cluster,
    Aws,
}

impl MetadataLevel {
    pub fn includes(&self, other: MetadataLevel) -> bool {
        use MetadataLevel::*;
        match (*self, other) {
            (Aws, _) => true,
            (Cluster, Container) | (Cluster, Task) | (Cluster, Service) | (Cluster, Cluster) => true,
            (Service, Container) | (Service, Task) | (Service, Service) => true,
            (Task, Container) | (Task, Task) => true,
            (Container, Container) => true,
            _ => false,
        }
    }
}
```

### From PATTERNS.md - LabelBuilder Pattern
```rust
pub struct LabelBuilder {
    level: MetadataLevel,
    container_data: Option<ContainerData>,
    task_data: Option<TaskData>,
    service_data: Option<ServiceData>,
    cluster_data: Option<ClusterData>,
    aws_data: Option<AwsData>,
}
```
</context>

<tasks>

<task type="auto">
  <name>Task 2-01-01: Add aws-sdk-sts and strum dependencies</name>
  <files>Cargo.toml</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/Cargo.toml` — current dependencies
  </read_first>
  <acceptance_criteria>
    - `aws-sdk-sts = "1.103"` appears in [dependencies] section
    - `strum = { version = "0.28", features = ["derive"] }` appears in [dependencies] section
    - `cargo check` passes with no errors
  </acceptance_criteria>
  <action>
    Add to Cargo.toml [dependencies] section after existing aws-sdk-ec2:
    
    ```toml
    aws-sdk-sts = "1.103"
    strum = { version = "0.28", features = ["derive"] }
    ```
    
    Per D-03: STS needed for account ID retrieval.
    Per RESEARCH.md: strum provides EnumString and Display derives.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Dependencies added and project compiles</done>
</task>

<task type="auto">
  <name>Task 2-01-02: Add StsError variant to DiscoveryError</name>
  <files>src/error.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/error.rs` — existing error variants
  </read_first>
  <acceptance_criteria>
    - `StsError(String)` variant exists in DiscoveryError enum
    - Error message format: "AWS STS API error: {0}"
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Add to src/error.rs in DiscoveryError enum after Ec2Error:
    
    ```rust
    #[error("AWS STS API error: {0}")]
    StsError(String),
    ```
    
    Per D-03: STS client needed for account ID lookup.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>StsError variant added to DiscoveryError</done>
</task>

<task type="auto">
  <name>Task 2-01-03: Create MetadataLevel enum</name>
  <files>src/models/metadata_level.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/models/target.rs` — derive pattern reference
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — full pattern
  </read_first>
  <acceptance_criteria>
    - File exists at src/models/metadata_level.rs
    - Enum has 5 variants: Container, Task, Service, Cluster, Aws
    - Implements: Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize
    - Default returns Task
    - FromStr parses case-insensitive: "container", "task", "service", "cluster", "aws"
    - Display outputs lowercase: "container", "task", "service", "cluster", "aws"
    - includes() method implements hierarchy correctly
    - `cargo test` passes for any tests in the file
  </acceptance_criteria>
  <action>
    Create src/models/metadata_level.rs with:
    
    ```rust
    use serde::{Deserialize, Serialize};
    use std::fmt;
    use std::str::FromStr;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum MetadataLevel {
        Container,
        Task,
        Service,
        Cluster,
        Aws,
    }

    impl Default for MetadataLevel {
        fn default() -> Self {
            MetadataLevel::Task
        }
    }

    impl FromStr for MetadataLevel {
        type Err = String;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.to_lowercase().as_str() {
                "container" => Ok(MetadataLevel::Container),
                "task" => Ok(MetadataLevel::Task),
                "service" => Ok(MetadataLevel::Service),
                "cluster" => Ok(MetadataLevel::Cluster),
                "aws" => Ok(MetadataLevel::Aws),
                _ => Err(format!(
                    "Invalid level: {}. Valid: container, task, service, cluster, aws",
                    s
                )),
            }
        }
    }

    impl fmt::Display for MetadataLevel {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                MetadataLevel::Container => write!(f, "container"),
                MetadataLevel::Task => write!(f, "task"),
                MetadataLevel::Service => write!(f, "service"),
                MetadataLevel::Cluster => write!(f, "cluster"),
                MetadataLevel::Aws => write!(f, "aws"),
            }
        }
    }

    impl MetadataLevel {
        /// Returns true if self includes the given level
        /// e.g., Aws.includes(Task) == true, Task.includes(Aws) == false
        pub fn includes(&self, other: MetadataLevel) -> bool {
            use MetadataLevel::*;
            match (*self, other) {
                (Aws, _) => true,
                (Cluster, Container)
                | (Cluster, Task)
                | (Cluster, Service)
                | (Cluster, Cluster) => true,
                (Service, Container) | (Service, Task) | (Service, Service) => true,
                (Task, Container) | (Task, Task) => true,
                (Container, Container) => true,
                _ => false,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_default_is_task() {
            assert_eq!(MetadataLevel::default(), MetadataLevel::Task);
        }

        #[test]
        fn test_from_str_case_insensitive() {
            assert_eq!("container".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
            assert_eq!("CONTAINER".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
            assert_eq!("Container".parse::<MetadataLevel>().unwrap(), MetadataLevel::Container);
        }

        #[test]
        fn test_from_str_invalid() {
            assert!("invalid".parse::<MetadataLevel>().is_err());
        }

        #[test]
        fn test_display() {
            assert_eq!(MetadataLevel::Container.to_string(), "container");
            assert_eq!(MetadataLevel::Task.to_string(), "task");
            assert_eq!(MetadataLevel::Service.to_string(), "service");
            assert_eq!(MetadataLevel::Cluster.to_string(), "cluster");
            assert_eq!(MetadataLevel::Aws.to_string(), "aws");
        }

        #[test]
        fn test_includes_hierarchy() {
            // Aws includes all
            assert!(MetadataLevel::Aws.includes(MetadataLevel::Container));
            assert!(MetadataLevel::Aws.includes(MetadataLevel::Aws));

            // Container includes only itself
            assert!(MetadataLevel::Container.includes(MetadataLevel::Container));
            assert!(!MetadataLevel::Container.includes(MetadataLevel::Task));

            // Task includes container and task
            assert!(MetadataLevel::Task.includes(MetadataLevel::Container));
            assert!(MetadataLevel::Task.includes(MetadataLevel::Task));
            assert!(!MetadataLevel::Task.includes(MetadataLevel::Service));

            // Service includes container, task, service
            assert!(MetadataLevel::Service.includes(MetadataLevel::Container));
            assert!(MetadataLevel::Service.includes(MetadataLevel::Task));
            assert!(MetadataLevel::Service.includes(MetadataLevel::Service));
            assert!(!MetadataLevel::Service.includes(MetadataLevel::Cluster));
        }
    }
    ```
    
    Per D-01 and RESEARCH.md: MetadataLevel defines the 5 levels with proper hierarchy.
  </action>
  <verify>
    <automated>cargo test metadata_level</automated>
  </verify>
  <done>MetadataLevel enum created with all traits and tests</done>
</task>

<task type="auto">
  <name>Task 2-01-04: Create LabelBuilder struct with API</name>
  <files>src/models/label_builder.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/models/metadata_level.rs` — MetadataLevel enum (from previous task)
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — full LabelBuilder pattern
    - `/Users/piotrek/git/ecs-sd/src/models/target.rs` — builder pattern reference
  </read_first>
  <acceptance_criteria>
    - File exists at src/models/label_builder.rs
    - LabelBuilder struct has level field and Option<T> data fields for all 5 levels
    - Implements new(level) constructor
    - Implements with_container(), with_task(), with_service(), with_cluster(), with_aws() methods
    - Implements build() that returns HashMap<String, String>
    - Uses consuming builder pattern (mut self -> Self)
    - Only includes labels for levels where level.includes(target_level) is true
    - `cargo test` passes for any tests in the file
  </acceptance_criteria>
  <action>
    Create src/models/label_builder.rs with:
    
    ```rust
    use crate::models::metadata_level::MetadataLevel;
    use aws_sdk_ecs::types::{ContainerDefinition, Service, Cluster};
    use aws_sdk_ecs::types::{Task, TaskDefinition};
    use std::collections::HashMap;

    pub struct LabelBuilder {
        level: MetadataLevel,
        container_data: Option<ContainerData>,
        task_data: Option<TaskData>,
        service_data: Option<ServiceData>,
        cluster_data: Option<ClusterData>,
        aws_data: Option<AwsData>,
    }

    struct ContainerData {
        name: String,
        image: String,
        port: u16,
    }

    struct TaskData {
        arn: String,
        family: String,
        version: String,
    }

    struct ServiceData {
        name: String,
        desired_count: i32,
        running_count: i32,
    }

    struct ClusterData {
        name: String,
        arn: String,
    }

    struct AwsData {
        region: String,
        account_id: String,
        availability_zone: Option<String>,
    }

    impl LabelBuilder {
        pub fn new(level: MetadataLevel) -> Self {
            Self {
                level,
                container_data: None,
                task_data: None,
                service_data: None,
                cluster_data: None,
                aws_data: None,
            }
        }

        pub fn with_container(mut self, def: &ContainerDefinition, port: u16) -> Self {
            self.container_data = Some(ContainerData {
                name: def.name().unwrap_or("unknown").to_string(),
                image: def.image().unwrap_or("unknown").to_string(),
                port,
            });
            self
        }

        pub fn with_task(mut self, task: &Task, task_def: &TaskDefinition) -> Self {
            let version = task_def
                .task_definition_arn()
                .and_then(|arn| arn.split(':').last())
                .unwrap_or("unknown")
                .to_string();

            self.task_data = Some(TaskData {
                arn: task.task_arn().unwrap_or("unknown").to_string(),
                family: task_def.family().unwrap_or("unknown").to_string(),
                version,
            });
            self
        }

        pub fn with_service(mut self, service: &Service) -> Self {
            self.service_data = Some(ServiceData {
                name: service.service_name().unwrap_or("unknown").to_string(),
                desired_count: service.desired_count().unwrap_or(0),
                running_count: service.running_count().unwrap_or(0),
            });
            self
        }

        pub fn with_cluster(mut self, cluster: &Cluster) -> Self {
            self.cluster_data = Some(ClusterData {
                name: cluster.cluster_name().unwrap_or("unknown").to_string(),
                arn: cluster.cluster_arn().unwrap_or("unknown").to_string(),
            });
            self
        }

        pub fn with_aws(
            mut self,
            region: &str,
            account_id: &str,
            az: Option<&str>,
        ) -> Self {
            self.aws_data = Some(AwsData {
                region: region.to_string(),
                account_id: account_id.to_string(),
                availability_zone: az.map(|s| s.to_string()),
            });
            self
        }

        pub fn build(self) -> HashMap<String, String> {
            let mut labels = HashMap::new();

            // Container level labels (META-01, META-02, META-03)
            if self.level.includes(MetadataLevel::Container) {
                if let Some(data) = self.container_data {
                    labels.insert("__meta_ecs_container_name".to_string(), data.name);
                    labels.insert("__meta_ecs_container_image".to_string(), data.image);
                    labels.insert("__meta_ecs_metrics_port".to_string(), data.port.to_string());
                }
            }

            // Task level labels (META-04, META-05, META-06)
            if self.level.includes(MetadataLevel::Task) {
                if let Some(data) = self.task_data {
                    labels.insert("__meta_ecs_task_arn".to_string(), data.arn);
                    labels.insert("__meta_ecs_task_family".to_string(), data.family);
                    labels.insert("__meta_ecs_task_version".to_string(), data.version);
                }
            }

            // Service level labels (META-07, META-08, META-09)
            if self.level.includes(MetadataLevel::Service) {
                if let Some(data) = self.service_data {
                    labels.insert("__meta_ecs_service_name".to_string(), data.name);
                    labels.insert(
                        "__meta_ecs_desired_count".to_string(),
                        data.desired_count.to_string(),
                    );
                    labels.insert(
                        "__meta_ecs_running_count".to_string(),
                        data.running_count.to_string(),
                    );
                }
            }

            // Cluster level labels (META-10, META-11)
            if self.level.includes(MetadataLevel::Cluster) {
                if let Some(data) = self.cluster_data {
                    labels.insert("__meta_ecs_cluster_name".to_string(), data.name);
                    labels.insert("__meta_ecs_cluster_arn".to_string(), data.arn);
                }
            }

            // AWS level labels (META-12, META-13, META-14)
            if self.level.includes(MetadataLevel::Aws) {
                if let Some(data) = self.aws_data {
                    labels.insert("__meta_ecs_region".to_string(), data.region);
                    labels.insert("__meta_ecs_account_id".to_string(), data.account_id);
                    if let Some(az) = data.availability_zone {
                        labels.insert("__meta_ecs_availability_zone".to_string(), az);
                    }
                }
            }

            labels
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_label_builder_container_level() {
            let labels = LabelBuilder::new(MetadataLevel::Container).build();
            assert!(labels.is_empty()); // No data added, so no labels
        }

        #[test]
        fn test_label_builder_includes_hierarchy() {
            // Container level only includes container labels
            let builder = LabelBuilder::new(MetadataLevel::Container);
            assert!(builder.level.includes(MetadataLevel::Container));
            assert!(!builder.level.includes(MetadataLevel::Task));

            // Aws level includes all
            let builder = LabelBuilder::new(MetadataLevel::Aws);
            assert!(builder.level.includes(MetadataLevel::Container));
            assert!(builder.level.includes(MetadataLevel::Task));
            assert!(builder.level.includes(MetadataLevel::Service));
            assert!(builder.level.includes(MetadataLevel::Cluster));
            assert!(builder.level.includes(MetadataLevel::Aws));
        }
    }
    ```
    
    Per D-01: LabelBuilder struct provides level-aware label construction.
    Per REQUIREMENTS.md META-01..14: All 14 label names defined in build() method.
    Per D-05: Labels omitted entirely when data unavailable (checked in build()).
  </action>
  <verify>
    <automated>cargo test label_builder</automated>
  </verify>
  <done>LabelBuilder struct created with full API and tests</done>
</task>

<task type="auto">
  <name>Task 2-01-05: Export MetadataLevel and LabelBuilder from models module</name>
  <files>src/models/mod.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/models/mod.rs` — current exports
  </read_first>
  <acceptance_criteria>
    - src/models/mod.rs exports metadata_level module and MetadataLevel
    - src/models/mod.rs exports label_builder module and LabelBuilder
    - `cargo check` passes
    - Can import via `use crate::models::{MetadataLevel, LabelBuilder};`
  </acceptance_criteria>
  <action>
    Modify src/models/mod.rs to add after existing target exports:
    
    ```rust
    pub mod target;
    pub use target::Target;

    pub mod metadata_level;
    pub use metadata_level::MetadataLevel;

    pub mod label_builder;
    pub use label_builder::LabelBuilder;

    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct FilterParams {
        pub cluster: Option<String>,
        pub service: Option<String>,
        pub family: Option<String>,
    }
    ```
    
    Per PATTERNS.md: Standard module export pattern.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Module exports updated</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Input → Query Param | MetadataLevel from_str validates level strings |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-01 | Tampering | MetadataLevel parsing | mitigate | FromStr validates against allowed values only |
| T-02-02 | Information Disclosure | Label values | accept | Labels contain AWS metadata, which is the intended purpose |
</threat_model>

<verification>
## Wave 1 Verification

### Automated Tests
```bash
cargo test metadata_level    # Tests enum traits and includes() logic
cargo test label_builder     # Tests builder API
cargo check                  # All modules compile
```

### Coverage Check
- [ ] MetadataLevel enum exists with all variants
- [ ] Default level is Task
- [ ] FromStr parses all 5 levels case-insensitively
- [ ] FromStr returns error for invalid levels
- [ ] includes() method correctly implements hierarchy
- [ ] LabelBuilder has all 5 with_* methods
- [ ] build() returns HashMap with correct label names
</verification>

<success_criteria>
1. `cargo test` passes for metadata_level and label_builder modules
2. All 5 MetadataLevel variants can be parsed from strings
3. LabelBuilder API is complete (new + 5 with_* methods + build)
4. All 14 label names defined in build() method
5. Module exports allow importing MetadataLevel and LabelBuilder
</success_criteria>

<output>
After completion, create `.planning/phases/02-metadata-labels/02-SUMMARY-01-core-label-infrastructure.md`
</output>
