#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for .aos signature verification and security
//!
//! Tests cryptographic signature generation, verification, and tamper detection.

use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
use adapteros_single_file_adapter::{
    LineageInfo, LoadOptions, PackageOptions, SingleFileAdapter, SingleFileAdapterLoader,
    SingleFileAdapterPackager, WeightGroupConfig, AOS_FORMAT_VERSION,
};
use tempfile::TempDir;

fn create_test_adapter(adapter_id: &str) -> SingleFileAdapter {
    let weights = vec![1u8; 1024]; // 1KB test weights
    let training_data = vec![TrainingExample::new(vec![1, 2, 3], vec![4, 5, 6])];
    let config = TrainingConfig {
        rank: 16,
        alpha: 32.0,
        learning_rate: 0.0005,
        batch_size: 8,
        epochs: 4,
        hidden_dim: 3584,
        weight_group_config: WeightGroupConfig::default(),
    };
    let lineage = LineageInfo {
        adapter_id: adapter_id.to_string(),
        version: "1.0.0".to_string(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    SingleFileAdapter::create(
        adapter_id.to_string(),
        weights,
        training_data,
        config,
        lineage,
    )
    .expect("Failed to create test adapter")
}

fn new_test_tempdir() -> Result<TempDir> {
    Ok(TempDir::with_prefix("aos-test-")?)
}

#[tokio::test]
async fn test_signature_roundtrip() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("signed.aos");

    // Create and sign adapter
    let mut adapter = create_test_adapter("roundtrip_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;

    assert!(adapter.is_signed());

    // Save and load
    SingleFileAdapterPackager::save(&adapter, &path).await?;
    let loaded = SingleFileAdapterLoader::load(&path).await?;

    // Verify signature persists
    assert!(loaded.is_signed());
    assert!(loaded.verify()?);

    // Check signature info
    let (key_id, timestamp) = loaded.signature_info().unwrap();
    assert!(!key_id.is_empty());
    assert!(timestamp > 0);

    Ok(())
}

#[tokio::test]
async fn test_tamper_detection_weights() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("tampered_weights.aos");

    // Create, sign, and save adapter
    let mut adapter = create_test_adapter("tamper_weights_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;
    SingleFileAdapterPackager::save(&adapter, &path).await?;

    // Load and tamper with weights
    let mut loaded = SingleFileAdapterLoader::load_with_options(
        &path,
        LoadOptions {
            skip_verification: true,
            skip_signature_check: false,
        },
    )
    .await?;

    loaded.weights[0] ^= 0xFF; // Flip bits in first byte

    // Verification should fail
    assert!(!loaded.verify()?);

    Ok(())
}

#[tokio::test]
async fn test_tamper_detection_manifest() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("tampered_manifest.aos");

    // Create, sign, and save adapter
    let mut adapter = create_test_adapter("tamper_manifest_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;
    SingleFileAdapterPackager::save(&adapter, &path).await?;

    // Load and tamper with manifest
    let mut loaded = SingleFileAdapterLoader::load_with_options(
        &path,
        LoadOptions {
            skip_verification: true,
            skip_signature_check: false,
        },
    )
    .await?;

    loaded.manifest.adapter_id = "TAMPERED".to_string();

    // Verification should fail (manifest hash mismatch)
    assert!(!loaded.verify()?);

    Ok(())
}

#[tokio::test]
async fn test_unsigned_adapter() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("unsigned.aos");

    // Create unsigned adapter
    let adapter = create_test_adapter("unsigned_test");
    assert!(!adapter.is_signed());

    // Save and load
    SingleFileAdapterPackager::save(&adapter, &path).await?;
    let loaded = SingleFileAdapterLoader::load(&path).await?;

    // Should verify even without signature (hash checks pass)
    assert!(!loaded.is_signed());
    assert!(loaded.verify()?);

    Ok(())
}

#[tokio::test]
async fn test_package_options_with_signature() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("packaged.aos");

    // Create, sign, and save with options
    let mut adapter = create_test_adapter("packaged_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;

    let options = PackageOptions::with_combined_weights();
    SingleFileAdapterPackager::save_with_options(&adapter, &path, options).await?;

    // Load and verify
    let loaded = SingleFileAdapterLoader::load(&path).await?;
    assert!(loaded.is_signed());
    assert!(loaded.verify()?);

    Ok(())
}

#[tokio::test]
async fn test_signature_performance() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let signed_path = temp_dir.path().join("signed_perf.aos");
    let unsigned_path = temp_dir.path().join("unsigned_perf.aos");

    // Create adapters
    let mut signed = create_test_adapter("signed_perf_test");
    let unsigned = create_test_adapter("unsigned_perf_test");

    let keypair = Keypair::generate();
    signed.sign(&keypair)?;

    // Save both
    SingleFileAdapterPackager::save(&signed, &signed_path).await?;
    SingleFileAdapterPackager::save(&unsigned, &unsigned_path).await?;

    // Time signed load
    let start = std::time::Instant::now();
    let _loaded_signed = SingleFileAdapterLoader::load(&signed_path).await?;
    let signed_duration = start.elapsed();

    // Time unsigned load
    let start = std::time::Instant::now();
    let _loaded_unsigned = SingleFileAdapterLoader::load(&unsigned_path).await?;
    let unsigned_duration = start.elapsed();

    // Log performance (signature verification should add minimal overhead)
    println!(
        "Load times: signed={:?}, unsigned={:?}, overhead={:?}",
        signed_duration,
        unsigned_duration,
        signed_duration.saturating_sub(unsigned_duration)
    );

    // Signature verification should add <10ms for small adapters
    assert!(signed_duration < unsigned_duration + std::time::Duration::from_millis(10));

    Ok(())
}

