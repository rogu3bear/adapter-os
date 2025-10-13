# Code Intelligence Implementation Roadmap

## Overview

Phased implementation plan with gates, dependencies, and acceptance criteria. No timelines—only sequences and quality bars.

---

## Sequence S-1: CodeGraph & Indexers

### Goals
Build foundation for code understanding: parsing, graph construction, deterministic serialization.

### Tasks

1. **Create aos-codegraph crate**
   - Cargo.toml with tree-sitter dependencies
   - Module structure (lib, graph, parser, symbols, etc.)

2. **Implement tree-sitter wrapper**
   - `LanguageParser` struct
   - Parse methods for Python, Rust, TypeScript
   - Query loading from `.scm` files

3. **Implement CodeGraph**
   - Node types (FileNode, SymbolNode, TestNode, FrameworkNode)
   - Edge types (Defines, Calls, Imports, TestCovers, etc.)
   - Graph building methods
   - Deterministic serialization (bincode)

4. **Symbol extraction**
   - Tree-sitter queries for each language
   - Function, class, method extraction
   - Signature and docstring parsing

5. **Call graph building**
   - Extract function calls from AST
   - Build call edges
   - Handle cross-file calls

6. **Framework detection**
   - Fingerprint definitions (Django, React, etc.)
   - Version extraction from manifests
   - Config file collection

### Gate Requirements

✅ **Determinism**:
- Same repo + commit → identical CodeGraph hash
- Test: Build graph twice, compare hashes

✅ **Completeness**:
- Symbol extraction ≥ 95% on test corpus
- Call graph accuracy ≥ 90%

✅ **Performance**:
- Parse 10K LOC < 10s
- Build graph for 50K LOC < 30s

### Test Corpus
- `tests/corpora/synthetic/python_basic/` (100 files, 2K LOC)
- `tests/corpora/synthetic/rust_basic/` (50 files, 3K LOC)
- `tests/corpora/synthetic/typescript_react/` (80 files, 4K LOC)

### Dependencies
- None (foundation layer)

---

## Sequence S-2: Symbol & Vector Indices

### Goals
Build queryable indices for fast symbol lookup and semantic search.

### Tasks

1. **Symbol index (SQLite FTS5)**
   - Schema design
   - Insertion from CodeGraph
   - FTS5 index building
   - Query interface

2. **Vector index (HNSW)**
   - Chunking strategy (symbol-aware)
   - Embedding generation (PyO3 or Candle)
   - HNSW construction
   - Search interface with filters

3. **Test map**
   - Build file_coverage and symbol_coverage maps
   - JSON serialization
   - Impact analysis queries

4. **CAS integration**
   - Store indices as content-addressed artifacts
   - Retrieval by hash
   - Registry metadata

### Gate Requirements

✅ **Symbol lookup**:
- Lookup time < 10ms (FTS5)
- Recall ≥ 95% on test queries

✅ **Vector search**:
- Search time < 100ms for k=5
- Recall@5 ≥ 90% on test queries

✅ **Test mapping**:
- Impact analysis correctness ≥ 95%
- Analysis time < 500ms

✅ **Determinism**:
- Indices hash-identical across builds

### Dependencies
- Requires S-1 (CodeGraph)

---

## Sequence S-3: Commit Delta Packs (CDP)

### Goals
Create ephemeral context for commits: diff, tests, linter output.

### Tasks

1. **CDP creation job**
   - git diff extraction
   - Changed symbol detection
   - Test runner integration (pytest, cargo test, etc.)
   - Linter integration (ruff, clippy, eslint)

2. **CDP structure**
   - JSON schema design
   - Compression (zstd)
   - Storage in CAS

3. **CDP expiry**
   - TTL enforcement
   - Cleanup job
   - Registry tracking

### Gate Requirements

✅ **CDP completeness**:
- All changed files captured
- Changed symbols detected accurately
- Test results included (if requested)

✅ **Performance**:
- CDP creation < 30s for typical commit (3-5 files)

✅ **Determinism**:
- Same commit → identical CDP (modulo timestamps)

✅ **TTL enforcement**:
- CDPs evicted after expiry
- Logged in telemetry

### Dependencies
- Requires S-1 (CodeGraph)

---

## Sequence S-4: Ephemeral Adapters

### Goals
Per-commit adapters with zero-train and micro-LoRA modes.

### Tasks

