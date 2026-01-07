//! List Commit Delta Packs for a repository

use crate::cdp::store::{CdpMetadata, CdpStore};
use crate::output::OutputWriter;
use anyhow::Result;
use clap::Parser;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::Serialize;
use std::path::PathBuf;

/// List Commit Delta Packs for a repository
#[derive(Parser, Debug, Clone)]
pub struct CdpListArgs {
    /// Repository ID to list CDPs for
    #[arg(long)]
    pub repo_id: String,

    /// CDP storage directory
    #[arg(long, default_value = "var/cdps")]
    pub storage: PathBuf,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// CDP info for JSON output
#[derive(Serialize)]
struct CdpInfo {
    cdp_id: String,
    repo_id: String,
    commit_sha: String,
    branch: String,
    author: String,
    message: String,
    test_passed: bool,
    linter_issues: usize,
    changed_symbols: usize,
    created_at: String,
}

impl From<&CdpMetadata> for CdpInfo {
    fn from(meta: &CdpMetadata) -> Self {
        CdpInfo {
            cdp_id: meta.cdp_id.to_string(),
            repo_id: meta.repo_id.clone(),
            commit_sha: meta.commit_sha.clone(),
            branch: meta.branch.clone(),
            author: meta.author.clone(),
            message: meta.message.clone(),
            test_passed: meta.test_passed,
            linter_issues: meta.linter_issues,
            changed_symbols: meta.changed_symbols,
            created_at: meta.created_at.to_rfc3339(),
        }
    }
}

/// Execute the CDP list command
pub async fn execute(args: CdpListArgs, output: &OutputWriter) -> Result<()> {
    // Initialize store
    let mut store = CdpStore::new(&args.storage)?;

    // Load existing metadata index
    let index_path = args.storage.join("metadata_index.json");
    if let Err(e) = store.load_metadata_index(&index_path) {
        tracing::debug!("Could not load metadata index: {}", e);
    }

    // List CDPs for repo
    let cdps = store.list_for_repo(&args.repo_id);

    if cdps.is_empty() {
        output.info(&format!("No CDPs found for repository: {}", args.repo_id));
        return Ok(());
    }

    // Build JSON data
    let json_data: Vec<CdpInfo> = cdps.iter().map(|cdp| CdpInfo::from(*cdp)).collect();

    // Build table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "CDP ID", "Commit", "Branch", "Author", "Message", "Tests", "Issues",
    ]);

    for cdp in &cdps {
        let short_id = if cdp.cdp_id.to_string().len() > 8 {
            &cdp.cdp_id.to_string()[..8]
        } else {
            &cdp.cdp_id.to_string()
        };
        let short_sha = if cdp.commit_sha.len() > 7 {
            &cdp.commit_sha[..7]
        } else {
            &cdp.commit_sha
        };

        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(short_sha),
            Cell::new(&cdp.branch),
            Cell::new(truncate_str(&cdp.author, 20)),
            Cell::new(truncate_str(&cdp.message, 40)),
            Cell::new(if cdp.test_passed { "✓" } else { "✗" }),
            Cell::new(cdp.linter_issues.to_string()),
        ]);
    }

    output.table(&table as &dyn std::fmt::Display, Some(&json_data))?;
    output.info(&format!("Found {} CDPs", cdps.len()));

    Ok(())
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
