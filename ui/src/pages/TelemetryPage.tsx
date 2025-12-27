import { useEffect, useMemo, useState } from 'react';
import { Link, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom';
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
import { useTrace } from '@/hooks/observability/useTrace';
import { useTelemetryTabRouter } from '@/hooks/navigation/useTabRouter';
import TelemetryAlertsTab from '@/pages/Telemetry/TelemetryAlertsTab';
import TelemetryExportsTab from '@/pages/Telemetry/TelemetryExportsTab';
import TelemetryFiltersTab from '@/pages/Telemetry/TelemetryFiltersTab';
import { ReceiptInspector } from '@/components/telemetry/ReceiptInspector';
import {
  buildTelemetryAlertsLink,
  buildTelemetryEventStreamLink,
  buildTelemetryExportsLink,
  buildTelemetryFiltersLink,
  buildTelemetryTraceLink,
  buildTelemetryViewerLink,
} from '@/utils/navLinks';
import type { TraceResponseV1 } from '@/api/types';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode } from '@/config/ui-mode';

export default function TelemetryPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const tenantId = selectedTenant || user?.tenant_id;
  const { uiMode } = useUiMode();
  const isKernelMode = uiMode === UiMode.Kernel && user?.role?.toLowerCase() === 'developer';

  const location = useLocation();
  const navigate = useNavigate();
  const params = useParams<{ traceId?: string }>();
  const [searchParams] = useSearchParams();
  const { activeTab, availableTabs, getTabPath } = useTelemetryTabRouter();

  const traceId = (params.traceId ?? '').trim();

  const sourceType = useMemo(() => {
    const fromQuery = (searchParams.get('source_type') ?? searchParams.get('sourceType') ?? '').trim();
    if (fromQuery) return fromQuery;

    const hashRaw = (location.hash ?? '').replace(/^#/, '').trim();
    if (!hashRaw || !hashRaw.includes('=')) return undefined;

    const hashParams = new URLSearchParams(hashRaw);
    const fromHash = (hashParams.get('source_type') ?? hashParams.get('sourceType') ?? '').trim();
    return fromHash || undefined;
  }, [location.hash, searchParams]);

  const telemetrySearch = useMemo(() => {
    if (!sourceType) return '';
    return `?source_type=${encodeURIComponent(sourceType)}`;
  }, [sourceType]);

  useEffect(() => {
    const tab = (searchParams.get('tab') ?? '').trim().toLowerCase();
    const legacyTraceId = (searchParams.get('trace_id') ?? searchParams.get('traceId') ?? searchParams.get('requestId') ?? '').trim();

    const allowedSearchKeys = new Set(['source_type']);
    const hasDisallowedSearchKeys = Array.from(searchParams.keys()).some((key) => !allowedSearchKeys.has(key));

    const hashRaw = (location.hash ?? '').replace(/^#/, '').trim();
    const hashParams = hashRaw.includes('=') ? new URLSearchParams(hashRaw) : null;
    const hasLegacyHash = Boolean(hashParams?.has('source_type') || hashParams?.has('sourceType'));

    const hasLegacyParams = tab.length > 0 || legacyTraceId.length > 0 || hasDisallowedSearchKeys || hasLegacyHash;
    if (!hasLegacyParams) return;

    const targetPath = (() => {
      switch (tab) {
        case 'events':
        case 'event-stream':
        case 'event_stream':
        case 'eventstream':
          return buildTelemetryEventStreamLink();
        case 'traces':
        case 'viewer':
        case 'trace':
        case 'viewer-trace':
        case 'viewer_trace':
          return legacyTraceId ? buildTelemetryTraceLink(legacyTraceId) : buildTelemetryViewerLink();
        case 'alerts':
          return buildTelemetryAlertsLink();
        case 'exports':
          return buildTelemetryExportsLink();
        case 'filters':
          return buildTelemetryFiltersLink();
        default:
          if (!tab && traceId) return location.pathname;
          return legacyTraceId ? buildTelemetryTraceLink(legacyTraceId) : location.pathname;
      }
    })();

    const targetHash = hasLegacyHash ? '' : location.hash;
    const targetUrl = `${targetPath}${telemetrySearch}${targetHash}`;
    const currentUrl = `${location.pathname}${location.search}${location.hash}`;

    if (targetUrl !== currentUrl) {
      navigate(targetUrl, { replace: true });
    }
  }, [location.hash, location.pathname, location.search, navigate, searchParams, telemetrySearch, traceId]);

  const { data: trace, isLoading, isFetching, isError, error } = useTrace(traceId || undefined, tenantId);

  const handleTraceIdChange = (nextTraceId: string) => {
    const trimmedTraceId = nextTraceId.trim();
    const targetUrl = trimmedTraceId
      ? buildTelemetryTraceLink(trimmedTraceId, { sourceType })
      : buildTelemetryViewerLink({ sourceType });
    navigate(targetUrl, { replace: true });
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
          <Tabs value={activeTab}>
            <TabsList className="grid grid-cols-3 gap-2 md:w-[480px]">
              {availableTabs.map((tab) => (
                <TabsTrigger key={tab.id} value={tab.id} asChild>
                  <Link to={`${getTabPath(tab.id)}${telemetrySearch}`}>{tab.label}</Link>
                </TabsTrigger>
              ))}
            </TabsList>

            <TabsContent value="event-stream" className="mt-6">
              <Telemetry user={user ?? undefined} selectedTenant={selectedTenant} />
            </TabsContent>

            <TabsContent value="viewer" className="mt-6">
              <TelemetryViewer initialRequestId={traceId || undefined} tenantId={tenantId} sourceType={sourceType} />
            </TabsContent>

            <TabsContent value="viewer-trace" className="mt-6">
              <TraceTabContent
                traceId={traceId}
                tenantId={tenantId}
                trace={trace ?? null}
                loading={isLoading || isFetching}
                error={isError ? error : null}
                onTraceIdChange={handleTraceIdChange}
                kernelMode={isKernelMode}
              />
            </TabsContent>

            <TabsContent value="alerts" className="mt-6">
              <TelemetryAlertsTab />
            </TabsContent>

            <TabsContent value="exports" className="mt-6">
              <TelemetryExportsTab />
            </TabsContent>

            <TabsContent value="filters" className="mt-6">
              <TelemetryFiltersTab />
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
  kernelMode: boolean;
}

function TraceTabContent({ traceId, tenantId, trace, loading, error, onTraceIdChange, kernelMode }: TraceTabContentProps) {
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

      {kernelMode && (
        <ReceiptInspector defaultTraceId={inputTraceId || traceId} />
      )}

      {loading && (
        <div className="space-y-3">
          <Skeleton className="h-28 w-full" />
          <Skeleton className="h-96 w-full" />
        </div>
      )}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>
            {(error as Error)?.message || 'Failed to load trace'}
          </AlertDescription>
        </Alert>
      ) : null}

      {!loading && !trace && !error && (
        <Alert>
          <AlertDescription>No trace loaded.</AlertDescription>
        </Alert>
      )}

      {trace && (
        <div className="space-y-4">
          <TraceSummaryPanel trace={trace} onExport={handleExport} />
          <TraceTokenTable
            tokens={trace.tokens}
            modelType={trace.model_type}
            activeExperts={trace.active_experts}
          />
        </div>
      )}
    </div>
  );
}
