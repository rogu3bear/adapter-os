# PRD-UI-010: UI Utility Usage Report

Generated: Sat Jan  3 22:30:27 CST 2026

## Summary

| Metric | Count |
|--------|-------|
| Defined in CSS | 637 |
| Allowlisted | 157 |
| Referenced in code | 1188 |
| ├─ Referenced & allowlisted | 157 |
| └─ Unknown (not allowlisted) | 766 |
| Core classes | 147 |
| Transitional classes | 10 |
| Reject classes | 0 |
| **Allowlisted but unused** | **0** |
| Defined but unused | 215 |

## Reduction Analysis

- Current allowlist size: **157**
- Target: **≤150**
- Unused (can remove): **0**
- After removal: **157**
- ❌ Still 7 over target

## Top 30 Referenced Utilities

| Class | References | Status |
|-------|------------|--------|
| `text-sm` | 416 | Core |
| `text-muted-foreground` | 396 | Core |
| `flex` | 377 | Core |
| `items-center` | 339 | Core |
| `font-medium` | 196 | Core |
| `gap-2` | 153 | defined |
| `text-xs` | 151 | Core |
| `border` | 120 | Core |
| `rounded-md` | 96 | Core |
| `font-mono` | 92 | Core |
| `gap-4` | 85 | defined |
| `justify-between` | 84 | Core |
| `py-2` | 81 | defined |
| `grid` | 81 | Core |
| `rounded-lg` | 79 | Core |
| `justify-center` | 69 | Core |
| `text-destructive` | 68 | Core |
| `font-bold` | 64 | Core |
| `h-4` | 57 | defined |
| `w-4` | 54 | defined |
| `inline-flex` | 54 | Core |
| `px-4` | 51 | defined |
| `p-4` | 50 | Core |
| `rounded` | 45 | Core |
| `px-3` | 45 | defined |
| `transition-colors` | 45 | Core |
| `bg-background` | 45 | Core |
| `font-semibold` | 43 | Core |
| `bg-destructive/10` | 43 | Core |
| `text-center` | 42 | Core |

## Unknown Classes (referenced but not defined/allowlisted)

These are valid-looking CSS classes not in the allowlist.

| Class | References |
|-------|------------|
| `--workspace-grid-gap:` | 1 |
| `-clip` | 1 |
| `-mb-px` | 3 |
| `-mt-px` | 1 |
| `-rotate-90` | 1 |
| `-title` | 1 |
| `-top-4` | 1 |
| `abc` | 2 |
| `abc123` | 1 |
| `abcd` | 1 |
| `able` | 1 |
| `aborted` | 8 |
| `access` | 3 |
| `account` | 3 |
| `action` | 7 |
| `actions` | 2 |
| `activity` | 1 |
| `adapter` | 10 |
| `adapters` | 21 |
| `address` | 1 |
| `admin` | 4 |
| `administration` | 1 |
| `after` | 1 |
| `ago` | 2 |
| `alert` | 2 |
| `alertdialog` | 1 |
| `alpha` | 4 |
| `any` | 1 |
| `aos-glass-theme` | 1 |
| `api` | 2 |
| `app` | 1 |
| `appear` | 1 |
| `appearance` | 1 |
| `append` | 1 |
| `application` | 1 |
| `appropriate` | 1 |
| `approved` | 1 |
| `assertive` | 1 |
| `assistant` | 9 |
| `associated` | 2 |
| `attention` | 1 |
| `audit` | 4 |
| `auditor` | 1 |
| `authentication` | 1 |
| `auto` | 1 |
| `automatically` | 2 |
| `available` | 9 |
| `backend` | 3 |
| `background-color` | 1 |
| `base` | 1 |

_...and 716 more_

## Reject Utilities Status

**Total Reject:** 0
**Referenced:** 0
**Unreferenced (safe to remove):** 0


## Unused Allowlisted Classes by Status

### Core (0)

### Transitional (0)

