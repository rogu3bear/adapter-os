# UX Patch Plan - Remaining Work

**Date:** 2025-10-31  
**Status:** Foundation Complete, Integration Pending  
**Completion:** ~40% of total plan

---

## Summary

The **foundation** for all 6 phases has been implemented, but **integration** across the codebase is incomplete. Here's what remains:

---

## ✅ Completed (Foundation)

### Phase 1: Navigation IA
- ✅ Navigation groups reorganized
- ✅ Mobile navigation component created
- ✅ Integration complete

### Phase 2: Density Context
- ✅ DensityContext created
- ❌ **NOT integrated into any pages**

### Phase 3: Polling Hook
- ✅ usePolling hook created
- ✅ LastUpdated component created
- ✅ TrainingPage refactored
- ❌ **Other components still use manual polling**

### Phase 4: Error Recovery
- ✅ ErrorBoundary updated
- ❌ **Toast errors still exist in 41+ files**

### Phase 5: Breadcrumbs
- ✅ BreadcrumbProvider integrated
- ✅ SingleFileAdapterTrainer has breadcrumbs
- ❌ **TrainingWizard missing breadcrumbs**

### Phase 6: Mobile Navigation
- ✅ Complete

---

## ❌ Incomplete Work

### Phase 2: Density Controls Integration (0% Complete)

#### Missing: Density Controls in TrainingWizard
**File:** `ui/src/components/TrainingWizard.tsx`  
**Status:** Not started  
**Required:**
- Import DensityControls and useDensity
- Wrap component or integrate DensityProvider
- Apply spacing/textSizes throughout wizard steps

**Citation:** 【ui/src/components/TrainingWizard.tsx§1-981】

#### Missing: Density Controls in All Major Pages (0/7 complete)
**Files:** All need DensityProvider integration
- ❌ `ui/src/pages/TenantsPage.tsx`
- ❌ `ui/src/pages/AdaptersPage.tsx`
- ❌ `ui/src/pages/PoliciesPage.tsx`
- ❌ `ui/src/pages/MetricsPage.tsx`
- ❌ `ui/src/pages/TelemetryPage.tsx`
- ❌ `ui/src/pages/InferencePage.tsx`
- ❌ `ui/src/pages/AuditPage.tsx`

**Pattern Required:**
```typescript
<DensityProvider pageKey="tenants">
  <TenantsPage />
</DensityProvider>
```

**Citation:** 【ui/src/pages/InferencePage.tsx§1-15】

---

### Phase 3: Polling Standardization (1/15+ Complete)

#### Completed:
- ✅ TrainingPage.tsx

#### Remaining Components Using Manual Polling:
1. ❌ `ui/src/components/ITAdminDashboard.tsx` - Uses 30s interval
2. ❌ `ui/src/components/AlertsPage.tsx` - Uses 2s interval
3. ❌ `ui/src/components/MonitoringDashboard.tsx` - Uses 5s interval
4. ❌ `ui/src/components/UserReportsPage.tsx` - Uses 60s interval
5. ❌ `ui/src/components/RealtimeMetrics.tsx` - Uses SSE + fallback polling
6. ❌ `ui/src/components/TrainingMonitor.tsx` - Likely has polling
7. ❌ `ui/src/components/BaseModelStatus.tsx` - Likely has polling
8. ❌ `ui/src/components/ResourceMonitor.tsx` - Likely has polling
9. ❌ `ui/src/components/WorkersTab.tsx` - Likely has polling
10. ❌ `ui/src/components/dashboard/ReportingSummaryWidget.tsx` - Likely has polling
11. ❌ `ui/src/components/observability/MetricsGrid.tsx` - Likely has polling
12. ❌ `ui/src/components/observability/TraceTimeline.tsx` - Likely has polling
13. ❌ `ui/src/components/Dashboard.tsx` - May have polling
14. ❌ `ui/src/components/AdaptersPage.tsx` - May have polling

**Pattern Required:**
```typescript
const { data, isLoading, lastUpdated } = usePolling(
  () => apiClient.getSystemMetrics(),
  'normal', // or 'fast' or 'slow'
  { showLoadingIndicator: true }
);
```

**Citation:** 【ui/src/hooks/usePolling.ts】

---

### Phase 4: Error Recovery Consistency (1/41+ Complete)

#### Completed:
- ✅ ErrorBoundary.tsx updated

#### Remaining Components with Toast Errors:
Found **41 files** still using `toast.error()`, `toast.success()`, etc.:

**High Priority (Frequently Used):**
1. ❌ `ui/src/components/SingleFileAdapterTrainer.tsx` - Multiple toast calls
2. ❌ `ui/src/components/TrainingPage.tsx` - Some toast calls remain
3. ❌ `ui/src/components/AlertsPage.tsx`
4. ❌ `ui/src/components/Adapters.tsx`
5. ❌ `ui/src/components/TrainingWizard.tsx`
6. ❌ `ui/src/components/InferencePlayground.tsx`
7. ❌ `ui/src/components/TestingPage.tsx`
8. ❌ `ui/src/components/AdaptersPage.tsx`
9. ❌ `ui/src/components/ITAdminDashboard.tsx`
10. ❌ `ui/src/components/UserReportsPage.tsx`

