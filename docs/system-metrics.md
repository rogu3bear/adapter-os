# System Metrics Monitoring

## Overview

AdapterOS includes comprehensive system resource monitoring that collects real-time metrics for CPU, memory, disk, network, and GPU utilization. The system supports both **SQLite and PostgreSQL backends** with automatic database selection, integrating with existing telemetry, policy enforcement, and alerting mechanisms.

## Architecture

### Core Components

1. **SystemMetricsCollector** - Real-time metrics collection using `sysinfo`
2. **SystemMetricsPolicy** - Policy enforcement and threshold checking
3. **SystemMonitor** - Continuous monitoring pipeline with telemetry integration
4. **SystemMetricsDb** - Database storage and retrieval
5. **GpuMetricsCollector** - GPU/Metal metrics collection

### Data Flow

```
System Resources → Collector → Policy Check → Telemetry → Database
                                    ↓
                              Alert/Incident
```

## Database Schema

Complete database schema documentation for system metrics:

- [Monitoring Flow](database-schema/workflows/monitoring-flow.md) - Detailed workflow animation showing metrics collection, health checks, threshold violations, and incident generation
- [Performance Dashboard](database-schema/workflows/performance-dashboard.md) - Real-time visualization of system health and performance metrics
- [Schema Diagram](database-schema/schema-diagram.md) - Complete ER diagram with `system_metrics`, `system_health_checks`, `threshold_violations`, and `metrics_aggregations` tables

### Key Database Tables

- `system_metrics` - Real-time performance data (CPU, memory, GPU, network, disk)
- `system_health_checks` - Automated health status validation
- `threshold_violations` - Performance threshold breach detection
- `metrics_aggregations` - Pre-computed time-series summaries
- `system_metrics_config` - Monitoring configuration parameters

See [database-schema/README.md](database-schema/README.md) for complete documentation.

## Configuration

### Default Configuration

```toml
[system_metrics]
collection_interval_secs = 30
sampling_rate = 0.05  # 5% sampling per Telemetry Ruleset #9
enable_gpu_metrics = true
enable_disk_metrics = true
enable_network_metrics = true
retention_days = 30

[thresholds]
cpu_warning = 70.0
cpu_critical = 90.0
memory_warning = 80.0
memory_critical = 95.0
disk_warning = 85.0
disk_critical = 95.0
gpu_warning = 80.0
gpu_critical = 95.0
min_memory_headroom = 15.0
```

### Environment Variables

- `AOS_METRICS_SAMPLING_RATE` - Override sampling rate (0.0-1.0)
- `AOS_METRICS_COLLECTION_INTERVAL` - Override collection interval in seconds
- `AOS_METRICS_ENABLE_GPU` - Enable/disable GPU metrics collection

### Control Plane Integration

In the control plane configuration (`configs/cp.toml`), adjust `metrics.system_metrics_interval_secs`
to control how frequently `system.metrics` events are emitted into the telemetry NDJSON pipeline.
Set the value to `0` to disable the background emitter entirely.

## API Endpoints

### GET /v1/metrics/system

Returns current system metrics and stores them for historical tracking:

```json
{
  "cpu_usage": 45.2,
  "memory_usage": 62.8,
  "active_workers": 3,
  "requests_per_second": 12.5,
  "avg_latency_ms": 24.3,
  "disk_usage": 23.1,
  "network_bandwidth": 1.2,
  "gpu_utilization": 15.5,
  "uptime_seconds": 86400,
  "process_count": 156,
  "load_average": {
    "load_1min": 1.2,
    "load_5min": 1.1,
    "load_15min": 1.0
  },
  "timestamp": 1640995200
}
```

### Telemetry NDJSON Sample (`metrics.system`)

The control plane server emits a placeholder telemetry event on the NDJSON stream every
30 seconds until real sensors are connected:

```json
{
  "event_type": "metrics.system",
  "kind": "metrics.system",
  "level": "info",
  "message": "Placeholder system metrics sample",
  "metadata": {
    "cpu_usage": 0.0,
    "memory_usage": 0.0,
    "disk_read_bytes": 0,
    "disk_write_bytes": 0,
    "network_rx_bytes": 0,
    "network_tx_bytes": 0,
    "gpu_utilization": null,
    "gpu_memory_used": null,
    "uptime_seconds": 0,
    "process_count": 0,
    "load_average": {
      "load_1min": 0.0,
      "load_5min": 0.0,
      "load_15min": 0.0
    },
    "timestamp": 0
  }
}
```

