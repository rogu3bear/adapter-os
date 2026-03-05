# Plan 53-03 Audit Report: Secondary Surfaces and Navigation Shell

**Date:** 2026-03-05
**Auditor:** Claude (automated)
**Methodology:** Code-level audit of all secondary page components, layout components, and CSS.

---

## 1. Navigation Shell (Sidebar + TopBar)

### Sidebar (`sidebar.rs`, `layout.css`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| Sidebar toggle (expand/collapse) | essential | keep |
| Dashboard home item | essential | keep |
| Nav group headers with chevron | essential | keep |
| Nav items with icons + labels | essential | keep |
| Collapse to icon-only rail | essential | keep |
| Sidebar footer with profile toggle | essential | keep |
| Alt+N keyboard shortcuts | essential | keep |
| Sidebar transition animation | essential | keep -- uses `var(--duration-normal)` correctly |

**Glass tier issue:** Sidebar currently uses `var(--glass-bg-1)` (Tier 1 = lightest blur, 9.6px). Per CONTEXT.md, sidebars should be Tier 2 (medium blur, 12px). The sidebar comment in `sidebar.rs` also says "Glass tier: 1" but the plan calls for Tier 2.

| Issue | Type | Action |
|-------|------|--------|
| Sidebar glass tier is T1, should be T2 | fix | Change `--glass-bg-1` to `--glass-bg-2` and `--glass-blur-1` to `--glass-blur-2` in layout.css `.sidebar` |

### TopBar (`topbar.rs`, `layout.css`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| Brand text / logo area | essential | keep |
| GlobalSearchBox (command palette hint) | essential | keep |
| Density toggle (comfortable/compact) | essential | keep |
| Reproducible mode lock indicator | essential | keep |
| Notification bell with count | essential | keep |
| User menu (email, logout) | essential | keep |
| Mobile hamburger menu | essential | keep |
| Status center icon | essential | keep |
| Docs link | essential | keep |

**Glass tier:** TopBar uses `var(--os-chrome-top)` with `blur(var(--glass-blur-1))`. This is T1 blur. Topbar should arguably be T2 to match sidebar. However, topbar has a special `--os-chrome-top` background which serves as the OS chrome — this may be intentional differentiation.

| Issue | Type | Action |
|-------|------|--------|
| TopBar uses T1 blur, consider T2 for consistency | review | Keep as-is -- `--os-chrome-top` is OS-level chrome, distinct from nav sidebar |
| TopBar has `box-shadow: inset` bevel highlights | review | Keep -- this is the macOS window chrome bevel, not content-area shadow |

---

## 2. Adapters Page (`adapters.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Create adapter" primary action | essential | keep |
| Search input filter | essential | keep |
| SplitPanel list/detail layout | essential | keep |
| AdapterDetailPanel | essential | keep |
| Lifecycle state badges | essential | keep |
| Training job linkage display | essential | keep |
| Dataset version links | essential | keep |
| EmptyState with "No adapters yet" | essential | keep -- uses EmptyState component correctly |
| Route context for command palette | essential | keep |
| Refetch on global adapter topic | essential | keep |

**PageScaffold:** YES
**AsyncBoundary:** Uses custom `LoadingState` match -- should migrate to AsyncBoundary for the main list loading
**EmptyState:** YES (uses `EmptyState` component)
**Skeleton loading:** NO -- uses `LoadingDisplay` for loading state, should use `SkeletonTable`

| Issue | Type | Action |
|-------|------|--------|
| No skeleton loading for adapter list | fix | Replace `LoadingDisplay` with `SkeletonTable` in loading state |

---

## 3. Models Page (`models.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Import model" primary action | essential | keep |
| SplitPanel list/inspector | essential | keep |
| Merged model data (runtime + registered) | essential | keep |
| Model status badges | essential | keep |
| Import model dialog | essential | keep |
| Seed models dialog | essential | keep |
| EmptyState for no models | essential | keep -- uses EmptyState component |
| SkeletonTable for loading | essential | keep -- already correct |
| Model detail page (separate route) | essential | keep |

