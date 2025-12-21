# Component Export Structure

This document describes the component export structure for the AdapterOS UI.

## Main Component Index (`/ui/src/components/index.ts`)

The main index file exports all major components for easy importing throughout the application.

### Dashboard Components
- `Dashboard` (default) - Role-based dashboard router
- `DashboardLayout` - Dashboard layout wrapper
- `DashboardProvider`, `useDashboard` - Dashboard context provider and hook
- `AdminDashboard` - Admin role dashboard
- `OperatorDashboard` - Operator role dashboard
- `SREDashboard` - SRE role dashboard
- `ComplianceDashboard` - Compliance role dashboard
- `ViewerDashboard` - Viewer role dashboard
- `roleConfigs` - Dashboard configuration for all roles

### Policy Components
- `PolicyPreflightDialog` - Pre-flight policy validation dialog

### Document Components
- `PDFViewer` - Full-featured PDF viewer
- `PDFViewerEmbedded` - Embedded PDF viewer
- `PDFViewerEmbeddedRef` (type) - Reference type for embedded viewer
- `DocumentChatLayout` - Document with chat interface layout

### Chat Components
- `EvidencePanel` - Evidence display panel
- `EvidenceItem` - Individual evidence item
- `ProofBadge` - Proof/verification badge
- `ChatMessage` (type) - Chat message type
- `ChatMessageComponent` - Chat message component
- `RouterActivitySidebar` - Router activity sidebar
- `RouterDetailsModal` - Router details modal
- `RouterIndicator` - Router status indicator
- `ChatErrorBoundary` - Error boundary for chat
- `RouterTechnicalView` - Technical router decision view
- `RouterSummaryView` - Summary router decision view
- `RouterDecisionSummary` (type) - Router decision summary type

### Legacy Components
- `LegacyDashboard` - Original dashboard component (will be deprecated)

## Sub-Index Files

### `/ui/src/components/dashboard/index.tsx`
Exports all dashboard-related components:
- Default `Dashboard` component (role router)
- All role-based dashboards
- `DashboardLayout`
- `DashboardProvider`, `useDashboard`
- `roleConfigs`

### `/ui/src/components/documents/index.ts`
Exports all document-related components:
- `PDFViewer`
- `PDFViewerEmbedded`
- `PDFViewerEmbeddedRef` (type)
- `DocumentChatLayout`

### `/ui/src/components/chat/index.ts`
Exports all chat-related components:
- `EvidencePanel`, `EvidenceItem`, `ProofBadge`
- `ChatMessage` (type), `ChatMessageComponent`
- `RouterActivitySidebar`, `RouterDetailsModal`, `RouterIndicator`
- `ChatErrorBoundary`
- `RouterTechnicalView`, `RouterSummaryView`
- `RouterDecisionSummary` (type)

## Usage Examples

### Importing Dashboard Components
```typescript
// From main index
import { Dashboard, DashboardLayout, AdminDashboard } from '@/components';

// From dashboard index
import { Dashboard, roleConfigs } from '@/components/dashboard';
```

### Importing Document Components
```typescript
// From main index
import { PDFViewerEmbedded, DocumentChatLayout } from '@/components';

// From documents index
import { PDFViewerEmbedded } from '@/components/documents';
```

### Importing Chat Components
```typescript
// From main index
import { RouterSummaryView, EvidencePanel } from '@/components';

// From chat index
import { RouterSummaryView } from '@/components/chat';
```

## Maintenance Notes

When adding new components:
1. Create the component in the appropriate directory
2. Export from the sub-index file (e.g., `dashboard/index.tsx`)
3. Re-export from main index (`components/index.ts`)
4. Update this documentation

When deprecating components:
1. Mark as deprecated in JSDoc
2. Add to "Legacy Components" section
3. Create migration guide if needed
4. Remove after 2-3 version cycles
