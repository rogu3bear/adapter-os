//! Language Policy Pack
//!
//! Enforces response language consistency with user input language.
//! Detects dominant language and ensures responses match.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported languages for detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    English,
    Spanish,
    French,
    German,
    Italian,
    Portuguese,
    Chinese,
    Japanese,
    Korean,
    Russian,
    Arabic,
    Unknown,
}

impl Language {
    /// Get ISO 639-1 code
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
            Language::French => "fr",
            Language::German => "de",
            Language::Italian => "it",
            Language::Portuguese => "pt",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Korean => "ko",
            Language::Russian => "ru",
            Language::Arabic => "ar",
            Language::Unknown => "und",
        }
    }

    /// Get language name
    pub fn name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Spanish",
            Language::French => "French",
            Language::German => "German",
            Language::Italian => "Italian",
            Language::Portuguese => "Portuguese",
            Language::Chinese => "Chinese",
            Language::Japanese => "Japanese",
            Language::Korean => "Korean",
            Language::Russian => "Russian",
            Language::Arabic => "Arabic",
            Language::Unknown => "Unknown",
        }
    }
}

/// Language detection patterns (common words/phrases)
static LANGUAGE_PATTERNS: Lazy<HashMap<Language, Vec<&'static str>>> = Lazy::new(|| {
    let mut patterns = HashMap::new();

    patterns.insert(
        Language::English,
        vec![
            "the", "is", "are", "was", "were", "have", "has", "been", "would", "could", "should",
            "will", "can", "this", "that", "with", "from", "they", "what", "which", "there",
            "their", "about", "would", "these", "other",
        ],
    );

    patterns.insert(
        Language::Spanish,
        vec![
            "el", "la", "los", "las", "es", "son", "está", "están", "que", "de", "en", "un", "una",
            "por", "para", "con", "como", "pero", "más", "muy", "también", "porque", "cuando",
            "donde", "quien",
        ],
    );

    patterns.insert(
        Language::French,
        vec![
            "le", "la", "les", "est", "sont", "été", "avoir", "être", "que", "de", "en", "un",
            "une", "pour", "avec", "comme", "mais", "plus", "très", "aussi", "parce", "quand",
            "où", "qui", "ce", "cette",
        ],
    );

    patterns.insert(
        Language::German,
        vec![
            "der", "die", "das", "ist", "sind", "war", "haben", "sein", "dass", "von", "in", "ein",
            "eine", "für", "mit", "wie", "aber", "mehr", "sehr", "auch", "weil", "wenn", "wo",
            "wer", "diese", "dieser",
        ],
    );

    patterns.insert(
        Language::Italian,
        vec![
            "il", "la", "lo", "gli", "le", "è", "sono", "stato", "essere", "che", "di", "in", "un",
            "una", "per", "con", "come", "ma", "più", "molto", "anche", "perché", "quando", "dove",
            "chi",
        ],
    );

    patterns.insert(
        Language::Portuguese,
        vec![
            "o", "a", "os", "as", "é", "são", "foi", "ter", "ser", "que", "de", "em", "um", "uma",
            "para", "com", "como", "mas", "mais", "muito", "também", "porque", "quando", "onde",
            "quem",
        ],
    );

    patterns
});

/// Language policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Enable language consistency checking
    pub enabled: bool,
    /// Minimum confidence for language detection (0.0 - 1.0)
    pub min_detection_confidence: f32,
    /// Allow responses in a different language if explicitly requested
    pub allow_explicit_language_switch: bool,
    /// Default language if detection fails
    pub default_language: Language,
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_detection_confidence: 0.3,
            allow_explicit_language_switch: true,
            default_language: Language::English,
        }
    }
}

/// Language detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDetectionResult {
    /// Detected language
    pub language: Language,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Number of language indicators found
    pub indicator_count: usize,
}

/// Language consistency check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConsistencyResult {
    /// Whether languages are consistent
    pub is_consistent: bool,
    /// Input language detection
    pub input_language: LanguageDetectionResult,
    /// Output language detection
    pub output_language: LanguageDetectionResult,
    /// Suggested action if inconsistent
    pub suggested_action: Option<String>,
}

