# Dialog Manager Quick Reference

> **TL;DR:** Use `useDialogManager` from `@/hooks/useDialogManager` for all modal/dialog state management. Type-safe, consistent, and eliminates boilerplate.

---

## Quick Start

### 1. Use Pre-built Managers (Most Common)

```typescript
import { useAdapterDialogs } from '@/hooks/useDialogManager';

function MyComponent() {
  const dialogs = useAdapterDialogs();

  return (
    <>
      <Button onClick={() => dialogs.openDialog('create')}>
        Create Adapter
      </Button>

      <Dialog
        open={dialogs.isOpen('create')}
        onOpenChange={(open) => !open && dialogs.closeDialog('create')}
      >
        <DialogContent>
          {/* Your dialog content */}
        </DialogContent>
      </Dialog>
    </>
  );
}
```

### 2. Create Custom Manager (For New Features)

```typescript
import { createDialogManager } from '@/hooks/useDialogManager';

const useMyDialogs = createDialogManager<
  'action1' | 'action2',
  {
    action1: { id: string };
    action2: { name: string; count: number };
  }
>(['action1', 'action2'] as const);

function MyFeature() {
  const dialogs = useMyDialogs();

  dialogs.openDialog('action1', { id: '123' });
  dialogs.openDialog('action2', { name: 'test', count: 5 });

  const data = dialogs.getData('action1'); // Type: { id: string } | null
}
```

---

## Available Pre-built Managers

| Manager | Import | Use Cases |
|---------|--------|-----------|
| `useAdapterDialogs` | `@/hooks/useDialogManager` | Adapter create, import, export, delete, health, training |
| `useChatDialogs` | `@/hooks/useDialogManager` | Chat share, tags, archive, delete |
| `useTrainingDialogs` | `@/hooks/useDialogManager` | Training create, cancel, delete, job/dataset details |
| `useDocumentDialogs` | `@/hooks/useDialogManager` | Document upload, delete, reprocess, view chunks |

---

## API Reference

### Opening Dialogs

```typescript
// No data
dialogs.openDialog('create');

// With data
dialogs.openDialog('delete', { adapterId: '123', adapterName: 'My Adapter' });
```

### Closing Dialogs

```typescript
// Close specific
dialogs.closeDialog('delete');

// Close all
dialogs.closeAllDialogs();
```

### Checking State

```typescript
if (dialogs.isOpen('delete')) {
  // Dialog is open
}
```

### Getting Data

```typescript
const data = dialogs.getData('delete');
if (data) {
  console.log(data.adapterId); // Type-safe access
}
```

---

## Common Patterns

### Pattern 1: Simple Confirmation Dialog

```typescript
const dialogs = useAdapterDialogs();

<Button onClick={() => dialogs.openDialog('delete', { adapterId, adapterName })}>
  Delete
</Button>

<Dialog
  open={dialogs.isOpen('delete')}
  onOpenChange={(open) => !open && dialogs.closeDialog('delete')}
>
  <DialogContent>
    <DialogHeader>
      <DialogTitle>Confirm Deletion</DialogTitle>
      <DialogDescription>
        Delete {dialogs.getData('delete')?.adapterName}?
      </DialogDescription>
    </DialogHeader>
    <DialogFooter>
      <Button onClick={() => dialogs.closeDialog('delete')}>Cancel</Button>
      <Button onClick={handleDelete}>Delete</Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
```

### Pattern 2: Multi-Step Workflow

```typescript
const dialogs = useAdapterDialogs();

// Step 1: Create
dialogs.openDialog('create');

// After create succeeds
dialogs.closeDialog('create');
dialogs.openDialog('training', { adapter: newAdapter });

// Cancel entire workflow
dialogs.closeAllDialogs();
```

### Pattern 3: Conditional Data

```typescript
const dialogs = useAdapterDialogs();

// Open with optional template
dialogs.openDialog('create', { templateId: 'template-123' });

// In dialog
const template = dialogs.getData('create')?.templateId;
{template && <div>Using template: {template}</div>}
```

---

## Migration from Old Pattern

### Before (Don't use)
```typescript
const [showDialog, setShowDialog] = useState(false);
const [dialogData, setDialogData] = useState(null);

// Opening
setDialogData({ id: '123' });
setShowDialog(true);

// In JSX
<Dialog open={showDialog} onOpenChange={setShowDialog}>
  {dialogData && <div>{dialogData.id}</div>}
</Dialog>
```

### After (Use this)
```typescript
const dialogs = useMyDialogs();

// Opening
dialogs.openDialog('myDialog', { id: '123' });

// In JSX
<Dialog
  open={dialogs.isOpen('myDialog')}
  onOpenChange={(open) => !open && dialogs.closeDialog('myDialog')}
>
  <div>{dialogs.getData('myDialog')?.id}</div>
</Dialog>
```

---

## TypeScript Tips

### Type-safe Dialog Data

```typescript
// Define your dialog types clearly
type MyDialogTypes = {
  create: undefined;  // No data needed
  edit: { id: string; name: string };  // Required data
  delete: { id: string };  // Required data
};

const useMyDialogs = createDialogManager<
  keyof MyDialogTypes,
  MyDialogTypes
>(['create', 'edit', 'delete'] as const);
```

### Autocomplete Support

```typescript
// TypeScript will autocomplete dialog names
dialogs.openDialog('create' | 'edit' | 'delete', ...);
                    ^
                    Autocomplete shows available dialogs
```

---

## Common Mistakes

### ❌ Don't: Forget to define data type for dialogs without data

```typescript
// Wrong - TypeScript error
{
  create: {},  // Should be undefined
}
```

### ✅ Do: Use undefined for dialogs without data

```typescript
// Correct
{
  create: undefined,
}
```

### ❌ Don't: Manually manage state alongside dialog manager

```typescript
// Wrong - duplicated state
const dialogs = useAdapterDialogs();
const [isOpen, setIsOpen] = useState(false);  // Don't do this
```

### ✅ Do: Use only the dialog manager

```typescript
// Correct
const dialogs = useAdapterDialogs();
dialogs.isOpen('create');  // Single source of truth
```

---

## Z-Index Hierarchy (Fixed in Workstream 5)

All dialogs now use `z-50` for consistent layering:

```
z-50: Dialog overlays and content
z-40: Toast notifications
z-30: Dropdown menus
z-20: Drawers
z-10: Sticky headers
```

---

## Examples

See `/Users/mln-dev/Dev/adapter-os/ui/src/hooks/useDialogManager.example.tsx` for comprehensive working examples.

---

## Support

For questions or issues with dialog management:
1. Check examples in `useDialogManager.example.tsx`
2. Review completion report: `ui/WORKSTREAM_5_COMPLETION_REPORT.md`
3. Consult the team

---

**Last Updated:** 2025-12-13
**Workstream:** 5 - Modal/Dialog Consolidation
