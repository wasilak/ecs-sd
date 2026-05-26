# Self-Registration: Making ecs-sd Discover Itself

This document explains how to make an ecs-sd instance appear in its own `/sd` output, allowing Prometheus to scrape ecs-sd's own `/metrics` endpoint.

## Overview

Self-registration is **emergent behavior** — no special code is required. When ecs-sd runs as an ECS task with the standard docker labels, it discovers itself just like any other container.

## Docker Labels for Self-Registration

Add these docker labels to your ecs-sd task definition:

```json
{
  "dockerLabels": {
    "prometheus.io/scrape": "true",
    "prometheus.io/port": "9090"
  }
}
```

Or if using the default port (same as --listen):

```json
{
  "dockerLabels": {
    "prometheus.io/scrape": "true",
    "prometheus.io/port": "8080"
  }
}
```

## How It Works

1. ecs-sd discovers all ECS tasks with `prometheus.io/scrape=true` in the configured clusters
2. If ecs-sd's own task has this label, it appears in the discovery results
3. The `/sd` endpoint includes ecs-sd's own target
4. Prometheus scrapes ecs-sd's `/metrics` via the discovered address

## Port Configuration

### Option 1: Same Port (Default)

```
ECS_SD_LISTEN=0.0.0.0:8080
# No ECS_SD_METRICS_PORT needed
```

Docker labels:
```json
{
  "prometheus.io/port": "8080"
}
```

### Option 2: Separate Metrics Port

```
ECS_SD_LISTEN=0.0.0.0:8080
ECS_SD_METRICS_PORT=9090
```

Docker labels:
```json
{
  "prometheus.io/port": "9090"
}
```

## Proxy Mode Considerations

In proxy mode, ecs-sd excludes itself from the routing table (PROX-07) to prevent proxy loops. However, if the task has `prometheus.io/scrape=true`, it will still appear in `/sd` output.

In proxy mode, the `/sd` response for ecs-sd will be:
```json
{
  "targets": ["<public-address>"],
  "labels": {
    "__metrics_path__": "/metrics"
  }
}
```

## Verification

To verify self-registration is working:

1. Deploy ecs-sd with the docker labels above
2. Wait for discovery to run (or check logs for "discovery refresh complete")
3. Query the `/sd` endpoint:
   ```bash
   curl http://ecs-sd:8080/sd | jq '.targets[]'
   ```
4. You should see ecs-sd's own address in the list

## Prometheus Configuration

Add ecs-sd to your prometheus.yml:

```yaml
scrape_configs:
  - job_name: 'ecs-sd'
    http_sd_configs:
      - url: http://ecs-sd:8080/sd
    relabel_configs:
      - source_labels: [__meta_ecs_task_definition_family]
        target_label: job
```

## Troubleshooting

### ecs-sd doesn't appear in /sd

1. Check that the docker labels are correctly applied to the task definition
2. Verify ecs-sd is discovering the cluster it's running in
3. Check logs: look for the task ARN in discovery output
4. Ensure `prometheus.io/scrape` is exactly `"true"` (not `"yes"` or `"1"`)

### /metrics returns empty

1. Verify the metrics module initialized correctly (check startup logs)
2. Check that at least one discovery refresh has completed
3. Test manually: `curl http://ecs-sd:8080/metrics`

## Security Note

The `/metrics` endpoint is unauthenticated, following standard Prometheus conventions. Ensure it's only accessible within your VPC or use security groups to restrict access.
