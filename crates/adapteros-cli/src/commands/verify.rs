//! Verification commands

use crate::http_client;
use crate::output::OutputWriter;
use adapteros_artifacts::bundle;
use adapteros_core::B3Hash;
use adapteros_platform::common::PlatformUtils;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Subcommand)]
pub enum VerifyCommand {
    /// Verify artifact bundle signature and hashes
    #[command(after_help = r#"Examples:
  # Verify artifact bundle
  aosctl verify bundle artifacts/adapters.zip
"#)]
    Bundle {
        /// Bundle path
        bundle: PathBuf,
    },

    /// Verify a packaged adapter directory
    #[command(after_help = r#"Examples:
  # Verify packaged adapter
  aosctl verify adapter --adapters-root ./adapters --adapter-id demo_adapter

  # JSON output
  aosctl verify adapter --adapters-root ./adapters --adapter-id demo_adapter --json
"#)]
    Adapter {
        /// Adapters root directory
        #[arg(long, default_value = "./adapters")]
        adapters_root: PathBuf,

        /// Adapter ID to verify
        #[arg(long)]
        adapter_id: String,
    },

    /// Verify adapter deliverables (A–F)
    #[command(after_help = r#"Examples:
  # Run full adapter verification
  aosctl verify adapters

  # JSON summary for CI
  aosctl --json verify adapters
"#)]
    Adapters,

    /// Verify determinism loop (dev-only; delegates to cargo xtask)
    #[command(after_help = r#"Examples:
  # Generate determinism report via xtask
  aosctl verify determinism-loop

  # In CI, prefer this over calling `cargo xtask determinism-report` directly
"#)]
    DeterminismLoop,

    /// Verify telemetry bundle chain
    #[command(after_help = r#"Examples:
  # Verify telemetry bundles
  aosctl verify telemetry --bundle-dir ./var/telemetry

  # JSON output
  aosctl verify telemetry --bundle-dir ./var/telemetry --json
"#)]
    Telemetry {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
    },

    /// Verify a fused CoreML package hash against expected metadata
    #[command(after_help = r#"Examples:
  # Verify CoreML package against metadata file
  aosctl verify coreml --package ./var/models/qwen-coreml --metadata ./var/models/qwen-coreml/adapteros_coreml_fusion.json

  # Verify CoreML package against explicit hash
  aosctl verify coreml --package ./var/models/qwen-coreml --expected-hash <HEX>
"#)]
    Coreml {
        /// Path to CoreML package directory or Manifest.json
        #[arg(long)]
        package: PathBuf,

        /// Expected fused manifest hash (hex)
        #[arg(long)]
        expected_hash: Option<String>,

        /// Path to fusion metadata JSON (emitted by export)
        #[arg(long)]
        metadata: Option<PathBuf>,
    },

    /// Verify cross-host federation signatures
    #[command(after_help = r#"Examples:
  # Verify federation signatures
  aosctl verify federation --bundle-dir ./var/telemetry

  # Custom database path
  aosctl verify federation --bundle-dir ./var/telemetry --database ./var/cp.db
"#)]
    Federation {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,

        /// Database path
        #[arg(long, default_value = "./var/cp.db")]
        database: PathBuf,
    },

    /// Verify audit chain integrity (policy audit + evidence envelopes)
    #[command(after_help = r#"Examples:
  # Verify all audit chains
  aosctl verify chains

  # Verify only policy audit chains
  aosctl verify chains --policy-only

  # Verify only evidence envelope chains
  aosctl verify chains --evidence-only

  # Filter to specific tenant
  aosctl verify chains --tenant-id my-tenant

  # Exit with error if any divergence detected (for CI)
  aosctl verify chains --fail-on-divergence

  # JSON output
  aosctl verify chains --json
"#)]
    Chains {
        /// Only verify policy audit chains
        #[arg(long)]
        policy_only: bool,

        /// Only verify evidence envelope chains
        #[arg(long)]
        evidence_only: bool,

        /// Filter to specific tenant
        #[arg(long)]
        tenant_id: Option<String>,

        /// Fail command if any divergence detected (exit code 1)
        #[arg(long, default_value = "false")]
        fail_on_divergence: bool,
    },
}

