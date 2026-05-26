# ECS SD Cluster Example
# 
# This example demonstrates how to use the ecs-sd-cluster module
# to deploy ecs-sd in cluster mode on AWS Fargate.

terraform {
  required_version = ">= 1.5.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}

# -------------------------------------------------------------------------
# Data Sources - Query existing infrastructure
# -------------------------------------------------------------------------

data "aws_vpc" "main" {
  # Find the main VPC by tag
  tags = {
    Name = "main"
  }
}

data "aws_subnets" "private" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.main.id]
  }
  
  tags = {
    Type = "private"
  }
}

data "aws_ecs_cluster" "main" {
  cluster_name = "production-cluster"
}

data "aws_service_discovery_dns_namespace" "main" {
  name = "local"
  type = "DNS_PRIVATE"
}

# -------------------------------------------------------------------------
# ECS SD Cluster Module
# -------------------------------------------------------------------------

module "ecs_sd_cluster" {
  source = "../../modules/ecs-sd-cluster"

  name            = "ecs-sd-cluster"
  ecs_cluster_id  = data.aws_ecs_cluster.main.id
  image           = "123456789012.dkr.ecr.us-east-1.amazonaws.com/ecs-sd:v0.5.0"
  desired_count   = 3
  
  # ECS clusters to discover (configure based on your environment)
  ecs_clusters = [
    "production-apps",
    "api-services"
  ]
  
  # Networking
  vpc_id     = data.aws_vpc.main.id
  subnets    = data.aws_subnets.private.ids
  
  # Service Discovery
  service_discovery_namespace_id = data.aws_service_discovery_dns_namespace.main.id
  
  # Security - adjust CIDRs to your network
  allowed_cidr_blocks = [
    data.aws_vpc.main.cidr_block,
    "10.0.0.0/8"    # RFC1918 private
  ]
  
  # Resource sizing
  cpu    = 256   # 0.25 vCPU
  memory = 512   # 512 MB
  
  # Logging
  log_retention_days = 14
  
  tags = {
    Environment = "production"
    Team        = "observability"
    Project     = "ecs-sd"
  }
}

# -------------------------------------------------------------------------
# Outputs
# -------------------------------------------------------------------------

output "ecs_sd_service_name" {
  description = "ECS service name"
  value       = module.ecs_sd_cluster.service_name
}

output "ecs_sd_security_group_id" {
  description = "Security group ID"
  value       = module.ecs_sd_cluster.security_group_id
}

output "ecs_sd_discovery_service" {
  description = "Cloud Map service discovery name"
  value       = module.ecs_sd_cluster.discovery_service_name
}

output "ecs_sd_cluster_seeds" {
  description = "Cluster seeds endpoint (for reference)"
  value       = module.ecs_sd_cluster.cluster_seeds
}

output "ecs_sd_log_group" {
  description = "CloudWatch log group name"
  value       = module.ecs_sd_cluster.log_group_name
}
