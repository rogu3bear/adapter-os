# Dashboard Inventory (agent note)

- **Routes (ui/src/config/routes.ts):** `/dashboard` → `DashboardPage`; `/training` and subroutes `/training/datasets`, `/training/datasets/:datasetId`, `/training/jobs`, `/training/jobs/:jobId`; `/adapters`, `/adapters/:adapterId`; `/admin/stacks`; `/chat`.
- **DashboardPage (ui/src/pages/DashboardPage.tsx):** wraps `FeatureLayout` + `Dashboard` component with auth/tenant from providers; current `Dashboard` shows system metrics, nodes/tenants counts, activity feed, plugin/base model status, quick actions, modals.
- **Training (ui/src/pages/TrainingPage.tsx + ui/src/components/TrainingPage.tsx):** canonical training surface with polling for jobs, `TrainingWizard` launch, `TrainingJobMonitor`; datasets live in `ui/src/pages/Training/DatasetsTab.tsx` with upload/validate/delete + `useTraining.useDatasets` hook.
- **Training detail:** `ui/src/pages/Training/TrainingJobsPage.tsx`, `ui/src/pages/Training/TrainingJobDetail.tsx`, `ui/src/pages/Training/DatasetDetailPage.tsx` for full flows; `TrainingWizard` is the existing start-training entry.
- **Chat:** `/chat` renders `ChatPage` using `ChatInterface`; supports `initialStackId` via `stack` query param; uses default stack from `useGetDefaultStack`.
- **Adapters & stacks:** `/adapters` (`AdaptersPage`) and `/admin/stacks` (`AdapterStacksTab`) for management; stacks provided by `useAdapterStacks` and default stack helpers in `useAdmin`.
- **Shared UI/layout:** `FeatureLayout`, `RootLayout`, `FeatureProviders`/`useTenant`, `useAuth` from `CoreProviders`; cards/buttons/badges live under `ui/src/components/ui/**`.
- **API client (ui/src/api/client.ts):** available calls include `listDatasets`, `getDataset`, `listTrainingJobs`, `getTrainingJob`, `listAdapters`, `getAdapter`, `listAdapterStacks`, `getAdapterStack`, `startTraining`, `getDefaultAdapterStack`, `setDefaultAdapterStack`.
