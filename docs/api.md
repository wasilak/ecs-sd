# API Reference

HTTP endpoints exposed by ecs-sd.

## Endpoints Overview

| Endpoint | Method | Description | Mode |
|----------|--------|-------------|------|
| `/health` | GET | Health check | All |
| `/sd` | GET | Service discovery targets | All |
| `/sd/refresh` | POST | Trigger cache refresh | All |
| `/proxy/:id/*path` | GET | Proxy to target (proxy mode only) | Proxy |
| `/metrics` | GET | Prometheus metrics | All |

---

## `GET /health`

Health check endpoint for load balancers and monitoring.

### Response

```json
{
  "status": "healthy",
  "app": "ecs-sd",
  "version": "0.3.2"
}
```

### Status Codes

| Code | Meaning |
|------|---------|
| 200 | Service is healthy |
| 503 | Service is unhealthy (not implemented) |

### Example

```bash
curl http://ecs-sd:8080/health
```

**Use case:** Load balancer health checks, Kubernetes liveness/readiness probes.

---

## `GET /sd`

Returns scrape targets in Prometheus `http_sd_configs` format.

Filtering uses canonical labels (`__meta_ecs_cluster_name`, `__meta_ecs_service_name`).
During migration, legacy labels (`__meta_ecs_cluster`, `__meta_ecs_service`) are also accepted for filtering compatibility.

### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `level` | string | — | Override metadata level (`container`, `task`, `service`, `cluster`, `aws`) |
| `cluster` | string | — | Filter by cluster name |
| `service` | string | — | Filter by ECS service name |
| `family` | string | — | Filter by task definition family |
| `tag_{name}` | string | — | Filter by ECS tag label suffix from `__meta_ecs_tag_*` (e.g. `tag_task_env=prod`) |
| `filter_mode` | string | `and` | How to combine all provided filters: `and` or `or` |

### Response Format

```json
[
  {
    "targets": ["10.0.1.42:9464"],
    "labels": {
      "__meta_ecs_cluster_name": "production",
      "__meta_ecs_service_name": "api-gateway",
      "__meta_ecs_task_family": "api-gateway",
      "__meta_ecs_task_version": "42",
      "__meta_ecs_task_arn": "arn:aws:ecs:...",
      "__meta_ecs_container_name": "app",
      "__meta_ecs_container_image": "nginx:1.25",
      "__meta_ecs_metrics_port": "9464",
      "__meta_ecs_desired_count": "3",
      "__meta_ecs_running_count": "3",
      "__meta_ecs_cluster_arn": "arn:aws:ecs:...",
      "__meta_ecs_region": "eu-west-1",
      "__meta_ecs_account_id": "123456789012",
      "__meta_ecs_availability_zone": "eu-west-1a"
    }
  }
]
```

### Discovery Mode Response

```json
[
  {
    "targets": ["10.0.1.42:9464"],
    "labels": {
      "__meta_ecs_cluster_name": "prod",
      "__meta_ecs_service_name": "api",
      "__meta_ecs_container_name": "app"
    }
  }
]
```

### Proxy Mode Response

```json
[
  {
    "targets": ["ecs-sd.example.com:8080"],
    "labels": {
      "__metrics_path__": "/proxy/a1b2c3d4-e5f6-7890-abcd-ef1234567890/metrics",
      "__meta_ecs_cluster_name": "prod",
      "__meta_ecs_service_name": "api"
    }
  }
]
```

**Note:** `__metrics_path__` is automatically used by Prometheus.

### Response Headers

| Header | Description |
|--------|-------------|
| `X-Cache-Age` | Seconds since last cache refresh |
| `X-Cache-State` | `fresh` or `stale` |
| `Content-Type` | `application/json` |

### Examples

**Basic request:**
```bash
curl http://ecs-sd:8080/sd
```

**With metadata level override:**
```bash
curl "http://ecs-sd:8080/sd?level=aws"
```

**Filter by cluster:**
```bash
curl "http://ecs-sd:8080/sd?cluster=production"
```

**Filter by service:**
```bash
curl "http://ecs-sd:8080/sd?service=api-gateway"
```

**Combined filters:**
```bash
curl "http://ecs-sd:8080/sd?level=service&cluster=production"
```

**Tag filters (same key can be repeated):**
```bash
curl "http://ecs-sd:8080/sd?tag_task_env=prod&tag_task_team=obs"
```

**OR mode across filters:**
```bash
curl "http://ecs-sd:8080/sd?cluster=production&tag_task_team=platform&filter_mode=or"
```

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'ecs-containers'
    http_sd_configs:
      - url: 'http://ecs-sd:8080/sd'
        refresh_interval: 60s
    relabel_configs:
      # Optional: add job label from task family
      - source_labels: [__meta_ecs_task_family]
        target_label: job