## CLI Commands

### View Current Metrics

```bash
# Show current system metrics
aosctl metrics show

# Show metrics in JSON format
aosctl metrics show --json

# Show system health status
aosctl metrics health
```

### View History

```bash
# Show metrics history (last 24 hours)
aosctl metrics history

# Show metrics history for specific time range
aosctl metrics history --hours 48 --limit 200

# Via API: Get historical metrics
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/metrics/system/history?hours=24&limit=100"
```

### Export Data

```bash
# Export metrics to JSON file
aosctl metrics export --output metrics.json --format json --hours 24

# Export metrics to CSV file
aosctl metrics export --output metrics.csv --format csv --hours 168
```

### Policy Management

```bash
# Check policy thresholds
aosctl metrics check

# Show threshold violations
aosctl metrics violations

# Show only unresolved violations
aosctl metrics violations --unresolved
```

### Configuration

```bash
# List current configuration
aosctl metrics config --list

# Set configuration value
aosctl metrics config --key sampling_rate --value 0.1

# Set threshold
aosctl metrics config --key cpu_warning --value 75.0
```

## Policy Integration

### Memory Ruleset #12

The system enforces minimum memory headroom requirements:

- **Minimum headroom**: 15% (configurable)
- **Violation action**: Log security event, trigger incident response
- **Recovery**: Automatic eviction of ephemeral adapters

### Performance Ruleset #11

Performance budgets are enforced:

- **CPU usage**: Warning at 70%, Critical at 90%
- **Memory usage**: Warning at 80%, Critical at 95%
- **Disk usage**: Warning at 85%, Critical at 95%
- **GPU utilization**: Warning at 80%, Critical at 95%

### Incident Response

When thresholds are exceeded:

1. **Warning**: Log telemetry event, continue operation
2. **Critical**: Log security event, trigger incident response
3. **Memory pressure**: Evict ephemeral adapters, reduce K-sparse
4. **Resource exhaustion**: Deny new sessions, alert operators

## Telemetry Integration

### Event Types

#### system.metrics
```json
{
  "event_type": "system.metrics",
  "cpu_usage": 45.2,
  "memory_usage": 62.8,
  "disk_read_bytes": 1024000,
  "disk_write_bytes": 512000,
  "network_rx_bytes": 2048000,
  "network_tx_bytes": 1536000,
  "gpu_utilization": 15.5,
  "gpu_memory_used": 1048576,
  "uptime_seconds": 86400,
  "process_count": 156,
  "load_average": {
    "load_1min": 1.2,
    "load_5min": 1.1,
    "load_15min": 1.0
  },
  "timestamp": 1640995200
}
```

#### system.threshold_violation
```json
{
  "event_type": "system.threshold_violation",
  "metric_name": "cpu_usage",
  "current_value": 95.0,
  "threshold_value": 90.0,
  "severity": "critical",
  "timestamp": 1640995200
}
```

#### system.health
```json
{
  "event_type": "system.health",
  "status": "warning",
  "checks": [
    {
      "name": "cpu_usage",
      "status": "warning",
      "message": "CPU usage 75% exceeds warning threshold 70%",
      "value": 75.0,
      "threshold": 70.0
    }
  ],
  "timestamp": 1640995200
}
```

### Sampling

- **System metrics**: 5% sampling rate (configurable)
- **Threshold violations**: 100% sampling (always logged)
- **Health checks**: 100% sampling (always logged)
- **Security events**: 100% sampling (always logged)

## Database Schema

AdapterOS supports both **SQLite and PostgreSQL** backends with automatic database selection. The schema is identical across both backends.

### system_metrics

Stores historical system metrics:

