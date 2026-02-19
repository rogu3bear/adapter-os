# Accessibility Audit (WCAG 2.1 AA)

**Scope:** `crates/adapteros-ui` (Leptos WASM)  
**Target:** WCAG 2.1 Level AA  
**Last audit:** 2026-02-18  
**Canonical reference:** `docs/UI_BEST_PRACTICES_AUDIT.md` links here for accessibility checklist.

---

## Canonical Anchors

Keep these headings stable; code/docs may link to `docs/audits/ACCESSIBILITY_AUDIT.md#anchor-name`.

<a id="skip-link"></a>
### Skip Link (WCAG 2.4.1 Bypass Blocks)

- **Location:** `crates/adapteros-ui/src/components/layout/shell.rs`
- **CSS:** `crates/adapteros-ui/dist/base.css` — `.skip-to-main` (visually hidden until `:focus-visible`)
- **Target:** `#main-content` on `<main id="main-content">`
- **Status:** Rectified

<a id="keyboard-navigation"></a>
### Keyboard Navigation (WCAG 2.1.1)

- **Clickable table rows:** `adapters.rs`, `update_center.rs` — `role="button"`, `tabindex="0"`, `aria-label`, `on:keydown` for Enter/Space
- **HelpTooltip:** `form_field.rs` — `on:keydown` for Enter/Space
- **Dialogs:** `dialog.rs` — focus trap, Escape closes, focus restoration
- **Command palette:** `command_palette.rs` — same pattern
- **Status:** Rectified

<a id="screen-reader"></a>
### Screen Reader Support (WCAG 4.1.2)

- **Icon-only buttons:** `aria-label` on ReasoningModeToggle, ContextToggleButton, ClearButton, GlassThemeToggle, Event Viewer, toast controls
- **Icons:** `icons.rs` — decorative `aria-hidden`, meaningful `role="img"` + `aria-label`
- **Form fields:** `FormField` requires visible label; `Input` supports `aria-label`, `aria-describedby`
- **Status:** Rectified

<a id="focus-visible"></a>
### Focus Indicators (WCAG 2.4.7)

- **Base:** `dist/base.css` — `.focus-ring:focus-visible`, `--focus-ring-width`, `--focus-ring-offset`
- **Stacks list:** `focus-visible:` (not `focus:`) on Deactivate/Activate/Delete
- **Forced colors:** `@media (forced-colors: active)` — `:focus-visible` outline
- **Status:** Rectified

<a id="reduced-motion"></a>
### Reduced Motion (WCAG 2.3.3)

- **Location:** `dist/base.css` — `@media (prefers-reduced-motion: reduce)`
- **Effect:** Animations/transitions → 0.01ms, scroll-behavior: auto
- **Status:** Implemented

<a id="contrast"></a>
### Color Contrast (WCAG 1.4.3)

- **Tokens:** `--wcag-aa-normal-text: 4.5`, `--wcag-aa-large-text: 3`, `--wcag-aa-ui-components: 3`
- **Glass spec:** `dist/glass.css` header — "Minimum: WCAG AA"
- **Status:** Design tokens in place; verify per-surface as needed

---

## Verification

```bash
# Playwright best-practices audit (includes skip link, main landmark)
npx playwright test -c playwright.fast.config.ts ui/routes.best_practices.audit.spec.ts --grep @audit

# UI build
cargo check -p adapteros-ui --target wasm32-unknown-unknown
```

---

## Rectification Log

| Date       | Item                          | Status   |
|------------|-------------------------------|----------|
| 2026-02-18 | Skip link CSS                 | Resolved |
| 2026-02-18 | HelpTooltip keyboard          | Resolved |
| 2026-02-18 | Adapters/UpdateCenter rows   | Resolved |
| 2026-02-18 | Chat dock aria-labels        | Resolved |
| 2026-02-18 | Stacks focus-visible         | Resolved |
| 2026-02-18 | Topbar/Glass/Toast aria-labels| Resolved |
