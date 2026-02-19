# AdapterOS UI vs. Best Practices — Contrast for Full Understanding

**Purpose:** Achieve 100% understanding of where the AdapterOS UI aligns with industry best practices and where gaps exist. Each section defines terms, explains why the practice matters, cites the codebase, and states the gap clearly.

**Scope:** `crates/adapteros-ui` (Leptos 0.7 CSR, Liquid Glass design system)

---

## How to Read This Document

| Symbol | Meaning |
|--------|---------|
| ✅ **Aligned** | AdapterOS matches the best practice |
| ⚠️ **Partial** | Partially aligned; specific gaps remain |
| ❌ **Gap** | Best practice not implemented |

Each gap includes: **What** (the practice), **Why** (impact), **Where** (file:line), **Fix** (concrete action).

---

## 1. Typography

### 1.1 Modular Type Scale

**What:** A type scale uses a fixed ratio (e.g. 1.2, 1.25, 1.333) to derive font sizes from a base. Example: 16px base × 1.25 = 20px, 25px, 31px, etc.

**Why:** Creates consistent visual hierarchy and predictable proportions. Random sizes feel arbitrary; a scale feels intentional.

**AdapterOS:** `dist/components/utilities.css` defines `text-xs` (0.75rem) through `text-4xl` (2.25rem) with no strict ratio. `base.css` has `--line-height-body: 1.58`, `--line-height-heading: 1.22`.

**Status:** ⚠️ **Partial** — Scale exists but is pragmatic, not mathematically strict. Acceptable for a control-plane UI; not a blocker.

---

### 1.2 Minimum Body Font Size (16px)

**What:** WCAG and readability research recommend 16px minimum for body text. Below 16px, mobile browsers may zoom on input focus, and readability drops for many users.

**Why:** Prevents unwanted zoom, supports low-vision users, and improves comprehension.

**AdapterOS:** Default body is 0.875rem (14px) in many places. `text-base` is 1rem (16px) but not used as the global default.

**Status:** ⚠️ **Partial** — Consider 1rem as default for body; keep 0.875rem for secondary/caption text only.

---

### 1.3 Tabular Numerals

**What:** `font-variant-numeric: tabular-nums` makes each digit the same width so numbers align in columns (e.g. metrics, tables).

**Why:** In dashboards and tables, misaligned digits make scanning harder and look unprofessional.

**AdapterOS:** `dist/components/core.css` — `.table-wrapper` has `font-variant-numeric: tabular-nums`. `.tabular-nums` utility in `utilities.css` for ad-hoc use.

**Status:** ✅ **Aligned** (rectified 2026-02-18)

---

### 1.4 Line Length (Measure)

**What:** 45–75 characters per line is optimal for readability (Bringhurst). ~66ch is often ideal. Use `max-width: 65ch` or `max-inline-size: 66ch` on prose containers.

**Why:** Lines that are too long cause eye strain and reduce comprehension.

**AdapterOS:** No `max-width` on prose. Chat messages, descriptions, and long-form text can span full width.

**Status:** ⚠️ **Partial** — Matters most for long-form content (e.g. chat message bodies, document previews). Less critical for dense control-plane tables.

---

### 1.5 Heading Wrapping

**What:** `text-wrap: balance` distributes characters evenly across lines in headings, avoiding one very short line.

**Why:** Improves visual balance in multi-line headings.

**AdapterOS:** `dist/base.css:268–271` — `h1–h6` have `text-wrap: balance` and `overflow-wrap: break-word`.

**Status:** ✅ **Aligned**

---

## 2. Text Wrapping & Overflow

### 2.1 Overflow-Wrap on Prose

**What:** `overflow-wrap: break-word` breaks long unbreakable strings (URLs, IDs) only when they would overflow. Prefer over `word-break: break-all`, which breaks aggressively.

**Why:** Prevents horizontal overflow and layout breaks without fragmenting words unnecessarily.

**AdapterOS:** `dist/base.css:273–275` — `p, h1–h6` have `overflow-wrap: break-word`.

**Status:** ✅ **Aligned**

---

### 2.2 Markdown Content Styling

