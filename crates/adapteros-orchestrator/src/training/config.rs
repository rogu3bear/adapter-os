//! Training configuration and backend mapping utilities.

use adapteros_core::backend::BackendKind;
use adapteros_lora_worker::training::TrainingBackend as WorkerTrainingBackend;
use adapteros_types::training::TrainingBackendKind;
use tracing::warn;

/// Post-actions configuration parsed from JSON
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostActions {
    /// Package adapter after training (default: true)
    #[serde(default = "default_true")]
    pub package: bool,
    /// Register adapter in registry after packaging (default: true)
    #[serde(default = "default_true")]
    pub register: bool,
    /// Create a new stack with the adapter after registration (default: true).
    #[serde(default = "default_true")]
    pub create_stack: bool,
    /// Activate the stack after creation (default: false).
    /// If true, sets the created stack as the tenant's default stack.
    /// WARNING: This changes the tenant's active inference behavior immediately.
    #[serde(default = "default_false")]
    pub activate_stack: bool,
    /// Tier to assign: persistent, warm, ephemeral (default: warm)
    #[serde(default = "default_tier")]
    pub tier: String,
    /// Custom adapters root directory (optional)
    pub adapters_root: Option<String>,
}

impl Default for PostActions {
    fn default() -> Self {
        Self {
            package: true,
            register: true,
            create_stack: true,
            activate_stack: false,
            tier: default_tier(),
            adapters_root: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_tier() -> String {
    "warm".to_string()
}

/// Convert TrainingBackendKind to core BackendKind
pub(crate) fn to_core_backend(kind: TrainingBackendKind) -> BackendKind {
    match kind {
        TrainingBackendKind::Auto => BackendKind::Auto,
        TrainingBackendKind::CoreML => BackendKind::CoreML,
        TrainingBackendKind::Mlx => BackendKind::Mlx,
        TrainingBackendKind::Metal => BackendKind::Metal,
        TrainingBackendKind::Cpu => BackendKind::CPU,
    }
}

/// Preferred backend mapping for worker config (preserves CoreML intent + fallback)
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PreferredBackendSelection {
    pub preferred: Option<WorkerTrainingBackend>,
    pub coreml_fallback: Option<WorkerTrainingBackend>,
}

/// Map API/DB preferred backend into worker enums (uses BackendKind for parsing)
pub(crate) fn map_preferred_backend(
    preferred: Option<TrainingBackendKind>,
    coreml_fallback: Option<TrainingBackendKind>,
) -> PreferredBackendSelection {
    let mut preferred_backend = None;
    let mut fallback_backend = None;

    if let Some(kind) = preferred {
        let core_kind = to_core_backend(kind);
        match WorkerTrainingBackend::try_from(core_kind) {
            Ok(mapped) => {
                preferred_backend = Some(mapped);

                // If the caller provided a CoreML fallback, keep it explicit; otherwise, do not
                // silently redirect. Fallbacks are handled downstream with explicit telemetry.
                if mapped == WorkerTrainingBackend::CoreML {
                    if let Some(fb) = coreml_fallback {
                        match WorkerTrainingBackend::try_from(to_core_backend(fb)) {
                            Ok(fb_mapped) => fallback_backend = Some(fb_mapped),
                            Err(err) => warn!(
                                backend = %fb.as_str(),
                                error = %err,
                                "CoreML fallback backend conversion failed; fallback disabled"
                            ),
                        }
                    }
                }
            }
            Err(err) => {
                warn!(
                    backend = %kind,
                    error = %err,
                    "Non-concrete preferred backend ignored; using auto-select"
                );
            }
        }
    }

    // Validate explicit fallback even if preferred backend isn't CoreML (defensive)
    if fallback_backend.is_none() {
        if let Some(fb) = coreml_fallback {
            match WorkerTrainingBackend::try_from(to_core_backend(fb)) {
                Ok(mapped) => fallback_backend = Some(mapped),
                Err(err) => warn!(
                    backend = %fb.as_str(),
                    error = %err,
                    "Invalid CoreML fallback backend ignored"
                ),
            }
        }
    }

    PreferredBackendSelection {
        preferred: preferred_backend,
        coreml_fallback: fallback_backend,
    }
}
