# SSE Streaming Integration Guide

This guide explains how to use the Server-Sent Events (SSE) streaming infrastructure in the AdapterOS UI for real-time updates.

## Overview

The streaming infrastructure provides real-time event updates across 7 major endpoints:

| Endpoint | Purpose | Event Type | Frequency | Codebase Context |
|----------|---------|-----------|-----------|------------------|
| `/v1/streams/training` | Training job progress | `TrainingStreamEvent` | Variable | ✓ Includes codebase metadata |
| `/v1/streams/discovery` | Adapter discovery | `DiscoveryStreamEvent` | Variable | – |
| `/v1/streams/contacts` | Collaboration events | `ContactStreamEvent` | Variable | – |
| `/v1/streams/file-changes` | File system changes | `FileChangeStreamEvent` | Variable | ✓ Contributes to context |
| `/v1/stream/metrics` | System metrics | `MetricsStreamEvent` | 5-sec interval | – |
| `/v1/stream/telemetry` | Telemetry events | `TelemetryStreamEvent` | Variable | – |
| `/v1/stream/adapters` | Adapter lifecycle | `AdapterStreamEvent` | Variable | ✓ Codebase state transitions |

## Architecture

### Components

1. **Streaming Types** (`src/api/streaming-types.ts`)
   - Strongly-typed definitions for all SSE events
   - Type guards and helpers
   - Discriminated unions for type safety

2. **Streaming Service** (`src/services/StreamingService.ts`)
   - Singleton service managing all SSE connections
   - Connection lifecycle and reconnection logic
   - Convenience methods for each endpoint

3. **Hooks**
   - **Base Hook** (`src/hooks/useSSE.ts`) - Low-level SSE subscription hook
   - **Specialized Hooks** (`src/hooks/useStreamingEndpoints.ts`) - Type-safe hooks for each endpoint
   - Built on React hooks with proper cleanup

4. **Integration Component** (`src/components/StreamingIntegration.tsx`)
   - Reference implementation showing all features
   - Real-time metric display
   - Event aggregation example

## Usage

### Option 1: Using the Streaming Service (Recommended for Complex Logic)

For more control and direct connection management:

```typescript
import { streamingService } from '../services/StreamingService';

// In a component or effect
const trainingSubscription = streamingService.subscribeToTraining({
  onMessage: (event) => {
    console.log('Training progress:', event.progress_pct);
  },
  onError: (error) => {
    console.error('Stream error:', error);
  },
  autoReconnect: true,
});

// Later: clean up
trainingSubscription.unsubscribe();
```

### Option 2: Using React Hooks (Recommended for Components)

For React components with automatic cleanup on unmount:

```typescript
import { useTrainingStream, useMetricsStream } from '../hooks/useStreamingEndpoints';

export function MyComponent() {
  // Training progress stream
  const { data: trainingData, error, connected, reconnect } = useTrainingStream({
    enabled: true,
    onMessage: (event) => {
      console.log('Progress:', event.progress_pct);
    },
  });

  // System metrics stream
  const { data: metricsData } = useMetricsStream({
    enabled: true,
  });

  return (
    <div>
      {connected ? 'Connected' : 'Disconnected'}
      {trainingData && <p>Progress: {trainingData.progress_pct}%</p>}
      {metricsData && <p>CPU: {metricsData.cpu.usage_percent}%</p>}
      {error && <p className="error">{error}</p>}
      <button onClick={reconnect}>Reconnect</button>
    </div>
  );
}
```

## Event Types

### Training Stream Events

```typescript
interface TrainingProgressEvent {
  job_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress_pct: number;
  current_epoch?: number;
  current_loss?: number;
  tokens_per_second?: number;
  timestamp: string;
}
```

### Metrics Stream Events

```typescript
interface SystemMetricsEvent {
  timestamp: string;
  cpu: { usage_percent: number; cores: number; temp_celsius?: number };
  memory: { used_gb: number; total_gb: number; usage_percent: number };
  disk: { used_gb: number; total_gb: number; usage_percent: number };
  network?: { rx_bytes?: number; tx_bytes?: number };
  gpu?: { utilization_percent?: number; memory_used_mb?: number };
}
```

