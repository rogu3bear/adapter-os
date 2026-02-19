# Material UI / Material Design Audit

**Scope:** `crates/adapteros-ui` (Leptos WASM, Liquid Glass design system)  
**Reference:** Material Design 2 (MD2) guidelines, Material UI component library  
**Date:** 2026-02-18

---

## Executive Summary

AdapterOS uses a **custom Liquid Glass design system**, not Material UI. This audit evaluates how the current design aligns with Material Design principles and where intentional or unintentional divergence occurs.

**Overall alignment:** **B+** — Strong foundation (typography, spacing, semantic tokens, accessibility). Divergence is mostly intentional (glass morphism vs. flat Material surfaces). A few areas could benefit from MD-inspired refinements.

---

## 1. Component-by-Component Comparison

### 1.1 Text Fields (Inputs)

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Label placement** | Floating label (inside outline) | **Label outside** (FormField) | ✅ **Better** — Smashing Magazine and UX research favor conventional labels; floating labels reduce clarity |
| **Outline** | Underline or outlined variant | Full border (`1px solid`) | ✅ Aligned |
| **Height** | 56dp (single-line) | 2.625rem (~42px) | ⚠️ Slightly shorter; acceptable for dense UIs |
| **Focus state** | Ripple + underline color | Ring + border color change | ✅ Clear focus indicator |
| **Error state** | Red underline + helper text | Red border + `form-field-error` | ✅ Aligned |
| **Placeholder** | Not a substitute for label | FormField enforces visible label | ✅ Correct |

**Recommendation:** No change. AdapterOS follows best practice (label outside, distinct border) over MD’s floating label.

---

### 1.2 Buttons

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Variants** | Filled, Outlined, Text | Primary, Secondary, Destructive, Outline, Ghost, Link | ✅ Equivalent mapping |
| **Elevation** | Filled: 0–2dp elevation | Glass + subtle shadow | ⚠️ Different aesthetic; glass is intentional |
| **Ripple** | Ink ripple on press | `translateY(1px)` on active | ⚠️ No ripple; simpler feedback |
| **Min touch target** | 48×48dp | btn-md: 2.25rem (~36px) | ⚠️ Below MD; consider 40px min for primary actions |
| **Focus** | 2px outline | `focus-visible` ring | ✅ Good |
| **Disabled** | 38% opacity | Semantic disabled tokens | ✅ Aligned |

**Recommendation:** Consider increasing primary button min-height to 40px (2.5rem) for touch targets. Ripple is optional; current active state is sufficient.

---

### 1.3 Cards

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Structure** | Media, content, actions | card-header, card-content, card-footer | ✅ Aligned |
| **Elevation** | 1dp default | `--surface-shadow-sm` | ✅ Equivalent |
| **Border** | Optional | `1px solid` required for glass | ✅ Intentional |
| **Border radius** | 4dp (small), 8dp (medium) | `calc(var(--radius) + 0.125rem)` (~9px) | ✅ Consistent |
| **Hover** | Optional elevation change | No hover elevation | ⚠️ Could add subtle elevation on interactive cards |

**Recommendation:** Optional: add `:hover` elevation for action cards (e.g. `.action-card`).

---

### 1.4 Dialogs

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Backdrop** | Scrim (opacity) | dialog-overlay | ✅ Aligned |
| **Content** | Centered, max-width | dialog-content, scrollable variant | ✅ Aligned |
| **Header** | Optional title + close | dialog-header, dialog-title | ✅ Aligned |
| **Focus trap** | Required | Implemented (dialog.rs) | ✅ Aligned |
| **Escape to close** | Required | Implemented | ✅ Aligned |
| **Glass treatment** | N/A | Tier 3 blur on content | ✅ Intentional |

**Recommendation:** No change. Dialog behavior matches MD expectations.

---

