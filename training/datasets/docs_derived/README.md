# Documentation-Derived Training Data

**Purpose:** Train adapters on canonical adapterOS patterns from AGENTS.md and architecture docs

## Overview

Documentation-derived training data captures the authoritative patterns, policies, and conventions defined in project documentation. This ensures adapters learn canonical implementations.

## Key Concepts

- **AGENTS.md:** Canonical developer reference
- **Policy Packs:** 23 enforced policies
- **Architecture Patterns:** Core design patterns
- **RBAC Matrix:** 5 roles, 20+ permissions
- **Error Handling:** AosError variants
- **Configuration:** Precedence rules

## Training Example Schema

```jsonl
{
  "input": {
    "topic": "error_handling",
    "context": "Loading adapter from disk",
    "doc_source": "AGENTS.md#error-handling"
  },
  "target": {
    "pattern": "use adapteros_core::{AosError, Result};\n\npub async fn load(&self, path: &Path) -> Result<Data> {\n    std::fs::read(path).map_err(|e| match e.kind() {\n        std::io::ErrorKind::NotFound => AosError::NotFound(format!(\"File not found: {}\", path.display())),\n        _ => AosError::Io(format!(\"Failed to read {}: {}\", path.display(), e))\n    })?\n}",
    "policy_refs": ["error_handling", "io_operations"],
    "quality_score": 0.95
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive",
    "doc_version": "2025-11-18"
  }
}
```

## Documentation Sources

### 1. AGENTS.md Sections
- **Standards & Conventions:** Code style, naming, logging
- **Policy Packs:** 23 canonical policies
- **RBAC:** Permission matrix, audit logging
- **Architecture Patterns:** State machines, hot-swap, HKDF
- **Database Schema:** Migration patterns
- **API Endpoints:** REST contracts

### 2. Architecture Docs
- **ARCHITECTURE_INDEX.md:** System overview
- **DEPRECATED_PATTERNS.md:** Anti-patterns
- **STACK_VERSIONING.md:** Stack lifecycle

### 3. Migration Files
- `migrations/*.sql` - Schema patterns
- `migrations/signatures.json` - Verification

## Quality Criteria

- **Min Examples:** 500
- **Min Relevance:** 0.90
- **Min Confidence:** 0.90
- **Doc Coverage:** >70% of AGENTS.md sections

## Example Datasets

- `error_handling/` - AosError patterns
- `logging_patterns/` - tracing! macros
- `policy_examples/` - Policy enforcement
- `rbac_patterns/` - Permission checks
- `config_precedence/` - CLI > Env > File
- `migration_patterns/` - Schema evolution
- `api_contracts/` - REST endpoint signatures

## Extraction Pipeline

```bash
# 1. Parse documentation
cat AGENTS.md | extract_code_blocks > patterns.jsonl

# 2. Validate against codebase
cargo test --doc

# 3. Quality check
check_doc_coverage AGENTS.md crates/

# 4. Generate training data
generate_training_from_docs patterns.jsonl > dataset.jsonl
```

## Quality Checks

1. **Code Validity:** All examples must compile
2. **Policy Compliance:** No anti-patterns
3. **Version Sync:** Docs match codebase
4. **Test Coverage:** Examples have tests

## References

- `AGENTS.md` - Canonical reference
- `ARCHITECTURE_INDEX.md` - System overview
- `docs/` - All documentation
- `migrations/` - Schema patterns
- `crates/adapteros-policy/src/packs/` - Policy implementations
