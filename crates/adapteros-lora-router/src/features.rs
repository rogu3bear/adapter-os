//! Feature extraction for adapter routing.
//!
//! Extracts routing signals from input context. Features are **internal routing
//! signals**, not cryptographic commitments—they help select adapters but don't
//! belong in receipts.
//!
//! # Architecture
//!
//! ```text
//! Context String ──► extract() ──► Features ──► Router ──► Decision
//!       │                                                      │
//!       └──────────────────── Receipt ─────────────────────────┘
//!                          (context + output, NOT features)
//! ```
//!
//! Features are derived intermediates. Receipts should bind the true input
//! (context string) to the output (adapter indices + Q15 gates), not the
//! features used internally to make the routing decision.
//!
//! # Determinism
//!
//! Extraction is deterministic (same input → same features) via:
//! - BTreeMap for ordered iteration
//! - `total_cmp` for IEEE 754 float ordering
//!
//! # Insufficient Input
//!
//! Returns `None` when input is too short for reliable detection.
//! Callers should **abstain from routing**, not use neutral features.

use std::collections::BTreeMap;

// =============================================================================
// Constants
// =============================================================================

/// Current feature schema version.
///
/// Bump this when the feature vector layout changes to ensure
/// compatibility checks between stored/cached features and runtime.
///
/// Version history:
/// - v1: Initial 22-dim layout (lang[8], framework[3], symbols[1], paths[1], verb[8], entropy[1])
pub const FEATURE_SCHEMA_VERSION: u32 = 1;

/// Minimum input length (in characters) for reliable feature detection.
///
/// Inputs shorter than this threshold will use default/neutral features
/// to avoid unreliable detection from insufficient context.
pub const MIN_INPUT_LENGTH: usize = 20;

/// Code features for router scoring
#[derive(Debug, Clone)]
pub struct CodeFeatures {
    pub lang_one_hot: Vec<f32>,
    /// Framework priors for scoring.
    /// Uses BTreeMap for deterministic iteration order (critical for reproducible routing).
    /// HashMap iteration order is non-deterministic due to SipHash randomization.
    pub framework_prior: BTreeMap<String, f32>,
    pub symbol_hits: f32,
    pub path_tokens: Vec<String>,
    pub commit_hint: Option<String>,
    pub prompt_verb: PromptVerb,
    pub attn_entropy: Option<f32>,
}

/// Prompt verb classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptVerb {
    Explain,
    Implement,
    Fix,
    Refactor,
    Test,
    Document,
    Review,
    Unknown,
}

impl CodeFeatures {
    /// Create empty features.
    pub fn new() -> Self {
        Self {
            lang_one_hot: vec![0.0; 8],
            framework_prior: BTreeMap::new(),
            symbol_hits: 0.0,
            path_tokens: Vec::new(),
            commit_hint: None,
            prompt_verb: PromptVerb::Unknown,
            attn_entropy: None,
        }
    }

    /// Extract features from context.
    ///
    /// Returns `None` if input is too short for reliable detection.
    /// Callers should **abstain from routing** when this returns `None`,
    /// not substitute neutral features.
    ///
    /// # Determinism
    ///
    /// Same input always produces same output.
    pub fn extract(context: &str) -> Option<Self> {
        if context.len() < MIN_INPUT_LENGTH {
            return None;
        }
        Some(Self::from_context(context))
    }

    /// Extract features unconditionally (no length check).
    ///
    /// Use [`extract`] in production to handle short inputs correctly.
    pub fn from_context(context: &str) -> Self {
        let mut features = Self::new();
        features.lang_one_hot = extract_lang_one_hot(context);
        features.framework_prior = extract_framework_prior(context);
        features.symbol_hits = count_symbol_hits(context);
        features.path_tokens = extract_path_tokens(context);
        features.prompt_verb = classify_prompt_verb(context);
        features
    }

    /// Validate that feature scores are finite (not NaN or infinity).
    ///
    /// Issue D-5 Fix: Add validation to reject NaN scores for determinism.
    /// Non-finite scores can cause non-deterministic behavior in sorting/comparisons.
    ///
    /// Returns true if all feature values are finite, false otherwise.
    pub fn validate_finite(&self) -> bool {
        // Check lang_one_hot
        if self.lang_one_hot.iter().any(|v| !v.is_finite()) {
            return false;
        }
        // Check framework_prior
        if self.framework_prior.values().any(|v| !v.is_finite()) {
            return false;
        }
        // Check symbol_hits
        if !self.symbol_hits.is_finite() {
            return false;
        }
        // Check attn_entropy if present
        if let Some(entropy) = self.attn_entropy {
            if !entropy.is_finite() {
                return false;
            }
        }
        true
    }