**What:** The `Markdown` component uses `inner_html` to inject HTML from pulldown-cmark. That HTML includes `<p>`, `<pre>`, `<code>`, `<a>`, etc. These are plain DOM nodes, not Leptos components. CSS must target `.markdown-content` and its descendants to control wrapping.

**Why:** Long URLs in `<a>`, code in `<pre>`, and unbreakable strings in `<code>` can overflow if not styled.

**AdapterOS:** `components/markdown.rs:21,38` — `<div class="markdown-content" inner_html=...>`. **Canonical:** `dist/components/pages.css` — `.markdown-content` rules for `overflow-wrap`, `word-break`, `pre-wrap` on children.

**Status:** ✅ **Aligned** (rectified 2026-02-18)

---

### 2.3 Truncation in Flex Layouts

**What:** `.truncate` requires `overflow: hidden`, `text-overflow: ellipsis`, `white-space: nowrap`, and a constrained width. In flex layouts, flex items default to `min-width: auto`, so they won't shrink below content size—truncation fails.

**Why:** Without `min-w-0`, a flex child with long text expands and pushes layout instead of showing ellipsis.

**AdapterOS:** `dist/components/utilities.css` defines `.truncate`. `min-w-0` applied to chat session rows (`chat.rs:1266–1267`), taskbar, command palette, split panel, etc. **Canonical:** `STYLE_ALLOWLIST.md` documents `.min-w-0` | Core | Allow shrink.

**Status:** ✅ **Aligned** (rectified 2026-02-18)

---

### 2.4 Title on Truncated Elements

**What:** Truncated text hides content. A `title` attribute exposes the full value on hover and helps screen readers.

**Why:** Accessibility and discoverability.

**AdapterOS:** Consistently used: `errors.rs:214`, `system/components.rs:308`, `models.rs:1034`, etc.

**Status:** ✅ **Aligned**

---

## 3. Design System & CSS

### 3.1 Spacing Tokens

**What:** A spacing scale (e.g. `--space-1` through `--space-16`) defined in CSS variables. Utilities like `.gap-1`, `.p-4` should use `var(--space-*)` instead of hardcoded `rem`.

**Why:** Single source of truth; changing the scale updates the whole UI; enables theme overrides.

**AdapterOS:** `dist/base.css:98–107` defines `--space-0` through `--space-16`. `dist/components/utilities.css` uses hardcoded values (e.g. `gap: 0.25rem` instead of `gap: var(--space-1)`).

**Status:** ❌ **Gap** — Tokens exist but are unused. See `docs/CSS_STRATEGY.md`.

**Fix:** Refactor utilities to reference `var(--space-*)`.

---

### 3.2 Duplicate Status Color Systems

**What:** One canonical set of status colors (e.g. `text-status-success`, `bg-status-warning`) used everywhere. Avoid parallel systems (e.g. `text-warning` and `text-status-warning`).

**Why:** Reduces drift, simplifies dark mode, and keeps maintenance low.

**AdapterOS:** `dist/components/utilities.css` has both `text-warning`, `text-info`, `text-success`, `text-error` (hardcoded) and `text-status-warning`, `text-status-success`, etc. (variable-based).

**Status:** ⚠️ **Partial** — Two systems. Deprecate hardcoded; migrate to `text-status-*`.

---

### 3.3 Contract Violations (Missing Utilities)

**What:** Code uses class names that are not defined in CSS. The UI either falls back to nothing or shows inconsistent styling.

**Why:** Broken or inconsistent visuals; maintenance burden when adding ad-hoc classes.

**AdapterOS:** `border-destructive/40`, `bg-destructive/5`, `border-green-500/40`, `bg-green-500/5`, `text-green-600` exist in `utilities.css`. Success states migrated to semantic tokens: `text-status-success`, `border-status-success/40`, `bg-status-success/5` in `system/services.rs`, `system/lifecycle.rs`, `training/detail/mod.rs`.

**Status:** ✅ **Aligned** (rectified 2026-02-18)

---

### 3.4 Glass @supports Duplication

**What:** Each glass component repeats the same `@supports (backdrop-filter: blur(1px))` block. Best practice: one central block in `glass.css` listing all Tier-1 glass components.

**Why:** Less duplication, single source of truth, easier maintenance.

