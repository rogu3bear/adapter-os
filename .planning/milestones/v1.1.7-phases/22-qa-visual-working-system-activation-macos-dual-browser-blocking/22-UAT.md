---
phase: "22"
name: "QA Visual Working System Activation (macOS Dual-Browser Blocking)"
created: 2026-02-26
updated: "2026-02-26T21:10:00Z"
status: pass_with_observation
---

# Phase 22: QA Visual Working System Activation (macOS Dual-Browser Blocking) — User Acceptance Testing

## Test Results

| # | Test | Status | Notes |
|---|------|--------|-------|
| 1 | Tier-2 CI quality gate uses dual-browser blocking commands on macOS | passed | `.github/workflows/ci.yml` contains explicit Chromium/WebKit `macos-14` blocking jobs invoking `test:gate:quality`. |
| 2 | Bundled suite selector returns non-zero tests for Chromium | passed | `PW_RUN_ID=phase22-gate-list-chromium` listed 39 tests from bundled lane and produced `var/playwright/runs/phase22-gate-list-chromium/report/index.html`. |
| 3 | Bundled suite selector returns non-zero tests for WebKit | passed | `PW_RUN_ID=phase22-gate-list-webkit` listed 39 tests from bundled lane and produced `var/playwright/runs/phase22-gate-list-webkit/report/index.html`. |
| 4 | Snapshot contract check passes (zero missing active + zero orphan baselines) | passed | `node scripts/check-visual-snapshot-contract.mjs` returned OK with darwin canonical baseline counts. |
| 5 | Documentation and planning artifacts reference canonical commands and evidence paths | passed | Root `README.md`, `tests/playwright/README.md`, and Phase 22 planning files now align to the same gate/baseline contract. |

## Operator Checklist

1. Confirm branch protection in GitHub requires both `playwright-ui-quality-gate-chromium` and `playwright-ui-quality-gate-webkit` checks.
2. Run full lane commands when touching bundled gate specs:
   - `cd tests/playwright && npm run test:gate:quality -- --project=chromium`
   - `cd tests/playwright && npm run test:gate:quality -- --project=webkit`
3. Keep snapshot updates macOS-only and commit only referenced `*-darwin.png` files.

## Exit Criteria

- **Pass:** Dual-browser selector truth is validated, snapshot contract passes, and documentation + evidence links are complete.
- **Observation:** Full assertion execution not run in this pass; execute full gate lane in CI or explicitly local when risk posture requires.

## Summary

Phase 22 UAT passes for contract anchoring and selector truth with one observation: full assertion execution remains a deliberate follow-up validation step.
