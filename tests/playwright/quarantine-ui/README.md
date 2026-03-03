# Quarantined UI Specs

These specs target frontend routes that were intentionally removed from the active UI surface during the reduction pass.

## Why This Folder Exists

- Keep historical test logic for future feature return.
- Exclude removed-route specs from default UI runs (`testDir: ui`).
- Avoid noisy failures in current stabilization gates.

## Quarantined Specs

- `collections.spec.ts`
- `datasets.spec.ts`
- `repositories.spec.ts`
- `routing.spec.ts`
- `stacks.spec.ts`

## Reactivation Rules

1. Restore the matching route(s) in `crates/adapteros-ui/src/lib.rs`.
2. Restore corresponding page module exports in `crates/adapteros-ui/src/pages/mod.rs`.
3. Restore nav/shell/contextual route references if applicable.
4. Move the spec back into `tests/playwright/ui/`.
5. Run targeted smoke coverage and update `docs/UI_DELETION_MANIFEST.md`.

## Scope Boundary

Backend/API endpoints remain intentionally intact for future return; this quarantine only applies to the frontend route surface.
