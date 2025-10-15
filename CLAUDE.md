# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AdapterOS is a directory attention layer for offline specialized AI - a Rust-based ML inference runtime optimized for Apple Silicon. It functions as a semantic attention mechanism that focuses AI capabilities on specific directories and codebases through a five-tier adapter hierarchy, providing code intelligence that understands both directory structure and contents.

**Key Characteristics:**
- Directory-aware AI that understands any codebase structure
- High-security inference with zero network egress during serving
- Deterministic execution with reproducible outputs
- Multi-tenant isolation with per-tenant process boundaries
- Evidence-grounded responses with RAG integration
- 20 policy packs enforcing compliance, security, and quality
- K-sparse LoRA routing with Metal-optimized kernels

## Build & Test Commands

### Building

```bash
# Build all crates (release mode)
cargo build --release

# Note: Metal backend is the primary production backend (no special flags needed)
# MLX backend is available via CLI selection but currently disabled due to PyO3 linker issues

# Build specific crate
cargo build -p mplora-worker
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run tests with all features
make test

# Run specific test file
cargo test --test determinism

# Run single test by name
cargo test test_router_topk --package mplora-router

# Run with output visible
cargo test -- --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Run clippy with warnings as errors
cargo clippy --workspace -- -D warnings

# Full check (fmt + clippy + test)
make check
```

### Metal Shaders

```bash
# Build Metal shaders
cd metal && bash build.sh

# Or use make
make metal
```

### Documentation

```bash
# Generate and open docs
cargo doc --no-deps --open
```

## Architecture Overview

### Workspace Structure

The project uses a Cargo workspace with ~40 crates organized by function:

**Core Inference Pipeline:**
- `mplora-worker` - Main inference engine with safety mechanisms
- `mplora-router` - K-sparse LoRA routing with Q15 quantized gates
- `mplora-kernel-api` - Trait definitions for compute kernels
- `mplora-kernel-mtl` - Metal-optimized fused kernels (attention + MLP + LoRA)
- `mplora-rag` - Evidence retrieval with HNSW vector search

**Control Plane:**
- `mplora-server` - Control plane API server
- `mplora-server-api` - REST API handlers and routes
- `mplora-cli` - Command-line tool (`aosctl`)
- `mplora-db` - SQLite database layer
- `mplora-client` - Client library for server communication

**Policy & Security:**
- `mplora-policy` - 20 policy packs enforcement engine
- `mplora-crypto` - Ed25519 signing, BLAKE3 hashing, HKDF
- `mplora-artifacts` - Content-addressed artifact store with SBOM
- `mplora-secd` - Secure Enclave integration (macOS)

**Infrastructure:**
- `mplora-telemetry` - Canonical JSON event logging with Merkle trees
- `mplora-manifest` - Plan manifests and configuration
- `mplora-profiler` - Adapter performance profiling
- `mplora-lifecycle` - Adapter lifecycle management
- `mplora-system-metrics` - GPU metrics and system monitoring

### Data Flow

1. **Request** arrives via Unix domain socket (no TCP)
2. **Policy check** validates tenant, evidence requirements
3. **Router** selects K adapters using feature-weighted scoring
4. **Evidence retrieval** (if required) via RAG system
5. **Kernels execute** fused Metal operations with selected LoRA adapters
6. **Policy validates** output (units, confidence thresholds)
7. **Response emitted** with trace (evidence refs, router summary)
8. **Telemetry logged** with sampling (100% for first 128 tokens, 5% after)

### Router System

The router (`mplora-router`) performs K-sparse adapter selection:

**Feature Vector (22 dimensions):**
- Language one-hot (8 dims): Python, Rust, JS, etc.
- Framework scores (3 dims): Django, React, etc.
- Symbol hits (1 dim): From code symbol index
- Path tokens (1 dim): File path matching
- Prompt verb one-hot (8 dims): "fix", "add", "refactor", etc.
- Attention entropy (1 dim): Optional complexity signal

