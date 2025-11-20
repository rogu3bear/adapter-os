# PRD-02 Deployment Guide

**PRD:** PRD-02 - Adapter & Stack Metadata Normalization + Version Guarantees
**Version:** 1.0
**Date:** 2025-11-19
**Status:** DRAFT - Not Ready for Production (Blockers Present)
**Author:** James KC Auchterlonie

---

## ⚠️ Pre-Deployment Checklist

**DO NOT DEPLOY** until all items are checked:

- [ ] **All 30 database tests pass** (Currently: 22/30 passing, 7 failing)
- [ ] **All compilation errors resolved** (Currently: 70 errors in lora-worker, 1 in sign-migrations)
- [ ] **TypeScript UI builds successfully** (Currently: 465 syntax errors)
- [ ] **This deployment guide reviewed and approved** by operations team
- [ ] **Rollback procedures tested** in staging environment
- [ ] **Production database backup created** and verified
- [ ] **API clients notified** of breaking changes (minimum 7-day notice recommended)
- [ ] **Security team sign-off** on JWT/RBAC changes (PRD-07)
- [ ] **Monitoring dashboards configured** for new metrics

**Current Blocker Status:** ❌ NOT READY - See AGENT_10_DOCUMENTATION_COMPLIANCE_REPORT.md for details

---

## Executive Summary

### What This Deployment Adds

**PRD-02 Implementation:**
1. Database schema versioning with `version` and `lifecycle_state` columns
2. SQL trigger enforcement of lifecycle state machine rules
3. API response schema versioning (`schema_version: "1.0.0"` in all responses)
4. Comprehensive audit trail for adapter lifecycle changes
5. Router decision telemetry for post-hoc analysis

**PRD-07 Security (Deployed Concurrently):**
1. JWT authentication with Ed25519 signatures
2. RBAC system with 5 roles and 20+ permissions
3. Enhanced tenant isolation
4. Immutable audit logging

### Breaking Changes

**⚠️ CRITICAL: This is a breaking change deployment**

**Database:**
- New columns: `version`, `lifecycle_state` in `adapters` table
- SQL triggers enforce state machine (retired is terminal, no backward transitions)
- Direct SQL updates now validated

**API:**
- All responses include `schema_version` field
- AdapterResponse includes `version` and `lifecycle_state` fields
- Invalid lifecycle transitions return 400 Bad Request instead of succeeding

**Impact:**
- API clients may break if not updated to handle new fields
- Automation scripts may fail on invalid state transitions
- Direct SQL scripts must be updated or will be rejected by triggers

### Estimated Downtime

**Total Downtime:** 5-10 minutes

**Breakdown:**
- Database migration: 2-5 minutes (depends on table size)
- Backend deployment: 1-2 minutes (binary restart)
- Frontend deployment: 30 seconds (static asset update)
- Verification: 1-2 minutes (smoke tests)

---

## Part 1: Pre-Deployment

### 1.1 Staging Environment Validation

**Run this in staging environment first:**

```bash
# 1. Deploy to staging
./scripts/deploy-staging.sh --prd-02

# 2. Run smoke tests
./scripts/smoke-test-prd02.sh

# 3. Verify database migrations
./target/release/aosctl db verify-schema --env staging

# 4. Test API responses
curl https://staging.adapteros.example.com/api/health | jq
# Expected: {"schema_version": "1.0.0", ...}

# 5. Test lifecycle state transitions
./target/release/aosctl adapter lifecycle set test-adapter active --env staging
./target/release/aosctl adapter lifecycle set test-adapter deprecated --env staging
./target/release/aosctl adapter lifecycle set test-adapter retired --env staging

# 6. Verify trigger enforcement (should fail)
./target/release/aosctl adapter lifecycle set test-adapter active --env staging
# Expected error: "Cannot transition from retired state (terminal)"

# 7. Test rollback procedure
./scripts/rollback-staging.sh --prd-02
# Verify old version works
curl https://staging.adapteros.example.com/api/health | jq
# Should NOT have schema_version field

# 8. Re-deploy (practice deployment)
./scripts/deploy-staging.sh --prd-02
```

