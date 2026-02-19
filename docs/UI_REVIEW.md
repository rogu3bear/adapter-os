# AdapterOS UI Review

**Generated:** 2025-02-18  
**Scope:** Leptos CSR app in `crates/adapteros-ui/`  
**Canonical source:** `crates/adapteros-ui/src/lib.rs` for routes and shell boundaries.  
**References:** [UI_LAYOUT_MATRIX.md](UI_LAYOUT_MATRIX.md), [UI_ACTION_RULES.md](UI_ACTION_RULES.md), [ui/route-map.md](ui/route-map.md)

---

## 1. Route Map

### 1.1 Public Routes (No Auth, No Shell)

| Route | Page | Description |
|-------|------|-------------|
| `/login` | `Login` | Credential form; redirects to `returnUrl` or default page on success |
| `/safe` | `Safe` | Minimal fallback UI (no API calls); "Go to Login" / "Try Main App" |
| `/style-audit` | `StyleAudit` | Dev tool for design system audit |

### 1.2 Legacy Redirects

| Route | Redirects To |
|-------|--------------|
| `/flight-recorder` | `/runs` |
| `/flight-recorder/:id` | `/runs/:id` (query params preserved) |

### 1.3 Protected Routes (Auth + Shell)

All routes below are wrapped by `ProtectedRoute` → `Shell` (TopBar, Sidebar, Taskbar, Outlet).

| Route | Page | Description |
|-------|------|-------------|
| `/` | `Dashboard` | Home; status cards, guided flow, journey steps |
| `/dashboard` | `Dashboard` | Same as `/` |
| `/adapters` | `Adapters` | Adapter list + split-panel detail |
| `/adapters/:id` | `AdapterDetail` | Full adapter detail page |
| `/update-center` | `UpdateCenter` | Promote/draft/review lifecycle |
| `/chat` | `Chat` | Chat landing; redirects to recent session or empty state |
| `/chat/:session_id` | `ChatSession` | Full chat workspace with session |
| `/system` | `System` | Kernel status, boot, services |
| `/settings` | `Settings` | Tabbed: Profile, Preferences, API, Security, System |
| `/user` | `User` | User profile (redirects to settings) |
| `/models` | `Models` | Base model list |
| `/models/:id` | `ModelDetail` | Model detail |
| `/policies` | `Policies` | Policy management |
| `/training` | `Training` | Training jobs list + detail |
| `/training/:id` | `TrainingDetailRoute` | Training job detail |
| `/stacks` | `Stacks` | Adapter stack list |
| `/stacks/:id` | `StackDetail` | Stack detail |
| `/collections` | `Collections` | Collection list |
| `/collections/:id` | `CollectionDetail` | Collection detail |
| `/documents` | `Documents` | Document list |
| `/documents/:id` | `DocumentDetail` | Document detail |
| `/datasets` | `Datasets` | Dataset list |
| `/datasets/:id` | `DatasetDetail` | Dataset detail |
| `/admin` | `Admin` | Tabbed: Users, Roles, API Keys, Org |
| `/audit` | `Audit` | Event viewer |
| `/runs` | `FlightRecorder` | Restore points / traces |
| `/runs/:id` | `FlightRecorderDetail` | Run detail |
| `/diff` | `Diff` | Diff viewer |
| `/workers` | `Workers` | Worker list |
| `/workers/:id` | `WorkerDetail` | Worker detail |
| `/monitoring` | `Monitoring` | Activity monitor |
| `/errors` | `Errors` | Recovery console |
| `/routing` | `Routing` | Routing rules/weights/decisions |
| `/repositories` | `Repositories` | Repo list |
| `/repositories/:id` | `RepositoryDetail` | Repo detail |
| `/reviews` | `Reviews` | Safety queue |
| `/reviews/:pause_id` | `ReviewDetail` | Review detail |
| `/welcome` | `Welcome` | Welcome/onboarding |
| `/agents` | `Agents` | Automation agents |
| `/files` | `FileBrowser` | Filesystem browser |

### 1.4 Fallback

- **404:** `NotFound` — shows path, suggestions for common paths, "Go to Dashboard"

---

## 2. Pages and Components per Page

