# UI Boundary Audit

Date: 2025-02-14
Scope: UI route components defined in `ui/src/config/routes.ts`

## Boundary model (current)
- Root layout wraps `Outlet` with `maxWidth=--layout-content-width-xl` and safe-area padding.
- `FeatureLayout` and `PageWrapper` apply page-level padding, headers, and `min-w-0/min-h-0`.
- `PageTable` and `Table` wrap tabular content with `overflow-x-auto`.
- Split panels in `FeatureLayout` keep the outer container non-scrolling and delegate overflow to panel bodies.

## Route component inventory
### Custom or no wrapper (manual boundary check)
- `ui/src/pages/Admin/StackDetailModal.tsx` (modal route)
- `ui/src/pages/DocumentLibrary/DocumentChatPage.tsx`
- `ui/src/pages/Repositories/RepositoriesShell.tsx` (route switch shell)
- `ui/src/pages/Security/ComplianceTab.tsx`
- `ui/src/pages/System/NodeDetailModal.tsx` (modal route)
- `ui/src/pages/Training/DatasetChatPage.tsx`
- `ui/src/pages/Training/ResultChatPage.tsx`

### FeatureLayout
- `ui/src/pages/Adapters/AdaptersShell.tsx`
- `ui/src/pages/Admin/AdminPage.tsx`
- `ui/src/pages/Admin/PluginsPage.tsx`
- `ui/src/pages/Admin/SettingsPage.tsx`
- `ui/src/pages/Dev/ContractsPage.tsx`
- `ui/src/pages/DevErrorsPage.tsx`
- `ui/src/pages/EvidencePage.tsx`
- `ui/src/pages/GoldenPage.tsx`
- `ui/src/pages/MetricsPage.tsx`
- `ui/src/pages/PoliciesPage.tsx`
- `ui/src/pages/Replay/ReplayShell.tsx`
- `ui/src/pages/RoutingPage.tsx`
- `ui/src/pages/System/MemoryTab.tsx`
- `ui/src/pages/System/MetricsTab.tsx`
- `ui/src/pages/System/NodesTab.tsx`
- `ui/src/pages/System/PilotStatusPage.tsx`
- `ui/src/pages/System/SystemOverviewPage.tsx`
- `ui/src/pages/System/WorkersTab.tsx`
- `ui/src/pages/TelemetryPage.tsx`
- `ui/src/pages/WorkspacesPage.tsx`

### PageWrapper
- `ui/src/pages/Admin/AdapterStacksTab.tsx`
- `ui/src/pages/AuditPage.tsx`
- `ui/src/pages/BaseModelsPage.tsx`
- `ui/src/pages/ChatPage.tsx`
- `ui/src/pages/DashboardPage.tsx`
- `ui/src/pages/Dev/RoutesDebugPage.tsx`
- `ui/src/pages/DocumentLibrary/index.tsx`
- `ui/src/pages/FederationPage.tsx`
- `ui/src/pages/InferencePage.tsx`
- `ui/src/pages/RouterConfigPage.tsx`
- `ui/src/pages/TestingPage.tsx`
- `ui/src/pages/Training/TrainingShell.tsx`

## Boundary overrides and full-bleed pages
- `ui/src/pages/ChatPage.tsx` uses `contentPadding="none"`; ensure internal rails own overflow.
- `ui/src/pages/TelemetryPage.tsx` uses `maxWidth="full"`; wide tables and traces should stay inside `overflow-x-auto` containers.
- `ui/src/pages/DocumentLibrary/DocumentChatPage.tsx`, `ui/src/pages/Training/DatasetChatPage.tsx`, `ui/src/pages/Training/ResultChatPage.tsx` implement custom full-height layouts with `overflow-hidden` main regions; re-check header and chip rows for long labels.
- Modal routes (`ui/src/pages/Admin/StackDetailModal.tsx`, `ui/src/pages/System/NodeDetailModal.tsx`) rely on `DialogContent` sizing; long strings should use `break-words` or `overflow-x-auto` where needed.

## Static spillover risk flags
- `ui/src/pages/Security/PolicyReviewQueue.tsx`: JSON preview uses `overflow-y-auto` only. Add `overflow-x-auto` or `break-words` if long values are causing horizontal spillover.
- `ui/src/pages/Security/ComplianceTab.tsx`: audit metadata uses `flex justify-between`; long IDs may overflow on narrow widths. Consider `break-all` or `truncate` + tooltip if this page is routed directly.
- `ui/src/pages/Adapters/AdapterManifest.tsx`: JSON viewer is wrapped in `overflow-auto`; if long tokens still spill, add `break-words` on `pre` content.

## How to verify visually
- Enable layout overlay in dev: add `?layoutDebug=true` to a route or call `window.__toggleLayoutDebug()`.
- Resize to narrow widths (1280/1024/768) and watch for red-outlined elements.
- For any element wider than the viewport, prefer `min-w-0` in flex containers and `overflow-x-auto` on long, unbroken content.
