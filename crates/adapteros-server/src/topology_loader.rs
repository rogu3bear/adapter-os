use adapteros_core::{AosError, Result};
use adapteros_db::{
    topology::{AdapterTopology, ClusterDefinition},
    Db,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct CatalogCluster {
    id: String,
    description: String,
    default_adapter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogAdapter {
    id: String,
    name: String,
    cluster_ids: Vec<String>,
    transition_probabilities: Option<HashMap<String, f64>>,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    clusters_version: String,
    clusters: Vec<CatalogCluster>,
    adapters: Vec<CatalogAdapter>,
}

/// Ingest topology graph from adapters/catalog.json into the DB for routing.
pub async fn ingest_catalog_topology(db: &Db, adapters_root: &Path) -> Result<()> {
    info!("Checking topology state...");

    let catalog_path = adapters_root.join("catalog.json");
    if !catalog_path.exists() {
        warn!(
            path = %catalog_path.display(),
            "No catalog.json found in adapters root, skipping topology load"
        );
        return Ok(());
    }

    let content = fs::read_to_string(&catalog_path)
        .await
        .map_err(|e| AosError::Io(e.to_string()))?;

    let catalog: Catalog = serde_json::from_str(&content).map_err(|e| {
        AosError::Serialization(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e,
        )))
    })?;

    info!(
        "Loading topology version {} with {} clusters and {} adapters",
        catalog.clusters_version,
        catalog.clusters.len(),
        catalog.adapters.len()
    );

    let clusters: Vec<ClusterDefinition> = catalog
        .clusters
        .into_iter()
        .map(|c| ClusterDefinition {
            id: c.id,
            description: c.description,
            default_adapter_id: c.default_adapter,
            version: catalog.clusters_version.clone(),
        })
        .collect();

    let adapters: Vec<AdapterTopology> = catalog
        .adapters
        .into_iter()
        .map(|a| AdapterTopology {
            adapter_id: a.id,
            name: a.name,
            cluster_ids: a.cluster_ids,
            transition_probabilities: a.transition_probabilities.unwrap_or_default(),
        })
        .collect();

    db.replace_topology(&catalog.clusters_version, &clusters, &adapters)
        .await?;

    info!("Topology loaded successfully");
    Ok(())
}
