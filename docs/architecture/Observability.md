# Observability Architecture

## Overview

AdapterOS provides fully offline, in-dashboard observability for logs, traces, and metrics. All data is kept in-memory with bounded buffers—no third-party services required.

## Architecture

### Telemetry Core

The observability system consists of three main components:

1. **TelemetryLogger** (`adapteros-telemetry`) - Structured logging with bounded in-memory buffers
2. **TraceBuilder** (`adapteros-trace`) - Distributed tracing with span collection
3. **MetricsRegistry** (`adapteros-telemetry/metrics`) - Time series metrics with sliding windows

### In-Memory Buffers

All observability data is stored in bounded ring buffers:

- **LogBuffer**: Up to 10,000 events (configurable), oldest evicted first
- **TraceBuffer**: Up to 100 traces (configurable)
- **MetricTimeSeries**: Configurable resolution (default 1s) and max points (default 1000)

### HTTP/SSE APIs

The server exposes offline-only endpoints:

- `GET /v1/metrics/snapshot` - Current metrics snapshot (JSON)
- `GET /v1/metrics/series` - Time series data for charts
- `GET /v1/logs/query` - Query logs with filters
- `GET /v1/logs/stream` - SSE stream of live logs
- `GET /v1/traces/search` - Search traces by criteria
- `GET /v1/traces/{trace_id}` - Get specific trace details
- `GET /v1/stream/telemetry` - SSE stream for telemetry events and bundle updates

All endpoints are protected by authentication and serve data from in-memory buffers only.

#### SSE Authentication

SSE endpoints use cookie-based session authentication. No token query parameters are required—the browser automatically sends session cookies with the EventSource connection. The middleware validates the session cookie and provides `Extension<Claims>` to handlers.

#### SSE Event Types

The `/v1/stream/telemetry` endpoint emits multiple event types:

- **`telemetry`**: Live telemetry events from the in-memory buffer (activity feed)
- **`bundles`**: Telemetry bundle updates (creation/purge notifications)
  - On connect: emits backlog of latest 50 bundles
  - Realtime: emits individual bundle objects as they're created or updated
  - Payload: single `TelemetryBundleResponse` object or array of bundles

### Dashboard UI

The observability dashboard (`/observability`) provides:

- **Metrics Tab**: Real-time metrics cards and time series charts
- **Traces Tab**: Trace search and span timeline visualization
- **Logs Tab**: Live log stream with filtering and auto-scroll

## Configuration

Environment variables:

- `AOS_TELEMETRY_ENABLED=true` - Enable telemetry collection
- `AOS_LOG_BUFFER=10000` - Log buffer capacity
- `AOS_TRACE_BUFFER=100` - Trace buffer capacity
- `AOS_METRICS_RESOLUTION_MS=1000` - Metrics snapshot interval
- `AOS_TELEMETRY_ENABLED=true` - Enable telemetry endpoints

## Usage

The dashboard is available at `/observability` when the server is running. All data is live and automatically updated via polling or SSE streams.

No external dependencies required—everything runs offline in-process.

