# Phase 2: Metadata Labels - Context

**Gathered:** 2026-05-19
**Status:** Ready for planning

---

## Phase Boundary

This phase delivers a complete metadata label system with 5 hierarchical levels (container → task → service → cluster → aws). All 14 labels from REQUIREMENTS.md META-01..14 must be implemented, plus global default and per-request level configuration (META-15..16).

**Key outcomes:**
1. `LabelBuilder` struct for level-aware label construction
2. `?level=<level>` query parameter support
3. All AWS-level metadata extraction (region, account ID, AZ)
4. Multi-tier cache supporting per-request level overrides
5. Proper handling of missing/optional metadata

---

## Implementation Decisions

### D-01: Label Building Architecture

**Pattern:** `LabelBuilder` struct with level-aware construction

- **Location:** `src/models/label_builder.rs`
- **Design:** Level-aware builder that takes `MetadataLevel` enum
- **Data input:** Full AWS SDK objects (Service, Cluster, Task, etc.)
- **Interface:** Builder methods like `.with_container_data()`, `.with_task_data()`, etc.
- **Output:** Returns `HashMap<String, String>` of labels for the target level

**Rationale:** Separates label formatting from discovery orchestration. Builder encapsulates AWS SDK field extraction knowledge.

---

### D-02: Metadata Level Filtering

**Timing:** At discovery time (filter what we fetch/build)

- DiscoveryService receives level parameter
- Level hierarchy: `container < task < service < cluster < aws`
- Higher levels include all lower-level labels

**Level flow:**
- DiscoveryService stores default level (from `--metadata-level` flag)
- Per-call override supported for `?level=` query parameter
- Method signature: `discover_all_clusters(clusters, level_override: Option<MetadataLevel>)`

**Cache strategy:**
- Single cache storing targets with all labels (aws-level)
- Handler filters labels at response time based on requested level
- Memory overhead acceptable for typical ECS scale (hundreds of tasks)

**Invalid levels:** Return 400 Bad Request with message:
```
Invalid level: foo. Valid: container, task, service, cluster, aws
```

---

### D-03: AWS-Level Metadata Extraction

**Account ID (`__meta_ecs_account_id`):**
- Source: STS `GetCallerIdentity` API call
- Timing: Once at DiscoveryService startup
- Storage: Cached in DiscoveryService struct

**Region (`__meta_ecs_region`):**
- Source: `aws_config::SdkConfig.region()`
- Timing: At startup when creating DiscoveryService
- Storage: Cached alongside account ID

**Availability Zone (`__meta_ecs_availability_zone`):**
- Source: EC2 `DescribeInstances` response (already called for private IP)
- Field: `placement.availability_zone`
- No extra API calls required

**Missing data handling:**
- Lenient approach — include targets with partial metadata
- Omit labels that can't be populated (don't include empty strings)
- Log missing data at debug level for troubleshooting

---

### D-04: Per-Request Level Override

**Implementation:** Multi-tier cache (all 5 levels)

- Each metadata level has its own cache: `HashMap<MetadataLevel, Vec<Target>>`
- Cache population: Separate discoveries per level (independent)
- Staleness handling: Serve stale, trigger background refresh (same as main cache)

**Handler flow:**
1. Parse `?level=` query parameter
2. Return `cache[&level].clone()` (filtered labels)
3. If cache stale, spawn background refresh task

**Trade-off:** More AWS API calls (5 separate discoveries) but optimal memory and fast responses for each level.

---

### D-05: Missing Metadata Handling

**Missing labels in output:**
- Omit entirely from target's labels HashMap
- Do not include keys with empty values
- Cleaner JSON, standard Prometheus practice

**Standalone tasks (no service):**
- Include in discovery
- Omit service-level labels (`__meta_ecs_service_name`, etc.)
- Log at debug: "Task X has no service, omitting service labels"

**Non-scrape tasks:**
- Skip entirely (Phase 1 behavior continues)
- Only discover containers with `prometheus.io/scrape: true` label

---

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §META-01..16 — Full metadata label requirements
- `.planning/ROADMAP.md` — Phase 2 scope and success criteria
- `.planning/PROJECT.md` — Key decisions and constraints

### Phase 1 Context
- `.planning/phases/01-core-discovery-http-api/01-CONTEXT.md` — Prior decisions to carry forward:
  - Partial results strategy
  - Query param filtering (case-sensitive exact match)
  - `prometheus.io/scrape` and `prometheus.io/port` labels
  - Label prefix: `__meta_ecs_*`

### Codebase Context
- `src/models/target.rs` — Target struct with labels HashMap
- `src/aws/discovery.rs` — DiscoveryService implementation
- `src/handlers/sd.rs` — Handler with query param parsing
- `src/config.rs` — Config with metadata_level field (currently unused)
- `src/state/app_state.rs` — AppState with cache and discovery service

### AWS SDK Patterns
- `aws_sdk_sts::Client` for account ID lookup
- `aws_sdk_ec2::Client` for AZ extraction (already in use)
- ARN format: `arn:aws:ecs:{region}:{account}:cluster/{name}`

---

## Existing Code Insights

### Reusable Assets
- **Target struct** — Already has `labels: HashMap<String, String>` and `.with_label()` helper
- **DiscoveryService** — ECS and EC2 clients already configured, pattern for AWS API calls established
- **FilterParams** — Query param parsing pattern exists for cluster/service/family
- **Cache pattern** — `Arc<RwLock<Vec<Target>>>` already in AppState

### Established Patterns
- **Error handling** — `DiscoveryError` enum with `thiserror`
- **Partial results** — Log errors, continue with other clusters
- **Async/await** — Tokio runtime, sequential AWS calls with `.await`
- **Tracing** — `info!`, `debug!`, `warn!` macros in use

### Integration Points
- **Query param handling** — Extend `FilterParams` or create new `LevelParam`
- **Handler routing** — Add level extraction in `sd_handler()` before filtering
- **Config integration** — Connect `config.metadata_level` to DiscoveryService

### Current Limitations
- Discovery only builds 3 labels: cluster_name, service_name, task_family
- `metadata_level` in Config is unused (placeholder from Phase 1)
- No level enum defined yet

---

## Specific Ideas

### Label Naming Convention
Follow Prometheus convention with `__meta_ecs_` prefix:
- `__meta_ecs_container_name`
- `__meta_ecs_task_arn`
- `__meta_ecs_service_name`
- etc.

### MetadataLevel Enum
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataLevel {
    Container,
    Task,
    Service,
    Cluster,
    Aws,
}
```

Implement `FromStr` for query param parsing and `Display` for logging.

### LabelBuilder API Sketch
```rust
impl LabelBuilder {
    pub fn new(level: MetadataLevel) -> Self
    pub fn with_container_data(&mut self, container: &ContainerDefinition) -> &mut Self
    pub fn with_task_data(&mut self, task: &Task, task_def: &TaskDefinition) -> &mut Self
    pub fn with_service_data(&mut self, service: &Service) -> &mut Self
    pub fn with_cluster_data(&mut self, cluster: &Cluster) -> &mut Self
    pub fn with_aws_data(&mut self, region: &str, account_id: &str, az: &str) -> &mut Self
    pub fn build(self) -> HashMap<String, String>
}
```

---

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 2-Metadata Labels*
*Context gathered: 2026-05-19*