/// Language policy implementation
pub struct LanguagePolicy {
    config: LanguageConfig,
}

impl LanguagePolicy {
    /// Create new language policy
    pub fn new(config: LanguageConfig) -> Self {
        Self { config }
    }

    /// Detect the dominant language of text
    pub fn detect_language(&self, text: &str) -> LanguageDetectionResult {
        if !self.config.enabled {
            return LanguageDetectionResult {
                language: self.config.default_language,
                confidence: 1.0,
                indicator_count: 0,
            };
        }

        // Check for CJK characters first (Chinese, Japanese, Korean)
        let cjk_result = self.detect_cjk(text);
        if cjk_result.confidence > 0.3 {
            return cjk_result;
        }

        // Check for Cyrillic (Russian)
        let cyrillic_count = text
            .chars()
            .filter(|c| matches!(*c, '\u{0400}'..='\u{04FF}'))
            .count();
        if cyrillic_count > text.len() / 10 {
            return LanguageDetectionResult {
                language: Language::Russian,
                confidence: (cyrillic_count as f32 / text.len() as f32).min(1.0),
                indicator_count: cyrillic_count,
            };
        }

        // Check for Arabic
        let arabic_count = text
            .chars()
            .filter(|c| matches!(*c, '\u{0600}'..='\u{06FF}'))
            .count();
        if arabic_count > text.len() / 10 {
            return LanguageDetectionResult {
                language: Language::Arabic,
                confidence: (arabic_count as f32 / text.len() as f32).min(1.0),
                indicator_count: arabic_count,
            };
        }

        // Pattern-based detection for Latin script languages
        let words: Vec<&str> = text.split_whitespace().collect();
        let word_count = words.len();

        if word_count == 0 {
            return LanguageDetectionResult {
                language: self.config.default_language,
                confidence: 0.0,
                indicator_count: 0,
            };
        }

        let mut scores: HashMap<Language, usize> = HashMap::new();

        for (lang, patterns) in LANGUAGE_PATTERNS.iter() {
            let count = words
                .iter()
                .filter(|word| {
                    let lower = word.to_lowercase();
                    patterns.iter().any(|p| lower == *p)
                })
                .count();
            if count > 0 {
                scores.insert(*lang, count);
            }
        }

        // Find the language with highest score
        if let Some((best_lang, best_count)) = scores.iter().max_by_key(|(_, count)| *count) {
            let confidence = (*best_count as f32 / word_count as f32).min(1.0);
            return LanguageDetectionResult {
                language: *best_lang,
                confidence,
                indicator_count: *best_count,
            };
        }

        LanguageDetectionResult {
            language: self.config.default_language,
            confidence: 0.0,
            indicator_count: 0,
        }
    }

    /// Detect CJK languages
    fn detect_cjk(&self, text: &str) -> LanguageDetectionResult {
        let cjk_chars: Vec<char> = text
            .chars()
            .filter(|c| {
                matches!(*c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
            })
            .collect();

        if cjk_chars.is_empty() {
            return LanguageDetectionResult {
                language: Language::Unknown,
                confidence: 0.0,
                indicator_count: 0,
            };
        }

        // Count specific script types
        let hiragana_katakana = cjk_chars
            .iter()
            .filter(|c| matches!(**c, '\u{3040}'..='\u{30FF}'))
            .count();
        let hangul = cjk_chars
            .iter()
            .filter(|c| matches!(**c, '\u{AC00}'..='\u{D7AF}'))
            .count();
        let han = cjk_chars
            .iter()
            .filter(|c| matches!(**c, '\u{4E00}'..='\u{9FFF}'))
            .count();

        let total = cjk_chars.len();
        let confidence = (total as f32 / text.chars().count() as f32).min(1.0);

        // Japanese uses hiragana/katakana mixed with kanji
        if hiragana_katakana > 0 {
            return LanguageDetectionResult {
                language: Language::Japanese,
                confidence,
                indicator_count: total,
            };
        }

        // Korean uses hangul
        if hangul > han {
            return LanguageDetectionResult {
                language: Language::Korean,
                confidence,
                indicator_count: total,
            };
        }

        // Default to Chinese for pure Han characters
        LanguageDetectionResult {
            language: Language::Chinese,
            confidence,
            indicator_count: total,
        }
    }

    /// Check language consistency between input and output
    pub fn check_consistency(&self, input: &str, output: &str) -> LanguageConsistencyResult {
        let input_lang = self.detect_language(input);
        let output_lang = self.detect_language(output);

        // Consider consistent if either detection is low confidence
        let is_consistent = input_lang.language == output_lang.language
            || input_lang.confidence < self.config.min_detection_confidence
            || output_lang.confidence < self.config.min_detection_confidence;

        let suggested_action = if !is_consistent {
            Some(format!(
                "Response is in {} but input was in {}. Consider matching the user's language.",
                output_lang.language.name(),
                input_lang.language.name()
            ))
        } else {
            None
        };

        LanguageConsistencyResult {
            is_consistent,
            input_language: input_lang,
            output_language: output_lang,
            suggested_action,
        }
    }
}

/// Context for language policy enforcement
#[derive(Debug)]
pub struct LanguageContext {
    pub input_text: String,
    pub output_text: String,
    pub tenant_id: String,
}

impl PolicyContext for LanguageContext {
    fn context_type(&self) -> &str {
        "language"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for LanguagePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Language
    }

