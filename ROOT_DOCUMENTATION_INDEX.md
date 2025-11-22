# Root-Level Documentation Index

**Location**: Repository root (`/Users/star/Dev/aos/`)
**Count**: 62 orphaned files (67 total - 5 referenced in CLAUDE.md)
**Status**: Deliverables, implementation reports, delivery documentation
**Maintenance**: Updated 2025-11-22

> **Note**: These files document completed work and delivery milestones. They are organizational context but not required reading. See [CLAUDE.md](CLAUDE.md) for the primary developer guide.

---

## Referenced Files (in CLAUDE.md)

These files are already documented in CLAUDE.md and are NOT orphaned:

- [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) - MLX FFI benchmark results
- [CONTRIBUTING.md](CONTRIBUTING.md) - PR guidelines
- [QUICKSTART.md](QUICKSTART.md) - Quick start guide
- [QUICKSTART_GPU_TRAINING.md](QUICKSTART_GPU_TRAINING.md) - GPU training quick start
- [README.md](README.md) - Project overview

---

## Implementation Reports (28 files)

Detailed implementation documentation for completed features.

### Azure/Cloud KMS (3 files)
- [AZURE_KMS_IMPLEMENTATION.md](AZURE_KMS_IMPLEMENTATION.md) - Azure Key Vault integration implementation
- [AZURE_KMS_QUICK_REFERENCE.md](AZURE_KMS_QUICK_REFERENCE.md) - Quick reference for Azure KMS usage
- [GCP_KMS_VERIFICATION_REPORT.md](GCP_KMS_VERIFICATION_REPORT.md) - GCP KMS verification results

### CVSS/EPSS Security Scoring (4 files)
- [CVSS_EPSS_CODE_REFERENCE.md](CVSS_EPSS_CODE_REFERENCE.md) - Code reference for CVSS/EPSS implementation
- [CVSS_EPSS_COMPLETION_REPORT.md](CVSS_EPSS_COMPLETION_REPORT.md) - Completion report for security scoring
- [CVSS_EPSS_IMPLEMENTATION_SUMMARY.md](CVSS_EPSS_IMPLEMENTATION_SUMMARY.md) - Implementation summary
- [README_CVSS_EPSS.md](README_CVSS_EPSS.md) - CVSS/EPSS feature readme

### Adapter System (3 files)
- [ACTIVATION_FUNCTIONS_IMPLEMENTATION.md](ACTIVATION_FUNCTIONS_IMPLEMENTATION.md) - Activation function implementations
- [ADAPTER_RECORD_IMPLEMENTATION_CHECKLIST.md](ADAPTER_RECORD_IMPLEMENTATION_CHECKLIST.md) - Adapter record checklist
- [ADAPTER_RECORD_REFACTORING_SUMMARY.md](ADAPTER_RECORD_REFACTORING_SUMMARY.md) - Adapter record refactoring

### K-Reduction / Memory (4 files)
- [K_REDUCTION_IMPLEMENTATION.md](K_REDUCTION_IMPLEMENTATION.md) - K-reduction implementation details
- [K_REDUCTION_QUICK_START.md](K_REDUCTION_QUICK_START.md) - Quick start for K-reduction
- [HEAP_OBSERVER_DELIVERABLES.md](HEAP_OBSERVER_DELIVERABLES.md) - Heap observer deliverables
- [MEMORY_POOL_IMPLEMENTATION.md](MEMORY_POOL_IMPLEMENTATION.md) - Memory pool implementation

### Login/Auth (4 files)
- [LOGIN_CHANGES_DETAILED.md](LOGIN_CHANGES_DETAILED.md) - Detailed login changes
- [LOGIN_FINAL_VERIFICATION.md](LOGIN_FINAL_VERIFICATION.md) - Final login verification
- [LOGIN_IMPLEMENTATION_CHECKLIST.md](LOGIN_IMPLEMENTATION_CHECKLIST.md) - Login implementation checklist
- [LOGIN_UPDATE_SUMMARY.md](LOGIN_UPDATE_SUMMARY.md) - Login update summary