1. **Zero-train mode**
   - Router priors from CDP
   - Mini index of changed files
   - Hot-attach to worker

2. **Micro-LoRA mode**
   - Training pair generation from CDP
   - LoRA training (rank 4-8)
   - Packaging and registration

3. **TTL management**
   - Ephemeral sessions tracking
   - Auto-eviction on expiry
   - Promotion to codebase adapter (optional)

### Gate Requirements

✅ **Zero-train**:
- Priors boost correct adapters (measure activation)
- No training time

✅ **Micro-LoRA**:
- Training time < 3 minutes (rank 4, GPU)
- Targeted test pass improvement ≥ 20%

✅ **TTL enforcement**:
- Evicted after expiry
- No cross-tenant access

✅ **Hot-attach**:
- Worker accepts ephemeral without restart
- Routing uses ephemeral correctly

### Dependencies
- Requires S-1 (CodeGraph)
- Requires S-3 (CDP)
- Requires existing LoRA training infrastructure

---

## Sequence S-5: Patch Propose/Apply

### Goals
Generate and apply code patches with evidence citations.

### Tasks

1. **Patch propose**
   - Request DTO (prompt, context, targets)
   - Evidence retrieval (symbols, tests, docs)
   - Router integration (code features)
   - Response with citations

2. **Patch structure**
   - Hunk format (unified diff)
   - File-level patches
   - Rationale and citations

3. **Patch apply**
   - Dry-run in temp worktree
   - Test execution
   - Linter execution
   - Policy validation

4. **Policy enforcement**
   - Path restrictions
   - Secret detection
   - Forbidden operations
   - Size limits

### Gate Requirements

✅ **Evidence**:
- ARR ≥ 0.95 (has citations)
- ECS@5 ≥ 0.75 (coverage)

✅ **Functional**:
- Compile success ≥ 95%
- Test pass@1 ≥ 80%

✅ **Safety**:
- Secret violations = 0
- Forbidden operations = 0

✅ **Determinism**:
- Same inputs → same patch (stable generation)

### Dependencies
- Requires S-1 (CodeGraph)
- Requires S-2 (Indices)
- Requires S-4 (Ephemeral adapters, optional)
- Requires aos-codepolicy crate

---

## Sequence S-6: Router Calibration (K=3)

### Goals
Tune router for code tasks with K=3 multi-adapter mixing.

### Tasks

1. **Feature extractors**
   - lang_one_hot
   - framework_prior
   - symbol_hits
   - path_tokens
   - attn_entropy
   - commit_hint
   - prompt_verb

2. **Scoring function**
   - Per-category scoring logic
   - Framework selection (max 1)
   - Top-K with constraints

3. **Calibration**
   - Run mixed code task corpus
   - Measure activation distribution
   - Tune priors and weights

### Gate Requirements

✅ **Activation distribution**:
- No single adapter > 80% activation
- All tiers represented in K=3 selections

✅ **Routing time**:
- Feature extraction + scoring < 10ms per token

✅ **Determinism**:
- Same features → same routing decision

### Dependencies
- Requires S-1, S-2, S-4, S-5
- Requires existing router infrastructure

---

## Sequence S-7: Metrics & Promotion Gates

### Goals
Evaluation framework and automated promotion gating.

### Tasks

1. **Metric computation**
   - CSR, Test Pass@k, SAD
   - ARR, ECS@k
   - SHVR, FOR
   - Activation distribution, router overhead
   - Latency, throughput

2. **Evaluation corpus**
   - Synthetic repos per language
   - Framework scaffolds
   - Cross-language tasks
   - Edge cases (ambiguous, malicious)

3. **Audit command**
   - `aosctl code-audit`
   - Run corpus, compute metrics
   - Compare against gates
   - Generate report

4. **Promotion integration**
   - Extend `aosctl promote` with code gates
   - Block promotion if any gate fails
   - Log gate results in telemetry

### Gate Requirements

✅ **Metrics**:
- All functional metrics ≥ targets
- All groundedness metrics ≥ targets
- All safety metrics = 0
- Deterministic replay passes

✅ **Automation**:
- Audit runs without manual intervention
- Clear pass/fail output
- Actionable error messages on failure

### Dependencies
- Requires S-1 through S-6 (full stack)

---

## Sequence S-8: API & CLI (Optional MVP Extension)

