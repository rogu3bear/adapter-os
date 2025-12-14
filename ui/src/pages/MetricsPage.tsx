import { Suspense, useEffect, useMemo, useState } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { MonitoringPage } from '@/pages/Monitoring/MonitoringPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';
import { buildTelemetryFiltersLink } from '@/utils/navLinks';
import { MetricsPanelSkeleton } from '@/components/skeletons/MetricsPanelSkeleton';
import { MetricsSparklinePanel } from '@/components/MetricsSparklinePanel';
import { useSystemMetrics } from '@/hooks/system/useSystemMetrics';
import { LoadingState } from '@/components/ui/loading-state';

export default function MetricsPage() {
  const { can } = useRBAC();

  const canViewMetrics = can(PERMISSIONS.METRICS_VIEW);
  const [windowMinutes, setWindowMinutes] = useState(5);
  const [history, setHistory] = useState<
    Array<{ timestamp: number; latencyMs: number; tokensPerSecond: number }>
  >([]);

  // Poll live metrics; keep enabled when user has permission to view
  const { metrics, isLoading } = useSystemMetrics('fast', canViewMetrics);

  useEffect(() => {
    if (!metrics) return;
    const now = Date.now();
    const latencyMs = metrics.latency_p95_ms ?? 0;
    const tps = metrics.tokens_per_second ?? 0;

    setHistory(prev => {
      const next = [...prev, { timestamp: now, latencyMs, tokensPerSecond: tps }];
      // Keep a rolling buffer for the largest window (60 min) with a small cushion
      const cutoffMs = now - 60 * 60_000;
      return next.filter(point => point.timestamp >= cutoffMs);
    });
  }, [metrics]);

  const { latencyPoints, tpsPoints } = useMemo(() => {
    const cutoff = Date.now() - windowMinutes * 60_000;
    const filtered = history.filter(point => point.timestamp >= cutoff);
    return {
      latencyPoints: filtered.map(p => p.latencyMs),
      tpsPoints: filtered.map(p => p.tokensPerSecond),
    };
  }, [history, windowMinutes]);

  return (
    <DensityProvider pageKey="metrics">
      <FeatureLayout title="Metrics" description="System performance and health metrics">
        {!canViewMetrics ? (
          <Alert variant="destructive">
            <ShieldAlert className="h-4 w-4" />
            <AlertDescription>
              You do not have permission to view metrics. Required permission: metrics:view
            </AlertDescription>
          </Alert>
        ) : isLoading && !metrics ? (
          <LoadingState variant="minimal" message="Loading metrics..." />
        ) : (
          <SectionErrorBoundary sectionName="Metrics">
            <div className="flex justify-end mb-4">
              <Button asChild variant="outline" size="sm">
                <Link to={buildTelemetryFiltersLink()}>View related telemetry</Link>
              </Button>
            </div>
            <div className="grid gap-4 md:grid-cols-2 mb-4">
              <MetricsSparklinePanel
                title="Latency"
                data={latencyPoints}
                unit="ms"
                windowMinutes={windowMinutes}
                onWindowChange={setWindowMinutes}
              />
              <MetricsSparklinePanel
                title="Tokens / sec"
                data={tpsPoints}
                unit="tps"
                windowMinutes={windowMinutes}
                onWindowChange={setWindowMinutes}
              />
            </div>
            <Suspense fallback={<MetricsPanelSkeleton />}>
              <MonitoringPage />
            </Suspense>
          </SectionErrorBoundary>
        )}
      </FeatureLayout>
    </DensityProvider>
  );
}