    fn name(&self) -> &'static str {
        "Language"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let lang_ctx = ctx
            .as_any()
            .downcast_ref::<LanguageContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid language context".to_string()))?;

        let consistency = self.check_consistency(&lang_ctx.input_text, &lang_ctx.output_text);

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        if !consistency.is_consistent {
            if let Some(action) = consistency.suggested_action {
                violations.push(Violation {
                    severity: Severity::Medium,
                    message: "Language mismatch detected".to_string(),
                    details: Some(action),
                });
            }
        }

        // Add low confidence warning
        if consistency.input_language.confidence < self.config.min_detection_confidence {
            warnings.push(format!(
                "Input language detection confidence is low: {:.2}",
                consistency.input_language.confidence
            ));
        }

        Ok(Audit {
            policy_id: PolicyId::Language,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_english() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result = policy.detect_language("The quick brown fox jumps over the lazy dog.");

        assert_eq!(result.language, Language::English);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_detect_spanish() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result =
            policy.detect_language("El rápido zorro marrón salta sobre el perro perezoso.");

        assert_eq!(result.language, Language::Spanish);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_detect_french() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result =
            policy.detect_language("Le renard brun rapide saute par-dessus le chien paresseux.");

        assert_eq!(result.language, Language::French);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_detect_german() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result =
            policy.detect_language("Der schnelle braune Fuchs springt über den faulen Hund.");

        assert_eq!(result.language, Language::German);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_detect_chinese() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result = policy.detect_language("快速的棕色狐狸跳过懒狗");

        assert_eq!(result.language, Language::Chinese);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_detect_japanese() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result = policy.detect_language("すばやい茶色のキツネは怠惰な犬を飛び越える");

        assert_eq!(result.language, Language::Japanese);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_check_consistency_same_language() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result = policy.check_consistency(
            "What is the weather today?",
            "The weather today is sunny with a high of 75 degrees.",
        );

        assert!(result.is_consistent);
        assert!(result.suggested_action.is_none());
    }

    #[test]
    fn test_check_consistency_different_language() {
        let policy = LanguagePolicy::new(LanguageConfig::default());
        let result = policy.check_consistency(
            "What is the weather today?",
            "El clima de hoy es soleado con una temperatura máxima de 75 grados.",
        );

        assert!(!result.is_consistent);
        assert!(result.suggested_action.is_some());
    }

    #[test]
    fn test_language_codes() {
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::Spanish.code(), "es");
        assert_eq!(Language::Chinese.code(), "zh");
        assert_eq!(Language::Japanese.code(), "ja");
    }

    #[test]
    fn test_disabled_policy() {
        let config = LanguageConfig {
            enabled: false,
            ..Default::default()
        };
        let policy = LanguagePolicy::new(config);
        let result = policy.detect_language("任意のテキスト");

        // Should return default language when disabled
        assert_eq!(result.language, Language::English);
        assert_eq!(result.confidence, 1.0);
    }
}
