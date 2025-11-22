# Server-Sent Events (SSE) Streaming Implementation

## Overview

This document describes the SSE streaming implementation for real-time metrics and telemetry in AdapterOS.

## Architecture

The SSE streaming system uses Rust's broadcast channels (`tokio::sync::broadcast`) to distribute telemetry events to multiple connected clients in real-time.

### Key Components

1. **Telemetry Channel** (`AppState.telemetry_tx`)
   - Type: `Arc<tokio::sync::broadcast::Sender<TelemetryEvent>>`
   - Capacity: 1000 events
   - Purpose: Distributes telemetry events to all subscribers

2. **SSE Handlers**
   - `telemetry_events_stream`: Streams all telemetry events
   - `stream_logs`: Streams filtered log events
   - `stream_metrics`: Streams metrics-specific events

## Implemented Endpoints

### 1. `/v1/stream/telemetry` - General Telemetry Stream

Streams all telemetry events in real-time.

**Handler**: `telemetry_events_stream` in `/crates/adapteros-server-api/src/handlers.rs`

**Features**:
- Subscribes to the broadcast channel (`telemetry_tx`)
- Serializes events to JSON
- Sends SSE events with type "telemetry"
- Keepalive interval: 15 seconds
- Handles client disconnections gracefully
- Logs stream errors

**Example Event**:
```
event: telemetry
data: {"id":"abc123","timestamp":"2025-11-19T12:00:00Z","event_type":"inference_complete","level":"Info","message":"Inference completed","component":"worker","identity":{"tenant_id":"tenant1","workspace_id":"ws1"},"metadata":{}}
```

### 2. `/v1/logs/stream` - Filtered Log Stream

Streams telemetry events filtered by log criteria.

**Handler**: `stream_logs` in `/crates/adapteros-server-api/src/handlers/telemetry.rs`

**Query Parameters**:
- `limit`: Maximum events to return (default: unlimited for streaming)
- `tenant_id`: Filter by tenant
- `event_type`: Filter by event type
- `level`: Filter by log level (debug, info, warn, error, critical)
- `component`: Filter by component name
- `trace_id`: Filter by trace ID

**Features**:
- Real-time filtering based on query parameters
- Keepalive interval: 30 seconds
- Only sends events matching filter criteria

**Example Request**:
```
GET /v1/logs/stream?level=error&component=worker
```

### 3. `/v1/metrics/stream` - Metrics Stream

Streams only metrics-related telemetry events.

**Handler**: `stream_metrics` in `/crates/adapteros-server-api/src/handlers/telemetry.rs`

**Features**:
- Filters events by type (metric, performance_metric, system_metric, adapter_metric, inference_metric)
- Sends SSE events with type "metric"
- Keepalive interval: 15 seconds
- No query parameters required

**Example Event**:
```
event: metric
data: {"id":"xyz789","timestamp":"2025-11-19T12:00:05Z","event_type":"performance_metric","level":"Info","message":"Throughput: 1000 tokens/sec","component":"metrics_collector","metadata":{"tokens_per_second":1000}}
```

## Implementation Details

### Broadcast Channel Usage

The implementation uses `BroadcastStream` from `tokio-stream` to convert the broadcast receiver into an async stream:

```rust
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

let rx = state.telemetry_tx.subscribe();
let stream = BroadcastStream::new(rx).filter_map(|result| {
    match result {
        Ok(event) => {
            // Process event
            Some(Ok(Event::default().event("telemetry").data(json)))
        }
        Err(e) => {
            // Log error and continue
            tracing::warn!("Stream error: {:?}", e);
            None
        }
    }
});
```

### Error Handling

The implementation includes robust error handling:

1. **Serialization Errors**: Logged and skipped (event not sent)
2. **Stream Errors**: Logged and stream continues
3. **Client Disconnections**: Handled gracefully by axum

### Keepalive Messages

Keepalive messages prevent connection timeouts:
- Sent as text comments ("keepalive")
- Interval varies by endpoint (15-30 seconds)
- Client should ignore these messages

### Backpressure Management

The broadcast channel has a fixed capacity (1000 events):
- If a client is too slow, it may miss events
- Lagged clients receive all available events but skip some
- Stream errors are logged but don't crash the connection

## Client Usage

### JavaScript/TypeScript Example

```typescript
const eventSource = new EventSource('/v1/stream/telemetry', {
  headers: {
    'Authorization': `Bearer ${token}`
  }
});

// Listen for telemetry events
eventSource.addEventListener('telemetry', (event) => {
  const data = JSON.parse(event.data);
  console.log('Telemetry event:', data);
});

// Listen for metric events
eventSource.addEventListener('metric', (event) => {
  const data = JSON.parse(event.data);
  console.log('Metric:', data);
});

// Handle errors
eventSource.onerror = (error) => {
  console.error('SSE error:', error);
};

// Clean up
eventSource.close();
```

### cURL Example

```bash
# Stream all telemetry events
curl -N -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/v1/stream/telemetry

# Stream filtered logs
curl -N -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/v1/logs/stream?level=error&component=worker"

# Stream metrics only
curl -N -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/v1/metrics/stream
```

## Performance Considerations

1. **Memory Usage**: Each subscriber holds a clone of the broadcast channel
2. **Event Lifetime**: Events are retained only until all subscribers have read them
3. **Throughput**: Broadcast channels are lock-free and highly performant
4. **Scalability**: Suitable for hundreds of concurrent SSE connections

## Future Enhancements

Potential improvements:
1. Add event sampling/throttling for high-volume streams
2. Implement event batching to reduce SSE overhead
3. Add Redis-based pub/sub for multi-instance deployments
4. Implement reconnection tokens to resume from last event
5. Add metrics for stream health (connected clients, events/sec, lag)

## Related Files

- `/crates/adapteros-server-api/src/handlers.rs` - General telemetry stream handler
- `/crates/adapteros-server-api/src/handlers/telemetry.rs` - Telemetry-specific handlers
- `/crates/adapteros-server-api/src/routes.rs` - Route definitions
- `/crates/adapteros-server-api/src/state.rs` - AppState with telemetry_tx
- `/crates/adapteros-server-api/src/telemetry/mod.rs` - Telemetry types and channel

## Testing

To test the SSE endpoints:

1. Start the server: `cargo run --bin adapteros-server`
2. In another terminal, subscribe to a stream:
   ```bash
   curl -N http://localhost:3000/v1/stream/telemetry
   ```
3. Trigger events (e.g., make API calls, run inference)
4. Observe events streaming in real-time

## Troubleshooting

**No events received**:
- Check that `telemetry_tx` is properly initialized in AppState
- Verify events are being sent to the broadcast channel
- Check authentication/authorization

**Connection timeouts**:
- Ensure keepalive messages are being sent
- Check proxy/load balancer timeout settings
- Verify client is processing events quickly enough

**Missing events**:
- Check broadcast channel capacity (1000 events)
- Monitor for stream lag warnings in logs
- Consider increasing channel capacity if needed
