use super::backend::{is_mock_backend, parse_backend_choice, validate_backend_feature};
use super::cli::{is_prod_runtime, Args, EXIT_CONFIG_ERROR, EXIT_TRANSIENT_ERROR};
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use super::coreml::{
    compute_coreml_package_hash, coreml_telemetry_from_settings, log_coreml_runtime,
    log_coreml_verification_result, resolve_coreml_verify_mode, resolve_expected_coreml_hash,
    resolve_fusion_ids, run_coreml_boot_smoke, CoremlVerificationStatus,
};
use super::helpers::{
    build_capabilities_detail, detect_capabilities, dev_no_auth_enabled, mock_capabilities_detail,
    setup_mock_base_model_cache, setup_panic_hook, WorkerIdentity, WORKER_IDENTITY,
    WORKER_TELEMETRY,
};
use super::manifest::{cache_manifest, fetch_manifest_from_cp, parse_manifest, LoadedManifest};
use super::registration::{
    notify_cp_status, register_with_cp_with_retry, RegistrationParams, RegistrationResult,
};
use adapteros_boot::jti_cache::JtiCacheStore;
use adapteros_config::{
    prepare_socket_path, reject_tmp_persistent_path, resolve_telemetry_dir,
    resolve_worker_socket_for_worker,
};
use adapteros_core::{
    constants::DEFAULT_ADAPTER_CACHE_SIZE, rebase_var_path, resolve_var_dir,
    tokenizer_config::SpecialTokenMap, AosError, B3Hash, ExecutionProfile, Result, SeedMode,
    WorkerStatus,
};
use adapteros_lora_kernel_api::MockKernels;
use adapteros_lora_worker::{
    backend_coordinator::BackendCoordinator,
    backend_factory::{
        configure_model_cache_pinning, configure_model_cache_telemetry,
        create_backend_with_model_hashes, detect_capabilities as detect_backend_capabilities,
        get_model_cache, resolve_base_model_pin_budget_bytes, resolve_base_model_pin_enabled,
        select_backend_from_execution_profile, validate_model_cache_budget, BackendChoice,
        BaseModelPinConfig, SelectionContext,
    },
    health::{HealthEvent, HealthTick},
    inference_pause::InferencePauseRegistry,
    uds_server::UdsServer,
    CoordinatedKernels, CoremlRuntimeTelemetry, CoremlVerificationSnapshot, DirectKernels,
    HealthConfig, HealthMonitor, KernelWrapper, Worker,
};
use adapteros_telemetry::TelemetryWriter;
use clap::Parser;
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{fs, time::Duration};
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{error, info, info_span, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_db::{CreateCoremlFusionPairParams, Db};

fn log_boot_phase(phase: &'static str) {
    info!(phase, "Worker boot phase");
}

#[derive(Debug, Default, Deserialize)]
struct LoggingToml {
    #[serde(default)]
    logging: Option<LoggingTomlSection>,
}

#[derive(Debug, Default, Deserialize)]
struct LoggingTomlSection {
    level: Option<String>,
    log_dir: Option<String>,
    json_format: Option<bool>,
    rotation: Option<String>,
}

#[derive(Debug)]
struct WorkerLoggingSettings {
    level: String,
    log_dir: Option<std::path::PathBuf>,
    log_prefix: String,
    json_format: bool,
    rotation: tracing_appender::rolling::Rotation,
}

fn resolve_config_toml_path_for_logging() -> Result<Option<String>> {
    if let Ok(val) = std::env::var("AOS_CONFIG_TOML") {
        if !val.trim().is_empty() {
            let path = std::path::Path::new(&val);
            reject_tmp_persistent_path(path, "config-toml")?;
            return Ok(Some(val));
        }
    }

    let default_path = std::path::Path::new("configs/cp.toml");
    if default_path.exists() {
        reject_tmp_persistent_path(default_path, "config-toml")?;
        return Ok(Some(default_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

fn load_logging_toml_section() -> Result<Option<LoggingTomlSection>> {
    let toml_path = resolve_config_toml_path_for_logging()?;
    let Some(path) = toml_path else {
        return Ok(None);
    };

    let raw = fs::read_to_string(&path).map_err(|e| {
        AosError::Io(format!("Failed to read config TOML for logging at {}: {}", path, e))
    })?;

    let parsed: LoggingToml = toml::from_str(&raw).map_err(|e| {
        AosError::Config(format!(
            "Invalid TOML in {} while parsing [logging] section: {}",
            path, e
        ))
    })?;

    Ok(parsed.logging)
}

fn resolve_log_dir_from_env_or_config(
    logging: Option<&LoggingTomlSection>,
) -> Result<(Option<std::path::PathBuf>, Option<String>)> {
    if let Ok(raw) = std::env::var("AOS_LOG_FILE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = std::path::PathBuf::from(trimmed);
            let log_prefix = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            let log_dir = match path.extension() {
                Some(_) => path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .to_path_buf(),
                None => path,
            };
            let resolved = rebase_var_path(log_dir);
            reject_tmp_persistent_path(&resolved, "log-dir")?;
            return Ok((Some(resolved), log_prefix));
        }
    }

    if let Ok(raw) = std::env::var("AOS_LOG_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let resolved = rebase_var_path(trimmed);
            reject_tmp_persistent_path(&resolved, "log-dir")?;
            return Ok((Some(resolved), None));
        }
    }

    if let Some(logging) = logging {
        if let Some(raw) = logging.log_dir.as_ref() {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let resolved = rebase_var_path(trimmed);
                reject_tmp_persistent_path(&resolved, "log-dir")?;
                return Ok((Some(resolved), None));
            }
        }
    }

    Ok((None, None))
}

fn resolve_worker_logging_settings() -> Result<WorkerLoggingSettings> {
    let logging_toml = load_logging_toml_section()?;
    let (log_dir, log_prefix_override) =
        resolve_log_dir_from_env_or_config(logging_toml.as_ref())?;

    let default_level = "aos_worker=info,adapteros_lora_worker=info".to_string();
    let level = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var("AOS_LOG_LEVEL"))
        .ok()
        .or_else(|| logging_toml.as_ref().and_then(|cfg| cfg.level.clone()))
        .unwrap_or(default_level);

    let json_format = std::env::var("AOS_LOG_FORMAT")
        .ok()
        .map(|raw| raw.to_lowercase())
        .and_then(|raw| match raw.as_str() {
            "json" => Some(true),
            "text" | "pretty" => Some(false),
            _ => None,
        })
        .unwrap_or_else(|| logging_toml.as_ref().and_then(|cfg| cfg.json_format).unwrap_or(false));

    let rotation_raw = logging_toml
        .as_ref()
        .and_then(|cfg| cfg.rotation.clone())
        .unwrap_or_else(|| "daily".to_string());
    let rotation = match rotation_raw.as_str() {
        "hourly" => tracing_appender::rolling::Rotation::HOURLY,
        "daily" => tracing_appender::rolling::Rotation::DAILY,
        "never" => tracing_appender::rolling::Rotation::NEVER,
        _ => tracing_appender::rolling::Rotation::DAILY,
    };

    let log_prefix = log_prefix_override.unwrap_or_else(|| "aos-worker".to_string());

    Ok(WorkerLoggingSettings {
        level,
        log_dir,
        log_prefix,
        json_format,
        rotation,
    })
}

fn init_worker_logging() -> Result<Option<WorkerGuard>> {
    let settings = resolve_worker_logging_settings()?;

    let env_filter = EnvFilter::try_new(&settings.level).unwrap_or_else(|e| {
        eprintln!(
            "WARNING: Invalid log filter '{}': {}. Falling back to default worker log level.",
            settings.level, e
        );
        EnvFilter::new("aos_worker=info,adapteros_lora_worker=info")
    });

    let (file_layer, guard) = if let Some(ref log_dir) = settings.log_dir {
        std::fs::create_dir_all(log_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create log directory {}: {}",
                log_dir.display(),
                e
            ))
        })?;

        let file_appender = tracing_appender::rolling::RollingFileAppender::new(
            settings.rotation,
            log_dir,
            &settings.log_prefix,
        );
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let layer = if settings.json_format {
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_ansi(false)
                .boxed()
        } else {
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .boxed()
        };

        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    let console_layer = if settings.json_format {
        tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .boxed()
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    Ok(guard)
}

