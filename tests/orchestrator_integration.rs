#![cfg(all(test, feature = "extended-tests"))]

//! Integration test for orchestrator promotion gates
//!
//! Tests the full orchestrator gate run with a test CPID,
//! verifying all gates execute correctly and produce proper reports.

use adapteros_orchestrator::{Orchestrator, OrchestratorConfig, ReportFormat};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[tokio::test]
async fn test_orchestrator_gate_run() -> Result<()> {
    // Create temporary directory for test
    let temp_dir = TempDir::with_prefix("aos-test-")?;
    let temp_path = temp_dir.path();

    // Create test database
    let db_path = temp_path.join("test.db");
    let db = adapteros_db::Db::connect(&db_path.to_string_lossy()).await?;
    db.migrate().await?;

    // Create test CPID
    let test_cpid = "cp_test_orchestrator_123";

    // Create test audit record
    let audit_json = serde_json::json!({
        "hallucination_metrics": {
            "arr": 0.96,
            "ecs5": 0.78,
            "hlr": 0.02,
            "cr": 0.005
        },
        "performance": {
            "latency_p95_ms": 20,
            "throughput_tokens_per_s": 45,
            "router_overhead_pct": 6.5
        }
    });

    db.create_audit(
        "test_tenant",
        test_cpid,
        "test_suite",
        None,
        &audit_json.to_string(),
        "completed",
    )
    .await?;

    // Create test plan record
    db.create_plan(
        "plan_123",
        "test_tenant",
        test_cpid,
        "manifest_hash_123",
        "{}",
        "layout_hash_123",
    )
    .await?;

    // Create test bundles directory
    let bundles_path = temp_path.join("bundles");
    fs::create_dir_all(&bundles_path)?;

    // Create test replay bundle
    let replay_content = r#"{"event_type": "test", "payload": {}, "timestamp": 1234567890}
{"event_type": "test2", "payload": {}, "timestamp": 1234567891}"#;
    fs::write(
        bundles_path.join(format!("{}_replay.ndjson", test_cpid)),
        replay_content,
    )?;

    // Create test manifests directory
    let manifests_path = temp_path.join("manifests");
    fs::create_dir_all(&manifests_path)?;

    // Create test manifest
    let manifest_content = r#"schema: adapteros.manifest.v3
base:
  model_id: test-model
  model_hash: b3:test123
  arch: llama
  vocab_size: 32000
  hidden_dim: 4096
  n_layers: 32
  n_heads: 32
adapters: []
router:
  k_sparse: 3
  gate_quant: q15
  entropy_floor: 0.02
telemetry:
  bundle:
    max_events: 1000
    max_bytes: 1048576
policies:
  determinism:
    require_metallib_embed: true
    require_kernel_hash_match: true
    rng: hkdf_seeded
    retrieval_tie_break: [score_desc, doc_id_asc]
seeds:
  global: b3:global123
  manifest_hash: b3:manifest123"#;

    fs::write(
        manifests_path.join(format!("{}.yaml", test_cpid)),
        manifest_content,
    )?;

    // Create test SBOM
    let target_dir = temp_path.join("target");
    fs::create_dir_all(&target_dir)?;

    let sbom_content = r#"{
  "spdxVersion": "SPDX-2.3",
  "dataLicense": "CC0-1.0",
  "spdxId": "SPDXRef-DOCUMENT",
  "name": "adapterOS",
  "documentNamespace": "https://github.com/rogu3bear/adapter-os/sbom/test",
  "creationInfo": {
    "created": "2025-01-01T00:00:00Z",
    "creators": ["Tool: aos-sbom"],
    "licenseListVersion": "3.20"
  },
  "packages": [
    {
      "spdxId": "SPDXRef-Package-0",
      "name": "test-package",
      "versionInfo": "1.0.0",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false
    }
  ],
  "files": []
}"#;

    fs::write(target_dir.join("sbom.spdx.json"), sbom_content)?;

    // Configure orchestrator
    let config = OrchestratorConfig {
        continue_on_error: false,
        cpid: test_cpid.to_string(),
        db_path: db_path.to_string_lossy().to_string(),
        bundles_path: bundles_path.to_string_lossy().to_string(),
        manifests_path: manifests_path.to_string_lossy().to_string(),
        ..Default::default()
    };

    // Create orchestrator
    let orchestrator = Orchestrator::new(config);

    // Run gates
    let report = orchestrator.run().await?;

    // Verify report
    assert_eq!(report.cpid, test_cpid);
    assert!(!report.gates.is_empty());

    // Check that determinism gate passed
    if let Some(determinism_result) = report.gates.get("Determinism") {
        assert!(determinism_result.passed, "Determinism gate should pass");
    }

    // Check that SBOM gate passed
    if let Some(sbom_result) = report.gates.get("SBOM") {
        assert!(sbom_result.passed, "SBOM gate should pass");
    }

    // Test JSON report generation
    let json_report = report.to_json()?;
    assert!(json_report.contains(test_cpid));

    // Test Markdown report generation
    let markdown_report = report.to_markdown();
    assert!(markdown_report.contains(test_cpid));
    assert!(markdown_report.contains("Gate Results"));

    // Test report file writing
    let report_path = temp_path.join("test_report.md");
    report.write_to_file(&report_path, ReportFormat::Markdown)?;

    let written_content = fs::read_to_string(&report_path)?;
    assert!(written_content.contains(test_cpid));

    println!("✓ Orchestrator integration test passed");
    println!("  CPID: {}", test_cpid);
    println!("  Gates: {}", report.gates.len());
    println!("  All passed: {}", report.all_passed);

    Ok(())
}