**All Other Files:**
- 31+ additional components with toast usage

**Pattern Required:**
```typescript
// Replace:
toast.error('Failed to load');
// With:
setError(err);
// And in render:
{error && ErrorRecoveryTemplates.genericError(
  () => refetch(),
  () => navigate('/dashboard')
)}
```

**Citation:** 【ui/src/components/ui/error-recovery.tsx§154-253】

---

### Phase 5: Breadcrumbs in Workflows (1/2 Complete)

#### Completed:
- ✅ SingleFileAdapterTrainer.tsx

#### Missing:
- ❌ `ui/src/components/TrainingWizard.tsx` - Needs breadcrumb integration

**Required Implementation:**
```typescript
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import { BreadcrumbNavigation } from '@/components/BreadcrumbNavigation';

export function TrainingWizard({ onComplete, onCancel }: TrainingWizardProps) {
  const { setBreadcrumbs } = useBreadcrumb();
  
  useEffect(() => {
    const steps = [
      { id: 'category', label: 'Category', icon: Code },
      { id: 'info', label: 'Basic Info', icon: Settings },
      // ... more steps
    ];
    setBreadcrumbs(steps.slice(0, currentStep + 1));
  }, [currentStep, setBreadcrumbs]);
  
  return (
    <div>
      <BreadcrumbNavigation />
      {/* Rest of wizard */}
    </div>
  );
}
```

**Citation:** 【ui/src/components/TrainingWizard.tsx§1-981】

---

## Completion Metrics

### Overall Progress
- **Foundation:** 100% (all core components/hooks created)
- **Integration:** ~15% (only a few components integrated)
- **Total Completion:** ~40%

### By Phase
1. **Phase 1:** ✅ 100% Complete
2. **Phase 2:** 🔄 20% Complete (context created, no pages integrated)
3. **Phase 3:** 🔄 7% Complete (hook created, 1/15+ components refactored)
4. **Phase 4:** 🔄 2% Complete (boundary updated, 41+ files still have toasts)
5. **Phase 5:** 🔄 50% Complete (1/2 workflows have breadcrumbs)
6. **Phase 6:** ✅ 100% Complete

---

## Required Work Breakdown

### High Priority (Core Functionality)
1. **Polling Standardization** - 14+ components need refactoring
2. **Error Recovery** - 41+ files need toast replacement
3. **Breadcrumbs** - TrainingWizard integration

### Medium Priority (UX Enhancement)
4. **Density Controls** - 7 pages + TrainingWizard need integration

### Estimated Effort
- **Polling Refactoring:** 2-3 days (14 components × ~30 min each)
- **Error Recovery:** 3-4 days (41 files × ~20 min each)
- **Breadcrumbs:** 1 day (TrainingWizard integration)
- **Density Controls:** 2 days (8 components × ~30 min each)

**Total Remaining:** ~8-10 days of work

---

## Next Steps

### Immediate Actions
1. Refactor polling in ITAdminDashboard, AlertsPage, MonitoringDashboard
2. Replace toast errors in high-traffic components (TrainingWizard, InferencePlayground)
3. Add breadcrumbs to TrainingWizard

### Short-term
4. Integrate density controls into all major pages
5. Complete polling refactoring for all components
6. Replace remaining toast errors

### Testing
7. Unit tests for usePolling hook
8. Integration tests for error recovery
9. E2E tests for complete workflows

---

## Files Needing Updates

### Polling Refactoring (14 files)
- `ui/src/components/ITAdminDashboard.tsx`
- `ui/src/components/AlertsPage.tsx`
- `ui/src/components/MonitoringDashboard.tsx`
- `ui/src/components/UserReportsPage.tsx`
- `ui/src/components/RealtimeMetrics.tsx`
- `ui/src/components/TrainingMonitor.tsx`
- `ui/src/components/BaseModelStatus.tsx`
- `ui/src/components/ResourceMonitor.tsx`
- `ui/src/components/WorkersTab.tsx`
- `ui/src/components/dashboard/ReportingSummaryWidget.tsx`
- `ui/src/components/observability/MetricsGrid.tsx`
- `ui/src/components/observability/TraceTimeline.tsx`
- `ui/src/components/Dashboard.tsx`
- `ui/src/components/AdaptersPage.tsx`

### Error Recovery (41+ files)
- All files listed in grep results need toast replacement

### Density Controls (8 files)
- `ui/src/components/TrainingWizard.tsx`
- `ui/src/pages/TenantsPage.tsx`
- `ui/src/pages/AdaptersPage.tsx`
- `ui/src/pages/PoliciesPage.tsx`
- `ui/src/pages/MetricsPage.tsx`
- `ui/src/pages/TelemetryPage.tsx`
- `ui/src/pages/InferencePage.tsx`
- `ui/src/pages/AuditPage.tsx`

### Breadcrumbs (1 file)
- `ui/src/components/TrainingWizard.tsx`

---

**Status:** Foundation Complete, Integration ~40% Complete  
**Remaining Work:** ~8-10 days estimated  
**Priority:** Polling → Errors → Breadcrumbs → Density

