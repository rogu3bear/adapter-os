# adapterOS UI Walkthrough

A guided tour of the Leptos web UI as it exists today: entry points, layout, and navigation.

## Prerequisites

- Backend and UI served together: `./start` (or `AOS_DEV_NO_AUTH=1 ./start` for full access without login).
- Or dev UI only: `cd crates/adapteros-ui && trunk serve` (requires `AOS_API_BASE_URL` set if API is elsewhere).

If the API base URL cannot be determined, the app shows a fatal error screen with setup instructions instead of a blank page.

---

## 1. Entry and auth

- **`/login`** — Login page (username, password, remember me). Unprotected. After success, redirects to `returnUrl` query param or `/`.
- **`/`** — Root is the **Dashboard**, wrapped in `ProtectedRoute` + `Shell`. Unauthenticated users are redirected to `/login`.
- **`/safe`** — Safe-mode route: no auth, no API calls. Use for testing when backend is down.
- **`/style-audit`** — Style-audit dev tool; no auth.

---

## 2. Shell layout (every protected page)

Once logged in, every protected route is rendered inside the **Shell**:

- **Top bar (TopBar)**  
  - Left: Hamburger (mobile), “adapterOS” branding, DEV/PROD badge.  
  - Center/right: Command palette trigger (e.g. “⌘K”), **user menu** (profile, preferences, logout).  
  - Mobile: hamburger opens a menu with Operate, Build, Configure, Data, Verify, Org.

- **Main content**  
  - Single main area: `#main-content` (skip link target). The current route’s page is rendered here.

- **Right: Chat dock**  
  - Optional dock (Docked / Narrow / Hidden). Toggle via chat controls.

- **Bottom: Taskbar**  
  - **Start** button → opens **Start menu** (module-based nav).  
  - **Module buttons**: Operate, Build, Configure, Data, Verify, Org.  
  - System tray (e.g. status, notifications).

- **Overlays**  
  - **OfflineBanner** when API is unreachable.  
  - **Command palette** (⌘K / Ctrl+K, or “/” when not in an input).  
  - **Telemetry overlay** (e.g. Ctrl+Shift+T).  
  - **Mobile**: optional chat overlay.

**Keyboard**

- ⌘K / Ctrl+K (or “/” outside inputs): open command palette.  
- Escape in inputs: blur.  
- Skip link: “Skip to main content” for accessibility.

---

## 3. Route map and modules

Routes are grouped by the **taskbar modules** and **Start menu** structure.

### Operate (Dashboard, system, workers, monitoring, errors)

| Route         | Description              |
|---------------|--------------------------|
| `/`           | **Dashboard** — system status, metrics, workers, readiness, guidance. |
| `/dashboard`  | Redirects to `/`.        |
| `/system`     | Infrastructure / system overview. |
| `/workers`    | Worker list.             |
| `/workers/:id`| Worker detail.           |
| `/monitoring` | Metrics / monitoring.    |
| `/errors`     | Incidents / errors.      |

### Build (Training, agents)

| Route          | Description        |
|----------------|--------------------|
| `/training`    | Training runs and configuration. |
| `/training` (detail) | Training run detail (e.g. `/training/:id` if present). |
| `/agents`      | Agents.            |

### Configure (Adapters, stacks, policies, models)

| Route           | Description          |
|-----------------|----------------------|
| `/adapters`     | Adapter list.        |
| `/adapters/:id` | Adapter detail.     |
| `/stacks`       | Runtime stacks.     |
| `/stacks/:id`   | Stack detail.       |
| `/policies`     | Policies.           |
| `/models`       | Models.             |

### Data (Datasets, documents, collections, repositories)

| Route                | Description           |
|----------------------|-----------------------|
| `/datasets`          | Datasets.             |
| `/datasets/:id`      | Dataset detail.       |
| `/documents`         | Documents.            |
| `/documents/:id`     | Document detail.      |
| `/collections`       | Collections.          |
| `/collections/:id`   | Collection detail.    |
| `/repositories`      | Repositories.         |
| `/repositories/:id`  | Repository detail.    |