### Adapter Stream Events

```typescript
interface AdapterStateTransitionEvent {
  adapter_id: string;
  tenant_id: string;
  previous_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  new_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  trigger: 'activation' | 'eviction' | 'manual' | 'timeout' | 'memory_pressure';
  timestamp: string;
}
```

See `src/api/streaming-types.ts` for complete type definitions.

### Codebase Adapter Context Capture

When a session is bound to a codebase adapter, specific streams contribute to the adapter's evolving context. This context is used for training and deterministic replay.

#### Streams That Contribute to Codebase Context

| Stream | Context Contribution | When Captured |
|--------|---------------------|---------------|
| `/v1/streams/file-changes` | Repository file modifications, additions, deletions | Real-time as files change |
| `/v1/streams/training` | Training progress with codebase metadata | During codebase adapter training |
| `/v1/stream/adapters` | Codebase adapter state transitions (bind/unbind/freeze) | On lifecycle events |

#### File Changes Stream for Codebase Adapters

The file-changes stream emits events that are recorded as part of the codebase adapter's context digest:

```typescript
interface CodebaseFileChangeEvent extends FileChangeStreamEvent {
  // Standard file change fields
  path: string;
  change_type: 'created' | 'modified' | 'deleted' | 'renamed';
  timestamp: string;

  // Codebase-specific fields (present when session is bound)
  codebase_adapter_id?: string;
  stream_id?: string;
  context_contribution?: {
    diff_hash: string;
    lines_added: number;
    lines_removed: number;
  };
}
```

#### Filtering Codebase Events

To filter for events relevant to a specific codebase adapter:

```typescript
import { useFileChangesStream, useAdaptersStream } from '../hooks/useStreamingEndpoints';

function CodebaseContextMonitor({ codebaseAdapterId }: { codebaseAdapterId: string }) {
  const [contextEvents, setContextEvents] = useState<CodebaseFileChangeEvent[]>([]);

  // Filter file changes for this codebase adapter
  useFileChangesStream({
    onMessage: (event) => {
      if ('codebase_adapter_id' in event && event.codebase_adapter_id === codebaseAdapterId) {
        setContextEvents((prev) => [...prev, event].slice(-50));
      }
    },
  });

  // Track codebase adapter state transitions
  const [adapterState, setAdapterState] = useState<string>('unknown');
  useAdaptersStream({
    onMessage: (event) => {
      if ('adapter_id' in event && event.adapter_id === codebaseAdapterId) {
        setAdapterState(event.new_state);
      }
    },
  });

  return (
    <div>
      <p>Adapter State: {adapterState}</p>
      <p>Context Events: {contextEvents.length}</p>
      <EventList events={contextEvents} />
    </div>
  );
}
```

#### Training Stream with Codebase Metadata

When training a codebase adapter, the training stream includes additional metadata:

```typescript
interface CodebaseTrainingProgressEvent extends TrainingProgressEvent {
  // Standard training fields
  job_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress_pct: number;

  // Codebase-specific fields
  codebase_metadata?: {
    repo_id: string;
    repo_commit: string;
    base_adapter_id: string;
    context_digest: string;
    version: string;
  };
}
```

#### Context Accumulation Pattern

For codebase adapters, context accumulates across the session until the adapter is frozen:

```typescript
function CodebaseContextAccumulator({ sessionId }: { sessionId: string }) {
  const [contextDigest, setContextDigest] = useState<string | null>(null);
  const [eventCount, setEventCount] = useState(0);

  useFileChangesStream({
    onMessage: (event) => {
      if ('context_contribution' in event) {
        setEventCount((c) => c + 1);
        // Server maintains running digest; this is for display only
        if (event.context_contribution?.diff_hash) {
          setContextDigest(event.context_contribution.diff_hash);
        }
      }
    },
  });

  return (
    <div className="context-status">
      <span>Session: {sessionId}</span>
      <span>Context Events: {eventCount}</span>
      {contextDigest && <span>Latest Digest: {contextDigest.slice(0, 8)}...</span>}
    </div>
  );
}
```