#### SQLite Schema
```sql
CREATE TABLE system_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    cpu_usage REAL NOT NULL,
    memory_usage REAL NOT NULL,
    disk_read_bytes INTEGER NOT NULL,
    disk_write_bytes INTEGER NOT NULL,
    network_rx_bytes INTEGER NOT NULL,
    network_tx_bytes INTEGER NOT NULL,
    gpu_utilization REAL,
    gpu_memory_used INTEGER,
    uptime_seconds INTEGER NOT NULL,
    process_count INTEGER NOT NULL,
    load_1min REAL NOT NULL,
    load_5min REAL NOT NULL,
    load_15min REAL NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);
```

#### PostgreSQL Schema
```sql
CREATE TABLE system_metrics (
    id SERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    cpu_usage DOUBLE PRECISION NOT NULL,
    memory_usage DOUBLE PRECISION NOT NULL,
    disk_read_bytes BIGINT NOT NULL,
    disk_write_bytes BIGINT NOT NULL,
    network_rx_bytes BIGINT NOT NULL,
    network_tx_bytes BIGINT NOT NULL,
    gpu_utilization DOUBLE PRECISION,
    gpu_memory_used BIGINT,
    uptime_seconds BIGINT NOT NULL,
    process_count INTEGER NOT NULL,
    load_1min DOUBLE PRECISION NOT NULL,
    load_5min DOUBLE PRECISION NOT NULL,
    load_15min DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_system_metrics_timestamp ON system_metrics(timestamp);
```

### threshold_violations

Tracks policy threshold violations:

#### SQLite Schema
```sql
CREATE TABLE threshold_violations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    metric_name TEXT NOT NULL,
    current_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'critical')),
    resolved_at INTEGER,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);
```

#### PostgreSQL Schema
```sql
CREATE TABLE threshold_violations (
    id SERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    metric_name TEXT NOT NULL,
    current_value DOUBLE PRECISION NOT NULL,
    threshold_value DOUBLE PRECISION NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'critical')),
    resolved_at BIGINT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_threshold_violations_timestamp ON threshold_violations(timestamp);
CREATE INDEX idx_threshold_violations_resolved ON threshold_violations(resolved_at) WHERE resolved_at IS NOT NULL;
```

### system_health_checks

Stores health check results:

#### SQLite Schema
```sql
CREATE TABLE system_health_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('healthy', 'warning', 'critical')),
    check_name TEXT NOT NULL,
    check_status TEXT NOT NULL CHECK (check_status IN ('healthy', 'warning', 'critical')),
    message TEXT NOT NULL,
    value REAL,
    threshold REAL,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);
```

#### PostgreSQL Schema
```sql
CREATE TABLE system_health_checks (
    id SERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('healthy', 'warning', 'critical')),
    check_name TEXT NOT NULL,
    check_status TEXT NOT NULL CHECK (check_status IN ('healthy', 'warning', 'critical')),
    message TEXT NOT NULL,
    value DOUBLE PRECISION,
    threshold DOUBLE PRECISION,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_health_checks_timestamp ON system_health_checks(timestamp);
```

## GPU Metrics

### Metal Integration

On macOS systems, GPU metrics are collected using Metal:

- **Device detection**: Automatic detection of Metal-capable devices
- **Performance counters**: Thread occupancy, memory throughput
- **Memory usage**: GPU memory allocation tracking
- **Utilization**: GPU compute utilization percentage

### MLX Integration

When MLX is available, additional metrics are collected:

- **MLX memory usage**: MLX-specific memory allocation
- **MLX utilization**: MLX compute utilization
- **Model loading**: MLX model memory footprint

## Monitoring Pipeline

### Continuous Monitoring

The system runs a continuous monitoring pipeline:

1. **Collection**: Collect metrics at configured interval
2. **Policy Check**: Validate against policy thresholds
3. **Telemetry**: Log events with appropriate sampling
4. **Alerting**: Trigger alerts for critical violations
5. **Storage**: Store metrics in database for historical analysis

### Performance Considerations

- **Collection overhead**: <1ms per collection
- **Memory usage**: <1MB for collector state
- **Database growth**: ~1KB per metric record
- **Network impact**: Minimal (local collection only)

## Troubleshooting

### Common Issues

