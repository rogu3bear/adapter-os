# Runbook: Determinism Violation

**Scenario:** Replay produces different output or hash mismatch detected

**Severity:** SEV-1 (Immediate response - CRITICAL)

**Last Updated:** 2025-12-15

---

## ⚠️ CRITICAL ALERT ⚠️

**Determinism violations are ZERO-TOLERANCE incidents. Any violation indicates:**
- Potential data corruption
- Audit trail compromise
- Policy enforcement bypass
- Regulatory compliance failure

**Immediate Actions Required:**
1. Quarantine affected adapter(s)
2. Stop all inference using affected adapters
3. Preserve system state for forensics
4. Page security team and senior engineer

---

## Symptoms

### Alert Indicators
- **Alert:** `DeterminismViolation` (immediate page, no delay)
- **Alert:** `ReplayMismatch` (replay hash differs from original)
- **Alert:** `HashVerificationFailed` (adapter hash mismatch)
- **Prometheus Query:** `increase(determinism_violations_total[5m]) > 0`

### User Reports
- "Replay shows different results"
- Audit verification failing
- Policy enforcement warnings
- Evidence hash mismatches

### System Indicators
- `determinism_violations_total` counter incrementing
- `replay_hash_mismatch` events in telemetry
- Policy audit chain broken
- Evidence verification failing

---

## Diagnosis Steps

### 1. Verify Violation Occurred

```bash
# Check determinism violation counter
curl -s 'http://localhost:9090/api/v1/query?query=determinism_violations_total' | jq .

# Get violation details from database
sqlite3 var/aos-cp.sqlite3 "
SELECT violation_id, violation_type, adapter_id, timestamp,
       expected_hash, actual_hash, metadata
FROM determinism_violations
ORDER BY timestamp DESC
LIMIT 10;"

# Check recent replay attempts
sqlite3 var/aos-cp.sqlite3 "
SELECT replay_id, original_inference_id, status,
       original_hash, replay_hash, mismatch_reason
FROM replay_executions
WHERE status='MISMATCH'
ORDER BY created_at DESC
LIMIT 10;"

# Review violation logs
grep -i "determinism.*violation\|hash.*mismatch\|replay.*fail" var/aos-cp.log | tail -50
```

**Violation Types:**
- **hash_mismatch:** Output hash differs between runs
- **adapter_corruption:** Adapter file hash changed
- **seed_divergence:** RNG seed produced different sequence
- **router_nondeterminism:** Router selected different adapters
- **buffer_corruption:** GPU buffer data corruption

### 2. Identify Affected Adapter(s)

```bash
# Get adapter IDs from violations
AFFECTED_ADAPTERS=$(sqlite3 var/aos-cp.sqlite3 "
SELECT DISTINCT adapter_id
FROM determinism_violations
WHERE timestamp > datetime('now', '-1 hour');")

echo "Affected adapters: $AFFECTED_ADAPTERS"

# Get adapter details
for adapter in $AFFECTED_ADAPTERS; do
    echo "=== Adapter: $adapter ==="
    sqlite3 var/aos-cp.sqlite3 "
    SELECT adapter_id, name, status, version, manifest_hash, last_verified_at
    FROM adapters
    WHERE adapter_id='$adapter';"
done

# Check if adapter currently loaded
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, status, loaded_at
FROM adapters
WHERE adapter_id IN ($AFFECTED_ADAPTERS)
  AND status='Loaded';"
```

### 3. Analyze Violation Pattern

```bash
# Identify violation type
sqlite3 var/aos-cp.sqlite3 "
SELECT violation_type, COUNT(*) as count
FROM determinism_violations
WHERE timestamp > datetime('now', '-1 hour')
GROUP BY violation_type
ORDER BY count DESC;"

# Check if specific to one tenant or global
sqlite3 var/aos-cp.sqlite3 "
SELECT tenant_id, COUNT(*) as violation_count
FROM determinism_violations
WHERE timestamp > datetime('now', '-1 hour')
GROUP BY tenant_id;"

# Check if correlated with specific operation
sqlite3 var/aos-cp.sqlite3 "
SELECT operation_type, COUNT(*) as count
FROM determinism_violations
WHERE timestamp > datetime('now', '-1 hour')
GROUP BY operation_type;"
```

