//! Operation graph reconstruction from trace events
//!
//! This module provides the ability to reconstruct a complete operation graph
//! from a sequence of trace events. The graph captures dependencies between
//! operations and enables deterministic replay through topological ordering.
//!
//! # Design
//!
//! - **Operation Nodes**: Represent individual operations with inputs/outputs
//! - **Dependency Edges**: Connect operations that depend on each other
//! - **Topological Ordering**: Kahn's algorithm for deterministic execution order
//! - **Hash Verification**: BLAKE3 hashes for input/output verification
//!
//! # Citations
//!
//! - Topological sorting: Kahn, "Topological sorting of large networks", 1962
//! - Graph storage: `HashMap` for O(1) node lookup following `std::collections` patterns
//! - Hash consistency: `B3Hash` for deterministic input/output verification【crates/adapteros-trace/src/schema.rs:187-190】

use std::collections::{HashMap, HashSet, VecDeque};

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

use crate::logical_clock::LogicalTimestamp;
use crate::schema::Event;

/// Operation node in the dependency graph
///
/// Represents a single operation with its inputs, outputs, and dependencies.
/// Each node is identified by a unique operation ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationNode {
    /// Unique operation identifier
    pub op_id: String,
    /// Event type (e.g., "inference.start", "kernel.execute")
    pub event_type: String,
    /// BLAKE3 hash of inputs
    pub inputs_hash: B3Hash,
    /// BLAKE3 hash of outputs
    pub outputs_hash: B3Hash,
    /// Logical timestamp for ordering
    pub logical_timestamp: LogicalTimestamp,
    /// Operation IDs this node depends on
    pub dependencies: Vec<String>,
    /// Operation IDs that depend on this node
    pub dependents: Vec<String>,
    /// Original inputs (for replay)
    pub inputs: HashMap<String, serde_json::Value>,
    /// Original outputs (for verification)
    pub outputs: HashMap<String, serde_json::Value>,
}

impl OperationNode {
    /// Create a new operation node
    pub fn new(
        op_id: String,
        event_type: String,
        inputs: HashMap<String, serde_json::Value>,
        outputs: HashMap<String, serde_json::Value>,
        logical_timestamp: LogicalTimestamp,
    ) -> Result<Self> {
        let inputs_hash = Self::hash_map(&inputs)?;
        let outputs_hash = Self::hash_map(&outputs)?;

        Ok(Self {
            op_id,
            event_type,
            inputs_hash,
            outputs_hash,
            logical_timestamp,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            inputs,
            outputs,
        })
    }

    /// Hash a map of values using canonical JSON serialization
    ///
    /// Follows the pattern in `crates/adapteros-trace/src/schema.rs:187-190` for
    /// deterministic hashing with BLAKE3.
    fn hash_map(map: &HashMap<String, serde_json::Value>) -> Result<B3Hash> {
        let canonical_bytes = serde_jcs::to_vec(map)
            .map_err(|e| AosError::Parse(format!("Failed to serialize map: {}", e)))?;

        Ok(B3Hash::hash(&canonical_bytes))
    }
}

/// Dependency edge between operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationEdge {
    /// Source operation ID (dependency)
    pub from: String,
    /// Target operation ID (dependent)
    pub to: String,
    /// Type of dependency (e.g., "data", "control", "memory")
    pub dependency_type: DependencyType,
}

/// Type of dependency between operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyType {
    /// Data dependency (output of one operation used as input to another)
    Data,
    /// Control dependency (execution order constraint)
    Control,
    /// Memory dependency (shared memory access)
    Memory,
    /// Ordering dependency (logical timestamp ordering)
    Ordering,
}

/// Operation graph builder
///
/// Constructs a directed acyclic graph (DAG) of operations from trace events.
/// Provides topological sorting for deterministic execution order.
///
/// # Example
///
/// ```rust,ignore
/// let mut builder = OperationGraphBuilder::new();
/// for event in events {
///     builder.add_event(&event)?;
/// }
/// let graph = builder.build()?;
/// let execution_order = graph.topological_order;
/// ```
pub struct OperationGraphBuilder {
    /// Map of operation ID to node
    nodes: HashMap<String, OperationNode>,
    /// List of dependency edges
    edges: Vec<OperationEdge>,
    /// Topological execution order (computed on build)
    topological_order: Vec<String>,
}

