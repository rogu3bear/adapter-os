# Code Intelligence Architecture

## Overview

AdapterOS's code intelligence stack transforms the system into a **local, deterministic, auditable codebase API** that understands repositories, frameworks, and diffs. Every suggestion is tied to evidence: files, symbols, tests, and framework docs, or it refuses.

**Goal**: A local system that proposes grounded patches and explanations without external data exfiltration, maintaining all existing AdapterOS guarantees (determinism, zero egress, multi-tenant isolation, audit trails).

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Developer Interface                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   aosctl     │  │   aos-ui     │  │  Git Hooks   │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
└─────────┼──────────────────┼──────────────────┼──────────────────┘
          │                  │                  │
          ▼                  ▼                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Control Plane (aos-cp)                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Code API Layer (aos-codeapi)                            │  │
│  │  - register-repo  - scan  - commit-delta                 │  │
│  │  - ephemeral ops  - patch propose/apply                  │  │
│  └──────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Code Jobs (aos-codejobs)                                │  │
│  │  - Scan & Index  - CDP Creation  - Ephemeral Training    │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
          │                                              │
          ▼                                              ▼
┌────────────────────────┐              ┌────────────────────────┐
│   CodeGraph System     │              │   Adapter Registry     │
│  ┌──────────────────┐  │              │  ┌──────────────────┐  │
│  │  Tree-sitter     │  │              │  │  Extended Schema │  │
│  │  Parsers         │  │              │  │  + Code Metadata │  │
│  └──────────────────┘  │              │  └──────────────────┘  │
│  ┌──────────────────┐  │              └────────────────────────┘
│  │  Symbol Index    │  │
│  │  (SQLite + FTS5) │  │
│  └──────────────────┘  │              ┌────────────────────────┐
│  ┌──────────────────┐  │              │   Inference Worker     │
│  │  Vector Index    │  │              │  ┌──────────────────┐  │
│  │  (Per-repo)      │  │              │  │  Router + Code   │  │
│  └──────────────────┘  │              │  │  Features        │  │
│  ┌──────────────────┐  │              │  └──────────────────┘  │
│  │  Test Map        │  │              │  ┌──────────────────┐  │
│  └──────────────────┘  │              │  │  K-Sparse Mixer  │  │
└────────────────────────┘              │  │  (base+code+     │  │
                                        │  │   framework+     │  │
                                        │  │   codebase+      │  │
                                        │  │   ephemeral)     │  │
                                        │  └──────────────────┘  │
                                        └────────────────────────┘
```

## Five-Tier Adapter Hierarchy

The code intelligence stack introduces a semantic layer hierarchy:

```
┌────────────────────────────────────────────────────────────────┐
│  Layer 5: Ephemeral (per-commit, TTL-bound)                    │
│  ├─ commit_abc123 (rank 4-8, TTL 24-72h)                       │
│  └─ Purpose: Fresh symbols, renamed params, recent decisions   │
├────────────────────────────────────────────────────────────────┤
│  Layer 4: Codebase (tenant-specific, repo-bound)               │
│  ├─ codebase_myrepo_v3 (rank 16-32)                            │
│  └─ Purpose: Internal APIs, conventions, ADRs, house style     │
├────────────────────────────────────────────────────────────────┤
│  Layer 3: Frameworks (type-specific, stack-bound)              │
│  ├─ framework_django_v1 (rank 8-16)                            │
│  ├─ framework_react_v2 (rank 8-16)                             │
│  └─ Purpose: Framework APIs, idioms, gotchas                   │
├────────────────────────────────────────────────────────────────┤
│  Layer 2: Code (domain-general coding knowledge)               │
│  ├─ code_lang_v1 (rank 16)                                     │
│  └─ Purpose: Language reasoning, patterns, refactoring         │
├────────────────────────────────────────────────────────────────┤
│  Layer 1: Base (general language model)                        │
│  └─ Qwen2.5-7B-Instruct or similar (int4)                      │
└────────────────────────────────────────────────────────────────┘
```

### Semantic Classification

- **Domain**: General-purpose knowledge (code, codebase)
- **Type**: Specific category/framework knowledge (frameworks)
- **Ephemeral**: Temporary, session-scoped (ephemeral)

## Hybrid Tier Model

To maintain backward compatibility while adding code intelligence semantics, we extend the existing `persistent`/`ephemeral` taxonomy with metadata:

**Existing Schema** (preserved):
- `tier`: `"persistent"` | `"ephemeral"` (storage/lifecycle)

**New Metadata** (additive):
- `category`: `"base"` | `"code"` | `"framework"` | `"codebase"` | `"ephemeral"` (semantic)
- `scope`: `"global"` | `"tenant"` | `"repo"` | `"commit"` (boundary)
- `framework_id`: Optional framework identifier
- `repo_id`: Optional repository identifier
- `commit_sha`: Optional commit hash

This allows the router to use semantic categories while preserving existing storage and lifecycle policies.

## Data Flow

### 1. Repository Onboarding

```
Developer                  Control Plane              CodeGraph System
    │                            │                           │
    ├─ aosctl code-init ────────>│                           │
    │  (repo path, langs)        │                           │
    │                            ├─ Create job ─────────────>│
    │                            │                           │
    │                            │                           ├─ Parse (tree-sitter)
    │                            │                           ├─ Extract symbols
    │                            │                           ├─ Build graph
    │                            │                           ├─ Detect frameworks
    │                            │                           ├─ Map tests
    │                            │                           ├─ Chunk & embed
    │                            │                           ├─ BLAKE3 hash all
    │                            │                           │
    │                            │<─ Artifacts (CAS) ────────┤
    │                            ├─ Register in registry
    │<─ Job complete ────────────┤
    │   (graph_id, indices)      │
