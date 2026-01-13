//! Parser for synthesis model output
//!
//! Handles extracting structured JSON from model output, including:
//! - Markdown code block extraction
//! - JSON validation and repair
//! - Fallback strategies for malformed output

use super::types::{CompletionExample, InstructionExample, QAPair, SynthesisOutput};
use adapteros_core::{AosError, Result};
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// Parser for synthesis model output
pub struct SynthesisOutputParser {
    /// Whether to attempt JSON repair on parse failures
    attempt_repair: bool,
    /// Maximum output length to parse
    max_output_length: usize,
}

impl Default for SynthesisOutputParser {
    fn default() -> Self {
        Self {
            attempt_repair: true,
            max_output_length: 32768,
        }
    }
}

impl SynthesisOutputParser {
    /// Create a new parser with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable JSON repair attempts
    pub fn with_repair(mut self, attempt_repair: bool) -> Self {
        self.attempt_repair = attempt_repair;
        self
    }

    /// Parse model output into structured synthesis output
    pub fn parse(&self, raw_output: &str) -> Result<SynthesisOutput> {
        // Truncate if too long
        let output = if raw_output.len() > self.max_output_length {
            &raw_output[..self.max_output_length]
        } else {
            raw_output
        };

        // Try to extract JSON from output
        let json_str = self.extract_json(output)?;

        // Parse JSON
        self.parse_json(&json_str)
    }

    /// Extract JSON from model output (handles markdown code blocks)
    fn extract_json(&self, output: &str) -> Result<String> {
        // Try markdown code block first
        if let Some(json) = extract_json_from_markdown(output) {
            return Ok(json);
        }

        // Try to find raw JSON object
        if let Some(json) = extract_raw_json_object(output) {
            return Ok(json);
        }

        // Return the whole output as a last resort
        Ok(output.trim().to_string())
    }

    /// Parse JSON string into SynthesisOutput
    fn parse_json(&self, json_str: &str) -> Result<SynthesisOutput> {
        // Try direct parse first
        match serde_json::from_str::<SynthesisOutput>(json_str) {
            Ok(output) => return Ok(output),
            Err(e) => {
                if !self.attempt_repair {
                    return Err(AosError::Validation(format!(
                        "Failed to parse synthesis output: {}",
                        e
                    )));
                }
                tracing::debug!("Direct parse failed, attempting repair: {}", e);
            }
        }

        // Try to repair common issues
        let repaired = repair_json(json_str);
        match serde_json::from_str::<SynthesisOutput>(&repaired) {
            Ok(output) => {
                tracing::debug!("JSON repair successful");
                Ok(output)
            }
            Err(e) => {
                // Try partial extraction as last resort
                match extract_partial_output(json_str) {
                    Some(output) if !output.is_empty() => {
                        tracing::debug!(
                            "Partial extraction recovered {} examples",
                            output.total_examples()
                        );
                        Ok(output)
                    }
                    _ => Err(AosError::Validation(format!(
                        "Failed to parse synthesis output after repair: {}",
                        e
                    ))),
                }
            }
        }
    }
}

/// Convenience function to parse synthesis output
pub fn parse_synthesis_output(raw_output: &str) -> Result<SynthesisOutput> {
    SynthesisOutputParser::default().parse(raw_output)
}

/// Extract JSON from markdown code block
fn extract_json_from_markdown(text: &str) -> Option<String> {
    static JSON_BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    let re = JSON_BLOCK_RE
        .get_or_init(|| Regex::new(r"```(?:json)?\s*([\s\S]*?)\s*```").expect("valid regex"));

    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// Extract raw JSON object from text
fn extract_raw_json_object(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // Find the start of a JSON object
    let start = trimmed.find('{')?;
    let chars: Vec<char> = trimmed[start..].chars().collect();

    // Find matching closing brace
    let mut depth = 0;
    let mut end = None;

    for (i, c) in chars.iter().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }

    end.map(|e| chars[..e].iter().collect())
}

