//! Plain text format parser.
//!
//! Supports two parsing strategies:
//! - `paragraph-pairs`: Split on double newlines, pair consecutive paragraphs as input/target
//! - `heading-content`: Lines starting with # are inputs, following content is target

use super::{RawSample, TextStrategy};
use crate::training::normalize::{normalize_text, validate_non_empty};
use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Parse a plain text file into raw samples.
pub fn parse_text_file(path: &Path, strategy: TextStrategy) -> Result<Vec<RawSample>> {
    let content = fs::read_to_string(path).map_err(|e| {
        AosError::Io(format!("Failed to read text file {}: {}", path.display(), e))
    })?;

    let path_str = path.display().to_string();
    let normalized = normalize_text(&content)?;

    let samples = match strategy {
        TextStrategy::ParagraphPairs => parse_paragraph_pairs(&normalized, &path_str)?,
        TextStrategy::HeadingContent => parse_heading_content(&normalized, &path_str)?,
    };

    if samples.is_empty() {
        return Err(AosError::Validation(format!(
            "Text file {} contains no valid samples using {:?} strategy",
            path_str, strategy
        )));
    }

    Ok(samples)
}

/// Parse using paragraph-pairs strategy.
/// Splits on double newlines, pairs consecutive paragraphs as input/target.
fn parse_paragraph_pairs(content: &str, path: &str) -> Result<Vec<RawSample>> {
    let paragraphs: Vec<&str> = content
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 2 {
        return Err(AosError::Validation(format!(
            "Text file {} has fewer than 2 paragraphs for paragraph-pairs strategy",
            path
        )));
    }

    let mut samples = Vec::new();

    // Pair consecutive paragraphs
    for (i, chunk) in paragraphs.chunks(2).enumerate() {
        if chunk.len() < 2 {
            // Odd number of paragraphs - skip the last one
            break;
        }

        let input = chunk[0].to_string();
        let target = chunk[1].to_string();

        // Validate non-empty (should already be from filtering, but defensive)
        let context = format!("{}:paragraph_pair_{}", path, i + 1);
        validate_non_empty(&input, "input", &context)?;
        validate_non_empty(&target, "target", &context)?;

        let mut metadata = HashMap::new();
        metadata.insert("source_file".to_string(), path.to_string());
        metadata.insert("pair_index".to_string(), (i + 1).to_string());
        metadata.insert("strategy".to_string(), "paragraph-pairs".to_string());

        samples.push(RawSample {
            input,
            target,
            weight: 1.0,
            metadata,
        });
    }

    Ok(samples)
}

/// Parse using heading-content strategy.
/// Lines starting with # are inputs, following content until next heading is target.
fn parse_heading_content(content: &str, path: &str) -> Result<Vec<RawSample>> {
    let mut samples = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_content = Vec::new();
    let mut heading_count = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check if line is a heading (starts with #)
        if trimmed.starts_with('#') {
            // Save previous section if we have one
            if let Some(heading) = current_heading.take() {
                let target = current_content.join("\n").trim().to_string();
                if !target.is_empty() {
                    heading_count += 1;
                    let context = format!("{}:heading_{}", path, heading_count);
                    validate_non_empty(&heading, "input", &context)?;

                    let mut metadata = HashMap::new();
                    metadata.insert("source_file".to_string(), path.to_string());
                    metadata.insert("heading_index".to_string(), heading_count.to_string());
                    metadata.insert("strategy".to_string(), "heading-content".to_string());

                    samples.push(RawSample {
                        input: heading,
                        target,
                        weight: 1.0,
                        metadata,
                    });
                }
            }

            // Start new section - strip # prefix and whitespace
            let heading_text = trimmed.trim_start_matches('#').trim().to_string();
            if !heading_text.is_empty() {
                current_heading = Some(heading_text);
                current_content.clear();
            }
        } else if current_heading.is_some() {
            // Add line to current content
            current_content.push(line.to_string());
        }
    }

    // Don't forget the last section
    if let Some(heading) = current_heading {
        let target = current_content.join("\n").trim().to_string();
        if !target.is_empty() {
            heading_count += 1;
            let context = format!("{}:heading_{}", path, heading_count);
            validate_non_empty(&heading, "input", &context)?;

            let mut metadata = HashMap::new();
            metadata.insert("source_file".to_string(), path.to_string());
            metadata.insert("heading_index".to_string(), heading_count.to_string());
            metadata.insert("strategy".to_string(), "heading-content".to_string());

            samples.push(RawSample {
                input: heading,
                target,
                weight: 1.0,
                metadata,
            });
        }
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_text(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".txt").unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_paragraph_pairs_basic() {
        let file = write_temp_text("First paragraph.\n\nSecond paragraph.\n\nThird.\n\nFourth.");
        let samples = parse_text_file(file.path(), TextStrategy::ParagraphPairs).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].input, "First paragraph.");
        assert_eq!(samples[0].target, "Second paragraph.");
        assert_eq!(samples[1].input, "Third.");
        assert_eq!(samples[1].target, "Fourth.");
    }

    #[test]
    fn test_paragraph_pairs_odd_count() {
        let file = write_temp_text("One.\n\nTwo.\n\nThree.");
        let samples = parse_text_file(file.path(), TextStrategy::ParagraphPairs).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "One.");
        assert_eq!(samples[0].target, "Two.");
    }

    #[test]
    fn test_paragraph_pairs_insufficient() {
        let file = write_temp_text("Only one paragraph.");
        let result = parse_text_file(file.path(), TextStrategy::ParagraphPairs);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("fewer than 2"));
    }

    #[test]
    fn test_heading_content_basic() {
        let file = write_temp_text("# Question 1\nAnswer to question 1.\n\n# Question 2\nAnswer to question 2.");
        let samples = parse_text_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].input, "Question 1");
        assert_eq!(samples[0].target, "Answer to question 1.");
        assert_eq!(samples[1].input, "Question 2");
        assert_eq!(samples[1].target, "Answer to question 2.");
    }

    #[test]
    fn test_heading_content_multiline() {
        let file = write_temp_text("# Title\nLine 1\nLine 2\nLine 3");
        let samples = parse_text_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "Title");
        assert!(samples[0].target.contains("Line 1"));
        assert!(samples[0].target.contains("Line 3"));
    }

    #[test]
    fn test_heading_content_different_levels() {
        let file = write_temp_text("# H1\nContent 1\n## H2\nContent 2\n### H3\nContent 3");
        let samples = parse_text_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].input, "H1");
        assert_eq!(samples[1].input, "H2");
        assert_eq!(samples[2].input, "H3");
    }

    #[test]
    fn test_heading_content_no_headings() {
        let file = write_temp_text("No headings here.\nJust plain text.");
        let samples = parse_text_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert!(samples.is_empty() || samples.iter().all(|s| !s.input.is_empty()));
    }

    #[test]
    fn test_metadata_contains_strategy() {
        let file = write_temp_text("P1\n\nP2");
        let samples = parse_text_file(file.path(), TextStrategy::ParagraphPairs).unwrap();
        assert_eq!(samples[0].metadata.get("strategy"), Some(&"paragraph-pairs".to_string()));
    }
}
