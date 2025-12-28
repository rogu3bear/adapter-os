import React from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Tenant as ApiTenant, Adapter } from '@/api/types';

export interface AssignAdaptersDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant: ApiTenant | null;
  adapters: Adapter[];
  selectedAdapters: string[];
  onSelectedAdaptersChange: (adapters: string[]) => void;
  onSubmit: () => void;
  canManage: boolean;
}

export function AssignAdaptersDialog({
  open,
  onOpenChange,
  tenant,
  adapters,
  selectedAdapters,
  onSelectedAdaptersChange,
  onSubmit,
  canManage,
}: AssignAdaptersDialogProps) {
  const handleClose = () => {
    onOpenChange(false);
    onSelectedAdaptersChange([]);
  };

  const handleCheckboxChange = (adapterId: string, checked: boolean) => {
    if (checked) {
      onSelectedAdaptersChange([...selectedAdapters, adapterId]);
    } else {
      onSelectedAdaptersChange(selectedAdapters.filter((id) => id !== adapterId));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Assign Adapters to {tenant?.name}</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 max-h-96 overflow-y-auto">
          {adapters.map((adapter) => (
            <div
              key={adapter.id}
              className="flex items-center space-x-2 p-2 border rounded"
            >
              <input
                type="checkbox"
                id={`adapter-${adapter.id}`}
                checked={selectedAdapters.includes(adapter.id)}
                onChange={(e) => handleCheckboxChange(adapter.id, e.target.checked)}
                className="h-4 w-4"
              />
              <label
                htmlFor={`adapter-${adapter.id}`}
                className="flex-1 cursor-pointer"
              >
                <p className="font-medium">{adapter.name}</p>
                <p className="text-xs text-muted-foreground">Rank: {adapter.rank}</p>
              </label>
            </div>
          ))}
          {adapters.length === 0 && (
            <p className="text-center text-muted-foreground">No adapters available</p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            Cancel
          </Button>
          <GlossaryTooltip termId="assign-adapters-action">
            <Button
              onClick={onSubmit}
              disabled={selectedAdapters.length === 0 || !canManage}
            >
              Assign {selectedAdapters.length} Adapters
            </Button>
          </GlossaryTooltip>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
