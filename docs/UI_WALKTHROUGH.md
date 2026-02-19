# adapterOS UI Walkthrough

> **Agent note:** Code is authoritative. Routes and layout may have changed. Re-verify in `crates/adapteros-ui/src/lib.rs` before trusting. See [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md) and [DOCS_AUDIT_2026-02-18.md](DOCS_AUDIT_2026-02-18.md).

**Canonical source:** `crates/adapteros-ui/src/lib.rs`, `crates/adapteros-ui/UI_CONTRACT.md`  
**Last Updated:** 2026-02-18

A guided tour of the Leptos web UI as it exists today: entry points, layout, and navigation.

For the end-to-end canonical user journey (ingest -> dataset -> adapter -> chat routing -> receipts/replay), see [CANONICAL_USER_WORKFLOW.md](CANONICAL_USER_WORKFLOW.md).

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
  - Mobile: hamburger opens a menu with Infer, Data, Train, Deploy, Route, Observe, Govern, Org.

- **Main content**  
  - Single main area: `#main-content` (skip link target). The current route’s page is rendered here.

- **Right: Chat dock**  
  - Optional dock (Docked / Narrow / Hidden). Toggle via chat controls.

- **Bottom: Taskbar**  
  - **Start** button → opens **Start menu** (module-based nav).  
  - **Module buttons**: Infer, Data, Train, Deploy, Route, Observe, Govern, Org.  
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

## 3. Route map and canonical modules

Routes are grouped by the canonical IA taxonomy:
`Infer`, `Data`, `Train`, `Deploy`, `Route`, `Observe`, `Govern`, `Org`.

Class tags used below:
- `Primary`
- `Tools`
- `Hidden`
- `Experimental`

Maturity tags:
- `Stable`
- `Experimental`
- `Incomplete`

### Infer

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/chat` | Primary | Stable | Interactive inference UI. |
| `/chat/:session_id` | Primary | Stable | Session deep link. |

### Data

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/documents` | Primary | Stable | Document list/ingest. |
| `/documents/:id` | Primary | Stable | Document detail. |
| `/collections` | Primary | Stable | Collection list. |
| `/collections/:id` | Primary | Stable | Collection detail. |
| `/datasets` | Primary | Stable | Dataset list. |
| `/datasets/:id` | Primary | Stable | Dataset detail. |
| `/repositories` | Primary | Stable | Repository list. |
| `/repositories/:id` | Primary | Stable | Repository detail. |

### Train

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/training` | Primary | Stable | Training jobs and configuration. |
| `/training/:id` | Hidden | Stable | Redirect alias to `/training?job_id=:id`. |

### Deploy

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/adapters` | Primary | Stable | Adapter list. |
| `/adapters/:id` | Primary | Stable | Adapter detail. |
| `/stacks` | Primary | Stable | Runtime stack list. |
| `/stacks/:id` | Primary | Stable | Stack detail. |
| `/models` | Primary | Stable | Model list. |
| `/models/:id` | Primary | Stable | Model detail. |

### Route

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/routing` | Primary | Stable | Routing rules and decisions. |

### Observe

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/` | Primary | Stable | Dashboard landing page. |
| `/dashboard` | Hidden | Stable | Redirect alias to `/`. |
| `/runs` | Primary | Stable | Canonical runs list. |
| `/runs/:id` | Primary | Stable | Runs detail hub. |
| `/diff` | Tools | Stable | Standalone run diff; may redirect to `/runs/:id?tab=diff...` with query IDs. |
| `/workers` | Primary | Stable | Worker list. |
| `/workers/:id` | Primary | Stable | Worker detail. |
| `/monitoring` | Primary | Stable | Monitoring/alerts surface. |
| `/errors` | Primary | Stable | Error/incidents surface. |

### Govern

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/policies` | Primary | Stable | Policy management. |
| `/audit` | Primary | Stable | Audit trail/compliance surface. |
| `/reviews` | Primary | Stable | Human review queue. |
| `/reviews/:pause_id` | Primary | Stable | Review detail deep link. |
| `/safe` | Hidden | Stable | Public safe-mode fallback route. |

### Org

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/admin` | Primary | Stable | Tenant/org administration. |
| `/settings` | Primary | Stable | Preferences, profile, system info. |
| `/user` | Hidden | Stable | Backward-compat redirect alias to `/settings`. |
| `/system` | Primary | Stable | System topology and status. |
| `/welcome` | Hidden | Stable | First-run onboarding checklist. |
| `/files` | Primary | Stable | Filesystem browser. |
| `/agents` | Experimental | Experimental + Incomplete | Orchestration surface; session creation is intentionally disabled in UI. |
| `/login` | Hidden | Stable | Public auth entry route. |

