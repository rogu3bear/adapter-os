//! Worker capability normalization and routing requirements.

use crate::types::InferenceRequestInternal;
use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_core::backend::BackendKind;
use serde::Serialize;
use std::str::FromStr;

pub const REASON_CAPABILITIES_MISSING: &str = "capabilities_missing";
pub const REASON_BACKEND_MISMATCH: &str = "backend_mismatch";
pub const REASON_STEP_REQUIRED: &str = "mode_step_required";
pub const REASON_BULK_REQUIRED: &str = "mode_bulk_required";
pub const REASON_LOGITS_REQUIRED: &str = "mode_logits_required";
pub const REASON_STREAMING_REQUIRED: &str = "mode_streaming_required";
pub const REASON_DETERMINISM_REQUIRED: &str = "determinism_required";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RequiredModes {
    pub require_step: bool,
    pub require_bulk: bool,
    pub require_logits: bool,
    pub require_streaming: bool,
}

impl RequiredModes {
    pub fn for_request(require_step: bool, stream: bool) -> Self {
        let require_step = require_step || stream;
        Self {
            require_step,
            require_bulk: false,
            require_logits: require_step,
            require_streaming: stream,
        }
    }

    pub fn from_request(request: &InferenceRequestInternal) -> Self {
        let require_streaming = request.stream;
        let require_step = request.require_step || request.reasoning_mode || require_streaming;
        // Only require bulk when the request explicitly pins to a bulk-only backend.
        let require_bulk = !require_step
            && !require_streaming
            && !request.allow_fallback
            && matches!(request.backend_profile, Some(BackendKind::MlxBridge));
        Self {
            require_step,
            require_bulk,
            require_logits: require_step,
            require_streaming,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkerCapabilityExclusion {
    pub worker_id: String,
    pub backend: Option<String>,
    pub reasons: Vec<String>,
    pub capabilities: Option<WorkerCapabilities>,
}

pub fn normalize_worker_capabilities(mut caps: WorkerCapabilities) -> WorkerCapabilities {
    if let Some(normalized) = canonical_backend_kind(&caps.backend_kind) {
        caps.backend_kind = normalized;
    } else {
        caps.backend_kind = caps.backend_kind.to_ascii_lowercase();
    }
    caps
}

pub fn parse_worker_capabilities(
    raw: Option<&str>,
    backend_label: Option<&str>,
    legacy_caps: &[String],
) -> Option<WorkerCapabilities> {
    if let Some(raw_json) = raw {
        if let Ok(caps) = serde_json::from_str::<WorkerCapabilities>(raw_json) {
            return Some(normalize_worker_capabilities(caps));
        }
        if let Ok(list) = serde_json::from_str::<Vec<String>>(raw_json) {
            if let Some(caps) = derive_from_backend_list(&list, backend_label) {
                return Some(caps);
            }
        }
    }

    if let Some(caps) = derive_from_backend_list(legacy_caps, backend_label) {
        return Some(caps);
    }

    backend_label.and_then(derive_from_backend_label)
}

pub fn backend_kind_from_caps(caps: Option<&WorkerCapabilities>) -> Option<BackendKind> {
    caps.as_ref()
        .and_then(|caps| backend_kind_from_label(&caps.backend_kind))
}

pub fn backend_kind_from_label(label: &str) -> Option<BackendKind> {
    let normalized = label.to_ascii_lowercase();
    let label = normalized.as_str();
    if label == "bridge" {
        return Some(BackendKind::MlxBridge);
    }
    BackendKind::from_str(label).ok()
}

pub fn capability_reasons(
    caps: Option<&WorkerCapabilities>,
    required: &RequiredModes,
    require_backend: Option<BackendKind>,
    require_determinism: bool,
) -> Vec<String> {
    let mut reasons = Vec::new();
    let caps = match caps {
        Some(caps) => caps,
        None => {
            reasons.push(REASON_CAPABILITIES_MISSING.to_string());
            return reasons;
        }
    };

    if let Some(required_backend) = require_backend {
        let worker_backend = backend_kind_from_label(&caps.backend_kind);
        if worker_backend != Some(required_backend) {
            reasons.push(REASON_BACKEND_MISMATCH.to_string());
        }
    }

    if required.require_step && !caps.supports_step {
        reasons.push(REASON_STEP_REQUIRED.to_string());
    }
    if required.require_bulk && !caps.supports_bulk {
        reasons.push(REASON_BULK_REQUIRED.to_string());
    }
    if required.require_logits && !caps.supports_logits {
        reasons.push(REASON_LOGITS_REQUIRED.to_string());
    }
    if required.require_streaming && !caps.supports_streaming {
        reasons.push(REASON_STREAMING_REQUIRED.to_string());
    }
    if require_determinism {
        match backend_kind_from_label(&caps.backend_kind) {
            Some(BackendKind::MlxBridge) | None => {
                reasons.push(REASON_DETERMINISM_REQUIRED.to_string());
            }
            _ => {}
        }
    }

    reasons
}

fn derive_from_backend_list(
    list: &[String],
    backend_label: Option<&str>,
) -> Option<WorkerCapabilities> {
    if let Some(label) = backend_label {
        return derive_from_backend_label(label);
    }

    let mut preferred: Option<WorkerCapabilities> = None;
    for entry in list {
        if let Some(caps) = derive_from_backend_label(entry) {
            preferred = Some(caps);
            break;
        }
    }
    preferred
}

fn derive_from_backend_label(label: &str) -> Option<WorkerCapabilities> {
    let normalized = canonical_backend_kind(label)?;
    let (supports_step, supports_bulk, supports_logits, supports_streaming) =
        match normalized.as_str() {
            "bridge" => (false, true, false, false),
            "mlx" | "metal" | "coreml" => (true, false, true, true),
            _ => (false, false, false, false),
        };
    let implementation = if normalized == "bridge" {
        Some("mlx_subprocess".to_string())
    } else {
        None
    };

    let gpu_backward = normalized == "mlx";
    let multi_backend = matches!(normalized.as_str(), "mlx" | "bridge");

    Some(WorkerCapabilities {
        backend_kind: normalized,
        implementation,
        supports_step,
        supports_bulk,
        supports_logits,
        supports_streaming,
        gpu_backward,
        multi_backend,
    })
}

fn canonical_backend_kind(label: &str) -> Option<String> {
    let normalized = label.to_ascii_lowercase();
    let normalized = normalized.as_str();
    match normalized {
        "mlxbridge" | "mlx-bridge" | "bridge" => Some("bridge".to_string()),
        "mlx" => Some("mlx".to_string()),
        "metal" => Some("metal".to_string()),
        "coreml" | "core-ml" => Some("coreml".to_string()),
        "cpu" => Some("cpu".to_string()),
        "auto" => None,
        _ => None,
    }
}
