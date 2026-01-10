//! Markdown format parser.
//!
//! Supports two parsing strategies:
//! - `heading-content`: Use headings as inputs, following content as targets (default)
//! - `paragraph-pairs`: Strip markdown formatting, pair consecutive paragraphs

use super::{RawSample, TextStrategy};
use crate::training::normalize::{normalize_text, validate_non_empty};
use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Parse a markdown file into raw samples.
pub fn parse_markdown_file(path: &Path, strategy: TextStrategy) -> Result<Vec<RawSample>> {
    let content = fs::read_to_string(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read markdown file {}: {}",
            path.display(),
            e
        ))
    })?;

    let path_str = path.display().to_string();
    let normalized = normalize_text(&content)?;

    let samples = match strategy {
        TextStrategy::HeadingContent => parse_heading_content(&normalized, &path_str)?,
        TextStrategy::ParagraphPairs => parse_paragraph_pairs(&normalized, &path_str)?,
    };

    if samples.is_empty() {
        return Err(AosError::Validation(format!(
            "Markdown file {} contains no valid samples using {:?} strategy",
            path_str, strategy
        )));
    }

    Ok(samples)
}

/// Parse using heading-content strategy.
/// ATX headings (# style) become inputs, content until next heading becomes target.
fn parse_heading_content(content: &str, path: &str) -> Result<Vec<RawSample>> {
    let mut samples = Vec::new();
    let mut current_heading: Option<(String, usize)> = None; // (text, level)
    let mut current_content = Vec::new();
    let mut heading_count = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for ATX heading (# style)
        if let Some((level, heading_text)) = parse_atx_heading(trimmed) {
            // Save previous section if we have content
            if let Some((heading, _)) = current_heading.take() {
                let target = strip_markdown(&current_content.join("\n"))
                    .trim()
                    .to_string();
                if !target.is_empty() {
                    heading_count += 1;
                    let context = format!("{}:heading_{}", path, heading_count);
                    validate_non_empty(&heading, "input", &context)?;

                    let mut metadata = HashMap::new();
                    metadata.insert("source_file".to_string(), path.to_string());
                    metadata.insert("heading_index".to_string(), heading_count.to_string());
                    metadata.insert("strategy".to_string(), "heading-content".to_string());
                    metadata.insert("format".to_string(), "markdown".to_string());

                    samples.push(RawSample {
                        input: heading,
                        target,
                        weight: 1.0,
                        metadata,
                    });
                }
            }

            // Start new section
            if !heading_text.is_empty() {
                current_heading = Some((heading_text, level));
                current_content.clear();
            }
        } else if current_heading.is_some() {
            current_content.push(line.to_string());
        }
    }

    // Don't forget the last section
    if let Some((heading, _)) = current_heading {
        let target = strip_markdown(&current_content.join("\n"))
            .trim()
            .to_string();
        if !target.is_empty() {
            heading_count += 1;
            let context = format!("{}:heading_{}", path, heading_count);
            validate_non_empty(&heading, "input", &context)?;

            let mut metadata = HashMap::new();
            metadata.insert("source_file".to_string(), path.to_string());
            metadata.insert("heading_index".to_string(), heading_count.to_string());
            metadata.insert("strategy".to_string(), "heading-content".to_string());
            metadata.insert("format".to_string(), "markdown".to_string());

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

/// Parse using paragraph-pairs strategy.
/// Strips markdown formatting, then pairs consecutive paragraphs.
fn parse_paragraph_pairs(content: &str, path: &str) -> Result<Vec<RawSample>> {
    // Strip markdown formatting first
    let plain = strip_markdown(content);

    let paragraphs: Vec<&str> = plain
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 2 {
        return Err(AosError::Validation(format!(
            "Markdown file {} has fewer than 2 paragraphs for paragraph-pairs strategy",
            path
        )));
    }

    let mut samples = Vec::new();

    for (i, chunk) in paragraphs.chunks(2).enumerate() {
        if chunk.len() < 2 {
            break;
        }

        let input = chunk[0].to_string();
        let target = chunk[1].to_string();

        let context = format!("{}:paragraph_pair_{}", path, i + 1);
        validate_non_empty(&input, "input", &context)?;
        validate_non_empty(&target, "target", &context)?;

        let mut metadata = HashMap::new();
        metadata.insert("source_file".to_string(), path.to_string());
        metadata.insert("pair_index".to_string(), (i + 1).to_string());
        metadata.insert("strategy".to_string(), "paragraph-pairs".to_string());
        metadata.insert("format".to_string(), "markdown".to_string());

        samples.push(RawSample {
            input,
            target,
            weight: 1.0,
            metadata,
        });
    }

    Ok(samples)
}

/// Parse ATX-style heading, returns (level, text).
fn parse_atx_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }

    // Count leading #s
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level > 6 {
        return None; // Invalid heading level
    }

    // Get text after #s
    let text = trimmed[level..].trim().to_string();

    // Handle optional closing #s (e.g., "## Heading ##")
    let text = text.trim_end_matches('#').trim().to_string();

    Some((level, text))
}

/// Strip common markdown formatting to get plain text.
fn strip_markdown(text: &str) -> String {
    let mut result = String::new();

    for line in text.lines() {
        let mut processed = line.to_string();

        // Skip code fences
        if processed.trim().starts_with("```") {
            continue;
        }

        // Remove inline code backticks
        processed = processed.replace('`', "");

        // Remove bold/italic markers
        // Handle *** and ___ first (bold+italic)
        processed = processed.replace("***", "");
        processed = processed.replace("___", "");
        // Then ** and __ (bold)
        processed = processed.replace("**", "");
        processed = processed.replace("__", "");
        // Then * and _ (italic) - be careful with underscores in words
        processed = remove_emphasis_markers(&processed);

        // Remove link syntax [text](url) -> text
        processed = strip_links(&processed);

        // Remove image syntax ![alt](url)
        processed = strip_images(&processed);

        // Remove heading markers (done separately for heading-content strategy)
        // Keep them for paragraph-pairs since we want the text

        // Remove horizontal rules
        let trimmed = processed.trim();
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            continue;
        }

        // Remove blockquote markers
        processed = processed.trim_start_matches('>').trim_start().to_string();

        // Remove list markers
        processed = strip_list_marker(&processed);

        result.push_str(&processed);
        result.push('\n');
    }

    result
}

