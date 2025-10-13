use crate::output::OutputWriter;
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Table};
use adapteros_db::Db;
use serde::Serialize;

#[derive(Serialize)]
struct PinInfo {
    adapter_id: String,
    tenant_id: String,
    pinned_until: Option<String>,
    reason: String,
}

#[derive(Serialize)]
struct PinnedAdapter {
    adapter_id: String,
    pinned_until: String,
    reason: String,
    pinned_at: String,
}

/// Pin an adapter to prevent eviction
pub async fn pin_adapter(
    db: &Db,
    tenant_id: &str,
    adapter_id: &str,
    ttl_hours: Option<u64>,
    reason: &str,
    output: &OutputWriter,
) -> Result<()> {
    let pinned_until = ttl_hours.map(|h| {
        let dt = chrono::Utc::now() + chrono::Duration::hours(h as i64);
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    });

    db.pin_adapter(tenant_id, adapter_id, pinned_until.as_deref(), reason, None)
        .await?;

    if let Some(ttl) = ttl_hours {
        output.success(format!(
            "Adapter {} pinned for tenant {} (expires in {} hours)",
            adapter_id, tenant_id, ttl
        ));
    } else {
        output.success(format!(
            "Adapter {} pinned for tenant {} (forever)",
            adapter_id, tenant_id
        ));
    }

    if output.is_json() {
        let info = PinInfo {
            adapter_id: adapter_id.to_string(),
            tenant_id: tenant_id.to_string(),
            pinned_until,
            reason: reason.to_string(),
        };
        output.json(&info)?;
    }

    Ok(())
}

/// Unpin an adapter to allow eviction
pub async fn unpin_adapter(
    db: &Db,
    tenant_id: &str,
    adapter_id: &str,
    output: &OutputWriter,
) -> Result<()> {
    db.unpin_adapter(tenant_id, adapter_id).await?;
    output.success(format!(
        "Adapter {} unpinned for tenant {}",
        adapter_id, tenant_id
    ));

    if output.is_json() {
        let info = serde_json::json!({
            "adapter_id": adapter_id,
            "tenant_id": tenant_id,
            "status": "unpinned"
        });
        output.json(&info)?;
    }

    Ok(())
}

/// List pinned adapters for a tenant
pub async fn list_pinned(db: &Db, tenant_id: &str, output: &OutputWriter) -> Result<()> {
    let pinned = db.list_pinned_adapters(tenant_id).await?;

    if pinned.is_empty() {
        output.warning(format!("No pinned adapters for tenant {}", tenant_id));
        return Ok(());
    }

    // Prepare JSON data
    let json_data: Vec<PinnedAdapter> = pinned
        .iter()
        .map(|pin| PinnedAdapter {
            adapter_id: pin.adapter_id.clone(),
            pinned_until: pin
                .pinned_until
                .clone()
                .unwrap_or_else(|| "forever".to_string()),
            reason: pin.reason.clone(),
            pinned_at: pin.pinned_at.clone(),
        })
        .collect();

    // Prepare table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Adapter ID", "Pinned Until", "Reason", "Pinned At"]);

    for pin in pinned {
        let until = pin.pinned_until.unwrap_or_else(|| "forever".to_string());
        table.add_row(vec![pin.adapter_id, until, pin.reason, pin.pinned_at]);
    }

    output.section(format!("Pinned adapters for tenant {}", tenant_id));
    output.table(&table as &dyn std::fmt::Display, Some(&json_data))?;

    Ok(())
}
