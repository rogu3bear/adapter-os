# PR-001: Periodic Audit Chain Verification Job

## Summary

Add scheduled and on-demand audit chain verification with alerting to detect tampered or divergent policy audit and evidence envelope chains.

## Problem Statement

Currently, chain verification only occurs on-demand when explicitly called. There is no automated mechanism to detect:
- Tampered `entry_hash` values in policy audit decisions
- Broken `previous_hash` linkage in audit chains
- `previous_root` mismatches in evidence envelope chains
- Chain sequence gaps from deleted entries

A compromised disk or control plane could silently corrupt chain integrity without detection until manual audit.

## Solution

1. Add CLI command `aosctl audit verify-chains` for on-demand verification
2. Add server-side periodic verification job (configurable interval, default hourly)
3. Emit `audit_chain_divergence_event()` on any detected divergence
4. Expose Prometheus metrics for monitoring and alerting

---

## Implementation Details

### File Changes

#### 1. `crates/adapteros-db/src/policy_audit.rs`

**Add function** `verify_all_tenant_chains`:

```rust
/// Verify policy audit chains for all tenants.
///
/// Returns a map of tenant_id -> verification result.
pub async fn verify_all_tenant_chains(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, ChainVerificationResult>> {
    let tenants: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT tenant_id FROM policy_audit_decisions ORDER BY tenant_id"
    )
    .fetch_all(pool)
    .await?;

    let mut results = BTreeMap::new();
    for tenant_id in tenants {
        let result = verify_policy_audit_chain(pool, &tenant_id).await?;
        if result.divergence_detected {
            // Emit observability event
            let event = audit_chain_divergence_event(
                format!("Entry hash mismatch at sequence {}", result.first_invalid_sequence.unwrap_or(0)),
                result.first_invalid_sequence,
                Some(tenant_id.clone()),
                None,
            );
            emit_observability_event(&event);
        }
        results.insert(tenant_id, result);
    }
    Ok(results)
}
```

**Add to `ChainVerificationResult`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    pub is_valid: bool,
    pub entries_checked: i64,
    pub divergence_detected: bool,
    pub first_invalid_sequence: Option<i64>,
    pub error_message: Option<String>,
    // NEW FIELDS
    pub tenant_id: String,
    pub verified_at: DateTime<Utc>,
    pub duration_ms: u64,
}
```

#### 2. `crates/adapteros-db/src/evidence_envelopes.rs`

**Add function** `verify_all_evidence_chains`:

```rust
/// Verify evidence envelope chains for all tenants and scopes.
pub async fn verify_all_evidence_chains(
    pool: &SqlitePool,
) -> Result<Vec<EvidenceChainVerificationResult>> {
    let scopes = [EvidenceScope::Telemetry, EvidenceScope::Policy, EvidenceScope::Inference];

    let tenants: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT tenant_id FROM evidence_envelopes ORDER BY tenant_id"
    )
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for tenant_id in &tenants {
        for scope in &scopes {
            let result = verify_evidence_chain(pool, tenant_id, *scope).await?;
            if result.divergence_detected {
                let event = audit_chain_divergence_event(
                    format!("Evidence chain divergence: {}", result.error_message.as_deref().unwrap_or("unknown")),
                    result.first_invalid_index.map(|i| i as i64),
                    Some(tenant_id.clone()),
                    None,
                );
                emit_observability_event(&event);
            }
            results.push(EvidenceChainVerificationResult {
                tenant_id: tenant_id.clone(),
                scope: *scope,
                ..result
            });
        }
    }
    Ok(results)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceChainVerificationResult {
    pub tenant_id: String,
    pub scope: EvidenceScope,
    pub is_valid: bool,
    pub envelopes_checked: usize,
    pub divergence_detected: bool,
    pub first_invalid_index: Option<usize>,
    pub error_message: Option<String>,
    pub verified_at: DateTime<Utc>,
    pub duration_ms: u64,
}
```

#### 3. `crates/adapteros-cli/src/commands/audit.rs` (NEW FILE)

```rust
//! Audit chain verification commands.

use adapteros_db::{
    evidence_envelopes::verify_all_evidence_chains,
    policy_audit::verify_all_tenant_chains,
};
use anyhow::Result;
use clap::Args;

use crate::output::OutputWriter;

#[derive(Debug, Args)]
pub struct AuditArgs {
    #[command(subcommand)]
    pub command: AuditCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum AuditCommand {
    /// Verify all audit chains for integrity
    VerifyChains(VerifyChainsArgs),
}

#[derive(Debug, Args)]
pub struct VerifyChainsArgs {
    /// Only verify policy audit chains
    #[arg(long)]
    pub policy_only: bool,

    /// Only verify evidence envelope chains
    #[arg(long)]
    pub evidence_only: bool,

    /// Filter to specific tenant
    #[arg(long)]
    pub tenant_id: Option<String>,

