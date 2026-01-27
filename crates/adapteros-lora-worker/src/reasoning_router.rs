//! Reasoning Router logic for streaming inference.
//!
//! Provides a lightweight reasoning loop that inspects streamed tokens,
//! detects thought boundaries, scores transitions between adapter clusters,
//! and emits hot-swap decisions with debounce and shadow-mode support.
#![allow(clippy::useless_vec)]

use std::collections::HashMap;

use adapteros_core::{cosine_similarity, normalize};
use blake3::Hasher;
use std::sync::Arc;
use tracing::{debug, info};

pub use crate::ane_embedder::TinyBertEmbedder;

/// Default token that signals explicit reasoning boundary.
pub const DEFAULT_THINKING_TOKEN: &str = "<thinking>";
const DEFAULT_EMBED_DIM: usize = 32;
const EPS: f32 = 1e-6;

/// Configuration for the reasoning loop.
#[derive(Debug, Clone)]
pub struct ReasoningRouterConfig {
    pub confidence_threshold: f32,
    pub debounce_tokens: usize,
    pub shadow_mode: bool,
    pub thinking_token: String,
    /// Maximum characters to keep in the rolling buffer.
    pub analysis_window: usize,
    /// Type of embedder to use.
    pub embedder_type: EmbedderType,
    /// Path to the embedder model (if using TinyBert).
    pub model_path: Option<String>,
}

/// Supported embedder types for the reasoning router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedderType {
    /// Legacy hash-based projection (semantic noise).
    Hashed,
    /// Tiny-BERT model pinned to ANE (semantic understanding).
    TinyBert,
}

impl Default for ReasoningRouterConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.82,
            debounce_tokens: 50,
            shadow_mode: false,
            thinking_token: DEFAULT_THINKING_TOKEN.to_string(),
            analysis_window: 1024,
            embedder_type: EmbedderType::Hashed,
            model_path: None,
        }
    }
}

impl ReasoningRouterConfig {
    /// Create an embedder based on configuration with robust fallback.
    pub fn create_embedder(&self) -> Arc<Embedder> {
        match self.embedder_type {
            EmbedderType::Hashed => Arc::new(Embedder::Hashed(FastEmbedder::default_quantized())),
            EmbedderType::TinyBert => {
                let model_path = self
                    .model_path
                    .as_ref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        std::path::PathBuf::from("var/models/tiny-bert-4bit-ane.mlpackage")
                    });

                // Attempt to load TinyBert
                match TinyBertEmbedder::load(&model_path, None) {
                    Ok(e) => {
                        info!("Loaded Tiny-BERT embedder from {:?}", model_path);
                        Arc::new(Embedder::TinyBert(Box::new(e)))
                    }
                    Err(e) => {
                        // Fallback to Hashed on failure
                        tracing::warn!(
                            "Failed to load Tiny-BERT embedder from {:?}, falling back to Hashed: {}",
                            model_path,
                            e
                        );
                        Arc::new(Embedder::Hashed(FastEmbedder::default_quantized()))
                    }
                }
            }
        }
    }
}

/// Unified embedder interface for the reasoning router.
pub enum Embedder {
    Hashed(FastEmbedder),
    TinyBert(Box<TinyBertEmbedder>),
}

impl Embedder {
    pub fn embed(&self, text: &str) -> Vec<f32> {
        match self {
            Self::Hashed(e) => e.embed(text),
            Self::TinyBert(e) => e.embed(text),
        }
    }

    pub fn dim(&self) -> usize {
        match self {
            Self::Hashed(e) => e.dim(),
            Self::TinyBert(e) => e.dimension(),
        }
    }
}

impl std::fmt::Debug for Embedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hashed(e) => f.debug_tuple("Hashed").field(e).finish(),
            Self::TinyBert(_) => f.debug_tuple("TinyBert").finish(),
        }
    }
}

/// Quantized, resident embedder for fast text-to-vector projection.
#[derive(Debug, Clone)]
pub struct FastEmbedder {
    dim: usize,
}

impl FastEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    pub fn default_quantized() -> Self {
        Self::new(DEFAULT_EMBED_DIM)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Deterministic, quantized embedding using a hashed projection.
    ///
    /// Uses i8-scaled hash buckets to approximate a tiny embedding model without
    /// external weights. This keeps the model resident and fast.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if text.trim().is_empty() {
            return vec![0.0; self.dim];
        }

        let mut accum = vec![0f32; self.dim];

        for (t_idx, token) in text.split_whitespace().enumerate() {
            let mut hasher = Hasher::new();
            hasher.update(token.as_bytes());
            let hash = hasher.finalize().as_bytes().to_owned();
            for d in 0..self.dim {
                let raw = hash[(d + t_idx) % hash.len()] as i8;
                accum[d] += raw as f32 / 127.0;
            }
        }

        normalize(&accum)
    }
}

