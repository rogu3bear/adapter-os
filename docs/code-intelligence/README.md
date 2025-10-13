# Code Intelligence Stack - Documentation Index

## Overview

Complete architectural specification for AdapterOS code intelligence: **base → code → frameworks → codebase → ephemeral-per-commit**.

This is a design/documentation phase providing the complete blueprint for implementing a local, deterministic, auditable codebase API.

---

## Core Architecture

### [code-intelligence-architecture.md](code-intelligence-architecture.md)
**Main architecture document**
- System overview and goals
- Five-tier adapter hierarchy
- Hybrid tier model (backward compatible)
- Data flow diagrams
- Integration points
- Evidence-first philosophy
- Memory budgeting
- Success criteria

### [code-intelligence-tiers.md](code-intelligence-tiers.md)
**Tier system design**
- Detailed tier definitions (Base, Code, Framework, Codebase, Ephemeral)
- Training data per tier
- Acceptance gates per tier
- Routing strategy (K=3)
- Memory management
- Lifecycle policies

---

## Data & Indexing

### [codegraph-spec.md](codegraph-spec.md)
**CodeGraph specification**
- Tree-sitter integration architecture
- Graph schema (nodes, edges)
- Symbol extraction per language
- Framework detection rules
- Test mapping
- Deterministic serialization
- CAS storage integration

### [code-indices.md](code-indices.md)
**Index formats and querying**
- Symbol index (SQLite FTS5)
- Vector index (HNSW)
- Test map (JSON)
- Query patterns
- Performance targets
- Per-tenant isolation

### [code-ingestion-pipeline.md](code-ingestion-pipeline.md)
**Data ingestion flow**
- 7-stage pipeline (scan → parse → graph → indices → package)
- Language-specific parsing
- Framework detection algorithms
- Chunking strategies
- Performance targets
- Determinism guarantees

---

## Schemas & Configuration

### [code-manifest-v4.md](code-manifest-v4.md)
**Manifest V4 extensions**
- Backward-compatible schema
- New adapter metadata (category, scope, framework_id, repo_id, commit_sha)
- Router feature configuration
- Code-specific policy packs
- Three complete manifest examples
- Migration from V3

### [code-registry-schema.md](code-registry-schema.md)
**Database schema extensions**
- New tables (repositories, code_graphs, symbol_indices, vector_indices, test_maps, commit_delta_packs, ephemeral_sessions)
- Extended adapters table
- Migration SQL (0002_code_intelligence.sql)
- Query examples
- Indexing strategy
- Storage estimates

---

## Routing & Features

### [code-router-features.md](code-router-features.md)
**Code-specific router features**
- Feature vector definition (9 features)
- Extraction algorithms (lang_one_hot, framework_prior, symbol_hits, path_tokens, attn_entropy, commit_hint, prompt_verb, retrieval_quality)
- Scoring function
- Top-K selection with constraints
- Example routing decisions
- Abstention rules
- Performance targets

---

## API Specifications

### [code-api-registry.md](code-api-registry.md)
**Registry & scanning APIs**
- `POST /v1/code/register-repo`
- `POST /v1/code/scan`
- `GET /v1/code/graph/{repo_id}@{commit}`
- Symbol search, semantic search
- Test impact analysis
- Framework detection
- Error codes
- Rate limiting

### [code-api-ephemeral.md](code-api-ephemeral.md)
**Ephemeral & patch APIs**
- `POST /v1/code/commit-delta` (CDP creation)
- `POST /v1/code/ephemeral/create`
- `POST /v1/code/patch/propose`
- `POST /v1/code/patch/apply`
- Response schemas with evidence citations
- Refusal responses
- Audit logging

### [code-api-security.md](code-api-security.md)
**Security & policy APIs**
- Path permission management
- Policy configuration
- Secret detection
- Patch validation
- Dependency validation
- Audit logging
- Incident reporting

---

