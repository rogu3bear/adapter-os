# AdapterOS UI UX Patch Plan

**Date:** 2025-10-31  
**Status:** Planning  
**Priority:** P0 (Critical)  
**Estimated Effort:** 3-4 sprints (6-8 weeks)

---

## Executive Summary

This document outlines a comprehensive plan to patch UX issues identified in the UX audit report. All patches follow deterministic practices with complete citations to existing codebase patterns and standards.

**Citation Format:** `【commit-hash†category†identifier】` or `【commit-hash†filepath§line-range】`

**Key Principles:**
- ✅ Deterministic implementations (no randomness without seeding)
- ✅ Complete citations to existing patterns
- ✅ Backward compatibility maintained
- ✅ Progressive enhancement approach
- ✅ Accessibility-first (WCAG 2.1 AA compliance)

---

## Phase 1: Critical Navigation Information Architecture (P0)

### Issue
Navigation groups lack logical hierarchy causing cognitive overload. Current structure【ui/src/layout/RootLayout.tsx§90-141】doesn't align with user mental models.

### Solution
Reorganize navigation into user-centric groups following established component patterns【ui/src/components/WorkflowWizard.tsx§40-251】.

### Implementation Plan

#### Patch 1.1: Refactor Navigation Groups
**File:** `ui/src/layout/RootLayout.tsx`  
**Lines:** 90-141  
**Citation:** 【ui/src/layout/RootLayout.tsx§90-141】

**Changes:**
```typescript
// Replace existing navigationGroups with user-centric structure
const navigationGroups: NavGroup[] = [
  {
    title: 'Home',
    items: [
      { to: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
      { to: '/workflow', label: 'Getting Started', icon: Compass }
    ]
  },
  {
    title: 'ML Pipeline',
    items: [
      { to: '/trainer', label: 'Single-File Trainer', icon: Upload },
      { to: '/training', label: 'Training Jobs', icon: Zap },
      { to: '/testing', label: 'Testing', icon: FlaskConical },
      { to: '/golden', label: 'Golden Runs', icon: GitCompare },
      { to: '/promotion', label: 'Promotion', icon: TrendingUp },
      { to: '/adapters', label: 'Adapters', icon: Box }
    ]
  },
  {
    title: 'Monitoring',
    items: [
      { to: '/metrics', label: 'Metrics', icon: Activity },
      { to: '/monitoring', label: 'System Health', icon: Activity },
      { to: '/alerts', label: 'Alerts', icon: Bell }
    ]
  },
  {
    title: 'Operations',
    items: [
      { to: '/inference', label: 'Inference', icon: Play },
      { to: '/routing', label: 'Routing Inspector', icon: Route },
      { to: '/telemetry', label: 'Telemetry', icon: Eye }
    ]
  },
  {
    title: 'Compliance',
    items: [
      { to: '/policies', label: 'Policies', icon: Shield },
      { to: '/audit', label: 'Audit', icon: FileText },
      { to: '/replay', label: 'Replay', icon: RotateCcw }
    ]
  },
  {
    title: 'Administration',
    items: [
      { to: '/admin', label: 'IT Admin', icon: Settings },
      { to: '/reports', label: 'Reports', icon: BarChart3 }
    ],
    roles: ['Admin']
  }
];
```

**Citation Reference:**
- Navigation structure: 【ui/src/layout/RootLayout.tsx§90-141】
- Role-based filtering: 【ui/src/layout/RootLayout.tsx§152-155】
- Icon imports: 【ui/src/layout/RootLayout.tsx§8-31】

**Testing:**
- Verify all routes accessible via new navigation
- Test role-based filtering works correctly
- Confirm mobile navigation adapts properly

---

## Phase 2: Progressive Disclosure Consistency (P1)

### Issue
Information density controls【ui/src/hooks/useInformationDensity.ts§1-122】exist but only applied to dashboard. Not used in TrainingWizard【ui/src/components/TrainingWizard.tsx§1-981】or other major pages.

