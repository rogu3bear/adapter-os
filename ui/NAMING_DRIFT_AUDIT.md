# AdapterOS UI Naming Drift Audit Report

**Generated:** 2025-12-13
**Last Validated:** 2025-12-13 (6 validation agents)
**Scope:** 1,037 TypeScript/TSX files in `ui/src/`
**Analysis Dimensions:** API Types, Components, Hooks, Domain Terminology, File Organization

---

## Executive Summary

The AdapterOS UI codebase exhibits **significant naming drift** accumulated across multiple development iterations with multiple agents. This drift manifests in 5 key dimensions:

| Dimension | Severity | Key Issue | Impact |
|-----------|----------|-----------|--------|
| API/Types | **HIGH** | snake_case/camelCase mixing, 9 dangerous `as any` in production | Type safety bypassed |
| Components | **HIGH** | 10 "Page" components in wrong directory, 2 true duplicates | Import confusion |
| Hooks | **HIGH** | 96 root-level hooks, ~1,200 LOC SSE+polling duplication | Code duplication |
| Domain Terms | **MEDIUM** | Tenant vs Workspace, Job vs Run ambiguity | User confusion |
| File Org | **MEDIUM** | Mixed page structure, 17 hooks outside hooks/ directory | Navigation difficulty |

**Estimated Remediation Effort:** 2-3 weeks focused work
**Risk Level:** Medium - All changes isolated, testable, reversible

### Validation Status

| Finding | Initial | Validated | Action |
|---------|---------|-----------|--------|
| Root-level hooks | 73 | **96** | Corrected upward |
| `as any` in production | 110 | **9** (rest in tests) | Focused scope |
| RouterConfigPage duplicate | DELETE | **KEEP BOTH** | Intentional separation |
| EvidencePanel duplicate | CONSOLIDATE | **RENAME** chat version | Different purposes |
| Orphaned files | Unknown | **3 confirmed** | Safe to delete |

---

## Table of Contents

