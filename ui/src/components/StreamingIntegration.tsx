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
      <div className="training-stream disconnected">
        <p>Training stream: Disconnected</p>
        {error && <p className="error">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  const trainingData = data as TrainingProgressEvent | null;
  return (
    <div className="training-stream">
      <h3>Training Progress</h3>
      {trainingData ? (
        <div>
          <p>Job ID: {trainingData.job_id}</p>
          <p>Status: {trainingData.status}</p>
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{ width: `${trainingData.progress_pct || 0}%` }}
            />
          </div>
          <p>{trainingData.progress_pct?.toFixed(1) || 0}% Complete</p>
          {trainingData.current_loss && <p>Loss: {trainingData.current_loss.toFixed(4)}</p>}
          {trainingData.tokens_per_second && <p>Speed: {trainingData.tokens_per_second} tokens/sec</p>}
          <p className="timestamp">Updated: {trainingData.timestamp}</p>
        </div>
      ) : (
        <p>Waiting for training events...</p>
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
      <div className="metrics-stream disconnected">
        <p>Metrics stream: Disconnected</p>
        {error && <p className="error">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  const metricsData = data as SystemMetricsEvent | null;
  return (
    <div className="metrics-stream">
      <h3>System Metrics</h3>
      {metricsData ? (
        <div className="metrics-grid">
          <div className="metric">
            <label>CPU</label>
            <span className="value">{metricsData.cpu?.usage_percent?.toFixed(1) || 0}%</span>
          </div>
          <div className="metric">
            <label>Memory</label>
            <span className="value">{metricsData.memory?.usage_percent?.toFixed(1) || 0}%</span>
          </div>
          <div className="metric">
            <label>Disk</label>
            <span className="value">{metricsData.disk?.usage_percent?.toFixed(1) || 0}%</span>
          </div>
          {metricsData.gpu && (
            <div className="metric">
              <label>GPU</label>
              <span className="value">{metricsData.gpu.utilization_percent?.toFixed(1) || 0}%</span>
            </div>
          )}
          <p className="timestamp">Updated: {metricsData.timestamp}</p>
        </div>
      ) : (
        <p>Waiting for metrics...</p>
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
      <div className="adapters-stream disconnected">
        <p>Adapter stream: Disconnected</p>
        {error && <p className="error">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  return (
    <div className="adapters-stream">
      <h3>Adapter State Changes</h3>
      {recentEvents.length > 0 ? (
        <div className="event-list">
          {recentEvents.map((item) => {
            const evt = item.event;
            if ('previous_state' in evt) {
              const stateEvent = evt as AdapterStateTransitionEvent;
              return (
                <div key={item.id} className="event-item state-change">
                  <p>
                    <strong>{stateEvent.adapter_id}</strong>
                  </p>
                  <p>
                    {stateEvent.previous_state} → {stateEvent.new_state || stateEvent.current_state}
                  </p>
                  <small>{new Date(stateEvent.timestamp).toLocaleString()}</small>
                </div>
              );
            }
            return (
              <div key={item.id} className="event-item">
                <p>
                  <strong>{evt.adapter_id}</strong>
                </p>
                <small>
                  {hasProperty(evt, 'timestamp')
                    ? typeof evt.timestamp === 'number'
                      ? new Date(evt.timestamp).toLocaleString()
                      : evt.timestamp
                    : 'N/A'}
                </small>
              </div>
            );
          })}
        </div>
      ) : (
        <p>Waiting for adapter events...</p>
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
      <div className="discovery-stream disconnected">
        <p>Discovery stream: Disconnected</p>
        {error && <p className="error">{typeof error === 'string' ? error : error.message}</p>}
      </div>
    );
  }

  return (
    <div className="discovery-stream">
      <h3>Recently Discovered Adapters</h3>
      {discoveredAdapters.length > 0 ? (
        <ul className="adapter-list">
          {discoveredAdapters.map((adapter) => (
            <li key={adapter.id}>{adapter.name}</li>
          ))}
        </ul>
      ) : (
        <p>No adapters discovered yet...</p>
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
    <div className="stream-health-monitor">
      <h3>Stream Health ({streams.length} active)</h3>
      <div className="streams-grid">
        {streams.map((stream) => (
          <div
            key={stream.endpoint}
            className={`stream-status ${stream.connected ? 'connected' : 'disconnected'}`}
          >
            <p className="endpoint">{stream.endpoint}</p>
            <p className="status">
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
    <div className="streaming-integration">
      <h2>Real-Time Streaming Integration</h2>

      {/* Stream Health Monitor */}
      <section className="monitor-section">
        <StreamHealthMonitor />
      </section>

      {/* Controls */}
      <section className="controls-section">
        <h3>Toggle Streams</h3>
        <div className="controls">
          <label>
            <input
              type="checkbox"
              checked={displayMetrics}
              onChange={(e) => setDisplayMetrics(e.target.checked)}
            />
            System Metrics
          </label>
          <label>
            <input
              type="checkbox"
              checked={displayTraining}
              onChange={(e) => setDisplayTraining(e.target.checked)}
            />
            Training Progress
          </label>
          <label>
            <input
              type="checkbox"
              checked={displayAdapters}
              onChange={(e) => setDisplayAdapters(e.target.checked)}
            />
            Adapter States
          </label>
          <label>
            <input
              type="checkbox"
              checked={displayDiscovery}
              onChange={(e) => setDisplayDiscovery(e.target.checked)}
            />
            Discovery Events
          </label>
        </div>
      </section>

      {/* Stream Displays */}
      <section className="streams-section">
        {displayMetrics && <MetricsDisplay />}
        {displayTraining && <TrainingProgressDisplay />}
        {displayAdapters && <AdapterStateDisplay />}
        {displayDiscovery && <DiscoveryDisplay />}
      </section>

      {/* Overall Status */}
      <section className="status-section">
        <h3>Overall Stream Status</h3>
        <p>
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