/// Run bundle verification (public entry point for Commands::Verify)
pub async fn run(bundle_path: &Path, output: &OutputWriter) -> Result<()> {
    run_bundle(bundle_path, output).await
}

/// Handle verify commands
pub async fn handle_verify_command(cmd: VerifyCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        VerifyCommand::Bundle { bundle } => run_bundle(&bundle, output).await,
        VerifyCommand::Adapter {
            adapters_root,
            adapter_id,
        } => {
            use crate::commands::verify_adapter;
            verify_adapter::run(adapters_root, adapter_id, output).await
        }
        VerifyCommand::Adapters => {
            use crate::commands::verify_adapters;
            verify_adapters::run(output).await?;
            Ok(())
        }
        VerifyCommand::DeterminismLoop => {
            use crate::commands::verify_determinism_loop;
            verify_determinism_loop::run(output).await?;
            Ok(())
        }
        VerifyCommand::Telemetry { bundle_dir } => {
            use crate::commands::telemetry;
            telemetry::verify_telemetry_chain(&bundle_dir, output)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
        VerifyCommand::Coreml {
            package,
            expected_hash,
            metadata,
        } => verify_coreml_package(&package, expected_hash, metadata, output),
        VerifyCommand::Federation {
            bundle_dir,
            database,
        } => {
            use crate::commands::federation;
            federation::verify_federation(&bundle_dir, &database, output).await
        }
        VerifyCommand::Chains {
            policy_only,
            evidence_only,
            tenant_id,
            fail_on_divergence,
        } => {
            verify_audit_chains(
                policy_only,
                evidence_only,
                tenant_id,
                fail_on_divergence,
                output,
            )
            .await
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceVerifyRequest {
    trace_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceDigestDiff {
    field: String,
    expected_hex: String,
    computed_hex: String,
    matches: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceVerificationReport {
    trace_id: String,
    tenant_id: Option<String>,
    source: String,
    pass: bool,
    verified_at: String,
    reasons: Vec<String>,
    mismatched_token: Option<u32>,
    context_digest: TraceDigestDiff,
    run_head_hash: TraceDigestDiff,
    output_digest: TraceDigestDiff,
    receipt_digest: TraceDigestDiff,
    signature_checked: bool,
    signature_valid: Option<bool>,
}

/// Verify a trace receipt via the replay verification endpoint
pub async fn verify_trace_receipt(
    trace_id: String,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::builder().build()?;
    let url = format!("{}/v1/replay/verify/trace", base_url.trim_end_matches('/'));

    let response = http_client::send_with_refresh_from_store(&client, |c, auth| {
        c.post(&url)
            .bearer_auth(&auth.token)
            .json(&TraceVerifyRequest {
                trace_id: trace_id.clone(),
            })
    })
    .await
    .context("Failed to call trace verification endpoint")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(format!(
            "Trace verification failed: {} {}",
            status, body
        )));
    }

    let report: TraceVerificationReport = response
        .json()
        .await
        .context("Failed to parse trace verification response")?;

    if output.mode().is_json() {
        output.print_json(&report)?;
        return Ok(());
    }

    output.section("Trace Receipt Verification");
    output.kv("Trace", &report.trace_id);
    output.kv("Tenant", report.tenant_id.as_deref().unwrap_or("-"));
    output.kv(
        "Status",
        if report.pass {
            "PASS"
        } else {
            "FAIL (see reasons)"
        },
    );
    output.kv("Verified at", &report.verified_at);

    for diff in [
        &report.context_digest,
        &report.run_head_hash,
        &report.output_digest,
        &report.receipt_digest,
    ] {
        let status = if diff.matches { "match" } else { "MISMATCH" };
        output.result(format!(
            "{}: {} (expected {}) [{}]",
            diff.field, diff.computed_hex, diff.expected_hex, status
        ));
    }

    if let Some(token) = report.mismatched_token {
        output.warning(format!("First mismatched token: {}", token));
    }

    if !report.reasons.is_empty() {
        output.warning(format!("Reasons: {}", report.reasons.join(", ")));
    }

    if report.signature_checked {
        if let Some(valid) = report.signature_valid {
            output.kv("Signature", if valid { "valid" } else { "invalid" });
        } else {
            output.kv("Signature", "checked");
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct VerificationResult {
    signature_verified: bool,
    sbom_complete: bool,
    build_signature_verified: bool,
    artifacts_verified: usize,
    artifacts_total: usize,
    bundle_hash: String,
}

/// Verify a bundle (internal implementation)
async fn run_bundle(bundle_path: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!("Verifying bundle: {}", bundle_path.display()));

    // Create temporary directory for extraction
    let temp_root = PlatformUtils::temp_dir();
    fs::create_dir_all(&temp_root).with_context(|| {
        format!(
            "Failed to create adapterOS temp directory {}",
            temp_root.display()
        )
    })?;
    let temp_dir = tempfile::Builder::new()
        .prefix("adapteros-verify-")
        .tempdir_in(&temp_root)
        .context("Failed to create temporary directory")?;

    output.progress("Extracting bundle");

    // Extract bundle
    bundle::extract_bundle(bundle_path, temp_dir.path()).context("Failed to extract bundle")?;

    output.progress_done(true);

    // Load signature file
    let signature_path = temp_dir.path().join("signature.sig");
    if !signature_path.exists() {
        return Err(anyhow::anyhow!("Signature file not found in bundle"));
    }

    let signature_data = fs::read(&signature_path).context("Failed to read signature file")?;

    output.success("Signature file found");

    // Load public key from metadata (stored in bundle for verification)
    let pubkey_path = temp_dir.path().join("public_key.hex");
    let public_key_hex = if pubkey_path.exists() {
        fs::read_to_string(&pubkey_path).context("Failed to read public key")?
    } else {
        output.warning("No public key found in bundle, skipping signature verification");
        output.verbose("(Public key should be in public_key.hex)");
        return Ok(());
    };

    // Decode hex-encoded public key
    let public_key_bytes =
        hex::decode(public_key_hex.trim()).context("Failed to decode public key hex")?;
    if public_key_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "Invalid public key length: expected 32 bytes, got {}",
            public_key_bytes.len()
        ));
    }
    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key =
        adapteros_crypto::PublicKey::from_bytes(&pk_array).context("Failed to parse public key")?;

    // Decode hex-encoded signature
    let signature_hex =
        String::from_utf8(signature_data).context("Failed to parse signature as UTF-8")?;
    let signature_bytes =
        hex::decode(signature_hex.trim()).context("Failed to decode signature hex")?;
    if signature_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature_bytes.len()
        ));
    }
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature =
        adapteros_crypto::Signature::from_bytes(&sig_array).context("Failed to parse signature")?;

    // Load SBOM file
    let sbom_path = temp_dir.path().join("sbom.json");
    if !sbom_path.exists() {
        return Err(anyhow::anyhow!("SBOM file not found in bundle"));
    }

    let sbom_content = fs::read_to_string(&sbom_path).context("Failed to read SBOM file")?;

    // Verify signature against SBOM content
    adapteros_crypto::verify_signature(&public_key, sbom_content.as_bytes(), &signature)
        .context("Signature verification failed")?;

    output.success("Signature verified successfully");

    let sbom: serde_json::Value =
        serde_json::from_str(&sbom_content).context("Failed to parse SBOM JSON")?;

    output.success("SBOM file found");

    // Validate build section exists and captures provenance
    let build_section = sbom["build"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("SBOM missing build section"))?;
    let build_id = build_section
        .get("build_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("SBOM missing build.build_id"))?;
    let git_sha = build_section
        .get("git_sha")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Verify SBOM completeness
    let artifacts = sbom["artifacts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("SBOM missing artifacts array"))?;

    output.info(format!("SBOM lists {} artifacts", artifacts.len()));

    // Verify hashes for all artifacts
    let mut verified_count = 0;
    for artifact in artifacts {
        let path = artifact["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing path"))?;

        let expected_hash = artifact["hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing hash"))?;
        let expected_hash = expected_hash.trim_start_matches("b3:");

        let artifact_path = temp_dir.path().join(path);
        if !artifact_path.exists() {
            return Err(anyhow::anyhow!("Artifact not found: {}", path));
        }

        // Compute hash
        let content = fs::read(&artifact_path)
            .with_context(|| format!("Failed to read artifact: {}", path))?;

        let computed_hash = B3Hash::hash(&content);

        if computed_hash.to_hex() != expected_hash {
            return Err(anyhow::anyhow!(
                "Hash mismatch for {}: expected {}, got {}",
                path,
                expected_hash,
                computed_hash.to_hex()
            ));
        }

        verified_count += 1;
        output.verbose(format!("Verified: {}", path));
    }

    output.success(format!("All {} artifact hashes verified", verified_count));

    // Validate build provenance bundle
    let provenance_path = temp_dir.path().join("build_provenance.json");
    if !provenance_path.exists() {
        return Err(anyhow::anyhow!(
            "Build provenance file not found (expected build_provenance.json)"
        ));
    }
    let provenance_content =
        fs::read_to_string(&provenance_path).context("Failed to read build_provenance.json")?;
    let provenance: serde_json::Value = serde_json::from_str(&provenance_content)
        .context("Failed to parse build_provenance.json")?;
    let provenance_build_id = provenance["build_id"].as_str().unwrap_or_default();
    if provenance_build_id != build_id {
        return Err(anyhow::anyhow!(
            "Build provenance build_id ({}) does not match SBOM build_id ({})",
            provenance_build_id,
            build_id
        ));
    }

    let provenance_sig_path = temp_dir.path().join("build_provenance.sig");
    if !provenance_sig_path.exists() {
        return Err(anyhow::anyhow!(
            "Build provenance signature not found (expected build_provenance.sig)"
        ));
    }

    let provenance_sig_hex =
        fs::read_to_string(&provenance_sig_path).context("Failed to read build_provenance.sig")?;
    let provenance_sig_bytes = hex::decode(provenance_sig_hex.trim())
        .context("Failed to decode build provenance signature hex")?;
    if provenance_sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid build provenance signature length: expected 64 bytes, got {}",
            provenance_sig_bytes.len()
        ));
    }
    let mut provenance_sig_array = [0u8; 64];
    provenance_sig_array.copy_from_slice(&provenance_sig_bytes);
    let provenance_signature = adapteros_crypto::Signature::from_bytes(&provenance_sig_array)
        .context("Failed to parse build provenance signature")?;

    adapteros_crypto::verify_signature(
        &public_key,
        provenance_content.as_bytes(),
        &provenance_signature,
    )
    .context("Build provenance signature verification failed")?;

    let sbom_hash = B3Hash::hash(sbom_content.as_bytes()).to_hex();
    if let Some(expected_sbom_hash) = provenance["sbom_hash"].as_str() {
        if expected_sbom_hash != sbom_hash {
            return Err(anyhow::anyhow!(
                "SBOM hash mismatch in provenance: expected {}, got {}",
                expected_sbom_hash,
                sbom_hash
            ));
        }
    } else {
        return Err(anyhow::anyhow!("Build provenance missing sbom_hash field"));
    }

    output.success("Build provenance signature verified");

    // Compute bundle hash for determinism verification
    let bundle_content =
        fs::read(bundle_path).context("Failed to read bundle for hash computation")?;
    let bundle_hash = adapteros_core::B3Hash::hash(&bundle_content);

    output.blank();
    output.success("Bundle verification complete");
    output.kv("Bundle hash (deterministic)", &bundle_hash.to_string());
    output.kv("Signature", "verified");
    output.kv("Build ID", build_id);
    output.kv("Git SHA", git_sha);
    output.kv("SBOM", "complete");
    output.kv(
        "Artifacts",
        &format!("{}/{} verified", verified_count, artifacts.len()),
    );

    if output.is_json() {
        let result = VerificationResult {
            signature_verified: true,
            sbom_complete: true,
            build_signature_verified: true,
            artifacts_verified: verified_count,
            artifacts_total: artifacts.len(),
            bundle_hash: bundle_hash.to_string(),
        };
        output.json(&result)?;
    }

    Ok(())
}

