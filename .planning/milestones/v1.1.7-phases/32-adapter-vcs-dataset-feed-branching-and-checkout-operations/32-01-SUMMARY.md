---
phase: 32-adapter-vcs-dataset-feed-branching-and-checkout-operations
plan: 01
status: completed
outcome: passed
completed_at: 2026-02-28
---

# Phase 32 Plan 01 Summary

## Outcome

Phase 32 is complete and passed. Checkout-first contracts now exist at server route and UI client layers with rollback compatibility preserved, and branch/version feed context now flows from adapter controls into training wizard state.

## What Changed

1. Added `/v1/adapter-repositories/{repo_id}/versions/checkout` server route alias and handler.
2. Exposed checkout handler in API routing/export surfaces.
3. Updated UI client to call `/versions/checkout` first and fallback to `/versions/rollback` when needed.
4. Added branch/version-aware dataset feed links from resolved adapter versions.
5. Added training query parsing (`repo_id`, `branch`, `source_version_id`) and surfaced context in wizard knowledge/review steps.

## Code Evidence

- New checkout handler and alias behavior: `crates/adapteros-server-api/src/handlers/adapter_versions.rs` lines 692-718.
- New checkout route binding: `crates/adapteros-server-api/src/routes/adapters.rs` lines 66-68.
- Checkout-first client with fallback: `crates/adapteros-ui/src/api/client.rs` lines 578-612.
- Branch/version feed links from adapter detail: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 557-575 and 843-851.
- Training query ingestion for branch/version context: `crates/adapteros-ui/src/pages/training/mod.rs` lines 146-155 and 195-206.
- Training wizard context rendering: `crates/adapteros-ui/src/pages/training/wizard.rs` lines 176-207, 262-287, 1059-1066, 1794-1796.

## Requirement Mapping

- `VCS-32-01`: satisfied.
- `VCS-32-02`: satisfied.
- `DOC-32-01`: satisfied.
