//! Discovery Signal Bridge
//!
//! Connects CodeGraph scanner to the signal protocol, emitting discovery
//! signals for repository scanning, symbol indexing, and framework detection.
//!
//! Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.2

use crate::signal::{Signal, SignalBuilder, SignalPriority, SignalType};
use serde_json::json;
use adapteros_deterministic_exec::channel::{DeterministicChannel, Sender};

/// Bridge for converting CodeGraph events to signals
///
/// This component wraps CodeGraph scanning operations and emits discovery
/// signals at key milestones (scan started, progress, symbol indexed, etc.)
///
/// # Example
/// ```no_run
/// use adapteros_lora_worker::discovery_signal_bridge::DiscoverySignalBridge;
/// use adapteros_deterministic_exec::channel::DeterministicChannel;
///
/// let (signal_tx, signal_rx) = DeterministicChannel::new(100);
/// let bridge = DiscoverySignalBridge::new(signal_tx);
///
/// // Emit scan started signal
/// bridge.on_scan_started("acme/payments", 0).await;
/// ```
pub struct DiscoverySignalBridge {
    signal_tx: Sender<Signal>,
}

impl DiscoverySignalBridge {
    /// Create a new discovery signal bridge
    pub fn new(signal_tx: Sender<Signal>) -> Self {
        Self { signal_tx }
    }

    /// Emit signal when repository scan starts
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.2
    pub async fn on_scan_started(&self, repo_id: &str, total_files_estimated: usize) {
        let signal = SignalBuilder::new(SignalType::RepoScanStarted)
            .priority(SignalPriority::Normal)
            .with_field("repo_id", json!(repo_id))
            .with_field("stage", json!("parsing"))
            .with_field("total_files_estimated", json!(total_files_estimated))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Emit progress update during scan
    ///
    /// Should be called periodically (e.g., every 100 files parsed).
    pub async fn on_scan_progress(
        &self,
        repo_id: &str,
        stage: &str,
        files_parsed: usize,
        symbol_count: usize,
    ) {
        let signal = SignalBuilder::new(SignalType::RepoScanProgress)
            .priority(SignalPriority::Low) // High frequency
            .with_field("repo_id", json!(repo_id))
            .with_field("stage", json!(stage))
            .with_field("files_parsed", json!(files_parsed))
            .with_field("symbol_count", json!(symbol_count))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Emit signal when a symbol is indexed
    ///
    /// Can be sampled (e.g., emit every Nth symbol) to reduce traffic.
    pub async fn on_symbol_indexed(
        &self,
        repo_id: &str,
        symbol_name: &str,
        symbol_type: &str,
        file_path: &str,
    ) {
        let signal = SignalBuilder::new(SignalType::SymbolIndexed)
            .priority(SignalPriority::Low)
            .with_field("repo_id", json!(repo_id))
            .with_field("symbol_name", json!(symbol_name))
            .with_field("symbol_type", json!(symbol_type))
            .with_field("file_path", json!(file_path))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Emit signal when a framework is detected
    ///
    /// Examples: "django 4.2", "fastapi 0.109", "actix-web 4.0"
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.2
    pub async fn on_framework_detected(&self, repo_id: &str, framework: &str) {
        let signal = SignalBuilder::new(SignalType::FrameworkDetected)
            .priority(SignalPriority::Normal)
            .with_field("repo_id", json!(repo_id))
            .with_field("framework", json!(framework))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Emit signal when test map is updated
    ///
    /// Test map identifies test files and their coverage mappings.
    pub async fn on_test_map_updated(&self, repo_id: &str, test_count: usize) {
        let signal = SignalBuilder::new(SignalType::TestMapUpdated)
            .priority(SignalPriority::Low)
            .with_field("repo_id", json!(repo_id))
            .with_field("test_count", json!(test_count))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Emit signal when repository scan completes
    ///
    /// Final signal marking completion, includes content hash for determinism.
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.2
    pub async fn on_scan_completed(
        &self,
        repo_id: &str,
        files_parsed: usize,
        symbol_count: usize,
        content_hash: &str,
    ) {
        let signal = SignalBuilder::new(SignalType::RepoScanCompleted)
            .priority(SignalPriority::High)
            .with_field("repo_id", json!(repo_id))
            .with_field("stage", json!("completed"))
            .with_field("files_parsed", json!(files_parsed))
            .with_field("symbol_count", json!(symbol_count))
            .with_field("content_hash", json!(content_hash))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }
}

/// Helper for scanning a repository with signal emissions
///
/// This is a wrapper that demonstrates how to integrate CodeGraph
/// with the discovery signal bridge.
///
/// # Example Usage
/// ```no_run
/// use adapteros_lora_worker::discovery_signal_bridge::scan_repository_with_signals;
/// use adapteros_deterministic_exec::channel::DeterministicChannel;
/// use std::path::Path;
///
/// # async fn example() {
/// let (signal_tx, signal_rx) = DeterministicChannel::new(100);
/// let result = scan_repository_with_signals(
///     "acme/payments",
///     Path::new("/repos/acme/payments"),
///     signal_tx
/// ).await;
/// # }
/// ```
pub async fn scan_repository_with_signals(
    repo_id: &str,
    repo_path: &std::path::Path,
    signal_tx: Sender<Signal>,
) -> adapteros_core::Result<()> {
    let bridge = DiscoverySignalBridge::new(signal_tx);

    // Emit scan started
    bridge.on_scan_started(repo_id, 0).await;

    // In a real implementation, this would call adapteros_codegraph::CodeGraph::from_directory
    // and emit signals at each stage. For now, this is a demonstration skeleton.

    // Stage 1: Parse files
    bridge.on_scan_progress(repo_id, "parsing", 0, 0).await;

    // Stage 2: Index symbols (would iterate through parsed symbols)
    // bridge.on_symbol_indexed(repo_id, "MyStruct", "struct", "src/lib.rs").await;

    // Stage 3: Detect frameworks (would analyze dependencies)
    // bridge.on_framework_detected(repo_id, "django 4.2").await;

    // Stage 4: Complete
    bridge
        .on_scan_completed(repo_id, 100, 500, "b3:abc123...")
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_started_signal() {
        let (tx, rx) = DeterministicChannel::new(10);
        let bridge = DiscoverySignalBridge::new(tx);

        bridge.on_scan_started("acme/payments", 150).await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::RepoScanStarted);
        assert_eq!(signal.priority, SignalPriority::Normal);
    }

    #[tokio::test]
    async fn test_framework_detected_signal() {
        let (tx, rx) = DeterministicChannel::new(10);
        let bridge = DiscoverySignalBridge::new(tx);

        bridge.on_framework_detected("acme/api", "fastapi 0.109").await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::FrameworkDetected);
    }

    #[tokio::test]
    async fn test_scan_completed_signal() {
        let (tx, rx) = DeterministicChannel::new(10);
        let bridge = DiscoverySignalBridge::new(tx);

        bridge
            .on_scan_completed("test/repo", 200, 1500, "b3:test_hash")
            .await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::RepoScanCompleted);
        assert_eq!(signal.priority, SignalPriority::High);
    }
}

