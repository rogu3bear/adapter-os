//! Determinism verification for embedding generation
//!
//! Provides utilities for verifying that embedding generation is deterministic
//! across runs with the same seed. This is critical for reproducible inference
//! and audit trails.

use crate::model::EmbeddingModel;
use adapteros_core::{B3Hash, Result};

/// Result of determinism verification
#[derive(Debug, Clone)]
pub struct DeterminismReport {
    /// Total number of verification runs performed
    pub total_runs: usize,
    /// Total number of test inputs
    pub total_inputs: usize,
    /// Whether all verifications passed
    pub passed: bool,
    /// List of failures encountered
    pub failures: Vec<DeterminismFailure>,
}

impl DeterminismReport {
    /// Create a new passing report
    pub fn passing(total_runs: usize, total_inputs: usize) -> Self {
        Self {
            total_runs,
            total_inputs,
            passed: true,
            failures: Vec::new(),
        }
    }

    /// Create a new failing report
    pub fn failing(
        total_runs: usize,
        total_inputs: usize,
        failures: Vec<DeterminismFailure>,
    ) -> Self {
        Self {
            total_runs,
            total_inputs,
            passed: false,
            failures,
        }
    }
}

/// Details of a determinism verification failure
#[derive(Debug, Clone)]
pub struct DeterminismFailure {
    /// Hash of the input text
    pub input_hash: B3Hash,
    /// Hash of the embedding from run A
    pub run_a_hash: B3Hash,
    /// Hash of the embedding from run B
    pub run_b_hash: B3Hash,
    /// Run number where the failure occurred
    pub run_number: usize,
}

impl DeterminismFailure {
    /// Create a new determinism failure
    pub fn new(
        input_hash: B3Hash,
        run_a_hash: B3Hash,
        run_b_hash: B3Hash,
        run_number: usize,
    ) -> Self {
        Self {
            input_hash,
            run_a_hash,
            run_b_hash,
            run_number,
        }
    }
}

/// Verify embedding model produces deterministic outputs
///
/// This function runs multiple embedding operations on the same inputs
/// and verifies that all outputs have identical vector hashes.
///
/// # Arguments
/// * `model` - The embedding model to verify
/// * `test_inputs` - Slice of test input strings
/// * `num_runs` - Number of times to run each input (minimum 2)
///
/// # Returns
/// A `DeterminismReport` containing verification results
///
/// # Example
/// ```ignore
/// let report = verify_determinism(&model, &["hello", "world"], 10).await?;
/// assert!(report.passed, "Model produced non-deterministic outputs");
/// ```
pub async fn verify_determinism<M: EmbeddingModel>(
    model: &M,
    test_inputs: &[&str],
    num_runs: usize,
) -> Result<DeterminismReport> {
    // Need at least 2 runs to verify determinism
    let num_runs = num_runs.max(2);
    let mut failures = Vec::new();

    for input in test_inputs {
        let input_hash = B3Hash::hash(input.as_bytes());
        let baseline = model.embed(input).await?;
        let baseline_hash = baseline.vector_hash();

        for run in 1..num_runs {
            let result = model.embed(input).await?;
            let result_hash = result.vector_hash();

            if baseline_hash != result_hash {
                failures.push(DeterminismFailure::new(
                    input_hash,
                    baseline_hash,
                    result_hash,
                    run,
                ));
                // Stop checking this input after first failure
                break;
            }
        }
    }

    let passed = failures.is_empty();
    Ok(DeterminismReport {
        total_runs: num_runs,
        total_inputs: test_inputs.len(),
        passed,
        failures,
    })
}

/// Verify determinism with custom hash comparison
///
/// This variant allows specifying a custom tolerance for floating-point
/// comparison. Useful when exact bit-for-bit comparison is too strict.
///
/// # Arguments
/// * `model` - The embedding model to verify
/// * `test_inputs` - Slice of test input strings
/// * `num_runs` - Number of times to run each input
/// * `tolerance` - Maximum allowed L2 distance between vectors (0.0 for exact match)
pub async fn verify_determinism_with_tolerance<M: EmbeddingModel>(
    model: &M,
    test_inputs: &[&str],
    num_runs: usize,
    tolerance: f32,
) -> Result<DeterminismReport> {
    let num_runs = num_runs.max(2);
    let mut failures = Vec::new();

    for input in test_inputs {
        let input_hash = B3Hash::hash(input.as_bytes());
        let baseline = model.embed(input).await?;

        for run in 1..num_runs {
            let result = model.embed(input).await?;

            let distance = l2_distance(&baseline.vector, &result.vector);
            if distance > tolerance {
                failures.push(DeterminismFailure::new(
                    input_hash,
                    baseline.vector_hash(),
                    result.vector_hash(),
                    run,
                ));
                break;
            }
        }
    }

    let passed = failures.is_empty();
    Ok(DeterminismReport {
        total_runs: num_runs,
        total_inputs: test_inputs.len(),
        passed,
        failures,
    })
}

/// Compute L2 (Euclidean) distance between two vectors
fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Builder for determinism verification with configurable options
#[derive(Debug, Clone)]
pub struct DeterminismVerifier<'a> {
    test_inputs: &'a [&'a str],
    num_runs: usize,
    tolerance: Option<f32>,
    stop_on_first_failure: bool,
}