### Verify (Audit, runs, reviews)

| Route       | Description        |
|-------------|--------------------|
| `/audit`    | Audit.             |
| `/runs`     | Flight recorder / runs list. |
| `/runs/:id` | Run detail.        |
| `/reviews`  | Reviews.           |

Legacy paths:

- `/flight-recorder` → redirect to `/runs`.
- `/flight-recorder/:id` → redirect to `/runs/:id`.

### Org (Admin)

| Route    | Description                      |
|----------|----------------------------------|
| `/admin` | Users, roles, API keys, org. Tabs: main, roles, keys, org. |

### Tools (Start menu: Chat, Routing, Diff)

| Route       | Description          |
|-------------|----------------------|
| `/chat`     | Chat.                |
| `/chat/:session_id` | Chat session. |
| `/routing`  | Routing debug.      |
| `/diff`     | Run diff.            |

### Account (user menu in top bar)

| Route      | Description        |
|------------|--------------------|
| `/user`    | User profile.      |
| `/settings`| Preferences (e.g. API config, profile, system info). |

---

## 4. Suggested walkthrough order

1. **Start**: `./start` or `AOS_DEV_NO_AUTH=1 ./start`, open app in browser.
2. **Login** (or skip if using dev no-auth): go to `/login`, sign in.
3. **Dashboard**: land on `/` — system status, metrics, workers, inference guidance.
4. **Navigation**: use **taskbar** (Operate, Build, Configure, Data, Verify, Org) and **Start** menu for full list of pages.
5. **Command palette**: ⌘K (or Ctrl+K) — search/navigate without using taskbar.
6. **Operate**: open **System** (`/system`), **Workers** (`/workers`), **Monitoring** (`/monitoring`), **Errors** (`/errors`).
7. **Build**: **Training** (`/training`), **Agents** (`/agents`).
8. **Configure**: **Adapters** (`/adapters`), **Stacks** (`/stacks`), **Policies** (`/policies`), **Models** (`/models`).
9. **Data**: **Datasets**, **Documents**, **Collections**, **Repositories** (list + detail where applicable).
10. **Verify**: **Audit** (`/audit`), **Runs** (`/runs`), **Reviews** (`/reviews`).
11. **Org**: **Admin** (`/admin`) — users, roles, API keys, organization.
12. **Tools**: **Chat** (`/chat`), **Routing** (`/routing`), **Run Diff** (`/diff`).
13. **Account**: user menu (top right) → Profile (`/user`), Preferences (`/settings`).
14. **Fallback**: unknown path → **NotFound** (router fallback).

---

## 5. Technical notes

- **Stack**: Leptos 0.7, CSR, WASM target `wasm32-unknown-unknown`.
- **Styling**: Pure CSS in `dist/` (Liquid Glass design system; see `dist/glass.css`).
- **API**: Typed client and shared types from `adapteros-api-types` (e.g. `wasm` feature). Base URL must be set or inferred (e.g. same origin when served via `./start`).
- **Auth**: `AuthProvider`; protected routes redirect to `/login` when not authenticated. Dev bypass: `AOS_DEV_NO_AUTH=1` to skip auth for UI iteration.

This document reflects the UI as defined in `crates/adapteros-ui/src/lib.rs` (routes), `shell.rs`, `topbar.rs`, `taskbar.rs`, and `start_menu.rs`.

---

## 6. Function and use case by feature

For each area, **function** = what it does; **use case** = when you’d use it.

### Entry and auth

