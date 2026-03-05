# Phase 53: UI Harmony and Visual Polish - Research

**Researched:** 2026-03-05
**Domain:** Leptos 0.7 WASM frontend — CSS design system consolidation, visual audit, interaction polish
**Confidence:** HIGH

## Summary

Phase 53 is a pure CSS/component cleanup phase targeting an existing Leptos 0.7 CSR WASM app with an established Liquid Glass design system. The codebase already has a well-structured 3-tier glass system (`glass.css`), comprehensive design tokens (`base.css`), and ~11K lines of component CSS split across 6 files. The UI has ~60 Rust components, ~20 page modules, and a collapsible sidebar navigation system. The work is about consistency and removal, not new capability.

The main tensions to resolve: (1) the font stack currently uses Plus Jakarta Sans but the user wants system fonts (SF Pro), (2) transition/animation durations are inconsistent across 155+ `transition` declarations, (3) utility classes mimic Tailwind patterns in hand-maintained CSS which creates class name drift, and (4) there is no visual regression test infrastructure — verification is manual squint-test plus compilation checks.

**Primary recommendation:** Work surface-by-surface starting with the chat/inference flow, applying a consistent audit-then-fix pattern: enumerate all elements on a page, flag dead/orphaned/redundant items, fix spacing/typography/glass tier violations, add missing hover/active/focus states, and ensure skeleton loading states match content shapes.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Audit all surfaces but prioritize the inference/chat flow first (core workflow), then dashboard, then secondary pages
- Claude audits each surface and proposes cuts — user approves the cut list before implementation
- "Bloat" = unused controls, dead sections, redundant text, anything that doesn't connect to a working feature or actively serve the operator
- Target feel: pro-app level — think Linear, Raycast, Arc browser quality (not trying to fake native macOS chrome)
- Three tiers map to UI hierarchy: Tier 1 (lightest blur) for cards/content areas, Tier 2 for panels/sidebars, Tier 3 (heaviest) for modals/overlays
- Real `backdrop-filter` blur — already in use, just needs consistency across all surfaces
- Shadows only on elevated surfaces (modals, dropdowns, popovers); no visible borders between sibling elements — use background tint differences and whitespace instead
- Single polished theme (no dark mode in this phase)
- Professional density — moderate, showing key info at a glance (Xcode/Instruments range)
- System font stack (`-apple-system`/SF Pro) throughout — SF Pro Display for headers, SF Pro Text for body
- Collapsible sidebar navigation — macOS-style, can collapse to icons for more content room
- SPA with smooth crossfades between views — no full page loads, gentle opacity transitions
- Every interactive element gets clear hover/active/focus states
- Loading: skeleton screens matching content layout (Apple/Linear style)
- Empty states: minimal centered text with next-step hint, no custom illustrations
- Error/warning: Claude decides per context — inline for form validation, toasts for async operations

### Claude's Discretion
- Exact animation durations and easing curves
- Specific color values within the Liquid Glass palette
- How to handle responsive breakpoints (WASM app, but window resizing)
- Order of surface-by-surface cleanup beyond inference-first priority
- Whether to consolidate duplicate CSS or create shared component classes

### Deferred Ideas (OUT OF SCOPE)
- Dark mode / system appearance matching — separate phase if needed
- Custom illustrations for empty states — not needed for MVP polish
- Keyboard shortcuts overlay / command palette visual redesign — separate from visual polish
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UI-53-01 | Visual audit: no orphaned components, dead controls, or redundant text across all pages | Audit methodology documented; surface enumeration approach; page-by-page verification strategy |
| UI-53-02 | Typography, spacing, and color follow Liquid Glass design system consistently | Font stack migration (Plus Jakarta Sans to SF Pro); design token audit; spacing scale enforcement |
| UI-53-03 | Core workflows complete in minimal clicks with clear visual feedback | Interaction state audit (hover/active/focus); skeleton loading patterns; transition consistency |
| A11Y-53-01 | Accessibility maintained or improved during visual polish | WCAG AA contrast ratios preserved; focus-visible states; reduced-motion support; ARIA attributes |
</phase_requirements>

## Standard Stack

### Core (Already In Use)
| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| Leptos | 0.7 | CSR WASM framework | In place, no changes |
| leptos_router | 0.7 | SPA routing | In place, no changes |
| Pure CSS | N/A | Liquid Glass design system | In place, needs consolidation |

