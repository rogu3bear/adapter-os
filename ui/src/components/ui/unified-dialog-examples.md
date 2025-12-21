# UnifiedDialog Component Examples

The `UnifiedDialog` component is a comprehensive, flexible dialog solution that consolidates patterns from multiple existing dialog implementations into a single, unified API.

## Features

- **Three specialized variants**: Basic, Confirmation, and Form dialogs
- **Consistent styling**: Built on Radix UI primitives with frost glass effects
- **Flexible sizing**: sm, md, lg, xl, and full sizes
- **Icon support**: Pre-configured icons for common use cases (destructive, success, warning)
- **Accessibility**: Full ARIA support with proper focus management
- **Prevent close**: Ability to prevent closing via ESC or outside click
- **Loading states**: Built-in support for async operations

## Basic Dialog

Use the basic dialog for general content display with custom footer actions.

```tsx
import { UnifiedDialog } from '@/components/ui/unified-dialog';
import { Button } from '@/components/ui/button';

function SettingsDialog() {
  const [open, setOpen] = useState(false);

  return (
    <UnifiedDialog
      open={open}
      onOpenChange={setOpen}
      title="Settings"
      description="Manage your application preferences"
      size="lg"
      footer={
        <>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave}>Save Changes</Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label>Theme</label>
          <select>...</select>
        </div>
        <div>
          <label>Language</label>
          <select>...</select>
        </div>
      </div>
    </UnifiedDialog>
  );
}
```

## With Trigger

You can include a trigger element that opens the dialog:

```tsx
<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  title="Help"
  trigger={<Button variant="outline">Open Help</Button>}
>
  <div>Help content...</div>
</UnifiedDialog>
```

## With Icon

Display an icon in the header for visual emphasis:

```tsx
import { InfoIcon } from 'lucide-react';

<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  title="Important Information"
  description="Please review this carefully"
  icon={<InfoIcon className="size-6 text-blue-500" />}
  showIconBackground={true}
>
  <div>Important details...</div>
</UnifiedDialog>
```

Or use a predefined icon variant:

```tsx
<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  title="Warning"
  iconVariant="warning"
  showIconBackground={true}
>
  <div>Warning message...</div>
</UnifiedDialog>
```

## Confirmation Dialog

Use `UnifiedDialog.Confirmation` for confirm/cancel actions like deletions or destructive operations.

### Basic Confirmation

```tsx
import { UnifiedDialog } from '@/components/ui/unified-dialog';

function DeleteConfirmation() {
  const [open, setOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteItem();
      toast.success('Item deleted');
    } catch (error) {
      toast.error('Failed to delete');
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <UnifiedDialog.Confirmation
      open={open}
      onOpenChange={setOpen}
      title="Delete Item?"
      description="This action cannot be undone. This will permanently delete the item."
      confirmText="Delete"
      cancelText="Cancel"
      confirmVariant="destructive"
      onConfirm={handleDelete}
      isLoading={isDeleting}
    />
  );
}
```

### Success Confirmation

```tsx
<UnifiedDialog.Confirmation
  open={open}
  onOpenChange={setOpen}
  title="Confirm Changes"
  description="Are you ready to apply these changes?"
  confirmText="Apply"
  confirmVariant="success"
  iconVariant="success"
  onConfirm={handleApply}
  onCancel={() => console.log('Cancelled')}
/>
```

### With Custom Icon

```tsx
import { AlertTriangleIcon } from 'lucide-react';

<UnifiedDialog.Confirmation
  open={open}
  onOpenChange={setOpen}
  title="Proceed with Caution"
  description="This operation will affect multiple resources."
  icon={<AlertTriangleIcon className="size-6 text-amber-500" />}
  confirmText="Proceed"
  onConfirm={handleProceed}
/>
```

## Form Dialog

Use `UnifiedDialog.Form` for data input with built-in form handling and validation.

### Basic Form