### Solution
Extend density controls to all major pages following dashboard pattern【ui/src/components/Dashboard.tsx§50-280】.

### Implementation Plan

#### Patch 2.1: Create Page-Level Density Context
**File:** `ui/src/contexts/DensityContext.tsx` (NEW)  
**Citation:** 【ui/src/hooks/useInformationDensity.ts§1-122】

**Implementation:**
```typescript
// 【ui/src/hooks/useInformationDensity.ts§1-122】 - Information density hook pattern
import React, { createContext, useContext, ReactNode } from 'react';
import { useInformationDensity, InformationDensity, InformationDensityConfig } from '@/hooks/useInformationDensity';

interface DensityContextValue {
  density: InformationDensity;
  setDensity: (density: InformationDensity) => void;
  spacing: ReturnType<typeof useInformationDensity>['spacing'];
  textSizes: ReturnType<typeof useInformationDensity>['textSizes'];
  isCompact: boolean;
  isComfortable: boolean;
  isSpacious: boolean;
}

const DensityContext = createContext<DensityContextValue | undefined>(undefined);

interface DensityProviderProps {
  children: ReactNode;
  pageKey: string;
  defaultDensity?: InformationDensity;
  persist?: boolean;
}

export function DensityProvider({ 
  children, 
  pageKey, 
  defaultDensity = 'comfortable',
  persist = true 
}: DensityProviderProps) {
  const config: InformationDensityConfig = {
    key: `page-${pageKey}`,
    defaultDensity,
    persist
  };
  
  const densityHook = useInformationDensity(config);

  const value: DensityContextValue = {
    density: densityHook.density,
    setDensity: densityHook.setDensity,
    spacing: densityHook.spacing,
    textSizes: densityHook.textSizes,
    isCompact: densityHook.isCompact,
    isComfortable: densityHook.isComfortable,
    isSpacious: densityHook.isSpacious
  };

  return (
    <DensityContext.Provider value={value}>
      {children}
    </DensityContext.Provider>
  );
}

export function useDensity(): DensityContextValue {
  const context = useContext(DensityContext);
  if (!context) {
    throw new Error('useDensity must be used within DensityProvider');
  }
  return context;
}
```

**Citation References:**
- Hook implementation: 【ui/src/hooks/useInformationDensity.ts§1-122】
- Dashboard usage: 【ui/src/components/Dashboard.tsx§37-38】

#### Patch 2.2: Add Density Controls to TrainingWizard
**File:** `ui/src/components/TrainingWizard.tsx`  
**Lines:** 1-50 (add imports and controls)  
**Citation:** 【ui/src/components/TrainingWizard.tsx§1-981】

**Changes:**
```typescript
// Add to TrainingWizard component
import { DensityControls } from './ui/density-controls';
import { useDensity } from '@/contexts/DensityContext';

export function TrainingWizard({ onComplete, onCancel }: TrainingWizardProps) {
  const { density, setDensity, spacing, textSizes } = useDensity();
  
  // Apply density-aware spacing to all cards and sections
  return (
    <div className={spacing.sectionGap}>
      <div className="flex justify-between items-center mb-4">
        <h2 className={textSizes.title}>Training Wizard</h2>
        <DensityControls density={density} onDensityChange={setDensity} />
      </div>
      {/* Apply spacing.cardPadding, spacing.formFieldGap throughout */}
    </div>
  );
}
```

**Citation References:**
- Density controls component: 【ui/src/components/ui/density-controls.tsx§16-88】
- Spacing usage: 【ui/src/hooks/useInformationDensity.ts§33-76】

#### Patch 2.3: Add Density Controls to All Major Pages
**Files:**
- `ui/src/pages/TenantsPage.tsx`
- `ui/src/pages/AdaptersPage.tsx`
- `ui/src/pages/PoliciesPage.tsx`
- `ui/src/pages/MetricsPage.tsx`
- `ui/src/pages/TelemetryPage.tsx`
- `ui/src/pages/InferencePage.tsx`
- `ui/src/pages/AuditPage.tsx`

