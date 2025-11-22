# 🚨 AdapterOS Documentation Index

**⚠️ CRITICAL: Read [CURRENT_STATUS_OVERRIDE.md](CURRENT_STATUS_OVERRIDE.md) first - it supersedes all other status information**

Organized documentation by functional area for easy navigation.

---

## 📋 Core Development Resources

### Development Process
- **[DEVELOPMENT_WORKFLOW.md](DEVELOPMENT_WORKFLOW.md)** - Complete development standards and processes
- **[DOCUMENTATION_MAINTENANCE.md](DOCUMENTATION_MAINTENANCE.md)** - Documentation standards and maintenance
- **[CURRENT_STATUS_OVERRIDE.md](CURRENT_STATUS_OVERRIDE.md)** - **Authoritative project status**

### Quality & Infrastructure
- **[INFRASTRUCTURE_HEALTH_GUIDE.md](INFRASTRUCTURE_HEALTH_GUIDE.md)** - Infrastructure monitoring and maintenance
- **[CONTRIBUTING.md](../../CONTRIBUTING.md)** - Contribution guidelines

---

## 📁 Directory Structure

### `/routing/` - Router Decision Tracking & K-Sparse Selection
Router telemetry, decision tracking, and K-sparse adapter selection.

- [README](./routing/README.md) - Overview and quick start
- [Telemetry V1 Skeleton Status](./routing/telemetry-v1-skeleton-status.md) - Complete infrastructure inventory
- [Telemetry Integration Guide](./routing/telemetry-integration-guide.md) - Step-by-step integration examples

**Related Files:**
- [Router Determinism Proof](./ROUTER_DETERMINISM_PROOF.md)
- [Router Trace Format](./ROUTER_TRACE_FORMAT.md)
- [Router Trace Actual Format](./ROUTER_TRACE_ACTUAL_FORMAT.md)

---

### `/training/` - Training Datasets & Jobs
Training infrastructure, dataset management, and job lifecycle.

- [Training Lifecycle Reflection](./TRAINING_LIFECYCLE_REFLECTION.md)
- [Training Fully Rectified](./TRAINING_FULLY_RECTIFIED.md)
- [Training Guide](./TRAINING.md)

**Related Files:**
- [Code Ingestion Status](./CODE_INGESTION_STATUS.md)
- [Codebase Ingestion](./CODEBASE_INGESTION.md)

---

### `/stacks/` - Adapter Stack Versioning & Management
Adapter stacks, versioning, and workflow composition.

- [Stack Versioning](./STACK_VERSIONING.md)

**Related Files:**
- [Adapter Taxonomy](./ADAPTER_TAXONOMY.md)
- [Adapter Lineage and Safe Mode](./ADAPTER_LINEAGE_AND_SAFE_MODE.md)

---

### `/determinism/` - Deterministic Execution & Verification
Determinism guarantees, attestation, and verification tooling.

- [Determinism Attestation](./DETERMINISM-ATTESTATION.md)
- [Determinism Audit](./determinism-audit.md)
- [Determinism Test Status](./DETERMINISM_TEST_STATUS.md)
- [PRD-08: Determinism Guardrail Suite](./PRD_8_DETERMINISM_GUARDRAIL_SUITE.md)

**Related Files:**
- [Phrase Continuity Plan](./DETERMINISM_PHRASE_CONTINUITY_PLAN.md)
- [Kernel Weight Loading Determinism](./KERNEL-WEIGHT-LOADING-DETERMINISM.md)

---

### `/replay/` - Replay Sessions & Divergence Detection
Replay infrastructure, session management, and divergence analysis.

- [Telemetry Bundle Verification](./TELEMETRY_BUNDLE_VERIFICATION.md)
- [Golden Runs Spec](./GOLDEN-RUNS-SPEC.md)

---

### `/metrics/` - System Metrics & Monitoring
Performance monitoring, system metrics, and observability.

- [System Metrics](./SYSTEM-METRICS.md)
- [Monitoring](./monitoring.md)
- [Auth Performance](./AUTH_PERFORMANCE.md)

**Related Files:**
- [Production Operations](./PRODUCTION_OPERATIONS.md)
- [Operational Runbooks](./OPERATIONAL_RUNBOOKS.md)
- [Operator Playbooks](./OPERATOR_PLAYBOOKS.md)

---

### `/cli_contract/` - CLI Interface & Contracts
CLI command reference, coverage analysis, and usage guides.

- [CLI Reference](./CLI_REFERENCE.md)
- [CLI Guide](./CLI_GUIDE.md)
- [CLI Coverage Analysis](./CLI_COVERAGE_ANALYSIS.md)