```tsx
import { UnifiedDialog } from '@/components/ui/unified-dialog';

function CreateItemDialog() {
  const [open, setOpen] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (data: any) => {
    setIsSubmitting(true);
    try {
      await createItem(data);
      toast.success('Item created');
    } catch (error) {
      toast.error('Failed to create item');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <UnifiedDialog.Form
      open={open}
      onOpenChange={setOpen}
      title="Create Item"
      description="Fill in the details below"
      submitText="Create"
      onSubmit={handleSubmit}
      isSubmitting={isSubmitting}
    >
      <div className="space-y-4">
        <div>
          <label htmlFor="title">Title</label>
          <input
            id="title"
            name="title"
            className="w-full rounded border px-3 py-2"
            required
          />
        </div>
        <div>
          <label htmlFor="description">Description</label>
          <textarea
            id="description"
            name="description"
            className="w-full rounded border px-3 py-2"
            rows={4}
          />
        </div>
      </div>
    </UnifiedDialog.Form>
  );
}
```

### With Validation

```tsx
function CreateUserDialog() {
  const [open, setOpen] = useState(false);
  const [isValid, setIsValid] = useState(false);
  const [formData, setFormData] = useState({ email: '', name: '' });

  // Simple validation
  useEffect(() => {
    const emailValid = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(formData.email);
    const nameValid = formData.name.length > 2;
    setIsValid(emailValid && nameValid);
  }, [formData]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setFormData(prev => ({ ...prev, [e.target.name]: e.target.value }));
  };

  return (
    <UnifiedDialog.Form
      open={open}
      onOpenChange={setOpen}
      title="Create User"
      submitText="Create User"
      onSubmit={handleSubmit}
      isValid={isValid}
    >
      <div className="space-y-4">
        <div>
          <label>Email</label>
          <input
            name="email"
            type="email"
            value={formData.email}
            onChange={handleChange}
          />
        </div>
        <div>
          <label>Name</label>
          <input
            name="name"
            value={formData.name}
            onChange={handleChange}
          />
        </div>
      </div>
    </UnifiedDialog.Form>
  );
}
```

### With react-hook-form

For complex forms, you can still use react-hook-form with the Form variant:

```tsx
import { useForm } from 'react-hook-form';

interface FormData {
  title: string;
  description: string;
  priority: 'low' | 'medium' | 'high';
}

function CreateTaskDialog() {
  const [open, setOpen] = useState(false);
  const { register, handleSubmit, formState, reset } = useForm<FormData>();

  const onSubmit = async (data: FormData) => {
    await createTask(data);
    toast.success('Task created');
    reset();
    setOpen(false);
  };

  return (
    <UnifiedDialog.Form
      open={open}
      onOpenChange={setOpen}
      title="Create Task"
      submitText="Create"
      onSubmit={handleSubmit(onSubmit)}
      isSubmitting={formState.isSubmitting}
      isValid={formState.isValid}
      resetOnClose={false} // We handle reset manually
    >
      <div className="space-y-4">
        <div>
          <label>Title</label>
          <input {...register('title', { required: true })} />
        </div>
        <div>
          <label>Description</label>
          <textarea {...register('description')} />
        </div>
        <div>
          <label>Priority</label>
          <select {...register('priority')}>
            <option value="low">Low</option>
            <option value="medium">Medium</option>
            <option value="high">High</option>
          </select>
        </div>
      </div>
    </UnifiedDialog.Form>
  );
}
```

### Keyboard Shortcuts

The Form dialog supports Cmd/Ctrl + Enter to submit:

```tsx
<UnifiedDialog.Form
  open={open}
  onOpenChange={setOpen}
  title="Quick Create"
  onSubmit={handleSubmit}
  isValid={isValid}
>
  <input name="title" placeholder="Press Cmd+Enter to submit" />
</UnifiedDialog.Form>
```

## Size Variants

All dialog types support size variants:

```tsx
// Small dialog (sm:max-w-sm)
<UnifiedDialog size="sm" ... />

// Medium dialog (sm:max-w-md) - Default for confirmations
<UnifiedDialog size="md" ... />

// Large dialog (sm:max-w-lg) - Default for basic and forms
<UnifiedDialog size="lg" ... />

// Extra large dialog (sm:max-w-xl)
<UnifiedDialog size="xl" ... />

// Full screen dialog (with margins)
<UnifiedDialog size="full" ... />
```

