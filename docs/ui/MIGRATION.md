# UI Navigation Migration Notes

This document describes the changes made to rectify the AdapterOS UI into a coherent 6-module navigation structure.

## Summary

The UI has been refactored from a 22+ page "operating system" feel to a focused control plane with 6 primary modules:

1. **Run** - Execute inference (Chat, Runs)
2. **Prove** - Verify provenance (Audit, Run Detail)
3. **Configure** - Set up behavior (Adapters, Stacks, Policies, Models)
4. **Data** - Manage data (Datasets, Documents, Repositories, Collections)
5. **Operate** - System health (Dashboard, Infrastructure, Workers, Metrics, Incidents, Training, Agents)
6. **Govern** - Administration (Admin, Human Review, Settings)

Plus a collapsed **Tools** section for debug pages.

## What Changed

### Navigation Components

| Component | Change |
|-----------|--------|
| `start_menu.rs` | Rewritten with 6-module collapsible structure |
| `taskbar.rs` | Updated with module-level shortcuts instead of page shortcuts |

### Run Detail Hub

The `/runs/:id` page is now the **canonical provenance viewer** with unified tabs:

| Tab | Content | Source |
|-----|---------|--------|
| Overview | Run summary, status, timing, quick links | New |
| Trace | TraceViewer component | Existing component |
| Receipt | Hash verification, metadata | Existing (Receipts tab) |
| Routing | K-sparse routing events | New |
| Tokens | Token accounting, cache stats | New |
| Events | Collapsible event groups | Existing (Events tab) |

### Label Renames

| Old Label | New Label |
|-----------|-----------|
| Flight Recorder | Runs |
| System | Infrastructure |
| Monitoring | Metrics |
| Errors | Incidents |
| Reviews | Human Review |
| Routing | Routing Debug (moved to Tools) |

### Route Compatibility

All existing routes continue to work:
- `/runs` → Runs list (formerly FlightRecorder)
- `/runs/:id` → Run Detail hub
- `/runs/:id?tab=trace` → Trace tab
- `/runs/:id?tab=receipt` → Receipt tab
- `/runs/:id?tab=routing` → Routing tab
- `/runs/:id?tab=tokens` → Tokens tab
- `/runs/:id?tab=events` → Events tab

No breaking changes to deep links.

## Files Modified

```
crates/adapteros-ui/src/components/layout/start_menu.rs  # Rewritten
crates/adapteros-ui/src/components/layout/taskbar.rs     # Rewritten
crates/adapteros-ui/src/pages/flight_recorder.rs         # Enhanced with tabs
```

## Files Added

```
docs/ui/route-map.md      # Complete route mapping
docs/ui/navigation.md     # Navigation architecture
docs/ui/MIGRATION.md      # This file
```

## Verification Checklist

- [x] StartMenu shows 6 modules + Tools (collapsed)
- [x] Taskbar shows module shortcuts
- [x] Run Detail has all 6 tabs (Overview, Trace, Receipt, Routing, Tokens, Events)
- [x] Tab navigation via URL query params (`?tab=trace`)
- [x] TraceViewer component integrated into Trace tab
- [x] All existing routes preserved
- [x] Build compiles for WASM

## Testing Notes

1. **Navigation Flow**
   - Click Start → verify 6 modules visible
   - Click Tools → verify it expands to show debug pages
   - Taskbar shows: Run, Prove, Configure, Data, Operate, Govern

2. **Run Detail Hub**
   - Navigate to `/runs`
   - Select a run → verify split panel opens
   - Click through all tabs → verify content loads
   - Test URL deep links: `/runs/:id?tab=receipt`

3. **Compatibility**
   - Existing bookmarks to `/runs/:id` still work
   - Chat trace links navigate to Run Detail

## Known Limitations

1. **Tokens tab**: Shows placeholder values; needs backend integration for actual token counts
2. **Routing tab**: Shows events but not the full routing debug panel
3. **Diff tab**: Not implemented in this iteration (would compare two runs)
