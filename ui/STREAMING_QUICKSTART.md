# SSE Streaming - Quick Start Guide

Get real-time updates from the AdapterOS backend in 5 minutes.

## Installation

All files are already created and integrated. No additional dependencies needed.

**Files Added:**
- `/src/api/streaming-types.ts` - Type definitions (350+ lines)
- `/src/services/StreamingService.ts` - Connection management (380+ lines)
- `/src/hooks/useStreamingEndpoints.ts` - React hooks (280+ lines)
- `/src/components/StreamingIntegration.tsx` - Reference implementation (470+ lines)
- `/src/streaming/index.ts` - Central exports
- `/STREAMING_GUIDE.md` - Comprehensive documentation
- `/STREAMING_QUICKSTART.md` - This file

## Available Streams

| Hook | Endpoint | Purpose |
|------|----------|---------|
| `useTrainingStream()` | `/v1/streams/training` | Training progress |
| `useMetricsStream()` | `/v1/stream/metrics` | System metrics (5-sec) |
| `useAdaptersStream()` | `/v1/stream/adapters` | Adapter state changes |
| `useDiscoveryStream()` | `/v1/streams/discovery` | Adapter discovery |
| `useContactsStream()` | `/v1/streams/contacts` | Collaboration events |
| `useFileChangesStream()` | `/v1/streams/file-changes` | File changes |
| `useTelemetryStream()` | `/v1/stream/telemetry` | Telemetry events |

## 1-Minute Examples

### Display System Metrics

```typescript
import { useMetricsStream } from '../streaming';

export function SystemStatus() {
  const { data, connected } = useMetricsStream();

  if (!connected) return <p>Loading metrics...</p>;

  return (
    <div>
      <p>CPU: {data?.cpu.usage_percent.toFixed(1)}%</p>
      <p>Memory: {data?.memory.usage_percent.toFixed(1)}%</p>
      <p>Disk: {data?.disk.usage_percent.toFixed(1)}%</p>
    </div>
  );
}
```

### Watch Adapter State Changes

```typescript
import { useAdaptersStream } from '../streaming';
import { AdapterStateTransitionEvent } from '../streaming';

export function AdapterMonitor() {
  const [changes, setChanges] = useState<AdapterStateTransitionEvent[]>([]);

  useAdaptersStream({
    onMessage: (event) => {
      if ('previous_state' in event) {
        setChanges(prev => [event, ...prev.slice(0, 4)]);
      }
    },
  });

  return (
    <ul>
      {changes.map((evt, i) => (
        <li key={i}>
          {evt.adapter_id}: {evt.previous_state} → {evt.new_state}
        </li>
      ))}
    </ul>
  );
}
```

### Track Training Progress

```typescript
import { useTrainingStream } from '../streaming';

export function TrainingProgress() {
  const { data, error, connected } = useTrainingStream();

  return (
    <div>
      <div style={{ width: '100%', height: '20px', background: '#eee' }}>
        <div
          style={{
            width: `${data?.progress_pct || 0}%`,
            height: '100%',
            background: '#4CAF50',
          }}
        />
      </div>
      <p>{data?.progress_pct.toFixed(1)}% Complete</p>
      {data?.current_loss && <p>Loss: {data.current_loss.toFixed(4)}</p>}
      {error && <p style={{ color: 'red' }}>Error: {error}</p>}
    </div>
  );
}
```

### Multiple Streams in One Component

```typescript
import {
  useMetricsStream,
  useAdaptersStream,
  useTrainingStream,
} from '../streaming';

export function Dashboard() {
  const metrics = useMetricsStream();
  const adapters = useAdaptersStream();
  const training = useTrainingStream();

  return (
    <div>
      <section>
        <h2>Metrics</h2>
        <p>Status: {metrics.connected ? '✓' : '✗'}</p>
        {metrics.data && <pre>{JSON.stringify(metrics.data, null, 2)}</pre>}
      </section>

      <section>
        <h2>Adapters</h2>
        <p>Status: {adapters.connected ? '✓' : '✗'}</p>
        {adapters.data && <pre>{JSON.stringify(adapters.data, null, 2)}</pre>}
      </section>

      <section>
        <h2>Training</h2>
        <p>Status: {training.connected ? '✓' : '✗'}</p>
        {training.data && <pre>{JSON.stringify(training.data, null, 2)}</pre>}
      </section>
    </div>
  );
}
```

### Low-Level Service Usage

