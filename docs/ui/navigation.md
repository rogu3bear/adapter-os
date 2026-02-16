# AdapterOS UI Navigation Architecture

## Canonical IA

The UI is a workflow-first control plane. The canonical primary taxonomy is:

`Infer · Data · Train · Deploy · Route · Observe · Govern · Org`

Naming canon:
- Use `Runs` for user-facing labels (not `Flight Recorder`).
- Legacy `flight-recorder` paths stay supported as redirects.

Route class canon (exactly one per route):
- `Primary`
- `Tools`
- `Hidden`
- `Experimental`

Maturity tags used in docs:
- `Stable`
- `Experimental`
- `Incomplete`

## Primary Module Structure

#### 1) Infer
- **Chat** (`/chat`) and session deep links (`/chat/:session_id`)

#### 2) Data
- **Documents** (`/documents`)
- **Collections** (`/collections`)
- **Datasets** (`/datasets`)
- **Repositories** (`/repositories`)

#### 3) Train
- **Training Jobs** (`/training`)

#### 4) Deploy
- **Adapters** (`/adapters`)
- **Stacks** (`/stacks`)
- **Models** (`/models`)

#### 5) Route
- **Routing** (`/routing`)

#### 6) Observe
- **Dashboard** (`/`)
- **Runs** (`/runs`)
- **Monitoring** (`/monitoring`)
- **Errors** (`/errors`)
- **Diff** (`/diff`, launcher/tooling page)
- **Workers** (`/workers`)

#### 7) Govern
- **Policies** (`/policies`)
- **Audit** (`/audit`)
- **Reviews** (`/reviews`)

#### 8) Org
- **Agents** (`/agents`) - `Experimental + Incomplete`
- **Files** (`/files`)
- **Admin** (`/admin`)
- **Settings** (`/settings`)
- **System** (`/system`)

## Tools, Hidden, Experimental Surfaces

### Tools
- `/diff` (standalone run-diff launcher; conditionally redirects with run IDs)
- `/style-audit` (style-system audit page)

### Hidden
- `/login`
- `/safe`
- `/welcome`
- `/dashboard` -> `/`
- `/training/:id` -> `/training?job_id=:id`
- `/user` -> `/settings`
- `/flight-recorder` -> `/runs`
- `/flight-recorder/:id` -> `/runs/:id` (query preserved)

### Experimental
- `/agents` - session creation is intentionally disabled in UI (`Incomplete`)

## StartMenu Layout

```
┌─────────────────────────────────────────┐
│           adapterOS                     │
├─────────────────────────────────────────┤
│  ▸ Infer                                │
│      Chat                               │
│                                         │
│  ▸ Data                                 │
│      Documents                          │
│      Collections                        │
│      Datasets                           │
│      Repositories                       │
│                                         │
│  ▸ Train                                │
│      Training Jobs                      │
│                                         │
│  ▸ Deploy                               │
│      Adapters                           │
│      Stacks                             │
│      Models                             │
│                                         │
│  ▸ Route                                │
│      Routing                            │
│                                         │
│  ▸ Observe                              │
│      Dashboard                          │
│      Runs                               │
│      Monitoring                         │
│      Errors                             │
│      Diff                               │
│      Workers                            │
│                                         │
│  ▸ Govern                               │
│      Policies                           │
│      Audit                              │
│      Reviews                            │
│                                         │
│  ▸ Org                                  │
│      Agents                             │
│      Files                              │
│      Admin                              │
│      Settings                           │
│      System                             │
├─────────────────────────────────────────┤
│  v0.13.1                    ⚙ Settings  │
└─────────────────────────────────────────┘
```

## Taskbar

The taskbar shows **module-level** shortcuts only (no random pages):

```
[≡]  Infer  Data  Train  Deploy  Route  Observe  Govern  Org  [tray]
```

## Runs Detail Hub

Canonical provenance view at `/runs/:id`:

```
Overview │ Trace │ Receipt │ Routing │ Tokens │ Diff │ Events
```

- **Overview** – Summary, status, timing
- **Trace** – Full trace timeline
- **Receipt** – Hash/receipt verification
- **Routing** – K‑sparse routing decisions
- **Tokens** – Token accounting
- **Diff** – Compare with another run
- **Events** – Raw diagnostic events (internal)

## Deep Links

- `/runs/:id` – Runs detail Overview
- `/runs/:id?tab=trace` – Trace tab
- `/runs/:id?tab=receipt` – Receipt tab
- `/runs/:id?tab=routing` – Routing tab
- `/runs/:id?tab=tokens` – Tokens tab
- `/runs/:id?tab=diff&compare=:other_trace_id` – Diff tab

## Legacy Redirects

- `/dashboard` -> `/`
- `/flight-recorder` -> `/runs`
- `/flight-recorder/:id` -> `/runs/:id`
- `/training/:id` -> `/training?job_id=:id`
- `/user` -> `/settings`

## Command Palette

Power users can open Cmd+K / Ctrl+K and search routes/actions, including Tools, Hidden, and deep-link targets.
