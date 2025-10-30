import React, { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';

interface Span {
  span_id: string;
  trace_id: string;
  parent_id: string | null;
  name: string;
  start_ns: number;
  end_ns: number | null;
  attributes: Record<string, any>;
  status: 'ok' | 'error' | 'unset';
}

interface Trace {
  trace_id: string;
  spans: Span[];
  root_span_id: string | null;
}

export function TraceTimeline() {
  const [traces, setTraces] = useState<string[]>([]);
  const [selectedTrace, setSelectedTrace] = useState<Trace | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  useEffect(() => {
    const fetchTraces = async () => {
      try {
        const data = await apiClient.request<string[]>('/api/traces/search');
        setTraces(data);
      } catch (err) {
        logger.error('Failed to fetch traces', { component: 'TraceTimeline', operation: 'fetchTraces' }, toError(err));
      }
    };

    fetchTraces();
    const interval = setInterval(fetchTraces, 5000); // Update every 5 seconds
    return () => clearInterval(interval);
  }, [searchQuery]);

  const handleTraceSelect = async (traceId: string) => {
    try {
      const trace = await apiClient.request<Trace>(`/api/traces/${traceId}`);
      setSelectedTrace(trace);
    } catch (err) {
      logger.error('Failed to fetch trace', { component: 'TraceTimeline', operation: 'fetchTrace' }, toError(err));
    }
  };

  return (
    <div className="grid gap-4 md:grid-cols-3">
      <Card className="md:col-span-1">
        <CardHeader>
          <CardTitle>Traces</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <Input
            placeholder="Search traces..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          <div className="space-y-2 max-h-96 overflow-y-auto">
            {traces.map((traceId) => (
              <Button
                key={traceId}
                variant="outline"
                className="w-full justify-start"
                onClick={() => handleTraceSelect(traceId)}
              >
                {traceId.substring(0, 16)}...
              </Button>
            ))}
            {traces.length === 0 && (
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
            <div className="space-y-4">
              <div>
                <div className="font-semibold">Trace ID:</div>
                <div className="font-mono text-sm">{selectedTrace.trace_id}</div>
              </div>
              <div className="space-y-2">
                <div className="font-semibold">Spans:</div>
                {selectedTrace.spans.map((span) => (
                  <div key={span.span_id} className="border-l-2 pl-4 py-2">
                    <div className="font-medium">{span.name}</div>
                    <div className="text-sm text-muted-foreground">
                      Duration: {span.end_ns
                        ? ((span.end_ns - span.start_ns) / 1_000_000).toFixed(2)
                        : 'ongoing'} ms
                    </div>
                    <div className="text-xs text-muted-foreground mt-1">
                      Status: {span.status}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              Select a trace to view details
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