### `/` (Dashboard)

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Page header, actions |
| `PageScaffoldActions` | SseIndicator, "Teach New Skill", "Refresh", "View infrastructure" |
| `SkeletonStatsGrid` | Loading placeholder |
| `Card` | Kernel Status, Prompt Studio, System Services |
| `StatusIconBox`, `StatusIndicator` | Status display |
| `JourneyFlowSection` | Guided flow steps |
| `JourneyStep` | Step 1–4 cards with CTAs |
| `ButtonLink`, `Button` | CTAs |
| `InferenceGuidance` | Guidance when inference not ready |
| `use_status_center` | "Why?" link |
| `use_system_status`, `use_live_system_metrics` | Data hooks |

### `/login`

| Component | Purpose |
|-----------|---------|
| `OfflineBanner` | Backend status |
| `Card` | Login form container |
| `FormField`, `Input`, `Checkbox` | Form fields |
| `Button` | Submit |
| `use_auth` | Auth state + login action |
| `get_return_url()` | Redirect after login |

### `/adapters`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Page header, breadcrumbs |
| `PageScaffoldActions` | "Teach New Skill", "Refresh" |
| `AsyncBoundary` | Loading/error/loaded |
| `SplitPanel` | List + detail |
| `AdaptersListInteractive` | Table + pagination |
| `AdapterDetailPanel` | Detail drawer |
| `Table`, `TableHeader`, `TableBody`, `TableRow`, `TableCell` | List |
| `Badge`, `Button`, `ListEmptyCard` | UI elements |
| `use_cached_api_resource`, `use_refetch_signal` | Data |

### `/adapters/:id` (AdapterDetail)

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header, breadcrumbs |
| `PageScaffoldActions` | "Start Conversation", "Refresh" |
| `AsyncBoundaryWithErrorRender` | Error handling |
| `Card` | Basic Info, Status, Tech Stack, Metadata, Statistics |
| `CopyableId`, `Link`, `Badge` | UI |

### `/chat`

| Component | Purpose |
|-----------|---------|
| `ChatWorkspace` | Main chat UI |
| `ChatUnavailableEntry` | When inference not ready |
| `use_system_status`, `InferenceReadyState` | Redirect logic |

### `/chat/:session_id` (ChatSession)

| Component | Purpose |
|-----------|---------|
| `ChatWorkspace` | Session list + chat area |
| `AdapterHeat`, `AdapterMagnet`, `ChatAdaptersRegion` | Adapter selection |
| `SuggestedAdapterView` | Suggested adapters |
| `Markdown`, `MarkdownStream` | Message rendering |
| `TraceButton`, `TracePanel` | Trace UI |
| `ConfirmationDialog`, `Dialog`, `Input`, `Textarea` | Dialogs |
| `use_chat`, `ChatSessionsManager` | Chat state |

### `/training`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header, breadcrumbs |
| `PageScaffoldActions` | "Create Job" |
| `SplitPanel` | List + detail |
| `TrainingStatusFilter`, `CoremlFilters` | Filters |
| `TrainingJobList` | Job list |
| `CreateJobWizard` | Dialog |
| `BackendReadinessPanel` | Readiness |
| `use_query_map` | `open_wizard`, `source`, `dataset_id`, etc. |

### `/training/:id` (TrainingDetailRoute)

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TrainingJobDetail` | Job detail |
| `JobDetailContent`, `LogViewer`, `MetricsChart` | Detail |

### `/settings`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TabNav`, `TabPanel` | Tabs |
| `ProfileSection`, `PreferencesSection`, `ApiConfigSection` | Sections |
| `SecuritySection`, `SystemInfoSection` | Sections |
| `KernelSettingsSection` | Link to models |

### `/admin`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TabNav`, `TabPanel` | Tabs |
| `UsersSection`, `RolesSection`, `ApiKeysSection`, `OrgSection` | Sections |
| `use_query_map` | `?tab=users|roles|keys|org` |

### `/models`, `/models/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DataTable`, `AsyncBoundary` | List |
| `ModelDetail` | Detail page |

### `/stacks`, `/stacks/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `StacksList`, `StackRow` | List |
| `StackDetailContent` | Detail |
| `CreateStackDialog`, `EditStackDialog` | Dialogs |

### `/documents`, `/documents/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DataTable`, `AsyncBoundary` | List |
| `DocumentDetail` | Detail |

### `/datasets`, `/datasets/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DataTable`, `AsyncBoundary` | List |
| `DatasetDetail` | Detail page |

