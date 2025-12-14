import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import type { TraceResponseV1 } from '@/api/types';

interface Span {
  span_id: string;
  trace_id: string;
  parent_id: string;
  name: string;
  start_ns: number;
  end_ns: number;
  attributes?: Record<string, unknown>;
  status: string;
  start_time?: string;
  end_time?: string;
  service_name?: string;
  kind?: string;
}

interface Trace {
  trace_id: string;
  spans: Span[];
  root_span_id: string | null;
}

export function TraceTimeline() {
  const [traces, setTraces] = useState<string[]>([]);
  const [selectedTrace, setSelectedTrace] = useState<Trace | TraceResponseV1 | null>(null);
  const [searchParams, setSearchParams] = useState({
    span_name: '',
    status: '',
    start_time_ns: '',
    end_time_ns: '',
  });

  const fetchTraces = async (): Promise<string[]> => {
    const params: Record<string, string | number> = {};
    if (searchParams.span_name) params.span_name = searchParams.span_name;
    if (searchParams.status) params.status = searchParams.status;
    if (searchParams.start_time_ns) params.start_time_ns = new Date(searchParams.start_time_ns).getTime() * 1_000_000; // Convert to nanoseconds
    if (searchParams.end_time_ns) params.end_time_ns = new Date(searchParams.end_time_ns).getTime() * 1_000_000;

    const data = await apiClient.searchTraces(Object.keys(params).length > 0 ? params : undefined);
    return data;
  };

  const { data: polledTraces, isLoading: loading, refetch } = usePolling(
    fetchTraces,
    'slow',
    {
      showLoadingIndicator: false,
      enabled: false, // Disable auto-polling, we'll refetch manually and on searchParams change
      onSuccess: (data) => {
        setTraces(data as string[]);
      },
      onError: (err) => {
        logger.error('Failed to fetch traces', { component: 'TraceTimeline', operation: 'fetchTraces' }, err);
      }
    }
  );

  // Refetch when searchParams change
  React.useEffect(() => {
    refetch();
  }, [searchParams, refetch]);

  // Update traces when polling data changes
  React.useEffect(() => {
    if (!polledTraces) return;
    setTraces(polledTraces);
  }, [polledTraces]);

  const handleTraceSelect = async (traceId: string) => {
    try {
      const trace = await apiClient.getTrace(traceId);
      if (trace && 'spans' in trace) {
        setSelectedTrace(trace as Trace);
      } else {
        setSelectedTrace(trace as TraceResponseV1 | null);
      }
    } catch (err) {
      logger.error('Failed to fetch trace', { component: 'TraceTimeline', operation: 'fetchTrace' }, toError(err));
    }
  };

  return (
    <div className="grid gap-4 md:grid-cols-3">
      <Card className="md:col-span-1">
        <CardHeader>
          <CardTitle>Trace Search</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Search Filters */}
          <div className="space-y-3">
            <Input
              placeholder="Span name..."
              value={searchParams.span_name}
              onChange={(e) => setSearchParams({ ...searchParams, span_name: e.target.value })}
            />
            <select
              value={searchParams.status}
              onChange={(e) => setSearchParams({ ...searchParams, status: e.target.value })}
              className="w-full px-3 py-2 border rounded"
            >
              <option value="">Any status</option>
              <option value="ok">OK</option>
              <option value="error">Error</option>
              <option value="unset">Unset</option>
            </select>
            <Input
              type="datetime-local"
              placeholder="Start time"
              value={searchParams.start_time_ns}
              onChange={(e) => setSearchParams({ ...searchParams, start_time_ns: e.target.value })}
            />
            <Input
              type="datetime-local"
              placeholder="End time"
              value={searchParams.end_time_ns}
              onChange={(e) => setSearchParams({ ...searchParams, end_time_ns: e.target.value })}
            />
            <Button
              variant="outline"
              onClick={() => setSearchParams({
                span_name: '',
                status: '',
                start_time_ns: '',
                end_time_ns: '',
              })}
              className="w-full"
            >
              Clear Filters
            </Button>
          </div>

          {/* Trace List */}
          <div className="space-y-2 max-h-96 overflow-y-auto">
            {loading ? (
              <div className="text-center py-4 text-muted-foreground">
                Searching traces...
              </div>
            ) : traces.length > 0 ? (
              traces.map((traceId) => (
                <Button
                  key={traceId}
                  variant={selectedTrace?.trace_id === traceId ? "default" : "outline"}
                  className="w-full justify-start text-left h-auto p-3"
                  onClick={() => handleTraceSelect(traceId)}
                >
                  <div className="font-mono text-xs break-all">
                    {traceId}
                  </div>
                </Button>
              ))
            ) : (
              <div className="text-center py-4 text-muted-foreground">
                No traces found
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <Card className="md:col-span-2">
        <CardHeader>
          <CardTitle>Trace Details</CardTitle>
        </CardHeader>
        <CardContent>
          {selectedTrace ? (
            'spans' in selectedTrace ? (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <div className="font-semibold text-sm text-muted-foreground">Trace ID</div>
                    <div className="font-mono text-sm break-all">{selectedTrace.trace_id}</div>
                  </div>
                  <div>
                    <div className="font-semibold text-sm text-muted-foreground">Root Span</div>
                    <div className="font-mono text-sm">{selectedTrace.root_span_id || 'N/A'}</div>
                  </div>
                </div>

                <div className="space-y-2">
                  <div className="font-semibold">Span Timeline ({selectedTrace.spans.length} spans)</div>
                  <div className="space-y-1">
                    {selectedTrace.spans
                      .sort((a, b) => a.start_ns - b.start_ns)
                      .map((span) => {
                        const duration = span.end_ns ? (span.end_ns - span.start_ns) / 1_000_000 : 0;
                        const startTime = new Date(span.start_ns / 1_000_000).toLocaleTimeString();
                        const statusColor = span.status === 'error' ? 'border-red-500' :
                                          span.status === 'ok' ? 'border-green-500' : 'border-gray-500';

                        return (
                          <div key={span.span_id} className={`border-l-4 pl-4 py-3 ${statusColor} bg-muted/20`}>
                            <div className="flex items-center justify-between">
                              <div className="font-medium">{span.name}</div>
                              <div className="text-xs text-muted-foreground">
                                {duration.toFixed(2)}ms
                              </div>
                            </div>
                            <div className="text-sm text-muted-foreground mt-1">
                              Started: {startTime}
                            </div>
                            <div className="text-xs text-muted-foreground">
                              ID: {span.span_id.substring(0, 16)}... | Status: {span.status}
                            </div>
                            {span.attributes && Object.keys(span.attributes).length > 0 && (
                              <div className="mt-2 text-xs">
                                <div className="font-medium text-muted-foreground">Attributes:</div>
                                <div className="font-mono bg-muted p-2 rounded mt-1 max-h-20 overflow-y-auto">
                                  {Object.entries(span.attributes).map(([key, value]) => (
                                    <div key={key}>{key}: {String(value)}</div>
                                  ))}
                                </div>
                              </div>
                            )}
                          </div>
                        );
                      })}
                  </div>
                </div>
              </div>
            ) : (
              <div className="space-y-3">
                <div>
                  <div className="font-semibold text-sm text-muted-foreground">Trace ID</div>
                  <div className="font-mono text-sm break-all">{selectedTrace.trace_id}</div>
                </div>
                {'tokens' in selectedTrace && (
                  <div className="text-sm text-muted-foreground">
                    Tokens recorded: {selectedTrace.tokens.length}
                  </div>
                )}
              </div>
            )
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              Select a trace to view its span timeline and details
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