/// Attempt to repair common JSON issues
fn repair_json(json_str: &str) -> String {
    let mut result = json_str.to_string();

    // Remove trailing commas before ] or }
    static TRAILING_COMMA_RE: OnceLock<Regex> = OnceLock::new();
    let re = TRAILING_COMMA_RE.get_or_init(|| Regex::new(r",(\s*[}\]])").expect("valid regex"));
    result = re.replace_all(&result, "$1").to_string();

    // Fix unquoted keys (simple cases)
    static UNQUOTED_KEY_RE: OnceLock<Regex> = OnceLock::new();
    let key_re = UNQUOTED_KEY_RE
        .get_or_init(|| Regex::new(r"(\{|,)\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*:").expect("valid regex"));
    result = key_re.replace_all(&result, r#"$1"$2":"#).to_string();

    // Fix single quotes to double quotes
    result = fix_single_quotes(&result);

    result
}

/// Convert single quotes to double quotes (basic handling)
fn fix_single_quotes(json_str: &str) -> String {
    let mut result = String::with_capacity(json_str.len());
    let mut in_double_string = false;
    let mut prev_char = '\0';

    for c in json_str.chars() {
        match c {
            '"' if prev_char != '\\' => {
                in_double_string = !in_double_string;
                result.push(c);
            }
            '\'' if !in_double_string && prev_char != '\\' => {
                result.push('"');
            }
            _ => result.push(c),
        }
        prev_char = c;
    }

    result
}

/// Extract partial output when full parsing fails
fn extract_partial_output(json_str: &str) -> Option<SynthesisOutput> {
    // Try to parse as a generic JSON value first
    let value: Value = serde_json::from_str(json_str).ok()?;

    let mut output = SynthesisOutput::default();

    // Try to extract qa_pairs
    if let Some(qa_array) = value.get("qa_pairs").and_then(|v| v.as_array()) {
        for item in qa_array {
            if let (Some(q), Some(a)) = (
                item.get("question").and_then(|v| v.as_str()),
                item.get("answer").and_then(|v| v.as_str()),
            ) {
                output.qa_pairs.push(QAPair {
                    question: q.to_string(),
                    answer: a.to_string(),
                    relevance: item
                        .get("relevance")
                        .and_then(|v| v.as_f64())
                        .map(|f| f as f32),
                });
            }
        }
    }

    // Try to extract instructions
    if let Some(inst_array) = value.get("instructions").and_then(|v| v.as_array()) {
        for item in inst_array {
            if let (Some(inst), Some(resp)) = (
                item.get("instruction").and_then(|v| v.as_str()),
                item.get("response").and_then(|v| v.as_str()),
            ) {
                output.instructions.push(InstructionExample {
                    instruction: inst.to_string(),
                    response: resp.to_string(),
                    instruction_type: item
                        .get("instruction_type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // Try to extract completions
    if let Some(comp_array) = value.get("completions").and_then(|v| v.as_array()) {
        for item in comp_array {
            if let (Some(ctx), Some(cont)) = (
                item.get("context").and_then(|v| v.as_str()),
                item.get("continuation").and_then(|v| v.as_str()),
            ) {
                output.completions.push(CompletionExample {
                    context: ctx.to_string(),
                    continuation: cont.to_string(),
                });
            }
        }
    }

    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_json() {
        let input = r#"{
            "qa_pairs": [
                {"question": "What is X?", "answer": "X is Y"}
            ],
            "instructions": [
                {"instruction": "Explain X", "response": "X is..."}
            ],
            "completions": [
                {"context": "The system", "continuation": "uses X"}
            ]
        }"#;

        let output = parse_synthesis_output(input).unwrap();
        assert_eq!(output.qa_pairs.len(), 1);
        assert_eq!(output.instructions.len(), 1);
        assert_eq!(output.completions.len(), 1);
    }

    #[test]
    fn test_parse_markdown_block() {
        let input = r#"Here is the output:

```json
{
    "qa_pairs": [{"question": "Q1", "answer": "A1"}],
    "instructions": [],
    "completions": []
}
```

That's all."#;

        let output = parse_synthesis_output(input).unwrap();
        assert_eq!(output.qa_pairs.len(), 1);
        assert_eq!(output.qa_pairs[0].question, "Q1");
    }

    #[test]
    fn test_extract_json_object() {
        let input = "Some text { \"key\": \"value\" } more text";
        let json = extract_raw_json_object(input).unwrap();
        assert_eq!(json, r#"{ "key": "value" }"#);
    }

    #[test]
    fn test_repair_trailing_comma() {
        let input = r#"{"qa_pairs": [{"question": "Q", "answer": "A",}],}"#;
        let repaired = repair_json(input);
        assert!(serde_json::from_str::<Value>(&repaired).is_ok());
    }

    #[test]
    fn test_empty_output() {
        let input = r#"{"qa_pairs": [], "instructions": [], "completions": []}"#;
        let output = parse_synthesis_output(input).unwrap();
        assert!(output.is_empty());
        assert_eq!(output.total_examples(), 0);
    }
}
