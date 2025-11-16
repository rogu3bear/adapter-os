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