**Common Patterns:**
- **Single adapter, multiple violations:** Adapter corruption
- **Multiple adapters, same tenant:** Tenant-specific issue
- **All adapters globally:** Systemic issue (backend, router)
- **Specific operation only:** Code path bug

### 4. Verify Critical Invariants

**Q15 Denominator Check (CRITICAL):**
```bash
# Verify Q15 denominator is 32767.0 (NOT 32768)
grep -n "32767\|32768" crates/adapteros-lora-router/src/lib.rs

# Expected: Only 32767.0 should appear
# If 32768 found: CRITICAL BUG - immediate fix required
```

**Seed Derivation Check:**
```bash
# Verify HKDF-SHA256 seed derivation
grep -A 10 "derive_seed" crates/adapteros-core/src/seed.rs

# Check for any uses of non-deterministic RNG
grep -r "rand::" crates/adapteros-lora-router/ crates/adapteros-lora-worker/
# Should use derive_seed(), NOT rand::thread_rng()
```

**Router Determinism Check:**
```bash
# Verify router sorting is deterministic
grep -A 20 "sort_by\|sort_unstable" crates/adapteros-lora-router/src/lib.rs

# Expected: sort_by with (score DESC, index ASC) tie-breaking
# If sort_unstable_by without tie-breaker: NON-DETERMINISTIC
```

**Compiler Flags Check (CRITICAL):**
```bash
# Verify no -ffast-math in build
grep -r "ffast-math" Cargo.toml */Cargo.toml

# Expected: Should NOT appear anywhere
# If found: CRITICAL - breaks determinism, rebuild immediately
```

### 5. Collect Forensic Evidence

```bash
# Capture system state BEFORE any remediation
mkdir -p var/forensics/$(date +%Y%m%d_%H%M%S)
FORENSICS_DIR="var/forensics/$(date +%Y%m%d_%H%M%S)"

# 1. Database snapshot
sqlite3 var/aos-cp.sqlite3 ".backup '$FORENSICS_DIR/aos-cp-snapshot.sqlite3'"

# 2. Worker state dump (if worker supports it)
WORKER_PID=$(pgrep -f aos-worker)
if [ -n "$WORKER_PID" ]; then
    kill -USR1 $WORKER_PID  # Trigger state dump
    sleep 2
    cp var/aos-worker-state.dump "$FORENSICS_DIR/"
fi

# 3. Adapter files (affected ones)
for adapter in $AFFECTED_ADAPTERS; do
    ADAPTER_PATH=$(sqlite3 var/aos-cp.sqlite3 "SELECT file_path FROM adapters WHERE adapter_id='$adapter';")
    cp "$ADAPTER_PATH" "$FORENSICS_DIR/"
    sha256sum "$ADAPTER_PATH" > "$FORENSICS_DIR/$(basename $ADAPTER_PATH).sha256"
done

# 4. Recent logs
tail -1000 var/aos-cp.log > "$FORENSICS_DIR/aos-cp.log"
tail -1000 var/aos-worker.log > "$FORENSICS_DIR/aos-worker.log"

# 5. Violation details
sqlite3 var/aos-cp.sqlite3 "
SELECT * FROM determinism_violations
WHERE timestamp > datetime('now', '-24 hours');" > "$FORENSICS_DIR/violations.txt"

# 6. System metrics at time of violation
sqlite3 var/aos-cp.sqlite3 "
SELECT * FROM system_metrics
WHERE timestamp > datetime('now', '-1 hour');" > "$FORENSICS_DIR/metrics.txt"

echo "Forensics collected in: $FORENSICS_DIR"
ls -lah "$FORENSICS_DIR"
```

