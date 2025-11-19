# Action History System Documentation

## Overview

The AdapterOS action history system provides comprehensive tracking, management, and analysis of all user operations. It includes:

- **Action Tracking**: Record all create, update, delete, load, and other operations
- **Undo/Redo**: Reversible operations with keyboard shortcuts (Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z)
- **Filtering & Search**: Find actions by type, resource, date, status, and keywords
- **Export**: JSON, CSV, Markdown formats with multiple scopes
- **Replay**: Execute action sequences with dry-run preview
- **Analytics**: Success rates, frequency, patterns, and anomaly detection
- **Persistence**: localStorage and IndexedDB with auto-backup
- **Pagination**: Efficient large dataset handling

## Architecture

```
┌─ hooks/
│  ├─ useEnhancedActionHistory.ts (core hook with all features)
│  └─ useHistoryPersistence.ts (storage management)
├─ contexts/
│  └─ HistoryContext.tsx (global state provider)
├─ components/
│  └─ HistoryViewer.tsx (full UI component)
├─ types/
│  └─ history.ts (type definitions)
├─ utils/
│  └─ history-utils.ts (helper functions)
└─ integration-examples/
   ├─ HistoryIntegration.md (guide & patterns)
   └─ AdapterOperationsWithHistory.tsx (working example)
```

## Quick Start

### 1. Setup Provider (App Level)

```typescript
import { HistoryProvider } from '@/contexts/HistoryContext';

function App() {
  return (
    <HistoryProvider maxSize={1000}>
      <YourApplication />
    </HistoryProvider>
  );
}
```

### 2. Track Actions in Components

```typescript
import { useHistory } from '@/contexts/HistoryContext';

function AdapterPanel() {
  const { addAction, undo, redo, canUndo, canRedo } = useHistory();

  const handleCreateAdapter = async (data) => {
    const startTime = Date.now();

    try {
      const result = await api.createAdapter(data);

      addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: `Created adapter: ${data.name}`,
        duration: Date.now() - startTime,
        undo: async () => await api.deleteAdapter(result.id),
        redo: async () => await api.createAdapter(data),
        metadata: { adapterId: result.id, name: data.name },
        tags: ['production'],
      });
    } catch (error) {
      addAction({
        action: 'create',
        resource: 'adapter',
        status: 'failed',
        description: `Failed to create adapter`,
        duration: Date.now() - startTime,
        errorMessage: error.message,
        undo: async () => {},
      });
    }
  };

  return (
    <div>
      <button onClick={handleCreateAdapter}>Create</button>
      <button onClick={undo} disabled={!canUndo}>Undo</button>
      <button onClick={redo} disabled={!canRedo}>Redo</button>
    </div>
  );
}
```

### 3. Display History UI

```typescript
import HistoryViewer from '@/components/HistoryViewer';

function HistoryPage() {
  return (
    <HistoryViewer
      showStats={true}
      showReplay={true}
      maxVisible={100}
    />
  );
}
```

## Core Concepts

### ActionHistoryItem

Every action tracked in the system has this structure:

```typescript
interface ActionHistoryItem {
  id: string;                 // Auto-generated: "{timestamp}-{random}"
  action: ActionType;         // create | update | delete | load | unload | swap | train | deploy | rollback | configure | other
  resource: ResourceType;     // adapter | stack | training | model | policy | node | tenant | other
  timestamp: number;          // Milliseconds since epoch
  description: string;        // Human-readable summary (required for search)
  status: ActionStatus;       // pending | success | failed | cancelled
  undo: () => Promise<void>;  // Function to revert action
  redo?: () => Promise<void>; // Function to re-execute action
  metadata?: any;             // Custom data for replay/analysis
  errorMessage?: string;      // Set if status === 'failed'
  duration?: number;          // Execution time in milliseconds
  userId?: string;            // User who performed action
  tenantId?: string;          // Tenant context
  tags?: string[];            // Custom labels for organization
}
```

### Action Types

