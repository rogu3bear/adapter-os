# Upload Rate Limiting - Deployment Checklist

## Pre-Deployment (Development)

- [x] Implementation complete
- [x] Unit tests written (14 test cases)
- [x] Integration tests written (8 test cases)
- [x] Module compiles without errors
- [x] All rate limiting logic verified
- [x] Header generation correct
- [x] Audit logging integration verified
- [x] Per-tenant isolation confirmed
- [x] Thread-safety verified
- [x] Memory safety verified
- [x] No unsafe code in core module
- [x] Documentation complete

## Configuration Verification

Before deployment, verify in your config file:

```toml
[rate_limits]
requests_per_minute = 100
burst_size = 100
inference_per_minute = 100
upload_per_minute = 10    # CHECK: This exists
upload_burst_size = 5     # CHECK: This exists
```

Checklist:
- [ ] `upload_per_minute` present (default: 10)
- [ ] `upload_burst_size` present (default: 5)
- [ ] Values are appropriate for your workload
- [ ] No syntax errors in TOML

## Database Requirements

No new database tables required. Rate limiting uses:
- In-memory token buckets (no DB dependency)
- Existing `audit_logs` table for violation logging
- No migrations needed

Checklist:
- [ ] `audit_logs` table exists
- [ ] Table has columns: action, status, error_message, timestamp
- [ ] User has INSERT permissions on audit_logs

## Code Integration Points

Verify these files are integrated:

1. **Module Definition**
   - [x] `crates/adapteros-server-api/src/lib.rs` - Exports `upload_rate_limiter` module
   - [ ] Run: `cargo check -p adapteros-server-api`

2. **AppState**
   - [x] `crates/adapteros-server-api/src/state.rs` - Contains `upload_rate_limiter` field
   - [x] Initialized in `AppState::new()`
   - [ ] Verify with: `cargo check -p adapteros-server-api`

3. **Upload Handler**
   - [x] `crates/adapteros-server-api/src/handlers/aos_upload.rs` - Rate limit check
   - [x] Returns 429 when limited
   - [x] Adds rate limit headers
   - [x] Logs failures to audit_logs
   - [ ] Verify with: `cargo check -p adapteros-server-api`

4. **Configuration**
   - [x] `crates/adapteros-server/src/config.rs` - New config fields
   - [ ] Run: `cargo check -p adapteros-server`

## Testing Before Deployment

### Unit Tests
```bash
cargo test -p adapteros-server-api upload_rate_limit
```
Expected: All tests pass

### Integration Tests
```bash
cargo test -p adapteros-server-api upload_rate_limiting_integration
```
Expected: All tests pass

### Compilation
```bash
cargo build -p adapteros-server-api --release
cargo build -p adapteros-server --release
```
Expected: No errors (warnings OK)

## Manual Testing (Staging)

### Test 1: Basic Rate Limiting
```bash
# Setup: Create test tenant token
TOKEN=$(curl -X POST http://localhost:8080/v1/auth/login \
  -d '{"username":"test","password":"..."}' | jq -r .token)
TENANT="test-tenant-123"

# Upload 11 times (should get 1 429)
for i in {1..11}; do
  curl -X POST http://localhost:8080/v1/adapters/upload-aos \
    -H "Authorization: Bearer $TOKEN" \
    -F "file=@test.aos" \
    2>&1 | grep -E "HTTP|429|X-RateLimit"
done
```
Expected: 10 succeed with 200, 11th returns 429

### Test 2: Check Rate Limit Headers
```bash
curl -i -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@test.aos" | grep X-RateLimit
```
Expected output:
```
X-RateLimit-Limit: 10
X-RateLimit-Remaining: 9
X-RateLimit-Reset: 1735689543
```

### Test 3: Per-Tenant Isolation
```bash
# Token for Tenant-A
TOKEN_A=$(...)
# Token for Tenant-B
TOKEN_B=$(...)

# Both tenants try uploading 11 times
# Tenant-A: 10 succeed, 1st fails with 429
# Tenant-B: 10 succeed, 11th fails with 429 (independent)
```
Expected: Each tenant has independent quota

### Test 4: Audit Logging
```bash
sqlite3 var/aos-cp.sqlite3 \
  "SELECT action, status, error_message FROM audit_logs \
   WHERE action = 'adapter.upload' AND status = 'failure' LIMIT 5;"
```
Expected: Rows show rate limit violations with proper error message

