import React from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { Bug, X, Hash, Copy, RefreshCw } from 'lucide-react';
import type { ReceiptVerificationResult } from '@/api/api-types';
import { toast } from 'sonner';

export interface NeuralDebuggerPanelProps {
  open: boolean;
  onClose: () => void;
  tokens: Array<{ content: string; timestamp: number; index: number }>;
  adapterId?: string | null;
  routerConfidence?: number | null;
  runHeadHash?: string | null;
  traceId?: string | null;
  verificationReport?: ReceiptVerificationResult | null;
  onOpenVerification?: () => void;
  onRefreshVerification?: () => void;
}

export function NeuralDebuggerPanel({
  open,
  onClose,
  tokens,
  adapterId,
  routerConfidence,
  runHeadHash,
  traceId,
  verificationReport,
  onOpenVerification,
  onRefreshVerification,
}: NeuralDebuggerPanelProps) {
  if (!open) return null;

  const latestTokens = tokens.slice(-120).reverse();
  const confidenceLabel =
    routerConfidence === null || routerConfidence === undefined
      ? 'n/a'
      : routerConfidence.toFixed(2);

  const effectiveRunHead =
    runHeadHash ||
    verificationReport?.run_head_hash?.computed_hex ||
    verificationReport?.run_head_hash?.expected_hex ||
    null;

  const handleCopy = async (value?: string | null) => {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      toast.success('Copied to clipboard');
    } catch {
      toast.error('Unable to copy value');
    }
  };

  return (
    <div className="absolute right-0 top-0 bottom-0 w-96 bg-background border-l z-10 flex flex-col shadow-lg">
      <div className="border-b px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-2 font-semibold text-sm">
          <Bug className="h-4 w-4" />
          Neural Debugger
        </div>
        <div className="flex items-center gap-2">
          {onRefreshVerification && (
            <Button variant="ghost" size="sm" onClick={onRefreshVerification}>
              <RefreshCw className="h-4 w-4 mr-1" />
              Refresh
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="Close neural debugger">
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>

      <div className="p-4 space-y-3 flex-1 overflow-hidden">
        <div className="grid gap-3">
          <div className="rounded-md border p-3">
            <div className="text-xs text-muted-foreground">Active adapter</div>
            <div className="flex items-center justify-between">
              <div className="font-mono text-sm">
                {adapterId || 'Base model'}
              </div>
              <Badge variant="outline" className="text-[11px]">
                confidence {confidenceLabel}
              </Badge>
            </div>
            {traceId && (
              <div className="text-[11px] text-muted-foreground mt-1">
                Trace: {traceId.slice(0, 12)}
              </div>
            )}
          </div>

          <div className="rounded-md border p-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Hash className="h-4 w-4" />
                run_head_hash
              </div>
              <Button
                variant="ghost"
                size="sm"
                disabled={!effectiveRunHead}
                onClick={() => handleCopy(effectiveRunHead)}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
            <div className={cn(
              'font-mono text-[11px] break-all mt-1',
              effectiveRunHead ? 'text-foreground' : 'text-muted-foreground'
            )}>
              {effectiveRunHead || 'Waiting for receipt...'}
            </div>
          </div>
        </div>

        <div className="rounded-md border p-3 flex-1 min-h-[200px]">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Token provenance
            </div>
            <Badge variant="secondary" className="text-[11px]">
              {tokens.length} tokens
            </Badge>
          </div>
          <ScrollArea className="h-56">
            <div className="space-y-1 font-mono text-[12px]">
              {latestTokens.length === 0 ? (
                <div className="text-muted-foreground text-xs">Waiting for stream...</div>
              ) : (
                latestTokens.map((token) => (
                  <div key={token.index} className="flex items-center justify-between gap-2">
                    <span className="text-muted-foreground">#{token.index}</span>
                    <span className="flex-1 truncate">{token.content || '▢'}</span>
                    <span className="text-[10px] text-muted-foreground">
                      {new Date(token.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </div>

        {verificationReport && (
          <div className="rounded-md border p-3 space-y-1">
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold text-muted-foreground">Verification snapshot</span>
              {onOpenVerification && (
                <Button variant="outline" size="sm" onClick={onOpenVerification} className="text-[11px]">
                  View report
                </Button>
              )}
            </div>
            <div className="font-mono text-[11px] break-all">
              receipt_digest: {verificationReport.receipt_digest?.computed_hex ?? verificationReport.receipt_digest?.expected_hex ?? 'n/a'}
            </div>
            <div className="font-mono text-[11px] break-all">
              run_head_hash: {effectiveRunHead ?? 'n/a'}
            </div>
            {typeof verificationReport.pass === 'boolean' && (
              <div className="text-[11px] text-muted-foreground">
                Pass: {verificationReport.pass ? 'true' : 'false'}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
