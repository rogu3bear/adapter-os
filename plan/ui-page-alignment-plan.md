# UI Page Alignment Plan

## Goal
Align all UI pages to use consistent patterns identified in the codebase analysis.

## Work Streams

### Stream A: FeatureLayout → PageWrapper Migrations (6 pages)
Each page needs to be migrated from FeatureLayout to PageWrapper for consistency.

| Task | File | Changes |
|------|------|---------|
| A1 | TelemetryShell.tsx | Replace FeatureLayout with PageWrapper, add pageKey |
| A2 | InferencePage.tsx | Replace FeatureLayout with PageWrapper, add pageKey |
| A3 | TrainingJobsPage.tsx | Replace FeatureLayout with PageWrapper, remove redundant DensityProvider |
| A4 | CreateAdapterPage.tsx | Replace FeatureLayout with PageWrapper, remove redundant DensityProvider |
| A5 | GuidedFlowPage.tsx | Replace FeatureLayout with PageWrapper, add pageKey |
| A6 | TestingPage.tsx | Replace FeatureLayout with PageWrapper, add pageKey |

### Stream B: TrainingJobDetail React Query Migration (1 page, complex)
| Task | File | Changes |
|------|------|---------|
| B1 | TrainingJobDetail.tsx | Replace useState+fetch with useQuery for job data |
| B2 | TrainingJobDetail.tsx | Replace useState+fetch with useQuery for logs |
| B3 | TrainingJobDetail.tsx | Replace useState+fetch with useQuery for metrics |
| B4 | TrainingJobDetail.tsx | Replace useState+fetch with useQuery for artifacts |

### Stream C: TelemetryShell Inline Tab Extraction (3 components)
| Task | File | Changes |
|------|------|---------|
| C1 | TelemetryAlertsTab.tsx | Extract inline TelemetryAlertsTab component to new file |
| C2 | TelemetryExportsTab.tsx | Extract inline TelemetryExportsTab component to new file |
| C3 | TelemetryFiltersTab.tsx | Extract inline TelemetryFiltersTab component to new file |

### Stream D: Hook Extractions (2 hooks)
| Task | File | Changes |
|------|------|---------|
| D1 | useVersionGuards.ts | Extract computeVersionGuards from RepoDetailPage to hook |
| D2 | Update RepoDetailPage | Use new useVersionGuards hook |

### Stream E: ReplayShell URL Param Migration (1 page)
| Task | File | Changes |
|------|------|---------|
| E1 | ReplayShell.tsx | Migrate ?sessionId query param to :sessionId URL param |
| E2 | routes.ts | Update replay routes to include :sessionId param |

### Stream F: Shared Table Components (investigation + implementation)
| Task | File | Changes |
|------|------|---------|
| F1 | Investigate existing table patterns | Document shared table approach |
| F2 | RepositoriesPage.tsx | Consider migrating to shared table if beneficial |

## Implementation Order
1. Streams A, C, D can run fully in parallel (no dependencies)
2. Stream B can run in parallel (isolated to one file)
3. Stream E requires route changes (coordinate with routes.ts)
4. Stream F is lower priority investigation

## Success Criteria
- All pages use PageWrapper (not raw FeatureLayout)
- No redundant DensityProvider wraps
- TrainingJobDetail uses React Query
- TelemetryShell has extracted tab components
- All changes pass existing tests
