# Database Schema Documentation

## Overview

Comprehensive documentation of the AdapterOS database schema with static diagrams and animated workflow sequences. This documentation provides complete visibility into database structure, relationships, and operational processes.

## Quick Navigation

### For Developers
Start here if you're building on AdapterOS or contributing to the codebase:
1. [Schema Diagram](schema-diagram.md) - Complete ER diagram with all tables and relationships
2. [Adapter Lifecycle](workflows/adapter-lifecycle.md) - Adapter management workflows
3. [Promotion Pipeline](workflows/promotion-pipeline.md) - Deployment processes
4. [Code Intelligence Flow](workflows/code-intelligence.md) - Repository integration workflows

### For Operators
Start here if you're deploying and managing AdapterOS:
1. [Monitoring Flow](workflows/monitoring-flow.md) - Real-time monitoring and metrics
2. [Incident Response](workflows/incident-response.md) - Troubleshooting and resolution
3. [Performance Dashboard](workflows/performance-dashboard.md) - Performance visualization
4. [Replication & Distribution](workflows/replication-distribution.md) - Cross-node synchronization

### For Security Auditors
Start here if you're evaluating AdapterOS for compliance:
1. [Security & Compliance](workflows/security-compliance.md) - Compliance workflows and verification
2. [Schema Diagram](schema-diagram.md) - Data relationships, constraints, and isolation
3. [Promotion Pipeline](workflows/promotion-pipeline.md) - Change control and approvals

## Schema Overview

### Core Database Features
- **30+ Tables**: Comprehensive coverage of all system components
- **Multi-tenant Isolation**: ITAR-compliant tenant boundaries
- **Cryptographic Security**: BLAKE3 hashing and Ed25519 signatures
- **Audit Trails**: Complete operation history for compliance
- **Performance Optimization**: Strategic indexing and aggregation tables

### Key Schema Groups

1. **Identity & Access Control**
   - `users` - Multi-role authentication
   - `tenants` - Multi-tenant isolation boundaries
   - `jwt_secrets` - Token rotation tracking

2. **Infrastructure Management**
   - `nodes` - Worker host management
   - `workers` - Process lifecycle management
   - `system_metrics` - Performance monitoring
   - `system_health_checks` - Health status tracking

3. **Model & Artifact Management**
   - `models` - Base model artifacts
   - `artifacts` - Content-addressed storage
   - `bundle_signatures` - Cryptographic verification

4. **Adapter Lifecycle**
   - `adapters` - LoRA adapters with state management
   - `ephemeral_adapters` - Commit-aware temporary adapters
   - `adapter_provenance` - Cryptographic signer tracking

5. **Configuration & Plans**
   - `manifests` - Declarative configuration
   - `plans` - Compiled execution plans
   - `cp_pointers` - Active plan pointers

6. **Policies & Compliance**
   - `policies` - Policy packs per tenant
   - `code_policies` - Code-specific configurations
   - `audits` - Compliance checks and metrics

7. **Code Intelligence**
   - `repositories` - Registered code repositories
   - `commits` - Commit metadata and analysis
   - `patch_proposals` - AI-generated code patches

8. **Telemetry & Monitoring**
   - `telemetry_bundles` - NDJSON event bundles
   - `system_metrics` - Real-time performance data
   - `threshold_violations` - Performance alerts

9. **Security & Incidents**
   - `incidents` - Security and policy violations
   - `enclave_operations` - Secure enclave audit trail
   - `key_metadata` - Cryptographic key lifecycle

10. **Replication & Distribution**
    - `replication_journal` - Cross-node replication
    - `replication_artifacts` - Artifact transfers

## Workflow Animations

### High Priority (Essential Operations)

#### [Adapter Lifecycle Flow](workflows/adapter-lifecycle.md)
Shows the complete adapter journey from creation to deployment, including all state transitions:
- Adapter registration and categorization
- State transitions: unloaded → cold → warm → hot
- Plan references and worker deployment
- Activation tracking and memory management

#### [Promotion Pipeline Flow](workflows/promotion-pipeline.md)
Critical deployment process from manifest to production:
- Manifest creation and plan compilation
- Dry run promotion and quality checks
- Signing and CP pointer updates
- Worker deployment and performance monitoring
- Rollback procedures and safety mechanisms

#### [Real-time Monitoring Flow](workflows/monitoring-flow.md)
Operational visibility for system health and performance:
- Metrics collection and processing
- Telemetry bundle creation
- Threshold violation detection
- Incident generation and alert management

### Medium Priority (Advanced Operations)

#### [Security & Compliance Flow](workflows/security-compliance.md)
Compliance verification and audit trail management:
- Artifact signature generation and verification
- Secure enclave operations
- Policy compliance checks
- ITAR restriction enforcement

#### [Replication & Distribution Flow](workflows/replication-distribution.md)
Infrastructure understanding for scaling:
- Cross-node artifact synchronization
- Replication journal management
- Transfer sessions and verification
- Hash validation and deployment

### Advanced Features (Optional)

#### [Code Intelligence Flow](workflows/code-intelligence.md)
Repository integration and ephemeral adapter generation:
- Repository analysis and commit tracking
- Symbol detection and framework identification
- Ephemeral adapter creation
- Patch proposal validation and deployment

#### [Performance Monitoring Dashboard](workflows/performance-dashboard.md)
Comprehensive performance visualization:
- CPU, memory, GPU, and network metrics
- Health check status indicators
- Performance trend analysis
- Real-time dashboard display

#### [Incident Response Flow](workflows/incident-response.md)
Troubleshooting guide for operational issues:
- Incident detection and classification
- Alert generation and acknowledgment
- Auto-remediation vs manual intervention
- Resolution tracking and verification

