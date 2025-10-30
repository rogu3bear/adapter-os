# Merge Citations and Commit References

## Unified Features Merge: 2025-10-29-mcpb-d1G9z → main

### Primary Merge Commit
**Commit:** `5a0b385`  
**Branch:** `2025-10-29-mcpb-d1G9z`  
**Date:** 2025-10-29  
**Author:** rogu3bear

### Feature Summary
This merge unifies partial UI features into the stable main branch:

1. **Reporting Dashboard** (`ReportingSummaryWidget.tsx`)
   - Metrics display: inferences, training jobs, active adapters, system health
   - Role-based integration (Viewer, Admin dashboards)
   - Real-time updates via API polling

2. **Directory-Based Training** (`TrainingWizard.tsx`)
   - Replaced file upload with directory path selection
   - Uses existing `directory_root` + `directory_path` API endpoints
   - Codegraph analyzer integration for automatic training example generation

3. **Adapter Selection** (`InferencePlayground.tsx`)
   - Dropdown selector for trained adapters
   - Auto-selects active adapters when available
   - Passes adapter IDs via `adapters` array in inference requests

### Commit Chain References

```
5a0b385 feat(ui): add reporting dashboard and directory-based training
│
├─ UI Components Added:
│  ├─ ui/src/components/dashboard/ReportingSummaryWidget.tsx (131 lines)
│  ├─ ui/src/components/TrainingWizard.tsx (modified: +230/-95)
│  └─ ui/src/components/InferencePlayground.tsx (modified: +62/-0)
│
├─ Dashboard Integration:
│  └─ ui/src/components/Dashboard.tsx (modified: +54/-0)
│     ├─ Viewer role: ReportingSummaryWidget priority 1
│     └─ Admin role: ReportingSummaryWidget priority 5
│
└─ API Usage:
   ├─ apiClient.getSystemMetrics() → SystemMetrics (memory_usage_pct, active_sessions)
   ├─ apiClient.listAdapters() → Adapter[] (filtered by current_state)
   ├─ apiClient.listTrainingJobs() → TrainingJob[]
   └─ apiClient.startTraining() → StartTrainingRequest (directory_root, directory_path)
```

### Related Commits (Context Chain)

1. **5fa15b9** - `refactor(examples): remove PyO3 path from basic_inference`
   - Citation: Examples cleanup

2. **b101290** - `fix(ui): wire MultiModelStatusWidget to apiClient and correct imports`
   - Citation: Widget API integration pattern (followed for ReportingSummaryWidget)

3. **6b2bbc7** - `feat(server): add base-llm runtime manager, multi-model status, and load/unload integration`
   - Citation: Base infrastructure for adapter management

4. **c0ff4de** - `Final fixes for telemetry alerting`
   - Citation: System metrics infrastructure

5. **501f9f2** - `Complete incomplete features with deterministic implementations`
   - Citation: Feature completion methodology

6. **04874c6** - `Capture current dirty state as baseline for feature completion`
   - Citation: State baseline

7. **b1ff181** - `merge: unify MLX FFI backend, telemetry threat detection, and UI model selector (deterministic)`
   - Citation: Previous unification pattern

### File Change Citations

#### New Files
- `ui/src/components/dashboard/ReportingSummaryWidget.tsx` (131 lines)
  - Lines 1-131: Complete widget implementation
  - API calls: Lines 27-38 (metrics fetching)
  - UI rendering: Lines 66-130 (widget layout)

#### Modified Files
- `ui/src/components/Dashboard.tsx`
  - Line 28: Import ReportingSummaryWidget
  - Lines 140-147: Admin widget configuration
  - Lines 212-225: Viewer widget configuration
  - Line 279: Pass selectedTenant prop to widgets

- `ui/src/components/TrainingWizard.tsx`
  - Line 13: Import Folder icon
  - Line 40: Change dataSourceType to include 'directory'
  - Lines 45-46: Add directoryRoot, directoryPath state
  - Lines 242-357: DataSourceStep refactor (remove file upload, add directory inputs)
  - Lines 847-863: Directory training request handling
  - Lines 926-929: Directory validation

- `ui/src/components/InferencePlayground.tsx`
  - Lines 27, 31-33: Import adapter types and Select components
  - Lines 49-50: Add adapters, selectedAdapterId state
  - Lines 96-113: Load adapters on mount
  - Lines 150-154: Include adapters in inference request
  - Lines 410-434: Adapter selector UI

### API Endpoint Citations

#### Training API (`/api/v1/training/start`)
**Reference:** `crates/adapteros-server-api/src/handlers.rs:6738-6784`
- Supports `directory_root: Option<String>` (absolute path)
- Supports `directory_path: Option<String>` (relative path, defaults to ".")
- Uses codegraph analyzer for automatic dataset building

#### Adapters API (`/api/v1/adapters`)
**Reference:** `crates/adapteros-server-api/src/handlers.rs`
- `GET /api/v1/adapters` → `listAdapters()` returns `Adapter[]`
- Adapter states: `'unloaded' | 'cold' | 'warm' | 'hot' | 'resident'`

#### System Metrics API (`/api/v1/metrics`)
**Reference:** `crates/adapteros-server-api/src/handlers.rs`
- `GET /api/v1/metrics` → `getSystemMetrics()` returns `SystemMetrics`
- Fields: `memory_usage_pct`, `active_sessions`, `adapter_count`, `tokens_per_second`, `latency_p95_ms`

### Conflict Resolution
**Status:** No conflicts detected
- All changes are additive (new widget, new features)
- No overlapping modifications to shared files
- Compatible with existing API contracts

### Deterministic Verification
- All API calls use existing endpoints
- No new backend infrastructure created
- TypeScript types match Rust API types (`ui/src/api/types.ts`)
- Follows existing widget patterns (`AdapterStatusWidget`, `ActiveAlertsWidget`)

### Testing References
- New widget follows existing dashboard widget pattern
- Uses same API client patterns as other widgets
- Error handling matches existing widget implementations

---

**Merge Strategy:** Fast-forward merge (no conflicts)  
**Validation:** All changes use existing APIs/features  
**Unification Complete:** ✓

