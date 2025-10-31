//! Adapter info command - show provenance and signer information

use anyhow::Result;

/// Show adapter information including signer and provenance
pub async fn run(adapter_id: &str) -> Result<()> {
    println!("Adapter Information");
    println!("==================\n");

    // Query registry for adapter info
    let registry = adapteros_registry::Registry::open("./var/registry.db")?;

    if let Some(adapter) = registry.get_adapter(adapter_id)? {
        println!("Adapter: {}", adapter.id);
        println!("Hash: {}", adapter.hash.to_hex());
        println!("Tier: {}", adapter.tier);
        println!("Rank: {}", adapter.rank);
        println!("Activation: {:.2}%", adapter.activation_pct);
        println!("Registered: {}", adapter.registered_at);

        // Query provenance from database if available
        if let Ok(db) = adapteros_db::Database::connect_env().await {
            // Try to get provenance information
            match get_provenance(&db, adapter_id).await {
                Ok(Some(prov)) => {
                    println!("\nProvenance:");
                    println!("  Signer: {} ({})", prov.signer_key, prov.key_name);
                    if let Some(registered_by) = prov.registered_by {
                        println!(
                            "  RegisteredBy: {} (uid {}) @ {}",
                            registered_by,
                            prov.registered_uid.unwrap_or(0),
                            prov.registered_at
                        );
                    }
                    println!("  Bundle: {}", prov.bundle_b3);
                }
                Ok(None) => {
                    println!("\nProvenance: Not available (legacy adapter)");
                }
                Err(e) => {
                    println!("\nProvenance: Error retrieving - {}", e);
                }
            }
        } else {
            println!("\nProvenance: Database not available");
        }

        println!("\nACL:");
        if adapter.acl.is_empty() {
            println!("  (allow all tenants)");
        } else {
            for tenant in &adapter.acl {
                println!("  - {}", tenant);
            }
        }
    } else {
        return Err(anyhow::anyhow!("Adapter not found: {}", adapter_id));
    }

    Ok(())
}

/// Provenance information
#[derive(Debug)]
struct ProvenanceInfo {
    signer_key: String,
    key_name: String,
    registered_by: Option<String>,
    registered_uid: Option<u32>,
    registered_at: String,
    bundle_b3: String,
}

/// Get provenance information from database
async fn get_provenance(
    db: &adapteros_db::Database,
    adapter_id: &str,
) -> Result<Option<ProvenanceInfo>> {
    match db {
        adapteros_db::Database::Sqlite(sqlite) => {
            get_provenance_sqlite(sqlite.pool(), adapter_id).await
        }
        adapteros_db::Database::Postgres(pg) => {
            get_provenance_postgres(pg.pool(), adapter_id).await
        }
    }
}

async fn get_provenance_sqlite(
    pool: &sqlx::SqlitePool,
    adapter_id: &str,
) -> Result<Option<ProvenanceInfo>> {
    let row = sqlx::query!(
        r#"
        SELECT signer_key, registered_by, registered_uid, registered_at, bundle_b3
        FROM adapter_provenance
        WHERE adapter_id = ?
        "#,
        adapter_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let key_name = extract_key_name(&row.signer_key);
        ProvenanceInfo {
            signer_key: row.signer_key,
            key_name,
            registered_by: row.registered_by,
            registered_uid: row.registered_uid.map(|u| u as u32),
            registered_at: row.registered_at,
            bundle_b3: row.bundle_b3,
        }
    }))
}

async fn get_provenance_postgres(
    pool: &sqlx::postgres::PgPool,
    adapter_id: &str,
) -> Result<Option<ProvenanceInfo>> {
    let row = sqlx::query!(
        r#"
        SELECT 
            signer_key,
            registered_by,
            registered_uid,
            registered_at::text AS registered_at,
            bundle_b3
        FROM adapter_provenance
        WHERE adapter_id = $1
        "#,
        adapter_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let key_name = extract_key_name(&row.signer_key);
        ProvenanceInfo {
            signer_key: row.signer_key,
            key_name,
            registered_by: row.registered_by,
            registered_uid: row.registered_uid.map(|u| u as u32),
            registered_at: row.registered_at,
            bundle_b3: row.bundle_b3,
        }
    }))
}

/// Extract key name from signer key string
fn extract_key_name(signer_key: &str) -> String {
    // For now, just return a mock key name
    // In production, this would query a key registry
    if signer_key.starts_with("ed25519:") {
        format!("key_{}", &signer_key[8..12])
    } else {
        "unknown".to_string()
    }
}