| Feature | Function | Use case |
|--------|----------|----------|
| **Login** (`/login`) | Authenticates with username/password; server sets httpOnly cookies; redirects to `returnUrl` or `/`. | Sign in when not using dev bypass; return after session expiry. |
| **Safe** (`/safe`) | Minimal UI with no auth and no API calls; links to Login and “Try Main App”. | Diagnose boot/load failures; stable fallback when the main app won’t load. |
| **Style audit** (`/style-audit`) | Component gallery: all UI components in variants, light/dark. | Visual regression testing; design review; baseline snapshots. |

---

### Operate (run and observe)

| Feature | Function | Use case |
|--------|----------|----------|
| **Dashboard** (`/`) | Single pane: system status, metrics (CPU/memory/GPU, RPS, latency), worker summary, inference readiness, guidance. | First stop after login; health at a glance; “is inference ready?” |
| **System** (`/system`) | System overview: status, workers, nodes, health, metrics summary, recent events; SSE for live worker updates. | Deeper infra check; see workers/nodes and live status. |
| **Workers** (`/workers`, `/workers/:id`) | List workers and per-worker detail (status, metrics, config); spawn worker dialog. | See who’s running; debug a worker; add capacity. |
| **Monitoring** (`/monitoring`) | Process monitoring: alerts, anomalies, health metrics (tabs). | Investigate alerts; spot anomalies; check health over time. |
| **Errors** (`/errors`) | Incidents: live error feed (SSE), history, analytics, alert rules, crash dumps. | Triage production errors; tune alerts; inspect crashes. |

---

### Build (training and agents)

| Feature | Function | Use case |
|--------|----------|----------|
| **Training** (`/training`, job detail) | List training jobs (filters, CoreML state); create job wizard; job detail (logs, metrics, config). | Run LoRA/adapter training; monitor jobs; debug failures. |
| **Agents** (`/agents`) | Agent orchestration: sessions, worker executors, orchestration config. | Manage multi-agent sessions and orchestration rules. |

---

### Configure (adapters, stacks, policies, models)

| Feature | Function | Use case |
|--------|----------|----------|
| **Adapters** (`/adapters`, `/adapters/:id`) | List registered adapters; split-panel detail (metadata, readiness). Refetches when training completes. | See what adapters exist; inspect one; confirm post-training registration. |
| **Stacks** (`/stacks`, `/stacks/:id`) | List runtime stacks (adapter compositions); create/edit stack; stack detail. Refetches when training completes. | Group adapters for inference; attach stacks to workflows. |
| **Policies** (`/policies`) | List policy packs; detail panel; create/validate policy. | Attach policy packs to stacks; audit or tune policy content. |
| **Models** (`/models`) | List base models and load status; model detail panel. | See which base models are loaded; plan capacity. |

---

### Data (datasets, documents, collections, repositories)

| Feature | Function | Use case |
|--------|----------|----------|
| **Datasets** (`/datasets`, `/datasets/:id`) | List training datasets; upload wizard; dataset detail and versions. | Prepare training data; inspect dataset metadata/versions. |
| **Documents** (`/documents`, `/documents/:id`) | List ingested documents (status, pagination); upload; document detail and chunks. | Manage RAG/ingestion; see indexing status; inspect chunks. |
| **Collections** (`/collections`, `/collections/:id`) | List document collections; create collection; add/remove documents; collection detail. | Group documents for retrieval; manage collection membership. |
| **Repositories** (`/repositories`, `/repositories/:id`) | List repos (sync status); register repo; detail: sync, scan, publish. | Register code/docs repos; run scans; manage publish workflow. |

---

### Verify (audit, runs, reviews)

| Feature | Function | Use case |
|--------|----------|----------|
| **Audit** (`/audit`) | Immutable audit log: timeline, hash chain, Merkle tree, compliance, embeddings tabs. | Prove integrity; verify chain; compliance evidence. |
| **Runs** (`/runs`, `/runs/:id`) | List diagnostic runs; run detail: overview, trace, receipt, routing, tokens, diff. | Inspect a request’s full provenance; compare two runs (determinism). |
| **Reviews** (`/reviews`) | Human-in-the-loop queue: paused inferences awaiting assessment; submit approve/reject. | Clear review queue; approve or reject paused items. |

