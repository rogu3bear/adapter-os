//! Evaluation metrics for retrieval quality
//!
//! Implements:
//! - Recall@K
//! - nDCG (normalized Discounted Cumulative Gain)
//! - MRR (Mean Reciprocal Rank)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Compute Recall@K
///
/// Measures the proportion of relevant documents that appear in the top-K retrieved results.
///
/// # Arguments
/// * `relevant` - List of relevant document IDs (ground truth)
/// * `retrieved` - List of retrieved document IDs (ranked by relevance)
/// * `k` - Number of top results to consider
///
/// # Returns
/// Recall value between 0.0 and 1.0
pub fn recall_at_k(relevant: &[String], retrieved: &[String], k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let top_k: HashSet<_> = retrieved.iter().take(k).collect();
    let found = relevant.iter().filter(|r| top_k.contains(r)).count();
    found as f64 / relevant.len() as f64
}

/// Compute Mean Reciprocal Rank (MRR)
///
/// MRR measures the rank position of the first relevant document.
/// Returns 1/rank where rank is 1-indexed position of first relevant doc.
///
/// # Arguments
/// * `relevant` - List of relevant document IDs (ground truth)
/// * `retrieved` - List of retrieved document IDs (ranked by relevance)
///
/// # Returns
/// MRR value between 0.0 and 1.0
pub fn mrr(relevant: &[String], retrieved: &[String]) -> f64 {
    let relevant_set: HashSet<_> = relevant.iter().collect();
    for (i, doc) in retrieved.iter().enumerate() {
        if relevant_set.contains(doc) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

/// Compute nDCG@K (Normalized Discounted Cumulative Gain)
///
/// nDCG measures ranking quality by considering both relevance and position.
/// Uses binary relevance (1 if relevant, 0 otherwise).
///
/// # Arguments
/// * `relevant` - List of relevant document IDs (ground truth)
/// * `retrieved` - List of retrieved document IDs (ranked by relevance)
/// * `k` - Number of top results to consider
///
/// # Returns
/// nDCG value between 0.0 and 1.0
pub fn ndcg_at_k(relevant: &[String], retrieved: &[String], k: usize) -> f64 {
    let relevant_set: HashSet<_> = relevant.iter().collect();

    // Compute DCG (Discounted Cumulative Gain)
    let dcg: f64 = retrieved
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, doc)| {
            let rel = if relevant_set.contains(doc) {
                1.0
            } else {
                0.0
            };
            rel / (i as f64 + 2.0).log2()
        })
        .sum();

    // Compute ideal DCG (all relevant docs at top)
    let ideal_k = k.min(relevant.len());
    let idcg: f64 = (0..ideal_k).map(|i| 1.0 / (i as f64 + 2.0).log2()).sum();

    if idcg < 1e-9 {
        return 0.0;
    }
    dcg / idcg
}

/// Source of an evaluation query
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum QuerySource {
    /// Query was automatically generated from a document
    Generated {
        /// Document ID the query was generated from
        from_doc: String,
    },
    /// Query was manually created by a human annotator
    Manual {
        /// Identifier of the annotator
        annotator: String,
    },
}

/// Evaluation query with ground truth relevance labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQuery {
    /// Unique identifier for this query
    pub query_id: String,
    /// The query text
    pub query_text: String,
    /// IDs of chunks that are relevant to this query (ground truth)
    pub relevant_chunk_ids: Vec<String>,
    /// Optional hard negatives (similar but not relevant chunks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_negatives: Option<Vec<String>>,
    /// How this query was created
    pub source: QuerySource,
}

/// Aggregated evaluation results across multiple queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResults {
    /// Mean Recall@5 across all queries
    pub recall_at_5: f64,
    /// Mean Recall@10 across all queries
    pub recall_at_10: f64,
    /// Mean Recall@20 across all queries
    pub recall_at_20: f64,
    /// Mean nDCG@10 across all queries
    pub ndcg_at_10: f64,
    /// Mean MRR across all queries (considering top 10)
    pub mrr_at_10: f64,
    /// Number of queries evaluated
    pub num_queries: usize,
}

