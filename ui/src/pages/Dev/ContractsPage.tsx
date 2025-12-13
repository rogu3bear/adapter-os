import { useEffect, useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import apiClient from '@/api/client';
import type { ContractSamplesResponse } from '@/api/api-types';
import { Loader2, Download, RefreshCw } from 'lucide-react';

type SampleKey = 'inference' | 'trace' | 'evidence';

function pretty(value: unknown) {
  return JSON.stringify(value, null, 2);
}

function DevOnlyGuard({ children }: { children: React.ReactNode }) {
  if (!import.meta.env.DEV) {
    return (
      <Alert variant="warning" className="mt-4">
        <AlertTitle>Dev mode required</AlertTitle>
        <AlertDescription>
          The contracts viewer is only available in development builds (import.meta.env.DEV).
        </AlertDescription>
      </Alert>
    );
  }
  return <>{children}</>;
}

export default function ContractsPage() {
  const [samples, setSamples] = useState<ContractSamplesResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSamples = useCallback(async () => {
    if (!import.meta.env.DEV) {
      setSamples(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const data = await apiClient.getContractSamples();
      setSamples(data);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load contract samples';
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchSamples();
  }, [fetchSamples]);

  const handleDownload = useCallback(
    (key: SampleKey) => {
      if (!samples) return;
      const blob = new Blob([pretty(samples[key])], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `${key}-sample.json`;
      link.click();
      URL.revokeObjectURL(url);
    },
    [samples]
  );

  return (
    <FeatureLayout
      title="Contract Samples"
      description="Live, fully expanded contract payloads for inference, trace, and evidence responses."
    >
      <DevOnlyGuard>
        <div className="flex items-center justify-between mb-4 gap-2">
          <div className="space-y-1">
            <p className="text-sm text-muted-foreground">
              Data is redacted server-side to remove prompts/PII while preserving digests and IDs.
            </p>
            <p className="text-xs text-muted-foreground">
              Source: /v1/dev/contracts (serves docs/contracts/*.json)
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={() => void fetchSamples()} disabled={loading}>
            {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <RefreshCw className="mr-2 h-4 w-4" />}
            Refresh
          </Button>
        </div>

        {error ? (
          <Alert variant="destructive" className="mb-4">
            <AlertTitle>Failed to load samples</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          <Card className="col-span-1">
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Inference Response</CardTitle>
                <CardDescription>Includes run_receipt and routing trace snippet</CardDescription>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => handleDownload('inference')}
                disabled={!samples}
              >
                <Download className="mr-2 h-4 w-4" />
                Download
              </Button>
            </CardHeader>
            <CardContent>
              {loading && !samples ? (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading inference sample...
                </div>
              ) : (
                <pre className="bg-muted/50 border rounded-md p-3 text-xs overflow-x-auto">
                  {samples ? pretty(samples.inference) : 'No sample available'}
                </pre>
              )}
            </CardContent>
          </Card>

          <Card className="col-span-1">
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Trace Response</CardTitle>
                <CardDescription>Span timeline with receipt digests</CardDescription>
              </div>
              <Button variant="outline" size="sm" onClick={() => handleDownload('trace')} disabled={!samples}>
                <Download className="mr-2 h-4 w-4" />
                Download
              </Button>
            </CardHeader>
            <CardContent>
              {loading && !samples ? (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading trace sample...
                </div>
              ) : (
                <pre className="bg-muted/50 border rounded-md p-3 text-xs overflow-x-auto">
                  {samples ? pretty(samples.trace) : 'No sample available'}
                </pre>
              )}
            </CardContent>
          </Card>

          <Card className="col-span-1">
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Evidence List</CardTitle>
                <CardDescription>Deterministic evidence entries</CardDescription>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => handleDownload('evidence')}
                disabled={!samples}
              >
                <Download className="mr-2 h-4 w-4" />
                Download
              </Button>
            </CardHeader>
            <CardContent>
              {loading && !samples ? (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading evidence sample...
                </div>
              ) : (
                <pre className="bg-muted/50 border rounded-md p-3 text-xs overflow-x-auto">
                  {samples ? pretty(samples.evidence) : 'No sample available'}
                </pre>
              )}
            </CardContent>
          </Card>
        </div>
      </DevOnlyGuard>
    </FeatureLayout>
  );
}