#[tokio::test]
async fn test_orchestrator_gate_failure() -> Result<()> {
    // Test orchestrator behavior when gates fail
    let temp_dir = TempDir::with_prefix("aos-test-")?;
    let temp_path = temp_dir.path();

    // Create test database
    let db_path = temp_path.join("test.db");
    let db = adapteros_db::Db::connect(&db_path.to_string_lossy()).await?;
    db.migrate().await?;

    let test_cpid = "cp_test_failure_456";

    // Create audit with failing metrics
    let failing_audit_json = serde_json::json!({
        "hallucination_metrics": {
            "arr": 0.85,  // Below threshold of 0.95
            "ecs5": 0.70, // Below threshold of 0.75
            "hlr": 0.05,  // Above threshold of 0.03
            "cr": 0.02    // Above threshold of 0.01
        }
    });

    db.create_audit(
        "test_tenant",
        test_cpid,
        "test_suite",
        None,
        &failing_audit_json.to_string(),
        "completed",
    )
    .await?;

    // Configure orchestrator
    let config = OrchestratorConfig {
        continue_on_error: true, // Continue on error to test all gates
        cpid: test_cpid.to_string(),
        db_path: db_path.to_string_lossy().to_string(),
        bundles_path: temp_path.join("bundles").to_string_lossy().to_string(),
        manifests_path: temp_path.join("manifests").to_string_lossy().to_string(),
        ..Default::default()
    };

    let orchestrator = Orchestrator::new(config);
    let report = orchestrator.run().await?;

    // Verify that metrics gate failed
    if let Some(metrics_result) = report.gates.get("Metrics") {
        assert!(
            !metrics_result.passed,
            "Metrics gate should fail with bad metrics"
        );
        assert!(metrics_result
            .message
            .contains("Hallucination metrics failed"));
    }

    // Overall report should indicate failure
    assert!(!report.all_passed, "Report should indicate failure");

    println!("✓ Orchestrator failure test passed");
    println!("  CPID: {}", test_cpid);
    println!("  All passed: {}", report.all_passed);

    Ok(())
}

#[tokio::test]
async fn test_orchestrator_cli_help() -> Result<()> {
    // Test that CLI help works
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "mplora-orchestrator", "--", "--help"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("adapterOS promotion gate orchestrator"));
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("gate"));

    println!("✓ CLI help test passed");

    Ok(())
}