> **Note**: The context digest is computed server-side and included in the codebase adapter's manifest. Client-side event counts are for display purposes; the authoritative context is maintained by the backend.

## Advanced Patterns

### Aggregating Multiple Streams

```typescript
import {
  useTrainingStream,
  useMetricsStream,
  useAdaptersStream,
} from '../hooks/useStreamingEndpoints';

export function DashboardPanel() {
  const training = useTrainingStream();
  const metrics = useMetricsStream();
  const adapters = useAdaptersStream();

  const isHealthy = metrics.data && metrics.data.cpu.usage_percent < 80;
  const isTraining = training.data?.status === 'running';

  return (
    <div>
      <HealthIndicator healthy={isHealthy} />
      {isTraining && <TrainingProgress data={training.data} />}
      <MetricsChart data={metrics.data} />
      <AdaptersList adapters={adapters.data} />
    </div>
  );
}
```

### Conditional Streaming

```typescript
function ConditionalStreamComponent({ shouldStream }: { shouldStream: boolean }) {
  const { data } = useMetricsStream({
    enabled: shouldStream, // Only connect when needed
  });

  return shouldStream ? <MetricsDisplay metrics={data} /> : <Placeholder />;
}
```

### Event Filtering and Transformation

```typescript
function FilteredAdapterStream() {
  const [recentStateChanges, setRecentStateChanges] = useState<AdapterStateTransitionEvent[]>([]);

  const { data } = useAdaptersStream({
    onMessage: (event) => {
      // Filter: only state transition events
      if ('previous_state' in event && 'new_state' in event) {
        setRecentStateChanges((prev) => [event, ...prev.slice(0, 9)]);
      }
    },
  });

  return <StateChangesList changes={recentStateChanges} />;
}
```

### Batching Updates

```typescript
function BatchedMetricsComponent() {
  const [metricsHistory, setMetricsHistory] = useState<SystemMetricsEvent[]>([]);
  const batchRef = useRef<SystemMetricsEvent[]>([]);

  useEffect(() => {
    const { unsubscribe } = streamingService.subscribeToMetrics({
      onMessage: (event) => {
        batchRef.current.push(event);

        // Batch updates every 10 events or 5 seconds
        if (batchRef.current.length >= 10) {
          setMetricsHistory((prev) => [...prev, ...batchRef.current].slice(-100));
          batchRef.current = [];
        }
      },
    });

    const timer = setInterval(() => {
      if (batchRef.current.length > 0) {
        setMetricsHistory((prev) => [...prev, ...batchRef.current].slice(-100));
        batchRef.current = [];
      }
    }, 5000);

    return () => {
      unsubscribe();
      clearInterval(timer);
    };
  }, []);

  return <MetricsChart data={metricsHistory} />;
}
```

## Error Handling

All streams have built-in error handling and reconnection:

```typescript
const { data, error, connected, reconnect } = useMetricsStream({
  onError: (event) => {
    logger.error('Stream error occurred', { event });
    // Optional: notify user, trigger fallback, etc.
  },
});

// Manual reconnection
{error && <button onClick={reconnect}>Reconnect</button>}
```

### Reconnection Strategy

- **Exponential backoff**: Starts at 1s, doubles each attempt, capped at 30s
- **Max attempts**: 10 reconnection attempts before giving up
- **Auto-reconnect**: Enabled by default, can be disabled via config

## Authentication

SSE connections use token-based authentication:

```typescript
// Token automatically appended from apiClient
// URL becomes: /api/v1/stream/metrics?token=<jwt>

// Server-side: extract token from query params
// Authorization header cannot be used with EventSource
```

See the API client for token management.

## Performance Considerations

1. **Connection Pooling**: Reuse subscriptions rather than creating new ones
2. **Selective Subscription**: Only subscribe to streams you need
3. **Disable When Not Visible**: Use the `enabled` option to disable streams in background tabs
4. **Event Aggregation**: Batch multiple events before state updates
5. **Cleanup**: Always unsubscribe or let hooks handle cleanup on unmount

## Monitoring Stream Health