### Token Sampling (3 files)
- [MLX_TOKEN_SAMPLING_IMPLEMENTATION.md](MLX_TOKEN_SAMPLING_IMPLEMENTATION.md) - MLX token sampling implementation
- [TOKEN_SAMPLING_IMPLEMENTATION_SUMMARY.md](TOKEN_SAMPLING_IMPLEMENTATION_SUMMARY.md) - Token sampling summary
- [TOKEN_SAMPLING_QUICK_REFERENCE.md](TOKEN_SAMPLING_QUICK_REFERENCE.md) - Token sampling quick reference

### Federation (2 files)
- [FEDERATION_IMPLEMENTATION_SUMMARY.md](FEDERATION_IMPLEMENTATION_SUMMARY.md) - Federation implementation summary
- [FEDERATION_TICK_LEDGER_IMPLEMENTATION.md](FEDERATION_TICK_LEDGER_IMPLEMENTATION.md) - Tick ledger implementation

### General Implementation (5 files)
- [IMPLEMENTATION_COMPLETE.md](IMPLEMENTATION_COMPLETE.md) - Implementation completion report
- [IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md) - General implementation summary
- [INTEGRATION_SUMMARY.md](INTEGRATION_SUMMARY.md) - Integration summary
- [ROUTER_MIGRATION_SUMMARY.md](ROUTER_MIGRATION_SUMMARY.md) - Router migration summary
- [ORCHESTRATOR_CHANGES_SUMMARY.md](ORCHESTRATOR_CHANGES_SUMMARY.md) - Orchestrator changes summary

**Status**: Implementation complete. Useful for understanding design decisions but not required for development.

---

## MLX Documentation (9 files)

MLX backend implementation and testing documentation.

- [MLX_FFI_PERFORMANCE_OPTIMIZATION_SUMMARY.md](MLX_FFI_PERFORMANCE_OPTIMIZATION_SUMMARY.md) - FFI performance optimization
- [MLX_INTEGRATION_REPORT.md](MLX_INTEGRATION_REPORT.md) - MLX integration report
- [MLX_MEMORY_POOL_SUMMARY.md](MLX_MEMORY_POOL_SUMMARY.md) - Memory pool summary
- [MLX_REAL_INTEGRATION_SUMMARY.md](MLX_REAL_INTEGRATION_SUMMARY.md) - Real MLX integration summary
- [MLX_TESTING_INDEX.md](MLX_TESTING_INDEX.md) - MLX testing index
- [REAL_BACKEND_CODE_REFERENCE.md](REAL_BACKEND_CODE_REFERENCE.md) - Real backend code reference
- [REAL_BACKEND_INTEGRATION.md](REAL_BACKEND_INTEGRATION.md) - Real backend integration
- [REAL_MLX_INTEGRATION_TESTING.md](REAL_MLX_INTEGRATION_TESTING.md) - Real MLX integration testing
- [FFI_WRAPPER_IMPLEMENTATION_SUMMARY.md](FFI_WRAPPER_IMPLEMENTATION_SUMMARY.md) - FFI wrapper implementation

**Current Info**: See [crates/adapteros-lora-mlx-ffi/README.md](crates/adapteros-lora-mlx-ffi/README.md) for current MLX info.

---

## Training/Metrics Documentation (4 files)

Training system and metrics implementation.

- [GPU_TRAINING_COMPLETION_SUMMARY.md](GPU_TRAINING_COMPLETION_SUMMARY.md) - GPU training completion summary
- [TRAINING_METRICS_IMPLEMENTATION.md](TRAINING_METRICS_IMPLEMENTATION.md) - Training metrics implementation
- [TRAINING_METRICS_INDEX.md](TRAINING_METRICS_INDEX.md) - Training metrics index
- [TRAINING_METRICS_QUICK_START.md](TRAINING_METRICS_QUICK_START.md) - Training metrics quick start

**Current Info**: See [docs/QUICKSTART_COMPLETE_SYSTEM.md](docs/QUICKSTART_COMPLETE_SYSTEM.md) for current training guide.

---

## CoreML Documentation (3 files)

CoreML backend verification and integration.

