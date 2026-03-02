---
phase: 31-adapter-vcs-foundation-git-like-version-control-language
plan: 01
status: completed
outcome: passed
completed_at: 2026-02-28
---

# Phase 31 Plan 01 Summary

## Outcome

Phase 31 is complete and passed. Adapter version-control surfaces now use checkout-first language and expose direct dataset-feed entry from version controls.

## What Changed

1. Shifted adapter detail/update-center wording from restore/rollback operator language to checkout-focused language.
2. Added repository workflow guidance and dataset feed CTA in adapter version controls.
3. Added checkout-first API client method with compatibility alias retained.

## Code Evidence

- Checkout action language and feed CTA in adapter detail: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 755-766, 838-851, 1063-1070.
- Update Center language shift: `crates/adapteros-ui/src/pages/update_center.rs` lines 1-6 and 103-107.
- Lifecycle button wording shift from restore term: `crates/adapteros-ui/src/components/adapter_lifecycle_controls.rs` lines 48-50.
- Checkout-first UI client API and compatibility alias: `crates/adapteros-ui/src/api/client.rs` lines 573-623.

## Requirement Mapping

- `VCS-31-01`: satisfied.
- `VCS-31-02`: foundation satisfied (full branch/version context completed in Phase 32).
- `DOC-31-01`: satisfied.
