# UI Spill Audit Report (2026-02-03)

**Scope:** Leptos web UI (all routes + `/style-audit`) and CodeGraph viewer  
**Modes:** Light + dark (desktop)  
**Status:** In progress — fixes applied, screenshots pending

## Summary
- Fixed high-risk overflow in shared table cells (Leptos) and CodeGraph viewer panels.
- Added wrapping and tooltips for long paths and identifiers.
- Screenshots still required for before/after evidence.

## Findings & Fixes

| ID | Surface/Route | Theme | Severity | Issue | Screenshot | Fix | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UI-001 | CodeGraph viewer / SidePanel | Light/Dark | High | Long names/paths/docstrings overflow or clip in side panel. | `var/ui-spill-audit/before/` (pending) | Added wrap/overflow rules and tooltips for file path. | Fixed |
| UI-002 | CodeGraph viewer / SearchBar | Light/Dark | High | Long symbol names and file paths overflow dropdown width. | `var/ui-spill-audit/before/` (pending) | Added max width, wrapping, and tooltips for metadata. | Fixed |
| UI-003 | CodeGraph viewer / DiffControls | Light/Dark | Medium | File name truncation without full path visibility. | `var/ui-spill-audit/before/` (pending) | Added title tooltip with full path. | Fixed |
| UI-004 | Leptos UI / Tables (global) | Light/Dark | High | Long IDs can overflow table cells and push layout. | `var/ui-spill-audit/before/` (pending) | Added `word-break` and `overflow-wrap` to table header/cell styles. | Fixed |

## Evidence Needed
- Capture before/after screenshots for entries UI-001 to UI-004.
- Baseline `/style-audit` in light + dark.
- High-traffic routes: `/dashboard`, `/chat`, `/adapters`, `/runs`, `/audit`, `/errors`, `/routing`, `/workers`, `/settings`, `/models`.

## Notes
- No API or backend changes.
- Desktop-only coverage per campaign scope.
