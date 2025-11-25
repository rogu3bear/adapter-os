# Dashboard System

Role-based dashboard system for AdapterOS UI with configurable widgets and quick actions.

## Overview

The Dashboard system provides role-specific views aligned with AdapterOS RBAC permissions:

- **Admin:** Full system management with tenant, adapter, policy, and node oversight
- **Operator:** Runtime operations (adapter loading, training, inference execution)
- **SRE:** Infrastructure monitoring and performance diagnostics
- **Compliance:** Audit trails, policy validation, and compliance reporting
- **Viewer:** Read-only dashboard with system metrics and adapter status

## Architecture

```
Dashboard/
├── index.tsx                     # Main dashboard router
├── DashboardProvider.tsx         # Shared state and refresh logic
├── DashboardLayout.tsx           # Common layout wrapper
├── config/
│   └── roleConfigs.ts           # Widget and quick action definitions
├── roles/
│   ├── AdminDashboard.tsx       # Admin role dashboard
│   ├── OperatorDashboard.tsx    # Operator role dashboard
│   ├── SREDashboard.tsx         # SRE role dashboard
│   ├── ComplianceDashboard.tsx  # Compliance role dashboard
│   └── ViewerDashboard.tsx      # Viewer role dashboard
└── [Widget Components].tsx      # Individual widget implementations
```

## Role Configuration

Role dashboards are defined in `/config/roleConfigs.ts`:

```typescript
export interface RoleDashboardConfig {
  role: string;                // RBAC role name
  title: string;               // Dashboard title
  displayName: string;         // Human-readable role name
  description: string;         // Role description
  defaultRoute: string;        // Default route for role
  widgets: WidgetConfig[];     // Widget configuration
  quickActions: QuickAction[]; // Quick action buttons
}
```

### Widget Configuration

```typescript
export interface WidgetConfig {
  id: string;                  // Unique widget ID
  title: string;               // Widget title
  description: string;         // Widget description
  component: string;           // Component name (e.g., 'AdapterStatsWidget')
  defaultSize?: 'small' | 'medium' | 'large'; // Default size
  position?: { row: number; col: number };    // Grid position
  permissions?: string[];      // Required permissions
  ariaLabel?: string;          // Accessibility label
}
```

### Quick Action Configuration

```typescript
export interface QuickAction {
  id: string;                  // Unique action ID
  label: string;               // Button label
  variant: 'primary' | 'secondary' | 'danger'; // Button style
  action: string;              // Route or handler
  icon?: string;               // Icon name
  permissions?: string[];      // Required permissions
  description?: string;        // Action description
  ariaLabel?: string;          // Accessibility label
}
```

## Usage

### Basic Dashboard Setup

```tsx
import { DashboardProvider } from '@/components/Dashboard/DashboardProvider';
import Dashboard from '@/components/Dashboard';

function App() {
  return (
    <DashboardProvider>
      <Dashboard />
    </DashboardProvider>
  );
}
```

### Role-Specific Dashboard

```tsx
import AdminDashboard from '@/components/Dashboard/roles/AdminDashboard';

function AdminPage() {
  return <AdminDashboard />;
}
```

### Using Dashboard Context

```tsx
import { useDashboard } from '@/components/Dashboard/DashboardProvider';

function MyWidget() {
  const { refreshInterval, triggerRefresh, isRefreshing } = useDashboard();

  return (
    <div>
      <button onClick={triggerRefresh} disabled={isRefreshing}>
        Refresh
      </button>
      <p>Auto-refresh every {refreshInterval}ms</p>
    </div>
  );
}
```

## Adding New Widgets

### 1. Create Widget Component

```tsx
// Dashboard/MyNewWidget.tsx
import React from 'react';
import { useDashboard } from './DashboardProvider';

export default function MyNewWidget() {
  const { isRefreshing } = useDashboard();

  return (
    <div className="bg-white rounded-lg shadow p-4">
      <h3 className="text-lg font-semibold mb-4">My New Widget</h3>
      {isRefreshing ? (
        <p>Refreshing...</p>
      ) : (
        <div>{/* Widget content */}</div>
      )}
    </div>
  );
}
```

### 2. Add Widget to Role Config

```typescript
// config/roleConfigs.ts
export const adminConfig: RoleDashboardConfig = {
  // ... existing config
  widgets: [
    // ... existing widgets
    {
      id: 'my-new-widget',
      title: 'My New Widget',
      description: 'Description of new widget',
      component: 'MyNewWidget',
      defaultSize: 'medium',
      position: { row: 2, col: 0 },
      permissions: ['AdapterView'], // Optional
      ariaLabel: 'My new widget showing custom metrics',
    },
  ],
};
```

### 3. Import Widget in Role Dashboard

```tsx
// roles/AdminDashboard.tsx
import MyNewWidget from '../MyNewWidget';

export default function AdminDashboard() {
  const config = adminConfig;

  return (
    <DashboardLayout title={config.title} quickActions={<QuickActions />}>
      {/* ... existing widgets */}
      <MyNewWidget />
    </DashboardLayout>
  );
}
```