/// Verify CoreML fused package hash against expected metadata or explicit hash.
fn verify_coreml_package(
    package: &Path,
    expected_hash: Option<String>,
    metadata: Option<PathBuf>,
    output: &OutputWriter,
) -> Result<()> {
    let manifest_path = manifest_path_for_package(package)?;
    let actual =
        hash_manifest(&manifest_path).context("Failed to hash CoreML manifest (Manifest.json)")?;
    let expected = if let Some(meta_path) = metadata {
        Some(load_fusion_metadata(&meta_path)?)
    } else if let Some(hash_hex) = expected_hash {
        Some(
            B3Hash::from_hex(&hash_hex)
                .map_err(|e| anyhow::anyhow!("Invalid expected hash: {}", e))?,
        )
    } else {
        None
    };

    if output.is_json() {
        #[derive(Serialize)]
        struct CoremlVerifyJson<'a> {
            manifest_path: &'a str,
            actual_hash: String,
            expected_hash: Option<String>,
            status: &'a str,
        }
        let status = if let Some(ref exp) = expected {
            if exp == &actual {
                "match"
            } else {
                "mismatch"
            }
        } else {
            "no_expected"
        };
        output.json(&CoremlVerifyJson {
            manifest_path: manifest_path.to_string_lossy().as_ref(),
            actual_hash: actual.to_hex(),
            expected_hash: expected.as_ref().map(|h| h.to_hex()),
            status,
        })?;
        if status == "mismatch" {
            return Err(anyhow::anyhow!("CoreML hash mismatch"));
        }
        return Ok(());
    }

    output.info(format!(
        "CoreML manifest: {}",
        manifest_path.to_string_lossy()
    ));
    output.kv("Actual hash", &actual.to_hex());

    match expected {
        Some(exp) => {
            output.kv("Expected hash", &exp.to_hex());
            if exp == actual {
                output.success("CoreML fused package hash verified");
            } else {
                output.error("CoreML fused package hash mismatch");
                return Err(anyhow::anyhow!("CoreML fused package hash mismatch"));
            }
        }
        None => {
            output.warning("No expected hash provided; reported actual hash only");
        }
    }

    Ok(())
}

