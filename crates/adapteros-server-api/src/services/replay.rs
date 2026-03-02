//! Replay service utilities
//! 【2025-01-27†refactor(server)†extract-replay-service】
//!
//! Extracted from handlers.rs to centralize replay session management.

use sqlx::Row;
use adapteros_replay::session::ReplaySession;
use adapteros_core::AosError;
use adapteros_db::Db;
use crate::state::AppState;
use tracing::info;

/// Fetch bundle metadata for replay session
/// 【2025-01-27†refactor(server)†extract-replay-service】
pub async fn fetch_bundle_metadata(db: &Db, bundle_id: &str) -> Result<(String, String), sqlx::Error> {
    let row = sqlx::query("SELECT metadata_json FROM telemetry_bundles WHERE id = ?")
        .bind(bundle_id)
        .fetch_optional(db.pool_result()?)
        .await?;
    let metadata: serde_json::Value = if let Some(r) = row {
        serde_json::from_str(r.get("metadata_json")).map_err(|e| AosError::Serialization(format!("Invalid metadata JSON: {}", e)))?
    } else {
        serde_json::from_str(r#"{"cpid": "default", "plan_id": "default"}"#).map_err(|e| AosError::Serialization(format!("Invalid default metadata: {}", e)))?
    };
    let cpid = metadata["cpid"].as_str().unwrap_or("default").to_string();
    let plan_id = metadata["plan_id"].as_str().unwrap_or("default").to_string();
    Ok((cpid, plan_id))
}

/// Reconstruct bundle from replay session
/// 【2025-01-27†refactor(server)†extract-replay-service】
pub async fn reconstruct_bundle(bundle_id: &str, state: &AppState) -> Result<String, AosError> {
    let session = ReplaySession::from_bundle(bundle_id, &state.db).await?;
    let trace = session.replay().await?;
    info!(bundle_id = bundle_id, "Bundle reconstructed");
    Ok(trace)
}