    /// Convert to flat 22-dimensional vector for router scoring.
    ///
    /// # Layout
    ///
    /// | Index | Dimension | Description |
    /// |-------|-----------|-------------|
    /// | 0-7   | 8         | Language one-hot (normalized) |
    /// | 8-10  | 3         | Framework scores (top-3, sorted desc) |
    /// | 11    | 1         | Symbol density (normalized 0-1) |
    /// | 12    | 1         | Path token count (normalized 0-1) |
    /// | 13-20 | 8         | Prompt verb one-hot |
    /// | 21    | 1         | Attention entropy (0.0 if not set) |
    ///
    /// # Determinism
    ///
    /// Output is deterministic: same `CodeFeatures` always produces same vector.
    /// Framework scores use `total_cmp` for IEEE 754 consistent ordering.
    pub fn to_vector(&self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(22);

        // [0-7] Language one-hot (8 dimensions)
        vec.extend_from_slice(&self.lang_one_hot);

        // [8-10] Framework scores (3 dimensions, top-3 sorted descending)
        // Determinism: total_cmp for IEEE 754 total ordering (handles NaN deterministically)
        let mut framework_scores: Vec<f32> = self.framework_prior.values().copied().collect();
        framework_scores.sort_by(|a, b| b.total_cmp(a));
        framework_scores.truncate(3);
        while framework_scores.len() < 3 {
            framework_scores.push(0.0);
        }
        vec.extend_from_slice(&framework_scores);

        // [11] Symbol density (1 dimension, normalized to 0-1)
        vec.push((self.symbol_hits / 10.0).min(1.0));

        // [12] Path token count (1 dimension, normalized to 0-1)
        vec.push((self.path_tokens.len() as f32 / 5.0).min(1.0));

        // [13-20] Prompt verb one-hot (8 dimensions)
        vec.extend_from_slice(&prompt_verb_one_hot(self.prompt_verb));

        // [21] Attention entropy (1 dimension, 0.0 if not computed)
        vec.push(self.attn_entropy.unwrap_or(0.0));

        debug_assert_eq!(vec.len(), 22, "Feature vector must be 22 dimensions");
        vec
    }

    /// Convert to extended feature vector (25 dimensions) for DIR
    /// Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    ///
    /// Note: The extended features (orthogonal_penalty, adapter_diversity, path_similarity)
    /// require router state and should be set externally before calling this method.
    /// By default, they return 0.0.
    pub fn to_vector_extended(&self) -> Vec<f32> {
        let mut vec = self.to_vector(); // Start with original 22 dimensions

        // DIR (Deterministic Inference Runtime) extensions (3 additional dimensions)
        // These are placeholders that should be set by the router
        vec.push(0.0); // [22]: orthogonal penalty (set by router)
        vec.push(0.0); // [23]: adapter diversity (set by router)
        vec.push(0.0); // [24]: path similarity (set by router)

        vec
    }

    /// Set DIR extension values in an extended feature vector
    ///
    /// This method should be called by the router to inject DIR-specific
    /// features that require routing history.
    ///
    /// # Arguments
    /// * `vec` - Mutable reference to 25-dimensional feature vector
    /// * `orthogonal_penalty` - Penalty for selecting similar adapters
    /// * `adapter_diversity` - Diversity score of recent selections
    /// * `path_similarity` - Similarity to previous routing paths
    pub fn set_mplora_features(
        vec: &mut Vec<f32>,
        orthogonal_penalty: f32,
        adapter_diversity: f32,
        path_similarity: f32,
    ) {
        if vec.len() >= 25 {
            vec[22] = orthogonal_penalty;
            vec[23] = adapter_diversity;
            vec[24] = path_similarity;
        }
    }

    /// Compute path similarity based on path tokens
    ///
    /// This is a simplified version that doesn't require router history.
    /// It measures how similar the current path context is to typical patterns.
    ///
    /// Returns a value between 0.0 (unique path) and 1.0 (common path)
    pub fn compute_path_similarity_score(&self) -> f32 {
        if self.path_tokens.is_empty() {
            return 0.0;
        }

        // Common path patterns that indicate high similarity
        let common_paths = [
            "src", "lib", "main", "test", "tests", "bin", "pkg", "app", "api", "core", "utils",
            "common", "shared", "internal",
        ];

        let mut similarity_count = 0;
        for token in &self.path_tokens {
            let token_lower = token.to_lowercase();
            if common_paths
                .iter()
                .any(|&common| token_lower.contains(common))
            {
                similarity_count += 1;
            }
        }

        // Normalize by number of path tokens
        if self.path_tokens.is_empty() {
            0.0
        } else {
            (similarity_count as f32 / self.path_tokens.len() as f32).min(1.0)
        }
    }

