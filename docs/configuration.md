# Configuration Reference

All configuration options for ecs-sd can be set via CLI flags or environment variables.

## Quick Reference

| Flag | Env Var | Default | Description |
|---|---|---|---|
| `--clusters` | `ECS_SD_CLUSTERS` | — (required) | Comma-separated ECS cluster names or ARNs |
| `--listen` | `ECS_SD_LISTEN` | `0.0.0.0:8080` | HTTP server bind address |
| `--refresh-interval` | `ECS_SD_REFRESH_INTERVAL` | `60s` | Background cache refresh interval |
| `--metadata-level` | `ECS_SD_METADATA_LEVEL` | `task` | Metadata detail level |
| `--mode` | `ECS_SD_MODE` | `discovery` | Operating mode: `discovery` or `proxy` |
| `--public-address` | `ECS_SD_PUBLIC_ADDRESS` | — | Public address for proxy mode |
| `--cluster-mode` | `ECS_SD_CLUSTER_MODE` | `standalone` | Cluster mode: `standalone` or `cluster` |
| `--cluster-seeds` | `ECS_SD_CLUSTER_SEEDS` | — | Seed addresses for cluster join |
| `--gossip-port` | `ECS_SD_GOSSIP_PORT` | `8081` | UDP port for gossip protocol |
| `--node-id` | `ECS_SD_NODE_ID` | `hostname:port` | Unique node identifier |
| `--metrics-port` | `ECS_SD_METRICS_PORT` | — | Optional separate metrics port |

---

## Core Configuration

### `--clusters` / `ECS_SD_CLUSTERS`

**Required.** Comma-separated list of ECS cluster names or ARNs to discover.

```bash
# Single cluster
ecs-sd --clusters production

# Multiple clusters
ecs-sd --clusters prod-us-east-1,prod-us-west-2,staging

# Using environment variable
export ECS_SD_CLUSTERS=prod-us-east-1,prod-us-west-2
ecs-sd
```

### `--listen` / `ECS_SD_LISTEN`

Socket address to bind the HTTP server.

```bash
# Default
ecs-sd --listen 0.0.0.0:8080

# Custom port
ecs-sd --listen 0.0.0.0:9090

# Localhost only (testing)
ecs-sd --listen 127.0.0.1:8080
```

### `--refresh-interval` / `ECS_SD_REFRESH_INTERVAL`

How often to refresh the discovery cache from AWS.

```bash
# Default: 60 seconds
ecs-sd --refresh-interval 60s

# Faster refresh (more AWS API calls)
ecs-sd --refresh-interval 30s

# Slower refresh (fewer AWS API calls)
ecs-sd --refresh-interval 5m
```

**Note:** Refresh uses ±10% random jitter to prevent thundering herd when multiple instances restart simultaneously.

### `--metadata-level` / `ECS_SD_METADATA_LEVEL`

Controls how much metadata is included in `/sd` responses.

| Level | Labels Included | Use Case |
|-------|-----------------|----------|
| `container` | Container name, image, port | Minimal footprint |
| `task` | + Task ARN, family, version | **Default** - Good balance |
| `service` | + Service name, counts | Service-level monitoring |
| `cluster` | + Cluster name, ARN | Cluster-level aggregation |
| `aws` | + Region, account ID, AZ | Full AWS context |

Higher levels include all labels from lower levels.

```bash
# Default: task level
ecs-sd --metadata-level task

# Minimal: container level only
ecs-sd --metadata-level container

# Maximum: all metadata
ecs-sd --metadata-level aws
```

**Override per-request:**
```bash
curl "http://ecs-sd:8080/sd?level=aws"
```

---

## Operating Mode

### `--mode` / `ECS_SD_MODE`

Choose between direct target exposure or reverse proxy mode.

| Mode | Description | Use Case |
|------|-------------|----------|
| `discovery` | Returns direct target IPs | EC2 launch type, direct network access |
| `proxy` | Returns ecs-sd address, proxies requests | Fargate, network segmentation |

```bash
# Default: discovery mode
ecs-sd --mode discovery

# Proxy mode for Fargate
ecs-sd --mode proxy --public-address https://ecs-sd.example.com
```

See [Proxy Mode](proxy-mode.md) for detailed documentation.

