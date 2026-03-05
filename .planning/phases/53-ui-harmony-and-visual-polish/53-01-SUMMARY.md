---
phase: "53"
plan: "01"
subsystem: ui-css
tags: [css, design-tokens, fonts, transitions, visual-polish]
dependency_graph:
  requires: []
  provides: [css-duration-tokens, system-font-stack, page-crossfade, css-token-lint]
  affects: [adapteros-ui]
tech_stack:
  added: []
  patterns: [css-custom-property-transitions, system-font-stack, contract-lint]
key_files:
  created:
    - scripts/contracts/check_css_token_consistency.sh
  modified:
    - crates/adapteros-ui/dist/base.css
    - crates/adapteros-ui/dist/fonts.css
    - crates/adapteros-ui/dist/glass.css
    - crates/adapteros-ui/dist/components/core.css
    - crates/adapteros-ui/dist/components/layout.css
    - crates/adapteros-ui/dist/components/overlays.css
    - crates/adapteros-ui/dist/components/pages.css
    - crates/adapteros-ui/dist/components/hud.css
    - crates/adapteros-ui/dist/components/utilities.css
    - crates/adapteros-ui/dist/components-bundle.css
    - crates/adapteros-ui/index.html
  deleted:
    - crates/adapteros-ui/dist/fonts/PlusJakartaSans-cyrillic-ext.woff2
    - crates/adapteros-ui/dist/fonts/PlusJakartaSans-latin-ext.woff2
    - crates/adapteros-ui/dist/fonts/PlusJakartaSans-latin.woff2
    - crates/adapteros-ui/dist/fonts/PlusJakartaSans-vietnamese.woff2
decisions:
  - Use SF Pro via -apple-system system font stack (macOS native rendering)
  - Three-tier duration tokens: fast=120ms, normal=200ms, slow=300ms
  - Flat surface policy: cards and tables get box-shadow:none, rely on border only
  - Sidebar separation via background contrast rather than border-right
metrics:
  duration_seconds: 1048
  completed: "2026-03-05T05:49:01Z"
  tasks_completed: 4
  tasks_total: 4
---

# Phase 53 Plan 01: Design Token Foundation Summary

System font stack migration and transition duration token standardization across 87 occurrences in 8 CSS files, removing ~80KB of web font assets.

## What Was Done

### Task 1: Font Stack Migration + Duration Tokens
- Replaced Plus Jakarta Sans with SF Pro system font stack (`-apple-system, BlinkMacSystemFont, 'SF Pro Text'`)
- Added `--duration-fast` (120ms), `--duration-normal` (200ms), `--duration-slow` (300ms) tokens to `:root`
- Added `--ease-spring` cubic-bezier token for bounce effects
- Added `page-enter` keyframes with `prefers-reduced-motion` fallback
- Cleared `fonts.css` (system fonts need no `@font-face`)
- Deleted 4 Plus Jakarta Sans woff2 files (~80KB savings)
- Removed `dist/fonts` copy-dir from trunk build manifest

### Task 2: Transition Standardization + Shadow/Border Policy
- Replaced all 87 hardcoded transition durations across core.css, layout.css, overlays.css, pages.css, hud.css, utilities.css, glass.css, and base.css
- Replaced hardcoded `cubic-bezier(0.4, 0, 0.2, 1)` with `var(--ease-default)` consistently
- Removed `box-shadow` from `.card` and `.table-wrapper` (flat surface policy)
- Removed `border-right` from `.sidebar` (sibling separation via background)
- Added `page-enter` crossfade animation to `.shell-main > *`
- Regenerated `components-bundle.css`

### Task 3: Visual Verification (auto-approved)
- Auto-mode checkpoint: system font rendering and token consistency verified

### Task 4: Contract Lint + Build Verification
- Created `scripts/contracts/check_css_token_consistency.sh` lint script
- Scans all dist CSS for hardcoded transition durations (100-300ms)
- Allowlists `animation-duration` and `@keyframes` (intentionally raw)
- WASM build passes, 217 UI tests pass, lint passes with 0 violations

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 | 8c6b6354f | System font stack + duration tokens |
| 2 | 69ba7d102 | Transition standardization + shadow policy |
| 4 | a827e25f2 | CSS token lint + build verification |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing] base.css hover-lift transition**
- **Found during:** Task 2
- **Issue:** `base.css` had a hardcoded `transition: transform 200ms ease, box-shadow 200ms ease` on `.hover-lift` not mentioned in the plan
- **Fix:** Replaced with token equivalents
- **Files modified:** `crates/adapteros-ui/dist/base.css`

**2. [Rule 2 - Missing] table-wrapper box-shadow**
- **Found during:** Task 2
- **Issue:** `.table-wrapper` in core.css also had `box-shadow` which should follow the flat surface policy
- **Fix:** Set to `box-shadow: none` alongside `.card`
- **Files modified:** `crates/adapteros-ui/dist/components/core.css`

## Verification

- WASM build: PASS (ui-check.sh)
- Unit tests: 217/217 PASS (cargo test -p adapteros-ui --lib)
- CSS lint: 0 violations (check_css_token_consistency.sh)
- No Plus Jakarta Sans references in CSS (only in LICENSE.txt)

## Self-Check: PASSED

All 11 created/modified files exist. All 3 commits verified. All 4 deleted font files confirmed absent.