---

## Resolution

### Immediate Action: Quarantine Affected Adapters

**MANDATORY FIRST STEP:**

```bash
# 1. Get list of affected adapters
AFFECTED_ADAPTERS=$(sqlite3 var/aos-cp.sqlite3 "
SELECT DISTINCT adapter_id
FROM determinism_violations
WHERE timestamp > datetime('now', '-1 hour');")

# 2. Quarantine each adapter
for adapter in $AFFECTED_ADAPTERS; do
    echo "Quarantining adapter: $adapter"

    curl -X POST "http://localhost:8080/api/v1/adapters/$adapter/quarantine" \
      -H "Content-Type: application/json" \
      -d '{
        "reason": "Determinism violation detected",
        "severity": "critical",
        "ticket_id": "INC-$(date +%Y%m%d-%H%M%S)"
      }'

    # Or via database
    sqlite3 var/aos-cp.sqlite3 "
    UPDATE adapters
    SET status='Quarantined',
        quarantine_reason='Determinism violation - hash mismatch',
        quarantined_at=datetime('now')
    WHERE adapter_id='$adapter';"
done

# 3. Verify quarantine
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, status, quarantine_reason
FROM adapters
WHERE status='Quarantined'
  AND quarantined_at > datetime('now', '-10 minutes');"

# 4. Unload from memory immediately
for adapter in $AFFECTED_ADAPTERS; do
    curl -X POST "http://localhost:8080/api/v1/adapters/$adapter/unload"
done
```

### Root Cause: Adapter Corruption

**If violation_type = 'adapter_corruption' or 'hash_mismatch':**

```bash
# 1. Verify adapter file integrity
ADAPTER_PATH=$(sqlite3 var/aos-cp.sqlite3 "SELECT file_path FROM adapters WHERE adapter_id='$adapter';")

# Get expected hash from database
EXPECTED_HASH=$(sqlite3 var/aos-cp.sqlite3 "SELECT manifest_hash FROM adapters WHERE adapter_id='$adapter';")

# Calculate actual hash
ACTUAL_HASH=$(sha256sum "$ADAPTER_PATH" | awk '{print $1}')

echo "Expected: $EXPECTED_HASH"
echo "Actual:   $ACTUAL_HASH"

# 2. If hashes differ, adapter is corrupted
if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
    echo "CORRUPTION CONFIRMED: Adapter file modified"

    # Check if backup exists
    if [ -f "${ADAPTER_PATH}.backup" ]; then
        echo "Restoring from backup..."
        cp "${ADAPTER_PATH}.backup" "$ADAPTER_PATH"

        # Verify backup
        BACKUP_HASH=$(sha256sum "$ADAPTER_PATH" | awk '{print $1}')
        if [ "$BACKUP_HASH" == "$EXPECTED_HASH" ]; then
            echo "Backup restored successfully"
        else
            echo "ERROR: Backup also corrupted - re-download from registry"
        fi
    else
        echo "No backup found - re-download from registry"
    fi
fi

# 3. Re-download from registry if available
./aosctl adapter download --id "$adapter" --verify-hash

# 4. Re-verify hash
NEW_HASH=$(sha256sum "$ADAPTER_PATH" | awk '{print $1}')
if [ "$NEW_HASH" == "$EXPECTED_HASH" ]; then
    echo "Adapter restored successfully"

    # Un-quarantine
    sqlite3 var/aos-cp.sqlite3 "
    UPDATE adapters
    SET status='Available', quarantined_at=NULL, quarantine_reason=NULL
    WHERE adapter_id='$adapter';"
else
    echo "ERROR: Cannot restore adapter - escalate to security team"
fi
```

### Root Cause: Router Non-Determinism

**If violation_type = 'router_nondeterminism':**

