# PRD-UI-010: Utility Usage Audit + Allowlist Reduction

## Status: COMPLETE (with advisory)

## Results

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Allowlisted utilities | 279 | 157 | -122 (44% reduction) |
| Unused utilities | 99 | 0 | -99 |
| Target | ≤150 | 157 | 7 over target |

## Deliverables

### ✅ Machine-produced reports
- `reports/ui_util_usage.md` - Full usage report with counts and analysis
- `reports/ui_util_unused.txt` - Allowlisted but never referenced (now empty)
- `reports/ui_util_defined_unused.txt` - Defined in CSS but never referenced
- `reports/ui_util_reject_remove.txt` - Reject utilities status

### ✅ Audit script
- `scripts/ui_util_audit.py` - Reusable audit tool

### ✅ Trim script
- `scripts/trim_allowlist.py` - Removes unused entries from allowlist

## Removed Utilities (122 total)

### Semantic Component Classes (unused)
- `btn`, `btn-*` variants (12 classes)
- `badge`, `badge-*` variants (7 classes)
- `card-*` variants (5 classes)
- `toggle-*` variants (6 classes)
- `status-*` color classes (6 classes)
- `spinner-*` variants (3 classes)
- `dialog-footer` (1 class)

### Spacing Utilities (unused values in ranges)
- `m-0` to `m-4` (5 classes)
- `gap-5`, `gap-7` (2 classes)
- `mt-5`, `mt-7` (2 classes)
- `mb-5`, `mb-6` (2 classes)
- `px-0`, `px-5`, `px-6`, `px-7` (4 classes)
- `space-x-0` to `space-x-4` (5 classes)
- `space-y-5`, `space-y-7`, `space-y-8` (3 classes)

### Layout Utilities (unused)
- `flex-row`, `shrink`, `inline`
- Centering transforms: `top-[50%]`, `left-[50%]`, etc.

### Typography/Visual (unused)
- `font-normal`, `leading-tight`
- `caption-bottom`, `caption-top`
- `align-*` variants
- `text-black`, `whitespace-*` variants

## Advisory: 7 Over Target

The remaining 7 classes above target (157 vs 150) are **Transitional** utilities that ARE actively used in the codebase:

```
px-1.5, px-2.5, py-0.5, py-1.5, gap-0.5, gap-1.5, space-y-0.5
```

**Options to reach ≤150:**
1. **Accept 157** - Minor variance, all utilities are in active use
2. **Refactor fractional spacing** - Replace `px-1.5` with `px-2` in ~12 components
3. **Create semantic classes** - Add `.input-padding` for common patterns

**Recommendation:** Accept 157 as the new baseline. The 7 remaining are functional fractional utilities with no dead code.

## Acceptance Criteria Status

| Criteria | Status |
|----------|--------|
| Used/unused report generated | ✅ |
| Reject utilities removed | ✅ (none marked Reject) |
| Transitional utilities reduced | ✅ (11 → 7) |
| Allowlist ≤150 | ⚠️ 157 (7 over, all in use) |

## Next Steps (PRD-UI-020)

With the audit complete, PRD-UI-020 (Single Style Authority - Tokens Win) can now proceed with confidence:
- Replace `rounded-*` utilities in `.rs` with semantic classes
- Migrate remaining appearance utilities to token-based styling
- Keep only layout utilities in component class strings