/// Remove single * or _ emphasis markers (careful with underscores in words).
fn remove_emphasis_markers(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for * emphasis
        if c == '*' {
            // Skip if it's part of ** (already handled)
            if i + 1 < chars.len() && chars[i + 1] == '*' {
                result.push(c);
            }
            // Otherwise skip the single *
        }
        // Check for _ emphasis (only at word boundaries)
        else if c == '_' {
            let prev_is_space = i == 0 || chars[i - 1].is_whitespace();
            let next_is_space = i + 1 >= chars.len() || chars[i + 1].is_whitespace();
            if prev_is_space || next_is_space {
                // Skip underscore at word boundary
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }

        i += 1;
    }

    result
}

/// Strip markdown links [text](url) -> text.
fn strip_links(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            // Look for ](
            if let Some(close_bracket) = find_matching_bracket(&chars, i) {
                if close_bracket + 1 < chars.len() && chars[close_bracket + 1] == '(' {
                    // Found a link, extract text
                    let link_text: String = chars[i + 1..close_bracket].iter().collect();
                    result.push_str(&link_text);

                    // Skip to end of URL
                    if let Some(close_paren) =
                        chars[close_bracket + 1..].iter().position(|&c| c == ')')
                    {
                        i = close_bracket + 2 + close_paren;
                        continue;
                    }
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Strip markdown images ![alt](url).
fn strip_images(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            // Look for ](
            if let Some(close_bracket) = find_matching_bracket(&chars, i + 1) {
                if close_bracket + 1 < chars.len() && chars[close_bracket + 1] == '(' {
                    // Found an image, skip it entirely
                    if let Some(close_paren) =
                        chars[close_bracket + 1..].iter().position(|&c| c == ')')
                    {
                        i = close_bracket + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Find the matching ] for a [ at position start.
fn find_matching_bracket(chars: &[char], start: usize) -> Option<usize> {
    if chars[start] != '[' {
        return None;
    }

    let mut depth = 1;
    for (i, &c) in chars[start + 1..].iter().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + 1 + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip list markers (-, *, +, 1., etc.).
fn strip_list_marker(line: &str) -> String {
    let trimmed = line.trim_start();

    // Unordered lists: -, *, +
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        return rest.to_string();
    }

    // Ordered lists: 1. 2. etc.
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;

    // Check for digits
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }

    // Check for . or ) followed by space
    if i > 0 && i < chars.len() && (chars[i] == '.' || chars[i] == ')') {
        if i + 1 < chars.len() && chars[i + 1] == ' ' {
            return chars[i + 2..].iter().collect();
        }
    }

    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_md(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".md").unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_heading_content_basic() {
        let file = write_temp_md("# Question\nThe answer is here.\n\n# Another\nMore content.");
        let samples = parse_markdown_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].input, "Question");
        assert!(samples[0].target.contains("answer"));
    }

    #[test]
    fn test_heading_content_multiline() {
        let file = write_temp_md("# Title\nParagraph 1.\n\nParagraph 2.\n\nParagraph 3.");
        let samples = parse_markdown_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(samples.len(), 1);
        assert!(samples[0].target.contains("Paragraph 1"));
        assert!(samples[0].target.contains("Paragraph 3"));
    }

    #[test]
    fn test_paragraph_pairs_strips_formatting() {
        let file = write_temp_md("**Bold text** here.\n\n*Italic* response.");
        let samples = parse_markdown_file(file.path(), TextStrategy::ParagraphPairs).unwrap();
        assert_eq!(samples.len(), 1);
        // Should have stripped ** and *
        assert!(!samples[0].input.contains("**"));
        assert!(!samples[0].target.contains("*"));
    }

    #[test]
    fn test_strip_links() {
        let result = strip_links("Check out [this link](http://example.com) for more.");
        assert_eq!(result, "Check out this link for more.");
    }

    #[test]
    fn test_strip_images() {
        let result = strip_images("Here is an image: ![alt text](image.png)");
        assert_eq!(result, "Here is an image: ");
    }

    #[test]
    fn test_strip_list_markers() {
        assert_eq!(strip_list_marker("- Item"), "Item");
        assert_eq!(strip_list_marker("* Item"), "Item");
        assert_eq!(strip_list_marker("1. Item"), "Item");
        assert_eq!(strip_list_marker("10. Item"), "Item");
        assert_eq!(strip_list_marker("Regular line"), "Regular line");
    }

    #[test]
    fn test_atx_heading_levels() {
        assert_eq!(parse_atx_heading("# H1"), Some((1, "H1".to_string())));
        assert_eq!(parse_atx_heading("## H2"), Some((2, "H2".to_string())));
        assert_eq!(parse_atx_heading("###### H6"), Some((6, "H6".to_string())));
        assert_eq!(parse_atx_heading("Not a heading"), None);
    }

    #[test]
    fn test_heading_with_closing_hashes() {
        assert_eq!(
            parse_atx_heading("## Heading ##"),
            Some((2, "Heading".to_string()))
        );
    }

    #[test]
    fn test_metadata_contains_format() {
        let file = write_temp_md("# Q\nA");
        let samples = parse_markdown_file(file.path(), TextStrategy::HeadingContent).unwrap();
        assert_eq!(
            samples[0].metadata.get("format"),
            Some(&"markdown".to_string())
        );
    }
}
