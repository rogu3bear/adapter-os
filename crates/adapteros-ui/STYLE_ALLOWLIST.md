# adapterOS UI Style Allowlist

> **PRD-UI-001**: Utility Surface Freeze & Reduction
>
> This document locks down the approved utility classes. No new styling may be added
> without updating this allowlist. The goal is to keep the system boring on purpose.

## Status Legend

| Status | Meaning |
|--------|---------|
| **Core** | Essential, keep permanently |
| **Transitional** | Allowed temporarily, migrate to semantic classes |
| **Reject** | Remove when refactoring component |

## Rules

1. **No negative margins** unless justified (e.g., badge positioning)
2. **No arbitrary sizing** unless component-scoped
3. **Prefer semantic tokens** over raw color utilities
4. **New styling requires PR review** and allowlist update

---

## 1. Semantic Component Classes (Core)

These are the preferred styling approach. Use these instead of utility classes.

### Buttons
| Class | Status | Notes |
|-------|--------|-------|

### Cards
| Class | Status | Notes |
|-------|--------|-------|
| `.card` | Core | Card container |

### Inputs
| Class | Status | Notes |
|-------|--------|-------|
| `.input` | Core | Text input |

### Dialogs
| Class | Status | Notes |
|-------|--------|-------|
| `.dialog-overlay` | Core | Modal backdrop |
| `.dialog-content` | Core | Modal container |
| `.dialog-header` | Core | Modal header |
| `.dialog-title` | Core | Modal title |
| `.dialog-description` | Core | Modal description |

### Tables
| Class | Status | Notes |
|-------|--------|-------|
| `.table` | Core | Table wrapper |
| `.table-header` | Core | Table header |
| `.table-header-cell` | Core | Header cell |
| `.table-body` | Core | Table body |
| `.table-row` | Core | Table row |
| `.table-cell` | Core | Table cell |

### Badges
| Class | Status | Notes |
|-------|--------|-------|

### Status Indicators
| Class | Status | Notes |
|-------|--------|-------|

### Toggle
| Class | Status | Notes |
|-------|--------|-------|
| `.toggle` | Core | Toggle base |

### Spinner
| Class | Status | Notes |
|-------|--------|-------|
| `.spinner` | Core | Loading spinner |

---

## 2. Layout Utilities (Core)

### Flexbox
| Class | Status | Notes |
|-------|--------|-------|
| `.flex` | Core | Display flex |
| `.flex-col` | Core | Column direction |
| `.flex-1` | Core | Flex grow |
| `.flex-wrap` | Core | Wrap items |
| `.items-center` | Core | Align center |
| `.items-start` | Core | Align start |
| `.items-end` | Core | Align end |
| `.justify-center` | Core | Justify center |
| `.justify-between` | Core | Justify space-between |
| `.justify-end` | Core | Justify end |
| `.justify-start` | Core | Justify start |
| `.shrink-0` | Core | No shrink |
| `.inline-flex` | Core | Inline flex |

### Grid
| Class | Status | Notes |
|-------|--------|-------|
| `.grid` | Core | Display grid |
| `.grid-cols-2` | Core | 2 columns |
| `.grid-cols-3` | Core | 3 columns |
| `.grid-cols-4` | Core | 4 columns |

### Display
| Class | Status | Notes |
|-------|--------|-------|
| `.hidden` | Core | Display none |
| `.block` | Core | Display block |
| `.inline-block` | Core | Display inline-block |

---

## 3. Spacing Utilities (Core)

### Padding
| Class | Status | Notes |
|-------|--------|-------|
| `.p-1` | Core | 0.25rem |
| `.p-2` | Core | 0.5rem |
| `.p-3` | Core | 0.75rem |
| `.p-4` | Core | 1rem |
| `.p-6` | Core | 1.5rem |
| `.p-8` | Core | 2rem |
| `.px-1` to `.px-4`, `.px-8` | Core | Horizontal padding |
| `.py-0` to `.py-12` | Core | Vertical padding |
| `.px-1.5`, `.px-2.5` | Transitional | Fractional - prefer p-2 |
| `.py-0.5`, `.py-1.5` | Transitional | Fractional - prefer p-1 |