### `--public-address` / `ECS_SD_PUBLIC_ADDRESS`

**Required when `mode=proxy`.** The URL Prometheus uses to reach ecs-sd.

Must be a full URL with domain and scheme (`http://` or `https://`).
If port is omitted, ecs-sd defaults to `80` for `http` and `443` for `https`.

```bash
# DNS name
ecs-sd --mode proxy --public-address https://ecs-sd.example.com

# Load balancer address
ecs-sd --mode proxy --public-address http://ecs-sd-lb.internal:8080

# Explicit non-default HTTPS port
ecs-sd --mode proxy --public-address https://ecs-sd.example.com:8443
```

This address is returned in `/sd` responses as the scrape target.

---

## Cluster Mode (HA)

### `--cluster-mode` / `ECS_SD_CLUSTER_MODE`

Enable high-availability clustering.

| Mode | Description |
|------|-------------|
| `standalone` | Single instance, no clustering (default) |
| `cluster` | Join a gossip-based cluster |

```bash
# Standalone (default)
ecs-sd --cluster-mode standalone

# Cluster mode
ecs-sd --cluster-mode cluster --cluster-seeds "node-2:8081,node-3:8081"
```

See [Cluster Mode](cluster-mode.md) for detailed documentation.

### `--cluster-seeds` / `ECS_SD_CLUSTER_SEEDS`

Comma-separated list of seed addresses for cluster join.

```bash
# Static IPs
ecs-sd --cluster-seeds "10.0.1.10:8081,10.0.1.11:8081"

# DNS names (Cloud Map, Route 53)
ecs-sd --cluster-seeds "ecs-sd-2.local:8081,ecs-sd-3.local:8081"

# Combination
ecs-sd --cluster-seeds "ecs-sd-2:8081,10.0.1.15:8081"
```

**Format:** `host:port` where port is the gossip port (not HTTP port).

### `--gossip-port` / `ECS_SD_GOSSIP_PORT`

UDP port for gossip protocol communication between cluster nodes.

```bash
# Default
ecs-sd --gossip-port 8081

# Custom port
ecs-sd --gossip-port 9091
```

**Important:** All nodes in the cluster must use the same gossip port. The port must be open between nodes (UDP ingress/egress).

### `--node-id` / `ECS_SD_NODE_ID`

Unique identifier for this node in the cluster.

```bash
# Auto-generated from HOSTNAME env var (default)
# Result: "ip-10-0-1-100:8081"

# Explicit node ID
ecs-sd --node-id "ecs-sd-1"

# With port
ecs-sd --node-id "ecs-sd-1:8081"
```

**Leader election:** The node with the lexicographically smallest node ID becomes leader.

---

## Metrics

### `--metrics-port` / `ECS_SD_METRICS_PORT`

Optional separate port for the Prometheus `/metrics` endpoint.

```bash
# Default: same as --listen
ecs-sd --listen 0.0.0.0:8080
# /metrics available on :8080

# Separate metrics port
ecs-sd --listen 0.0.0.0:8080 --metrics-port 9090
# /sd on :8080, /metrics on :9090
```

**Use cases:**
- Security separation (different security groups for metrics vs discovery)
- Load balancer health checks on discovery endpoint only
- Internal monitoring on separate port

---

## Configuration Examples

### Basic Discovery Mode

```bash
ecs-sd \
  --clusters production \
  --listen 0.0.0.0:8080 \
  --refresh-interval 60s \
  --metadata-level task
```

### Proxy Mode for Fargate

```bash
ecs-sd \
  --clusters production \
  --listen 0.0.0.0:8080 \
  --mode proxy \
  --public-address https://ecs-sd.example.com:8080 \
  --metadata-level task
```

### Cluster Mode (3 nodes)

```bash
# Node 1 (becomes leader if lexicographically smallest)
ecs-sd \
  --clusters production \
  --cluster-mode cluster \
  --cluster-seeds "ecs-sd-2:8081,ecs-sd-3:8081" \
  --node-id "ecs-sd-1" \
  --gossip-port 8081

# Node 2
ecs-sd \
  --clusters production \
  --cluster-mode cluster \
  --cluster-seeds "ecs-sd-1:8081,ecs-sd-3:8081" \
  --node-id "ecs-sd-2" \
  --gossip-port 8081

# Node 3
ecs-sd \
  --clusters production \
  --cluster-mode cluster \
  --cluster-seeds "ecs-sd-1:8081,ecs-sd-2:8081" \
  --node-id "ecs-sd-3" \
  --gossip-port 8081
```