impl OperationGraphBuilder {
    /// Create a new operation graph builder
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            topological_order: Vec::new(),
        }
    }

    /// Add an event to the graph
    ///
    /// Extracts operation information from the event and builds dependency edges.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to add to the graph
    ///
    /// # Returns
    ///
    /// `Ok(())` if the event was added successfully, or an error if the event
    /// could not be processed.
    pub fn add_event(&mut self, event: &Event) -> Result<()> {
        // Create operation node from event
        let node = OperationNode::new(
            event.op_id.clone(),
            event.event_type.clone(),
            event.inputs.clone(),
            event.outputs.clone(),
            event.logical_timestamp,
        )?;

        // Extract dependencies from inputs
        let dependencies = self.extract_dependencies(&event.inputs, &event.event_type)?;

        // Add dependency edges
        for dep_op_id in &dependencies {
            // Add edge from dependency to this operation
            self.edges.push(OperationEdge {
                from: dep_op_id.clone(),
                to: event.op_id.clone(),
                dependency_type: DependencyType::Data,
            });

            // Update dependent list of the dependency node
            if let Some(dep_node) = self.nodes.get_mut(dep_op_id) {
                dep_node.dependents.push(event.op_id.clone());
            }
        }

        // Store node with updated dependencies
        let mut node = node;
        node.dependencies = dependencies;
        self.nodes.insert(event.op_id.clone(), node);

        Ok(())
    }

    /// Extract operation dependencies from inputs
    ///
    /// Looks for references to other operations in the input data:
    /// - "op_ref": direct operation reference
    /// - "source_op": source operation ID
    /// - "input_op": input operation ID
    /// - "_op_id" suffix: any field ending in "_op_id"
    fn extract_dependencies(
        &self,
        inputs: &HashMap<String, serde_json::Value>,
        event_type: &str,
    ) -> Result<Vec<String>> {
        let mut deps = Vec::new();

        for (key, value) in inputs {
            // Check for direct operation references
            if key == "op_ref" || key == "source_op" || key == "input_op" || key.ends_with("_op_id")
            {
                if let Some(op_id) = value.as_str() {
                    deps.push(op_id.to_string());
                }
            }

            // Check for array of operation references
            if key.ends_with("_ops") || key == "dependencies" {
                if let Some(arr) = value.as_array() {
                    for item in arr {
                        if let Some(op_id) = item.as_str() {
                            deps.push(op_id.to_string());
                        }
                    }
                }
            }
        }

        // For inference events, add implicit dependencies based on token position
        if event_type.starts_with("inference.") {
            // Token generation depends on previous token
            if let Some(token_pos) = inputs.get("token_id").and_then(|v| v.as_u64()) {
                if token_pos > 0 {
                    // Add dependency on previous token
                    let prev_token_op = format!("token_{}", token_pos - 1);
                    if self.nodes.contains_key(&prev_token_op) {
                        deps.push(prev_token_op);
                    }
                }
            }
        }

        // Sort for deterministic ordering
        deps.sort();
        deps.dedup();

        Ok(deps)
    }

    /// Build the operation graph and compute topological order
    ///
    /// Uses Kahn's algorithm for topological sorting to ensure deterministic
    /// execution order. The algorithm:
    /// 1. Find all nodes with no dependencies (in-degree 0)
    /// 2. Process them in logical timestamp order
    /// 3. Remove processed nodes and update dependent in-degrees
    /// 4. Repeat until all nodes are processed
    ///
    /// # Returns
    ///
    /// An `OperationGraph` with nodes, edges, and topological execution order.
    ///
    /// # Errors
    ///
    /// Returns an error if the graph contains cycles (invalid DAG).
    pub fn build(mut self) -> Result<OperationGraph> {
        self.build_topological_order()?;

        Ok(OperationGraph {
            nodes: self.nodes,
            edges: self.edges,
            topological_order: self.topological_order,
        })
    }

    /// Build topological order using Kahn's algorithm
    ///
    /// Ensures deterministic ordering by:
    /// 1. Using logical timestamps to break ties
    /// 2. Sorting nodes at the same level by op_id for reproducibility
    fn build_topological_order(&mut self) -> Result<()> {
        // Compute in-degree for each node
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for (op_id, node) in &self.nodes {
            in_degree.insert(op_id.clone(), node.dependencies.len());
        }

        // Find all nodes with in-degree 0 (no dependencies)
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut zero_degree_nodes: Vec<(String, LogicalTimestamp)> = Vec::new();

        for (op_id, degree) in &in_degree {
            if *degree == 0 {
                if let Some(node) = self.nodes.get(op_id) {
                    zero_degree_nodes.push((op_id.clone(), node.logical_timestamp));
                }
            }
        }

        // Sort by logical timestamp for deterministic ordering
        zero_degree_nodes.sort_by(|a, b| a.1.cmp(&b.1));
        for (op_id, _) in zero_degree_nodes {
            queue.push_back(op_id);
        }

        let mut order = Vec::new();

        // Process nodes in topological order
        while let Some(op_id) = queue.pop_front() {
            order.push(op_id.clone());

            // Process all dependents of this node
            if let Some(node) = self.nodes.get(&op_id) {
                let mut next_level: Vec<(String, LogicalTimestamp)> = Vec::new();

                for dependent_id in &node.dependents {
                    // Decrease in-degree
                    if let Some(degree) = in_degree.get_mut(dependent_id) {
                        *degree -= 1;

                        // If in-degree becomes 0, add to queue
                        if *degree == 0 {
                            if let Some(dep_node) = self.nodes.get(dependent_id) {
                                next_level.push((dependent_id.clone(), dep_node.logical_timestamp));
                            }
                        }
                    }
                }

                // Sort by logical timestamp and add to queue
                next_level.sort_by(|a, b| a.1.cmp(&b.1));
                for (dep_id, _) in next_level {
                    queue.push_back(dep_id);
                }
            }
        }

        // Check for cycles
        if order.len() != self.nodes.len() {
            return Err(AosError::Validation(format!(
                "Graph contains cycles: processed {} of {} nodes",
                order.len(),
                self.nodes.len()
            )));
        }

        self.topological_order = order;
        Ok(())
    }
}

