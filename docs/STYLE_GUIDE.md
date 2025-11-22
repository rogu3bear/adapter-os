# AdapterOS Documentation Style Guide

**Version**: 1.0
**Last Updated**: 2025-11-22
**Author/Maintainer**: AdapterOS Documentation Team
**Status**: Active
**Related**: [CLAUDE.md](../CLAUDE.md), [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md)

---

## Overview

This guide standardizes documentation format across the AdapterOS codebase. All new documentation must follow these conventions to ensure consistency, discoverability, and maintainability.

---

## File Naming

**Pattern**: `PURPOSE_COMPONENT.md`

Examples:
- `GUIDE_MEMORY_POOL.md` (tutorial/how-to)
- `REFERENCE_STREAMING_API.md` (API reference)
- `PROOF_MLX_FFI_INTEGRATION.md` (proof/verification doc)
- `CHECKLIST_IMPLEMENTATION.md` (checklist)
- `STATUS_RELEASE_2025_11_22.md` (status report)
- `ARCHITECTURE_DETERMINISM.md` (architecture decision)
- `README.md` (overview for crate/directory)

**Conventions**:
- Use SCREAMING_SNAKE_CASE
- Primary component first, purpose second
- Special: Archive docs use `ORIGINAL_NAME.md` (don't rename)

**Prefixes by Document Type**:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `GUIDE_` | Tutorials, how-to guides | `GUIDE_TRAINING_PIPELINE.md` |
| `REFERENCE_` | API documentation, specs | `REFERENCE_REST_API.md` |
| `ARCHITECTURE_` | Design decisions, patterns | `ARCHITECTURE_MULTI_BACKEND.md` |
| `PROOF_` | Verification, integration proofs | `PROOF_DETERMINISM.md` |
| `CHECKLIST_` | Implementation checklists | `CHECKLIST_RELEASE.md` |
| `STATUS_` | Status reports, changelogs | `STATUS_ALPHA_2025_11.md` |
| `RUNBOOK_` | Operational procedures | `RUNBOOK_INCIDENT_RESPONSE.md` |
| `ADR_` | Architecture Decision Records | `ADR_COREML_STRATEGY.md` |

---

## Metadata

**Required at top of every doc**:

```markdown
# Title

**Version**: X.Y
**Last Updated**: YYYY-MM-DD
**Author/Maintainer**: Team or person name
**Status**: Active / Deprecated / Archived
**Related**: [Link to related doc](path/to/doc.md)

---

## Overview
...
```

**Full Example**:

```markdown
# Memory Pool Quick Start

**Version**: 2.1
**Last Updated**: 2025-11-22
**Author/Maintainer**: Memory Team
**Status**: Active
**Related**: [GUIDE_K_REDUCTION.md](GUIDE_K_REDUCTION.md), [ARCHITECTURE_PATTERNS.md#memory](ARCHITECTURE_PATTERNS.md#memory)

---

## Overview

This guide explains how to configure and use the unified memory pool for adapter management.
```

**Status Values**:
- `Active` - Current, maintained documentation
- `Deprecated` - Superseded, kept for backward compatibility
- `Archived` - Historical reference only, moved to `/docs/archive/`
- `Draft` - Work in progress, not yet reviewed

---

## Headers

- `# Title` - Document title (exactly 1 per file)
- `## Section` - Major sections
- `### Subsection` - Subheadings
- `#### Sub-subsection` - Use sparingly, prefer flatter hierarchy

**Rules**:
- Use sentence case for headers: `## Memory management` not `## Memory Management`
- Exception: Proper nouns and acronyms: `## CoreML Integration`
- Use markdown TOC for docs >1000 lines
- No trailing punctuation in headers

**Table of Contents** (for long documents):

```markdown
## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Configuration](#configuration)
  - [Basic Setup](#basic-setup)
  - [Advanced Options](#advanced-options)
- [API Reference](#api-reference)
```

---

## Code Blocks

**Always specify language**:

```rust
// Good - language specified
fn example() -> Result<()> {
    Ok(())
}
```

**Supported languages**: `rust`, `bash`, `toml`, `json`, `sql`, `swift`, `cpp`, `yaml`, `markdown`

**Indentation in lists**:

1. First step explanation:

    ```bash
    cargo build --release
    ```

2. Second step with nested code:

    ```rust
    let config = Config::load()?;
    ```

**Command output**:

```bash
$ cargo test --workspace
   Compiling adapteros-core v0.1.0
   Compiling adapteros-config v0.1.0
    Finished test [unoptimized + debuginfo] target(s) in 12.34s
```

---

## Links

### Internal Links

- Use relative paths: `[Text](path/to/file.md)`
- No trailing slashes
- Add anchor for headings: `[Section](#section-name)`
- Anchor format: lowercase, hyphens for spaces

**Good**:
```markdown
See [Memory Pool Guide](GUIDE_MEMORY_POOL.md) for details.
Refer to the [configuration section](#configuration) below.
Check [CLAUDE.md](../CLAUDE.md#quick-reference) for commands.
```

**Bad**:
```markdown
See [Memory Pool Guide](./docs/GUIDE_MEMORY_POOL.md/)  <!-- trailing slash, absolute path -->
Refer to the [Configuration Section](#Configuration Section)  <!-- wrong anchor format -->
```

### External Links

- Use full URLs: `https://example.com`
- Add source attribution when appropriate
- Prefer official documentation

```markdown
See the [MLX Documentation](https://ml-explore.github.io/mlx/) for framework details.
```

---

## Tables

**Standard format**:

```markdown
| Column A | Column B | Column C |
|----------|----------|----------|
| Value 1  | Value 2  | Value 3  |
| Value 4  | Value 5  | Value 6  |
```

**Alignment**:

```markdown
| Left     | Center   | Right    |
|:---------|:--------:|---------:|
| text     | text     | 123      |
```

**Rules**:
- Escape pipes in content: `\|`
- Align columns for source readability
- Keep tables under 5 columns when possible
- Use code blocks for complex data structures

---

## Deprecation & Archival

### Deprecated Documents

Add deprecation notice at top immediately after metadata:

```markdown
# Old Feature Guide

**Version**: 1.0
**Last Updated**: 2025-10-01
**Status**: Deprecated

---

> **DEPRECATED**: This document is superseded by [GUIDE_NEW_FEATURE.md](GUIDE_NEW_FEATURE.md).
> Kept for backward compatibility until 2026-01-01.

---

## Overview
...
```

### Archived Documents

1. Move to `/docs/archive/` directory
2. Add archive notice after metadata:

```markdown
# Historical Feature Documentation

**Version**: 1.0
**Last Updated**: 2024-06-15
**Status**: Archived

---

> **ARCHIVED**: Historical reference only. This feature was removed in v2.0.
> See [GUIDE_CURRENT.md](../GUIDE_CURRENT.md) for current information.

---
```

---

## Cross-References

### See Also Section

Add at the end of documents:

```markdown
## See Also

- [GUIDE_RELATED.md](GUIDE_RELATED.md) - Related tutorial
- [Crate README](../crates/adapteros-memory/README.md) - Implementation details
- [ARCHITECTURE_PATTERNS.md#memory](ARCHITECTURE_PATTERNS.md#memory) - Design decisions
- External: [MLX Documentation](https://ml-explore.github.io/mlx/)
```

### Inline References

Use contextual links within text:

```markdown
The memory pool (see [GUIDE_MEMORY_POOL.md](GUIDE_MEMORY_POOL.md)) handles automatic eviction
when pressure exceeds the threshold defined in [configuration](#configuration).
```

---

## Examples vs. Edge Cases

### Good/Bad Code Pairs

Show both correct and incorrect approaches:

**Good**:
```rust
// Proper error handling with context
pub async fn load(&self, path: &Path) -> Result<Data> {
    std::fs::read(path).map_err(|e| AosError::Io(format!("Failed to read {}: {}", path.display(), e)))?
}
```

**Bad**:
```rust
// Missing error context - avoid this
pub async fn load(&self, path: &Path) -> Result<Data> {
    Ok(std::fs::read(path)?)  // Error message loses path information
}
```

### Common Mistakes Section

```markdown
### Common Mistakes

1. **Forgetting to seed RNG**

   ```rust
   // Wrong - non-deterministic
   let value = rand::thread_rng().gen::<u64>();

   // Correct - HKDF-seeded
   let seed = derive_seed(&manifest_hash, "sampling");
   let value = seeded_rng(&seed).gen::<u64>();
   ```

2. **Blocking in async context**

   ```rust
   // Wrong - blocks the executor
   std::thread::sleep(Duration::from_secs(1));

   // Correct - yields to executor
   tokio::time::sleep(Duration::from_secs(1)).await;
   ```
```

### Error Handling Examples

Always show error cases:

```rust
match adapter.load().await {
    Ok(data) => info!(adapter_id = %id, "Adapter loaded"),
    Err(AosError::NotFound(msg)) => {
        warn!(adapter_id = %id, error = %msg, "Adapter not found");
        return Err(AosError::NotFound(msg));
    }
    Err(e) => {
        error!(adapter_id = %id, error = %e, "Failed to load adapter");
        return Err(e);
    }
}
```

---

## Special Sections

### Quick Reference Boxes

Use blockquotes for key information:

> **Quick Start**: Run `cargo build --release` then `./target/release/aosctl serve`

### Warning Boxes

```markdown
> **Warning**: This operation cannot be undone. Ensure you have backups before proceeding.
```

### Note Boxes

```markdown
> **Note**: This feature requires macOS 15+ for MLTensor support.
```

---

## Approval Process

### Creating New Documentation

1. Follow naming convention (`PURPOSE_COMPONENT.md`)
2. Include all required metadata
3. Add entry to `DOCUMENTATION_INDEX.md` (if it exists)
4. Add cross-references to related docs
5. Get review from relevant maintainer
6. Merge to main branch

### Deprecating Documentation

1. Add deprecation notice with date and replacement link
2. Update status to `Deprecated`
3. Create replacement document (if applicable)
4. Update all incoming cross-references
5. Schedule archival date (typically 3-6 months)

### Archiving Documentation

1. Move file to `/docs/archive/`
2. Update status to `Archived`
3. Add archive notice with redirect to current doc
4. Update `DOCUMENTATION_INDEX.md`
5. Keep file indefinitely for historical reference

---

## Checklist for New Documents

- [ ] File follows `PURPOSE_COMPONENT.md` naming
- [ ] Metadata block complete (Version, Date, Author, Status, Related)
- [ ] Single `# Title` header
- [ ] All code blocks have language specified
- [ ] Internal links use relative paths
- [ ] Tables are properly formatted
- [ ] See Also section included (if applicable)
- [ ] Added to documentation index
- [ ] No trailing whitespace
- [ ] No broken links

---

## See Also

- [CLAUDE.md](../CLAUDE.md) - Main developer guide
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Architecture documentation index
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guidelines
