use adapteros_db::Db;
use adapteros_orchestrator::code_jobs::{
    ArtifactStore, CommitDeltaPack, CommitDeltaJob, SymbolIndexArtifact, TestMapArtifact,
};
use adapteros_orchestrator::{CodeJobManager, UpdateIndicesJob};
use adapteros_retrieval::codegraph::{CodeGraph, Language, Span, SymbolId, SymbolKind, SymbolNode};
use std::path::PathBuf;

fn make_symbol(name: &str, file_path: &str, kind: SymbolKind, line: u32) -> SymbolNode {
    let span = Span::new(line, 1, line, 10, 0, 10);
    let id = SymbolId::new(file_path, &span.to_string(), name);
    SymbolNode::new(
        id,
        name.to_string(),
        kind,
        Language::Rust,
        span,
        file_path.to_string(),
    )
}

fn build_graph(symbols: Vec<SymbolNode>) -> CodeGraph {
    let mut graph = CodeGraph::new();
    for symbol in symbols {
        graph.symbols.insert(symbol.id.clone(), symbol);
    }
    graph
}

#[tokio::test]
async fn commit_delta_job_writes_pack() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let tmp_root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&tmp_root)?;
    let temp_dir = tempfile::tempdir_in(&tmp_root)?;

    let store = ArtifactStore::new(temp_dir.path().to_path_buf());
    let mut base_symbol = make_symbol("foo", "src/lib.rs", SymbolKind::Function, 1);
    let mut head_symbol = base_symbol.clone();
    head_symbol.docstring = Some("updated".to_string());

    let removed_symbol = make_symbol("old_fn", "src/old.rs", SymbolKind::Function, 3);
    let added_symbol = make_symbol("new_fn", "src/new.rs", SymbolKind::Function, 5);

    let base_graph = build_graph(vec![base_symbol.clone(), removed_symbol]);
    let head_graph = build_graph(vec![head_symbol, added_symbol]);

    store
        .store_codegraph(&base_graph, "repo-1", "base")
        .await?;
    store
        .store_codegraph(&head_graph, "repo-1", "head")
        .await?;

    let manager = CodeJobManager::new(db, temp_dir.path().to_path_buf());
    manager
        .execute_commit_delta_job(CommitDeltaJob {
            repo_id: "repo-1".to_string(),
            base_commit: "base".to_string(),
            head_commit: "head".to_string(),
        })
        .await?;

    let cdp_path = temp_dir.path().join("repo-1").join("base-head.cdp.json");
    assert!(cdp_path.exists());

    let json = tokio::fs::read_to_string(&cdp_path).await?;
    let pack: CommitDeltaPack = serde_json::from_str(&json)?;

    assert_eq!(pack.added.len(), 1);
    assert_eq!(pack.removed.len(), 1);
    assert_eq!(pack.modified.len(), 1);

    Ok(())
}

#[tokio::test]
async fn update_indices_job_writes_artifacts() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let tmp_root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&tmp_root)?;
    let temp_dir = tempfile::tempdir_in(&tmp_root)?;

    let store = ArtifactStore::new(temp_dir.path().to_path_buf());
    let symbol = make_symbol("test_feature", "src/tests.rs", SymbolKind::Function, 10);
    let graph = build_graph(vec![symbol]);

    store
        .store_codegraph(&graph, "repo-2", "commit-1")
        .await?;

    let manager = CodeJobManager::new(db, temp_dir.path().to_path_buf());
    manager
        .execute_update_indices_job(UpdateIndicesJob {
            repo_id: "repo-2".to_string(),
            commit_sha: "commit-1".to_string(),
        })
        .await?;

    let symbols_path = temp_dir
        .path()
        .join("repo-2")
        .join("commit-1.symbols.index.json");
    let tests_path = temp_dir
        .path()
        .join("repo-2")
        .join("commit-1.tests.map.json");

    assert!(symbols_path.exists());
    assert!(tests_path.exists());

    let symbols_json = tokio::fs::read_to_string(&symbols_path).await?;
    let tests_json = tokio::fs::read_to_string(&tests_path).await?;

    let symbols: SymbolIndexArtifact = serde_json::from_str(&symbols_json)?;
    let tests: TestMapArtifact = serde_json::from_str(&tests_json)?;

    assert_eq!(symbols.symbols.len(), 1);
    assert_eq!(tests.tests.len(), 1);

    Ok(())
}
