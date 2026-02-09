# Telemetry Export Tests

This directory contains comprehensive integration tests for the telemetry export functionality in adapterOS.

## Test Coverage

### File: `telemetry_export_tests.rs`

#### 1. Metric Collection Tests (`metric_collection_tests` module)

Tests for collecting various types of Prometheus-compatible metrics:

- **Counter Metrics** (`test_counter_metrics_collection`)
  - Metal kernel failures tracking
  - Multiple error types and kernel types
  - Bulk increment operations

- **Gauge Metrics** (`test_gauge_metrics_collection`)
  - GPU memory pressure
  - Memory pressure ratio by pool type
  - VRAM usage tracking

- **Histogram Metrics** (`test_histogram_metrics_collection`)
  - Metal kernel execution time distribution
  - Hot-swap latency percentiles
  - Bucket, sum, and count verification

- **Hash Operation Metrics** (`test_hash_operation_metrics`)
  - BLAKE3 and SHA256 operations
  - Size bucket tracking
  - Bulk operations

- **HKDF Derivation Metrics** (`test_hkdf_derivation_metrics`)
  - Domain-specific key derivations
  - Router, dropout, sampling domains

- **Adapter Lifecycle Metrics** (`test_adapter_lifecycle_metrics`)
  - State transitions (cold → warm → hot)
  - Eviction tracking

- **KV Cache Residency Metrics** (`test_kv_cache_residency_metrics`)
  - HOT/COLD entry counts
  - Byte tracking by residency state
  - Eviction by residency type
  - Quota violations and purgeable failures

#### 2. Prometheus Export Format Tests (`prometheus_export_tests` module)

Tests validating Prometheus text format compliance:

- **Text Format Structure** (`test_prometheus_text_format_structure`)
  - HELP comments
  - TYPE comments
  - Metric with labels

- **Histogram Format** (`test_prometheus_histogram_format`)
  - Bucket labels with `le` (less than or equal)
  - Sum and count suffixes
  - Proper bucket formatting

- **Label Formatting** (`test_prometheus_label_formatting`)
  - Multiple labels per metric
  - Proper escaping and quoting
  - Tenant ID, query type, table name labels

- **Naming Conventions** (`test_prometheus_metric_naming_conventions`)
  - Counters end with `_total`
  - Time-based metrics use `_seconds` suffix
  - Adherence to Prometheus best practices

- **Gauge Values** (`test_prometheus_gauge_values`)
  - Executor tick counter
  - Hotswap queue depth
  - Pinned entries count

- **Empty Export** (`test_prometheus_export_no_metrics`)
  - Valid export with zero metrics
  - Pre-registered metric definitions

#### 3. Tenant-Scoped Metrics Tests (`tenant_scoped_metrics_tests` module)

Tests for multi-tenant isolation and metrics scoping:

- **Tenant-Scoped Database Metrics** (`test_tenant_scoped_database_metrics`)
  - Per-tenant query duration tracking
  - Different tables and query types
  - Tenant isolation verification

- **Tenant Isolation Violations** (`test_tenant_isolation_violation_metrics`)
  - Cross-tenant access attempts
  - Unauthorized query tracking
  - Resource type labels

- **Tenant Query Error Tracking** (`test_tenant_query_error_tracking`)
  - Timeout errors
  - Constraint violations
  - Per-tenant error categorization

- **Tenant Index Scans** (`test_tenant_index_scan_metrics`)
  - Index usage by tenant
  - Range vs full scans
  - Composite index tracking

- **System Tenant Label** (`test_system_tenant_label`)
  - Non-tenant-specific operations
  - Migration and system queries

#### 4. Bundle Creation Tests (`bundle_creation_tests` module)

Tests for telemetry bundle management and signing:

- **Basic Bundle Creation** (`test_bundle_writer_basic_creation`)
  - NDJSON file creation
  - Signature file generation
  - Event persistence

- **Bundle Rotation** (`test_bundle_rotation_on_event_count`)
  - Automatic rotation on event count threshold
  - Multiple bundle files
  - Sequential naming

- **Bundle Signatures** (`test_bundle_signature_creation`)
  - Merkle root computation
  - Ed25519 signature generation
  - Public key persistence
  - Metadata structure validation

- **Bundle Compression** (`test_bundle_compression`)
  - Zstd compression
  - Compression statistics
  - Metadata preservation

- **Public Key Retrieval** (`test_bundle_public_key_retrieval`)
  - Hex-encoded format
  - 64-character length (32-byte Ed25519 key)

#### 5. Bundle Store Tests (`bundle_store_tests` module)

