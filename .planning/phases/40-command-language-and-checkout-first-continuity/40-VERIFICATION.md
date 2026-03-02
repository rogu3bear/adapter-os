---
phase: 40-command-language-and-checkout-first-continuity
status: completed
verification: passed
verified_at: 2026-02-28
---

# Phase 40 Verification

## Commands

```bash
cargo check -p adapteros-ui

runtime=/Users/star/.codex/skills/gsd-codex-runtime/runtime/get-shit-done/bin/gsd-tools.cjs
node "$runtime" verify artifacts .planning/phases/40-command-language-and-checkout-first-continuity/40-01-PLAN.md --cwd /Users/star/Dev/adapter-os --raw
node "$runtime" verify key-links .planning/phases/40-command-language-and-checkout-first-continuity/40-01-PLAN.md --cwd /Users/star/Dev/adapter-os --raw

rg -n "run checkout or promote|Default path: resolve a version, run checkout or promote|Recommended default" \
  crates/adapteros-ui/src/components/adapter_detail_panel.rs \
  crates/adapteros-ui/src/pages/dashboard.rs \
  crates/adapteros-ui/src/pages/update_center.rs
```

## Results

- `cargo check -p adapteros-ui`: passed.
- `verify artifacts` + `verify key-links` for `40-01-PLAN.md`: valid.
- Targeted search confirms checkout-first default-path wording is aligned across planned surfaces.

## Codebase Citations

- `crates/adapteros-ui/src/pages/dashboard.rs` (guided flow checkout/promote-before-feed default path)
- `crates/adapteros-ui/src/pages/update_center.rs` (update-center command framing and checkout-first recommended text)
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs` (recommended-next-action command-map wording parity)
- `crates/adapteros-ui/src/components/layout/nav_registry.rs` (command/discoverability keyword continuity)

## Best-Practice Citations

- NARA Plain Language Principles: https://www.archives.gov/open/plain-writing/10-principles.html
- Digital.gov Plain Language Guide: https://digital.gov/guides/plain-language/writing
