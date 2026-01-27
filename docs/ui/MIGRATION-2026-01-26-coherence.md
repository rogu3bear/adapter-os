# UI Coherence Migration - 2026-01-26

This document describes the changes made to improve UI coherence, cross-linking, and terminology consistency.

## Summary

This update integrates the UI into a cohesive product experience where:
- Every inference output maps to a Run
- Every Run has a receipt that can be verified
- Cross-links connect related data throughout the UI
- Terminology is consistent across all pages

## Changes

### Cross-Linking (Chat → Runs)

**File: `crates/adapteros-ui/src/pages/chat.rs`**

Added "Run" and "Receipt" links to assistant messages:
- "Run" → `/runs/{trace_id}` (Overview tab)
- "Receipt" → `/runs/{trace_id}?tab=receipt`

These links appear next to the existing TraceButton for any assistant response that has a trace_id.

### Cross-Linking (Audit → Runs)

**File: `crates/adapteros-ui/src/pages/audit/tabs.rs`**

Timeline entries now detect run-related resources (inference, run, trace, diag_run) and render the resource_id as a clickable link to `/runs/{id}`.

### Cross-Linking (Diff → Runs)

**File: `crates/adapteros-ui/src/pages/diff.rs`**

The `/diff` page now redirects to Run Detail when query params are provided:
- `/diff?run={id}` → `/runs/{id}?tab=diff`
- `/diff?run_a={id}&run_b={id2}` → `/runs/{id}?tab=diff&compare={id2}`

### Run Detail Configuration Section

**File: `crates/adapteros-ui/src/pages/flight_recorder.rs`**

Added "Configuration" card to Overview tab showing:
- Stack (currently "Unknown" - backend doesn't capture yet)
- Model (currently "Unknown")
- Policy (currently "Unknown")
- Backend (currently "Unknown")

Includes explanatory text that configuration identifiers are not yet captured in diagnostic runs.

### Quick Actions

**Dashboard** (`crates/adapteros-ui/src/pages/dashboard.rs`):
- New Run → `/chat`
- Verify Receipt → `/runs`
- Activate Stack → `/stacks`
- Upload Document → `/datasets`
- View Alerts → `/monitoring` (if permission)

**Run Detail** (`crates/adapteros-ui/src/pages/flight_recorder.rs`):
- Copy Run ID
- Copy Receipt Hash
- Export
- Open Diff

## Documentation Created

- `docs/ui/terminology.md` - Canonical terminology mapping
- `docs/ui/data-flow.md` - Data fetching patterns and polling documentation
- `docs/ui/qa-checklist.md` - QA validation checklist

## Terminology Changes

| Old | New | Notes |
|-----|-----|-------|
| Flight Recorder | Runs | Page header already correct |
| Proof | Receipt | Consistent across all surfaces |
| Verification | Receipt | Tab label standardized |

## Breaking Changes

None. All existing routes continue to work. New redirects added for convenience.

## Testing

Verify the following flows work:

1. **Chat → Run Detail**: Send a message, click "Run" link, verify Run Detail opens
2. **Run Detail → Receipt**: Click "Receipt" tab, verify hashes displayed
3. **Audit → Run Detail**: View timeline with inference entries, click resource_id
4. **Diff redirect**: Navigate to `/diff?run=abc123`, verify redirect to `/runs/abc123?tab=diff`
5. **Quick Actions**: Test all quick action buttons on Dashboard and Run Detail
