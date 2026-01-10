//! Format parsers for dataset ingestion.
//!
//! Provides parsers for different input formats:
//! - JSONL (instruction tuning format)
//! - CSV (tabular data with column mapping)
//! - Plain text (paragraph-based or heading-based)
//! - Markdown (heading-content or paragraph pairs)

pub mod csv_parser;
pub mod jsonl;
pub mod markdown;
pub mod text;

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A raw sample before tokenization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSample {
    /// Input text (prompt/instruction).
    pub input: String,
    /// Target text (response/output).
    pub target: String,
    /// Sample weight (default 1.0).
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_weight() -> f32 {
    1.0
}

impl RawSample {
    /// Create a new raw sample with default weight.
    pub fn new(input: String, target: String) -> Self {
        Self {
            input,
            target,
            weight: 1.0,
            metadata: HashMap::new(),
        }
    }

    /// Create a new raw sample with weight.
    pub fn with_weight(input: String, target: String, weight: f32) -> Self {
        Self {
            input,
            target,
            weight,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the sample.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Supported dataset formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetFormat {
    /// JSONL with instruction/input/output fields.
    Jsonl,
    /// CSV with configurable column mapping.
    Csv,
    /// Plain text (paragraph or heading based).
    Text,
    /// Markdown (heading-content or paragraph pairs).
    Markdown,
}

impl DatasetFormat {
    /// Detect format from file extension.
    pub fn detect(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "jsonl" | "json" => Some(Self::Jsonl),
            "csv" | "tsv" => Some(Self::Csv),
            "txt" => Some(Self::Text),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    /// Get the format name for manifest metadata.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
            Self::Text => "text",
            Self::Markdown => "markdown",
        }
    }
}

impl std::fmt::Display for DatasetFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for DatasetFormat {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "jsonl" | "json" => Ok(Self::Jsonl),
            "csv" | "tsv" => Ok(Self::Csv),
            "text" | "txt" => Ok(Self::Text),
            "markdown" | "md" => Ok(Self::Markdown),
            _ => Err(AosError::Validation(format!(
                "Unknown dataset format: {}. Supported: jsonl, csv, text, markdown",
                s
            ))),
        }
    }
}

/// Text parsing strategy for plain text and markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextStrategy {
    /// Split on double newlines, pair consecutive paragraphs.
    #[default]
    ParagraphPairs,
    /// Use headings as input, following content as target.
    HeadingContent,
}

impl std::fmt::Display for TextStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParagraphPairs => write!(f, "paragraph-pairs"),
            Self::HeadingContent => write!(f, "heading-content"),
        }
    }
}

impl std::str::FromStr for TextStrategy {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "paragraph-pairs" | "paragraphs" => Ok(Self::ParagraphPairs),
            "heading-content" | "headings" | "qa" => Ok(Self::HeadingContent),
            _ => Err(AosError::Validation(format!(
                "Unknown text strategy: {}. Supported: paragraph-pairs, heading-content",
                s
            ))),
        }
    }
}

/// Column mapping for CSV format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMapping {
    /// Column name or index for input field.
    pub input_col: String,
    /// Column name or index for target field.
    pub target_col: String,
    /// Optional column for weight.
    pub weight_col: Option<String>,
}

impl Default for ColumnMapping {
    fn default() -> Self {
        Self {
            input_col: "input".to_string(),
            target_col: "target".to_string(),
            weight_col: None,
        }
    }
}

/// Parser configuration.
#[derive(Debug, Clone, Default)]
pub struct ParserConfig {
    /// Column mapping for CSV format.
    pub column_mapping: Option<ColumnMapping>,
    /// Text parsing strategy.
    pub text_strategy: TextStrategy,
}

/// Parse a file into raw samples.
pub fn parse_file(
    path: &Path,
    format: DatasetFormat,
    config: &ParserConfig,
) -> Result<Vec<RawSample>> {
    match format {
        DatasetFormat::Jsonl => jsonl::parse_jsonl_file(path),
        DatasetFormat::Csv => {
            let mapping = config.column_mapping.clone().unwrap_or_default();
            csv_parser::parse_csv_file(path, &mapping)
        }
        DatasetFormat::Text => text::parse_text_file(path, config.text_strategy),
        DatasetFormat::Markdown => markdown::parse_markdown_file(path, config.text_strategy),
    }
}

// Re-exports for convenience
pub use csv_parser::parse_csv_file;
pub use jsonl::parse_jsonl_file;
pub use markdown::parse_markdown_file;
pub use text::parse_text_file;