**Pattern:** Follow TrainingWizard pattern above, wrap each page in DensityProvider with unique pageKey.

**Citation References:**
- Page structure: 【ui/src/pages/InferencePage.tsx§1-15】
- Feature layout: 【ui/src/layout/FeatureLayout.tsx】

---

## Phase 3: Standardize Polling Patterns (P1)

### Issue
Polling intervals vary significantly: TrainingPage【ui/src/components/TrainingPage.tsx§38】uses 5s, ITAdminDashboard【ui/src/components/ITAdminDashboard.tsx§76】uses 30s, AlertsPage【ui/src/components/AlertsPage.tsx§132】uses 2s.

### Solution
Create standardized polling hook following existing patterns【ui/src/hooks/useActivityFeed.ts§172-294】with SSE fallback.

### Implementation Plan

#### Patch 3.1: Create Standardized Polling Hook
**File:** `ui/src/hooks/usePolling.ts` (NEW)  
**Citation:** 【ui/src/hooks/useActivityFeed.ts§172-294】

**Implementation:**
```typescript
// 【ui/src/hooks/useActivityFeed.ts§172-294】 - SSE + polling pattern
// 【ui/src/components/RealtimeMetrics.tsx§138-182】 - Metrics polling pattern
// 【ui/src/utils/logger.ts】 - Error handling pattern
import { useState, useEffect, useCallback, useRef } from 'react';

export interface PollingConfig {
  intervalMs?: number; // Override default interval
  enabled?: boolean;
  showLoadingIndicator?: boolean;
  onError?: (error: Error) => void;
  onSuccess?: (data: unknown) => void;
}

export type PollingSpeed = 'fast' | 'normal' | 'slow';

const POLLING_INTERVALS: Record<PollingSpeed, number> = {
  fast: 2000,    // Real-time updates (alerts, training progress)
  normal: 5000,  // Standard updates (metrics, dashboard)
  slow: 30000    // Background updates (system health, admin)
};

export interface UsePollingReturn<T> {
  data: T | null;
  isLoading: boolean;
  lastUpdated: Date | null;
  error: Error | null;
  refetch: () => Promise<void>;
}

export function usePolling<T>(
  fetchFn: () => Promise<T>,
  speed: PollingSpeed = 'normal',
  config?: PollingConfig
): UsePollingReturn<T> {
  const { 
    intervalMs = POLLING_INTERVALS[speed], 
    enabled = true, 
    showLoadingIndicator = false, 
    onError,
    onSuccess
  } = config || {};
  
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const mountedRef = useRef(true);

  const fetchData = useCallback(async () => {
    if (!mountedRef.current) return;
    
    try {
      if (showLoadingIndicator) setIsLoading(true);
      const result = await fetchFn();
      
      if (!mountedRef.current) return;
      
      setData(result);
      setLastUpdated(new Date());
      setError(null);
      onSuccess?.(result);
    } catch (err) {
      if (!mountedRef.current) return;
      
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      onError?.(error);
    } finally {
      if (mountedRef.current && showLoadingIndicator) {
        setIsLoading(false);
      }
    }
  }, [fetchFn, showLoadingIndicator, onError, onSuccess]);

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  useEffect(() => {
    mountedRef.current = true;
    
    if (!enabled) {
      setIsLoading(false);
      return;
    }

    // Initial fetch
    fetchData();

    // Set up polling interval
    intervalRef.current = setInterval(fetchData, intervalMs);

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [fetchData, intervalMs, enabled]);

  return { data, isLoading, lastUpdated, error, refetch };
}
```

**Citation References:**
- Activity feed polling: 【ui/src/hooks/useActivityFeed.ts§175-176】
- Real-time metrics: 【ui/src/components/RealtimeMetrics.tsx§171-172】
- Error handling pattern: 【ui/src/utils/logger.ts】

