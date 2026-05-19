# Phase 2: Metadata Labels - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-19
**Phase:** 2-Metadata Labels
**Areas discussed:** Label Building Architecture, Metadata Level Filtering, AWS-Level Metadata Extraction, Per-Request Level Override, Missing Metadata Handling

---

## Label Building Architecture

| Option | Description | Selected |
|--------|-------------|----------|
| LabelBuilder struct | Dedicated builder with methods like .with_container_labels(), .with_task_labels() — clean separation but new abstraction | ✓ |
| Inline expansion | Add labels directly in discovery.rs where data is available — simplest, keeps context together | |
| Target factory methods | Add methods to Target like from_task(), from_service() — keeps label logic with data structure | |

**User's choice:** LabelBuilder struct
**Notes:** Level-aware builder that takes level enum, only adds labels for that level and below. Passes full AWS SDK objects to builder. Located in src/models/label_builder.rs.

---

## Metadata Level Filtering

| Option | Description | Selected |
|--------|-------------|----------|
| At discovery time | DiscoveryService receives level, only fetches/builds needed metadata. Efficient but complex control flow. | ✓ |
| At response time | Always build complete targets with all labels, filter in HTTP handler. Simpler, wastes memory. | |
| Hybrid approach | Always build container+task (fast), conditionally fetch service/cluster/aws. Balanced approach. | |

**Level Flow Options:**
| Option | Description | Selected |
|--------|-------------|----------|
| DiscoveryService stores level | Set at construction: DiscoveryService::new(ecs, ec2, level). All discovery uses that level. | |
| Pass per discovery call | discover_all_clusters(&self, clusters, level). Flexible for per-request overrides. | |
| Both — stored default, override per call | Service has default level, methods accept optional override. Most flexible. | ✓ |

**Cache Strategy:**
| Option | Description | Selected |
|--------|-------------|----------|
| Single cache — all labels | Cache always stores aws-level targets. Handler filters by level. Memory overhead but simple. | ✓ |
| Separate caches per level | Cache[Container], Cache[Task], etc. No filtering needed, but 5x cache management. | |
| Cache aws-level only | Always discover at aws level for cache. Per-request discovery for other levels. Trade memory for API calls. | |

**Invalid Level Handling:**
| Option | Description | Selected |
|--------|-------------|----------|
| Default to 'task' level | Invalid ?level=foo → silently use 'task'. Graceful degradation. | |
| Return 400 Bad Request | Invalid level → HTTP error with message listing valid options. | ✓ |
| Log warning, use default | Log: 'Unknown level foo, using task', then proceed. Observable but forgiving. | |

**User's choice:** Discovery-time filtering with both stored default and per-call override. Single cache with all labels. Return 400 for invalid levels.

---

## AWS-Level Metadata Extraction

**Account ID Extraction:**
| Option | Description | Selected |
|--------|-------------|----------|
| ARN parser utility | Helper fn parse_arn_account(arn) -> Option<String>. Used by LabelBuilder. | |
| Inline in LabelBuilder | LabelBuilder extracts via arn.split(':').nth(4) directly. | |
| STS GetCallerIdentity | Use STS API to get account ID at startup. Most reliable. | ✓ |

**Availability Zone Source:**
| Option | Description | Selected |
|--------|-------------|----------|
| From EC2 DescribeInstances | AZ is in the same API response as private_ip. No extra calls. | ✓ |
| From ECS DescribeContainerInstances | Container instance has attributes including AZ. | |
| Cache AZ lookup | Separate HashMap to avoid repeated EC2 calls. | |

**Region Source:**
| Option | Description | Selected |
|--------|-------------|----------|
| From SDK config at startup | Store region in DiscoveryService from aws_config::SdkConfig. | ✓ |
| From STS GetCallerIdentity ARN | Parse region from caller ARN. | |
| From EC2 DescribeInstances | Instances have placement.region. | |

**Missing Data Handling:**
| Option | Description | Selected |
|--------|-------------|----------|
| Strict validation — fail target | If AZ or account ID missing, skip the target. | |
| Lenient — include what we have | Missing AZ → omit __meta_ecs_availability_zone. Target still useful. | ✓ |
| Log warnings for missing data | Include target with partial labels, but log warnings. | |

**User's choice:** STS for account ID, EC2 for AZ, SDK config for region. Lenient approach with partial metadata.
**Notes:** User provided Rust code examples showing STS GetCallerIdentity and config.region() usage.

---

## Per-Request Level Override

| Option | Description | Selected |
|--------|-------------|----------|
| Re-filter cached targets | Cache stores all labels. Handler removes labels based on ?level. | |
| Trigger fresh discovery | Ignore cache, run discovery with requested level. | |
| Cache by level (multi-tier) | Separate caches for each level. Complex but optimal. | ✓ |

**Cache Tiers:**
| Option | Description | Selected |
|--------|-------------|----------|
| All 5 levels | container, task, service, cluster, aws — each has own cache. | ✓ |
| 3 tiers — minimal, standard, full | container, task/service, cluster/aws. Simpler. | |
| 2 tiers — task and aws | Default 'task' level and 'aws' level only. | |

**Cache Population:**
| Option | Description | Selected |
|--------|-------------|----------|
| Derive from aws cache | Build aws-level cache first, derive others by filtering. | |
| Separate discoveries | Each level runs its own discovery. More API calls but independent. | ✓ |
| Hybrid | Derive container/task/service/cluster, aws needs separate fetch. | |

**Staleness Handling:**
| Option | Description | Selected |
|--------|-------------|----------|
| Serve stale, trigger refresh | Return cached data immediately, spawn background refresh. | ✓ |
| Wait for fresh discovery | Block request until discovery completes. | |
| Serve stale with header | Add X-Cache-Stale: true header. | |

**User's choice:** Multi-tier cache with all 5 levels. Separate discoveries per level. Serve stale with background refresh.

---

## Missing Metadata Handling

**Missing Labels in Output:**
| Option | Description | Selected |
|--------|-------------|----------|
| Omit entirely | Don't include the label key at all. Cleaner JSON. | ✓ |
| Include with empty string | {"__meta_ecs_service_name": ""}. Explicit but may confuse. | |
| Include with placeholder | {"__meta_ecs_service_name": "__none__"}. Self-documenting. | |

**Standalone Tasks:**
| Option | Description | Selected |
|--------|-------------|----------|
| Include with empty service labels | Task is included, but service labels omitted. | ✓ |
| Skip standalone tasks | Only discover tasks that belong to services. | |
| Add synthetic label | __meta_ecs_service_name = "__standalone__". | |

**Observability:**
| Option | Description | Selected |
|--------|-------------|----------|
| Debug level logs | Log at debug: 'Task X has no service, omitting service labels'. | ✓ |
| Info level summaries | Log at info: '5 tasks without service discovered'. | |
| No logging | Silent omission. | |

**Discovery Scope:**
| Option | Description | Selected |
|--------|-------------|----------|
| Skip entirely (current behavior) | Only discover tasks with prometheus.io/scrape: true. | ✓ |
| Discover but mark | Include with __meta_ecs_scrape: "false". | |
| Make configurable | Flag --include-all-tasks includes everything. | |

**User's choice:** Omit missing labels entirely. Include standalone tasks without service labels. Log at debug level. Keep current scrape label behavior.

---

## the agent's Discretion

None — user made explicit choices for all questions.

---

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Log generated: 2026-05-19*