| Type | Use Case | Example |
|------|----------|---------|
| `create` | New resource | Create adapter, create stack |
| `update` | Modify existing | Edit config, update parameters |
| `delete` | Remove resource | Delete adapter, remove policy |
| `load` | Load to memory | Load adapter into VRAM |
| `unload` | Remove from memory | Evict adapter from cache |
| `swap` | Replace resource | Hot-swap adapters |
| `train` | Training operation | Start LoRA training job |
| `deploy` | Deployment | Deploy new model version |
| `rollback` | Revert to previous | Rollback deployment |
| `configure` | Configuration change | Update settings, apply policy |
| `other` | Miscellaneous | Default catch-all |

### Resource Types

| Type | Description |
|------|-------------|
| `adapter` | LoRA adapter |
| `stack` | Adapter stack (composition) |
| `training` | Training job |
| `model` | Base model |
| `policy` | Policy definition |
| `node` | System node |
| `tenant` | Tenant workspace |
| `other` | Other resources |

## API Reference

### useEnhancedActionHistory Hook

#### Core Methods

```typescript
// Add action to history
addAction(action: Omit<ActionHistoryItem, 'id' | 'timestamp'>) => void

// Undo last action
undo: () => Promise<boolean>

// Redo last undone action
redo: () => Promise<boolean>

// Clear all history
clearHistory: () => void

// Get action by ID
getActionById: (id: string) => ActionHistoryItem | undefined
```

#### Filtering & Search

```typescript
// Apply filters
setFilter: (filters: HistoryFilterOptions) => void

// Search by text
setSearch: (query: string) => void

// Get filtered results
filteredActions: ActionHistoryItem[]

// Get paginated results
paginatedActions: ActionHistoryItem[]
```

#### Selection

```typescript
// Toggle individual action
toggleSelection: (actionId: string) => void

// Select all visible
selectAll: () => void

// Clear selection
clearSelection: () => void

// Check selection state
isSelected: (id: string) => boolean
selectedCount: number
```

#### Pagination

```typescript
// Update pagination
setPagination: (pagination: { page: number; pageSize: number }) => void

// State
pagination: { page: number; pageSize: number }
totalPages: number
```

#### Replay

```typescript
// Replay single action
replayAction: (actionId: string, dryRun?: boolean) => Promise<boolean>

// Replay multiple with options
replayActions: (options: HistoryReplayOptions) => Promise<ReplayResult>
```

#### Export

```typescript
// Export in multiple formats
exportHistory: (options: HistoryExportOptions) => Promise<string>
```

#### Analytics

```typescript
// Get comprehensive stats
stats: ActionStats
```

### useHistoryPersistence Hook

```typescript
// Save/load from localStorage
saveToLocalStorage: (actions: ActionHistoryItem[]) => boolean
loadFromLocalStorage: () => ActionHistoryItem[]

// Save/load from IndexedDB
saveToIndexedDB: (actions: ActionHistoryItem[]) => Promise<boolean>
loadFromIndexedDB: () => Promise<ActionHistoryItem[]>

// Backup management
createBackup: (actions: ActionHistoryItem[]) => string
downloadBackup: (actions: ActionHistoryItem[], filename?: string) => void
importHistory: (file: File) => Promise<ActionHistoryItem[] | null>

// Storage management
getStorageQuota: () => Promise<StorageQuota | null>
clearAllStorage: () => boolean
```

## Features in Detail

### 1. Filtering

Filter by multiple criteria simultaneously:

```typescript
const { setFilter, filteredActions } = useHistory();

setFilter({
  actionTypes: ['create', 'delete'],
  resourceTypes: ['adapter'],
  statuses: ['success'],
  startDate: Date.now() - 86400000, // Last 24 hours
  endDate: Date.now(),
  userIds: ['user-123'],
  tenantIds: ['default'],
  tags: ['production', 'critical'],
});
```

### 2. Search

Full-text search across specified fields:

```typescript
setSearch('adapter-name');

// Searches in:
// - description: "Created adapter: adapter-name"
// - metadata: JSON stringified metadata
// - errorMessage: error details
```