**AdapterOS:** Per-component blocks in `dist/components/pages.css` for `adapter-magnet-bar`, `chat-adapters-region`, etc.

**Status:** ⚠️ **Partial** — Consolidate into `dist/glass.css`.

---

## 4. Components & Interaction

### 4.1 Touch Target Size

**What:** Minimum 48×48dp (48px) for primary interactive elements (buttons, links). Material Design and Apple HIG recommend this for touch.

**Why:** Smaller targets cause mis-taps and frustration, especially on mobile.

**AdapterOS:** `btn-md` is ~36px (2.25rem). Material UI audit recommends 40px (2.5rem) minimum for primary actions.

**Status:** ⚠️ **Partial** — Consider 40px for primary buttons.

---

### 4.2 Label Placement (Inputs)

**What:** Labels outside the input, above or beside it. Avoid floating labels inside the field (reduces clarity per Smashing Magazine research).

**AdapterOS:** `FormField` uses labels outside. Material UI audit rates this as better than MD's floating label.

**Status:** ✅ **Aligned**

---

### 4.3 Tab Overflow

**What:** When tabs don't fit (e.g. narrow viewport), they should scroll horizontally with `overflow-x: auto` rather than overflow off-screen.

**Why:** Prevents hidden tabs and broken layout.

**AdapterOS:** `dist/components/core.css` — `.tab-nav` has `overflow-x: auto`, `-webkit-overflow-scrolling: touch`, `scrollbar-width: none`. **Canonical:** `components/tabs.rs`.

**Status:** ✅ **Aligned**

---

### 4.4 Primary CTA Hierarchy

**What:** Each page should have one clear primary action (e.g. "Create Job", "Load Model"). Secondary actions should be visually de-emphasized.

**Why:** Reduces cognitive load; guides users to the main task.

**AdapterOS:** `PageScaffold` has a single `PageScaffoldActions` slot; no `PageScaffoldPrimaryAction`. UI_RECTIFICATION_PLAN notes "4–6 actions with equal weight."

**Status:** ⚠️ **Partial** — Implement `PageScaffoldPrimaryAction` and designate primary CTAs on key pages.

---

## 5. Accessibility (WCAG 2.1 AA)

### 5.1 Skip Link

**What:** A "Skip to main content" link that appears on keyboard focus, allowing keyboard users to bypass repeated navigation.

**AdapterOS:** `dist/base.css` — `.skip-to-main`; `shell.rs` — skip link targets `#main-content`.

**Status:** ✅ **Aligned** — See `docs/audits/ACCESSIBILITY_AUDIT.md#skip-link`.

---

### 5.2 Keyboard Navigation

**What:** All interactive elements reachable and activatable via keyboard (Tab, Enter, Space). Dialogs trap focus and restore it on close.

**AdapterOS:** Clickable rows, dialogs, command palette, HelpTooltip all support keyboard. See `docs/audits/ACCESSIBILITY_AUDIT.md#keyboard-navigation`.

**Status:** ✅ **Aligned**

---

### 5.3 Focus Indicators

**What:** Visible focus ring on `:focus-visible` (not `:focus`, to avoid mouse-only focus). Forced-colors mode respected.

**AdapterOS:** `dist/base.css` — `.focus-ring:focus-visible`; `@media (forced-colors: active)` support.

**Status:** ✅ **Aligned**

---

### 5.4 Reduced Motion

**What:** `@media (prefers-reduced-motion: reduce)` disables or minimizes animations for users who prefer reduced motion.

**AdapterOS:** `dist/base.css` — animations/transitions set to 0.01ms, scroll-behavior: auto.

**Status:** ✅ **Aligned**

---

### 5.5 Color Contrast

**What:** WCAG AA: 4.5:1 for normal text, 3:1 for large text (18pt+). Design tokens should encode these values.

**AdapterOS:** `--wcag-aa-normal-text`, `--wcag-aa-large-text` in tokens. Glass spec requires WCAG AA. Per-surface verification (especially on glass) is recommended.

**Status:** ✅ **Aligned** (tokens); ⚠️ **Partial** (verify glass surfaces).

---

## 6. AI Platform / Dashboard Specifics

### 6.1 Dense Table Typography