## Implementation

### [code-crates.md](code-crates.md)
**Crate structure**
- Four new crates:
  - `aos-codegraph`: Parsing, graph building, symbol extraction
  - `aos-codepolicy`: Code-specific policy validation
  - `aos-codejobs`: Background jobs (scan, CDP, training)
  - `aos-codeapi`: DTOs, handlers, response builders
- Public APIs per crate
- Integration with existing crates
- Build & test procedures
- Size estimates

### [code-dependencies.md](code-dependencies.md)
**Dependency specification**
- Tree-sitter + language grammars
- SQLite FTS5, HNSW
- Embedding models (PyO3 or Candle)
- Version pinning strategy
- Cargo features
- Platform support
- Licensing
- Security considerations
- Performance targets

### [code-implementation-roadmap.md](code-implementation-roadmap.md)
**Phased implementation plan**
- 9 sequences (S-1 through S-9)
- Gate requirements per sequence
- Dependencies between sequences
- Testing strategy per phase
- Rollout strategy (internal → alpha → beta → GA)
- Rollback plan
- Estimated complexity (~12.6K LOC)
- Success criteria

---

## Policy & Safety

### [code-policies.md](code-policies.md)
**Code-specific policy packs**
- Evidence requirements (min spans)
- Patch safety (path restrictions)
- Secret detection (regex patterns)
- Forbidden operations (eval, exec, shell escape)
- Auto-apply gates (test coverage)
- Patch size limits
- Dependency policy
- Review requirements
- Enforcement flow
- Telemetry
- Configuration examples (permissive vs strict)

---

## Evaluation & Quality

### [code-evaluation.md](code-evaluation.md)
**Evaluation framework**
- 13 metrics:
  - Functional: CSR, Test Pass@k, SAD
  - Groundedness: ARR, ECS@k
  - Safety: SHVR, FOR
  - Routing: activation distribution, router overhead
  - Performance: latency, throughput
  - Regression: FFR, RIR
- Test corpus structure (synthetic repos, framework scaffolds, edge cases)
- Promotion gates (all metrics must pass)
- Running evaluation (`aosctl code-audit`)
- Continuous monitoring

---

## User Interfaces

### [code-ui-screens.md](code-ui-screens.md)
**UI specifications**
- 7 screens (all Rust/WASM):
  1. Repository Setup
  2. Adapters View (with activation heatmaps)
  3. Commit Inspector
  4. Routing Inspector
  5. Patch Lab (propose/review/apply)
  6. Policy Editor
  7. Metrics Dashboard
- State management
- API integration via `aos-cp-client`
- Styling conventions

### [code-cli-commands.md](code-cli-commands.md)
**CLI extensions**
- Repository: `code-init`, `code-update`, `code-list`
- Training: `adapter-train`
- Ephemeral: `commit-ephemeral`, `hot-reload-ephemeral`
- Patch: `patch-propose`, `patch-apply`
- Audit: `code-audit`
- Utilities: `code-search`, `code-stats`
- Complete examples with flags
- Exit codes

### Database Schema

For database structure related to code intelligence features:

- [Code Intelligence Workflow](../database-schema/workflows/code-intelligence.md) - Animated workflow showing repository analysis, commit tracking, and ephemeral adapter generation
- [Schema Diagram](../database-schema/schema-diagram.md) - Complete ER diagram including `repositories`, `commits`, `patch_proposals`, and `ephemeral_adapters` tables

**Key Database Tables**:
- `repositories` - Registered code repositories with language detection
- `commits` - Commit metadata and analysis with symbol tracking
- `patch_proposals` - AI-generated code patches with validation
- `ephemeral_adapters` - Commit-aware temporary adapters
- `adapters` - Includes ephemeral category adapters linked to commits

See [database-schema documentation](../database-schema/README.md) for complete details.

---

## Quick Navigation

