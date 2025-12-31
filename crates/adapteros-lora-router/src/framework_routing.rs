//! Framework aware routing utilities.
//!
//! The router scores framework-specific adapters based on keyword matches
//! in the query and metadata about the framework detection confidence.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Context describing a framework adapter available to the router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkRoutingContext {
    pub adapter_id: String,
    pub framework: String,
    pub rank: u8,
    pub confidence: f32,
    pub activation_threshold: f32,
    pub keywords: Vec<String>,
}

/// Scored framework adapter ready for integration into K-sparse routing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrameworkRoutingScore {
    pub adapter_id: String,
    pub score: f32,
    pub rank: u8,
    pub matched_keywords: Vec<String>,
}

/// Compute scores for framework adapters given a natural language query.
/// The caller provides the router's framework weight so that scores can be
/// combined with other feature channels.
pub fn compute_framework_scores(
    query: &str,
    contexts: &[FrameworkRoutingContext],
    weight: f32,
) -> Vec<FrameworkRoutingScore> {
    let normalized = query.to_lowercase();
    let tokens = tokenize(&normalized);

    let mut scored = Vec::with_capacity(contexts.len());
    for context in contexts {
        let mut matched = Vec::new();
        for keyword in &context.keywords {
            let keyword_lower = keyword.to_lowercase();
            if tokens.contains(keyword_lower.as_str()) || normalized.contains(&keyword_lower) {
                matched.push(keyword.clone());
            }
        }

        let framework_match = normalized.contains(&context.framework.to_lowercase());
        let rank_factor = 1.0 / (context.rank.max(1) as f32);
        let keyword_score = matched.len() as f32 * 0.15;
        let base_score = context.confidence * 0.6 + keyword_score + rank_factor * 0.4;
        let final_score = (base_score + if framework_match { 0.2 } else { 0.0 }) * weight;

        scored.push(FrameworkRoutingScore {
            adapter_id: context.adapter_id.clone(),
            score: (final_score * 100.0).round() / 100.0,
            rank: context.rank,
            matched_keywords: matched,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.adapter_id.cmp(&b.adapter_id))
    });
    scored
}

fn tokenize(query: &str) -> BTreeSet<&str> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_frameworks_deterministically() {
        let contexts = vec![
            FrameworkRoutingContext {
                adapter_id: "a".into(),
                framework: "Django".into(),
                rank: 8,
                confidence: 0.9,
                activation_threshold: 0.8,
                keywords: vec!["orm".into(), "django".into()],
            },
            FrameworkRoutingContext {
                adapter_id: "b".into(),
                framework: "React".into(),
                rank: 9,
                confidence: 0.8,
                activation_threshold: 0.7,
                keywords: vec!["jsx".into(), "component".into()],
            },
        ];

        let scores = compute_framework_scores("build a React component with JSX", &contexts, 0.25);
        assert_eq!(scores[0].adapter_id, "b");
        assert!(scores[0].score >= scores[1].score);
    }
}
