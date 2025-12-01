/**
 * TrainingStreamPage Component
 * 
 * Displays real-time training stream with adapter lifecycle events,
 * promotion/demotion events, and profiler metrics.
 * 
 * Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8.2
 */

import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { useTimestamp } from '@/hooks/useTimestamp';
import { useLiveData } from '@/hooks/useLiveData';

interface TrainingEvent {
  type: string;
  timestamp: number;
  payload: {
    adapter_id?: string;
    from_state?: string;
    to_state?: string;
    reason?: string;
    metrics?: {
      activation_pct?: number;
      avg_latency_us?: number;
      memory_bytes?: number;
    };
  };
}

interface TrainingStreamPageProps {
  selectedTenant: string;
}

interface TrainingData {
  events: TrainingEvent[];
  adapterStates: Map<string, string>;
  metricsHistory: Record<string, unknown>[];
}

export function TrainingStreamPage({ selectedTenant }: TrainingStreamPageProps) {
  const [events, setEvents] = useState<TrainingEvent[]>([]);
  const [adapterStates, setAdapterStates] = useState<Map<string, string>>(new Map());
  const [metricsHistory, setMetricsHistory] = useState<Record<string, unknown>[]>([]);

  const handleSSEMessage = useCallback((eventData: unknown) => {
    const data = eventData as TrainingEvent;

    setEvents((prev) => [data, ...prev].slice(0, 100)); // Keep last 100

    // Update adapter states
    if (
      (data.type === 'adapter_state_transition' || data.type === 'adapter_promoted') &&
      data.payload.adapter_id
    ) {
      setAdapterStates((prev) => {
        const updated = new Map(prev);
        updated.set(data.payload.adapter_id, data.payload.to_state || 'unknown');
        return updated;
      });
    }

    // Add to metrics history
    if (data.type === 'profiler_metrics' && data.payload.metrics) {
      setMetricsHistory((prev) =>
        [...prev, { timestamp: data.timestamp, ...data.payload.metrics }].slice(-60) // Keep last 60
      );
    }
  }, []);

  // Use standardized live data hook
  const { sseConnected } = useLiveData<TrainingData>({
    sseEndpoint: `/v1/streams/training?tenant=${selectedTenant}`,
    sseEventType: 'training',
    fetchFn: async () => ({ events: [], adapterStates: new Map(), metricsHistory: [] }),
    pollingSpeed: 'fast',
    enabled: true,
    onSSEMessage: handleSSEMessage,
    operationName: 'TrainingStream',
  });

  const getEventIcon = (type: string) => {
    switch (type) {
      case 'adapter_promoted':
        return '⬆️';
      case 'adapter_demoted':
        return '⬇️';
      case 'k_reduced':
        return '⚠️';
      case 'profiler_metrics':
        return '📊';
      default:
        return '📝';
    }
  };

  const getStateBadgeColor = (state: string) => {
    switch (state) {
      case 'resident':
        return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300';
      case 'hot':
        return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-300';
      case 'warm':
        return 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-300';
      case 'cold':
        return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-300';
      default:
        return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-300';
    }
  };

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold">Training Stream</h1>
        <p className="text-gray-600 dark:text-gray-400 mt-2">
          Live adapter lifecycle and training metrics
        </p>
      </div>

      {/* Adapter States */}
      <Card>
        <CardHeader>
          <CardTitle>Adapter States</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {Array.from(adapterStates.entries()).map(([adapterId, state]) => (
              <div
                key={adapterId}
                className="flex items-center justify-between p-2 border rounded"
              >
                <span className="text-sm font-medium truncate">{adapterId}</span>
                <Badge className={getStateBadgeColor(state)}>{state}</Badge>
              </div>
            ))}
          </div>
          {adapterStates.size === 0 && (
            <p className="text-gray-500 text-center py-4">No adapter states yet...</p>
          )}
        </CardContent>
      </Card>

      {/* Metrics Chart (Simple) */}
      {metricsHistory.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Recent Metrics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <div className="flex justify-between mb-2">
                  <span className="text-sm">Activation %</span>
                  <span className="text-sm font-medium">
                    {(() => {
                      // eslint-disable-next-line @typescript-eslint/no-explicit-any
                      const lastMetric = metricsHistory[metricsHistory.length - 1] as any;
                      const pct = lastMetric?.activation_pct;
                      return typeof pct === 'number' ? pct.toFixed(1) : '0.0';
                    })()}%
                  </span>
                </div>
                <Progress
                  value={(() => {
                    // eslint-disable-next-line @typescript-eslint/no-explicit-any
                    const lastMetric = metricsHistory[metricsHistory.length - 1] as any;
                    return Number(lastMetric?.activation_pct) || 0;
                  })()}
                  className="h-2"
                />
              </div>
              <div>
                <div className="flex justify-between mb-2">
                  <span className="text-sm">Avg Latency (µs)</span>
                  <span className="text-sm font-medium">
                    {(() => {
                      // eslint-disable-next-line @typescript-eslint/no-explicit-any
                      const lastMetric = metricsHistory[metricsHistory.length - 1] as any;
                      return Number(lastMetric?.avg_latency_us) || 0;
                    })()}
                  </span>
                </div>
                <Progress
                  value={(() => {
                    // eslint-disable-next-line @typescript-eslint/no-explicit-any
                    const lastMetric = metricsHistory[metricsHistory.length - 1] as any;
                    const latency = Number(lastMetric?.avg_latency_us) || 0;
                    return Math.min((latency / 1000) * 100, 100);
                  })()}
                  className="h-2"
                />
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Event Feed */}
      <Card>
        <CardHeader>
          <CardTitle>Live Events</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2 max-h-96 overflow-y-auto">
            {events.length === 0 && (
              <p className="text-gray-500 text-center py-4">Waiting for events...</p>
            )}
            {events.map((event, idx) => (
              <div key={idx} className="flex items-center gap-3 p-2 border-b">
                <span className="text-2xl">{getEventIcon(event.type)}</span>
                <div className="flex-1">
                  <div className="font-medium">{event.type.replace(/_/g, ' ').toUpperCase()}</div>
                  {event.payload.adapter_id && (
                    <div className="text-sm text-gray-600 dark:text-gray-400">
                      {event.payload.adapter_id}
                      {event.payload.from_state && event.payload.to_state && (
                        <span>
                          {' '}
                          • {event.payload.from_state} → {event.payload.to_state}
                        </span>
                      )}
                    </div>
                  )}
                  {event.payload.reason && (
                    <div className="text-xs text-gray-500">{event.payload.reason}</div>
                  )}
                </div>
                <div className="text-xs text-gray-400">
                  {useTimestamp(new Date(event.timestamp).toISOString())}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