### Margin
| Class | Status | Notes |
|-------|--------|-------|
| `.mx-auto` | Core | Center horizontally |
| `.my-2`, `.my-4` | Core | Vertical margin |
| `.mt-1` to `.mt-4`, `.mt-6`, `.mt-8` | Core | Top margin |
| `.mb-1` to `.mb-4` | Core | Bottom margin |
| `.ml-1`, `.ml-2`, `.ml-auto` | Core | Left margin |
| `.mr-1`, `.mr-2` | Core | Right margin |

### Negative Margins (Transitional)
| Class | Status | Notes |
|-------|--------|-------|
| `.-top-1` | Transitional | Badge positioning only |
| `.-right-1` | Transitional | Badge positioning only |

### Space Between
| Class | Status | Notes |
|-------|--------|-------|
| `.space-y-0` to `.space-y-4`, `.space-y-6` | Core | Vertical spacing |
| `.space-y-0.5`, `.space-y-1.5` | Transitional | Prefer gap utilities |

### Gap
| Class | Status | Notes |
|-------|--------|-------|
| `.gap-1` to `.gap-4`, `.gap-6`, `.gap-8` | Core | Flex/grid gap |
| `.gap-0.5`, `.gap-1.5` | Transitional | Prefer whole numbers |

---

## 4. Sizing Utilities (Core)

### Width
| Class | Status | Notes |
|-------|--------|-------|
| `.w-full` | Core | 100% width |
| `.w-2` to `.w-12` | Core | Fixed widths |
| `.w-64`, `.w-72`, `.w-80`, `.w-96` | Core | Large widths |

### Height
| Class | Status | Notes |
|-------|--------|-------|
| `.h-full` | Core | 100% height |
| `.h-2` to `.h-14` | Core | Fixed heights |
| `.h-screen` | Core | Viewport height |

### Min/Max
| Class | Status | Notes |
|-------|--------|-------|
| `.min-h-screen` | Core | Min viewport height |
| `.min-w-0` | Core | Allow shrink |
| `.max-w-sm` to `.max-w-2xl` | Core | Max widths |
| `.max-h-64`, `.max-h-80`, `.max-h-96` | Core | Max heights |

---

## 5. Typography Utilities (Core)

### Font Size
| Class | Status | Notes |
|-------|--------|-------|
| `.text-xs` | Core | 0.75rem |
| `.text-sm` | Core | 0.875rem (default body) |
| `.text-base` | Core | 1rem |
| `.text-lg` | Core | 1.125rem |
| `.text-xl` | Core | 1.25rem |
| `.text-2xl` | Core | 1.5rem (card titles) |
| `.text-3xl` | Core | 1.875rem (page titles) |

### Font Weight
| Class | Status | Notes |
|-------|--------|-------|
| `.font-medium` | Core | 500 |
| `.font-semibold` | Core | 600 |
| `.font-bold` | Core | 700 |

### Font Family
| Class | Status | Notes |
|-------|--------|-------|
| `.font-mono` | Core | Monospace |

### Font Features
| Class | Status | Notes |
|-------|--------|-------|
| `.tabular-nums` | Core | Tabular numbers |

### Text Alignment
| Class | Status | Notes |
|-------|--------|-------|
| `.text-left` | Core | Left align |
| `.text-center` | Core | Center align |
| `.text-right` | Core | Right align |

### Line Height / Spacing
| Class | Status | Notes |
|-------|--------|-------|
| `.leading-none` | Core | Line height 1 |
| `.tracking-tight` | Core | Letter spacing -0.025em |
| `.tracking-wider` | Transitional | Rarely used |

### Text Overflow
| Class | Status | Notes |
|-------|--------|-------|
| `.truncate` | Core | Ellipsis overflow |
| `.whitespace-nowrap` | Core | No wrap |
| `.whitespace-normal` | Core | Normal wrap |
| `.whitespace-pre-wrap` | Core | Pre + wrap |
| `.break-words` | Core | Break on words |
| `.break-all` | Transitional | Aggressive break |

---

## 6. Color Utilities