### `/collections`, `/collections/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DataTable`, `AsyncBoundary` | List |
| `CollectionDetail` | Detail |

### `/workers`, `/workers/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `WorkersSummary`, `WorkersList`, `WorkerRow` | List |
| `WorkerDetailPanel`, `WorkerDetailView` | Detail |
| `WorkerMetricsPanel`, `SpawnWorkerDialog` | Metrics, dialogs |

### `/runs` (FlightRecorder), `/runs/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DataTable`, `AsyncBoundary` | List |
| `FlightRecorderDetail` | Detail |

### `/routing`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TabNav`, `TabPanel` | Tabs |
| `RoutingRules`, `RoutingWeights`, `RoutingDecisions` | Sub-pages |

### `/repositories`, `/repositories/:id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `RepositoryList` | List |
| `RepositoryDetailPanel` | Detail |
| `RegisterRepositoryDialog` | Dialog |

### `/audit`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TabNav`, `TabPanel` | Tabs |
| `TimelineTab`, `HashChainTab`, `MerkleTreeTab`, `ComplianceTab`, `EmbeddingsTab` | Sub-tabs |

### `/reviews`, `/reviews/:pause_id`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `ReviewDetail` | Detail |

### `/system`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `SystemContent` | Main content |
| `BootStatusSection`, `StorageVisibilityPanel` | Sections |
| `ServiceControlPanel`, `AdminLifecyclePanel` | Panels |

### `/monitoring`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| Charts, metrics | |

### `/errors`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| Error list | |

### `/diff`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `DiffResults` | Diff |

### `/policies`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| Policy list | |

### `/update-center`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| Update center UI | |

### `/welcome`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `Welcome` | Onboarding |

### `/agents`

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `Agents` | Agent list |

### `/files` (FileBrowser)

| Component | Purpose |
|-----------|---------|
| `PageScaffold` | Header |
| `TreeView` | File tree |

### `/safe`

| Component | Purpose |
|-----------|---------|
| `Card` | Container |
| `Button` | "Go to Login", "Try Main App" |

### `/style-audit`

| Component | Purpose |
|-----------|---------|
| Design system audit | |

### `/user`

| Component | Purpose |
|-----------|---------|
| Redirect to `/settings` | |

### 404 (NotFound)

| Component | Purpose |
|-----------|---------|
| `NotFoundSurface` | Title, description, action |
| `suggestions_for_path()` | Path-based suggestions |

---

## 3. Navigation Logic

### 3.1 Auth Flow

1. **ProtectedRoute** wraps all Shell routes.
2. If `AuthState::Unauthenticated` → redirect to `/login?returnUrl={encoded_path}`.
3. If `AuthState::Authenticated` → render children (Shell).
4. If `AuthState::Error` → show error UI with retry.
5. If `AuthState::Timeout` → show timeout UI with retry.

### 3.2 Login Flow

1. User submits credentials.
2. On success → `window.location.href = returnUrl || default_page`.
3. `returnUrl` from `?returnUrl=` query param.
4. `default_page` from `UserSettings::load().default_page`.

### 3.3 Shell Layout

- **Shell** provides: `Sidebar`, `Workspace` (Outlet), `Taskbar`, `TopBar`, `LogicalControlRail`, `ChatDockPanel`, `MobileChatOverlay`, `TelemetryOverlay`.
- **Outlet** renders the matched child route.
- **ParentRoute** keeps Shell mounted across SPA navigation.

### 3.4 Sidebar Navigation

- **SidebarNav** uses `nav_registry::nav_groups(profile)`.
- **UiProfile** (Primary vs Full) controls which groups appear.
- **Workflow groups:** Infer → Data → Train → Deploy → Route → Observe → Govern → Org.
- **Active:** `location.pathname` matches `item.route`.
- **Alt+1..8:** `route_for_alt_shortcut(profile, digit)` → navigates to first route in group.

### 3.5 Taskbar Navigation

- **Taskbar** uses `build_taskbar_modules(profile)`.
- **ModuleButton** links to first route in group.
- **Active:** current path matches any route in module.
- **Desktop:** Start button toggles sidebar.
- **Mobile:** Start button opens StartMenu popup.

### 3.6 Command Palette (Ctrl+K, /)

