//! List adapters

use crate::output::OutputWriter;
use adapteros_registry::Registry;
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
}

pub async fn run(tier: Option<&str>, output: &OutputWriter) -> Result<()> {
    let registry = Registry::open("registry.db")?;
    let adapters = registry.list_adapters()?;

    // Filter adapters if tier is specified
    let filtered: Vec<_> = adapters
        .into_iter()
        .filter(|adapter| tier.map_or(true, |filter_tier| adapter.tier == filter_tier))
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
            hash: adapter.hash.to_string(),
            tier: adapter.tier.clone(),
            rank: adapter.rank,
            activation_pct: adapter.activation_pct as f64,
        })
        .collect();

    // Prepare table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["ID", "Hash", "Tier", "Rank", "Activation %"]);

    for adapter in &filtered {
        table.add_row(vec![
            Cell::new(&adapter.id),
            Cell::new(adapter.hash.to_string()),
            Cell::new(&adapter.tier),
            Cell::new(adapter.rank.to_string()),
            Cell::new(format!("{:.2}", adapter.activation_pct)),
        ]);
    }

    output.table(&table as &dyn std::fmt::Display, Some(&json_data))?;

    Ok(())
}
