# AdapterOS UI Navigation Architecture

## Design Philosophy

The UI is a **control plane for ML inference**, not a desktop operating system.

Mental model:
1. **Run** inference
2. **Prove** what happened (audit/provenance)
3. **Configure** behavior (adapters, stacks, policies, models)
4. **Data** management (datasets, documents)
5. **Operate** the system (health ladder)
6. **Govern** usage (admin, reviews, settings)

## Module Structure

### Primary Navigation (6 Modules)

```
Run · Prove · Configure · Data · Operate · Govern
```

Each module is a coherent product area:

#### 1) Run
- **Chat** – Interactive inference
- **Runs** – Run history + canonical Run Detail hub

#### 2) Prove
- **Audit** – Immutable audit trail

#### 3) Configure
- **Adapters** – LoRA adapter management
- **Runtime Stacks** – Adapter composition
- **Policies** – Policy packs
- **Models** – Base model registry

#### 4) Data
- **Datasets** – Training datasets
- **Documents** – RAG document store

#### 5) Operate
Health ladder (no duplicate dashboards):
- **Dashboard** – Summary + alerts
- **Infrastructure** – Topology/services
- **Workers** – Worker control
- **Metrics** – Monitoring charts
- **Incidents** – Error feed

#### 6) Govern
- **Admin** – Users, roles, API keys
- **Human Review** – Review queue
- **Settings** – Preferences & system info

### Tools (Collapsed)

Debug/experimental utilities, hidden by default:
- **Routing Debug** – K‑sparse routing inspection
- **Run Diff** – Compare runs (launcher)
- **Style Audit** – CSS/system audit

### Hidden / Not In Primary Nav

Accessible via command palette or direct link:
- **Training** (`/training`)
- **Repositories** (`/repositories`) and **Collections** (`/collections`)
- **Agents** (`/agents`, experimental)
- **Safety Mode** (`/safe`, public fallback)
- **Login** (`/login`)

## StartMenu Layout

```
┌─────────────────────────────────────────┐
│           adapterOS                     │
├─────────────────────────────────────────┤
│  ▸ Run                                  │
│      Chat                               │
│      Runs                               │
│                                         │
│  ▸ Prove                                │
│      Audit                              │
│                                         │
│  ▸ Configure                            │
│      Adapters                           │
│      Runtime Stacks                     │
│      Policies                           │
│      Models                             │
│                                         │
│  ▸ Data                                 │
│      Datasets                           │
│      Documents                          │
│                                         │
│  ▸ Operate                              │
│      Dashboard                          │
│      Infrastructure                     │
│      Workers                            │
│      Metrics                            │
│      Incidents                           │
│                                         │
│  ▸ Govern                               │
│      Admin                              │
│      Human Review                       │
│      Settings                           │
│                                         │
│  ▾ Tools                                │
│      Routing Debug                      │
│      Run Diff                           │
│      Style Audit                        │
├─────────────────────────────────────────┤
│  v0.13.1                    ⚙ Settings  │
└─────────────────────────────────────────┘
```

## Taskbar

The taskbar shows **module-level** shortcuts only (no random pages):

```
[≡]  Run  Prove  Configure  Data  Operate  Govern  [tray]
```

## Run Detail Hub

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

- `/runs/:id` – Run Detail Overview
- `/runs/:id?tab=trace` – Trace tab
- `/runs/:id?tab=receipt` – Receipt tab
- `/runs/:id?tab=routing` – Routing tab
- `/runs/:id?tab=tokens` – Tokens tab
- `/runs/:id?tab=diff&compare=:other_trace_id` – Diff tab

## Command Palette

Power users access all pages via Cmd+K:
- Fuzzy search across routes and actions
- Includes Tools/Hidden pages
- Preserves deep links
