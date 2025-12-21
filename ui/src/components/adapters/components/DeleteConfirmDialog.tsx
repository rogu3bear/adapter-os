import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { AlertTriangle } from 'lucide-react';

interface DeleteConfirmDialogProps {
  open: boolean;
  adapterId: string | null;
  onConfirm: (id: string) => void;
  onCancel: () => void;
}

export function DeleteConfirmDialog({ open, adapterId, onConfirm, onCancel }: DeleteConfirmDialogProps) {
  if (!open || !adapterId) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onCancel()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Confirm Delete</DialogTitle>
        </DialogHeader>
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            Are you sure you want to delete adapter <code className="font-mono">{adapterId}</code>? This action cannot be undone.
          </AlertDescription>
        </Alert>
        <div className="flex items-center justify-end space-x-2 mt-4">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={() => onConfirm(adapterId)}>
            Delete Adapter
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