**Weighted Scoring:**
- Language: 0.30 (strong signal)
- Framework: 0.25 (strong signal)
- Symbol hits: 0.20 (moderate)
- Path tokens: 0.15 (moderate)
- Prompt verb: 0.10 (weak)

**Output:** Top-K adapter indices with Q15 quantized gates (0-32767)

Router weights can be calibrated using `mplora-router/calibration.rs`.

### Policy Engine

20 policy packs enforced by `mplora-policy`:

1. **Egress Ruleset** - Zero network during serving, PF enforcement
2. **Determinism Ruleset** - Precompiled kernels, HKDF seeding
3. **Router Ruleset** - K bounds, entropy floor, Q15 gates
4. **Evidence Ruleset** - Mandatory open-book grounding
5. **Refusal Ruleset** - Abstain on low confidence
6. **Numeric & Units** - Unit normalization and validation
7. **RAG Index** - Per-tenant isolation, deterministic ordering
8. **Isolation** - Process per tenant (UID/GID separation)
9. **Telemetry** - Sampling rules, bundle rotation
10. **Retention** - Bundle retention policies
11. **Performance** - Latency budgets (p95 < 24ms)
12. **Memory** - 15% headroom, eviction order
13. **Artifacts** - Signature + SBOM required
14. **Secrets** - Secure Enclave backed
15. **Build & Release** - Determinism gates
16. **Compliance** - Control matrix mapping
17. **Incident** - Runbook procedures
18. **LLM Output** - JSON format, trace requirements
19. **Adapter Lifecycle** - Activation thresholds
20. **Full Pack Example** - Complete JSON schema

### Determinism Guarantees

- **Metal kernels** precompiled to `.metallib` (no runtime compilation)
- **RNG seeding** via HKDF from global seed
- **Retrieval ordering** deterministic: `(score desc, doc_id asc)`
- **JSON serialization** canonical (JCS)
- **Compiler flags** fixed (no fast-math)

### Telemetry System

Events logged to `mplora-telemetry`:
- **Router decisions** - Full log first 128 tokens, 5% sampling after
- **Inference events** - Duration, success, memory usage
- **Policy violations** - 100% sampling
- **Memory events** - Evictions, K reductions, OOM events

Events are:
1. Serialized to canonical JSON (JCS)
2. Hashed with BLAKE3
3. Bundled with Merkle tree
4. Signed with Ed25519

## Development Guidelines

### Adding New Adapters

1. Use `aosctl register-adapter` to add to registry
2. Provide capability card with dataset lineage
3. Ensure adapter hash is in allowed ACL
4. Adapters below 2% activation will be evicted

### Working with Policies

Policy packs are defined in `.cursor/rules/global.mdc` and enforced by `mplora-policy`. When adding features:

1. Check which policy packs apply
2. Implement enforcement in appropriate crate
3. Add telemetry for violations
4. Update compliance matrix if needed

### Metal Kernel Development

Metal shaders are in `metal/` directory:
- Source: `.metal` files
- Compiled: `.metallib` files (precompiled, embedded in binary)
- Build: `metal/build.sh` script

**Critical:** Kernels must be deterministic (no fast-math, fixed rounding).

### Testing Determinism

Use `tests/determinism.rs` to verify outputs are reproducible:

```bash
cargo test --test determinism -- --nocapture
```

Determinism failures block promotion gates.

### Working with the Database

SQLite schema managed via migrations in `migrations/`. Use `mplora-db` crate:

```rust
use adapteros_db::Database;

let db = Database::open("var/aos.db")?;
let tenant = db.get_tenant("default")?;
```

### Memory Management

Worker maintains ≥15% unified memory headroom. On pressure:

1. Drop ephemeral adapters (TTL expired)
2. Reduce K by 1 (try before evicting hot adapters)
3. Evict cold adapters (LRU)
4. Evict warm adapters (LRU)
5. Deny new sessions

See `mplora-lifecycle` for implementation.

### Git Integration

New `mplora-git` crate provides:
- Repository session management
- Commit diff extraction
- File history tracking
- Integration with code intelligence