---

### Org (admin)

| Feature | Function | Use case |
|--------|----------|----------|
| **Admin** (`/admin`) | Tabs: **Users** (list, roles); **Roles**; **API Keys** (create/revoke); **Organization**. Tenant-scoped. | Manage org users, roles, API keys, and org settings. |

---

### Tools (Chat, Routing, Diff)

| Feature | Function | Use case |
|--------|----------|----------|
| **Chat** (`/chat`, `/chat/:session_id`) | Chat UI with SSE streaming; sessions list; adapter bar; suggested adapters; trace panel. | Interactive inference; try prompts; see which adapters were used. |
| **Routing** (`/routing`) | Tabs: **Management** (routing rules), **Decisions** (inspect how requests are routed). | Debug K-sparse routing; add/tune rules; see decision history. |
| **Run Diff** (`/diff`) | Compare two diagnostic runs (anchor + first divergence). Redirects to `/runs/:id?tab=diff&compare=:id`. | Compare two runs for determinism; find first divergence. |

---

### Account (user menu)

| Feature | Function | Use case |
|--------|----------|----------|
| **User** (`/user`) | Redirects to `/settings`. | Old links/bookmarks; canonical account is Settings. |
| **Settings** (`/settings`) | Tabs: **Profile**, **API config**, **Preferences**, **System info**. Persists to localStorage. | Change display name, API base, UI prefs; see client/system info. |

---

### Shell and global UI

| Feature | Function | Use case |
|--------|----------|----------|
| **Command palette** (⌘K / “/”) | Search and navigate to routes without taskbar. | Fast navigation; keyboard-first. |
| **Chat dock** | Optional right panel: same chat as `/chat` but in-shell. | Chat while on another page. |
| **Offline banner** | Shown when API is unreachable; “Retry” and cached-data message. | Know when backend is down; retry when it’s back. |
| **Telemetry overlay** (Ctrl+Shift+T) | Toggle overlay for telemetry/debug. | Inspect client-side telemetry. |
| **Start menu** | Full module nav: Operate, Build, Configure, Data, Verify, Org, Tools, Account. | Discover all pages; jump to a section. |
| **Taskbar** | Module shortcuts (Operate–Org) + Start + system tray. | Switch module without opening Start. |

---

## Using the style audit to verify component changes

The **style audit** (`/style-audit`) is a component gallery that renders the same shared components used across the app. Use it to ensure changes propagate and stay consistent.

### Why it reflects the rest of the UI

- The audit page imports components from `crate::components::*` (e.g. `Button`, `Card`, `FormField`, `Table`). There is no duplicate implementation: **editing a component in `src/components/` updates both the audit and every page that uses it.**
- The audit shows **variants** (e.g. button sizes, status colors) and a **light/dark toggle**, so you can check one place for theme and variant coverage.

### Workflow when you change components

1. **Edit the component** in `crates/adapteros-ui/src/components/` (and any CSS in `dist/` if needed).
2. **Open `/style-audit`** (no login). If your component already has a section (Buttons, Cards, Form Inputs, Banners, etc.), check it there in both light and dark.
3. **If the component is not in the audit**, add a `<ComponentSection title="…">` (and optional `<SubSection>`) in `crates/adapteros-ui/src/pages/style_audit.rs` so future changes can be validated in one place.
4. **Keep design system rules**: use tokens and tiers from `dist/glass.css` (see header comment: Tier 1–3, borders, noise, motion). Stay within the allowlist in `STYLE_ALLOWLIST.md` for utility classes.

### Visual regression (optional)

- Capture screenshots of `/style-audit` before and after changes (e.g. with Playwright or manual) and compare.
- Use the audit as the **single baseline page** for visual regression so you don't have to snapshot every route.
