# Leptos Text Wrapping & Styling — Canonical Anchors

**Purpose:** Canonical reference for Leptos-specific text wrapping, truncation, and styling in the adapterOS UI.

**Scope:** `crates/adapteros-ui` (Leptos 0.7 CSR, Liquid Glass design system)

---

## Canonical Sources

| Area | Canonical File | Notes |
|------|----------------|-------|
| **Markdown component** | `src/components/markdown.rs` | Uses `inner_html`; no children |
| **Markdown CSS** | `dist/components/pages.css` | `.markdown-content` rules for p, pre, code, a |
| **Truncation utility** | `dist/components/utilities.css` | `.truncate`, `.min-w-0` |
| **Tab nav** | `dist/components/core.css` | `.tab-nav` with `overflow-x: auto` |
| **Tab component** | `src/components/tabs.rs` | `TabNav`, `TabButton`, `TabPanel` |
| **Table numerals** | `dist/components/core.css` | `.table-wrapper` has `font-variant-numeric: tabular-nums` |
| **Status tokens** | `dist/components/utilities.css` | `text-status-success`, `border-status-success/40`, `bg-status-success/5` |

---

## Leptos-Specific Patterns

### Class binding

| Pattern | Use case |
|--------|----------|
| `class="truncate break-words"` | Static classes |
| `class=format!("flex {}", extra)` | One-time dynamic classes |
| `class=move \|\| format!("...", signal.get())` | Reactive classes that update with signals |

### inner_html and markdown

- **No child elements** — `inner_html` replaces all children.
- **Children are plain DOM** — pulldown-cmark outputs `<p>`, `<pre>`, `<code>`, `<a>`, etc. Style via `.markdown-content` descendants.

### Truncation in flex

- `.truncate` needs constrained width.
- Flex items default to `min-width: auto` — add `min-w-0` to truncating element or its flex parent.

### title on truncated content

- Truncated text should expose full value via `title` for hover and accessibility.

---

## References

- [Leptos Styling](https://book.leptos.dev/interlude_styling.html)
- [Leptos inner_html](https://docs.rs/leptos/latest/leptos/html/struct.InnerHtml.html)
- `docs/UI_BEST_PRACTICES_CONTRAST.md` — Full gap analysis and status