### Supporting (Already In Use)
| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| web-sys | 0.3 | DOM access for scroll, focus | In place |
| gloo-timers | 0.3 | WASM-safe timers for transitions | In place |
| wasm-bindgen | 0.2 | JS interop | In place |

### No New Dependencies Needed
This phase is CSS-only plus minor Rust component cleanup. No new crates or libraries required. The existing design token system, component library, and CSS build pipeline are sufficient.

## Architecture Patterns

### Existing CSS Architecture (Preserve This)
```
crates/adapteros-ui/dist/
  base.css              # Reset, tokens, animations, utilities (520 lines)
  fonts.css             # Plus Jakarta Sans @font-face (113 lines) -> TO BE REPLACED
  glass.css             # Liquid Glass 3-tier system (748 lines)
  components.css        # @import bundle entry point
  components/
    core.css            # Buttons, cards, inputs, dialogs, tables (2223 lines)
    utilities.css       # Atomic utility classes (1944 lines)
    layout.css          # Shell, sidebar, topbar, page scaffold (2136 lines)
    overlays.css        # Toast, status center, telemetry (1525 lines)
    pages.css           # Feature/page-specific styles (2950 lines)
    hud.css             # HUD mode styles (649 lines)
  components-bundle.css # Generated by scripts/bundle-css.sh
```

### Design Token Hierarchy (Established)
```
:root {
  --color-*          Base semantic colors (background, foreground, primary, etc.)
  --space-*          Spacing scale (0 through 16)
  --radius-*         Border radius scale (sm, md, lg, full)
  --font-sans        Body font (TO CHANGE: Plus Jakarta Sans -> SF Pro)
  --font-display     Heading font (TO CHANGE: Plus Jakarta Sans -> SF Pro Display)
  --font-mono        Code font (keep as-is: ui-monospace stack)
  --density-*        Density-aware spacing tokens
}

.theme-glass {
  --glass-bg-1/2/3   Glass tier backgrounds
  --glass-blur-1/2/3 Glass tier blur amounts
  --glass-border     Glass border color
  --glass-shadow-*   Glass shadow levels
  --glass-text-*     Glass-safe text colors
  --action-*         Action button colors
}
```

### Component Hierarchy (Existing)
```
Shell
  TopBar                    # Brand, search, user menu, density toggle
  SidebarNav                # Collapsible workflow groups (already implemented)
    SidebarGroup            # Chat, Build, Evidence, Versions, System
    SidebarItem             # Individual nav links
  Workspace
    PageScaffold            # Standardized page layout
      PageScaffoldHeader    # Title, breadcrumbs, actions
      PageScaffoldMain      # Content area
      PageScaffoldInspector # Optional right rail
```

