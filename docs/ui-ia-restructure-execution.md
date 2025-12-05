# UI IA Restructure Execution

Use this as a page-by-page checklist to impose the Build/Run/Observe/Verify structure and related redesign steps.

## Page-to-cluster mapping (breadcrumb/header/nav label)
- **Label rule**: Prefix breadcrumbs and page headers with the cluster name (`Build`, `Run`, `Observe`, `Verify`). Sidebar/nav group should be the same cluster name; no other nav groups remain.
- **Build**
  - `/workflow` (onboarding root), `/flow/lora` (guided), `/personas` (tour), `/management` (ops console for setup)
  - `/create-adapter`, `/adapters` (+ `/:adapterId`, `/activations`, `/usage`, `/lineage`, `/manifest`, `/new`)
  - `/training` (+ `/jobs`, `/jobs/:jobId`, `/datasets`, `/datasets/:datasetId`, `/templates`), `/trainer`
  - `/promotion`, `/base-models`, `/router-config`, `/admin` (+ `/tenants`, `/tenants/:tenantId`, `/stacks`, `/plugins`, `/settings`)
- **Run**
  - `/dashboard` (entry for operators), `/inference`, `/chat`, `/documents` (+ `/:documentId/chat`), `/code-intelligence`
- **Observe**
  - `/monitoring`, `/metrics`, `/metrics/advanced`, `/routing`
  - `/system` (+ `/nodes`, `/workers`, `/memory`, `/metrics`)
  - `/telemetry` (+ `/telemetry/viewer`), `/reports`, `/help`
- **Verify**
  - `/testing`, `/golden`, `/replay`
  - `/security/policies`, `/security/audit`, `/security/compliance`
  - `/owner` (keep reachable but mark as Verify/legacy landing), `/_dev/routes`, `/dev/errors` (dev-only under Verify)
- **Dev route parity**: `/_dev/routes` should mirror IA and serves as the canonical debug view for verifying route coverage.

## Step 2: Collapse multi-page detail clusters into tabbed single pages
- **Adapters (`/adapters`)**: Tabs = Overview (registry table + summary stats), Activations, Usage, Lineage, Manifest, Register (inline form), Policies (if applicable). Detail routes stay routable but render the tabbed shell with hash-based selection.
- **Training (`/training`)**: Tabs = Overview (pipeline status + recent jobs), Jobs, Datasets, Templates, Artifacts (logs/checkpoints link-outs), Settings (training defaults).
- **Telemetry (`/telemetry`)**: Tabs = Event Stream (live/paginated), Viewer (schema-aware drilldown), Alerts (routing anomalies/egress blocks), Exports (download bundles), Filters (saved views).
- **Replay (`/replay`)**: Tabs = Runs (searchable grid), Decision Trace (router decisions + seeds), Evidence (RAG/context hashes), Compare (A/B of runs), Export (bundle download).

## Step 3: Onboarding via `/workflow`
- Make `/workflow` the onboarding root with a fixed 3-step checklist:
  1) Connect base model + register first adapter (links to `/base-models` and `/create-adapter`).
  2) Run a sample inference and chat sanity probe (links to `/inference` and `/chat`).
  3) Verify evidence: check telemetry + replay a run (links to `/telemetry` and `/replay`).
- Show completion state per step (done/in-progress/blocked) and surface follow-ups (e.g., promotion or policy hardening).

## Step 4: Dashboard rewrite (only start page for operators)
- Primary cards: Health at a glance (workers/nodes, GPU/ANE/memory), Last routing anomalies (recent router decisions with seeds and gate values), Adapters currently in play (active stacks + adapter IDs), One-run sanity probe (CTA to `/inference` with last used params).
- Secondary: Policy posture (guardrails on/off), Egress posture (mode + PF status), Training queue (recent jobs), RAG freshness (latest document ingest time).
- Navigation: dashboard is the default landing after auth; top CTA buttons go to `/workflow` (onboarding) and `/inference` (probe).

## Step 5: Role-based sidebar pruning (menu matrix)
- **Admin**: Build (all), Run (dashboard, inference, chat, documents), Observe (all), Verify (all).
- **Operator**: Build (adapters, training, router-config, promotion), Run (dashboard, inference, chat, documents), Observe (monitoring, metrics, system, telemetry, routing), Verify (testing, golden, replay).
- **SRE**: Build (router-config only), Run (dashboard, inference), Observe (monitoring, metrics, system, telemetry, routing, reports), Verify (testing, replay).
- **Compliance/Auditor**: Build (none), Run (dashboard), Observe (telemetry, reports), Verify (policies, audit, compliance, replay, golden).
- **Viewer**: Run (dashboard, inference read-only), Observe (monitoring snapshot, metrics read-only), Verify (none).

MLNavigator Inc 2025-12-05.