## Usage Examples

### Getting Started
1. **Understand the Schema**: Start with [schema-diagram.md](schema-diagram.md) to see all tables and relationships
2. **Learn Core Workflows**: Review [adapter-lifecycle.md](workflows/adapter-lifecycle.md) and [promotion-pipeline.md](workflows/promotion-pipeline.md)
3. **Explore Operations**: Study [monitoring-flow.md](workflows/monitoring-flow.md) for real-time observability

### For Troubleshooting
1. **Check Incidents**: Use [incident-response.md](workflows/incident-response.md) for error resolution patterns
2. **Monitor Performance**: Reference [monitoring-flow.md](workflows/monitoring-flow.md) for metrics analysis
3. **Verify Compliance**: Review [security-compliance.md](workflows/security-compliance.md) for audit trails

### For Development
1. **Code Integration**: Study [code-intelligence.md](workflows/code-intelligence.md) for repository workflows
2. **Replication**: Review [replication-distribution.md](workflows/replication-distribution.md) for scaling patterns
3. **Performance**: Use [performance-dashboard.md](workflows/performance-dashboard.md) for optimization

## Design Principles

### Multi-Tenant Architecture
- **Isolation**: All tenant-scoped data references `tenants.id` with CASCADE deletes
- **ITAR Compliance**: `itar_flag` enables export control restrictions
- **Resource Quotas**: Tenant-level resource allocation and monitoring

### Cryptographic Security
- **Content Addressing**: BLAKE3 hashes for integrity and deduplication
- **Digital Signatures**: Ed25519 signatures for critical operations
- **Key Management**: Secure enclave integration with key rotation
- **Provenance Tracking**: Cryptographic signer and registrar information

### Performance Optimization
- **Strategic Indexing**: Optimized indexes for common query patterns
- **Aggregation Tables**: Pre-computed metrics for dashboard performance
- **Connection Pooling**: Efficient database connection management
- **Query Optimization**: Structured for high-throughput operations

### Lifecycle Management
- **State Tracking**: Comprehensive state management for adapters, workers, and plans
- **Audit Trails**: Complete operation history for compliance and debugging
- **Version Control**: Policy and configuration versioning
- **Rollback Support**: Safe rollback mechanisms for deployments

### Observability & Monitoring
- **Real-time Metrics**: Comprehensive system performance monitoring
- **Health Checks**: Automated system health validation
- **Alert Management**: Configurable alerting with acknowledgment
- **Telemetry Bundles**: Structured event logging with verification

## Related Documentation

### Core Architecture
- [System Architecture](../architecture.md) - Overall system design
- [Control Plane](../control-plane.md) - Control plane lifecycle
- [Runaway Prevention](../runaway-prevention.md) - Safety mechanisms

### Code Intelligence
- [Code Intelligence](../code-intelligence/README.md) - Code analysis stack
- [CodeGraph Specification](../codegraph-spec.md) - Code graph design

### Operational Guides
- [Quick Start Guide](../QUICKSTART.md) - Getting started
- [aosctl Manual](../../crates/adapteros-cli/docs/aosctl_manual.md) - CLI reference
- [Deployment Guide](../DEPLOYMENT.md) - Deployment procedures

## Document Status

All database schema documentation complete as of 2025-11-13:

| Document | Status | Diagrams | Tables | Workflows |
|----------|--------|----------|--------|-----------|
| schema-diagram.md | ✅ | 1 ER | 30+ | - |
| adapter-lifecycle.md | ✅ | 1 sequence | 5 | 1 |
| promotion-pipeline.md | ✅ | 1 flowchart | 6 | 1 |
| monitoring-flow.md | ✅ | 1 graph | 8 | 1 |
| security-compliance.md | ✅ | 1 sequence | 7 | 1 |
| replication-distribution.md | ✅ | 1 graph | 3 | 1 |
| code-intelligence.md | ✅ | 1 flowchart | 4 | 1 |
| performance-dashboard.md | ✅ | 1 graph | 7 | 1 |
| incident-response.md | ✅ | 1 sequence | 6 | 1 |
| Recent Schema Updates | 🔄 | - | Updated | Recent migrations and schema enhancements |

**Note:** Recent modifications to the database schema include updates to support service supervisor integration and improved metrics collection. See git history for details.

## Contributing

When implementing or updating database schema documentation:
1. Reference the relevant workflow document
2. Follow Mermaid diagram syntax and best practices
3. Ensure all table references are accurate and current
4. Update cross-references in related documents
5. Run validation checks before submitting changes

See [VALIDATION.md](VALIDATION.md) for validation framework and [MAINTENANCE.md](MAINTENANCE.md) for maintenance procedures.

---
**AdapterOS Database Schema**: Comprehensive, secure, and auditable database design supporting multi-tenant, ITAR-compliant operations with full cryptographic verification and observability.

## Database Operations Outline

### Schema Entities
- **Adapters/Tenants**: Core isolation (tenant_id FK).
- **Metrics/Telemetry**: Time-series (activity_events.rs).

### Operations (src/)
- **Domain Ops** (domain_adapters.rs): CRUD with ACLs.
  - Distinguish: Queries (fetch_all) vs. Mutations (insert).
- **Migrations**: 56 SQL files, Postgres compat (migrations_postgres/).

### Access Pattern
- Pool: `Db::pool()`.
- Error: AosError::Database.

[source: crates/adapteros-db/src/lib.rs L1-L50]
[source: crates/adapteros-db/src/domain_adapters.rs L1-L100]
[source: crates/adapteros-db/migrations/ L1-L10]