```

### 2. Codebase Adapter Training

```
Developer                  Control Plane              Training Pipeline
    │                            │                           │
    ├─ aosctl adapter-train ────>│                           │
    │  (repo, rank)              │                           │
    │                            ├─ Load graph & docs ──────>│
    │                            │                           │
    │                            │                           ├─ Generate pairs
    │                            │                           │   (READMEs, ADRs,
    │                            │                           │    code patterns)
    │                            │                           ├─ Train LoRA
    │                            │                           ├─ Validate
    │                            │                           ├─ Package bundle
    │                            │                           │
    │                            │<─ Signed bundle ──────────┤
    │                            ├─ Import (aos-artifacts)
    │                            ├─ Register adapter
    │<─ Adapter ID ──────────────┤
```

### 3. Commit Flow (Ephemeral Adapter)

```
Git Hook                   Control Plane              Ephemeral System
    │                            │                           │
    ├─ POST commit-delta ───────>│                           │
    │  (repo, commit)            │                           │
    │                            ├─ git diff ────────────────>│
    │                            │                           │
    │                            │                           ├─ Changed files
    │                            │                           ├─ Symbol delta
    │                            │                           ├─ Run tests
    │                            │                           ├─ Run linter
    │                            │                           ├─ Capture logs
    │                            │                           ├─ Create CDP
    │                            │                           │
    │                            │<─ CDP (hashed) ───────────┤
    │                            ├─ Optional: train micro-LoRA
    │                            │   (rank 4, TTL=72h)
    │<─ Ephemeral ID ────────────┤
    │   (auto-attach to worker)  │
```

### 4. Code Request (with Routing)

```
Request                    Worker                     Router                  Response
    │                        │                            │                       │
    ├─ "Fix failing test" ──>│                            │                       │
    │  + context (file)      │                            │                       │
    │                        ├─ Retrieve evidence ───────>│                       │
    │                        │  (symbol index,            │                       │
    │                        │   code chunks,             │                       │
    │                        │   test logs)               │                       │
    │                        │                            │                       │
    │                        ├─ Compute features ────────>│                       │
    │                        │  (lang, framework,         │                       │
    │                        │   symbol_hits, etc.)       │                       │
    │                        │                            │                       │
    │                        │                            ├─ Score adapters
    │                        │                            ├─ Top-K (K=3)
    │                        │                            │   [codebase,
    │                        │                            │    framework_pytest,
    │                        │                            │    code]
    │                        │                            │
    │                        │<─ Decision ────────────────┤
    │                        │  (indices, gates_q15)      │
    │                        │                            │
    │                        ├─ Run kernels (fused path)
    │                        ├─ Generate with evidence
    │                        ├─ Policy check
    │                        │  (min spans, safety)
    │                        │                            │
    │<─ Response ────────────┤                            │
    │  (patch + citations)   │                            │
