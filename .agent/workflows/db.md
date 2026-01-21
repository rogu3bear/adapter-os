---
description: Database operations - migrations, status, health
---

# Database Workflow

// turbo-all

## Apply Migrations
```bash
./aosctl db migrate
```

## Check Status
```bash
./aosctl db status
```

## Create New Migration
```bash
touch migrations/V{number}__{description}.sql
# Edit the file, then sign:
./scripts/sign_migrations.sh
```

## Verify Signatures
```bash
python scripts/verify_migration_signatures.py
```

## Check for Conflicts
```bash
./scripts/check_migration_conflicts.sh
```

## Database Health
```bash
./scripts/check-db.sh
```
