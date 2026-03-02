# Phase 43: Repository Command Timeline - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Add an operator-visible repository command timeline in adapter detail so every checkout/promote state transition is easy to verify before the next dataset feed.

This phase is UI/API-consumption only. It reuses existing timeline API contracts and does not change backend schema.

</domain>

<decisions>
## Implementation Decisions

### Timeline source of truth
- Use existing `get_repo_timeline` client API for repository history retrieval.
- Keep timeline render latest-first and scoped to the selected adapter repository.
- Refresh timeline immediately after `Run Promote` and `Run Checkout` success paths.

### Operator guidance continuity
- Place timeline in the existing Update Center section inside adapter detail (no new page).
- Keep command-first natural language aligned with checkout/promote/feed-dataset workflow.
- Maintain assistive cues and concise status wording.

### Claude's Discretion
- Number of timeline rows shown by default.
- Visual phrasing for timeline event labels so long as state-change semantics remain explicit.

</decisions>

<deferred>
## Deferred Ideas

- Cross-repo merged command timeline surface remains out of scope.
- Timeline filtering/search controls remain out of scope for this phase.

</deferred>

---

*Phase: 43-repository-command-timeline*
*Context gathered: 2026-02-28*
