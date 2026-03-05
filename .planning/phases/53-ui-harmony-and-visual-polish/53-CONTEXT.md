# Phase 53: UI Harmony and Visual Polish - Context

**Gathered:** 2026-03-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Strip UI bloat, unify Liquid Glass visual language across all surfaces, and make every page feel Apple-native and effortless. No new features — just make what exists look and feel right.

</domain>

<decisions>
## Implementation Decisions

### Visual audit scope
- Audit all surfaces but prioritize the inference/chat flow first (core workflow), then dashboard, then secondary pages
- Claude audits each surface and proposes cuts — user approves the cut list before implementation
- "Bloat" = unused controls, dead sections, redundant text, anything that doesn't connect to a working feature or actively serve the operator
- Target feel: pro-app level — think Linear, Raycast, Arc browser quality (not trying to fake native macOS chrome)

### Liquid Glass consistency
- Three tiers map to UI hierarchy: Tier 1 (lightest blur) for cards/content areas, Tier 2 for panels/sidebars, Tier 3 (heaviest) for modals/overlays
- Real `backdrop-filter` blur — already in use, just needs consistency across all surfaces
- Shadows only on elevated surfaces (modals, dropdowns, popovers); no visible borders between sibling elements — use background tint differences and whitespace instead
- Single polished theme (no dark mode in this phase — one well-done theme beats two mediocre ones)

### Layout and density
- Professional density — moderate, showing key info at a glance (Xcode/Instruments range, not Apple System Settings sparse)
- System font stack (`-apple-system`/SF Pro) throughout — SF Pro Display for headers, SF Pro Text for body
- Collapsible sidebar navigation — macOS-style, can collapse to icons for more content room
- SPA with smooth crossfades between views — no full page loads, gentle opacity transitions

### Interaction polish
- Every interactive element gets clear hover/active/focus states — buttons, cards, links, table rows
- Loading: skeleton screens matching content layout (Apple/Linear style)
- Empty states: minimal centered text with next-step hint, no custom illustrations
- Error/warning: Claude decides per context — inline for form validation, toasts for async operations

### Claude's Discretion
- Exact animation durations and easing curves
- Specific color values within the Liquid Glass palette
- How to handle responsive breakpoints (WASM app, but window resizing)
- Order of surface-by-surface cleanup beyond inference-first priority
- Whether to consolidate duplicate CSS or create shared component classes

</decisions>

<specifics>
## Specific Ideas

- Linear's clean card aesthetic is the reference point — information-dense but not cluttered
- Arc browser's sidebar collapse behavior is a good model for the navigation
- The existing Liquid Glass CSS should be rationalized, not rewritten — extend and unify, don't start over
- Every page should pass a "squint test" — visual hierarchy clear even when blurred

</specifics>

<deferred>
## Deferred Ideas

- Dark mode / system appearance matching — separate phase if needed
- Custom illustrations for empty states — not needed for MVP polish
- Keyboard shortcuts overlay / command palette visual redesign — separate from visual polish

</deferred>

---

*Phase: 53-ui-harmony-and-visual-polish*
*Context gathered: 2026-03-05*
