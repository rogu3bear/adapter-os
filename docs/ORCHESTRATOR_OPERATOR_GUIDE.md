# Orchestrator Gates: Operator Guide

## Quick Reference

### What are Promotion Gates?

Promotion gates are automated quality checks that verify a candidate promotion (CP) is safe to deploy to production. The orchestrator runs 6 gates:

| Gate | Purpose | Dependency Check |
|------|---------|------------------|
| **Determinism** | Replay execution produces zero diff | `/srv/aos/bundles` |
| **Security** | Vulnerability and policy scanning | `cargo`, `deny.toml` |
| **Metallib** | GPU kernel hash validation | Metallib binary |
| **Telemetry** | Audit trail chain integrity | Telemetry bundles |
| **Metrics** | Quality thresholds (ARR/ECS5) | Database |
| **Performance** | Latency/throughput budgets | Database |
| **SBOM** | Software Bill of Materials | `target/sbom.spdx.json` |

## Pre-Flight Checklist

Before running promotion gates, verify these dependencies are available:

### 1. Paths on Disk

```bash
# Determinism: Replay bundles
ls -la /srv/aos/bundles/{CPID}_replay.ndjson

# Alternative: Check fallback paths
ls -la var/bundles/{CPID}_replay.ndjson
ls -la bundles/{CPID}_replay.ndjson
ls -la target/bundles/{CPID}_replay.ndjson

# Metallib: GPU kernel library
ls -la crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib

# SBOM: Software manifest
ls -la target/sbom.spdx.json
```

### 2. Configuration Files

```bash
# Security gate needs policy config
cat deny.toml
# Should have [vulnerabilities], [licenses], [advisories] sections
```

### 3. Tools

```bash
# Verify Rust toolchain
cargo --version
which cargo

# Verify optional security tools (not critical if missing)
cargo-audit --version || echo "cargo-audit not installed"
cargo-deny --version || echo "cargo-deny not installed"
```

### 4. Database

```bash
# Verify database accessibility
sqlite3 var/aos-cp.sqlite3 "SELECT count(*) FROM adapters;"
# Should return a number, not an error
```

## Running Gates

### Basic Invocation

```bash
# Run all gates for a specific CPID
./target/release/aosctl gates run --cpid my-cpid

# Continue even if gates fail (for debugging)
./target/release/aosctl gates run --cpid my-cpid --continue-on-error

# Skip dependency checks (dangerous - use only in isolated environments)
./target/release/aosctl gates run --cpid my-cpid --skip-dependency-checks

# Allow running with degraded dependencies (e.g., missing optional paths)
./target/release/aosctl gates run --cpid my-cpid --allow-degraded-mode

# Don't require telemetry bundles (for testing/staging)
./target/release/aosctl gates run --cpid my-cpid --no-require-telemetry
```

### Output Interpretation

#### Success Case
```
INFO  Running promotion gate gate=Determinism
INFO  All dependencies available gate=determinism
INFO  Replay bundle loaded successfully bundle_path=.../replay.ndjson event_count=10234
INFO  Gate check completed gate=Determinism status=passed

[... similar for other gates ...]

INFO  All 6 gates passed cpid=my-cpid result=ready-to-promote
```

#### Partial Degradation (Warning)
```
WARN  Some optional dependencies missing gate=security messages=["deny.toml not found; skipping cargo-deny"]
INFO  Skipping cargo-deny check
INFO  Gate check completed gate=Security status=passed
```

**Meaning:** Gate passed, but security checks were reduced. Safe to proceed with awareness that dependency-related checks were skipped.

#### Critical Dependency Missing (Failure)
```
ERROR Critical dependencies missing gate=determinism
ERROR No valid replay bundle found (checked primary: /srv/aos/bundles, fallbacks: ["var/bundles", ...])
INFO  Gate check completed gate=Determinism status=failed

ERROR Promotion blocked: critical dependencies missing
```

**Meaning:** Cannot proceed with promotion. Must resolve missing path.

## Common Issues & Solutions

### Issue 1: Replay Bundle Not Found

**Error:**
```
Replay bundle not found: {CPID}_replay.ndjson. Run determinism test first.
(checked primary: /srv/aos/bundles, fallbacks: ["var/bundles", "bundles", "target/bundles"])
```

**Solutions:**

a) **Run determinism test first:**
```bash
./target/release/aosctl determinism-test --cpid {CPID}
# Generates replay bundle in /srv/aos/bundles
```

b) **Place bundle in fallback location:**
```bash
# If /srv/aos/bundles not available (dev environment):
cp /path/to/replay.ndjson var/bundles/{CPID}_replay.ndjson
```

c) **Create test bundle (dev only):**
```bash
mkdir -p var/bundles
# ... populate with determinism test output
```

d) **Skip determinism (staging only):**
```bash
./target/release/aosctl gates run --cpid {CPID} \
  --skip-gates determinism
```

### Issue 2: Metallib Path Wrong