## Prevent Close

Prevent the dialog from being closed via ESC key or outside click:

```tsx
<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  title="Processing..."
  preventClose={true}
  showCloseButton={false}
>
  <div>Please wait while we process your request...</div>
</UnifiedDialog>
```

For forms with unsaved changes:

```tsx
function EditDialog() {
  const [hasChanges, setHasChanges] = useState(false);

  return (
    <UnifiedDialog.Form
      open={open}
      onOpenChange={(newOpen) => {
        if (!newOpen && hasChanges) {
          if (confirm('You have unsaved changes. Are you sure?')) {
            setOpen(false);
          }
        } else {
          setOpen(newOpen);
        }
      }}
      title="Edit Item"
      onSubmit={handleSubmit}
    >
      <input onChange={() => setHasChanges(true)} />
    </UnifiedDialog.Form>
  );
}
```

## Custom Header

Replace the default title/description with custom header content:

```tsx
<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  header={
    <div className="flex items-center justify-between">
      <div>
        <h2 className="text-xl font-bold">Custom Header</h2>
        <p className="text-sm text-muted-foreground">With custom layout</p>
      </div>
      <Badge>New</Badge>
    </div>
  }
>
  <div>Content...</div>
</UnifiedDialog>
```

## Migration from Existing Components

### From Modal.tsx

```tsx
// Before
<Modal
  open={open}
  onOpenChange={setOpen}
  title="Settings"
  description="Configure your preferences"
  footer={<Button>Save</Button>}
>
  <div>Content</div>
</Modal>

// After
<UnifiedDialog
  open={open}
  onOpenChange={setOpen}
  title="Settings"
  description="Configure your preferences"
  footer={<Button>Save</Button>}
>
  <div>Content</div>
</UnifiedDialog>
```

### From ConfirmationModal.tsx

```tsx
// Before
<ConfirmationModal
  open={open}
  onOpenChange={setOpen}
  title="Delete Item?"
  description="This cannot be undone"
  confirmText="Delete"
  confirmVariant="destructive"
  onConfirm={handleDelete}
  isLoading={isDeleting}
/>

// After
<UnifiedDialog.Confirmation
  open={open}
  onOpenChange={setOpen}
  title="Delete Item?"
  description="This cannot be undone"
  confirmText="Delete"
  confirmVariant="destructive"
  onConfirm={handleDelete}
  isLoading={isDeleting}
/>
```

### From FormModal.tsx

```tsx
// Before
<FormModal
  open={open}
  onOpenChange={setOpen}
  title="Create Item"
  submitText="Create"
  onSubmit={handleSubmit}
  isSubmitting={isSubmitting}
>
  <input name="title" />
</FormModal>

// After
<UnifiedDialog.Form
  open={open}
  onOpenChange={setOpen}
  title="Create Item"
  submitText="Create"
  onSubmit={handleSubmit}
  isSubmitting={isSubmitting}
>
  <input name="title" />
</UnifiedDialog.Form>
```

## Best Practices

1. **Use the right variant**: Choose Confirmation for yes/no decisions, Form for data input, and Basic for everything else.

2. **Provide clear descriptions**: Always include a description for Confirmation dialogs to explain the consequences.

3. **Handle loading states**: Always show loading states for async operations in Confirmation and Form dialogs.

4. **Validate forms**: Use the `isValid` prop to disable submit buttons when form data is invalid.

5. **Size appropriately**: Use `sm` for simple confirmations, `lg` for forms, and `xl`/`full` only when necessary.

6. **Accessibility**: Ensure all form inputs have proper labels and ARIA attributes.

7. **Error handling**: Always handle errors in onConfirm/onSubmit callbacks and show appropriate feedback.

8. **Reset state**: Use `resetOnClose` to clear form data when the dialog closes (enabled by default for Form variant).
