/**
 * Example Usage of Unified Dialog Manager
 *
 * This file demonstrates best practices for using the new dialog management system.
 * Delete this file after reviewing the examples.
 */

import React from 'react';
import {
  createDialogManager,
  useAdapterDialogs,
  useChatDialogs,
} from '@/hooks/ui/useDialogManager';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { logger } from '@/utils/logger';

// ============================================================================
// Example 1: Using Pre-built Dialog Managers
// ============================================================================

export function AdapterListExample() {
  // Use pre-built dialog manager for common adapter operations
  const dialogs = useAdapterDialogs();

  const handleDeleteAdapter = (adapterId: string, adapterName: string) => {
    // Open delete dialog with typed data
    dialogs.openDialog('delete', { adapterId, adapterName });
  };

  const handleShowHealth = (adapter: { id: string; name: string }) => {
    // Open health dialog with adapter data
    dialogs.openDialog('health', { adapter });
  };

  return (
    <div>
      <Button onClick={() => dialogs.openDialog('create')}>
        Create New Adapter
      </Button>

      <Button onClick={() => handleDeleteAdapter('adapter-123', 'My Adapter')}>
        Delete Adapter
      </Button>

      {/* Delete Confirmation Dialog */}
      <Dialog
        open={dialogs.isOpen('delete')}
        onOpenChange={(open) => !open && dialogs.closeDialog('delete')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Adapter</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete{' '}
              <strong>{dialogs.getData('delete')?.adapterName}</strong>?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => dialogs.closeDialog('delete')}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const data = dialogs.getData('delete');
                if (data) {
                  if (import.meta.env.DEV) {
                    logger.debug('Deleting adapter', {
                      component: 'AdapterListExample',
                      operation: 'delete',
                      adapterId: data.adapterId,
                    });
                  }
                  dialogs.closeDialog('delete');
                }
              }}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Health Dialog */}
      <Dialog
        open={dialogs.isOpen('health')}
        onOpenChange={(open) => !open && dialogs.closeDialog('health')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Adapter Health</DialogTitle>
            <DialogDescription>
              Health status for {dialogs.getData('health')?.adapter.name}
            </DialogDescription>
          </DialogHeader>
          {/* Health content here */}
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// Example 2: Creating Custom Dialog Manager
// ============================================================================

// Define custom dialog types for your feature
const useWorkflowDialogs = createDialogManager<
  'create' | 'edit' | 'delete' | 'duplicate',
  {
    create: { templateId?: string };
    edit: { workflowId: string; workflowName: string };
    delete: { workflowId: string; workflowName: string };
    duplicate: { workflowId: string };
  }
>(['create', 'edit', 'delete', 'duplicate'] as const);

export function WorkflowManagerExample() {
  const dialogs = useWorkflowDialogs();

  return (
    <div>
      <Button onClick={() => dialogs.openDialog('create')}>
        New Workflow
      </Button>

      <Button
        onClick={() =>
          dialogs.openDialog('create', { templateId: 'template-123' })
        }
      >
        New Workflow from Template
      </Button>

      <Button
        onClick={() =>
          dialogs.openDialog('edit', {
            workflowId: 'wf-123',
            workflowName: 'My Workflow',
          })
        }
      >
        Edit Workflow
      </Button>

      {/* Create Dialog */}
      <Dialog
        open={dialogs.isOpen('create')}
        onOpenChange={(open) => !open && dialogs.closeDialog('create')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create Workflow</DialogTitle>
            {dialogs.getData('create')?.templateId && (
              <DialogDescription>
                Using template: {dialogs.getData('create')?.templateId}
              </DialogDescription>
            )}
          </DialogHeader>
          {/* Create form here */}
        </DialogContent>
      </Dialog>

      {/* Edit Dialog */}
      <Dialog
        open={dialogs.isOpen('edit')}
        onOpenChange={(open) => !open && dialogs.closeDialog('edit')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              Edit {dialogs.getData('edit')?.workflowName}
            </DialogTitle>
          </DialogHeader>
          {/* Edit form here */}
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// Example 3: Chat Operations with Pre-built Manager
// ============================================================================

export function ChatSessionListExample() {
  const dialogs = useChatDialogs();

  const handleShareSession = (sessionId: string) => {
    dialogs.openDialog('share', { sessionId });
  };

  const handleDeleteSession = (sessionId: string, sessionName: string) => {
    dialogs.openDialog('delete', { sessionId, sessionName });
  };

  return (
    <div>
      <Button onClick={() => handleShareSession('session-123')}>
        Share Session
      </Button>

      <Button onClick={() => handleDeleteSession('session-123', 'My Chat')}>
        Delete Session
      </Button>

      {/* Share Dialog */}
      <Dialog
        open={dialogs.isOpen('share')}
        onOpenChange={(open) => !open && dialogs.closeDialog('share')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Share Session</DialogTitle>
            <DialogDescription>
              Session ID: {dialogs.getData('share')?.sessionId}
            </DialogDescription>
          </DialogHeader>
          {/* Share UI here */}
        </DialogContent>
      </Dialog>

      {/* Delete Dialog */}
      <Dialog
        open={dialogs.isOpen('delete')}
        onOpenChange={(open) => !open && dialogs.closeDialog('delete')}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Session</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete{' '}
              <strong>{dialogs.getData('delete')?.sessionName}</strong>?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => dialogs.closeDialog('delete')}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const data = dialogs.getData('delete');
                if (data) {
                  if (import.meta.env.DEV) {
                    logger.debug('Deleting session', {
                      component: 'ChatSessionExample',
                      operation: 'delete',
                      sessionId: data.sessionId,
                    });
                  }
                  dialogs.closeDialog('delete');
                }
              }}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// Example 4: Advanced - Multiple Dialogs with Shared State
// ============================================================================

export function AdvancedDialogExample() {
  const dialogs = useAdapterDialogs();

  // You can open multiple dialogs in sequence
  const handleComplexWorkflow = async () => {
    // Step 1: Create adapter
    dialogs.openDialog('create');

    // After creation (in actual code, this would be in onSuccess callback):
    // dialogs.closeDialog('create');
    // dialogs.openDialog('training', { adapter: newAdapter });
  };

  // Close all dialogs at once (useful for cleanup)
  const handleCancel = () => {
    dialogs.closeAllDialogs();
  };

  return (
    <div>
      <Button onClick={handleComplexWorkflow}>Start Workflow</Button>
      <Button onClick={handleCancel}>Cancel All</Button>

      {/* Multiple dialogs can be defined, only one shown at a time based on state */}
      <Dialog open={dialogs.isOpen('create')} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Step 1: Create Adapter</DialogTitle>
          </DialogHeader>
        </DialogContent>
      </Dialog>

      <Dialog open={dialogs.isOpen('training')} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Step 2: Configure Training</DialogTitle>
          </DialogHeader>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// Migration Guide from Old Pattern
// ============================================================================

/*
OLD PATTERN (Don't use):
```typescript
const [showDeleteDialog, setShowDeleteDialog] = useState(false);
const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

// Open
setDeleteTarget(adapterId);
setShowDeleteDialog(true);

// Close
setShowDeleteDialog(false);
setDeleteTarget(null);
```

NEW PATTERN (Use this):
```typescript
const dialogs = useAdapterDialogs();

// Open
dialogs.openDialog('delete', { adapterId, adapterName });

// Close
dialogs.closeDialog('delete');

// Check state
if (dialogs.isOpen('delete')) {
  const data = dialogs.getData('delete');
  // data is type-safe: { adapterId: string; adapterName: string } | null
}
```

BENEFITS:
✓ Type-safe data passing
✓ No separate state variables
✓ Consistent API across all dialogs
✓ Automatic cleanup
✓ Better developer experience with autocomplete
*/