**Error:**
```
Metal kernel library not found: crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib
(and alternate paths not found)
```

**Solutions:**

a) **Check actual location:**
```bash
find . -name "aos_kernels.metallib" -o -name "*.metallib"
```

b) **Rebuild metal kernels:**
```bash
cd crates/adapteros-lora-kernel-mtl
cargo build --release
make metal  # If available
```

c) **Link from build output:**
```bash
ln -s target/shaders/aos_kernels.metallib \
      crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib
```

### Issue 3: Cargo Tools Missing

**Error:**
```
WARN cargo-audit not available, skipping vulnerability check
WARN cargo-deny not available, skipping dependency policy check
```

**If tools are required:**

a) **Install tools:**
```bash
cargo install cargo-audit
cargo install cargo-deny
```

b) **Verify install:**
```bash
cargo-audit --version
cargo-deny --version
which cargo-deny
```

c) **Check PATH:**
```bash
echo $PATH
# Verify ~/.cargo/bin is in PATH
```

### Issue 4: deny.toml Missing

**Error:**
```
WARN deny.toml not found - skipping cargo-deny check
```

**Solutions:**

a) **Create deny.toml at project root:**
```toml
[vulnerabilities]
deny = ["GHSA-*"]

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause"]
deny = ["GPL-2.0", "AGPL-3.0"]

[advisories]
# Latest advisory database
vulnerability = "deny"
unmaintained = "warn"
unsound = "deny"
```

b) **Skip cargo-deny (if acceptable):**
```bash
# Already skipped with warning if file missing
# Safe for staging environments
```

### Issue 5: SBOM Not Generated

**Error:**
```
SBOM not found: target/sbom.spdx.json. Run 'cargo xtask sbom' first.
```

**Solutions:**

a) **Generate SBOM:**
```bash
cargo xtask sbom
# Creates target/sbom.spdx.json and optional .sig
```

b) **Verify generation:**
```bash
cat target/sbom.spdx.json | jq '.packages | length'
# Should show package count > 0
```

c) **Skip SBOM (low severity):**
```bash
./target/release/aosctl gates run --cpid {CPID} \
  --skip-gates sbom
```

### Issue 6: Telemetry Bundles Missing

**Error:**
```
No telemetry bundles found for CPID: {CPID}. Checked: var/telemetry/{CPID}
```

**Solutions:**

a) **Generate telemetry bundles:**
```bash
./target/release/aosctl telemetry-bundle --cpid {CPID}
# Exports bundles to var/telemetry
```

b) **Check alternative location:**
```bash
ls -la var/telemetry/{CPID}/
ls -la .telemetry/{CPID}/
ls -la /var/aos/telemetry/{CPID}/
```

c) **Make telemetry optional (dev/staging):**
```bash
./target/release/aosctl gates run --cpid {CPID} \
  --no-require-telemetry
```

### Issue 7: Database Connection Failed

**Error:**
```
Failed to connect to database: var/aos-cp.sqlite3
Could not open file: ...
```

**Solutions:**

a) **Verify database exists:**
```bash
ls -la var/aos-cp.sqlite3
file var/aos-cp.sqlite3
```

b) **Initialize database:**
```bash
./target/release/aosctl db migrate
# Creates schema and tables
```

c) **Check database permissions:**
```bash
ls -la var/
# Database should be readable by your user
chmod 644 var/aos-cp.sqlite3
```

d) **Verify database integrity:**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
# Should output "ok"
```

### Issue 8: Metrics/Performance Thresholds Failed

**Error:**
```
Hallucination metrics failed: ARR 0.910 < 0.950
```

**This is expected** - quality metrics failed. Options:

a) **Improve model quality:**
   - Retrain adapter with better data
   - Adjust training hyperparameters
   - Add more training examples

b) **Review threshold settings:**
   - Check if thresholds are reasonable for your model
   - Consult quality/training team for adjustment

c) **Investigate audit results:**
```bash
sqlite3 var/aos-cp.sqlite3 \
  "SELECT result_json FROM audits WHERE cpid = '{CPID}' ORDER BY created_at DESC LIMIT 1;"
```

## Environment Variables

### Configuration via Environment

```bash
# Override database path
export AOS_DB_PATH=var/aos-cp-staging.sqlite3

# Override bundles path
export AOS_BUNDLES_PATH=/opt/aos/bundles

# Override manifests path
export AOS_MANIFESTS_PATH=./manifests-custom

# Skip dependency checks (dangerous)
export AOS_SKIP_DEPENDENCY_CHECKS=1

# Allow degraded mode
export AOS_ALLOW_DEGRADED_MODE=1

# Don't require telemetry
export AOS_NO_REQUIRE_TELEMETRY=1