- **CommandPalette** uses `use_search()`.
- **SearchIndex:** pages, adapters, models, workers, stacks, contextual actions.
- **On select:** `SearchAction::Navigate(path)` → `navigate(path)`; `SearchAction::Execute(command)` → `execute_command()`.
- **Recent items** recorded for quick access.
- **Contextual actions** based on `RouteContext` (e.g. "Open Chat with selected adapter").

### 3.7 Cross-Page Navigation

| From | To | Mechanism |
|------|------|-----------|
| Dashboard | Training | `/training?open_wizard=1` |
| Dashboard | Chat | `/chat` |
| Dashboard | Runs | `/runs` |
| Dashboard | Update Center | `/update-center` |
| Adapters | Chat | `/chat?adapter={id}` |
| Adapters | Training | `/training?open_wizard=1` |
| Adapter Detail | Chat | `chat_path_with_adapter(id)` |
| Adapter Detail | Training | `/training?adapter_name={name}` |
| Training | Adapter Detail | `return_to` query param |
| Documents | Training | `/training?source={doc_id}` |
| Datasets | Training | `/training?dataset_id={id}` |
| Settings | Models | `/models` |

### 3.8 Chat Redirect Logic

- **/chat** with no session:
  - If inference ready and recent session → redirect to `/chat/{session_id}` (preserves `?prompt=`, `?adapter=`).
  - Else → show empty state or `ChatUnavailableEntry`.

### 3.9 Query Params

- **Training:** `open_wizard`, `source`, `dataset_id`, `adapter_name`, `return_to`, `job_id`, etc.
- **Admin:** `?tab=users|roles|keys|org` (synced to URL).

### 3.10 Document Title

- **Shell** `Effect` sets `document.title` from route (e.g. "Home — AdapterOS", "Adapter Library — AdapterOS").

---

## 4. Shared Components Summary

| Component | Usage |
|-----------|-------|
| `PageScaffold` | All protected pages |
| `PageScaffoldActions` | Most pages |
| `AsyncBoundary` | List/detail pages |
| `SplitPanel` | Adapters, Training, Workers |
| `Card` | Content blocks |
| `Table`, `DataTable` | Lists |
| `TabNav`, `TabPanel` | Settings, Admin, Audit, Routing |
| `Button`, `ButtonLink` | CTAs |
| `Badge` | Status |
| `Dialog`, `ConfirmationDialog` | Modals |
| `Link` | Inline links |
| `BreadcrumbTrail` | Via PageScaffold breadcrumbs |

---

## 5. Hooks and Data Flow

| Hook | Purpose |
|------|---------|
| `use_auth` | Auth state + login action |
| `use_system_status` | System readiness |
| `use_api_resource`, `use_cached_api_resource` | API data |
| `use_refetch_signal` | Global refetch (e.g. Adapters) |
| `use_navigate`, `use_location`, `use_params_map`, `use_query_map` | Routing |
| `use_chat` | Chat state |
| `use_status_center` | Status center panel |
| `use_settings` | User settings |
| `use_ui_profile` | Primary vs Full nav |
| `use_sidebar` | Sidebar expanded/collapsed |
| `try_use_route_context` | Selected entity for Command Palette |

---

## 6. Route Hierarchy

```
App
├── AuthProvider
│   └── AppProviders
│       └── SearchProvider
│           └── InFlightProvider
│               └── RouteErrorBoundary
│                   └── Router
│                       ├── ToastContainer
│                       ├── Routes
│                       │   ├── /login (Login)
│                       │   ├── /safe (Safe)
│                       │   ├── /style-audit (StyleAudit)
│                       │   ├── /flight-recorder → Redirect
│                       │   ├── /flight-recorder/:id → Redirect
│                       │   └── ParentRoute (ProtectedRoute)
│                       │       └── Shell
│                       │           └── Outlet → [matched child route]
│                       └── CommandPalette
```

---

## 7. Notes

- **Phase 6 guidance:** see `docs/UI_LAYOUT_MATRIX.md` (layout selection) and `docs/UI_ACTION_RULES.md` (action hierarchy + wizard/dialog rule).
- **Path hygiene:** All runtime data under `./var/`; no `/tmp` usage.
- **Dev bypass:** `AOS_DEV_NO_AUTH=1` bypasses auth for UI iteration.
- **Blank UI:** Run `./scripts/build-ui.sh` or `./scripts/dev-up.sh`.
- **UI contract:** Frontend owns rendering, API calls, SSE; backend owns crypto, policy, receipts.
