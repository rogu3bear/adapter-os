# DB Migrations Policy

## Naming And Ordering

AdapterOS uses two migration filename schemes in `migrations/`:

- **Sequential (legacy):** `NNNN_description.sql` (four-digit, zero-padded)
- **Timestamp (preferred for new work):** `YYYYMMDDHHmmss_description.sql` (14 digits)

SQLx orders migrations by the numeric prefix in the filename.

## Sequential Cap (0301)

The sequential `NNNN_*.sql` series is intentionally capped at **`0301`**. This is enforced by
`crates/adapteros-db/tests/migration_conflicts.rs` so that new schema changes do not require
renumbering or reshuffling the legacy sequence.

If you need a new migration, prefer the timestamp format.

## Determinism Notes

Migration `0301_add_adapter_stable_id.sql` introduces `adapters.stable_id` and backfills it in a
deterministic order (`created_at ASC, id ASC` per tenant). Do not change the meaning of
`adapters.stable_id` or the backfill ordering: routing determinism depends on stable tie-breaking.

## Compatibility

Do not renumber or delete previously shipped migrations. If a migration must be superseded, add a
new migration that is safe to apply on top of existing databases.