# Run gates
./target/release/aosctl gates run --cpid my-cpid
```

## Report Interpretation

### JSON Report

After running gates, check the report:

```bash
./target/release/aosctl gates run --cpid my-cpid --output json > gates-report.json
cat gates-report.json | jq
```

**Key fields:**
```json
{
  "cpid": "my-cpid",
  "timestamp": "2025-11-21T10:30:00Z",
  "dependency_checks": [
    {
      "gate_id": "determinism",
      "all_available": true,
      "degradation_level": 0,
      "messages": []  // Empty = no issues
    }
  ],
  "gates": {
    "Determinism": {
      "passed": true,
      "message": "Gate passed",
      "evidence": null
    }
  },
  "all_passed": true  // Ready for promotion
}
```

### Markdown Report

```bash
./target/release/aosctl gates run --cpid my-cpid --output markdown > gates-report.md
cat gates-report.md
```

Check the Dependency Status table first:
- ✅ All Available = no issues
- ⚠️ Some Missing = check Degradation column
- Partial degradation = optional dependencies missing (usually OK)
- Critical degradation = required paths missing (blocks promotion)

## Best Practices

### 1. Always Check Dependencies First

Before running gates:
```bash
./target/release/aosctl gates check-deps --all
```

### 2. Run Gates on Clean System

```bash
# Fresh database
rm var/aos-cp.sqlite3
./target/release/aosctl db migrate

# Regenerate all artifacts
cargo build --release
cargo xtask sbom
./target/release/aosctl determinism-test --cpid my-cpid
```

### 3. Use Staging Environment First

```bash
# Dev environment (skip some dependencies)
./target/release/aosctl gates run --cpid my-cpid \
  --allow-degraded-mode \
  --no-require-telemetry

# Staging (most dependencies)
./target/release/aosctl gates run --cpid my-cpid

# Production (all dependencies required)
./target/release/aosctl gates run --cpid my-cpid \
  --all-passed-required
```

### 4. Log and Preserve Reports

```bash
# Archive gate reports
mkdir -p reports/{CPID}
./target/release/aosctl gates run --cpid {CPID} \
  --output json > reports/{CPID}/gates-$(date +%Y%m%d-%H%M%S).json
```

### 5. Investigate Failures Thoroughly

```bash
# Get full database audit details
sqlite3 var/aos-cp.sqlite3 \
  "SELECT id, cpid, status, result_json FROM audits WHERE cpid = '{CPID}' ORDER BY created_at DESC LIMIT 1;" \
  | jq .result_json

# Check logs for warnings
./target/release/aosctl gates run --cpid {CPID} 2>&1 | grep -i "warn\|error\|fatal"
```

## Monitoring

### Health Check

```bash
# Regular dependency status
./target/release/aosctl gates check-deps --all

# Each gate individually
./target/release/aosctl gates check-deps --gate determinism
./target/release/aosctl gates check-deps --gate security
# ... etc
```

### Alerting

Monitor these conditions:

1. **Critical dependency missing**: Blocks all promotions
2. **Metadata quality below threshold**: Review audit results
3. **Telemetry gaps**: Check determinism test completeness
4. **Metallib mismatch**: Kernel rebuild needed

## Support & Debugging

### Debug Mode

```bash
# Verbose logging
RUST_LOG=debug ./target/release/aosctl gates run --cpid my-cpid 2>&1 | tee debug.log

# Filter to specific gate
RUST_LOG=debug ./target/release/aosctl gates run --cpid my-cpid 2>&1 | grep determinism
```

### Collect Diagnostics

```bash
# Gather full environment info
./target/release/aosctl diagnose --cpid my-cpid > diagnostics.json

# Check all dependency status
./target/release/aosctl gates check-deps --all --verbose > deps-status.json

# Database schema check
sqlite3 var/aos-cp.sqlite3 ".schema" > schema.sql
```

### Contact Development Team

When reporting issues, include:
1. CPID being tested
2. Full gates report (JSON)
3. Dependency check output
4. Environment details (OS, Rust version)
5. Relevant logs (RUST_LOG=debug)

## Quick Command Reference

```bash
# Run all gates
aosctl gates run --cpid {CPID}

# Check dependencies only
aosctl gates check-deps --all

# Run specific gate
aosctl gates run --cpid {CPID} --gate determinism

# Skip gates
aosctl gates run --cpid {CPID} --skip-gates security,sbom

# Output formats
aosctl gates run --cpid {CPID} --output json
aosctl gates run --cpid {CPID} --output markdown

# Debug/testing modes
aosctl gates run --cpid {CPID} --continue-on-error
aosctl gates run --cpid {CPID} --allow-degraded-mode
aosctl gates run --cpid {CPID} --no-require-telemetry
aosctl gates run --cpid {CPID} --skip-dependency-checks

# View audit results
sqlite3 var/aos-cp.sqlite3 "SELECT cpid, status, result_json FROM audits WHERE cpid = '{CPID}' LIMIT 1" | jq .result_json
```

## References

- [Orchestrator Architecture](./ORCHESTRATOR_DEPENDENCY_CHECKS.md)
- [Gate Implementation](../crates/adapteros-orchestrator/src/gates/)
- [Configuration](../crates/adapteros-orchestrator/src/lib.rs)