## Customizing Per Role

### Add Role-Specific Widget

```typescript
// config/roleConfigs.ts

// Add widget only to SRE dashboard
export const sreConfig: RoleDashboardConfig = {
  // ... existing config
  widgets: [
    {
      id: 'performance-profiler',
      title: 'Performance Profiler',
      description: 'Detailed performance metrics and bottleneck analysis',
      component: 'PerformanceProfilerWidget',
      defaultSize: 'large',
      permissions: ['MetricsView'], // SRE has this permission
    },
  ],
};
```

### Add Role-Specific Quick Action

```typescript
// config/roleConfigs.ts

// Add quick action only to Operator dashboard
export const operatorConfig: RoleDashboardConfig = {
  // ... existing config
  quickActions: [
    {
      id: 'load-adapter',
      label: 'Load Adapter',
      variant: 'primary',
      action: '/adapters/load',
      icon: 'upload',
      permissions: ['AdapterLoad'],
      description: 'Load adapter into runtime',
      ariaLabel: 'Navigate to adapter loading page',
    },
  ],
};
```

### Conditional Widget Rendering

```tsx
// Inside role dashboard component
import { useAuth } from '@/providers/CoreProviders';

export default function AdminDashboard() {
  const { user } = useAuth();

  return (
    <DashboardLayout title="Admin Dashboard">
      {/* Always visible */}
      <SystemOverviewWidget />

      {/* Conditionally rendered based on permission */}
      {user?.permissions?.includes('TenantManage') && (
        <TenantSummaryWidget />
      )}

      {/* Conditionally rendered based on role */}
      {user?.role === 'admin' && (
        <PolicyComplianceWidget />
      )}
    </DashboardLayout>
  );
}
```

## Widget Components

### Available Widgets

| Widget | Purpose | Required Permissions |
|--------|---------|---------------------|
| **SystemOverviewWidget** | System metrics and health | MetricsView |
| **AdapterStatsWidget** | Adapter registry stats | AdapterList |
| **TrainingJobsWidget** | Active training jobs | TrainingView |
| **PolicyComplianceWidget** | Policy enforcement status | PolicyView |
| **ActiveAlertsWidget** | System alerts | MetricsView |
| **ActivityFeedWidget** | Recent system activity | AuditView |
| **ComplianceScoreWidget** | Compliance score | PolicyValidate |
| **BaseModelWidget** | Base model information | ModelView |
| **MLPipelineWidget** | ML pipeline status | TrainingView |
| **MultiModelStatusWidget** | Multi-model status | ModelView |
| **NextStepsWidget** | Recommended actions | None |
| **PluginStatusWidget** | Plugin status | PluginView |
| **ReportingSummaryWidget** | Reporting summary | AuditView |
| **ServiceStatusWidget** | Service status | NodeView |

### Widget Styling

Widgets use consistent styling:

```tsx
<div className="bg-white rounded-lg shadow p-4">
  <h3 className="text-lg font-semibold mb-4">{title}</h3>
  <div className="space-y-2">
    {/* Widget content */}
  </div>
</div>
```

**Standard classes:**
- Container: `bg-white rounded-lg shadow p-4`
- Heading: `text-lg font-semibold mb-4`
- Content: `space-y-2` or `space-y-4`
- Loading: `animate-pulse bg-slate-200 h-4 rounded`
- Error: `text-red-600 text-sm`

## Dashboard Context API

The `DashboardProvider` provides shared state:

```typescript
interface DashboardContextValue {
  refreshInterval: number;          // Auto-refresh interval (ms)
  setRefreshInterval: (ms: number) => void;
  isRefreshing: boolean;            // Currently refreshing
  triggerRefresh: () => void;       // Manual refresh trigger
  lastRefresh: Date | null;         // Last refresh timestamp
}
```

**Example usage:**

```tsx
function MyWidget() {
  const {
    refreshInterval,
    setRefreshInterval,
    triggerRefresh,
    isRefreshing
  } = useDashboard();

  return (
    <div>
      <button onClick={triggerRefresh} disabled={isRefreshing}>
        {isRefreshing ? 'Refreshing...' : 'Refresh'}
      </button>
      <select
        value={refreshInterval}
        onChange={(e) => setRefreshInterval(Number(e.target.value))}
      >
        <option value={10000}>10s</option>
        <option value={30000}>30s</option>
        <option value={60000}>60s</option>
      </select>
    </div>
  );
}
```

## Layout System

### DashboardLayout Component

Provides consistent header, quick actions, and content area:

```tsx
<DashboardLayout
  title="Admin Dashboard"
  quickActions={
    <>
      <Button variant="primary" onClick={handleAction}>Action</Button>
      <Button variant="secondary" onClick={handleSecondary}>Secondary</Button>
    </>
  }
>
  {/* Dashboard widgets */}
</DashboardLayout>
```