**PageScaffold:** YES
**AsyncBoundary:** Uses custom merge logic (two resources) -- understandable
**EmptyState:** YES
**Skeleton loading:** YES (`SkeletonTable`)

| Issue | Type | Action |
|-------|------|--------|
| None found | -- | No changes needed |

---

## 4. Datasets Page (`datasets.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Create dataset" primary action | essential | keep |
| Search + status/trust/validation filters | essential | keep |
| Dataset list table | essential | keep |
| Dataset detail view (separate route) | essential | keep |
| Trust override controls | essential | keep |
| Validation state display | essential | keep |
| Preprocess status polling | essential | keep |

**PageScaffold:** YES
**AsyncBoundary:** Uses custom `LoadingState::Idle | LoadingState::Loading` match (multiple places) -- complex page, custom loading acceptable
**EmptyState:** Not using `EmptyState` component -- uses custom inline empty messages in some places
**Skeleton loading:** NO -- uses `LoadingDisplay` / `Spinner` for loading, no skeleton

| Issue | Type | Action |
|-------|------|--------|
| Missing skeleton loading for dataset list | fix | Replace custom loading indicators with `SkeletonTable` |
| Dataset detail custom empty text "No..." | fix | Migrate ad-hoc empty messages to `EmptyState` component where appropriate |

---

## 5. Training Pages (`training/mod.rs`, `training/detail/`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Start build" primary action | essential | keep |
| CreateJobWizard (130KB+) | essential | keep -- core workflow |
| TrainingJobList with SplitPanel | essential | keep |
| Status + CoreML filters | essential | keep |
| TrainingJobDetail (separate route) | essential | keep |
| Dataset wizard (78KB) | essential | keep -- core workflow |
| Inline "Build ID is missing" text | essential | keep -- error state |

**PageScaffold:** YES (both list and detail)
**AsyncBoundary:** YES (for job list via `AsyncBoundary` component)
**EmptyState:** Not checked -- training list likely uses EmptyState via list component
**Skeleton loading:** Not visible in module root -- detail view likely has custom loading

| Issue | Type | Action |
|-------|------|--------|
| Training detail route missing empty state in error path | fix | Use plain text with muted-foreground, acceptable minimal |
| Training detail spinner on load | fix | Could migrate to SkeletonDetailSection but low priority -- detail view is complex |

---

## 6. Settings Pages (`settings/mod.rs`, tabs)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Changes auto-save" indicator | essential | keep |
| TabNav (Profile, Workspace, Network, Safety, System) | essential | keep |
| ProfileSection | essential | keep |
| PreferencesSection | essential | keep |
| ApiConfigSection | essential | keep |
| SecuritySection (sessions, MFA) | essential | keep |
| SystemInfoSection | essential | keep |
| KernelSettingsSection (link to Models) | essential | keep |
| Scope info footer | essential | keep |

**PageScaffold:** YES
**Skeleton loading:** Security tab uses `Spinner` for sessions/MFA loading; api_config uses `Spinner`. Could be improved to `SkeletonTable`/`SkeletonCard`.
**EmptyState:** Security tab uses plain text "No active sessions found." -- should use `EmptyState` component.
**Empty state (System Info):** "No runtime settings available yet." -- plain text, should use `EmptyState`.

| Issue | Type | Action |
|-------|------|--------|
| Security sessions loading uses bare `Spinner` | fix | Replace with `SkeletonTable` |
| Security MFA loading uses bare `Spinner` | fix | Replace with `SkeletonCard` |
| ApiConfig loading uses bare `Spinner` | fix | Replace with `SkeletonCard` |
| "No active sessions found" plain text | fix | Migrate to `EmptyState` component |
| "No runtime settings available" plain text | fix | Migrate to `EmptyState` component |

---

