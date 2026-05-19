---
title: External Integrations
created: 2026-05-19
codebase: ecs-sd
---

# External Integrations

## AWS Services

### Amazon ECS (Elastic Container Service)
**Primary integration** - The core purpose of this tool is to interact with AWS ECS.

**Operations performed:**
- `DescribeClusters` - Retrieve cluster details
- `ListTasks` - List tasks within clusters
- `DescribeTasks` - Get detailed task information
- `DescribeTaskDefinition` - Retrieve container definitions

**Code location:** `src/main.rs`

**Usage pattern:**
```rust
let client = Client::new(&config);
let clusters = client.describe_clusters().set_clusters(...).send().await?;
let tasks = client.list_tasks().set_cluster(...).send().await?;
```

### AWS Configuration
- Region provider chain with fallback to `us-east-1`
- Uses default credential provider chain
- Latest behavior version for AWS SDK features

**Configuration code:**
```rust
let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
let config = aws_config::defaults(BehaviorVersion::latest())
    .region(region_provider)
    .load()
    .await;
```

## Hardcoded Configuration

### Target Clusters
The application currently targets specific ECS clusters:
- `service-platform-default`
- `arn:aws:ecs:eu-west-1:723255075185:cluster/service-platform-default`

**Location:** `src/main.rs:15-18`

### AWS Region
- Default fallback region: `us-east-1`
- Supports region provider chain for configuration override

## Authentication
Uses AWS SDK's default credential provider chain:
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS credentials file (`~/.aws/credentials`)
3. IAM role (if running on EC2/ECS/Lambda)
4. Web identity token (for IRSA/EKS)

## Data Extracted

### Cluster Information
- Cluster ARN
- Cluster name

### Task Information
- Task ARN
- Task Definition ARN
- Last status

### Container Definition
- Container name
- Docker image
- Environment variables (name and value)

## Security Considerations
⚠️ **WARNING**: The current implementation prints environment variable values to stdout. This could expose sensitive secrets if containers contain credentials in environment variables.

**Location:** `src/main.rs:89-95`

---

*Document generated: 2026-05-19*
