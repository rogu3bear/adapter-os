import React from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Tenant as ApiTenant } from '@/api/types';

export interface EditTenantDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant: ApiTenant | null;
  editName: string;
  onEditNameChange: (name: string) => void;
  onSubmit: () => void;
  canManage: boolean;
}

export function EditTenantDialog({
  open,
  onOpenChange,
  tenant,
  editName,
  onEditNameChange,
  onSubmit,
  canManage,
}: EditTenantDialogProps) {
  const handleClose = () => {
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit Workspace</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <div className="flex items-center gap-1 mb-1">
              <Label htmlFor="workspace-name">Workspace Name</Label>
              <GlossaryTooltip termId="tenant-name">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </div>
            <Input
              id="workspace-name"
              value={editName}
              onChange={(e) => onEditNameChange(e.target.value)}
              placeholder="Enter workspace name"
            />
          </div>
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={handleClose}
            aria-label="Cancel workspace edit"
          >
            Cancel
          </Button>
          <GlossaryTooltip termId="save-tenant-changes">
            <Button
              onClick={onSubmit}
              aria-label="Save workspace changes"
              disabled={!canManage}
            >
              Save Changes
            </Button>
          </GlossaryTooltip>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