### For Architects
1. Start: [code-intelligence-architecture.md](code-intelligence-architecture.md)
2. Then: [code-intelligence-tiers.md](code-intelligence-tiers.md)
3. Then: [codegraph-spec.md](codegraph-spec.md)

### For Backend Developers
1. Start: [code-crates.md](code-crates.md)
2. Then: [code-api-registry.md](code-api-registry.md), [code-api-ephemeral.md](code-api-ephemeral.md)
3. Then: [code-implementation-roadmap.md](code-implementation-roadmap.md)

### For DevOps/Operations
1. Start: [code-cli-commands.md](code-cli-commands.md)
2. Then: [code-policies.md](code-policies.md)
3. Then: [code-evaluation.md](code-evaluation.md)

### For Frontend Developers
1. Start: [code-ui-screens.md](code-ui-screens.md)
2. Then: [code-api-registry.md](code-api-registry.md)

### For Security/Compliance
1. Start: [code-policies.md](code-policies.md)
2. Then: [code-api-security.md](code-api-security.md)
3. Then: [code-evaluation.md](code-evaluation.md) (safety metrics)

---

## Document Status

All documents complete as of 2025-10-05:

| Document                          | Status | Pages | Schemas | Examples |
|-----------------------------------|--------|-------|---------|----------|
| code-intelligence-architecture.md | ✅     | ~15   | 2       | 5        |
| code-intelligence-tiers.md        | ✅     | ~18   | 5       | 8        |
| codegraph-spec.md                 | ✅     | ~22   | 8       | 12       |
| code-manifest-v4.md               | ✅     | ~16   | 1       | 3        |
| code-registry-schema.md           | ✅     | ~14   | 10      | 8        |
| code-router-features.md           | ✅     | ~20   | 8       | 6        |
| code-indices.md                   | ✅     | ~14   | 3       | 8        |
| code-ingestion-pipeline.md        | ✅     | ~18   | 0       | 15       |
| code-api-registry.md              | ✅     | ~12   | 10      | 10       |
| code-api-ephemeral.md             | ✅     | ~16   | 12      | 12       |
| code-api-security.md              | ✅     | ~14   | 8       | 8        |
| code-crates.md                    | ✅     | ~12   | 4       | 6        |
| code-policies.md                  | ✅     | ~18   | 2       | 4        |
| code-evaluation.md                | ✅     | ~16   | 1       | 10       |
| code-ui-screens.md                | ✅     | ~15   | 7       | 7        |
| code-cli-commands.md              | ✅     | ~14   | 0       | 20       |
| code-dependencies.md              | ✅     | ~10   | 0       | 5        |
| code-implementation-roadmap.md    | ✅     | ~16   | 0       | 1        |
| **Total**                         | ✅     | **270+** | **81** | **148**  |

---

## Next Steps

This documentation provides the complete blueprint. Implementation proceeds in phases:

1. **Review & approve architecture** (this phase)
2. **Implement S-1**: CodeGraph & indexers
3. **Implement S-2**: Symbol & vector indices
4. **Implement S-3**: Commit delta packs
5. **Implement S-4**: Ephemeral adapters
6. **Implement S-5**: Patch propose/apply
7. **Implement S-6**: Router calibration
8. **Implement S-7**: Metrics & promotion gates
9. **(Optional) S-8**: API & CLI
10. **(Optional) S-9**: UI screens

Each phase has clear gate requirements and acceptance criteria documented in [code-implementation-roadmap.md](code-implementation-roadmap.md).

---

## Contributing

When implementing:
1. Reference the relevant specification document
2. Follow schemas and APIs exactly
3. Meet all gate requirements before moving to next sequence
4. Run determinism tests after each change
5. Update docs if behavior deviates from spec

---

## License

This documentation is part of AdapterOS and follows the dual MIT/Apache-2.0 license.

---

**AdapterOS Code Intelligence**: Local, deterministic, auditable code assistance without third-party data exfiltration.
