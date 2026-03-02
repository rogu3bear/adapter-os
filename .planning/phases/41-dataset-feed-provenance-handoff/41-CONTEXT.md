# Phase 41: Dataset Feed Provenance Handoff - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Guarantee that feed-dataset actions preserve branch/version provenance into training-entry workflows and expose that continuity clearly to operators. This phase strengthens continuity behavior and messaging in existing flows, without introducing new training subsystems.

</domain>

<decisions>
## Implementation Decisions

### Provenance handoff contract
- Feed actions must preserve `repo_id`, `branch`, and `source_version_id` when launching training-entry flow.
- Selected-version feed action remains the preferred path for explicit branch/version continuity.
- Handoff contract is treated as an invariant for update/detail-to-training transitions.

### Operator continuity messaging
- Launch messaging must explicitly state that branch/source-version context is prefilled.
- Selected-version summaries should keep branch context visible at action time.
- Provenance wording must stay consistent with phase-40 canonical command vocabulary.

### Continuity safety behavior
- If provenance context is partially unavailable, surface explicit guidance and keep operators on a safe manual-selection path.
- Do not silently drop context fields when transitioning into training workflow.

### Claude's Discretion
- Exact UX placement of continuity helper text.
- Whether continuity hint appears inline, secondary text, or compact status badge, provided clarity is preserved.

</decisions>

<specifics>
## Specific Ideas

- Validate feed launch behavior against current training query-param intake (`repo_id`, `branch`, `source_version_id`) and keep that contract centralized.
- Keep the operator-facing text short: "Training opens with branch and source version prefilled."

</specifics>

<deferred>
## Deferred Ideas

- Multi-step provenance timeline UI beyond current selected-version messaging (future UX phase).
- Cross-page provenance breadcrumbs outside update/detail/training-entry core flow.

</deferred>

---

*Phase: 41-dataset-feed-provenance-handoff*
*Context gathered: 2026-02-28*
