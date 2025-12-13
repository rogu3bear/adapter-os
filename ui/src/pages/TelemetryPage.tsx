import { useEffect, useMemo, useState } from 'react';
import { useLocation, useSearchParams } from 'react-router-dom';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Telemetry } from '@/components/Telemetry';
import { TelemetryViewer } from '@/components/telemetry/TelemetryViewer';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Skeleton } from '@/components/ui/skeleton';
import { TraceSummaryPanel } from '@/components/trace/TraceSummaryPanel';
import { TraceTokenTable } from '@/components/trace/TraceTokenTable';
import { useTrace } from '@/hooks/useTrace';
import type { TraceResponseV1 } from '@/api/types';

type TelemetryTab = 'events' | 'traces' | 'viewer';

const telemetryTabs: TelemetryTab[] = ['events', 'traces', 'viewer'];

function normalizeTab(value: string | null): TelemetryTab {
  return telemetryTabs.includes(value as TelemetryTab) ? (value as TelemetryTab) : 'events';
}

export default function TelemetryPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const tenantId = selectedTenant || user?.tenant_id;

  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();

  useEffect(() => {
    const legacyTraceId = searchParams.get('traceId');
    if (legacyTraceId && !searchParams.get('trace_id')) {
      const next = new URLSearchParams(searchParams);
      next.set('trace_id', legacyTraceId);
      next.delete('traceId');
      setSearchParams(next, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const tab = normalizeTab(searchParams.get('tab'));
  const traceId = useMemo(
    () => (searchParams.get('trace_id') ?? searchParams.get('traceId') ?? '').trim(),
    [searchParams]
  );

  const sourceType = useMemo(() => {
    const hash = location.hash?.replace('#', '');
    return searchParams.get('source_type') || (hash?.startsWith('source_type=') ? hash.split('=')[1] : undefined);
  }, [location.hash, searchParams]);

  const { data: trace, isLoading, isFetching, isError, error } = useTrace(traceId || undefined, tenantId);

  const handleTabChange = (next: string) => {
    const normalized = normalizeTab(next);
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set('tab', normalized);
    setSearchParams(nextParams, { replace: true });
  };

  const handleTraceIdChange = (nextTraceId: string) => {
    const nextParams = new URLSearchParams(searchParams);
    if (nextTraceId) {
      nextParams.set('trace_id', nextTraceId);
    } else {
      nextParams.delete('trace_id');
    }
    nextParams.set('tab', 'traces');
    setSearchParams(nextParams, { replace: true });
  };

  return (
    <DensityProvider pageKey="telemetry">
      <FeatureLayout
        title="Telemetry"
        description="Event stream, traces, and viewer in one place"
        maxWidth="full"
        contentPadding="default"
        customHeader={
          <IaPageHeader
            cluster="Observe"
            title="Telemetry"
            description="Event stream, traces, and viewer in one place"
          />
        }
      >
        <SectionErrorBoundary sectionName="Telemetry">
          <Tabs value={tab} onValueChange={handleTabChange}>
            <TabsList className="grid grid-cols-3 gap-2 md:w-[480px]">
              <TabsTrigger value="events">Events</TabsTrigger>
              <TabsTrigger value="traces">Traces</TabsTrigger>
              <TabsTrigger value="viewer">Viewer</TabsTrigger>
            </TabsList>

            <TabsContent value="events" className="mt-6">
              <Telemetry user={user} selectedTenant={selectedTenant} />
            </TabsContent>

            <TabsContent value="traces" className="mt-6">
              <TraceTabContent
                traceId={traceId}
                tenantId={tenantId}
                trace={trace ?? null}
                loading={isLoading || isFetching}
                error={isError ? error : null}
                onTraceIdChange={handleTraceIdChange}
              />
            </TabsContent>

            <TabsContent value="viewer" className="mt-6">
              <TelemetryViewer initialRequestId={traceId || undefined} tenantId={tenantId} sourceType={sourceType} />
            </TabsContent>
          </Tabs>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

interface TraceTabContentProps {
  traceId: string;
  tenantId?: string;
  trace: TraceResponseV1 | null;
  loading: boolean;
  error: unknown;
  onTraceIdChange: (nextTraceId: string) => void;
}

function TraceTabContent({ traceId, tenantId, trace, loading, error, onTraceIdChange }: TraceTabContentProps) {
  const [inputTraceId, setInputTraceId] = useState(traceId);

  useEffect(() => {
    setInputTraceId(traceId);
  }, [traceId]);

  const handleLoad = () => {
    onTraceIdChange(inputTraceId.trim());
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
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Load trace</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 md:flex-row md:items-center">
          <Input
            data-cy="trace-id-input"
            value={inputTraceId}
            onChange={(e) => setInputTraceId(e.target.value)}
            placeholder="Paste Trace ID"
            className="md:max-w-sm"
          />
          <Button onClick={handleLoad} disabled={!inputTraceId.trim()}>
            Load trace
          </Button>
          {traceId && (
            <div className="text-xs text-muted-foreground">
              Tenant: <span className="font-mono">{tenantId ?? 'default'}</span>
            </div>
          )}
        </CardContent>
      </Card>

      {loading && (
        <div className="space-y-3">
          <Skeleton className="h-28 w-full" />
          <Skeleton className="h-96 w-full" />
        </div>
      )}

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error instanceof Error ? error.message : 'Failed to load trace'}</AlertDescription>
        </Alert>
      )}

      {!loading && !trace && !error && (
        <Alert>
          <AlertDescription>No trace loaded.</AlertDescription>
        </Alert>
      )}

      {trace && (
        <div className="space-y-4">
          <TraceSummaryPanel trace={trace} onExport={handleExport} />
          <TraceTokenTable tokens={trace.tokens} />
        </div>
      )}
    </div>
  );
}