impl Default for OperationGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Constructed operation graph
///
/// Represents a complete DAG of operations with deterministic execution order.
#[derive(Debug, Clone)]
pub struct OperationGraph {
    /// Map of operation ID to node
    pub nodes: HashMap<String, OperationNode>,
    /// List of dependency edges
    pub edges: Vec<OperationEdge>,
    /// Topological execution order
    pub topological_order: Vec<String>,
}

impl OperationGraph {
    /// Get a node by operation ID
    pub fn get_node(&self, op_id: &str) -> Option<&OperationNode> {
        self.nodes.get(op_id)
    }

    /// Get all nodes in topological order
    pub fn nodes_in_order(&self) -> Vec<&OperationNode> {
        self.topological_order
            .iter()
            .filter_map(|op_id| self.nodes.get(op_id))
            .collect()
    }

    /// Verify the graph structure
    ///
    /// Checks that:
    /// - All edges reference existing nodes
    /// - All dependencies are valid
    /// - Topological order is correct
    pub fn verify(&self) -> Result<()> {
        // Verify all edges reference existing nodes
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                return Err(AosError::Validation(format!(
                    "Edge references non-existent source node: {}",
                    edge.from
                )));
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(AosError::Validation(format!(
                    "Edge references non-existent target node: {}",
                    edge.to
                )));
            }
        }

        // Verify all nodes in topological order exist
        for op_id in &self.topological_order {
            if !self.nodes.contains_key(op_id) {
                return Err(AosError::Validation(format!(
                    "Topological order references non-existent node: {}",
                    op_id
                )));
            }
        }

        // Verify topological order is correct (all dependencies come before dependents)
        let mut processed: HashSet<String> = HashSet::new();
        for op_id in &self.topological_order {
            if let Some(node) = self.nodes.get(op_id) {
                for dep in &node.dependencies {
                    if !processed.contains(dep) {
                        return Err(AosError::Validation(format!(
                            "Topological order violation: {} depends on {} but {} comes later",
                            op_id, dep, dep
                        )));
                    }
                }
            }
            processed.insert(op_id.clone());
        }

        Ok(())
    }

    /// Get statistics about the graph
    pub fn stats(&self) -> GraphStats {
        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            max_depth: self.calculate_max_depth(),
            avg_dependencies: self.calculate_avg_dependencies(),
        }
    }

    /// Calculate maximum depth of the graph
    fn calculate_max_depth(&self) -> usize {
        let mut depths: HashMap<String, usize> = HashMap::new();

        for op_id in &self.topological_order {
            if let Some(node) = self.nodes.get(op_id) {
                let max_dep_depth = node
                    .dependencies
                    .iter()
                    .filter_map(|dep| depths.get(dep))
                    .max()
                    .unwrap_or(&0);

                depths.insert(op_id.clone(), max_dep_depth + 1);
            }
        }

        depths.values().max().copied().unwrap_or(0)
    }

    /// Calculate average number of dependencies per node
    fn calculate_avg_dependencies(&self) -> f64 {
        if self.nodes.is_empty() {
            return 0.0;
        }

        let total_deps: usize = self.nodes.values().map(|n| n.dependencies.len()).sum();
        total_deps as f64 / self.nodes.len() as f64
    }
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Number of nodes in the graph
    pub node_count: usize,
    /// Number of edges in the graph
    pub edge_count: usize,
    /// Maximum depth of the graph
    pub max_depth: usize,
    /// Average number of dependencies per node
    pub avg_dependencies: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical_clock::LogicalTimestamp;
    use crate::schema::{Event, EventMetadata};
    use serde_json::json;

    fn create_test_event(
        op_id: &str,
        event_type: &str,
        inputs: HashMap<String, serde_json::Value>,
        outputs: HashMap<String, serde_json::Value>,
        logical_timestamp: LogicalTimestamp,
    ) -> Event {
        let metadata = EventMetadata {
            global_seed: B3Hash::hash(b"test_seed"),
            plan_id: "test_plan".to_string(),
            cpid: "test_cpid".to_string(),
            tenant_id: "test_tenant".to_string(),
            session_id: "test_session".to_string(),
            adapter_ids: Vec::new(),
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };

        Event::new(
            0,
            op_id.to_string(),
            event_type.to_string(),
            inputs,
            outputs,
            metadata,
            logical_timestamp,
        )
    }

    #[test]
    fn test_operation_node_creation() {
        let inputs = HashMap::new();
        let outputs = HashMap::new();
        let timestamp = LogicalTimestamp::new(0, 0, None, B3Hash::hash(b"test"));

        let node =
            OperationNode::new("op1".to_string(), "test".to_string(), inputs, outputs, timestamp)
                .unwrap();

        assert_eq!(node.op_id, "op1");
        assert_eq!(node.event_type, "test");
        assert_eq!(node.dependencies.len(), 0);
        assert_eq!(node.dependents.len(), 0);
    }

    #[test]
    fn test_graph_builder_single_node() {
        let mut builder = OperationGraphBuilder::new();

        let timestamp = LogicalTimestamp::new(0, 0, None, B3Hash::hash(b"test"));
        let event = create_test_event("op1", "test", HashMap::new(), HashMap::new(), timestamp);

        builder.add_event(&event).unwrap();

        let graph = builder.build().unwrap();

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.edges.len(), 0);
        assert_eq!(graph.topological_order.len(), 1);
        assert_eq!(graph.topological_order[0], "op1");
    }

    #[test]
    fn test_graph_builder_with_dependencies() {
        let mut builder = OperationGraphBuilder::new();

        // Create event1 with no dependencies
        let timestamp1 = LogicalTimestamp::new(0, 0, None, B3Hash::hash(b"ts1"));
        let event1 = create_test_event("op1", "test", HashMap::new(), HashMap::new(), timestamp1);
        builder.add_event(&event1).unwrap();

        // Create event2 that depends on event1
        let mut inputs2 = HashMap::new();
        inputs2.insert("source_op".to_string(), json!("op1"));
        let timestamp2 = LogicalTimestamp::new(1, 0, None, B3Hash::hash(b"ts2"));
        let event2 = create_test_event("op2", "test", inputs2, HashMap::new(), timestamp2);
        builder.add_event(&event2).unwrap();

        let graph = builder.build().unwrap();

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.topological_order.len(), 2);
        assert_eq!(graph.topological_order[0], "op1");
        assert_eq!(graph.topological_order[1], "op2");

        // Verify node dependencies
        let node2 = graph.get_node("op2").unwrap();
        assert_eq!(node2.dependencies.len(), 1);
        assert_eq!(node2.dependencies[0], "op1");
    }

    #[test]
    fn test_graph_builder_complex_dependencies() {
        let mut builder = OperationGraphBuilder::new();

        // op1 (no deps)
        let ts1 = LogicalTimestamp::new(0, 0, None, B3Hash::hash(b"ts1"));
        let event1 = create_test_event("op1", "test", HashMap::new(), HashMap::new(), ts1);
        builder.add_event(&event1).unwrap();

        // op2 (no deps)
        let ts2 = LogicalTimestamp::new(1, 0, None, B3Hash::hash(b"ts2"));
        let event2 = create_test_event("op2", "test", HashMap::new(), HashMap::new(), ts2);
        builder.add_event(&event2).unwrap();

        // op3 depends on op1 and op2
        let mut inputs3 = HashMap::new();
        inputs3.insert("dependencies".to_string(), json!(["op1", "op2"]));
        let ts3 = LogicalTimestamp::new(2, 0, None, B3Hash::hash(b"ts3"));
        let event3 = create_test_event("op3", "test", inputs3, HashMap::new(), ts3);
        builder.add_event(&event3).unwrap();

        let graph = builder.build().unwrap();

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);

        // Verify topological order
        let order = &graph.topological_order;
        let op1_idx = order.iter().position(|x| x == "op1").unwrap();
        let op2_idx = order.iter().position(|x| x == "op2").unwrap();
        let op3_idx = order.iter().position(|x| x == "op3").unwrap();

        // op3 must come after both op1 and op2
        assert!(op3_idx > op1_idx);
        assert!(op3_idx > op2_idx);

        // Verify graph structure
        graph.verify().unwrap();
    }

    #[test]
    fn test_token_dependency_inference() {
        let mut builder = OperationGraphBuilder::new();

        // token_0
        let mut inputs0 = HashMap::new();
        inputs0.insert("token_id".to_string(), json!(0));
        let ts0 = LogicalTimestamp::new(0, 0, Some(0), B3Hash::hash(b"ts0"));
        let event0 = create_test_event("token_0", "inference.token", inputs0, HashMap::new(), ts0);
        builder.add_event(&event0).unwrap();

        // token_1 should automatically depend on token_0
        let mut inputs1 = HashMap::new();
        inputs1.insert("token_id".to_string(), json!(1));
        let ts1 = LogicalTimestamp::new(1, 0, Some(1), B3Hash::hash(b"ts1"));
        let event1 = create_test_event("token_1", "inference.token", inputs1, HashMap::new(), ts1);
        builder.add_event(&event1).unwrap();

        let graph = builder.build().unwrap();

        let node1 = graph.get_node("token_1").unwrap();
        assert_eq!(node1.dependencies.len(), 1);
        assert_eq!(node1.dependencies[0], "token_0");
    }

    #[test]
    fn test_graph_stats() {
        let mut builder = OperationGraphBuilder::new();

        for i in 0..5 {
            let mut inputs = HashMap::new();
            if i > 0 {
                inputs.insert("source_op".to_string(), json!(format!("op{}", i - 1)));
            }
            let ts = LogicalTimestamp::new(i, 0, None, B3Hash::hash(format!("ts{}", i).as_bytes()));
            let event = create_test_event(&format!("op{}", i), "test", inputs, HashMap::new(), ts);
            builder.add_event(&event).unwrap();
        }

        let graph = builder.build().unwrap();
        let stats = graph.stats();

        assert_eq!(stats.node_count, 5);
        assert_eq!(stats.edge_count, 4);
        assert_eq!(stats.max_depth, 5);
        assert_eq!(stats.avg_dependencies, 0.8); // (0+1+1+1+1) / 5 = 0.8
    }
}