**Staging Sign-Off:** All tests must pass before proceeding to production

### 1.2 Backup Production Database

**Create full database backup:**

```bash
# PostgreSQL
pg_dump \
  --host=prod-db.example.com \
  --username=adapteros \
  --format=custom \
  --file=backup_pre_prd02_$(date +%Y%m%d_%H%M%S).dump \
  adapteros_production

# Verify backup integrity
pg_restore --list backup_pre_prd02_*.dump | head -20

# Upload to secure backup storage
aws s3 cp backup_pre_prd02_*.dump s3://adapteros-backups/prd-02/ \
  --storage-class STANDARD_IA \
  --server-side-encryption AES256

# Verify upload
aws s3 ls s3://adapteros-backups/prd-02/
```

**Backup Retention:** Keep for minimum 30 days after successful deployment

### 1.3 Notify Stakeholders

**Notification Timeline:**

**T-7 days:** Initial notification
```
Subject: BREAKING CHANGE - AdapterOS API Update Scheduled for [DATE]

Dear AdapterOS API Consumers,

We are planning a breaking change deployment on [DATE] at [TIME] UTC.

BREAKING CHANGES:
1. All API responses will include a new "schema_version" field
2. Adapter endpoints will include "version" and "lifecycle_state" fields
3. Lifecycle state transitions will be strictly enforced

ACTION REQUIRED:
- Update your API client code to handle new fields
- Review state transition automation scripts
- Test against staging environment: https://staging.adapteros.example.com

DOCUMENTATION:
- Migration Guide: https://docs.adapteros.example.com/prd-02-migration
- API Changelog: https://docs.adapteros.example.com/changelog

Questions? Reply to this email or join our Slack channel #api-changes.

Best regards,
AdapterOS Operations Team
```

**T-1 day:** Reminder notification
**T-4 hours:** Final reminder
**T+1 hour:** Deployment complete notification

### 1.4 Schedule Maintenance Window

**Recommended Maintenance Window:** Off-peak hours (e.g., 2:00 AM - 3:00 AM UTC)

**Calendar Invite:**
- **Subject:** AdapterOS PRD-02 Deployment - BREAKING CHANGES
- **Attendees:** Operations team, on-call engineers, product owner
- **Duration:** 2 hours (includes buffer for rollback if needed)
- **Description:** See deployment guide at `/docs/PRD-02_DEPLOYMENT_GUIDE.md`

---

## Part 2: Deployment Procedure

### 2.1 Database Migration

**⚠️ CRITICAL: Run migrations in this exact order**

```bash
# 1. Put application in maintenance mode
./scripts/maintenance-mode enable --message "Database migration in progress"

# 2. Verify no active connections to tables being modified
psql -h prod-db.example.com -U adapteros -d adapteros_production -c \
  "SELECT * FROM pg_stat_activity WHERE datname='adapteros_production' AND state='active';"

# 3. Run migrations sequentially (DO NOT SKIP ANY)
./target/release/aosctl db migrate --target 0068 --confirm
# Migration 0068: Metadata normalization (adds version, lifecycle_state columns)
# Expected output: "Migration 0068 applied successfully"

./target/release/aosctl db migrate --target 0070 --confirm
# Migration 0070: Routing decisions telemetry table
# Expected output: "Migration 0070 applied successfully"

./target/release/aosctl db migrate --target 0071 --confirm
# Migration 0071: Lifecycle version history audit trail
# Expected output: "Migration 0071 applied successfully"

./target/release/aosctl db migrate --target 0075 --confirm
# Migration 0075: State transition SQL triggers (CRITICAL - enforces rules)
# Expected output: "Migration 0075 applied successfully"

./target/release/aosctl db migrate --target 0077 --confirm
# Migration 0077: JWT security (PRD-07)
# Expected output: "Migration 0077 applied successfully"

./target/release/aosctl db migrate --target 0078 --confirm
# Migration 0078: Tenant security (PRD-07)
# Expected output: "Migration 0078 applied successfully"

# 4. Verify migrations applied
./target/release/aosctl db verify-schema
# Expected output: "Schema version: 0078 - All checks passed"

# 5. Verify trigger creation
psql -h prod-db.example.com -U adapteros -d adapteros_production -c \
  "SELECT tgname FROM pg_trigger WHERE tgrelid = 'adapters'::regclass;"
# Expected output: enforce_adapter_lifecycle_transitions

# 6. Test trigger enforcement in transaction (will rollback)
psql -h prod-db.example.com -U adapteros -d adapteros_production <<SQL
BEGIN;
-- Try invalid transition (should fail)
UPDATE adapters SET lifecycle_state = 'draft'
WHERE lifecycle_state = 'active' LIMIT 1;
-- Expected error: "Cannot transition from active to draft"
ROLLBACK;
SQL
```

