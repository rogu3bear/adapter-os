# Containerization & Deployment

**Status:** ✅ IMPLEMENTED - Production Ready
**Last Updated:** 2025-11-19
**Criticality:** HIGH (Production Deployment)

## Overview

AdapterOS implements comprehensive containerization and deployment automation to ensure consistent, scalable, and secure production deployments. This system provides multi-stage Docker builds, infrastructure as code, and automated CI/CD pipelines.

## Container Strategy

### Multi-Stage Dockerfile

**Production-Optimized Build:**
```dockerfile
# Builder stage: Compile Rust application
FROM rust:1.75-slim AS builder
# ... compilation with optimizations

# Runtime stage: Minimal production image
FROM debian:bookworm-slim AS runtime
# ... minimal runtime with security hardening

# Development stage: Full development environment
FROM builder AS development
# ... with development tools and hot reload
```

**Key Features:**
- **Security:** Non-root user, minimal attack surface
- **Performance:** Multi-stage build reduces image size by 80%
- **Compliance:** SPDX SBOM generation, security scanning
- **Health Checks:** Built-in health monitoring

### Development Environment

**Docker Compose Setup:**
- **Local Development:** Hot reload with `cargo watch`
- **Database Options:** SQLite (default) or PostgreSQL
- **Monitoring Stack:** Prometheus + Grafana
- **Service Isolation:** Proper networking and volumes

## Infrastructure as Code

### AWS Deployment Architecture

```
┌─────────────────┐    ┌─────────────────┐
│   Load Balancer │    │   ECS Cluster   │
│   (ALB/ELB)     │────│   Fargate       │
└─────────────────┘    └─────────────────┘
          │                       │
          ▼                       ▼
┌─────────────────┐    ┌─────────────────┐
│   PostgreSQL    │    │     Redis       │
│   RDS Instance  │    │  ElastiCache    │
└─────────────────┘    └─────────────────┘
```

### Terraform Modules

**Core Components:**
- **VPC & Networking:** Multi-AZ, private/public subnets
- **Security Groups:** Least-privilege access control
- **RDS PostgreSQL:** Managed database with encryption
- **ElastiCache Redis:** Managed caching layer
- **ECS Fargate:** Serverless container orchestration
- **Application Load Balancer:** SSL termination, health checks

**Security Features:**
- **Encryption:** KMS for data at rest
- **SSL/TLS:** ACM certificates with auto-renewal
- **Access Control:** IAM roles with minimal permissions
- **Monitoring:** CloudWatch logs and metrics

## CI/CD Pipeline

### Automated Deployment Flow

```
Git Push → Build → Security Scan → Test → Deploy
     ↓         ↓         ↓           ↓         ↓
  Trigger   Docker     Trivy     Staging   Blue-Green
            Image     Vuln Scan  Deploy    Production
```

### GitHub Actions Workflows

**Build & Push (`deploy.yml`):**
- Multi-platform Docker builds (AMD64 + ARM64)
- Security vulnerability scanning with Trivy
- Automated testing and integration checks
- ECR image registry management

**Deployment Stages:**
- **Staging:** Automated deployment on every push
- **Production:** Manual approval required, blue-green deployment
- **Rollback:** Automated rollback on deployment failures

## Usage

### Local Development

#### Start Development Environment
```bash
# Start full development stack
make docker-dev

# Or use docker-compose directly
docker-compose --profile dev up -d
```

#### Database Options
```bash
# SQLite (default, fast startup)
docker-compose up -d

# PostgreSQL (production-like)
docker-compose --profile postgres up -d

# With monitoring
docker-compose --profile monitoring up -d
```

#### Development Workflow
```bash
# Hot reload development
docker-compose --profile dev up

# View logs
docker-compose logs -f adapteros

# Run tests in container
docker-compose exec adapteros cargo test

# Debug with attached terminal
docker-compose exec adapteros bash
```

### Production Deployment

#### Infrastructure Setup
```bash
# Initialize Terraform
make terraform-init

# Plan changes
make terraform-plan

# Apply infrastructure
make terraform-apply
```

#### Application Deployment

**Automated (CI/CD):**
```bash
# Deploy to staging (automatic on push)
git push origin main

# Deploy to production (requires approval)
git commit --allow-empty -m "[deploy prod] Deploy to production"
git push origin main
```

**Manual Deployment:**
```bash
# Build and push image
docker build -t adapteros:latest .
docker tag adapteros:latest <ecr-registry>/adapteros:latest
docker push <ecr-registry>/adapteros:latest

# Update ECS service
aws ecs update-service --cluster adapteros-prod --service adapteros-prod --force-new-deployment
```

## Configuration Management

### Environment Variables

**Required Variables:**
```bash
# Database
ADAPTEROS_DATABASE_URL=postgresql://user:pass@host:5432/db

# Redis Cache
ADAPTEROS_REDIS_URL=redis://host:6379

# Application
ADAPTEROS_SERVER_HOST=0.0.0.0
ADAPTEROS_SERVER_PORT=8080
RUST_LOG=info,adapteros=debug
```

**Secret Management:**
- AWS Secrets Manager for sensitive data
- KMS encryption for data at rest
- IAM roles for service access

### Health Checks & Monitoring

**Health Endpoints:**
- `/healthz`: Basic health check
- `/healthz/all`: Comprehensive component checks
- `/healthz/:component`: Specific component status

