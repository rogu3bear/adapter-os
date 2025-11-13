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

## Testing & Utils
- **Hooks** (24 files, e.g., usePolling.ts): Polling, mutations, queries.
- **Tests** (__tests__/, e2e/): Cover 30% (e.g., dashboard.cy.ts).
- **Navigation** (utils/navigation.ts): Routes guard (route-guard.tsx).

[source: ui/src/providers/CoreProviders.tsx L1-L50]
[source: ui/src/components/Dashboard.tsx L1-L100]
[source: ui/src/components/ui/button.tsx L1-L20]
