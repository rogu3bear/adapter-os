# CSS Strategy: Better Operation

Analysis of `crates/adapteros-ui/dist/` CSS (Feb 2025). Goal: reduce complexity, close gaps, and improve maintainability.

---

## 1. Current Architecture

| File | Lines | Role |
|------|-------|------|
| `base.css` | 452 | Design tokens, reset, animations |
| `components.css` | 18 | Import bundle (core, utilities, layout, overlays, pages) |
| `glass.css` | 730 | Liquid Glass theme, elevation, backdrop-filter |
| `charts.css` | 416 | Chart-specific styles |
| `components/core.css` | 1,880 | Buttons, cards, inputs, dialogs, etc. |
| `components/utilities.css` | 1,524 | Atomic utilities (Tailwind-style) |
| `components/layout.css` | 1,706 | Shell, sidebar, topbar, responsive |
| `components/overlays.css` | 1,573 | Toast, status center, progress stages |
| `components/pages.css` | 2,440 | Feature-specific components |

**Load order** (index.html): fonts → base → components → glass → charts

---

## 2. Gaps (Contract Violations)

Code uses classes that **do not exist** in CSS:

| Used in code | Defined? | Location |
|--------------|----------|----------|
| `border-destructive/40` | No | lifecycle.rs, services.rs, workers/dialogs.rs |
| `border-green-500/40` | No | lifecycle.rs, services.rs |
| `bg-green-500/5` | No | lifecycle.rs, services.rs |
| `text-green-600` | No | lifecycle.rs, services.rs, training/detail.rs |

**Fix**: Add these utilities to `utilities.css` or replace with existing equivalents (`text-status-success`, `border-status-success/40`, `bg-status-success/5`).

---

## 3. Unused Design Tokens

`base.css` defines a spacing scale that is **never used**:

```css
--space-0: 0;
--space-px: 1px;
--space-0_5: 0.125rem;
--space-1: 0.25rem;
--space-2: 0.5rem;
/* ... through --space-16 */
```

`utilities.css` uses hardcoded `rem` values instead:

```css
.gap-1 { gap: 0.25rem; }  /* Should be: gap: var(--space-1); */
.p-4 { padding: 1rem; }   /* Should be: padding: var(--space-4); */
```

**Fix**: Refactor utilities to use `var(--space-*)`. Reduces drift and enables theme overrides.

---

## 4. Duplicate Status Color Systems

In `utilities.css` there are **two parallel systems**:

| System | Classes | Dark mode |
|--------|---------|-----------|
| Hardcoded | `text-warning`, `text-info`, `text-success`, `text-error` + `-strong`, `-muted` | Manual `.dark` overrides |
| Variable | `text-status-warning`, `text-status-success`, etc. | Via `--color-status-*` in base.css |

**Fix**: Deprecate hardcoded variants. Use only `text-status-*` and `bg-status-*`. Migrate usages, then remove ~40 lines.

---

## 5. Repeated theme-glass + @supports Pattern

Each glass component in `pages.css` gets:

```css
.theme-glass .component-name { background: var(--glass-bg-1); ... }
@supports (backdrop-filter: blur(1px)) {
  .theme-glass .component-name {
    backdrop-filter: blur(var(--glass-blur-1)) saturate(var(--glass-saturation));
    -webkit-backdrop-filter: ...;
  }
}
```

Repeated for: `adapter-magnet-bar`, `adapter-chip-tooltip`, `chat-adapters-region`, etc.

**Fix**: Collect all Tier-1 glass components into a single `@supports` block in `glass.css`:

```css
@supports (backdrop-filter: blur(1px)) {
  .theme-glass .adapter-magnet-bar,
  .theme-glass .adapter-chip-tooltip,
  .theme-glass .chat-adapters-region {
    backdrop-filter: blur(var(--glass-blur-1)) saturate(var(--glass-saturation));
    -webkit-backdrop-filter: blur(var(--glass-blur-1)) saturate(var(--glass-saturation));
  }
}
```

