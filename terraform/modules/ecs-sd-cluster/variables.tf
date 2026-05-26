variable "name" {
  description = "Service name for the ecs-sd cluster"
  type        = string
}

variable "ecs_cluster_id" {
  description = "ECS cluster ID to deploy the service into"
  type        = string
}

variable "image" {
  description = "Container image URI for ecs-sd"
  type        = string
}

variable "desired_count" {
  description = "Number of ecs-sd instances to run"
  type        = number
  default     = 3
}

variable "cpu" {
  description = "Fargate vCPU units (256 = 0.25 vCPU)"
  type        = number
  default     = 256
}

variable "memory" {
  description = "Fargate memory in MB"
  type        = number
  default     = 512
}

variable "gossip_port" {
  description = "UDP port for gossip protocol (inter-node communication)"
  type        = number
  default     = 8081
}

variable "ecs_clusters" {
  description = "List of ECS cluster names to discover services from"
  type        = list(string)
}

variable "subnets" {
  description = "List of VPC subnet IDs for the ECS tasks"
  type        = list(string)
}

variable "vpc_id" {
  description = "VPC ID for security group creation"
  type        = string
}

variable "service_discovery_namespace_id" {
  description = "Cloud Map namespace ID for service discovery"
  type        = string
}

variable "allowed_cidr_blocks" {
  description = "CIDR blocks allowed to access the HTTP API"
  type        = list(string)
  default     = ["10.0.0.0/8"]
}

variable "log_retention_days" {
  description = "CloudWatch log retention in days"
  type        = number
  default     = 7
}

variable "tags" {
  description = "Tags to apply to all resources"
  type        = map(string)
  default     = {}
}