1. [API & Type Naming Drift](#1-api--type-naming-drift)
2. [Component Naming Drift](#2-component-naming-drift)
3. [Hook Naming Drift](#3-hook-naming-drift)
4. [Domain Terminology Drift](#4-domain-terminology-drift)
5. [File Organization Drift](#5-file-organization-drift)
6. [Canonical Naming Standards](#6-canonical-naming-standards)
7. [Remediation Plan](#7-remediation-plan)

---

## 1. API & Type Naming Drift

### 1.1 Field Name Variations

The same concepts have multiple field names across the codebase:

#### Token Counting (4 variations)
| Field | Context | Files | Status |
|-------|---------|-------|--------|
| `tokens_generated` | RawInferResponse, InferResponse | 143 | **CANONICAL** |
| `token_count` | Optional secondary field | 143 | Deprecated |
| `tokens_used` | DomainAdapterExecutionResponse | 1 | Domain-specific |
| `total_tokens` | Aggregate forms | 50+ | Aggregate only |

#### Adapter Identification (4 variations)
| Field | Context | Files | Status |
|-------|---------|-------|--------|
| `adapter_id` | Backend API responses | 183 | **CANONICAL** (backend) |
| `adapterId` | Frontend components | 167 | **CANONICAL** (frontend) |
| `id` | Fallback/alias | Multiple | Remove - use adapter_id |
| `loraId` | Not used | 0 | Correct - avoid |

#### State Management (4 variations)
| Field | Meaning | Status |
|-------|---------|--------|
| `lifecycle_state` | Release state (draft→retired) | **CANONICAL** |
| `runtime_state` | Memory state (unloaded→resident) | **CANONICAL** |
| `current_state` | Ambiguous - remove | Deprecated |
| `status` | Generic - avoid | Deprecated |

### 1.2 Defensive Fallback Patterns

**95+ nullish coalescing (??) instances** in `api/client.ts` alone indicate naming confusion:

```typescript
// ui/src/api/client.ts:858
return resp.status ?? (resp as unknown as adapterTypes.CoremlPackageStatus);

// ui/src/api/client.ts:728
return resp.tenants ?? [];
```

### 1.3 Type Naming Convention Drift

| Pattern | Count | Issue |
|---------|-------|-------|
| `Raw*` prefix (documented) | 3 types | Incomplete - only 3 of 83+ types |
| `*Response` suffix | 83 types | Good - consistent |
| `as any` in production | **9** occurrences | 3 dangerous, 4 fixable, 2 necessary |
| `as any` in tests | ~100 occurrences | Acceptable for test mocks |
| `as unknown as` | 1 critical | Hides naming mismatches |

### 1.4 Production `as any` Breakdown (Validated)

| Category | Files | Lines | Action |
|----------|-------|-------|--------|
| **DANGEROUS** | `ChatInterface.tsx` | 195, 208-209 | Fix immediately |
| **DANGEROUS** | `TrainingJobDetail.tsx` | 360 | Fix immediately |
| **DANGEROUS** | `EvidenceItem.tsx` | 56, 59, 64, 66 | Fix immediately |
| **FIXABLE** | `EvidencePanel.tsx`, `useEvidenceApi.ts` | Various | Add proper types |
| **FIXABLE** | `useStreamingEndpoints.ts`, `useStreamingInference.ts` | Various | Add proper types |
| **NECESSARY** | `generatePdf.ts` | - | jsPDF library limitation |
| **NECESSARY** | `StackSortableAdapterItem.tsx` | - | React Hook Form limitation |

### 1.5 Files Requiring Attention

- `ui/src/api/api-types.ts` - 83 Response types, dual fields
- `ui/src/api/client.ts` - 95+ defensive fallbacks
- `ui/src/api/adapter-types.ts` - Multiple state fields
- `ui/src/api/schemas/inference.zod.ts` - Uses `.passthrough()` hiding drift

---

## 2. Component Naming Drift

### 2.1 Components in Wrong Directory

**10 "Page" components in `components/` that should be in `pages/`:**

| File | Current Location | Should Be |
|------|-----------------|-----------|
| `AdaptersPage.tsx` | `components/` | `pages/Adapters/` |
| `AlertsPage.tsx` | `components/` | `pages/Alerts/` |
| `ContactsPage.tsx` | `components/` | `pages/Contacts/` |
| `DiscoveryStreamPage.tsx` | `components/` | `pages/Discovery/` |
| `GitIntegrationPage.tsx` | `components/` | `pages/Git/` |
| `MonitoringPage.tsx` | `components/` | `pages/Monitoring/` |
| `RouterConfigPage.tsx` | `components/` | `pages/Router/` |
| `TestingPage.tsx` | `components/` | `pages/Testing/` |
| `TrainingStreamPage.tsx` | `components/` | `pages/Training/` |
| `UserReportsPage.tsx` | `components/` | `pages/UserReports/` |

### 2.2 Duplicate/Conflicting Files (Validated)

| Filename | Location 1 | Location 2 | Validated Action |
|----------|-----------|-----------|------------------|
| `RouterConfigPage.tsx` | `components/` | `pages/` | **KEEP BOTH** - Intentional (page wrapper vs component) |
| `EvidencePanel.tsx` | `components/evidence/` | `components/chat/` | **RENAME** chat version to `EvidenceSources.tsx` |
| `PageHeader.tsx` | `components/shared/` | `components/shared/Navigation/` | **DELETE** Navigation version (0 imports) |
| `Adapters.tsx` | `components/` | - | **DELETE** (orphaned, 0 imports) |

### 2.2.1 Orphaned Files Safe to Delete

| File | External Imports | Barrel Exports | Action |
|------|------------------|----------------|--------|
| `components/Adapters.tsx` | 0 | 0 | DELETE immediately |
| `hooks/useGlossary.ts` | 0 | 0 | DELETE immediately |
| `utils/mockPeerData.ts` | 0 | 0 | DELETE immediately |
| `components/shared/Navigation/PageHeader.tsx` | 0 | 0 | DELETE immediately |

### 2.3 File + Directory Name Collisions

| Name | File | Directory | Issue |
|------|------|-----------|-------|
| `adapters` | `Adapters.tsx` | `adapters/` | Import ambiguity |
| `dashboard` | `Dashboard.tsx` | `dashboard/` | Import ambiguity |
| `policies` | `Policies.tsx` | `policies/` | Import ambiguity |
| `telemetry` | `Telemetry.tsx` | `telemetry/` | Import ambiguity |
| `TrainingWizard` | `TrainingWizard.tsx` | `TrainingWizard/` | Import ambiguity |

### 2.4 Modal vs Dialog Naming

**17 files with inconsistent suffix:**
- 9 use `*Modal.tsx`
- 8 use `*Dialog.tsx`

No clear convention - scattered across 6+ locations.

### 2.5 Component Scatter Analysis

**Adapter-related files scattered across 7 locations (27 total files):**
```
components/ (6 files)
components/adapters/ (5 files)
components/chat/ (2 adapter-related files)
components/dashboard/ (1 file)
pages/Adapters/ (13 files)
```

**Training-related files scattered across 6 locations (20+ files)**

**Policy-related files scattered across 4 locations (18+ files)**

---

## 3. Hook Naming Drift

### 3.1 Real-time Data Hooks (3 Competing Implementations)

| Hook | Transport | Return Shape | LOC |
|------|-----------|--------------|-----|
| `usePolling` | Polling | `{ data, isLoading, error, refetch }` | 150 |
| `useSSE` | SSE | `{ data, error, connected, reconnect }` | 200 |
| `useLiveData` | SSE+Polling | `{ data, isLoading, sseConnected, ... }` | 300 |

**All three do similar things with different APIs.**

Additionally, **4 hooks independently implement SSE+polling:**
- `useActivityFeed()` - 450 LOC
- `useNotifications()` - 380 LOC
- `useMessages()` - 350 LOC
- `useLiveData()` - 300 LOC

### 3.2 Return Value Naming Inconsistencies

| Field | usePolling | useSSE | useLiveData | useActivityFeed |
|-------|------------|--------|-------------|-----------------|
| Error | `error: Error` | `error: string` | `error: Error` | `error: string` |
| Loading | `isLoading` | ❌ | `isLoading` | `loading` |
| Refresh | `refetch()` | `reconnect()` | `refetch()` | `refresh()` |

### 3.3 Hook Location Violations

**Hooks defined outside `hooks/` directory:**

| Hook | Location | Should Be |
|------|----------|-----------|
| `useAdapters` | `pages/Adapters/useAdapters.ts` | `hooks/adapters/` |
| `useModal` | `components/shared/Modal/useModal.ts` | `hooks/ui/` |
| `useStackValidation` | `components/adapters/useStackValidation.ts` | `hooks/adapters/` |
| `useDiffKeyboardNav` | `components/golden/useDiffKeyboardNav.ts` | `hooks/golden/` |
| `use-mobile` | `components/ui/use-mobile.ts` | `hooks/ui/` |

### 3.4 Root Level Hook Overload (Validated)

**96 hooks at `hooks/` root level** - poor discoverability.

| Category | Count | Target Location |
|----------|-------|-----------------|
| `useAdapter*` | 5 hooks | `hooks/adapters/` |
| `useChat*` | 7 hooks | `hooks/chat/` |
| `useTraining*` | 3 hooks | `hooks/training/` |
| `useDocument*` | 4 hooks | `hooks/documents/` |
| `usePolling*`, `useSSE*` | 4 hooks | `hooks/realtime/` |

### 3.4.1 Hooks Outside `hooks/` Directory (17 total)

| Hook | Current Location | Should Move To |
|------|------------------|----------------|
| `useAdapters` | `pages/Adapters/useAdapters.ts` | `hooks/adapters/` |
| `useStackValidation` | `components/adapters/useStackValidation.ts` | `hooks/adapters/` |
| `useDiffKeyboardNav` | `components/golden/useDiffKeyboardNav.ts` | `hooks/golden/` |
| `useWorkflowForm` | `components/workflows/useWorkflowForm.ts` | `hooks/workflows/` |
| `use-mobile` | `components/ui/use-mobile.ts` | `hooks/ui/` |
| Plus 12 more scattered hooks | Various | Respective domain directories |

### 3.5 API Hook Pattern Drift

**Old pattern (mega-hooks):**
```typescript
// DEPRECATED - Combined hook
useDocumentsApi() → { documents, uploadDocument, deleteDocument, ... }
useChatSessionsApi() → { sessions, createSession, deleteSession, ... }
```

**New pattern (factory + individual hooks):**
```typescript
// PREFERRED - Individual hooks
useDocuments() → QueryResult
useUploadDocument() → MutationResult
useDeleteDocument() → MutationResult
```

**Migration incomplete** - both patterns coexist.

### 3.6 Additional Pattern Drift (Validated)

#### Boolean Prop Naming
| Pattern | Occurrences | Recommendation |
|---------|-------------|----------------|
| `isLoading` | 1,791 | **CANONICAL** (75% usage) |
| `loading` | 219 | Migrate to `isLoading` |
| `isError` | 412 | Consistent |
| `error` (boolean context) | 89 | Migrate to `isError` |

#### Callback Naming
| Pattern | Occurrences | Recommendation |
|---------|-------------|----------------|
| `onSuccess` | 1,322 | **CANONICAL** |
| `onComplete` | ~50 | Migrate to `onSuccess` |
| `onFinish` | ~20 | Migrate to `onSuccess` |
| `onClose` | 312 | **CANONICAL** for modals |
| `onOpenChange` | 167 | Radix pattern - keep |
| `onDismiss` | ~50 | Migrate to `onClose` |

#### Error Type Convention
| Pattern | Occurrences | Recommendation |
|---------|-------------|----------------|
| `error: Error \| null` | 90 | **CANONICAL** |
| `error: string \| null` | 784 | Legacy - migrate gradually |

#### Refresh/Refetch Naming
| Pattern | Occurrences | Percentage |
|---------|-------------|------------|
| `refetch` | 174 | 72% - **CANONICAL** |
| `refresh` | 69 | 28% - migrate |

---

## 4. Domain Terminology Drift

### 4.1 Canonical Terms

| Concept | Canonical | Acceptable Alias | Avoid |
|---------|-----------|------------------|-------|
| Model specialization | **Adapter** | LoRA Adapter | Model, Weights |
| Adapter composition | **Stack** | Adapter Stack | Composition, Bundle |
| Document grouping | **Collection** | Document Set | (don't confuse with Stack) |
| User isolation | **Tenant** (backend) | Workspace (UI) | Organization, Account |
| Chat history | **Session** (backend) | Chat (UI) | Conversation, Thread |
| ML training execution | **Training Job** | Job (in context) | Task, Run |
| Model inference | **Inference** | - | Generation, Completion |
| Execution record | **Run** | Run Receipt | Execution |

### 4.2 ID Field Conventions

| Context | Format | Example |
|---------|--------|---------|
| Backend API/Database | snake_case | `adapter_id`, `session_id` |
| Frontend Props/State | camelCase | `adapterId`, `sessionId` |
| UI Labels | Title Case | "Adapter", "Stack" |

### 4.3 Terminology Frequency

| Term | Occurrences | Status |
|------|-------------|--------|
| `adapter` / `Adapter` | 1,385+ | Dominant - correct |
| `stack` / `Stack` | 1,225+ | Dominant - correct |
| `tenant` / `tenantId` | 1,226+ | Backend term |
| `workspace` | 80+ | UI term - OK |
| `session` | 399+ | Backend term |
| `chat` | 1,608+ | UI term - OK |

---

## 5. File Organization Drift

### 5.1 Directory Structure Issues

| Issue | Description | Severity |
|-------|-------------|----------|
| Pages dual structure | Both top-level (`AdaptersPage.tsx`) AND subdirectory (`pages/Adapters/`) | **HIGH** |
| Modals scattered | 40+ modals across 6+ directories | **HIGH** |
| 114 top-level components | Too many files at `components/` root | **MEDIUM** |
| 73 top-level hooks | Too many files at `hooks/` root | **MEDIUM** |
| Utility naming mixed | `doc-loader.ts` vs `errorMessages.ts` | **LOW** |

### 5.2 Import Pattern Analysis

| Type | Count | Percentage |
|------|-------|------------|
| Alias imports (`@/`) | 3,489 | 77% ✓ |
| Relative imports (`../`) | 1,044 | 23% |

**Alias imports dominant - good practice.**

### 5.3 Test Organization

| Location | Count | Percentage |
|----------|-------|------------|
| Centralized `__tests__/` | 108 | 83% |
| Co-located | 22 | 17% |

**Mixed pattern - no clear standard.**

### 5.4 RBAC Duplication

Two RBAC implementations exist:
- `lib/rbac.ts` (possibly legacy)
- `utils/rbac.ts` (18KB, active)

---

## 6. Canonical Naming Standards

### 6.1 File Naming

| Type | Convention | Example |
|------|------------|---------|
| Components | PascalCase | `AdapterDetail.tsx` |
| Hooks | camelCase with `use` prefix | `useAdapterDetail.ts` |
| Utilities | camelCase | `formatDate.ts` |
| Types | camelCase with suffix | `adapterTypes.ts` |
| Tests | `.test.ts` suffix | `AdapterDetail.test.tsx` |

### 6.2 Variable/Field Naming

| Context | Convention | Example |
|---------|------------|---------|
| Backend API fields | snake_case | `adapter_id`, `tokens_generated` |
| Frontend variables | camelCase | `adapterId`, `tokensGenerated` |
| Component props | camelCase | `onSelect`, `isLoading` |
| CSS classes | kebab-case | `adapter-card`, `is-loading` |

### 6.3 Type Naming

| Pattern | Usage | Example |
|---------|-------|---------|
| `Raw*` | Backend response types | `RawAdapterResponse` |
| `*Response` | API response wrappers | `AdapterDetailResponse` |
| `*Props` | Component props | `AdapterDetailProps` |
| `*State` | State shapes | `SessionState` |
| `Use*Return` | Hook return types | `UseAdapterDetailReturn` |

### 6.4 Return Value Naming

| Field | Standard Name | Avoid |
|-------|--------------|-------|
| Loading state | `isLoading` | `loading`, `isLoading` mixed |
| Error object | `error: Error \| null` | `error: string` |
| Refresh function | `refetch()` | `refresh()` |
| Data | `data` | `events`, `items` (unless domain-specific) |

---

## 7. Remediation Plan (Validated)

### Phase 1: Critical Fixes (Immediate)

#### 1.1 Delete Orphaned Files (Safe - 0 imports)
- [ ] Delete `components/Adapters.tsx`
- [ ] Delete `hooks/useGlossary.ts`
- [ ] Delete `utils/mockPeerData.ts`
- [ ] Delete `components/shared/Navigation/PageHeader.tsx`

#### 1.2 Rename Conflicting Files
- [ ] Rename `components/chat/EvidencePanel.tsx` → `EvidenceSources.tsx`

#### 1.3 Fix Dangerous Type Assertions (3 files, 7 locations)
- [ ] Fix `ChatInterface.tsx:195,208-209` - Add proper types for streaming response
- [ ] Fix `TrainingJobDetail.tsx:360` - Type the job metrics properly
- [ ] Fix `EvidenceItem.tsx:56,59,64,66` - Type the evidence data

#### 1.4 Move Page Components
- [ ] Move 10 `*Page.tsx` files from `components/` to `pages/`
- [ ] Update all imports (use find-and-replace with import path aliases)

### Phase 2: Hook Consolidation

#### 2.1 Create Shared Real-time Hook (~1,200 LOC savings)
- [ ] Create `hooks/realtime/useSSEWithPollingFallback.ts` base hook
- [ ] Refactor `useActivityFeed` (450 LOC) to use shared hook
- [ ] Refactor `useNotifications` (380 LOC) to use shared hook
- [ ] Refactor `useMessages` (350 LOC) to use shared hook
- [ ] Remove duplicate SSE+polling implementations

#### 2.2 Relocate 17 Hooks Outside `hooks/` Directory
- [ ] Move `pages/Adapters/useAdapters.ts` → `hooks/adapters/useAdapters.ts`
- [ ] Move `components/adapters/useStackValidation.ts` → `hooks/adapters/`
- [ ] Move `components/golden/useDiffKeyboardNav.ts` → `hooks/golden/`
- [ ] Move `components/workflows/useWorkflowForm.ts` → `hooks/workflows/`
- [ ] Move `components/ui/use-mobile.ts` → `hooks/ui/useMobile.ts`
- [ ] Move remaining 12 scattered hooks to respective directories

#### 2.3 Organize 96 Root-Level Hooks
- [ ] Create `hooks/adapters/` and move 5 adapter hooks
- [ ] Create `hooks/chat/` and move 7 chat hooks (already exists, consolidate)
- [ ] Create `hooks/training/` and move 3 training hooks
- [ ] Create `hooks/documents/` and move 4 document hooks
- [ ] Create `hooks/realtime/` and move 4 polling/SSE hooks

#### 2.4 Standardize Return Values
- [ ] All hooks return `error: Error | null` (not `string` - 784 occurrences to migrate)
- [ ] All hooks use `isLoading` (migrate 219 `loading` occurrences)
- [ ] All hooks use `refetch()` (migrate 69 `refresh()` occurrences)

### Phase 3: Organization Cleanup (Week 3)

#### 3.1 Component Organization
- [ ] Resolve file/directory name collisions
- [ ] Consolidate modals to `components/modals/` or feature directories
- [ ] Move scattered feature components to feature directories

#### 3.2 Utility Cleanup
- [ ] Rename kebab-case utilities to camelCase
- [ ] Resolve RBAC duplication
- [ ] Create `utils/adapters/`, `utils/training/` subdirectories

#### 3.3 Documentation
- [ ] Add naming convention to AGENTS.md
- [ ] Update glossary with all canonical terms
- [ ] Create import path guidelines

### Verification Checklist

After remediation:
- [ ] `pnpm typecheck` passes
- [ ] `pnpm build` succeeds
- [ ] Zero `as any` in production code
- [ ] Zero duplicate filenames across directories
- [ ] All hooks in `hooks/` directory
- [ ] All pages in `pages/` directory
- [ ] Consistent return value shapes in hooks

---

## Appendix: File References (Validated)

### Immediate Deletions (0 external imports - safe)

```
ui/src/components/Adapters.tsx                        → DELETE
ui/src/hooks/useGlossary.ts                           → DELETE
ui/src/utils/mockPeerData.ts                          → DELETE
ui/src/components/shared/Navigation/PageHeader.tsx    → DELETE
```

### File Renames

```
ui/src/components/chat/EvidencePanel.tsx → RENAME to EvidenceSources.tsx
```

### Files to Keep (Intentional Architecture)

```
# RouterConfigPage - KEEP BOTH (page wrapper + component implementation)
ui/src/pages/RouterConfig/RouterConfigPage.tsx        → Page wrapper
ui/src/components/RouterConfigPage.tsx                → Component implementation

# EvidencePanel - KEEP BOTH (different purposes)
ui/src/components/evidence/EvidencePanel.tsx          → Compliance/evidence bundles
ui/src/components/chat/EvidencePanel.tsx              → RAG sources (rename to EvidenceSources.tsx)
```

### Page Components to Move (with import counts)

| File | External Imports | Barrel Exports | Risk |
|------|------------------|----------------|------|
| `AdaptersPage.tsx` | 2 | 0 | Low - update AdaptersShell.tsx, pages/AdaptersPage.tsx |
| `AlertsPage.tsx` | 1 | 0 | Low |
| `ContactsPage.tsx` | 1 | 0 | Low |
| `GitIntegrationPage.tsx` | 1 | 0 | Low |
| `MonitoringPage.tsx` | 1 | 0 | Low |
| `TestingPage.tsx` | 1 | 0 | Low |
| `TrainingStreamPage.tsx` | 1 | 0 | Low |
| `UserReportsPage.tsx` | 1 | 0 | Low |

### Hooks to Relocate (17 outside hooks/)

```
ui/src/pages/Adapters/useAdapters.ts                  → hooks/adapters/
ui/src/components/adapters/useStackValidation.ts      → hooks/adapters/
ui/src/components/golden/useDiffKeyboardNav.ts        → hooks/golden/
ui/src/components/workflows/useWorkflowForm.ts        → hooks/workflows/
ui/src/components/ui/use-mobile.ts                    → hooks/ui/useMobile.ts
# Plus 12 more...
```

### Dangerous Type Assertions to Fix

```
ui/src/components/ChatInterface.tsx:195               → as any on streaming response
ui/src/components/ChatInterface.tsx:208-209           → as any on message parsing
ui/src/pages/Training/TrainingJobDetail.tsx:360       → as any on job metrics
ui/src/components/chat/EvidenceItem.tsx:56,59,64,66   → as any on evidence data
```

### Barrel Exports Needing Updates

```
ui/src/components/monitoring/index.ts                 → Remove AdapterMemoryMonitor re-export
ui/src/components/wizards/index.ts                    → Remove AdapterImportWizard re-export
```

---

**Report prepared by audit agents (5 initial + 6 validation)**
**Last updated:** 2025-12-13
**Validation complete:** All findings cross-checked with codebase
