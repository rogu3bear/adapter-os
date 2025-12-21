//! DB Optimization Registry Validation
//!
//! Enforces coordination + rollout safety invariants for DB optimizations:
//! - Unique optimization IDs
//! - Dependency references resolve
//! - Conflict surface (`touches`) does not overlap across non-archived optimizations
//! - Rollback scripts exist for each optimization
//! - Referenced migration/rollback files exist on disk

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("adapteros-db should be in crates/adapteros-db")
        .to_path_buf()
}

#[derive(Debug, Deserialize)]
struct Registry {
    #[serde(default)]
    optimization: Vec<Optimization>,
}

#[derive(Debug, Deserialize)]
struct Optimization {
    id: String,
    title: String,
    owner_team: String,
    #[serde(default)]
    owner_contacts: Vec<String>,
    status: String,

    #[serde(default)]
    migrations: Vec<String>,
    #[serde(default)]
    rollback_scripts: Vec<String>,

    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    conflicts_with: Vec<String>,

    #[serde(default)]
    touches: Vec<String>,

    canary: String,
    rollback: String,

    #[serde(default)]
    impact_assessment: Vec<String>,
    #[serde(default)]
    success_metrics: Vec<String>,
}

fn read_registry() -> Result<Registry> {
    let root = workspace_root();
    let path = root.join("optimizations/db/registry.toml");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read registry: {}", path.display()))?;
    let reg: Registry = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML registry: {}", path.display()))?;
    Ok(reg)
}

fn assert_nonempty(field_name: &str, v: &str, id: &str) -> Result<()> {
    if v.trim().is_empty() {
        return Err(anyhow!(
            "Optimization '{}' has empty field: {}",
            id,
            field_name
        ));
    }
    Ok(())
}

fn assert_paths_exist(base_dir: &Path, paths: &[String], id: &str, kind: &str) -> Result<()> {
    for rel in paths {
        let p = base_dir.join(rel);
        if !p.exists() {
            return Err(anyhow!(
                "Optimization '{}' references missing {} path: {} (resolved: {})",
                id,
                kind,
                rel,
                p.display()
            ));
        }
    }
    Ok(())
}

fn assert_file_contains(path: &Path, needle: &str, id: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read file for optimization '{}': {}",
            id,
            path.display()
        )
    })?;
    if !content.contains(needle) {
        return Err(anyhow!(
            "Optimization '{}' missing required marker '{}' in file: {}",
            id,
            needle,
            path.display()
        ));
    }
    Ok(())
}

fn is_archived(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("archived")
}

#[test]
fn test_db_optimization_registry_invariants() -> Result<()> {
    let reg = read_registry()?;
    let root = workspace_root();

    if reg.optimization.is_empty() {
        return Err(anyhow!(
            "Registry has no entries; add at least one [[optimization]] to optimizations/db/registry.toml"
        ));
    }

    let allowed_statuses: HashSet<&'static str> = HashSet::from([
        "proposed",
        "in_progress",
        "merged",
        "rolled_out",
        "archived",
    ]);

    // Uniqueness checks
    let mut seen_ids = HashSet::<String>::new();
    let mut seen_touches = HashMap::<String, String>::new(); // touch -> id

    // For dependency validation
    let all_ids: HashSet<String> = reg.optimization.iter().map(|o| o.id.clone()).collect();

    for opt in &reg.optimization {
        // Required string fields
        assert_nonempty("id", &opt.id, &opt.id)?;
        assert_nonempty("title", &opt.title, &opt.id)?;
        assert_nonempty("owner_team", &opt.owner_team, &opt.id)?;
        assert_nonempty("status", &opt.status, &opt.id)?;
        assert_nonempty("canary", &opt.canary, &opt.id)?;
        assert_nonempty("rollback", &opt.rollback, &opt.id)?;

        if !seen_ids.insert(opt.id.clone()) {
            return Err(anyhow!("Duplicate optimization id found: {}", opt.id));
        }

        // Owner contacts required
        if opt.owner_contacts.is_empty() {
            return Err(anyhow!(
                "Optimization '{}' must include at least one owner_contact",
                opt.id
            ));
        }

        // Status validation
        let st = opt.status.trim().to_lowercase();
        if !allowed_statuses.contains(st.as_str()) {
            return Err(anyhow!(
                "Optimization '{}' has invalid status '{}'. Allowed: {:?}",
                opt.id,
                opt.status,
                allowed_statuses
            ));
        }

        // Safety fields
        if opt.impact_assessment.is_empty() {
            return Err(anyhow!(
                "Optimization '{}' must include at least one impact_assessment item",
                opt.id
            ));
        }
        if opt.success_metrics.is_empty() {
            return Err(anyhow!(
                "Optimization '{}' must include at least one success_metrics item",
                opt.id
            ));
        }

        // Referenced paths
        let migrations_dir = root.join("migrations");
        assert_paths_exist(&migrations_dir, &opt.migrations, &opt.id, "migration")?;
        assert_paths_exist(&migrations_dir, &opt.rollback_scripts, &opt.id, "rollback")?;

        // Ensure migrations carry a stable link back to the registry entry.
        // This makes it harder to accidentally ship an optimization without coordination.
        let marker = format!("Optimization-ID: {}", opt.id);
        for rel in &opt.migrations {
            assert_file_contains(&migrations_dir.join(rel), &marker, &opt.id)?;
        }
        for rel in &opt.rollback_scripts {
            assert_file_contains(&migrations_dir.join(rel), &marker, &opt.id)?;
        }

        if opt.rollback_scripts.is_empty() {
            return Err(anyhow!(
                "Optimization '{}' must define at least one rollback_scripts entry",
                opt.id
            ));
        }

        // Dependency resolution
        for dep in &opt.depends_on {
            if !all_ids.contains(dep) {
                return Err(anyhow!(
                    "Optimization '{}' depends_on unknown id: '{}'",
                    opt.id,
                    dep
                ));
            }
        }
        for c in &opt.conflicts_with {
            if !all_ids.contains(c) {
                return Err(anyhow!(
                    "Optimization '{}' conflicts_with unknown id: '{}'",
                    opt.id,
                    c
                ));
            }
        }

        // touches uniqueness among non-archived
        if !is_archived(&opt.status) {
            if opt.touches.is_empty() {
                return Err(anyhow!(
                    "Optimization '{}' must declare at least one touches entry (conflict surface)",
                    opt.id
                ));
            }
            for t in &opt.touches {
                let key = t.trim().to_string();
                if key.is_empty() {
                    return Err(anyhow!(
                        "Optimization '{}' contains an empty touches entry",
                        opt.id
                    ));
                }
                if let Some(prev_id) = seen_touches.insert(key.clone(), opt.id.clone()) {
                    return Err(anyhow!(
                        "touches conflict: '{}' declared by both '{}' and '{}'",
                        key,
                        prev_id,
                        opt.id
                    ));
                }
            }
        }
    }

    // Additional sanity: ensure all_ids is consistent with seen_ids
    if all_ids.len() != seen_ids.len() {
        return Err(anyhow!(
            "Registry id set mismatch; duplicates likely present (all_ids={}, seen_ids={})",
            all_ids.len(),
            seen_ids.len()
        ));
    }

    println!(
        "✓ DB optimization registry validated ({} entries, {} unique touches)",
        reg.optimization.len(),
        seen_touches.len()
    );

    Ok(())
}
