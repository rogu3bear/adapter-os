---
phase: "22"
name: "QA Visual Working System Activation (macOS Dual-Browser Blocking)"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 22: QA Visual Working System Activation (macOS Dual-Browser Blocking) — Context

## Decisions

- Milestone strategy is locked: open QA milestone `v1.1.6` now; treat `GOV-16` as accepted external debt until capability changes.
- CI gate level is locked: dual-browser blocking.
- Phase focus is locked: stabilize and align existing gates first; no new product surface.
- Baseline policy is locked: macOS snapshots are canonical.
- Enforcement mode is locked: run blocking visual gate on macOS runners.
- Blocking suite type is locked: bundled lane (`visual + console + route audit + critical detail flows`).
- CI contract for Phase 22 must use explicit suite commands and must not rely on grep-based style-audit selectors.
- Playwright contract for Phase 22 must enforce snapshot references/baselines deterministically and fail on missing or orphaned snapshot files.

## Discretion Areas

- Exact sequencing of verification commands as long as dual-browser contract checks are explicitly evidenced.
- README wording/placement for gate policy documentation as long as canonical commands and baseline policy are unambiguous.
- Evidence collection strategy (`--list` selector truth checks vs full suite runs) as long as at least one concrete run-scoped validation path is recorded.

## Deferred Ideas

- Any Playwright harness rewrite or framework migration.
- Chat visual expansion (`ENABLE_CHAT_VISUALS` remains unchanged and deferred).
- Governance capable-proof debt retirement (`GOV-16`) in this milestone.
