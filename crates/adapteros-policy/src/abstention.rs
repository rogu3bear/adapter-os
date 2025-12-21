//! Abstention logic

/// Determine if the system should abstain from answering
pub fn should_abstain(
    confidence: f32,
    threshold: f32,
    evidence_count: usize,
    min_evidence: usize,
) -> bool {
    confidence < threshold || evidence_count < min_evidence
}
