# Archive Rotation Policy

**Version**: 1.0
**Last Updated**: 2025-11-22
**Applies To**: All documentation in /docs/archive/ and root-level delivery docs

## Purpose

Define clear criteria for when documentation becomes historical and when to promote archived docs to active status.

## Documentation Lifecycle

### Stage 1: Active
- Located in `/docs/` (not archive)
- Regularly updated
- Cross-referenced from current guides
- Examples: ARCHITECTURE_PATTERNS.md, CLAUDE.md

### Stage 2: Deprecated
- Still in `/docs/` but marked with deprecation notice
- Points to replacement documentation
- Kept for backward links/history
- Phased out over 2 releases

### Stage 3: Archived
- Moved to `/docs/archive/` after deprecation period
- Marked with "ARCHIVED - Historical reference" notice
- No longer updated
- Useful for understanding historical decisions

## Criteria for Archival

**A document should be archived when:**

1. **Superseded by newer doc** - A newer, authoritative version exists
   - Example: Old MLX integration guide superseded by current MLX_INTEGRATION.md
   - Action: Add deprecation notice → wait 1 release → move to archive/

2. **Completed feature/phase** - Documents a completed implementation phase
   - Example: Phase completion reports, implementation checklists
   - Action: Move to archive/completed-phases/

3. **Historical reference only** - Useful for understanding decisions but not current
   - Example: Old benchmarks, legacy architecture decisions
   - Action: Move to archive/historical-reports/

4. **Over 6 months stale** - Not updated in 6 months and no longer referenced
   - Action: Archive or refresh

## Promotion Criteria (Archive → Active)

**A document should be promoted from archive when:**

1. **Becomes current again** - Archived feature is re-activated
   - Example: Old training pipeline doc relevant again
   - Action: Review, update dates, move back to /docs/

2. **Missing current documentation** - Important topic has no active doc
   - Example: Archive doc better than missing coverage
   - Action: Restore, update, refresh references

## Process

### Archival Workflow
```
Active Doc
  ↓ (add deprecation notice)
Deprecated
  ↓ (wait 1 release)
Move to /docs/archive/[category]/
Add "ARCHIVED" notice with date
Update cross-references to point to replacement
```

### Promotion Workflow
```
Archived Doc
  ↓ (identify need for current info)
Restore to /docs/
Update all dates and metadata
Add cross-references
Review for accuracy
  ↓
Active Doc
```

## Current Archive Content (2025-11-22)

### Stale Completion Claims
**Status**: These files claim "COMPLETE" but codebase has 1,173+ TODOs
- /docs/archive/completed-phases/ (17 files)
- /docs/archive/ai-generated/ (AI-generated summaries, clearly marked stale)
- Action: Keep as historical reference, update maintenance notes

### Implementation Reports (67 root-level files)
**Status**: Delivery documentation, not regularly updated
- Action: Consider moving to /docs/archive/deliverables/ with index

### AI-Generated Snapshots
**Status**: Clearly marked as experimental AI outputs
- /docs/archive/ai-generated/ (158 files)
- Action: Keep unchanged, clearly marked

## Maintenance Schedule

- **Monthly**: Review stale docs (>6 months old), plan archival
- **Quarterly**: Update DOCUMENTATION_INDEX.md with new archives
- **Annually**: Comprehensive archive audit, consider promotion candidates

## Ownership

- **Active docs**: Codebase maintainers
- **Deprecated docs**: Original author + maintainer (joint approval to archive)
- **Archive rotation**: Documentation team (quarterly review)

---

## See Also
- [DOCUMENTATION_INDEX.md](DOCUMENTATION_INDEX.md)
- [STYLE_GUIDE.md](STYLE_GUIDE.md)
- [docs/README.md](README.md)
