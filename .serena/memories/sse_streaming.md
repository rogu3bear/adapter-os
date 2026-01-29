# SSE Streaming in AdapterOS

## Overview

AdapterOS implements a comprehensive Server-Sent Events (SSE) streaming system with:
- Monotonic event IDs for reliable ordering
- Ring buffer storage with replay support for client reconnection
- Gap detection and recovery hints
- Circuit breaker pattern on both server and client

## Server-Side Architecture

### Core Module: `crates/adapteros-server-api/src/sse/`

**Files:**
- `mod.rs` - Module exports and integration tests
- `types.rs` - Core types (`SseStreamType`, `SseEvent`, `SseErrorEvent`)
- `event_manager.rs` - `SseEventManager` for centralized event management
- `ring_buffer.rs` - `SseRingBuffer` for bounded event storage

### Stream Types (`SseStreamType`)

15 distinct stream types, each with independent ID sequences and buffers:

| Stream Type | Event Name | Default Buffer Size |
|-------------|------------|---------------------|
| `SystemMetrics` | "metrics" | 1000 |
| `Telemetry` | "telemetry" | 1500 |
| `AdapterState` | "adapters" | 1000 |
| `Workers` | "workers" | 500 |
| `Training` | "training" | 500 |
| `Alerts` | "alerts" | 200 |
| `Anomalies` | "anomalies" | 200 |
| `Dashboard` | "dashboard_metrics" | 1000 |
| `Inference` | "inference" | 2000 |
| `Discovery` | "discovery" | 1000 |
| `Activity` | "activity" | 1000 |
| `BootProgress` | "boot_progress" | 1000 |
| `DatasetProgress` | "dataset_progress" | 1000 |
| `GitProgress` | "git_progress" | 1000 |
| `TraceReceipts` | "trace_receipts" | 1000 |

### SseEvent Structure

```rust
pub struct SseEvent {
    pub id: u64,              // Monotonic event ID
    pub event_type: String,   // SSE event: field
    pub data: String,         // JSON payload
    pub timestamp_ms: u64,    // Creation timestamp
    pub retry_ms: Option<u32>, // Reconnect hint (default: 3000ms)
}
```

### SseEventManager

Central component stored in `AppState.sse_manager`:

```rust
// Create event with monotonic ID
let event = manager.create_event(SseStreamType::SystemMetrics, "metrics", json_data).await;

// Convert to Axum SSE event
let sse_event = SseEventManager::to_axum_event(&event);

// Handle reconnection with Last-Event-ID
let last_id = SseEventManager::parse_last_event_id(&headers);
let replay_events = manager.get_replay_events(stream_type, last_id).await;

// Gap detection
let result = manager.get_replay_with_analysis(stream_type, last_id).await;
// Returns: ReplayResult { events, has_gap, dropped_count }
```

### Ring Buffer

- O(1) insertion with drop-oldest semantics
- O(n) replay from specific event ID
- Thread-safe using `RwLock` + atomic counters
- Preserves sequence counter on clear (monotonic guarantee)

### SSE Protocol Format

```
id: 42
event: metrics
retry: 3000
data: {"cpu": 50, "memory": 60}

```

## Streaming Endpoints

### Routes (from `routes/mod.rs`)

| Endpoint | Handler | Purpose |
|----------|---------|---------|
| `GET /v1/stream/metrics` | `system_metrics_stream` | System metrics (5s intervals) |
| `GET /v1/stream/telemetry` | `telemetry_events_stream` | Telemetry events (2s polling) |
| `GET /v1/stream/adapters` | `adapter_state_stream` | Adapter state changes (3s) |
| `GET /v1/stream/workers` | `workers_stream` | Worker status |
| `GET /v1/stream/boot-progress` | `boot_progress_stream` | Boot progress (500ms) |
| `GET /v1/stream/notifications` | `notifications_stream` | User notifications (5s) |
| `GET /v1/stream/messages/{id}` | `messages_stream` | Workspace messages (2s) |
| `GET /v1/stream/activity/{id}` | `activity_stream` | Workspace activity (3s) |
| `GET /v1/stream/trace-receipts` | `trace_receipts_stream` | Inference receipts (5s) |
| `POST /v1/infer/stream` | `streaming_infer` | Token-by-token inference |
| `POST /v1/infer/stream/progress` | `streaming_infer_with_progress` | Inference with progress |

### Handler Patterns

All handlers use `stream::unfold` for stateful streaming:

```rust
let stream = stream::unfold(
    (state, has_permission, false),
    |(state, has_permission, error_sent)| async move {
        // Permission check with single error event
        if !has_permission {
            if error_sent { return None; }
            return Some((Ok(Event::default().event("error").data("...")), (state, false, true)));
        }
        
        // Poll interval
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        // Create and return event
        Some((Ok(Event::default().event("metrics").data(json)), (state, has_permission, false)))
    },
);

Sse::new(stream).keep_alive(
    KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive")
)
```

### Security Features

