---
phase: 44-command-deck-adapter-ops-parity
created: 2026-02-28
status: ready_for_planning
---

# Phase 44: Command Deck AdapterOps Parity - Research

**Researched:** 2026-02-28
**Domain:** command palette adapter operations
**Confidence:** HIGH

## Evidence Highlights

- Contextual action generation already exists and is route-aware.
- Command execution handler already supports command-key dispatch and can be extended safely.
- Update Center already publishes selected adapter context via `RouteContext`.

## Planning Implications

- Add adapter operation actions to contextual + static search surfaces.
- Add command handlers for promote/checkout/feed intents.
- Parse `adapter_id` and `command` query hints in Update Center for deep-linked continuity.

## Citations

- `crates/adapteros-ui/src/search/contextual.rs`
- `crates/adapteros-ui/src/components/command_palette.rs`
- `crates/adapteros-ui/src/signals/search.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`

## Best-Practice Citations

- WAI-ARIA APG (keyboard interaction consistency): https://www.w3.org/TR/wai-aria-practices/
- PlainLanguage.gov principles: https://www.plainlanguage.gov/guidelines/