    /// Fail command if any divergence detected
    #[arg(long, default_value = "true")]
    pub fail_on_divergence: bool,
}

pub async fn run(args: AuditArgs, output: &OutputWriter) -> Result<()> {
    match args.command {
        AuditCommand::VerifyChains(verify_args) => run_verify_chains(verify_args, output).await,
    }
}

async fn run_verify_chains(args: VerifyChainsArgs, output: &OutputWriter) -> Result<()> {
    let pool = adapteros_db::connect_default().await?;
    let mut any_divergence = false;

    if !args.evidence_only {
        output.section("Policy Audit Chain Verification");
        let results = verify_all_tenant_chains(&pool).await?;

        for (tenant_id, result) in &results {
            if let Some(ref filter) = args.tenant_id {
                if tenant_id != filter {
                    continue;
                }
            }

            if result.divergence_detected {
                any_divergence = true;
                output.error(format!(
                    "DIVERGENCE: tenant={} sequence={} entries_checked={}",
                    tenant_id,
                    result.first_invalid_sequence.unwrap_or(0),
                    result.entries_checked
                ));
            } else {
                output.success(format!(
                    "OK: tenant={} entries_checked={}",
                    tenant_id, result.entries_checked
                ));
            }
        }

        if output.is_json() {
            output.json(&results)?;
        }
    }

    if !args.policy_only {
        output.section("Evidence Envelope Chain Verification");
        let results = verify_all_evidence_chains(&pool).await?;

        for result in &results {
            if let Some(ref filter) = args.tenant_id {
                if &result.tenant_id != filter {
                    continue;
                }
            }

            if result.divergence_detected {
                any_divergence = true;
                output.error(format!(
                    "DIVERGENCE: tenant={} scope={:?} index={} envelopes_checked={}",
                    result.tenant_id,
                    result.scope,
                    result.first_invalid_index.unwrap_or(0),
                    result.envelopes_checked
                ));
            } else {
                output.success(format!(
                    "OK: tenant={} scope={:?} envelopes_checked={}",
                    result.tenant_id, result.scope, result.envelopes_checked
                ));
            }
        }

        if output.is_json() {
            output.json(&results)?;
        }
    }

    if any_divergence && args.fail_on_divergence {
        anyhow::bail!("Audit chain divergence detected");
    }

    Ok(())
}
```

#### 4. `crates/adapteros-cli/src/commands/mod.rs`

**Add module**:
```rust
pub mod audit;
```

#### 5. `crates/adapteros-cli/src/app.rs`

**Add subcommand**:
```rust
#[derive(Debug, Subcommand)]
pub enum Commands {
    // ... existing commands ...

