#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_core::{AosError, B3Hash, Result};
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_db::Db;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_kernel_coreml::export::validate_coreml_fusion;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_kernel_coreml::ComputeUnits;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_worker::backend_factory::CoreMLBackendSettings;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_worker::CoremlRuntimeTelemetry;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_manifest::ManifestV3;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use std::fs;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use tracing::{info, warn};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoremlVerifyMode {
    Off,
    Warn,
    Strict,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn resolve_coreml_verify_mode() -> CoremlVerifyMode {
    match std::env::var("AOS_COREML_VERIFY_MODE")
        .unwrap_or_else(|_| "warn".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "off" | "disable" | "disabled" => CoremlVerifyMode::Off,
        "strict" | "fail" | "enforce" => CoremlVerifyMode::Strict,
        _ => CoremlVerifyMode::Warn,
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn coreml_manifest_path(model_path: &Path) -> Result<PathBuf> {
    let manifest_path = if model_path.is_dir() {
        model_path.join("Manifest.json")
    } else {
        model_path.to_path_buf()
    };

    if !manifest_path.exists() {
        return Err(AosError::Validation(
            "CoreML manifest not found (expected Manifest.json)".to_string(),
        ));
    }

    Ok(manifest_path)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn compute_coreml_package_hash(model_path: &Path) -> Result<B3Hash> {
    let manifest_path = coreml_manifest_path(model_path)?;
    let bytes = fs::read(&manifest_path)
        .map_err(|e| AosError::Io(format!("Failed to read CoreML manifest for hashing: {}", e)))?;
    Ok(B3Hash::hash(&bytes))
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn expected_coreml_hash_from_metadata(model_path: &Path) -> Option<B3Hash> {
    let candidate = if model_path.is_dir() {
        model_path.join("adapteros_coreml_fusion.json")
    } else {
        model_path
            .parent()
            .map(|p| p.join("adapteros_coreml_fusion.json"))
            .unwrap_or_else(|| PathBuf::from("adapteros_coreml_fusion.json"))
    };
    if !candidate.exists() {
        return None;
    }
    validate_coreml_fusion(&candidate)
        .map(|meta| meta.fused_manifest_hash)
        .ok()
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn resolve_fusion_ids(manifest: &ManifestV3) -> (Option<String>, Option<String>) {
    let base_model_id = manifest
        .fusion
        .as_ref()
        .and_then(|f| f.base_model_id.clone())
        .or_else(|| Some(manifest.base.model_id.clone()));

    let adapter_id = manifest
        .fusion
        .as_ref()
        .and_then(|f| f.adapter_id.clone())
        .or_else(|| {
            if manifest.adapters.len() == 1 {
                Some(manifest.adapters[0].id.clone())
            } else {
                None
            }
        });

    (base_model_id, adapter_id)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub async fn resolve_expected_coreml_hash(
    manifest: &ManifestV3,
    model_path: &Path,
    tenant_id: &str,
    db: Option<&Db>,
) -> (Option<B3Hash>, Option<String>) {
    if let Some(db) = db {
        let (base_model_id, adapter_id) = resolve_fusion_ids(manifest);
        if let (Some(base_id), Some(adapter_id)) = (base_model_id, adapter_id) {
            match db
                .get_coreml_fusion_pair(tenant_id, &base_id, &adapter_id)
                .await
            {
                Ok(Some(pair)) => {
                    if let Ok(hash) = B3Hash::from_hex(&pair.coreml_package_hash) {
                        return (Some(hash), Some("db".to_string()));
                    }
                    if let Ok(hash) = B3Hash::from_hex(&pair.fused_manifest_hash) {
                        return (Some(hash), Some("db".to_string()));
                    }
                    warn!(
                        tenant_id = %tenant_id,
                        base_model_id = %base_id,
                        adapter_id = %adapter_id,
                        "Failed to parse CoreML fusion hash from database record"
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(
                        tenant_id = %tenant_id,
                        base_model_id = %base_id,
                        adapter_id = %adapter_id,
                        error = %e,
                        "CoreML fusion pair lookup failed, falling back to manifest/env"
                    );
                }
            }
        }
    }

    if let Some(fusion) = &manifest.fusion {
        if let Some(hash) = fusion.fused_manifest_hash {
            return (Some(hash), Some("manifest.fused_manifest_hash".to_string()));
        }
        if let Some(hash) = fusion.coreml_package_hash {
            return (Some(hash), Some("manifest.coreml_package_hash".to_string()));
        }
    }

    if let Ok(env_hash) = std::env::var("AOS_COREML_EXPECTED_HASH") {
        if let Ok(parsed) = B3Hash::from_hex(&env_hash) {
            return (Some(parsed), Some("env".to_string()));
        }
    }

    if let Some(hash) = expected_coreml_hash_from_metadata(model_path) {
        return (Some(hash), Some("metadata".to_string()));
    }

    (None, None)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoremlVerificationStatus {
    Match,
    Mismatch,
    MissingExpected,
    MissingActual,
    Skipped,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
impl CoremlVerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoremlVerificationStatus::Match => "match",
            CoremlVerificationStatus::Mismatch => "mismatch",
            CoremlVerificationStatus::MissingExpected => "missing_expected",
            CoremlVerificationStatus::MissingActual => "missing_actual",
            CoremlVerificationStatus::Skipped => "skipped",
        }
    }

    pub fn is_mismatch(&self) -> bool {
        matches!(self, CoremlVerificationStatus::Mismatch)
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn log_coreml_verification_result(
    mode: CoremlVerifyMode,
    expected: Option<&B3Hash>,
    actual: Option<&B3Hash>,
    source: Option<&str>,
) -> Result<CoremlVerificationStatus> {
    match (expected, actual) {
        (Some(exp), Some(act)) => {
            if exp == act {
                info!(
                    mode = ?mode,
                    expected_source = source.unwrap_or("unknown"),
                    fused_manifest_hash = %act.to_hex(),
                    "CoreML fused package verified"
                );
                Ok(CoremlVerificationStatus::Match)
            } else if mode == CoremlVerifyMode::Strict {
                Err(AosError::Validation(format!(
                    "CoreML fused package hash mismatch (expected {}, got {})",
                    exp.to_hex(),
                    act.to_hex()
                )))
            } else {
                warn!(
                    mode = ?mode,
                    expected_source = source.unwrap_or("unknown"),
                    expected_hash = %exp.to_hex(),
                    actual_hash = %act.to_hex(),
                    "CoreML fused package hash mismatch"
                );
                Ok(CoremlVerificationStatus::Mismatch)
            }
        }
        (None, Some(act)) => {
            match mode {
                CoremlVerifyMode::Strict => {
                    return Err(AosError::Validation(format!(
                        "CoreML verification strict but expected hash missing (actual {})",
                        act.to_hex()
                    )))
                }
                CoremlVerifyMode::Warn => {
                    warn!(
                        mode = "warn",
                        expected_source = source.unwrap_or("unknown"),
                        actual_hash = %act.to_hex(),
                        "CoreML verification skipped (no expected hash)"
                    );
                }
                CoremlVerifyMode::Off => {}
            }
            Ok(CoremlVerificationStatus::MissingExpected)
        }
        (Some(exp), None) => {
            match mode {
                CoremlVerifyMode::Strict => {
                    return Err(AosError::Validation(format!(
                    "CoreML verification strict but failed to compute actual hash (expected {})",
                    exp.to_hex()
                )))
                }
                CoremlVerifyMode::Warn => {
                    warn!(
                        mode = "warn",
                        expected_source = source.unwrap_or("unknown"),
                        expected_hash = %exp.to_hex(),
                        "CoreML verification skipped (actual hash unavailable)"
                    );
                }
                CoremlVerifyMode::Off => {}
            }
            Ok(CoremlVerificationStatus::MissingActual)
        }
        (None, None) => {
            if mode != CoremlVerifyMode::Off {
                warn!(
                    mode = ?mode,
                    "CoreML verification skipped (no expected or actual hash)"
                );
            }
            Ok(CoremlVerificationStatus::Skipped)
        }
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn render_coreml_compute_units(units: ComputeUnits) -> &'static str {
    match units {
        ComputeUnits::CpuOnly => "cpu_only",
        ComputeUnits::CpuAndGpu => "cpu_and_gpu",
        ComputeUnits::CpuAndNeuralEngine => "cpu_and_neural_engine",
        ComputeUnits::All => "all",
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn coreml_effective_compute_units(settings: &CoreMLBackendSettings) -> ComputeUnits {
    if settings.production_mode {
        ComputeUnits::CpuAndNeuralEngine
    } else {
        settings.compute_units
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn coreml_telemetry_from_settings(settings: &CoreMLBackendSettings) -> CoremlRuntimeTelemetry {
    let effective_units = coreml_effective_compute_units(settings);
    let gpu_used = settings.gpu_available
        && matches!(effective_units, ComputeUnits::CpuAndGpu | ComputeUnits::All);
    let ane_used = settings.ane_available
        && matches!(
            effective_units,
            ComputeUnits::CpuAndNeuralEngine | ComputeUnits::All
        );

    CoremlRuntimeTelemetry {
        compute_preference: Some(settings.preference.to_string()),
        compute_units: Some(render_coreml_compute_units(effective_units).to_string()),
        gpu_available: Some(settings.gpu_available),
        ane_available: Some(settings.ane_available),
        gpu_used: Some(gpu_used),
        ane_used: Some(ane_used),
        production_mode: Some(settings.production_mode),
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn coreml_device_label(ane_used: bool, gpu_used: bool) -> &'static str {
    if ane_used {
        "ane"
    } else if gpu_used {
        "gpu"
    } else {
        "cpu"
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn coreml_fallback_reason(settings: &CoreMLBackendSettings) -> Option<String> {
    let mut reasons = Vec::new();
    let effective_units = coreml_effective_compute_units(settings);

    if settings.production_mode && settings.compute_units != effective_units {
        reasons.push("production_enforced_ane");
    }

    if settings.production_mode && !settings.ane_available {
        reasons.push("ane_required_for_production");
    }

    if matches!(
        settings.preference,
        adapteros_config::CoreMLComputePreference::CpuAndGpu
            | adapteros_config::CoreMLComputePreference::All
    ) && !settings.gpu_available
    {
        reasons.push("gpu_unavailable");
    }

    if matches!(
        settings.preference,
        adapteros_config::CoreMLComputePreference::CpuAndNe
            | adapteros_config::CoreMLComputePreference::All
    ) && !settings.ane_available
    {
        reasons.push("ane_unavailable");
    }

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join(","))
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn log_coreml_runtime(label: &str, settings: &CoreMLBackendSettings) {
    let effective_units = coreml_effective_compute_units(settings);
    let gpu_used = settings.gpu_available
        && matches!(effective_units, ComputeUnits::CpuAndGpu | ComputeUnits::All);
    let ane_used = settings.ane_available
        && matches!(
            effective_units,
            ComputeUnits::CpuAndNeuralEngine | ComputeUnits::All
        );
    let fallback_reason = coreml_fallback_reason(settings);
    info!(
        coreml_lane = label,
        compute_preference = %settings.preference,
        compute_units = render_coreml_compute_units(effective_units),
        compute_units_config = render_coreml_compute_units(settings.compute_units),
        production_mode = settings.production_mode,
        ane_available = settings.ane_available,
        gpu_available = settings.gpu_available,
        ane_used = ane_used,
        gpu_used = gpu_used,
        selected_device = coreml_device_label(ane_used, gpu_used),
        fallback_reason = fallback_reason.as_deref().unwrap_or("none"),
        "CoreML runtime selection"
    );
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn run_coreml_boot_smoke(
    label: &str,
    kernels: &mut dyn FusedKernels,
    vocab_size: usize,
    input_len: usize,
) -> Result<()> {
    if vocab_size == 0 {
        return Err(AosError::Config(
            "CoreML boot smoke requires non-zero vocab_size".to_string(),
        ));
    }
    if input_len == 0 {
        return Err(AosError::Config(
            "CoreML boot smoke requires non-zero input_len".to_string(),
        ));
    }

    let ring = RouterRing::new(0);
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids = vec![0; input_len];

    kernels
        .run_step(&ring, &mut io)
        .map_err(|e| AosError::Kernel(format!("CoreML boot smoke ({label}) failed: {}", e)))?;

    info!(
        coreml_lane = label,
        vocab_size, input_len, "CoreML boot smoke inference completed"
    );

    Ok(())
}
