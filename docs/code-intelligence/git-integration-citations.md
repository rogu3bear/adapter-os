# Git Integration Citations and Evidence Requirements

**Version:** 1.0.0  
**Last Updated:** 2025-01-27  
**Status:** Implementation Guide

---

## Overview

This document establishes the citation format and evidence requirements for git repository integration in AdapterOS, following the evidence-first philosophy established in the codebase.

## Evidence Requirements

### Primary Evidence Sources

**Evidence**: `docs/code-intelligence/code-policies.md:45-78`  
**Policy**: Evidence requirements for code suggestions

Every git repository operation must cite at least one evidence span:

```rust
// Evidence: docs/code-intelligence/code-policies.md:45-78
// Policy: Evidence requirements for code suggestions
pub fn validate_evidence_requirements(
    evidence: &[EvidenceSpan],
    policy: &CodePolicy
) -> Result<()> {
    let evidence_count = evidence.len();
    
    if evidence_count < policy.evidence_min_spans {
        return Err(PolicyViolation::InsufficientEvidence {
            required: policy.evidence_min_spans,
            provided: evidence_count,
        });
    }
    
    Ok(())
}
```

### Evidence Types for Git Operations

**Evidence**: `docs/code-intelligence/code-intelligence-architecture.md:1-22`  
**Pattern**: Code intelligence stack architecture

1. **Repository Analysis Evidence**
   - Git metadata (commits, branches, authors)
   - File structure and language detection
   - Framework identification

2. **Training Evidence**
   - Code patterns and conventions
   - Test coverage and quality metrics
   - Documentation and API usage

3. **Security Evidence**
   - Path validation and sanitization
   - Secret detection and prevention
   - Access control verification

## Citation Format

### Standard Citation Structure

**Evidence**: `docs/llm-interface-specification.md:1020-1043`  
**Pattern**: Citation format specification

```typescript
interface Citation {
  // Source identification
  docId: string;
  revision: string;
  spanId: string;
  
  // Citation metadata
  citationType: 'direct' | 'paraphrase' | 'synthesis';
  confidence: number;
  
  // Text anchoring
  generatedText: {
    start: number;
    end: number;
  };
  
  // Provenance
  spanHash: string;
  retrievalTimestamp: Date;
}
```

### Implementation Citations

**Evidence**: `crates/mplora-worker/src/evidence.rs:304-384`  
**Pattern**: Evidence policy implementation

```rust
// Evidence: crates/mplora-worker/src/evidence.rs:304-384
// Policy: Evidence policy with retrieval constraints
#[derive(Debug, Clone)]
pub struct EvidencePolicy {
    pub min_spans: usize,
    pub min_sources: usize,
    pub min_avg_score: f32,
    pub max_retrieval_time_ms: u64,
}
```

## Policy Compliance

### Security Requirements

**Evidence**: `docs/code-intelligence/code-policies.md:82-84`  
**Policy**: Patch safety and path restrictions

```json
{
  "code": {
    "path_allowlist": ["src/**", "lib/**", "tests/**"],
    "path_denylist": ["**/.env*", "**/secrets/**", "**/*.pem"],
    "secret_patterns": [
      "(?i)(api[_-]?key|password|secret|token)\\s*[:=]\\s*['\"][^'\"]{8,}['\"]",
      "(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)",
      "-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----"
    ]
  }
}
```

### Performance Requirements

**Evidence**: `docs/llm-interface-specification.md:42-47`  
**Policy**: Deterministic behavior and bounded operations

```rust
// Evidence: docs/llm-interface-specification.md:42-47
// Policy: Determinism first, zero egress, policy enforcement
pub fn enforce_deterministic_behavior(
    operation: &GitOperation,
    context: &PolicyContext
) -> Result<()> {
    // Ensure reproducible results given same inputs
    // No network access during inference
    // All operations subject to tenant-specific policy gates
    Ok(())
}
```

## Implementation Standards

### Code Quality Requirements

**Evidence**: `CONTRIBUTING.md:14-18`  
**Policy**: Code standards and quality gates

- Follow Rust standard style (`cargo fmt`)
- Pass all clippy lints (`cargo clippy`)
- Add tests for new functionality
- Document public APIs
- Keep unsafe code in designated crates only

### Documentation Standards

**Evidence**: `CONTRIBUTING.md:35-58`  
**Policy**: Documentation structure and validation

- Follow established patterns in `docs/database-schema/`
- Use Mermaid diagrams for workflow animations
- Ensure all cross-references are accurate
- Validate syntax before submitting changes

## Validation Checklist

### Evidence Validation

- [ ] Minimum 1 evidence span per factual claim
- [ ] All citations include source identification
- [ ] Confidence scores meet policy thresholds
- [ ] Retrieval timestamps are recorded

### Security Validation

- [ ] Path restrictions enforced
- [ ] Secret patterns detected and blocked
- [ ] Access control verified
- [ ] Input sanitization applied

### Performance Validation

- [ ] Deterministic behavior maintained
- [ ] Bounded operation times
- [ ] Memory usage within limits
- [ ] No network egress during inference

## References

- [Code Intelligence Architecture](code-intelligence-architecture.md)
- [Code Policies](code-policies.md)
- [LLM Interface Specification](../../llm-interface-specification.md)
- [Patch Proposal System](../../patch-proposal-system.md)
- [Contributing Guidelines](../../CONTRIBUTING.md)
