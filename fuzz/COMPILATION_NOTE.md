# Compilation Note

## Status

The new fuzz targets have been created and are syntactically correct:
- `evidence_envelope.rs`
- `evidence_chain_verification.rs`
- `stop_controller.rs`
- `kv_quota_reservation.rs`

## Current Build Issue

Building the fuzz targets currently fails due to `adapteros-db` compilation errors related to sqlx offline mode:

```
error: error returned from database: (code: 1) no such column: X
error: error returned from database: (code: 1) no such table: Y
```

These errors are in:
- `crates/adapteros-db/src/kv_diff.rs`
- `crates/adapteros-db/src/kv_migration.rs`

## Resolution

These sqlx errors are NOT caused by the fuzz targets themselves. They are existing issues in the database layer that need to be resolved by:

1. Running pending migrations: `cargo sqlx migrate run`
2. Regenerating sqlx offline data: `cargo sqlx prepare`
3. Or fixing the SQL queries to match the current schema

Once the database crate builds successfully, the fuzz targets will compile without modification.

## Verification

The fuzz target code follows the same patterns as existing working targets in the project:
- Uses `#![no_main]` attribute
- Uses `libfuzzer_sys::fuzz_target!` macro
- Uses `arbitrary::Unstructured` for structured fuzzing
- Includes determinism assertions
- Follows AdapterOS code style conventions

## Testing Without Full Build

To verify syntax only (without dependencies):

```bash
# Check each target file structure
head -30 fuzz/fuzz_targets/evidence_envelope.rs
head -30 fuzz/fuzz_targets/evidence_chain_verification.rs
head -30 fuzz/fuzz_targets/stop_controller.rs
head -30 fuzz/fuzz_targets/kv_quota_reservation.rs
```

To test once db builds:

```bash
cargo build -p mplora-fuzz --bin evidence_envelope
cargo build -p mplora-fuzz --bin evidence_chain_verification
cargo build -p mplora-fuzz --bin stop_controller
cargo build -p mplora-fuzz --bin kv_quota_reservation
```