### Tools and legacy aliases

| Route | Class | Maturity | Description |
|---|---|---|---|
| `/style-audit` | Tools | Stable | Style-system audit page (public dev tool). |
| `/flight-recorder` | Hidden | Stable | Legacy alias redirect to `/runs`. |
| `/flight-recorder/:id` | Hidden | Stable | Legacy alias redirect to `/runs/:id` (query preserved). |

---

## 4. Suggested walkthrough order

1. **Start**: `./start` or `AOS_DEV_NO_AUTH=1 ./start`, open app in browser.
2. **Login** (or skip if using dev no-auth): go to `/login`, sign in.
3. **Dashboard**: land on `/` — system status, metrics, workers, inference guidance.
4. **Navigation**: use **taskbar** (Infer, Data, Train, Deploy, Route, Observe, Govern, Org) and **Start** menu for full list of pages.
5. **Command palette**: ⌘K (or Ctrl+K) — search/navigate without using taskbar.
6. **Infer**: open **Chat** (`/chat`) and an existing session (`/chat/:session_id`).
7. **Data**: open **Datasets**, **Documents**, **Collections**, **Repositories** (list + detail where applicable).
8. **Train**: open **Training** (`/training`) and test deep-link alias (`/training/:id`).
9. **Deploy**: open **Adapters** (`/adapters`), **Stacks** (`/stacks`), **Models** (`/models`).
10. **Route**: open **Routing** (`/routing`).
11. **Observe**: open **Runs** (`/runs`), **Run Detail** (`/runs/:id`), **Diff** (`/diff`), **Workers** (`/workers`), **Monitoring** (`/monitoring`), **Errors** (`/errors`).
12. **Govern**: open **Policies** (`/policies`), **Audit** (`/audit`), **Reviews** (`/reviews` and `/reviews/:pause_id`), and **Safe** (`/safe`).
13. **Org**: open **Admin** (`/admin`), **System** (`/system`), **Files** (`/files`), **Settings** (`/settings`), **Welcome** (`/welcome`), and **Agents** (`/agents`, experimental/incomplete).
14. **Legacy aliases**: confirm `/dashboard`, `/flight-recorder`, `/flight-recorder/:id`, and `/user` redirect correctly.
15. **Fallback**: unknown path -> **NotFound** (router fallback).

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

### Observe (runtime health and diagnostics)

| Feature | Function | Use case |
|--------|----------|----------|
| **Dashboard** (`/`) | Single pane: system status, metrics (CPU/memory/GPU, RPS, latency), worker summary, inference readiness, guidance. | First stop after login; health at a glance; “is inference ready?” |
| **Workers** (`/workers`, `/workers/:id`) | List workers and per-worker detail (status, metrics, config) with lifecycle controls: spawn, drain, stop, restart, remove. Spawn supports a quick default path plus optional advanced socket-path override; drain rejects new requests while in-flight work completes; stop initiates drain + process stop signal; remove is guarded to terminal states (`stopped`, `error`, `crashed`, `failed`). | See who’s running; control lifecycle safely; add or retire capacity. |
| **Monitoring** (`/monitoring`) | Process monitoring: alerts, anomalies, health metrics (tabs). | Investigate alerts; spot anomalies; check health over time. |
| **Errors** (`/errors`) | Incidents: live error feed (SSE), history, analytics, alert rules, crash dumps. | Triage production errors; tune alerts; inspect crashes. |

#### Worker Spawn Modes (`/workers`)

- **Quick Spawn (default path):** click **Spawn Worker** in the page actions for one-click spawn using safe defaults: active node first (else first node), ready/active deployment config first (else first config), and auto-generated socket path.
- **Advanced mode:** click **Advanced Spawn** (or switch mode in the dialog) to manually set node, deployment config, and socket path.
- **Prerequisites:** Quick mode explains what is missing when defaults are unavailable, with next actions in-line (for example: register a node or create a deployment config). Advanced submit is disabled until node, deployment config, and socket path are valid.
- **Expected behavior:** successful submit closes the dialog, shows a "Worker spawned" success notification, and refreshes the workers list.
- **Status terminology:** canonical worker status semantics are defined in `docs/ui/terminology.md` (see **Worker Status Semantics**).

---

### Train and experimental Org surfaces

