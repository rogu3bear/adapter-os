//! AdapterOS (.aos) manifest parser
//!
//! Provides lightweight symbol extraction from AdapterOS single-file
//! adapter manifests to integrate with the codegraph.

use crate::parsers::LanguageParser;
use crate::types::{Language, ParseResult, Span, SymbolId, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use adapteros_single_file_adapter::SingleFileAdapterLoader;
use std::path::Path;

/// Parser for AdapterOS single-file adapter manifests.
pub struct AdapterParser {
    runtime: Option<tokio::runtime::Runtime>,
}

impl AdapterParser {
    /// Create a new Adapter parser instance.
    pub fn new() -> Result<Self> {
        Ok(Self { runtime: None })
    }

    fn block_on<F, T>(&mut self, fut: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(fut)
        } else {
            if self.runtime.is_none() {
                self.runtime = Some(tokio::runtime::Runtime::new().map_err(|e| {
                    AosError::Internal(format!(
                        "Failed to create Tokio runtime for AdapterOS parser: {}",
                        e
                    ))
                })?);
            }
            self.runtime
                .as_ref()
                .expect("AdapterParser runtime must be available")
                .block_on(fut)
        }
    }

    fn manifest_symbol(path: &Path, field: &str, value: &str) -> SymbolNode {
        let span = Span::new(1, 1, 1, 1, 0, 0);
        let span_repr = span.to_string();
        let file_id = path.to_string_lossy().to_string();
        let symbol_name = format!("{}:{}", field, value);
        let symbol_id = SymbolId::new(&file_id, &span_repr, &symbol_name);
        let mut symbol = SymbolNode::new(
            symbol_id,
            value.to_string(),
            SymbolKind::Class,
            Language::AdapterOS,
            span,
            file_id,
        );
        symbol.visibility = Visibility::Public;
        symbol.module_path = vec![field.to_string()];
        symbol.docstring = Some(format!("Adapter manifest {}: {}", field, value));
        symbol
    }
}

impl LanguageParser for AdapterParser {
    fn language(&self) -> Language {
        Language::AdapterOS
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        if path.extension().and_then(|ext| ext.to_str()) != Some("aos") {
            return Err(AosError::Parse(format!(
                "AdapterParser can only parse .aos files: {}",
                path.display()
            )));
        }

        let adapter = self.block_on(SingleFileAdapterLoader::load(path))?;
        let manifest = adapter.manifest;

        let mut symbols = Vec::with_capacity(3);

        if !manifest.adapter_id.trim().is_empty() {
            symbols.push(Self::manifest_symbol(
                path,
                "adapter_id",
                &manifest.adapter_id,
            ));
        }

        if !manifest.base_model.trim().is_empty() {
            symbols.push(Self::manifest_symbol(
                path,
                "base_model",
                &manifest.base_model,
            ));
        }

        if !manifest.category.trim().is_empty() {
            symbols.push(Self::manifest_symbol(path, "category", &manifest.category));
        }

        Ok(ParseResult {
            file_path: path.to_path_buf(),
            symbols,
        })
    }

    fn supported_extensions(&self) -> &[&str] {
        &["aos"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{
        format::{AdapterWeights, WeightGroup, WeightGroupType, WeightMetadata},
        LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig, TrainingExample,
    };
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    fn build_test_adapter() -> SingleFileAdapter {
        let weight_group = |group_type: WeightGroupType| WeightGroup {
            lora_a: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            lora_b: vec![vec![0.5, 0.6], vec![0.7, 0.8]],
            metadata: WeightMetadata {
                example_count: 1,
                avg_loss: 0.1,
                training_time_ms: 10,
                group_type,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
        };

        let adapter_weights = AdapterWeights {
            positive: weight_group(WeightGroupType::Positive),
            negative: weight_group(WeightGroupType::Negative),
            combined: None,
        };

        let training_data = vec![TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        let config = TrainingConfig::default();
        let lineage = LineageInfo {
            adapter_id: "adapteros.test".to_string(),
            version: "1.0.0".to_string(),
            parent_version: None,
            parent_hash: None,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        SingleFileAdapter::create(
            "adapteros.test".to_string(),
            adapter_weights,
            training_data,
            config,
            lineage,
        )
        .expect("failed to create test adapter")
    }

    #[tokio::test]
    async fn parses_manifest_metadata() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("sample.aos");

        let adapter = build_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .expect("failed to package adapter");

        let mut parser = AdapterParser::new().expect("parser init failed");
        let result = parser.parse_file(&aos_path).expect("parse failed");

        assert_eq!(result.file_path, aos_path);
        assert_eq!(result.symbols.len(), 3);

        let names: HashSet<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(adapter.manifest.adapter_id.as_str()));
        assert!(names.contains(adapter.manifest.base_model.as_str()));
        assert!(names.contains(adapter.manifest.category.as_str()));

        for symbol in &result.symbols {
            assert_eq!(symbol.language, Language::AdapterOS);
            assert_eq!(symbol.kind, SymbolKind::Class);
            assert_eq!(symbol.visibility, Visibility::Public);
            assert_eq!(symbol.file_path, aos_path.to_string_lossy());
            assert_eq!(symbol.module_path.len(), 1);
            assert!(symbol.docstring.as_deref().unwrap().contains(':'));
        }
    }
}