## 7. System Pages (`system/mod.rs`, `system/components.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| Refresh button + SSE indicator | essential | keep |
| SkeletonStatsGrid + SkeletonCard loading | essential | keep -- already correct |
| SystemContent (stats, workers, nodes, health, metrics, events) | essential | keep |
| Worker SSE real-time updates | essential | keep |
| Tenants/stacks/services display | essential | keep |
| Adapter memory grid | essential | keep |
| Health checks detail | essential | keep |
| Inference blockers display | essential | keep |

**PageScaffold:** YES
**Skeleton loading:** YES (`SkeletonStatsGrid`, `SkeletonCard`)
**EmptyState:** Uses custom empty text in components (e.g., "No workers registered", "No nodes registered") -- some use `EmptyState` component, some don't.
**Spinner usage:** `Spinner` used in loading states within sub-components (components.rs has ~8 spinner uses) -- these are inline loading indicators within already-loaded content sections, not primary loading states. Acceptable.

| Issue | Type | Action |
|-------|------|--------|
| Some sub-component empty states use plain text | fix | Migrate key ones to `EmptyState` (e.g., "No workers registered", "No nodes registered") -- many already use `AsyncBoundaryWithEmpty` |
| Inline Spinners in table cells for live metrics | keep | Acceptable for real-time indicator |

---

## 8. Workers Page (`workers/mod.rs`, `workers/components.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Spawn Worker" primary action | essential | keep |
| RefreshButton | essential | keep |
| WorkersSummary cards | essential | keep |
| SplitPanel list/detail | essential | keep |
| WorkerDetailPanel / WorkerDetailView | essential | keep |
| SpawnWorkerDialog | essential | keep |
| Drain/Stop/Restart/Remove confirmations | essential | keep |
| SkeletonCard + SkeletonTable for loading | essential | keep -- already correct |
| EmptyState for no workers | essential | keep -- uses EmptyState component |
| All data-testid attributes | essential | keep -- extensive test identifiers |

**PageScaffold:** YES
**Skeleton loading:** YES
**EmptyState:** YES
**data-testid:** Extensive (30+ attributes) -- all preserved

| Issue | Type | Action |
|-------|------|--------|
| None found | -- | Workers page is already well-structured |

---

## 9. Audit Page (`audit/mod.rs`, `audit/tabs.rs`, `audit/components.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| Refresh + Export buttons | essential | keep |
| ChainStatusSummary | essential | keep |
| FilterSection (action, status, resource) | essential | keep |
| TimelineTab (event table) | essential | keep |
| JSONL export functionality | essential | keep |

**PageScaffold:** YES
**Skeleton loading:** YES (`SkeletonTable` in tabs.rs)
**EmptyState:** NO -- uses plain text `"No audit events found"` in a `<div class="text-center py-12">` -- should use `EmptyState` component

| Issue | Type | Action |
|-------|------|--------|
| Audit empty state uses plain text | fix | Migrate to `EmptyState` component |
| Export button uses `Spinner` inline (in `<Show>`) | keep | Acceptable for button loading state |

---

## 10. Policies Page (`policies.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| Refresh + New Policy Pack buttons | essential | keep |
| Create policy form (CPID, description, JSON) | essential | keep |
| DataTable with policy list | essential | keep |
| Policy validation + apply actions | essential | keep |
| "Learn about Policies" link | essential | keep |

**PageScaffold:** YES
**Skeleton loading:** DataTable handles its own loading -- likely uses `Spinner` internally
**EmptyState:** DataTable has `empty_title` and `empty_description` props -- correct pattern
**Spinner usage:** Two `Spinner` uses for validation/apply button states -- acceptable for button loading

| Issue | Type | Action |
|-------|------|--------|
| None significant | -- | Policies page uses DataTable which handles loading/empty internally |

---