```bash
# 1. Review router decision for affected inference
sqlite3 var/aos-cp.sqlite3 "
SELECT routing_id, adapter_ids_json, gates_json, k_selected
FROM routing_decisions
WHERE inference_id IN (
    SELECT inference_id FROM determinism_violations
    WHERE violation_type='router_nondeterminism'
    LIMIT 10
);"

# 2. Check if router seed is deterministic
grep -A 10 "router_seed\|derive_seed" crates/adapteros-lora-router/src/lib.rs

# 3. Verify sorting logic
# Router MUST sort by: (score DESC, index ASC) for tie-breaking
grep -A 20 "sort_by.*score" crates/adapteros-lora-router/src/lib.rs

# 4. If sorting logic incorrect, this is a CODE BUG
echo "If router sorting is not deterministic, this requires emergency hotfix"
echo "Contact senior engineer immediately"

# 5. Check Q15 gate computation
grep -n "32767\|q15" crates/adapteros-lora-router/src/lib.rs
# MUST use 32767.0 as denominator, NOT 32768.0
```

### Root Cause: Seed Divergence

**If violation_type = 'seed_divergence':**

```bash
# 1. Verify HKDF seed derivation
grep -A 20 "derive_seed" crates/adapteros-core/src/seed.rs

# 2. Check for non-deterministic RNG usage
# MUST use derive_seed(), NOT rand::thread_rng()
grep -r "thread_rng\|OsRng\|random" crates/adapteros-lora-router/ crates/adapteros-lora-worker/

# 3. If non-deterministic RNG found, CODE BUG
echo "Non-deterministic RNG detected - emergency hotfix required"

# 4. Verify global seed is set
grep "BLAKE3_GLOBAL_SEED" var/aos-cp.log | head -5

# 5. Check for seed corruption in database
sqlite3 var/aos-cp.sqlite3 "
SELECT seed_hash, created_at
FROM seed_metadata
ORDER BY created_at DESC
LIMIT 10;"
```

### Root Cause: GPU Buffer Corruption

**If violation_type = 'buffer_corruption':**

```bash
# 1. Check for GPU errors in logs
grep -i "metal.*error\|gpu.*error\|buffer.*corrupt" var/aos-worker.log | tail -50

# 2. Check for thermal throttling (macOS)
sudo powermetrics --samplers gpu_power -n 1 | grep -i "temp\|throttle"

# 3. Verify Metal command buffer status
grep -i "command.*buffer\|mtl.*error" var/aos-worker.log | tail -20

# 4. Check for memory pressure during inference
sqlite3 var/aos-cp.sqlite3 "
SELECT timestamp, memory_pressure_level, gpu_utilization
FROM system_metrics
WHERE timestamp > datetime('now', '-1 hour')
  AND memory_pressure_level > 0.75;"

# 5. If GPU hardware issue suspected:
echo "Possible hardware failure - run GPU diagnostics"
echo "Consider switching to CPU backend temporarily"

# Fallback to CPU (if available)
# Edit configs/cp.toml:
# [backend]
# fallback_to_cpu = true
```

---

## Validation

After remediation, verify determinism restored:

```bash
# 1. Run determinism test suite
cargo test --package adapteros-lora-router determinism_tests -- --nocapture

# 2. Test replay with same inputs
# Get original inference details
ORIGINAL_INFERENCE_ID="<from violation log>"

sqlite3 var/aos-cp.sqlite3 "
SELECT prompt, adapter_id, manifest_hash, router_seed, sampling_params_json
FROM inference_trace
WHERE inference_id='$ORIGINAL_INFERENCE_ID';"

# Replay inference
curl -X POST "http://localhost:8080/api/v1/replay/$ORIGINAL_INFERENCE_ID" \
  -H "Content-Type: application/json"

# Compare hashes
sqlite3 var/aos-cp.sqlite3 "
SELECT original_hash, replay_hash,
       CASE WHEN original_hash = replay_hash THEN 'MATCH' ELSE 'MISMATCH' END as status
FROM replay_executions
WHERE original_inference_id='$ORIGINAL_INFERENCE_ID'
ORDER BY created_at DESC
LIMIT 1;"

# 3. Monitor for new violations (should be zero)
watch -n 30 'curl -s "http://localhost:9090/api/v1/query?query=determinism_violations_total" | jq .'

# 4. Run extended determinism check
make determinism-check

# 5. Verify policy audit chain intact
sqlite3 var/aos-cp.sqlite3 "
SELECT COUNT(*) as broken_chains
FROM policy_audit
WHERE previous_hash IS NOT NULL
  AND previous_hash != (
    SELECT event_hash FROM policy_audit pa2
    WHERE pa2.sequence_num = policy_audit.sequence_num - 1
  );"

# Expected: 0 broken chains
```

