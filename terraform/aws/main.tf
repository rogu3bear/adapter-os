# AdapterOS Infrastructure as Code
# AWS Deployment Configuration

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  backend "s3" {
    bucket = "adapteros-terraform-state"
    key    = "adapteros-infrastructure.tfstate"
    region = "us-east-1"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "AdapterOS"
      Environment = var.environment
      ManagedBy   = "Terraform"
    }
  }
}

# =============================================================================
# NETWORKING
# =============================================================================

module "vpc" {
  source = "terraform-aws-modules/vpc/aws"

  name = "adapteros-${var.environment}"
  cidr = var.vpc_cidr

  azs             = var.availability_zones
  private_subnets = var.private_subnets
  public_subnets  = var.public_subnets

  enable_nat_gateway = true
  single_nat_gateway = var.environment == "prod" ? false : true

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# Security Groups
resource "aws_security_group" "adapteros" {
  name_prefix = "adapteros-"
  vpc_id      = module.vpc.vpc_id

  # HTTP/HTTPS ingress
  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
    description = "HTTP"
  }

  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
    description = "HTTPS"
  }

  # SSH for maintenance (restrict in production)
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.environment == "prod" ? var.allowed_ssh_cidrs : ["0.0.0.0/0"]
    description = "SSH"
  }

  # Database access (internal only)
  ingress {
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [aws_security_group.adapteros.id]
    description     = "PostgreSQL"
  }

  # Redis access (internal only)
  ingress {
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [aws_security_group.adapteros.id]
    description     = "Redis"
  }

  # Egress - all outbound allowed
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# DATABASE
# =============================================================================

resource "aws_db_subnet_group" "adapteros" {
  name       = "adapteros-${var.environment}"
  subnet_ids = module.vpc.private_subnets

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_db_instance" "adapteros" {
  identifier = "adapteros-${var.environment}"

  # Instance configuration
  instance_class    = var.db_instance_class
  engine           = "postgres"
  engine_version   = "15.4"
  allocated_storage = var.db_allocated_storage

  # Database configuration
  db_name  = "adapteros"
  username = var.db_username
  password = var.db_password

  # Networking
  db_subnet_group_name   = aws_db_subnet_group.adapteros.name
  vpc_security_group_ids = [aws_security_group.adapteros.id]
  publicly_accessible    = false

  # Backup and maintenance
  backup_retention_period = var.environment == "prod" ? 30 : 7
  backup_window          = "03:00-04:00"
  maintenance_window     = "sun:04:00-sun:05:00"

  # Monitoring
  monitoring_interval = 60
  monitoring_role_arn = aws_iam_role.rds_enhanced_monitoring.arn

  # Security
  storage_encrypted = true
  kms_key_id       = aws_kms_key.adapteros.arn

  # Performance
  performance_insights_enabled = true
  max_allocated_storage        = var.db_max_allocated_storage

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# RDS Enhanced Monitoring Role
resource "aws_iam_role" "rds_enhanced_monitoring" {
  name = "adapteros-rds-enhanced-monitoring-${var.environment}"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "monitoring.rds.amazonaws.com"
        }
      }
    ]
  })

  managed_policy_arns = ["arn:aws:iam::aws:policy/service-role/AmazonRDSEnhancedMonitoringRole"]
}

# =============================================================================
# CACHE (Redis)
# =============================================================================

resource "aws_elasticache_subnet_group" "adapteros" {
  name       = "adapteros-${var.environment}"
  subnet_ids = module.vpc.private_subnets
}

resource "aws_elasticache_cluster" "adapteros" {
  cluster_id           = "adapteros-${var.environment}"
  engine              = "redis"
  node_type           = var.redis_node_type
  num_cache_nodes     = var.environment == "prod" ? 2 : 1
  parameter_group_name = "default.redis7"
  port                = 6379

  subnet_group_name = aws_elasticache_subnet_group.adapteros.name
  security_group_ids = [aws_security_group.adapteros.id]

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# APPLICATION LOAD BALANCER
# =============================================================================

resource "aws_lb" "adapteros" {
  name               = "adapteros-${var.environment}"
  internal           = false
  load_balancer_type = "application"
  security_groups    = [aws_security_group.adapteros.id]
  subnets            = module.vpc.public_subnets

  enable_deletion_protection = var.environment == "prod"

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_lb_target_group" "adapteros" {
  name     = "adapteros-${var.environment}"
  port     = 8080
  protocol = "HTTP"
  vpc_id   = module.vpc.vpc_id

  health_check {
    enabled             = true
    healthy_threshold   = 2
    unhealthy_threshold = 2
    timeout             = 5
    interval            = 30
    path                = "/healthz"
    matcher             = "200"
  }

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.adapteros.arn
  port              = "80"
  protocol          = "HTTP"

  default_action {
    type             = "redirect"
    redirect {
      port        = "443"
      protocol    = "HTTPS"
      status_code = "HTTP_301"
    }
  }
}

resource "aws_lb_listener" "https" {
  load_balancer_arn = aws_lb.adapteros.arn
  port              = "443"
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-2016-08"
  certificate_arn   = aws_acm_certificate.adapteros.arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.adapteros.arn
  }
}