fn manifest_path_for_package(package: &Path) -> Result<PathBuf> {
    let manifest_path = if package.is_dir() {
        package.join("Manifest.json")
    } else {
        package.to_path_buf()
    };
    if !manifest_path.exists() {
        return Err(anyhow::anyhow!(
            "Manifest.json not found at {}",
            manifest_path.display()
        ));
    }
    Ok(manifest_path)
}

fn hash_manifest(path: &Path) -> Result<B3Hash> {
    let bytes = fs::read(path).context("Failed to read manifest")?;
    Ok(B3Hash::hash(&bytes))
}

#[derive(Debug, Deserialize)]
struct FusionMetadata {
    fused_manifest_hash: Option<String>,
}

fn load_fusion_metadata(path: &Path) -> Result<B3Hash> {
    let data = fs::read_to_string(path).context("Failed to read fusion metadata")?;
    let metadata: FusionMetadata =
        serde_json::from_str(&data).context("Failed to parse fusion metadata JSON")?;
    let hash_hex = metadata
        .fused_manifest_hash
        .ok_or_else(|| anyhow::anyhow!("Fusion metadata missing fused_manifest_hash"))?;
    B3Hash::from_hex(&hash_hex).map_err(|e| anyhow::anyhow!("Invalid fused hash: {}", e))
}