    /// Audit chain verification and integrity checks
    Audit(audit::AuditArgs),
}

// In run() match:
Commands::Audit(args) => audit::run(args, &output).await,
```

#### 6. `crates/adapteros-server/src/boot/metrics.rs`

**Add periodic verification job**:

```rust
use adapteros_db::{
    evidence_envelopes::verify_all_evidence_chains,
    policy_audit::verify_all_tenant_chains,
};
use std::time::Duration;
use tokio::time::interval;

/// Start periodic audit chain verification.
///
/// Runs verification at the configured interval and emits metrics/alerts on divergence.
pub fn start_audit_verification_job(
    pool: SqlitePool,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_secs));

        loop {
            ticker.tick().await;

            tracing::info!("Running periodic audit chain verification");

            // Verify policy audit chains
            match verify_all_tenant_chains(&pool).await {
                Ok(results) => {
                    let divergent_count = results.values().filter(|r| r.divergence_detected).count();
                    metrics::gauge!("audit_policy_chains_verified").set(results.len() as f64);
                    metrics::gauge!("audit_policy_chains_divergent").set(divergent_count as f64);

                    if divergent_count > 0 {
                        tracing::error!(
                            divergent_count,
                            "Policy audit chain divergence detected in periodic verification"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to verify policy audit chains");
                    metrics::counter!("audit_verification_errors_total", "type" => "policy").increment(1);
                }
            }

            // Verify evidence envelope chains
            match verify_all_evidence_chains(&pool).await {
                Ok(results) => {
                    let divergent_count = results.iter().filter(|r| r.divergence_detected).count();
                    metrics::gauge!("audit_evidence_chains_verified").set(results.len() as f64);
                    metrics::gauge!("audit_evidence_chains_divergent").set(divergent_count as f64);

                    if divergent_count > 0 {
                        tracing::error!(
                            divergent_count,
                            "Evidence envelope chain divergence detected in periodic verification"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to verify evidence envelope chains");
                    metrics::counter!("audit_verification_errors_total", "type" => "evidence").increment(1);
                }
            }
        }
    })
}
```

#### 7. `configs/cp.toml`

**Add configuration**:
```toml
[audit]
# Enable periodic audit chain verification
periodic_verification_enabled = true
# Verification interval in seconds (default: 3600 = 1 hour)
verification_interval_secs = 3600
# Fail server startup if initial verification fails
fail_on_startup_divergence = true
```

---

## Acceptance Criteria

- [ ] `aosctl audit verify-chains` command exists and runs successfully
- [ ] Command verifies all policy audit chains with per-tenant results
- [ ] Command verifies all evidence envelope chains with per-scope results
- [ ] `--tenant-id` filter limits verification to specific tenant
- [ ] `--policy-only` and `--evidence-only` flags work correctly
- [ ] JSON output mode (`--json`) produces machine-readable results
- [ ] Divergence emits `audit_chain_divergence_event()` with chain_sequence
- [ ] Prometheus metrics exposed: `audit_*_chains_verified`, `audit_*_chains_divergent`
- [ ] Server runs periodic verification at configurable interval
- [ ] Exit code 1 when `--fail-on-divergence` and divergence detected

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-db/tests/audit_verification_tests.rs`

```rust
#[tokio::test]
async fn test_verify_all_tenant_chains_clean() {
    let pool = setup_test_db().await;

    // Insert valid chain entries for 2 tenants
    insert_valid_policy_chain(&pool, "tenant-a", 5).await;
    insert_valid_policy_chain(&pool, "tenant-b", 3).await;

    let results = verify_all_tenant_chains(&pool).await.unwrap();

    assert_eq!(results.len(), 2);
    assert!(!results["tenant-a"].divergence_detected);
    assert!(!results["tenant-b"].divergence_detected);
    assert_eq!(results["tenant-a"].entries_checked, 5);
}

#[tokio::test]
async fn test_verify_all_tenant_chains_corrupted() {
    let pool = setup_test_db().await;

    // Insert valid chain, then corrupt entry 3
    insert_valid_policy_chain(&pool, "tenant-a", 5).await;
    corrupt_policy_audit_entry(&pool, "tenant-a", 3).await;

    let results = verify_all_tenant_chains(&pool).await.unwrap();

    assert!(results["tenant-a"].divergence_detected);
    assert_eq!(results["tenant-a"].first_invalid_sequence, Some(3));
}

#[tokio::test]
async fn test_verify_evidence_chains_scope_isolation() {
    let pool = setup_test_db().await;

    // Valid telemetry chain, corrupted inference chain
    insert_valid_evidence_chain(&pool, "tenant-a", EvidenceScope::Telemetry, 3).await;
    insert_valid_evidence_chain(&pool, "tenant-a", EvidenceScope::Inference, 3).await;
    corrupt_evidence_envelope(&pool, "tenant-a", EvidenceScope::Inference, 2).await;

    let results = verify_all_evidence_chains(&pool).await.unwrap();

    let telemetry = results.iter().find(|r| r.scope == EvidenceScope::Telemetry).unwrap();
    let inference = results.iter().find(|r| r.scope == EvidenceScope::Inference).unwrap();

    assert!(!telemetry.divergence_detected);
    assert!(inference.divergence_detected);
}
```

### Integration Tests

**File**: `tests/audit_chain_verification_integration.rs`

```rust
#[tokio::test]
async fn test_cli_audit_verify_chains_json_output() {
    let output = run_cli(&["audit", "verify-chains", "--json"]).await;

    assert!(output.status.success());
    let json: Value = serde_json::from_str(&output.stdout).unwrap();
    assert!(json.is_object());
}

#[tokio::test]
async fn test_cli_fails_on_divergence() {
    // Setup corrupted chain
    setup_corrupted_chain().await;

    let output = run_cli(&["audit", "verify-chains", "--fail-on-divergence"]).await;

    assert!(!output.status.success());
    assert!(output.stderr.contains("divergence detected"));
}
```

### E2E Tests

**File**: `tests/e2e/audit_periodic_verification.rs`

```rust
#[tokio::test]
async fn test_periodic_verification_emits_metrics() {
    let server = start_test_server_with_config(r#"
        [audit]
        periodic_verification_enabled = true
        verification_interval_secs = 1
    "#).await;

    // Wait for at least one verification cycle
    tokio::time::sleep(Duration::from_secs(2)).await;

    let metrics = fetch_metrics(&server).await;
    assert!(metrics.contains("audit_policy_chains_verified"));
    assert!(metrics.contains("audit_evidence_chains_verified"));
}
```

---

## Rollout Plan

1. **Week 1**: Merge PR, deploy to staging with `periodic_verification_enabled = false`
2. **Week 2**: Enable periodic verification on staging, monitor for false positives
3. **Week 3**: Enable on production with 4-hour interval
4. **Week 4**: Reduce interval to 1-hour after confirming stability

---

## Metrics & Alerting

### Prometheus Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `audit_policy_chains_verified` | Gauge | - | Number of policy chains verified in last run |
| `audit_policy_chains_divergent` | Gauge | - | Number of divergent policy chains |
| `audit_evidence_chains_verified` | Gauge | - | Number of evidence chains verified |
| `audit_evidence_chains_divergent` | Gauge | - | Number of divergent evidence chains |
| `audit_verification_errors_total` | Counter | type | Verification errors by type |
| `audit_divergence_total` | Counter | tenant_id, scope | Total divergences detected |

### Alert Rules

```yaml
groups:
  - name: audit_chain_integrity
    rules:
      - alert: AuditChainDivergence
        expr: audit_policy_chains_divergent > 0 or audit_evidence_chains_divergent > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Audit chain divergence detected"
          description: "{{ $value }} chains have diverged. Immediate investigation required."

      - alert: AuditVerificationFailure
        expr: increase(audit_verification_errors_total[1h]) > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Audit verification errors occurring"
```