    /// Set attention entropy from recent inference logits
    pub fn set_attn_entropy(&mut self, entropy: f32) {
        self.attn_entropy = Some(entropy);
    }
}

impl Default for CodeFeatures {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract language one-hot encoding from context
fn extract_lang_one_hot(context: &str) -> Vec<f32> {
    let mut one_hot = vec![0.0; 8];

    let context_lower = context.to_lowercase();

    // Language detection heuristics
    let languages = [
        ("python", 0, vec![".py", "python", "def ", "import "]),
        ("rust", 1, vec![".rs", "rust", "fn ", "use ", "impl "]),
        (
            "typescript",
            2,
            vec![".ts", "typescript", "interface ", "type "],
        ),
        (
            "javascript",
            3,
            vec![".js", "javascript", "const ", "function "],
        ),
        ("go", 4, vec![".go", "golang", "func ", "package "]),
        ("java", 5, vec![".java", "java", "class ", "public "]),
        ("c", 6, vec![".c", ".h", "#include", "void ", "int "]),
        ("c++", 7, vec![".cpp", ".hpp", "c++", "std::", "class "]),
    ];

    for (_, idx, keywords) in languages {
        for keyword in keywords {
            if context_lower.contains(keyword) {
                one_hot[idx] = 1.0;
                break;
            }
        }
    }

    // Normalize if multiple languages detected
    let sum: f32 = one_hot.iter().sum();
    if sum > 0.0 {
        for val in &mut one_hot {
            *val /= sum;
        }
    }

    one_hot
}

/// Extract framework priors from context.
/// Returns BTreeMap for deterministic iteration order in feature vector construction.
fn extract_framework_prior(context: &str) -> BTreeMap<String, f32> {
    let mut priors = BTreeMap::new();
    let context_lower = context.to_lowercase();

    let frameworks = [
        // Python
        ("django", vec!["django", "manage.py", "settings.py"]),
        ("flask", vec!["flask", "@app.route", "Flask("]),
        ("fastapi", vec!["fastapi", "@app.get", "FastAPI("]),
        ("pytest", vec!["pytest", "test_", "@pytest"]),
        ("pydantic", vec!["pydantic", "BaseModel", "Field("]),
        // Rust
        ("axum", vec!["axum", "Router", "extract::"]),
        ("tokio", vec!["tokio", "#[tokio::main]", "async fn"]),
        ("actix-web", vec!["actix", "HttpRequest", "HttpResponse"]),
        // JavaScript/TypeScript
        ("react", vec!["react", "useState", "useEffect", "jsx"]),
        ("nextjs", vec!["next", "getServerSideProps", "next.config"]),
        ("express", vec!["express", "app.get", "req.body"]),
        ("vue", vec!["vue", "v-if", "v-for", "<template>"]),
    ];

    for (name, keywords) in frameworks {
        let mut score = 0.0;
        for keyword in keywords {
            if context_lower.contains(keyword) {
                score += 1.0;
            }
        }
        if score > 0.0 {
            priors.insert(name.to_string(), score);
        }
    }

    priors
}

/// Count symbol hits (function names, class names, etc.)
fn count_symbol_hits(context: &str) -> f32 {
    let mut count = 0.0;

    // Simple heuristic: count patterns that look like symbols
    for word in context.split_whitespace() {
        // CamelCase or snake_case identifiers
        if word.len() > 2 {
            let has_underscore = word.contains('_');
            let has_mixed_case =
                word.chars().any(|c| c.is_uppercase()) && word.chars().any(|c| c.is_lowercase());

            if has_underscore || has_mixed_case {
                count += 1.0;
            }
        }
    }

    count
}

/// Extract path tokens from context
fn extract_path_tokens(context: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    // Look for file paths
    for word in context.split_whitespace() {
        if word.contains('/') || word.contains('\\') {
            // Split path into components
            for component in word.split(&['/', '\\'][..]) {
                if !component.is_empty() && component.len() > 1 {
                    tokens.push(component.to_string());
                }
            }
        }
    }

    tokens
}

/// Classify prompt verb
fn classify_prompt_verb(context: &str) -> PromptVerb {
    let context_lower = context.to_lowercase();

    if context_lower.contains("explain")
        || context_lower.contains("what is")
        || context_lower.contains("how does")
    {
        PromptVerb::Explain
    } else if context_lower.contains("implement")
        || context_lower.contains("add")
        || context_lower.contains("create")
    {
        PromptVerb::Implement
    } else if context_lower.contains("fix")
        || context_lower.contains("bug")
        || context_lower.contains("error")
    {
        PromptVerb::Fix
    } else if context_lower.contains("refactor")
        || context_lower.contains("improve")
        || context_lower.contains("optimize")
    {
        PromptVerb::Refactor
    } else if context_lower.contains("test") || context_lower.contains("verify") {
        PromptVerb::Test
    } else if context_lower.contains("document") || context_lower.contains("comment") {
        PromptVerb::Document
    } else if context_lower.contains("review") || context_lower.contains("check") {
        PromptVerb::Review
    } else {
        PromptVerb::Unknown
    }
}

/// Convert prompt verb to one-hot encoding
fn prompt_verb_one_hot(verb: PromptVerb) -> Vec<f32> {
    let mut one_hot = vec![0.0; 8];
    let idx = match verb {
        PromptVerb::Explain => 0,
        PromptVerb::Implement => 1,
        PromptVerb::Fix => 2,
        PromptVerb::Refactor => 3,
        PromptVerb::Test => 4,
        PromptVerb::Document => 5,
        PromptVerb::Review => 6,
        PromptVerb::Unknown => 7,
    };
    one_hot[idx] = 1.0;
    one_hot
}

/// Extract attention entropy from recent logits
///
/// Computes average entropy over last N tokens to detect model uncertainty.
/// High entropy with low evidence → abstain (Ruleset #5).
///
/// # Arguments
/// * `recent_logits` - Logit vectors from recent tokens (shape: [num_tokens, vocab_size])
/// * `window_size` - Number of recent tokens to consider (default: 8)
///
/// # Returns
/// Average entropy over the window (higher = more uncertain)
///
/// Reference: docs/code-intelligence/code-router-features.md lines 225-257
pub fn extract_attn_entropy(recent_logits: &[Vec<f32>], window_size: Option<usize>) -> f32 {
    if recent_logits.is_empty() {
        return 0.0;
    }

    let window = window_size.unwrap_or(8);
    let relevant = if recent_logits.len() > window {
        &recent_logits[recent_logits.len() - window..]
    } else {
        recent_logits
    };

    let entropies: Vec<f32> = relevant
        .iter()
        .map(|logits| {
            let probs = softmax(logits);
            compute_entropy(&probs)
        })
        .collect();

    if entropies.is_empty() {
        0.0
    } else {
        entropies.iter().sum::<f32>() / entropies.len() as f32
    }
}

/// Compute softmax normalization
fn softmax(logits: &[f32]) -> Vec<f32> {
    if logits.is_empty() {
        return Vec::new();
    }

    // Subtract max for numerical stability (f64 intermediate)
    let max_logit = logits
        .iter()
        .map(|&x| x as f64)
        .fold(f64::NEG_INFINITY, f64::max);

    // Kahan-summed exponentials in f64 to reduce rounding drift
    let mut sum = 0.0f64;
    let mut c = 0.0f64;
    let exp_logits: Vec<f64> = logits
        .iter()
        .map(|&x| {
            let exp = ((x as f64) - max_logit).exp();

            let y = exp - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;

            exp
        })
        .collect();

    if sum == 0.0 {
        return vec![1.0 / logits.len() as f32; logits.len()]; // Uniform if all zero
    }

    exp_logits.iter().map(|&x| (x / sum) as f32).collect()
}

/// Compute Shannon entropy of probability distribution
///
/// H(p) = -Σ p(x) * log2(p(x))
///
/// Higher entropy = more uncertain/uniform distribution
fn compute_entropy(probs: &[f32]) -> f32 {
    probs
        .iter()
        .filter(|&&p| p > 1e-9) // Skip near-zero probabilities to avoid log(0)
        .map(|&p| -p * p.log2())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Short Input Tests
    // =========================================================================

    #[test]
    fn test_extract_short_input_returns_none() {
        assert!(CodeFeatures::extract("tiny").is_none());
        assert!(CodeFeatures::extract(&"a".repeat(19)).is_none());
    }

    #[test]
    fn test_extract_sufficient_input_returns_some() {
        let features =
            CodeFeatures::extract("Fix this python bug in src/main.py with def function()")
                .expect("should extract from sufficient input");

        assert!(features.lang_one_hot[0] > 0.0); // Python detected
        assert_eq!(features.prompt_verb, PromptVerb::Fix);
    }

    // =========================================================================
    // Determinism Tests
    // =========================================================================

    #[test]
    fn test_extraction_determinism() {
        let context =
            "Fix the python FastAPI bug in src/api/handlers.py with async def endpoint():";

        let v1 = CodeFeatures::from_context(context).to_vector();
        let v2 = CodeFeatures::from_context(context).to_vector();

        for (a, b) in v1.iter().zip(v2.iter()) {
            assert!(
                a.to_bits() == b.to_bits(),
                "Feature vectors must be bit-identical"
            );
        }
    }

    // =========================================================================
    // Original Tests
    // =========================================================================

    #[test]
    fn test_extract_lang_one_hot() {
        let context = "Here is a python function:\ndef test(): pass";
        let one_hot = extract_lang_one_hot(context);
        assert!(one_hot[0] > 0.0, "Python should be detected");
    }

    #[test]
    fn test_framework_prior() {
        let context = "Using FastAPI with @app.get decorator";
        let priors = extract_framework_prior(context);
        assert!(priors.contains_key("fastapi"));
        assert!(priors["fastapi"] > 0.0);
    }

    #[test]
    fn test_symbol_hits() {
        let context = "call_function with snake_case and CamelCase";
        let hits = count_symbol_hits(context);
        assert!(hits >= 2.0);
    }

    #[test]
    fn test_path_tokens() {
        let context = "Look at src/main.rs and lib/utils.py";
        let tokens = extract_path_tokens(context);
        assert!(tokens.len() >= 4); // src, main.rs, lib, utils.py
    }

    #[test]
    fn test_prompt_verb_classification() {
        assert_eq!(
            classify_prompt_verb("Explain how this works"),
            PromptVerb::Explain
        );
        assert_eq!(classify_prompt_verb("Fix this bug"), PromptVerb::Fix);
        assert_eq!(
            classify_prompt_verb("Implement a new feature"),
            PromptVerb::Implement
        );
        assert_eq!(
            classify_prompt_verb("Refactor the code"),
            PromptVerb::Refactor
        );
    }

    #[test]
    fn test_features_to_vector() {
        let features = CodeFeatures::from_context("Fix this python bug in src/main.py");
        let vec = features.to_vector();

        // Should have: 8 (lang) + 3 (framework) + 1 (symbols) + 1 (paths) + 8 (verb) + 1 (entropy) = 22
        assert_eq!(vec.len(), 22);
    }

    #[test]
    fn test_extract_attn_entropy() {
        // Create mock logits with different entropy levels
        // Low entropy: peaked distribution
        let peaked_logits = vec![10.0, 0.0, 0.0, 0.0, 0.0];
        // High entropy: uniform distribution
        let uniform_logits = vec![1.0, 1.0, 1.0, 1.0, 1.0];

        let low_entropy_sequence = vec![peaked_logits.clone(), peaked_logits];
        let high_entropy_sequence = vec![uniform_logits.clone(), uniform_logits];

        let low_entropy = extract_attn_entropy(&low_entropy_sequence, None);
        let high_entropy = extract_attn_entropy(&high_entropy_sequence, None);

        // High entropy should be greater than low entropy
        assert!(high_entropy > low_entropy);
        assert!(low_entropy < 1.0); // Peaked distribution should have low entropy
        assert!(high_entropy > 2.0); // Uniform over 5 items has entropy ~2.32
    }

    #[test]
    fn test_softmax() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs = softmax(&logits);

        // Probabilities should sum to 1.0
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);

        // Higher logit should have higher probability
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_compute_entropy() {
        // Uniform distribution has maximum entropy
        let uniform = vec![0.25, 0.25, 0.25, 0.25];
        let uniform_entropy = compute_entropy(&uniform);

        // Peaked distribution has low entropy
        let peaked = vec![0.97, 0.01, 0.01, 0.01];
        let peaked_entropy = compute_entropy(&peaked);

        assert!(uniform_entropy > peaked_entropy);
        assert!((uniform_entropy - 2.0).abs() < 0.1); // log2(4) = 2.0
        assert!(peaked_entropy < 0.5);
    }

    #[test]
    fn test_entropy_with_features() {
        let mut features = CodeFeatures::from_context("Test code");

        // Create some logits to compute entropy
        let logits = vec![vec![5.0, 1.0, 1.0, 1.0], vec![4.0, 2.0, 1.0, 1.0]];

        let entropy = extract_attn_entropy(&logits, None);
        features.set_attn_entropy(entropy);

        assert!(features.attn_entropy.is_some());
        assert!(features.attn_entropy.unwrap() > 0.0);

        let vec = features.to_vector();
        // Last element should be the entropy value
        assert_eq!(*vec.last().unwrap(), entropy);
    }
}
