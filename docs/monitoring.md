# Monitoring Integration

AdapterOS provides comprehensive monitoring capabilities for production deployments, including metrics export to Prometheus/Grafana and enhanced health checking.

## Health Check Endpoint

### `/healthz` - Enhanced Health Check

Returns service health status including model runtime information.

**Response Format:**
```json
{
  "status": "healthy",
  "version": "1.2.3",
  "models": {
    "total_models": 5,
    "loaded_count": 3,
    "healthy": true,
    "inconsistencies_count": 0
  }
}
```

**Fields:**
- `status`: Overall service health ("healthy" or "unhealthy")
- `version`: AdapterOS version
- `models`: Model runtime health (optional, present when model runtime is available)
  - `total_models`: Number of models registered in database
  - `loaded_count`: Number of models currently loaded in runtime
  - `healthy`: True if no inconsistencies detected
  - `inconsistencies_count`: Number of detected inconsistencies

**Health Check Caching:**
- Results are cached for 30 seconds to reduce database load
- Cache hit/miss performance is tracked in metrics

## Metrics Endpoint

### `/metrics` - Prometheus/OpenMetrics Export

Provides comprehensive system and application metrics in Prometheus-compatible format.

**Authentication:**
- Requires Bearer token authentication
- Token configured via `metrics.bearer_token` in config

**Example Request:**
```bash
curl -H "Authorization: Bearer YOUR_TOKEN" http://localhost:8080/metrics
```

**Metrics Categories:**
- **Inference Metrics**: Latency percentiles, throughput, queue depths
- **System Metrics**: CPU, memory, disk I/O, network utilization
- **Adapter Lifecycle**: Load/unload operations and failures
- **Policy Metrics**: Violations and abstain events
- **Health Check Performance**: Cache hit rates and response times

**Sample Output:**
```
# TYPE adapteros_inference_latency_seconds histogram
adapteros_inference_latency_seconds_bucket{tenant_id="default",adapter_id="llama-7b",le="0.1"} 125
adapteros_inference_latency_seconds_bucket{tenant_id="default",adapter_id="llama-7b",le="0.25"} 180
...

# TYPE adapteros_queue_depth gauge
adapteros_queue_depth{queue_type="request",tenant_id="default"} 3
```

## Configuration

```toml
[metrics]
enabled = true
bearer_token = "your_secure_token_here"
server_enabled = true
server_port = 9090
system_metrics_interval_secs = 30
```

## Prometheus Configuration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'adapteros'
    static_configs:
      - targets: ['your-server:8080']
    metrics_path: '/metrics'
    bearer_token: 'your_secure_token_here'
```

## Grafana Dashboards

Import the AdapterOS dashboard JSON (available in `docs/dashboards/`) which includes:

- Inference latency and throughput graphs
- Queue depth monitoring
- System resource utilization
- Model health status
- Error rates and policy violations

## Security Considerations

- **Bearer Token**: Use a strong, randomly generated token
- **Network Access**: Restrict access to monitoring endpoints in production
- **Rate Limiting**: Consider adding rate limits for health checks in high-traffic scenarios

## Troubleshooting

### Health Check Returns Unhealthy Models

Check for:
- Models stuck in "loading" state
- Database connectivity issues
- Runtime crashes preventing model loading

### Metrics Endpoint Returns 401

Verify:
- Bearer token is correctly configured
- Token is included in Authorization header
- Config file has correct `bearer_token` value

### High Health Check Latency

Check:
- Database performance
- Cache TTL (currently 30 seconds)
- Model runtime responsiveness
