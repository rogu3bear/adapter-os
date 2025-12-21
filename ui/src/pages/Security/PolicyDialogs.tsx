/**
 * PolicyDialogs - Shared dialog components for policy management
 * Extracted to eliminate duplication between PoliciesPage and PoliciesTab
 */

import React from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { CheckCircle, AlertTriangle } from 'lucide-react';
import type { PolicyComparisonResponse } from '@/api/types';

interface ApplyPolicyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  cpid: string;
  onCpidChange: (cpid: string) => void;
  content: string;
  onContentChange: (content: string) => void;
  onApply: () => void;
  isApplying: boolean;
}

export function ApplyPolicyDialog({
  open,
  onOpenChange,
  cpid,
  onCpidChange,
  content,
  onContentChange,
  onApply,
  isApplying,
}: ApplyPolicyDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px]">
        <DialogHeader>
          <DialogTitle>Apply Policy</DialogTitle>
          <DialogDescription>
            Enter the policy ID and content to apply a new or updated policy.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label htmlFor="cpid">Policy ID</Label>
            <Input
              id="cpid"
              value={cpid}
              onChange={(e) => onCpidChange(e.target.value)}
              placeholder="e.g., policy-egress-v1"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="content">Policy Content (JSON)</Label>
            <Textarea
              id="content"
              value={content}
              onChange={(e) => onContentChange(e.target.value)}
              placeholder='{"rules": [...], "version": "1.0"}'
              className="font-mono h-48"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={onApply} disabled={isApplying}>
            {isApplying ? 'Applying...' : 'Apply Policy'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface ComparePoliciesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  cpid1: string;
  onCpid1Change: (cpid: string) => void;
  cpid2: string;
  onCpid2Change: (cpid: string) => void;
  onCompare: () => void;
  isComparing: boolean;
  result: PolicyComparisonResponse | null;
}

export function ComparePoliciesDialog({
  open,
  onOpenChange,
  cpid1,
  onCpid1Change,
  cpid2,
  onCpid2Change,
  onCompare,
  isComparing,
  result,
}: ComparePoliciesDialogProps) {
  const isIdentical = result?.identical ?? (result?.differences?.length === 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[700px]">
        <DialogHeader>
          <DialogTitle>Compare Policies</DialogTitle>
          <DialogDescription>
            Enter two policy IDs to compare their differences.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="cpid1">First Policy ID</Label>
              <Input
                id="cpid1"
                value={cpid1}
                onChange={(e) => onCpid1Change(e.target.value)}
                placeholder="e.g., policy-v1"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="cpid2">Second Policy ID</Label>
              <Input
                id="cpid2"
                value={cpid2}
                onChange={(e) => onCpid2Change(e.target.value)}
                placeholder="e.g., policy-v2"
              />
            </div>
          </div>
          <Button onClick={onCompare} disabled={isComparing} className="w-full">
            {isComparing ? 'Comparing...' : 'Compare'}
          </Button>

          {result && (
            <div className="mt-4 space-y-4">
              <div className="flex items-center gap-2">
                {isIdentical ? (
                  <>
                    <CheckCircle className="h-5 w-5 text-green-500" />
                    <span className="font-medium text-green-700">Policies are identical</span>
                  </>
                ) : (
                  <>
                    <AlertTriangle className="h-5 w-5 text-yellow-500" />
                    <span className="font-medium text-yellow-700">
                      Found {result.differences?.length || 0} difference(s)
                    </span>
                  </>
                )}
              </div>

              {result.differences && result.differences.length > 0 && (
                <div className="border rounded-md p-4 bg-muted/50 max-h-64 overflow-y-auto">
                  <h4 className="font-medium mb-2">Differences:</h4>
                  <ul className="space-y-2 text-sm">
                    {result.differences.map((diff, idx) => (
                      <li key={idx} className="flex items-start gap-2">
                        <span className="font-mono text-xs">{diff}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
