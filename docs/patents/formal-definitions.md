<<<<<<< HEAD
# Formal Definitions and Implementation Citations

**Purpose:** Map formal definitions to current implementations for patent documentation
**Last Updated:** 2025-11-15

---

## Table of Contents

- [Stack-Based Adapter Routing](#stack-based-adapter-routing)
- [Dynamic Adapter Loading](#dynamic-adapter-loading)
- [Code Ingestion Pipeline](#code-ingestion-pipeline)
- [Document Processing](#document-processing)
- [Training From Code](#training-from-code)

---

## Stack-Based Adapter Routing

### Definition

**Adapter Stack Filtering** is a mechanism to constrain K-sparse router selection to a predefined subset of adapters (a "stack"), enabling domain-specific or tenant-specific adapter isolation while preserving deterministic routing behavior.

### Formal Properties

1. **Subset Constraint**: Given adapter catalog `C = {a₁, a₂, ..., aₙ}` and stack `S ⊆ C`, router selection `R(x, C, k) → {aᵢ₁, ..., aᵢₖ}` is constrained such that `{aᵢ₁, ..., aᵢₖ} ⊆ S`.

2. **Determinism Preservation**: For identical seed `σ`, features `x`, priors `p`, and stack `S`, router produces identical selection: `R(x, p, S, σ) = R(x, p, S, σ)`.

3. **Catalog Consistency**: Stack activation fails if `∃ a ∈ S : a ∉ C`, ensuring referential integrity.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-lora-router/src/lib.rs` L181-L196: `StackFilter` data structure
- `crates/adapteros-lora-router/src/lib.rs` L228-L273: Stack activation and catalog management
- `crates/adapteros-lora-router/src/lib.rs` L772-L786: `restrict_scores_to_stack` filtering logic

**Correctness Proofs:**
- `tests/router_correctness_proofs.rs` L11-L45: Determinism proof
- `tests/router_correctness_proofs.rs` L48-L81: Subset containment proof
- `tests/router_correctness_proofs.rs` L84-L109: Catalog consistency proof

**Integration Points:**
- `crates/adapteros-lora-router/src/lib.rs` L425-L430: Stack filtering in `route()` method
- `crates/adapteros-lora-router/src/lib.rs` L531-L538: Stack filtering in `route_with_telemetry()` method

---

## Dynamic Adapter Loading

### Definition

**Runtime Adapter Loading** enables Metal GPU kernels to dynamically load adapter weight matrices from SafeTensors format into GPU buffers, supporting hot-swapping and memory-efficient adapter management.

### Formal Properties

1. **Memory Accounting**: Total GPU memory usage `M = Σᵢ size(Aᵢ)` where `Aᵢ` are loaded adapters, tracked atomically.

2. **Type Safety**: SafeTensors tensor `T` with shape `[m, n]` and dtype `f32` maps to Metal buffer `B` with size `m × n × 4` bytes.

3. **Integrity**: Loaded adapter `A` with ID `id`, rank `r`, and alpha `α` maintains invariant: `∀ module ∈ A : shape(lora_a) = [input_dim, r] ∧ shape(lora_b) = [r, output_dim]`.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-lora-kernel-mtl/src/mplora.rs` L26-L46: `LoadedAdapter` and `AdapterModuleBuffers` structures
- `crates/adapteros-lora-kernel-mtl/src/mplora.rs` L76-L79: GPU memory tracking with `AtomicU64`
- `crates/adapteros-lora-kernel-mtl/src/mplora.rs` L119-L175: SafeTensors parsing and validation

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L136-L180: Metal kernel fault injection suite
- `tests/fault_injection_harness.rs` L143-L156: Invalid SafeTensors data handling
- `tests/fault_injection_harness.rs` L158-L169: Dimension overflow protection
- `tests/fault_injection_harness.rs` L171-L186: Buffer size validation

---

## Code Ingestion Pipeline

### Definition

**Code Ingestion** is the process of extracting, validating, and chunking source code from repositories for use in adapter training datasets.

### Formal Properties

1. **Path Safety**: For repository root `R` and file path `f`, canonical path `c(f)` must satisfy `c(f) ∈ subtree(R)`, preventing directory traversal.

2. **Binary Exclusion**: File `f` is processed only if `is_text(f) = true`, where `is_text` detects binary content.

3. **Size Bounds**: File `f` is processed only if `size(f) ≤ MAX_FILE_SIZE`, preventing resource exhaustion.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-orchestrator/src/code_ingestion.rs`: Code ingestion module

**Command Interface:**
- `crates/adapteros-cli/src/commands/adapter_train_from_code.rs`: CLI command implementation
- `crates/adapteros-cli/src/commands/adapter.rs` L455-L460: Command registration
- `crates/adapteros-cli/src/commands/adapter.rs` L530: Command handler dispatch

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L182-L284: Code ingestion fault injection suite
- `tests/fault_injection_harness.rs` L186-L212: Path traversal protection tests
- `tests/fault_injection_harness.rs` L214-L234: Malicious filename handling
- `tests/fault_injection_harness.rs` L236-L254: Symlink traversal protection
- `tests/fault_injection_harness.rs` L256-L273: Binary file detection
- `tests/fault_injection_harness.rs` L275-L295: Maximum file size enforcement

---

## Document Processing

### Definition

**Document Ingestion** encompasses parsing, chunking, and extracting structured data from documentation formats (Markdown, PDF) for training dataset construction.

### Formal Properties

1. **Chunk Boundaries**: Document `D` is partitioned into chunks `{C₁, ..., Cₙ}` such that `∀ i : |Cᵢ| ≤ MAX_CHUNK_SIZE` and `∪ᵢ Cᵢ = D`.

2. **Format Safety**: Parser `P(d)` for document `d` must handle malformed input without undefined behavior.

3. **Compression Bomb Protection**: PDF parser rejects documents where `decompressed_size / compressed_size > MAX_EXPANSION_RATIO`.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-ingest-docs/src/chunker.rs`: Document chunking algorithm
- `crates/adapteros-ingest-docs/src/markdown.rs`: Markdown parser
- `crates/adapteros-ingest-docs/src/pdf.rs`: PDF extraction
- `crates/adapteros-ingest-docs/src/types.rs`: Type definitions
- `crates/adapteros-ingest-docs/src/utils.rs`: Utility functions

**Workspace Integration:**
- `Cargo.toml` L37: Workspace member registration

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L58-L134: Document ingestion fault injection suite
- `tests/fault_injection_harness.rs` L62-L84: Malformed markdown handling
- `tests/fault_injection_harness.rs` L86-L100: PDF bomb protection
- `tests/fault_injection_harness.rs` L102-L120: Chunker boundary conditions

---

## Training From Code

### Definition

**Training From Code** enables direct training of adapters from repository snapshots without manual dataset preparation, integrating code ingestion, chunking, and training pipeline.

### Formal Properties

1. **End-to-End Workflow**: Repository `R` → Code ingestion → Dataset `D` → Training → Adapter `A`, maintaining provenance chain.

2. **Reproducibility**: Given repository hash `h(R)`, seed `σ`, and hyperparameters `θ`, training produces deterministic adapter `A(R, σ, θ)`.

3. **Safety Isolation**: Training process operates within sandbox constraints (no network egress, filesystem isolation).

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-cli/src/commands/adapter_train_from_code.rs`: Complete workflow implementation

**Integration:**
- `crates/adapteros-cli/src/commands/mod.rs` L6: Module registration
- `crates/adapteros-cli/Cargo.toml` L40: Orchestrator dependency for code ingestion

**Test Coverage:**
- `crates/adapteros-cli/tests/train_from_code_tests.rs`: Integration tests
- `crates/adapteros-cli/tests/data/train_from_code_repo/`: Test repository fixture

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L13-L56: Train-from-code fault injection suite
- `tests/fault_injection_harness.rs` L17-L31: Malformed repository path handling
- `tests/fault_injection_harness.rs` L33-L48: Path traversal protection
- `tests/fault_injection_harness.rs` L50-L57: Extremely long path handling

---

## TUI Service Control

### Definition

**TUI Service Control** provides terminal user interface for managing AdapterOS daemon lifecycle, adapter operations, and system monitoring.

### Formal Properties

1. **State Consistency**: Service state transitions follow finite state machine: `Stopped → Starting → Running → Stopping → Stopped`.

2. **Concurrent Safety**: Multiple UI operations maintain consistency through atomic state updates.

3. **API Resilience**: UI handles API failures gracefully without crashing or corrupting state.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-tui/src/app/service_control.rs`: Service control logic

**State Management:**
- `crates/adapteros-tui/src/app.rs`: Application state
- `crates/adapteros-tui/src/app/types.rs`: Type definitions
- `crates/adapteros-tui/src/app/api.rs`: API client

**UI Components:**
- `crates/adapteros-tui/src/ui/dashboard.rs`: Dashboard view
- `crates/adapteros-tui/src/ui/mod.rs`: UI module

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L286-L320: TUI service control fault injection
- `tests/fault_injection_harness.rs` L290-L308: Concurrent state mutation safety
- `tests/fault_injection_harness.rs` L310-L325: Invalid API response handling

---

## Inference Pipeline Updates

### Definition

Updates to the **Inference Pipeline** enhance adapter management, tokenization, and request processing.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-lora-worker/src/inference_pipeline.rs`: Pipeline refactoring
- `crates/adapteros-lora-worker/src/tokenizer.rs`: Tokenizer improvements

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L363-L408: Inference pipeline fault injection
- `tests/fault_injection_harness.rs` L367-L381: Invalid token sequence handling
- `tests/fault_injection_harness.rs` L383-L398: Sequence length limit enforcement
- `tests/fault_injection_harness.rs` L400-L413: Batch size validation

---

## Adapter Manifest Extensions

### Definition

**Adapter Manifest Extensions** add metadata fields for improved adapter tracking and validation.

### Implementation Citations

**Primary Implementation:**
- `crates/adapteros-manifest/src/lib.rs`: Extended manifest schema

**Fault Injection Tests:**
- `tests/fault_injection_harness.rs` L322-L361: Manifest parsing fault injection
- `tests/fault_injection_harness.rs` L326-L344: Malformed manifest handling
- `tests/fault_injection_harness.rs` L346-L370: Hash collision resistance

---

## Build Profile Configuration

### Definition

**Build Profile Configuration** for `mplora-fuzz` enables optimized compilation of Metal kernel fuzzing harness.

### Implementation Citations

**Primary Implementation:**
- `Cargo.toml` L205-L213: Profile configuration for `mplora-fuzz` package

---

## Cross-References

### Related Patent Documentation
- `docs/PATENT_MPLORA_ARCHITECTURE.md`: MPLoRA architecture specification
- `docs/PATENT_MPLORA_NOVELTY.md`: Novelty claims
- `docs/PATENT_IMPLEMENTATION_STATUS.md`: Implementation status tracking

### Test Coverage
- `tests/router_correctness_proofs.rs`: Router correctness proofs (270 lines)
- `tests/fault_injection_harness.rs`: Comprehensive fault injection suite (470 lines)

### Key Principles

All implementations cited above adhere to AdapterOS core principles:
- **Determinism**: Reproducible execution with seeded randomness
- **Safety**: Input validation, bounds checking, error handling
- **Auditability**: Clear provenance and logging
- **Isolation**: Sandbox constraints, no unauthorized egress

---

**Citation Format:** `<file_path>` L<start>-L<end>

**Note:** Line numbers are approximate and may shift with code changes. Use git blame for precise tracking.
=======
# Formal Definitions and Patent Citations

This document maintains citations linking patent claims to their current implementations in the AdapterOS codebase.

---

## Adapter Taxonomy System

**Feature:** Hierarchical naming system with semantic validation and lineage tracking

**Patent Gate:** Adversarial test coverage for security-critical naming validation

### Core Implementation

1. **AdapterName Type**
   - **Location:** `crates/adapteros-core/src/naming.rs` (lines 1-450)
   - **Validation:** Format `{tenant}/{domain}/{purpose}/{revision}` with regex enforcement
   - **Security:** Reserved namespace blocking, tenant isolation, SQL injection prevention

2. **StackName Type**
   - **Location:** `crates/adapteros-core/src/naming.rs` (lines 451-620)
   - **Validation:** Format `stack.{namespace}[.{identifier}]` with max length enforcement
   - **Security:** Profanity filtering, path traversal prevention

3. **NamingPolicy**
   - **Location:** `crates/adapteros-policy/src/packs/naming_policy.rs` (lines 1-450)
   - **Policy ID:** 23 (23rd canonical policy pack)
   - **Enforcement:** Tenant isolation, revision monotonicity, reserved namespace protection

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 723-1014)

1. **Malformed Input Resistance** (lines 727-752)
   - Empty strings, missing components, invalid characters
   - **Citation:** test_adapter_name_malformed_inputs

2. **Reserved Namespace Protection** (lines 754-774)
   - Blocks system, admin, root, global, default, test tenants
   - **Citation:** test_adapter_name_reserved_namespaces

3. **Tenant Isolation Enforcement** (lines 776-796)
   - Prevents cross-tenant adapter creation
   - **Citation:** test_adapter_name_tenant_isolation_violation

4. **Revision Monotonicity** (lines 798-817)
   - Prevents large revision gaps (>5)
   - **Citation:** test_adapter_name_revision_monotonicity_violation

5. **Injection Attack Prevention** (lines 860-892)
   - SQL injection resistance
   - Path traversal prevention
   - **Citations:**
     - test_adapter_name_sql_injection_attempt
     - test_adapter_name_path_traversal_attempt

6. **Input Validation Edge Cases** (lines 894-978)
   - Consecutive hyphens rejection
   - Leading/trailing hyphens rejection
   - Unicode character rejection
   - Max length enforcement (200 chars for adapters, 100 for stacks)
   - **Citations:**
     - test_adapter_name_consecutive_hyphens
     - test_adapter_name_leading_trailing_hyphens
     - test_adapter_name_unicode_rejection
     - test_adapter_name_max_length_violation

### Database Schema

**Migration:** `migrations/0061_adapter_taxonomy.sql`

1. **Semantic Name Columns**
   - adapter_name (UNIQUE), tenant_namespace, domain, purpose, revision
   - **Constraints:** Unique(tenant_id, domain, purpose, revision)

2. **Lineage Tracking**
   - parent_id (REFERENCES adapters.id), fork_type, fork_reason
   - **Validation:** Triggers prevent circular dependencies

3. **Validation Triggers**
   - validate_adapter_name_format (BEFORE INSERT)
   - validate_parent_exists (BEFORE INSERT)
   - **Security:** Format validation at database layer

### REST API

**Endpoints:** `crates/adapteros-server-api/src/handlers.rs`

1. **POST /v1/adapters/validate-name**
   - Real-time name validation before registration
   - Returns parsed components and violation details

2. **POST /v1/stacks/validate-name**
   - Stack name validation endpoint
   - Checks namespace and identifier validity

3. **GET /v1/adapters/next-revision/{tenant}/{domain}/{purpose}**
   - Auto-increments revision numbers
   - Enforces monotonicity constraints

### UI Integration

**Component:** `ui/src/components/Adapters.tsx` (lines 88-131, 515-560)

1. **Semantic Name Display**
   - Color-coded components: blue (tenant), green (domain), standard (purpose)
   - Revision badges, fork type indicators, lineage markers

2. **Backward Compatibility**
   - Falls back to legacy names when semantic fields absent
   - Gradual migration support

### Fuzz Testing

**Targets:** `fuzz/fuzz_targets/`

1. **adapter_name_parse.rs**
   - Fuzzes AdapterName::parse with arbitrary byte sequences
   - Tests component extraction, revision parsing, lineage checking

2. **stack_name_parse.rs**
   - Fuzzes StackName::parse with arbitrary inputs
   - Tests namespace/identifier extraction, validation

### Test Coverage Summary

- **Unit Tests:** 24 passing (15 core + 9 policy)
- **Adversarial Tests:** 16 attack resistance tests
- **Integration Tests:** End-to-end validation in `tests/adapter_taxonomy_integration.rs`
- **Fuzz Tests:** 2 targets for robustness verification

---

## Lineage and Fork Semantics

**Feature:** Parent-child relationships with fork type tracking

### Implementation

1. **Fork Types**
   - **Extension:** Incremental improvement, same lineage
   - **Independent:** Divergent use case, breaks compatibility

2. **Lineage Rules**
   - Parent must exist before child registration
   - Parent must be in same tenant namespace
   - Circular dependencies forbidden

3. **Citations**
   - **Core Type:** `crates/adapteros-core/src/naming.rs` (ForkType enum)
   - **Policy Enforcement:** `crates/adapteros-policy/src/packs/naming_policy.rs`
   - **Database Schema:** `migrations/0061_adapter_taxonomy.sql` (parent_id, fork_type columns)

---

## Policy Engine Threshold Configuration

**Feature:** Config-driven policy thresholds ensuring documentation and enforcement cannot diverge

**Patent Gate:** Adversarial test coverage for floating-point edge cases and boundary conditions

### Core Implementation

1. **PolicyEngine Threshold Methods**
   - **Location:** `crates/adapteros-policy/src/lib.rs` (lines 107-163)
   - **Methods:** `check_resource_limits`, `check_system_thresholds`, `check_memory_headroom`, `should_open_circuit_breaker`
   - **Configuration Source:** `adapteros_manifest::Policies` (PerformancePolicy, MemoryPolicy)

2. **PerformancePolicy Struct**
   - **Location:** `crates/adapteros-manifest/src/lib.rs` (lines 432-440)
   - **Fields:**
     - `max_tokens: usize` - Maximum tokens per request (default: 1000)
     - `cpu_threshold_pct: f32` - CPU usage threshold (default: 90.0)
     - `memory_threshold_pct: f32` - Memory usage threshold (default: 95.0)
     - `circuit_breaker_threshold: usize` - Failure count threshold (default: 5)

3. **MemoryPolicy Struct**
   - **Location:** `crates/adapteros-manifest/src/lib.rs` (lines 443-447)
   - **Fields:**
     - `min_headroom_pct: u8` - Minimum memory headroom (default: 15)

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 1016-1200)

1. **Floating-Point Edge Cases** (lines 1020-1055)
   - NaN input handling
   - Infinity (positive/negative) input handling
   - Subnormal values
   - **Citation:** test_policy_thresholds_nan_handling, test_policy_thresholds_infinity_handling

2. **Integer Overflow/Underflow** (lines 1057-1087)
   - usize::MAX for max_tokens
   - Zero threshold edge cases
   - **Citation:** test_policy_thresholds_integer_overflow, test_policy_thresholds_zero_thresholds

3. **Boundary Value Testing** (lines 1089-1120)
   - Exact threshold boundaries (at, above, below)
   - Negative percentage values
   - Percentage values exceeding 100.0
   - **Citation:** test_policy_thresholds_boundary_values, test_policy_thresholds_negative_percentages

4. **Error Message Validation** (lines 1122-1145)
   - Ensures error messages include actual threshold values
   - Prevents silent failures
   - **Citation:** test_policy_thresholds_error_message_accuracy

### Integration Tests

**Location:** `crates/adapteros-policy/tests/policy_engine_thresholds.rs` (lines 1-317)

1. **Config-Driven Enforcement** (lines 17-121)
   - Verifies thresholds come from config, not hard-coded
   - Tests runtime config changes

2. **Documentation Contract** (lines 268-316)
   - Enforces contract between manifest defaults and PolicyEngine
   - Prevents documentation drift

## Multi-Agent Coordination System

**Feature:** Deterministic cross-agent synchronization with tick-based barriers and global sequencing

**Patent Gate:** Adversarial test coverage for distributed coordination failures

### Core Implementation

1. **AgentBarrier Type**
   - **Location:** `crates/adapteros-deterministic-exec/src/multi_agent.rs` (lines 36-150)
   - **Synchronization:** Tick-based barriers ensuring all agents reach same logical time
   - **Determinism:** Global sequence counter prevents race conditions
   - **Fault Tolerance:** Timeout protection and agent registration validation

2. **GlobalTickLedger**
   - **Location:** `crates/adapteros-deterministic-exec/src/global_ledger.rs` (lines 73-604)
   - **Consistency:** Merkle chain verification for cross-host event ordering
   - **Replay Protection:** Tamper-evident event logging with BLAKE3 hashes
   - **Federation:** Cross-host divergence detection and consistency reporting

3. **CoordinatedAction**
   - **Location:** `crates/adapteros-deterministic-exec/src/multi_agent.rs` (lines 170-204)
   - **Atomicity:** Global sequence numbers for deterministic action ordering
   - **Integrity:** BLAKE3 hash verification of action payloads
   - **Serialization:** Deterministic encoding for cross-agent communication

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 1281-1382)

1. **Barrier Synchronization Attacks** (lines 1285-1303)
   - Empty agent names, extremely long names, special characters
   - **Citation:** test_multi_agent_barrier_adversarial_conditions

2. **Merkle Chain Tampering** (lines 1305-1382)
   - Adversarial event data, tampered hashes, replay attacks
   - **Citation:** test_global_tick_ledger_merkle_chain_integrity

3. **Cross-Host Consistency** (lines 1365-1381)
   - Divergence detection, tampered peer data validation
   - **Citation:** test_global_tick_ledger_merkle_chain_integrity

**Federation Coverage:** `tests/federation_adversarial_tests.rs` (lines 1-227)

1. **Configuration Attacks** (lines 11-31)
   - Malformed peer hosts, invalid hostnames, IP addresses
   - **Citation:** test_federation_malformed_config

2. **Signature Verification** (lines 33-52)
   - Empty messages, wrong messages, empty signatures
   - **Citation:** test_signature_verification_adversarial

3. **Replay Attack Prevention** (lines 54-73)
   - Same message replay detection and rejection
   - **Citation:** test_federation_replay_attack_prevention

4. **Man-in-the-Middle Protection** (lines 75-94)
   - Certificate validation and tampering detection
   - **Citation:** test_federation_man_in_middle_protection

5. **Denial-of-Service Protection** (lines 96-125)
   - Connection limit enforcement and resource exhaustion prevention
   - **Citation:** test_federation_dos_protection

6. **Message Tampering Detection** (lines 127-146)
   - Content modification detection and integrity verification
   - **Citation:** test_federation_message_tampering_detection

## Database Schema Recovery System

**Feature:** Automatic schema migration and integrity verification with adversarial input handling

**Patent Gate:** Fault injection testing for database corruption and migration failures

### Core Implementation

1. **Schema Validation**
   - **Location:** `crates/adapteros-db/src/lib.rs` (lines 1-200)
   - **Migration:** Automatic application of missing schema versions
   - **Integrity:** Table existence and constraint validation
   - **Recovery:** Graceful handling of corrupted database files

2. **Process Monitoring Schema**
   - **Location:** `crates/adapteros-db/src/process_monitoring.rs` (lines 1-1400)
   - **Tables:** process_monitoring_rules, process_alerts, process_anomalies
   - **Constraints:** Foreign key relationships and data type validation
   - **Indexing:** Performance optimization for monitoring queries

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 1384-1527)

1. **Database Corruption** (lines 1389-1433)
   - Corrupted files, invalid SQLite headers, migration failures
   - **Citation:** test_database_schema_recovery_adversarial

2. **Process Monitoring Attacks** (lines 1435-1527)
   - Empty fields, extreme values, special characters, NaN/Infinity
   - **Citation:** test_process_monitoring_adversarial_inputs

3. **Schema Integrity** (lines 1411-1433)
   - Table existence verification, constraint validation
   - **Citation:** test_database_schema_recovery_adversarial

---

**Last Updated:** 2025-11-16
**Maintainer:** AdapterOS Core Team
>>>>>>> integration-branch
