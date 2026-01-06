//! Result merger for consolidating agent proposals
//!
//! Merges proposals from multiple agents into a unified plan,
//! detecting and resolving conflicts.

use crate::error::{AgentSpawnError, Result};
use crate::protocol::{FileModification, ModificationType, TaskProposal};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::info;

/// Strategy for resolving conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictResolution {
    /// Higher confidence wins
    #[default]
    HighestConfidence,
    /// Earliest sequence number wins
    FirstWins,
    /// Latest sequence number wins
    LastWins,
    /// Leave conflicts unresolved for manual review
    RequireReview,
}

/// A detected conflict between proposals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    /// Path of the conflicting file
    pub file_path: PathBuf,

    /// Proposals that conflict
    pub proposals: Vec<ProposalRef>,

    /// Type of conflict
    pub conflict_type: ConflictType,

    /// Resolution (if resolved)
    pub resolution: Option<Resolution>,
}

/// Reference to a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRef {
    /// Agent that made the proposal
    pub agent_id: String,

    /// Task ID
    #[serde(with = "hex_bytes")]
    pub task_id: [u8; 32],

    /// Confidence score
    pub confidence: f32,

    /// Sequence number
    pub sequence: u64,
}

/// Type of conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    /// Multiple agents modifying the same file
    SameFileModification,

    /// One agent creates, another modifies
    CreateModifyConflict,

    /// One agent deletes, another modifies
    DeleteModifyConflict,

    /// Overlapping line ranges
    OverlappingRanges,
}

/// Resolution decision for a conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// The winning proposal
    pub winner: ProposalRef,

    /// Reason for the decision
    pub reason: String,

    /// Modifications to apply
    pub modifications: Vec<FileModification>,
}

/// The final unified plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPlan {
    /// All accepted modifications
    pub modifications: Vec<FileModification>,

    /// Unresolved conflicts requiring review
    pub unresolved_conflicts: Vec<ConflictReport>,

    /// Summary of the plan
    pub summary: String,

    /// Agents that contributed
    pub contributors: Vec<String>,

    /// Overall confidence score (weighted average)
    pub confidence: f32,

    /// BLAKE3 hash of the entire plan
    #[serde(with = "hex_bytes")]
    pub plan_hash: [u8; 32],

    /// Timestamp when plan was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Number of proposals merged
    pub proposals_merged: usize,

    /// Number of conflicts resolved
    pub conflicts_resolved: usize,
}

impl UnifiedPlan {
    /// Compute the hash of the plan
    pub fn compute_hash(modifications: &[FileModification], contributors: &[String]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();

        for m in modifications {
            hasher.update(m.file_path.to_string_lossy().as_bytes());
            if let Some(ref content) = m.new_content {
                hasher.update(content.as_bytes());
            }
        }

        for c in contributors {
            hasher.update(c.as_bytes());
        }

        *hasher.finalize().as_bytes()
    }

    /// Check if the plan has unresolved conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.unresolved_conflicts.is_empty()
    }

    /// Get files that will be modified
    pub fn affected_files(&self) -> Vec<&PathBuf> {
        self.modifications.iter().map(|m| &m.file_path).collect()
    }
}

/// Merges proposals from multiple agents into a unified plan
pub struct ResultMerger {
    /// Strategy for conflict resolution
    conflict_resolution: ConflictResolution,
}

impl ResultMerger {
    /// Create a new result merger
    pub fn new(conflict_resolution: ConflictResolution) -> Self {
        Self {
            conflict_resolution,
        }
    }

