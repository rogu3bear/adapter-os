# Stack Research (v1.1.14)

**Milestone focus:** natural-language-first operator guidance, git-like adapter operations, and maximally assistive UI behavior.
**Researched:** 2026-02-28
**Confidence:** HIGH (grounded to current repo surfaces and standards)

## Existing Stack Fit

No framework migration is required for this milestone. Current stack already supports the needed work:

- **Leptos UI primitives with ARIA plumbing** are in place via shared components (`Button`/`ButtonLink` support `aria-label`), so accessibility improvements can be done without introducing a parallel UI system.
  - Citation: `crates/adapteros-ui/src/components/button.rs:100-123`
- **Command-first adapter workflow surfaces already exist** in Adapter Detail, Dashboard Guided Flow, and Update Center.
  - Citations:
    - `crates/adapteros-ui/src/components/adapter_detail_panel.rs:783-823`
    - `crates/adapteros-ui/src/pages/dashboard.rs:238-277`
    - `crates/adapteros-ui/src/pages/update_center.rs:215-236`
- **Dataset-feed continuity hooks already exist** through training query params (`repo_id`, `branch`, `source_version_id`), so milestone work can extend behavior rather than create new architecture.
  - Citation: `crates/adapteros-ui/src/pages/training/mod.rs:195-206`

## Best-Practice Alignment Targets

- Use explicit accessible names and predictable semantics for interactive controls and status messaging.
  - Source: WAI-ARIA APG (W3C): https://www.w3.org/TR/wai-aria-practices/
- Keep operator language concise, active-voice, and action-first to reduce ambiguity/cognitive load.
  - Sources:
    - NARA Plain Language Principles: https://www.archives.gov/open/plain-writing/10-principles.html
    - Digital.gov Plain Language: https://digital.gov/guides/plain-language/writing
- Preserve git-like mental model clarity by separating branch switching behavior from file restoration semantics.
  - Sources:
    - git switch docs: https://git-scm.com/docs/git-switch
    - reset/restore/revert distinctions: https://git-scm.com/docs/git

## Recommendation

- **Do not add new dependencies.**
- Implement milestone work in existing `adapter_detail_panel`, `dashboard`, `update_center`, and training-entry query flow.
- Keep vocabulary grounded in existing command map (`checkout`, `promote`, `feed-dataset`) and avoid introducing overloaded terms.