#[tokio::test]
async fn test_skip_signature_verification() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("skip_sig.aos");

    // Create signed adapter with tampered weights
    let mut adapter = create_test_adapter("skip_sig_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;
    adapter.weights[0] ^= 0xFF; // Tamper

    // Normal save (signature is now invalid due to tamper)
    SingleFileAdapterPackager::save(&adapter, &path).await?;

    // Loading with signature check should fail
    let result = SingleFileAdapterLoader::load(&path).await;
    assert!(result.is_err());

    // Loading with signature check skipped should succeed
    let loaded = SingleFileAdapterLoader::load_with_options(
        &path,
        LoadOptions {
            skip_verification: false,
            skip_signature_check: true,
        },
    )
    .await;
    assert!(loaded.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_skip_bypass_disallowed_in_production_mode() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("prod_guard.aos");

    // Create a signed adapter
    let mut adapter = create_test_adapter("prod_guard_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;
    SingleFileAdapterPackager::save(&adapter, &path).await?;

    // Enable production_mode and request skip
    std::env::set_var("AOS_SERVER_PRODUCTION_MODE", "true");
    let result = SingleFileAdapterLoader::load_with_options(
        &path,
        LoadOptions {
            skip_verification: false,
            skip_signature_check: true,
        },
    )
    .await;
    std::env::remove_var("AOS_SERVER_PRODUCTION_MODE");

    assert!(
        matches!(result, Err(AosError::PolicyViolation(_))),
        "Skip flags must be blocked when production_mode is enabled"
    );

    Ok(())
}

#[tokio::test]
async fn test_format_version_in_signed_adapter() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let path = temp_dir.path().join("version_check.aos");

    // Create and sign adapter
    let mut adapter = create_test_adapter("version_test");
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;

    // Verify format version is set
    assert_eq!(adapter.manifest.format_version, AOS_FORMAT_VERSION);

    // Save and reload
    SingleFileAdapterPackager::save(&adapter, &path).await?;
    let loaded = SingleFileAdapterLoader::load(&path).await?;

    // Format version should persist
    assert_eq!(loaded.manifest.format_version, AOS_FORMAT_VERSION);
    assert!(loaded.verify()?);

    Ok(())
}
