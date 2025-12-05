# IA current vs spec

## Build (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /management | Management Panel | 2 | Build | Build |  |
| /workflow | Getting Started | 3 | Build | Build |  |
| /personas | Product Tour | 4 | Build | Build |  |
| /flow/lora | Guided Setup | 5 | Build | Build |  |
| /trainer | Quick Training | 1 | Build | Build |  |
| /create-adapter | Create Adapter | 0 | Build | Build |  |
| /training | Training | 2 | Build | Build |  |
| /training/jobs | — | — | — | Build | no navTitle |
| /training/jobs/:jobId | — | — | — | Build | no navTitle |
| /training/datasets | — | — | — | Build | no navTitle |
| /training/datasets/:datasetId | — | — | — | Build | no navTitle |
| /training/templates | — | — | — | Build | no navTitle |
| /promotion | Promotion | 5 | Build | Build |  |
| /adapters | Adapters | 6 | Build | Build |  |
| /adapters/new | — | — | — | Build | no navTitle |
| /adapters/:adapterId | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/activations | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/usage | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/lineage | — | — | — | Build | shell route, no navTitle |
| /adapters/:adapterId/manifest | — | — | — | Build | shell route, no navTitle |
| /admin | Admin | 1 | Build | Build |  |
| /admin/tenants | Organizations | 2 | Build | Build |  |
| /admin/tenants/:tenantId | — | — | — | Build | no navTitle |
| /admin/stacks | Adapter Stacks | 3 | Build | Build |  |
| /admin/plugins | Plugins | 4 | Build | Build |  |
| /admin/settings | Settings | 5 | Build | Build |  |
| /base-models | Base Models | 0 | Build | Build |  |
| /router-config | Adapter Routing | 6 | Build | Build |  |
| /federation | Federation | 7 | Build | Build | IA-EXTRA |

## Build (expected)
- /workflow, /flow/lora, /personas, /management
- /create-adapter, /adapters (+ detail routes)
- /training (+ jobs, datasets, templates), /trainer
- /promotion, /base-models, /router-config
- /admin (+ tenants, stacks, plugins, settings)

## Run (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /dashboard | Dashboard | 1 | Run | Run |  |
| /inference | Inference | 1 | Run | Run |  |
| /chat | Chat | 2 | Run | Run |  |
| /documents | Documents | 3 | Run | Run |  |
| /documents/:documentId/chat | — | — | — | Run | no navTitle |
| /code-intelligence | Code Intelligence | 6 | Run | Run |  |

## Run (expected)
- /dashboard, /inference, /chat
- /documents (+ chat)
- /code-intelligence

## Observe (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /metrics | Metrics | 1 | Observe | Observe |  |
| /monitoring | System Health | 2 | Observe | Observe |  |
| /routing | Selection History | 3 | Observe | Observe |  |
| /system | System Overview | 1 | Observe | Observe | navOrder ties metrics |
| /system/nodes | Nodes | 2 | Observe | Observe |  |
| /system/workers | Workers | 3 | Observe | Observe |  |
| /system/memory | Memory | 4 | Observe | Observe |  |
| /system/metrics | System Metrics | 5 | Observe | Observe |  |
| /telemetry | Event History | 4 | Observe | Observe |  |
| /telemetry/viewer | — | — | — | Observe | no navTitle |
| /reports | Reports | 6 | Observe | Observe |  |
| /help | Help Center | 1 | Observe | Observe |  |

## Observe (expected)
- /monitoring, /metrics, /metrics/advanced, /routing
- /system (+ nodes, workers, memory, metrics)
- /telemetry (+ viewer), /reports, /help

## Verify (current)
| Path | navTitle | navOrder | navGroup | cluster | Notes |
| --- | --- | --- | --- | --- | --- |
| /owner | Owner Home | 0 | Verify | Verify |  |
| /testing | Testing | 3 | Verify | Verify |  |
| /golden | Verified Runs | 4 | Verify | Verify |  |
| /replay | Run History | 5 | Verify | Verify |  |
| /security/policies | Guardrails | 1 | Verify | Verify |  |
| /security/audit | Audit Logs | 2 | Verify | Verify |  |
| /security/compliance | Compliance | 3 | Verify | Verify |  |
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

## IA-EXTRA handling
- /federation retained and labeled `IA-EXTRA` in route config.
- Dev-only routes (`/dev/errors`, `/_dev/routes`) kept for debugging; nav entries gated by DEV flag. `/_dev/routes` should mirror IA and is the canonical debug view for route parity.

MLNavigator Inc 2025-12-05.

