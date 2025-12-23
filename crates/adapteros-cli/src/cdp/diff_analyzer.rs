//! Git diff analysis for CDP creation

use adapteros_core::{AosError, Result};
use crate::cdp::{DiffSummary, SymbolChangeType, SymbolKind, ChangedSymbol};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git diff analyzer
pub struct DiffAnalyzer {
    repo_path: PathBuf,
}

/// Result of analyzing a git diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffAnalysis {
    /// Summary of changes
    pub summary: DiffSummary,
    /// Changed symbols detected
    pub changed_symbols: Vec<ChangedSymbol>,
    /// Raw diff output
    pub raw_diff: String,
}

impl DiffAnalyzer {
    /// Create a new diff analyzer
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Analyze diff between two commits
    pub fn analyze_commits(&self, from_commit: &str, to_commit: &str) -> Result<DiffAnalysis> {
        let raw_diff = self.get_raw_diff(from_commit, to_commit)?;
        let summary = self.parse_diff_summary(&raw_diff)?;
        let changed_symbols = self.extract_changed_symbols(&raw_diff)?;

        Ok(DiffAnalysis {
            summary,
            changed_symbols,
            raw_diff,
        })
    }

    /// Analyze diff for a single commit (against its parent)
    pub fn analyze_commit(&self, commit_sha: &str) -> Result<DiffAnalysis> {
        let parent_sha = self.get_parent_commit(commit_sha)?;
        self.analyze_commits(&parent_sha, commit_sha)
    }

    /// Analyze uncommitted changes
    pub fn analyze_uncommitted(&self) -> Result<DiffAnalysis> {
        let raw_diff = self.get_uncommitted_diff()?;
        let summary = self.parse_diff_summary(&raw_diff)?;
        let changed_symbols = self.extract_changed_symbols(&raw_diff)?;

        Ok(DiffAnalysis {
            summary,
            changed_symbols,
            raw_diff,
        })
    }