impl EvalResults {
    /// Compute aggregated evaluation metrics from queries and their retrieval results
    ///
    /// # Arguments
    /// * `queries` - Evaluation queries with ground truth
    /// * `results` - Retrieved document IDs for each query (parallel to queries)
    ///
    /// # Returns
    /// Aggregated metrics across all queries
    pub fn compute(queries: &[EvalQuery], results: &[Vec<String>]) -> Self {
        let n = queries.len();
        if n == 0 {
            return Self {
                recall_at_5: 0.0,
                recall_at_10: 0.0,
                recall_at_20: 0.0,
                ndcg_at_10: 0.0,
                mrr_at_10: 0.0,
                num_queries: 0,
            };
        }

        let (mut r5, mut r10, mut r20, mut ndcg, mut m) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (q, r) in queries.iter().zip(results) {
            r5 += recall_at_k(&q.relevant_chunk_ids, r, 5);
            r10 += recall_at_k(&q.relevant_chunk_ids, r, 10);
            r20 += recall_at_k(&q.relevant_chunk_ids, r, 20);
            ndcg += ndcg_at_k(&q.relevant_chunk_ids, r, 10);
            m += mrr(&q.relevant_chunk_ids, r);
        }

        Self {
            recall_at_5: r5 / n as f64,
            recall_at_10: r10 / n as f64,
            recall_at_20: r20 / n as f64,
            ndcg_at_10: ndcg / n as f64,
            mrr_at_10: m / n as f64,
            num_queries: n,
        }
    }
}