**Success Criteria:**
- Replay produces identical hash
- Determinism test suite passes
- No new violations for 24 hours
- Policy audit chain valid
- Adapter re-verified and working

---

## Root Cause Prevention

### Post-Incident Actions

1. **Add Continuous Determinism Testing:**
   ```yaml
   # Add to CI/CD pipeline
   - name: Determinism Tests
     run: |
       cargo test --workspace determinism_tests
       ./scripts/replay-regression-suite.sh
     on:
       pull_request:
       schedule:
         - cron: '0 */6 * * *'  # Every 6 hours
   ```

2. **Implement Adapter Integrity Monitoring:**
   ```bash
   # Cron job to verify adapter hashes
   cat > scripts/verify-adapter-integrity.sh <<'EOF'
   #!/bin/bash
   # Verify all adapter file hashes match database

   sqlite3 var/aos-cp.sqlite3 "SELECT adapter_id, file_path, manifest_hash FROM adapters;" | \
   while IFS='|' read adapter path expected_hash; do
       if [ -f "$path" ]; then
           actual_hash=$(sha256sum "$path" | awk '{print $1}')
           if [ "$actual_hash" != "$expected_hash" ]; then
               echo "CORRUPTION: $adapter - expected $expected_hash, got $actual_hash"
               # Auto-quarantine
               sqlite3 var/aos-cp.sqlite3 "UPDATE adapters SET status='Quarantined', quarantine_reason='Hash mismatch detected' WHERE adapter_id='$adapter';"
           fi
       fi
   done
   EOF

   chmod +x scripts/verify-adapter-integrity.sh
   # Run hourly
   (crontab -l; echo "0 * * * * /path/to/adapter-os/scripts/verify-adapter-integrity.sh") | crontab -
   ```

3. **Enhanced Determinism Alerts:**
   ```yaml
   groups:
     - name: adapteros.determinism
       rules:
         - alert: DeterminismViolation
           expr: increase(determinism_violations_total[5m]) > 0
           for: 0m  # Immediate page
           labels:
             severity: critical
             oncall_override: "page_immediately"
           annotations:
             summary: "CRITICAL: Determinism violation detected"
             description: "Violation type: {{ $labels.violation_type }}, Adapter: {{ $labels.adapter_id }}"
             action: "Quarantine adapter immediately, preserve forensics, page security team"
             runbook: "docs/runbooks/DETERMINISM_VIOLATION.md"

         - alert: ReplayFailureRate
           expr: rate(replay_failures_total[10m]) > 0.01
           for: 5m
           labels:
             severity: warning
           annotations:
             summary: "Replay failure rate elevated"
             action: "Investigate before failures become violations"
   ```

