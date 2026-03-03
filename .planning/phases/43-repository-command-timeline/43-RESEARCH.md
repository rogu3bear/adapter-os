---
phase: 43-repository-command-timeline
created: 2026-02-28
status: ready_for_planning
---

# Phase 43: Repository Command Timeline - Research

**Researched:** 2026-02-28
**Domain:** adapter repository timeline visibility
**Confidence:** HIGH

## Evidence Highlights

- UI client already exposes repository timeline retrieval (`get_repo_timeline`).
- Timeline event type already exists in API types and is WASM-safe.
- Adapter detail Update Center is the native surface for command workflow continuity.

## Planning Implications

- Implement timeline rendering in `AdapterVersionPromotionSection` with latest-first cards.
- Refresh timeline on promote/checkout completion to keep history deterministic for operators.
- Keep wording plain-language and action-first.

## Citations

- `crates/adapteros-ui/src/api/client.rs`
- `crates/adapteros-ui/src/api/types.rs`
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`

## Best-Practice Citations

- WAI-ARIA APG (status updates and predictable interactions): https://www.w3.org/TR/wai-aria-practices/
- Nielsen Norman Group - Visibility of system status: https://www.nngroup.com/articles/visibility-system-status/
