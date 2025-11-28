//! Drift check command

// Database integration will be added when drift checking is fully implemented
use adapteros_core::DriftPolicy;
use adapteros_verify::{
    get_or_create_fingerprint_keypair, DeviceFingerprint, DriftEvaluator, DriftSeverity,
};
use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{error, info, warn};

/// Execute drift check
pub async fn drift_check(
    db_path: Option<PathBuf>,
    baseline_path: Option<PathBuf>,
    save_current: bool,
    save_baseline: bool,
) -> Result<i32> {
    info!("Starting drift check...");

    // Capture current fingerprint
    let current =
        DeviceFingerprint::capture_current().context("Failed to capture device fingerprint")?;

    info!("Current fingerprint: {}", current.summary());

    // Determine baseline path
    let baseline_path =
        baseline_path.unwrap_or_else(|| PathBuf::from("var/baseline_fingerprint.json"));

    // Get keypair for signing/verification
    let keypair =
        get_or_create_fingerprint_keypair().context("Failed to get fingerprint signing keypair")?;

    // Check if baseline exists
    if !baseline_path.exists() {
        warn!("No baseline fingerprint found at {:?}", baseline_path);

        if save_baseline {
            warn!("Creating new baseline fingerprint...");
            current
                .save_signed(&baseline_path, &keypair)
                .context("Failed to save baseline fingerprint")?;
            info!("✓ Baseline fingerprint created and signed");
            return Ok(10); // Exit code 10: No drift (first run)
        } else {
            error!("No baseline fingerprint exists. Run with --save-baseline to create one.");
            return Ok(12); // Critical: no baseline to compare against
        }
    }

    // Load baseline fingerprint
    let baseline = DeviceFingerprint::load_verified(&baseline_path, &keypair.public_key())
        .context("Failed to load and verify baseline fingerprint")?;

    info!("Baseline fingerprint: {}", baseline.summary());

    // Load drift policy
    let drift_policy = load_drift_policy(db_path.as_deref()).await?;

    // Compare fingerprints
    let evaluator = DriftEvaluator::from_policy(&drift_policy);
    let drift_report = evaluator
        .compare(&baseline, &current)
        .context("Failed to compare fingerprints")?;

    // Print drift report
    println!("\n{}", "=".repeat(60));
    println!("Drift Detection Report");
    println!("{}", "=".repeat(60));
    println!();
    println!("Baseline hash: {}", drift_report.baseline_hash);
    println!("Current hash:  {}", drift_report.current_hash);
    println!();

    if drift_report.drift_detected {
        println!("Status: DRIFT DETECTED");
        println!("Severity: {:?}", drift_report.severity);
        println!("Fields changed: {}", drift_report.field_drifts.len());
        println!();

        for field_drift in &drift_report.field_drifts {
            let severity_marker = match field_drift.severity {
                DriftSeverity::None => "   ",
                DriftSeverity::Info => " ℹ ",
                DriftSeverity::Warning => " ⚠ ",
                DriftSeverity::Critical => " ✗ ",
            };

            println!(
                "{}[{:?}] {}:",
                severity_marker, field_drift.severity, field_drift.field_name
            );
            println!("    Baseline: {}", field_drift.baseline_value);
            println!("    Current:  {}", field_drift.current_value);
            println!();
        }

        if drift_report.should_block() {
            error!("CRITICAL DRIFT: This system cannot run inference until drift is resolved.");
        } else if drift_report.severity == DriftSeverity::Warning {
            warn!("WARNING: Drift may affect determinism.");
        }
    } else {
        println!("Status: NO DRIFT DETECTED");
        println!("✓ Environment fingerprint matches baseline");
    }

    println!("{}", "=".repeat(60));
    println!();

    // Save current fingerprint if requested
    if save_current {
        let current_path = PathBuf::from("var/system_fingerprint.json");
        current
            .save_signed(&current_path, &keypair)
            .context("Failed to save current fingerprint")?;
        info!("Current fingerprint saved to: {:?}", current_path);
    }

    // Save drift report to history
    save_drift_history(&drift_report)?;

    // Return exit code based on severity
    let exit_code = match drift_report.severity {
        DriftSeverity::None => 10,     // No drift
        DriftSeverity::Info => 10,     // Info only
        DriftSeverity::Warning => 11,  // Warning
        DriftSeverity::Critical => 12, // Critical
    };

    info!("Drift check complete (exit code: {})", exit_code);
    Ok(exit_code)
}

/// Load drift policy from database or use default
async fn load_drift_policy(db_path: Option<&std::path::Path>) -> Result<DriftPolicy> {
    let db_path = db_path.unwrap_or(std::path::Path::new("var/aos.db"));

    if db_path.exists() {
        let db_path_str = db_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {}", db_path.display()))?;
        match adapteros_db::Db::connect(db_path_str).await {
            Ok(db) => {
                // Try to load tenant-specific policies
                match db.get_policies("default").await {
                    Ok(policies) => {
                        info!("Loaded drift policy from database");
                        return Ok(policies.drift);
                    }
                    Err(e) => {
                        warn!(
                            "Could not load policies from database: {}, using defaults",
                            e
                        );
                    }
                }
            }
            Err(e) => {
                warn!("Could not open database: {}, using default policy", e);
            }
        }
    } else {
        warn!("Database not found, using default drift policy");
    }

    Ok(DriftPolicy::default())
}

/// Save drift report to history directory
fn save_drift_history(drift_report: &adapteros_verify::DriftReport) -> Result<()> {
    use std::fs;

    let history_dir = PathBuf::from("var/drift_history");
    fs::create_dir_all(&history_dir).context("Failed to create drift history directory")?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let report_path = history_dir.join(format!("drift_report_{}.json", timestamp));

    let report_json =
        serde_json::to_string_pretty(&drift_report).context("Failed to serialize drift report")?;

    fs::write(&report_path, &report_json).context("Failed to write drift report")?;

    info!("Drift report saved to: {:?}", report_path);

    // Sign the drift report
    let keypair = get_or_create_fingerprint_keypair()?;
    let report_hash = adapteros_core::B3Hash::hash(report_json.as_bytes());
    let signature = adapteros_crypto::sign_bytes(&keypair, report_hash.as_bytes());

    let sig_path = report_path.with_extension("sig");
    fs::write(&sig_path, signature.to_bytes()).context("Failed to write drift report signature")?;

    Ok(())
}
