import React, { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { InferResponse, BackendName } from '@/api/types';
import {
  Copy,
  ShieldCheck,
  ShieldX,
  Server,
  Gauge,
  CheckCircle2,
} from 'lucide-react';
import { toast } from 'sonner';
import { useNavigate } from 'react-router-dom';
import { ProofBar } from '@/components/receipts/ProofBar';

interface RunReceiptPanelProps {
  response: InferResponse | null;
  requestedBackend?: BackendName | string | null;
  requestedDeterminismMode?: string | null;
}

const normalizeValue = (value?: string | null): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export function RunReceiptPanel({
  response,
  requestedBackend,
  requestedDeterminismMode,
}: RunReceiptPanelProps) {
  const navigate = useNavigate();

  const receipt = response?.run_receipt;
  const backendUsed = useMemo(
    () => normalizeValue(response?.backend_used || response?.backend || (requestedBackend as string)) || 'auto',
    [response?.backend_used, response?.backend, requestedBackend],
  );

  const determinismMode = useMemo(
    () => normalizeValue(response?.determinism_mode_applied || (requestedDeterminismMode as string)) || 'unknown',
    [response?.determinism_mode_applied, requestedDeterminismMode],
  );

  const adaptersUsed = response?.adapters_used ?? [];
  const evidenceSpans = (response?.trace as { evidence_spans?: unknown[] } | undefined)?.evidence_spans ?? [];
  const hasEvidence = Array.isArray(evidenceSpans) && evidenceSpans.length > 0;

  if (!response) return null;

  const handleCopy = async (label: string, value?: string | null) => {
    if (!value) {
      toast.error(`${label} is unavailable to copy`);
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label} copied`);
    } catch {
      toast.error(`Unable to copy ${label}`);
    }
  };

  const handleOpenTrace = () => {
    if (!receipt?.trace_id) {
      toast.error('Trace ID is unavailable');
      return;
    }
    navigate(`/telemetry?tab=viewer&requestId=${encodeURIComponent(receipt.trace_id)}`);
  };

  const handleExportEvidence = () => {
    if (!hasEvidence) return;

    const bundle = {
      trace_id: receipt?.trace_id ?? response.id,
      receipt_digest: receipt?.receipt_digest,
      run_head_hash: receipt?.run_head_hash,
      output_digest: receipt?.output_digest,
      logical_prompt_tokens: receipt?.logical_prompt_tokens,
      prefix_cached_token_count: receipt?.prefix_cached_token_count,
      billed_input_tokens: receipt?.billed_input_tokens,
      logical_output_tokens: receipt?.logical_output_tokens,
      billed_output_tokens: receipt?.billed_output_tokens,
      adapters_used: adaptersUsed,
      determinism_mode_applied: determinismMode,
      backend_used: backendUsed,
      response_text: response.text,
      evidence_spans: evidenceSpans,
    };

    const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `evidence-bundle-${receipt?.receipt_digest || response.id}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    toast.success('Evidence bundle exported');
  };

  const digestItems = [
    { key: 'trace-id', label: 'Trace ID', value: receipt?.trace_id },
    { key: 'run-head', label: 'Run head hash', value: receipt?.run_head_hash },
    { key: 'output-digest', label: 'Output digest', value: receipt?.output_digest },
    { key: 'receipt-digest', label: 'Receipt digest', value: receipt?.receipt_digest },
  ];

  const accountingItems = [
    { key: 'logical-prompt', label: 'Logical prompt tokens', value: receipt?.logical_prompt_tokens },
    { key: 'prefix-cached', label: 'Prefix cached tokens', value: receipt?.prefix_cached_token_count },
    { key: 'billed-input', label: 'Billed input tokens', value: receipt?.billed_input_tokens },
    { key: 'logical-output', label: 'Logical output tokens', value: receipt?.logical_output_tokens },
    { key: 'billed-output', label: 'Billed output tokens', value: receipt?.billed_output_tokens },
  ];

  const formatNumber = (value?: number) =>
    typeof value === 'number' ? value.toLocaleString() : 'Not provided';

  const signaturePresent = Boolean(receipt?.signature);
  const attestationPresent = Boolean(receipt?.attestation);

  return (
    <Card data-cy="run-receipt-panel">
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-3">
          <div>
            <CardTitle className="text-base">Run receipt</CardTitle>
            <CardDescription>Backend, determinism, and signed evidence for this run.</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2 justify-end">
            <Badge variant="outline" className="gap-1" data-cy="receipt-backend-used">
              <Server className="h-3 w-3" />
              {backendUsed}
            </Badge>
            <Badge
              variant={determinismMode === 'deterministic' ? 'default' : 'secondary'}
              className="gap-1"
              data-cy="receipt-determinism-mode"
            >
              <Gauge className="h-3 w-3" />
              {determinismMode === 'deterministic' ? 'Deterministic (strict)' : determinismMode}
            </Badge>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <ProofBar
          receiptDigest={receipt?.receipt_digest}
          traceId={receipt?.trace_id}
          backendUsed={backendUsed}
          determinismMode={determinismMode}
          evidenceAvailable={hasEvidence}
          onOpenTrace={handleOpenTrace}
          onExportEvidence={hasEvidence ? handleExportEvidence : undefined}
        />
        <div className="grid gap-3 md:grid-cols-2">
          <div className="space-y-2">
            <div className="text-sm font-medium">Adapters used</div>
            <div className="flex flex-wrap gap-2" data-cy="receipt-adapters-used">
              {adaptersUsed.length > 0 ? (
                adaptersUsed.map((adapter) => (
                  <Badge key={adapter} variant="secondary" className="text-xs">
                    {adapter}
                  </Badge>
                ))
              ) : (
                <span className="text-xs text-muted-foreground">Base model only</span>
              )}
            </div>
          </div>

          <div className="flex flex-wrap gap-2" data-cy="receipt-flags">
            <Badge variant={signaturePresent ? 'default' : 'outline'} className="gap-1">
              {signaturePresent ? <ShieldCheck className="h-3 w-3" /> : <ShieldX className="h-3 w-3" />}
              Signature {signaturePresent ? 'present' : 'missing'}
            </Badge>
            <Badge variant={attestationPresent ? 'default' : 'outline'} className="gap-1">
              {attestationPresent ? <CheckCircle2 className="h-3 w-3" /> : <ShieldX className="h-3 w-3" />}
              Attestation {attestationPresent ? 'present' : 'missing'}
            </Badge>
            <Badge variant="outline" className="gap-1">
              <Gauge className="h-3 w-3" />
              Strict mode {determinismMode === 'deterministic' ? 'on' : 'off'}
            </Badge>
          </div>
        </div>

        <Separator />

        <div className="grid gap-3 sm:grid-cols-2">
          {digestItems.map((item) => (
            <div key={item.key} className="space-y-1">
              <div className="flex items-center justify-between gap-2 text-sm font-medium">
                <span>{item.label}</span>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8"
                  onClick={() => handleCopy(item.label, item.value)}
                  aria-label={`Copy ${item.label}`}
                  data-cy={`copy-${item.key}`}
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
              <div className="font-mono text-xs break-all min-h-[20px]">
                {item.value || 'Not provided'}
              </div>
            </div>
          ))}
        </div>

        <Separator />

        <div className="space-y-2">
          <div className="text-sm font-medium">Token accounting</div>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {accountingItems.map((item) => (
              <div key={item.key} className="space-y-1">
                <div className="flex items-center justify-between gap-2 text-sm font-medium">
                  <span>{item.label}</span>
                </div>
                <div className="font-mono text-xs break-all min-h-[20px]">
                  {formatNumber(item.value)}
                </div>
              </div>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
