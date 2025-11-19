# MetricsRegistry Time-Series Implementation

## Overview

This document describes the production-ready time-series storage implementation for `MetricsRegistry` in the AdapterOS system.

## Implementation Details

### Location
- **File**: `/Users/star/Dev/aos/crates/adapteros-server-api/src/telemetry/mod.rs`
- **Lines**: 453-679

### Key Features

1. **In-Memory Time-Series Storage**
   - Uses `Arc<RwLock<BTreeMap<String, Vec<MetricDataPoint>>>>` for thread-safe, ordered time-series data
   - Stores multiple named metric series with sorted data points by timestamp
   - Each data point consists of timestamp (milliseconds) and value (f64)

2. **Retention Policy**
   - Configurable retention period (default: 1 hour = 3600 seconds)
   - Automatic cleanup of old data points via `cleanup_old_data()` method
   - Removes empty series to save memory

3. **Data Recording Methods**
   - `record_metric(series_name, value)` - Records a data point with current timestamp
   - `record_metric_at(series_name, value, timestamp)` - Records with explicit timestamp
   - Binary search insertion to maintain sorted order by timestamp

4. **Data Retrieval Methods**
   - `get_series(name)` - Synchronous retrieval of a metric series
   - `get_series_async(name)` - Async variant for retrieval
   - `list_series()` - Lists all available metric series names
   - `list_series_async()` - Async variant for listing

5. **Time-Range Filtering**
   - `MetricsSeries::get_points(start, end)` - Filters data points by time range
   - Supports optional start and end timestamps
   - Returns only points within the specified range

6. **Snapshot Collection**
   - `collect_snapshot(snapshot)` - Collects metrics from MetricsCollector
   - Automatically records 21 different metric series:
     - **Latency metrics**: inference/router/kernel p50/p95/p99 (9 series)
     - **Queue depth**: request/adapter/kernel queues (3 series)
     - **Throughput**: tokens/sec, total tokens, sessions/min (3 series)
     - **System**: active sessions, memory usage, CPU % (3 series)
     - **Policy**: violations, abstain events (2 series)
     - **Adapters**: activations, evictions, active count (3 series)

7. **Background Collection Task**
   - `start_collection_task(collector, interval_secs)` - Spawns background task
   - Periodically collects metrics snapshots
   - Automatically cleans up old data
   - Returns JoinHandle for task management
   - Uses `MissedTickBehavior::Skip` to avoid backlog

8. **Statistics Methods**
   - `retention_seconds()` - Gets retention period
   - `series_count()` - Returns number of tracked series
   - `total_data_points()` - Returns total data points across all series

## Usage Example

```rust
use std::sync::Arc;
use adapteros_server_api::telemetry::{MetricsRegistry, MetricsCollector};

// Create registry with custom retention (2 hours)
let registry = Arc::new(MetricsRegistry::with_retention(7200));
let collector = Arc::new(MetricsCollector::new().unwrap());

// Start background collection task (collect every 5 seconds)
let task_handle = registry.clone().start_collection_task(
    collector.clone(),
    5  // interval in seconds
);

// Retrieve metrics series
if let Some(series) = registry.get_series("tokens_per_second") {
    // Filter for last 10 minutes
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let ten_min_ago = now - (10 * 60 * 1000);

    let recent_points = series.get_points(Some(ten_min_ago), Some(now));
    println!("Recent TPS data points: {}", recent_points.len());
}

// List all available metrics
let all_series = registry.list_series();
println!("Available metrics: {:?}", all_series);

// Get statistics
println!("Tracking {} series with {} total data points",
    registry.series_count().await,
    registry.total_data_points().await);

// Stop the collection task when done
task_handle.abort();
```

## API Integration

The handlers in `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/telemetry.rs` use these methods:

- `GET /api/metrics/series` - Uses `registry.list_series()` and `registry.get_series()`
- `GET /api/metrics/series?series_name=X` - Uses `registry.get_series()` with time-range filtering

## Data Flow

1. **Collection**: MetricsCollector gathers current metrics → produces MetricsSnapshot
2. **Recording**: MetricsRegistry receives snapshot → records 21 time-series
3. **Storage**: Data points stored in BTreeMap, sorted by timestamp
4. **Cleanup**: Old data points removed based on retention policy
5. **Retrieval**: Handlers query series by name → filter by time range → return to client

## Testing

Comprehensive test coverage includes:

1. `test_metrics_registry_creation` - Verifies initialization
2. `test_metrics_registry_record_and_retrieve` - Tests data recording and retrieval
3. `test_metrics_registry_list_series` - Tests series listing
4. `test_metrics_series_time_range_filtering` - Tests time-based filtering
5. `test_metrics_registry_cleanup` - Tests retention policy
6. `test_metrics_registry_stats` - Tests statistics methods
7. `test_metrics_registry_collect_snapshot` - Tests snapshot collection

## Performance Characteristics

- **Time Complexity**:
  - Record: O(log n) per data point (binary search insertion)
  - Retrieve: O(1) for lookup + O(n) for clone
  - Cleanup: O(n) where n is total data points
  - List: O(k) where k is number of series

- **Space Complexity**:
  - O(k * m) where k is number of series and m is average points per series
  - With 1-hour retention and 5-second intervals: ~720 points per series
  - For 21 series: ~15,120 data points maximum

## Thread Safety

- All data structures protected by `Arc<RwLock<_>>`
- Multiple readers can access simultaneously
- Writers block both readers and other writers
- Lock-free for synchronous variants using `blocking_read()`

## Future Enhancements

Potential improvements:

1. **Downsampling**: Aggregate old data points to reduce memory
2. **Persistence**: Optional disk-backed storage for historical data
3. **Compression**: Use delta encoding for timestamps
4. **Aggregations**: Pre-compute min/max/avg for time windows
5. **Labels**: Support metric labels for multi-dimensional data
6. **Export**: Direct export to Prometheus/InfluxDB formats

## Conclusion

This implementation provides a production-ready, efficient, and easy-to-use time-series storage system for AdapterOS metrics. It balances memory usage with retention needs while providing flexible querying capabilities.