See `migrations/0015_git_sessions.sql` for schema.

## Important Implementation Notes

### Agent Hallucination Prevention Framework

**Purpose:** Prevent hallucinations and duplicates by enforcing mandatory verification and visibility rules.

#### Core Principles
1. **Verify Every Tool Operation** - Never trust tool success without verification
2. **Evidence-Based Claims** - Every claim must include verification evidence
3. **Incremental Verification** - One change → verify → document → repeat
4. **Completion Bias Prevention** - Use accurate completion percentages
5. **Tool Reliability Rules** - Use multiple verification methods
6. **Duplicate Prevention** - Check for existing implementations before creating new ones
7. **Full Context Visibility** - Always search codebase before making assumptions

#### Mandatory Verification Workflow

**Before ANY implementation:**
```bash
1. codebase_search("existing implementation")  # Search for duplicates
2. grep "function_name|struct_name|trait_name"  # Check for existing symbols
3. read_file("existing_files")  # Understand current implementation
4. Document findings and evidence
```

**After EVERY tool operation:**
```bash
1. read_file(target_file)           # Re-read to verify changes
2. grep "expected_content" target_file  # Confirm specific changes
3. cargo check --package target_package  # Compile if applicable
4. cargo test --package target_package    # Run tests if applicable
5. Manual verification of functionality   # Test the actual behavior
6. Check for duplicate implementations across crates
```

#### Duplicate Prevention Rules

**Mandatory Pre-Implementation Checks:**
- [ ] Search codebase for existing implementations using `codebase_search`
- [ ] Check for similar function/struct/trait names using `grep`
- [ ] Review existing crates in `crates/` directory
- [ ] Check `tools/inventory/` for existing functionality
- [ ] Verify no duplicate policy packs exist
- [ ] Document why new implementation is needed vs reusing existing

**Duplicate Detection Patterns:**
- [ ] Function names: `grep "fn function_name"`
- [ ] Struct names: `grep "struct StructName"`
- [ ] Trait names: `grep "trait TraitName"`
- [ ] Policy packs: Check `tools/inventory/policies.json`
- [ ] Database migrations: Check `migrations/` directory
- [ ] API endpoints: Check `crates/*/src/handlers.rs`

### Verification Framework

Per `.cursor/rules/global.mdc`, always verify tool operations:

1. After file modifications, re-read to confirm changes
2. Use `grep` to verify specific content
3. Run `cargo check` if modifying Rust code
4. Run tests for modified packages
5. Never trust tool success without verification

### Package Managers

- **Rust:** Use `cargo` (standard)
- **Python:** Use `uv` (never pip directly)
- **JavaScript:** Use `pnpm` (never npm/yarn)

### Crate Dependencies

When adding dependencies, check if already in `[workspace.dependencies]` in root `Cargo.toml`. Reuse workspace deps when possible for version consistency.

### Backend Architecture

MPLoRA uses a unified deterministic kernel abstraction with attestation enforcement:

**Metal Backend (Primary - Deterministic)**
- Native Metal implementation with precompiled kernels (`.metallib`)
- BLAKE3 hash verification of kernel binaries
- HKDF-seeded RNG for reproducible execution
- Deterministic floating-point mode (no fast-math)
- Maximum performance on Apple Silicon
- **DEFAULT AND REQUIRED FOR PRODUCTION**

**Backend Attestation System**
- All backends must implement `attest_determinism()` from `FusedKernels` trait
- Returns `DeterminismReport` with metallib hash, RNG method, compiler flags
- Policy engine validates attestation before allowing serving operations
- CLI tool `aosctl audit-determinism` provides detailed validation reports

**Feature Flags:**
- `default = ["deterministic-only"]` - Metal backend only (production)
- `experimental-backends` - Enables MLX/CoreML (development/testing only)

**MLX Backend (Experimental - Non-Deterministic)**
- Python/MLX implementation for development and experimentation
- **Requires `--features experimental-backends` at compile time**
- Currently disabled due to PyO3 linker issues
- **NOT FOR PRODUCTION**: Cannot guarantee reproducible outputs
- Runtime guards prevent accidental use in production builds