    /// Merge all proposals into a unified plan
    pub fn merge(&self, proposals: Vec<TaskProposal>) -> Result<UnifiedPlan> {
        info!(proposal_count = proposals.len(), "Merging proposals");

        if proposals.is_empty() {
            return Ok(UnifiedPlan {
                modifications: vec![],
                unresolved_conflicts: vec![],
                summary: "No proposals to merge".into(),
                contributors: vec![],
                confidence: 0.0,
                plan_hash: [0u8; 32],
                created_at: chrono::Utc::now(),
                proposals_merged: 0,
                conflicts_resolved: 0,
            });
        }

        // Group modifications by file path
        let mut file_proposals: HashMap<PathBuf, Vec<(usize, &FileModification, &TaskProposal)>> =
            HashMap::new();

        for (idx, proposal) in proposals.iter().enumerate() {
            for modification in &proposal.modifications {
                file_proposals
                    .entry(modification.file_path.clone())
                    .or_default()
                    .push((idx, modification, proposal));
            }
        }

        // Detect conflicts
        let mut conflicts = Vec::new();
        let mut accepted_modifications = Vec::new();
        let mut resolved_count = 0;

        for (file_path, mods) in file_proposals {
            if mods.len() == 1 {
                // No conflict - accept the modification
                accepted_modifications.push(mods[0].1.clone());
            } else {
                // Potential conflict
                let conflict = self.detect_conflict(&file_path, &mods);

                if let Some(mut conflict) = conflict {
                    // Try to resolve
                    if let Some(resolution) = self.resolve_conflict(&conflict, &mods) {
                        conflict.resolution = Some(resolution.clone());
                        accepted_modifications.extend(resolution.modifications);
                        resolved_count += 1;
                    } else if self.conflict_resolution != ConflictResolution::RequireReview {
                        // Conflict couldn't be resolved, add to unresolved
                        conflicts.push(conflict);
                    } else {
                        conflicts.push(conflict);
                    }
                } else {
                    // No actual conflict (e.g., different line ranges)
                    for (_, m, _) in &mods {
                        accepted_modifications.push((*m).clone());
                    }
                }
            }
        }

        // Collect contributors
        let contributors: Vec<String> = proposals
            .iter()
            .map(|p| p.agent_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Calculate weighted confidence
        let total_weight: f32 = proposals.iter().map(|p| p.modifications.len() as f32).sum();
        let weighted_confidence = if total_weight > 0.0 {
            proposals
                .iter()
                .map(|p| p.confidence * p.modifications.len() as f32)
                .sum::<f32>()
                / total_weight
        } else {
            0.0
        };

        // Generate summary
        let summary = self.generate_summary(&accepted_modifications, &conflicts);

        let plan_hash = UnifiedPlan::compute_hash(&accepted_modifications, &contributors);

        let plan = UnifiedPlan {
            modifications: accepted_modifications,
            unresolved_conflicts: conflicts,
            summary,
            contributors,
            confidence: weighted_confidence,
            plan_hash,
            created_at: chrono::Utc::now(),
            proposals_merged: proposals.len(),
            conflicts_resolved: resolved_count,
        };

        info!(
            modifications = plan.modifications.len(),
            conflicts = plan.unresolved_conflicts.len(),
            confidence = %plan.confidence,
            "Merge complete"
        );

        if plan.unresolved_conflicts.is_empty()
            || self.conflict_resolution == ConflictResolution::RequireReview
        {
            Ok(plan)
        } else {
            Err(AgentSpawnError::UnresolvableConflict {
                count: plan.unresolved_conflicts.len(),
            })
        }
    }

    /// Detect if there's a conflict for a file
    fn detect_conflict(
        &self,
        file_path: &Path,
        mods: &[(usize, &FileModification, &TaskProposal)],
    ) -> Option<ConflictReport> {
        // Check for conflicting modification types
        let types: Vec<_> = mods.iter().map(|(_, m, _)| m.modification_type).collect();

        let has_delete = types.contains(&ModificationType::Delete);
        let has_create = types.contains(&ModificationType::Create);
        let has_modify = types.contains(&ModificationType::Modify);

        let conflict_type = if has_delete {
            if has_modify || has_create {
                Some(ConflictType::DeleteModifyConflict)
            } else {
                None // Multiple deletes are not a conflict
            }
        } else if has_create && has_modify {
            Some(ConflictType::CreateModifyConflict)
        } else if mods.len() > 1 && types.iter().all(|t| *t == ModificationType::Modify) {
            // Check for overlapping line ranges
            if self.has_overlapping_ranges(mods) {
                Some(ConflictType::OverlappingRanges)
            } else {
                None // Non-overlapping modifications are OK
            }
        } else if mods.len() > 1 {
            Some(ConflictType::SameFileModification)
        } else {
            None
        };

        conflict_type.map(|ct| ConflictReport {
            file_path: file_path.to_path_buf(),
            proposals: mods
                .iter()
                .map(|(_, _, p)| ProposalRef {
                    agent_id: p.agent_id.clone(),
                    task_id: p.task_id,
                    confidence: p.confidence,
                    sequence: 0, // Would need to track this
                })
                .collect(),
            conflict_type: ct,
            resolution: None,
        })
    }

    /// Check if any line ranges overlap
    fn has_overlapping_ranges(&self, mods: &[(usize, &FileModification, &TaskProposal)]) -> bool {
        let ranges: Vec<_> = mods.iter().filter_map(|(_, m, _)| m.line_range).collect();

        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                let (a_start, a_end) = ranges[i];
                let (b_start, b_end) = ranges[j];

                // Check overlap
                if a_start <= b_end && b_start <= a_end {
                    return true;
                }
            }
        }

