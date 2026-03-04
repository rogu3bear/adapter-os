# Phase 48: Commit Dirty Tree - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Commit the 84 modified + ~25 untracked files accumulated across milestones v1.1.15-v1.1.17 into logical, atomic commits. No new features, no refactoring — just get the working state committed with a clean tree.

</domain>

<decisions>
## Implementation Decisions

### Commit grouping
- Claude decides best grouping based on what actually changed (by logical change, by crate, or hybrid)
- Planning/evidence files may be grouped with related code or separated — Claude's call based on logical coherence
- All 84 modified files and ~25 untracked files are committed — no reverts
- Commit messages follow standard project convention (Claude picks scoped prefixes based on actual change type)

### Review depth
- Read each diff before committing — not blind commit
- Fix issues found during review before committing (dead code, incomplete changes, bugs)
- Run `cargo clippy` on affected crates for lint enforcement — no manual CLAUDE.md compliance audit needed
- Include untracked files in the review and commit scope

### Claude's Discretion
- Exact commit grouping strategy (by-crate, by-logical-change, or hybrid)
- Whether planning docs get their own commit or interleave with code
- Commit message wording and scope prefixes
- How to handle test_data/ changes (generated artifacts vs intentional)
- Order of commits

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The goal is a clean `git status` with a readable commit history.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 48-commit-dirty-tree*
*Context gathered: 2026-03-04*
