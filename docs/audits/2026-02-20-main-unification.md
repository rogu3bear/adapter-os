# Main Branch Unification Audit Anchor (2026-02-20)

## Scope
- Mode: include WIP (`Include-WIP`) unification onto `main`.
- Goal: deterministic consolidation of committed partial features plus staged working-tree deltas.
- Non-goal: broad workspace/full E2E test sweep.

## Deterministic Anchors
- Remote base: `origin/main` at `20bb35fc0835912185e42eafaf130823510ff11f`.
- Pre-unification local head: `7b836d9c1ee051f0d6d7809d7ac989c1d3620c6b`.
- Safety rollback tag: `integration-anchor-20260220-115134`.

## Commit Citation Set (Pre-existing Partial Features)
- `9e4237e15253f067f6e63b6e50e93ab440f6d67f` docs: restore rectification and adapter-flow prompts
- `556b98c717398e1495d3cc8d7e9295c01e354966` ui: consolidate startup status polling and remove wasm warning noise
- `a534a33c7854535b5237a518714b4cbd8a51cbf8` ui: stabilize form control ids and expose input attributes
- `6fd263709d7f8acbbf95f7e7a17db50d1481f8e2` ui: harden chat and adapter surfaces for wrap and contrast
- `20d20435efe4faaf1a6a92498565245a6f5412e6` build: drop preload integrity attrs during UI digest refresh
- `fcfdd10ddef821bc1c6195efeb6a4bee8effd753` ui: normalize button primitives across panels and chat
- `8dd7e159fa2f513e1ca370d2212b7151fe049f1a` ui: extract inline layout styles into semantic core classes
- `d1e7447edea82a07d74c0963680ae8a89e819d1e` ui: unify adapter CTA path and status surface guidance
- `7eb0aaf44d9da5ba631edb560e8a8fd0c968aaaa` ui: stabilize adapter creation flow and responsive readability
- `f0bb667a5abdb4a2d86825c9b804e67515a67f85` chore(repo): add editor, attributes, and code ownership defaults
- `b4c162ca3b0c98c8e14a7e7349b9df1fbd9d0d9f` docs: add MVP launch playbooks and trust-native reference prompts
- `73381db86b7d389f6a53eb431ed7409547911c59` feat(server): enforce canonical error codes and add drift guard
- `eff9773c80033d3eca97029a0814161d03a2fc55` feat(dev): harden local bootstrap and dataset/training workflows
- `f6c4a3c2775a79d20f710a9470cb3ac1dd6b18d7` feat(api-types): add trust-native contracts and knowledge scope migration
- `382fba2cca7bb96d6ed7fb4759240fa409cebb48` feat(ui): add HUD shell dispatch and trust-native chat surfaces
- `3db05e4b7548ebd15346c3e50b48778629275871` feat(ui): add HUD desktop stylesheet and bundle import
- `632cf047791a78327ae11d266c3e0289d2cd8f01` test(playwright): add trust-native journeys, demo flow, and visual baselines
- `f76e148f8ee5cceca4e33a9dd7b5ddae9894b101` chore(docs): refresh generated error inventory artifacts
- `7254b0a0723f12e7ae5321ef968ece95155de7fc` feat(db): enforce DAT-008 schema contract and runtime table parity
- `b42628d4983a12c054222b58ccc080ab8150153e` feat(api): rectify db contracts and publish schema v2.0.0
- `7b836d9c1ee051f0d6d7809d7ac989c1d3620c6b` feat(ui): add hud keyboard shortcuts and progress rail

## Conflict Resolution Ledger (Explicit)

| ID | Conflict | Resolution | Evidence paths |
|---|---|---|---|
| UNI-001 | Legacy UI surfaces vs HUD shell/routing profile | Keep HUD-oriented shell and remove superseded legacy surfaces; keep quarantine track for deprecated UI route specs | `crates/adapteros-ui/src/components/layout/hud_shell.rs`, `crates/adapteros-ui/src/pages/mod.rs`, `tests/playwright/quarantine-ui/README.md` |
| UNI-002 | Core route smoke coverage vs new SSR/no-JS expectations | Keep core smoke and add no-JS SSR route smoke in same matrix | `tests/playwright/ui/routes.core.smoke.spec.ts`, `tests/playwright/ui/routes.core.nojs.ssr.spec.ts`, `crates/adapteros-server/src/ssr.rs` |
| UNI-003 | API/UI surface contract drift risk during broad deletes/adds | Keep generated matrix + contract check script to anchor expected surface | `docs/api-surface-matrix.md`, `docs/generated/api-surface-matrix.json`, `scripts/contracts/check_api_surface.py` |
| UNI-004 | Scratch artifacts mixed with source edits | Exclude scratch captures from repository state; moved to external temp backup | `/tmp/adapteros-integration-scratch-20260220-115147` |

## Verification (Targeted)
- `cargo test -p adapteros-api-types ui::tests::parse_falls_back_to_primary_for_unknown_values --lib` -> pass
- `cargo test -p adapteros-server-api handlers::ui_config::tests --lib` -> pass
- `cargo test -p adapteros-ui --lib` -> pass (`199 passed`)
- `cd tests/playwright && npx playwright test -c playwright.ui.config.ts ui/routes.core.smoke.spec.ts ui/routes.core.nojs.ssr.spec.ts --project=chromium` -> pass (`21 passed`)

## Excluded Artifacts
- Scratch PNG captures and temp analysis JSON were intentionally excluded from tracked state:
  - `audit-*.png`
  - `hud-*.png`
  - `tmp_columns.json`
