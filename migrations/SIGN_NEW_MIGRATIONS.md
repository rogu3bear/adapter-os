# New Migrations Requiring Ed25519 Signatures

**Status:** PENDING SIGNATURE
**Date:** 2025-11-19

## Migrations Needing Signatures

The following new migrations have been created but require Ed25519 signatures before deployment:

1. **0072_tenant_snapshots.sql**
   - Purpose: Tenant state snapshots for point-in-time recovery
   - Renumbered from crate migration 0066

2. **0073_index_hashes.sql**
   - Purpose: Index hash tracking for integrity verification
   - Renumbered from crate migration 0067

3. **0074_legacy_index_migration.sql**
   - Purpose: Index version tracking and hash recomputation
   - Renumbered from crate migration 0068

## How to Sign

Run the migration signing script to generate Ed25519 signatures and update `migrations/signatures.json`:

```bash
# If sign-migrations binary exists:
cargo run --bin sign-migrations

# Or if shell script exists:
./scripts/sign_migrations.sh

# Or manually using adapteros-db migration verification tool:
cargo run --package adapteros-db --bin verify-migrations -- sign
```

## Signature Format

Each migration requires a BLAKE3 hash and Ed25519 signature in the following format:

```json
{
  "0072_tenant_snapshots.sql": {
    "hash": "<BLAKE3_HASH>",
    "signature": "<BASE64_ED25519_SIGNATURE>",
    "algorithm": "ed25519",
    "hash_algorithm": "blake3"
  }
}
```

## Verification

After signing, verify all signatures:

```bash
cargo test -p adapteros-db test_all_root_migrations_have_signatures
cargo test -p adapteros-db migration_conflict_summary
```

## Local escape hatch

For local debugging only, you may set `AOS_SKIP_MIGRATION_SIGNATURES=1` to temporarily bypass verification. Do **not** use this flag in CI, release builds, or any shared environment.

## Critical

**DO NOT deploy these migrations to production until they are properly signed.**

The migration verification system will reject unsigned migrations when `require_signed_migrations=true` in production mode.

---

**Maintained by:** James KC Auchterlonie
**Sign Before:** Production deployment
MLNavigator Inc 2025-12-07.