```

## Integration Points

### With Existing Crates

1. **aos-registry**: Extended schema stores code metadata (repos, frameworks, symbols)
2. **aos-artifacts**: CAS stores CodeGraphs, indices, CDPs as content-addressed artifacts
3. **aos-worker**: Injects code-specific router features, consumes code evidence
4. **aos-router**: New feature scorers for code tasks
5. **aos-policy**: Enforces code-specific policies (path allowlist, evidence requirements)
6. **aos-rag**: Retrieves from code indices (symbols, chunks, tests)
7. **aos-telemetry**: Logs code events (scan, train, patch apply, adapter activation)

### New Crates

1. **aos-codegraph**: Tree-sitter wrapper, graph building, symbol extraction
2. **aos-codepolicy**: Code-specific policy validation, patch safety
3. **aos-codejobs**: Background jobs for scanning, indexing, CDP creation
4. **aos-codeapi**: DTOs, endpoints, response builders for code features

## Determinism Guarantees

All existing determinism requirements apply to code intelligence:

1. **CodeGraph hashing**: `b3(repo_path || commit || parsed_content)` is stable
2. **Symbol index ordering**: Deterministic sorting by (file, line, symbol)
3. **Framework detection**: Rule-based (no ML), version-pinned
4. **Patch serialization**: Canonical JSON, sorted hunks by (file, line)
5. **Tool logs**: Normalized timestamps/paths for replay
6. **Ephemeral training**: Seeded from global seed + commit hash

Two nodes with identical inputs produce identical CodeGraphs, indices, and patches.

## Security & Isolation

1. **Per-tenant indices**: No cross-tenant symbol leakage
2. **Repo permissions**: Path allowlist/denylist enforced before patch apply
3. **Ephemeral TTL**: Strict expiry, logged eviction, no resurrection
4. **No external deps**: Code suggestions cannot add dependencies without policy approval
5. **Secret detection**: Refuse patches that access env vars matching secret patterns
6. **CAS integrity**: All artifacts BLAKE3-hashed and optionally signed

## Evidence-First Philosophy

Every code suggestion **must** cite at least one of:
- Code span from symbol index
- Test log span
- Framework documentation span
- Internal API documentation

If evidence is insufficient, the system returns structured refusal:
```json
{
  "status": "insufficient_evidence",
  "needed": ["file_path", "symbol", "test_target"]
}
```

No hallucinated APIs. No invented function names.

## Memory Budgeting

Target configuration for M1/M2/M3 with 32-64 GB unified memory:

- Base 7B int4: ~4-5 GB
- Code adapter r16: ~75-95 MB
- Framework adapters r12 (2-3 resident): ~110-210 MB
- Codebase adapter r24: ~110-140 MB
- Ephemeral r4 (multiple): ~20-30 MB each
- KV cache (4k context): ~2 GB
- Symbol + vector indices: ~500 MB per repo

Total: ~8-10 GB with headroom for multiple active repos.

## Performance Targets

- **Symbol lookup**: <50ms (SQLite FTS5)
- **Vector retrieval**: <100ms (HNSW, 5 neighbors)
- **Router decision**: <10ms per token
- **Patch propose**: <2s (includes retrieval + generation)
- **Test impact analysis**: <500ms (precomputed map)

## Success Criteria

A code CP promotes only if:

1. **Compile success rate** ≥ 95% on eval corpus
2. **Test pass@k** improvement on targeted tests
3. **ARR** ≥ 0.95 (has evidence)
4. **ECS@5** ≥ 0.75 (coverage)
5. **No secret violations** (zero tolerance)
6. **Deterministic replay** (zero diff on two nodes)
7. **Framework adapter balance** (no single adapter >80% activation)

## Next Steps

See individual specification documents for detailed design:
- [Tier System Design](code-intelligence-tiers.md)
- [CodeGraph Specification](../codegraph-spec.md)
- [Manifest V4 Extensions](code-manifest-v4.md)
- [Router Features](code-router-features.md)
- [API Specifications](code-api-registry.md)
- [Implementation Roadmap](code-implementation-roadmap.md)
