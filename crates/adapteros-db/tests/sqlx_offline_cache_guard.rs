//! Guardrail: ensure the committed SQLx offline cache exists and is well-formed JSON.
//!
//! CI runs `cargo sqlx prepare --workspace --check`, but this test catches the
//! most common local failure mode earlier: missing/corrupted `.sqlx` artifacts.

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn sqlx_offline_cache_present_and_valid_json() -> Result<()> {
    let root = workspace_root();
    let sqlx_dir = root.join("crates/adapteros-db/.sqlx");
    assert!(
        sqlx_dir.is_dir(),
        "SQLx offline cache dir missing: {}",
        sqlx_dir.display()
    );

    let mut query_files = 0usize;
    for entry in fs::read_dir(&sqlx_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("query-") || !name.ends_with(".json") {
            continue;
        }

        let raw = fs::read_to_string(&path)?;
        serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap_or_else(|e| panic!("Invalid JSON in {}: {e}", path.display()));
        query_files += 1;
    }

    assert!(
        query_files > 0,
        "SQLx offline cache dir had no query-*.json files: {}",
        sqlx_dir.display()
    );

    Ok(())
}