### Pattern 1: Surface Audit Methodology
**What:** Systematic per-surface visual audit.
**When to use:** Each surface cleanup task.
**Process:**
1. Enumerate every visible element on the page
2. Classify each: essential / redundant / dead / orphaned
3. Check glass tier assignment (is it correct for the element's role?)
4. Verify spacing uses design tokens (not hardcoded px)
5. Check typography (correct font weight, size, line-height)
6. Verify interaction states exist (hover, active, focus-visible)
7. Check skeleton loader matches content layout
8. Verify empty state follows pattern (centered text + hint)
9. Run "squint test" — visual hierarchy clear when blurred

### Pattern 2: Font Stack Migration
**What:** Replace Plus Jakarta Sans with system font stack.
**When to use:** Phase 53 typography task.
**Implementation:**
```css
/* base.css: Change these two tokens */
--font-sans: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Helvetica Neue', system-ui, sans-serif;
--font-display: -apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Helvetica Neue', system-ui, sans-serif;

/* fonts.css: Remove Plus Jakarta Sans @font-face declarations entirely */
/* OR keep as fallback but move -apple-system first in stack */
```
**Considerations:** SF Pro is available on macOS by default. On other platforms the stack falls through to system-ui which is fine since this is a macOS-targeted app. Removing the self-hosted Plus Jakarta Sans woff2 files saves ~80KB of font payload.

### Pattern 3: Transition Consolidation
**What:** Standardize transition durations and easing across all CSS.
**Recommendation (Claude's discretion):**
```css
/* Recommended standard durations */
:root {
  --duration-fast: 120ms;      /* Hover states, toggles */
  --duration-normal: 200ms;    /* Most interactions */
  --duration-slow: 300ms;      /* Page crossfades, glass transitions */
  --ease-default: cubic-bezier(0.4, 0, 0.2, 1);  /* Already defined */
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1); /* Subtle spring for emphasis */
}
```
Currently there are 155+ `transition` declarations with inconsistent durations (100ms, 150ms, 200ms, 300ms) and easing (ease, ease-in-out, var(--ease-default)). Standardize to use CSS custom properties.

### Pattern 4: View Crossfade
**What:** Smooth opacity transitions between SPA page views.
**Implementation approach:**
```css
/* Page content fade-in on route change */
.shell-main > * {
  animation: page-enter var(--duration-slow) var(--ease-default) both;
}

@keyframes page-enter {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
```
Note: Leptos router's `<Outlet>` replaces the child on route change, so CSS animation on the content wrapper is sufficient. No Rust changes needed for basic crossfade.

### Anti-Patterns to Avoid
- **Don't rewrite the CSS architecture.** The CONTEXT.md explicitly says "rationalize, not rewrite." Extend existing tokens and patterns; don't create a parallel system.
- **Don't add Tailwind-style utility explosion.** The existing utility classes are hand-maintained. Prefer semantic component classes (`.card`, `.btn-primary`) over atomic utilities for new styles.
- **Don't introduce CSS-in-Rust.** Keep styles in `.css` files under `dist/`. The existing pipeline works.
- **Don't touch dark mode.** Explicitly deferred. Only polish the light theme.
- **Don't introduce layout shifts.** Every visual change must preserve the same content flow. Use `will-change` sparingly for blur-heavy elements.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Skeleton loaders | Custom shimmer per page | Existing `<Skeleton*>` components from `skeleton.rs` | Already have SkeletonCard, SkeletonText, SkeletonTable, SkeletonStatsGrid, SkeletonPageHeader, SkeletonDetailSection |
| Empty states | Custom per-page empty | Existing `<EmptyState>` from `async_state.rs` with `EmptyStateVariant` | Already has centered layout, icon, message, action pattern |
| Page layout | Custom page wrappers | Existing `<PageScaffold>` with slots | Already handles title, breadcrumbs, actions, inspector rail |
| Interaction feedback | Custom hover/focus per element | CSS pseudo-classes on existing component classes | Extend `.btn`, `.card`, `.sidebar-item`, etc. with consistent states |
| Toast notifications | Custom notification system | Existing `<ToastContainer>` + toast signal system | Already handles success/error/warning with auto-dismiss |
| Loading states | Per-component loading | Existing `<AsyncBoundary>` / `<AsyncBoundaryWithEmpty>` | Already wraps loading/error/empty/loaded states |

**Key insight:** The component library is comprehensive. Phase 53 is about making every surface *use* the library consistently, not building new primitives.

## Common Pitfalls

### Pitfall 1: Signal Disposal Panics During Visual Changes
**What goes wrong:** Changing component structure (removing/adding elements) can trigger signal disposal panics if the removed elements had active reactive subscriptions.
**Why it happens:** Leptos 0.7 disposes signals when owning component unmounts. If a reactive closure calls `.get()` on a disposed signal, it panics.
**How to avoid:** Always use `.try_get().unwrap_or_default()` in reactive closures. Use `<Show>` instead of `{move || if ...}` for conditional rendering. Never remove a component that owns reactive state without considering its subscribers.
**Warning signs:** "BorrowMutError" or "already borrowed" in WASM console during navigation.

### Pitfall 2: backdrop-filter Performance
**What goes wrong:** Adding `backdrop-filter: blur()` to too many elements causes jank, especially with nested blur layers.
**Why it happens:** Each blur layer requires GPU compositing. Nested blurs multiply the cost.
**How to avoid:** Limit blur to 3-5 elements visible at once. Use `will-change: transform` on blur elements (already done in glass.css for some). Never nest blur inside blur unless the inner element has `isolation: isolate`.
**Warning signs:** Choppy scrolling, high GPU usage on Activity Monitor.

### Pitfall 3: Font Metric Shifts on Stack Change
**What goes wrong:** Switching from Plus Jakarta Sans to SF Pro changes font metrics (character width, ascender/descender), causing layout shifts on every surface.
**Why it happens:** Different fonts have different metrics even at the same nominal size. Plus Jakarta Sans is wider than SF Pro.
**How to avoid:** When changing the font stack, audit all fixed-width containers (sidebar width, table columns, input fields). SF Pro is slightly narrower, so text will reflow. Allow for the sidebar potentially having extra space at its current `--layout-rail-width: 22rem`.
**Warning signs:** Text truncation where it wasn't before, or excess whitespace in fixed containers.

### Pitfall 4: Breaking Test-Contracted IDs
**What goes wrong:** Removing "dead" elements that actually have test-contracted IDs used by Playwright or integration tests.
**Why it happens:** Some elements look decorative but have `data-testid` or hardcoded IDs referenced in tests.
**How to avoid:** Before removing any element, grep for its ID/class across the entire codebase: `tests/`, `crates/adapteros-e2e/`, and Playwright configs. The chat workspace header comment explicitly lists contracted IDs.
**Warning signs:** Tests fail after visual cleanup.

### Pitfall 5: CSS Specificity Conflicts Between glass.css and core.css
**What goes wrong:** Changing a component's glass tier or removing a border creates unexpected visual results because `glass.css` overrides compete with `core.css` base styles.
**Why it happens:** The `.theme-glass .card` selector in `glass.css` has higher specificity than `.card` in `core.css`. The load order is: base -> components (core, utilities, layout, overlays, pages, hud) -> glass.
**How to avoid:** When removing borders between sibling elements (per the user decision), modify both the base style AND the glass override. Check `components-bundle.css` to see the final cascade order.
**Warning signs:** Style changes only appearing when glass theme is on/off.

### Pitfall 6: Removing Elements Used by the Command Palette
**What goes wrong:** Elements that appear "dead" on a page may actually be referenced by the command palette's contextual actions system.
**Why it happens:** `CommandPalette` reads route context and provides contextual actions. Some page controls are only activated through the palette.
**How to avoid:** Check `search/` module and `command_palette.rs` for route-based action registration before removing controls from pages.

## Code Examples

### Example 1: Standardized Hover State (Buttons)
```css
/* Existing pattern in core.css — ensure ALL buttons follow this */
.btn {
  transition: all var(--duration-normal) var(--ease-default);
}
.btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: var(--glass-shadow-md);
}
.btn:active:not(:disabled) {
  transform: translateY(1px);
  transition: transform var(--duration-fast) ease;
}
.btn:focus-visible {
  outline: 2px solid var(--color-ring);
  outline-offset: 2px;
}
```

### Example 2: Sidebar Border Removal (Decision: No Borders Between Siblings)
```css
/* Current: sidebar has explicit border-right */
.sidebar {
  border-right: 1px solid var(--color-border);
}

/* After: use background tint difference + whitespace */
.sidebar {
  border-right: none;
  background: color-mix(in srgb, var(--glass-bg-2) 92%, var(--color-muted));
}
/* The shell-main already has a slightly different background that creates visual separation */
```

### Example 3: System Font Stack (base.css change)
```css
/* Replace in :root */
--font-sans: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, 'Helvetica Neue', sans-serif;
--font-display: -apple-system, BlinkMacSystemFont, 'SF Pro Display', system-ui, 'Helvetica Neue', sans-serif;
```

### Example 4: Page Crossfade Animation
```css
/* Add to base.css animations section */
@keyframes page-enter {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.shell-main > * {
  animation: page-enter var(--duration-slow, 300ms) var(--ease-default) both;
}
```

### Example 5: Removing Dead Controls (Rust Side)
```rust
// BEFORE: Button that does nothing or links to unimplemented feature
view! {
    <Button variant=ButtonVariant::Ghost on_click=Callback::new(|_| {})>
        "Export Report"
    </Button>
}

// AFTER: Remove entirely. If the feature is planned, leave no ghost.
// Do NOT replace with a disabled button — that's worse than absent.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Plus Jakarta Sans (self-hosted woff2) | System font stack (SF Pro on macOS) | Phase 53 | Removes ~80KB font payload, native feel, no FOIT |
| Individual transition durations (100-300ms hardcoded) | CSS custom property durations (--duration-fast/normal/slow) | Phase 53 | Consistent animation timing, single source of truth |
| Per-element shadow/border decisions | Elevation system (data-elevation="1/2/3") | Already in glass.css | Ensures consistent glass tier assignment |
| Utility-class layout inline | PageScaffold component | Already in layout | Consistent page structure across all surfaces |

**Already Modern:**
- Liquid Glass 3-tier system with proper `@supports` fallbacks
- CSS custom properties for all design tokens
- `prefers-reduced-motion` and `forced-colors` media queries
- WCAG AA contrast compliance with glass-specific text tokens
- Skeleton loading components with shimmer animation
- Density system (comfortable/compact) via data attribute

## Open Questions

1. **Font file cleanup scope**
   - What we know: Plus Jakarta Sans woff2 files are in `dist/fonts/`. The user wants SF Pro system fonts.
   - What's unclear: Should the Plus Jakarta Sans files be deleted entirely, or kept as fallback for non-macOS testing?
   - Recommendation: Delete the font files and fonts.css @font-face declarations. This is a macOS-targeted app. The system font stack has adequate fallbacks (system-ui, Helvetica Neue).

2. **Border removal between siblings — scope**
   - What we know: User wants no visible borders between sibling elements, using background tint + whitespace instead.
   - What's unclear: Does this apply to table rows? Table headers currently have bottom borders for readability.
   - Recommendation: Apply to layout chrome (sidebar/main separation, card-to-card spacing) but preserve functional borders in data tables where row separation aids scanning.

3. **Shadow audit — what counts as "elevated"?**
   - What we know: Shadows only on elevated surfaces (modals, dropdowns, popovers).
   - What's unclear: Do cards count as elevated? Currently cards have `--glass-shadow-md`.
   - Recommendation: Cards in main content areas lose box-shadow (they sit flat). Cards in overlays/popovers keep shadow. The sidebar panel keeps subtle shadow as it overlaps content.

4. **CSS deduplication scope**
   - What we know: `components-bundle.css` is generated by concatenating 6 source files. There's likely duplicated patterns across files.
   - What's unclear: How much duplication exists and whether consolidation risks specificity order changes.
   - Recommendation: This is in Claude's discretion. Focus on obvious duplicate patterns (same property declarations on similar selectors) but don't restructure the file organization. The current 6-file split is logical.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust native tests (cargo test) + manual visual audit |
| Config file | `crates/adapteros-ui/Cargo.toml` |
| Quick run command | `cargo test -p adapteros-ui --lib` |
| Full suite command | `./scripts/ui-check.sh && cargo test -p adapteros-ui --lib` |
| Estimated runtime | ~15 seconds (WASM check) + ~5 seconds (lib tests) |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UI-53-01 | No orphaned components, dead controls, redundant text | manual (visual audit) | N/A — requires human visual inspection | N/A |
| UI-53-02 | Typography/spacing/color follow Liquid Glass consistently | smoke + manual | `./scripts/ui-check.sh` (compile check) | Yes |
| UI-53-03 | Core workflows complete in minimal clicks | manual (workflow walkthrough) | N/A — requires running app | N/A |
| A11Y-53-01 | Accessibility maintained during polish | unit | `cargo test -p adapteros-ui --lib` | Yes (tests/components.rs, tests/web.rs) |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `./scripts/ui-check.sh && cargo test -p adapteros-ui --lib`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** Full suite green + manual visual walkthrough before `/gsd:verify-work`
- **Estimated feedback latency per task:** ~20 seconds

### Wave 0 Gaps
None. The existing test infrastructure (WASM compilation check via `ui-check.sh` and unit tests via `cargo test -p adapteros-ui --lib`) covers the automated verification needs. Visual audit requirements are inherently manual — no test framework can validate "looks right" for a design polish phase. The existing 817 lines of tests in `tests/` verify component rendering, routing, and chat state management which will catch regressions from structural changes.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/adapteros-ui/dist/*.css` — all design tokens, glass system, component styles
- Codebase analysis: `crates/adapteros-ui/src/components/` — full component inventory (60+ components)
- Codebase analysis: `crates/adapteros-ui/src/pages/` — full page inventory (20+ pages)
- Codebase analysis: `crates/adapteros-ui/src/components/layout/nav_registry.rs` — complete navigation structure
- Project CLAUDE.md — Leptos 0.7 patterns, signal disposal rules, CSS conventions

### Secondary (MEDIUM confidence)
- CSS `backdrop-filter` performance characteristics — based on WebKit implementation notes and general browser rendering knowledge

### Tertiary (LOW confidence)
- SF Pro font metrics comparison with Plus Jakarta Sans — no direct measurement performed, based on general typographic knowledge that system fonts tend to be narrower than geometric sans-serif fonts

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, well-understood existing codebase
- Architecture: HIGH — existing CSS architecture is documented and stable; changes are incremental
- Pitfalls: HIGH — based on direct codebase analysis and established Leptos 0.7 patterns from project MEMORY.md
- Font migration: MEDIUM — SF Pro availability on macOS is certain, but metric differences are estimated not measured

**Research date:** 2026-03-05
**Valid until:** 2026-04-05 (stable domain — CSS/design systems move slowly)