See `docs/determinism-attestation.md` for comprehensive documentation.

### Code Style

- Use `tracing` for logging (not `println!`)
- Errors via `mplora-core::AosError` and `Result<T>`
- Telemetry via `TelemetryWriter::log(event_type, data)`
- No network I/O in worker (Unix domain sockets only)

## Common Tasks

### Running the Server

```bash
# Build and start server
cargo run --release --bin mplora-server -- --config configs/cp.toml

# Or use CLI
cargo run --release --bin aosctl -- serve --plan-id <plan-id>
```

### Importing Models

```bash
./target/release/aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
```

### Building Plans

```bash
./target/release/aosctl build-plan \
  --tenant-id default \
  --manifest configs/cp.toml
```

### Debugging Router Decisions

```bash
# Enable router telemetry
export RUST_LOG=adapteros_lora_router=debug

# Run with trace output
cargo test test_router_scoring -- --nocapture
```

### Auditing Backend Determinism

```bash
# Validate Metal backend attestation
aosctl audit-determinism

# Get JSON report for automation
aosctl audit-determinism --format json > attestation.json

# Check exit code for CI/CD
aosctl audit-determinism && echo "Backend is deterministic"
```

### Testing Patch Proposals

```bash
# End-to-end patch proposal test
cargo test --test patch_proposal_e2e -- --nocapture

# Security validation
cargo test --test patch_security -- --nocapture
```

## Key Files & Locations

- **Manifests:** `configs/cp.toml` - Server and policy configuration
- **Migrations:** `migrations/*.sql` - Database schema evolution
- **Metal shaders:** `metal/*.metal` - GPU compute kernels
- **Tests:** `tests/*.rs` - Integration tests
- **CLI manual:** `crates/mplora-cli/docs/aosctl_manual.md`
- **Policy rules:** `.cursor/rules/global.mdc` - 20 policy packs

## Troubleshooting

### Compilation Issues

- **SQLite conflicts:** Some crates are disabled in workspace (see `Cargo.toml` comments)
- **PyO3 linker errors:** Install Xcode command-line tools and ensure PyO3 is properly configured
- **Metal shader errors:** Run `make metal` to rebuild shaders

### Test Failures

- **Determinism tests:** Check RNG seeding and retrieval ordering
- **Policy tests:** Verify policy pack configuration
- **Integration tests:** May require database initialization

### Performance Issues

- **Router overhead >8%:** Check feature vector size and gate quantization
- **Memory pressure:** Reduce K or increase headroom threshold
- **Kernel latency:** Profile with `tests/kernel_profile.rs`

## 20 Policy Packs

The system enforces 20 comprehensive policy packs covering all aspects of operation:

### Core Policy Packs

1. **Egress Ruleset** - Zero network during serving, PF enforcement
2. **Determinism Ruleset** - Precompiled kernels, HKDF seeding
3. **Router Ruleset** - K bounds, entropy floor, Q15 gates
4. **Evidence Ruleset** - Mandatory open-book grounding
5. **Refusal Ruleset** - Abstain on low confidence
6. **Numeric & Units** - Unit normalization and validation
7. **RAG Index** - Per-tenant isolation, deterministic ordering
8. **Isolation** - Process per tenant (UID/GID separation)
9. **Telemetry** - Sampling rules, bundle rotation
10. **Retention** - Bundle retention policies
11. **Performance** - Latency budgets (p95 < 24ms)
12. **Memory** - 15% headroom, eviction order
13. **Artifacts** - Signature + SBOM required
14. **Secrets** - Secure Enclave backed
15. **Build & Release** - Determinism gates
16. **Compliance** - Control matrix mapping
17. **Incident** - Runbook procedures
18. **LLM Output** - JSON format, trace requirements
19. **Adapter Lifecycle** - Activation thresholds
20. **Full Pack Example** - Complete JSON schema

### Policy Enforcement Locations