Tests for content-addressed bundle storage:

- **Bundle Store Creation** (`test_bundle_store_creation`)
  - Directory structure
  - Retention policy initialization

- **Store and Retrieve** (`test_bundle_store_and_retrieve`)
  - Content-addressed storage
  - Hash verification on retrieval
  - Metadata persistence

- **Tenant-Scoped Storage** (`test_tenant_scoped_bundle_storage`)
  - Per-tenant directories
  - Isolated storage paths
  - Multi-tenant verification

- **List Bundles by Tenant** (`test_list_bundles_for_tenant`)
  - Tenant filtering
  - Sequence number ordering
  - Bundle enumeration

- **Content-Addressed Deduplication** (`test_content_addressed_deduplication`)
  - Identical content detection
  - Single storage instance
  - Hash-based deduplication

#### 6. UDS Exporter Tests (`uds_exporter_tests` module)

Tests for Unix Domain Socket metrics export:

- **Basic Setup** (`test_uds_exporter_basic_setup`)
  - Socket creation
  - Bind verification
  - Graceful shutdown

- **Metric Registration** (`test_uds_exporter_metric_registration`)
  - Counter registration and increment
  - Gauge registration and setting
  - Metric value verification

- **Labels Support** (`test_uds_exporter_with_labels`)
  - Multi-label metrics
  - Tenant ID labeling
  - Operation labels

- **Histogram Export** (`test_histogram_prometheus_format`)
  - Bucket formatting
  - Sum and count aggregation
  - Prometheus histogram compliance

## Running Tests

```bash
# Run all telemetry export tests
cargo test -p adapteros-telemetry --test telemetry_export_tests

# Run specific test module
cargo test -p adapteros-telemetry --test telemetry_export_tests metric_collection_tests

# Run single test
cargo test -p adapteros-telemetry --test telemetry_export_tests test_counter_metrics_collection

# Run with output
cargo test -p adapteros-telemetry --test telemetry_export_tests -- --nocapture

# Run sequentially (for debugging)
cargo test -p adapteros-telemetry --test telemetry_export_tests -- --test-threads=1
```

## Test Utilities

### `new_test_tempdir()`

Helper function to create temporary directories under `var/tmp` (via `adapteros_core::tempdir_in_var("aos-test-")`) for proper test isolation and to avoid `/tmp`-style paths.

## Coverage Summary

- **Metric Collection**: 7 tests covering counters, gauges, histograms, hash ops, HKDF, lifecycle, and KV cache
- **Prometheus Export**: 6 tests validating format compliance
- **Tenant Scoping**: 5 tests ensuring multi-tenant isolation
- **Bundle Creation**: 5 tests for NDJSON bundles with signatures
- **Bundle Store**: 5 tests for content-addressed storage
- **UDS Exporter**: 4 tests for socket-based metrics export

**Total: 32 comprehensive integration tests**

## Key Features Tested

1. **Metrics Collection**
   - Prometheus-compatible metrics (counters, gauges, histograms)
   - Tenant-scoped metrics
   - High-cardinality label support

2. **Prometheus Export**
   - Text format compliance
   - HELP and TYPE comments
   - Histogram buckets and aggregations
   - Label formatting

3. **Bundle Management**
   - NDJSON event serialization
   - Merkle tree signing with Ed25519
   - Automatic rotation
   - Compression (zstd)

4. **Tenant Isolation**
   - Per-tenant bundle directories
   - Tenant-scoped metrics
   - Isolation violation tracking

5. **Content-Addressed Storage**
   - BLAKE3 hashing
   - Deduplication
   - Integrity verification

6. **Zero Network Egress**
   - Unix Domain Socket export
   - Local-only metrics serving
   - No HTTP endpoints required

## Architecture Notes

The tests follow adapterOS design principles:

- **Determinism**: Metrics are reproducible and verifiable
- **Security**: No `/tmp` usage, proper tenant isolation
- **Auditability**: Bundle signatures and Merkle chains
- **Performance**: Content-addressed storage, efficient UDS
- **Multi-tenancy**: Explicit tenant scoping throughout

## Future Enhancements

Potential areas for additional test coverage:

1. **Compression Algorithms**: Test gzip and lz4 in addition to zstd
2. **Retention Policies**: Test eviction strategies and incident bundle preservation
3. **Chain Verification**: Test bundle chain integrity and signature verification
4. **Concurrent Access**: Test multiple writers and readers
5. **Error Scenarios**: Test disk full, permission errors, corrupted bundles
6. **Performance**: Benchmark tests for high-throughput scenarios
