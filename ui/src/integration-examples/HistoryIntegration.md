# Action History Integration Guide

## Overview

The enhanced action history system provides comprehensive history management with filtering, search, export, and replay capabilities. This guide demonstrates integration patterns and usage examples.

## Components

### 1. **useEnhancedActionHistory Hook** (`hooks/useEnhancedActionHistory.ts`)
Core hook providing all history functionality.

```typescript
import useEnhancedActionHistory from '@/hooks/useEnhancedActionHistory';

function MyComponent() {
  const {
    addAction,
    undo,
    redo,
    filteredActions,
    exportHistory,
    replayAction,
    stats,
  } = useEnhancedActionHistory({
    maxSize: 1000,
    persistToLocalStorage: true,
    autoCleanup: true,
  });

  // Track an action
  const handleCreateAdapter = async () => {
    const startTime = Date.now();
    try {
      await api.createAdapter(data);

      addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: `Created adapter: ${data.name}`,
        duration: Date.now() - startTime,
        undo: async () => { /* undo logic */ },
        redo: async () => { /* redo logic */ },
        metadata: { adapterId: data.id },
        tags: ['production', 'critical'],
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
        metadata: { error: error.code },
      });
    }
  };

  return (
    <div>
      <button onClick={handleCreateAdapter}>Create Adapter</button>
      <button onClick={undo} disabled={!canUndo}>Undo</button>
      <button onClick={redo} disabled={!canRedo}>Redo</button>
    </div>
  );
}
```

### 2. **HistoryViewer Component** (`components/HistoryViewer.tsx`)
Full-featured UI for viewing and managing history.

```typescript
import HistoryViewer from '@/components/HistoryViewer';

function Dashboard() {
  const handleReplayAction = async (action: ActionHistoryItem) => {
    // Custom replay logic
    return await api.executeAction(action);
  };

  return (
    <HistoryViewer
      onReplayAction={handleReplayAction}
      showStats={true}
      showReplay={true}
      maxVisible={100}
    />
  );
}
```

### 3. **HistoryContext** (`contexts/HistoryContext.tsx`)
Global context for application-wide history access.

```typescript
import { HistoryProvider, useHistory } from '@/contexts/HistoryContext';

// Wrap your app
function App() {
  return (
    <HistoryProvider maxSize={1000}>
      <YourApp />
    </HistoryProvider>
  );
}

// Use anywhere
function MyComponent() {
  const {
    addAction,
    filteredActions,
    stats,
    exportHistory,
  } = useHistory();

  // Use history functionality
}
```

### 4. **useHistoryPersistence Hook** (`hooks/useHistoryPersistence.ts`)
Manage persistence to localStorage/IndexedDB.

```typescript
import useHistoryPersistence from '@/hooks/useHistoryPersistence';

function HistoryManager() {
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
  });

  const handleBackup = async () => {
    const actions = await loadFromIndexedDB();
    downloadBackup(actions, 'history-backup.json');
  };

  const handleImport = async (file: File) => {
    const actions = await importHistory(file);
    if (actions) {
      actions.forEach(addAction);
    }
  };

  return (
    <div>
      <button onClick={handleBackup}>Download Backup</button>
      <input
        type="file"
        accept=".json"
        onChange={(e) => handleImport(e.target.files?.[0]!)}
      />
    </div>
  );
}
```

## Usage Patterns

### Pattern 1: Basic Action Tracking

```typescript
const addAction = useCallback(async (name: string) => {
  const startTime = Date.now();

  try {
    const result = await performAction(name);

    history.addAction({
      action: 'create',
      resource: 'adapter',
      status: 'success',
      description: `Created ${name}`,
      duration: Date.now() - startTime,
      undo: async () => await undoAction(result.id),
      redo: async () => await performAction(name),
      metadata: { id: result.id, name },
      userId: currentUser.id,
      tenantId: currentTenant.id,
    });
  } catch (error) {
    history.addAction({
      action: 'create',
      resource: 'adapter',
      status: 'failed',
      description: `Failed to create ${name}`,
      duration: Date.now() - startTime,
      errorMessage: error.message,
      undo: async () => {},
      userId: currentUser.id,
      tenantId: currentTenant.id,
    });
  }
}, [history]);
```

### Pattern 2: Filtering and Search

```typescript
function SearchHistory() {
  const { setFilter, setSearch, filteredActions, pagination, setPagination } = useHistory();

  const handleFilterByDate = (startDate: number, endDate: number) => {
    setFilter({
      startDate,
      endDate,
      actionTypes: ['create', 'delete'],
      statuses: ['success'],
    });
  };

  const handleSearch = (query: string) => {
    setSearch(query);
    setPagination({ page: 0, pageSize: 50 });
  };

  return (
    <div>
      <input
        placeholder="Search..."
        onChange={(e) => handleSearch(e.target.value)}
      />
      <button onClick={() => handleFilterByDate(startTime, endTime)}>
        Filter by Date
      </button>
      <ActionList actions={filteredActions} />
    </div>
  );
}
```

### Pattern 3: Export Functionality

