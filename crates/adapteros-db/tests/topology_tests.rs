use adapteros_db::{AdapterTopology, ClusterDefinition, Db};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct TestCatalog {
    version: String,
    #[serde(default)]
    clusters_version: Option<String>,
    clusters: Vec<TestCluster>,
    adapters: Vec<TestAdapter>,
}

#[derive(Debug, Deserialize)]
struct TestCluster {
    id: String,
    description: String,
    #[serde(default)]
    default_adapter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TestAdapter {
    id: String,
    name: String,
    #[serde(default)]
    cluster_ids: Vec<String>,
    #[serde(default)]
    transition_probabilities: HashMap<String, f64>,
}

#[tokio::test]
async fn ingest_catalog_builds_topology_graph() -> anyhow::Result<()> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let sample_catalog = r#"
    {
        "version": "1.1",
        "clusters_version": "2025.01",
        "clusters": [
            {"id": "math", "description": "Math cluster", "default_adapter": "math-a"},
            {"id": "logic", "description": "Logic cluster", "default_adapter": "logic-a"}
        ],
        "adapters": [
            {
                "id": "math-a",
                "name": "Math Adapter",
                "cluster_ids": ["math"],
                "transition_probabilities": {"logic": 0.6}
            },
            {
                "id": "logic-a",
                "name": "Logic Adapter",
                "cluster_ids": ["logic"],
                "transition_probabilities": {"math": 0.4}
            }
        ]
    }
    "#;

    let parsed: TestCatalog = serde_json::from_str(sample_catalog)?;
    let clusters_version = parsed
        .clusters_version
        .clone()
        .unwrap_or_else(|| parsed.version.clone());

    let clusters: Vec<ClusterDefinition> = parsed
        .clusters
        .iter()
        .map(|c| ClusterDefinition {
            id: c.id.clone(),
            description: c.description.clone(),
            default_adapter_id: c.default_adapter.clone(),
            version: clusters_version.clone(),
        })
        .collect();

    let adapters: Vec<AdapterTopology> = parsed
        .adapters
        .iter()
        .map(|a| AdapterTopology {
            adapter_id: a.id.clone(),
            name: a.name.clone(),
            cluster_ids: a.cluster_ids.clone(),
            transition_probabilities: a.transition_probabilities.clone(),
        })
        .collect();

    let db = Db::new_in_memory().await?;
    db.replace_topology(&clusters_version, &clusters, &adapters)
        .await?;

    let graph = db.get_topology_graph().await?;
    assert_eq!(graph.clusters_version, "2025.01");
    assert_eq!(graph.clusters.len(), 2);
    assert_eq!(graph.adapters.len(), 2);

    // Adjacency should surface math -> logic edge from transition probabilities
    let math_edges = graph.adjacency.get("math").expect("math cluster edges");
    assert_eq!(math_edges.len(), 1);
    assert_eq!(math_edges[0].to_cluster_id, "logic");
    assert!((math_edges[0].probability - 0.6).abs() < f64::EPSILON);

    Ok(())
}