### 1.5 Tabs

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Indicator** | Underline on active | `border-bottom: 2px solid` | ✅ Aligned |
| **Scroll** | Horizontal scroll for overflow | tab-nav flex | ⚠️ No scroll; may overflow on narrow viewports |
| **Touch target** | 48dp height | `padding: 1rem 0.25rem` | ✅ Adequate |

**Recommendation:** Add `overflow-x-auto` to `.tab-nav` for narrow screens.

---

### 1.6 Toggle / Switch

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Track** | Rounded rectangle | `border-radius: 9999px` (pill) | ✅ Aligned |
| **Thumb** | Circular, slides | toggle-thumb, translateX | ✅ Aligned |
| **States** | On, Off, Disabled | toggle-on, toggle-off, disabled | ✅ Aligned |
| **Focus** | Focus ring | focus-visible ring | ✅ Aligned |

**Recommendation:** No change.

---

### 1.7 Checkbox

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Shape** | Rounded square | `border-radius: 0.25rem` | ✅ Aligned |
| **Size** | 18dp | 1rem (16px) | ⚠️ Slightly smaller |
| **Indeterminate** | Supported | Not visible in audit | ⚠️ Verify if needed |
| **Label** | Required | checkbox-label wrapper | ✅ Aligned |

**Recommendation:** Consider 18px checkbox for better touch target.

---

### 1.8 Tables

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Header** | Sticky, distinct | table-header, table-header-cell | ✅ Aligned |
| **Row hover** | Optional | table-row:hover | ✅ Aligned |
| **Density** | Default, comfortable, compact | Single density | ⚠️ No density option |
| **Sort indicators** | Arrow icons | Not in core.css | ⚠️ Page-level if used |

**Recommendation:** Document table patterns in UI_LAYOUT_MATRIX; density is optional.

---

### 1.9 Snackbar / Toast

| Aspect | Material Design 2 | AdapterOS | Assessment |
|--------|-------------------|-----------|------------|
| **Position** | Bottom center (default) | Toast component | ✅ Equivalent |
| **Auto-dismiss** | 4–10s | Configurable | ✅ Aligned |
| **Action button** | Optional | Toast with action | ✅ Aligned |
| **Stacking** | Multiple | Overlays system | ✅ Aligned |

**Recommendation:** No change.

---

## 2. Design System Audit (per design-audit skill)

### 2.1 Typography

| Check | Status | Notes |
|-------|--------|------|
| Font pairing | ✅ | Plus Jakarta Sans + system mono; max 2 families |
| Type scale | ⚠️ | text-xs (0.75rem) → text-3xl (1.875rem); not strict 1.25/1.333 ratio |
| Line height | ✅ | --line-height-body 1.58, heading 1.22 |
| Readability | ✅ | 0.875rem default; consider 1rem for body in dense forms |
| Hierarchy | ✅ | card-title, page titles via clamp() |

**Red flags:** None critical. Type scale is pragmatic rather than mathematically strict.

---

### 2.2 Spacing & Layout

| Check | Status | Notes |
|-------|--------|------|
| Spacing scale | ✅ | --space-1 through --space-16 (4px base) |
| Utilities use tokens | ⚠️ | CSS_STRATEGY.md: utilities use hardcoded rem, not var(--space-*) |
| Proximity | ✅ | form-field, card-header/content/footer grouped |
| Responsive | ✅ | layout.css, breakpoints sm/md/lg/xl |

**Red flags:** Unused --space-* tokens (see CSS_STRATEGY.md). Wire utilities to tokens for consistency.

---

### 2.3 Color System

| Check | Status | Notes |
|-------|--------|------|
| Palette | ✅ | Semantic tokens (primary, destructive, status-*) |
| Dark mode | ✅ | .dark overrides for all surfaces |
| Opacity | ✅ | color-mix(), /10, /20, etc. |
| Duplicate systems | ⚠️ | text-warning vs text-status-warning; CSS_STRATEGY recommends consolidation |

**Red flags:** Two status color systems; deprecate hardcoded in favor of text-status-*.

---

### 2.4 Visual Hierarchy

