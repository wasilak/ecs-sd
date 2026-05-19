---
phase: 02-metadata-labels
plan: 02
type: execute
wave: 1
depends_on:
  - 02-PLAN-01-core-label-infrastructure.md
files_modified:
  - src/aws/client.rs
  - src/aws/discovery.rs
  - src/aws/mod.rs
  - src/state/app_state.rs
  - src/main.rs
autonomous: true
requirements:
  - META-01
  - META-02
  - META-03
  - META-04
  - META-05
  - META-06
  - META-07
  - META-08
  - META-09
  - META-10
  - META-11
  - META-12
  - META-13
  - META-14
must_haves:
  truths:
    - STS client created and used for account ID lookup
    - DiscoveryService caches region and account_id
    - DiscoveryService::new is async and returns Result
    - LabelBuilder used to construct all labels
    - Availability zone extracted from EC2 DescribeInstances
    - All 14 labels populated correctly
  artifacts:
    - path: src/aws/client.rs
      provides: create_sts_client() function
      exports: [create_sts_client]
    - path: src/aws/discovery.rs
      provides: Updated DiscoveryService with STS, all labels
      exports: [DiscoveryService]
    - path: src/state/app_state.rs
      provides: Updated AppState::new as async with STS client
      exports: [AppState]
  key_links:
    - from: DiscoveryService
      to: LabelBuilder
      via: with_container, with_task, with_service, with_cluster, with_aws calls
    - from: DiscoveryService
      to: STS
      via: sts_client.get_caller_identity()
---

<objective>
Integrate LabelBuilder into the discovery flow and implement all 14 metadata labels.

Purpose: Connect the label building infrastructure to actual AWS API calls, populating all metadata labels from container, task, service, cluster, and AWS levels.

Output:
- STS client creation in aws/client.rs
- Updated DiscoveryService with STS client and cached metadata
- LabelBuilder integration in discovery flow
- All 14 labels populated from AWS SDK objects
- Updated AppState and main.rs for async DiscoveryService construction
</objective>

<execution_context>
@/Users/piotrek/.config/opencode/get-shit-done/workflows/execute-plan.md
</execution_context>

<context>
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-CONTEXT.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-RESEARCH.md
@/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md
@/Users/piotrek/git/ecs-sd/src/aws/discovery.rs — current implementation
@/Users/piotrek/git/ecs-sd/src/state/app_state.rs — current AppState

## Interface Context

### From 01-SUMMARY-03-discovery-logic.md
Current DiscoveryService::new signature:
```rust
pub fn new(ecs_client: aws_sdk_ecs::Client, ec2_client: aws_sdk_ec2::Client) -> Self
```

### From PATTERNS.md - New DiscoveryService
```rust
pub struct DiscoveryService {
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    sts_client: aws_sdk_sts::Client,
    account_id: String,
    region: String,
}

pub async fn new(
    ecs_client: aws_sdk_ecs::Client,
    ec2_client: aws_sdk_ec2::Client,
    sts_client: aws_sdk_sts::Client,
    region: String,
) -> Result<Self, DiscoveryError>
```

### From PATTERNS.md - STS Client Creation
```rust
pub async fn create_sts_client() -> aws_sdk_sts::Client {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    aws_sdk_sts::Client::new(&config)
}
```

### Label Integration Point
In resolve_target_address, extract AZ and pass to LabelBuilder:
```rust
let availability_zone = instances
    .reservations()
    .first()
    .and_then(|r| r.instances().first())
    .and_then(|i| i.placement())
    .and_then(|p| p.availability_zone())
    .map(|s| s.to_string());

let labels = LabelBuilder::new(MetadataLevel::Aws)
    .with_container(container_def, port)
    .with_task(task, &task_def)
    .with_service(service)
    .with_cluster(cluster)
    .with_aws(&self.region, &self.account_id, availability_zone.as_deref())
    .build();
```
</context>

