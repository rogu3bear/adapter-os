# AdapterOS UI Route Map

**Canonical source:** `crates/adapteros-ui/src/lib.rs` — route definitions and shell boundaries.

This inventory maps every live route declared in `lib.rs` to the canonical IA taxonomy:

`Infer`, `Data`, `Train`, `Deploy`, `Route`, `Observe`, `Govern`, `Org`.

Route class canon (exactly one per route): `Primary`, `Tools`, `Hidden`, `Experimental`.

Naming canon:
- User-facing label is `Runs` (not `Flight Recorder`).
- Legacy aliases remain documented as redirects for backward compatibility.

## Canonical Module Ownership

- **Infer**: inference interaction surfaces (`/chat` and session deep links).
- **Data**: datasets, documents, collections, repositories.
- **Train**: training flows and deep-link entry points.
- **Deploy**: models, adapters, stacks.
- **Route**: routing policy/decision inspection.
- **Observe**: dashboard, runs, workers, monitoring, errors, diff tooling.
- **Govern**: policies, audit, human review, safety fallback.
- **Org**: admin, settings/account, system/workspace utilities.

## Live Route Inventory

| Route | Module | Class | Maturity | Behavior / Notes |
|---|---|---|---|---|
| `/login` | Org | Hidden | Stable | Public auth entry. |
| `/safe` | Govern | Hidden | Stable | Public safe-mode fallback (no auth/API calls). |
| `/style-audit` | Org | Tools | Stable | Style-system audit page (dev utility). Public route; live API-backed sections require authentication. |
| `/dashboard` | Observe | Hidden | Stable | Redirect alias to `/`. |
| `/flight-recorder` | Observe | Hidden | Stable | Legacy redirect alias to `/runs`. |
| `/flight-recorder/:id` | Observe | Hidden | Stable | Legacy redirect alias to `/runs/:id` (preserves query string). |
| `/` | Observe | Primary | Stable | Dashboard landing page. |
| `/adapters` | Deploy | Primary | Stable | Adapter inventory. |
| `/adapters/:id` | Deploy | Primary | Stable | Adapter detail. |
| `/update-center` | Deploy | Primary | Stable | Update center for adapters and system components. |
| `/chat` | Infer | Primary | Stable | Inference chat entry point. |
| `/chat/:session_id` | Infer | Primary | Stable | Chat session deep link. |
| `/system` | Org | Primary | Stable | System status/topology surface. |
| `/settings` | Org | Primary | Stable | User preferences and system info. |
| `/user` | Org | Hidden | Stable | Backward-compat redirect alias to `/settings`. |
| `/models` | Deploy | Primary | Stable | Model registry/list. |
| `/models/:id` | Deploy | Primary | Stable | Model detail. |
| `/policies` | Govern | Primary | Stable | Policy management. |
| `/training` | Train | Primary | Stable | Training jobs and orchestration. |
| `/training/:id` | Train | Hidden | Stable | Redirect alias to `/training?job_id=:id`. |
| `/stacks` | Deploy | Primary | Stable | Runtime stack list. |
| `/stacks/:id` | Deploy | Primary | Stable | Runtime stack detail. |
| `/collections` | Data | Primary | Stable | Collection list. |
| `/collections/:id` | Data | Primary | Stable | Collection detail. |
| `/documents` | Data | Primary | Stable | Document list/ingestion surface. |
| `/documents/:id` | Data | Primary | Stable | Document detail. |
| `/datasets` | Data | Primary | Stable | Dataset list. |
| `/datasets/:id` | Data | Primary | Stable | Dataset detail. |
| `/admin` | Org | Primary | Stable | Tenant/org administration. |
| `/audit` | Govern | Primary | Stable | Audit and compliance trail. |
| `/runs` | Observe | Primary | Stable | Canonical runs history list. |
| `/runs/:id` | Observe | Primary | Stable | Canonical run detail hub. |
| `/diff` | Observe | Tools | Stable | Run-diff launcher page; if run IDs are present in query params, redirects to `/runs/:id?tab=diff...`. |
| `/workers` | Observe | Primary | Stable | Worker list and lifecycle controls. |
| `/workers/:id` | Observe | Primary | Stable | Worker detail. |
| `/monitoring` | Observe | Primary | Stable | Monitoring and alerts. |
| `/errors` | Observe | Primary | Stable | Error/incidents surface. |
| `/routing` | Route | Primary | Stable | Routing rules and decision inspection. |
| `/repositories` | Data | Primary | Stable | Repository list/sync controls. |
| `/repositories/:id` | Data | Primary | Stable | Repository detail. |
| `/reviews/:pause_id` | Govern | Primary | Stable | Review detail/deep link for paused items. |
| `/reviews` | Govern | Primary | Stable | Review queue. |
| `/welcome` | Org | Hidden | Stable | First-run onboarding/checklist surface. |
| `/agents` | Org | Experimental | Experimental + Incomplete | Agent orchestration UI (`Agents (Beta)` in nav); session creation action is intentionally disabled in UI. |
| `/files` | Org | Primary | Stable | Filesystem browser. |

## Redirect And Legacy Alias Notes

- `/dashboard` -> `/`
- `/flight-recorder` -> `/runs`
- `/flight-recorder/:id` -> `/runs/:id` (query params preserved)
- `/training/:id` -> `/training?job_id=:id`
- `/user` -> `/settings`
- `/diff` conditionally redirects only when query includes run IDs; otherwise it remains a standalone diff page.
