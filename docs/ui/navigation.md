# AdapterOS UI Navigation Architecture

## Design Philosophy

The UI is a **control plane for ML inference**, not a desktop operating system.

The user mental model:
1. **Run** inference
2. **Prove** what happened (provenance, receipts, audit)
3. **Configure** behavior (adapters, stacks, policies)
4. **Data** management (datasets, documents, repos)
5. **Operate** the system (health, workers, metrics)
6. **Govern** usage (admin, reviews, settings)

## Module Structure

### Primary Navigation (6 Modules)

```
┌─────────────────────────────────────────────────────────────┐
│  Run       Prove      Configure    Data    Operate   Govern │
└─────────────────────────────────────────────────────────────┘
```

Each module is a coherent product area:

#### 1. Run
Execute inference and manage sessions.
- **Chat** - Interactive inference with streaming
- **Runs** - Run history and diagnostics

#### 2. Prove
Verify provenance and compliance.
- **Audit** - Immutable audit log with hash chain
- **Run Detail** - Trace, receipt, routing for any run

#### 3. Configure
Set up inference behavior.
- **Adapters** - LoRA adapter management
- **Stacks** - Runtime stack composition
- **Policies** - Policy pack enforcement
- **Models** - Base model registry

#### 4. Data
Manage training and retrieval data.
- **Datasets** - Training datasets
- **Documents** - RAG document store
- **Repositories** - Code repository scanning
- **Collections** - Document collections

#### 5. Operate
System health and operations.
- **Dashboard** - Live system summary
- **Infrastructure** - Topology and services
- **Workers** - Worker pool management
- **Metrics** - Monitoring charts
- **Incidents** - Error tracking
- **Training** - Training jobs
- **Agents** - Agent management

#### 6. Govern
Administration and compliance.
- **Admin** - Users, roles, API keys
- **Human Review** - Review queue for paused inference
- **Settings** - User preferences

### Tools (Collapsed)

Debug and experimental pages, hidden by default:
- **Routing Debug** - K-sparse routing testing
- **Diff Viewer** - Run comparison tool
- **Style Audit** - Design system audit

## StartMenu Layout

```
┌─────────────────────────────────────────┐
│           AdapterOS                     │
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
│      Stacks                             │
│      Policies                           │
│      Models                             │
│                                         │
│  ▸ Data                                 │
│      Datasets                           │
│      Documents                          │
│      Repositories                       │
│      Collections                        │
│                                         │
│  ▸ Operate                              │
│      Dashboard                          │
│      Infrastructure                     │
│      Workers                            │
│      Metrics                            │
│      Incidents                          │
│      Training                           │
│      Agents                             │
│                                         │
│  ▸ Govern                               │
│      Admin                              │
│      Human Review                       │
│      Settings                           │
│                                         │
│  ▾ Tools                                │
│      Routing Debug                      │
│      Diff Viewer                        │
│      Style Audit                        │
├─────────────────────────────────────────┤
│  v0.13.1                    ⚙ Settings  │
└─────────────────────────────────────────┘
```

## Taskbar

The taskbar shows module-level shortcuts (not individual pages):

```
┌──────────────────────────────────────────────────────────────────┐
│ [≡] │ Run │ Prove │ Configure │ Data │ Operate │ Govern │ [tray] │
└──────────────────────────────────────────────────────────────────┘
```

Clicking a module:
- If one primary page: navigates directly
- If multiple pages: shows a mini-menu

## Run Detail Hub

The canonical provenance view for any run at `/runs/:id`:

```
┌─────────────────────────────────────────────────────────────────┐
│  Run: abc-123-def                                    [Actions ▾]│
├─────────────────────────────────────────────────────────────────┤
│  Overview │ Trace │ Receipt │ Routing │ Tokens │ Diff          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [Tab content based on selection]                               │
│                                                                 │
│  - Overview: Summary card, timing, status, adapters used        │
│  - Trace: Full TraceViewer with timeline visualization          │
│  - Receipt: ReceiptVerification with hash, signature, hardware  │
│  - Routing: TokenDecisions with K-sparse breakdown              │
│  - Tokens: Token accounting, cache hits, billing                │
│  - Diff: Compare with another run (optional)                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Deep Links

All routes support deep linking via URL:
- `/runs/:id` - Run detail, Overview tab
- `/runs/:id?tab=trace` - Trace tab
- `/runs/:id?tab=receipt` - Receipt tab
- `/runs/:id?tab=routing` - Routing tab
- `/runs/:id?tab=tokens` - Tokens tab
- `/runs/:id?tab=diff&compare=:other_id` - Diff tab comparing runs

## Command Palette

Power users access all pages via Cmd+K:
- Searches across all routes and actions
- Supports fuzzy matching
- Shows keyboard shortcuts
- Preserves access to hidden/Tools pages

## Mobile/Responsive

- **Desktop (≥1024px)**: Full navigation with all modules
- **Tablet (768-1023px)**: Collapsed modules, hamburger menu
- **Mobile (<768px)**: Bottom nav with 4 key modules + menu