/// Topology prior describing transition probabilities between clusters.
#[derive(Debug, Clone)]
pub struct TopologyPrior {
    transitions: HashMap<(String, String), f32>,
    default_prob: f32,
}

impl TopologyPrior {
    pub fn new(default_prob: f32) -> Self {
        Self {
            transitions: HashMap::new(),
            default_prob,
        }
    }

    pub fn probability(&self, from: &str, to: &str) -> f32 {
        self.transitions
            .get(&(from.to_string(), to.to_string()))
            .copied()
            .unwrap_or(self.default_prob)
            .clamp(0.0, 1.0)
    }

    pub fn with_transition(mut self, from: &str, to: &str, prob: f32) -> Self {
        self.transitions
            .insert((from.to_string(), to.to_string()), prob.clamp(0.0, 1.0));
        self
    }
}

impl Default for TopologyPrior {
    fn default() -> Self {
        Self::new(0.5)
    }
}

/// Combined score for a potential transition.
#[derive(Debug, Clone)]
pub struct TransitionScore {
    pub target: Option<String>,
    pub confidence: f32,
    pub semantic: f32,
    pub topology: f32,
}

/// Scorer that blends semantic similarity with topology priors.
#[derive(Debug, Clone)]
pub struct ReasoningScorer {
    clusters: HashMap<String, Vec<f32>>,
    topology: TopologyPrior,
    semantic_weight: f32,
    topology_weight: f32,
}

impl ReasoningScorer {
    pub fn new(
        clusters: HashMap<String, Vec<f32>>,
        topology: TopologyPrior,
        semantic_weight: f32,
        topology_weight: f32,
    ) -> Self {
        Self {
            clusters,
            topology,
            semantic_weight,
            topology_weight,
        }
    }

    pub fn from_adapter_ids(adapter_ids: &[String], embedder: &Embedder) -> Self {
        let clusters = adapter_ids
            .iter()
            .map(|id| (id.clone(), embedder.embed(id)))
            .collect();
        Self::new(clusters, TopologyPrior::default(), 0.7, 0.3)
    }

    pub fn score_transition(
        &self,
        current_cluster: &str,
        thought_vector: &[f32],
    ) -> TransitionScore {
        let mut best_target: Option<String> = None;
        let mut best_conf = 0.0;
        let mut best_semantic = 0.0;
        let mut best_topology = 0.0;

        // Sort clusters by key for deterministic iteration order
        let mut sorted_clusters: Vec<_> = self.clusters.iter().collect();
        sorted_clusters.sort_by_key(|(name, _)| name.as_str());

        for (name, centroid) in sorted_clusters {
            if centroid.len() != thought_vector.len() {
                continue;
            }

            let semantic = cosine_similarity(thought_vector, centroid);
            let topology = self.topology.probability(current_cluster, name);
            let combined = self.semantic_weight * semantic + self.topology_weight * topology;

            if combined > best_conf {
                best_conf = combined;
                best_semantic = semantic;
                best_topology = topology;
                best_target = Some(name.clone());
            }
        }

        TransitionScore {
            target: best_target,
            confidence: best_conf,
            semantic: best_semantic,
            topology: best_topology,
        }
    }
}

/// Convenience wrapper to match the requested free function signature.
pub fn score_transition(
    current_cluster: &str,
    thought_vector: &[f32],
    scorer: &ReasoningScorer,
) -> TransitionScore {
    scorer.score_transition(current_cluster, thought_vector)
}

/// Concrete transition emitted by the inspector.
#[derive(Debug, Clone)]
pub struct ThoughtTransition {
    pub from: String,
    pub to: String,
    pub thought: String,
    pub confidence: f32,
    pub semantic: f32,
    pub topology: f32,
    pub token_index: usize,
}

/// Result of a reasoning decision.
#[derive(Debug, Clone)]
pub struct HotSwapDecision {
    pub transition: ThoughtTransition,
    pub shadow_mode: bool,
}

/// Streaming inspector that watches tokens for thought boundaries.
#[derive(Debug, Clone)]
pub struct StreamInspector {
    buffer: String,
    scorer: ReasoningScorer,
    embedder: Arc<Embedder>,
    config: ReasoningRouterConfig,
    current_cluster: String,
    last_swap_token: Option<usize>,
}

impl StreamInspector {
    pub fn new(
        initial_cluster: String,
        scorer: ReasoningScorer,
        embedder: Arc<Embedder>,
        config: ReasoningRouterConfig,
    ) -> Self {
        Self {
            buffer: String::new(),
            scorer,
            embedder,
            config,
            current_cluster: initial_cluster,
            last_swap_token: None,
        }
    }

    pub fn current_cluster(&self) -> &str {
        &self.current_cluster
    }