pub async fn run_worker() -> Result<()> {
    // Load canonical .env before any environment-based resolution
    adapteros_config::model::load_dotenv();

    // Initialize tracing
    let _log_guard = init_worker_logging()?;

    let args = Args::parse();

    // Early validation: check model cache budget BEFORE any expensive operations
    // This is a fail-fast check to avoid 100-200ms of wasted work (manifest loading,
    // backend selection) when the configuration is missing.
    info!("Validating model cache budget configuration...");
    let cache_budget_bytes = match validate_model_cache_budget() {
        Ok(bytes) => bytes,
        Err(e) => {
            error!(error = %e, "FATAL: Model cache budget not configured");
            eprintln!(
                "ERROR: Model cache budget not configured.\n\
             Set AOS_MODEL_CACHE_MAX_MB=<megabytes> environment variable\n\
             or model.cache.max.mb in the config TOML file."
            );
            // Exit before registration; no CP status notification is possible yet.
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    };
    info!("Model cache budget validated successfully");
    log_boot_phase("config-validated");

    let adapter_cache_bytes = match args.adapter_cache_bytes {
        Some(0) => {
            return Err(AosError::Validation(
                "Adapter cache budget bytes must be greater than zero".to_string(),
            ))
        }
        Some(bytes) => bytes,
        None => DEFAULT_ADAPTER_CACHE_SIZE,
    };
    info!(adapter_cache_bytes, "Adapter cache budget configured");

    // Set up panic hook for fatal error reporting
    let worker_id = args
        .worker_id
        .clone()
        .unwrap_or_else(|| format!("worker-{}", uuid::Uuid::now_v7()));

    // Store worker identity for panic hook access
    let _ = WORKER_IDENTITY.set(WorkerIdentity {
        worker_id: worker_id.clone(),
        cp_url: args.cp_url.clone(),
        tenant_id: args.tenant_id.clone(),
    });

    // Install panic hook for fatal error reporting
    setup_panic_hook();
    info!(worker_id = %worker_id, cp_url = %args.cp_url, "Panic hook installed for fatal error reporting");

    // Resolve UDS path with fallback logic and guard against tmp directories
    let resolved_uds = resolve_worker_socket_for_worker(&args.tenant_id, args.uds_path.as_deref())
        .map_err(|e| {
            error!(
                tenant_id = %args.tenant_id,
                uds_override = ?args.uds_path,
                error = %e,
                "Worker socket resolution failed"
            );
            e
        })?;
    let uds_path = resolved_uds.path.clone();
    prepare_socket_path(&uds_path, "worker").map_err(|e| {
        error!(
            tenant_id = %args.tenant_id,
            uds_path = %uds_path.display(),
            error = %e,
            "Failed to prepare worker socket path"
        );
        e
    })?;

    info!(
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        uds_path = %uds_path.display(),
        uds_source = %resolved_uds.source,
        "Starting aos-worker"
    );

    // Resolve model and tokenizer paths
    let model_path = match &args.model_path {
        Some(path) => path.clone(),
        None => adapteros_config::get_model_path_with_fallback()?,
    };
    reject_tmp_persistent_path(&model_path, "model-path")?;
    if !model_path.exists() {
        return Err(AosError::Validation(format!(
            "Model path does not exist: {}",
            model_path.display()
        )));
    }

    // Resolve tokenizer via canonical discovery (CLI arg > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())?;

    // Resolve manifest content (hash-first)
    let expected_manifest_hash = args
        .manifest_hash
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|h| B3Hash::from_hex(h).map_err(|e| AosError::Validation(e.to_string())))
        .transpose()?;

    if let Some(path) = args.manifest.as_ref() {
        reject_tmp_persistent_path(path, "worker-manifest")?;
    }

    let loaded_manifest = if let Some(expected_hash) = expected_manifest_hash {
        if let Some(path) = args.manifest.as_ref() {
            if !path.exists() {
                return Err(AosError::Validation(format!(
                    "Manifest file not found at {}",
                    path.display()
                )));
            }
            let manifest_raw = fs::read_to_string(path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read manifest at {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let manifest = parse_manifest(&manifest_raw)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        } else {
            info!(
                manifest_hash = %expected_hash.to_hex(),
                cp_url = %args.cp_url,
                tenant_id = %args.tenant_id,
                "Fetching manifest from control plane"
            );
            let manifest_json =
                fetch_manifest_from_cp(&args.cp_url, &args.tenant_id, &expected_hash)?;
            let manifest = parse_manifest(&manifest_json)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch after fetch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        }
    } else {
        let path = args.manifest.as_ref().ok_or_else(|| {
            AosError::Validation(
                "Manifest hash not provided. Supply --manifest-hash/AOS_MANIFEST_HASH or --manifest/AOS_WORKER_MANIFEST"
                    .to_string(),
            )
        })?;
        if !path.exists() {
            return Err(AosError::Validation(format!(
                "Manifest file not found at {}",
                path.display()
            )));
        }
        let manifest_raw = fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read manifest at {}: {}",
                path.display(),
                e
            ))
        })?;
        let manifest = parse_manifest(&manifest_raw)?;
        let computed_hash = manifest.compute_hash()?;
        let canonical_json = manifest
            .to_json()
            .map_err(|e| AosError::Validation(format!("Failed to canonicalize manifest: {}", e)))?;
        cache_manifest(&computed_hash, &canonical_json);
        LoadedManifest {
            manifest,
            _canonical_json: canonical_json,
            hash: computed_hash,
        }
    };

    let manifest = loaded_manifest.manifest;

    let pin_enabled = if args.pin_base_model {
        true
    } else {
        resolve_base_model_pin_enabled()?
    };
    if let Some(0) = args.pin_budget_bytes {
        return Err(AosError::Validation(
            "Pin budget bytes must be greater than zero".to_string(),
        ));
    }
    let mut pin_budget_bytes = if let Some(bytes) = args.pin_budget_bytes {
        Some(bytes)
    } else {
        resolve_base_model_pin_budget_bytes()?
    };
    if is_prod_runtime() {
        if !pin_enabled {
            return Err(AosError::Config(
                "Production runtime requires --pin-base-model or AOS_PIN_BASE_MODEL=true"
                    .to_string(),
            ));
        }
        if pin_budget_bytes.is_none() {
            return Err(AosError::Config(
                "Production runtime requires --pin-budget-bytes or AOS_PIN_BUDGET_BYTES"
                    .to_string(),
            ));
        }
    }
    if pin_enabled && pin_budget_bytes.is_none() {
        pin_budget_bytes = Some(cache_budget_bytes);
        info!(
            pin_budget_bytes,
            "Base model pin budget not set; defaulting to model cache budget"
        );
    } else if !pin_enabled && pin_budget_bytes.is_some() {
        warn!("Pin budget configured but base model pinning is disabled; ignoring pin budget");
    }

    let base_model_id = manifest.base.model_id.clone();
    configure_model_cache_pinning(BaseModelPinConfig {
        enabled: pin_enabled,
        budget_bytes: pin_budget_bytes,
        model_id: Some(base_model_id.clone()),
    })?;
    info!(
        pin_enabled,
        pin_budget_bytes,
        model_id = %base_model_id,
        "Base model pinning configured"
    );
    let manifest_hash = loaded_manifest.hash;

    info!(
        model_id = %manifest.base.model_id,
        manifest_hash = %manifest_hash.to_hex(),
        k_sparse = manifest.router.k_sparse,
        "Manifest loaded and verified"
    );
    log_boot_phase("manifest-loaded");
    let model_hash_hex = manifest.base.model_hash.to_hex();
    let tokenizer_hash_hex = manifest.base.tokenizer_hash.to_hex();

    // Validate tokenizer early to avoid late MLX failures and enforce manifest fidelity.
    // Retry lightly on transient I/O errors (e.g., slow network FS) but fail-fast on schema/logic issues.
    let tokenizer_meta = {
        let mut attempt = 0;
        let mut last_err: Option<AosError> = None;
        loop {
            attempt += 1;
            match SpecialTokenMap::validate_tokenizer(
                &tokenizer_path,
                Some(manifest.base.vocab_size as usize),
            ) {
                Ok(meta) => break meta,
                Err(e) => {
                    let msg = format!("{}", e);
                    let fatal = msg.contains("vocab_size")
                        || msg.contains("hash mismatch")
                        || msg.contains("missing model")
                        || msg.contains("invalid JSON");
                    if fatal || attempt >= 3 {
                        error!(
                            attempt,
                            fatal,
                            error = %e,
                            path = %tokenizer_path.display(),
                            "Tokenizer validation failed"
                        );
                        return Err(e);
                    }
                    warn!(
                        attempt,
                        error = %e,
                        "Tokenizer validation transient error; retrying with backoff"
                    );
                    tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    last_err = Some(e);
                }
            }
        }
    };

    if tokenizer_meta.hash != manifest.base.tokenizer_hash {
        return Err(AosError::Validation(format!(
            "Tokenizer hash mismatch: manifest {} != computed {}",
            manifest.base.tokenizer_hash.to_hex(),
            tokenizer_meta.hash.to_hex()
        )));
    }

    info!(
        vocab_size = tokenizer_meta.vocab_size,
        added_tokens = tokenizer_meta.added_tokens,
        normalizer = ?tokenizer_meta.normalizer,
        tokenizer_hash = %manifest.base.tokenizer_hash.to_hex(),
        "Tokenizer validated and schema checked"
    );

    let mock_backend = is_mock_backend(&args.backend);
    let prod_runtime = is_prod_runtime();
    if mock_backend && !cfg!(debug_assertions) {
        return Err(AosError::Config(
            "Mock backend is only allowed in debug builds".to_string(),
        ));
    }

    let backend_choice;
    let kernels;
    let available_backends;

    if mock_backend {
        info!("Mock backend requested; using MockKernels for worker");
        setup_mock_base_model_cache(&manifest_hash, cache_budget_bytes)?;

        backend_choice = BackendChoice::CPU;
        kernels = KernelWrapper::Direct(DirectKernels::new(Box::new(MockKernels::new())));
        available_backends = adapteros_lora_worker::AvailableBackends {
            primary: backend_choice,
            fallback: None,
            coreml_primary: None,
            coreml_fallback: None,
        };
    } else {
        // Select backend (ExecutionProfile is the canonical source)
        let requested_backend = parse_backend_choice(&args.backend);
        validate_backend_feature(&requested_backend)?;

        let capabilities = detect_backend_capabilities();
        let exec_profile = ExecutionProfile {
            seed_mode: SeedMode::BestEffort,
            backend_profile: requested_backend,
            require_explicit_fallback_opt_out: false,
        };
        let selection = select_backend_from_execution_profile(&SelectionContext::new(
            exec_profile,
            capabilities.clone(),
        ))?;
        info!(
            requested = %requested_backend.as_str(),
            selected = %selection.selected.as_str(),
            overridden = selection.overridden,
            reason = selection.reason.unwrap_or("none"),
            "Resolved backend selection at worker startup"
        );
        if selection.overridden {
            info!(
                requested = %requested_backend.as_str(),
                selected = %selection.selected.as_str(),
                reason = ?selection.reason,
                "Backend request overridden based on capabilities"
            );
        }
        let backend_choice_local = selection.selected;
        if prod_runtime && backend_choice_local == BackendChoice::MlxBridge {
            return Err(AosError::Config(
                "Production runtime forbids MLX bridge backend selection".to_string(),
            ));
        }

        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        let coreml_primary_settings = if backend_choice_local == BackendChoice::CoreML {
            Some(adapteros_lora_worker::backend_factory::resolve_coreml_backend_settings())
        } else {
            None
        };
        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        let coreml_primary_runtime = coreml_primary_settings
            .as_ref()
            .map(coreml_telemetry_from_settings);
        #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
        let coreml_primary_runtime: Option<CoremlRuntimeTelemetry> = None;
        #[allow(unused_mut)]
        let mut fallback_coreml_runtime: Option<CoremlRuntimeTelemetry> = None;

        // NOTE: Model cache budget validation moved to startup (line ~952) for fail-fast behavior.
        // The budget is validated before any expensive operations like manifest loading.

        // Create kernel backend with manifest hash for cache identity and model hash for integrity verification
        info!(
            backend = %backend_choice_local.as_str(),
            manifest_hash = %manifest_hash.to_hex(),
            model_hash = %manifest.base.model_hash.to_hex(),
            "Creating kernel backend with integrity verification"
        );
        #[allow(unused_mut)]
        let mut primary_kernels = create_backend_with_model_hashes(
            backend_choice_local,
            &model_path,
            Some(&manifest_hash),
            Some(&manifest.base.model_hash),
        )
        .map_err(|e| {
            if backend_choice_local == BackendChoice::CoreML {
                let class = adapteros_lora_worker::backend_factory::classify_coreml_error(&e);
                error!(
                    coreml_failure_stage = class.stage,
                    coreml_failure_reason = class.reason,
                    error = %e,
                    "CoreML backend initialization failed"
                );
            }
            e
        })?;
        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        if let Some(settings) = coreml_primary_settings.as_ref() {
            log_coreml_runtime("primary", settings);
            run_coreml_boot_smoke(
                "primary",
                primary_kernels.as_mut(),
                manifest.base.vocab_size as usize,
                manifest.router.sample_tokens_full,
            )?;
        }

        // Optional fallback backend via coordinator
        let mut fallback_backend_kind: Option<BackendChoice> = None;
        #[allow(unused_mut)]
        let mut fallback_kernels = if args.coordinator_enabled {
            match BackendCoordinator::select_fallback_backend(&backend_choice_local, &capabilities)
            {
                Ok(choice) => {
                    if prod_runtime && choice == BackendChoice::MlxBridge {
                        return Err(AosError::Config(
                            "Production runtime forbids MLX bridge fallback backend".to_string(),
                        ));
                    }
                    match create_backend_with_model_hashes(
                        choice,
                        &model_path,
                        Some(&manifest_hash),
                        Some(&manifest.base.model_hash),
                    ) {
                        #[allow(unused_mut)]
                        Ok(mut k) => {
                            if choice == BackendChoice::CoreML {
                                #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
                                {
                                    let settings =
                                        adapteros_lora_worker::backend_factory::resolve_coreml_backend_settings();
                                    log_coreml_runtime("fallback", &settings);
                                    run_coreml_boot_smoke(
                                        "fallback",
                                        k.as_mut(),
                                        manifest.base.vocab_size as usize,
                                        manifest.router.sample_tokens_full,
                                    )?;
                                    fallback_coreml_runtime =
                                        Some(coreml_telemetry_from_settings(&settings));
                                }
                            }
                            info!(fallback_backend = ?choice, "Created fallback backend");
                            fallback_backend_kind = Some(choice);
                            Some(k)
                        }
                        Err(e) => {
                            if choice == BackendChoice::CoreML {
                                let class =
                                    adapteros_lora_worker::backend_factory::classify_coreml_error(
                                        &e,
                                    );
                                error!(
                                    coreml_failure_stage = class.stage,
                                    coreml_failure_reason = class.reason,
                                    error = %e,
                                    "CoreML fallback backend initialization failed"
                                );
                            }
                            warn!(error = %e, "Failed to create fallback backend, continuing without fallback");
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "No suitable fallback backend available, continuing without fallback");
                    None
                }
            }
        } else {
            None
        };

        let kernels_local = if args.coordinator_enabled {
            KernelWrapper::Coordinated(CoordinatedKernels::new(primary_kernels, fallback_kernels))
        } else {
            KernelWrapper::Direct(DirectKernels::new(primary_kernels))
        };

        let available_backends_local = adapteros_lora_worker::AvailableBackends {
            primary: backend_choice_local,
            fallback: fallback_backend_kind,
            coreml_primary: coreml_primary_runtime,
            coreml_fallback: fallback_coreml_runtime,
        };

        backend_choice = backend_choice_local;
        kernels = kernels_local;
        available_backends = available_backends_local;
    }

    // Compute and verify CoreML fused package hash when CoreML is in play.
    #[allow(unused_variables)]
    let coreml_in_use = backend_choice == BackendChoice::CoreML
        || matches!(available_backends.fallback, Some(BackendChoice::CoreML));

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    let (coreml_package_hash_hex, coreml_verification) = if coreml_in_use {
        let coreml_db = match Db::connect_env().await {
            Ok(db) => Some(db),
            Err(e) => {
                warn!(
                    error = %e,
                    "CoreML verification DB unavailable; continuing without registry lookup"
                );
                None
            }
        };

        let actual_hash = match compute_coreml_package_hash(&model_path) {
            Ok(hash) => Some(hash),
            Err(e) => {
                warn!(error = %e, "Failed to compute CoreML package hash");
                None
            }
        };
        let (expected_hash, expected_source) = resolve_expected_coreml_hash(
            &manifest,
            &model_path,
            &args.tenant_id,
            coreml_db.as_ref(),
        )
        .await;
        let mode = resolve_coreml_verify_mode();
        let status = log_coreml_verification_result(
            mode,
            expected_hash.as_ref(),
            actual_hash.as_ref(),
            expected_source.as_deref(),
        )?;

        let verification_snapshot = CoremlVerificationSnapshot {
            mode: Some(format!("{:?}", mode).to_lowercase()),
            expected: expected_hash.as_ref().map(|h| h.to_hex()),
            actual: actual_hash.as_ref().map(|h| h.to_hex()),
            source: expected_source.clone(),
            status: Some(status.as_str().to_string()),
            mismatch: status.is_mismatch(),
        };

        if status == CoremlVerificationStatus::Match {
            if let (Some(db), Some(actual_hex)) =
                (coreml_db.as_ref(), actual_hash.as_ref().map(|h| h.to_hex()))
            {
                let (base_model_id, adapter_id) = resolve_fusion_ids(&manifest);
                if let (Some(base_id), Some(adapter_id)) = (base_model_id, adapter_id) {
                    let params = CreateCoremlFusionPairParams {
                        tenant_id: args.tenant_id.clone(),
                        base_model_id: base_id,
                        adapter_id,
                        fused_manifest_hash: actual_hex.clone(),
                        coreml_package_hash: actual_hex.clone(),
                        adapter_hash_b3: manifest
                            .fusion
                            .as_ref()
                            .and_then(|f| f.adapter_hash)
                            .map(|h| h.to_hex()),
                        base_model_hash_b3: manifest
                            .fusion
                            .as_ref()
                            .and_then(|f| f.base_model_hash)
                            .map(|h| h.to_hex()),
                        metadata_path: None,
                    };
                    if let Err(e) = db.upsert_coreml_fusion_pair(params).await {
                        warn!(
                            error = %e,
                            "Failed to upsert CoreML fusion pair after verification"
                        );
                    }
                }
            }
        }

        (actual_hash.map(|h| h.to_hex()), Some(verification_snapshot))
    } else {
        (None, None)
    };
    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    let (coreml_package_hash_hex, coreml_verification): (
        Option<String>,
        Option<CoremlVerificationSnapshot>,
    ) = (None, None);

    // Create telemetry writer - use env var or var/telemetry
    let resolved_telemetry = resolve_telemetry_dir()?;
    if let Err(e) = std::fs::create_dir_all(&resolved_telemetry.path) {
        warn!(
            error = %e,
            path = %resolved_telemetry.path.display(),
            source = %resolved_telemetry.source,
            "Failed to create telemetry directory; continuing"
        );
    }
    let telemetry =
        TelemetryWriter::new(&resolved_telemetry.path, 10000, 100_000_000).map_err(|e| {
            adapteros_core::AosError::Worker(format!("Failed to create telemetry writer: {}", e))
        })?;
    let _ = WORKER_TELEMETRY.set(telemetry.clone());
    info!(
        path = %resolved_telemetry.path.display(),
        source = %resolved_telemetry.source,
        "Telemetry writer initialized"
    );
    configure_model_cache_telemetry(telemetry.clone());

    // Track lifecycle locally for state validation
    let mut lifecycle = WorkerStatus::Created;
    let backend_label = if mock_backend {
        "mock"
    } else {
        backend_choice.as_str()
    };
    let cp_enabled = !dev_no_auth_enabled();
    if !cp_enabled {
        warn!("Dev no-auth enabled; skipping control plane registration and status updates");
    }

    // Register with control plane before UDS bind to acquire quotas and publish the socket path.
    let capabilities = detect_capabilities(backend_label);
    let capabilities_detail = if mock_backend {
        mock_capabilities_detail()
    } else {
        build_capabilities_detail(backend_choice)
    };
    let uds_path_str = uds_path.to_string_lossy().to_string();

    let manifest_hash_hex = manifest_hash.to_hex();
    if cp_enabled {
        log_boot_phase("cp-register");
        info!(
            worker_id = %worker_id,
            tenant_id = %args.tenant_id,
            plan_id = %args.plan_id,
            manifest_hash = %manifest_hash_hex,
            backend = %backend_label,
            model_hash = %model_hash_hex,
            "Registering with control plane"
        );
    }

    let registration_result = if cp_enabled {
        match register_with_cp_with_retry(&RegistrationParams {
            cp_url: &args.cp_url,
            worker_id: &worker_id,
            tenant_id: &args.tenant_id,
            plan_id: &args.plan_id,
            manifest_hash: &manifest_hash_hex,
            backend: backend_label,
            model_hash: &model_hash_hex,
            tokenizer_hash_b3: &manifest.base.tokenizer_hash.to_hex(),
            tokenizer_vocab_size: manifest.base.vocab_size,
            uds_path: &uds_path_str,
            capabilities: &capabilities,
            capabilities_detail: &capabilities_detail,
            strict_mode: args.strict,
        }) {
            Ok(result) => {
                lifecycle = lifecycle
                    .transition_to(WorkerStatus::Registered)
                    .map_err(|e| AosError::Lifecycle(e.to_string()))?;
                notify_cp_status(
                    &args.cp_url,
                    &worker_id,
                    WorkerStatus::Registered.as_str(),
                    "registration-accepted",
                    &args.backend,
                    &model_hash_hex,
                    &manifest_hash_hex,
                    &manifest.base.tokenizer_hash.to_hex(),
                    manifest.base.vocab_size,
                );
                log_boot_phase("cp-registered");
                info!(
                    heartbeat_interval = result.heartbeat_interval_secs,
                    kv_quota_bytes = ?result.kv_quota_bytes,
                    kv_residency_policy_id = ?result.kv_residency_policy_id,
                    "Worker registration accepted by control plane"
                );
                result
            }
            Err(reason) => {
                let _lifecycle = lifecycle
                    .transition_to(WorkerStatus::Error)
                    .unwrap_or(lifecycle);
                // Registration failed after CP contact; always notify before exiting.
                notify_cp_status(
                    &args.cp_url,
                    &worker_id,
                    WorkerStatus::Error.as_str(),
                    "registration-failed",
                    &args.backend,
                    &model_hash_hex,
                    &manifest_hash_hex,
                    &manifest.base.tokenizer_hash.to_hex(),
                    manifest.base.vocab_size,
                );
                error!(reason = %reason, "Worker registration failed - exiting");
                return Err(AosError::Worker(format!("Registration failed: {}", reason)));
            }
        }
    } else {
        lifecycle = lifecycle
            .transition_to(WorkerStatus::Registered)
            .map_err(|e| AosError::Lifecycle(e.to_string()))?;
        log_boot_phase("cp-skipped");
        RegistrationResult {
            heartbeat_interval_secs: 30,
            kv_quota_bytes: None,
            kv_residency_policy_id: None,
        }
    };

    log_boot_phase("registered");

    let notify_cp_error = |reason: &str| {
        if cp_enabled {
            notify_cp_status(
                &args.cp_url,
                &worker_id,
                WorkerStatus::Error.as_str(),
                reason,
                &args.backend,
                &model_hash_hex,
                &manifest_hash_hex,
                &tokenizer_hash_hex,
                manifest.base.vocab_size,
            );
        }
    };

    // Create KV quota manager from registration response
    let quota_manager = Arc::new(adapteros_lora_worker::TenantKvQuotaManager::new(
        args.tenant_id.clone(),
        registration_result.kv_quota_bytes,
    ));

    info!(
        tenant_id = %args.tenant_id,
        kv_quota_bytes = ?registration_result.kv_quota_bytes,
        kv_residency_policy_id = ?registration_result.kv_residency_policy_id,
        quota_enforced = quota_manager.is_quota_enforced(),
        "KV quota manager initialized"
    );

    // Create worker with quota manager
    info!("Creating worker instance");

    // Fail fast on non-UTF8 paths rather than silently coercing to "".
    // Determinism expectation: invalid configuration must error, not change behavior.
    let tokenizer_path_str = tokenizer_path.to_str().ok_or_else(|| {
        AosError::Validation(format!(
            "Tokenizer path is not valid UTF-8: {:?} (display: {})",
            tokenizer_path,
            tokenizer_path.display()
        ))
    })?;
    let model_path_str = model_path.to_str().ok_or_else(|| {
        AosError::Validation(format!(
            "Model path is not valid UTF-8: {:?} (display: {})",
            model_path,
            model_path.display()
        ))
    })?;

    // PRD-06: Compute worker_id as u32 from BLAKE3 hash for deterministic identity binding
    // Using BLAKE3 ensures stability across Rust versions (unlike DefaultHasher)
    let worker_id_u32 = {
        let hash = adapteros_core::B3Hash::hash(worker_id.as_bytes());
        let bytes: [u8; 4] = hash.as_bytes()[0..4].try_into().unwrap_or([0; 4]);
        u32::from_le_bytes(bytes)
    };

    let worker = Worker::new(
        manifest.clone(),
        &args.tenant_id,
        kernels,
        available_backends,
        None, // No RAG system for now
        tokenizer_path_str,
        model_path_str,
        telemetry,
        coreml_package_hash_hex.clone(),
        coreml_verification.clone(),
        Some(quota_manager),
        registration_result.kv_residency_policy_id.clone(),
        Some(adapter_cache_bytes),
        worker_id_u32,
    )
    .await
    .map_err(|e| {
        notify_cp_error("worker-init-failed");
        e
    })?;
    log_boot_phase("worker-created");

    if let Ok(cache) = get_model_cache() {
        let pin_state = cache.base_model_pin_state();
        let pinned = match pin_state.base_model_key.as_ref() {
            Some(key) => cache.is_pinned(key),
            None => pin_state.enabled,
        };
        let rss_bytes = worker.get_memory_usage_bytes();

        if let Some(telemetry) = worker.telemetry().clone() {
            let model_id = pin_state.model_id.unwrap_or_else(|| base_model_id.clone());
            let _ = telemetry.log(
                "model.residency",
                serde_json::json!({
                    "model_id": model_id,
                    "pinned": pinned,
                    "load_count": pin_state.load_count,
                    "evict_count": pin_state.evict_count,
                    "rss_bytes": rss_bytes,
                }),
            );
        }
    }

    // Wire inference pause registry for human-in-the-loop review protocol
    let pause_registry = Arc::new(InferencePauseRegistry::new());
    let worker = worker.with_pause_registry(pause_registry);

    let worker = Arc::new(Mutex::new(worker));
    let drain_flag = Arc::new(AtomicBool::new(false));

    let heartbeat_interval = registration_result.heartbeat_interval_secs;

    // Align health monitoring interval with control plane heartbeat expectation
    {
        let mut guard = worker.lock().await;
        let telemetry_for_monitor = guard.telemetry().clone();
        // Allow configuring max memory growth for large models (default 8GB for 4-bit quantized models)
        let max_memory_growth = std::env::var("AOS_MAX_MEMORY_GROWTH_BYTES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(8 * 1024 * 1024 * 1024); // 8GB default
        let config = HealthConfig {
            check_interval: std::time::Duration::from_secs(heartbeat_interval as u64),
            max_memory_growth,
            ..Default::default()
        };
        let monitor = if let Some(t) = telemetry_for_monitor {
            HealthMonitor::new(config)
                .map_err(|e| {
                    notify_cp_error("health-monitor-init-failed");
                    e
                })?
                .with_telemetry(t, args.tenant_id.clone(), worker_id.clone())
        } else {
            HealthMonitor::new(config).map_err(|e| {
                notify_cp_error("health-monitor-init-failed");
                e
            })?
        };
        guard.set_health_monitor(Arc::new(monitor));
    }

    // Start UDS server after registration (bind before marking healthy)
    // Try to load worker verifying key for CP->Worker authentication
    // In strict mode, we use retry with exponential backoff (worker may start before CP generates keypair).
    // In non-strict mode, we try once and fall back to no authentication if key is missing.
    const KEY_LOAD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

    let keys_dir = resolve_var_dir().join("keys");
    let keys_dir_str = keys_dir.to_string_lossy();
    let worker_verifying_key = if args.strict {
        // Strict mode: use retry with deadline, then fail with transient error code
        match adapteros_boot::load_worker_public_key_with_retry(
            keys_dir_str.as_ref(),
            KEY_LOAD_DEADLINE,
        ) {
            Ok(key) => {
                info!("Worker public key loaded for CP->Worker authentication");
                Some(key)
            }
            Err(e) => {
                error!(
                    error = %e,
                    deadline_secs = KEY_LOAD_DEADLINE.as_secs(),
                    "STRICT MODE: Failed to load worker public key after retry"
                );
                notify_cp_error("worker-key-load-failed");
                // Use transient error code so orchestrator will retry
                std::process::exit(EXIT_TRANSIENT_ERROR);
            }
        }
    } else {
        // Non-strict mode: try once, fall back to no auth if missing
        match adapteros_boot::load_worker_public_key(keys_dir_str.as_ref()) {
            Ok(key) => {
                info!("Worker public key loaded for CP->Worker authentication");
                Some(key)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Worker public key not found, running without CP->Worker token validation"
                );
                None
            }
        }
    };

    // Initialize persistent JTI cache for replay defense (only when auth is enabled)
    // The cache is loaded from disk on startup and persisted on shutdown.
    let jti_cache = if worker_verifying_key.is_some() {
        let jti_cache_path = keys_dir.join("jti_cache.json");
        let cache = JtiCacheStore::load_or_new(jti_cache_path);
        info!(
            entries = cache.len(),
            capacity = cache.capacity(),
            "JTI cache initialized for replay defense"
        );
        Some(Arc::new(Mutex::new(cache)))
    } else {
        None
    };

    let inference_cancellations = {
        let guard = worker.lock().await;
        guard.inference_cancel_registry()
    };

    info!(uds_path = %uds_path.display(), "Starting UDS server");
    log_boot_phase("uds-bind");
    let server = if let Some(verifying_key) = worker_verifying_key {
        let jti_cache = jti_cache.expect("JTI cache should be initialized when auth is enabled");
        UdsServer::new_with_worker_auth(
            uds_path.clone(),
            worker.clone(),
            inference_cancellations.clone(),
            None,
            drain_flag.clone(),
            verifying_key,
            worker_id.clone(),
            jti_cache,
        )
    } else {
        // In non-strict mode, this is allowed
        UdsServer::new(
            uds_path.clone(),
            worker.clone(),
            inference_cancellations.clone(),
            None,
            drain_flag.clone(),
        )
    };
    let listener = match server.bind().await {
        Ok(listener) => listener,
        Err(e) => {
            // Report bind failure after registration so CP will not consider this worker live.
            notify_cp_error("uds-bind-failed");
            return Err(e);
        }
    };

    lifecycle = lifecycle
        .transition_to(WorkerStatus::Healthy)
        .map_err(|e| AosError::Lifecycle(e.to_string()))?;
    if cp_enabled {
        // Publish healthy after UDS bind so CP only routes to a listening socket.
        notify_cp_status(
            &args.cp_url,
            &worker_id,
            WorkerStatus::Healthy.as_str(),
            "uds-listening",
            &args.backend,
            &model_hash_hex,
            &manifest_hash_hex,
            &tokenizer_hash_hex,
            manifest.base.vocab_size,
        );
    }
    log_boot_phase("uds-listening");

    // Spawn health monitor loop with telemetry + shutdown hook
    let (health_monitor, telemetry_for_health) = {
        let guard = worker.lock().await;
        (guard.health_monitor(), guard.telemetry().clone())
    };
    let health_monitor_for_task = health_monitor.clone();
    let cp_url_health = args.cp_url.clone();
    let worker_id_health = worker_id.clone();
    let backend_health = args.backend.clone();
    let model_hash_health = model_hash_hex.clone();
    let manifest_hash_health = manifest_hash_hex.clone();
    let tokenizer_hash_health = tokenizer_hash_hex.clone();
    let tokenizer_vocab_health = manifest.base.vocab_size;
    let drain_flag_health = drain_flag.clone();
    let health_monitor_handle = tokio::spawn(async move {
        if let Err(e) = health_monitor_for_task
            .start_monitoring_with_hook(|monitor, tick| {
                if let Some(t) = telemetry_for_health.as_ref() {
                    if let HealthTick::Status { status, .. } = &tick {
                        if let Ok(event) = HealthEvent::from_monitor(monitor, status) {
                            let _ = t.log("worker_health", event);
                        }
                    }
                }

                if matches!(tick, HealthTick::Shutdown { .. }) {
                    if cp_enabled {
                        // Health-triggered shutdown must notify CP to stop routing work.
                        notify_cp_status(
                            &cp_url_health,
                            &worker_id_health,
                            WorkerStatus::Error.as_str(),
                            "health-shutdown",
                            &backend_health,
                            &model_hash_health,
                            &manifest_hash_health,
                            &tokenizer_hash_health,
                            tokenizer_vocab_health,
                        );
                    }
                    drain_flag_health.store(true, Ordering::Relaxed);
                }

                Ok(())
            })
            .await
        {
            warn!(error = %e, "Health monitor exited with error");
        }
    });

    let serve_span = info_span!(
        "worker_serve",
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        backend = %args.backend,
        manifest_hash = %manifest_hash_hex,
        uds_path = %uds_path_str,
        coordinator_enabled = args.coordinator_enabled,
    );
    let _serve_span_guard = serve_span.enter();

    // Run server with drain handling
    let shutdown_signal = signal::ctrl_c();
    tokio::pin!(shutdown_signal);
    let serve_fut = server.serve_with_listener(listener);
    tokio::pin!(serve_fut);
    let mut shutdown_signal_received = false;
    let serve_result = tokio::select! {
        res = &mut serve_fut => res,
        _ = &mut shutdown_signal => {
            shutdown_signal_received = true;
            info!(worker_id = %worker_id, "Drain signal received, initiating worker drain");

            // Persist JTI cache before shutdown to maintain replay defense across restarts
            if let Err(e) = server.persist_jti_cache().await {
                warn!(error = %e, "Failed to persist JTI cache during shutdown");
            }

            // Cleanup model cache during drain to free pinned entries
            if let Ok(cache) = get_model_cache() {
                info!("Cleaning up model cache before drain");
                cache.cleanup_all();
            }

            drain_flag.store(true, Ordering::Relaxed);
            lifecycle = lifecycle.transition_to(WorkerStatus::Draining)
                .map_err(|e| AosError::Lifecycle(e.to_string()))?;
            if cp_enabled {
                // Notify CP of drain so it can stop routing new requests before shutdown.
                notify_cp_status(
                    &args.cp_url,
                    &worker_id,
                    WorkerStatus::Draining.as_str(),
                    "drain-signal",
                    &args.backend,
                    &model_hash_hex,
                    &manifest_hash_hex,
                    &tokenizer_hash_hex,
                    manifest.base.vocab_size,
                );
            }
            serve_fut.await
        }
    };

    let final_status = if health_monitor.is_shutdown_requested() {
        WorkerStatus::Error
    } else {
        WorkerStatus::Stopped
    };

    {
        let mut guard = worker.lock().await;
        if let Err(e) = guard.shutdown().await {
            warn!(error = %e, "Worker shutdown reported an error");
        }
    }

    join_task_with_timeout(
        "health_monitor",
        health_monitor_handle,
        Duration::from_secs(5),
    )
    .await;

    if let Err(e) = serve_result {
        if shutdown_signal_received {
            warn!(error = %e, "UDS server returned an error during shutdown");
        } else {
            notify_cp_error("uds-serve-failed");
            return Err(e);
        }
    }

    // Notify stopped (or error if health triggered shutdown) on clean exit
    let _lifecycle = lifecycle
        .transition_to(final_status)
        .map_err(|e| AosError::Lifecycle(e.to_string()))?;
    if cp_enabled {
        // Final status acts as the worker unregister/terminal signal for the control plane.
        notify_cp_status(
            &args.cp_url,
            &worker_id,
            final_status.as_str(),
            if final_status == WorkerStatus::Error {
                "health-shutdown"
            } else {
                "clean shutdown"
            },
            &args.backend,
            &model_hash_hex,
            &manifest_hash_hex,
            &tokenizer_hash_hex,
            manifest.base.vocab_size,
        );
    }

    Ok(())
}

async fn join_task_with_timeout(
    name: &str,
    mut handle: tokio::task::JoinHandle<()>,
    timeout: Duration,
) {
    tokio::select! {
        res = &mut handle => {
            if let Err(e) = res {
                warn!(task = name, error = %e, "Shutdown task failed");
            }
        }
        _ = tokio::time::sleep(timeout) => {
            warn!(
                task = name,
                timeout_ms = timeout.as_millis() as u64,
                "Shutdown task timed out; aborting"
            );
            handle.abort();
            if let Err(e) = handle.await {
                warn!(task = name, error = %e, "Shutdown task abort failed");
            }
        }
    }
}