**Migration Timing:**
- Migration 0068: ~30-60 seconds (adds columns with defaults)
- Migration 0070: ~10-20 seconds (creates new table)
- Migration 0071: ~10-20 seconds (creates new table)
- Migration 0075: ~10-20 seconds (creates triggers and indexes)
- Migration 0077: ~10-20 seconds (JWT security table)
- Migration 0078: ~10-20 seconds (tenant security updates)
- **Total:** ~2-3 minutes

### 2.2 Backend Deployment

```bash
# 1. Build release binary
cd /path/to/aos
cargo build --release --locked
# Verify build
./target/release/adapteros-server --version
# Expected output: adapteros-server v0.05-unstable

# 2. Copy binary to deployment directory
sudo cp target/release/adapteros-server /usr/local/bin/adapteros-server-prd02
sudo chmod +x /usr/local/bin/adapteros-server-prd02

# 3. Update systemd service (zero-downtime deployment)
sudo systemctl stop adapteros-server
sudo cp /usr/local/bin/adapteros-server /usr/local/bin/adapteros-server-rollback
sudo cp /usr/local/bin/adapteros-server-prd02 /usr/local/bin/adapteros-server

# 4. Update configuration (add schema version config)
sudo vim /etc/adapteros/config.toml
# Add:
# [api]
# schema_version = "1.0.0"

# 5. Start new server
sudo systemctl start adapteros-server

# 6. Verify startup
sudo systemctl status adapteros-server
# Expected: active (running)

# 7. Check logs for errors
sudo journalctl -u adapteros-server -n 50 --no-pager
# Look for: "AdapterOS server started successfully" and no ERROR lines

# 8. Verify API health endpoint
curl http://localhost:8080/api/health | jq
# Expected: {"status": "healthy", "schema_version": "1.0.0", ...}
```

**Backend Timing:** ~1-2 minutes (systemd restart)

### 2.3 Frontend Deployment

```bash
# 1. Build UI (after TypeScript errors fixed)
cd ui/
pnpm install
pnpm build

# 2. Verify build
ls -lh dist/
# Should show index.html, assets/, etc.

# 3. Deploy to CDN/static hosting
rsync -avz --delete dist/ /var/www/adapteros/
# Or for CDN:
aws s3 sync dist/ s3://adapteros-frontend/ --delete

# 4. Clear CDN cache
aws cloudfront create-invalidation \
  --distribution-id E1234567890ABC \
  --paths "/*"

# 5. Verify deployment
curl https://app.adapteros.example.com/ | grep schema_version
# Should see TypeScript code referencing schema_version
```

**Frontend Timing:** ~30 seconds (static file copy) + CDN propagation time (1-5 minutes)

### 2.4 Disable Maintenance Mode

```bash
# 1. Verify all services healthy
./scripts/health-check.sh
# Expected: All checks passed

# 2. Disable maintenance mode
./scripts/maintenance-mode disable

# 3. Monitor error rates
# (Use your monitoring dashboard)
```