### 3. Export

Export with scope and format selection:

```typescript
// Export filtered actions as JSON
const json = await exportHistory({
  format: 'json',
  scope: 'filtered',
  includeMetadata: true,
});

// Export selected as CSV
const csv = await exportHistory({
  format: 'csv',
  scope: 'selected',
});

// Export all as Markdown report
const md = await exportHistory({
  format: 'markdown',
  scope: 'all',
});
```

### 4. Replay

Execute action sequences with preview:

```typescript
// Dry run to preview
const preview = await replayActions({
  actions: selectedActions,
  dryRun: true,
});

// Execute for real
const result = await replayActions({
  actions: selectedActions,
  stopOnError: true,
  batchSize: 5,
});

console.log(`Success: ${result.successCount}/${result.totalActions}`);
console.log(`Errors: ${result.errors}`);
```

### 5. Analytics

Comprehensive statistics and insights:

```typescript
const { stats } = useHistory();

// Stats available:
// - totalActions: number
// - actionsByType: Record<ActionType, number>
// - actionsByResource: Record<ResourceType, number>
// - successRate: number (0-100)
// - averageDuration: number (ms)
// - mostCommonAction: ActionType | null
// - actionsOverTime: Array<{timestamp, count}>
// - recentActions: ActionHistoryItem[]
```

### 6. Persistence

Automatic and manual persistence:

```typescript
const {
  saveToLocalStorage,
  loadFromIndexedDB,
  downloadBackup,
  importHistory,
  getStorageQuota,
} = useHistoryPersistence({
  useIndexedDB: true,
  useLocalStorage: true,
  autoBackup: true,
  backupInterval: 3600000, // 1 hour
});

// Check storage
const quota = await getStorageQuota();
console.log(`Using ${quota.used} of ${quota.total} bytes`);

// Manual backup
downloadBackup(actions, 'backup.json');

// Import from file
const input = document.querySelector<HTMLInputElement>('input[type=file]');
input?.addEventListener('change', async (e) => {
  const file = e.currentTarget.files?.[0];
  if (file) {
    const actions = await importHistory(file);
  }
});
```

## HistoryViewer Component

Complete UI for history visualization and management:

```typescript
<HistoryViewer
  // Callback when user replays an action
  onReplayAction={async (action) => {
    // Custom replay logic
    return await executeAction(action);
  }}
  // Show analytics tab
  showStats={true}
  // Show replay buttons
  showReplay={true}
  // Max actions to keep in memory
  maxVisible={100}
/>
```

Features:
- Timeline view with status indicators
- List view with compact layout
- Analytics dashboard with charts
- Search and multi-filter UI
- Selection with bulk operations
- Export dialog
- Undo/redo controls
- Action details panel

## Utility Functions

Helper functions for analysis and formatting:

```typescript
import {
  formatTimestamp,
  formatDuration,
  getActionLabel,
  getResourceLabel,
  categorizeByTimePeriod,
  findRelatedActions,
  buildActionChain,
  calculateSuccessRate,
  findAnomalies,
  groupActions,
  calculateImpactScore,
  generateSummary,
  generateDetailedReport,
} from '@/utils/history-utils';

// Format for display
formatTimestamp(Date.now());        // "3:45:30 PM"
formatDuration(5000);               // "5.00s"

// Labels
getActionLabel('create');           // "Created"
getResourceLabel('adapter');        // "Adapter"

// Analysis
const chain = buildActionChain(action, allActions);
const related = findRelatedActions(action, allActions, 5000);
const anomalies = findAnomalies(actions);
const score = calculateImpactScore(action);

// Reporting
const summary = generateSummary(actions);
const report = generateDetailedReport(actions);
```

## Best Practices

### Action Design

1. **Always provide undo**: Reversible operations are critical
   ```typescript
   undo: async () => {
     await api.deleteAdapter(adapterId);
   },
   redo: async () => {
     await api.createAdapter(originalData);
   },
   ```