---

### `/behaviors/` - System Behaviors & Policies
Policy enforcement, RBAC, and runtime behaviors.

- [Policies](./POLICIES.md)
- [Policy Enforcement](./POLICY_ENFORCEMENT.md)
- [AOS Runtime Policy](./AOS_RUNTIME_POLICY.md)
- [Policy Hash Watcher](./POLICY-HASH-WATCHER.md)

**Related Files:**
- [Authentication](./AUTHENTICATION.md)
- [Evidence Retrieval](./EVIDENCE_RETRIEVAL.md)

---

### `/code_ingest/` - Code Intelligence & Ingestion
Code analysis, domain adapter layer, and codebase ingestion.

- [Code Ingestion Status](./CODE_INGESTION_STATUS.md)
- [Codebase Ingestion](./CODEBASE_INGESTION.md)
- [Domain Adapter Layer](./DOMAIN-ADAPTER-LAYER.md)
- [CodeGraph Spec](./CODEGRAPH-SPEC.md)

---

### `/architecture/` - System Architecture & Design
High-level architecture, component diagrams, and design decisions.

- [Architecture Index](./ARCHITECTURE_INDEX.md)
- [Architecture Overview](./ARCHITECTURE.md)
- [Services and Systems](./SERVICES_AND_SYSTEMS.md)
- [Concepts](./CONCEPTS.md)
- [Crate Index](./CRATE_INDEX.md)

**Related Files:**
- [Kernel Hotswap Architecture](./KERNEL_HOTSWAP_ARCHITECTURE.md)
- [Hot Swap Scenarios](./HOT_SWAP_SCENARIOS.md)
- [Runtime Diagrams](./RUNTIME-DIAGRAMS.md)
- [Diagram Reference](./DIAGRAM_REFERENCE.md)

---

### `/api/` - API Reference & Contracts
REST API documentation, contract maps, and specifications.

- [API Contract Map](./API_CONTRACT_MAP.md)
- [API Reference](./API.md)
- [LLM Interface Specification](./LLM-INTERFACE-SPECIFICATION.md)

---

### `/database/` - Database Schema & Migrations
Database schema documentation and migration guides.

- [Database Schema](./database-schema/) - Full schema documentation

---

### `/deployment/` - Deployment & Operations
Deployment guides, configuration, and production readiness.

- [Deployment Guide](./DEPLOYMENT-GUIDE.md)
- [Deployment](./DEPLOYMENT.md)
- [Production Readiness](./PRODUCTION_READINESS.md)
- [Production Operations](./PRODUCTION_OPERATIONS.md)
- [Disaster Recovery](./DISASTER_RECOVERY.md)

**Related Files:**
- [Config Precedence](./CONFIG_PRECEDENCE.md)
- [Server Config Phase 6 Implementation](./server-config-phase6-implementation.md)

---

### `/ui/` - User Interface & Frontend
UI integration, component hierarchy, and user flows.

- [UI Integration Verification](./UI_INTEGRATION_VERIFICATION.md)
- [UI Component Hierarchy](./UI-COMPONENT-HIERARCHY.md)
- [User Flow](./USER_FLOW.md)

---

### `/security/` - Security & Cryptography
Cryptography, secure enclave, and security protocols.

- [Crypto](./CRYPTO.md)
- [Secure Enclave Integration](./SECURE-ENCLAVE-INTEGRATION.md)
- [Signal Protocol Implementation](./SIGNAL-PROTOCOL-IMPLEMENTATION.md)
- [Keychain Integration](./keychain-integration.md)

---

### `/testing/` - Testing Coverage & Verification
Test plans, coverage analysis, and verification strategies.

- [Testing Coverage Gaps](./TESTING_COVERAGE_GAPS.md)
- [Testing Coverage Additions](./TESTING_COVERAGE_ADDITIONS.md)
- [Testing Model Loading](./TESTING_MODEL_LOADING.md)
- [Cypress Lifecycle Test Plan](./CYPRESS_LIFECYCLE_TEST_PLAN.md)
- [Playbook Verification](./PLAYBOOK_VERIFICATION.md)

---

### `/flows/` - User Flows & Workflows
End-to-end user flows and workflow documentation.

- [User Flow](./USER_FLOW.md)
- [Persona Demo User Guide](./PERSONA_DEMO_USER_GUIDE.md)
- [Personas](./PERSONAS.md)

---

### `/troubleshooting/` - Troubleshooting & Runbooks
Troubleshooting guides, runbooks, and common issues.

