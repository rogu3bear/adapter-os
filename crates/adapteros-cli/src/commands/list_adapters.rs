//! List adapters

use crate::output::OutputWriter;
use adapteros_db::{metadata::AdapterMeta, Db};
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::Serialize;

#[derive(Serialize)]
struct AdapterInfo {
    id: String,
    hash: String,
    tier: String,
    rank: u32,
    activation_pct: f64,
    version: String,
    lifecycle_state: String,
}

#[derive(Serialize)]
struct AdapterListResponse {
    schema_version: String,
    adapters: Vec<AdapterMeta>,
}

pub async fn run(tier: Option<&str>, include_meta: bool, output: &OutputWriter) -> Result<()> {
    // If --include-meta is set, fetch full metadata from database
    if include_meta {
        let db = Db::connect_env().await?;
        let mut adapter_records = db.list_adapters().await?;

        // Filter by tier if specified
        if let Some(tier_filter) = tier {
            adapter_records.retain(|a| a.tier == tier_filter);
        }

        if adapter_records.is_empty() {
            output.warning("No adapters found");
            return Ok(());
        }

        // Convert to AdapterMeta
        let adapter_metas: Vec<AdapterMeta> = adapter_records
            .into_iter()
            .map(|record| AdapterMeta::from(record))
            .collect();

        // Create response with schema version
        let response = AdapterListResponse {
            schema_version: adapteros_db::metadata::API_SCHEMA_VERSION.to_string(),
            adapters: adapter_metas,
        };

        // Output as pretty-printed JSON
        let json = serde_json::to_string_pretty(&response)?;
        println!("{}", json);
        return Ok(());
    }

    // Default behavior: use database and table format
    let db = Db::connect_env().await?;
    let adapters = db.list_adapters().await?;

    // Filter adapters if tier is specified
    let filtered: Vec<_> = adapters
        .into_iter()
        .filter(|adapter| tier.map_or(true, |ft| adapter.tier == ft))
        .collect();

    if filtered.is_empty() {
        output.warning("No adapters found");
        return Ok(());
    }

    // Prepare JSON data
    let json_data: Vec<AdapterInfo> = filtered
        .iter()
        .map(|adapter| AdapterInfo {
            id: adapter.id.clone(),
            hash: adapter.hash_b3.clone(),
            tier: adapter.tier.clone(),
            rank: adapter.rank as u32,
            activation_pct: 0.0, // Not available in basic adapter struct
            version: adapter.version.clone(),
            lifecycle_state: adapter.lifecycle_state.clone(),
        })
        .collect();

    // Prepare table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Name", "Version", "Lifecycle", "Tier", "Rank", "State"]);

    for adapter in &filtered {
        table.add_row(vec![
            Cell::new(&adapter.name),
            Cell::new(&adapter.version),
            Cell::new(&adapter.lifecycle_state),
            Cell::new(&adapter.tier),
            Cell::new(adapter.rank.to_string()),
            Cell::new(&adapter.current_state),
        ]);
    }

    output.table(&table as &dyn std::fmt::Display, Some(&json_data))?;

    Ok(())
}
