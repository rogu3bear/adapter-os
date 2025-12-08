## Adapter UI updates (Dec 2025)
- `AdaptersPage` now persists filter/search/sort via `useAdapterFilterState` (localStorage keyed by tenant/user) and renders the saved state on reload.
- `useAdapterActions` wraps load/unload/delete flows with shared `ConfirmationModal` copy, refetches lists, and surfaces 409 conflicts with the refreshed-view toast.
- Adapter detail includes a Recent activity block composed from lineage/activation history with existing badges/list primitives.
MLNavigator Inc 2025-12-08.
# UI Component Outline

## Providers & Layouts (Global Structure)
- **CoreProviders.tsx** (`ui/src/providers/`): Wraps app with contexts (e.g., auth, theme).
  - Distinguish: FeatureProviders (RBAC) vs. AppProviders (routing).
- **RootLayout.tsx** (`ui/src/layout/`): Navigation, toasts, error boundaries.
  - Flow: Auth guard → Page render → Footer.

## Pages (High-Level Views)
- **Dashboard.tsx** → Widgets (e.g., ActiveAlertsWidget.tsx, RealtimeMetrics.tsx).
  - Distinguish: Observability (MetricsChart.tsx) vs. Management (AdaptersPage.tsx).
- **TrainingPage.tsx** → Wizards (TrainingWizard.tsx, SingleFileAdapterTrainer.tsx).
  - Flow: Select repo → Ingest → Train → Monitor.
- **MonitoringPage.tsx** → Panels (RoutingInspector.tsx, ReplayPanel.tsx).
  - Distinguish: Real-time (usePolling.ts) vs. Historical (TraceVisualizer.tsx).

## Reusable Components (ui/)
- **Primitives** (e.g., button.tsx, dialog.tsx): Shadcn-based, 70+ files.
  - Distinguish: Input forms (form.tsx) vs. Navigation (sidebar.tsx).
- **Domain-Specific** (e.g., AdapterImportWizard.tsx, PolicyEditor.tsx).
  - Hooks: useAdapterOperations.ts (mutations) vs. useActivityFeed.ts (queries).

## Dashboard Widgets
- Wrap every dashboard widget in `DashboardWidgetFrame` to get consistent title, subtitle, refresh, last-updated, and standardized loading/error/empty/ready states.
- Refresh buttons should call the widget’s React Query `refetch` (or invalidate) and show the last update timestamp from the underlying query or polling hook.
- See the pattern below for required props and migration rules.

## Dashboard widget pattern
- All new dashboard widgets MUST use `DashboardWidgetFrame`.
- When modifying an existing widget, migrate it into `DashboardWidgetFrame` if feasible.
- Map your data fetch status into `state` (`'loading' | 'error' | 'empty' | 'ready'`), wire `onRefresh` to the query `refetch`, and pass `lastUpdated` from the same source.
```tsx
const { data, isLoading, error, refetch } = useQuery(...);
const state: DashboardWidgetState = isLoading ? 'loading' : error ? 'error' : !data ? 'empty' : 'ready';

<DashboardWidgetFrame
  title="Adapter health"
  subtitle="Fleet-wide status"
  state={state}
  onRefresh={refetch}
  lastUpdated={data?.updatedAt ? new Date(data.updatedAt) : null}
  emptyMessage="No adapters yet"
>
  <WidgetBody data={data} />
</DashboardWidgetFrame>
```

## Testing & Utils
- **Hooks** (24 files, e.g., usePolling.ts): Polling, mutations, queries.
- **Tests** (__tests__/, e2e/): Cover 30% (e.g., dashboard.cy.ts).
- **Navigation** (utils/navigation.ts): Routes guard (route-guard.tsx).

[source: ui/src/providers/CoreProviders.tsx L1-L50]
[source: ui/src/components/Dashboard.tsx L1-L100]
[source: ui/src/components/ui/button.tsx L1-L20]
MLNavigator Inc 2025-12-08.
