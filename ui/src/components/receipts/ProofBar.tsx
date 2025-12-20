import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { Copy, ExternalLink, FileDown } from 'lucide-react';
import { toast } from 'sonner';

interface ProofBarProps {
  receiptDigest?: string | null;
  traceId?: string | null;
  backendUsed?: string | null;
  determinismMode?: string | null;
  evidenceAvailable?: boolean;
  onOpenTrace?: () => void;
  onExportEvidence?: () => void;
  className?: string;
}

const normalize = (value?: string | null) => {
  if (!value) return null;
  const trimmed = value.trim();
  return trimmed.length ? trimmed : null;
};

function CopyField({
  label,
  value,
  onCopy,
  dataCy,
}: {
  label: string;
  value: string | null;
  onCopy: (label: string, value: string | null) => void;
  dataCy?: string;
}) {
  const display = value ?? 'Not available';
  return (
    <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground" data-cy={dataCy}>
      <span className="font-medium">{label}:</span>
      <span
        className="font-mono text-foreground/80 break-all"
        data-cy={dataCy ? `${dataCy}-value` : undefined}
      >
        {display}
      </span>
      <Button
        size="icon"
        variant="ghost"
        className="h-7 w-7 text-muted-foreground"
        onClick={() => onCopy(label, value)}
        aria-label={`Copy ${label}`}
      >
        <Copy className="h-4 w-4" />
      </Button>
    </div>
  );
}

export function ProofBar({
  receiptDigest,
  traceId,
  backendUsed,
  determinismMode,
  evidenceAvailable,
  onOpenTrace,
  onExportEvidence,
  className,
}: ProofBarProps) {
  const normalizedReceipt = normalize(receiptDigest);
  const normalizedTraceId = normalize(traceId);
  const normalizedBackend = normalize(backendUsed);
  const normalizedDeterminism = normalize(determinismMode) ?? 'unknown';

  const handleCopy = async (label: string, value: string | null) => {
    if (!value) {
      toast.error(`${label} is not available to copy`);
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label} copied`);
    } catch {
      toast.error(`Unable to copy ${label}`);
    }
  };

  return (
    <div
      className={cn(
        'flex flex-col gap-2 rounded-md border bg-muted/50 p-3 text-xs text-muted-foreground',
        className,
      )}
      data-cy="proof-bar"
    >
      <div className="flex flex-wrap items-center gap-3">
        <CopyField
          label="Receipt digest"
          value={normalizedReceipt}
          onCopy={handleCopy}
          dataCy="proofbar-receipt-digest"
        />
        <CopyField
          label="Trace ID"
          value={normalizedTraceId}
          onCopy={handleCopy}
          dataCy="proofbar-trace-id"
        />
        <div className="flex items-center gap-2">
          <span className="font-medium">Backend:</span>
          <span className="font-mono text-foreground/80">
            {normalizedBackend ?? 'Not available'}
          </span>
        </div>
        <Badge
          variant={normalizedDeterminism === 'deterministic' ? 'default' : 'secondary'}
          className="text-[11px]"
        >
          {normalizedDeterminism}
        </Badge>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          className="gap-2 text-muted-foreground"
          onClick={onOpenTrace}
          disabled={!normalizedTraceId || !onOpenTrace}
          data-cy="proofbar-open-trace"
        >
          <ExternalLink className="h-4 w-4" />
          Open Trace
        </Button>
        {evidenceAvailable && onExportEvidence && (
          <Button
            variant="secondary"
            size="sm"
            className="gap-2"
            onClick={onExportEvidence}
            data-cy="export-evidence"
          >
            <FileDown className="h-4 w-4" />
            Export Evidence
          </Button>
        )}
      </div>
    </div>
  );
}
