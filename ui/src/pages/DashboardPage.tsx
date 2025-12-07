import { useEffect, useMemo, useRef } from 'react';
import { useAuth } from '@/providers/CoreProviders';
import PageWrapper from '@/layout/PageWrapper';
import RoleBasedDashboard from '@/components/dashboard/index';
import { DashboardProvider } from '@/components/dashboard/DashboardProvider';
import { ModelSelector } from '@/components/ModelSelector';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { Button } from '@/components/ui/button';
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { useNavigate, Link } from 'react-router-dom';
import { useSystemMetrics, useMetricsSnapshot } from '@/hooks/useSystem';
import { Skeleton } from '@/components/ui/skeleton';
import { formatMetricValue, hasUsableMetric } from '@/utils/metrics';
import { logger } from '@/utils/logger';

export default function DashboardPage() {
  const { user } = useAuth();
  const navigate = useNavigate();
  const {
    metrics: systemMetrics,
    isLoading: metricsLoading,
    error: metricsError,
    refetch: refetchMetrics,
  } = useSystemMetrics('fast', true);
  const {
    data: metricsSnapshot,
    isLoading: snapshotLoading,
    error: snapshotError,
    refetch: refetchSnapshot,
  } = useMetricsSnapshot(true);
  const greeting = user
    ? `Welcome back, ${user.display_name || user.email}`
    : 'System overview, health monitoring, and alerts';
  const runActions = [
    {
      title: 'Chat',
      description: 'Use adapters in conversational mode.',
      to: '/chat',
    },
    {
      title: 'Inference',
      description: 'Probe adapters with single-turn inference.',
      to: '/inference',
    },
    {
      title: 'Documents',
      description: 'Manage documents feeding retrieval and RAG.',
      to: '/documents',
    },
    {
      title: 'Telemetry Viewer',
      description: 'Inspect per-session routing and tokens.',
      to: '/telemetry/viewer',
    },
    {
      title: 'System health',
      description: 'Open monitoring for high-level health signals.',
      to: '/monitoring',
    },
  ];

  const loggedMetricsError = useRef(false);
  const loggedSnapshotError = useRef(false);

  useEffect(() => {
    if (metricsError && !loggedMetricsError.current) {
      logger.error('Dashboard system metrics failed to load', { component: 'DashboardPage' }, metricsError);
      loggedMetricsError.current = true;
    } else if (!metricsError) {
      loggedMetricsError.current = false;
    }
  }, [metricsError]);

  useEffect(() => {
    if (snapshotError && !loggedSnapshotError.current) {
      logger.error('Dashboard metrics snapshot failed to load', { component: 'DashboardPage' }, snapshotError);
      loggedSnapshotError.current = true;
    } else if (!snapshotError) {
      loggedSnapshotError.current = false;
    }
  }, [snapshotError]);

  const healthSummary = useMemo(() => {
    if (!systemMetrics) return null;
    return {
      cpu: systemMetrics.cpu_usage_percent ?? systemMetrics.cpu_usage ?? null,
      memory: systemMetrics.memory_usage_percent ?? systemMetrics.memory_usage_pct ?? null,
      tokensPerSecond: systemMetrics.tokens_per_second ?? null,
      errorRate: systemMetrics.error_rate ?? null,
      activeSessions: systemMetrics.active_sessions ?? null,
    };
  }, [systemMetrics]);

  const hasHealthData = useMemo(() => {
    if (!healthSummary) return false;
    return hasUsableMetric([
      healthSummary.cpu,
      healthSummary.memory,
      healthSummary.tokensPerSecond,
      healthSummary.errorRate,
    ]);
  }, [healthSummary]);

  const trafficSummary = useMemo(() => {
    const snapshotGauge = metricsSnapshot?.gauges || {};
    const snapshotMetrics = metricsSnapshot?.metrics || {};
    const rpm =
      snapshotGauge['adapteros_requests_per_min'] ??
      snapshotMetrics['adapteros_requests_per_min'] ??
      null;
    const errorRate =
      healthSummary?.errorRate ??
      snapshotGauge['adapteros_error_rate'] ??
      snapshotMetrics['adapteros_error_rate'] ??
      null;
    const tps =
      healthSummary?.tokensPerSecond ??
      snapshotGauge['adapteros_tokens_per_second'] ??
      snapshotMetrics['adapteros_tokens_per_second'] ??
      null;

    return {
      rpm,
      errorRate,
      tps,
      activeSessions: healthSummary?.activeSessions ?? null,
    };
  }, [healthSummary, metricsSnapshot]);

  const hasTrafficData = useMemo(
    () =>
      hasUsableMetric([
        trafficSummary?.rpm,
        trafficSummary?.errorRate,
        trafficSummary?.tps,
        trafficSummary?.activeSessions,
      ]),
    [trafficSummary],
  );

  const healthState = metricsError
    ? 'error'
    : metricsLoading
      ? 'loading'
      : hasHealthData
        ? 'ready'
        : 'empty';

  const trafficState =
    snapshotError || metricsError
      ? 'error'
      : snapshotLoading
        ? 'loading'
        : hasTrafficData
          ? 'ready'
          : 'empty';

  const kvCounters = useMemo(() => {
    const counters = metricsSnapshot?.counters || metricsSnapshot?.metrics || {};
    return {
      fallbacks: counters['kv.fallbacks_total'],
      errors: counters['kv.errors_total'],
      drift: counters['kv.drift_detections_total'],
      degraded: counters['kv.degraded_events_total'],
    };
  }, [metricsSnapshot]);

  const kvState = snapshotError
    ? 'error'
    : snapshotLoading
      ? 'loading'
      : hasUsableMetric([
          kvCounters.fallbacks,
          kvCounters.errors,
          kvCounters.drift,
          kvCounters.degraded,
        ])
        ? 'ready'
        : 'empty';

  const handleRetryHealth = () => {
    void Promise.all([refetchMetrics(), refetchSnapshot()]);
  };

  const handleRetryTraffic = () => {
    void Promise.all([refetchSnapshot(), refetchMetrics()]);
  };

  const errorRatePercent = healthSummary?.errorRate != null ? healthSummary.errorRate * 100 : null;
  const trafficErrorRatePercent =
    trafficSummary?.errorRate != null ? trafficSummary.errorRate * 100 : null;

  return (
    <PageWrapper
      pageKey="dashboard"
      title="Dashboard"
      description={greeting}
      maxWidth="xl"
      customHeader={
        <IaPageHeader
          cluster="Run"
          title="Dashboard"
          description={greeting}
          secondaryActions={[
            {
              label: 'Onboarding checklist',
              onClick: () => navigate('/workflow'),
            },
            {
              label: 'Run probe',
              onClick: () => navigate('/inference'),
            },
          ]}
        >
          <ModelSelector />
        </IaPageHeader>
      }
    >
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 mb-6">
        <Card className="h-full">
          <CardHeader>
            <CardTitle>System health</CardTitle>
            <CardDescription>Live signals from monitoring</CardDescription>
          </CardHeader>
          <CardFooter className="flex w-full">
            {healthState === 'loading' ? (
              <div className="flex w-full items-center gap-3" role="status">
                <Skeleton className="h-16 w-full" />
                <span className="sr-only">Loading system health</span>
              </div>
            ) : healthState === 'error' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <div>
                  <div className="font-medium text-foreground">Unable to load metrics</div>
                  <div className="text-xs">Please retry or open monitoring.</div>
                </div>
                <Button size="sm" variant="secondary" onClick={handleRetryHealth}>
                  Retry
                </Button>
              </div>
            ) : healthState === 'empty' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <span>No recent data</span>
                <Button asChild size="sm" variant="link">
                  <Link to="/monitoring">Open monitoring</Link>
                </Button>
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-3 text-sm w-full">
                <div>CPU: {formatMetricValue(healthSummary?.cpu, { decimals: 1, suffix: '%' })}</div>
                <div>
                  Memory: {formatMetricValue(healthSummary?.memory, { decimals: 1, suffix: '%' })}
                </div>
                <div>
                  Tokens/sec:{' '}
                  {formatMetricValue(healthSummary?.tokensPerSecond, { decimals: 1, placeholder: '—' })}
                </div>
                <div>
                  Error rate:{' '}
                  {formatMetricValue(errorRatePercent, { decimals: 2, suffix: '%', placeholder: '—' })}
                </div>
              </div>
            )}
          </CardFooter>
        </Card>
        <Card className="h-full">
          <CardHeader>
            <CardTitle>Traffic summary</CardTitle>
            <CardDescription>Requests and sessions</CardDescription>
          </CardHeader>
          <CardFooter className="flex w-full">
            {trafficState === 'loading' ? (
              <div className="flex w-full items-center gap-3" role="status">
                <Skeleton className="h-16 w-full" />
                <span className="sr-only">Loading traffic metrics</span>
              </div>
            ) : trafficState === 'error' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <div>
                  <div className="font-medium text-foreground">Unable to load metrics</div>
                  <div className="text-xs">Please retry or open monitoring.</div>
                </div>
                <Button size="sm" variant="secondary" onClick={handleRetryTraffic}>
                  Retry
                </Button>
              </div>
            ) : trafficState === 'empty' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <span>No recent data</span>
                <Button asChild size="sm" variant="link">
                  <Link to="/metrics">View metrics</Link>
                </Button>
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-3 text-sm w-full">
                <div>Requests/min: {formatMetricValue(trafficSummary?.rpm, { decimals: 1 })}</div>
                <div>
                  Active sessions: {formatMetricValue(trafficSummary?.activeSessions, { decimals: 0 })}
                </div>
                <div>Tokens/sec: {formatMetricValue(trafficSummary?.tps, { decimals: 1 })}</div>
                <div>
                  Error rate: {formatMetricValue(trafficErrorRatePercent, { decimals: 2, suffix: '%' })}
                </div>
              </div>
            )}
          </CardFooter>
        </Card>
        <Card className="h-full">
          <CardHeader>
            <CardTitle>KV health</CardTitle>
            <CardDescription>Fallbacks, errors, drift, degraded</CardDescription>
          </CardHeader>
          <CardFooter className="flex w-full">
            {kvState === 'loading' ? (
              <div className="flex w-full items-center gap-3" role="status">
                <Skeleton className="h-16 w-full" />
                <span className="sr-only">Loading KV metrics</span>
              </div>
            ) : kvState === 'error' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <div>
                  <div className="font-medium text-foreground">Unable to load KV metrics</div>
                  <div className="text-xs">Retry or open KV runbook.</div>
                </div>
                <Button size="sm" variant="secondary" onClick={handleRetryTraffic}>
                  Retry
                </Button>
              </div>
            ) : kvState === 'empty' ? (
              <div className="flex w-full items-center justify-between text-sm text-muted-foreground">
                <span>No KV signals yet</span>
                <Button asChild size="sm" variant="link">
                  <Link to="/monitoring">Open monitoring</Link>
                </Button>
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-3 text-sm w-full">
                <div>
                  Fallbacks:{' '}
                  {formatMetricValue(kvCounters.fallbacks, { decimals: 0, placeholder: '—' })}
                </div>
                <div>
                  Errors: {formatMetricValue(kvCounters.errors, { decimals: 0, placeholder: '—' })}
                </div>
                <div>
                  Drift detections:{' '}
                  {formatMetricValue(kvCounters.drift, { decimals: 0, placeholder: '—' })}
                </div>
                <div>
                  Degraded events:{' '}
                  {formatMetricValue(kvCounters.degraded, { decimals: 0, placeholder: '—' })}
                </div>
              </div>
            )}
          </CardFooter>
        </Card>
        <Card className="h-full">
          <CardHeader>
            <CardTitle>Next actions</CardTitle>
            <CardDescription>Jump to common operator workflows</CardDescription>
          </CardHeader>
          <CardFooter className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={() => navigate('/routing')}>
              Investigate routing anomalies
            </Button>
            <Button size="sm" variant="secondary" onClick={() => navigate('/replay')}>
              Inspect recent sessions
            </Button>
            <Button size="sm" variant="secondary" onClick={() => navigate('/testing')}>
              Review test coverage
            </Button>
          </CardFooter>
        </Card>
      </div>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 mb-6">
        {runActions.map(action => (
          <Card key={action.title} className="h-full">
            <CardHeader>
              <CardTitle>{action.title}</CardTitle>
              <CardDescription>{action.description}</CardDescription>
            </CardHeader>
            <CardFooter>
              <Button asChild variant="secondary" size="sm">
                <Link to={action.to}>Open</Link>
              </Button>
            </CardFooter>
          </Card>
        ))}
      </div>
      <DashboardProvider>
        <SectionErrorBoundary sectionName="Dashboard">
          <RoleBasedDashboard />
        </SectionErrorBoundary>
      </DashboardProvider>
    </PageWrapper>
  );
}