---

## Part 3: Post-Deployment Verification

### 3.1 Smoke Tests

```bash
# 1. API Health Check
curl https://api.adapteros.example.com/api/health | jq
# Expected: {"status": "healthy", "schema_version": "1.0.0"}

# 2. List Adapters (verify schema_version in response)
curl -H "Authorization: Bearer $TOKEN" \
  https://api.adapteros.example.com/api/adapters | jq '.[0]'
# Expected: Response includes "schema_version", "version", "lifecycle_state"

# 3. Test Lifecycle Transition (valid)
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"lifecycle_state": "active"}' \
  https://api.adapteros.example.com/api/adapters/test-001/lifecycle
# Expected: 200 OK

# 4. Test Lifecycle Transition (invalid - should fail)
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"lifecycle_state": "draft"}' \
  https://api.adapteros.example.com/api/adapters/test-001/lifecycle
# Expected: 400 Bad Request, error: "Invalid transition: active -> draft"

# 5. Verify JWT Authentication (PRD-07)
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "***"}' \
  https://api.adapteros.example.com/api/auth/login | jq
# Expected: JWT token returned

# 6. Verify RBAC (PRD-07)
curl -H "Authorization: Bearer $VIEWER_TOKEN" \
  -X POST \
  https://api.adapteros.example.com/api/adapters/register
# Expected: 403 Forbidden (Viewer role cannot register adapters)

# 7. Verify Audit Logging (PRD-07)
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  "https://api.adapteros.example.com/v1/audit/logs?limit=10" | jq
# Expected: Recent audit log entries returned
```

### 3.2 Monitoring Validation

**Check these metrics:**

1. **Error Rate:** Should not spike after deployment
   - Target: < 1% error rate
   - Alert threshold: > 5% error rate

2. **Response Time:** Should not increase significantly
   - Target: p95 < 200ms
   - Alert threshold: p95 > 500ms

3. **Database Query Performance:**
   - Check slow query log for new migrations
   - Verify trigger execution time < 10ms

4. **API Request Rate:**
   - Should return to normal within 5 minutes of maintenance window end
   - Alert if traffic doesn't recover

### 3.3 User Acceptance Testing

**Coordinate with QA team:**

- [ ] Test adapter creation flow in UI
- [ ] Test lifecycle state transitions in UI
- [ ] Verify version numbers display correctly
- [ ] Test error handling for invalid transitions
- [ ] Verify audit trail shows lifecycle changes

---

## Part 4: Rollback Procedures

### 4.1 When to Rollback

**Rollback if:**
- Error rate > 10% for more than 5 minutes
- Critical functionality broken (adapters cannot be created/loaded)
- Database corruption detected
- Security vulnerability discovered
- Stakeholder directive

**DO NOT rollback for:**
- Minor UI glitches (fix forward)
- Isolated client errors (likely client-side issues)
- Performance < 20% degradation (monitor and optimize)

### 4.2 Database Rollback

**Option 1: Restore from Backup (Fastest)**

```bash
# 1. Stop application
./scripts/maintenance-mode enable --message "Emergency rollback in progress"
sudo systemctl stop adapteros-server

# 2. Restore database from backup
pg_restore \
  --host=prod-db.example.com \
  --username=adapteros \
  --dbname=adapteros_production \
  --clean \
  --if-exists \
  backup_pre_prd02_*.dump

# 3. Verify restoration
psql -h prod-db.example.com -U adapteros -d adapteros_production -c \
  "SELECT COUNT(*) FROM adapters WHERE lifecycle_state IS NULL;"
# Expected: Non-zero count (old schema had NULL lifecycle_state)

# 4. Proceed to backend rollback (Section 4.3)
```

**Option 2: Rollback Migrations (Preserves Data)**

