# Migration Operations Guide

Quick reference for database migration tasks in AdapterOS.

---

## Verify All Migrations

```bash
bash scripts/sign_migrations.sh
```

**Expected output:**
```
✓ Successfully signed 78 migrations
✓ Verified 78/78 signatures
✓ All migrations successfully signed and verified!
```

---

## Check Migration Count

```bash
# Files on disk
ls migrations/*.sql | wc -l

# Signatures in JSON
jq '.signatures | keys | length' migrations/signatures.json

# Both should equal: 78
```

---

## List All Migrations

```bash
jq '.signatures | keys[]' migrations/signatures.json | sort
```

---

## Verify Specific Migration

```bash
# View signature and hash
jq '.signatures."0060_create_pinned_adapters_table.sql"' migrations/signatures.json

# Check hash matches file
ls -la migrations/0060_create_pinned_adapters_table.sql
```

---

## Create New Migration

```bash
# 1. Find next available number
jq '.signatures | keys[] | select(. | startswith("008"))' migrations/signatures.json | sort | tail -1

# 2. Create migration file (e.g., 0081_my_feature.sql)
touch migrations/0081_my_feature.sql

# 3. Write SQL
cat > migrations/0081_my_feature.sql << 'EOF'
-- Migration: My Feature
-- Description: Brief description of what this adds
-- Date: 2025-11-21

CREATE TABLE my_table (
    id TEXT PRIMARY KEY,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_my_table_created_at ON my_table(created_at);
EOF

# 4. Sign migration
bash scripts/sign_migrations.sh

# 5. Verify
jq '.signatures."0081_my_feature.sql"' migrations/signatures.json
```

---

## Key Files Reference

| File | Purpose | Readable |
|------|---------|----------|
| `var/migration_signing_key.txt` | Private key | Owner only (600) |
| `var/migration_signing_key.pub` | Public key | Everyone (644) |
| `migrations/signatures.json` | Signatures | Everyone (644) |

---

## Signature Verification Details

All migrations use:
- **Algorithm:** Ed25519 (FIPS 186-5)
- **Hash:** BLAKE3 (primary) or SHA256 (fallback)
- **Format:** Base64 encoded

Example signature entry:
```json
{
  "0001_init.sql": {
    "hash": "c892c5f22f9907547c7dab915c8e473bc2108bc5e7b40b120a7ef68ddd5facc4",
    "signature": "q+KagnRL8AVwXsV8wP+bcomgRPOxaM6mQi3O29lI9ArD8Uyx3txgbtibyL64qwy2hsCofY1uhMJsUPDyjwGvCQ==",
    "algorithm": "ed25519",
    "hash_algorithm": "blake3"
  }
}
```

---

## Security Checklist

- [ ] Private key secured: `chmod 600 var/migration_signing_key.txt`
- [ ] Key not committed to repo (add to .gitignore if needed)
- [ ] Signatures verified before deployment
- [ ] Audit logs checked for unauthorized modifications
- [ ] No manual SQL modifications without re-signing

---

## Troubleshooting

### Migration count mismatch
```bash
# Count files
find migrations -name "*.sql" | wc -l

# Count signatures
jq '.signatures | keys | length' migrations/signatures.json

# Find missing signatures
comm -23 <(ls migrations/*.sql | sed 's|.*/||' | sort) \
         <(jq -r '.signatures | keys[]' migrations/signatures.json | sort)
```

### Signature verification fails
```bash
# Re-sign all migrations
bash scripts/sign_migrations.sh

# If still fails, check key exists
ls -la var/migration_signing_key.txt
```

### Key missing
```bash
# Generate new key (warning: breaks previous signatures)
openssl genpkey -algorithm Ed25519 -out var/migration_signing_key.txt
chmod 600 var/migration_signing_key.txt

# Re-sign all migrations
bash scripts/sign_migrations.sh
```

---

## Maintenance

### Regular Tasks

1. **Weekly:** Run `bash scripts/sign_migrations.sh` to verify no tampering
2. **Per Release:** Verify all migrations before deployment
3. **Quarterly:** Review migration audit logs

### Migration Audit Query

```bash
# Show migration metadata
jq '.signed_at, .schema_version, .signatures | length' migrations/signatures.json

# Check public key
jq '.public_key' migrations/signatures.json | base64 -d
```

---

## Related Files

- Main reference: `/Users/star/Dev/aos/docs/MIGRATION_VERIFICATION.md`
- Signing script: `/Users/star/Dev/aos/scripts/sign_migrations.sh`
- Signatures: `/Users/star/Dev/aos/migrations/signatures.json`
- CLAUDE.md: Database migration section (lines ~495-525)

---

**Status:** All 78 migrations verified and signed
**Last Verified:** 2025-11-21
