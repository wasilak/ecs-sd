# ECS SD Cluster Example

This example demonstrates how to use the `ecs-sd-cluster` Terraform module to deploy ecs-sd in cluster mode on AWS Fargate.

## Prerequisites

Before running this example, ensure you have:

1. **Existing ECS Cluster**: An ECS cluster running on Fargate
   ```bash
   aws ecs list-clusters
   ```

2. **Cloud Map Namespace**: A private DNS namespace in Cloud Map
   ```bash
   aws servicediscovery list-namespaces
   ```
   
   If you don't have one, create it:
   ```bash
   aws servicediscovery create-private-dns-namespace \
     --name local \
     --vpc vpc-xxxxxxxx
   ```

3. **VPC and Subnets**: A VPC with private subnets
   - The example assumes VPC tags: `Name = main`
   - Subnet tags: `Type = private`

4. **Container Image**: ecs-sd Docker image pushed to ECR
   ```bash
   aws ecr describe-repositories --repository-names ecs-sd
   ```

5. **Terraform**: Version 1.5.0 or later
   ```bash
   terraform --version
   ```

## Configuration

Edit `main.tf` and update these values for your environment:

| Setting | Location | Description |
|---------|----------|-------------|
| `image` | module.ecs_sd_cluster | Your ECR image URI |
| `ecs_clusters` | module.ecs_sd_cluster | List of ECS clusters to discover |
| `allowed_cidr_blocks` | module.ecs_sd_cluster | Your network CIDRs |
| `data.aws_ecs_cluster.main` | data source | Your ECS cluster name |
| `data.aws_service_discovery_dns_namespace.main` | data source | Your Cloud Map namespace |

## Usage

### 1. Initialize Terraform

```bash
cd terraform/examples/cluster
terraform init
```

### 2. Review the plan

```bash
terraform plan
```

### 3. Apply the configuration

```bash
terraform apply
```

Confirm with `yes` when prompted.

## Verification

After deployment, verify the service is running:

### Check ECS Service

```bash
# Get service details
aws ecs describe-services \
  --cluster production-cluster \
  --services ecs-sd-cluster

# Check running tasks
aws ecs list-tasks \
  --cluster production-cluster \
  --service-name ecs-sd-cluster
```

### Check Cloud Map Registrations

```bash
# List service instances
aws servicediscovery list-instances \
  --service-id $(aws servicediscovery list-services \
    --filters Name=NAMESPACE_ID,Values=$(aws servicediscovery list-namespaces \
      --query 'Namespaces[?Name==`local`].Id' --output text) \
    --query 'Services[?Name==`ecs-sd-cluster`].Id' --output text)
```

### Check Logs

```bash
# View recent logs
aws logs tail /ecs/ecs-sd-cluster --follow
```

Or in CloudWatch Console:
- Navigate to CloudWatch → Logs → Log groups → `/ecs/ecs-sd-cluster`

### Test the API

From within the VPC (e.g., via a bastion host or another ECS task):

```bash
# Get task IP
curl http://ecs-sd-cluster.local:8080/metrics

# Or via the service discovery endpoint
curl http://ecs-sd-cluster.ecs-sd-cluster.local:8080/health
```

## Cluster Formation Verification

Check that nodes are forming a cluster:

```bash
# Look for gossip-related log entries
aws logs filter-log-events \
  --log-group-name /ecs/ecs-sd-cluster \
  --filter-pattern 'gossip' \
  --limit 20
```

Expected output shows:
- Nodes discovering each other via gossip
- Leader performing discovery refreshes
- Cache synchronization messages

## Troubleshooting

### Nodes not forming cluster

Check security group rules:
```bash
aws ec2 describe-security-groups \
  --group-ids $(terraform output -raw ecs_sd_security_group_id)
```

Verify UDP 8081 inbound is allowed from the security group itself.

### Tasks failing to start

Check task stop reason:
```bash
aws ecs describe-tasks \
  --cluster production-cluster \
  --tasks $(aws ecs list-tasks --cluster production-cluster --service-name ecs-sd-cluster --query 'taskArns[0]' --output text) \
  --query 'tasks[0].stoppedReason'
```

### No Cloud Map registrations

Verify the service registry is attached:
```bash
aws ecs describe-services \
  --cluster production-cluster \
  --services ecs-sd-cluster \
  --query 'services[0].serviceRegistries'
```

## Cleanup

To destroy the resources:

```bash
terraform destroy
```

**Note**: This will delete the ECS service and tasks, but will not affect the ECS cluster or Cloud Map namespace (they are data sources, not managed resources).

## Customization

### Scaling

To increase the number of nodes:

```bash
terraform apply -var="desired_count=5"
```

Or edit `main.tf` and change `desired_count`.

### Resource Limits

For production workloads, increase CPU and memory:

```hcl
cpu    = 512   # 0.5 vCPU
memory = 1024  # 1 GB
```

### Multi-Region

To deploy in multiple regions, create a `providers.tf` with alias configurations:

```hcl
provider "aws" {
  alias  = "us-west-2"
  region = "us-west-2"
}

module "ecs_sd_west" {
  providers = {
    aws = aws.us-west-2
  }
  source = "../../modules/ecs-sd-cluster"
  # ... configuration
}
```

## References

- [ECS SD Module README](../../modules/ecs-sd-cluster/README.md)
- [Operational Runbook](../../../docs/ops-runbook.md)
- [AWS ECS Documentation](https://docs.aws.amazon.com/ecs/)
- [AWS Cloud Map Documentation](https://docs.aws.amazon.com/cloud-map/)