```bash
# 1. Stop application
./scripts/maintenance-mode enable --message "Emergency rollback in progress"
sudo systemctl stop adapteros-server

# 2. Create pre-rollback backup (in case rollback fails)
pg_dump -h prod-db.example.com -U adapteros -Fc \
  -f backup_pre_rollback_$(date +%Y%m%d_%H%M%S).dump \
  adapteros_production

# 3. Rollback migrations in reverse order
./target/release/aosctl db rollback --target 0077  # Remove tenant security
./target/release/aosctl db rollback --target 0076  # Remove JWT security
./target/release/aosctl db rollback --target 0074  # Remove triggers
./target/release/aosctl db rollback --target 0070  # Remove lifecycle history
./target/release/aosctl db rollback --target 0069  # Remove routing decisions
./target/release/aosctl db rollback --target 0067  # Remove metadata normalization

# 4. Verify rollback
./target/release/aosctl db verify-schema
# Expected: Schema version: 0067

# 5. Check for orphaned data
psql -h prod-db.example.com -U adapteros -d adapteros_production <<SQL
-- Check if version/lifecycle_state columns still exist (they should NOT)
SELECT column_name FROM information_schema.columns
WHERE table_name = 'adapters'
AND column_name IN ('version', 'lifecycle_state');
SQL
# Expected: No rows (columns dropped)

# 6. Proceed to backend rollback (Section 4.3)
```

**⚠️ WARNING: Option 2 loses data created during PRD-02 deployment**
- Lifecycle history table data will be lost
- Routing decision telemetry will be lost
- Version/lifecycle_state values set during deployment will be lost

**Recommendation:** Use Option 1 (restore from backup) unless data loss is acceptable

### 4.3 Backend Rollback

```bash
# 1. Restore old binary
sudo cp /usr/local/bin/adapteros-server-rollback /usr/local/bin/adapteros-server

# 2. Restore old configuration
sudo cp /etc/adapteros/config.toml.backup /etc/adapteros/config.toml

# 3. Start old server
sudo systemctl start adapteros-server

# 4. Verify startup
sudo systemctl status adapteros-server
# Expected: active (running)

# 5. Verify API health (should NOT have schema_version)
curl http://localhost:8080/api/health | jq
# Expected: {"status": "healthy"} (no schema_version field)

# 6. Disable maintenance mode
./scripts/maintenance-mode disable
```

### 4.4 Frontend Rollback

```bash
# 1. Restore old UI build
rsync -avz --delete dist.backup/ /var/www/adapteros/
# Or for CDN:
aws s3 sync s3://adapteros-frontend-backup/ s3://adapteros-frontend/ --delete

# 2. Clear CDN cache
aws cloudfront create-invalidation \
  --distribution-id E1234567890ABC \
  --paths "/*"

# 3. Verify rollback
curl https://app.adapteros.example.com/ | grep schema_version
# Expected: No matches (old UI doesn't reference schema_version)
```

### 4.5 Post-Rollback Actions

```bash
# 1. Verify all services healthy
./scripts/health-check.sh

# 2. Notify stakeholders
# (Send email: "PRD-02 deployment rolled back due to [REASON]")

# 3. Schedule post-mortem
# (Review what went wrong, plan fixes)

# 4. Document rollback in incident log
# (Include timeline, root cause, lessons learned)
```

---

## Part 5: Troubleshooting

### 5.1 Common Issues

**Issue: Database migration fails with "column already exists"**

**Cause:** Migration 0068 already partially applied

**Solution:**
```sql
-- Check if columns exist
SELECT column_name FROM information_schema.columns
WHERE table_name = 'adapters'
AND column_name IN ('version', 'lifecycle_state');

-- If they exist, mark migration as applied without re-running
INSERT INTO schema_migrations (version) VALUES (0068)
ON CONFLICT DO NOTHING;
```

---

**Issue: SQL trigger blocks legitimate state transition**

**Cause:** Trigger logic too strict or bug in application code

