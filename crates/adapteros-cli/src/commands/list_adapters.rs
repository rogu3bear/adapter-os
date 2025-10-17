//! List adapters

use crate::output::{OutputWriter, TableCell};
use adapteros_registry::Registry;
use anyhow::Result;
use comfy_table::CellAlignment;
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

    let header = vec![
        "ID",
        "Hash",
        "Tier",
        TableCell::new("Rank").align(CellAlignment::Right),
        TableCell::new("Activation %").align(CellAlignment::Right),
    ];

    let rows = filtered
        .iter()
        .map(|adapter| {
            output.table_row(vec![
                adapter.id.clone(),
                adapter.hash.to_string(),
                adapter.tier.clone(),
                TableCell::new(adapter.rank.to_string()).align(CellAlignment::Right),
                TableCell::new(format!("{:.2}", adapter.activation_pct))
                    .align(CellAlignment::Right),
            ])
        })
        .collect::<Vec<_>>();

    output.table(header, rows, None::<Vec<TableCell>>, Some(&json_data))?;

    Ok(())
}