<tasks>

<task type="auto">
  <name>Task 2-02-01: Create STS client factory</name>
  <files>src/aws/client.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/aws/mod.rs` — check if client.rs is exported
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — STS client pattern
  </read_first>
  <acceptance_criteria>
    - File exists at src/aws/client.rs
    - Exports create_sts_client() async function
    - Returns aws_sdk_sts::Client
    - Uses aws_config::load_defaults with BehaviorVersion::latest()
    - Added to src/aws/mod.rs exports
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Create src/aws/client.rs:
    
    ```rust
    use aws_config::BehaviorVersion;

    pub async fn create_sts_client() -> aws_sdk_sts::Client {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        aws_sdk_sts::Client::new(&config)
    }
    ```
    
    Per D-03: STS client needed for account ID retrieval.
    Per RESEARCH.md: Uses official AWS SDK pattern for client creation.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>STS client factory created</done>
</task>

<task type="auto">
  <name>Task 2-02-02: Export client module from aws</name>
  <files>src/aws/mod.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/aws/mod.rs` — current contents
  </read_first>
  <acceptance_criteria>
    - src/aws/mod.rs exports client module
    - src/aws/mod.rs exports discovery module
    - Can import via `use crate::aws::{client, discovery};`
  </acceptance_criteria>
  <action>
    Create or update src/aws/mod.rs:
    
    ```rust
    pub mod client;
    pub mod discovery;
    
    pub use discovery::DiscoveryService;
    ```
    
    If file doesn't exist, create it. If it exists, ensure client module is exported.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>AWS module exports updated</done>
</task>

