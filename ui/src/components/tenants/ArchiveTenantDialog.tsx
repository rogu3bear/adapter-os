import React from 'react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { AlertTriangle } from 'lucide-react';
import { Tenant as ApiTenant } from '@/api/types';

export interface ArchiveTenantDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant: ApiTenant | null;
  onSubmit: () => void;
  canManage: boolean;
}

export function ArchiveTenantDialog({
  open,
  onOpenChange,
  tenant,
  onSubmit,
  canManage,
}: ArchiveTenantDialogProps) {
  const handleClose = () => {
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Archive Workspace</DialogTitle>
        </DialogHeader>
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            This will archive workspace <strong>{tenant?.name}</strong>. All associated
            resources will be suspended. This action can be reversed by an
            administrator.
          </AlertDescription>
        </Alert>
        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            Cancel
          </Button>
          <GlossaryTooltip termId="archive-tenant-action">
            <Button variant="destructive" onClick={onSubmit} disabled={!canManage}>
              Archive Workspace
            </Button>
          </GlossaryTooltip>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