**Features:**
- Responsive header with role display
- Quick actions navigation area
- Skip links for accessibility
- Semantic HTML landmarks
- Mobile-responsive layout

### Grid Layout

Widgets use CSS Grid for responsive layouts:

```tsx
<div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
  <div className="col-span-1 md:col-span-2">
    <LargeWidget />
  </div>
  <div className="col-span-1">
    <SmallWidget />
  </div>
</div>
```

## Accessibility

All dashboards follow accessibility best practices:

- **Semantic HTML:** Use `<header>`, `<main>`, `<nav>`, `<section>`
- **ARIA Labels:** All interactive elements have `aria-label`
- **Skip Links:** Keyboard navigation skip links to main content
- **Live Regions:** `aria-live="polite"` for dynamic updates
- **Focus Management:** Proper tab order and focus indicators
- **Color Contrast:** WCAG AA compliant color combinations

**Example:**

```tsx
<button
  onClick={handleRefresh}
  aria-label="Refresh dashboard widgets"
  aria-busy={isRefreshing}
>
  Refresh
</button>
```

## Performance Tips

1. **Lazy Loading:** Load widgets on demand
   ```tsx
   const HeavyWidget = lazy(() => import('./HeavyWidget'));
   ```

2. **Memoization:** Prevent unnecessary re-renders
   ```tsx
   const MemoizedWidget = React.memo(MyWidget);
   ```

3. **Debounced Refresh:** Throttle refresh triggers
   ```tsx
   const debouncedRefresh = useMemo(
     () => debounce(triggerRefresh, 1000),
     [triggerRefresh]
   );
   ```

4. **Conditional Polling:** Only poll when dashboard visible
   ```tsx
   useEffect(() => {
     if (!document.hidden) {
       const interval = setInterval(fetchData, refreshInterval);
       return () => clearInterval(interval);
     }
   }, [document.hidden, refreshInterval]);
   ```

## Testing

### Unit Tests

```tsx
import { render, screen } from '@testing-library/react';
import { DashboardProvider } from './DashboardProvider';
import MyWidget from './MyWidget';

describe('MyWidget', () => {
  it('renders widget content', () => {
    render(
      <DashboardProvider>
        <MyWidget />
      </DashboardProvider>
    );
    expect(screen.getByText('My Widget')).toBeInTheDocument();
  });

  it('refreshes on button click', async () => {
    render(
      <DashboardProvider>
        <MyWidget />
      </DashboardProvider>
    );
    const refreshBtn = screen.getByRole('button', { name: /refresh/i });
    fireEvent.click(refreshBtn);
    expect(screen.getByText(/refreshing/i)).toBeInTheDocument();
  });
});
```

### Integration Tests

```tsx
describe('AdminDashboard', () => {
  it('shows all admin widgets', () => {
    render(
      <DashboardProvider>
        <AdminDashboard />
      </DashboardProvider>
    );
    expect(screen.getByText('System Overview')).toBeInTheDocument();
    expect(screen.getByText('Tenant Summary')).toBeInTheDocument();
    expect(screen.getByText('Policy Compliance')).toBeInTheDocument();
  });

  it('respects permissions', () => {
    const mockUser = { role: 'admin', permissions: ['AdapterView'] };
    render(
      <AuthProvider value={{ user: mockUser }}>
        <DashboardProvider>
          <AdminDashboard />
        </DashboardProvider>
      </AuthProvider>
    );
    // Verify only permitted widgets are shown
  });
});
```

## Migration Guide

### From Legacy Dashboard

1. **Replace old dashboard imports:**
   ```tsx
   // OLD
   import Dashboard from './OldDashboard';

   // NEW
   import Dashboard from '@/components/Dashboard';
   import { DashboardProvider } from '@/components/Dashboard/DashboardProvider';
   ```

2. **Wrap with DashboardProvider:**
   ```tsx
   <DashboardProvider>
     <Dashboard />
   </DashboardProvider>
   ```

3. **Update widget components:**
   - Use new `useDashboard()` hook instead of props
   - Follow new styling conventions
   - Add accessibility attributes

4. **Update role configuration:**
   - Move widget definitions to `roleConfigs.ts`
   - Add permissions to widget config
   - Define quick actions

## Related Files

- **RBAC Matrix:** [CLAUDE.md](../../../CLAUDE.md) - Permission definitions
- **Routes:** [ui/src/config/routes.ts](../../config/routes.ts) - Route configuration
- **Auth Provider:** [ui/src/providers/CoreProviders.tsx](../../providers/CoreProviders.tsx) - Auth context
- **API Hooks:** [ui/src/hooks/](../../hooks/) - Data fetching hooks

## References

- [CLAUDE.md](../../../CLAUDE.md) - RBAC roles and permissions
- [docs/RBAC.md](../../../docs/RBAC.md) - Complete permission matrix
- [docs/UI_INTEGRATION.md](../../../docs/UI_INTEGRATION.md) - UI architecture patterns
