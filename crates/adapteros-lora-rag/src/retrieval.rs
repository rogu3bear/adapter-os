//! Evidence span extraction and hashing

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};

/// Evidence span with provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSpan {
    pub doc_id: String,
    pub rev: String,
    pub text: String,
    pub score: f32,
    pub span_hash: B3Hash,
    pub superseded: Option<String>,
}

impl EvidenceSpan {
    /// Check if this span is from a superseded document
    pub fn is_superseded(&self) -> bool {
        self.superseded.is_some()
    }

    /// Generate warning if superseded
    pub fn supersession_warning(&self) -> Option<String> {
        self.superseded.as_ref().map(|new_rev| {
            format!(
                "Document {} revision {} has been superseded by {}",
                self.doc_id, self.rev, new_rev
            )
        })
    }
}
