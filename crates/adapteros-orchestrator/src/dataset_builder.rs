//! Build training examples directly from a code directory using adapteros-codegraph
//!
//! This helper converts a directory fingerprint/scan into a set of
//! adapteros_lora_worker::training::TrainingExample, reusing the
//! DatasetGenerator logic for sliding window pair creation.

use adapteros_codegraph::analyze_directory;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    dataset::ChangeType, dataset::FilePatch, DatasetGenerator, TrainingExample,
};
use adapteros_platform::common::PlatformUtils;
use adapteros_secure_fs::path_policy::{canonicalize_strict, canonicalize_strict_in_allowed_roots};
use std::path::Path;
use tracing::{info, warn};
use walkdir::WalkDir;

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
pub fn build_from_directory(
    root: &Path,
    rel: &Path,
    cfg: DatasetBuilderConfig,
) -> Result<Vec<TrainingExample>> {
    if !root.is_absolute() {
        return Err(AosError::Validation(
            "directory root must be absolute".into(),
        ));
    }

    // Analyze the directory to get symbols and metadata
    let canonical_root = canonicalize_strict(root)?;
    let allowed_roots = [canonical_root.clone()];
    let target_path = canonical_root.join(rel);
    let target_path = canonicalize_strict_in_allowed_roots(&target_path, &allowed_roots)
        .map_err(|e| AosError::Validation(format!("Dataset path rejected: {}", e)))?;
    let analysis = analyze_directory(&target_path)?;

    info!(
        path = %analysis.path.display(),
        files = analysis.total_files,
        lines = analysis.total_lines,
        symbols = analysis.symbols.len(),
        "Building training dataset from directory"
    );

    // Walk the directory and create a patch per file.
    // Treat each file as a self-modification (old == new) to seed examples
    // via sliding windows. This yields deterministic pairs without diffs.
    let mut patches: Vec<FilePatch> = Vec::new();
    for entry_result in WalkDir::new(&analysis.path) {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    path = %analysis.path.display(),
                    error = %e,
                    "Failed to read directory entry while building dataset"
                );
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path_abs = entry.path().to_path_buf();
        let content = match std::fs::read_to_string(&path_abs) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    path = %path_abs.display(),
                    error = %e,
                    "Failed to read file content while building dataset"
                );
                continue;
            }
        };
        let rel_path = path_abs
            .strip_prefix(&canonical_root)
            .unwrap_or(&path_abs)
            .to_path_buf();
        patches.push(FilePatch {
            file_path: PlatformUtils::normalize_path_separators(&rel_path.to_string_lossy()),
            old_content: content.clone(),
            new_content: content,
            change_type: ChangeType::Modify,
        });
    }

    let gen = DatasetGenerator::new(cfg.context_window, cfg.min_examples);
    let examples = gen.generate_from_patches(&patches)?;
    gen.validate_examples(&examples)?;
    Ok(examples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_build_from_directory() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let sub = root.join("src");
        std::fs::create_dir_all(&sub).unwrap();
        let mut f = std::fs::File::create(sub.join("lib.rs")).unwrap();
        writeln!(f, "fn hello() {{ println!(\"hi\"); }}").unwrap();

        let cfg = DatasetBuilderConfig {
            context_window: 64,
            min_examples: 1,
        };
        let examples = build_from_directory(&root, Path::new("src"), cfg).unwrap();
        assert!(!examples.is_empty());
    }
}
