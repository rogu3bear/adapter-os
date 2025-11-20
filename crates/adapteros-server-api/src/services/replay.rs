use sqlx::Row;
use adapteros_deterministic_exec::GlobalSeed;
use uuid::Uuid;
use rand::RngCore;
use adapteros_replay::session::ReplaySession;
use adapteros_core::AosError;
use tracing::info;

pub async fn fetch_bundle_metadata(db: &Db, bundle_id: &str) -> Result<(String, String), sqlx::Error> {
    let row = sqlx::query("SELECT metadata_json FROM telemetry_bundles WHERE id = ?")
        .bind(bundle_id)
        .fetch_optional(db.pool())
        .await?;
    let metadata: serde_json::Value = if let Some(r) = row {
        serde_json::from_str(r.get("metadata_json")).map_err(|e| AosError::Serialization(format!("Invalid metadata JSON: {}", e)))?
    } else {
        serde_json::from_str(r#"{"cpid": "default", "plan_id": "default"}"#).map_err(|e| AosError::Serialization(format!("Invalid default metadata: {}", e)))?
    };
    let cpid = metadata["cpid"].as_str().unwrap_or("default").to_string();
    let plan_id = metadata["plan_id"].as_str().unwrap_or("default").to_string();
    let seed = GlobalSeed::get_or_init(b"replay_seed");
    let mut rng = seed.rng();
    let session_id = Uuid::from_bytes([rng.next_u64() as u8; 16].try_into().expect("Failed to create UUID bytes")).to_string(); // Seeded Uuid
    Ok((cpid, plan_id))
}

pub async fn reconstruct_bundle(bundle_id: &str, state: &AppState) -> Result<String, AosError> {
    let session = ReplaySession::from_bundle(bundle_id, &state.db).await?;
    let trace = session.replay().await?;
    info!(bundle_id = bundle_id, "Bundle reconstructed");
    Ok(trace)
}