impl Default for EvalResults {
    fn default() -> Self {
        Self {
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            recall_at_20: 0.0,
            ndcg_at_10: 0.0,
            mrr_at_10: 0.0,
            num_queries: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recall_at_k() {
        let relevant = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let retrieved = vec!["a".to_string(), "d".to_string(), "b".to_string()];

        // At k=1: only "a" retrieved, 1 of 3 relevant found
        assert!((recall_at_k(&relevant, &retrieved, 1) - 1.0 / 3.0).abs() < 1e-6);
        // At k=2: "a" and "d" retrieved, still only 1 of 3 relevant found
        assert!((recall_at_k(&relevant, &retrieved, 2) - 1.0 / 3.0).abs() < 1e-6);
        // At k=3: "a", "d", "b" retrieved, 2 of 3 relevant found
        assert!((recall_at_k(&relevant, &retrieved, 3) - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_recall_at_k_empty() {
        let relevant: Vec<String> = vec![];
        let retrieved = vec!["a".to_string()];
        assert!((recall_at_k(&relevant, &retrieved, 5) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_recall_at_k_perfect() {
        let relevant = vec!["a".to_string(), "b".to_string()];
        let retrieved = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!((recall_at_k(&relevant, &retrieved, 2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_mrr() {
        // First relevant at position 1
        let relevant1 = vec!["a".to_string()];
        let retrieved1 = vec!["a".to_string(), "b".to_string()];
        assert!((mrr(&relevant1, &retrieved1) - 1.0).abs() < 1e-6);

        // First relevant at position 2
        let relevant2 = vec!["b".to_string()];
        let retrieved2 = vec!["a".to_string(), "b".to_string()];
        assert!((mrr(&relevant2, &retrieved2) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_mrr_not_found() {
        let relevant = vec!["x".to_string()];
        let retrieved = vec!["a".to_string(), "b".to_string()];
        assert!((mrr(&relevant, &retrieved) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_mrr_multiple_relevant() {
        // MRR only considers first relevant doc
        let relevant = vec!["b".to_string(), "c".to_string()];
        let retrieved = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // First relevant "b" is at position 2
        assert!((mrr(&relevant, &retrieved) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_ndcg() {
        let relevant = vec!["a".to_string(), "b".to_string()];
        let retrieved = vec!["a".to_string(), "c".to_string(), "b".to_string()];
        let ndcg = ndcg_at_k(&relevant, &retrieved, 3);
        // "a" at pos 0, "b" at pos 2 (not perfect, but close)
        assert!(ndcg > 0.9 && ndcg < 1.0);
    }

    #[test]
    fn test_ndcg_perfect() {
        let relevant = vec!["a".to_string(), "b".to_string()];
        let retrieved = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ndcg = ndcg_at_k(&relevant, &retrieved, 3);
        // Perfect ranking: all relevant at top
        assert!((ndcg - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_ndcg_empty_relevant() {
        let relevant: Vec<String> = vec![];
        let retrieved = vec!["a".to_string()];
        let ndcg = ndcg_at_k(&relevant, &retrieved, 5);
        assert!((ndcg - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_ndcg_no_relevant_found() {
        let relevant = vec!["x".to_string(), "y".to_string()];
        let retrieved = vec!["a".to_string(), "b".to_string()];
        let ndcg = ndcg_at_k(&relevant, &retrieved, 2);
        // DCG = 0, so nDCG = 0
        assert!((ndcg - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_eval_results_compute() {
        let queries = vec![
            EvalQuery {
                query_id: "q1".to_string(),
                query_text: "test query 1".to_string(),
                relevant_chunk_ids: vec!["a".to_string(), "b".to_string()],
                hard_negatives: None,
                source: QuerySource::Manual {
                    annotator: "test".to_string(),
                },
            },
            EvalQuery {
                query_id: "q2".to_string(),
                query_text: "test query 2".to_string(),
                relevant_chunk_ids: vec!["c".to_string()],
                hard_negatives: Some(vec!["d".to_string()]),
                source: QuerySource::Generated {
                    from_doc: "doc1".to_string(),
                },
            },
        ];

        let results = vec![
            vec!["a".to_string(), "b".to_string(), "x".to_string()],
            vec!["c".to_string(), "d".to_string()],
        ];

        let eval = EvalResults::compute(&queries, &results);
        assert_eq!(eval.num_queries, 2);
        // Both queries have perfect recall@5
        assert!((eval.recall_at_5 - 1.0).abs() < 1e-6);
        // Both have first relevant at position 1, so MRR = 1.0
        assert!((eval.mrr_at_10 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_eval_results_empty() {
        let queries: Vec<EvalQuery> = vec![];
        let results: Vec<Vec<String>> = vec![];
        let eval = EvalResults::compute(&queries, &results);
        assert_eq!(eval.num_queries, 0);
        assert!((eval.recall_at_5 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_query_source_serialization() {
        let generated = QuerySource::Generated {
            from_doc: "doc1".to_string(),
        };
        let json = serde_json::to_string(&generated).unwrap();
        assert!(json.contains("Generated"));
        assert!(json.contains("doc1"));

        let manual = QuerySource::Manual {
            annotator: "alice".to_string(),
        };
        let json = serde_json::to_string(&manual).unwrap();
        assert!(json.contains("Manual"));
        assert!(json.contains("alice"));
    }

    #[test]
    fn test_eval_query_serialization() {
        let query = EvalQuery {
            query_id: "q1".to_string(),
            query_text: "What is Rust?".to_string(),
            relevant_chunk_ids: vec!["chunk1".to_string(), "chunk2".to_string()],
            hard_negatives: None,
            source: QuerySource::Manual {
                annotator: "test".to_string(),
            },
        };

        let json = serde_json::to_string(&query).unwrap();
        let parsed: EvalQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.query_id, "q1");
        assert_eq!(parsed.relevant_chunk_ids.len(), 2);
        assert!(parsed.hard_negatives.is_none());
    }

    #[test]
    fn test_eval_results_default() {
        let default = EvalResults::default();
        assert_eq!(default.num_queries, 0);
        assert!((default.recall_at_5 - 0.0).abs() < 1e-6);
        assert!((default.ndcg_at_10 - 0.0).abs() < 1e-6);
    }
}