For more control, use the service directly:

```typescript
import { streamingService } from '../streaming';

export function CustomComponent() {
  useEffect(() => {
    // Subscribe to metrics
    const sub = streamingService.subscribeToMetrics({
      onMessage: (event) => {
        console.log('CPU:', event.cpu.usage_percent);
      },
      onError: (error) => {
        console.error('Stream error:', error);
      },
    });

    // Later: unsubscribe
    return () => sub.unsubscribe();
  }, []);
}
```

## Configuration

### Enable/Disable Streams

```typescript
// Disable stream (useful for background tabs)
useMetricsStream({ enabled: false });

// Re-enable dynamically
const [enabled, setEnabled] = useState(true);
useMetricsStream({ enabled });
```

### Custom Event Handlers

```typescript
useMetricsStream({
  onMessage: (event) => {
    // Handle new event
    console.log('Update:', event.timestamp);
  },
  onError: (error) => {
    // Handle connection error
    console.error('Error:', error);
  },
  onOpen: () => {
    // Connected
    console.log('Stream connected');
  },
  onClose: () => {
    // Disconnected
    console.log('Stream closed');
  },
});
```

### Reconnection Options

Default configuration (usually fine):
- Max reconnect attempts: 10
- Initial backoff: 1 second
- Max backoff: 30 seconds
- Auto-reconnect: enabled

These are handled automatically in most cases.

## Imports

### Option 1: From Streaming Module (Recommended)

```typescript
import {
  useMetricsStream,
  useAdaptersStream,
  streamingService,
  type MetricsStreamEvent,
  type AdapterStreamEvent,
} from '../streaming';
```

### Option 2: Direct Imports

```typescript
import { useMetricsStream } from '../hooks/useStreamingEndpoints';
import { streamingService } from '../services/StreamingService';
import type { MetricsStreamEvent } from '../api/streaming-types';
```

## Common Patterns

### Aggregating Events

```typescript
const [allEvents, setAllEvents] = useState<Event[]>([]);

// Subscribe to multiple streams
useMetricsStream({
  onMessage: (evt) => setAllEvents(p => [evt, ...p.slice(0, 99)]),
});

useAdaptersStream({
  onMessage: (evt) => setAllEvents(p => [evt, ...p.slice(0, 99)]),
});
```

### Conditional Streaming

```typescript
const [isVisible, setIsVisible] = useState(true);

const { data } = useMetricsStream({
  enabled: isVisible, // Only stream when visible
});
```

### Real-Time Notifications

```typescript
useTelemetryStream({
  onMessage: (event) => {
    if (event.status === 'failure') {
      showNotification(`${event.action} failed!`, 'error');
    }
  },
});
```

## Debugging

Check active streams:

```typescript
import { streamingService } from '../streaming';

console.log(streamingService.getActiveSubscriptions());
// Output:
// [
//   { id: 'metrics-1234', endpoint: '/v1/stream/metrics', connected: true },
//   { id: 'adapters-5678', endpoint: '/v1/stream/adapters', connected: false },
// ]
```

## Troubleshooting

### No events received
1. Check browser DevTools → Network tab for EventSource connection
2. Ensure you have valid auth token
3. Check server is running and sending events

### Frequent disconnects
1. Check server logs
2. Try manually calling `reconnect()`
3. Check network connection

### Memory leaks
1. Ensure cleanup on unmount (hooks handle this automatically)
2. Don't forget to unsubscribe from service

## Next Steps

1. **Reference Implementation**: Check `/src/components/StreamingIntegration.tsx`
2. **Full Guide**: Read `/STREAMING_GUIDE.md` for advanced patterns
3. **Type Definitions**: See `/src/api/streaming-types.ts` for all event structures
4. **Service API**: Check `/src/services/StreamingService.ts` for service methods

## Need Help?

- **Type Safety**: Use TypeScript types from `../streaming`
- **Memory Leaks**: Hooks handle cleanup automatically
- **Performance**: Disable unused streams with `enabled: false`
- **Debugging**: Check logs with component: 'StreamingService'

## Checklist

- [x] Types defined for all 7 endpoints
- [x] React hooks for each endpoint
- [x] Connection management service
- [x] Error handling and reconnection
- [x] Authentication support
- [x] Reference implementation component
- [x] Comprehensive documentation
- [x] Quick start guide (this file)
- [x] Central export module

You're ready to use real-time streaming in your UI!