| Check | Status | Notes |
|-------|--------|------|
| Focal point | ⚠️ | UI_RECTIFICATION_PLAN: "4–6 actions with equal weight"; primary CTA needed |
| Primary actions | ⚠️ | PageScaffoldPrimaryAction slot planned |
| Information density | ✅ | Collapsible filters, compact backend panel planned |

**Red flags:** Action hierarchy (Phase 2 of rectification plan) not yet implemented.

---

### 2.5 Component Consistency

| Check | Status | Notes |
|-------|--------|------|
| Buttons | ✅ | btn-* variants, sizes (sm, md, lg, icon) |
| Inputs | ✅ | .input, .select, .input-textarea uniform |
| Border radius | ✅ | --radius 0.5rem; rounded-sm/md/lg/xl |
| Shadows | ✅ | 3-level (sm, md, lg) + glass shadows |
| Icons | ⚠️ | icons.rs; verify single set (no Material + Heroicons mix) |

**Red flags:** Verify icon set consistency (one style, one stroke width).

---

### 2.6 Motion & Interaction

| Check | Status | Notes |
|-------|--------|------|
| Transitions | ✅ | 150–200ms, --ease-default |
| Easing | ✅ | cubic-bezier(0.4, 0, 0.2, 1) |
| Hover states | ✅ | All interactive elements |
| Reduced motion | ✅ | @media (prefers-reduced-motion: reduce) |
| Idle animation | ✅ | Forbidden in glass spec; state-change only |

**Red flags:** None. Motion is restrained and accessible.

---

## 3. Material Design Principles: Alignment Summary

| Principle | AdapterOS | Notes |
|-----------|----------|-------|
| **Material is the metaphor** | Divergent | Glass morphism instead of paper/surface; intentional |
| **Bold, graphic, intentional** | ✅ | Clear hierarchy, semantic colors |
| **Motion provides meaning** | ✅ | State-change transitions, no decorative motion |
| **Flexible foundation** | ✅ | CSS variables, dark mode, theme-glass |
| **Cross-platform** | ✅ | Web-first; responsive layout |

---

## 4. Recommended Changes (Priority Order)

| Priority | Change | Effort | Impact |
|----------|--------|--------|--------|
| P1 | Implement PageScaffoldPrimaryAction; add primary CTA to Dashboard, Adapters, Training | 2–4h | High — fixes hierarchy |
| P2 | Consolidate status colors (deprecate text-warning, use text-status-*) | 2h | Medium — reduces drift |
| P3 | Add overflow-x-auto to .tab-nav for narrow viewports | 15m | Medium — prevents overflow |
| P4 | Wire spacing utilities to var(--space-*) | 3h | Medium — consistency |
| P5 | Consider 40px min height for primary buttons | 30m | Low — touch targets |
| P6 | Add missing utilities (border-destructive/40, etc.) per CSS_STRATEGY | 1h | Low — fixes contract violations |

---

## 5. Out of Scope (Intentional Divergence)

- **No Material UI library** — AdapterOS is Leptos + Pure CSS; no React/JS dependency
- **No floating labels** — Conventional labels preferred
- **No ripple effects** — Simpler active state
- **Glass morphism** — Distinct from flat Material surfaces; brand differentiation

---

## 6. Verification Commands

```bash
# UI build
cargo check -p adapteros-ui --target wasm32-unknown-unknown

# Best-practices audit (accessibility, structure)
npx playwright test -c playwright.fast.config.ts ui/routes.best_practices.audit.spec.ts --grep @audit

# CSS strategy fixes
./scripts/build-ui.sh
```

---

## References

- Material Design 2: https://m2.material.io/
- Material UI components: https://mui.com/material-ui/all-components/
- Smashing Magazine: "Material Design Text Fields Are Badly Designed" (conventional labels preferred)
- AdapterOS: `dist/glass.css` (Liquid Glass spec), `STYLE_ALLOWLIST.md`, `CSS_STRATEGY.md`