<task type="auto">
  <name>Task 2-02-03: Update DiscoveryService with STS and metadata caching</name>
  <files>src/aws/discovery.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/aws/discovery.rs` — full current implementation
    - `/Users/piotrek/git/ecs-sd/src/models/label_builder.rs` — LabelBuilder API (from Plan 01)
    - `/Users/piotrek/git/ecs-sd/src/models/metadata_level.rs` — MetadataLevel (from Plan 01)
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — pattern for DiscoveryService changes
  </read_first>
  <acceptance_criteria>
    - DiscoveryService struct has sts_client, account_id, region fields
    - new() is async and returns Result<Self, DiscoveryError>
    - new() calls sts_client.get_caller_identity() to get account_id
    - StsError handled appropriately
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/aws/discovery.rs:
    
    1. Add imports at top:
    ```rust
    use crate::models::{LabelBuilder, MetadataLevel};
    use aws_sdk_sts::Client as StsClient;
    ```
    
    2. Update struct definition (lines 7-11):
    ```rust
    #[derive(Clone)]
    pub struct DiscoveryService {
        ecs_client: aws_sdk_ecs::Client,
        ec2_client: aws_sdk_ec2::Client,
        sts_client: StsClient,
        account_id: String,
        region: String,
    }
    ```
    
    3. Replace constructor (lines 13-19) with async version:
    ```rust
    impl DiscoveryService {
        pub async fn new(
            ecs_client: aws_sdk_ecs::Client,
            ec2_client: aws_sdk_ec2::Client,
            sts_client: StsClient,
            region: String,
        ) -> Result<Self, DiscoveryError> {
            // Get account ID from STS
            let caller_identity = sts_client
                .get_caller_identity()
                .send()
                .await
                .map_err(|e| DiscoveryError::StsError(e.to_string()))?;

            let account_id = caller_identity
                .account()
                .ok_or_else(|| DiscoveryError::StsError("No account ID in response".to_string()))?
                .to_string();

            Ok(Self {
                ecs_client,
                ec2_client,
                sts_client,
                account_id,
                region,
            })
        }
    ```
    
    Per D-03: STS GetCallerIdentity retrieves account ID once at startup.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>DiscoveryService updated with STS and metadata caching</done>
</task>

<task type="auto">
  <name>Task 2-02-04: Integrate LabelBuilder into discovery flow with all labels</name>
  <files>src/aws/discovery.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/aws/discovery.rs` — current target building code around lines 175-185
    - `/Users/piotrek/git/ecs-sd/src/models/label_builder.rs` — LabelBuilder API
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — pattern for label building integration
  </read_first>
  <acceptance_criteria>
    - resolve_target_address returns (String, Option<String>) for address and AZ
    - discover_cluster_targets passes cluster reference to LabelBuilder
    - discover_cluster_targets passes service reference to LabelBuilder
    - All 14 labels are built via LabelBuilder
    - Target constructed with labels from LabelBuilder
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/aws/discovery.rs:
    
    1. Update resolve_target_address signature and return (lines 269-314):
    ```rust
    async fn resolve_target_address(
        &self,
        container_instance_arn: &str,
        port: u16,
    ) -> Result<(String, Option<String>), DiscoveryError> {
        // ... existing cluster extraction (lines 276-279) ...
        
        // ... existing container instance lookup (lines 282-295) ...
        
        // ... existing EC2 describe instances (lines 298-304) ...
        
        let private_ip = instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.private_ip_address())
            .ok_or(DiscoveryError::NoPrivateIp)?;
        
        // Extract availability zone
        let availability_zone = instances
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.placement())
            .and_then(|p| p.availability_zone())
            .map(|s| s.to_string());

        Ok((format!("{}:{}", private_ip, port), availability_zone))
    }
    ```
    
    2. Update target building in discover_cluster_targets (around lines 175-192):
    ```rust
    // Replace the existing target construction block:
    match self.resolve_target_address(container_instance_arn, port).await {
        Ok((address, availability_zone)) => {
            let labels = LabelBuilder::new(MetadataLevel::Aws)
                .with_container(container_def, port)
                .with_task(task, &task_def)
                .with_service(service)
                .with_cluster(cluster)
                .with_aws(&self.region, &self.account_id, availability_zone.as_deref())
                .build();

            let target = Target {
                targets: vec![address],
                labels,
            };

            targets.push(target);
        }
        Err(e) => {
            warn!("Failed to resolve target address: {}", e);
            continue;
        }
    }
    ```
    
    Per D-01: LabelBuilder used for level-aware construction.
    Per D-03: AZ extracted from EC2, region and account_id from cached values.
    Per META-01..14: All 14 labels populated via LabelBuilder.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>LabelBuilder integrated, all 14 labels populated</done>
</task>

<task type="auto">
  <name>Task 2-02-05: Update AppState for async DiscoveryService construction</name>
  <files>src/state/app_state.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/state/app_state.rs` — current implementation
    - `/Users/piotrek/git/ecs-sd/.planning/phases/02-metadata-labels/02-PATTERNS.md` — pattern for AppState changes
  </read_first>
  <acceptance_criteria>
    - AppState::new is async
    - AppState::new takes sts_client and region parameters
    - AppState::new returns Result<Self, DiscoveryError>
    - DiscoveryService constructed with await?
    - `cargo check` passes
  </acceptance_criteria>
  <action>
    Modify src/state/app_state.rs:
    
    ```rust
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use crate::config::Config;
    use crate::error::DiscoveryError;
    use crate::models::Target;
    use crate::aws::DiscoveryService;

    #[derive(Clone)]
    pub struct AppState {
        pub cache: Arc<RwLock<Vec<Target>>>,
        pub config: Arc<Config>,
        pub discovery: DiscoveryService,
    }

    impl AppState {
        pub async fn new(
            config: Config,
            ecs_client: aws_sdk_ecs::Client,
            ec2_client: aws_sdk_ec2::Client,
            sts_client: aws_sdk_sts::Client,
            region: String,
        ) -> Result<Self, DiscoveryError> {
            let discovery = DiscoveryService::new(ecs_client, ec2_client, sts_client, region).await?;

            Ok(Self {
                cache: Arc::new(RwLock::new(Vec::new())),
                config: Arc::new(config),
                discovery,
            })
        }
    }
    ```
    
    Per D-03: STS client passed through for DiscoveryService initialization.
    Per PATTERNS.md: Async constructor pattern for async STS call.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>AppState updated for async construction</done>
</task>

<task type="auto">
  <name>Task 2-02-06: Update main.rs for STS client and async AppState</name>
  <files>src/main.rs</files>
  <read_first>
    - `/Users/piotrek/git/ecs-sd/src/main.rs` — current implementation
  </read_first>
  <acceptance_criteria>
    - main.rs creates STS client using aws::client::create_sts_client
    - main.rs extracts region from aws_config::SdkConfig
    - AppState::new called with .await?
    - Error handling for DiscoveryService initialization
    - `cargo build` passes
  </acceptance_criteria>
  <action>
    Modify src/main.rs (around the AppState creation section):
    
    ```rust
    // After creating ECS and EC2 clients:
    let sts_client = aws::client::create_sts_client().await;
    
    // Extract region from config
    let region = sdk_config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "us-east-1".to_string());
    
    // Create AppState with async constructor
    let app_state = AppState::new(
        config,
        ecs_client,
        ec2_client,
        sts_client,
        region,
    )
    .await
    .map_err(|e| {
        eprintln!("Failed to initialize discovery service: {}", e);
        std::process::exit(1);
    })?;
    ```
    
    Per D-03: Region extracted from SDK config, STS client created and passed.
    Per RESEARCH.md: Uses SdkConfig.region() for region detection.
  </action>
  <verify>
    <automated>cargo build</automated>
  </verify>
  <done>main.rs updated for STS and async AppState</done>
</task>

<task type="auto">
  <name>Task 2-02-07: Build and verify compilation</name>
  <files></files>
  <read_first>
  </read_first>
  <acceptance_criteria>
    - `cargo build` passes with no errors
    - All warnings reviewed and acceptable
  </acceptance_criteria>
  <action>
    Run full build to verify all integrations work:
    
    ```bash
    cargo build
    ```
    
    Address any compilation errors that arise from the integration.
  </action>
  <verify>
    <automated>cargo build</automated>
  </verify>
  <done>Full build passes</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| AWS API → DiscoveryService | STS credentials for account lookup |
| EC2 Response → LabelBuilder | Availability zone extraction |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-03 | Information Disclosure | STS credentials in logs | mitigate | Never log STS credentials or full Identity response |
| T-02-04 | Denial of Service | STS call failure on startup | mitigate | Log error clearly, fail fast with descriptive message |
| T-02-05 | Information Disclosure | EC2 placement data | accept | AZ is non-sensitive infrastructure metadata |
</threat_model>

<verification>
## Wave 1 Verification

### Automated Tests
```bash
cargo build              # Full build passes
cargo test               # All tests pass
```

### Coverage Check
- [ ] STS client factory exists
- [ ] DiscoveryService has sts_client, account_id, region fields
- [ ] DiscoveryService::new is async with Result return
- [ ] STS GetCallerIdentity called in constructor
- [ ] resolve_target_address returns (address, az) tuple
- [ ] LabelBuilder::with_aws called with region, account_id, az
- [ ] AppState::new is async with STS client parameter
- [ ] main.rs extracts region from SdkConfig
- [ ] Full build compiles
</verification>

<success_criteria>
1. `cargo build` passes with no errors
2. DiscoveryService caches account_id and region from STS/SDK
3. All 14 labels are constructed via LabelBuilder
4. Availability zone extracted from EC2 DescribeInstances
5. AppState and main.rs properly handle async DiscoveryService construction
</success_criteria>

<output>
After completion, create `.planning/phases/02-metadata-labels/02-SUMMARY-02-label-implementation.md`
</output>
