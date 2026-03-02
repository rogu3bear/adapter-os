use adapteros_api_types::ModelLoadStatus;
use adapteros_db::models::BaseModelStatus as DbBaseModelStatus;

/// Aggregated model status view derived from per-node/per-tenant records.
pub struct AggregatedModelStatus<'a> {
    pub status: ModelLoadStatus,
    /// Most recent status record (by updated_at) for metadata projection.
    pub latest: Option<&'a DbBaseModelStatus>,
}

/// Normalize raw status strings into canonical `ModelLoadStatus`.
#[inline]
pub fn normalize_status(raw: &str) -> ModelLoadStatus {
    ModelLoadStatus::parse_status(raw)
}

/// Aggregate multiple status reports into a cluster-level status.
///
/// Precedence:
/// - ready if any ready
/// - loading if any loading (and none ready)
/// - unloading if any unloading (and none ready/loading)
/// - error if any error (and none ready/loading)
/// - otherwise no-model
pub fn aggregate_status<'a, I>(records: I) -> AggregatedModelStatus<'a>
where
    I: IntoIterator<Item = &'a DbBaseModelStatus>,
{
    let mut latest: Option<&'a DbBaseModelStatus> = None;
    let mut has_ready = false;
    let mut has_loading = false;
    let mut has_unloading = false;
    let mut has_error = false;

    for record in records.into_iter() {
        let normalized = normalize_status(&record.status);

        // Track latest by updated_at (string is RFC3339, lexicographically sortable)
        if latest
            .as_ref()
            .map(|current| record.updated_at > current.updated_at)
            .unwrap_or(true)
        {
            latest = Some(record);
        }

        match normalized {
            ModelLoadStatus::Ready => has_ready = true,
            ModelLoadStatus::Loading => has_loading = true,
            // Legacy "checking" is treated as canonical "loading".
            ModelLoadStatus::Checking => has_loading = true,
            ModelLoadStatus::Unloading => has_unloading = true,
            ModelLoadStatus::Error => has_error = true,
            ModelLoadStatus::NoModel => {}
        }
    }

    let status = if has_ready {
        ModelLoadStatus::Ready
    } else if has_loading {
        ModelLoadStatus::Loading
    } else if has_unloading {
        ModelLoadStatus::Unloading
    } else if has_error {
        ModelLoadStatus::Error
    } else {
        ModelLoadStatus::NoModel
    };

    AggregatedModelStatus { status, latest }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(status: &str, updated_at: &str) -> DbBaseModelStatus {
        DbBaseModelStatus {
            id: "id".to_string(),
            tenant_id: "t1".to_string(),
            model_id: "m1".to_string(),
            status: status.to_string(),
            loaded_at: None,
            unloaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn aggregates_ready_over_loading() {
        let statuses = vec![
            record("loading", "2024-01-01T00:00:01Z"),
            record("ready", "2024-01-01T00:00:02Z"),
        ];
        let agg = aggregate_status(statuses.iter());
        assert_eq!(agg.status, ModelLoadStatus::Ready);
        assert!(agg.latest.is_some());
    }

    #[test]
    fn aggregates_loading_when_no_ready() {
        let statuses = vec![
            record("loading", "2024-01-01T00:00:01Z"),
            record("no-model", "2024-01-01T00:00:02Z"),
        ];
        let agg = aggregate_status(statuses.iter());
        assert_eq!(agg.status, ModelLoadStatus::Loading);
    }

    #[test]
    fn aggregates_error_when_no_ready_or_loading() {
        let statuses = vec![record("error", "2024-01-01T00:00:01Z")];
        let agg = aggregate_status(statuses.iter());
        assert_eq!(agg.status, ModelLoadStatus::Error);
    }

    #[test]
    fn aggregates_unloading_when_no_ready_loading() {
        let statuses = vec![record("unloading", "2024-01-01T00:00:01Z")];
        let agg = aggregate_status(statuses.iter());
        assert_eq!(agg.status, ModelLoadStatus::Unloading);
    }

    #[test]
    fn aggregates_no_model_when_empty() {
        let statuses: Vec<DbBaseModelStatus> = Vec::new();
        let agg = aggregate_status(statuses.iter());
        assert_eq!(agg.status, ModelLoadStatus::NoModel);
        assert!(agg.latest.is_none());
    }

    #[test]
    fn ready_is_only_routable_state() {
        assert!(ModelLoadStatus::Ready.is_ready());
        assert!(!ModelLoadStatus::Loading.is_ready());
        assert!(!ModelLoadStatus::Unloading.is_ready());
        assert!(!ModelLoadStatus::NoModel.is_ready());
        assert!(!ModelLoadStatus::Error.is_ready());
        assert!(!ModelLoadStatus::Checking.is_ready());
    }

    #[test]
    fn normalizes_ready_and_legacy_loaded() {
        assert_eq!(
            ModelLoadStatus::Ready,
            ModelLoadStatus::parse_status("ready")
        );
        assert_eq!(
            ModelLoadStatus::Ready,
            ModelLoadStatus::parse_status("loaded")
        );
    }

    #[test]
    fn normalizes_no_model_variants() {
        assert_eq!(
            ModelLoadStatus::NoModel,
            ModelLoadStatus::parse_status("no-model")
        );
        assert_eq!(
            ModelLoadStatus::NoModel,
            ModelLoadStatus::parse_status("unloaded")
        );
    }
}