- [Troubleshooting](./TROUBLESHOOTING.md)
- [Operational Runbook](./OPERATIONAL_RUNBOOK.md)
- [Runaway Prevention](./RUNAWAY-PREVENTION.md)

---

## 🚀 Quick Start Guides

- **New Users**: [MVP Quickstart](./MVP_QUICKSTART.md) → [Quickstart](./QUICKSTART.md)
- **Getting Started with Diagrams**: [Diagram Guide](./GETTING_STARTED_WITH_DIAGRAMS.md)
- **CLI Users**: [CLI Guide](./CLI_GUIDE.md) → [CLI Reference](./CLI_REFERENCE.md)
- **Operators**: [Operational Runbooks](./OPERATIONAL_RUNBOOKS.md) → [Operator Playbooks](./OPERATOR_PLAYBOOKS.md)

---

## 📚 Reference Documentation

### Core Concepts
- [Glossary](./GLOSSARY.md) - Key terminology and concepts
- [Concepts](./CONCEPTS.md) - High-level system concepts
- [Version Guarantees](./VERSION_GUARANTEES.md) - Versioning and compatibility

### Development Guides
- [Documentation Maintenance](./DOCUMENTATION_MAINTENANCE.md)
- [Documentation Quality Audit](./DOCUMENTATION_QUALITY_AUDIT.md)
- [Duplication Prevention Guide](./DUPLICATION_PREVENTION_GUIDE.md)
- [Deprecated Patterns](./DEPRECATED_PATTERNS.md)

### Integration Guides
- [Cursor Integration Guide](./CURSOR_INTEGRATION_GUIDE.md)
- [Cursor Integration Implementation](./CURSOR_INTEGRATION_IMPLEMENTATION.md)
- [MLX Integration](./MLX_INTEGRATION.md)
- [Qwen Integration](./QWEN-INTEGRATION.md)
- [Model Loading Integration](./MODEL_LOADING_INTEGRATION.md)
- [Kernel Integration Progress](./KERNEL_INTEGRATION_PROGRESS.md)

### Specifications
- [MPLoRA End-to-End](./MPLORA-E2E.md)
- [RCU Spec](./RCU_SPEC.md)
- [Phase 2.4 Metal Shader Spec](./PHASE_2_4_METAL_SHADER_SPEC.md)
- [RAG pgvector](./RAG-PGVECTOR.md)
- [Patch Proposal System](./PATCH-PROPOSAL-SYSTEM.md)
- [Federation Daemon Implementation](./FEDERATION_DAEMON_IMPLEMENTATION.md)

---

## 🔧 Maintenance & Quality

### Audits & Rectifications
- [Corners Rectified](./CORNERS_RECTIFIED.md) - Known issues and fixes
- [Hallucination Audit 2025](./HALLUCINATION_AUDIT_2025.md)
- [Hallucination Audit Patent](./HALLUCINATION_AUDIT_PATENT.md)
- [Breaking Changes Alert](./BREAKING_CHANGES_ALERT.md)

### PRD Completion Tracking
- [PRD-02 Blockers](./PRD-02-BLOCKERS.md)
- [PRD-02 Completion Guide](./PRD-02-COMPLETION-GUIDE.md)
- [PRD 5.3.4 HotSwap CutCorners Fixes](./PRD_5_3_4_HotSwap_CutCorners_Fixes.md)
- [PRD 5.3.5 Hallucination Rectification](./PRD_5_3_5_Hallucination_Rectification.md)

### Repository Health
- [Repo Health System](./REPO_HEALTH_SYSTEM.md)
- [Repo Normalization Plan](./REPO_NORMALIZATION_PLAN.md)
- [Duplication Monitoring](./DUPLICATION_MONITORING.md)
- [Retry Policy Standardization](./RETRY_POLICY_STANDARDIZATION.md)

---

## 📖 Patent & Research Documentation

### Patents
- [Patent: MPLoRA Architecture](./PATENT_MPLORA_ARCHITECTURE.md)
- [Patent: MPLoRA Application](./PATENT_MPLORA_APPLICATION.md)
- [Patent: MPLoRA Novelty](./PATENT_MPLORA_NOVELTY.md)
- [Patent: Phrase Continuity](./PATENT_PHRASE_CONTINUITY.md)

---

## 📋 Document Map

For a complete list of all documentation files, see [Documentation Map](./DOCUMENTATION_MAP.md).

---

**Last Updated**: 2025-11-18
**Maintained By**: AdapterOS Team
**License**: Proprietary (© 2025 JKCA)
