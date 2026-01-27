# AdapterOS UI Route Map

This document inventories all routes and maps each to the 6-module control-plane navigation (plus Tools/Hidden). It also records redirects and hidden entries for compatibility.

## Modules (Target)

- **Run**: run inference and inspect runs (canonical object)
- **Prove**: audit/provenance verification
- **Configure**: adapters, stacks, policies, models
- **Data**: datasets + documents (core data surfaces)
- **Operate**: health ladder (dashboard → infrastructure → workers → metrics → incidents)
- **Govern**: admin, human review, settings
- **Tools**: debug/experimental utilities
- **Hidden**: public or system-only entry points not shown in primary nav

## Complete Route Mapping

| Old Route | New Canonical Route | Module | Status | Reason |
|---|---|---|---|---|
| `/login` | `/login` | Hidden | Keep | Auth entry (not in nav) |
| `/` | `/` | Operate | Keep | Dashboard summary |
| `/dashboard` | `/` | Operate | Redirect | Single dashboard entry |
| `/adapters` | `/adapters` | Configure | Keep | Adapter management |
| `/adapters/:id` | `/adapters/:id` | Configure | Keep | Adapter detail |
| `/chat` | `/chat` | Run | Keep | Primary inference surface |
| `/chat/:session_id` | `/chat/:session_id` | Run | Keep | Session detail |
| `/system` | `/system` | Operate | Keep | Infrastructure topology/services |
| `/settings` | `/settings` | Govern | Keep | Preferences & system info |
| `/models` | `/models` | Configure | Keep | Model registry |
| `/policies` | `/policies` | Configure | Keep | Policy packs |
| `/training` | `/training` | Operate | Hide | Advanced surface (via search/deep link) |
| `/stacks` | `/stacks` | Configure | Keep | Runtime stacks |
| `/stacks/:id` | `/stacks/:id` | Configure | Keep | Stack detail |
| `/collections` | `/collections` | Data | Hide | Secondary data surface |
| `/collections/:id` | `/collections/:id` | Data | Hide | Secondary data surface |
| `/documents` | `/documents` | Data | Keep | Document store |
| `/documents/:id` | `/documents/:id` | Data | Keep | Document detail |
| `/datasets` | `/datasets` | Data | Keep | Dataset management |
| `/datasets/:id` | `/datasets/:id` | Data | Keep | Dataset detail |
| `/repositories` | `/repositories` | Data | Hide | Secondary data surface |
| `/repositories/:id` | `/repositories/:id` | Data | Hide | Secondary data surface |
| `/admin` | `/admin` | Govern | Keep | Administration |
| `/audit` | `/audit` | Prove | Keep | Audit log |
| `/runs` | `/runs` | Run | Keep | Runs list |
| `/runs/:id` | `/runs/:id` | Run | Keep | Run Detail hub (tabs) |
| `/flight-recorder` | `/runs` | Run | Redirect | Legacy name |
| `/flight-recorder/:id` | `/runs/:id` | Run | Redirect | Legacy name |
| `/diff` | `/runs/:id?tab=diff` | Tools | Redirect | Diff moves under Run Detail (launcher kept) |
| `/workers` | `/workers` | Operate | Keep | Worker pool management |
| `/workers/:id` | `/workers/:id` | Operate | Keep | Worker detail |
| `/monitoring` | `/monitoring` | Operate | Keep | Metrics (renamed) |
| `/errors` | `/errors` | Operate | Keep | Incidents (renamed) |
| `/routing` | `/routing` | Tools | Keep | Routing Debug |
| `/reviews` | `/reviews` | Govern | Keep | Human Review |
| `/agents` | `/agents` | Tools | Hide | Experimental surface |
| `/safe` | `/safe` | Hidden | Hide | Safety mode (public fallback) |
| `/style-audit` | `/style-audit` | Tools | Keep | Style audit |

## Run Detail Tabs (Canonical)

`/runs/:id` is the single provenance front door. Tabs:
- **Overview**
- **Trace**
- **Receipt**
- **Routing**
- **Tokens**
- **Diff** (optional)

## Redirect Notes

- `/dashboard` → `/`
- `/flight-recorder` → `/runs`
- `/flight-recorder/:id` → `/runs/:id`
- `/diff` → Run Detail Diff launcher (select a run, then open `/runs/:id?tab=diff`)
