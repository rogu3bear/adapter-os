//! Benchmark harness for embedding evaluation
//!
//! Provides:
//! - BenchmarkConfig for configuring benchmark runs
//! - BenchmarkHarness for running benchmarks and verifying determinism
//! - BenchmarkReport for storing and serializing results

use crate::eval::EvalQuery;
use crate::index::SearchResult;
use crate::receipt::RetrievalReceipt;
use adapteros_core::B3Hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Evaluation queries with ground truth
    pub eval_queries: Vec<EvalQuery>,
    /// K values to compute Recall@K for
    pub k_values: Vec<usize>,
    /// Batch sizes to measure throughput at
    pub batch_sizes: Vec<usize>,
    /// Number of runs for determinism verification
    pub num_determinism_runs: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            eval_queries: vec![],
            k_values: vec![5, 10, 20],
            batch_sizes: vec![1, 8, 32],
            num_determinism_runs: 100,
        }
    }
}

/// Full benchmark report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// Unique report identifier
    pub report_id: String,
    /// Timestamp when report was generated
    pub timestamp: DateTime<Utc>,

    // Model info
    /// BLAKE3 hash of the embedding model
    pub model_hash: B3Hash,
    /// Human-readable model name
    pub model_name: String,
    /// Whether the model is a fine-tuned variant
    pub is_finetuned: bool,
    /// Hash of LoRA adapter if fine-tuned
    pub lora_adapter_hash: Option<B3Hash>,

    // Corpus info
    /// Deterministic version hash of the corpus
    pub corpus_version_hash: B3Hash,
    /// Number of chunks in the corpus
    pub num_chunks: usize,

    // Retrieval metrics
    /// Recall@K for each K value
    pub recall_at_k: HashMap<usize, f64>,
    /// Normalized Discounted Cumulative Gain at 10
    pub ndcg_at_10: f64,
    /// Mean Reciprocal Rank at 10
    pub mrr_at_10: f64,

    // System metrics
    /// 50th percentile embedding latency in milliseconds
    pub embed_latency_p50_ms: f64,
    /// 99th percentile embedding latency in milliseconds
    pub embed_latency_p99_ms: f64,
    /// Throughput (queries/sec) for each batch size
    pub throughput_per_sec: HashMap<usize, f64>,
    /// Resident set size memory usage in MB
    pub memory_rss_mb: f64,
    /// Time to build the index in milliseconds
    pub index_build_time_ms: f64,
    /// Index size in bytes
    pub index_size_bytes: u64,

    // Determinism verification
    /// Whether all determinism checks passed
    pub determinism_pass: bool,
    /// Number of determinism verification runs
    pub determinism_runs: usize,
    /// Descriptions of any determinism failures
    pub determinism_failures: Vec<String>,

    // All receipts from the benchmark
    /// Retrieval receipts for audit trail
    pub receipts: Vec<RetrievalReceipt>,
}

/// Determinism verification result
#[derive(Debug)]
pub struct DeterminismReport {
    /// Total number of runs performed
    pub total_runs: usize,
    /// Total number of queries verified
    pub total_queries: usize,
    /// Whether all checks passed
    pub passed: bool,
    /// Descriptions of failures (empty if passed)
    pub failures: Vec<String>,
}

/// Benchmark harness for running evaluations
pub struct BenchmarkHarness {
    config: BenchmarkConfig,
}

impl BenchmarkHarness {
    /// Create a new benchmark harness with the given configuration
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Get the benchmark configuration
    pub fn config(&self) -> &BenchmarkConfig {
        &self.config
    }

    /// Verify determinism across multiple runs
    ///
    /// Takes results from multiple runs and verifies that all runs
    /// produced identical results (same chunk IDs and scores).
    ///
    /// # Arguments
    /// * `results_by_run` - Outer vec: runs, middle vec: queries, inner vec: results
    ///
    /// # Returns
    /// DeterminismReport indicating pass/fail and any failures
    pub fn verify_determinism(
        &self,
        results_by_run: &[Vec<Vec<SearchResult>>],
    ) -> DeterminismReport {
        let num_runs = results_by_run.len();
        if num_runs < 2 {
            return DeterminismReport {
                total_runs: num_runs,
                total_queries: if num_runs == 0 {
                    0
                } else {
                    results_by_run[0].len()
                },
                passed: true,
                failures: vec![],
            };
        }

        let num_queries = results_by_run[0].len();
        let mut failures = vec![];

        let baseline = &results_by_run[0];
        for (run_idx, run) in results_by_run.iter().enumerate().skip(1) {
            if run.len() != num_queries {
                failures.push(format!(
                    "Run {} has {} queries, expected {}",
                    run_idx,
                    run.len(),
                    num_queries
                ));
                continue;
            }

            for (query_idx, (base, current)) in baseline.iter().zip(run).enumerate() {
                if !Self::results_match(base, current) {
                    failures.push(format!(
                        "Query {} diverged on run {} vs baseline",
                        query_idx, run_idx
                    ));
                }
            }
        }

        DeterminismReport {
            total_runs: num_runs,
            total_queries: num_queries,
            passed: failures.is_empty(),
            failures,
        }
    }

