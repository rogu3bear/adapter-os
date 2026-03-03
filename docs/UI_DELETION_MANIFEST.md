# UI Deletion Manifest

This manifest tracks the currently active UI route surface after reduction and where to update references when deleting, restoring, or renaming routes.

## UI/API Decoupling Policy

- UI route removal does **not** imply API removal.
- API retention is governed by `docs/api-surface-matrix.md` and enforced by `scripts/contracts/check_api_surface.py`.
- Non-UI API domains may remain active when marked `kept_no_ui` for runtime or platform reasons.

## Process

1. Route source of truth: `crates/adapteros-ui/src/lib.rs`
2. Navigation registry: `crates/adapteros-ui/src/components/layout/nav_registry.rs`
3. Shell title mapping: `crates/adapteros-ui/src/components/layout/shell.rs`
4. Contextual actions: `crates/adapteros-ui/src/search/contextual.rs`
5. Active Playwright suite: `tests/playwright/ui/`

Quick grep:

```bash
rg -n 'path!\("/' crates/adapteros-ui/src/lib.rs
rg -n "'/" tests/playwright/ui
```

## Active Public Routes

- `/login`
- `/safe`

## Active Protected Routes

- `/`, `/dashboard`
- `/adapters`, `/adapters/:id`
- `/update-center`
- `/chat`, `/chat/:session_id`
- `/system`
- `/settings`, `/user` (compatibility alias to settings)
- `/models`, `/models/:id`
- `/policies`
- `/training`, `/training/:id`
- `/documents`, `/documents/:id`
- `/admin`
- `/audit`
- `/runs`, `/runs/:id`
- `/workers`, `/workers/:id`
- `/welcome`

## Compatibility Redirects (Keep)

- `/flight-recorder` -> `/runs`
- `/flight-recorder/:id` -> `/runs/:id`
- `/user` remains as compatibility path to settings surface

## Active Test Anchors

Core stabilization gates:

- `tests/playwright/ui/routes.core.smoke.spec.ts`
- `tests/playwright/ui/routes.core.nojs.ssr.spec.ts`

Additional active-route coverage:

- `tests/playwright/ui/routes.data.smoke.spec.ts`
- `tests/playwright/ui/routes.ops.smoke.spec.ts`
- `tests/playwright/ui/routes.best_practices.audit.spec.ts`
- `tests/playwright/ui/public.spec.ts`
- `tests/playwright/ui/visual.spec.ts`

## Quarantined Frontend Surfaces

Removed frontend route specs are retained for future reactivation under:

- `tests/playwright/quarantine-ui/`
- `tests/playwright/quarantine-ui/README.md`

Quarantined specs:

- `collections.spec.ts`
- `datasets.spec.ts`
- `repositories.spec.ts`
- `routing.spec.ts`
- `stacks.spec.ts`

Note: backend/API capabilities for these domains remain intact by design; only frontend route entry points are reduced.
