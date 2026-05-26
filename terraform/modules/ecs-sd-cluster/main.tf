# ECS SD Cluster Module
# Deploys ecs-sd in cluster mode on AWS Fargate with service discovery

terraform {
  required_version = ">= 1.5.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}

# CloudWatch Log Group for container logs
resource "aws_cloudwatch_log_group" "this" {
  name              = "/ecs/${var.name}"
  retention_in_days = var.log_retention_days
  tags              = var.tags
}

# Security Group for ecs-sd tasks
resource "aws_security_group" "this" {
  name        = "${var.name}-sg"
  description = "Security group for ecs-sd cluster"
  vpc_id      = var.vpc_id

  # HTTP API ingress
  ingress {
    description = "HTTP API"
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidr_blocks
  }

  # UDP Gossip - self-referencing for inter-node communication
  ingress {
    description = "Gossip protocol (inter-node)"
    from_port   = var.gossip_port
    to_port     = var.gossip_port
    protocol    = "udp"
    self        = true
  }

  # Egress all
  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(var.tags, {
    Name = "${var.name}-sg"
  })
}

# IAM Role for ECS Task Execution
resource "aws_iam_role" "execution" {
  name = "${var.name}-execution-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
      }
    ]
  })

  tags = var.tags
}

resource "aws_iam_role_policy_attachment" "execution_managed" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

# IAM Role for ECS Task (application permissions)
resource "aws_iam_role" "task" {
  name = "${var.name}-task-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
      }
    ]
  })

  tags = var.tags
}

# Policy for ecs-sd to read ECS clusters and Cloud Map
resource "aws_iam_role_policy" "task_ecs_sd" {
  name = "${var.name}-ecs-sd-policy"
  role = aws_iam_role.task.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "ecs:ListClusters",
          "ecs:ListServices",
          "ecs:DescribeServices",
          "ecs:DescribeTasks",
          "ecs:ListTasks",
          "ec2:DescribeInstances",
          "servicediscovery:DiscoverInstances",
          "servicediscovery:GetService",
          "servicediscovery:ListServices",
          "servicediscovery:GetNamespace",
          "servicediscovery:ListNamespaces"
        ]
        Resource = "*"
      }
    ]
  })
}

# Cloud Map Service Discovery
resource "aws_service_discovery_service" "this" {
  name = var.name

  dns_config {
    namespace_id = var.service_discovery_namespace_id

    dns_records {
      ttl  = 10
      type = "A"
    }

    routing_policy = "MULTIVALUE"
  }

  health_check_custom_config {
    failure_threshold = 1
  }

  tags = var.tags
}

# ECS Task Definition
resource "aws_ecs_task_definition" "this" {
  family                   = var.name
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.cpu
  memory                   = var.memory
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.task.arn

  container_definitions = jsonencode([
    {
      name  = var.name
      image = var.image
      essential = true
      
      portMappings = [
        {
          containerPort = 8080
          protocol      = "tcp"
        },
        {
          containerPort = var.gossip_port
          protocol      = "udp"
        }
      ]

      environment = [
        {
          name  = "ECS_SD_CLUSTER_MODE"
          value = "cluster"
        },
        {
          name  = "ECS_SD_CLUSTER_SEEDS"
          value = "${var.name}.${aws_service_discovery_service.this.name}.${data.aws_service_discovery_dns_namespace.this.name}:${var.gossip_port}"
        },
        {
          name  = "ECS_SD_GOSSIP_PORT"
          value = tostring(var.gossip_port)
        },
        {
          name  = "ECS_SD_CLUSTERS"
          value = join(",", var.ecs_clusters)
        },
        {
          name  = "ECS_SD_HTTP_PORT"
          value = "8080"
        },
        {
          name  = "RUST_LOG"
          value = "info"
        }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.this.name
          "awslogs-region"        = data.aws_region.current.name
          "awslogs-stream-prefix" = "ecs"
        }
      }

      healthCheck = {
        command     = ["CMD-SHELL", "curl -f http://localhost:8080/health || exit 1"]
        interval    = 30
        timeout     = 5
        retries     = 3
        startPeriod = 60
      }
    }
  ])

  tags = var.tags
}

# Data sources
data "aws_region" "current" {}

data "aws_service_discovery_dns_namespace" "this" {
  id = var.service_discovery_namespace_id
}

# ECS Service
resource "aws_ecs_service" "this" {
  name            = var.name
  cluster         = var.ecs_cluster_id
  task_definition = aws_ecs_task_definition.this.arn
  desired_count   = var.desired_count
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = var.subnets
    security_groups  = [aws_security_group.this.id]
    assign_public_ip = false
  }

  service_registries {
    registry_arn = aws_service_discovery_service.this.arn
  }

  deployment_configuration {
    maximum_percent         = 200
    minimum_healthy_percent = 100
  }

  tags = var.tags
}
