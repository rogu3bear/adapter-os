# adapteros-plugin-advanced-metrics

Advanced metrics collection plugin for adapterOS, providing detailed performance tracking and Prometheus endpoint.

## Features

- **Inference Latency Tracking**: Records latency percentiles (p50, p95, p99) per adapter
- **Training Duration Histograms**: Tracks training job durations from start to completion
- **Adapter Activation Patterns**: Monitors which adapters are used and how frequently
- **Token Throughput**: Measures tokens processed per tenant with rate tracking
- **System Metrics**: Collects CPU, memory, and GPU utilization from metrics ticks
- **Prometheus Endpoint**: Exposes metrics in Prometheus text format for scraping

## Architecture

### Event Subscriptions

The plugin subscribes to three event types:

1. **OnMetricsTick**: System-wide metrics collection (CPU, memory, adapters)
2. **OnInferenceComplete**: Inference request metrics (latency, tokens, adapters)
3. **OnTrainingJobEvent**: Training job lifecycle tracking (duration, status)

### Metrics Collected

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `adapteros_inference_latency_ms` | Histogram | adapter_id, tenant_id | Inference latency percentiles |
| `adapteros_training_duration_seconds` | Histogram | adapter_id, tenant_id, status | Training job duration |
| `adapteros_adapter_activations_total` | Counter | adapter_id, tenant_id | Total adapter activations |
| `adapteros_tenant_tokens_total` | Counter | tenant_id | Total tokens processed |
| `adapteros_tenant_tokens_per_sec` | Gauge | tenant_id | Current token throughput |
| `adapteros_system_cpu_percent` | Gauge | node | System CPU usage |
| `adapteros_system_memory_bytes` | Gauge | node, type | System memory usage |
| `adapteros_system_active_adapters` | Gauge | node | Active adapter count |

## Usage

### Enabling the Plugin

```rust
use adapteros_plugin_advanced_metrics::AdvancedMetricsPlugin;
use adapteros_core::plugins::{Plugin, PluginConfig};

let plugin = AdvancedMetricsPlugin::new();
let config = PluginConfig {
    name: "advanced-metrics".to_string(),
    enabled: true,
    specific: Default::default(),
};

plugin.load(&config).await?;
plugin.start().await?;
```

### Accessing Metrics

#### Prometheus Endpoint

```rust
use adapteros_plugin_advanced_metrics::metrics_endpoint;

let plugin = AdvancedMetricsPlugin::new();
let collector = plugin.collector();

// Generate Prometheus text format
let response = metrics_endpoint(collector).await?;
println!("{}", response.body);
```

#### JSON Format

```rust
use adapteros_plugin_advanced_metrics::endpoints::metrics_json;

let collector = plugin.collector();
let json = metrics_json(collector).await?;
println!("{}", serde_json::to_string_pretty(&json)?);
```

### Example Output

**Prometheus Format:**
```
# HELP adapteros_inference_latency_ms Inference latency in milliseconds by adapter
# TYPE adapteros_inference_latency_ms histogram
adapteros_inference_latency_ms_bucket{adapter_id="code-review",tenant_id="tenant-a",le="10"} 0
adapteros_inference_latency_ms_bucket{adapter_id="code-review",tenant_id="tenant-a",le="25"} 0
adapteros_inference_latency_ms_bucket{adapter_id="code-review",tenant_id="tenant-a",le="50"} 5
adapteros_inference_latency_ms_bucket{adapter_id="code-review",tenant_id="tenant-a",le="100"} 15
...
```

**JSON Format:**
```json
{
  "stats": {
    "tracked_adapters": 3,
    "inference_events": 150,
    "training_events": 5,
    "metrics_ticks": 120
  },
  "adapters": [
    {
      "adapter_id": "code-review",
      "activation_count": 75,
      "avg_latency_ms": 85.3
    }
  ]
}
```

## Integration

### REST API Endpoint

To expose metrics via REST API, add to your server routes:

```rust
use adapteros_plugin_advanced_metrics::metrics_endpoint;

async fn metrics_handler(
    State(plugin): State<Arc<AdvancedMetricsPlugin>>,
) -> Result<Response, StatusCode> {
    let collector = plugin.collector();
    let response = metrics_endpoint(collector)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", response.content_type)
        .body(response.body)
        .unwrap())
}

// Add route
router.route("/v1/plugins/advanced-metrics/metrics", get(metrics_handler))
```

### Prometheus Scraping

Configure Prometheus to scrape the endpoint:

```yaml
scrape_configs:
  - job_name: 'adapteros-advanced-metrics'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/v1/plugins/advanced-metrics/metrics'
    scrape_interval: 15s
```

## Testing

```bash
# Run unit tests
cargo test -p adapteros-plugin-advanced-metrics

# Run with output
cargo test -p adapteros-plugin-advanced-metrics -- --nocapture
```

## Performance Considerations

- Metrics collection is lock-free for read operations
- Histogram buckets are pre-configured for typical latency ranges
- Training job duration tracking uses O(1) memory per active job
- Metrics are kept in-memory; consider Prometheus retention for long-term storage

## License

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