**Solution:**
```sql
-- Temporarily disable trigger (emergency only)
ALTER TABLE adapters DISABLE TRIGGER enforce_adapter_lifecycle_transitions;

-- Perform manual state transition
UPDATE adapters SET lifecycle_state = 'active' WHERE id = 'adapter-123';

-- Re-enable trigger
ALTER TABLE adapters ENABLE TRIGGER enforce_adapter_lifecycle_transitions;

-- File bug report: "Trigger blocked valid transition: [DETAILS]"
```

---

**Issue: API clients fail with "unknown field schema_version"**

**Cause:** Client JSON parser is strict mode, rejects unknown fields

**Solution:**
```json
// Update client JSON parser to permissive mode
// Example in Python:
import json
data = json.loads(response_text)  # Automatically ignores unknown fields

// Example in Java (Jackson):
ObjectMapper mapper = new ObjectMapper();
mapper.configure(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES, false);
```

---

**Issue: High database CPU after migration**

**Cause:** Trigger execution overhead or missing index

**Solution:**
```sql
-- Check if indexes created
SELECT indexname FROM pg_indexes WHERE tablename = 'adapters';
-- Expected: idx_adapters_lifecycle_state

-- If missing, create manually:
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state
  ON adapters(lifecycle_state);

-- Monitor trigger execution time
SELECT * FROM pg_stat_user_functions
WHERE funcname LIKE '%lifecycle%';
```

---

### 5.2 Emergency Contacts

**On-Call Engineer:** See PagerDuty schedule
**Database Team:** database-team@example.com
**Security Team:** security@example.com
**Product Owner:** product@example.com

**Escalation Path:**
1. On-call engineer (immediate)
2. Database team lead (if database issue)
3. Engineering manager (if rollback needed)
4. CTO (if security issue or extended downtime)

---

## Part 6: Success Criteria

**Deployment is successful when:**

- [ ] All database migrations applied successfully
- [ ] All services start without errors
- [ ] Smoke tests pass (Section 3.1)
- [ ] Error rate < 1% for 1 hour after deployment
- [ ] Response time p95 < 200ms
- [ ] No customer-reported issues for 24 hours
- [ ] Monitoring dashboards show green metrics
- [ ] QA team sign-off on user acceptance tests

**Deployment is considered failed if:**
- Any database migration fails
- Error rate > 5% for more than 10 minutes
- Critical functionality broken (cannot create/load adapters)
- Rollback required within first hour

---

## Part 7: Configuration Changes

### 7.1 New Configuration Fields

**File:** `/etc/adapteros/config.toml`

**Add to `[api]` section:**
```toml
[api]
# API schema version (included in all responses)
schema_version = "1.0.0"

# Enable lifecycle state validation (recommended)
enforce_lifecycle_transitions = true

# Enable version validation (recommended)
enforce_version_format = true  # Validates SemVer or monotonic
```

**Add to `[database]` section:**
```toml
[database]
# Lifecycle history retention (days)
lifecycle_history_retention_days = 90

# Routing decision retention (days)
routing_decision_retention_days = 30
```

**Add to `[security]` section (PRD-07):**
```toml
[security]
# JWT token TTL (hours)
jwt_ttl_hours = 8

# Enable RBAC enforcement
rbac_enabled = true
```

### 7.2 Environment Variables

**New required environment variables:**
```bash
# JWT signing key (Ed25519 private key)
export AOS_JWT_SIGNING_KEY="base64_encoded_ed25519_private_key"

# Admin API token (for /v1/audit endpoints)
export AOS_API_ADMIN_TOKEN="random_secure_token"
```

**Optional environment variables:**
```bash
# Enable strict mode (rejects old clients)
export AOS_API_STRICT_MODE=false

# Disable lifecycle trigger enforcement (emergency only)
export AOS_DISABLE_LIFECYCLE_TRIGGERS=false
```

---

## Part 8: Post-Deployment Tasks

### 8.1 Immediate (Within 24 Hours)

- [ ] Monitor error logs for new error patterns
- [ ] Review slow query log for trigger performance
- [ ] Check audit logs for failed lifecycle transitions
- [ ] Update internal documentation with new API fields
- [ ] Send post-deployment summary to stakeholders