#### Patch 3.2: Refactor TrainingPage to Use Standard Polling
**File:** `ui/src/components/TrainingPage.tsx`  
**Lines:** 25-40  
**Citation:** 【ui/src/components/TrainingPage.tsx§25-40】

**Changes:**
```typescript
// Replace manual polling with standardized hook
import { usePolling } from '@/hooks/usePolling';

export function TrainingPage() {
  const { data: trainingJobs, isLoading, lastUpdated } = usePolling(
    () => apiClient.listTrainingJobs(),
    'fast', // Training progress needs frequent updates
    { showLoadingIndicator: true }
  );

  // Remove manual useEffect polling code
}
```

**Citation References:**
- Current polling: 【ui/src/components/TrainingPage.tsx§25-40】
- API client: 【ui/src/api/client.ts】

#### Patch 3.3: Add Last Updated Indicators
**File:** `ui/src/components/ui/last-updated.tsx` (NEW)  
**Citation:** 【ui/src/hooks/usePolling.ts】

**Implementation:**
```typescript
// 【ui/src/hooks/usePolling.ts】 - lastUpdated timestamp
import { Clock } from 'lucide-react';
import { useTimestamp } from '@/hooks/useTimestamp';

export function LastUpdated({ timestamp }: { timestamp: Date | null }) {
  const relativeTime = useTimestamp(timestamp);
  
  if (!timestamp) return null;
  
  return (
    <div className="flex items-center gap-1 text-xs text-muted-foreground">
      <Clock className="h-3 w-3" />
      <span>Updated {relativeTime}</span>
    </div>
  );
}
```

**Citation References:**
- Timestamp hook: 【ui/src/hooks/useTimestamp.ts】

---

## Phase 4: Error Recovery Consistency (P2)

### Issue
ErrorRecovery component【ui/src/components/ui/error-recovery.tsx§1-256】exists but many components still use basic `toast.error()` calls.

### Solution
Replace all error toasts with ErrorRecovery components following established patterns【ui/src/components/ui/error-recovery.tsx§154-253】.

### Implementation Plan

#### Patch 4.1: Create Global Error Boundary
**File:** `ui/src/components/ErrorBoundary.tsx` (UPDATE)  
**Citation:** 【ui/src/components/ErrorBoundary.tsx】

**Changes:**
```typescript
// 【ui/src/components/ui/error-recovery.tsx§236-252】 - Generic error template
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';

export class ErrorBoundary extends React.Component<Props, State> {
  // ... existing error boundary code ...
  
  render() {
    if (this.state.hasError) {
      return ErrorRecoveryTemplates.genericError(
        this.state.error,
        () => window.location.reload()
      );
    }
    return this.props.children;
  }
}
```

**Citation References:**
- Error recovery templates: 【ui/src/components/ui/error-recovery.tsx§154-253】
- Error boundary pattern: 【ui/src/components/ErrorBoundary.tsx】

#### Patch 4.2: Replace Toast Errors in TrainingPage
**File:** `ui/src/components/TrainingPage.tsx`  
**Lines:** 30-35  
**Citation:** 【ui/src/components/TrainingPage.tsx§30-35】

**Changes:**
```typescript
// Replace toast.error with ErrorRecovery
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';

// In catch blocks:
catch (err) {
  logger.error('Failed to fetch training jobs', { component: 'TrainingPage' }, toError(err));
  // Remove: toast.error('Failed to load training jobs');
  // Add: ErrorRecovery component in UI
  setError(err instanceof Error ? err : new Error(String(err)));
}

// In render:
{error && ErrorRecoveryTemplates.trainingError(
  () => fetchJobs(),
  () => setIsWizardOpen(true)
)}
```

**Citation References:**
- Training error template: 【ui/src/components/ui/error-recovery.tsx§189-204】
- Logger pattern: 【ui/src/utils/logger.ts】

#### Patch 4.3: Audit and Replace All Toast Errors
**Files to Update:**
- `ui/src/components/ITAdminDashboard.tsx` (Line 67)
- `ui/src/components/UserReportsPage.tsx` (Line 71)
- `ui/src/components/SingleFileAdapterTrainer.tsx` (multiple)
- `ui/src/components/InferencePlayground.tsx` (multiple)
- All other components using `toast.error()`

