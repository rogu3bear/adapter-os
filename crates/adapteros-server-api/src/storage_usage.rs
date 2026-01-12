use crate::state::AppState;
use adapteros_core::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct TenantStorageUsage {
    pub dataset_bytes: u64,
    pub adapter_bytes: u64,
    pub dataset_versions: u64,
    pub adapter_versions: u64,
}

impl TenantStorageUsage {
    pub fn total_bytes(&self) -> u64 {
        self.dataset_bytes + self.adapter_bytes
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceStorageUsage {
    pub dataset_bytes: u64,
    pub dataset_count: u64,
}

impl WorkspaceStorageUsage {
    pub fn total_bytes(&self) -> u64 {
        self.dataset_bytes
    }
}

/// Compute per-tenant storage usage by summing dataset sizes and adapter artifacts.
pub async fn compute_tenant_storage_usage(
    state: &AppState,
    tenant_id: &str,
) -> Result<TenantStorageUsage> {
    let dataset_bytes = state
        .db
        .sum_dataset_sizes_for_tenant(tenant_id)
        .await
        .unwrap_or(0) as u64;

    let dataset_versions = state
        .db
        .count_dataset_versions_for_tenant(tenant_id)
        .await
        .unwrap_or(0) as u64;

    let adapter_versions = state
        .db
        .count_adapter_versions_for_tenant(tenant_id)
        .await
        .unwrap_or(0) as u64;

    // Sum adapter artifact sizes from filesystem; ignore missing files gracefully.
    let mut adapter_bytes: u64 = 0;
    for (path_str, _) in state
        .db
        .list_adapter_artifacts_for_tenant(tenant_id)
        .await
        .unwrap_or_default()
    {
        let path = PathBuf::from(path_str);
        if let Ok(meta) = tokio::fs::metadata(&path).await {
            adapter_bytes = adapter_bytes.saturating_add(meta.len());
        }
    }

    Ok(TenantStorageUsage {
        dataset_bytes,
        adapter_bytes,
        dataset_versions,
        adapter_versions,
    })
}

/// Compute per-workspace storage usage by summing dataset sizes.
pub async fn compute_workspace_storage_usage(
    state: &AppState,
    workspace_id: &str,
) -> Result<WorkspaceStorageUsage> {
    let dataset_bytes = state
        .db
        .sum_dataset_sizes_for_workspace(workspace_id)
        .await
        .unwrap_or(0) as u64;

    let dataset_count = state
        .db
        .count_datasets_for_workspace(workspace_id)
        .await
        .unwrap_or(0) as u64;

    Ok(WorkspaceStorageUsage {
        dataset_bytes,
        dataset_count,
    })
}
