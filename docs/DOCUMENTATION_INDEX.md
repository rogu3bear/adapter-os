# AdapterOS Documentation Index

**Last Updated**: 2025-01-27 (post-consolidation)
**Total Documentation Files**: ~126 active + 47 archived (down from 825)
**Maintained By**: AdapterOS Team

---

## Quick Links

| Category | Link | Description |
|----------|------|-------------|
| Developer Guide | [CLAUDE.md](../CLAUDE.md) | Primary developer reference |
| Project Overview | [README.md](../README.md) | Project introduction |
| Architecture Index | [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) | Architecture documentation hub |
| Quick Start | [QUICKSTART.md](../QUICKSTART.md) | End-to-end macOS setup |
| Root-Level Docs | [See below](#root-level-documentation-67-files) | Implementation reports and deliverables |

---

## Documentation Overview

| Location | File Count | Description |
|----------|------------|-------------|
| `/docs/` (active) | ~126 | Essential active documentation |
| `/docs/archive/minimal/` | 47 | Minimal historical archive |
| Root level (`/`) | ~70 | Project documentation (README, QUICKSTART, etc.) |
| `/crates/` | ~80 | Crate-specific documentation |
| `/tests/` | ~22 | Test documentation |
| `/ui/` | ~61 | UI component documentation |

**Note:** Archive directories (ai-generated, completed-phases, historical-reports) were removed during consolidation (2025-01-27). See [CONSOLIDATION_LOG.md](CONSOLIDATION_LOG.md) for details.

---

## By Category

### Architecture and Design (20+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Architecture Index** | [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) | Main architecture hub |
| **Architecture Patterns** | [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) | Detailed patterns and diagrams |
| **System Architecture** | [architecture.md](architecture.md) | High-level system design |
| **Precision Diagrams** | [architecture/PRECISION-DIAGRAMS.md](architecture/PRECISION-DIAGRAMS.md) | Code-verified diagrams |
| **MasterPlan** | [architecture/MASTERPLAN.md](architecture/MASTERPLAN.md) | Complete system design |
| **Multi-Backend Strategy** | [ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md) | Backend selection rationale |
| **AOS Format** | [AOS_FORMAT.md](AOS_FORMAT.md) | .aos archive specification |
| **Control Plane** | [CONTROL-PLANE.md](CONTROL-PLANE.md) | Control plane architecture |
| **Observability** | [architecture/OBSERVABILITY.md](architecture/OBSERVABILITY.md) | Observability architecture |
| **Production Deployment** | [architecture/PRODUCTION_DEPLOYMENT.md](architecture/PRODUCTION_DEPLOYMENT.md) | Production setup |
| **Production Gap Analysis** | [architecture/PRODUCTION_GAP_ANALYSIS.md](architecture/PRODUCTION_GAP_ANALYSIS.md) | Gap analysis |
| **Deterministic Validation** | [architecture/DETERMINISTIC_VALIDATION.md](architecture/DETERMINISTIC_VALIDATION.md) | Determinism verification |
| **Database Type Fix Plan** | [architecture/DATABASE_TYPE_FIX_PLAN.md](architecture/DATABASE_TYPE_FIX_PLAN.md) | Database type fixes |
| **RBAC Coverage** | [architecture/RBAC_COVERAGE.md](architecture/RBAC_COVERAGE.md) | RBAC analysis |
| **Replay Architecture** | [architecture/REPLAY.md](architecture/REPLAY.md) | Replay system design |
| **AOS Filetype Architecture** | [architecture/AOS_FILETYPE_ARCHITECTURE.md](architecture/AOS_FILETYPE_ARCHITECTURE.md) | AOS format details |

### Backend Integration (15+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **CoreML Integration** | [COREML_INTEGRATION.md](COREML_INTEGRATION.md) | CoreML setup, ANE optimization, and operational status |
| **MLX Integration** | [MLX_INTEGRATION.md](MLX_INTEGRATION.md) | MLX framework integration |
| **MLX Quick Reference** | [MLX_QUICK_REFERENCE.md](MLX_QUICK_REFERENCE.md) | MLX quick start |
| **MLX Deployment Guide** | [MLX_BACKEND_DEPLOYMENT_GUIDE.md](MLX_BACKEND_DEPLOYMENT_GUIDE.md) | MLX deployment steps |
| **MLX Router Integration** | [MLX_ROUTER_HOTSWAP_INTEGRATION.md](MLX_ROUTER_HOTSWAP_INTEGRATION.md) | Router hot-swap |
| **MLX Memory** | [MLX_MEMORY.md](MLX_MEMORY.md) | Memory management, usage, and quick reference |
| **MLX HKDF Seeding** | [MLX_HKDF_SEEDING.md](MLX_HKDF_SEEDING.md) | Deterministic seeding |
| **MLX Integration Checklist** | [MLX_INTEGRATION_CHECKLIST.md](MLX_INTEGRATION_CHECKLIST.md) | Integration checklist |
| **Adding New Backend** | [ADDING_NEW_BACKEND.md](ADDING_NEW_BACKEND.md) | Backend template |
| **Metal Hotswap Integration** | [METAL_HOTSWAP_INTEGRATION.md](METAL_HOTSWAP_INTEGRATION.md) | Metal hot-swap |
| **Metal Heap Observation** | [METAL_HEAP_OBSERVATION_FFI.md](METAL_HEAP_OBSERVATION_FFI.md) | Metal FFI |
| **Metal Kernels** | [metal/PHASE4-METAL-KERNELS.md](metal/PHASE4-METAL-KERNELS.md) | GPU kernels |

### Database and Schema (18+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Database Reference** | [DATABASE_REFERENCE.md](DATABASE_REFERENCE.md) | Schema reference |
| **Database Schema Overview** | [database-schema.md](database-schema.md) | Schema overview |
| **Schema Diagram** | [database-schema/SCHEMA-DIAGRAM.md](database-schema/SCHEMA-DIAGRAM.md) | Complete ERD |
| **Schema README** | [database-schema/README.md](database-schema/README.md) | Schema documentation |
| **Schema Maintenance** | [database-schema/MAINTENANCE.md](database-schema/MAINTENANCE.md) | Maintenance guide |
| **Schema Validation** | [database-schema/VALIDATION.md](database-schema/VALIDATION.md) | Validation procedures |
| **User Isolation** | [database/USER_ISOLATION.md](database/USER_ISOLATION.md) | Tenant isolation |
| **SQLx Offline Mode** | [database/SQLX_OFFLINE_MODE.md](database/SQLX_OFFLINE_MODE.md) | Offline compilation |
| **Migration Signing** | [db/MIGRATION_SIGNING_TODO.md](db/MIGRATION_SIGNING_TODO.md) | Migration signatures |
| **DB Schema** | [db/SCHEMA.md](db/SCHEMA.md) | Schema details |
| **Migration Operations** | [MIGRATION_OPERATIONS.md](MIGRATION_OPERATIONS.md) | Migration procedures |
| **Migration Verification** | [MIGRATION_VERIFICATION.md](MIGRATION_VERIFICATION.md) | Migration verification |

#### Database Workflows (9 docs)

| Document | Path | Description |
|----------|------|-------------|
| **Adapter Lifecycle** | [database-schema/workflows/ADAPTER-LIFECYCLE.md](database-schema/workflows/ADAPTER-LIFECYCLE.md) | Adapter states |
| **Promotion Pipeline** | [database-schema/workflows/PROMOTION-PIPELINE.md](database-schema/workflows/PROMOTION-PIPELINE.md) | CP promotion |
| **Monitoring Flow** | [database-schema/workflows/MONITORING-FLOW.md](database-schema/workflows/MONITORING-FLOW.md) | Metrics |
| **Security Compliance** | [database-schema/workflows/SECURITY-COMPLIANCE.md](database-schema/workflows/SECURITY-COMPLIANCE.md) | Security |
| **Incident Response** | [database-schema/workflows/INCIDENT-RESPONSE.md](database-schema/workflows/INCIDENT-RESPONSE.md) | Troubleshooting |
| **Code Intelligence** | [database-schema/workflows/CODE-INTELLIGENCE.md](database-schema/workflows/CODE-INTELLIGENCE.md) | Code analysis |
| **Git Repository** | [database-schema/workflows/GIT-REPOSITORY-WORKFLOW.md](database-schema/workflows/GIT-REPOSITORY-WORKFLOW.md) | Git integration |
| **Performance Dashboard** | [database-schema/workflows/PERFORMANCE-DASHBOARD.md](database-schema/workflows/PERFORMANCE-DASHBOARD.md) | Performance |
| **Replication** | [database-schema/workflows/REPLICATION-DISTRIBUTION.md](database-schema/workflows/REPLICATION-DISTRIBUTION.md) | Multi-node |

### Code Intelligence (20 docs)

| Document | Path | Description |
|----------|------|-------------|
| **Overview** | [code-intelligence/README.md](code-intelligence/README.md) | Code intelligence overview |
| **Architecture** | [code-intelligence/CODE-INTELLIGENCE-ARCHITECTURE.md](code-intelligence/CODE-INTELLIGENCE-ARCHITECTURE.md) | System design |
| **Tiers** | [code-intelligence/CODE-INTELLIGENCE-TIERS.md](code-intelligence/CODE-INTELLIGENCE-TIERS.md) | Feature tiers |
| **API Registry** | [code-intelligence/CODE-API-REGISTRY.md](code-intelligence/CODE-API-REGISTRY.md) | REST API |
| **API Ephemeral** | [code-intelligence/CODE-API-EPHEMERAL.md](code-intelligence/CODE-API-EPHEMERAL.md) | Ephemeral API |
| **API Security** | [code-intelligence/CODE-API-SECURITY.md](code-intelligence/CODE-API-SECURITY.md) | Security API |
| **CLI Commands** | [code-intelligence/CODE-CLI-COMMANDS.md](code-intelligence/CODE-CLI-COMMANDS.md) | CLI reference |
| **Crates** | [code-intelligence/CODE-CRATES.md](code-intelligence/CODE-CRATES.md) | Crate details |
| **Dependencies** | [code-intelligence/CODE-DEPENDENCIES.md](code-intelligence/CODE-DEPENDENCIES.md) | Dependencies |
| **Evaluation** | [code-intelligence/CODE-EVALUATION.md](code-intelligence/CODE-EVALUATION.md) | Metrics and testing |
| **Implementation Roadmap** | [code-intelligence/CODE-IMPLEMENTATION-ROADMAP.md](code-intelligence/CODE-IMPLEMENTATION-ROADMAP.md) | Roadmap |
| **Indices** | [code-intelligence/CODE-INDICES.md](code-intelligence/CODE-INDICES.md) | Index system |
| **Ingestion Pipeline** | [code-intelligence/CODE-INGESTION-PIPELINE.md](code-intelligence/CODE-INGESTION-PIPELINE.md) | Ingestion |
| **Manifest V4** | [code-intelligence/CODE-MANIFEST-V4.md](code-intelligence/CODE-MANIFEST-V4.md) | Manifest spec |
| **Policies** | [code-intelligence/CODE-POLICIES.md](code-intelligence/CODE-POLICIES.md) | Policy configuration |
| **Registry Schema** | [code-intelligence/CODE-REGISTRY-SCHEMA.md](code-intelligence/CODE-REGISTRY-SCHEMA.md) | Schema |
| **Router Features** | [code-intelligence/CODE-ROUTER-FEATURES.md](code-intelligence/CODE-ROUTER-FEATURES.md) | Routing |
| **UI Screens** | [code-intelligence/CODE-UI-SCREENS.md](code-intelligence/CODE-UI-SCREENS.md) | UI screens |
| **Git Integration Architecture** | [code-intelligence/GIT-INTEGRATION-ARCHITECTURE.md](code-intelligence/GIT-INTEGRATION-ARCHITECTURE.md) | Git architecture |
| **Git Integration Citations** | [code-intelligence/GIT-INTEGRATION-CITATIONS.md](code-intelligence/GIT-INTEGRATION-CITATIONS.md) | Citations |

### Training and Datasets (10+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Training Overview** | [Training.md](Training.md) | Training overview |
| **Training Metrics** | [TRAINING_METRICS.md](TRAINING_METRICS.md) | Metrics documentation |
| **Dataset Training Integration** | [DATASET_TRAINING_INTEGRATION.md](DATASET_TRAINING_INTEGRATION.md) | Dataset integration |
| **User Guide: Datasets** | [USER_GUIDE_DATASETS.md](USER_GUIDE_DATASETS.md) | Dataset user guide |
| **Codebase Ingestion** | [CODEBASE_INGESTION.md](CODEBASE_INGESTION.md) | Ingestion pipeline |
| **GPU Training Integration** | [GPU_TRAINING_INTEGRATION.md](GPU_TRAINING_INTEGRATION.md) | GPU training |

### CLI and Operational (14+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **CLI Guide** | [CLI_GUIDE.md](CLI_GUIDE.md) | CLI overview |
| **CLI Guide** | [CLI_GUIDE.md](CLI_GUIDE.md) | Complete CLI guide with command reference |
| **AOSCTL Manual** | [cli/AOSCTL.md](cli/AOSCTL.md) | aosctl commands |
| **AOS Command** | [cli/AOS.md](cli/AOS.md) | aos command |
| **AOS Launch** | [cli/AOS-LAUNCH.md](cli/AOS-LAUNCH.md) | Launch options |
| **CLI Overview** | [cli/OVERVIEW.md](cli/OVERVIEW.md) | CLI overview |
| **XTask** | [cli/XTASK.md](cli/XTASK.md) | xtask commands |

#### Runbooks (14 docs)

| Document | Path | Description |
|----------|------|-------------|
| **Runbook README** | [runbooks/README.md](runbooks/README.md) | Runbook index |
| **Alert Playbooks** | [runbooks/ALERT_PLAYBOOKS.md](runbooks/ALERT_PLAYBOOKS.md) | Alert responses |
| **Critical Components** | [runbooks/CRITICAL_COMPONENTS_RUNBOOK.md](runbooks/CRITICAL_COMPONENTS_RUNBOOK.md) | Component procedures |
| *Additional runbooks* | [runbooks/](runbooks/) | See directory |

### Lifecycle and Execution (10+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Lifecycle** | [LIFECYCLE.md](LIFECYCLE.md) | Adapter state machine |
| **Deterministic Execution** | [DETERMINISM_GUARANTEES.md](DETERMINISM_GUARANTEES.md) | HKDF, tick ledger |
| **Determinism Attestation** | [DETERMINISM-ATTESTATION.md](DETERMINISM-ATTESTATION.md) | Attestation |
| **Determinism Audit** | [DETERMINISM-AUDIT.md](DETERMINISM-AUDIT.md) | Audit procedures |
| **Hot Swap** | [HOT_SWAP.md](HOT_SWAP.md) | Hot-swap documentation |
| **Hot Swap Scenarios** | [HOT_SWAP_SCENARIOS.md](HOT_SWAP_SCENARIOS.md) | Scenarios |
| **Inference Flow** | [INFERENCE_FLOW.md](INFERENCE_FLOW.md) | Inference pipeline |
| **Inference Pipeline Integration** | [INFERENCE_PIPELINE_INTEGRATION.md](INFERENCE_PIPELINE_INTEGRATION.md) | Integration |

### Security and Policy (15+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Security** | [SECURITY.md](SECURITY.md) | Security overview |
| **RBAC** | [RBAC.md](RBAC.md) | Role-based access control |
| **Policy Enforcement** | [POLICY_ENFORCEMENT.md](POLICY_ENFORCEMENT.md) | Policy engine |
| **AOS Runtime Policy** | [AOS_RUNTIME_POLICY.md](AOS_RUNTIME_POLICY.md) | Runtime policies |
| **Authentication** | [AUTHENTICATION.md](AUTHENTICATION.md) | Auth system |
| **Authentication** | [AUTHENTICATION.md](AUTHENTICATION.md) | Authentication architecture and performance |
| **Crypto** | [CRYPTO.md](CRYPTO.md) | Cryptography |
| **Keychain Integration** | [KEYCHAIN-INTEGRATION.md](KEYCHAIN-INTEGRATION.md) | macOS Keychain |
| **Azure KeyVault** | [AZURE_KEYVAULT_INTEGRATION.md](AZURE_KEYVAULT_INTEGRATION.md) | Azure KMS |
| **Enclave Fallback** | [ENCLAVE_FALLBACK.md](ENCLAVE_FALLBACK.md) | Secure enclave |

### API and Integration (15+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **API Overview** | [api.md](api.md) | API documentation |
| **Telemetry Endpoints** | [api/TELEMETRY_ENDPOINTS.md](api/TELEMETRY_ENDPOINTS.md) | Telemetry API |
| **UI Integration** | [UI_INTEGRATION.md](UI_INTEGRATION.md) | Web UI integration |
| **UI Component Hierarchy** | [UI-COMPONENT-HIERARCHY.md](UI-COMPONENT-HIERARCHY.md) | React components |
| **Telemetry Events** | [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) | Event catalog |
| **System Metrics** | [SYSTEM-METRICS.md](SYSTEM-METRICS.md) | System metrics |
| **Domain Adapter Layer** | [DOMAIN-ADAPTER-LAYER.md](DOMAIN-ADAPTER-LAYER.md) | Domain adapters |
| **Patch Proposal System** | [PATCH-PROPOSAL-SYSTEM.md](PATCH-PROPOSAL-SYSTEM.md) | Patch proposals |
| **Golden Runs Spec** | [GOLDEN-RUNS-SPEC.md](GOLDEN-RUNS-SPEC.md) | Golden runs |

### Flows and Diagrams (7 docs)

| Document | Path | Description |
|----------|------|-------------|
| **Flows Overview** | [flows/README.md](flows/README.md) | Flow diagrams |
| **Diagrams** | [flows/DIAGRAMS.md](flows/DIAGRAMS.md) | Diagram reference |
| **Load Flow** | [flows/LOAD.md](flows/LOAD.md) | Load operations |
| **Record Flow** | [flows/RECORD.md](flows/RECORD.md) | Recording |
| **Replay Flow** | [flows/REPLAY.md](flows/REPLAY.md) | Replay |
| **Route Flow** | [flows/route.md](flows/route.md) | Routing |
| **Run Flow** | [flows/RUN.md](flows/RUN.md) | Execution |

### Adapter Documentation (10+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Adapter Taxonomy** | [ADAPTER_TAXONOMY.md](ADAPTER_TAXONOMY.md) | Adapter categories |
| **Adapter Lineage** | [ADAPTER_LINEAGE_AND_SAFE_MODE.md](ADAPTER_LINEAGE_AND_SAFE_MODE.md) | Lineage and safe mode |
| **Adapter Record Index** | [ADAPTER_RECORD_INDEX.md](ADAPTER_RECORD_INDEX.md) | Record index |
| **Adapter Record Examples** | [ADAPTER_RECORD_EXAMPLES.md](ADAPTER_RECORD_EXAMPLES.md) | Examples |
| **Adapter Record Integration** | [ADAPTER_RECORD_INTEGRATION_GUIDE.md](ADAPTER_RECORD_INTEGRATION_GUIDE.md) | Integration guide |
| **Adapter Record Refactoring** | [ADAPTER_RECORD_REFACTORING.md](ADAPTER_RECORD_REFACTORING.md) | Refactoring |

### Miscellaneous (20+ docs)

| Document | Path | Description |
|----------|------|-------------|
| **Concepts** | [CONCEPTS.md](CONCEPTS.md) | Mental model and glossary |
| **Glossary** | [GLOSSARY.md](GLOSSARY.md) | Terms |
| **Config Precedence** | [CONFIG_PRECEDENCE.md](CONFIG_PRECEDENCE.md) | Configuration |
| **Deployment Guide** | [DEPLOYMENT-GUIDE.md](DEPLOYMENT-GUIDE.md) | Deployment |
| **Deployment** | [DEPLOYMENT.md](DEPLOYMENT.md) | Deployment overview |
| **Containerization** | [CONTAINERIZATION_DEPLOYMENT.md](CONTAINERIZATION_DEPLOYMENT.md) | Docker/containers |
| **Development Workflow** | [DEVELOPMENT_WORKFLOW.md](DEVELOPMENT_WORKFLOW.md) | Dev workflow |
| **Dependency Consolidation** | [DEPENDENCY_CONSOLIDATION.md](DEPENDENCY_CONSOLIDATION.md) | Dependencies |
| **Deprecated Patterns** | [DEPRECATED_PATTERNS.md](DEPRECATED_PATTERNS.md) | Anti-patterns |
| **Disaster Recovery** | [DISASTER_RECOVERY.md](DISASTER_RECOVERY.md) | DR procedures |
| **Feature Flags** | [FEATURE_FLAGS.md](FEATURE_FLAGS.md) | Feature flags |
| **Evidence Retrieval** | [EVIDENCE_RETRIEVAL.md](EVIDENCE_RETRIEVAL.md) | Evidence system |
| **LLM Interface Spec** | [LLM-INTERFACE-SPECIFICATION.md](LLM-INTERFACE-SPECIFICATION.md) | LLM interface |
| **MPLORA E2E** | [MPLORA-E2E.md](MPLORA-E2E.md) | End-to-end |
| **Policy Engine Outline** | [POLICY-ENGINE-OUTLINE.md](POLICY-ENGINE-OUTLINE.md) | Policy engine |
| **Qwen Integration** | [QWEN-INTEGRATION.md](QWEN-INTEGRATION.md) | Qwen model |
| **RCU Spec** | [RCU_SPEC.md](RCU_SPEC.md) | RCU specification |
| **Runaway Prevention** | [RUNAWAY-PREVENTION.md](RUNAWAY-PREVENTION.md) | Safety mechanisms |
| **CodeGraph Spec** | [CODEGRAPH-SPEC.md](CODEGRAPH-SPEC.md) | Code graph |
| **Signal Protocol** | [SIGNAL-PROTOCOL-IMPLEMENTATION.md](SIGNAL-PROTOCOL-IMPLEMENTATION.md) | Signal protocol |
| **Stub Implementations** | [STUB_IMPLEMENTATIONS.md](STUB_IMPLEMENTATIONS.md) | Stubs |
| **Style Guide** | [STYLE_GUIDE.md](STYLE_GUIDE.md) | Code style |
| **Test Index** | [TEST_INDEX.md](TEST_INDEX.md) | Test documentation |
| **Troubleshooting** | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | Troubleshooting |
| **User Flow** | [USER_FLOW.md](USER_FLOW.md) | User flows |
| **Version Guarantees** | [VERSION_GUARANTEES.md](VERSION_GUARANTEES.md) | Version guarantees |

---

## Crate Documentation (80 files)

### adapteros-lora-mlx-ffi (21 docs)

MLX FFI integration crate with comprehensive documentation.

| Document | Path | Description |
|----------|------|-------------|
| README | [crates/adapteros-lora-mlx-ffi/README.md](../crates/adapteros-lora-mlx-ffi/README.md) | Overview |
| Developer Guide | [crates/adapteros-lora-mlx-ffi/DEVELOPER_GUIDE.md](../crates/adapteros-lora-mlx-ffi/DEVELOPER_GUIDE.md) | Development |
| Benchmarks | [crates/adapteros-lora-mlx-ffi/BENCHMARKS.md](../crates/adapteros-lora-mlx-ffi/BENCHMARKS.md) | Performance |
| Benchmarking README | [crates/adapteros-lora-mlx-ffi/BENCHMARKING_README.md](../crates/adapteros-lora-mlx-ffi/BENCHMARKING_README.md) | Benchmark guide |
| Memory Management | [crates/adapteros-lora-mlx-ffi/MEMORY_MANAGEMENT.md](../crates/adapteros-lora-mlx-ffi/MEMORY_MANAGEMENT.md) | Memory |
| Memory Quick Reference | [crates/adapteros-lora-mlx-ffi/MEMORY_QUICK_REFERENCE.md](../crates/adapteros-lora-mlx-ffi/MEMORY_QUICK_REFERENCE.md) | Quick ref |
| Memory Tracking | [crates/adapteros-lora-mlx-ffi/README_MEMORY_TRACKING.md](../crates/adapteros-lora-mlx-ffi/README_MEMORY_TRACKING.md) | Tracking |
| Multi-Adapter Routing | [crates/adapteros-lora-mlx-ffi/MULTI_ADAPTER_ROUTING.md](../crates/adapteros-lora-mlx-ffi/MULTI_ADAPTER_ROUTING.md) | Routing |
| Performance Guide | [crates/adapteros-lora-mlx-ffi/PERFORMANCE_GUIDE.md](../crates/adapteros-lora-mlx-ffi/PERFORMANCE_GUIDE.md) | Performance |
| Quantization Guide | [crates/adapteros-lora-mlx-ffi/QUANTIZATION_GUIDE.md](../crates/adapteros-lora-mlx-ffi/QUANTIZATION_GUIDE.md) | Quantization |
| FFI Integration Proof | [crates/adapteros-lora-mlx-ffi/MLX_FFI_INTEGRATION_PROOF.md](../crates/adapteros-lora-mlx-ffi/MLX_FFI_INTEGRATION_PROOF.md) | Proof |
| API Reference | [crates/adapteros-lora-mlx-ffi/docs/API_REFERENCE.md](../crates/adapteros-lora-mlx-ffi/docs/API_REFERENCE.md) | API |
| Troubleshooting | [crates/adapteros-lora-mlx-ffi/docs/TROUBLESHOOTING.md](../crates/adapteros-lora-mlx-ffi/docs/TROUBLESHOOTING.md) | Troubleshooting |
| Test Index | [crates/adapteros-lora-mlx-ffi/tests/INDEX.md](../crates/adapteros-lora-mlx-ffi/tests/INDEX.md) | Test index |
| Test README | [crates/adapteros-lora-mlx-ffi/tests/README.md](../crates/adapteros-lora-mlx-ffi/tests/README.md) | Test overview |
| Quick Reference | [crates/adapteros-lora-mlx-ffi/tests/QUICK_REFERENCE.md](../crates/adapteros-lora-mlx-ffi/tests/QUICK_REFERENCE.md) | Quick ref |
| Test Execution Guide | [crates/adapteros-lora-mlx-ffi/tests/TEST_EXECUTION_GUIDE.md](../crates/adapteros-lora-mlx-ffi/tests/TEST_EXECUTION_GUIDE.md) | Execution |
| KV Cache Verification | [crates/adapteros-lora-mlx-ffi/tests/KV_CACHE_ATTENTION_VERIFICATION.md](../crates/adapteros-lora-mlx-ffi/tests/KV_CACHE_ATTENTION_VERIFICATION.md) | KV cache |
| Verification Summary | [crates/adapteros-lora-mlx-ffi/tests/VERIFICATION_SUMMARY.md](../crates/adapteros-lora-mlx-ffi/tests/VERIFICATION_SUMMARY.md) | Summary |

### adapteros-memory (20 docs)

Memory management crate documentation.

| Document | Path | Description |
|----------|------|-------------|
| Examples | [crates/adapteros-memory/EXAMPLES.md](../crates/adapteros-memory/EXAMPLES.md) | Usage examples |
| Quick Reference | [crates/adapteros-memory/QUICK_REFERENCE.md](../crates/adapteros-memory/QUICK_REFERENCE.md) | Quick ref |
| Integration Guide | [crates/adapteros-memory/INTEGRATION_GUIDE.md](../crates/adapteros-memory/INTEGRATION_GUIDE.md) | Integration |
| Heap Observer | [crates/adapteros-memory/README_HEAP_OBSERVER.md](../crates/adapteros-memory/README_HEAP_OBSERVER.md) | Heap observer |
| Heap Observer Impl | [crates/adapteros-memory/HEAP_OBSERVER_IMPL.md](../crates/adapteros-memory/HEAP_OBSERVER_IMPL.md) | Implementation |
| Heap Observer Integration | [crates/adapteros-memory/HEAP_OBSERVER_INTEGRATION.md](../crates/adapteros-memory/HEAP_OBSERVER_INTEGRATION.md) | Integration |
| K-Reduction Index | [crates/adapteros-memory/K_REDUCTION_INDEX.md](../crates/adapteros-memory/K_REDUCTION_INDEX.md) | K-reduction |
| K-Reduction Guide | [crates/adapteros-memory/K_REDUCTION_INTEGRATION_GUIDE.md](../crates/adapteros-memory/K_REDUCTION_INTEGRATION_GUIDE.md) | Guide |
| K-Reduction Quick Ref | [crates/adapteros-memory/K_REDUCTION_QUICK_REFERENCE.md](../crates/adapteros-memory/K_REDUCTION_QUICK_REFERENCE.md) | Quick ref |
| Metal Build Integration | [crates/adapteros-memory/METAL_BUILD_INTEGRATION.md](../crates/adapteros-memory/METAL_BUILD_INTEGRATION.md) | Metal |
| Hardware Testing | [crates/adapteros-memory/HARDWARE_TESTING_GUIDE.md](../crates/adapteros-memory/HARDWARE_TESTING_GUIDE.md) | Hardware |
| Unified Memory | [crates/adapteros-memory/UNIFIED_MEMORY_MANAGEMENT.md](../crates/adapteros-memory/UNIFIED_MEMORY_MANAGEMENT.md) | UMA |
| Implementation Checklist | [crates/adapteros-memory/IMPLEMENTATION_CHECKLIST.md](../crates/adapteros-memory/IMPLEMENTATION_CHECKLIST.md) | Checklist |
| Files Delivered | [crates/adapteros-memory/FILES_DELIVERED.md](../crates/adapteros-memory/FILES_DELIVERED.md) | Deliverables |

### Other Crates

| Crate | Doc Count | Key Documents |
|-------|-----------|---------------|
| adapteros-server-api | 7 | [README](../crates/adapteros-server-api/README.md), [Rate Limiting](../crates/adapteros-server-api/RATE_LIMITING.md), [Telemetry](../crates/adapteros-server-api/TELEMETRY_INTEGRATION.md) |
| adapteros-tui | 6 | [README](../crates/adapteros-tui/README.md), [docs/](../crates/adapteros-tui/docs/) |
| adapteros-lora-lifecycle | 6 | [GPU Verification](../crates/adapteros-lora-lifecycle/GPU_VERIFICATION_INTEGRATION.md), [Kernel Backend](../crates/adapteros-lora-lifecycle/KERNEL_BACKEND_USAGE.md) |
| adapteros-policy | 5 | [docs/VERSION_MATCHER.md](../crates/adapteros-policy/docs/VERSION_MATCHER.md), [docs/OFFLINE_CVE_MODE.md](../crates/adapteros-policy/docs/OFFLINE_CVE_MODE.md) |
| adapteros-federation | 3 | [README](../crates/adapteros-federation/README.md), [PEER_DISCOVERY.md](../crates/adapteros-federation/PEER_DISCOVERY.md), [PEER_REGISTRY_API.md](../crates/adapteros-federation/PEER_REGISTRY_API.md) |
| adapteros-lora-kernel-mtl | 2 | [COREML_FFI.md](../crates/adapteros-lora-kernel-mtl/COREML_FFI.md), [COREML_FFI_INTEGRATION.md](../crates/adapteros-lora-kernel-mtl/COREML_FFI_INTEGRATION.md) |
| adapteros-crypto | 2 | [KMS_TESTING_GUIDE.md](../crates/adapteros-crypto/KMS_TESTING_GUIDE.md), [KMS_SECURITY_TESTS_SUMMARY.md](../crates/adapteros-crypto/KMS_SECURITY_TESTS_SUMMARY.md) |
| adapteros-aos | 1 | [README](../crates/adapteros-aos/README.md) |
| adapteros-cli | 1 | [docs/aosctl_manual.md](../crates/adapteros-cli/docs/aosctl_manual.md) |
| adapteros-db | 1 | [migrations/DEPRECATED.md](../crates/adapteros-db/migrations/DEPRECATED.md) |
| adapteros-lora-rag | 1 | [README](../crates/adapteros-lora-rag/README.md) |
| adapteros-orchestrator | 1 | [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md) |
| adapteros-telemetry | 1 | [README](../crates/adapteros-telemetry/README.md) |
| adapteros-verify | 1 | [templates/README](../crates/adapteros-verify/templates/README.md) |

---

## Root-Level Documentation (67 files)

Implementation reports, deliverables, and quick-start guides at project root.

### Quick Start Guides

| Document | Path | Description |
|----------|------|-------------|
| **Quick Start** | [../QUICKSTART.md](../QUICKSTART.md) | End-to-end macOS setup |
| **GPU Training Quick Start** | [../QUICKSTART_GPU_TRAINING.md](../QUICKSTART_GPU_TRAINING.md) | GPU training guide |
| **K-Reduction Quick Start** | [../K_REDUCTION_QUICK_START.md](../K_REDUCTION_QUICK_START.md) | K-reduction guide |
| **Token Sampling Quick Ref** | [../TOKEN_SAMPLING_QUICK_REFERENCE.md](../TOKEN_SAMPLING_QUICK_REFERENCE.md) | Token sampling |
| **Training Metrics Quick Start** | [../TRAINING_METRICS_QUICK_START.md](../TRAINING_METRICS_QUICK_START.md) | Training metrics |
| **Azure KMS Quick Ref** | [../AZURE_KMS_QUICK_REFERENCE.md](../AZURE_KMS_QUICK_REFERENCE.md) | Azure KMS |

### Implementation Summaries

| Document | Path | Description |
|----------|------|-------------|
| Activation Functions | [../ACTIVATION_FUNCTIONS_IMPLEMENTATION.md](../ACTIVATION_FUNCTIONS_IMPLEMENTATION.md) | Activation impl |
| Azure KMS | [../AZURE_KMS_IMPLEMENTATION.md](../AZURE_KMS_IMPLEMENTATION.md) | Azure KMS impl |
| CVSS/EPSS | [../CVSS_EPSS_IMPLEMENTATION_SUMMARY.md](../CVSS_EPSS_IMPLEMENTATION_SUMMARY.md) | CVE scoring |
| Enclave Fallback | [../ENCLAVE_FALLBACK_CHANGES.md](../ENCLAVE_FALLBACK_CHANGES.md) | Enclave changes |
| Federation | [../FEDERATION_IMPLEMENTATION_SUMMARY.md](../FEDERATION_IMPLEMENTATION_SUMMARY.md) | Federation |
| Federation Tick Ledger | [../FEDERATION_TICK_LEDGER_IMPLEMENTATION.md](../FEDERATION_TICK_LEDGER_IMPLEMENTATION.md) | Tick ledger |
| FFI Wrapper | [../FFI_WRAPPER_IMPLEMENTATION_SUMMARY.md](../FFI_WRAPPER_IMPLEMENTATION_SUMMARY.md) | FFI wrapper |
| GPU Training | [../GPU_TRAINING_COMPLETION_SUMMARY.md](../GPU_TRAINING_COMPLETION_SUMMARY.md) | GPU training |
| Heap Observer | [../HEAP_OBSERVER_DELIVERABLES.md](../HEAP_OBSERVER_DELIVERABLES.md) | Heap observer |
| K-Reduction | [../K_REDUCTION_IMPLEMENTATION.md](../K_REDUCTION_IMPLEMENTATION.md) | K-reduction |
| Login | [../LOGIN_CHANGES_DETAILED.md](../LOGIN_CHANGES_DETAILED.md), [../LOGIN_UPDATE_SUMMARY.md](../LOGIN_UPDATE_SUMMARY.md) | Login changes |
| Memory Pool | [../MEMORY_POOL_IMPLEMENTATION.md](../MEMORY_POOL_IMPLEMENTATION.md) | Memory pool |
| Metal Build | [../METAL_BUILD_SYSTEM_INTEGRATION.md](../METAL_BUILD_SYSTEM_INTEGRATION.md) | Metal build |
| MLX | [../MLX_FFI_PERFORMANCE_OPTIMIZATION_SUMMARY.md](../MLX_FFI_PERFORMANCE_OPTIMIZATION_SUMMARY.md), [../MLX_MEMORY_POOL_SUMMARY.md](../MLX_MEMORY_POOL_SUMMARY.md), [../MLX_REAL_INTEGRATION_SUMMARY.md](../MLX_REAL_INTEGRATION_SUMMARY.md) | MLX |
| Quantization | [../QUANTIZATION_COMPRESSION_IMPLEMENTATION.md](../QUANTIZATION_COMPRESSION_IMPLEMENTATION.md) | Quantization |
| Router Migration | [../ROUTER_MIGRATION_SUMMARY.md](../ROUTER_MIGRATION_SUMMARY.md) | Router |
| Security | [../SECURITY_IMPLEMENTATION_SUMMARY.md](../SECURITY_IMPLEMENTATION_SUMMARY.md) | Security |
| Token Sampling | [../TOKEN_SAMPLING_IMPLEMENTATION_SUMMARY.md](../TOKEN_SAMPLING_IMPLEMENTATION_SUMMARY.md), [../MLX_TOKEN_SAMPLING_IMPLEMENTATION.md](../MLX_TOKEN_SAMPLING_IMPLEMENTATION.md) | Sampling |
| Training Metrics | [../TRAINING_METRICS_IMPLEMENTATION.md](../TRAINING_METRICS_IMPLEMENTATION.md) | Training |

### Verification and Reports

| Document | Path | Description |
|----------|------|-------------|
| Benchmark Results | [../BENCHMARK_RESULTS.md](../BENCHMARK_RESULTS.md) | Performance results |
| Benchmark Guide | [../BENCHMARK_GUIDE.md](../BENCHMARK_GUIDE.md) | Benchmarking |
| CoreML Verification | [../COREML_INTEGRATION_VERIFICATION.md](../COREML_INTEGRATION_VERIFICATION.md), [../COREML_DETERMINISM_VERIFICATION.md](../COREML_DETERMINISM_VERIFICATION.md) | CoreML |
| GCP KMS Verification | [../GCP_KMS_VERIFICATION_REPORT.md](../GCP_KMS_VERIFICATION_REPORT.md) | GCP KMS |
| Login Verification | [../LOGIN_FINAL_VERIFICATION.md](../LOGIN_FINAL_VERIFICATION.md) | Login |
| Deadlock Analysis | [../DEADLOCK_ANALYSIS.md](../DEADLOCK_ANALYSIS.md) | Deadlock |
| Audit | [../AUDIT_UNFINISHED_FEATURES.md](../AUDIT_UNFINISHED_FEATURES.md) | Unfinished features |

### Index Documents

| Document | Path | Description |
|----------|------|-------------|
| MLX Testing Index | [../MLX_TESTING_INDEX.md](../MLX_TESTING_INDEX.md) | MLX tests |
| Training Metrics Index | [../TRAINING_METRICS_INDEX.md](../TRAINING_METRICS_INDEX.md) | Training metrics |

### Project Documents

| Document | Path | Description |
|----------|------|-------------|
| README | [../README.md](../README.md) | Project overview |
| CLAUDE.md | [../CLAUDE.md](../CLAUDE.md) | Developer guide |
| CONTRIBUTING | [../CONTRIBUTING.md](../CONTRIBUTING.md) | Contribution guide |
| CHANGELOG | [../CHANGELOG.md](../CHANGELOG.md) | Change log |
| DEPRECATIONS | [../DEPRECATIONS.md](../DEPRECATIONS.md) | Deprecations |
| PRD | [../PRD.md](../PRD.md) | Product requirements |
| Deliverables Manifest | [../DELIVERABLES_MANIFEST.md](../DELIVERABLES_MANIFEST.md) | Deliverables |

---

## Test Documentation (22 files)

| Document | Path | Description |
|----------|------|-------------|
| **Test README** | [../tests/README.md](../tests/README.md) | Test overview |
| **Quick Start** | [../tests/QUICK_START.md](../tests/QUICK_START.md) | Test quick start |
| **Integration Tests** | [../tests/INTEGRATION_TESTS.md](../tests/INTEGRATION_TESTS.md) | Integration tests |
| **Type Validation Suite** | [../tests/TYPE_VALIDATION_SUITE.md](../tests/TYPE_VALIDATION_SUITE.md) | Type validation |
| **Type Validation README** | [../tests/README_TYPE_VALIDATION.md](../tests/README_TYPE_VALIDATION.md) | Type validation |
| **Error Recovery** | [../tests/ERROR_RECOVERY_TESTS.md](../tests/ERROR_RECOVERY_TESTS.md), [../tests/ERROR_RECOVERY_SUMMARY.md](../tests/ERROR_RECOVERY_SUMMARY.md) | Error recovery |
| **E2E README** | [../tests/e2e/README.md](../tests/e2e/README.md) | E2E tests |
| **E2E Dataset Tests** | [../tests/e2e/README_DATASET_TESTS.md](../tests/e2e/README_DATASET_TESTS.md), [../tests/e2e/DATASET_TO_INFERENCE_TESTS.md](../tests/e2e/DATASET_TO_INFERENCE_TESTS.md) | Dataset E2E |
| **Training Datasets** | [../tests/training/datasets/README.md](../tests/training/datasets/README.md) | Training datasets |
| **Benchmark README** | [../tests/benchmark/README.md](../tests/benchmark/README.md) | Benchmarks |
| **Integration README** | [../tests/integration/README.md](../tests/integration/README.md) | Integration |
| **Unit README** | [../tests/unit/README.md](../tests/unit/README.md) | Unit tests |

---

## UI Documentation (61 files)

### Core UI Docs

| Document | Path | Description |
|----------|------|-------------|
| **README** | [../ui/README.md](../ui/README.md) | UI overview |
| **Quick Start** | [../ui/QUICK_START.md](../ui/QUICK_START.md) | UI quick start |
| **Feature Overview** | [../ui/FEATURE_OVERVIEW.md](../ui/FEATURE_OVERVIEW.md) | Features |
| **Testing** | [../ui/TESTING.md](../ui/TESTING.md) | UI testing |
| **Troubleshooting** | [../ui/TROUBLESHOOTING.md](../ui/TROUBLESHOOTING.md) | Troubleshooting |
| **Changes** | [../ui/CHANGES.md](../ui/CHANGES.md) | UI changes |

### RBAC and Security

| Document | Path | Description |
|----------|------|-------------|
| RBAC Index | [../ui/RBAC_INDEX.md](../ui/RBAC_INDEX.md) | RBAC index |
| RBAC Quick Start | [../ui/RBAC_QUICK_START.md](../ui/RBAC_QUICK_START.md) | Quick start |
| RBAC Guide | [../ui/RBAC_IMPLEMENTATION_GUIDE.md](../ui/RBAC_IMPLEMENTATION_GUIDE.md) | Guide |
| RBAC Checklist | [../ui/RBAC_CHECKLIST.md](../ui/RBAC_CHECKLIST.md) | Checklist |

### Streaming

| Document | Path | Description |
|----------|------|-------------|
| Streaming README | [../ui/STREAMING_README.md](../ui/STREAMING_README.md) | Overview |
| Streaming Guide | [../ui/STREAMING_GUIDE.md](../ui/STREAMING_GUIDE.md) | Guide |
| Streaming Quick Start | [../ui/STREAMING_QUICKSTART.md](../ui/STREAMING_QUICKSTART.md) | Quick start |
| Streaming Implementation | [../ui/STREAMING_IMPLEMENTATION_SUMMARY.md](../ui/STREAMING_IMPLEMENTATION_SUMMARY.md) | Implementation |

### Component Documentation

| Directory | Doc Count | Description |
|-----------|-----------|-------------|
| [../ui/src/components/adapters/](../ui/src/components/adapters/) | 3 | Adapter components |
| [../ui/src/components/golden/](../ui/src/components/golden/) | 6 | Golden run components |
| [../ui/src/components/training/](../ui/src/components/training/) | 3 | Training components |
| [../ui/src/components/workflows/](../ui/src/components/workflows/) | 1 | Workflow components |
| [../ui/src/schemas/](../ui/src/schemas/) | 4 | Schema documentation |
| [../ui/src/utils/](../ui/src/utils/) | 2 | Utility documentation |
| [../ui/docs/](../ui/docs/) | 2 | Design system docs |

---

## Archive Documentation (292 files)

**Warning**: Archive contains historical documents that may be outdated. See [archive/README.md](archive/README.md) for details.

| Directory | Count | Description |
|-----------|-------|-------------|
| [archive/ai-generated/](archive/ai-generated/) | 194 | AI-generated reports (2025-11-21) |
| [archive/historical-reports/](archive/historical-reports/) | 57 | Historical reports |
| [archive/completed-phases/](archive/completed-phases/) | 20 | Phase completion docs |
| [archive/ui-patch-docs/](archive/ui-patch-docs/) | 8 | UI patch documentation |
| [archive/integration-2025-10/](archive/integration-2025-10/) | 6 | Integration docs |
| [archive/phase2-integration-2025-10/](archive/phase2-integration-2025-10/) | 3 | Phase 2 integration |
| [archive/deprecated/](archive/deprecated/) | 1 | Deprecated docs |
| [archive/temp/](archive/temp/) | 1 | Temporary files |

### Archive Staleness Warning

Documents in the archive may contain:
- Outdated completion claims
- Superseded implementation details
- Historical context no longer relevant

Always verify against current codebase and active documentation.

---

## Documentation by Audience

### For New Developers

1. [CONCEPTS.md](CONCEPTS.md) - Mental model and glossary
2. [../QUICKSTART.md](../QUICKSTART.md) - Getting started
3. [../CLAUDE.md](../CLAUDE.md) - Developer guide
4. [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Architecture overview
5. [architecture/PRECISION-DIAGRAMS.md](architecture/PRECISION-DIAGRAMS.md) - System diagrams

### For Operators

1. [CONTROL-PLANE.md](CONTROL-PLANE.md) - Control plane
2. [runbooks/README.md](runbooks/README.md) - Runbooks
3. [DEPLOYMENT-GUIDE.md](DEPLOYMENT-GUIDE.md) - Deployment
4. [database-schema/workflows/MONITORING-FLOW.md](database-schema/workflows/MONITORING-FLOW.md) - Monitoring
5. [database-schema/workflows/INCIDENT-RESPONSE.md](database-schema/workflows/INCIDENT-RESPONSE.md) - Incident response

### For Security Auditors

1. [SECURITY.md](SECURITY.md) - Security overview
2. [RBAC.md](RBAC.md) - Access control
3. [POLICY_ENFORCEMENT.md](POLICY_ENFORCEMENT.md) - Policies
4. [database-schema/workflows/SECURITY-COMPLIANCE.md](database-schema/workflows/SECURITY-COMPLIANCE.md) - Compliance
5. [CRYPTO.md](CRYPTO.md) - Cryptography

### For Researchers

1. [MLX_INTEGRATION.md](MLX_INTEGRATION.md) - MLX integration
2. [DETERMINISM_GUARANTEES.md](DETERMINISM_GUARANTEES.md) - Determinism
3. [QWEN-INTEGRATION.md](QWEN-INTEGRATION.md) - Qwen model
4. [metal/PHASE4-METAL-KERNELS.md](metal/PHASE4-METAL-KERNELS.md) - Metal kernels
5. [COREML_INTEGRATION.md](COREML_INTEGRATION.md) - CoreML

---

## Documentation Maintenance

### Update Schedule

- **With code changes**: Update relevant documentation
- **Monthly**: Review and update indexes
- **Quarterly**: Full documentation audit

### Contributing

See [../CONTRIBUTING.md](../CONTRIBUTING.md) for contribution guidelines.

When adding documentation:
1. Place in appropriate directory
2. Update relevant index files
3. Add links from related documents
4. Follow existing format and structure

### Archive Policy

- Move completed phase docs to `archive/completed-phases/`
- Move deprecated docs to `archive/deprecated/`
- AI-generated temporary docs go to `archive/ai-generated/`
- Maintain archive README with warnings

---

**Last Updated**: 2025-11-22
**Total Files**: 878
**Active Docs**: 586
**Archived Docs**: 292
**Crate Docs**: 80
**Test Docs**: 22
**UI Docs**: 61
