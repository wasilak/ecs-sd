# ecs-sd

**Prometheus HTTP Service Discovery for AWS ECS**

Automatically discover and expose scrape targets for ECS containers with `prometheus.io/scrape=true`. Supports EC2 and Fargate launch types, with optional high-availability clustering.

```mermaid
flowchart LR
    P["Prometheus"] -->|GET /sd| E["ecs-sd"]
    E -->|discovers| AWS["AWS ECS"]
    P -->|scrapes| T["ECS Tasks"]
```

## Features

- **Zero-config discovery** — Finds containers with Docker labels `prometheus.io/scrape=true` and `prometheus.io/port`
- **EC2 & Fargate support** — Direct targets for EC2, proxy mode for Fargate
- **High availability** — Gossip-based clustering with automatic leader election
- **Prometheus native** — Returns `http_sd_configs` compatible JSON
- **5 metadata levels** — From container to AWS account context
- **Stale-while-revalidate** — Always serves cached data, refreshes in background

## Quick Start

### Discovery Mode (EC2)

```bash
docker run -p 8080:8080 \
  -e ECS_SD_CLUSTERS=my-cluster \
  -e AWS_REGION=eu-west-1 \
  ghcr.io/wasilak/ecs-sd
```

### Proxy Mode (Fargate)

```bash
docker run -p 8080:8080 \
  -e ECS_SD_CLUSTERS=my-cluster \
  -e ECS_SD_MODE=proxy \
  -e ECS_SD_PUBLIC_ADDRESS=https://ecs-sd.example.com \
  -e AWS_REGION=eu-west-1 \
  ghcr.io/wasilak/ecs-sd
```

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'ecs'
    http_sd_configs:
      - url: 'http://ecs-sd:8080/sd'
```

That's it — Prometheus automatically discovers all ECS containers with metrics endpoints.

## Documentation

| Document | Description |
|----------|-------------|
| [Configuration Reference](docs/configuration.md) | All CLI flags and environment variables |
| [Proxy Mode](docs/proxy-mode.md) | Fargate support and reverse proxy mode |
| [Cluster Mode](docs/cluster-mode.md) | HA clustering with automatic failover |
| [API Reference](docs/api.md) | HTTP endpoints and response formats |
| [Self-Registration](docs/self-registration.md) | Monitoring ecs-sd itself |
| [Operational Runbook](docs/ops-runbook.md) | Production operations and troubleshooting |

## Container Discovery

Containers are discovered when their task definition includes these Docker labels:

| Label | Value | Purpose |
|-------|-------|---------|
| `prometheus.io/scrape` | `true` | Opt-in to discovery |
| `prometheus.io/port` | numeric | Metrics endpoint port |

Example task definition:

```json
{
  "containerDefinitions": [{
    "dockerLabels": {
      "prometheus.io/scrape": "true",
      "prometheus.io/port": "8080"
    }
  }]
}
```

## Architecture

### Operating Modes

**Discovery Mode** (default) — EC2 launch type:
- Returns direct container IPs to Prometheus
- Best when Prometheus has network access to containers

**Proxy Mode** — Fargate or network segmentation:
- Acts as reverse proxy for metrics scraping
- Required for Fargate (private ENI IPs)

**Cluster Mode** — High availability:
- Multiple ecs-sd instances form a cluster
- One leader discovers from AWS; followers serve from cache
- Automatic failover in ~15 seconds

```mermaid
flowchart TD
    subgraph "Discovery Mode"
        D1["/sd returns<br/>container IPs"]
    end
    
    subgraph "Proxy Mode"
        D2["/sd returns<br/>ecs-sd address"]
        P2["/proxy routes<br/>to containers"]
    end
    
    subgraph "Cluster Mode"
        L["Leader<br/>Discovers"]
        F["Followers<br/>Serve"]
        G[(Gossip)]
        L --> G --> F
    end
```

## Configuration

Essential options:

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--clusters` | `ECS_SD_CLUSTERS` | required | ECS clusters to discover |
| `--listen` | `ECS_SD_LISTEN` | `:8080` | HTTP bind address |
| `--mode` | `ECS_SD_MODE` | `discovery` | `discovery` or `proxy` |
| `--public-address` | `ECS_SD_PUBLIC_ADDRESS` | — | Required for proxy mode |
| `--cluster-mode` | `ECS_SD_CLUSTER_MODE` | `standalone` | `standalone` or `cluster` |

See [Configuration Reference](docs/configuration.md) for all options.

## API

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check |
| `GET /sd` | Service discovery targets |
| `POST /sd/refresh` | Trigger cache refresh |
| `GET /proxy/:id/metrics` | Proxy to target (proxy mode) |
| `GET /metrics` | Prometheus metrics |

