//! Adapter operations module
//!
//! Contains Worker methods for adapter operations including:
//! - execute_adapter_command
//! - verify_gpu_integrity
//! - validate_effective_adapter_gate

use crate::{
    ensure_preload_allowed, memory::MemoryPressureLevel, AdapterCommand, AdapterCommandResult,
    InferenceRequest, Worker,
};
use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::FusedKernels;
use std::collections::HashSet;

/// Worker methods for adapter operations
impl<K: FusedKernels + crate::StrictnessControl + Send + Sync + 'static> Worker<K> {
    /// Validate effective adapter gate
    ///
    /// Ensures that requested adapter IDs exist in the manifest and that
    /// pinned adapters are within the effective set.
    pub(crate) fn validate_effective_adapter_gate(
        &self,
        request: &InferenceRequest,
    ) -> Result<Option<HashSet<usize>>> {
        let manifest_ids: Vec<&str> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.as_str())
            .collect();

        let Some(effective_ids) = request.effective_adapter_ids.as_ref() else {
            return Ok(None);
        };

        if effective_ids.is_empty() {
            return Ok(Some(HashSet::new()));
        }

        let mut allowed_indices = HashSet::new();

        for effective_id in effective_ids {
            let Some(idx) = manifest_ids
                .iter()
                .position(|id| id == &effective_id.as_str())
            else {
                return Err(AosError::AdapterNotInManifest {
                    adapter_id: effective_id.clone(),
                    available: manifest_ids.iter().map(|s| s.to_string()).collect(),
                });
            };
            allowed_indices.insert(idx);
        }

        if let Some(pinned_ids) = request.pinned_adapter_ids.as_ref() {
            for pinned in pinned_ids {
                let Some(idx) = manifest_ids.iter().position(|id| id == &pinned.as_str()) else {
                    return Err(AosError::AdapterNotInManifest {
                        adapter_id: pinned.clone(),
                        available: manifest_ids.iter().map(|s| s.to_string()).collect(),
                    });
                };
                if !allowed_indices.contains(&idx) {
                    return Err(AosError::AdapterNotInEffectiveSet {
                        adapter_id: pinned.clone(),
                        effective_set: effective_ids.clone(),
                    });
                }
            }
        }

        Ok(Some(allowed_indices))
    }

    /// Execute adapter hot-swap command
    pub async fn execute_adapter_command(
        &mut self,
        command: AdapterCommand,
    ) -> Result<AdapterCommandResult> {
        if let AdapterCommand::Preload { ref adapter_id, .. } = &command {
            // Check live pressure before attempting to load another adapter.
            let pressure_before: MemoryPressureLevel =
                self.memory_monitor.current_pressure_level().await;

            if pressure_before == MemoryPressureLevel::Critical {
                tracing::warn!(
                    adapter_id = %adapter_id,
                    "Critical memory pressure before adapter preload; attempting eviction"
                );

                // Attempt to free memory through lifecycle eviction logic.
                let lifecycle = self.lifecycle.lock().await;
                if let Err(evict_err) = lifecycle.handle_memory_pressure(&self.profiler) {
                    tracing::warn!(
                        adapter_id = %adapter_id,
                        error = %evict_err,
                        "Eviction attempt during preload guard failed"
                    );
                }
            }

            let pressure_after: MemoryPressureLevel =
                self.memory_monitor.current_pressure_level().await;
            ensure_preload_allowed(pressure_before, pressure_after)?;
        }

        self.hotswap.execute(command).await
    }

    /// Verify GPU buffers for all loaded adapters
    ///
    /// Reads GPU buffer checkpoints and validates against stored fingerprints.
    /// Also checks memory footprint against adaptive baseline with 2 sigma tolerance.
    ///
    /// Returns a report with verified/failed/skipped adapters.
    ///
    /// # Usage
    ///
    /// This method can be called on-demand to verify GPU integrity after adapter
    /// operations (load, swap, rollback) or as part of periodic health checks.
    ///
    /// ```rust
    /// use adapteros_lora_lifecycle::GpuIntegrityReport;
    ///
    /// // Example of how to check a GPU integrity report
    /// let report = GpuIntegrityReport {
    ///     verified: vec![(0, "adapter-1".to_string())],
    ///     failed: vec![],
    ///     skipped: vec![],
    ///     total_checked: 1,
    ///     timestamp: 0,
    /// };
    ///
    /// // Check if any adapters failed verification
    /// if !report.failed.is_empty() {
    ///     // Handle integrity failures
    ///     for (idx, id, reason) in &report.failed {
    ///         eprintln!("Adapter {} (idx {}) failed: {}", id, idx, reason);
    ///     }
    /// }
    /// ```
    ///
    /// In async context with a Worker instance:
    /// ```ignore
    /// let report = worker.verify_gpu_integrity().await?;
    /// ```
    pub async fn verify_gpu_integrity(
        &self,
    ) -> Result<adapteros_lora_lifecycle::GpuIntegrityReport> {
        use adapteros_lora_lifecycle::GpuIntegrityReport;

        let mut verified = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();

        // Get adapters that should have GPU buffers loaded
        let loaded_adapters = {
            let lifecycle = self.lifecycle.lock().await;
            lifecycle.get_loaded_adapters()
        };

        let mut kernels_lock = self.kernels.lock().await;

        // Proceed with verification - backends without GPU tracking will skip via default trait impls
        for (adapter_id_u16, adapter_id, _state) in &loaded_adapters {
            // Try to verify GPU buffers
            #[cfg(target_os = "macos")]
            match kernels_lock.verify_adapter_buffers(*adapter_id_u16) {
                Ok((buffer_size, first, last, mid)) => {
                    // Create fingerprint from current GPU state
                    use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                    let current_fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);
                    let checkpoint_hash_hex = current_fp.checkpoint_hash.to_hex();

                    // Verify against stored baseline
                    match kernels_lock.verify_gpu_fingerprint(
                        *adapter_id_u16,
                        buffer_size,
                        &checkpoint_hash_hex,
                    ) {
                        Ok(true) => {
                            // Check memory footprint against baseline
                            let (within_tolerance, z_score, baseline_stats) =
                                kernels_lock.check_memory_footprint(*adapter_id_u16, buffer_size);

                            let (baseline_mean, baseline_stddev, _sample_count) =
                                baseline_stats.unwrap_or((buffer_size as f64, 0.0, 0));

                            if within_tolerance {
                                verified.push((*adapter_id_u16, adapter_id.clone()));

                                // Emit telemetry for successful verification
                                use adapteros_lora_lifecycle::GpuIntegrityVerificationEvent;
                                if let Some(t) = &self.telemetry {
                                    let _ = t.log(
                                        "gpu_integrity_verification",
                                        GpuIntegrityVerificationEvent {
                                            adapter_id: adapter_id.clone(),
                                            adapter_idx: *adapter_id_u16,
                                            verified: true,
                                            buffer_bytes: buffer_size,
                                            checkpoint_hash: current_fp.checkpoint_hash.to_hex(),
                                            memory_footprint_within_tolerance: true,
                                            z_score: Some(z_score),
                                            baseline_mean: Some(baseline_mean),
                                            timestamp: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs(),
                                        },
                                    );
                                }
                            } else {
                                failed.push((
                                    *adapter_id_u16,
                                    adapter_id.clone(),
                                    format!(
                                        "Memory footprint anomaly: {} bytes (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                        buffer_size, baseline_mean, baseline_stddev, z_score
                                    ),
                                ));

                                // Emit telemetry for memory footprint anomaly
                                use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                                if let Some(t) = &self.telemetry {
                                    let _ = t.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                        adapter_id: adapter_id.clone(),
                                        adapter_idx: *adapter_id_u16,
                                        violation_type: "memory_anomaly".to_string(),
                                        details: format!(
                                            "Memory footprint {} bytes exceeds 2σ tolerance (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                            buffer_size, baseline_mean, baseline_stddev, z_score
                                        ),
                                        buffer_bytes: Some(buffer_size),
                                        z_score: Some(z_score),
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),
                                    });
                                }
                            }
                        }
                        Ok(false) => {
                            // No baseline exists yet - store this as the baseline
                            if let Err(e) = kernels_lock.store_gpu_fingerprint(
                                *adapter_id_u16,
                                buffer_size,
                                &checkpoint_hash_hex,
                            ) {
                                tracing::warn!(
                                    adapter_id = %adapter_id,
                                    error = %e,
                                    "Failed to store GPU fingerprint baseline (non-fatal)"
                                );
                            } else {
                                tracing::info!(
                                    adapter_id = %adapter_id,
                                    adapter_idx = adapter_id_u16,
                                    "Stored initial GPU fingerprint baseline"
                                );
                            }
                            verified.push((*adapter_id_u16, adapter_id.clone()));
                        }
                        Err(msg) => {
                            failed.push((
                                *adapter_id_u16,
                                adapter_id.clone(),
                                format!("GPU buffer fingerprint mismatch: {}", msg),
                            ));

                            // Emit telemetry for fingerprint mismatch
                            use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                            if let Some(t) = &self.telemetry {
                                let _ = t.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                    adapter_id: adapter_id.clone(),
                                    adapter_idx: *adapter_id_u16,
                                    violation_type: "fingerprint_mismatch".to_string(),
                                    details: format!("GPU buffer checkpoint hash does not match stored fingerprint: {}", msg),
                                    buffer_bytes: Some(buffer_size),
                                    z_score: None,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    // Adapter not loaded or verification not supported
                    skipped.push((*adapter_id_u16, adapter_id.clone()));
                    tracing::debug!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "GPU verification skipped"
                    );
                }
            }

            // Non-macOS platforms don't have Metal GPU verification
            #[cfg(not(target_os = "macos"))]
            {
                skipped.push((*adapter_id_u16, adapter_id.clone()));
                tracing::debug!(
                    adapter_id = %adapter_id,
                    "GPU verification not available on this platform"
                );
            }
        }

        drop(kernels_lock);

        Ok(GpuIntegrityReport {
            verified,
            failed,
            skipped,
            total_checked: loaded_adapters.len(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }
}
