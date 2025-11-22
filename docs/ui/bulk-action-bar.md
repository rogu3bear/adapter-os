# Bulk Action Bar Component

## Overview

The Bulk Action Bar provides consistent bulk action patterns across the AdapterOS UI, enabling efficient multi-item operations with proper state management and user feedback.

## Implementation

[source: ui/src/components/ui/bulk-action-bar.tsx L1-L20]

## Features

- **Selection Management**: Tracks selected items across different views
- **Action Batching**: Groups related operations for efficient processing
- **State Persistence**: Maintains selection state during navigation
- **Progress Feedback**: Real-time progress indicators for long-running operations
- **Error Handling**: Comprehensive error reporting and recovery options

## Usage Examples

### Adapter Management
```typescript
// Bulk adapter operations
const selectedAdapters = ['adapter1', 'adapter2', 'adapter3'];

<BulkActionBar
  selectedItems={selectedAdapters}
  actions={[
    {
      label: 'Unload Adapters',
      action: () => unloadAdapters(selectedAdapters),
      variant: 'destructive'
    },
    {
      label: 'Export Metadata',
      action: () => exportAdapterMetadata(selectedAdapters)
    }
  ]}
/>
```

### Model Operations
```typescript
// Bulk model management
<BulkActionBar
  selectedItems={selectedModels}
  actions={[
    {
      label: 'Load Models',
      action: () => loadModels(selectedModels),
      confirmMessage: 'This will load multiple models into memory. Continue?'
    }
  ]}
/>
```

## API Reference

### Props

| Prop | Type | Description |
|------|------|-------------|
| `selectedItems` | `string[]` | Array of selected item IDs |
| `actions` | `BulkAction[]` | Available bulk actions |
| `onClear` | `() => void` | Callback when selection is cleared |
| `loading` | `boolean` | Shows loading state |

### BulkAction Interface

```typescript
interface BulkAction {
  label: string;           // Action button text
  action: () => void;      // Action handler
  variant?: 'default' | 'destructive';  // Visual style
  confirmMessage?: string; // Confirmation dialog text
  disabled?: boolean;      // Action availability
}
```

## State Management

The component integrates with AdapterOS state management for:

- **Selection Persistence**: Maintains selections across route changes
- **Action History**: Tracks completed bulk operations
- **Error Recovery**: Provides undo functionality for failed operations
- **Progress Tracking**: Real-time status updates for long operations

## Accessibility

- **Keyboard Navigation**: Full keyboard support for all actions
- **Screen Reader**: Comprehensive ARIA labels and descriptions
- **Focus Management**: Proper focus handling during operations
- **High Contrast**: Supports system high contrast modes

## Integration Points

### With Data Tables
```typescript
// Integration with table components
<DataTable
  data={adapters}
  selectable={true}
  onSelectionChange={setSelectedAdapters}
  bulkActions={<BulkActionBar selectedItems={selectedAdapters} actions={adapterActions} />}
/>
```

### With Command Systems
```typescript
// Command palette integration
useCommandPalette([
  {
    id: 'bulk-unload',
    title: 'Unload Selected Adapters',
    action: () => bulkUnloadAdapters(selectedAdapters)
  }
]);
```

## Performance Considerations

- **Virtualization**: Efficiently handles large selection sets
- **Debouncing**: Prevents excessive API calls during rapid selection changes
- **Memory Management**: Cleans up event listeners and state on unmount
- **Lazy Loading**: Actions are loaded only when needed

## Error Handling

The component provides comprehensive error handling for:

- **Network Failures**: Automatic retry with exponential backoff
- **Partial Failures**: Detailed reporting of which items failed
- **Validation Errors**: Clear feedback for invalid operations
- **Timeout Handling**: Graceful handling of long-running operations

## Testing

### Unit Tests
```typescript
describe('BulkActionBar', () => {
  it('renders correct number of action buttons', () => {
    // Test implementation
  });

  it('handles selection clearing', () => {
    // Test implementation
  });
});
```

### Integration Tests
```typescript
describe('Bulk Operations', () => {
  it('completes bulk adapter unload', async () => {
    // Integration test
  });
});
```

## Related Components

- `DataTable` - Provides selection interface
- `ConfirmationDialog` - Handles action confirmations
- `ProgressIndicator` - Shows operation progress
- `ToastProvider` - Displays operation feedback

---

## Citations

- [UI Components](../ui/README.md) - General UI component documentation
- [State Management](../../architecture.md#state-management) - Application state patterns
- [Error Handling](../../core/ERROR-HANDLING.md) - Error handling patterns
