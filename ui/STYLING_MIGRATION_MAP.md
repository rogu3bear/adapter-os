# CSS to Tailwind Migration Map

This document maps custom CSS utility classes to their Tailwind equivalents.

## Utility Classes Migration

### Layout & Flexbox
- `flex-center` → `flex items-center justify-center`
- `flex-between` → `flex items-center justify-between`
- `flex-standard` → `flex items-center` (standard gap already handled by Tailwind)
- `grid-standard` → `grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4`

### Components
- `card-standard` → `p-4 rounded-lg border border-border bg-card shadow-md`
- `table-standard` → `border-collapse w-full` (keep minimal, Tailwind handles width)
- `table-cell-standard` → `p-4 border-b border-border`

### Icons
- `icon-standard` → `h-4 w-4` (default icon size)
- `icon-small` → `h-3 w-3`
- `icon-large` → `h-6 w-6`

### Status Indicators
- `status-indicator` → Base class, keep in CSS for variants
- `status-indicator status-success` → Keep variant classes (color variants need CSS)
- `status-indicator status-error` → Keep variant classes
- `status-indicator status-warning` → Keep variant classes
- `status-indicator status-info` → Keep variant classes
- `status-indicator status-neutral` → Keep variant classes

### Forms
- `form-field` → `mb-4` (space-y-4 on parent, or mb-4 per field)
- `form-label` → `font-medium text-sm mb-1`

### Sections
- `section-header` → `flex items-center justify-between mb-6`
- `section-title` → `text-2xl font-bold`
- `section-description` → `text-sm text-muted-foreground`

### Modals
- `modal-standard` → `max-w-md` (standard modal width)
- `modal-large` → `max-w-4xl`

## Classes to Keep in CSS

The following classes should remain in `design-system.css` because they:
1. Define complex color variants (status-indicator variants)
2. Are semantic components with multiple properties
3. Have variant logic that's cleaner in CSS

- `status-indicator` base class + variants (status-success, status-error, etc.)
- Any complex component styles that combine multiple design tokens

## Migration Priority

1. **High frequency utilities** (221 matches):
   - `card-standard`, `flex-center`, `table-cell-standard`, `grid-standard`
   - `flex-standard`, `flex-between`, `icon-standard`, `table-standard`

2. **Medium frequency**:
   - `form-field`, `form-label`, `section-*` classes

3. **Low frequency / Keep in CSS**:
   - `status-indicator` variants (complex color logic)