`GET /sd` supports flexible filtering. See the [Filtering section](#filtering) below and the full [API Reference](docs/api.md).

## Filtering

The `/sd` endpoint accepts query parameters to narrow down which targets Prometheus receives. All filters are optional — omitting them returns everything.

### Basic filters

| Parameter | Matches on | Repeatable? |
|-----------|-----------|-------------|
| `cluster` | ECS cluster name | Yes |
| `service` | ECS service name | Yes |
| `family` | Task definition family | Yes |
| `tag_{name}` | ECS tag (e.g. `tag_env=prod`) | Yes |
| `filter_mode` | How fields combine: `and` (default) or `or` | No |
| `level` | Metadata level to include | No |

### Single values — the simple case

```bash
# Only targets in the "production" cluster
curl "http://ecs-sd:8080/sd?cluster=production"

# Only the "api-gateway" service
curl "http://ecs-sd:8080/sd?service=api-gateway"

# Narrow down further: production cluster AND api-gateway service
curl "http://ecs-sd:8080/sd?cluster=production&service=api-gateway"
```

### Multiple values for the same filter — OR within a field

Repeat a parameter to match any of the given values. Useful when you want targets from several clusters, services, or task families at once.

```bash
# Targets from production OR staging
curl "http://ecs-sd:8080/sd?cluster=production&cluster=staging"

# Two specific task families
curl "http://ecs-sd:8080/sd?family=api-task&family=worker-task"

# Services from two clusters — cluster filter is OR, combined with service filter by AND
curl "http://ecs-sd:8080/sd?cluster=production&cluster=staging&service=api-gateway"
```

### Tag filters — AND across different tags, OR within the same tag

ECS tags attached to your tasks or services can be used as filters. Use `tag_{tag-name}={value}` syntax.

**Different tag names → AND** (target must satisfy all of them):

```bash
# Must have env=production AND team=observability
curl "http://ecs-sd:8080/sd?tag_env=production&tag_team=observability"
```

**Same tag name repeated → OR** (target matches if it has any of the values):

```bash
# env=production OR env=staging
curl "http://ecs-sd:8080/sd?tag_env=production&tag_env=staging"
```

**Both combined** — group by tag name, then AND the groups:

```bash
# (env=production OR env=staging) AND team=observability
curl "http://ecs-sd:8080/sd?tag_env=production&tag_env=staging&tag_team=observability"
```

This maps naturally to how you'd think about it: "give me all observability-team services, from both prod and staging."

### Combining field-level filters with `filter_mode`

By default, the different filter fields (cluster, service, family, tag groups) are combined with **AND** — a target must satisfy every field you specify. Switch to **OR** to get targets that match any one of them.

```bash
# Targets in the "production" cluster AND tagged with team=platform
curl "http://ecs-sd:8080/sd?cluster=production&tag_team=platform"

# Targets in "production" cluster OR tagged with team=platform
curl "http://ecs-sd:8080/sd?cluster=production&tag_team=platform&filter_mode=or"
```

### Metadata level

Control how much metadata is included in each target's labels. Only one level can be specified.

```bash
# Include all AWS-level metadata
curl "http://ecs-sd:8080/sd?level=aws"

# Only cluster-level metadata, filtered to a specific cluster
curl "http://ecs-sd:8080/sd?level=cluster&cluster=production"
```

Available levels (each includes everything from the levels above it):
`container` → `task` → `service` → `cluster` → `aws`

---

## AWS IAM

Required permissions:

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": [
      "ecs:ListClusters",
      "ecs:DescribeClusters",
      "ecs:ListServices",
      "ecs:DescribeServices",
      "ecs:ListTasks",
      "ecs:DescribeTasks",
      "ecs:DescribeTaskDefinition",
      "ec2:DescribeInstances",
      "ec2:DescribeContainerInstances",
      "ec2:DescribeNetworkInterfaces",
      "sts:GetCallerIdentity"
    ],
    "Resource": "*"
  }]
}
```

## Deployment

### Docker Compose

```yaml
version: '3.8'
services:
  ecs-sd:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_REFRESH_INTERVAL: 60s
    ports:
      - "8080:8080"
```

### ECS Fargate

Run ecs-sd as an ECS service with:
- Cloud Map service discovery for cluster seed resolution
- Auto-scaling policies
- Security group rules for HTTP and gossip traffic
- IAM roles with ECS/EC2/STS read permissions

## Building

```bash
git clone https://github.com/wasilak/ecs-sd.git
cd ecs-sd
cargo build --release
```

Requires Rust 1.85+ (2024 edition).

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE)
