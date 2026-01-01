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
import { Tenant as ApiTenant, Policy } from '@/api/types';

export interface AssignPoliciesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant: ApiTenant | null;
  policies: Policy[];
  selectedPolicies: string[];
  onSelectedPoliciesChange: (policies: string[]) => void;
  onSubmit: () => void;
  canManage: boolean;
}

export function AssignPoliciesDialog({
  open,
  onOpenChange,
  tenant,
  policies,
  selectedPolicies,
  onSelectedPoliciesChange,
  onSubmit,
  canManage,
}: AssignPoliciesDialogProps) {
  const handleClose = () => {
    onOpenChange(false);
    onSelectedPoliciesChange([]);
  };

  const handleCheckboxChange = (policyId: string, checked: boolean) => {
    if (checked) {
      onSelectedPoliciesChange([...selectedPolicies, policyId]);
    } else {
      onSelectedPoliciesChange(selectedPolicies.filter((id) => id !== policyId));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Assign Policies to {tenant?.name}</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 max-h-96 overflow-y-auto" role="group">
          {policies.map((policy) => (
            <div
              key={policy.cpid}
              className="flex items-center space-x-2 p-2 border rounded"
            >
              <input
                type="checkbox"
                id={`policy-${policy.cpid}`}
                checked={selectedPolicies.includes(policy.cpid || '')}
                onChange={(e) =>
                  policy.cpid && handleCheckboxChange(policy.cpid, e.target.checked)
                }
                className="h-4 w-4"
              />
              <label
                htmlFor={`policy-${policy.cpid}`}
                className="flex-1 cursor-pointer"
              >
                <p className="font-medium">{policy.cpid || 'Unknown Policy'}</p>
                <p className="text-xs text-muted-foreground">
                  Hash: {policy.schema_hash?.substring(0, 16) || 'N/A'}
                </p>
              </label>
            </div>
          ))}
          {policies.length === 0 && (
            <p className="text-center text-muted-foreground">No policies available</p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            Cancel
          </Button>
          <GlossaryTooltip termId="assign-policies-action">
            <Button
              onClick={onSubmit}
              disabled={selectedPolicies.length === 0 || !canManage}
            >
              Assign {selectedPolicies.length} Policies
            </Button>
          </GlossaryTooltip>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
