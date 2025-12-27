import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient } from '@/api/services';
import type { ReceiptVerificationResult } from '@/api/api-types';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { CheckCircle2, Loader2, Shield } from 'lucide-react';

interface ReceiptInspectorProps {
  defaultTraceId?: string;
}

export function ReceiptInspector({ defaultTraceId = '' }: ReceiptInspectorProps) {
  const [traceId, setTraceId] = useState(defaultTraceId);
  const [result, setResult] = useState<ReceiptVerificationResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTraceId(defaultTraceId);
  }, [defaultTraceId]);

  const handleVerify = useCallback(async () => {
    const trimmed = traceId.trim();
    if (!trimmed) {
      setError('Enter a Trace ID to inspect');
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const response = await apiClient.verifyTraceReceipt(trimmed);
      setResult(response);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Verification failed';
      setError(message);
      setResult(null);
    } finally {
      setIsLoading(false);
    }
  }, [traceId]);

  const chain = useMemo(() => {
    if (!result) return [];
    const contextDigest = (result as unknown as { contextDigest?: ReceiptVerificationResult['context_digest'] }).contextDigest ?? result.context_digest;
    const runHeadHash = (result as unknown as { runHeadHash?: ReceiptVerificationResult['run_head_hash'] }).runHeadHash ?? result.run_head_hash;
    const outputDigest = (result as unknown as { outputDigest?: ReceiptVerificationResult['output_digest'] }).outputDigest ?? result.output_digest;
    const receiptDigest = (result as unknown as { receiptDigest?: ReceiptVerificationResult['receipt_digest'] }).receiptDigest ?? result.receipt_digest;
    const entries = [
      { key: 'context_digest', label: 'Context digest', diff: contextDigest },
      { key: 'run_head_hash', label: 'Run head hash', diff: runHeadHash },
      { key: 'output_digest', label: 'Output digest', diff: outputDigest },
      { key: 'receipt_digest', label: 'Receipt digest', diff: receiptDigest },
    ];
    return entries
      .map((entry) => ({
        ...entry,
        match: entry.diff?.matches ?? (entry.diff?.expected_hex && entry.diff?.computed_hex
          ? entry.diff.expected_hex === entry.diff.computed_hex
          : null),
      }))
      .filter((entry) => entry.diff);
  }, [result]);

  const signatureValid = useMemo(() => {
    if (!result) return null;
    const value = (result as unknown as { signatureValid?: boolean | null }).signatureValid;
    return value ?? result.signature_valid ?? null;
  }, [result]);

  const signatureChecked = useMemo(() => {
    if (!result) return false;
    const value = (result as unknown as { signatureChecked?: boolean }).signatureChecked;
    return value ?? result.signature_checked ?? false;
  }, [result]);

  return (
    <Card className="border-primary/40 shadow-md">
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <div>
            <CardTitle>Receipt Inspector</CardTitle>
            <CardDescription>Paste a Trace ID to replay the Merkle chain and receipts.</CardDescription>
          </div>
          <Badge variant="secondary" className="uppercase text-[10px]">Merkle</Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-col gap-3 md:flex-row md:items-center">
          <Input
            value={traceId}
            onChange={(e) => setTraceId(e.target.value)}
            placeholder="trace-id or request-id"
            className="md:max-w-sm"
          />
          <Button onClick={handleVerify} disabled={isLoading}>
            {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />}
            Inspect receipt
          </Button>
            {result && (
              <Badge variant={result.pass ? 'default' : 'destructive'} className="text-[11px]">
                {result.pass ? 'Verified' : 'Divergent'}
              </Badge>
            )}
        </div>

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {result && (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="outline" className="gap-1 text-[11px]">
                <Shield className="h-3 w-3" /> Signature {signatureValid === false ? 'invalid' : signatureValid ? 'valid' : 'unchecked'}
              </Badge>
              <Badge variant="outline" className="gap-1 text-[11px]">
                <CheckCircle2 className="h-3 w-3" /> Receipt check {signatureChecked ? 'complete' : 'skipped'}
              </Badge>
              {typeof result.mismatched_token === 'number' && (
                <Badge variant="destructive" className="text-[11px]">
                  Diverged at token {result.mismatched_token}
                </Badge>
              )}
            </div>

            <Separator />

            <div className="space-y-2">
              <div className="text-xs uppercase tracking-wide text-muted-foreground">Merkle chain</div>
              <div className="space-y-2">
                {chain.map((node, idx) => (
                  <div key={`${node.key}-${idx}`} className="flex items-stretch gap-3">
                    <div className="flex flex-col items-center pt-1">
                      <span className={`h-3 w-3 rounded-full ${node.match === false ? 'bg-destructive' : node.match ? 'bg-emerald-400' : 'bg-amber-400'}`} />
                      {idx < chain.length - 1 && <span className="w-px flex-1 bg-border" />}
                    </div>
                    <div className="flex-1 rounded-md border bg-muted/40 px-3 py-2">
                      <div className="flex items-center justify-between gap-2">
                        <div className="text-[11px] uppercase font-semibold text-muted-foreground">
                          {node.label}
                        </div>
                        <Badge variant={node.match === false ? 'destructive' : 'secondary'} className="text-[10px] uppercase">
                          {node.match === false ? 'Mismatch' : 'Q15 chain'}
                        </Badge>
                      </div>
                      <div className="font-mono text-xs break-all text-muted-foreground mt-1">
                        expected: {node.diff?.expected_hex || 'n/a'}
                      </div>
                      <div className="font-mono text-xs break-all">
                        computed: {node.diff?.computed_hex || 'n/a'}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {result.reasons && result.reasons.length > 0 && (
              <div className="space-y-2">
                <div className="text-xs uppercase tracking-wide text-muted-foreground">Reasons</div>
                <div className="flex flex-wrap gap-2">
                  {result.reasons.map((reason) => (
                    <Badge key={reason} variant="outline" className="text-[11px] uppercase">
                      {reason}
                    </Badge>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {!result && !error && (
          <Alert>
            <AlertDescription className="text-sm">
              Paste a Trace ID to fetch receipts and render the Merkle proof chain.
            </AlertDescription>
          </Alert>
        )}
      </CardContent>
    </Card>
  );
}
