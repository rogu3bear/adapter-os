import { useMemo, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Skeleton } from '@/components/ui/skeleton';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { useTrace } from '@/hooks/observability/useTrace';
import { TraceSummaryPanel } from '@/components/trace/TraceSummaryPanel';
import { TraceTokenTable } from '@/components/trace/TraceTokenTable';

export default function TelemetryTraceViewerPage() {
  const params = useParams();
  const [searchParams] = useSearchParams();
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  const initialTraceId = useMemo(
    () => params.traceId || searchParams.get('traceId') || '',
    [params.traceId, searchParams]
  );

  const [inputTraceId, setInputTraceId] = useState(initialTraceId);
  const [activeTraceId, setActiveTraceId] = useState(initialTraceId);

  const tenantId = selectedTenant || user?.tenant_id;

  const { data: trace, isLoading, isError, error } = useTrace(activeTraceId || undefined, tenantId);

  const handleLoad = () => {
    if (inputTraceId?.trim()) {
      setActiveTraceId(inputTraceId.trim());
    }
  };

  const handleExport = () => {
    if (!trace) return;
    const blob = new Blob([JSON.stringify(trace, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `trace-${trace.trace_id}-evidence.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  return (
    <DensityProvider pageKey="telemetry-trace-viewer">
      <FeatureLayout
        title="Trace Viewer"
        description="Inspect per-token routing decisions and policy digests for recorded traces."
        maxWidth="full"
      >
        <SectionErrorBoundary sectionName="Trace Viewer">
          <div className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle>Load trace</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-3 md:flex-row md:items-center">
                <Input
                  value={inputTraceId}
                  onChange={(e) => setInputTraceId(e.target.value)}
                  placeholder="Paste trace_id"
                  className="md:max-w-sm"
                />
                <Button onClick={handleLoad} disabled={!inputTraceId}>
                  Load trace
                </Button>
                {activeTraceId && (
                  <div className="text-xs text-muted-foreground">
                    Workspace: <span className="font-mono">{tenantId ?? 'default'}</span>
                  </div>
                )}
              </CardContent>
            </Card>

            {isLoading && (
              <div className="space-y-3">
                <Skeleton className="h-28 w-full" />
                <Skeleton className="h-96 w-full" />
              </div>
            )}

            {isError && (
              <Alert variant="destructive">
                <AlertDescription>{error instanceof Error ? error.message : 'Failed to load trace'}</AlertDescription>
              </Alert>
            )}

            {!isLoading && !trace && !isError && (
              <Alert>
                <AlertDescription>Enter a valid trace_id to inspect trace details.</AlertDescription>
              </Alert>
            )}

            {trace && (
              <div className="space-y-4">
                <TraceSummaryPanel trace={trace} onExport={handleExport} />
                <TraceTokenTable tokens={trace.tokens} />
              </div>
            )}
          </div>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}
