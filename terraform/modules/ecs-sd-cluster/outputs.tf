output "service_name" {
  description = "ECS service name"
  value       = aws_ecs_service.this.name
}

output "service_arn" {
  description = "ECS service ARN"
  value       = aws_ecs_service.this.id
}

output "security_group_id" {
  description = "Security group ID for the ecs-sd tasks"
  value       = aws_security_group.this.id
}

output "discovery_service_name" {
  description = "Cloud Map service discovery name"
  value       = aws_service_discovery_service.this.name
}

output "discovery_service_arn" {
  description = "Cloud Map service discovery ARN"
  value       = aws_service_discovery_service.this.arn
}

output "task_definition_arn" {
  description = "ECS task definition ARN"
  value       = aws_ecs_task_definition.this.arn
}

output "execution_role_arn" {
  description = "IAM execution role ARN"
  value       = aws_iam_role.execution.arn
}

output "task_role_arn" {
  description = "IAM task role ARN"
  value       = aws_iam_role.task.arn
}

output "log_group_name" {
  description = "CloudWatch log group name"
  value       = aws_cloudwatch_log_group.this.name
}

output "cluster_seeds" {
  description = "The ECS_SD_CLUSTER_SEEDS value for manual reference"
  value       = "${var.name}.${aws_service_discovery_service.this.name}.${data.aws_service_discovery_dns_namespace.this.name}:${var.gossip_port}"
  sensitive   = false
}