1. **Permission checks**: Each stream validates required permissions (MetricsView, TelemetryView, etc.)
2. **Tenant isolation**: Events filtered by `tenant_id` from JWT claims
3. **ID obfuscation**: Internal IDs obfuscated with per-session BLAKE3 keyed hash
4. **Circuit breaker**: Server-side circuit breaker for error handling (5 failures, 30s recovery)

## Inference Streaming

### Handler: `streaming_infer.rs`

Token-by-token streaming using OpenAI-compatible format:

```rust
pub enum StreamEvent {
    Token(String),
    Done { finish_reason: String },
    Error(String),
    Paused { pause_id, inference_id, context }, // Human-in-the-loop review
}
```

### Response Format (OpenAI-compatible)

```json
{
    "id": "chatcmpl-uuid",
    "object": "chat.completion.chunk",
    "created": 1234567890,
    "model": "adapteros",
    "choices": [{
        "index": 0,
        "delta": { "content": "Hello" },
        "finish_reason": null
    }]
}
```

### Worker Integration

Uses `mpsc` channel from `adapteros-lora-worker`:

```rust
let (worker_tx, mut worker_rx) = mpsc::channel::<WorkerStreamEvent>(256);
worker.infer_stream(inference_req, worker_tx).await;

while let Some(event) = worker_rx.recv().await {
    match event {
        WorkerStreamEvent::Token(token) => tx.send(StreamEvent::Token(token.text)),
        WorkerStreamEvent::Complete(response) => tx.send(StreamEvent::Done { ... }),
        WorkerStreamEvent::Error(error) => tx.send(StreamEvent::Error(error)),
        WorkerStreamEvent::Paused { .. } => // Forward pause for review
    }
}
```

## Client-Side (Leptos WASM)

### Module: `crates/adapteros-ui/src/api/sse.rs`

### SseConnection

```rust
let connection = SseConnection::with_config(endpoint, CircuitBreakerConfig::default());
connection.connect(|event: SseEvent| {
    // Handle event
})?;
```

### CircuitBreakerConfig

```rust
CircuitBreakerConfig {
    failure_threshold: 3,
    retry_delay_ms: 1000,
    max_retry_delay_ms: 30000,
    reset_timeout_ms: 60000,
    idle_timeout_ms: Some(120_000),
    with_credentials: true,
}
```

### Hooks

```rust
// Basic SSE hook
let (state, reconnect) = use_sse("/v1/stream/metrics", |event| { ... });

// JSON-parsing hook
let (state, reconnect) = use_sse_json::<MetricsData, _>("/v1/stream/metrics", |data| { ... });

// Event-type specific hook
let (state, reconnect) = use_sse_json_events::<T, _>(
    "/v1/stream/activity",
    &["activity", "heartbeat"],
    |event| { ... }
);
```

### SseState

```rust
pub enum SseState {
    Disconnected,
    Connecting,
    Connected,
    Error,
    CircuitOpen,
}
```

### SSE Parser: `crates/adapteros-ui/src/sse.rs`

Parses both AdapterOS native and OpenAI-compatible formats:

```rust
// AdapterOS format
// data: {"event": "Token", "text": "Hello"}

// OpenAI format  
// data: {"choices": [{"delta": {"content": "Hello"}}]}

let parsed = parse_sse_event_with_info(event_data);
// Returns: ParsedSseEvent { token, trace_id, latency_ms, token_count, ... }
```

### InferenceEvent Types

```rust
pub enum InferenceEvent {
    Token { text: String },
    Done { total_tokens, latency_ms, trace_id, prompt_tokens, completion_tokens },
    Error { message: String },
    Other, // Loading, Ready, etc.
}
```

## Integration Points

1. **AppState** contains `sse_manager: Arc<SseEventManager>` for centralized event creation
2. **Routes** wire up streaming handlers under `/v1/stream/*` paths
3. **Middleware** enforces auth before streaming handlers
4. **Telemetry** events flow through `telemetry_buffer` and are streamed filtered by tenant

## Error Events

```rust
pub enum SseErrorEvent {
    StreamDisconnected { last_event_id, reason, reconnect_hint_ms },
    BufferOverflow { dropped_count, oldest_available_id },
    EventGapDetected { client_last_id, server_oldest_id, events_lost, recovery_hint },
    Heartbeat { current_id, timestamp_ms },
}

pub enum EventGapRecoveryHint {
    RefetchFullState,
    ContinueWithGap,
    RestartStream,
    RefetchResource { resource_type, resource_id },
}
```

## Key Files

| File | Purpose |
|------|---------|
| `adapteros-server-api/src/sse/` | Core SSE module |
| `adapteros-server-api/src/handlers/streaming.rs` | General streaming handlers |
| `adapteros-server-api/src/handlers/streaming_infer.rs` | Inference streaming |
| `adapteros-server-api/src/state.rs` | AppState with `sse_manager` |
| `adapteros-server-api/src/routes/mod.rs` | Route definitions |
| `adapteros-ui/src/api/sse.rs` | Client SSE connection |
| `adapteros-ui/src/sse.rs` | Event parsing |
| `adapteros-ui/src/hooks/use_sse_notifications.rs` | Notification hook |
