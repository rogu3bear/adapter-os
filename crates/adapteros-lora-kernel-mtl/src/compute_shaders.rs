//! Compute shader registry for Metal 3.x
//!
//! Metal 3.x introduces new shader stages and threadgroup sizes. The
//! registry keeps metadata about precompiled pipelines and tracks
//! execution statistics for telemetry.

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Binding for a compute shader resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBinding {
    pub index: u32,
    pub name: String,
}

/// Descriptor describing a compute shader pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeShaderDescriptor {
    pub name: String,
    pub source_hash: B3Hash,
    pub threadgroup_size: (u16, u16, u16),
    pub bindings: Vec<ResourceBinding>,
}

/// Runtime statistics for shader dispatches.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderExecutionStats {
    pub dispatches: u64,
    pub last_used: Option<DateTime<Utc>>,
    pub total_work_items: u128,
}

impl Default for ShaderExecutionStats {
    fn default() -> Self {
        Self {
            dispatches: 0,
            last_used: None,
            total_work_items: 0,
        }
    }
}

/// Registry of available compute shaders.
#[derive(Debug, Default)]
pub struct ComputeShaderRegistry {
    descriptors: BTreeMap<String, ComputeShaderDescriptor>,
    stats: BTreeMap<String, ShaderExecutionStats>,
}

impl ComputeShaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ComputeShaderDescriptor) -> Result<()> {
        if self.descriptors.contains_key(&descriptor.name) {
            return Err(AosError::Kernel(format!(
                "Compute shader '{}' already registered",
                descriptor.name
            )));
        }

        self.stats
            .insert(descriptor.name.clone(), ShaderExecutionStats::default());
        self.descriptors.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }

    pub fn descriptor(&self, name: &str) -> Option<&ComputeShaderDescriptor> {
        self.descriptors.get(name)
    }

    pub fn record_dispatch(&mut self, name: &str, workgroups: (u32, u32, u32)) -> Result<()> {
        let stats = self
            .stats
            .get_mut(name)
            .ok_or_else(|| AosError::Kernel(format!("Unknown compute shader '{}'", name)))?;
        stats.dispatches += 1;
        stats.last_used = Some(Utc::now());
        let work_items = workgroups.0 as u128 * workgroups.1 as u128 * workgroups.2 as u128;
        stats.total_work_items = stats.total_work_items.saturating_add(work_items);
        Ok(())
    }

    pub fn stats(&self, name: &str) -> Option<&ShaderExecutionStats> {
        self.stats.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ComputeShaderDescriptor)> {
        self.descriptors.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_prevents_duplicate_registration() {
        let mut registry = ComputeShaderRegistry::new();
        let descriptor = ComputeShaderDescriptor {
            name: "kernel".to_string(),
            source_hash: B3Hash::hash(b"shader"),
            threadgroup_size: (32, 1, 1),
            bindings: vec![ResourceBinding {
                index: 0,
                name: "input".to_string(),
            }],
        };
        registry.register(descriptor.clone()).expect("register");
        let err = registry.register(descriptor).unwrap_err();
        assert!(matches!(err, AosError::Kernel(_)));
    }

    #[test]
    fn registry_tracks_dispatch_stats() {
        let mut registry = ComputeShaderRegistry::new();
        registry
            .register(ComputeShaderDescriptor {
                name: "kernel".to_string(),
                source_hash: B3Hash::hash(b"shader"),
                threadgroup_size: (16, 4, 1),
                bindings: vec![],
            })
            .unwrap();

        registry
            .record_dispatch("kernel", (8, 1, 1))
            .expect("dispatch");
        let stats = registry.stats("kernel").unwrap();
        assert_eq!(stats.dispatches, 1);
        assert!(stats.total_work_items >= 8);
    }
}