### Text Colors (Core - Semantic)
| Class | Status | Notes |
|-------|--------|-------|
| `.text-foreground` | Core | Primary text |
| `.text-muted-foreground` | Core | Secondary text |
| `.text-primary` | Core | Primary color |
| `.text-primary-foreground` | Core | On primary |
| `.text-secondary-foreground` | Core | On secondary |
| `.text-destructive` | Core | Error text |
| `.text-destructive-foreground` | Core | On destructive |
| `.text-accent-foreground` | Core | On accent |

### Text Colors (Transitional - Raw)
| Class | Status | Notes |
|-------|--------|-------|
| `.text-white` | Transitional | Use semantic instead |
| `.text-green-500` | Transitional | Use `.status-green` |
| `.text-yellow-500`, `.text-yellow-600` | Transitional | Use `.status-yellow` |
| `.text-red-500` | Transitional | Use `.status-red` |
| `.text-blue-500` | Transitional | Use `.status-blue` |

### Background Colors (Core - Semantic)
| Class | Status | Notes |
|-------|--------|-------|
| `.bg-background` | Core | Page background |
| `.bg-foreground` | Core | Inverse background |
| `.bg-card` | Core | Card background |
| `.bg-muted` | Core | Muted background |
| `.bg-primary` | Core | Primary color |
| `.bg-secondary` | Core | Secondary color |
| `.bg-accent` | Core | Accent color |
| `.bg-destructive` | Core | Destructive color |
| `.bg-transparent` | Core | Transparent |

### Background Colors (Core - Opacity)
| Class | Status | Notes |
|-------|--------|-------|
| `.bg-muted/50` | Core | 50% muted |
| `.bg-primary/10`, `.bg-primary/20`, `.bg-primary/90` | Core | Primary opacity |
| `.bg-destructive/10` | Core | 10% destructive |

### Background Colors (Transitional - Status)
| Class | Status | Notes |
|-------|--------|-------|
| `.bg-gray-400`, `.bg-gray-500` | Transitional | Use `.status-gray` |
| `.bg-green-400`, `.bg-green-500` | Transitional | Use `.status-green` |
| `.bg-yellow-400`, `.bg-yellow-500` | Transitional | Use `.status-yellow` |
| `.bg-red-400`, `.bg-red-500` | Transitional | Use `.status-red` |
| `.bg-blue-400`, `.bg-blue-500` | Transitional | Use `.status-blue` |
| `.bg-purple-500` | Transitional | Define semantic if needed |

### Background Colors (Core - Zinc)
| Class | Status | Notes |
|-------|--------|-------|
| `.bg-zinc-800`, `.bg-zinc-900`, `.bg-zinc-950` | Core | Code blocks only |

---

## 7. Border Utilities (Core)

### Border Width
| Class | Status | Notes |
|-------|--------|-------|
| `.border` | Core | 1px border |
| `.border-2` | Core | 2px border |
| `.border-t`, `.border-b`, `.border-l`, `.border-r` | Core | Single side |

### Border Color
| Class | Status | Notes |
|-------|--------|-------|
| `.border-border` | Core | Default border |
| `.border-input` | Core | Input border |
| `.border-destructive` | Core | Error border |
| `.border-transparent` | Core | Transparent |

### Border Radius
| Class | Status | Notes |
|-------|--------|-------|
| `.rounded` | Core | Default radius (8px) |
| `.rounded-sm` | Core | Small (4px) |
| `.rounded-md` | Core | Medium (6px) |
| `.rounded-lg` | Core | Large (8px) |
| `.rounded-xl` | Core | Extra large (12px) |
| `.rounded-full` | Core | Pill shape |

---

## 8. Shadow Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|
| `.shadow-sm` | Core | Subtle shadow |
| `.shadow-md` | Core | Medium shadow |
| `.shadow-lg` | Core | Large shadow |
| `.shadow-xl` | Core | Extra large |

---

## 9. Position Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|
| `.relative` | Core | Relative position |
| `.absolute` | Core | Absolute position |
| `.fixed` | Core | Fixed position |
| `.sticky` | Core | Sticky position |
| `.inset-0` | Core | Full inset |
| `.top-0`, `.right-0`, `.bottom-0`, `.left-0` | Core | Edge positioning |
| `.top-4`, `.right-4` | Core | With spacing |
| `.z-10` to `.z-50` | Core | Z-index scale |