**Pattern:** Replace with appropriate ErrorRecoveryTemplates based on context.

**Citation References:**
- All error templates: 【ui/src/components/ui/error-recovery.tsx§154-253】

---

## Phase 5: Workflow Breadcrumb Navigation (P2)

### Issue
Multi-step workflows lack breadcrumb navigation. BreadcrumbContext【ui/src/contexts/BreadcrumbContext.tsx§1-64】exists but not integrated into workflows.

### Solution
Integrate breadcrumbs into all multi-step workflows following existing BreadcrumbNavigation pattern【ui/src/components/BreadcrumbNavigation.tsx§1-61】.

### Implementation Plan

#### Patch 5.1: Add Breadcrumbs to TrainingWizard
**File:** `ui/src/components/TrainingWizard.tsx`  
**Lines:** 1-30 (add breadcrumb integration)  
**Citation:** 【ui/src/components/TrainingWizard.tsx§1-981】

**Changes:**
```typescript
// 【ui/src/contexts/BreadcrumbContext.tsx§1-64】 - Breadcrumb context
// 【ui/src/components/BreadcrumbNavigation.tsx§1-61】 - Breadcrumb component
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import { BreadcrumbNavigation } from '@/components/BreadcrumbNavigation';
import { Zap, Settings, Database, Code } from 'lucide-react';

export function TrainingWizard({ onComplete, onCancel }: TrainingWizardProps) {
  const { setBreadcrumbs } = useBreadcrumb();
  
  useEffect(() => {
    const steps = [
      { id: 'category', label: 'Category', icon: Code },
      { id: 'info', label: 'Basic Info', icon: Settings },
      { id: 'data', label: 'Data Source', icon: Database },
      { id: 'config', label: 'Configuration', icon: Settings },
      { id: 'params', label: 'Parameters', icon: Zap },
      { id: 'package', label: 'Packaging', icon: Box }
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

**Citation References:**
- Breadcrumb context: 【ui/src/contexts/BreadcrumbContext.tsx§1-64】
- Breadcrumb component: 【ui/src/components/BreadcrumbNavigation.tsx§1-61】
- Breadcrumb UI: 【ui/src/components/ui/breadcrumb.tsx§1-110】

#### Patch 5.2: Add Breadcrumbs to SingleFileAdapterTrainer
**File:** `ui/src/components/SingleFileAdapterTrainer.tsx`  
**Lines:** 38-67  
**Citation:** 【ui/src/components/SingleFileAdapterTrainer.tsx§38-594】

**Changes:**
```typescript
// Add breadcrumb integration for 4-step flow
const steps = ['upload', 'configure', 'training', 'complete'];
const stepLabels = {
  upload: 'Upload File',
  configure: 'Configure Training',
  training: 'Training Progress',
  complete: 'Test & Download'
};

useEffect(() => {
  setBreadcrumbs([
    { id: 'upload', label: stepLabels.upload },
    { id: 'configure', label: stepLabels.configure },
    { id: 'training', label: stepLabels.training },
    { id: 'complete', label: stepLabels.complete }
  ].slice(0, stepIndex + 1));
}, [step, setBreadcrumbs]);
```

**Citation References:**
- Trainer steps: 【ui/src/components/SingleFileAdapterTrainer.tsx§30-67】
- Workflow pattern: 【ui/src/components/WorkflowWizard.tsx§24-31】

#### Patch 5.3: Add Breadcrumbs to FeatureLayout
**File:** `ui/src/layout/FeatureLayout.tsx`  
**Citation:** 【ui/src/layout/FeatureLayout.tsx】

**Changes:**
```typescript
// Add automatic breadcrumb generation for all feature pages
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import { useLocation } from 'react-router-dom';

