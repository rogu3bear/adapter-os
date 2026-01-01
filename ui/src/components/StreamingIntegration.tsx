/**
 * Streaming Integration Component
 *
 * Demonstrates how to use the SSE streaming endpoints and hooks for real-time
 * updates across the AdapterOS UI. This component shows best practices for:
 * - Subscribing to streams with proper cleanup
 * - Handling connection states and errors
 * - Aggregating multiple stream data
 * - Displaying real-time metrics and events
 *
 * This is a reference implementation that can be adapted for specific features.
 */

import React, { useState, useCallback, useMemo } from 'react';
import {
  useTrainingStream,
  useMetricsStream,
  useAdaptersStream,
  useDiscoveryStream,
  useAllStreamsStatus,
} from '@/hooks/streaming/useStreamingEndpoints';
import { streamingService } from '@/services/StreamingService';
import { logger } from '@/utils/logger';
import type {
  TrainingProgressEvent,
  SystemMetricsEvent,
  AdapterStreamEvent,
  AdapterStateTransitionEvent,
} from '@/api/streaming-types';
import { hasProperty } from '@/types/utilities';

// ============================================================================
// Types
// ============================================================================

interface StreamStatus {
  endpoint: string;
  connected: boolean;
  lastEvent?: string;
  eventCount: number;
}

// ============================================================================
// Subcomponents
// ============================================================================

/**
 * Displays training progress in real-time
 */
