//! Comprehensive drift detection tests

use adapteros_core::B3Hash;
use adapteros_manifest::DriftPolicy;
use adapteros_verify::{DeviceFingerprint, DriftEvaluator, DriftSeverity};

#[test]
fn test_fingerprint_stability_across_restarts() {
    // Capture fingerprint twice and ensure hashes match
    let fp1 = DeviceFingerprint::capture_current().expect("Failed to capture fingerprint 1");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let fp2 = DeviceFingerprint::capture_current().expect("Failed to capture fingerprint 2");

    assert_eq!(
        fp1.compute_hash().unwrap(),
        fp2.compute_hash().unwrap(),
        "Fingerprints should be identical across captures"
    );
}

#[test]
fn test_drift_detection_no_drift() {
    let baseline = DeviceFingerprint {
        schema_version: 1,
        device_model: "MacBookPro18,3".to_string(),
        soc_id: "Apple M1 Pro".to_string(),
        gpu_pci_id: "Apple7::12345".to_string(),
        os_version: "14.0".to_string(),
        os_build: "23A344".to_string(),
        metal_family: "Apple9".to_string(),
        gpu_driver_version: "14.0".to_string(),
        path_hash: B3Hash::hash(b"path"),
        env_hash: B3Hash::hash(b"env"),
        cpu_features: vec!["aarch64".to_string()],
        firmware_hash: None,
        boot_version_hash: None,
    };

    let current = baseline.clone();

    let evaluator = DriftEvaluator::new();
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert!(!report.drift_detected);
    assert_eq!(report.severity, DriftSeverity::None);
    assert!(!report.should_block());
}

#[test]
fn test_drift_detection_os_build_change() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A345");

    let evaluator = DriftEvaluator::new();
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert!(report.drift_detected);
    assert_eq!(report.severity, DriftSeverity::Critical);
    assert!(report.should_block());
    assert_eq!(report.field_drifts.len(), 1);
    assert_eq!(report.field_drifts[0].field_name, "os_build");
}

#[test]
fn test_drift_detection_gpu_change() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A344");
    current.metal_family = "Apple8".to_string();

    let evaluator = DriftEvaluator::new();
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert!(report.drift_detected);
    assert_eq!(report.severity, DriftSeverity::Critical);
    assert!(report.should_block());
}

#[test]
fn test_drift_policy_tolerance_enforcement() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A345");

    // With zero tolerance - should be critical
    let policy_strict = DriftPolicy {
        os_build_tolerance: 0,
        gpu_driver_tolerance: 0,
        env_hash_tolerance: 0,
        allow_warnings: true,
        block_on_critical: true,
    };

    let evaluator_strict = DriftEvaluator::from_policy(&policy_strict);
    let report_strict = evaluator_strict.compare(&baseline, &current).unwrap();

    assert_eq!(report_strict.severity, DriftSeverity::Critical);
    assert!(report_strict.should_block());

    // With tolerance - should be warning
    let policy_lenient = DriftPolicy {
        os_build_tolerance: 1,
        gpu_driver_tolerance: 1,
        env_hash_tolerance: 1,
        allow_warnings: true,
        block_on_critical: true,
    };

    let evaluator_lenient = DriftEvaluator::from_policy(&policy_lenient);
    let report_lenient = evaluator_lenient.compare(&baseline, &current).unwrap();

    assert_eq!(report_lenient.severity, DriftSeverity::Warning);
    assert!(!report_lenient.should_block());
}

#[test]
fn test_critical_drift_blocks_startup() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A344");
    current.device_model = "MacBookPro19,1".to_string(); // Different device!

    let evaluator = DriftEvaluator::new();
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert_eq!(report.severity, DriftSeverity::Critical);
    assert!(
        report.should_block(),
        "Device model change should block startup"
    );
}

#[test]
fn test_warning_drift_allows_startup() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A344");
    current.env_hash = B3Hash::hash(b"different_env");

    let policy = DriftPolicy {
        os_build_tolerance: 0,
        gpu_driver_tolerance: 0,
        env_hash_tolerance: 1,
        allow_warnings: true,
        block_on_critical: true,
    };

    let evaluator = DriftEvaluator::from_policy(&policy);
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert_eq!(report.severity, DriftSeverity::Info);
    assert!(!report.should_block(), "Info-level drift should not block");
}

