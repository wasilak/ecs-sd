# ECS SD Cluster Terraform Module

This module deploys ecs-sd in cluster mode on AWS Fargate with Cloud Map service discovery for automatic seed resolution.

## Features

- **Cluster Mode**: Deploys multiple ecs-sd instances that form a gossip-based cluster
- **Service Discovery**: Uses AWS Cloud Map for DNS-based seed discovery
- **Security**: Self-referencing security group rules for gossip protocol
- **IAM**: Minimal permissions with ECS read and Cloud Map discovery access
- **Observability**: CloudWatch Logs integration

## Usage

```hcl
module "ecs_sd_cluster" {
  source = "./modules/ecs-sd-cluster"

  name            = "ecs-sd-cluster"
  ecs_cluster_id  = aws_ecs_cluster.main.id
  image           = "123456789012.dkr.ecr.us-east-1.amazonaws.com/ecs-sd:v0.5.0"
  desired_count   = 3
  
  ecs_clusters    = ["production-apps", "staging-apps"]
  
  vpc_id          = aws_vpc.main.id
  subnets         = aws_subnet.private[*].id
  
  service_discovery_namespace_id = aws_service_discovery_private_dns_namespace.main.id
  
  allowed_cidr_blocks = ["10.0.0.0/8", "172.16.0.0/12"]
  
  tags = {
    Environment = "production"
    Team        = "observability"
  }
}
```

## Inputs

| Name | Description | Type | Default | Required |
|------|-------------|------|---------|----------|
| name | Service name for the ecs-sd cluster | string | n/a | yes |
| ecs_cluster_id | ECS cluster ID to deploy into | string | n/a | yes |
| image | Container image URI for ecs-sd | string | n/a | yes |
| desired_count | Number of ecs-sd instances | number | 3 | no |
| cpu | Fargate vCPU units (256 = 0.25 vCPU) | number | 256 | no |
| memory | Fargate memory in MB | number | 512 | no |
| gossip_port | UDP port for gossip protocol | number | 8081 | no |
| ecs_clusters | List of ECS clusters to discover | list(string) | n/a | yes |
| subnets | VPC subnet IDs for tasks | list(string) | n/a | yes |
| vpc_id | VPC ID for security group | string | n/a | yes |
| service_discovery_namespace_id | Cloud Map namespace ID | string | n/a | yes |
| allowed_cidr_blocks | CIDRs allowed to access HTTP API | list(string) | ["10.0.0.0/8"] | no |
| log_retention_days | CloudWatch log retention | number | 7 | no |
| tags | Tags for all resources | map(string) | {} | no |

## Outputs

| Name | Description |
|------|-------------|
| service_name | ECS service name |
| security_group_id | Security group ID |
| discovery_service_name | Cloud Map service name |
| cluster_seeds | The ECS_SD_CLUSTER_SEEDS endpoint |

## Cloud Map Seed Discovery

The module automatically configures seed discovery via Cloud Map DNS. Each task registers itself in the Cloud Map service, and the `ECS_SD_CLUSTER_SEEDS` environment variable is set to the DNS name that resolves to all healthy instances.

Example:
```
ECS_SD_CLUSTER_SEEDS=ecs-sd-cluster.ecs-sd-cluster.local:8081
```

This DNS name resolves to all task IPs, allowing new nodes to discover existing cluster members automatically.

## Security

- **Gossip Port**: UDP 8081 is restricted with `self = true`, meaning only other tasks in this security group can reach the gossip port
- **HTTP API**: Restricted to `allowed_cidr_blocks` (default: RFC1918 private ranges)
- **IAM**: Task role has minimal permissions:
  - ECS: List/Describe clusters, services, tasks
  - EC2: Describe instances
  - Cloud Map: Discover instances, list namespaces/services

## Requirements

| Name | Version |
|------|---------|
| terraform | >= 1.5.0 |
| aws | >= 5.0 |

## License

MIT