# =============================================================================
# ECS CLUSTER & SERVICE
# =============================================================================

resource "aws_ecs_cluster" "adapteros" {
  name = "adapteros-${var.environment}"

  setting {
    name  = "containerInsights"
    value = "enabled"
  }

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_ecs_task_definition" "adapteros" {
  family                   = "adapteros-${var.environment}"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn           = aws_iam_role.ecs_task.arn

  container_definitions = jsonencode([
    {
      name  = "adapteros"
      image = "${aws_ecr_repository.adapteros.repository_url}:${var.image_tag}"

      portMappings = [
        {
          containerPort = 8080
          hostPort      = 8080
          protocol      = "tcp"
        }
      ]

      environment = [
        {
          name  = "ADAPTEROS_DATABASE_URL"
          value = "postgresql://${var.db_username}:${var.db_password}@${aws_db_instance.adapteros.endpoint}/${aws_db_instance.adapteros.db_name}"
        },
        {
          name  = "ADAPTEROS_REDIS_URL"
          value = "redis://${aws_elasticache_cluster.adapteros.cache_nodes[0].address}:${aws_elasticache_cluster.adapteros.cache_nodes[0].port}"
        },
        {
          name  = "RUST_LOG"
          value = "info,adapteros=debug"
        }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.adapteros.name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }

      healthCheck = {
        command = ["CMD-SHELL", "curl -f http://localhost:8080/healthz || exit 1"]
        interval = 30
        timeout  = 5
        retries  = 3
      }

      essential = true
    }
  ])

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_ecs_service" "adapteros" {
  name            = "adapteros-${var.environment}"
  cluster         = aws_ecs_cluster.adapteros.id
  task_definition = aws_ecs_task_definition.adapteros.arn
  desired_count   = var.desired_count

  network_configuration {
    security_groups = [aws_security_group.adapteros.id]
    subnets         = module.vpc.private_subnets
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.adapteros.arn
    container_name   = "adapteros"
    container_port   = 8080
  }

  depends_on = [aws_lb_listener.https]

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# IAM ROLES
# =============================================================================

resource "aws_iam_role" "ecs_execution" {
  name = "adapteros-ecs-execution-${var.environment}"

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

  managed_policy_arns = [
    "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
  ]
}

resource "aws_iam_role" "ecs_task" {
  name = "adapteros-ecs-task-${var.environment}"

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

  inline_policy {
    name = "adapteros-task-policy"
    policy = jsonencode({
      Version = "2012-10-17"
      Statement = [
        {
          Effect = "Allow"
          Action = [
            "rds:DescribeDBInstances",
            "elasticache:DescribeCacheClusters",
            "secretsmanager:GetSecretValue"
          ]
          Resource = "*"
        }
      ]
    })
  }
}

# =============================================================================
# MONITORING & LOGGING
# =============================================================================

resource "aws_cloudwatch_log_group" "adapteros" {
  name              = "/ecs/adapteros-${var.environment}"
  retention_in_days = var.environment == "prod" ? 90 : 30

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# SECURITY
# =============================================================================

resource "aws_kms_key" "adapteros" {
  description             = "AdapterOS encryption key"
  deletion_window_in_days = 30

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

resource "aws_acm_certificate" "adapteros" {
  domain_name       = var.domain_name
  validation_method = "DNS"

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# CONTAINER REGISTRY
# =============================================================================

resource "aws_ecr_repository" "adapteros" {
  name                 = "adapteros-${var.environment}"
  image_tag_mutability = "MUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }

  tags = {
    Name = "adapteros-${var.environment}"
  }
}

# =============================================================================
# OUTPUTS
# =============================================================================

output "load_balancer_dns" {
  description = "DNS name of the load balancer"
  value       = aws_lb.adapteros.dns_name
}

output "database_endpoint" {
  description = "Database endpoint"
  value       = aws_db_instance.adapteros.endpoint
  sensitive   = true
}

output "redis_endpoint" {
  description = "Redis cluster endpoint"
  value       = aws_elasticache_cluster.adapteros.cache_nodes[0].address
}

output "ecr_repository_url" {
  description = "ECR repository URL"
  value       = aws_ecr_repository.adapteros.repository_url
}

