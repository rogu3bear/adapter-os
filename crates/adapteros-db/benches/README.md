# adapterOS Database Benchmarks

This directory contains performance benchmarks comparing SQL and KV storage backends for adapter operations.

## Overview

The benchmarks measure the performance characteristics of four storage modes:

1. **SQL-only**: Traditional SQLite backend (current production default)
2. **KV-only**: New key-value backend (future target)
3. **DualWrite**: Writes to both backends, reads from SQL (migration validation)
4. **KvPrimary**: Writes to both backends, reads from KV (migration cutover)

## Benchmark Suites

### Criterion Benchmarks (Full Suite)

The `kv_vs_sql.rs` benchmark provides detailed statistical analysis using the Criterion framework.

**Note**: Currently disabled due to compilation issues in the main library during ongoing refactoring.

```bash
# Run full criterion benchmarks (when available)
cargo bench --package adapteros-db --bench kv_vs_sql

# View HTML reports
open target/criterion/report/index.html
```

**Operations Tested**:
- `get_adapter`: Single adapter retrieval by ID
- `list_adapters`: List all adapters for a tenant (10, 50, 100 adapters)
- `register_adapter`: Create new adapter (write operation)
- `update_adapter_state`: Update adapter state (write operation)
- `adapter_lineage`: Recursive lineage query (SQL CTE vs KV traversal)

### Simple Timing Benchmarks (Tests)

The `tests/kv_vs_sql_benchmark.rs` module provides lightweight timing-based benchmarks that work even during refactoring.

```bash
# Run all timing benchmarks
cargo test --package adapteros-db kv_vs_sql_benchmark -- --nocapture --ignored

# Run individual benchmarks
cargo test --package adapteros-db benchmark_get_adapter -- --nocapture --ignored
cargo test --package adapteros-db benchmark_list_adapters -- --nocapture --ignored
cargo test --package adapteros-db benchmark_register_adapter -- --nocapture --ignored
cargo test --package adapteros-db benchmark_update_adapter_state -- --nocapture --ignored
```

**Example Output**:
```
=== Benchmark: get_adapter ===

get_adapter (1000 iterations):
  SQL-only:   45.2ms (45.20 µs/op)
  KV-only:    12.3ms (12.30 µs/op)
  DualWrite:  48.1ms (48.10 µs/op)
  KvPrimary:  13.7ms (13.70 µs/op)

  KV speedup: 3.67x
```

## Performance Expectations

### Read Operations (get_adapter, list_adapters)

- **KV-only**: Expected to be 2-5x faster for point reads
  - Direct hash-based lookup
  - No SQL parsing overhead
  - Optimized for key-value access patterns

- **SQL-only**: Baseline performance
  - B-tree index lookups
  - Query planner overhead
  - Good for complex queries

- **DualWrite**: Same as SQL-only for reads
  - Reads come from SQL backend
  - KV writes are background

- **KvPrimary**: Similar to KV-only
  - Reads from KV backend
  - SQL writes are background

### Write Operations (register_adapter, update_adapter_state)

- **KV-only**: Expected to be 1.5-3x faster
  - Direct key-value writes
  - No transaction overhead
  - Simpler write path

- **SQL-only**: Baseline performance
  - Transaction management
  - Index updates
  - Foreign key checks

- **DualWrite**: Expected ~2x slower than SQL-only
  - Writes to both backends
  - Double write amplification
  - Worth it for migration validation

- **KvPrimary**: Same as DualWrite
  - Writes to both backends
  - Validates KV read path

### Complex Operations (lineage queries)

- **SQL-only**: Uses recursive CTEs
  - Efficient for deep hierarchies
  - Database-level optimization

- **KV-only**: Uses Rust-based graph traversal
  - Fetches related records iteratively
  - More network roundtrips
  - May be slower for deep trees

## Migration Strategy

The benchmarks support validating the SQL-to-KV migration path:

1. **Phase 1 (SqlOnly)**: Current production, baseline performance
2. **Phase 2 (DualWrite)**: Validate KV writes match SQL, accept 2x write overhead
3. **Phase 3 (KvPrimary)**: Test KV read path, keep SQL as safety net
4. **Phase 4 (KvOnly)**: Final state, best performance

Use benchmarks to validate each phase transition:

```bash
# Validate DualWrite doesn't degrade reads
cargo test --package adapteros-db benchmark_get_adapter -- --nocapture --ignored

# Validate KvPrimary reads are faster
cargo test --package adapteros-db benchmark_list_adapters -- --nocapture --ignored

# Check write overhead is acceptable
cargo test --package adapteros-db benchmark_register_adapter -- --nocapture --ignored
```

## Interpreting Results

### Speedup Metrics

- **1.0x-1.5x**: Marginal improvement, consider other factors
- **1.5x-3x**: Significant improvement, migration likely beneficial
- **3x+**: Major improvement, strong case for migration

### Dual-Write Overhead

- Expected: 1.5x-2.5x slower than single backend
- Acceptable during migration validation (temporary)
- Should not impact production if writes are async

### When KV is Slower

KV backend may be slower for:
- Complex joins (use SQL)
- Range scans (SQL B-tree is optimized)
- Deep recursive queries (SQL CTE is efficient)

Use hybrid approach: KV for hot data, SQL for analytics.

## Benchmark Configuration

### Database Setup

- In-memory SQLite with WAL mode
- Temporary KV database (redb format)
- Migrations applied (full schema)
- Default tenant seeded

### Test Data

- 10, 50, 100 adapters for list operations
- Unique IDs per test to avoid conflicts
- Realistic adapter configurations (rank=16, tier=warm)

### Measurement

- Multiple iterations for statistical significance
- Warmup runs to eliminate cold-start effects
- Black-box operations to prevent compiler optimization

## Troubleshooting

### Benchmark Won't Compile

If the criterion benchmark fails due to library changes:
1. Use the timing-based tests instead
2. They're more resilient to refactoring
3. Still provide useful performance data

### Inconsistent Results

- Run benchmarks multiple times
- Check for background processes
- Use `--release` mode for realistic timings
- Ensure database files are on SSD

### Performance Regression

If KV is slower than expected:
1. Check if indexes are enabled
2. Verify KV cache is working
3. Look for N+1 query patterns
4. Profile with `perf` or `flamegraph`

## Future Work

- [ ] Add benchmarks for concurrent operations
- [ ] Test with larger datasets (1K, 10K adapters)
- [ ] Benchmark with real storage (not in-memory)
- [ ] Add memory usage profiling
- [ ] Test with multiple tenants

## References

- [AGENTS.md](../../AGENTS.md): Storage migration strategy
- [docs/DATABASE.md](../../docs/DATABASE.md): Schema documentation
- [Criterion.rs](https://github.com/bheisler/criterion.rs): Benchmark framework

---

Copyright JKCA | 2025 James KC Auchterlonie