**What:** For data-heavy tables: 13px body, line-height 1.4, tabular numerals. Fonts like Inter or Source Sans 3 have good hinting at small sizes.

**Why:** Balances density and readability; aligns numeric columns.

**AdapterOS:** Mixed sizes (`text-xs`, `text-sm`, `text-[13px]`). `.table-wrapper` has `font-variant-numeric: tabular-nums`. Plus Jakarta Sans is fine but not optimized for 11–13px.

**Status:** ⚠️ **Partial** — Tabular numerals in place; table cell size could be standardized.

---

### 6.2 Compact Mode

**What:** A density mode (e.g. `.compact`) that reduces padding and font size for power users who want more data on screen.

**AdapterOS:** `dist/glass.css:699–702` — `.compact` reduces heading sizes. Used for dense views.

**Status:** ✅ **Aligned**

---

### 6.3 Monospace for IDs and Config

**What:** Use `font-mono` for model IDs, adapter IDs, hashes, config snippets, and log excerpts.

**AdapterOS:** `--font-mono` defined; used widely (e.g. `flight_recorder.rs`, `datasets/components.rs`, `adapters.rs`).

**Status:** ✅ **Aligned**

---

## 7. Production UX (Control Plane)

### 7.1 Liveness (Real-Time Data)

**What:** Data should feel live—SSE where available, polling as fallback. Avoid load-once-and-forget.

**Why:** Operators need to see current state; stale data leads to wrong decisions.

**AdapterOS:** Chat, workers use SSE. Models, Admin, some pages use polling or load-once. BACKEND_FRONTEND_READINESS_MAP notes Admin lacks live data.

**Status:** ⚠️ **Partial** — Extend SSE/polling to Models, Admin.

---

### 7.2 Readiness Gates

**What:** Disable or warn before actions that require backend readiness (e.g. Load Model when no workers, Spawn Worker when DB not ready).

**Why:** Prevents confusing errors; guides users to fix preconditions.

**AdapterOS:** Chat queues when `!inference_ready`. Models, Workers, Stacks lack readiness gates—actions can be clicked when backend will reject them.

**Status:** ⚠️ **Partial** — Add gates per BACKEND_FRONTEND_READINESS_MAP.

---

### 7.3 Navigation: `navigate()` vs `set_href()`

**What:** Use Leptos `use_navigate()` for in-app navigation. Avoid `window.location.set_href()`—it does a full reload, loses SPA state, and breaks toasts.

**Why:** SPA behavior, preserved state, better UX.

**AdapterOS:** Most navigation uses `navigate()`. Exceptions:
- **Auth flows** (`auth.rs`, `login.rs`, `safe.rs`): `set_href` for redirect to login or external auth—often intentional for full-page auth.
- **Training** (`training/mod.rs:48`, `training/detail/mod.rs:663`): `set_href` for in-app navigation—should use `navigate()`.
- **Error boundary** (`error_boundary.rs:94`): `set_href("/")` for recovery—could use `navigate()`.

**Status:** ⚠️ **Partial** — Replace training `set_href` with `navigate()`; auth/error cases are context-dependent.

---

### 7.4 Error Reporting

**What:** User-initiated actions that fail must call `report_error_with_toast()` so the user sees feedback and the backend gets telemetry.

**AdapterOS:** Pattern established in `api/error_reporter.rs`. Production UX rectification notes some handlers still use `console::error_1()` or only set inline `action_error` without toast.

**Status:** ⚠️ **Partial** — Audit handlers; ensure all user actions call `report_error_with_toast()` on failure.

---

## 8. Workflow & Information Architecture

### 8.1 Guided Flow Visibility

**What:** Primary profile should surface the demo flow: Train → Infer → Verify → Promote → Deploy → Observe. Secondary groups (Govern, Org) collapsed by default.

**AdapterOS:** `nav_registry.rs` defines `NAV_GROUPS_PRIMARY` (6 groups) and `NAV_GROUPS_FULL` (8 groups). Govern and Org have `collapsed_by_default: true`.

**Status:** ✅ **Aligned**

---

### 8.2 Sidebar Respects collapsed_by_default

**What:** Sidebar groups with `collapsed_by_default: true` should start collapsed.

