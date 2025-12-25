//! MoE (Mixture of Experts) type definitions
//!
//! This module provides semantic type aliases for expert routing data
//! to improve code clarity and distinguish from adapter routing concepts.

/// Index of a transformer layer (0-based)
pub type LayerIdx = usize;

/// Expert ID within a layer (typically 0-255 for MoE models)
pub type ExpertId = u8;

/// Per-token expert routing: which expert was selected at each layer
///
/// Each tuple represents (layer_index, expert_id) indicating that
/// `expert_id` was activated at `layer_index` for this token.
///
/// ## Example
/// ```ignore
/// // Token activated expert 5 at layer 0, expert 10 at layer 1
/// let routing: ExpertRouting = vec![(0, 5), (1, 10)];
/// ```
pub type ExpertRouting = Vec<(LayerIdx, ExpertId)>;

/// Expert routing for an entire sequence of tokens
///
/// Outer vec is indexed by token position, inner vec contains
/// the expert selections for that token.
pub type SequenceExpertRouting = Vec<ExpertRouting>;