```

---

## `POST /sd/refresh`

Triggers an immediate cache refresh from AWS.

### Response

```json
{
  "status": "ok",
  "targets_discovered": 42
}
```

### Status Codes

| Code | Meaning |
|------|---------|
| 200 | Refresh successful |


### Example

```bash
curl -X POST http://ecs-sd:8080/sd/refresh
```

**Use case:** Manual refresh after deploying new tasks, debugging.

**Note:** Cache refresh also runs automatically on the configured interval.

---

## `GET /proxy/:id/*path`

Proxies requests to actual targets (proxy mode only).

### Path Parameters

| Parameter | Description |
|-----------|-------------|
| `:id` | Target UUID (from `/sd` response) |
| `*path` | Path to proxy (e.g., `/metrics`, `/health`) |

### Response

Streamed response from the target container.

### Status Codes

| Code | Meaning |
|------|---------|
| 200 | Successful proxy |
| 404 | Target UUID not found |
| 502 | Target connection failed |
| 504 | Target timeout |

### Examples

**Standard metrics endpoint:**
```bash
curl http://ecs-sd:8080/proxy/a1b2c3d4-e5f6-7890-abcd-ef1234567890/metrics
```

**Custom health endpoint:**
```bash
curl http://ecs-sd:8080/proxy/a1b2c3d4-e5f6-7890-abcd-ef1234567890/health
```

**Arbitrary path:**
```bash
curl http://ecs-sd:8080/proxy/a1b2c3d4-e5f6-7890-abcd-ef1234567890/api/v1/status
```

**Note:** This endpoint only exists in proxy mode (`ECS_SD_MODE=proxy`).

---

## `GET /metrics`

Returns Prometheus-formatted metrics about ecs-sd's operation.

### Response Format

Prometheus text exposition format.

### Metrics

#### Discovery Metrics

```
# HELP ecs_sd_discovery_duration_seconds Discovery duration in seconds
# TYPE ecs_sd_discovery_duration_seconds histogram
ecs_sd_discovery_duration_seconds_bucket{le="0.01"} 1
ecs_sd_discovery_duration_seconds_bucket{le="0.02"} 2
ecs_sd_discovery_duration_seconds_sum 0.5
ecs_sd_discovery_duration_seconds_count 10

# HELP ecs_sd_discovery_targets_total Total number of discovered targets
# TYPE ecs_sd_discovery_targets_total gauge
ecs_sd_discovery_targets_total 42

# HELP ecs_sd_discovery_errors_total Total number of discovery errors
# TYPE ecs_sd_discovery_errors_total counter
ecs_sd_discovery_errors_total 0
```

#### Cache Metrics

```
# HELP ecs_sd_cache_age_seconds Age of cache in seconds since last refresh
# TYPE ecs_sd_cache_age_seconds gauge
ecs_sd_cache_age_seconds 45

# HELP ecs_sd_cache_refreshes_total Total number of cache refreshes
# TYPE ecs_sd_cache_refreshes_total counter
ecs_sd_cache_refreshes_total{result="success"} 100
ecs_sd_cache_refreshes_total{result="error"} 2
```

#### Proxy Metrics (Proxy Mode Only)

```
# HELP ecs_sd_proxy_requests_total Total number of proxy requests
# TYPE ecs_sd_proxy_requests_total counter
ecs_sd_proxy_requests_total{status="200"} 5000
ecs_sd_proxy_requests_total{status="502"} 10

# HELP ecs_sd_proxy_duration_seconds Proxy request duration in seconds
# TYPE ecs_sd_proxy_duration_seconds histogram
ecs_sd_proxy_duration_seconds_bucket{le="0.001"} 100
ecs_sd_proxy_duration_seconds_sum 5.2
ecs_sd_proxy_duration_seconds_count 5010
```

#### Cluster Metrics (Cluster Mode Only)

```
# HELP ecs_sd_cluster_nodes_total Total number of nodes in the cluster
# TYPE ecs_sd_cluster_nodes_total gauge
ecs_sd_cluster_nodes_total 3

# HELP ecs_sd_cluster_is_leader Whether this node is the leader
# TYPE ecs_sd_cluster_is_leader gauge
ecs_sd_cluster_is_leader 1
```

### Example

```bash
curl http://ecs-sd:8080/metrics
```

### Prometheus Scraping

To monitor ecs-sd itself:

```yaml
scrape_configs:
  - job_name: 'ecs-sd'
    static_configs:
      - targets: ['ecs-sd:8080']
```

Or via self-registration (see [Self-Registration](self-registration.md)).

---

## Error Responses

### 404 Not Found

```json
{
  "error": "Not Found",
  "message": "Target not found in routing table"
}
```

### 502 Bad Gateway

```json
{
  "error": "Bad Gateway",
  "message": "Failed to connect to target"
}
```

### 503 Service Unavailable

```json
{
  "error": "Service Unavailable",
  "message": "Cache is empty and discovery failed"
}
```

---

## Label Reference

Labels included in `/sd` responses by metadata level:

### Container Level

| Label | Description |
|-------|-------------|
| `__meta_ecs_container_name` | Container name from task definition |
| `__meta_ecs_container_image` | Container image |
| `__meta_ecs_metrics_port` | Port from `prometheus.io/port` label |

### Task Level

| Label | Description |
|-------|-------------|
| `__meta_ecs_task_arn` | Full task ARN |
| `__meta_ecs_task_family` | Task definition family |
| `__meta_ecs_task_version` | Task definition revision |

### Service Level

| Label | Description |
|-------|-------------|
| `__meta_ecs_service_name` | ECS service name |
| `__meta_ecs_desired_count` | Desired task count |
| `__meta_ecs_running_count` | Running task count |

### Cluster Level

| Label | Description |
|-------|-------------|
| `__meta_ecs_cluster_name` | ECS cluster name |
| `__meta_ecs_cluster_arn` | Full cluster ARN |

### AWS Level

| Label | Description |
|-------|-------------|
| `__meta_ecs_region` | AWS region |
| `__meta_ecs_account_id` | AWS account ID |
| `__meta_ecs_availability_zone` | EC2 availability zone |

**Special Labels:**

| Label | Description |
|-------|-------------|
| `__metrics_path__` | Prometheus scrape path (proxy mode only) |

---

## See Also

- [Configuration Reference](configuration.md) - HTTP port configuration
- [Proxy Mode](proxy-mode.md) - Proxy endpoint details
- [Self-Registration](self-registration.md) - Monitoring ecs-sd