Remove per-component `@supports` blocks from `pages.css`. Saves ~30 lines and centralizes glass behavior.

---

## 6. Inline Colors Instead of Variables

Components like `adapter-magnet-info` use hardcoded values:

```css
background: hsla(0 0% 0% / 0.08);
.dark .adapter-magnet-info { background: hsla(0 0% 100% / 0.1); }
```

**Fix**: Introduce tokens in `base.css`:

```css
--color-surface-subtle: hsl(0 0% 0% / 0.08);
.dark { --color-surface-subtle: hsl(0 0% 100% / 0.1); }
```

Then use `var(--color-surface-subtle)`. Eliminates per-component `.dark` overrides.

---

## 7. Utilities: Tailwind Without JIT

`utilities.css` is 1,524 lines of hand-maintained atomic classes. Each utility is 2–4 lines. No purge/dead-code removal.

**Options**:

| Option | Effort | Benefit |
|--------|--------|---------|
| **A. Use --space-* in utilities** | Low | Consistency, smaller diffs when changing scale |
| **B. Add PurgeCSS / stylelint** | Medium | Remove unused rules; requires build integration |
| **C. Adopt Tailwind (or UnoCSS)** | High | JIT, smaller output; adds build tooling |

**Recommendation**: Start with A. If bundle size becomes an issue, evaluate B (PurgeCSS with content glob on `src/**/*.rs`).

---

## 8. Responsive Utilities Duplication

Each breakpoint (`sm`, `md`, `lg`, `xl`, `2xl`) repeats the same patterns:

```css
@media (min-width: 640px) {
  .sm\:flex { display: flex; }
  .sm\:hidden { display: none; }
  .sm\:grid-cols-2 { ... }
  /* ... */
}
@media (min-width: 768px) {
  .md\:flex { display: flex; }   /* Same rule, different breakpoint */
  .md\:hidden { display: none; }
  /* ... */
}
```

**Fix**: Document which responsive classes are actually used. If `sm:flex` and `md:flex` are both used, keep both. If only `md:grid-cols-*` is used at 768px, consider dropping `sm:grid-cols-*` to reduce size. Run a usage audit first.

---

## 9. Build Pipeline

Trunk concatenates CSS via `@import`; no minification or purging. Output is ~9k lines in `static/components-*.css`.

**Possible improvements**:

- Post-build: `csso` or `clean-css` for minification
- PurgeCSS: content `crates/adapteros-ui/src/**/*.rs` + `index.html`
- Critical CSS: inline above-the-fold styles (lower priority)

---

## 10. Prioritized Action Plan

| Priority | Action | Impact | Effort |
|----------|--------|--------|--------|
| P0 | Add missing utilities (`border-destructive/40`, `border-green-500/40`, `bg-green-500/5`, `text-green-600`) | Fixes broken/inconsistent UI | 1h |
| P1 | Consolidate theme-glass `@supports` blocks in `glass.css` | Less duplication, single source of truth | 2h |
| P2 | Unify status colors (deprecate hardcoded, use `text-status-*`) | Simpler maintenance | 2h |
| P3 | Wire utilities to `--space-*` tokens | Consistency, theme flexibility | 3h |
| P4 | Replace inline colors with `--color-surface-subtle` etc. | Fewer `.dark` overrides | 4h |
| P5 | Usage audit + PurgeCSS or responsive-prune | Smaller bundle | 1 day |

---

## 11. Verification Commands

```bash
# Build UI and check output size
./scripts/build-ui.sh
wc -l crates/adapteros-server/static/components-*.css

# Grep for class usage (example: find unused utilities)
rg 'class="[^"]*"' crates/adapteros-ui/src --no-filename -o | tr ' ' '\n' | sort -u > /tmp/used-classes.txt
# Compare against utilities.css selectors
```

---

## References

- Liquid Glass spec: `dist/glass.css` header
- Design tokens: `dist/base.css` `:root`
- Component map: `dist/components.css` comments
