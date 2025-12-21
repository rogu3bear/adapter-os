# UI Error Handling

This document summarizes the shared primitives for logging and surfacing UI errors and warnings.

## Taxonomy
- Severity: `info` (heads-up), `warning` (non-blocking or transient), `error` (blocking or failed action).
- Context: scope (`global`, `page`, `section`, `modal`, `chat`, etc.), route/pageKey, component name.
- Optional `userMessageKey` allows consistent, localizable messaging across toasts and logs.

## Logging Primitives
- `logUIError(error, { scope, component, route?, pageKey?, severity?, userMessageKey? })`
  - Defaults to `severity: 'error'`.
  - Sends severity into structured logs and `captureException` extra metadata.
- `logUIWarning(error, context)` is a thin helper that calls `logUIError` with `severity: 'warning'`.

## When to use warning vs error
- `logUIError`: failures that block a primary flow or whole page (e.g., dashboard page fetch failure, adapter list failure, modal that cannot render).
- `logUIWarning`: recoverable or secondary issues that do not stop the main flow (e.g., optional widget failure, telemetry hiccups that auto-retry).

### Examples
- Soft warning (non-blocking widget):

```ts
try {
  await fetchOptionalTelemetry();
} catch (err) {
  logUIWarning(err, {
    scope: 'section',
    component: 'TelemetryPanel',
    pageKey: 'telemetry',
  });
}
```

- Hard error (blocks page load):

```ts
try {
  await fetchDashboard();
} catch (err) {
  logUIError(err, {
    scope: 'page',
    component: 'DashboardPage',
    pageKey: 'dashboard',
    route: '/dashboard',
  });
}
```

- Query handler mapping (HTTP → severity):

```ts
import { normalizeQueryError } from '@/lib/queryErrorHandler';

const { severity } = normalizeQueryError({ status: 503, message: 'Service unavailable' });
// severity === 'error' because 5xx is treated as a blocking failure
```

## Error Boundaries
- Global (`components/shared/Feedback/ErrorBoundary`), page (`components/ui/page-error-boundary`), section (`components/ui/section-error-boundary`), and modal (`components/ui/modal-error-boundary`) boundaries all log with `severity: 'error'`.
- Each fallback shows destructive styling, a concise message, and a “Try again” action that calls the boundary reset/refetch hook where provided.
- Section fallback accepts `severity?: 'warning' | 'error'` for softer, non-blocking panels (uses amber styling for warnings).

## Query Error Handling
- Central handler lives in `lib/queryErrorHandler` and is registered on both the query and mutation caches in `AppProviders`.
- Mapping rules:
  - Network/timeout/`failed to fetch` → `warning` + toast (yellow), `userMessageKey` defaults to `ui.error.network`.
  - HTTP 5xx → `error` + toast (red).
  - HTTP 4xx except 401/403 → `error` + toast (red).
  - 401/403 → logged as `warning`, toast suppressed (auth flow handles UX).
- Deduplication: a single toast per error key within ~6 seconds (`toastKey` meta can override). `suppressErrorToast` meta disables UX surfaces but still logs.
- Titles/descriptions: `meta.errorMessage`/`meta.toastDescription` override; otherwise the normalized error message is used.

## Toast & UI Patterns
- Toast variants mirror severity (`warning` vs `error`) to keep visual distinction between soft and hard failures.
- Hard failures (errors) keep the destructive patterns in boundaries/cards; warnings use amber styling on section fallback for non-blocking panels.

MLNavigator Inc 2025-12-08.