| Feature | Function | Use case |
|--------|----------|----------|
| **Training** (`/training`, `/training/:id`) | List training jobs (filters, CoreML state); create job wizard; job detail (logs, metrics, config). `/training/:id` redirects to `/training?job_id=:id`. | Run LoRA/adapter training; monitor jobs; debug failures. |
| **Agents** (`/agents`) | Agent orchestration: sessions, worker executors, orchestration config. `Experimental + Incomplete`: session creation is intentionally disabled in the UI. | Manage multi-agent sessions and orchestration rules using currently available controls. |

---

### Deploy and Govern (adapters, stacks, models, policies)

| Feature | Function | Use case |
|--------|----------|----------|
| **Adapters** (`/adapters`, `/adapters/:id`) | List registered adapters; split-panel detail (metadata, readiness). Refetches when training completes. | See what adapters exist; inspect one; confirm post-training registration. |
| **Stacks** (`/stacks`, `/stacks/:id`) | List runtime stacks (adapter compositions); create/edit stack; stack detail. Refetches when training completes. | Group adapters for inference; attach stacks to workflows. |
| **Policies** (`/policies`) | List policy packs; detail panel; create/validate policy. | Attach policy packs to stacks; audit or tune policy content. |
| **Models** (`/models`, `/models/:id`) | List base models and load status; model detail panel. | See which base models are loaded; plan capacity. |

---

### Data (datasets, documents, collections, repositories)

| Feature | Function | Use case |
|--------|----------|----------|
| **Datasets** (`/datasets`, `/datasets/:id`) | List training datasets; upload wizard; dataset detail and versions. | Prepare training data; inspect dataset metadata/versions. |
| **Documents** (`/documents`, `/documents/:id`) | List ingested documents (status, pagination); upload; document detail and chunks. | Manage RAG/ingestion; see indexing status; inspect chunks. |
| **Collections** (`/collections`, `/collections/:id`) | List document collections; create collection; add/remove documents; collection detail. | Group documents for retrieval; manage collection membership. |
| **Repositories** (`/repositories`, `/repositories/:id`) | List repos (sync status); register repo; detail: sync, scan, publish. | Register code/docs repos; run scans; manage publish workflow. |

---

### Govern and Observe (audit, runs, reviews)

| Feature | Function | Use case |
|--------|----------|----------|
| **Audit** (`/audit`) | Immutable audit log: timeline, hash chain, Merkle tree, compliance, embeddings tabs. | Prove integrity; verify chain; compliance evidence. |
| **Runs** (`/runs`, `/runs/:id`) | List diagnostic runs; run detail: overview, trace, receipt, routing, tokens, diff. | Inspect a request’s full provenance; compare two runs (determinism). |
| **Reviews** (`/reviews`, `/reviews/:pause_id`) | Human-in-the-loop queue and review-detail deep links for paused inferences. | Clear review queue; inspect a specific paused item; approve or reject. |

---

### Org (admin, system, files, onboarding)

| Feature | Function | Use case |
|--------|----------|----------|
| **Admin** (`/admin`) | Tabs: **Users** (list, roles); **Roles**; **API Keys** (create/revoke); **Organization**. Tenant-scoped. | Manage org users, roles, API keys, and org settings. |
| **System** (`/system`) | System overview: status, workers, nodes, health, metrics summary, recent events; SSE for live worker updates. | Deeper infra check; inspect node and worker status. |
| **Files** (`/files`) | Filesystem browser scoped to allowed roots. | Inspect server-side workspace files and directories. |
| **Welcome** (`/welcome`) | First-run setup checklist and readiness guidance. | Bootstrap a new environment before first inference run. |

---

### Infer, Route, and Observe tooling

| Feature | Function | Use case |
|--------|----------|----------|
| **Chat** (`/chat`, `/chat/:session_id`) | Chat UI with SSE streaming; sessions list; adapter bar; suggested adapters; trace panel. | Interactive inference; try prompts; see which adapters were used. |
| **Routing** (`/routing`) | Tabs: **Management** (routing rules), **Decisions** (inspect how requests are routed). | Debug K-sparse routing; add/tune rules; see decision history. |
| **Run Diff** (`/diff`) | Compare two diagnostic runs (anchor + first divergence). Remains a standalone page; when run IDs are provided in query params it redirects to `/runs/:id?tab=diff&compare=:id`. | Compare two runs for determinism; find first divergence. |

---

### Org account surfaces

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
| **Start menu** | Full module nav: Infer, Data, Train, Deploy, Route, Observe, Govern, Org. | Discover all pages; jump to a section. |
| **Taskbar** | Module shortcuts (Infer-Org) + Start + system tray. | Switch module without opening Start. |

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