    /// Process a streamed token, returning a hot-swap decision when warranted.
    pub fn on_token(&mut self, token: &str, token_index: usize) -> Option<HotSwapDecision> {
        self.buffer.push_str(token);
        if self.buffer.len() > self.config.analysis_window {
            let keep_from = self.buffer.len() - self.config.analysis_window;
            // Use drain to avoid allocation on hot path
            self.buffer.drain(..keep_from);
        }

        if !self.is_boundary_token(token) {
            return None;
        }

        let thought = self.buffer.trim().to_string();
        self.buffer.clear();
        if thought.is_empty() {
            return None;
        }

        let thought_vector = self.embedder.embed(&thought);
        let score = self
            .scorer
            .score_transition(&self.current_cluster, &thought_vector);

        let target = match &score.target {
            Some(t) if score.confidence >= self.config.confidence_threshold => t.clone(),
            _ => {
                debug!(
                    from = %self.current_cluster,
                    confidence = score.confidence,
                    "Reasoning decision below threshold, staying on current adapter"
                );
                return None;
            }
        };

        let cooldown_ready = self
            .last_swap_token
            .is_none_or(|last| token_index.saturating_sub(last) >= self.config.debounce_tokens);

        info!(
            "Reasoning decision: [Thought: \"{}\"] -> [Transition: {} -> {}] -> [Score: {:.2}]",
            truncate(&thought, 120),
            self.current_cluster,
            target,
            score.confidence
        );

        if !cooldown_ready {
            debug!(
                last_swap = ?self.last_swap_token,
                debounce_tokens = self.config.debounce_tokens,
                "Hot-swap suppressed by debounce window"
            );
            return None;
        }

        let transition = ThoughtTransition {
            from: self.current_cluster.clone(),
            to: target.clone(),
            thought,
            confidence: score.confidence,
            semantic: score.semantic,
            topology: score.topology,
            token_index,
        };

        if !self.config.shadow_mode {
            self.last_swap_token = Some(token_index);
            self.current_cluster = target;
        }

        Some(HotSwapDecision {
            transition,
            shadow_mode: self.config.shadow_mode,
        })
    }

    fn is_boundary_token(&self, token: &str) -> bool {
        token.contains('\n') || token.trim() == self.config.thinking_token
    }
}

// cosine_similarity and normalize are imported from adapteros_core::vector_math

fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    format!("{}...", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn scorer_prefers_math_cluster() {
        let mut clusters = HashMap::new();
        clusters.insert("creative".to_string(), vec![1.0, 0.0]);
        clusters.insert("math".to_string(), vec![0.0, 1.0]);

        let topology = TopologyPrior::default().with_transition("creative", "math", 0.9);
        let scorer = ReasoningScorer::new(clusters, topology, 0.7, 0.3);
        let thought = vec![0.05, 0.95];

        let score = scorer.score_transition("creative", &thought);
        assert_eq!(score.target.as_deref(), Some("math"));
        assert!(score.confidence > 0.8);
        assert!(score.semantic > 0.8);
    }

    #[test]
    fn stream_inspector_triggers_on_mock_stream() {
        let clusters = HashMap::from([
            ("creative".to_string(), vec![1.0, 0.0]),
            ("math".to_string(), vec![0.0, 1.0]),
        ]);
        let topology = TopologyPrior::default().with_transition("creative", "math", 0.9);
        let scorer = ReasoningScorer::new(clusters, topology, 0.7, 0.3);
        let embedder = Arc::new(Embedder::Hashed(FastEmbedder::new(2)));
        let mut inspector = StreamInspector::new(
            "creative".to_string(),
            scorer,
            embedder,
            ReasoningRouterConfig {
                debounce_tokens: 2,
                confidence_threshold: 0.5,
                shadow_mode: false,
                thinking_token: DEFAULT_THINKING_TOKEN.to_string(),
                analysis_window: 256,
                embedder_type: EmbedderType::Hashed,
                model_path: None,
            },
        );

        let stream = vec!["Let's plan", "\n", "Compute 2+2", "\n", "<thinking>"];
        let mut transitions = Vec::new();
        for (idx, token) in stream.iter().enumerate() {
            if let Some(decision) = inspector.on_token(token, idx) {
                transitions.push(decision.transition);
            }
        }

        assert!(
            transitions.iter().any(|t| t.to == "math"),
            "expected math transition"
        );
    }

    #[test]
    fn create_embedder_fallback_test() {
        // Config for TinyBert with non-existent model
        let config = ReasoningRouterConfig {
            embedder_type: EmbedderType::TinyBert,
            model_path: Some("non/existent/path".to_string()),
            ..Default::default()
        };

        // Should fallback to Hashed
        let embedder = config.create_embedder();
        match *embedder {
            Embedder::Hashed(_) => assert!(true),
            _ => assert!(false, "Should fallback to Hashed"),
        }
    }
}