2. **Include metadata**: For replay and analysis
   ```typescript
   metadata: {
     adapterId: result.id,
     name: data.name,
     rank: data.rank,
     previousState: oldData,
   },
   ```

3. **Track duration**: For performance analysis
   ```typescript
   const startTime = Date.now();
   // ... operation ...
   duration: Date.now() - startTime,
   ```

4. **Set proper status**: success/failed/pending/cancelled
   ```typescript
   status: operationSucceeded ? 'success' : 'failed',
   ```

5. **Use tags**: For organization and filtering
   ```typescript
   tags: ['production', 'critical', 'adapter-type-lora'],
   ```

### Performance

- Default max history: 1000 actions (adjust as needed)
- Search is client-side (O(n) complexity)
- Consider pagination for large result sets
- Use IndexedDB for large histories (>10MB)
- Enable auto-cleanup to remove old actions

### Storage

- localStorage: ~5-10MB typical (browser dependent)
- IndexedDB: Unlimited (browser dependent)
- Auto-cleanup removes actions older than 30 days
- Manual backups save to JSON file

## Keyboard Shortcuts

- **Cmd/Ctrl+Z**: Undo
- **Cmd/Ctrl+Shift+Z**: Redo
- **Cmd/Ctrl+Y**: Redo (alternate)

## Troubleshooting

### History not persisting
1. Check browser storage permissions
2. Verify `persistToLocalStorage` option is true
3. Check storage quota: `getStorageQuota()`
4. Try clearing and reloading

### Replay not working
1. Ensure action has `redo` function defined
2. Check that replay function references are still valid
3. Use dry-run mode to test: `dryRun: true`
4. Check browser console for errors

### Search not finding results
1. Verify search fields are configured correctly
2. Check case sensitivity setting
3. Try different search terms
4. Ensure actions have descriptions set

### Performance issues
1. Reduce `maxSize` to limit history
2. Enable `autoCleanup` to remove old actions
3. Paginate results instead of showing all
4. Use IndexedDB instead of localStorage for large histories

## Examples

See `/Users/star/Dev/aos/ui/src/integration-examples/`:

- `HistoryIntegration.md` - Comprehensive integration guide with patterns
- `AdapterOperationsWithHistory.tsx` - Working example with adapter operations

## Files Created/Modified

### New Files
- `types/history.ts` - Type definitions
- `hooks/useEnhancedActionHistory.ts` - Core hook (600+ lines)
- `hooks/useHistoryPersistence.ts` - Storage management (350+ lines)
- `contexts/HistoryContext.tsx` - Global context provider
- `components/HistoryViewer.tsx` - Full UI component (500+ lines)
- `utils/history-utils.ts` - Helper functions (400+ lines)
- `integration-examples/HistoryIntegration.md` - Integration guide
- `integration-examples/AdapterOperationsWithHistory.tsx` - Working example
- `README-HISTORY.md` - This documentation

### Integration Points

The history system integrates with:
- `ui/src/components/ui/export-dialog.tsx` - Export UI
- `ui/src/components/ui/confirmation-dialog.tsx` - Confirmations
- `ui/src/components/ui/empty-state.tsx` - Empty states
- `ui/src/utils/logger.ts` - Structured logging
- `ui/src/hooks/useUndoRedo.ts` - Alternative undo/redo hook

## Summary

The enhanced action history system provides production-grade history management with:

✅ **Tracking**: All operations with metadata and timestamps
✅ **Undo/Redo**: Reversible operations with keyboard shortcuts
✅ **Filtering**: By type, resource, date, status, and tags
✅ **Search**: Full-text search across descriptions and metadata
✅ **Export**: JSON, CSV, Markdown with multiple scopes
✅ **Replay**: Single and batch action execution with dry-run
✅ **Analytics**: Success rates, frequency, patterns, anomalies
✅ **Persistence**: localStorage and IndexedDB with auto-backup
✅ **UI**: Complete timeline/list/stats viewer component
✅ **Pagination**: Efficient large dataset handling

Ready for integration into the AdapterOS platform!
