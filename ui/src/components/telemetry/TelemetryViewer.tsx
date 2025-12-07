import { useMemo, useState } from 'react';
import { formatDistanceToNow } from 'date-fns';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ScrollArea } from '@/components/ui/scroll-area';
import { LineChart, Line, CartesianGrid, XAxis, YAxis, Tooltip as ReTooltip, ResponsiveContainer } from 'recharts';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { useSessionTelemetry } from '@/hooks/useSessionTelemetry';
import { CHART_COLORS } from '@/constants/chart-colors';
import { cn } from '@/components/ui/utils';
import { Link } from 'react-router-dom';
import { buildReplayRunsLink } from '@/utils/navLinks';

interface TelemetryViewerProps {
  initialRequestId?: string | null;
  tenantId?: string;
  liveTokens?: Array<{ index: number; content: string; timestamp: number }>;
  backendName?: string;
  workerId?: string;
  tokenText?: Array<{ index: number; content: string }>;
  sourceType?: string;
}

export function TelemetryViewer({
  initialRequestId,
  tenantId,
  liveTokens,
  backendName,
  workerId,
  tokenText,
  sourceType,
}: TelemetryViewerProps) {
  const [manualId, setManualId] = useState(initialRequestId ?? '');
  const {
    sessions,
    sessionsLoading,
    sessionsError,
    selectedRequestId,
    selectRequestId,
    steps,
    stepsLoading,
    stepsError,
    tokensPerSecond,
    latencyP50,
    metricsLoading,
    metricsError,
    totalTokens,
    refetchSessions,
    refetchSteps,
    refetchMetrics,
    page,
    setPage,
    filterRequestId,
    setFilterRequestId,
    adapterMap,
  } = useSessionTelemetry({
    initialRequestId: initialRequestId ?? undefined,
    tenantId,
    sourceType,
  });

  const currentSession = useMemo(
    () => sessions.find((s) => s.requestId === selectedRequestId) ?? null,
    [sessions, selectedRequestId]
  );

  const tokenLookup = useMemo(() => {
    const map = new Map<number, string>();
    (liveTokens || []).forEach((t) => map.set(t.index, t.content));
    (tokenText || []).forEach((t) => map.set(t.index, t.content));
    return map;
  }, [liveTokens, tokenText]);

  const timelineRows = useMemo(
    () =>
      steps.map((step) => ({
        key: `${step.step}-${step.input_token_id ?? 'n'}`,
        tokenIndex: step.input_token_id ?? step.step,
        timestamp: step.timestamp,
        adapters: step.adapters_fired.map((a) => {
          const mapped = adapterMap.get(a.adapter_idx);
          return {
            ...a,
            adapter_id: mapped?.id,
            adapter_name: mapped?.name,
          };
        }),
        entropy: step.entropy,
        tau: step.tau,
      })),
    [steps, adapterMap]
  );

  const latestTps = tokensPerSecond.length > 0 ? tokensPerSecond[tokensPerSecond.length - 1].value : 0;
  const totalTokensDisplay = liveTokens?.length ?? totalTokens;

  const timelineTitle = selectedRequestId ? `Session ${selectedRequestId}` : 'Select a session';

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Telemetry Session</CardTitle>
          <CardDescription>Select a session and inspect routing + token timeline.</CardDescription>
          {selectedRequestId && (
            <div className="text-xs text-muted-foreground">
              <Link to={buildReplayRunsLink(selectedRequestId)} className="underline underline-offset-4">
                Open in replay
              </Link>
            </div>
          )}
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
            <div className="space-y-2">
              <div className="text-sm font-medium">Recent sessions</div>
              {sessionsLoading ? (
                <Skeleton className="h-10 w-full" />
              ) : sessionsError ? (
                <ErrorRecovery error={sessionsError.message} onRetry={() => refetchSessions()} />
              ) : sessions.length === 0 ? (
                <Alert>
                  <AlertDescription>No sessions found yet.</AlertDescription>
                </Alert>
              ) : (
                <Select
                  value={selectedRequestId ?? undefined}
                  onValueChange={(value) => selectRequestId(value)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Pick a session" />
                  </SelectTrigger>
                  <SelectContent>
                    {sessions.map((s) => (
                      <SelectItem key={s.requestId} value={s.requestId}>
                        {s.requestId} · {formatDistanceToNow(new Date(s.timestamp), { addSuffix: true })}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>Page {page}</span>
                <div className="flex gap-2">
                  <Button variant="outline" size="sm" disabled={page <= 1 || sessionsLoading} onClick={() => setPage(Math.max(1, page - 1))}>
                    Prev
                  </Button>
                  <Button variant="outline" size="sm" disabled={sessions.length < 1 || sessionsLoading} onClick={() => setPage(page + 1)}>
                    Next
                  </Button>
                </div>
              </div>
            </div>

            <div className="space-y-2">
              <div className="text-sm font-medium">Load by request ID</div>
              <div className="flex gap-2">
                <Input
                  value={manualId}
                  placeholder="paste request ID"
                  onChange={(e) => setManualId(e.target.value)}
                  className="flex-1"
                />
                <Button
                  variant="secondary"
                  onClick={() => manualId && selectRequestId(manualId)}
                  disabled={!manualId}
                >
                  Load
                </Button>
              </div>
              <Input
                value={filterRequestId}
                onChange={(e) => {
                  setFilterRequestId(e.target.value);
                  setPage(1);
                }}
                placeholder="filter request id..."
              />
            </div>

            <div className="space-y-2">
              <div className="text-sm font-medium">Session facts</div>
              <div className="grid grid-cols-2 gap-2 text-sm">
                <div className="rounded-md bg-muted p-2">
                  <div className="text-xs text-muted-foreground">Total tokens</div>
                  <div className="font-semibold">{totalTokensDisplay}</div>
                </div>
                <div className="rounded-md bg-muted p-2">
                  <div className="text-xs text-muted-foreground">Tokens/sec (latest)</div>
                  <div className="font-semibold">{latestTps.toFixed(2)}</div>
                </div>
                {backendName && (
                  <div className="rounded-md bg-muted p-2">
                    <div className="text-xs text-muted-foreground">Backend</div>
                    <div className="font-semibold">{backendName}</div>
                  </div>
                )}
                {workerId && (
                  <div className="rounded-md bg-muted p-2">
                    <div className="text-xs text-muted-foreground">Worker</div>
                    <div className="font-semibold">{workerId}</div>
                  </div>
                )}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Metrics */}
      <Card>
        <CardHeader>
          <CardTitle>Tokens per second</CardTitle>
          <CardDescription>Derived from advanced metrics tokens_per_second series.</CardDescription>
        </CardHeader>
        <CardContent>
          {metricsError ? (
            <ErrorRecovery error={metricsError.message} onRetry={() => refetchMetrics()} />
          ) : metricsLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : tokensPerSecond.length === 0 ? (
            <Alert>
              <AlertDescription>No metrics available for the selected window.</AlertDescription>
            </Alert>
          ) : (
            <div className="h-60 w-full">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={tokensPerSecond}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis
                    dataKey="timestamp"
                    tickFormatter={(value) => new Date(value).toLocaleTimeString()}
                    tick={{ fontSize: 12 }}
                  />
                  <YAxis tick={{ fontSize: 12 }} />
                  <ReTooltip labelFormatter={(value) => new Date(value).toLocaleTimeString()} />
                  <Line
                    type="monotone"
                    dataKey="value"
                    stroke={CHART_COLORS.primary}
                    dot={false}
                    name="tokens/sec"
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Latency p50 (ms)</CardTitle>
          <CardDescription>From advanced metrics latency_p50_ms series.</CardDescription>
        </CardHeader>
        <CardContent>
          {metricsError ? (
            <ErrorRecovery error={metricsError.message} onRetry={() => refetchMetrics()} />
          ) : metricsLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : latencyP50.length === 0 ? (
            <Alert>
              <AlertDescription>No latency metrics available for the selected window.</AlertDescription>
            </Alert>
          ) : (
            <div className="h-60 w-full">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={latencyP50}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis
                    dataKey="timestamp"
                    tickFormatter={(value) => new Date(value).toLocaleTimeString()}
                    tick={{ fontSize: 12 }}
                  />
                  <YAxis tick={{ fontSize: 12 }} />
                  <ReTooltip labelFormatter={(value) => new Date(value).toLocaleTimeString()} />
                  <Line
                    type="monotone"
                    dataKey="value"
                    stroke={CHART_COLORS.secondary ?? '#8884d8'}
                    dot={false}
                    name="latency p50 (ms)"
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Timeline */}
      <Card>
        <CardHeader>
          <CardTitle>{timelineTitle}</CardTitle>
          <CardDescription>Per-token/step routing view with adapter gates.</CardDescription>
        </CardHeader>
        <CardContent>
          {stepsError ? (
            <ErrorRecovery error={stepsError.message} onRetry={() => refetchSteps()} />
          ) : stepsLoading ? (
            <Skeleton className="h-48 w-full" />
          ) : timelineRows.length === 0 ? (
            <Alert>
              <AlertDescription>No routing steps recorded for this session.</AlertDescription>
            </Alert>
          ) : (
            <ScrollArea className="w-full">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-[calc(var(--base-unit)*25)]">Token #</TableHead>
                    <TableHead>Adapters fired</TableHead>
                    <TableHead>Token text</TableHead>
                    <TableHead>Entropy / Tau</TableHead>
                    <TableHead>Timestamp</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {timelineRows.map((row) => (
                    <TableRow key={row.key}>
                      <TableCell className="font-mono text-sm">{row.tokenIndex}</TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-2">
                          {row.adapters.map((adapter) => (
                            <Badge
                              key={`${adapter.adapter_idx}-${adapter.gate_value}`}
                              variant={adapter.selected ? 'default' : 'secondary'}
                              className={cn('text-xs', adapter.selected ? '' : 'opacity-70')}
                              title={`gate ${adapter.gate_value.toFixed(3)}`}
                            >
                              {(adapter.adapter_name || adapter.adapter_id || `idx ${adapter.adapter_idx}`)} · {adapter.gate_value.toFixed(3)}
                            </Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell className="text-sm">
                        {tokenLookup.get(row.tokenIndex) ?? '—'}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {row.entropy !== undefined ? row.entropy.toFixed(3) : '—'} /{' '}
                        {row.tau !== undefined ? row.tau.toFixed(3) : '—'}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {formatDistanceToNow(new Date(row.timestamp), { addSuffix: true })}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </ScrollArea>
          )}
        </CardContent>
      </Card>

      {currentSession && (
        <Alert>
          <AlertDescription>
            Session started {formatDistanceToNow(new Date(currentSession.timestamp), { addSuffix: true })} ·{' '}
            {currentSession.adapters.length} adapter(s)
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

export default TelemetryViewer;

