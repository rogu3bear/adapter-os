//! Vector mathematics utilities for embedding operations.
//!
//! This module provides common vector math operations used across the codebase,
//! centralized to avoid duplication and ensure consistent behavior.

/// Epsilon constant for numerical stability in vector operations.
pub const EPS: f32 = 1e-9;

/// Compute cosine similarity between two vectors.
///
/// Returns a value between -1.0 and 1.0, where:
/// - 1.0 means identical vectors (same direction)
/// - 0.0 means orthogonal vectors
/// - -1.0 means opposite vectors
///
/// # Arguments
/// * `a` - First vector
/// * `b` - Second vector
///
/// # Returns
/// * 0.0 if vectors have different lengths, are empty, or have zero magnitude
/// * Otherwise, the cosine similarity clamped to [-1.0, 1.0]
///
/// # Example
/// ```
/// use adapteros_core::vector_math::cosine_similarity;
///
/// let a = vec![1.0, 0.0, 0.0];
/// let b = vec![1.0, 0.0, 0.0];
/// assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
///
/// let c = vec![1.0, 0.0, 0.0];
/// let d = vec![0.0, 1.0, 0.0];
/// assert!(cosine_similarity(&c, &d).abs() < 1e-6);
/// ```
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = (norm_a.sqrt() * norm_b.sqrt()).max(EPS);
    (dot / denom).clamp(-1.0, 1.0)
}

/// Compute dot product of two vectors.
///
/// # Panics
/// Panics if vectors have different lengths.
#[inline]
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have same length");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Compute the L2 (Euclidean) norm of a vector.
#[inline]
pub fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Normalize a vector to unit length (L2 normalization).
///
/// Returns a zero vector if the input has zero magnitude.
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = l2_norm(v);
    if norm < EPS {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        // Zero vector should return 0.0 (not NaN or panic)
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_normalize() {
        let v = vec![3.0, 4.0];
        let normalized = normalize(&v);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_zero() {
        let v = vec![0.0, 0.0, 0.0];
        let normalized = normalize(&v);
        assert_eq!(normalized, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((dot_product(&a, &b) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_norm() {
        let v = vec![3.0, 4.0];
        assert!((l2_norm(&v) - 5.0).abs() < 1e-6);
    }
}
