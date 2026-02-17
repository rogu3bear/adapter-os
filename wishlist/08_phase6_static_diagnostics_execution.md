# Phase 6 Execution: Static Diagnostics Harmonization

## Objective
- Harmonize boot diagnostics expectations between full static UI and static-minimal diagnostic surfaces.

## Deliverable A: Mismatch Checklist

| Surface | Current behavior | Mismatch |
|---|---|---|
| `crates/adapteros-server/static/index.html` | Rich boot-progress stages, panic overlay, WASM fetch tracking, backend `/healthz` post-mount probe | Full diagnostic stack exists only here. |
| `crates/adapteros-server/static-minimal/index-minimal.html` | Minimal HTML bootstrap with module script only | No boot/panic diagnostics parity with main static UI. |
| `crates/adapteros-server/static-minimal/api-test.html` | API test harness with endpoint buttons and inline logging | Health endpoint now canonicalized to `/healthz`; residual mismatch is styling/diagnostic semantics vs main boot UI. |

## Deliverable B: Shared Snippet Strategy (No duplicate JS logic)
1. Extract shared diagnostics primitives (`safe error banner`, `status badge`, `boot stage formatter`) into a single static script under `crates/adapteros-server/static-minimal/` and reuse in both minimal pages.
2. Keep `static/index.html` as authoritative advanced boot runtime (WASM intercept + panic overlay) and expose only a thin compatibility subset to minimal pages.
3. Normalize endpoint constants in minimal harness to canonical health/readiness routes (`/healthz`, `/readyz`) to match AGENTS contract.
4. Add a lightweight parity checklist in docs for any future edits touching these three files.

## Deliverable C: Migration Sequence
1. Canonicalize endpoint literals in `api-test.html` (`/health` -> `/healthz`).
2. Introduce shared diagnostics helper include for `index-minimal.html` + `api-test.html`.
3. Keep advanced boot diagnostics isolated in `static/index.html` but document required baseline fields for minimal surfaces.

## Verification Run
- Ran static/minimal diagnostics anchor scan:
`rg -n "boot|panic|__TRUNK_HASH__|health|readyz|api|fetch|status" crates/adapteros-server/static/index.html crates/adapteros-server/static-minimal/index-minimal.html crates/adapteros-server/static-minimal/api-test.html`
- Result: confirms diagnostics asymmetry remains; endpoint drift resolved (`api-test.html` now uses `/healthz`).

- Ran direct content inspections:
`nl -ba crates/adapteros-server/static/index.html`
`nl -ba crates/adapteros-server/static-minimal/index-minimal.html`
`nl -ba crates/adapteros-server/static-minimal/api-test.html`
- Result: checklist evidence captured.

## Phase 6 Completion
- [x] Mismatch checklist delivered.
- [x] Shared snippet strategy delivered.
- [x] Migration order delivered.
- [x] Verification evidence captured.
