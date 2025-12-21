# System Pages Implementation

## Overview

Complete implementation of System management pages for the AdapterOS UI. The system provides comprehensive monitoring and management of nodes, workers, memory, and system metrics.

## Files Created

### Main Pages
1. **SystemPage.tsx** - Main container with tab navigation
2. **SystemOverviewTab.tsx** - Dashboard with health cards and infrastructure summary
3. **NodesTab.tsx** - Node management interface
4. **WorkersTab.tsx** - Worker process management interface
5. **MemoryTab.tsx** - Memory usage visualization and management
6. **MetricsTab.tsx** - Real-time metrics dashboard with charts

### Components
7. **NodeTable.tsx** - DataTable for nodes with actions (ping, offline, evict)
8. **NodeDetailModal.tsx** - Detailed node information modal
9. **WorkerTable.tsx** - DataTable for workers with actions (view logs, debug, stop)
10. **WorkerLogsModal.tsx** - Worker logs, details, and crash information viewer

### Hooks
11. **useSystem.ts** - Re-export of system hooks for centralized imports

### Index
12. **index.ts** - Module exports for easy importing

## Features

### SystemPage
- Tab-based navigation (Overview, Nodes, Workers, Memory, Metrics)
- Integrated with FeatureLayout and DensityProvider
- Clean, responsive design

### SystemOverviewTab
- Real-time system health metrics (CPU, Memory, Disk, GPU)
- Color-coded health badges (healthy/warning/critical)
- Performance metrics (Active Adapters, Sessions, Tokens/sec, Latency)
- Infrastructure summary cards for Nodes and Workers
- Auto-refreshing data with last updated timestamp

### NodesTab
- Full node listing with DataTable
- Node status badges (healthy/offline/error)
- Actions: Ping, Mark Offline, Evict
- Click-through to detailed node view
- Real-time refresh capability

### NodeTable
- Columns: Node ID, Hostname, Status, Memory, GPUs, Metal Family, Last Heartbeat
- Dropdown menu for node actions
- Confirmation dialogs for destructive actions
- Toast notifications for operation feedback

### NodeDetailModal
- Basic information (ID, hostname, status, last seen)
- Hardware details (memory, GPU count/type, Metal family)
- Worker processes running on the node
- Status badges for workers

### WorkersTab
- Full worker listing with DataTable
- Worker status tracking
- Actions: View Logs, Stop, Debug
- Real-time refresh capability

### WorkerTable
- Columns: Worker ID, Status, Type, Node ID, Tenant, Plan, Memory, CPU, Created
- Dropdown menu for worker actions
- Confirmation dialogs for stop operations
- Toast notifications for operation feedback

### WorkerLogsModal
- Three-tab interface: Details, Logs, Crashes
- **Details Tab**: Worker metadata (ID, status, type, node, tenant, plan, PID, memory, CPU, uptime)
- **Logs Tab**: Scrollable log viewer with timestamp, level badges, and messages
- **Crashes Tab**: Crash reports with stack traces, exit codes, signals, and recovery actions
- Real-time log streaming
- Color-coded log levels (error/warn/info/debug)

### MemoryTab
- Memory usage overview cards (Total, Available, Pressure Level)
- Memory usage progress bar and chart
- Pressure level badges (low/medium/high/critical)
- Adapter memory table with eviction capability
- Columns: Adapter ID, Memory Usage, Last Accessed
- Evict adapter action with confirmation

### MetricsTab
- Real-time charts using recharts:
  - System Resource Usage (CPU, Memory, Disk, GPU) - Line Chart
  - Performance Metrics (Tokens/sec, Latency) - Area Chart
- Historical data collection (last 20 data points)
- Comprehensive metrics grid:
  - CPU Usage
  - Memory Usage (with used/total)
  - GPU Usage (with VRAM used/total)
  - Disk Usage
  - Network RX/TX (in MB)
  - Tokens/sec
  - Latency (P95)
  - CPU/GPU Temperature
  - GPU Power consumption
  - Cache Hit Rate
  - Error Rate
  - Active Adapters/Sessions
  - Disk Read/Write speed
- Auto-refresh every few seconds (fast polling)
- Last updated badge

## API Integration

All components use the following API methods from `/Users/star/Dev/aos/ui/src/api/client.ts`:

### Nodes
- `listNodes()` - Get all nodes
- `getNodeDetails(nodeId)` - Get detailed node info
- `testNodeConnection(nodeId)` - Ping node
- `markNodeOffline(nodeId)` - Mark node as offline
- `evictNode(nodeId)` - Remove node from cluster

### Workers
- `listWorkers(tenantId?, nodeId?)` - Get all workers
- `getWorkerDetails(workerId)` - Get detailed worker info
- `stopWorker(workerId, force)` - Stop a worker process
- `getProcessLogs(workerId, filters?)` - Get worker logs
- `getProcessCrashes(workerId)` - Get worker crash history

### Memory
- `getMemoryUsage()` - Get memory usage and adapter list
- `evictAdapter(adapterId)` - Evict adapter from memory

