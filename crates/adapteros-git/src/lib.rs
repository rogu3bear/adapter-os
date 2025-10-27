//! Git integration for AdapterOS

pub mod diff_analyzer;
pub mod subsystem;

pub use diff_analyzer::{DiffAnalyzer, DiffAnalysis, DiffSummary, ChangedSymbol, SymbolKind, SymbolChangeType};
pub use subsystem::GitSubsystem;

// NOTE: The original GitSubsystem implementation (watcher, commit daemon, branch manager)
// has been temporarily stubbed out to resolve a feature conflict. The primary
// functionality of this crate is now the DiffAnalyzer. The GitSubsystem will be
// fully implemented in a future iteration.