### Combined: Proxy + Cluster + Separate Metrics

```bash
ecs-sd \
  --clusters production \
  --listen 0.0.0.0:8080 \
  --metrics-port 9090 \
  --mode proxy \
  --public-address ecs-sd-lb.example.com:8080 \
  --cluster-mode cluster \
  --cluster-seeds "ecs-sd-2:8081" \
  --node-id "ecs-sd-1" \
  --gossip-port 8081 \
  --metadata-level task
```

---

## Docker Compose Examples

### Simple Single Instance

```yaml
version: '3.8'
services:
  ecs-sd:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_REFRESH_INTERVAL: 60s
      ECS_SD_METADATA_LEVEL: task
    ports:
      - "8080:8080"
```

### Cluster Mode

```yaml
version: '3.8'
services:
  ecs-sd-1:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_CLUSTER_MODE: cluster
      ECS_SD_CLUSTER_SEEDS: "ecs-sd-2:8081,ecs-sd-3:8081"
      ECS_SD_NODE_ID: node-1
      ECS_SD_GOSSIP_PORT: "8081"
    ports:
      - "8080:8080"
      - "8081:8081/udp"

  ecs-sd-2:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_CLUSTER_MODE: cluster
      ECS_SD_CLUSTER_SEEDS: "ecs-sd-1:8081,ecs-sd-3:8081"
      ECS_SD_NODE_ID: node-2
      ECS_SD_GOSSIP_PORT: "8081"
    ports:
      - "8081:8080"
      - "8082:8081/udp"

  ecs-sd-3:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_CLUSTER_MODE: cluster
      ECS_SD_CLUSTER_SEEDS: "ecs-sd-1:8081,ecs-sd-2:8081"
      ECS_SD_NODE_ID: node-3
      ECS_SD_GOSSIP_PORT: "8081"
    ports:
      - "8082:8080"
      - "8083:8081/udp"
```

### Proxy Mode with Separate Metrics

```yaml
version: '3.8'
services:
  ecs-sd:
    image: ghcr.io/wasilak/ecs-sd
    environment:
      ECS_SD_CLUSTERS: production
      ECS_SD_MODE: proxy
      ECS_SD_PUBLIC_ADDRESS: https://ecs-sd.example.com:8080
      ECS_SD_METRICS_PORT: "9090"
    ports:
      - "8080:8080"
      - "9090:9090"
```

---

## Environment Variable Precedence

Environment variables take precedence over defaults but CLI flags override both:

```bash
# 1. Default value
ecs-sd --clusters prod  # Uses default --listen 0.0.0.0:8080

# 2. Environment variable overrides default
export ECS_SD_LISTEN=0.0.0.0:9090
ecs-sd --clusters prod  # Uses 0.0.0.0:9090

# 3. CLI flag overrides environment variable
export ECS_SD_LISTEN=0.0.0.0:9090
ecs-sd --clusters prod --listen 0.0.0.0:8080  # Uses 0.0.0.0:8080
```

---

## Validation

ecs-sd validates configuration at startup and exits with an error if:

- `--clusters` is empty or missing
- `--listen` is not a valid socket address
- `--refresh-interval` is less than 1 second
- `--mode=proxy` without `--public-address`
- `--cluster-seeds` contains invalid addresses (missing port)
- `--gossip-port` is not a valid port number

Example error output:
```
Error: ConfigError(InvalidValue("--public-address / ECS_SD_PUBLIC_ADDRESS is required in proxy mode"))
```

---

## See Also

- [Proxy Mode](proxy-mode.md) - Detailed proxy mode documentation
- [Cluster Mode](cluster-mode.md) - HA clustering setup and operations
- [API Reference](api.md) - HTTP endpoints
- [Self-Registration](self-registration.md) - Monitoring ecs-sd itself
- [Operational Runbook](ops-runbook.md) - Production operations