impl<'a> DeterminismVerifier<'a> {
    /// Create a new verifier with default options
    pub fn new(test_inputs: &'a [&'a str]) -> Self {
        Self {
            test_inputs,
            num_runs: 10,
            tolerance: None,
            stop_on_first_failure: true,
        }
    }

    /// Set the number of verification runs
    pub fn num_runs(mut self, runs: usize) -> Self {
        self.num_runs = runs;
        self
    }

    /// Set tolerance for floating-point comparison (None for exact match)
    pub fn tolerance(mut self, tol: Option<f32>) -> Self {
        self.tolerance = tol;
        self
    }

    /// Whether to stop checking an input after first failure
    pub fn stop_on_first_failure(mut self, stop: bool) -> Self {
        self.stop_on_first_failure = stop;
        self
    }

    /// Run verification against the given model
    pub async fn verify<M: EmbeddingModel>(&self, model: &M) -> Result<DeterminismReport> {
        match self.tolerance {
            Some(tol) => {
                verify_determinism_with_tolerance(model, self.test_inputs, self.num_runs, tol).await
            }
            None => verify_determinism(model, self.test_inputs, self.num_runs).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Embedding;
    use async_trait::async_trait;

    /// Mock model that always produces deterministic outputs
    struct DeterministicMockModel {
        dim: usize,
        model_hash: B3Hash,
    }

    impl DeterministicMockModel {
        fn new(dim: usize) -> Self {
            Self {
                dim,
                model_hash: B3Hash::hash(b"deterministic_mock"),
            }
        }
    }

    #[async_trait]
    impl EmbeddingModel for DeterministicMockModel {
        async fn embed(&self, text: &str) -> Result<Embedding> {
            // Deterministic: same input always produces same output
            let input_hash = B3Hash::hash(text.as_bytes());
            let seed = u64::from_le_bytes(input_hash.as_bytes()[..8].try_into().unwrap());
            let vector: Vec<f32> = (0..self.dim)
                .map(|i| ((seed.wrapping_add(i as u64) % 1000) as f32) / 1000.0)
                .collect();
            Ok(Embedding::new(vector, self.model_hash, input_hash))
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.embed(text).await?);
            }
            Ok(results)
        }

        fn model_hash(&self) -> &B3Hash {
            &self.model_hash
        }

        fn tokenizer_hash(&self) -> &B3Hash {
            &self.model_hash
        }

        fn embedding_dimension(&self) -> usize {
            self.dim
        }
    }

    /// Mock model that produces non-deterministic outputs
    struct NonDeterministicMockModel {
        dim: usize,
        model_hash: B3Hash,
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl NonDeterministicMockModel {
        fn new(dim: usize) -> Self {
            Self {
                dim,
                model_hash: B3Hash::hash(b"nondeterministic_mock"),
                call_count: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl EmbeddingModel for NonDeterministicMockModel {
        async fn embed(&self, text: &str) -> Result<Embedding> {
            let input_hash = B3Hash::hash(text.as_bytes());
            // Non-deterministic: include call count in output
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let vector: Vec<f32> = (0..self.dim).map(|i| (i + count) as f32 / 1000.0).collect();
            Ok(Embedding::new(vector, self.model_hash, input_hash))
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.embed(text).await?);
            }
            Ok(results)
        }

        fn model_hash(&self) -> &B3Hash {
            &self.model_hash
        }

        fn tokenizer_hash(&self) -> &B3Hash {
            &self.model_hash
        }

        fn embedding_dimension(&self) -> usize {
            self.dim
        }
    }

    #[tokio::test]
    async fn test_determinism_verification_passes() {
        let model = DeterministicMockModel::new(384);
        let inputs = vec!["hello", "world", "test"];
        let report = verify_determinism(&model, &inputs, 10).await.unwrap();
        assert!(report.passed);
        assert!(report.failures.is_empty());
        assert_eq!(report.total_inputs, 3);
        assert_eq!(report.total_runs, 10);
    }

    #[tokio::test]
    async fn test_determinism_verification_fails() {
        let model = NonDeterministicMockModel::new(384);
        let inputs = vec!["hello"];
        let report = verify_determinism(&model, &inputs, 5).await.unwrap();
        assert!(!report.passed);
        assert!(!report.failures.is_empty());
    }

    #[tokio::test]
    async fn test_determinism_with_tolerance() {
        let model = DeterministicMockModel::new(384);
        let inputs = vec!["test"];
        let report = verify_determinism_with_tolerance(&model, &inputs, 5, 0.001)
            .await
            .unwrap();
        assert!(report.passed);
    }

    #[tokio::test]
    async fn test_determinism_verifier_builder() {
        let model = DeterministicMockModel::new(384);
        let inputs = vec!["hello", "world"];
        let report = DeterminismVerifier::new(&inputs)
            .num_runs(5)
            .stop_on_first_failure(true)
            .verify(&model)
            .await
            .unwrap();
        assert!(report.passed);
    }

    #[test]
    fn test_l2_distance() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let dist = l2_distance(&a, &b);
        assert!((dist - std::f32::consts::SQRT_2).abs() < 1e-6);

        let c = vec![1.0, 2.0, 3.0];
        let d = vec![1.0, 2.0, 3.0];
        assert!((l2_distance(&c, &d)).abs() < 1e-9);
    }

    #[test]
    fn test_report_constructors() {
        let passing = DeterminismReport::passing(10, 5);
        assert!(passing.passed);
        assert!(passing.failures.is_empty());

        let failure = DeterminismFailure::new(
            B3Hash::hash(b"input"),
            B3Hash::hash(b"a"),
            B3Hash::hash(b"b"),
            1,
        );
        let failing = DeterminismReport::failing(10, 5, vec![failure]);
        assert!(!failing.passed);
        assert_eq!(failing.failures.len(), 1);
    }
}