    /// Get raw git diff output
    fn get_raw_diff(&self, from_commit: &str, to_commit: &str) -> Result<String> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--no-color")
            .arg("--unified=3")
            .arg(format!("{}..{}", from_commit, to_commit))
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to run git diff: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid git diff output: {}", e)))?)
    }

    /// Get uncommitted diff
    fn get_uncommitted_diff(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--no-color")
            .arg("--unified=3")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to run git diff: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid git diff output: {}", e)))?)
    }

    /// Get parent commit SHA
    fn get_parent_commit(&self, commit_sha: &str) -> Result<String> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg(format!("{}^", commit_sha))
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to get parent commit: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Failed to get parent commit: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid parent commit output: {}", e)))?
            .trim()
            .to_string())
    }

    /// Parse diff summary from raw diff
    fn parse_diff_summary(&self, raw_diff: &str) -> Result<DiffSummary> {
        let mut summary = DiffSummary::new();
        let mut language_changes = HashMap::new();

        for line in raw_diff.lines() {
            if line.starts_with("diff --git") {
                // Parse file path from diff header
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let file_path = parts[3].trim_start_matches("b/");
                    let path = PathBuf::from(file_path);
                    
                    // Determine change type based on file existence
                    if self.is_file_added(&path) {
                        summary.added_files.push(path.clone());
                    } else if self.is_file_deleted(&path) {
                        summary.deleted_files.push(path.clone());
                    } else {
                        summary.modified_files.push(path.clone());
                    }

                    // Track language changes
                    if let Some(lang) = self.detect_language(&path) {
                        *language_changes.entry(lang).or_insert(0) += 1;
                    }
                }
            } else if line.starts_with("+") && !line.starts_with("+++") {
                summary.lines_added += 1;
            } else if line.starts_with("-") && !line.starts_with("---") {
                summary.lines_removed += 1;
            }
        }

        summary.language_changes = language_changes;
        Ok(summary)
    }

    /// Extract changed symbols from diff
    fn extract_changed_symbols(&self, raw_diff: &str) -> Result<Vec<ChangedSymbol>> {
        let mut changed_symbols = Vec::new();

        for line in raw_diff.lines() {
            if line.starts_with("diff --git") {
                // Extract file path
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let file_path = PathBuf::from(parts[3].trim_start_matches("b/"));
                    
                    // Analyze symbols in this file
                    let file_symbols = self.analyze_file_symbols(&file_path, raw_diff)?;
                    changed_symbols.extend(file_symbols);
                }
            }
        }

        Ok(changed_symbols)
    }

    /// Analyze symbols in a specific file
    fn analyze_file_symbols(&self, file_path: &Path, raw_diff: &str) -> Result<Vec<ChangedSymbol>> {
        let mut symbols = Vec::new();
        
        // Extract file-specific diff section
        let file_diff = self.extract_file_diff(file_path, raw_diff)?;
        
        // Simple heuristic: look for function/struct definitions in added lines
        for line in file_diff.lines() {
            if line.starts_with("+") && !line.starts_with("+++") {
                let content = &line[1..]; // Remove the '+'
                
                if let Some(symbol) = self.parse_symbol_from_line(content, file_path) {
                    symbols.push(symbol);
                }
            }
        }

        Ok(symbols)
    }

    /// Extract diff section for a specific file
    fn extract_file_diff(&self, file_path: &Path, raw_diff: &str) -> Result<String> {
        let mut in_file_section = false;
        let mut file_diff = String::new();

        for line in raw_diff.lines() {
            if line.starts_with("diff --git") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let diff_file_path = parts[3].trim_start_matches("b/");
                    in_file_section = diff_file_path == file_path.to_string_lossy();
                }
            } else if line.starts_with("diff --git") && in_file_section {
                break; // End of this file's diff section
            } else if in_file_section {
                file_diff.push_str(line);
                file_diff.push('\n');
            }
        }

        Ok(file_diff)
    }

    /// Parse symbol from a line of code
    fn parse_symbol_from_line(&self, line: &str, file_path: &Path) -> Option<ChangedSymbol> {
        let trimmed = line.trim();
        
        // Rust function definition
        if trimmed.starts_with("fn ") {
            if let Some(name) = self.extract_function_name(trimmed) {
                return Some(ChangedSymbol {
                    name,
                    kind: SymbolKind::Function,
                    file_path: file_path.to_path_buf(),
                    change_type: SymbolChangeType::Added,
                    line: 0, // Will be filled in by caller
                    column_range: None,
                });
            }
        }
        
        // Rust struct definition
        if trimmed.starts_with("struct ") {
            if let Some(name) = self.extract_struct_name(trimmed) {
                return Some(ChangedSymbol {
                    name,
                    kind: SymbolKind::Struct,
                    file_path: file_path.to_path_buf(),
                    change_type: SymbolChangeType::Added,
                    line: 0,
                    column_range: None,
                });
            }
        }
        
        // Rust trait definition
        if trimmed.starts_with("trait ") {
            if let Some(name) = self.extract_trait_name(trimmed) {
                return Some(ChangedSymbol {
                    name,
                    kind: SymbolKind::Trait,
                    file_path: file_path.to_path_buf(),
                    change_type: SymbolChangeType::Added,
                    line: 0,
                    column_range: None,
                });
            }
        }

        None
    }

    /// Extract function name from function definition
    fn extract_function_name(&self, line: &str) -> Option<String> {
        // Simple regex-like parsing: "fn function_name("
        if let Some(start) = line.find("fn ") {
            let after_fn = &line[start + 3..];
            if let Some(end) = after_fn.find('(') {
                return Some(after_fn[..end].trim().to_string());
            }
        }
        None
    }

    /// Extract struct name from struct definition
    fn extract_struct_name(&self, line: &str) -> Option<String> {
        // Simple regex-like parsing: "struct StructName"
        if let Some(start) = line.find("struct ") {
            let after_struct = &line[start + 7..];
            if let Some(end) = after_struct.find(' ') {
                return Some(after_struct[..end].trim().to_string());
            } else if let Some(end) = after_struct.find('{') {
                return Some(after_struct[..end].trim().to_string());
            }
        }
        None
    }

    /// Extract trait name from trait definition
    fn extract_trait_name(&self, line: &str) -> Option<String> {
        // Simple regex-like parsing: "trait TraitName"
        if let Some(start) = line.find("trait ") {
            let after_trait = &line[start + 6..];
            if let Some(end) = after_trait.find(' ') {
                return Some(after_trait[..end].trim().to_string());
            } else if let Some(end) = after_trait.find('{') {
                return Some(after_trait[..end].trim().to_string());
            }
        }
        None
    }

    /// Check if file was added
    fn is_file_added(&self, file_path: &Path) -> bool {
        // Simple heuristic: check if file exists in working directory
        self.repo_path.join(file_path).exists()
    }

    /// Check if file was deleted
    fn is_file_deleted(&self, file_path: &Path) -> bool {
        // Simple heuristic: check if file doesn't exist in working directory
        !self.repo_path.join(file_path).exists()
    }

    /// Detect language from file extension
    fn detect_language(&self, file_path: &Path) -> Option<String> {
        if let Some(ext) = file_path.extension() {
            match ext.to_str()? {
                "rs" => Some("rust".to_string()),
                "py" => Some("python".to_string()),
                "js" | "jsx" => Some("javascript".to_string()),
                "ts" | "tsx" => Some("typescript".to_string()),
                "go" => Some("go".to_string()),
                "java" => Some("java".to_string()),
                "cpp" | "cc" | "cxx" => Some("cpp".to_string()),
                "c" => Some("c".to_string()),
                _ => None,
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_diff_analyzer_creation() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let analyzer = DiffAnalyzer::new(temp_dir.path());
        assert_eq!(analyzer.repo_path, temp_dir.path());
    }

    #[test]
    fn test_parse_function_name() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let analyzer = DiffAnalyzer::new(temp_dir.path());
        
        assert_eq!(
            analyzer.extract_function_name("fn test_function() -> String"),
            Some("test_function".to_string())
        );
        
        assert_eq!(
            analyzer.extract_function_name("    fn   complex_function_name   ("),
            Some("complex_function_name".to_string())
        );
        
        assert_eq!(
            analyzer.extract_function_name("struct MyStruct"),
            None
        );
    }

    #[test]
    fn test_parse_struct_name() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let analyzer = DiffAnalyzer::new(temp_dir.path());
        
        assert_eq!(
            analyzer.extract_struct_name("struct MyStruct {"),
            Some("MyStruct".to_string())
        );
        
        assert_eq!(
            analyzer.extract_struct_name("    struct   ComplexStruct   "),
            Some("ComplexStruct".to_string())
        );
        
        assert_eq!(
            analyzer.extract_struct_name("fn my_function()"),
            None
        );
    }

    #[test]
    fn test_detect_language() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let analyzer = DiffAnalyzer::new(temp_dir.path());
        
        assert_eq!(
            analyzer.detect_language(&PathBuf::from("src/lib.rs")),
            Some("rust".to_string())
        );
        
        assert_eq!(
            analyzer.detect_language(&PathBuf::from("main.py")),
            Some("python".to_string())
        );
        
        assert_eq!(
            analyzer.detect_language(&PathBuf::from("app.js")),
            Some("javascript".to_string())
        );
        
        assert_eq!(
            analyzer.detect_language(&PathBuf::from("README.md")),
            None
        );
    }
}