export function FeatureLayout({ title, description, children }: Props) {
  const location = useLocation();
  const { setBreadcrumbs } = useBreadcrumb();
  
  useEffect(() => {
    // Auto-generate breadcrumbs from route path
    const pathSegments = location.pathname.split('/').filter(Boolean);
    const breadcrumbs = pathSegments.map((segment, index) => ({
      id: segment,
      label: segment.charAt(0).toUpperCase() + segment.slice(1),
      href: '/' + pathSegments.slice(0, index + 1).join('/')
    }));
    setBreadcrumbs(breadcrumbs);
  }, [location.pathname, setBreadcrumbs]);
  
  return (
    <div>
      <BreadcrumbNavigation />
      {/* Rest of layout */}
    </div>
  );
}
```

**Citation References:**
- Feature layout: 【ui/src/layout/FeatureLayout.tsx】
- Route structure: 【ui/src/main.tsx§252-312】

---

## Phase 6: Mobile Navigation Optimization (P2)

### Issue
Mobile navigation becomes complex with collapsible groups. Current implementation【ui/src/layout/RootLayout.tsx§197-247】works but needs simplification.

### Solution
Simplify mobile navigation to top-level categories only, following responsive patterns【ui/src/layout/RootLayout.tsx§196-250】.

### Implementation Plan

#### Patch 6.1: Create Mobile Navigation Component
**File:** `ui/src/components/MobileNavigation.tsx` (NEW)  
**Citation:** 【ui/src/layout/RootLayout.tsx§196-250】

**Implementation:**
```typescript
// 【ui/src/layout/RootLayout.tsx§196-250】 - Mobile sidebar pattern
// 【ui/src/layout/RootLayout.tsx§78-88】 - NavGroup interface
// Simplified mobile navigation - top-level categories only
import React from 'react';
import { Button } from './ui/button';
import type { UserRole } from '@/api/types';

