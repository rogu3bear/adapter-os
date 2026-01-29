# Database Migration System

## Directory Structure

```
migrations/
├── 0001_init.sql              # First migration
├── ...                        # ~292+ migrations
├── 0292_backend_compile_flags.sql
├── signatures.json            # Ed25519 signatures
├── refinery.toml              # Legacy config
└── README.md
```

---

## CLI Commands

| Command | Purpose |
|---------|---------|
| `aosctl db migrate` | Apply pending migrations |
| `aosctl db migrate --verify-only` | Verify signatures only |
| `aosctl db migrate --db-path <path>` | Custom database |
| `aosctl db health` | Check migration health |
| `aosctl db unlock` | Clear stuck locks |
| `aosctl db reset` | Delete and recreate (dev only) |

---

## Signature System

Every migration requires Ed25519 signature in `signatures.json`:

```json
{
  "schema_version": "1.0",
  "signatures": {
    "0001_init.sql": {
      "hash": "<BLAKE3_HASH>",
      "signature": "<BASE64_ED25519_SIGNATURE>",
      "algorithm": "ed25519",
      "hash_algorithm": "blake3"
    }
  }
}
```

---

## Adding a New Migration

```bash
# 1. Create migration file (next available number)
touch migrations/0293_my_feature.sql

# 2. Write SQL
# 3. Update SQLx offline cache
cargo sqlx prepare --workspace

# 4. Sign migrations
./scripts/sign_migrations.sh

# 5. Commit both files
git add migrations/0293_my_feature.sql migrations/signatures.json
```

---

## SQLx Offline Mode

```bash
# Prepare offline cache
cargo sqlx prepare --workspace

# Verify offline builds work
SQLX_OFFLINE=1 cargo check -p adapteros-db
```

**Cache location**: `crates/adapteros-db/.sqlx/`

---

## Key Code Locations

| File | Purpose |
|------|---------|
| `crates/adapteros-db/src/lib.rs:1079` | `Db::migrate()` |
| `crates/adapteros-db/src/migration_verify.rs` | Signature verification |
| `crates/adapteros-db/src/migration_validation.rs` | Checksum/ordering |
| `crates/adapteros-cli/src/commands/db.rs` | CLI handlers |

---

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `AOS_SKIP_MIGRATION_SIGNATURES=1` | Skip verification (dev only) |
| `AOS_MIGRATION_TIMEOUT_SECS` | Timeout (default 120s) |

---

## Verification Tests

```bash
# Check all migrations have signatures
cargo test -p adapteros-db test_all_root_migrations_have_signatures

# Check for conflicts
cargo test -p adapteros-db migration_conflict_summary
```

---

## Best Practices

1. **Never skip signatures in CI/production**
2. **Always update SQLx offline cache** after adding migrations
3. **Keep migration numbers sequential**
4. **Sign immediately** after creating migrations
5. **Commit both files** - SQL and signatures.json