        false
    }

    /// Try to resolve a conflict
    fn resolve_conflict(
        &self,
        _conflict: &ConflictReport,
        mods: &[(usize, &FileModification, &TaskProposal)],
    ) -> Option<Resolution> {
        match self.conflict_resolution {
            ConflictResolution::RequireReview => None,

            ConflictResolution::HighestConfidence => {
                let winner = mods.iter().max_by(|a, b| {
                    a.2.confidence
                        .partial_cmp(&b.2.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })?;

                Some(Resolution {
                    winner: ProposalRef {
                        agent_id: winner.2.agent_id.clone(),
                        task_id: winner.2.task_id,
                        confidence: winner.2.confidence,
                        sequence: 0,
                    },
                    reason: format!(
                        "Highest confidence ({:.2}) from {}",
                        winner.2.confidence, winner.2.agent_id
                    ),
                    modifications: vec![winner.1.clone()],
                })
            }

            ConflictResolution::FirstWins => {
                let winner = mods.iter().min_by_key(|(idx, _, _)| idx)?;

                Some(Resolution {
                    winner: ProposalRef {
                        agent_id: winner.2.agent_id.clone(),
                        task_id: winner.2.task_id,
                        confidence: winner.2.confidence,
                        sequence: 0,
                    },
                    reason: format!("First proposal from {}", winner.2.agent_id),
                    modifications: vec![winner.1.clone()],
                })
            }

            ConflictResolution::LastWins => {
                let winner = mods.iter().max_by_key(|(idx, _, _)| idx)?;

                Some(Resolution {
                    winner: ProposalRef {
                        agent_id: winner.2.agent_id.clone(),
                        task_id: winner.2.task_id,
                        confidence: winner.2.confidence,
                        sequence: 0,
                    },
                    reason: format!("Last proposal from {}", winner.2.agent_id),
                    modifications: vec![winner.1.clone()],
                })
            }
        }
    }

    /// Generate a summary of the merge
    fn generate_summary(
        &self,
        modifications: &[FileModification],
        conflicts: &[ConflictReport],
    ) -> String {
        let creates = modifications
            .iter()
            .filter(|m| m.modification_type == ModificationType::Create)
            .count();
        let modifies = modifications
            .iter()
            .filter(|m| m.modification_type == ModificationType::Modify)
            .count();
        let deletes = modifications
            .iter()
            .filter(|m| m.modification_type == ModificationType::Delete)
            .count();

        let mut summary = format!(
            "Plan: {} file(s) to create, {} to modify, {} to delete",
            creates, modifies, deletes
        );

        if !conflicts.is_empty() {
            summary.push_str(&format!(". {} unresolved conflict(s)", conflicts.len()));
        }

        summary
    }
}

// Hex serialization helper
mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proposal(agent_id: &str, file: &str, confidence: f32) -> TaskProposal {
        TaskProposal {
            task_id: [0u8; 32],
            agent_id: agent_id.into(),
            modifications: vec![FileModification {
                file_path: PathBuf::from(file),
                modification_type: ModificationType::Modify,
                original_content_hash: None,
                new_content: Some("test content".into()),
                diff: None,
                line_range: Some((1, 10)),
                explanation: None,
            }],
            rationale: "Test".into(),
            confidence,
            depends_on: vec![],
            conflicts_with: vec![],
            content_hash: [0u8; 32],
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_merge_no_conflict() {
        let merger = ResultMerger::new(ConflictResolution::HighestConfidence);

        let proposals = vec![
            make_proposal("agent-01", "file1.rs", 0.9),
            make_proposal("agent-02", "file2.rs", 0.8),
        ];

        let plan = merger.merge(proposals).unwrap();

        assert_eq!(plan.modifications.len(), 2);
        assert!(plan.unresolved_conflicts.is_empty());
        assert_eq!(plan.contributors.len(), 2);
    }

    #[test]
    fn test_merge_with_conflict_resolution() {
        let merger = ResultMerger::new(ConflictResolution::HighestConfidence);

        let proposals = vec![
            make_proposal("agent-01", "same_file.rs", 0.7),
            make_proposal("agent-02", "same_file.rs", 0.9),
        ];

        let plan = merger.merge(proposals).unwrap();

        // Should resolve to agent-02's version (higher confidence)
        assert_eq!(plan.modifications.len(), 1);
        assert!(plan.unresolved_conflicts.is_empty());
        assert_eq!(plan.conflicts_resolved, 1);
    }

    #[test]
    fn test_merge_require_review() {
        let merger = ResultMerger::new(ConflictResolution::RequireReview);

        let proposals = vec![
            make_proposal("agent-01", "same_file.rs", 0.7),
            make_proposal("agent-02", "same_file.rs", 0.9),
        ];

        let plan = merger.merge(proposals).unwrap();

        // Conflict should be unresolved
        assert!(!plan.unresolved_conflicts.is_empty());
    }

    #[test]
    fn test_unified_plan_hash() {
        let mods = vec![FileModification {
            file_path: PathBuf::from("test.rs"),
            modification_type: ModificationType::Modify,
            original_content_hash: None,
            new_content: Some("content".into()),
            diff: None,
            line_range: None,
            explanation: None,
        }];

        let contributors = vec!["agent-01".to_string()];

        let hash1 = UnifiedPlan::compute_hash(&mods, &contributors);
        let hash2 = UnifiedPlan::compute_hash(&mods, &contributors);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, [0u8; 32]);
    }
}