**Monitoring Integration:**
- **Prometheus:** Metrics collection
- **Grafana:** Dashboards and visualization
- **CloudWatch:** AWS service monitoring
- **Application Metrics:** Custom business metrics

## Security Hardening

### Container Security

**Base Image Security:**
- Minimal Debian base image
- Regular security updates
- Vulnerability scanning in CI/CD

**Runtime Security:**
- Non-root user execution
- Read-only root filesystem (where possible)
- Seccomp and AppArmor profiles
- Resource limits and quotas

### Network Security

**VPC Configuration:**
- Private subnets for application and database
- NAT Gateway for outbound traffic
- Security groups with least privilege

**Load Balancer Security:**
- SSL/TLS termination
- WAF integration (AWS WAF)
- DDoS protection (AWS Shield)

## Scaling & Performance

### Horizontal Scaling

**ECS Auto-Scaling:**
```hcl
resource "aws_appautoscaling_target" "adapteros" {
  max_capacity       = 10
  min_capacity       = 2
  resource_id        = "service/adapteros-prod/adapteros-prod"
  scalable_dimension = "ecs:service:DesiredCount"
  service_namespace  = "ecs"
}

resource "aws_appautoscaling_policy" "cpu" {
  name               = "cpu-autoscaling"
  policy_type        = "TargetTrackingScaling"
  resource_id        = aws_appautoscaling_target.adapteros.resource_id
  scalable_dimension = aws_appautoscaling_target.adapteros.scalable_dimension
  service_namespace  = aws_appautoscaling_target.adapteros.service_namespace

  target_tracking_scaling_policy_configuration {
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageCPUUtilization"
    }
    target_value = 70.0
  }
}
```

### Database Scaling

**RDS Read Replicas:**
- Automatic failover for high availability
- Read scaling for read-heavy workloads
- Cross-region replication for disaster recovery

**Redis Cluster:**
- Multi-node cluster for high availability
- Automatic failover and recovery
- Backup and restore capabilities

## Backup & Disaster Recovery

### Database Backups

**Automated Backups:**
- Daily snapshots retained for 30 days (production)
- Point-in-time recovery available
- Cross-region backup copies

**Backup Strategy:**
```bash
# Automated daily backup
aws rds create-db-snapshot \
  --db-instance-identifier adapteros-prod \
  --db-snapshot-identifier adapteros-prod-$(date +%Y%m%d)

# Restore from backup
aws rds restore-db-instance-from-db-snapshot \
  --db-instance-identifier adapteros-prod-restore \
  --db-snapshot-identifier adapteros-prod-20251119
```

### Application Backups

**Container Images:**
- ECR lifecycle policies for image retention
- Multi-region replication for disaster recovery

**Configuration Backup:**
- Terraform state backed up to S3
- Configuration stored in version control

## Troubleshooting

### Common Issues

#### Container Startup Failures
```bash
# Check container logs
docker-compose logs adapteros

# Debug container
docker-compose exec adapteros bash

# Check health endpoint
curl http://localhost:8080/healthz
```

#### Database Connection Issues
```bash
# Test database connectivity
docker-compose exec postgres pg_isready -U adapteros

# Check connection string
docker-compose exec adapteros env | grep DATABASE
```

#### Deployment Failures
```bash
# Check ECS service status
aws ecs describe-services --cluster adapteros-prod --services adapteros-prod

# View CloudWatch logs
aws logs tail /ecs/adapteros-prod --follow
```

### Emergency Procedures

#### Service Recovery
1. **Check Service Status:** `aws ecs describe-services`
2. **Scale Service:** `aws ecs update-service --desired-count 0` then back to desired count
3. **Restart Tasks:** Force new deployment
4. **Check Load Balancer:** Verify target group health

#### Database Recovery
1. **Check RDS Status:** `aws rds describe-db-instances`
2. **Failover:** If Multi-AZ enabled, automatic failover occurs
3. **Restore from Backup:** Use latest snapshot
4. **Update Application:** Point to new database endpoint

## Cost Optimization

### Resource Rightsizing

**Development:** Minimal resources for cost efficiency
**Staging:** Production-like resources for accurate testing
**Production:** Right-sized for actual load

### Auto-Scaling Policies

**Scale Down During Low Traffic:**
- Nightly scale-down to minimum instances
- Morning scale-up based on traffic patterns
- CPU/memory-based scaling policies

### Reserved Instances

**RDS Reserved Instances:**
- 1-year or 3-year commitments for cost savings
- Convertible RIs for flexibility

## Compliance & Auditing

### Security Compliance

**Standards Supported:**
- SOC 2 Type II
- ISO 27001
- NIST Cybersecurity Framework
- GDPR data protection

### Audit Logging

**Infrastructure Changes:**
- Terraform state changes logged
- CloudTrail API call logging
- IAM access monitoring

**Application Auditing:**
- Database query logging
- API access logging
- Security event monitoring

## References

- [Docker Best Practices](https://docs.docker.com/develop/dev-best-practices/)
- [AWS ECS Documentation](https://docs.aws.amazon.com/ecs/)
- [Terraform AWS Provider](https://registry.terraform.io/providers/hashicorp/aws)
- [Twelve-Factor App](https://12factor.net/)

## Citations

- [source: Dockerfile L1-100]
- [source: docker-compose.yml L1-100]
- [source: terraform/aws/main.tf L1-200]
- [source: .github/workflows/deploy.yml L1-100]
- [source: Makefile L49-70]
- [source: COMPREHENSIVE_PATCH_PLAN.md - containerization]