#### High CPU Usage
```bash
# Check current CPU usage
aosctl metrics show

# Check CPU threshold violations
aosctl metrics violations --unresolved

# Adjust CPU thresholds if needed
aosctl metrics config --key cpu_warning --value 80.0
```

#### Memory Pressure
```bash
# Check memory headroom
aosctl metrics health

# Check memory violations
aosctl metrics violations | grep memory

# Adjust memory thresholds
aosctl metrics config --key memory_warning --value 85.0
```

#### Disk Space Issues
```bash
# Check disk usage
aosctl metrics show

# Check disk violations
aosctl metrics violations | grep disk

# Clean up old metrics data
aosctl metrics config --key retention_days --value 7
```

### Debug Mode

Enable debug logging for metrics collection:

```bash
export RUST_LOG=debug
aosctl metrics show
```

### Performance Tuning

#### Reduce Collection Frequency
```bash
aosctl metrics config --key collection_interval_secs --value 60
```

#### Adjust Sampling Rate
```bash
aosctl metrics config --key sampling_rate --value 0.01
```

#### Disable GPU Metrics
```bash
aosctl metrics config --key enable_gpu_metrics --value false
```

## Security Considerations

### Data Privacy

- **No sensitive data**: Only system resource metrics are collected
- **Local storage**: All data stored locally in SQLite database
- **No network transmission**: Metrics never leave the local system

### Access Control

- **API authentication**: Metrics API requires valid JWT token
- **CLI permissions**: Metrics CLI commands require appropriate permissions
- **Database access**: Metrics database protected by file permissions

### Audit Trail

- **Policy violations**: All threshold violations logged to telemetry
- **Configuration changes**: All config changes logged to audit trail
- **Access attempts**: All API access attempts logged

## Best Practices

### Threshold Configuration

- **Start conservative**: Begin with higher thresholds and adjust down
- **Monitor trends**: Use historical data to set appropriate thresholds
- **Test changes**: Validate threshold changes in staging environment
- **Document rationale**: Document why specific thresholds were chosen

### Monitoring Strategy

- **Baseline establishment**: Collect baseline metrics for 1-2 weeks
- **Trend analysis**: Use historical data to identify patterns
- **Alert tuning**: Adjust alerts based on false positive rates
- **Regular review**: Review and update thresholds quarterly

### Performance Optimization

- **Sampling rate**: Use appropriate sampling rate for your needs
- **Retention policy**: Set retention based on analysis requirements
- **Database maintenance**: Regularly clean up old metrics data
- **Resource usage**: Monitor the monitoring system itself

## Integration Examples

### Custom Alerting

```rust
use adapteros_system_metrics::{SystemMonitor, MetricsConfig};

let config = MetricsConfig::default();
let mut monitor = SystemMonitor::new(telemetry_writer, config);

// Custom alerting logic
if monitor.get_health_status() == SystemHealthStatus::Critical {
    // Send alert to external system
    send_alert("System health critical").await;
}
```

### Metrics Export

```rust
use adapteros_system_metrics::SystemMetricsDb;

// Automatic backend detection - works with both SQLite and PostgreSQL
let db = SystemMetricsDb::from_database(&database_connection);
let metrics = db.get_metrics_history(24, Some(1000)).await?;

// Export to external monitoring system
export_to_prometheus(metrics).await?;
```

### Policy Customization

```rust
use adapteros_system_metrics::{SystemMetricsPolicy, PerformanceThresholds};

let thresholds = PerformanceThresholds {
    max_cpu_usage: 85.0,  // Custom threshold
    max_memory_usage: 90.0,
    // ... other thresholds
};

let policy = SystemMetricsPolicy::new(thresholds);
```

## Future Enhancements

### Planned Features

1. **Predictive analytics**: ML-based anomaly detection
2. **Custom metrics**: Support for application-specific metrics
3. **Distributed monitoring**: Multi-node metrics aggregation
4. **Advanced alerting**: Complex alert rules and escalation
5. **Visualization**: Built-in metrics dashboards

### Extension Points

1. **Custom collectors**: Plugin system for custom metric collectors
2. **External integrations**: Prometheus, Grafana, DataDog support
3. **Advanced policies**: Complex policy rules and conditions
4. **Machine learning**: Anomaly detection and prediction models