- [COREML_ATTESTATION_DETAILS.md](COREML_ATTESTATION_DETAILS.md) - CoreML attestation details
- [COREML_DETERMINISM_VERIFICATION.md](COREML_DETERMINISM_VERIFICATION.md) - CoreML determinism verification
- [COREML_INTEGRATION_VERIFICATION.md](COREML_INTEGRATION_VERIFICATION.md) - CoreML integration verification

**Current Info**: See [docs/COREML_ACTIVATION.md](docs/COREML_ACTIVATION.md) for operational status.

---

## Security/Compliance (3 files)

Security implementation and testing documentation.

- [SECURITY_IMPLEMENTATION_SUMMARY.md](SECURITY_IMPLEMENTATION_SUMMARY.md) - Security implementation summary
- [SECURITY_TEST_README.md](SECURITY_TEST_README.md) - Security test readme
- [ENCLAVE_FALLBACK_CHANGES.md](ENCLAVE_FALLBACK_CHANGES.md) - Enclave fallback changes

---

## Benchmark/Performance (3 files)

Benchmarking guides and results.

- [BENCHMARK_GUIDE.md](BENCHMARK_GUIDE.md) - Benchmarking guide
- [BENCHMARK_REPORT_20251122_024934.md](BENCHMARK_REPORT_20251122_024934.md) - Benchmark report (dated)
- [QUANTIZATION_COMPRESSION_IMPLEMENTATION.md](QUANTIZATION_COMPRESSION_IMPLEMENTATION.md) - Quantization compression implementation

---

## Delivery & Manifest (3 files)

Delivery documentation and manifests.

- [DELIVERABLES_MANIFEST.md](DELIVERABLES_MANIFEST.md) - Complete deliverables manifest
- [DELIVERABLES_MLX_PERFORMANCE_OPTIMIZATION.md](DELIVERABLES_MLX_PERFORMANCE_OPTIMIZATION.md) - MLX performance optimization deliverables
- [STRESS_TESTS_SUMMARY.md](STRESS_TESTS_SUMMARY.md) - Stress test summary

---

## Build System (1 file)

Build and compilation documentation.

- [METAL_BUILD_SYSTEM_INTEGRATION.md](METAL_BUILD_SYSTEM_INTEGRATION.md) - Metal build system integration

---

## Testing (2 files)

Test implementation documentation.

- [ADAPTER_STACK_FILTERING_TESTS.md](ADAPTER_STACK_FILTERING_TESTS.md) - Adapter stack filtering tests
- [TEST_FIXES_SUMMARY.md](TEST_FIXES_SUMMARY.md) - Test fixes summary

---

## Miscellaneous (6 files)

Other documentation files.

- [AUDIT_UNFINISHED_FEATURES.md](AUDIT_UNFINISHED_FEATURES.md) - Audit of unfinished features
- [CHANGELOG.md](CHANGELOG.md) - Project changelog
- [DEADLOCK_ANALYSIS.md](DEADLOCK_ANALYSIS.md) - Deadlock analysis
- [DEPRECATIONS.md](DEPRECATIONS.md) - Deprecated features
- [PRD.md](PRD.md) - Product requirements document

---

## How to Use This Index

| If you are... | Start here |
|--------------|------------|
| **New developer** | Skip this section, read [CLAUDE.md](CLAUDE.md) instead |
| **Understanding design decisions** | Browse implementation reports above |
| **Updating architecture** | Check if related implementation doc exists |
| **Preparing release notes** | Check [DELIVERABLES_MANIFEST.md](DELIVERABLES_MANIFEST.md) |
| **Debugging MLX issues** | See MLX Documentation section |
| **Investigating security** | See Security/Compliance section |

---

## File Count Summary

| Category | Count |
|----------|-------|
| Implementation Reports | 28 |
| MLX Documentation | 9 |
| Training/Metrics | 4 |
| CoreML Documentation | 3 |
| Security/Compliance | 3 |
| Benchmark/Performance | 3 |
| Delivery & Manifest | 3 |
| Build System | 1 |
| Testing | 2 |
| Miscellaneous | 6 |
| **Total Orphaned** | **62** |
| Referenced in CLAUDE.md | 5 |
| **Grand Total** | **67** |

---

**Last Updated**: 2025-11-22
**Maintained By**: AdapterOS Team
