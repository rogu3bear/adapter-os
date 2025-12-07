# IA current vs spec

## Build (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /management | — | — | Build | Build | legacy, hidden |
| /workflow | Onboarding | 0 | Build | Build | Build landing |
| /personas | — | — | Build | Build | legacy, hidden |
| /flow/lora | — | — | Build | Build | legacy, hidden |
| /trainer | — | — | Build | Build | legacy, hidden |
| /create-adapter | — | — | Build | Build | action-only |
| /training | Training | 2 | Build | Build |  |
| /training/jobs | — | — | — | Build | no navTitle |
| /training/jobs/:jobId | — | — | — | Build | no navTitle |
| /training/datasets | — | — | — | Build | no navTitle |
| /training/datasets/:datasetId | — | — | — | Build | no navTitle |
| /training/templates | — | — | — | Build | no navTitle |
| /promotion | — | — | Build | Build | legacy, hidden |
| /adapters | Adapters | 1 | Build | Build |  |
| /adapters/new | — | — | — | Build | no navTitle |
| /adapters/:adapterId | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/activations | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/usage | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/lineage | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/manifest | — | — | — | Build | shell route, no navTitle |
| /admin | Admin | 5 | Build | Build |  |
| /admin/tenants | Organizations | 2 | Build | Build |  |
| /admin/tenants/:tenantId | — | — | — | Build | no navTitle |
| /admin/stacks | Adapter Stacks | 3 | Build | Build |  |
| /admin/plugins | Plugins | 4 | Build | Build |  |
| /admin/settings | Settings | 5 | Build | Build |  |
| /base-models | Base Models | 4 | Build | Build |  |
| /router-config | Router Config | 3 | Build | Build |  |
| /federation | — | — | Build | Build | IA-EXTRA, hidden |

## Build (expected)
- /workflow, /flow/lora, /personas, /management
- /create-adapter, /adapters (+ detail routes)
- /training (+ jobs, datasets, templates), /trainer
- /promotion, /base-models, /router-config
- /admin (+ tenants, stacks, plugins, settings)

## Build lifecycle (overview)
- Register adapter: `/adapters/new` (lands on adapter overview)
- Train adapter: `/training/jobs` (job creation, uses defaults from Settings/Templates)
- Configure routing: `/router-config` (verify effective set and policies)
- Preselection params (optional): `?adapterId=<id>` for Training/Router Config, `?datasetId=<id>` for Training job creation

## Run (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /dashboard | Dashboard | 0 | Run | Run | Run landing |
| /inference | Inference | 1 | Run | Run |  |
| /chat | Chat | 2 | Run | Run |  |
| /documents | Documents | 3 | Run | Run |  |
| /documents/:documentId/chat | — | — | — | Run | no navTitle |
| /code-intelligence | Code Intelligence | 4 | Run | Run |  |

## Run (expected)
- /dashboard, /inference, /chat
- /documents (+ chat)
- /code-intelligence

## Observe (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /monitoring | Monitoring | 0 | Observe | Observe | Observe landing |
| /metrics | Metrics | 1 | Observe | Observe |  |
| /routing | Routing History | 2 | Observe | Observe |  |
| /system | System Overview | 3 | Observe | Observe |  |
| /system/nodes | Nodes | 2 | Observe | Observe |  |
| /system/workers | Workers | 3 | Observe | Observe |  |
| /system/memory | Memory | 4 | Observe | Observe |  |
| /system/metrics | System Metrics | 5 | Observe | Observe |  |
| /telemetry | Event History | 4 | Observe | Observe |  |
| /telemetry/viewer | — | — | — | Observe | no navTitle |
| /reports | Reports | 5 | Observe | Observe |  |
| /help | Help Center | 6 | Observe | Observe |  |

## Observe (expected)
- /monitoring, /metrics, /metrics/advanced, /routing
- /system (+ nodes, workers, memory, metrics)
- /telemetry (+ viewer), /reports, /help

