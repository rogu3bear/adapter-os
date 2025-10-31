//! Git integration for AdapterOS

pub mod branch_manager;
pub mod diff_analyzer;
pub mod subsystem;

pub use branch_manager::BranchManager;
pub use diff_analyzer::{
    ChangedSymbol, DiffAnalysis, DiffAnalyzer, DiffSummary, SymbolChangeType, SymbolKind,
};
pub use subsystem::{CommitDiff, CommitInfo, GitConfig, GitSubsystem};

// NOTE: The original GitSubsystem implementation (watcher, commit daemon, branch manager)
// has been temporarily stubbed out to resolve a feature conflict. The primary
// functionality of this crate is now the DiffAnalyzer. The GitSubsystem will be
// fully implemented in a future iteration.