### Goals
Expose code intelligence via REST API and CLI.

### Tasks

1. **API endpoints**
   - `/v1/code/register-repo`
   - `/v1/code/scan`
   - `/v1/code/commit-delta`
   - `/v1/code/ephemeral/create`
   - `/v1/code/patch/propose`
   - `/v1/code/patch/apply`
   - Security endpoints

2. **CLI commands**
   - `aosctl code-init`
   - `aosctl adapter-train`
   - `aosctl commit-ephemeral`
   - `aosctl patch-propose`
   - `aosctl patch-apply`
   - `aosctl code-audit`

3. **Documentation**
   - API reference (OpenAPI)
   - CLI help text
   - Examples

### Gate Requirements

✅ **API**:
- All endpoints respond correctly
- Error handling comprehensive
- Rate limiting enforced

✅ **CLI**:
- All commands work end-to-end
- Help text accurate
- Exit codes consistent

### Dependencies
- Requires S-1 through S-7 (full stack)
- Requires aos-codeapi crate

---

## Sequence S-9: UI Screens (Optional)

### Goals
Rust/WASM UI for code intelligence.

### Tasks

1. **Repository Setup screen**
2. **Adapters View screen**
3. **Commit Inspector screen**
4. **Routing Inspector screen**
5. **Patch Lab screen**
6. **Policy Editor screen**
7. **Metrics Dashboard screen**

### Gate Requirements

✅ **Functionality**:
- All screens render correctly
- API integration works
- State management stable

✅ **UX**:
- Responsive layout
- Clear error messages
- Loading states

### Dependencies
- Requires S-8 (API)
- Requires existing aos-ui-web infrastructure

---

## Testing Strategy Per Phase

### Unit Tests
- Every function in aos-codegraph, aos-codepolicy, aos-codejobs
- Target: >80% code coverage

### Integration Tests
- End-to-end flows (scan → index → propose → apply)
- Target: All happy paths + major error paths

### Determinism Tests
- Replay bundles on two nodes
- Compare hashes
- Target: Zero diff

### Performance Tests
- Benchmark key operations (parse, search, route)
- Compare against targets
- Target: All targets met

### Adversarial Tests
- Malicious inputs (injection attempts)
- Cross-tenant isolation
- Target: All attacks blocked

---

## Rollout Strategy

### Phase 1: Internal Testing
- Deploy to single dev tenant
- Limited repos (1-2)
- Manual invocation

### Phase 2: Alpha
- Deploy to select tenants
- Limited automation (manual patch review)
- Collect metrics

### Phase 3: Beta
- Deploy to more tenants
- Enable auto-apply (with strict policies)
- Monitor regression rates

### Phase 4: General Availability
- Deploy to all tenants (opt-in)
- Full automation available
- Continuous monitoring

---

## Rollback Plan

### Trigger Conditions
- Regression rate > 5%
- Secret violations detected
- Determinism failure
- Performance degradation > 20%

### Rollback Procedure
1. Disable code CP pointer
2. Revert to previous CP
3. Export incident bundle
4. Investigate root cause
5. Fix and re-audit before re-promotion

---

## Estimated Complexity

| Sequence | LOC (new) | Test LOC | Complexity | Risk    |
|----------|-----------|----------|------------|---------|
| S-1      | ~2500     | ~800     | High       | Medium  |
| S-2      | ~1500     | ~600     | Medium     | Low     |
| S-3      | ~800      | ~300     | Medium     | Low     |
| S-4      | ~1200     | ~400     | High       | Medium  |
| S-5      | ~1500     | ~700     | High       | High    |
| S-6      | ~600      | ~200     | Medium     | Medium  |
| S-7      | ~1000     | ~400     | Medium     | Low     |
| S-8      | ~1500     | ~500     | Low        | Low     |
| S-9      | ~2000     | ~400     | Medium     | Low     |
| **Total**| **~12.6K**| **~4.3K**|            |         |

---

## Success Criteria

Code intelligence is **production-ready** when:

1. ✅ All S-1 through S-7 gates passed
2. ✅ Evaluation corpus pass rate ≥ 95%
3. ✅ Deterministic replay: zero diff
4. ✅ Safety violations: zero
5. ✅ Alpha deployment: no critical incidents
6. ✅ Beta metrics: regression rate < 3%

At that point, AdapterOS v0.1 Alpha is ready for opt-in general availability.
