//! Embedding training loss functions.
//!
//! Provides contrastive learning loss functions for training semantic embedding models:
//! - **Triplet loss**: Learns to minimize anchor-positive distance while maximizing anchor-negative distance
//! - **Contrastive loss**: Uses explicit similarity labels for pairs
//! - **InfoNCE loss**: Efficient in-batch negative sampling (NT-Xent)

use adapteros_core::Result;

/// Compute cosine similarity between two vectors.
///
/// Returns a value in [-1, 1] where 1 means identical direction,
/// 0 means orthogonal, and -1 means opposite direction.
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    // Avoid division by zero
    let denom = norm_a * norm_b;
    if denom < 1e-8 {
        return 0.0;
    }

    dot / denom
}

/// Compute cosine distance between two vectors.
///
/// Returns a value in [0, 2] where 0 means identical direction.
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    1.0 - cosine_similarity(a, b)
}

/// Compute L2 (Euclidean) distance between two vectors.
#[inline]
pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Triplet margin loss.
///
/// Computes: `max(0, d(anchor, positive) - d(anchor, negative) + margin)`
///
/// This loss encourages the model to place the anchor closer to the positive
/// example than to the negative example, with at least `margin` separation.
///
/// # Arguments
/// * `anchor` - The anchor embedding
/// * `positive` - Embedding of a similar/related item
/// * `negative` - Embedding of a dissimilar/unrelated item
/// * `margin` - Minimum desired separation (typical: 0.5-1.0)
///
/// # Returns
/// Loss value >= 0. Zero when constraint is satisfied.
pub fn triplet_loss(anchor: &[f32], positive: &[f32], negative: &[f32], margin: f32) -> f32 {
    let d_pos = cosine_distance(anchor, positive);
    let d_neg = cosine_distance(anchor, negative);

    (d_pos - d_neg + margin).max(0.0)
}

/// Batch triplet loss with mean reduction.
///
/// Computes triplet loss for each (anchor, positive, negative) triplet
/// and returns the mean loss.
pub fn batch_triplet_loss(
    anchors: &[Vec<f32>],
    positives: &[Vec<f32>],
    negatives: &[Vec<f32>],
    margin: f32,
) -> f32 {
    debug_assert_eq!(anchors.len(), positives.len());
    debug_assert_eq!(anchors.len(), negatives.len());

    if anchors.is_empty() {
        return 0.0;
    }

    let total: f32 = anchors
        .iter()
        .zip(positives)
        .zip(negatives)
        .map(|((a, p), n)| triplet_loss(a, p, n, margin))
        .sum();

    total / anchors.len() as f32
}

/// Contrastive loss for pairs with similarity labels.
///
/// For similar pairs (label=1): `d(a, b)^2`
/// For dissimilar pairs (label=0): `max(0, margin - d(a, b))^2`
///
/// # Arguments
/// * `a`, `b` - The two embeddings
/// * `label` - 1.0 for similar pairs, 0.0 for dissimilar pairs
/// * `margin` - Minimum distance for dissimilar pairs (typical: 1.0)
pub fn contrastive_loss(a: &[f32], b: &[f32], label: f32, margin: f32) -> f32 {
    let dist = cosine_distance(a, b);

    // Similar pairs: minimize distance
    // Dissimilar pairs: maximize distance up to margin
    label * dist.powi(2) + (1.0 - label) * (margin - dist).max(0.0).powi(2)
}

/// InfoNCE (Noise Contrastive Estimation) loss.
///
/// Also known as NT-Xent (Normalized Temperature-scaled Cross Entropy).
/// Uses in-batch negatives for efficient contrastive learning.
///
/// For a batch of N embedding pairs (query_i, positive_i):
/// - Each query's positive is the corresponding positive
/// - All other positives in the batch are negatives
///
/// Loss = -log(exp(sim(q_i, p_i)/τ) / Σ_j exp(sim(q_i, p_j)/τ))
///
/// # Arguments
/// * `queries` - Query embeddings [batch_size, dim]
/// * `positives` - Positive embeddings [batch_size, dim]
/// * `temperature` - Temperature scaling (typical: 0.07)
///
/// # Returns
/// Mean InfoNCE loss across the batch.
pub fn info_nce_loss(queries: &[Vec<f32>], positives: &[Vec<f32>], temperature: f32) -> f32 {
    debug_assert_eq!(queries.len(), positives.len());
    debug_assert!(temperature > 0.0, "Temperature must be positive");

    if queries.is_empty() {
        return 0.0;
    }

    let batch_size = queries.len();
    let mut total_loss = 0.0f32;

    for i in 0..batch_size {
        // Compute similarity between query_i and all positives
        let mut logits = Vec::with_capacity(batch_size);
        for j in 0..batch_size {
            let sim = cosine_similarity(&queries[i], &positives[j]);
            logits.push(sim / temperature);
        }

        // Softmax + cross-entropy for position i (the positive)
        // This is numerically stable log-softmax
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let log_sum_exp: f32 = logits.iter().map(|&x| (x - max_logit).exp()).sum::<f32>().ln() + max_logit;
        let log_prob = logits[i] - log_sum_exp;

        total_loss -= log_prob;
    }

    total_loss / batch_size as f32
}

/// Symmetric InfoNCE loss.
///
/// Computes InfoNCE in both directions (query→positive and positive→query)
/// and averages. This provides more training signal per batch.
pub fn symmetric_info_nce_loss(
    embeddings_a: &[Vec<f32>],
    embeddings_b: &[Vec<f32>],
    temperature: f32,
) -> f32 {
    let loss_ab = info_nce_loss(embeddings_a, embeddings_b, temperature);
    let loss_ba = info_nce_loss(embeddings_b, embeddings_a, temperature);

    (loss_ab + loss_ba) / 2.0
}

/// L2 normalize a vector in-place.
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// L2 normalize a vector, returning a new vector.
pub fn l2_normalized(v: &[f32]) -> Vec<f32> {
    let mut result = v.to_vec();
    l2_normalize(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_triplet_loss_satisfied() {
        // Anchor closer to positive than negative
        let anchor = vec![1.0, 0.0];
        let positive = vec![0.9, 0.1];
        let negative = vec![-1.0, 0.0];
        let loss = triplet_loss(&anchor, &positive, &negative, 0.5);
        assert_eq!(loss, 0.0); // Constraint satisfied with margin
    }

    #[test]
    fn test_triplet_loss_violated() {
        // Anchor closer to negative than positive
        let anchor = vec![1.0, 0.0];
        let positive = vec![-0.5, 0.5];
        let negative = vec![0.9, 0.1];
        let loss = triplet_loss(&anchor, &positive, &negative, 0.5);
        assert!(loss > 0.0); // Constraint violated
    }

    #[test]
    fn test_info_nce_loss_perfect() {
        // When each query is identical to its positive and different from others
        let queries = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        let positives = queries.clone();
        let loss = info_nce_loss(&queries, &positives, 0.07);
        // Loss should be low (softmax will give high prob to correct index)
        assert!(loss < 1.0);
    }

    #[test]
    fn test_l2_normalize() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }
}
