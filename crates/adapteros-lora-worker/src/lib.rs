#![allow(dead_code, unused_variables, unused_imports)]

use adapteros_core::{AosError, B3Hash, Result};

// Remove #[derive(Clone)]
pub struct Worker<K: FusedKernels> {
    manifest: ManifestV3,
    policy_engine: PolicyEngine,
    router: Router,
    rag_system: RagSystem,
    telemetry_writer: TelemetryWriter,
    // ... other fields ...
}

// Add unsafe impl Send and Sync
unsafe impl<K: FusedKernels + Send + Sync> Send for Worker<K> {}
unsafe impl<K: FusedKernels + Send + Sync> Sync for Worker<K> {}