## Verify (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /owner | Owner Home (Legacy) | 99 | Verify | Verify | legacy |
| /testing | Testing | 3 | Verify | Verify | Verify landing |
| /golden | Verified Runs | 4 | Verify | Verify |  |
| /replay | Run History | 5 | Verify | Verify |  |
| /security/policies | Guardrails | 0 | Verify | Verify |  |
| /security/audit | Audit Logs | 1 | Verify | Verify |  |
| /security/compliance | Compliance | 2 | Verify | Verify |  |
| /dev/errors (dev) | Error Inspector | 1 | Verify | Verify | IA-EXTRA dev |
| /_dev/routes (dev) | Routes Manifest | 2 | Verify | Verify | IA-EXTRA dev |

## Verify (expected)
- /testing, /golden, /replay
- /security/policies, /security/audit, /security/compliance
- /owner, /_dev/routes, /dev/errors

## Mismatches
- Missing from UI but in IA: none.
- Present in UI but not in IA (IA-EXTRA): /federation; dev-only /dev/errors, /_dev/routes.
- Cluster/navGroup inconsistencies: none (aligned to clusters).
- Routes with cluster but no navTitle: training detail routes, adapter detail routes (and subpaths), documents/:id/chat, telemetry/viewer, admin tenant detail; shell/detail pages only.
- Tabbed-shell consolidation status: Adapters, Training, Telemetry, and Replay routes now land in tab shells; register/policies, artifacts/settings, alerts/exports/filters, and replay sub-tabs still rely on hash-based placeholder content that needs wiring to real data.

## Implementation status
- Cluster labels and tabbed shells are wired as per IA.
- IA-EXTRA routes are retained intentionally for advanced or tooling flows.

## Slim UX (nav-hidden but supported)
- /management — legacy setup console (hidden from nav)
- /personas — product tour (legacy)
- /flow/lora — guided setup (legacy)
- /trainer — quick training (legacy)
- /promotion — legacy promotion flow
- /create-adapter — action-only entry, linked from adapters
- /metrics/advanced — detail-only
- /federation — IA-EXTRA tooling; direct URL only
- Label updates: `/workflow` → Onboarding, `/routing` → Routing History, `/owner` → Owner Home (Legacy)
- Dashboard metrics tiles handle loading/empty/error states gracefully and are test-backed to prevent regressions

## IA-EXTRA handling
- /federation retained and labeled `IA-EXTRA` in route config.
- Dev-only routes (`/dev/errors`, `/_dev/routes`) kept for debugging; nav entries gated by DEV flag. `/_dev/routes` should mirror IA and is the canonical debug view for route parity.

## Operator flows
- Incident ➜ Observe: Operators start on `/dashboard` or `/monitoring` to spot health or traffic anomalies, then pivot to `/metrics` for charts or `/routing` for adapter-level signals.
- Observe ➜ Telemetry: From spikes or anomalies, they open `/telemetry#alerts` or `/telemetry#filters` to scope events by tenant/session and confirm routing behavior.
- Telemetry ➜ Replay: With a request or session ID, they jump to `/replay#runs` (preselected via query where available) to view decision trace, evidence, and exports.
- Verify ➜ Coverage: `/testing` and `/golden` link into `/replay#compare` to compare baselines against live runs; security pages (`/security/policies`, `/security/audit`, `/security/compliance`) surface replay links when entries carry session context.

## Route-domain notes (2025-12-07)
- Chat: `/chat` and `/documents/:documentId/chat` (forces `source_type=document`); CLI chat uses `source_type=cli|owner_system|cli_prompt` and lands in the same sessions table.
- Auth: handled via login/logout flows (no dedicated nav); `admin_tenants` echoes from claims and the `"*"` wildcard is dev-only via `AOS_DEV_NO_AUTH` (debug).
- Router: `/router-config` (Build cluster) for effective routing state; `/routing` (Observe) shows decision history; telemetry viewer forwards `source_type` into routing-decision filters.
- Telemetry: `/telemetry` + `/telemetry/viewer` (and `/code-intelligence` redirect) propagate `source_type` query parameters for router/telemetry alignment.
- Reports: `/reports` (Observe) is tenant-scoped and backed by the same monitoring payload as `/monitoring`.

MLNavigator Inc 2025-12-07.

