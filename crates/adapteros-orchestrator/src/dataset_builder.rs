//! Build training examples directly from a code directory using adapteros-codegraph
//!
//! This helper converts a directory fingerprint/scan into a set of
//! adapteros_lora_worker::training::TrainingExample, reusing the
//! DatasetGenerator logic for sliding window pair creation.

use adapteros_codegraph::analyze_directory;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{dataset::ChangeType, dataset::FilePatch, DatasetGenerator, TrainingExample};
use std::path::{Path, PathBuf};
use tracing::info;

/// Configuration for building a directory dataset
#[derive(Debug, Clone)]
pub struct DatasetBuilderConfig {
    /// Max tokens per example (context window)
    pub context_window: usize,
    /// Minimum examples required (validator threshold)
    pub min_examples: usize,
}

impl Default for DatasetBuilderConfig {
    fn default() -> Self {
        Self {
            context_window: 512,
            min_examples: 10,
        }
    }
}

/// Build training examples from a repository root + relative path
pub fn build_from_directory(root: &Path, rel: &Path, cfg: DatasetBuilderConfig) -> Result<Vec<TrainingExample>> {
    if !root.is_absolute() {
        return Err(AosError::Validation("directory root must be absolute".into()));
    }
    let analysis = analyze_directory(root, rel).map_err(|e| AosError::Validation(format!("directory analysis failed: {}", e)))?;

    info!(
        path = %analysis.path.display(),
        files = analysis.total_files,
        lines = analysis.total_lines,
        symbols = analysis.symbols.len(),
        "Building training dataset from directory"
    );

    // Heuristic: create a patch per file present (treat as Modify of same content),
    // so DatasetGenerator will create sliding windows of tokens. In future, we can
    // incorporate diffs/commits to create true old/new pairs.
    let mut patches: Vec<FilePatch> = Vec::new();

    // Build a file list by asking codegraph again limited to rel path; for now,
    // we just use the fingerprint path itself as a single patch placeholder.
    patches.push(FilePatch {
        file_path: normalize_path(&analysis.path),
        old_content: String::new(),
        new_content: read_file_best_effort(&analysis.path),
        change_type: ChangeType::Modify,
    });

    let gen = DatasetGenerator::new(cfg.context_window, cfg.min_examples);
    let examples = gen.generate_from_patches(&patches)?;
    gen.validate_examples(&examples)?;
    Ok(examples)
}

fn normalize_path(p: &PathBuf) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn read_file_best_effort(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::io::Write;

    #[test]
    fn test_build_from_directory() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let sub = root.join("src");
        std::fs::create_dir_all(&sub).unwrap();
        let mut f = std::fs::File::create(sub.join("lib.rs")).unwrap();
        writeln!(f, "fn hello() {{ println!(\"hi\"); }}").unwrap();

        let cfg = DatasetBuilderConfig { context_window: 64, min_examples: 1 };
        let examples = build_from_directory(&root, Path::new("src"), cfg).unwrap();
        assert!(!examples.is_empty());
    }
}

