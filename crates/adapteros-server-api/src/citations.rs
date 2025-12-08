use crate::state::AppState;
use adapteros_api_types::inference::Citation;
use adapteros_core::Result;

/// Stub: citation indexing is disabled; returns Ok without work.
pub async fn build_dataset_index(
    _state: &AppState,
    _dataset_id: &str,
    _tenant_id: &str,
) -> Result<()> {
    Ok(())
}

/// Stub: citation index is not built; returns empty list.
pub async fn load_or_build_index(
    _state: &AppState,
    _dataset_id: &str,
    _tenant_id: &str,
) -> Result<Vec<Citation>> {
    Ok(Vec::new())
}

/// Gather citations for the given adapters and query text (stubbed empty).
pub async fn collect_citations_for_adapters(
    _state: &AppState,
    _tenant_id: &str,
    _adapters: &[String],
    _query: &str,
    _top_k: usize,
) -> Vec<Citation> {
    Vec::new()
}