    /// Check if two result sets match (same chunk IDs and scores within tolerance)
    fn results_match(a: &[SearchResult], b: &[SearchResult]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (ra, rb) in a.iter().zip(b) {
            if ra.chunk_id != rb.chunk_id {
                return false;
            }
            // Use f32 epsilon for score comparison (scores are f32)
            if (ra.score - rb.score).abs() > 1e-6 {
                return false;
            }
        }
        true
    }

    /// Compute percentile from sorted latencies
    ///
    /// # Arguments
    /// * `sorted` - Pre-sorted array of latency values
    /// * `p` - Percentile to compute (0-100)
    ///
    /// # Returns
    /// The value at the given percentile
    pub fn percentile(sorted: &[f64], p: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_config_default() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.k_values, vec![5, 10, 20]);
        assert_eq!(config.batch_sizes, vec![1, 8, 32]);
        assert_eq!(config.num_determinism_runs, 100);
        assert!(config.eval_queries.is_empty());
    }

    #[test]
    fn test_benchmark_config_serialization() {
        let config = BenchmarkConfig {
            eval_queries: vec![],
            k_values: vec![5, 10],
            batch_sizes: vec![1, 4],
            num_determinism_runs: 50,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: BenchmarkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.k_values, vec![5, 10]);
        assert_eq!(parsed.num_determinism_runs, 50);
    }

    #[test]
    fn test_benchmark_report_serialization() {
        let report = BenchmarkReport {
            report_id: "test".to_string(),
            timestamp: Utc::now(),
            model_hash: B3Hash::hash(b"model"),
            model_name: "test-model".to_string(),
            is_finetuned: false,
            lora_adapter_hash: None,
            corpus_version_hash: B3Hash::hash(b"corpus"),
            num_chunks: 100,
            recall_at_k: [(5, 0.8), (10, 0.9)].into_iter().collect(),
            ndcg_at_10: 0.85,
            mrr_at_10: 0.75,
            embed_latency_p50_ms: 10.0,
            embed_latency_p99_ms: 25.0,
            throughput_per_sec: [(1, 100.0), (8, 500.0)].into_iter().collect(),
            memory_rss_mb: 512.0,
            index_build_time_ms: 1000.0,
            index_size_bytes: 1024 * 1024,
            determinism_pass: true,
            determinism_runs: 100,
            determinism_failures: vec![],
            receipts: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: BenchmarkReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model_name, "test-model");
        assert_eq!(parsed.num_chunks, 100);
        assert!(parsed.determinism_pass);
    }

    #[test]
    fn test_benchmark_report_with_lora() {
        let report = BenchmarkReport {
            report_id: "finetuned".to_string(),
            timestamp: Utc::now(),
            model_hash: B3Hash::hash(b"model"),
            model_name: "finetuned-model".to_string(),
            is_finetuned: true,
            lora_adapter_hash: Some(B3Hash::hash(b"lora")),
            corpus_version_hash: B3Hash::hash(b"corpus"),
            num_chunks: 50,
            recall_at_k: HashMap::new(),
            ndcg_at_10: 0.0,
            mrr_at_10: 0.0,
            embed_latency_p50_ms: 0.0,
            embed_latency_p99_ms: 0.0,
            throughput_per_sec: HashMap::new(),
            memory_rss_mb: 0.0,
            index_build_time_ms: 0.0,
            index_size_bytes: 0,
            determinism_pass: true,
            determinism_runs: 0,
            determinism_failures: vec![],
            receipts: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: BenchmarkReport = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_finetuned);
        assert!(parsed.lora_adapter_hash.is_some());
    }

    #[test]
    fn test_determinism_verification_pass() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            }]],
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            }]],
        ];
        let report = harness.verify_determinism(&results);
        assert!(report.passed);
        assert_eq!(report.total_runs, 2);
        assert_eq!(report.total_queries, 1);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn test_determinism_verification_fail_chunk_id() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            }]],
            vec![vec![SearchResult {
                chunk_id: "b".to_string(),
                score: 0.9,
                rank: 0,
            }]], // Different chunk_id
        ];
        let report = harness.verify_determinism(&results);
        assert!(!report.passed);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].contains("diverged"));
    }

    #[test]
    fn test_determinism_verification_fail_score() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            }]],
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.5,
                rank: 0,
            }]], // Different score
        ];
        let report = harness.verify_determinism(&results);
        assert!(!report.passed);
    }

    #[test]
    fn test_determinism_verification_single_run() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![vec![vec![SearchResult {
            chunk_id: "a".to_string(),
            score: 0.9,
            rank: 0,
        }]]];
        let report = harness.verify_determinism(&results);
        // Single run always passes (nothing to compare)
        assert!(report.passed);
        assert_eq!(report.total_runs, 1);
    }

    #[test]
    fn test_determinism_verification_empty() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results: Vec<Vec<Vec<SearchResult>>> = vec![];
        let report = harness.verify_determinism(&results);
        assert!(report.passed);
        assert_eq!(report.total_runs, 0);
        assert_eq!(report.total_queries, 0);
    }

    #[test]
    fn test_determinism_verification_multiple_queries() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![
            vec![
                vec![SearchResult {
                    chunk_id: "a".to_string(),
                    score: 0.9,
                    rank: 0,
                }],
                vec![SearchResult {
                    chunk_id: "b".to_string(),
                    score: 0.8,
                    rank: 0,
                }],
            ],
            vec![
                vec![SearchResult {
                    chunk_id: "a".to_string(),
                    score: 0.9,
                    rank: 0,
                }],
                vec![SearchResult {
                    chunk_id: "b".to_string(),
                    score: 0.8,
                    rank: 0,
                }],
            ],
        ];
        let report = harness.verify_determinism(&results);
        assert!(report.passed);
        assert_eq!(report.total_queries, 2);
    }

    #[test]
    fn test_determinism_verification_length_mismatch() {
        let harness = BenchmarkHarness::new(BenchmarkConfig::default());
        let results = vec![
            vec![vec![SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            }]],
            vec![], // Different number of queries
        ];
        let report = harness.verify_determinism(&results);
        assert!(!report.passed);
        assert!(report.failures[0].contains("queries"));
    }

    #[test]
    fn test_percentile() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((BenchmarkHarness::percentile(&sorted, 50.0) - 3.0).abs() < 1e-6);
        assert!((BenchmarkHarness::percentile(&sorted, 0.0) - 1.0).abs() < 1e-6);
        assert!((BenchmarkHarness::percentile(&sorted, 100.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_percentile_single_element() {
        let sorted = vec![42.0];
        assert!((BenchmarkHarness::percentile(&sorted, 0.0) - 42.0).abs() < 1e-6);
        assert!((BenchmarkHarness::percentile(&sorted, 50.0) - 42.0).abs() < 1e-6);
        assert!((BenchmarkHarness::percentile(&sorted, 100.0) - 42.0).abs() < 1e-6);
    }

    #[test]
    fn test_percentile_empty() {
        let sorted: Vec<f64> = vec![];
        assert!((BenchmarkHarness::percentile(&sorted, 50.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_percentile_p99() {
        let sorted: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        // p99 of 1..100 should be close to 99
        let p99 = BenchmarkHarness::percentile(&sorted, 99.0);
        assert!(p99 >= 98.0 && p99 <= 100.0);
    }

    #[test]
    fn test_harness_config_accessor() {
        let config = BenchmarkConfig {
            eval_queries: vec![],
            k_values: vec![1, 2, 3],
            batch_sizes: vec![4, 5],
            num_determinism_runs: 10,
        };
        let harness = BenchmarkHarness::new(config);
        assert_eq!(harness.config().k_values, vec![1, 2, 3]);
        assert_eq!(harness.config().batch_sizes, vec![4, 5]);
    }

    #[test]
    fn test_results_match_different_lengths() {
        let a = vec![SearchResult {
            chunk_id: "a".to_string(),
            score: 0.9,
            rank: 0,
        }];
        let b = vec![
            SearchResult {
                chunk_id: "a".to_string(),
                score: 0.9,
                rank: 0,
            },
            SearchResult {
                chunk_id: "b".to_string(),
                score: 0.8,
                rank: 1,
            },
        ];
        assert!(!BenchmarkHarness::results_match(&a, &b));
    }

    #[test]
    fn test_results_match_score_within_tolerance() {
        let a = vec![SearchResult {
            chunk_id: "a".to_string(),
            score: 0.9,
            rank: 0,
        }];
        let b = vec![SearchResult {
            chunk_id: "a".to_string(),
            score: 0.9 + 1e-8, // Within 1e-6 tolerance
            rank: 0,
        }];
        assert!(BenchmarkHarness::results_match(&a, &b));
    }
}
