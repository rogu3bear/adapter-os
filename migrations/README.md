# Database Migrations

## Schema Version
Current: 0271 (273 total files including placeholders)

## Intentional Gaps
The following migration numbers are intentionally skipped:
- 0136: Reserved for future auth schema changes
- 0180: Reserved for future federation features

## Running Migrations
```bash
./aosctl db migrate              # Apply pending migrations
./aosctl db migrate --verify-only # Verify signatures without running migrations
./aosctl db health               # Check migration health and DB integrity
```

## Adding New Migrations
1. Create `migrations/NNNN_description.sql` (use next available number)
2. Run `cargo sqlx prepare --workspace` to update offline cache
3. Sign: `./scripts/sign_migrations.sh`
4. Commit both the migration and updated `signatures.json`

## Signature Requirements

All migrations require Ed25519 signatures for deployment. The migration verification system will reject unsigned migrations when `require_signed_migrations=true` in production mode.

### Signing Process
```bash
# Sign all migrations (generates/updates signatures.json)
./scripts/sign_migrations.sh

# Verify signatures
cargo test -p adapteros-db test_all_root_migrations_have_signatures
```

### Local Development Escape Hatch
For local debugging only, you may set `AOS_SKIP_MIGRATION_SIGNATURES=1` to temporarily bypass verification. **Do not use this flag in CI, release builds, or any shared environment.**

## CI Verification

The CI pipeline includes an `sqlx-offline` job that:
1. Runs all migrations against a fresh SQLite database
2. Verifies the committed SQLx offline cache matches the schema
3. Confirms all SQLx crates compile in offline mode

See `.github/workflows/ci.yml` for details.

## Additional Documentation

- [SIGN_NEW_MIGRATIONS.md](./SIGN_NEW_MIGRATIONS.md) - Pending signatures tracking
- [docs/DATABASE.md](../docs/DATABASE.md) - Database architecture overview
