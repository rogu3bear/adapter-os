# UI Data Flow Documentation

This document describes the data fetching patterns, polling strategies, and refetch triggers in the AdapterOS UI.

## Core Hooks

### `use_api_resource<T>(fetch)`

Primary data fetching hook. Returns `(ReadSignal<LoadingState<T>>, Callback<()>)`.

Features:
- Automatic initial fetch on mount
- Version tracking to prevent stale data overwrites
- Automatic error reporting to server
- Returns refetch callback for manual refresh

```rust
let (data, refetch) = use_api_resource(|client| async move {
    client.list_items().await
});
```

### `use_polling(interval_ms, fetch)`

Simple polling with automatic cleanup on unmount.

Features:
- Skips polling when tab is hidden (reduces server load)
- Skips polling when offline
- Proper interval cleanup on unmount
- Returns cancel function

```rust
let cancel = use_polling(10_000, move || async move {
    refetch.run(());
});
```

### `use_conditional_polling(interval_ms, should_poll, fetch)`

Polling that enables/disables based on a reactive signal.

Use case: Poll only when there are running jobs to monitor.

```rust
let has_running_jobs = Signal::derive(move || {
    jobs.get().data().map(|j| j.iter().any(|x| x.status == "running")).unwrap_or(false)
});
let _ = use_conditional_polling(3_000, has_running_jobs.into(), move || async move {
    refetch.run(());
});
```

### `use_sse_notifications()`

SSE-based real-time notifications from the server.

Features:
- Reconnects on disconnect
- Provides notification stream for UI updates
- Used on Dashboard and System pages

## Polling Intervals by Page

| Page | Endpoint | Interval | Trigger | Notes |
|------|----------|----------|---------|-------|
| `/runs` (FlightRecorder) | `list_diag_runs` | 10s | `use_polling` | Always polls |
| `/workers` | `list_workers`, `list_nodes` | 10s | `use_polling` | Always polls |
| `/system` | `system_status`, `workers`, `nodes`, `metrics`, `models_status`, `state` | 30s | `use_polling` | Lower frequency |
| `/monitoring` | `list_alerts`, `system_metrics`, `active_alerts`, `alert_rules` | 10s | `use_polling` | Always polls |
| `/training/:id` | `get_training_job` | 3s | `use_conditional_polling` | Only when job is running |
| Dashboard | `system_status`, `workers`, `activity`, `metrics` | N/A | SSE + initial | Uses SSE notifications |

## Refetch Triggers

### Manual Refetch (User Actions)

These trigger explicit `refetch.run(())` calls:

| Page | Action | What Refreshes |
|------|--------|----------------|
| `/diff` | "Refresh Runs" button | Runs list |
| `/adapters` | Delete/Update adapter | Adapters list |
| `/stacks` | Create/Update stack | Stacks list |
| `/policies` | Update policy | Policy detail |
| `/datasets` | Update/Delete dataset | Dataset list |
| `/audit` | Tab change | Logs, chain, or compliance data |
| `/training` | Status filter change | Jobs list |
| `/repositories` | Sync repository | Repository list |

### Reactive Refetch (Signal Changes)

Some resources refetch when reactive signals change:

| Page | Signal | Effect |
|------|--------|--------|
| `/runs` | `status_filter` | Refetches with new filter |
| `/audit` | Filter signals (action, status, resource) | Refetches with new query |
| `/training` | Status filter | Refetches with new filter |
| `/routing/decisions` | Status filter | Refetches with new filter |

### SSE-Triggered Updates

Dashboard uses SSE for real-time updates instead of polling:

```rust
let (_notifications, _refetch_notifications) = use_sse_notifications();
```

The SSE stream provides:
- Training job status changes
- System state changes
- Alert notifications

## Duplicate Fetch Prevention

### Built-in Protections

1. **Version Tracking**: `use_api_resource` uses atomic version counters to discard stale responses
2. **Tab Visibility**: Polling hooks skip when tab is hidden
3. **Offline Detection**: Polling hooks skip when offline
4. **Conditional Polling**: Only polls when condition signal is true

### Identified Duplicate Patterns

The following pages fetch overlapping data:

1. **Workers List**
   - `/workers`: `list_workers()` + polling
   - `/system`: `list_workers()` + polling
   - Dashboard: `list_workers()` (no polling)

2. **System Status**
   - `/system`: `system_status()` + polling
   - Dashboard: `system_status()` (no polling)

3. **System Metrics**
   - `/system`: `system_metrics()` + polling
   - `/monitoring`: `system_metrics()` + polling
   - Dashboard: `system_metrics()` (no polling)

### Recommendations

1. **Consider shared context for cross-page data**
   - Workers list, system status, and metrics could be in a shared `SystemContext`
   - Pages would subscribe rather than fetch independently

2. **Use SSE more broadly**
   - Training detail page could use SSE instead of 3s polling
   - Monitoring alerts could use SSE for real-time updates

3. **Standardize polling intervals**
   - Currently: 3s, 10s, 30s depending on page
   - Consider: 5s for real-time needs, 30s for background updates

## LoadingState Pattern

All resources use consistent `LoadingState<T>` enum:

```rust
enum LoadingState<T> {
    Idle,      // Not started
    Loading,   // In progress
    Loaded(T), // Success
    Error(ApiError), // Failed
}
```

UI components should handle all states:

```rust
{move || match data.get() {
    LoadingState::Idle | LoadingState::Loading => view! { <Spinner/> }.into_any(),
    LoadingState::Loaded(items) => view! { <ItemList items=items/> }.into_any(),
    LoadingState::Error(e) => view! { <ErrorDisplay error=e/> }.into_any(),
}}
```

## Canonical Endpoints by Module

### Runs Module
- `list_diag_runs(query)` - List runs with filtering
- `export_diag_run(id)` - Full run export with events
- `get_inference_trace_detail(trace_id)` - Trace data for run

### Audit Module
- `get_audit_logs(query)` - Timeline entries
- `get_audit_chain(limit)` - Hash chain entries
- `verify_audit_chain()` - Chain verification
- `get_compliance_audit(query)` - Compliance report

### System Module
- `system_status()` - Overall system health
- `system_metrics()` - Resource metrics
- `list_workers()` - Worker instances
- `list_nodes()` - Node topology
- `list_models_status()` - Model loading status

### Training Module
- `list_training_jobs(query)` - Jobs with filtering
- `get_training_job(id)` - Single job detail
- `list_datasets()` - Available datasets
- `training_backend_readiness()` - Backend status

## Error Handling

All API errors are automatically reported via `report_error()`:
- Logs to console in development
- Reports to server endpoint `/ui/errors`
- Includes page path for context
- Skips aborted requests (navigation)

Components should use `ErrorDisplay` for consistent error UI:

```rust
LoadingState::Error(e) => view! {
    <ErrorDisplay error=e/>
}.into_any()
```
