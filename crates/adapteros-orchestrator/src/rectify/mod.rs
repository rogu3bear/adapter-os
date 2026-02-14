//! Source Change Detection and Rectification (AARA Lifecycle)
//!
//! The rectify module handles detecting when source documents change and
//! triggers re-synthesis to keep adapters up-to-date with their source material.
//!
//! # AARA Lifecycle - RECTIFY Phase
//!
//! When source documents change:
//! 1. **Detect** - Compare current document hashes with stored hashes
//! 2. **Identify** - Find all adapters trained on changed documents
//! 3. **Re-synthesize** - Generate new training examples from updated documents
//! 4. **Version** - Create new adapter versions (draft state)
//! 5. **Validate** - Run validation tests before promotion
//!
//! # Example
//!
//! ```ignore
//! use adapteros_orchestrator::rectify::{ChangeDetector, RectifyWorkflow};
//!
//! let detector = ChangeDetector::new(db_pool);
//!
//! // Check for changes in a document repository
//! let changes = detector.detect_changes("repo-123").await?;
//!
//! // Rectify affected adapters
//! for change in changes {
//!     let workflow = RectifyWorkflow::new(&change);
//!     let new_version = workflow.execute().await?;
//! }
//! ```

mod detector;
mod types;
mod workflow;

pub use detector::ChangeDetector;
pub use types::{
    AffectedAdapter, ChangeAction, ChangeType, NewAdapterVersion, RectifyResult, RectifyStatus,
    SourceChangeEvent, VersionState,
};
pub use workflow::{BatchRectifyWorkflow, RectifyBatchSummary, RectifyWorkflow};
