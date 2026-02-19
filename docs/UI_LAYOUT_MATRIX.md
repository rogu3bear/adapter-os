# UI Layout Matrix

Phase 6 decision guide for choosing page layout primitives in the Leptos UI.

## Decision Matrix

| Pattern | Use when | Avoid when | Concrete existing examples |
|---|---|---|---|
| `SplitPanel` | Operators must keep list context visible while inspecting detail. Selection in the list is the main interaction. | The detail flow is long-form editing that needs full width or a dedicated route. | `/adapters`, `/training`, `/workers`, `/runs` (Restore Points), `/update-center` |
| `TabNav` + `TabPanel` | One surface has multiple views of the same entity/domain and should keep shared context in place. | Tabs would hide unrelated flows that should be separate routes. | `/settings`, `/admin`, `/routing`, `/audit` |
| `DataTable` only | A list-heavy page where row actions and filters are primary, and side-by-side detail is not needed. | Users frequently need list + detail at the same time. | `/collections`, `/reviews`, `/agents` |

## Quick Selection Rules

1. Need list + detail simultaneously for fast triage: choose `SplitPanel`.
2. Need multiple perspectives under one route context: choose `TabNav`/`TabPanel`.
3. Need a clean list workflow with row-level actions: choose `DataTable` only.

## Guided Flow Alignment

- Prefer `SplitPanel` on Guided Flow surfaces where continuity matters across Teach -> Verify -> Promote (for example: Training, Restore Points, Update Center).