### 8.2 Short-Term (Within 1 Week)

- [ ] Schedule post-deployment retrospective
- [ ] Update API documentation with examples
- [ ] Create migration guide for API consumers
- [ ] Implement monitoring alerts for new metrics
- [ ] Benchmark lifecycle history table growth rate

### 8.3 Long-Term (Within 1 Month)

- [ ] Implement archiving for lifecycle history (90-day retention)
- [ ] Implement archiving for routing decisions (30-day retention)
- [ ] Create performance optimization plan if trigger overhead > 10ms
- [ ] Evaluate client migration success rate
- [ ] Plan next PRD deployment (incorporate lessons learned)

---

## Appendices

### Appendix A: Migration SQL Reference

**Migration 0068: Metadata Normalization**
```sql
-- Add version and lifecycle_state columns
ALTER TABLE adapters
  ADD COLUMN version TEXT DEFAULT '0.1.0',
  ADD COLUMN lifecycle_state TEXT DEFAULT 'draft';

-- Add NOT NULL constraints after backfilling
ALTER TABLE adapters
  ALTER COLUMN version SET NOT NULL,
  ALTER COLUMN lifecycle_state SET NOT NULL;
```

**Migration 0075: State Transition Triggers**
```sql
-- Create trigger function
CREATE OR REPLACE FUNCTION enforce_adapter_lifecycle_transitions()
RETURNS TRIGGER AS $$
BEGIN
  -- Rule 1: Retired is terminal
  IF OLD.lifecycle_state = 'retired' THEN
    RAISE EXCEPTION 'Cannot transition from retired state (terminal)';
  END IF;

  -- Rule 2: Ephemeral cannot be deprecated
  IF NEW.tier = 'ephemeral' AND NEW.lifecycle_state = 'deprecated' THEN
    RAISE EXCEPTION 'Ephemeral adapters cannot be deprecated';
  END IF;

  -- Rule 3: No backward transitions
  IF (OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'draft') OR
     (OLD.lifecycle_state = 'deprecated' AND NEW.lifecycle_state IN ('draft', 'active')) THEN
    RAISE EXCEPTION 'Invalid backward transition: % -> %', OLD.lifecycle_state, NEW.lifecycle_state;
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Attach trigger
CREATE TRIGGER enforce_adapter_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapters
FOR EACH ROW
WHEN (OLD.lifecycle_state != NEW.lifecycle_state)
EXECUTE FUNCTION enforce_adapter_lifecycle_transitions();
```

### Appendix B: Rollback SQL Reference

**Rollback Migration 0075:**
```sql
-- Drop trigger
DROP TRIGGER IF EXISTS enforce_adapter_lifecycle_transitions ON adapters;

-- Drop trigger function
DROP FUNCTION IF EXISTS enforce_adapter_lifecycle_transitions();

-- Drop indexes
DROP INDEX IF EXISTS idx_adapters_lifecycle_state;
```

**Rollback Migration 0068:**
```sql
-- Drop columns (WARNING: Loses data)
ALTER TABLE adapters
  DROP COLUMN IF EXISTS version,
  DROP COLUMN IF EXISTS lifecycle_state;
```

### Appendix C: API Response Examples

**Old API Response (Pre-PRD-02):**
```json
{
  "id": "adapter-123",
  "name": "code-review-adapter",
  "tier": "production",
  "rank": 16
}
```

**New API Response (Post-PRD-02):**
```json
{
  "schema_version": "1.0.0",
  "id": "adapter-123",
  "name": "code-review-adapter",
  "tier": "production",
  "rank": 16,
  "version": "2.1.0",
  "lifecycle_state": "active"
}
```

---

**Deployment Guide Status:** DRAFT - Not Ready for Production
**Last Updated:** 2025-11-19
**Next Review:** After all blockers resolved
**Owner:** Operations Team
**Approver:** Engineering Manager

---

**END OF DEPLOYMENT GUIDE**