### Centering (Transitional)
| Class | Status | Notes |
|-------|--------|-------|
| `.-translate-x-1/2` | Transitional | Use `.dialog-content` |

---

## 10. Effect Utilities (Core)

### Opacity
| Class | Status | Notes |
|-------|--------|-------|
| `.opacity-25` | Core | 25% visible |
| `.opacity-50` | Core | 50% visible |
| `.opacity-75` | Core | 75% visible |

### Backdrop
| Class | Status | Notes |
|-------|--------|-------|
| `.backdrop-blur-sm` | Core | Subtle blur |

---

## 11. Interaction Utilities (Core)

### Cursor
| Class | Status | Notes |
|-------|--------|-------|
| `.cursor-pointer` | Core | Clickable |
| `.cursor-not-allowed` | Core | Disabled |

### Pointer Events
| Class | Status | Notes |
|-------|--------|-------|
| `.pointer-events-none` | Core | Disable events |

### Selection
| Class | Status | Notes |
|-------|--------|-------|

---

## 12. Transition Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|
| `.transition-colors` | Core | Color transitions |
| `.transition-all` | Core | All properties |
| `.transition-opacity` | Core | Opacity transitions |
| `.transition-transform` | Core | Transform transitions |
| `.duration-200` | Core | 200ms duration |
| `.duration-300` | Core | 300ms duration |

---

## 13. State Utilities (Core)

### Hover
| Class | Status | Notes |
|-------|--------|-------|
| `.hover:bg-muted` | Core | Muted on hover |
| `.hover:bg-muted/50` | Core | 50% muted on hover |
| `.hover:bg-accent` | Core | Accent on hover |
| `.hover:bg-primary/90` | Core | 90% primary on hover |
| `.hover:text-foreground` | Core | Foreground on hover |
| `.hover:text-accent-foreground` | Core | Accent fg on hover |
| `.hover:opacity-100` | Core | Full opacity on hover |
| `.hover:underline` | Core | Underline on hover |

### Focus
| Class | Status | Notes |
|-------|--------|-------|
| `.focus:outline-none` | Core | Remove outline |
| `.focus:ring-2` | Core | Focus ring |
| `.focus-visible:outline-none` | Core | Remove outline |
| `.focus-visible:ring-2` | Core | Focus ring |
| `.focus-visible:ring-offset-2` | Core | Ring with offset |
| `.ring-offset-background` | Core | Ring offset color |

### Disabled
| Class | Status | Notes |
|-------|--------|-------|
| `.disabled:opacity-50` | Core | 50% opacity |
| `.disabled:cursor-not-allowed` | Core | Not-allowed cursor |
| `.disabled:pointer-events-none` | Core | No events |

---

## 14. Table Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|

---

## 15. Overflow Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|
| `.overflow-hidden` | Core | Hide overflow |
| `.overflow-auto` | Core | Auto scrollbars |
| `.overflow-y-auto` | Core | Vertical scroll |

---

## 16. Accessibility Utilities (Core)

| Class | Status | Notes |
|-------|--------|-------|
| `.sr-only` | Core | Screen reader only |

---

## Rejected Patterns

These patterns should NOT be added:

| Pattern | Reason |
|---------|--------|
| Arbitrary values (`[50%]`, `[#hex]`) | Use semantic tokens |
| More than 5 z-index levels | Simplify stacking |
| Negative margins beyond `-1` | Use gap/flex instead |
| Custom colors outside palette | Define semantic token |
| Per-component color overrides | Use variants |
| Animation classes beyond base | Use CSS animations |

---

## Migration Path

### Phase 1: Freeze (Current)
- No new utilities without allowlist update
- Document all existing usage

### Phase 2: Consolidation
- Replace raw colors with semantic tokens
- Replace negative margins with layout patterns
- Replace fractional spacing with whole numbers

### Phase 3: Reduction
- Remove transitional utilities not in active use
- Consolidate similar utilities
- Target: <150 total utility classes

---

## Enforcement

1. **CI Check**: Lint for undefined classes (future)
2. **PR Review**: Any CSS changes require allowlist review
3. **Audit**: Monthly utility usage report

---

*Last updated: 2026-01-03*
*Utility count: ~200 (target: <150)*