interface NavItem {
  to: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

interface NavGroup {
  title: string;
  items: NavItem[];
  roles?: UserRole[];
}

interface MobileNavigationProps {
  groups: NavGroup[];
  onNavigate: (path: string) => void;
  userRole?: UserRole;
}

export function MobileNavigation({ groups, onNavigate, userRole }: MobileNavigationProps) {
  // Flatten navigation to top-level items only on mobile
  // Filter by role if specified
  const shouldShowGroup = (group: NavGroup): boolean => {
    if (!group.roles || group.roles.length === 0) return true;
    return userRole ? group.roles.includes(userRole) : false;
  };

  const mobileItems = groups
    .filter(shouldShowGroup)
    .flatMap(group => 
      group.items.map(item => ({ ...item, group: group.title }))
    );
  
  return (
    <div className="md:hidden space-y-1">
      {/* Simplified list without collapsible groups */}
      {mobileItems.map(item => {
        const Icon = item.icon;
        return (
          <Button
            key={item.to}
            variant="ghost"
            className="w-full justify-start h-12 px-4" // Minimum 44px touch target (WCAG 2.1)
            onClick={() => onNavigate(item.to)}
            aria-label={`Navigate to ${item.label}`}
          >
            <Icon className="h-5 w-5 mr-3 flex-shrink-0" />
            <span className="text-sm font-medium">{item.label}</span>
          </Button>
        );
      })}
    </div>
  );
}
```

**Citation References:**
- Mobile sidebar: 【ui/src/layout/RootLayout.tsx§197-247】
- Touch target standards: WCAG 2.1 (minimum 44x44px)

#### Patch 6.2: Update RootLayout for Mobile Optimization
**File:** `ui/src/layout/RootLayout.tsx`  
**Lines:** 196-250  
**Citation:** 【ui/src/layout/RootLayout.tsx§196-250】

**Changes:**
```typescript
// 【ui/src/components/ui/use-mobile.ts】 - Mobile detection hook
// 【ui/src/components/MobileNavigation.tsx】 - Mobile navigation component
import { useMobile } from '@/components/ui/use-mobile';
import { MobileNavigation } from '@/components/MobileNavigation';

export default function RootLayout() {
  const isMobile = useMobile();
  // ... existing code ...

  return (
    <div className="min-h-screen bg-background">
      {/* ... header code ... */}
      
      {/* Sidebar */}
      <div className={`fixed inset-y-0 left-0 z-50 w-64 transform ${isSidebarOpen ? 'translate-x-0' : '-translate-x-full'} transition-transform md:translate-x-0 md:static md:inset-auto md:w-64 md:shadow-none overflow-y-auto bg-background border-r`}>
        <div className="p-4 space-y-1">
          <Button className="md:hidden mb-4 w-full justify-start" variant="ghost" onClick={() => setIsSidebarOpen(false)} aria-label="Close menu">
            <X className="h-5 w-5 mr-2" />
            Close Menu
          </Button>
          
          {isMobile ? (
            <MobileNavigation 
              groups={navigationGroups.filter(shouldShowGroup)}
              onNavigate={(path) => {
                navigate(path);
                setIsSidebarOpen(false);
              }}
              userRole={user?.role}
            />
          ) : (
            // Existing desktop navigation with collapsible groups
            navigationGroups.filter(shouldShowGroup).map((group) => {
              const isCollapsed = collapsedGroups[group.title];
              return (
                <div key={group.title} className="mb-4">
                  {/* ... existing collapsible group code ... */}
                </div>
              );
            })
          )}
        </div>
      </div>
      
      {/* ... rest of layout ... */}
    </div>
  );
}
```

**Citation References:**
- Mobile detection: 【ui/src/components/ui/use-mobile.ts】
- Current navigation: 【ui/src/layout/RootLayout.tsx§196-250】

---

## Testing Strategy

### Unit Tests
**Files:** `ui/src/__tests__/`
- Navigation group filtering logic
- Polling hook interval management
- Breadcrumb context state management
- Density control persistence
- Error recovery component rendering

**Test Examples:**
```typescript
// 【ui/src/__tests__/ActivityFeed.integration.test.tsx】 - Integration test pattern
describe('usePolling', () => {
  it('should poll at correct intervals', async () => {
    // Test implementation
  });
  
  it('should handle errors gracefully', async () => {
    // Test implementation
  });
});

describe('DensityContext', () => {
  it('should persist density preferences', () => {
    // Test implementation
  });
});
```

**Citation:** 【ui/src/__tests__/ActivityFeed.integration.test.tsx】

### Integration Tests
**Files:** `ui/src/__tests__/`
- Complete user workflows with new navigation
- Error recovery paths
- Mobile navigation behavior
- Progressive disclosure across pages
- Breadcrumb navigation in workflows

**Test Scenarios:**
- Training wizard with breadcrumbs
- Error recovery in training flow
- Mobile navigation accessibility
- Density control persistence across pages

**Citation:** 【ui/src/__tests__/ActivityFeed.integration.test.tsx】 【ui/src/__tests__/Journeys.test.tsx】

### E2E Tests
**Framework:** Cypress/Playwright (per codebase standards)
- Complete ML pipeline workflow (train → test → deploy)
- Role-based navigation access
- Mobile responsive behavior
- Error recovery scenarios
- Progressive disclosure interactions

**Test Coverage:**
- All 6 user roles navigation flows
- Mobile vs desktop navigation
- Error recovery user paths
- Density control user interactions

**Citation:** Testing patterns from codebase standards, E2E framework per project requirements

---

## Implementation Timeline

### Sprint 1 (Weeks 1-2)
- ✅ Phase 1: Navigation IA refactor
- ✅ Phase 3: Standardize polling patterns
- ✅ Testing and validation

### Sprint 2 (Weeks 3-4)
- ✅ Phase 2: Progressive disclosure consistency
- ✅ Phase 4: Error recovery consistency
- ✅ Testing and validation

### Sprint 3 (Weeks 5-6)
- ✅ Phase 5: Workflow breadcrumbs
- ✅ Phase 6: Mobile optimization
- ✅ Testing and validation

### Sprint 4 (Weeks 7-8)
- ✅ Final testing and polish
- ✅ Documentation updates
- ✅ User acceptance testing

---

## Success Criteria

### Phase 1 Success
- [ ] Navigation reorganized into 6 user-centric groups
- [ ] All routes accessible via new navigation
- [ ] Role-based filtering works correctly
- [ ] Zero broken navigation links

### Phase 2 Success
- [ ] Density controls on all major pages
- [ ] Preferences persist per page
- [ ] Visual consistency across pages
- [ ] User can adjust density on any page

### Phase 3 Success
- [ ] Standardized polling hook implemented
- [ ] All components use standardized polling
- [ ] Last updated indicators visible
- [ ] Consistent refresh intervals

### Phase 4 Success
- [ ] Error boundary implemented
- [ ] All toast errors replaced with ErrorRecovery
- [ ] Consistent error UX across app
- [ ] Recovery actions work correctly

### Phase 5 Success
- [ ] Breadcrumbs in all multi-step workflows
- [ ] Breadcrumbs auto-generate for feature pages
- [ ] Navigation between steps works
- [ ] Visual breadcrumb indicators clear

### Phase 6 Success
- [ ] Mobile navigation simplified
- [ ] Touch targets meet WCAG standards
- [ ] Mobile UX improved
- [ ] Desktop navigation unchanged

---

## Risk Mitigation

### Risk: Breaking Existing Navigation
**Mitigation:** Implement feature flag for new navigation, allow rollback  
**Citation:** Feature flag patterns from codebase

### Risk: Performance Impact of Polling
**Mitigation:** Use SSE where possible, optimize polling intervals  
**Citation:** 【ui/src/hooks/useActivityFeed.ts§172-294】

### Risk: Mobile UX Regression
**Mitigation:** Extensive mobile testing, gradual rollout  
**Citation:** Mobile testing standards

---

## Citations Summary

### Key Code References
- Navigation: 【ui/src/layout/RootLayout.tsx§90-141】
- Density controls: 【ui/src/hooks/useInformationDensity.ts§1-122】
- Polling patterns: 【ui/src/hooks/useActivityFeed.ts§172-294】
- Error recovery: 【ui/src/components/ui/error-recovery.tsx§1-256】
- Breadcrumbs: 【ui/src/contexts/BreadcrumbContext.tsx§1-64】
- Workflow patterns: 【ui/src/components/WorkflowWizard.tsx§40-251】

### Component Patterns
- Dashboard density: 【ui/src/components/Dashboard.tsx§37-38】
- Training polling: 【ui/src/components/TrainingPage.tsx§25-40】
- Error templates: 【ui/src/components/ui/error-recovery.tsx§154-253】
- Mobile sidebar: 【ui/src/layout/RootLayout.tsx§196-250】

---

## Appendix: File Change Summary

### New Files
- `ui/src/contexts/DensityContext.tsx`
- `ui/src/hooks/usePolling.ts`
- `ui/src/components/ui/last-updated.tsx`
- `ui/src/components/MobileNavigation.tsx`

### Modified Files
- `ui/src/layout/RootLayout.tsx` (navigation refactor)
- `ui/src/components/TrainingWizard.tsx` (density + breadcrumbs)
- `ui/src/components/TrainingPage.tsx` (polling + errors)
- `ui/src/components/SingleFileAdapterTrainer.tsx` (breadcrumbs)
- `ui/src/components/ErrorBoundary.tsx` (error recovery)
- `ui/src/layout/FeatureLayout.tsx` (breadcrumbs)
- All page components (density controls)
- All components with toast errors (error recovery)

### Estimated Changes
- **New Files:** 4
- **Modified Files:** 20+
- **Lines Added:** ~2000
- **Lines Removed:** ~500
- **Net Change:** +1500 lines

---

**Document Status:** Ready for Implementation  
**Next Review:** After Sprint 1 completion  
**Owner:** UX Team  
**Stakeholders:** Product, Engineering, Design