- `aos-policy`: parse, validate, and expose typed policy objects
- `aos-kernel-mtl`: determinism, kernel hash checks, Q15 contracts
- `aos-router`: top-K, entropy floor, quantized gates
- `aos-rag`: index scope, deterministic order, supersession logic
- `aos-worker`: refusal, numeric sanity, memory watermark, zero-egress preflight
- `aos-telemetry`: canonical JSON, sampling, bundle rotation + signing
- `aos-artifacts`: signature + SBOM gates; CAS only
- `aos-cli`: promotion gate, replay check, incident runbooks

### Promotion Checklist

A Control Plane can promote only if all policy requirements are met:

- **Determinism:** metallib present and hashed; replay shows zero diff
- **Backend Attestation:** `attest_determinism()` passes validation; Metal backend only
- **Feature Flags:** Built with `deterministic-only` (default), no experimental backends
- **Egress:** PF enforced; outbound tests fail as expected
- **Router:** K, entropy floor, and gate quantization match policy
- **Evidence:** ARR ≥ 0.95 and ECS@5 ≥ 0.75 on regulated suite
- **Refusal:** underspecified prompts refuse with required fields listed
- **Numeric:** unit sanity passes; canonical units in traces
- **Isolation:** adversarial cross-tenant suite zero leaks
- **Telemetry:** event coverage and sampling match the pack; bundle signed
- **Performance:** budgets satisfied; router overhead ≤ threshold
- **Artifacts:** signed, SBOM complete, CAS verified
- **Compliance:** control matrix cross-links resolve to existing evidence
- **Rollback:** previous CP available; `aos-cli rollback` dry run passes

## Directory Intelligence Architecture

### Five-Tier Adapter Hierarchy

The system uses a semantic five-tier hierarchy for directory-aware AI:

```
Layer 5: Ephemeral (per-directory-change, TTL-bound)
├─ directory_change_abc123 (rank 4-8, TTL 24-72h)
└─ Purpose: Fresh symbols, recent directory changes

Layer 4: Directory-Specific (tenant-specific, path-bound)  
├─ directory_myproject_v3 (rank 16-32)
└─ Purpose: Internal APIs, conventions, directory style

Layer 3: Frameworks (type-specific, stack-bound)
├─ framework_django_v1 (rank 8-16)
├─ framework_react_v2 (rank 8-16) 
└─ Purpose: Framework APIs, idioms, gotchas

Layer 2: Code (domain-general coding knowledge)
├─ code_lang_v1 (rank 16)
└─ Purpose: Language reasoning, patterns, refactoring

Layer 1: Base (general language model)
└─ Qwen2.5-7B-Instruct (int4)
```

### Directory Intelligence Features

- **Path-aware**: Understands directory structure and file organization
- **Language detection**: Auto-detects programming languages from file extensions
- **Framework detection**: Identifies frameworks from directory structure and config files
- **Symbol mapping**: Maps functions, classes, variables across the directory tree
- **Test discovery**: Finds and maps tests to code symbols within the directory

### Code Intelligence Pipeline

```
Any Directory Path
    │
    ├──> [1] Scan & Parse (tree-sitter)
    │         └──> Parsed ASTs per file
    │
    ├──> [2] Extract Symbols & Build Graph
    │         └──> CodeGraph (nodes + edges)
    │
    ├──> [3] Detect Frameworks
    │         └──> frameworks.json
    │
    ├──> [4] Build Symbol Index
    │         └──> SQLite FTS5 database
    │
    ├──> [5] Chunk & Embed
    │         └──> Vector index (HNSW)
    │
    └──> [6] Package & Store
              └──> CAS artifacts + registry entries
```

## References

- **README.md** - Quick start and installation
- **docs/architecture.md** - System architecture deep dive
- **docs/control-plane.md** - API documentation
- **docs/QUICKSTART.md** - 10-minute getting started guide
- **docs/MLX_INTEGRATION.md** - Python/MLX integration guide
- **docs/code-intelligence/** - Directory intelligence specifications
- **.cursor/rules/global.mdc** - Complete policy packs and rules