#[test]
fn test_fingerprint_signing_verification() {
    use adapteros_crypto::Keypair;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let fingerprint_path = temp_dir.path().join("test_fingerprint.json");

    let fp = create_test_fingerprint("23A344");
    let keypair = Keypair::generate();

    // Save signed
    fp.save_signed(&fingerprint_path, &keypair).unwrap();

    // Verify signature file exists
    let sig_path = fingerprint_path.with_extension("sig");
    assert!(sig_path.exists(), "Signature file should exist");

    // Load and verify
    let loaded_fp =
        DeviceFingerprint::load_verified(&fingerprint_path, &keypair.public_key()).unwrap();

    assert_eq!(fp, loaded_fp, "Loaded fingerprint should match original");
}

#[test]
fn test_signature_tampering_detection() {
    use adapteros_crypto::Keypair;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let fingerprint_path = temp_dir.path().join("test_fingerprint.json");

    let fp = create_test_fingerprint("23A344");
    let keypair = Keypair::generate();

    // Save signed
    fp.save_signed(&fingerprint_path, &keypair).unwrap();

    // Tamper with the JSON file
    let mut json = fs::read_to_string(&fingerprint_path).unwrap();
    json = json.replace("23A344", "23A999");
    fs::write(&fingerprint_path, json).unwrap();

    // Try to load - should fail signature verification
    let result = DeviceFingerprint::load_verified(&fingerprint_path, &keypair.public_key());
    assert!(
        result.is_err(),
        "Tampered fingerprint should fail verification"
    );
}

#[test]
fn test_canonical_json_hash_stability() {
    let fp1 = create_test_fingerprint("23A344");
    let fp2 = create_test_fingerprint("23A344");

    let hash1 = fp1.compute_hash().unwrap();
    let hash2 = fp2.compute_hash().unwrap();

    assert_eq!(
        hash1, hash2,
        "Identical fingerprints should have identical hashes"
    );
}

#[test]
fn test_multiple_field_drift() {
    let baseline = create_test_fingerprint("23A344");
    let mut current = create_test_fingerprint("23A345");
    current.gpu_driver_version = "14.1".to_string();
    current.env_hash = B3Hash::hash(b"different_env");

    let evaluator = DriftEvaluator::new();
    let report = evaluator.compare(&baseline, &current).unwrap();

    assert!(report.drift_detected);
    assert!(
        report.field_drifts.len() >= 2,
        "Should detect multiple field changes"
    );
}

#[test]
fn test_device_model_match() {
    let fp1 = create_test_fingerprint("23A344");
    let fp2 = create_test_fingerprint("23A344");

    assert!(fp1.matches(&fp2), "Identical fingerprints should match");

    let mut fp3 = create_test_fingerprint("23A344");
    fp3.device_model = "Different".to_string();

    assert!(
        !fp1.matches(&fp3),
        "Different fingerprints should not match"
    );
}

#[test]
fn test_summary_generation() {
    let fp = create_test_fingerprint("23A344");
    let summary = fp.summary();

    assert!(summary.contains("MacBookPro18,3"));
    assert!(summary.contains("Apple M1 Pro"));
    assert!(summary.contains("23A344"));
}

// Helper function to create a test fingerprint
fn create_test_fingerprint(os_build: &str) -> DeviceFingerprint {
    DeviceFingerprint {
        schema_version: 1,
        device_model: "MacBookPro18,3".to_string(),
        soc_id: "Apple M1 Pro".to_string(),
        gpu_pci_id: "Apple7::12345".to_string(),
        os_version: "14.0".to_string(),
        os_build: os_build.to_string(),
        metal_family: "Apple9".to_string(),
        gpu_driver_version: "14.0".to_string(),
        path_hash: B3Hash::hash(b"/usr/bin:/usr/local/bin"),
        env_hash: B3Hash::hash(b"stable_env"),
        cpu_features: vec!["aarch64".to_string()],
        firmware_hash: None,
        boot_version_hash: None,
    }
}