4. **Code Guardrails:**
   ```rust
   // Add compile-time checks
   #[cfg(test)]
   mod determinism_invariants {
       use super::*;

       #[test]
       fn verify_q15_denominator() {
           // Ensure Q15 denominator is EXACTLY 32767.0
           assert_eq!(Q15_DENOMINATOR, 32767.0, "Q15 denominator MUST be 32767.0");
       }

       #[test]
       fn verify_no_fast_math() {
           // Ensure -ffast-math is not enabled
           #[cfg(feature = "fast-math")]
           compile_error!("fast-math breaks determinism - FORBIDDEN");
       }

       #[test]
       fn verify_router_sorting_deterministic() {
           // Test that sorting is deterministic with tie-breaking
           let mut decisions = generate_tied_scores();
           decisions.sort_by(|a, b| {
               b.score.partial_cmp(&a.score)
                   .unwrap_or(Ordering::Equal)
                   .then_with(|| a.index.cmp(&b.index))  // MUST have tie-breaker
           });
           // Verify order is deterministic
       }
   }
   ```

### Monitoring Improvements

**Replay Regression Suite:**
```bash
# scripts/replay-regression-suite.sh
#!/bin/bash
# Test replay determinism on known-good inferences

TEST_INFERENCES=(
    "inf_test_1"
    "inf_test_2"
    "inf_test_3"
)

FAILED=0

for inf_id in "${TEST_INFERENCES[@]}"; do
    echo "Testing replay: $inf_id"

    # Replay
    curl -X POST "http://localhost:8080/api/v1/replay/$inf_id" > /dev/null

    # Check hash
    RESULT=$(sqlite3 var/aos-cp.sqlite3 "
    SELECT CASE WHEN original_hash = replay_hash THEN 'PASS' ELSE 'FAIL' END
    FROM replay_executions
    WHERE original_inference_id='$inf_id'
    ORDER BY created_at DESC LIMIT 1;")

    if [ "$RESULT" != "PASS" ]; then
        echo "FAILED: $inf_id"
        FAILED=$((FAILED + 1))
    fi
done

if [ $FAILED -gt 0 ]; then
    echo "DETERMINISM REGRESSION: $FAILED tests failed"
    exit 1
fi

echo "All replay tests passed"
```

---

## Escalation

### Escalate to Security Team IMMEDIATELY:
- **ALL determinism violations** (zero-tolerance)
- Policy audit chain broken
- Adapter tampering suspected
- Data corruption detected

### Escalate to Senior Engineer IMMEDIATELY:
- Root cause is code bug (router, seed derivation)
- Systemic issue affecting multiple adapters
- GPU/hardware failure suspected
- Compiler flag misconfiguration

### Notify Compliance Team If:
- Audit trail compromised
- Regulatory evidence invalid
- Customer audit request affected
- SLA breach imminent

### Notify Customers If:
- Violation affects their production inference
- Audit evidence for their use case invalid
- Requires re-running affected jobs
- Data integrity cannot be guaranteed

---

## Notes

**Zero-Tolerance Policy:**
- **ANY** determinism violation is critical
- **NO** violations acceptable in production
- **IMMEDIATE** quarantine and investigation
- **MANDATORY** security team notification

**Known Causes (Historical):**
1. **Q15 Denominator Bug:** Using 32768 instead of 32767 (FIXED)
2. **Router Sorting:** Missing tie-breaker on score equality (FIXED)
3. **Non-Deterministic RNG:** Using thread_rng() instead of derive_seed() (FIXED)
4. **GPU Buffer Corruption:** Rare Metal command buffer issues on M1 (HARDWARE)
5. **Adapter File Tampering:** Manual file modification (SECURITY)

**Forensics Retention:**
- Preserve all violation forensics for 90 days
- Archive for 1 year minimum
- Include in incident postmortem
- Share with security team

**Performance Impact:**
- Determinism checks add 1-2ms per inference (acceptable)
- Replay verification adds 10-20ms (acceptable for audit)
- Hash computation overhead negligible

---

**Owner:** Security Team + SRE Team
**Last Incident:** [Link to most recent postmortem]
**Related Runbooks:** [WORKER_CRASH.md](./WORKER_CRASH.md)

**⚠️ REMEMBER: Determinism violations are NEVER acceptable. When in doubt, quarantine and escalate. ⚠️**
