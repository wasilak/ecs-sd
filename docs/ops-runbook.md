# ECS SD Cluster Operational Runbook

This runbook provides operational procedures for managing ecs-sd clusters on AWS ECS Fargate.

## Table of Contents

1. [Health Checks](#health-checks)
2. [Leader Failover](#leader-failover)
3. [Troubleshooting](#troubleshooting)
4. [Scaling](#scaling)
5. [Monitoring Checklist](#monitoring-checklist)

---

## Health Checks

### Check if a Node is Leader

The leader node is responsible for performing ECS discovery and publishing results to the cluster.

**Method 1: CloudWatch Logs Insights**

```sql
fields @timestamp, @message
| filter @message like /Performing initial discovery/
| stats count() by bin(5m)
```

**Method 2: AWS CLI Log Search**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"Performing initial discovery"' \
  --limit 10
```

**Expected Output**: Only the leader node logs this message. If you see it from multiple nodes simultaneously, you may have a split-brain situation.

**Method 3: Real-time Log Stream**

```bash
aws logs tail /ecs/ecs-sd-cluster --follow | grep "initial discovery"
```

### Check Cluster Membership

**Count unique node IDs in gossip logs:**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"gossip node"' \
  --limit 100 \
  --output text --query 'events[*].message' | \
  grep -oE 'node_id: [a-zA-Z0-9-]+' | sort | uniq -c
```

**Expected**: Number of unique node IDs should equal `desired_count` (e.g., 3 nodes = 3 unique IDs).

### Gossip Activity Query

**CloudWatch Logs Insights - Gossip activity over time:**

```sql
fields @timestamp, @message
| filter @message like /gossip/
| stats count() by bin(1m)
```

This shows gossip protocol activity. Steady rate indicates healthy cluster communication.

---

## Leader Failover

### Expected Failover Behavior

When the leader fails:
1. Remaining nodes detect leader failure via gossip failure detection (10-15 seconds)
2. Next node (lexicographically smallest remaining node ID) becomes leader
3. New leader begins discovery cycles
4. Cluster continues operating normally

### Verification Steps

**1. Identify old leader:**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"Performing initial discovery"' \
  --start-time $(date -v-10M -u +%s)000 \
  --query 'events[*].message'
```

**2. Watch for new leader:**

```bash
aws logs tail /ecs/ecs-sd-cluster --follow | grep "initial discovery"
```

**3. Verify discovery continues:**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"discovery refresh complete"' \
  --limit 10
```

**Expected Timeline:**
- T+0: Leader stops (ECS task stops or fails)
- T+10-15s: New leader elected
- T+20-30s: New leader performs first discovery
- T+60s+ (depends on refresh_interval): Normal discovery cycles resume

### Log Patterns to Monitor

**Healthy startup sequence:**
```
gossip node <node_id> starting on 0.0.0.0:8081
gossip: joined cluster via seed <seed_addr>
Performing initial discovery...
discovery refresh complete: <N> targets
```

**Leader transition:**
```
[old leader] Task stopped (no more logs)
[new leader] Performing initial discovery... (within 15 seconds)
```

---

## Troubleshooting

### Symptom: Nodes Not Forming Cluster

**Symptoms:**
- Each node shows only itself in gossip
- Multiple nodes logging "Performing initial discovery..." (split-brain)
- Cache not synchronizing between nodes

**Check 1: Security Group Configuration**

```bash
aws ec2 describe-security-groups \
  --group-ids <security-group-id> \
  --query 'SecurityGroups[0].IpPermissions[]'
```

**Required**: UDP port 8081 with `UserIdGroupPairs` (self-referencing rule).

Expected output should include:
```json
{
  "FromPort": 8081,
  "ToPort": 8081,
  "IpProtocol": "udp",
  "UserIdGroupPairs": [{"GroupId": "<same-sg-id>"}]
}
```

**Fix if missing:**
```bash
aws ec2 authorize-security-group-ingress \
  --group-id <security-group-id> \
  --protocol udp \
  --port 8081 \
  --source-group <security-group-id>
```

**Check 2: ECS_SD_CLUSTER_SEEDS Environment Variable**

```bash
aws ecs describe-task-definition \
  --task-definition ecs-sd-cluster \
  --query 'taskDefinition.containerDefinitions[0].environment'
```

Verify `ECS_SD_CLUSTER_SEEDS` contains valid `host:port` pairs.

Expected format:
```
ecs-sd-cluster.ecs-sd-cluster.local:8081
```

**Check 3: Cloud Map DNS Resolution**

Exec into a running task (via ECS Exec):

```bash
aws ecs execute-command \
  --cluster <cluster-name> \
  --task <task-id> \
  --container ecs-sd-cluster \
  --interactive \
  --command "sh"

# Inside the container
nslookup ecs-sd-cluster.ecs-sd-cluster.local
```

**Expected**: Should resolve to multiple IPs (one per healthy task).

If DNS fails:
- Check Cloud Map namespace exists: `aws servicediscovery list-namespaces`
- Check service registered: `aws servicediscovery list-services`
- Check task has service registry in ECS service

### Symptom: Multiple Leaders (Split-Brain)

**Cause:** Network partition between nodes prevents gossip communication.

**Detection:**
```sql
fields @timestamp, @message, @logStream
| filter @message like /Performing initial discovery/
| stats count() by @logStream, bin(1m)
```

If multiple tasks log "initial discovery" simultaneously for >30 seconds, you have split-brain.

**Resolution:**
1. **Do NOT manually intervene** - chitchat gossip protocol will heal automatically
2. Verify network connectivity is restored
3. Monitor for cluster merge (gossip will sync state)
4. If persists >5 minutes: restart all tasks (ECS will reschedule)

```bash
# Force redeployment
aws ecs update-service \
  --cluster <cluster-name> \
  --service ecs-sd-cluster \
  --force-new-deployment
```

### Symptom: Followers Not Syncing Cache

**Check 1: Verify Leader is Running Discovery**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"discovery refresh complete"' \
  --limit 5
```

Should see recent entries. If none in 5+ minutes, leader may be stuck.

**Check 2: Gossip State Key Publishing**

```bash
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"ecs_sd.cache.v1"' \
  --limit 20
```

Should see gossip state updates containing the cache key.

**Fix:**
If leader isn't publishing:
1. Check ECS task health: `aws ecs describe-tasks --cluster <cluster> --tasks <task-id>`
2. Review task logs for errors
3. Restart leader task if stuck

---

## Scaling

### Increasing Desired Count

**Scale up from 3 to 5 nodes:**

```bash
aws ecs update-service \
  --cluster <cluster-name> \
  --service ecs-sd-cluster \
  --desired-count 5
```

Or via Terraform:
```bash
terraform apply -var="desired_count=5"
```

**What happens:**
1. ECS launches new tasks
2. New tasks register in Cloud Map
3. New tasks discover existing cluster via DNS seeds
4. Gossip protocol adds new nodes to cluster
5. New followers receive cache state from leader
6. Cluster now has 5 nodes (1 leader + 4 followers)

**Verification:**
```bash
# Count running tasks
aws ecs describe-services \
  --cluster <cluster-name> \
  --services ecs-sd-cluster \
  --query 'services[0].runningCount'

# Check gossip membership
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern '"joined cluster"' \
  --limit 20
```

### Decreasing Desired Count

**Scale down from 5 to 3 nodes:**

```bash
aws ecs update-service \
  --cluster <cluster-name> \
  --service ecs-sd-cluster \
  --desired-count 3
```

**What happens:**
1. ECS drains connections from tasks to be stopped
2. Tasks unregister from Cloud Map
3. Remaining nodes detect departure via gossip failure detection
4. If leader was stopped, new leader elected automatically
5. Cluster continues with 3 nodes

**Important Considerations:**

- **Never scale below 2 nodes** in production (risk of single point of failure)
- **Leader may change** during scale-down (plan for brief discovery pause)
- **Cache remains available** on remaining nodes

### Auto Scaling (Optional)

Configure ECS Service Auto Scaling based on metrics:

```bash
aws application-autoscaling register-scalable-target \
  --service-namespace ecs \
  --resource-id service/<cluster-name>/ecs-sd-cluster \
  --scalable-dimension ecs:service:DesiredCount \
  --min-capacity 3 \
  --max-capacity 10
```

**Note:** ecs-sd is not CPU/memory intensive. Scale based on:
- Number of discovered ECS services
- Request rate to `/metrics` endpoint

---

## Monitoring Checklist

### Critical Alerts

| Alert | Condition | Severity | Action |
|-------|-----------|----------|--------|
| **Zero Healthy Nodes** | Cloud Map `HealthyInstanceCount` = 0 | P1 | Check ECS service, task failures, security groups |
| **No Discovery Refresh** | No "discovery refresh complete" logs in 5 minutes | P1 | Check leader health, restart if stuck |
| **Cache Stale** | Cache age > refresh_interval × 2 | P2 | Verify leader discovery, check ECS API permissions |
| **Split-Brain Detected** | Multiple simultaneous leaders > 30 seconds | P2 | Verify network connectivity, force redeploy if needed |
| **High Gossip Latency** | Gossip round-trip time > 100ms | P3 | Check network, consider AZ placement |

### CloudWatch Alarms

**Example: Zero healthy instances in Cloud Map**

```bash
aws cloudwatch put-metric-alarm \
  --alarm-name ecs-sd-zero-healthy \
  --alarm-description "No healthy ecs-sd instances" \
  --metric-name HealthyInstanceCount \
  --namespace AWS/ServiceDiscovery \
  --dimensions Name=ServiceId,Value=<service-id> \
  --statistic Average \
  --period 60 \
  --evaluation-periods 2 \
  --threshold 0 \
  --comparison-operator LessThanOrEqualToThreshold
```

**Example: Discovery stopped (logs-based)**

```bash
# Use CloudWatch Logs Insights scheduled query
fields @timestamp
| filter @message like /discovery refresh complete/
| stats count() as discovery_count
| filter discovery_count = 0
```

### Recommended Metrics to Track

**From ECS:**
- `RunningTaskCount` - Should match desired_count
- `PendingTaskCount` - Spikes indicate deployment/restart
- `CPUUtilization` - Should be <50% under normal load
- `MemoryUtilization` - Should be <70%

**From Cloud Map:**
- `HealthyInstanceCount` - Should equal desired_count
- `UnHealthyInstanceCount` - Should be 0

**From Logs:**
- Discovery refresh frequency
- Gossip node join/leave events
- Leader election events
- Error rate (filter @message like /ERROR/)

### Daily Health Check Command

```bash
#!/bin/bash
# ecs-sd-health-check.sh

CLUSTER="production-cluster"
SERVICE="ecs-sd-cluster"
LOG_GROUP="/ecs/ecs-sd-cluster"

echo "=== ECS SD Cluster Health Check ==="
echo ""

echo "1. Running Task Count:"
aws ecs describe-services \
  --cluster $CLUSTER \
  --services $SERVICE \
  --query 'services[0].{desired:desiredCount,running:runningCount}'

echo ""
echo "2. Recent Discovery Refreshes:"
aws logs filter-log-events \
  --log-group-name $LOG_GROUP \
  --filter-pattern '"discovery refresh complete"' \
  --limit 3 \
  --query 'events[*].{time:timestamp,msg:message}'

echo ""
echo "3. Current Leader:"
aws logs filter-log-events \
  --log-group-name $LOG_GROUP \
  --filter-pattern '"Performing initial discovery"' \
  --limit 1 \
  --query 'events[0].{time:timestamp,msg:message}'

echo ""
echo "4. Recent Gossip Activity (last 5 min):"
aws logs filter-log-events \
  --log-group-name $LOG_GROUP \
  --filter-pattern 'gossip' \
  --start-time $(date -v-5M -u +%s)000 \
  --query 'length(events)'

echo ""
echo "=== Health Check Complete ==="
```

### Incident Response Playbook

**Scenario: Complete cluster down**

1. **Check ECS Service**
   ```bash
   aws ecs describe-services --cluster <cluster> --services ecs-sd-cluster
   ```
   Look for: service status, events, stopped tasks

2. **Check Task Failures**
   ```bash
   aws ecs list-tasks --cluster <cluster> --service ecs-sd-cluster --desired-status STOPPED
   aws ecs describe-tasks --cluster <cluster> --tasks <task-arn>
   ```
   Look for: stoppedReason, exit codes

3. **Check Security Groups**
   - Verify UDP 8081 ingress from self is present
   - Verify egress to ECS/Cloud Map APIs

4. **Force Redeploy**
   ```bash
   aws ecs update-service --cluster <cluster> --service ecs-sd-cluster --force-new-deployment
   ```

5. **Escalate** if:
   - Tasks stuck in PENDING >5 minutes
   - Repeated crashes with same error
   - AWS service issues (check https://health.aws.amazon.com)

---

## References

- [ECS Documentation](https://docs.aws.amazon.com/ecs/)
- [Cloud Map Documentation](https://docs.aws.amazon.com/cloud-map/)
- [ecs-sd Project README](../../README.md)
- [Terraform Module](../../terraform/modules/ecs-sd-cluster/)

## Support

For issues not covered in this runbook:
1. Check application logs: CloudWatch `/ecs/ecs-sd-cluster`
2. Review Terraform state: `terraform show`
3. Open issue: [GitHub Issues](https://github.com/stepstone/ecs-sd/issues)
