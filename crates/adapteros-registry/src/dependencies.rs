use std::collections::{BTreeMap, BTreeSet};

use adapteros_core::{AosError, Result};
use adapteros_manifest::AdapterDependencies;

use crate::Registry;

/// Graph describing adapter dependency relationships. Each entry maps an adapter ID to the
/// adapters it directly depends on.
pub type DependencyGraph = BTreeMap<String, Vec<String>>;

/// Resolves and validates adapter dependencies prior to loading.
pub struct DependencyResolver<'a> {
    registry: &'a Registry,
}

impl<'a> DependencyResolver<'a> {
    /// Create a new dependency resolver backed by the registry database.
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Validate dependencies for an adapter and return the resolved dependency graph.
    ///
    /// The `dependency_provider` callback must return dependency metadata for any adapter that
    /// appears in the dependency tree. This allows the resolver to recursively validate
    /// relationships and detect circular dependencies.
    pub fn resolve<F>(
        &self,
        adapter_id: &str,
        dependencies: &AdapterDependencies,
        base_model: &str,
        mut dependency_provider: F,
    ) -> Result<DependencyGraph>
    where
        F: FnMut(&str) -> Result<Option<AdapterDependencies>>,
    {
        let mut graph = DependencyGraph::new();
        let mut conflicts = BTreeMap::new();
        let mut visiting = Vec::new();
        let mut visited = BTreeSet::new();

        self.visit_node(
            adapter_id,
            dependencies,
            base_model,
            &mut dependency_provider,
            &mut graph,
            &mut conflicts,
            &mut visiting,
            &mut visited,
        )?;

        self.validate_conflict_graph(adapter_id, &graph, &conflicts)?;

        Ok(graph)
    }

    fn visit_node<F>(
        &self,
        adapter_id: &str,
        dependencies: &AdapterDependencies,
        base_model: &str,
        dependency_provider: &mut F,
        graph: &mut DependencyGraph,
        conflicts: &mut BTreeMap<String, Vec<String>>,
        visiting: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<()>
    where
        F: FnMut(&str) -> Result<Option<AdapterDependencies>>,
    {
        if visited.contains(adapter_id) {
            return Ok(());
        }

        if visiting.iter().any(|v| v == adapter_id) {
            let mut cycle = visiting.clone();
            cycle.push(adapter_id.to_string());
            return Err(AosError::Registry(format!(
                "Circular dependency detected: {}",
                cycle.join(" -> ")
            )));
        }

        visiting.push(adapter_id.to_string());

        if let Some(required_base) = dependencies.base_model.as_deref() {
            if required_base != base_model {
                return Err(AosError::Registry(format!(
                    "Adapter {} requires base model {} but {} is active",
                    adapter_id, required_base, base_model
                )));
            }
        }

        let mut seen_requires = BTreeSet::new();
        for required in &dependencies.requires_adapters {
            if !seen_requires.insert(required) {
                return Err(AosError::Registry(format!(
                    "Adapter {} lists duplicate dependency {}",
                    adapter_id, required
                )));
            }

            if required == adapter_id {
                return Err(AosError::Registry(format!(
                    "Adapter {} cannot depend on itself",
                    adapter_id
                )));
            }
        }

        let mut seen_conflicts = BTreeSet::new();
        for conflict in &dependencies.conflicts_with {
            if !seen_conflicts.insert(conflict) {
                return Err(AosError::Registry(format!(
                    "Adapter {} lists duplicate conflict {}",
                    adapter_id, conflict
                )));
            }

            if conflict == adapter_id {
                return Err(AosError::Registry(format!(
                    "Adapter {} cannot conflict with itself",
                    adapter_id
                )));
            }

            if dependencies
                .requires_adapters
                .iter()
                .any(|required| required == conflict)
            {
                return Err(AosError::Registry(format!(
                    "Adapter {} both requires and conflicts with {}",
                    adapter_id, conflict
                )));
            }

            if self.registry.get_adapter(conflict)?.is_some() {
                return Err(AosError::Registry(format!(
                    "Adapter {} conflicts with registered adapter {}",
                    adapter_id, conflict
                )));
            }
        }

        graph.insert(
            adapter_id.to_string(),
            dependencies.requires_adapters.clone(),
        );
        conflicts.insert(adapter_id.to_string(), dependencies.conflicts_with.clone());

        for required in &dependencies.requires_adapters {
            if visiting.iter().any(|v| v == required) {
                let mut cycle = visiting.clone();
                cycle.push(required.to_string());
                return Err(AosError::Registry(format!(
                    "Circular dependency detected: {}",
                    cycle.join(" -> ")
                )));
            }

            if self.registry.get_adapter(required)?.is_none() {
                return Err(AosError::Registry(format!(
                    "Adapter {} requires missing adapter {}",
                    adapter_id, required
                )));
            }

            if !visited.contains(required.as_str()) {
                let child_dependencies = dependency_provider(required)?.ok_or_else(|| {
                    AosError::Registry(format!(
                        "Missing dependency manifest for adapter {}",
                        required
                    ))
                })?;

                self.visit_node(
                    required,
                    &child_dependencies,
                    base_model,
                    dependency_provider,
                    graph,
                    conflicts,
                    visiting,
                    visited,
                )?;
            }
        }

        visiting.pop();
        visited.insert(adapter_id.to_string());

        Ok(())
    }

    fn validate_conflict_graph(
        &self,
        root_id: &str,
        graph: &DependencyGraph,
        conflicts: &BTreeMap<String, Vec<String>>,
    ) -> Result<()> {
        let mut all_nodes: BTreeSet<&str> = graph.keys().map(|k| k.as_str()).collect();
        for deps in graph.values() {
            for dep in deps {
                all_nodes.insert(dep);
            }
        }

        for (adapter, conflict_list) in conflicts {
            for conflict in conflict_list {
                if all_nodes.contains(conflict.as_str()) {
                    return Err(AosError::Registry(format!(
                        "Adapter {} conflicts with dependency {} while resolving {}",
                        adapter, conflict, root_id
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
