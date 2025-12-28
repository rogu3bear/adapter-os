/**
 * VerificationReportDialog - Display verification report details
 *
 * Modal dialog showing verification report for a trace with copy functionality.
 */

import React from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Loader2, RefreshCw, Copy } from 'lucide-react';
import { toast } from 'sonner';
import type { ReceiptVerificationResult } from '@/api/api-types';

// ============================================================================
// Types
// ============================================================================

export interface VerificationReportDialogProps {
  /** Whether dialog is open */
  open: boolean;
  /** Handler for dialog open state change */
  onOpenChange: (open: boolean) => void;
  /** Current trace ID being displayed */
  traceId: string | null;
  /** Whether verification is loading */
  loading: boolean;
  /** Error message if any */
  error: string | null;
  /** Verification reports map */
  reports: Record<string, ReceiptVerificationResult>;
  /** Handler for refresh */
  onRefresh: (traceId: string) => void;
}

// ============================================================================
// Component
// ============================================================================

export function VerificationReportDialog({
  open,
  onOpenChange,
  traceId,
  loading,
  error,
  reports,
  onRefresh,
}: VerificationReportDialogProps) {
  const report = traceId ? reports[traceId] : undefined;
  const runHeadHash = report?.run_head_hash?.computed_hex || report?.run_head_hash?.expected_hex;

  const handleCopyRunHeadHash = async () => {
    if (!runHeadHash) return;
    try {
      await navigator.clipboard.writeText(runHeadHash);
      toast.success('run_head_hash copied');
    } catch {
      toast.error('Unable to copy run_head_hash');
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>Verification Report</DialogTitle>
          <p className="text-sm text-muted-foreground">
            Trace: {traceId ?? 'n/a'}
          </p>
        </DialogHeader>
        <div className="bg-muted/60 rounded-md p-3 text-xs font-mono max-h-[60vh] overflow-auto border">
          {loading ? (
            <div className="flex items-center gap-2 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              <span>Verifying...</span>
            </div>
          ) : report ? (
            <pre className="whitespace-pre-wrap break-all">
              {JSON.stringify(report, null, 2)}
            </pre>
          ) : (
            <div className="text-muted-foreground">No verification data yet.</div>
          )}
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={!traceId}
            onClick={() => traceId && onRefresh(traceId)}
          >
            <RefreshCw className="h-4 w-4 mr-1" />
            Refresh report
          </Button>
          {runHeadHash && (
            <Button variant="ghost" size="sm" onClick={handleCopyRunHeadHash}>
              <Copy className="h-4 w-4 mr-1" />
              Copy run_head_hash
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
