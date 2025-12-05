# Tab shell status

## Adapters (`/adapters`)
- Tabs: Overview, Activations, Usage, Lineage, Manifest, Register, Policies.
- Detail routes mapped into shell: `/adapters/:adapterId` → Overview; `.../activations` → Activations; `.../usage` → Usage; `.../lineage` → Lineage; `.../manifest` → Manifest; `/adapters/new` → Register; hash `#policies` → Policies.
- Overview tab shows adapter list when no `adapterId`; detail tabs show adapter-specific data when present.

## Training (`/training`)
- Tabs: Overview, Jobs, Datasets, Templates, Artifacts, Settings.
- Detail routes mapped into shell: `/training/jobs` and `/training/jobs/:jobId` → Jobs tab (detail renders when `jobId` present); `/training/datasets` and `/training/datasets/:datasetId` → Datasets tab (detail renders when `datasetId` present); `/training/templates` → Templates tab; hashes `#artifacts`, `#settings` select placeholder tabs.

## Telemetry (`/telemetry`)
- Tabs: Event Stream, Viewer, Alerts, Exports, Filters.
- Detail routes mapped into shell: `/telemetry` → Event Stream; `/telemetry/viewer` → Viewer; hashes `#alerts`, `#exports`, `#filters` pick remaining tabs.

## Replay (`/replay`)
- Tabs: Runs, Decision Trace, Evidence, Compare, Export.
- Detail routes mapped into shell: `/replay` → Runs; hashes `#decision-trace`, `#evidence`, `#compare`, `#export` pick other tabs.

MLNavigator Inc 2025-12-05.

