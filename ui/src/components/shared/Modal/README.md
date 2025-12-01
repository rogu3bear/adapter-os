# Modal System Documentation

**Location:** `/ui/src/components/shared/Modal/`
**Status:** Production-ready, zero adoption (0 imports)
**Built on:** Radix UI Dialog primitives
**Last Updated:** 2025-11-30

---

## Table of Contents

1. [Overview](#overview)
2. [Components](#components)
3. [When to Use Which Modal](#when-to-use-which-modal)
4. [Migration Guide](#migration-guide)
5. [Component Reference](#component-reference)
6. [Hooks](#hooks)
7. [Advanced Usage](#advanced-usage)
8. [Examples](#examples)

---

## Overview

The shared Modal system provides three specialized modal components built on Radix Dialog primitives with consistent styling, accessibility, and behavior across AdapterOS.

### Key Features

- **Radix UI primitives** - Accessible, keyboard navigation, focus management
- **Frost glass styling** - Consistent with AdapterOS design system
- **Flexible sizing** - 5 size variants (sm, md, lg, xl, full)
- **TypeScript support** - Full type safety with generics
- **Form integration** - Built-in support for react-hook-form
- **Loading states** - Automatic spinner and disabled state handling
- **Keyboard shortcuts** - Cmd/Ctrl+Enter to submit forms
- **Prevent close** - Lock modals during critical operations
- **Auto-reset** - Optional form reset on close

### Available Components

| Component | Purpose | Use Cases |
|-----------|---------|-----------|
| `Modal` | Base modal with header/body/footer slots | Custom layouts, complex content |
| `FormModal` | Form submission modal | User input, data creation/editing |
| `ConfirmationModal` | Confirm/cancel actions | Deletions, destructive actions, yes/no decisions |

---

## Components

### 1. Modal (Base Component)

Flexible modal with customizable header, body, and footer sections.

**Best for:**
- Custom layouts
- Complex content
- Multi-section modals
- When you need full control

```tsx
import { Modal } from "@/components/shared/Modal";

<Modal
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Edit Item"
  description="Make changes to your item below."
  size="lg"
  footer={
    <>
      <Button variant="outline" onClick={() => setIsOpen(false)}>Cancel</Button>
      <Button onClick={handleSave}>Save</Button>
    </>
  }
>
  {/* Your custom content */}
  <div className="space-y-4">
    {/* ... */}
  </div>
</Modal>
```

### 2. FormModal

Optimized for forms with built-in submission handling and validation support.

**Best for:**
- Creating/editing entities (adapters, stacks, tenants, users)
- Input forms with validation
- Multi-field data entry
- Integration with react-hook-form

```tsx
import { FormModal } from "@/components/shared/Modal";

const { register, handleSubmit, formState } = useForm<FormData>();

<FormModal
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Create Adapter"
  description="Enter adapter details below."
  onSubmit={handleSubmit(onSubmit)}
  isSubmitting={isSubmitting}
  isValid={formState.isValid}
  submitText="Create"
>
  <div className="space-y-4">
    <FormField>
      <FormLabel>Name</FormLabel>
      <Input {...register("name")} />
    </FormField>
    <FormField>
      <FormLabel>Description</FormLabel>
      <Textarea {...register("description")} />
    </FormField>
  </div>
</FormModal>
```

### 3. ConfirmationModal

Specialized for confirm/cancel decisions with visual variants.

**Best for:**
- Delete confirmations
- Destructive actions
- Yes/no decisions
- Action confirmations

```tsx
import { ConfirmationModal } from "@/components/shared/Modal";

<ConfirmationModal
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Delete Adapter?"
  description="This action cannot be undone. This will permanently delete the adapter."
  confirmText="Delete"
  confirmVariant="destructive"
  onConfirm={handleDelete}
  isLoading={isDeleting}
/>
```

---

## When to Use Which Modal

### Decision Tree

```
Need user confirmation?
├─ Yes → ConfirmationModal
│  ├─ Destructive action (delete, etc.) → confirmVariant="destructive"
│  ├─ Success confirmation → confirmVariant="success"
│  └─ Default confirmation → confirmVariant="default"
│
└─ No → Need form submission?
   ├─ Yes → FormModal
   │  ├─ Using react-hook-form → FormModalWithHookForm
   │  └─ Native form → FormModal
   │
   └─ No → Custom content?
      └─ Yes → Modal (base component)
```

### Use Case Matrix

| Scenario | Component | Props to Focus On |
|----------|-----------|-------------------|
| Delete adapter | `ConfirmationModal` | `confirmVariant="destructive"`, `onConfirm` |
| Create stack | `FormModal` | `onSubmit`, `isValid`, `isSubmitting` |
| Edit tenant | `FormModalWithHookForm` | `form`, `onSubmit` |
| Display details | `Modal` | Custom `children`, `footer` |
| Confirm action | `ConfirmationModal` | `confirmVariant="default"` |
| Success message | `ConfirmationModal` | `confirmVariant="success"` |

---

## Migration Guide

### From Dialog to Modal

The shared Modal system replaces direct Dialog usage with specialized components.

#### Before (Dialog)

```tsx
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

<Dialog open={isOpen} onOpenChange={setIsOpen}>
  <DialogContent>
    <DialogHeader>
      <DialogTitle>Edit Stack</DialogTitle>
      <DialogDescription>
        Make changes to your stack below.
      </DialogDescription>
    </DialogHeader>
    <form onSubmit={handleSubmit}>
      <div className="space-y-4">
        <Input name="name" />
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={() => setIsOpen(false)}>
          Cancel
        </Button>
        <Button type="submit">Save</Button>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
```

#### After (Modal)

```tsx
import { FormModal } from "@/components/shared/Modal";

<FormModal
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Edit Stack"
  description="Make changes to your stack below."
  onSubmit={handleSubmit}
  submitText="Save"
>
  <Input name="name" />
</FormModal>
```

#### Benefits of Migration

- **Less boilerplate** - 60% fewer lines of code
- **Consistent styling** - Automatic frost glass, animations
- **Built-in form handling** - Submit, cancel, validation
- **Loading states** - Automatic spinner and disabled states
- **Keyboard shortcuts** - Cmd/Ctrl+Enter to submit
- **Type safety** - Generic type support for form data

---

## Component Reference

### Modal Props

```typescript
interface ModalProps {
  /** Whether the modal is currently open */
  open: boolean;
  /** Callback when the open state changes */
  onOpenChange: (open: boolean) => void;
  /** Modal title displayed in the header */
  title?: React.ReactNode;
  /** Optional description text below the title */
  description?: React.ReactNode;
  /** Content rendered in the modal body */
  children?: React.ReactNode;
  /** Content rendered in the modal footer */
  footer?: React.ReactNode;
  /** Optional trigger element that opens the modal */
  trigger?: React.ReactNode;
  /** Size variant of the modal */
  size?: "sm" | "md" | "lg" | "xl" | "full";
  /** Whether to show the close button in the header */
  showCloseButton?: boolean;
  /** Custom header content (replaces title/description) */
  header?: React.ReactNode;
  /** Optional CSS class for the modal content */
  className?: string;
  /** Prevents closing when clicking outside or pressing Escape */
  preventClose?: boolean;
}
```

### FormModal Props

```typescript
interface FormModalProps<T = unknown> {
  /** Whether the modal is currently open */
  open: boolean;
  /** Callback when the open state changes */
  onOpenChange: (open: boolean) => void;
  /** Modal title */
  title: string;
  /** Optional description */
  description?: React.ReactNode;
  /** Form content */
  children: React.ReactNode;
  /** Text for the submit button */
  submitText?: string; // default: "Submit"
  /** Text for the cancel button */
  cancelText?: string; // default: "Cancel"
  /** Callback when form is submitted */
  onSubmit: (data: T) => void | Promise<void>;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Whether the form submission is in progress */
  isSubmitting?: boolean;
  /** Whether the form is currently valid */
  isValid?: boolean;
  /** Size variant of the modal */
  size?: "sm" | "md" | "lg" | "xl" | "full";
  /** Optional CSS class for the modal content */
  className?: string;
  /** Prevents closing when clicking outside or pressing Escape */
  preventClose?: boolean;
  /** Whether to reset form state when modal closes */
  resetOnClose?: boolean; // default: true
}
```

### ConfirmationModal Props

```typescript
interface ConfirmationModalProps {
  /** Whether the modal is currently open */
  open: boolean;
  /** Callback when the open state changes */
  onOpenChange: (open: boolean) => void;
  /** Modal title */
  title: string;
  /** Description or message to display */
  description?: React.ReactNode;
  /** Text for the confirm button */
  confirmText?: string; // default: "Confirm"
  /** Text for the cancel button */
  cancelText?: string; // default: "Cancel"
  /** Variant style for the confirm button */
  confirmVariant?: "default" | "destructive" | "success";
  /** Callback when confirm is clicked */
  onConfirm: () => void | Promise<void>;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Whether the confirm action is in progress */
  isLoading?: boolean;
  /** Custom icon to display */
  icon?: React.ReactNode;
  /** Optional CSS class for the modal content */
  className?: string;
  /** Prevents closing when clicking outside or pressing Escape */
  preventClose?: boolean;
}
```

### Size Variants

| Size | Max Width | Use Case |
|------|-----------|----------|
| `sm` | 384px | Simple forms, confirmations |
| `md` | 448px | Standard forms (default for ConfirmationModal) |
| `lg` | 512px | Complex forms (default for FormModal/Modal) |
| `xl` | 576px | Large forms, multi-section content |
| `full` | calc(100vw - 4rem) | Maximum space, complex workflows |

---

## Hooks

### useModal

Manage modal state with data passing support.

```typescript
function useModal<T = unknown>(initialOpen?: boolean): UseModalReturn<T>

interface UseModalReturn<T> {
  isOpen: boolean;
  data: T | undefined;
  open: (data?: T) => void;
  close: () => void;
  toggle: () => void;
  onOpenChange: (open: boolean) => void;
}
```

**Example:**
```tsx
const editModal = useModal<{ id: string; name: string }>();

// Open with data
editModal.open({ id: "123", name: "My Adapter" });

// Use in component
<Modal open={editModal.isOpen} onOpenChange={editModal.onOpenChange}>
  <p>Editing: {editModal.data?.name}</p>
</Modal>
```

### useModalManager

Manage multiple modals by key.

```typescript
function useModalManager<K extends string>()
```

**Example:**
```tsx
const modals = useModalManager<"edit" | "delete" | "create">();

// Open specific modal
modals.open("edit", { id: "123" });

// Check if modal is open
modals.isOpen("edit"); // true

// Get modal data
const data = modals.getData<{ id: string }>("edit");

// Close modal
modals.close("edit");

// Use in component
<FormModal
  open={modals.isOpen("edit")}
  onOpenChange={modals.onOpenChange("edit")}
  {...}
/>
```

### useConfirmation

Hook for confirmation modal with async action support.

```typescript
function useConfirmation(options: {
  onConfirm: () => void | Promise<void>;
  onCancel?: () => void;
})
```

**Example:**
```tsx
const confirm = useConfirmation({
  onConfirm: async () => {
    await deleteAdapter(id);
  },
  onCancel: () => {
    console.log("Deletion cancelled");
  },
});

<button onClick={confirm.trigger}>Delete</button>
<ConfirmationModal
  {...confirm.modalProps}
  title="Delete Adapter?"
  description="This action cannot be undone."
  confirmVariant="destructive"
/>
```

---

## Advanced Usage

### Prevent Close During Operations

Lock modal during async operations:

```tsx
<FormModal
  preventClose={isSubmitting}
  onSubmit={async (data) => {
    // Modal cannot be closed during submission
    await saveData(data);
  }}
/>
```

### Custom Header

Replace default title/description with custom content:

```tsx
<Modal
  open={isOpen}
  onOpenChange={setIsOpen}
  header={
    <div className="flex items-center gap-2">
      <AlertIcon className="size-5 text-amber-500" />
      <h2 className="text-lg font-semibold">Warning</h2>
    </div>
  }
>
  {/* ... */}
</Modal>
```

### Controlled Form Reset

Control when form resets:

```tsx
<FormModal
  resetOnClose={false} // Don't auto-reset
  onCancel={() => {
    // Manual reset logic
    form.reset();
  }}
/>
```

### React Hook Form Integration

Use the specialized wrapper for react-hook-form:

```tsx
import { FormModalWithHookForm } from "@/components/shared/Modal";

const form = useForm<FormData>({
  resolver: zodResolver(schema),
});

<FormModalWithHookForm
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Create Stack"
  form={form}
  onSubmit={async (data) => {
    await createStack(data);
  }}
>
  <FormField
    control={form.control}
    name="name"
    render={({ field }) => (
      <FormItem>
        <FormLabel>Name</FormLabel>
        <FormControl>
          <Input {...field} />
        </FormControl>
        <FormMessage />
      </FormItem>
    )}
  />
</FormModalWithHookForm>
```

### Custom Icons in Confirmation

Override default variant icons:

```tsx
<ConfirmationModal
  icon={<CustomIcon className="size-6" />}
  confirmVariant="destructive"
  {...}
/>
```

### Keyboard Shortcuts

FormModal automatically supports:
- **Cmd/Ctrl+Enter** - Submit form (when valid and not submitting)
- **Escape** - Close modal (unless `preventClose`)

---

## Examples

### Example 1: Create Adapter Modal

```tsx
import { FormModal, useModal } from "@/components/shared/Modal";
import { useForm } from "react-hook-form";

interface CreateAdapterForm {
  name: string;
  description: string;
  rank: number;
}

export function CreateAdapterButton() {
  const modal = useModal();
  const { register, handleSubmit, formState } = useForm<CreateAdapterForm>();
  const [isSubmitting, setIsSubmitting] = useState(false);

  const onSubmit = async (data: CreateAdapterForm) => {
    setIsSubmitting(true);
    try {
      await createAdapter(data);
      toast.success("Adapter created successfully");
    } catch (error) {
      toast.error("Failed to create adapter");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <>
      <Button onClick={() => modal.open()}>Create Adapter</Button>

      <FormModal
        open={modal.isOpen}
        onOpenChange={modal.onOpenChange}
        title="Create Adapter"
        description="Enter details for your new adapter."
        onSubmit={handleSubmit(onSubmit)}
        isSubmitting={isSubmitting}
        isValid={formState.isValid}
        submitText="Create"
        size="lg"
      >
        <div className="space-y-4">
          <div>
            <label className="text-sm font-medium">Name</label>
            <Input {...register("name", { required: true })} />
          </div>
          <div>
            <label className="text-sm font-medium">Description</label>
            <Textarea {...register("description")} />
          </div>
          <div>
            <label className="text-sm font-medium">Rank</label>
            <Input
              type="number"
              {...register("rank", { valueAsNumber: true })}
              defaultValue={16}
            />
          </div>
        </div>
      </FormModal>
    </>
  );
}
```

### Example 2: Delete Confirmation

```tsx
import { ConfirmationModal, useModal } from "@/components/shared/Modal";

export function DeleteAdapterButton({ adapterId, adapterName }: Props) {
  const modal = useModal();
  const [isDeleting, setIsDeleting] = useState(false);

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteAdapter(adapterId);
      toast.success("Adapter deleted");
    } catch (error) {
      toast.error("Failed to delete adapter");
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <>
      <Button
        variant="destructive"
        onClick={() => modal.open()}
      >
        Delete
      </Button>

      <ConfirmationModal
        open={modal.isOpen}
        onOpenChange={modal.onOpenChange}
        title={`Delete "${adapterName}"?`}
        description="This action cannot be undone. This will permanently delete the adapter and all associated data."
        confirmText="Delete Adapter"
        confirmVariant="destructive"
        onConfirm={handleDelete}
        isLoading={isDeleting}
      />
    </>
  );
}
```

### Example 3: Edit Modal with Data

```tsx
import { FormModal, useModal } from "@/components/shared/Modal";

interface Stack {
  id: string;
  name: string;
  description: string;
}

export function StackTable({ stacks }: { stacks: Stack[] }) {
  const editModal = useModal<Stack>();

  const handleEdit = async (data: Partial<Stack>) => {
    await updateStack(editModal.data!.id, data);
  };

  return (
    <>
      <table>
        {stacks.map((stack) => (
          <tr key={stack.id}>
            <td>{stack.name}</td>
            <td>
              <Button onClick={() => editModal.open(stack)}>Edit</Button>
            </td>
          </tr>
        ))}
      </table>

      <FormModal
        open={editModal.isOpen}
        onOpenChange={editModal.onOpenChange}
        title="Edit Stack"
        onSubmit={handleEdit}
      >
        <Input
          name="name"
          defaultValue={editModal.data?.name}
        />
        <Textarea
          name="description"
          defaultValue={editModal.data?.description}
        />
      </FormModal>
    </>
  );
}
```

### Example 4: Multi-Modal Manager

```tsx
import { useModalManager, FormModal, ConfirmationModal } from "@/components/shared/Modal";

export function AdapterActions({ adapter }: { adapter: Adapter }) {
  const modals = useModalManager<"edit" | "delete" | "clone">();

  return (
    <>
      <Button onClick={() => modals.open("edit", adapter)}>Edit</Button>
      <Button onClick={() => modals.open("clone", adapter)}>Clone</Button>
      <Button onClick={() => modals.open("delete", adapter)}>Delete</Button>

      {/* Edit Modal */}
      <FormModal
        open={modals.isOpen("edit")}
        onOpenChange={modals.onOpenChange("edit")}
        title="Edit Adapter"
        onSubmit={handleEdit}
      >
        {/* ... */}
      </FormModal>

      {/* Clone Modal */}
      <FormModal
        open={modals.isOpen("clone")}
        onOpenChange={modals.onOpenChange("clone")}
        title="Clone Adapter"
        onSubmit={handleClone}
      >
        {/* ... */}
      </FormModal>

      {/* Delete Modal */}
      <ConfirmationModal
        open={modals.isOpen("delete")}
        onOpenChange={modals.onOpenChange("delete")}
        title="Delete Adapter?"
        confirmVariant="destructive"
        onConfirm={handleDelete}
      />
    </>
  );
}
```

### Example 5: Complex Layout with Custom Footer

```tsx
import { Modal, ModalBody, ModalFooter, useModal } from "@/components/shared/Modal";

export function AdapterDetailsModal({ adapterId }: Props) {
  const modal = useModal();
  const [activeTab, setActiveTab] = useState("details");

  return (
    <Modal
      open={modal.isOpen}
      onOpenChange={modal.onOpenChange}
      title="Adapter Details"
      size="xl"
      footer={
        <div className="flex justify-between w-full">
          <Button variant="outline" onClick={modal.close}>Close</Button>
          <div className="space-x-2">
            <Button variant="outline" onClick={handleExport}>Export</Button>
            <Button onClick={handleEdit}>Edit</Button>
          </div>
        </div>
      }
    >
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="details">Details</TabsTrigger>
          <TabsTrigger value="metrics">Metrics</TabsTrigger>
          <TabsTrigger value="lineage">Lineage</TabsTrigger>
        </TabsList>
        <TabsContent value="details">
          {/* Details content */}
        </TabsContent>
        <TabsContent value="metrics">
          {/* Metrics content */}
        </TabsContent>
        <TabsContent value="lineage">
          {/* Lineage content */}
        </TabsContent>
      </Tabs>
    </Modal>
  );
}
```

---

## Migration Checklist

When migrating from Dialog to Modal:

- [ ] Identify modal type (confirmation, form, or custom)
- [ ] Choose appropriate component (ConfirmationModal, FormModal, or Modal)
- [ ] Replace Dialog imports with Modal imports
- [ ] Move DialogHeader content to `title` and `description` props
- [ ] Extract form submit/cancel logic to `onSubmit`/`onCancel`
- [ ] Remove manual DialogFooter - buttons are automatic
- [ ] Update state management to use `useModal` hook
- [ ] Add TypeScript types for form data
- [ ] Test keyboard shortcuts (Escape, Cmd/Ctrl+Enter)
- [ ] Verify loading/disabled states
- [ ] Remove custom styling (frost glass is automatic)

---

## Related Documentation

- [Radix Dialog Documentation](https://www.radix-ui.com/primitives/docs/components/dialog)
- [AdapterOS Design System](/ui/src/styles/design-system.css)
- [Button Component](/ui/src/components/ui/button.tsx)
- [Form Components](/ui/src/components/ui/form.tsx)

---

Copyright JKCA | 2025 James KC Auchterlonie