function TrainingProgressDisplay() {
  const { data, error, connected } = useTrainingStream({
    enabled: true,
    onMessage: (event) => {
      logger.debug('Training event', {
        component: 'TrainingProgressDisplay',
        job_id: event.job_id,
        progress: hasProperty(event, 'progress_pct') ? event.progress_pct : undefined,
      });
    },
  });

  if (!connected && !data) {
    return (
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-sm text-foreground">Training stream: Disconnected</p>
        {error && <p className="mt-2 text-sm text-red-600">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  const trainingData = data as TrainingProgressEvent | null;
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <h3 className="font-semibold text-lg text-foreground">Training Progress</h3>
      {trainingData ? (
        <div className="mt-4 space-y-2">
          <p className="text-sm">Job ID: {trainingData.job_id}</p>
          <p className="text-sm">Status: {trainingData.status}</p>
          <div className="w-full bg-gray-200 rounded-full h-2 overflow-hidden">
            <div
              className="bg-primary h-full transition-all duration-300"
              style={{ width: `${trainingData.progress_pct || 0}%` }}
            />
          </div>
          <p className="text-sm">{trainingData.progress_pct?.toFixed(1) || 0}% Complete</p>
          {trainingData.current_loss && <p className="text-sm">Loss: {trainingData.current_loss.toFixed(4)}</p>}
          {trainingData.tokens_per_second && <p className="text-sm">Speed: {trainingData.tokens_per_second} tokens/sec</p>}
          <p className="text-xs text-muted-foreground">Updated: {trainingData.timestamp}</p>
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">Waiting for training events...</p>
      )}
    </div>
  );
}

/**
 * Displays system metrics in real-time
 */
function MetricsDisplay() {
  const { data, error, connected } = useMetricsStream({
    enabled: true,
    onMessage: (event) => {
      logger.debug('Metrics event', {
        component: 'MetricsDisplay',
        cpu: hasProperty(event, 'cpu') && hasProperty(event.cpu, 'usage_percent')
          ? event.cpu.usage_percent
          : undefined,
        memory: hasProperty(event, 'memory') && hasProperty(event.memory, 'usage_percent')
          ? event.memory.usage_percent
          : undefined,
      });
    },
  });

  if (!connected && !data) {
    return (
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-sm text-foreground">Metrics stream: Disconnected</p>
        {error && <p className="mt-2 text-sm text-red-600">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  const metricsData = data as SystemMetricsEvent | null;
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <h3 className="font-semibold text-lg text-foreground">System Metrics</h3>
      {metricsData ? (
        <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
          <div className="rounded-md border border-border bg-muted p-3">
            <p className="text-xs font-medium text-muted-foreground">CPU</p>
            <p className="mt-1 text-lg font-semibold text-foreground">{metricsData.cpu?.usage_percent?.toFixed(1) || 0}%</p>
          </div>
          <div className="rounded-md border border-border bg-muted p-3">
            <p className="text-xs font-medium text-muted-foreground">Memory</p>
            <p className="mt-1 text-lg font-semibold text-foreground">{metricsData.memory?.usage_percent?.toFixed(1) || 0}%</p>
          </div>
          <div className="rounded-md border border-border bg-muted p-3">
            <p className="text-xs font-medium text-muted-foreground">Disk</p>
            <p className="mt-1 text-lg font-semibold text-foreground">{metricsData.disk?.usage_percent?.toFixed(1) || 0}%</p>
          </div>
          {metricsData.gpu && (
            <div className="rounded-md border border-border bg-muted p-3">
              <p className="text-xs font-medium text-muted-foreground">GPU</p>
              <p className="mt-1 text-lg font-semibold text-foreground">{metricsData.gpu.utilization_percent?.toFixed(1) || 0}%</p>
            </div>
          )}
          <p className="col-span-2 text-xs text-muted-foreground md:col-span-4">Updated: {metricsData.timestamp}</p>
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">Waiting for metrics...</p>
      )}
    </div>
  );
}

/**
 * Displays adapter state transitions in real-time
 */
function AdapterStateDisplay() {
  const [recentEvents, setRecentEvents] = useState<Array<{ id: string; event: AdapterStreamEvent }>>(
    []
  );

  const { data, error, connected } = useAdaptersStream({
    enabled: true,
    onMessage: (event) => {
      const adapterId = hasProperty(event, 'adapter_id') ? String(event.adapter_id) : 'unknown';
      const id = `${adapterId}-${Date.now()}`;
      setRecentEvents((prev) => [{ id, event }, ...prev.slice(0, 9)]);
    },
  });

  if (!connected && recentEvents.length === 0) {
    return (
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-sm text-foreground">Adapter stream: Disconnected</p>
        {error && <p className="mt-2 text-sm text-red-600">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <h3 className="font-semibold text-lg text-foreground">Adapter State Changes</h3>
      {recentEvents.length > 0 ? (
        <div className="mt-4 space-y-2">
          {recentEvents.map((item) => {
            const evt = item.event;
            if ('previous_state' in evt) {
              const stateEvent = evt as AdapterStateTransitionEvent;
              return (
                <div key={item.id} className="rounded border border-border bg-muted p-3">
                  <p className="font-medium text-sm text-foreground">
                    {stateEvent.adapter_id}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {stateEvent.previous_state} → {stateEvent.new_state || stateEvent.current_state}
                  </p>
                  <p className="text-xs text-muted-foreground">{new Date(stateEvent.timestamp).toLocaleString()}</p>
                </div>
              );
            }
            return (
              <div key={item.id} className="rounded border border-border bg-muted p-3">
                <p className="font-medium text-sm text-foreground">
                  {evt.adapter_id}
                </p>
                <p className="text-xs text-muted-foreground">
                  {hasProperty(evt, 'timestamp')
                    ? typeof evt.timestamp === 'number'
                      ? new Date(evt.timestamp).toLocaleString()
                      : evt.timestamp
                    : 'N/A'}
                </p>
              </div>
            );
          })}
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">Waiting for adapter events...</p>
      )}
    </div>
  );
}

/**
 * Displays discovery events in real-time
 */
function DiscoveryDisplay() {
  const [discoveredAdapters, setDiscoveredAdapters] = useState<Array<{ id: string; name: string }>>(
    []
  );

  const { data, error, connected } = useDiscoveryStream({
    enabled: true,
    onMessage: (event) => {
      if ('adapter_id' in event && 'name' in event) {
        setDiscoveredAdapters((prev) => {
          const updated = [...prev];
          const index = updated.findIndex((a) => a.id === event.adapter_id);
          if (index >= 0) {
            updated[index] = { id: event.adapter_id, name: event.name };
          } else {
            updated.unshift({ id: event.adapter_id, name: event.name });
          }
          return updated.slice(0, 5);
        });
      }
    },
  });

  if (!connected && discoveredAdapters.length === 0) {
    return (
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-sm text-foreground">Discovery stream: Disconnected</p>
        {error && <p className="mt-2 text-sm text-red-600">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <h3 className="font-semibold text-lg text-foreground">Recently Discovered Adapters</h3>
      {discoveredAdapters.length > 0 ? (
        <ul className="mt-4 space-y-2">
          {discoveredAdapters.map((adapter) => (
            <li key={adapter.id} className="rounded border border-border bg-muted px-3 py-2 text-sm text-foreground">
              {adapter.name}
            </li>
          ))}
        </ul>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">No adapters discovered yet...</p>
      )}
    </div>
  );
}

/**
 * Stream health monitor - shows connection status of all streams
 */
function StreamHealthMonitor() {
  const [streams, setStreams] = useState<StreamStatus[]>([]);

  React.useEffect(() => {
    const updateStatus = () => {
      const activeStreams = streamingService.getActiveSubscriptions();
      setStreams(
        activeStreams.map((stream) => ({
          endpoint: stream.endpoint,
          connected: stream.connected,
          eventCount: 0, // Could be tracked with counters
        }))
      );
    };

    const interval = setInterval(updateStatus, 2000);
    updateStatus();

    return () => clearInterval(interval);
  }, []);

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <h3 className="font-semibold text-lg text-foreground">Stream Health ({streams.length} active)</h3>
      <div className="mt-4 grid grid-cols-1 gap-2 md:grid-cols-2">
        {streams.map((stream) => (
          <div
            key={stream.endpoint}
            className={`rounded border p-3 ${
              stream.connected
                ? 'border-green-200 bg-green-50 dark:border-green-900 dark:bg-green-950'
                : 'border-red-200 bg-red-50 dark:border-red-900 dark:bg-red-950'
            }`}
          >
            <p className="text-sm font-medium text-foreground break-all">{stream.endpoint}</p>
            <p className={`text-sm font-medium mt-1 ${stream.connected ? 'text-green-600' : 'text-red-600'}`}>
              {stream.connected ? '✓ Connected' : '✗ Disconnected'}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// Main Integration Component
// ============================================================================

/**
 * Main streaming integration component
 * Demonstrates all available streaming endpoints in action
 */
export function StreamingIntegration() {
  const [displayMetrics, setDisplayMetrics] = useState(true);
  const [displayTraining, setDisplayTraining] = useState(true);
  const [displayAdapters, setDisplayAdapters] = useState(true);
  const [displayDiscovery, setDisplayDiscovery] = useState(true);

  // Get overall stream status
  const allStreamsStatus = useAllStreamsStatus();

  return (
    <div className="space-y-6 p-6">
      <h2 className="text-2xl font-bold text-foreground">Real-Time Streaming Integration</h2>

      {/* Stream Health Monitor */}
      <section className="space-y-3">
        <StreamHealthMonitor />
      </section>

      {/* Controls */}
      <section className="rounded-lg border border-border bg-card p-4">
        <h3 className="font-semibold text-lg text-foreground">Toggle Streams</h3>
        <div className="mt-4 flex flex-wrap gap-4">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={displayMetrics}
              onChange={(e) => setDisplayMetrics(e.target.checked)}
              className="rounded border border-border"
            />
            <span className="text-sm text-foreground">System Metrics</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={displayTraining}
              onChange={(e) => setDisplayTraining(e.target.checked)}
              className="rounded border border-border"
            />
            <span className="text-sm text-foreground">Training Progress</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={displayAdapters}
              onChange={(e) => setDisplayAdapters(e.target.checked)}
              className="rounded border border-border"
            />
            <span className="text-sm text-foreground">Adapter States</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={displayDiscovery}
              onChange={(e) => setDisplayDiscovery(e.target.checked)}
              className="rounded border border-border"
            />
            <span className="text-sm text-foreground">Discovery Events</span>
          </label>
        </div>
      </section>

      {/* Stream Displays */}
      <section className="space-y-4">
        {displayMetrics && <MetricsDisplay />}
        {displayTraining && <TrainingProgressDisplay />}
        {displayAdapters && <AdapterStateDisplay />}
        {displayDiscovery && <DiscoveryDisplay />}
      </section>

      {/* Overall Status */}
      <section className="rounded-lg border border-border bg-card p-4">
        <h3 className="font-semibold text-lg text-foreground">Overall Stream Status</h3>
        <p className="mt-2 text-sm text-foreground">
          {Object.values(allStreamsStatus)
            .filter((v) => typeof v === 'boolean')
            .filter(Boolean).length}{' '}
          of{' '}
          {Object.values(allStreamsStatus)
            .filter((v) => typeof v === 'boolean').length}{' '}
          streams connected
        </p>
      </section>
    </div>
  );
}

export default StreamingIntegration;
