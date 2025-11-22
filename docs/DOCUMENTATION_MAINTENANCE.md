# Documentation Maintenance Guide

**Version**: 1.0
**Last Updated**: 2025-11-22
**Target Audience**: Maintainers, contributors

## Overview

Process for maintaining 864+ documentation files across 8+ locations.

## Regular Maintenance Tasks

### Monthly (Developer)
- [ ] Update "Last Updated" dates on docs you modify
- [ ] Fix broken links you discover while reading docs
- [ ] Add "See Also" links when you learn about related features

### Quarterly (Team)
- [ ] Run broken link checker (see tools below)
- [ ] Review stale docs (>6 months old)
- [ ] Update archive index in DOCUMENTATION_INDEX.md
- [ ] Check for orphaned docs using ROOT_DOCUMENTATION_INDEX.md

### Annually (Lead)
- [ ] Comprehensive documentation audit
- [ ] Archive rotation review (promote if needed)
- [ ] Update STYLE_GUIDE.md with new patterns
- [ ] Review all "deprecated" docs for deletion candidates

## Tools & Scripts

### Link Verification Script (TODO - not yet created)
```bash
# Check for broken internal links
./scripts/verify_docs_links.sh

# Output: List of broken [Text](path) references
```

### Orphaned Doc Detection
```bash
# Find .md files not referenced in any README or index
./scripts/find_orphaned_docs.sh

# Output: List of unreferenced .md files
```

## Common Maintenance Patterns

### Pattern 1: Update Documentation After Code Change

**When you change code that's documented:**
1. Find related docs using DOCUMENTATION_INDEX.md
2. Update "Last Updated" date
3. If significant change, update doc content
4. Test links in updated section

### Pattern 2: Deprecate Documentation

**When a feature/doc becomes obsolete:**
1. Create replacement doc (if needed)
2. Add deprecation notice to old doc:
   ```markdown
   ⚠️ **DEPRECATED** (2025-11-22) - See [GUIDE_NEW_FEATURE.md](GUIDE_NEW_FEATURE.md) instead.
   ```
3. Add "See Also" link in replacement doc
4. Keep old doc for 1 release
5. Move to `/docs/archive/` after deprecation period

### Pattern 3: Add New Documentation

**When creating new doc:**
1. Use naming convention from STYLE_GUIDE.md
2. Add metadata (version, updated date, author)
3. Add "See Also" section
4. Add to DOCUMENTATION_INDEX.md
5. Update CLAUDE.md if it's a core guide

### Pattern 4: Archive Stale Documentation

**When doc is no longer current:**
1. Review for historical value
2. Add "ARCHIVED - Historical reference" notice with date
3. Update cross-references to point to current doc
4. Move to `/docs/archive/[category]/`
5. Update DOCUMENTATION_INDEX.md

## Validation Checklist

Before merging PR with documentation changes:

- [ ] All new .md files follow naming convention (PURPOSE_COMPONENT.md)
- [ ] All new docs have metadata block (version, updated, author, status)
- [ ] All new docs have "See Also" section
- [ ] All broken links fixed
- [ ] Deprecated docs have clear replacement links
- [ ] DOCUMENTATION_INDEX.md updated if adding/removing files
- [ ] No typos (use spell-checker if available)
- [ ] Code examples tested/verified

## Known Issues & Workarounds

### Issue 1: Circular References
**Problem**: Doc A references Doc B which references Doc A
**Workaround**: Use one direction only, add "See Also" (not "See previous")

### Issue 2: Stale Completion Claims
**Problem**: Archive docs claim features "COMPLETE" but code has TODOs
**Workaround**: Keep archive docs as-is for history, point readers to current docs

### Issue 3: Root-Level Orphaned Docs
**Problem**: 67 implementation reports scattered at root
**Status**: Indexed in ROOT_DOCUMENTATION_INDEX.md
**Action**: Monitor for new orphans

## Escalation

- **Broken link**: Fix directly if obvious, file issue if unclear
- **Stale doc**: Create new version, deprecate old
- **Confusing structure**: Document rationale, open issue for refactor
- **Orphaned doc**: Add to ROOT_DOCUMENTATION_INDEX.md, consider archival

---

## Quick Reference

**Standard locations:**
- Active docs: `/docs/`
- Crate docs: `/crates/*/README.md`
- Quick starts: `/QUICKSTART*.md`
- Archive: `/docs/archive/`
- Root metadata: `CLAUDE.md`, `README.md`, `CONTRIBUTING.md`

**Key indexes:**
- `/docs/DOCUMENTATION_INDEX.md` - Master index
- `/docs/README.md` - Docs directory guide
- `/ROOT_DOCUMENTATION_INDEX.md` - Root orphaned files
- `/CLAUDE.md` - Developer guide

**Style & standards:**
- `/docs/STYLE_GUIDE.md` - Naming, metadata, format
- `/docs/ARCHIVE_ROTATION_POLICY.md` - Lifecycle management
