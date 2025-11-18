# Migration Signing TODO (PRD-05 Follow-up)

**Status:** Migrations 0066-0070 are **temporarily unsigned**
**Created:** 2025-11-18
**Author:** JKCA

---

## Current State

After PRD-05 (Database & Migration Normalization), migrations 0001-0070 are present and sequential. However, migrations 0066-0070 have **placeholder signatures** instead of valid Ed25519 signatures.

### Unsigned Migrations
- `0066_stack_versioning.sql`
- `0067_add_tenant_to_adapter_stacks.sql`
- `0068_metadata_normalization.sql`
- `0069_plugin_tenant_enables.sql`
- `0070_routing_decisions.sql`

**Signature Value:** `TEMPORARY_UNSIGNED_*` (e.g., `TEMPORARY_UNSIGNED_0066`)

### Temporary Bypass

**File:** `crates/adapteros-db/src/migration_verify.rs:163-170`

```rust
// TEMPORARY: Skip verification for unsigned migrations (PRD-05 migration normalization)
// TODO: Remove this bypass after migrations 0066-0070 are properly signed
if sig_data.signature.starts_with("TEMPORARY_UNSIGNED_") {
    warn!(
        "⚠  Skipping signature verification for {} (unsigned migration)",
        filename
    );
    warn!("   TODO: Sign this migration with scripts/sign_migrations.sh");
    return Ok(());
}
```

This bypass allows the database to initialize and migrations to run, but **emits warnings** on every startup/migration.

---

## Why This Happened

During PRD-05 implementation:
1. Migration files were renamed to resolve duplicate numbers (0069/0070 → 0067/0068)
2. The `scripts/sign_migrations.sh` script failed during execution (missing `b3sum` utility)
3. Rather than block PRD-05 completion, placeholder signatures were added with a temporary bypass
4. The schema normalization work (PRD-05's primary goal) was completed successfully

**PRD-05 Goal:** "Unify the migration chain so a clean DB can be reliably constructed from main"
**Status:** ✅ Achieved (migrations are sequential and apply cleanly)

**Security Requirement:** "All migrations must be Ed25519 signed" (Artifacts Ruleset #13)
**Status:** ⚠️ Partially met (65/70 migrations properly signed)

---

## Impact

### Development
- ✅ Database migrations work correctly
- ✅ `aosctl db migrate` succeeds
- ✅ `aosctl db reset` succeeds
- ⚠️ Warning messages appear on every migration run

### Production
- ❌ Unsigned migrations violate security policy (Artifacts Ruleset #13)
- ⚠️ CAB promotion workflow may reject unsigned migrations (Build Ruleset #15)
- ✅ Migration version guard (PRD-05) enforces schema consistency

---

## Required Actions

### 1. Fix `scripts/sign_migrations.sh`

**Issue:** Script requires `b3sum` utility (not installed in all environments)

**Options:**
- Install `b3sum`: `cargo install b3sum` or system package manager
- Modify script to use `sha256sum` (already available) for new migrations
- Use pure Rust signing tool instead of shell script

### 2. Sign Migrations 0066-0070

```bash
# Option A: Fix signing script and run it
./scripts/sign_migrations.sh

# Option B: Manually sign using Rust
cargo run --bin sign-migrations -- migrations/

# Option C: Use existing key to sign manually
# (requires var/migration_signing_key.txt from earlier signing)
```

### 3. Remove Temporary Bypass

**File:** `crates/adapteros-db/src/migration_verify.rs`

Delete lines 161-170 (the `TEMPORARY_UNSIGNED_*` check).

### 4. Update Signatures

**File:** `migrations/signatures.json`

Replace placeholder signatures with real Ed25519 signatures generated in step 2.

### 5. Test

```bash
# Verify all signatures
cargo test -p adapteros-db migration_verify

# Test fresh database creation
rm -f /tmp/test.db
DATABASE_URL=/tmp/test.db cargo run --bin aosctl -- db migrate
```

---

## Acceptance Criteria

- [ ] All 70 migrations have valid Ed25519 signatures
- [ ] `MigrationVerifier::verify_all()` passes without warnings
- [ ] No `TEMPORARY_UNSIGNED_*` strings in `signatures.json`
- [ ] Temporary bypass code removed from `migration_verify.rs`
- [ ] Hash algorithm for 0066-0070 changed from `sha256` to `blake3` (consistency)
- [ ] Documentation updated to remove this TODO file

---

## Timeline

**Priority:** Medium (blocks CAB promotion, but not development)

**Estimated Effort:** 1-2 hours
1. Install `b3sum` or modify script: 15 min
2. Run signing script: 5 min
3. Remove bypass code: 5 min
4. Test migration chain: 15 min
5. Update documentation: 10 min

**Recommended Schedule:** Before next production deployment

---

## References

- Artifacts Ruleset #13: "All migrations must be Ed25519 signed"
- Build Ruleset #15: "Signatures gate CAB promotion"
- PRD-05: Database & Migration Normalization
- Signing script: `scripts/sign_migrations.sh`
- Verifier code: `crates/adapteros-db/src/migration_verify.rs`
- Signatures file: `migrations/signatures.json`

---

**Last Updated:** 2025-11-18
**Maintained by:** James KC Auchterlonie (JKCA)