### Metrics
- `getSystemMetrics()` - Get current system metrics
- `getMetricsSnapshot()` - Get metrics snapshot

## Hooks Used

From `/Users/star/Dev/aos/ui/src/hooks/useSystemMetrics.ts`:

- `useSystemMetrics(speed, enabled)` - System metrics with polling
- `useNodes(speed, enabled)` - Node list with polling
- `useNodeDetails(nodeId, enabled)` - Node details with polling
- `useNodeOperations()` - Node mutation operations
- `useWorkers(tenantId, nodeId, speed, enabled)` - Worker list with polling
- `useWorkerDetails(workerId, enabled)` - Worker details with polling
- `useWorkerLogs(workerId, filters, enabled)` - Worker logs with polling
- `useWorkerCrashes(workerId, enabled)` - Worker crashes with polling
- `useWorkerOperations()` - Worker mutation operations
- `useMemoryUsage(speed, enabled)` - Memory usage with polling
- `useMemoryOperations()` - Memory mutation operations
- `useComputedMetrics(metrics)` - Computed metric values
- `useSystemHealthStatus(metrics)` - Overall health status

## Polling Speeds

- **fast** - High frequency updates (e.g., metrics charts)
- **normal** - Standard polling (e.g., tables, overview)
- **slow** - Infrequent updates (e.g., node/worker counts)

## Type Safety

All components are fully typed using TypeScript interfaces from:
- `/Users/star/Dev/aos/ui/src/api/api-types.ts`

Key types:
- `Node` - Node information
- `NodeDetailsResponse` - Detailed node data
- `NodePingResponse` - Ping result
- `WorkerResponse` - Worker information
- `WorkerDetailsResponse` - Detailed worker data
- `ProcessLog` - Log entry
- `ProcessCrash` - Crash report
- `SystemMetrics` - System metrics
- `MemoryUsage` - Memory information

## UI Components

Uses shadcn/ui components:
- `Card`, `CardContent`, `CardDescription`, `CardHeader`, `CardTitle`
- `Badge` (with variants: success, warning, destructive, secondary)
- `Button`
- `Progress`
- `Skeleton` (loading states)
- `Dialog`, `DialogContent`, `DialogDescription`, `DialogHeader`, `DialogTitle`
- `Tabs`, `TabsContent`, `TabsList`, `TabsTrigger`
- `ScrollArea`
- `DropdownMenu`, `DropdownMenuContent`, `DropdownMenuItem`, `DropdownMenuTrigger`
- `DataTable` (from `/Users/star/Dev/aos/ui/src/components/shared/DataTable/DataTable.tsx`)

## Charts

Uses recharts library (v2.15.2):
- `LineChart`, `Line` - For resource usage trends
- `AreaChart`, `Area` - For performance metrics
- `XAxis`, `YAxis`, `CartesianGrid`, `Tooltip`, `Legend`
- `ResponsiveContainer` - For responsive sizing

## Notifications

Uses toast notifications via `useToast()` hook for:
- Operation success messages
- Error handling
- Action confirmations

## Routing

To integrate into app routing, add to your router configuration:

```typescript
import { SystemPage } from '@/pages/System';

// In your routes:
{
  path: '/system',
  element: <SystemPage />,
}
```

Or use individual tabs:

```typescript
import { NodesTab, WorkersTab, MemoryTab, MetricsTab } from '@/pages/System';
```

## Error Handling

All components include:
- Error state rendering
- Toast notifications for failures
- Confirmation dialogs for destructive actions
- Loading skeletons
- Graceful fallbacks for missing data

## Responsive Design

- Mobile-friendly grid layouts
- Responsive card grids (1 col mobile, 2-4 cols desktop)
- Scrollable tables and logs
- Adaptive chart sizing

## Real-time Updates

- Auto-polling with configurable speeds
- Last updated timestamps
- Manual refresh buttons
- Circuit breaker pattern for failed requests

## Security

- Confirmation dialogs for destructive operations (evict node, stop worker, evict adapter)
- Permission-aware operations (uses existing auth system)

## Performance

- Efficient polling with usePolling hook
- Circuit breaker for failed requests
- Skeleton loading states for better UX
- Memoized computed values
- Optimized re-renders with useMemo

## Accessibility

- Semantic HTML
- ARIA labels via shadcn/ui components
- Keyboard navigation support
- Screen reader friendly badges and status indicators

## Future Enhancements

Potential additions:
- Real-time streaming via `/v1/stream/metrics` SSE endpoint
- Metrics export (CSV/JSON)
- Custom metric dashboards
- Alert configuration UI
- Historical metrics persistence
- Metric comparison views
- Custom time ranges for charts

## Testing Recommendations

Test scenarios:
1. Node ping and eviction
2. Worker stop and log viewing
3. Memory pressure levels and adapter eviction
4. Real-time metric updates
5. Error handling for failed API calls
6. Responsive layout on different screen sizes
7. Table sorting and filtering
8. Modal interactions

## Copyright

© 2025 JKCA / James KC Auchterlonie. All rights reserved.