/// Verify audit chain integrity (policy audit + evidence envelopes)
///
/// Checks all audit chains for tampering or divergence:
/// - Policy audit decision chains (per-tenant BLAKE3 chain)
/// - Evidence envelope chains (per-tenant+scope chain)
async fn verify_audit_chains(
    policy_only: bool,
    evidence_only: bool,
    tenant_filter: Option<String>,
    fail_on_divergence: bool,
    output: &OutputWriter,
) -> Result<()> {
    use adapteros_db::Db;

    output.section("Audit Chain Verification");

    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    let mut any_divergence = false;
    let mut total_policy_checked = 0usize;
    let mut total_evidence_checked = 0usize;

    // Verify policy audit chains
    if !evidence_only {
        output.info("Verifying policy audit chains...");

        let results = db
            .verify_all_policy_audit_chains()
            .await
            .context("Failed to verify policy audit chains")?;

        let mut policy_results = Vec::new();

        for (tenant_id, result) in &results {
            if let Some(ref filter) = tenant_filter {
                if tenant_id != filter {
                    continue;
                }
            }

            total_policy_checked += 1;

            if result.divergence_detected {
                any_divergence = true;
                output.error(format!(
                    "DIVERGENCE: tenant={} sequence={} entries={} error={}",
                    tenant_id,
                    result.first_invalid_sequence.unwrap_or(0),
                    result.entries_checked,
                    result.error_message.as_deref().unwrap_or("unknown")
                ));
            } else {
                output.success(format!(
                    "OK: tenant={} entries_checked={} duration={}ms",
                    tenant_id, result.entries_checked, result.duration_ms
                ));
            }

            policy_results.push(serde_json::json!({
                "tenant_id": tenant_id,
                "is_valid": result.is_valid,
                "entries_checked": result.entries_checked,
                "divergence_detected": result.divergence_detected,
                "first_invalid_sequence": result.first_invalid_sequence,
                "error_message": result.error_message,
                "duration_ms": result.duration_ms,
            }));
        }

        if output.is_json() {
            output.json(&serde_json::json!({
                "type": "policy_audit",
                "chains_verified": policy_results.len(),
                "results": policy_results,
            }))?;
        }
    }

    // Verify evidence envelope chains
    if !policy_only {
        output.blank();
        output.info("Verifying evidence envelope chains...");

        let results = db
            .verify_all_evidence_chains()
            .await
            .context("Failed to verify evidence chains")?;

        let mut evidence_results = Vec::new();

        for result in &results {
            if let Some(ref filter) = tenant_filter {
                if &result.tenant_id != filter {
                    continue;
                }
            }

            total_evidence_checked += 1;

            if result.divergence_detected {
                any_divergence = true;
                output.error(format!(
                    "DIVERGENCE: tenant={} scope={:?} index={} envelopes={} error={}",
                    result.tenant_id,
                    result.scope,
                    result.first_invalid_index.unwrap_or(0),
                    result.envelopes_checked,
                    result.error_message.as_deref().unwrap_or("unknown")
                ));
            } else if result.envelopes_checked > 0 {
                output.success(format!(
                    "OK: tenant={} scope={:?} envelopes_checked={} duration={}ms",
                    result.tenant_id, result.scope, result.envelopes_checked, result.duration_ms
                ));
            }

            evidence_results.push(serde_json::json!({
                "tenant_id": result.tenant_id,
                "scope": format!("{:?}", result.scope),
                "is_valid": result.is_valid,
                "envelopes_checked": result.envelopes_checked,
                "divergence_detected": result.divergence_detected,
                "first_invalid_index": result.first_invalid_index,
                "error_message": result.error_message,
                "duration_ms": result.duration_ms,
            }));
        }

        if output.is_json() {
            output.json(&serde_json::json!({
                "type": "evidence_envelope",
                "chains_verified": evidence_results.len(),
                "results": evidence_results,
            }))?;
        }
    }

    // Summary
    output.blank();
    output.section("Summary");
    output.kv("Policy chains verified", &total_policy_checked.to_string());
    output.kv(
        "Evidence chains verified",
        &total_evidence_checked.to_string(),
    );

    if any_divergence {
        output.error("DIVERGENCE DETECTED - audit chain integrity compromised");
        if fail_on_divergence {
            return Err(anyhow::anyhow!("Audit chain divergence detected"));
        }
    } else {
        output.success("All audit chains verified successfully");
    }

    Ok(())
}