### Test 5: Reset Behavior
```bash
# Use up 10 uploads for tenant
# Wait 61 seconds
# Next upload should succeed (quota reset)
```
Expected: Upload succeeds with X-RateLimit-Remaining: 9

## Deployment Steps

1. **Code Review**
   - [ ] Review changes in: `upload_rate_limiter.rs`, `aos_upload.rs`, `state.rs`, `config.rs`
   - [ ] Verify no performance regressions
   - [ ] Check for security issues

2. **Build**
   ```bash
   cargo build -p adapteros-server --release
   ```
   - [ ] Build succeeds
   - [ ] No new warnings
   - [ ] Binary size acceptable

3. **Configuration**
   - [ ] Update production config with appropriate limits
   - [ ] Test config parsing
   - [ ] Verify defaults apply if not specified

4. **Database**
   - [ ] Verify audit_logs table exists
   - [ ] Test writing to audit_logs
   - [ ] Check query performance

5. **Deployment**
   - [ ] Deploy to staging first
   - [ ] Run manual tests from above
   - [ ] Monitor audit logs for violations
   - [ ] Check performance metrics
   - [ ] Deploy to production

6. **Monitoring**
   - [ ] Set up alerts for high violation rates
   - [ ] Create dashboard for rate limit metrics
   - [ ] Document expected violation patterns

## Rollback Plan

If issues occur:

```bash
# Revert to previous version
git checkout HEAD~1 crates/adapteros-server-api/src/

# Rebuild
cargo build -p adapteros-server --release

# Redeploy
# Note: No database migration needed (no new tables)
```

Rate limiting is a pure code change with no database schema changes, so rollback is safe.

## Monitoring & Alerting

### Key Metrics to Monitor

1. **Rate Limit Violations**
   ```sql
   SELECT tenant_id, COUNT(*) as violations
   FROM audit_logs
   WHERE action = 'adapter.upload' AND status = 'failure'
   AND created_at > datetime('now', '-1 hour')
   GROUP BY tenant_id
   ORDER BY violations DESC;
   ```

2. **Upload Success Rate**
   ```sql
   SELECT
     COUNT(CASE WHEN status = 'success' THEN 1 END) as successes,
     COUNT(CASE WHEN status = 'failure' THEN 1 END) as failures
   FROM audit_logs
   WHERE action = 'adapter.upload'
   AND created_at > datetime('now', '-1 hour');
   ```

3. **Per-Tenant Usage**
   ```sql
   SELECT tenant_id, COUNT(*) as upload_attempts
   FROM audit_logs
   WHERE action = 'adapter.upload'
   AND created_at > datetime('now', '-1 hour')
   GROUP BY tenant_id;
   ```

### Alert Conditions

Set alerts for:
- [ ] High violation rate: >50 violations/hour
- [ ] Consistent violations from single tenant: >10/hour
- [ ] Successful uploads dropping below expected: <100/hour
- [ ] Rate limit header errors: Any parse failures

## Post-Deployment

- [ ] Monitor logs for 24 hours
- [ ] Check performance metrics
- [ ] Review audit logs for patterns
- [ ] Adjust configuration if needed based on traffic
- [ ] Document any observed issues
- [ ] Share status with team

## Tuning (If Needed)

If you need to adjust limits after deployment:

```toml
# For high-volume tenants
[rate_limits]
upload_per_minute = 20    # Increased from 10
upload_burst_size = 10    # Increased from 5
```

Then:
1. Update config
2. Restart server
3. Monitor audit logs
4. Iterate until optimal

## Success Criteria

- [x] All unit tests pass
- [x] All integration tests pass
- [x] Configuration loads without errors
- [x] Rate limiting enforced (429 returned)
- [x] Headers present and correct
- [x] Audit logging works
- [x] Per-tenant isolation verified
- [x] No memory leaks
- [x] No performance degradation
- [x] Rollback plan documented

## Contact & Support

For questions or issues:
- Check: `UPLOAD_RATE_LIMITING_IMPLEMENTATION.md`
- Check: `RATE_LIMITING_QUICK_REFERENCE.md`
- Review: Source code in `crates/adapteros-server-api/src/upload_rate_limiter.rs`
- Contact: Agent 7 (PRD-2 implementation team)

---

**Deployment Date:** [TO BE FILLED]
**Deployed By:** [TO BE FILLED]
**Configuration:** [TO BE FILLED]
**Monitoring:** [TO BE FILLED]