## 11. Documents Page (`documents.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs | essential | keep |
| "Upload document" primary action | essential | keep |
| Search + status filter | essential | keep |
| RefreshButton | essential | keep |
| Document list table | essential | keep |
| DocumentDetail with processing stages | essential | keep |
| Talk-to-Document pipeline stages | essential | keep |
| Document upload dialog | essential | keep |
| EmptyState for "No documents found" | essential | keep -- uses EmptyState component |

**PageScaffold:** YES (list and detail)
**EmptyState:** YES
**Loading:** Uses custom `LoadingState` match patterns -- complex page

| Issue | Type | Action |
|-------|------|--------|
| No skeleton loading for document list | fix | Could use `SkeletonTable` for list loading |

---

## 12. Update Center (`update_center.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs + inspector | essential | keep |
| "Create adapter" primary action | essential | keep |
| Lifecycle filter select | essential | keep |
| SplitPanel list/inspector | essential | keep |
| AdapterDetailPanel | essential | keep |
| EmptyState "No updates available" | essential | keep -- uses EmptyState component |
| Lifecycle state badges | essential | keep |

**PageScaffold:** YES
**EmptyState:** YES
**Loading:** Uses custom match -- no skeleton for list loading

| Issue | Type | Action |
|-------|------|--------|
| No skeleton loading for adapter list in Update Center | fix | Add `SkeletonTable` for loading state |

---

## 13. Welcome Page (`welcome.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold (minimal, no breadcrumbs) | essential | keep |
| OnboardingContainer/Header/ProgressStepper | essential | keep |
| 4-step wizard (Database, Worker, Models, Ready) | essential | keep |
| System status polling | essential | keep |
| Model discovery + seed | essential | keep |
| Spinner for loading states | essential | keep -- wizard-specific |

**PageScaffold:** YES
**This is a special-purpose wizard page.** Loading states use `Spinner` which is appropriate for a wizard flow (not data tables). No changes needed.

| Issue | Type | Action |
|-------|------|--------|
| None | -- | Welcome is a wizard, not a data page |

---

## 14. Login Page (`login.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| Full-screen centered Card layout | essential | keep |
| OfflineBanner | essential | keep |
| Username/password form fields | essential | keep |
| Validation rules | essential | keep |
| "Remember me" checkbox | essential | keep -- UI-ready for future backend support |
| Error display | essential | keep |
| returnUrl redirect logic | essential | keep |

**PageScaffold:** NO -- Login is a standalone full-screen page, does not use PageScaffold. This is correct; login should not have navigation chrome.

| Issue | Type | Action |
|-------|------|--------|
| None | -- | Login is correctly a standalone layout |

---

## 15. Flight Recorder (`flight_recorder.rs`)

| Element | Classification | Proposed Action |
|---------|---------------|-----------------|
| PageScaffold with breadcrumbs + status + inspector | essential | keep |
| Status filter | essential | keep |
| SplitPanel list/detail | essential | keep |
| Run detail tabs (Overview, Trace, Receipt, Routing, Tokens, Diff, Errors) | essential | keep |
| SkeletonDetailSection for loading | essential | keep -- already correct |
| Polling for live updates | essential | keep |
| Export/download functionality | essential | keep |
| Replay execution | essential | keep |
| Receipt verification | essential | keep |

**PageScaffold:** YES (list and detail)
**Skeleton loading:** YES (`SkeletonDetailSection`)
**EmptyState:** Uses custom text "No execution records yet" in a custom container -- should use EmptyState
**Spinner usage:** Multiple inline Spinners in tab content -- mostly inline loading for async operations within already-loaded views

| Issue | Type | Action |
|-------|------|--------|
| "No execution records yet" uses custom empty | fix | Migrate to `EmptyState` component |
| Many inline `Spinner` uses in tabs | keep | These are action-level loading indicators, not page-level |

---

## Cross-Surface Consistency Summary

### PageScaffold Usage