```typescript
import { useAllStreamsStatus } from '../hooks/useStreamingEndpoints';

export function StreamMonitor() {
  const status = useAllStreamsStatus();

  return (
    <div>
      <p>Training: {status.training ? '✓' : '✗'}</p>
      <p>Metrics: {status.metrics ? '✓' : '✗'}</p>
      <p>Adapters: {status.adapters ? '✓' : '✗'}</p>
      <p>All: {status.allConnected ? '✓ Connected' : '✗ Disconnected'}</p>
    </div>
  );
}
```

## Debugging

Enable detailed logging:

```typescript
// Stream events are logged with:
// component: 'StreamingService'
// operation: 'subscribe', 'connect', 'reconnect', etc.
// endpoint: '/v1/stream/metrics'

// Check active subscriptions:
const subscriptions = streamingService.getActiveSubscriptions();
console.log('Active streams:', subscriptions);
```

## Testing

For testing components with streams:

```typescript
import { render, waitFor } from '@testing-library/react';
import { useMetricsStream } from '../hooks/useStreamingEndpoints';

// Mock the hook:
jest.mock('../hooks/useStreamingEndpoints', () => ({
  useMetricsStream: jest.fn(() => ({
    data: { cpu: { usage_percent: 50 } },
    error: null,
    connected: true,
    reconnect: jest.fn(),
  })),
}));

// Test component:
test('renders metrics', () => {
  const { getByText } = render(<MetricsComponent />);
  expect(getByText('50')).toBeInTheDocument();
});
```

## Best Practices

1. **Always provide cleanup**
   ```typescript
   useEffect(() => {
     const sub = streamingService.subscribeToMetrics({ ... });
     return () => sub.unsubscribe(); // Cleanup
   }, []);
   ```

2. **Memoize callbacks**
   ```typescript
   const handleMessage = useCallback((event) => {
     // Handle event
   }, [dependency]);
   const { data } = useMetricsStream({ onMessage: handleMessage });
   ```

3. **Separate concerns**
   - Service for connection management
   - Hooks for React integration
   - Components for UI rendering

4. **Type safety**
   - Use proper type guards for discriminated unions
   - Leverage TypeScript for compile-time safety
   - Avoid `any` types

5. **Error recovery**
   - Provide user feedback on connection issues
   - Implement fallback UI
   - Log errors for debugging

## Troubleshooting

### Stream not receiving events
- Check network tab for SSE connection
- Verify server is sending events
- Ensure `enabled: true` in hook config
- Check authentication token is valid

### Frequent reconnections
- Check server logs for errors
- Review network stability
- Consider disabling client-side reconnection for debugging
- Increase `maxBackoffMs` if server is recovering

### Memory leaks
- Ensure cleanup on component unmount
- Unsubscribe from unused streams
- Check for circular references in event handlers

### High CPU usage
- Reduce event frequency via batching
- Disable streams when not visible
- Optimize event handlers (avoid inline functions)
- Monitor with `streamingService.getActiveSubscriptions()`

## Examples

See `src/components/StreamingIntegration.tsx` for a complete reference implementation with:
- Multiple stream subscriptions
- Real-time metric display
- State aggregation
- Error handling
- Connection monitoring

## Future Enhancements

Potential improvements for consideration:

1. **Stream Compression**: Gzip compression for large payloads
2. **Client-side Filtering**: Server-side event filtering to reduce bandwidth
3. **Message Queuing**: Guarantee message delivery with local queue
4. **Metrics Aggregation**: Built-in time-series metrics
5. **WebSocket Fallback**: For environments with poor SSE support
6. **Stream Replay**: Replay recent events on reconnect

## Related Documentation

- [AGENTS.md](../AGENTS.md) - Project conventions and architecture
- [src/api/streaming-types.ts](src/api/streaming-types.ts) - Type definitions
- [src/services/StreamingService.ts](src/services/StreamingService.ts) - Service implementation
- [src/hooks/useStreamingEndpoints.ts](src/hooks/useStreamingEndpoints.ts) - React hooks
- [REST API Reference](../AGENTS.md#rest-api-reference) - API endpoint documentation
