# AdapterOS Terraform Variables

variable "aws_region" {
  description = "AWS region for deployment"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be one of: dev, staging, prod"
  }
}

variable "vpc_cidr" {
  description = "CIDR block for VPC"
  type        = string
  default     = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "Availability zones for deployment"
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b", "us-east-1c"]
}

variable "private_subnets" {
  description = "Private subnet CIDR blocks"
  type        = list(string)
  default     = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
}

variable "public_subnets" {
  description = "Public subnet CIDR blocks"
  type        = list(string)
  default     = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
}

variable "allowed_ssh_cidrs" {
  description = "CIDR blocks allowed for SSH access (production only)"
  type        = list(string)
  default     = []
}

# Database Configuration
variable "db_instance_class" {
  description = "RDS instance class"
  type        = string
  default     = "db.t3.medium"
}

variable "db_allocated_storage" {
  description = "Initial allocated storage for database (GB)"
  type        = number
  default     = 20
}

variable "db_max_allocated_storage" {
  description = "Maximum allocated storage for database (GB)"
  type        = number
  default     = 100
}

variable "db_username" {
  description = "Database username"
  type        = string
  default     = "adapteros"
}

variable "db_password" {
  description = "Database password"
  type        = string
  sensitive   = true
}

# Cache Configuration
variable "redis_node_type" {
  description = "Redis node type"
  type        = string
  default     = "cache.t3.micro"
}

# ECS Configuration
variable "task_cpu" {
  description = "ECS task CPU units"
  type        = string
  default     = "1024"
}

variable "task_memory" {
  description = "ECS task memory (MB)"
  type        = string
  default     = "2048"
}

variable "desired_count" {
  description = "Desired number of ECS tasks"
  type        = number
  default     = 2
}

# Container Configuration
variable "image_tag" {
  description = "Docker image tag to deploy"
  type        = string
  default     = "latest"
}

# Domain Configuration
variable "domain_name" {
  description = "Domain name for SSL certificate"
  type        = string
  default     = "api.adapteros.com"
}

# =============================================================================
# ENVIRONMENT-SPECIFIC OVERRIDES
# =============================================================================

# Development environment defaults
locals {
  dev_defaults = {
    db_instance_class = "db.t3.micro"
    redis_node_type  = "cache.t3.micro"
    task_cpu         = "512"
    task_memory      = "1024"
    desired_count    = 1
  }

  # Staging environment defaults
  staging_defaults = {
    db_instance_class = "db.t3.small"
    redis_node_type  = "cache.t3.small"
    task_cpu         = "1024"
    task_memory      = "2048"
    desired_count    = 2
  }

  # Production environment defaults
  prod_defaults = {
    db_instance_class = "db.t3.large"
    redis_node_type  = "cache.t3.medium"
    task_cpu         = "2048"
    task_memory      = "4096"
    desired_count    = 3
  }

  # Select defaults based on environment
  config = var.environment == "prod" ? local.prod_defaults : (
    var.environment == "staging" ? local.staging_defaults : local.dev_defaults
  )
}

