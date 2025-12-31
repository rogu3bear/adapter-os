//! Path aware routing utilities.
//!
//! Directory adapters rely on path prefixes.  This module scores adapters
//! based on how well their prefixes match path-like tokens extracted from
//! the query.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryRoutingContext {
    pub adapter_id: String,
    pub path_prefix: String,
    pub rank: u8,
    pub depth: usize,
    pub language_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathRoutingScore {
    pub adapter_id: String,
    pub score: f32,
    pub rank: u8,
    pub matched_token: Option<String>,
}

pub fn compute_path_scores(
    query: &str,
    contexts: &[DirectoryRoutingContext],
    weight: f32,
) -> Vec<PathRoutingScore> {
    let tokens = extract_path_tokens(query);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    let mut scores = Vec::with_capacity(contexts.len());
    for context in contexts {
        let prefix = context.path_prefix.to_lowercase();
        let mut best_score = 0.0f32;
        let mut matched_token = None;

        for token in lower_tokens.iter() {
            if token.starts_with(&prefix) {
                let token_depth = depth_of(token);
                let diff = token_depth.saturating_sub(context.depth);
                let token_score = 1.0 / (1.0 + diff as f32);
                if token_score > best_score {
                    best_score = token_score;
                    matched_token = Some(token.clone());
                }
            } else if prefix.contains(token) {
                let token_score = 0.5;
                if token_score > best_score {
                    best_score = token_score;
                    matched_token = Some(token.clone());
                }
            }
        }

        if let Some(lang) = &context.language_hint {
            if query.to_lowercase().contains(&lang.to_lowercase()) {
                best_score += 0.2;
            }
        }

        let rank_bonus = 1.0 / (context.rank.max(1) as f32);
        let final_score = (best_score + rank_bonus * 0.3).max(0.0) * weight;
        scores.push(PathRoutingScore {
            adapter_id: context.adapter_id.clone(),
            score: (final_score * 100.0).round() / 100.0,
            rank: context.rank,
            matched_token,
        });
    }

    scores.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.adapter_id.cmp(&b.adapter_id))
    });
    scores
}

fn extract_path_tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|token| {
            let cleaned = token
                .trim_matches(|c: char| "\"'`.,;:()[]{}".contains(c))
                .replace('\\', "/");
            if cleaned.contains('/') || cleaned.contains('.') {
                Some(cleaned)
            } else {
                None
            }
        })
        .collect()
}

fn depth_of(path: &str) -> usize {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_matching_prefixes() {
        let contexts = vec![DirectoryRoutingContext {
            adapter_id: "dir::api".into(),
            path_prefix: "src/api".into(),
            rank: 18,
            depth: 2,
            language_hint: Some("python".into()),
        }];
        let scores = compute_path_scores(
            "Refactor src/api/routes.py to new python style",
            &contexts,
            0.15,
        );
        assert_eq!(scores[0].adapter_id, "dir::api");
        assert!(scores[0].score > 0.0);
        assert!(scores[0].matched_token.is_some());
    }
}
