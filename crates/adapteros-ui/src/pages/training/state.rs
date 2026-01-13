//! Derived CoreML state helpers for training jobs.
//!
//! Keeps the rendering components simple and testable by encapsulating
//! how we interpret backend/export intent vs. results.

use adapteros_api_types::{TrainingBackendKind, TrainingJobResponse};

/// Client-side CoreML filters applied after fetching jobs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoremlFilterState {
    pub requested: bool,
    pub exported: bool,
    pub fallback: bool,
}

/// Derived CoreML-related state for a training job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoremlState {
    pub coreml_requested: bool,
    pub coreml_export_requested: bool,
    pub coreml_exported: bool,
    pub coreml_fallback: bool,
    pub export_status: Option<String>,
    pub export_reason: Option<String>,
    pub fallback_reason: Option<String>,
    pub requested_backend: Option<String>,
    pub backend: Option<String>,
    pub fused_package_hash: Option<String>,
    pub package_path: Option<String>,
    pub metadata_path: Option<String>,
}

impl CoremlState {
    /// Derive CoreML display state from a training job response.
    pub fn from_job(job: &TrainingJobResponse) -> Self {
        let requested_backend = job.requested_backend.clone();
        let backend = job.backend.clone();
        let export_status = job.coreml_export_status.clone();
        let fused_package_hash = job.coreml_fused_package_hash.clone();
        let package_path = job.coreml_package_path.clone();

        let coreml_requested =
            requested_backend.as_deref() == Some(TrainingBackendKind::CoreML.as_str());
        let export_requested = job.coreml_export_requested.unwrap_or(false);
        let export_reason = job.coreml_export_reason.clone();

        // Detect fallback: CoreML was requested but a different backend was used
        let is_fallback =
            coreml_requested && backend.as_deref() != Some(TrainingBackendKind::CoreML.as_str());
        let fallback_reason = if is_fallback {
            job.coreml_training_fallback
                .clone()
                .or_else(|| job.backend_reason.clone())
        } else {
            None
        };

        let export_success_status = matches!(
            export_status.as_deref(),
            Some("succeeded") | Some("metadata_only")
        );
        let coreml_exported =
            export_success_status || fused_package_hash.is_some() || package_path.is_some();
        // Mark as fallback if CoreML was requested but different backend used,
        // regardless of whether a reason was provided
        let coreml_fallback = is_fallback;

        Self {
            coreml_requested,
            coreml_export_requested: export_requested,
            coreml_exported,
            coreml_fallback,
            export_status,
            export_reason,
            fallback_reason,
            requested_backend,
            backend,
            fused_package_hash,
            package_path,
            metadata_path: job.coreml_metadata_path.clone(),
        }
    }
}

/// Check whether a job matches the active CoreML filters.
pub fn matches_coreml_filters(job: &TrainingJobResponse, filters: &CoremlFilterState) -> bool {
    let state = CoremlState::from_job(job);
    if filters.requested && !state.coreml_requested {
        return false;
    }
    if filters.exported && !state.coreml_exported {
        return false;
    }
    if filters.fallback && !state.coreml_fallback {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn base_job_value() -> Value {
        json!({
            "schema_version": "1.0.0",
            "id": "job-1",
            "adapter_name": "demo",
            "synthetic_mode": false,
            "status": "running",
            "total_epochs": 1,
            "learning_rate": 0.001,
            "created_at": "2024-01-01T00:00:00Z"
        })
    }

    fn job_with(extra: Value) -> TrainingJobResponse {
        let mut base = base_job_value();
        if let (Value::Object(base_obj), Value::Object(extra_obj)) = (&mut base, extra) {
            for (k, v) in extra_obj {
                base_obj.insert(k, v);
            }
        }
        serde_json::from_value(base).expect("job deserializes")
    }

    #[test]
    fn detects_coreml_export_success() {
        let job = job_with(json!({
            "requested_backend": "coreml",
            "backend": "coreml",
            "coreml_export_requested": true,
            "coreml_export_status": "succeeded",
            "coreml_fused_package_hash": "hash123",
            "coreml_package_path": "/tmp/pkg.mlpackage"
        }));

        let state = CoremlState::from_job(&job);
        assert!(state.coreml_requested);
        assert!(state.coreml_export_requested);
        assert!(state.coreml_exported);
        assert!(!state.coreml_fallback);
        assert_eq!(state.export_status.as_deref(), Some("succeeded"));
        assert_eq!(state.fused_package_hash.as_deref(), Some("hash123"));
    }

    #[test]
    fn detects_coreml_fallback_with_reason() {
        let job = job_with(json!({
            "requested_backend": "coreml",
            "backend": "mlx",
            "coreml_training_fallback": "ane_missing",
            "backend_reason": "ANE unavailable"
        }));

        let state = CoremlState::from_job(&job);
        assert!(state.coreml_requested);
        assert!(state.coreml_fallback);
        assert_eq!(state.backend.as_deref(), Some("mlx"));
        assert_eq!(state.fallback_reason.as_deref(), Some("ane_missing"));
    }

    #[test]
    fn matches_filters_on_state() {
        let job = job_with(json!({
            "requested_backend": "coreml",
            "backend": "metal",
            "coreml_export_requested": true,
            "coreml_export_status": "failed",
            "coreml_export_reason": "conversion_error"
        }));

        let requested_only = CoremlFilterState {
            requested: true,
            ..Default::default()
        };
        assert!(matches_coreml_filters(&job, &requested_only));

        let exported_only = CoremlFilterState {
            exported: true,
            ..Default::default()
        };
        assert!(!matches_coreml_filters(&job, &exported_only));

        let fallback_only = CoremlFilterState {
            fallback: true,
            ..Default::default()
        };
        assert!(matches_coreml_filters(&job, &fallback_only));
    }
}
