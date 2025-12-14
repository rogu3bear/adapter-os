# VERIFIED DEAD CODE - HIGH CONFIDENCE

## Methodology
This list contains code verified through multiple methods:
1. No import statements found in codebase
2. No string references to file/export names
3. Manual verification of grep results
4. Excluded tests, examples, and self-references

---

## COMPLETELY UNUSED COMPONENTS (100% Confidence)

### UI Component Library - Safe to Delete
```
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/menu-indicators.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/undo-redo-toolbar.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/page-headers/DashboardPageHeader.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/content-section.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/role-guard.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ui/carousel.tsx
```

### Feature Components - Safe to Delete
```
/Users/mln-dev/Dev/adapter-os/ui/src/components/MessageThread.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/MobileNavigation.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/chat/SimplifiedChatWidget.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/Icon.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ExportButton.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ContextualHelp.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/FavoritesPanel.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/Footer.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/ITAdminDashboard.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/training/TrainingComparisonExample.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/training/DatasetBuilderExample.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/golden/LayerComparisonTable.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/workflows/TemplateCustomizer.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/dashboard/ReportingSummaryWidget.tsx
```

---

## COMPLETELY UNUSED UTILITIES (100% Confidence)

### Adapter Utilities
```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/adapters/categoryHelpers.ts
export function getCategoryLabel() // DEAD CODE

// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/adapters/stateHelpers.ts
export function getAdapterStateIcon() // DEAD CODE
export function getAdapterStateColor() // DEAD CODE
```

### Lifecycle Utilities
```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/lifecycle.ts
export function getLifecycleDescription() // DEAD CODE
export function isHealthyLifecycleState() // DEAD CODE
```

### Memory/Performance
```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/memoryEstimation.ts
export function estimateAdapterMemory() // DEAD CODE
```

### Error Handling
```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/errorMessages.ts
export function getUserFriendlyError() // DEAD CODE
```

### UI Utilities
```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/utils/visual-hierarchy.ts
export function getVisualHierarchyClasses() // DEAD CODE
```

---

## EXAMPLE FILES (Should Be in /examples or /docs)

```
/Users/mln-dev/Dev/adapter-os/ui/src/components/chat/ReplayResultDialog.example.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/chat/ChatShareDialog.example.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/components/PolicyPreflightDialog.example.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/hooks/useDialogManager.example.tsx
/Users/mln-dev/Dev/adapter-os/ui/src/hooks/chat/useChatStreaming.example.tsx
```

**Recommendation:** Move to `ui/examples/` directory for documentation purposes

---

## UNUSED HOOKS (90% Confidence)

```typescript
// File: /Users/mln-dev/Dev/adapter-os/ui/src/hooks/async/useAsyncOperation.ts
export function useAsyncOperation() // Only re-exported, never used

// File: /Users/mln-dev/Dev/adapter-os/ui/src/hooks/async/useRetry.ts
export function useRetry() // Only re-exported, never used
```

**Note:** These are exported from `hooks/async/index.ts` but have zero usage in the codebase. Possible future-use or library code.

---

## ESTIMATED IMPACT

### Lines of Code That Can Be Deleted
- Components: ~20 files, ~2,000-3,000 LOC
- Utilities: ~8 functions, ~200-400 LOC
- Example files: 5 files, ~500-800 LOC

### Total Estimated Savings
- Files: ~25-30
- Lines: ~2,700-4,200 LOC
- Bundle size reduction: ~5-10% (estimate)

---

## RECOMMENDED DELETION ORDER

### Phase 1: Zero Risk (No dependencies)
1. Delete example files (move to examples/ dir)
2. Delete unused UI components (menu-indicators, undo-redo-toolbar, carousel, etc.)
3. Delete unused utility functions

### Phase 2: Low Risk (Verify once more)
1. Delete feature components (MessageThread, MobileNavigation, Icon, etc.)
2. Delete dashboard widgets (ReportingSummaryWidget)
3. Delete training examples (TrainingComparisonExample, DatasetBuilderExample)

### Phase 3: Review Required
1. Review hooks (useAsyncOperation, useRetry) - confirm not part of public API
2. Review pages without routes - verify truly orphaned

---

## VERIFICATION COMMANDS

Before deleting, run these commands to verify:

```bash
# Verify component is not imported
rg "from.*component-name|import.*ComponentName" src/

# Verify function is not called
rg "\bfunctionName\(" src/

# Check for dynamic imports
rg "lazy.*ComponentName|import\(.*component-name" src/
```

---

## FILES ALREADY DELETED (Git Status Only)

These are correctly deleted and just need git commit:
- `/Users/mln-dev/Dev/adapter-os/ui/src/hooks/useGlossary.ts`
- `/Users/mln-dev/Dev/adapter-os/ui/src/utils/mockPeerData.ts`
