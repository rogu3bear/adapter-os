# AdapterOS UI Route Map

This document maps all existing routes to the new 6-module navigation structure.

## Module Structure

| Module | Purpose | Primary Routes |
|--------|---------|----------------|
| **Run** | Execute inference, view active sessions | `/chat`, `/runs` |
| **Prove** | Verify provenance, audit trail, receipts | `/audit`, `/runs/:id` |
| **Configure** | Set up adapters, stacks, policies | `/adapters`, `/stacks`, `/policies` |
| **Data** | Manage datasets, documents, repositories | `/datasets`, `/documents`, `/repositories`, `/collections` |
| **Operate** | System health, workers, monitoring | `/`, `/system`, `/workers`, `/monitoring`, `/errors` |
| **Govern** | Admin, reviews, settings | `/admin`, `/reviews`, `/settings` |
| **Tools** | Debug and experimental pages | `/routing`, `/diff`, `/style-audit`, `/safe` |

## Complete Route Mapping

### Run Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/chat` | `/chat` | Keep | Primary inference interface |
| `/chat/:session_id` | `/chat/:session_id` | Keep | Session detail |
| `/runs` | `/runs` | Keep | Run history (formerly FlightRecorder) |
| `/runs/:id` | `/runs/:id` | **Enhanced** | Canonical Run Detail hub with tabs |

### Prove Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/audit` | `/audit` | Keep | Audit log with hash chain |
| `/runs/:id?tab=trace` | `/runs/:id?tab=trace` | New | Trace tab in Run Detail |
| `/runs/:id?tab=receipt` | `/runs/:id?tab=receipt` | New | Receipt verification tab |
| `/runs/:id?tab=routing` | `/runs/:id?tab=routing` | New | Routing decisions tab |

### Configure Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/adapters` | `/adapters` | Keep | Adapter management |
| `/adapters/:id` | `/adapters/:id` | Keep | Adapter detail |
| `/stacks` | `/stacks` | Keep | Runtime stack configuration |
| `/stacks/:id` | `/stacks/:id` | Keep | Stack detail |
| `/policies` | `/policies` | Keep | Policy pack management |
| `/models` | `/models` | Keep | Model registry |

### Data Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/datasets` | `/datasets` | Keep | Dataset management |
| `/datasets/:id` | `/datasets/:id` | Keep | Dataset detail |
| `/documents` | `/documents` | Keep | Document management |
| `/documents/:id` | `/documents/:id` | Keep | Document detail |
| `/repositories` | `/repositories` | Keep | Code repository scanning |
| `/repositories/:id` | `/repositories/:id` | Keep | Repository detail |
| `/collections` | `/collections` | Keep | Collection management |
| `/collections/:id` | `/collections/:id` | Keep | Collection detail |

### Operate Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/` | `/` | Keep | Dashboard (system summary) |
| `/system` | `/system` | **Rename** | Now "Infrastructure" in nav |
| `/workers` | `/workers` | Keep | Worker management |
| `/workers/:id` | `/workers/:id` | Keep | Worker detail |
| `/monitoring` | `/monitoring` | **Rename** | Now "Metrics" in nav |
| `/errors` | `/errors` | **Rename** | Now "Incidents" in nav |
| `/training` | `/training` | Keep | Training jobs |
| `/agents` | `/agents` | Keep | Agent management |

### Govern Module

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/admin` | `/admin` | Keep | Administration panel |
| `/reviews` | `/reviews` | **Rename** | Now "Human Review" in nav |
| `/settings` | `/settings` | Keep | User settings |

### Tools (Collapsed)

| Old Route | New Route | Status | Reason |
|-----------|-----------|--------|--------|
| `/routing` | `/routing` | **Move** | Now "Routing Debug" under Tools |
| `/diff` | `/diff` | **Move** | Now under Tools |
| `/style-audit` | `/style-audit` | Keep | Dev tool under Tools |
| `/safe` | `/safe` | **Hide** | Dev-only, not in nav |

### Hidden/Redirects

| Old Route | Target | Status | Reason |
|-----------|--------|--------|--------|
| `/login` | `/login` | Keep | Authentication (not in nav) |
| `/diff?run_a=X&run_b=Y` | `/runs/:id?tab=diff` | Redirect | Redirect to Run Detail diff tab |

## Navigation Label Changes

| Old Label | New Label | Location |
|-----------|-----------|----------|
| FlightRecorder | Runs | StartMenu, page header |
| System | Infrastructure | StartMenu, Taskbar |
| Monitoring | Metrics | StartMenu |
| Errors | Incidents | StartMenu |
| Reviews | Human Review | StartMenu |
| Safe | Safety Mode | Tools (hidden) |
| Routing | Routing Debug | Tools |

## Run Detail Hub Tabs

The canonical `/runs/:id` page now has these tabs:

| Tab | Content | Source |
|-----|---------|--------|
| Overview | Run summary, status, timing | New |
| Trace | Full trace visualization | TraceViewer component |
| Receipt | Cryptographic receipt verification | ReceiptVerification component |
| Routing | K-sparse routing decisions | TokenDecisions + routing debug |
| Tokens | Token accounting, cache stats | From trace data |
| Diff | Compare with another run | From diff.rs (optional) |

## Compatibility Matrix

All old routes continue to work via:
1. Direct serving (unchanged routes)
2. Leptos router redirects (moved routes)
3. Query parameter preservation

No breaking changes for deep links or bookmarks.
