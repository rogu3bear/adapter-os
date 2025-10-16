use std::collections::BTreeMap;
use std::time::SystemTime;

use adapteros_core::{AosError, B3Hash, Result};

/// Descriptor metadata for a registered compute shader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeShaderDescriptor {
    /// Unique shader name used for registration and lookup.
    pub name: String,
    /// BLAKE3 hash of the shader source for change detection.
    pub source_hash: B3Hash,
    /// Threadgroup size used when dispatching this shader.
    pub threadgroup_size: (u64, u64, u64),
    /// Logical buffer or texture bindings referenced by the shader.
    pub bindings: Vec<String>,
}

impl ComputeShaderDescriptor {
    fn new(
        name: String,
        source: &str,
        threadgroup_size: (u64, u64, u64),
        bindings: Vec<String>,
    ) -> Self {
        Self {
            source_hash: B3Hash::hash(source.as_bytes()),
            name,
            threadgroup_size,
            bindings,
        }
    }
}

/// Execution statistics collected for each compute shader dispatch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShaderExecutionStats {
    /// Number of dispatches recorded for this shader.
    pub dispatches: u64,
    /// Timestamp of the most recent dispatch.
    pub last_used: Option<SystemTime>,
    /// Total number of work items executed across all dispatches.
    pub total_work_items: u128,
}

#[derive(Debug, Clone)]
struct RegistryEntry {
    descriptor: ComputeShaderDescriptor,
    stats: ShaderExecutionStats,
}

/// Registry for Metal compute shaders and their execution metadata.
#[derive(Debug, Default)]
pub struct ComputeShaderRegistry {
    entries: BTreeMap<String, RegistryEntry>,
}

impl ComputeShaderRegistry {
    /// Create an empty compute shader registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new compute shader descriptor.
    ///
    /// Returns an error when attempting to register the same shader name twice.
    pub fn register<S, B>(
        &mut self,
        name: S,
        source: &str,
        threadgroup_size: (u64, u64, u64),
        bindings: B,
    ) -> Result<&ComputeShaderDescriptor>
    where
        S: Into<String>,
        B: Into<Vec<String>>,
    {
        let name = name.into();
        if self.entries.contains_key(&name) {
            return Err(AosError::Kernel(format!(
                "Compute shader '{}' already registered",
                name
            )));
        }

        let descriptor =
            ComputeShaderDescriptor::new(name.clone(), source, threadgroup_size, bindings.into());
        self.entries.insert(
            name.clone(),
            RegistryEntry {
                descriptor,
                stats: ShaderExecutionStats::default(),
            },
        );

        Ok(&self
            .entries
            .get(&name)
            .expect("entry just inserted")
            .descriptor)
    }

    /// Retrieve a registered compute shader descriptor by name.
    pub fn descriptor(&self, name: &str) -> Option<&ComputeShaderDescriptor> {
        self.entries.get(name).map(|entry| &entry.descriptor)
    }

    /// Record a dispatch event for the given shader.
    ///
    /// `threadgroups` represents the grid dispatched for the compute kernel.
    pub fn record_dispatch(&mut self, name: &str, threadgroups: (u64, u64, u64)) -> Result<()> {
        let entry = self
            .entries
            .get_mut(name)
            .ok_or_else(|| AosError::Kernel(format!("Unknown compute shader '{}'", name)))?;

        entry.stats.dispatches = entry.stats.dispatches.saturating_add(1);
        entry.stats.last_used = Some(SystemTime::now());
        let tg = entry.descriptor.threadgroup_size;
        let work_items = (threadgroups.0 as u128)
            .saturating_mul(tg.0 as u128)
            .saturating_mul(threadgroups.1 as u128)
            .saturating_mul(tg.1 as u128)
            .saturating_mul(threadgroups.2 as u128)
            .saturating_mul(tg.2 as u128);
        entry.stats.total_work_items = entry.stats.total_work_items.saturating_add(work_items);
        Ok(())
    }

    /// Fetch execution statistics for a registered shader.
    pub fn stats(&self, name: &str) -> Option<&ShaderExecutionStats> {
        self.entries.get(name).map(|entry| &entry.stats)
    }

    /// Iterate over registered shaders with their descriptors and statistics.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&str, &ComputeShaderDescriptor, &ShaderExecutionStats)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), &entry.descriptor, &entry.stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_shader() -> ComputeShaderRegistry {
        let mut registry = ComputeShaderRegistry::new();
        registry
            .register(
                "test_shader",
                "kernel void test() {}",
                (8, 8, 1),
                vec!["input".to_string(), "output".to_string()],
            )
            .expect("failed to register shader");
        registry
    }

    #[test]
    fn register_returns_descriptor() {
        let registry = registry_with_shader();
        let descriptor = registry
            .descriptor("test_shader")
            .expect("descriptor not found");
        assert_eq!(descriptor.name, "test_shader");
        assert_eq!(descriptor.threadgroup_size, (8, 8, 1));
        assert_eq!(descriptor.bindings, vec!["input", "output"]);
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = ComputeShaderRegistry::new();
        registry
            .register(
                "dup_shader",
                "kernel void test() {}",
                (1, 1, 1),
                Vec::<String>::new(),
            )
            .expect("initial registration should succeed");

        let err = registry
            .register(
                "dup_shader",
                "kernel void test() {}",
                (1, 1, 1),
                Vec::<String>::new(),
            )
            .expect_err("duplicate registration should fail");

        assert!(matches!(err, AosError::Kernel(_)));
    }

    #[test]
    fn record_dispatch_updates_stats() {
        let mut registry = registry_with_shader();
        registry
            .record_dispatch("test_shader", (2, 3, 1))
            .expect("record dispatch");

        let stats = registry.stats("test_shader").expect("stats not found");
        assert_eq!(stats.dispatches, 1);
        assert!(stats.last_used.is_some());
        assert_eq!(stats.total_work_items, 2 * 3 * 1 * 8 * 8 * 1);
    }

    #[test]
    fn iterates_over_registered_shaders() {
        let registry = registry_with_shader();
        let collected: Vec<_> = registry
            .iter()
            .map(|(name, descriptor, stats)| {
                (name.to_string(), descriptor.name.clone(), stats.dispatches)
            })
            .collect();

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, "test_shader");
        assert_eq!(collected[0].1, "test_shader");
        assert_eq!(collected[0].2, 0);
    }
}
