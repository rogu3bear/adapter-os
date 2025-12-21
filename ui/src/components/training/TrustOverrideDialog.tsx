// TrustOverrideDialog - Dialog for applying admin trust overrides to dataset versions
import React, { useState } from 'react';
import { AlertCircle, CheckCircle, Clock, Shield } from 'lucide-react';
import { toast } from 'sonner';

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';

import { apiClient } from '@/api/services';
import type { TrustState } from '@/api/training-types';

interface TrustOverrideDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  datasetId: string;
  datasetVersionId?: string;
  currentTrustState?: TrustState;
  onSuccess?: () => void;
}

const TRUST_STATE_OPTIONS: Array<{
  value: TrustState;
  label: string;
  description: string;
  icon: React.ElementType;
  className: string;
}> = [
  {
    value: 'allowed',
    label: 'Allowed',
    description: 'Dataset is approved for training',
    icon: CheckCircle,
    className: 'text-green-500',
  },
  {
    value: 'allowed_with_warning',
    label: 'Allowed with warning',
    description: 'Dataset approved but requires caution',
    icon: AlertCircle,
    className: 'text-amber-500',
  },
  {
    value: 'needs_approval',
    label: 'Needs approval',
    description: 'Dataset requires manual review before use',
    icon: Clock,
    className: 'text-orange-500',
  },
  {
    value: 'blocked',
    label: 'Blocked',
    description: 'Dataset cannot be used for training',
    icon: AlertCircle,
    className: 'text-red-500',
  },
];

export function TrustOverrideDialog({
  open,
  onOpenChange,
  datasetId,
  datasetVersionId,
  currentTrustState,
  onSuccess,
}: TrustOverrideDialogProps) {
  const [selectedState, setSelectedState] = useState<TrustState | ''>('');
  const [reason, setReason] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    if (!selectedState) {
      toast.error('Please select a trust state');
      return;
    }

    if (!reason.trim()) {
      toast.error('Please provide a reason for the override');
      return;
    }

    setIsSubmitting(true);
    try {
      await apiClient.applyDatasetTrustOverride(datasetId, {
        override_state: selectedState,
        reason: reason.trim(),
      });

      toast.success(`Trust state updated to "${selectedState}"`);
      onSuccess?.();
      onOpenChange(false);
      // Reset form
      setSelectedState('');
      setReason('');
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to apply trust override';
      toast.error(message);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    if (!isSubmitting) {
      onOpenChange(false);
      setSelectedState('');
      setReason('');
    }
  };

  const currentStateConfig = currentTrustState
    ? TRUST_STATE_OPTIONS.find((o) => o.value === currentTrustState)
    : undefined;

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Shield className="h-5 w-5" />
            Override Trust State
          </DialogTitle>
          <DialogDescription>
            Apply an administrative override to the dataset trust state. This will bypass the
            automatically derived trust status and affect whether this dataset can be used for
            training.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Current trust state display */}
          {currentTrustState && currentStateConfig && (
            <div className="rounded-lg border p-3 bg-muted/30">
              <Label className="text-xs text-muted-foreground">Current Trust State</Label>
              <div className="flex items-center gap-2 mt-1">
                <currentStateConfig.icon className={`h-4 w-4 ${currentStateConfig.className}`} />
                <Badge variant="outline">{currentStateConfig.label}</Badge>
              </div>
            </div>
          )}

          {/* Version ID if available */}
          {datasetVersionId && (
            <div className="text-sm text-muted-foreground">
              Overriding version: <code className="text-xs bg-muted px-1 rounded">{datasetVersionId}</code>
            </div>
          )}

          {/* New trust state selection */}
          <div className="space-y-2">
            <Label htmlFor="trust-state">New Trust State</Label>
            <Select
              value={selectedState}
              onValueChange={(value) => setSelectedState(value as TrustState)}
            >
              <SelectTrigger id="trust-state">
                <SelectValue placeholder="Select a trust state..." />
              </SelectTrigger>
              <SelectContent>
                {TRUST_STATE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    <div className="flex items-center gap-2">
                      <option.icon className={`h-4 w-4 ${option.className}`} />
                      <span>{option.label}</span>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedState && (
              <p className="text-xs text-muted-foreground">
                {TRUST_STATE_OPTIONS.find((o) => o.value === selectedState)?.description}
              </p>
            )}
          </div>

          {/* Reason field */}
          <div className="space-y-2">
            <Label htmlFor="reason">Reason for Override</Label>
            <Textarea
              id="reason"
              placeholder="Provide a reason for this trust override (required for audit trail)..."
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              rows={3}
              className="resize-none"
            />
            <p className="text-xs text-muted-foreground">
              This reason will be recorded in the audit log.
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={isSubmitting || !selectedState || !reason.trim()}
          >
            {isSubmitting ? 'Applying...' : 'Apply Override'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default TrustOverrideDialog;