```typescript
async function exportHistory() {
  const { exportHistory, filteredActions } = useHistory();

  const jsonData = await exportHistory({
    format: 'json',
    scope: 'filtered',
    includeMetadata: true,
  });

  const csvData = await exportHistory({
    format: 'csv',
    scope: 'selected',
  });

  const markdownData = await exportHistory({
    format: 'markdown',
    scope: 'all',
  });

  // Save to file
  downloadFile(jsonData, 'history.json', 'application/json');
}
```

### Pattern 4: Action Replay

```typescript
async function replayActions() {
  const { replayActions, filteredActions } = useHistory();

  // Dry run to preview
  const preview = await replayActions({
    actions: filteredActions.slice(0, 5),
    dryRun: true,
  });

  console.log(`Preview: ${preview.successCount} actions will run`);

  // Execute for real
  const result = await replayActions({
    actions: filteredActions.slice(0, 5),
    stopOnError: true,
    batchSize: 2,
  });

  console.log(`Replayed: ${result.successCount} succeeded, ${result.failureCount} failed`);
}
```

### Pattern 5: Analytics and Stats

```typescript
function HistoryAnalytics() {
  const { stats } = useHistory();

  return (
    <div className="grid grid-cols-4 gap-4">
      <Card>
        <h3>Total Actions</h3>
        <p>{stats.totalActions}</p>
      </Card>

      <Card>
        <h3>Success Rate</h3>
        <p>{stats.successRate.toFixed(1)}%</p>
      </Card>

      <Card>
        <h3>Avg Duration</h3>
        <p>{(stats.averageDuration / 1000).toFixed(2)}s</p>
      </Card>

      <Card>
        <h3>Most Common</h3>
        <p>{stats.mostCommonAction}</p>
      </Card>

      <div className="col-span-4">
        <h3>Actions by Type</h3>
        {Object.entries(stats.actionsByType)
          .filter(([_, count]) => count > 0)
          .map(([type, count]) => (
            <ProgressBar key={type} label={type} value={count} max={stats.totalActions} />
          ))}
      </div>
    </div>
  );
}
```

## Type Definitions

### ActionHistoryItem
```typescript
interface ActionHistoryItem {
  id: string;                              // Unique identifier
  action: ActionType;                      // Type of action
  resource: ResourceType;                  // Resource affected
  timestamp: number;                       // When action occurred
  description: string;                     // Human-readable description
  status: 'pending' | 'success' | 'failed' | 'cancelled';
  undo: () => Promise<void> | void;       // Undo function
  redo?: () => Promise<void> | void;      // Redo function (optional)
  metadata?: Record<string, any>;         // Custom data
  errorMessage?: string;                  // Error if failed
  duration?: number;                      // Execution time in ms
  userId?: string;                        // User who performed action
  tenantId?: string;                      // Tenant context
  tags?: string[];                        // Custom tags
}
```

### ActionType
- `create` - Create new resource
- `update` - Modify existing resource
- `delete` - Remove resource
- `load` - Load resource into memory
- `unload` - Unload resource
- `swap` - Replace resource
- `train` - Training operation
- `deploy` - Deployment action
- `rollback` - Revert to previous state
- `configure` - Configuration change
- `other` - Miscellaneous

### ResourceType
- `adapter` - LoRA adapter
- `stack` - Adapter stack
- `training` - Training job
- `model` - Base model
- `policy` - Policy definition
- `node` - System node
- `tenant` - Tenant workspace
- `other` - Other resources

## Integration Checklist

- [ ] Import hook/context in component
- [ ] Add action tracking to operations
- [ ] Implement undo/redo handlers
- [ ] Configure storage options
- [ ] Add filter/search UI if needed
- [ ] Setup export functionality
- [ ] Enable replay if applicable
- [ ] Configure auto-cleanup
- [ ] Test persistence
- [ ] Monitor storage quota

## Performance Considerations

1. **History Size**: Default 1000 actions, adjust based on memory constraints
2. **Search**: Runs on client-side, O(n) complexity - consider pagination
3. **Storage**: Uses localStorage (5-10MB typical) or IndexedDB (unlimited)
4. **Auto-cleanup**: Removes actions older than 30 days by default
5. **Export**: Large exports may freeze UI - consider streaming for >10k actions

## Best Practices

1. **Always provide undo/redo**: Track changes so users can revert
2. **Include metadata**: Helps with replay and analysis
3. **Set proper status**: Distinguish success/failure for accurate stats
4. **Use tags**: Organize related actions for filtering
5. **Limit history size**: Balance between detail and memory usage
6. **Regular backups**: Enable auto-backup for critical operations
7. **Meaningful descriptions**: Use for search and audit trail

## Troubleshooting

### History not persisting
- Check browser storage permissions
- Verify `persistToLocalStorage` is enabled
- Check storage quota with `getStorageQuota()`

### Replay not working
- Ensure action has `redo` function
- Check function references are still valid
- Use dry-run mode to test

### Search not finding results
- Verify search fields configuration
- Check case sensitivity setting
- Try broader search terms

### Export file too large
- Reduce time range
- Filter before export
- Use CSV instead of JSON

## Examples

See `/Users/star/Dev/aos/ui/src/integration-examples/` for complete working examples.