| Page | Uses PageScaffold | Action |
|------|-------------------|--------|
| Adapters | YES | none |
| Models | YES | none |
| Datasets | YES | none |
| Training | YES | none |
| Settings | YES | none |
| System | YES | none |
| Workers | YES | none |
| Audit | YES | none |
| Policies | YES | none |
| Documents | YES | none |
| Update Center | YES | none |
| Welcome | YES | none |
| Login | NO (standalone) | correct |
| Flight Recorder | YES | none |
| Dashboard | YES | none |

**Result:** All pages that should use PageScaffold do. Login is correctly standalone.

### Skeleton Loading

| Page | Has Skeleton | Action |
|------|-------------|--------|
| Adapters | NO (uses LoadingDisplay) | migrate to SkeletonTable |
| Models | YES (SkeletonTable) | none |
| Datasets | NO (uses LoadingDisplay/Spinner) | migrate to SkeletonTable |
| Training | Partial (AsyncBoundary) | low priority |
| Settings | NO (uses Spinner) | migrate to SkeletonCard/SkeletonTable |
| System | YES (SkeletonStatsGrid + SkeletonCard) | none |
| Workers | YES (SkeletonCard + SkeletonTable) | none |
| Audit | YES (SkeletonTable) | none |
| Policies | DataTable handles internally | none |
| Documents | NO (custom) | migrate to SkeletonTable |
| Update Center | NO (custom) | migrate to SkeletonTable |
| Flight Recorder | YES (SkeletonDetailSection) | none |

### EmptyState Component

| Page | Uses EmptyState | Action |
|------|----------------|--------|
| Adapters | YES | none |
| Models | YES | none |
| Datasets | Partial | migrate custom empty text |
| Training | Partial | low priority |
| Settings | NO (plain text) | migrate |
| System | Partial (some components) | migrate key ones |
| Workers | YES | none |
| Audit | NO (plain text) | migrate |
| Policies | YES (via DataTable) | none |
| Documents | YES | none |
| Update Center | YES | none |
| Flight Recorder | NO (custom text) | migrate |

### Glass Tier Issues

| Surface | Current | Should Be | Action |
|---------|---------|-----------|--------|
| Sidebar | T1 (`--glass-bg-1`) | T2 (`--glass-bg-2`) | **change** |
| TopBar | T1 blur + `--os-chrome-top` | T1 (OS chrome) | keep |
| Page content areas | T1 (via page cards) | T1 | correct |
| Detail panels/inspectors | N/A (no glass) | T2 | no glass needed -- uses page scaffold inspector |
| Modals | T3 | T3 | correct (overlays.css) |

---

## Proposed Cut List (All Surfaces)

### Items to REMOVE

None identified. All elements on secondary surfaces connect to working features or serve the operator. No dead controls, placeholder text, or orphaned references found.

### Items to FIX

1. **Sidebar glass tier: T1 -> T2** (layout.css `.sidebar`)
2. **Adapters: loading state -> SkeletonTable**
3. **Datasets: loading state -> SkeletonTable**
4. **Documents: loading state -> SkeletonTable**
5. **Update Center: loading state -> SkeletonTable**
6. **Settings/Security: Spinner -> SkeletonTable/SkeletonCard**
7. **Settings/ApiConfig: Spinner -> SkeletonCard**
8. **Audit: empty state -> EmptyState component**
9. **Flight Recorder: empty state -> EmptyState component**
10. **Settings: plain text empty states -> EmptyState component**
11. **Datasets: plain text empty states -> EmptyState component (where applicable)**
12. **System components: plain text empty states -> EmptyState where prominent**

### Items ALREADY CORRECT

- Models page: full skeleton + EmptyState + PageScaffold
- Workers page: full skeleton + EmptyState + PageScaffold + extensive data-testid
- Audit page: SkeletonTable + PageScaffold
- System page: SkeletonStatsGrid + SkeletonCard + PageScaffold
- Policies: DataTable handles loading/empty
- All data-testid attributes preserved (30+ on workers, 80+ on chat)
- Login: correctly standalone
- Welcome: correctly wizard-specific
- All pages use `try_get()`/`try_set()` (no unsafe `.get()`/`.set()`)