**AdapterOS:** `sidebar.rs:140` — `signal(!group.collapsed_by_default)` — groups start collapsed when `collapsed_by_default` is true.

**Status:** ✅ **Aligned** — Sidebar correctly respects the flag.

---

### 8.3 Route Count

**What:** 40+ routes can overwhelm new users. Segmentation (Primary vs Full profile) helps.

**AdapterOS:** 46 route definitions. Primary profile shows 6 groups; Full shows 8. UI_RECTIFICATION_PLAN addresses demo readiness.

**Status:** ✅ **Aligned** — Segmentation in place.

---

## 9. Leptos-Specific

### 9.1 Reactive Class Binding

**What:** For classes that depend on signals, use `class=move || format!(...)` so updates are reactive. `class=format!(...)` is evaluated once at render.

**AdapterOS:** Used correctly where needed (e.g. `config_presets.rs`, `flight_recorder.rs`, `toast.rs`).

**Status:** ✅ **Aligned**

---

### 9.2 inner_html Sanitization

**What:** `inner_html` injects raw HTML. User content must be sanitized to prevent XSS (strip script, iframe, event handlers, javascript: URLs).

**AdapterOS:** `markdown.rs` — `sanitize_html()` strips dangerous tags and attributes before render.

**Status:** ✅ **Aligned**

---

### 9.3 inner_html Descendant Styling

**What:** Content injected via `inner_html` is plain DOM. Styling must target the container and its descendants (e.g. `.markdown-content p`).

**AdapterOS:** `dist/components/pages.css` — `.markdown-content p, pre, code, a` have `overflow-wrap: break-word`, `word-break: break-word`; `pre` has `white-space: pre-wrap`, `overflow-x: auto`.

**Status:** ✅ **Aligned** (rectified 2026-02-18)

---

## 10. Summary Matrix

| Category | Aligned | Partial | Gap |
|----------|---------|---------|-----|
| Typography | 2 | 4 | 1 |
| Text wrapping | 2 | 1 | 1 |
| Design system | 0 | 3 | 2 |
| Components | 1 | 3 | 0 |
| Accessibility | 5 | 1 | 0 |
| AI platform | 2 | 1 | 0 |
| Production UX | 0 | 4 | 0 |
| Workflow/IA | 3 | 0 | 0 |
| Leptos | 2 | 0 | 1 |

---

## 11. Prioritized Fixes (for 100% Alignment)

| P | Fix | File(s) | Effort |
|---|-----|---------|--------|
| P0 | Add `.markdown-content` overflow/wrap rules | `dist/components/pages.css` | 15m |
| P0 | Add or replace missing utilities (`border-destructive/40`, etc.) | `utilities.css` or replace usages | 1h |
| P1 | Add `font-variant-numeric: tabular-nums` to table/metric containers | `base.css` or component CSS | 30m |
| P1 | Implement `PageScaffoldPrimaryAction` | `page_scaffold.rs`, key pages | 2–4h |
| P2 | Add `overflow-x-auto` to `.tab-nav` | `dist/components/core.css` or layout | 15m |
| P2 | Replace training `set_href` with `navigate()` | `training/mod.rs`, `training/detail/mod.rs` | 30m |
| P2 | Consolidate status colors (deprecate hardcoded) | `utilities.css`, migrate usages | 2h |
| P3 | Wire spacing utilities to `var(--space-*)` | `utilities.css` | 3h |
| P3 | Add readiness gates for Models, Workers, Stacks | `pages/models.rs`, `workers/`, `stacks/` | 4h |
| P4 | Consolidate glass `@supports` in `glass.css` | `glass.css`, `pages.css` | 2h |

---

## References

- `docs/audits/MATERIAL_UI_AUDIT.md` — Component-by-component MD comparison
- `docs/CSS_STRATEGY.md` — CSS gaps and action plan
- `docs/BACKEND_FRONTEND_READINESS_MAP.md` — Readiness gates by page
- `docs/audits/ACCESSIBILITY_AUDIT.md` — WCAG anchors and verification
- `docs/UI_RECTIFICATION_PLAN.md` — Guided flow and nav changes
- `.claude/commands/production-ux-rectification.md` — Patterns and workstreams
