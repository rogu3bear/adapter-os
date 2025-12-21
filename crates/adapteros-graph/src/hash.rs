//! Hash graph implementation with canonical tensor metadata

use crate::canonical::{canonical_tensor_repr, CanonicalTensor, HASH_VERSION};
use crate::tensor::Tensor;
use adapteros_core::{B3Hash, Result};

/// Hash graph node representing a tensor with canonical metadata
#[derive(Debug, Clone)]
pub struct HashGraphNode {
    /// Hash schema version
    pub hash_version: u8,
    /// Canonical tensor representation
    pub canonical: CanonicalTensor,
    /// Tensor data hash
    pub data_hash: B3Hash,
    /// Combined metadata + data hash
    pub node_hash: B3Hash,
}

impl HashGraphNode {
    /// Create a new hash graph node from tensor
    pub fn from_tensor(tensor: &Tensor) -> Result<Self> {
        // Create canonical representation
        let canonical = canonical_tensor_repr(tensor)?;

        // Hash tensor data
        let data_hash = B3Hash::hash(&tensor.data);

        // Serialize canonical metadata
        let metadata_bytes = canonical.to_canonical_bytes()?;

        // Create combined hash: version + metadata + data
        let node_hash =
            B3Hash::hash_multi(&[&[HASH_VERSION], &metadata_bytes, data_hash.as_bytes()]);

        Ok(Self {
            hash_version: HASH_VERSION,
            canonical,
            data_hash,
            node_hash,
        })
    }

    /// Get the node hash
    pub fn hash(&self) -> B3Hash {
        self.node_hash
    }

    /// Get the data hash
    pub fn data_hash(&self) -> B3Hash {
        self.data_hash
    }

    /// Get canonical representation
    pub fn canonical(&self) -> &CanonicalTensor {
        &self.canonical
    }
}

/// Hash graph containing multiple tensor nodes
#[derive(Debug, Clone)]
pub struct HashGraph {
    /// Hash schema version
    pub hash_version: u8,
    /// Graph nodes
    pub nodes: Vec<HashGraphNode>,
    /// Graph-level hash
    pub graph_hash: B3Hash,
}

impl Default for HashGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl HashGraph {
    /// Create a new hash graph
    pub fn new() -> Self {
        Self {
            hash_version: HASH_VERSION,
            nodes: Vec::new(),
            graph_hash: B3Hash::hash(&[]),
        }
    }

    /// Add a tensor node to the graph
    pub fn add_tensor(&mut self, tensor: &Tensor) -> Result<()> {
        let node = HashGraphNode::from_tensor(tensor)?;
        self.nodes.push(node);
        self.update_graph_hash();
        Ok(())
    }

    /// Update the graph hash based on all nodes
    fn update_graph_hash(&mut self) {
        if self.nodes.is_empty() {
            self.graph_hash = B3Hash::hash(&[]);
            return;
        }

        // Sort nodes by their hash for deterministic ordering
        let mut node_hashes: Vec<&B3Hash> = self.nodes.iter().map(|n| &n.node_hash).collect();
        node_hashes.sort();

        // Hash all node hashes together
        let hash_bytes: Vec<&[u8]> = node_hashes.iter().map(|h| h.as_bytes() as &[u8]).collect();
        self.graph_hash = B3Hash::hash_multi(&hash_bytes);
    }

    /// Get the graph hash
    pub fn hash(&self) -> B3Hash {
        self.graph_hash
    }

    /// Get number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if graph is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get node by index
    pub fn get_node(&self, index: usize) -> Option<&HashGraphNode> {
        self.nodes.get(index)
    }

    /// Get all node hashes
    pub fn node_hashes(&self) -> Vec<B3Hash> {
        self.nodes.iter().map(|n| n.node_hash).collect()
    }
}

/// Compute hash for tensor using canonical metadata
pub fn hash_tensor_with_metadata(tensor: &Tensor) -> Result<B3Hash> {
    let node = HashGraphNode::from_tensor(tensor)?;
    Ok(node.hash())
}

/// Compute hash for multiple tensors
pub fn hash_tensors(tensors: &[&Tensor]) -> Result<B3Hash> {
    let mut graph = HashGraph::new();
    for tensor in tensors {
        graph.add_tensor(tensor)?;
    }
    Ok(graph.hash())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::{DataType, DeviceFamily, MemoryLayout, Tensor};

    fn create_test_tensor() -> Tensor {
        Tensor::new(
            DataType::Float32,
            vec![2, 3],
            MemoryLayout::RowMajor,
            DeviceFamily::MetalM3,
            vec![0u8; 24],
        )
        .unwrap()
    }

    #[test]
    fn test_hash_graph_node_creation() {
        let tensor = create_test_tensor();
        let node = HashGraphNode::from_tensor(&tensor).unwrap();

        assert_eq!(node.canonical.version, HASH_VERSION);
        assert_eq!(node.canonical.shape, vec![2, 3]);
    }

    #[test]
    fn test_hash_graph_operations() {
        let tensor1 = create_test_tensor();
        let tensor2 = Tensor::new(
            DataType::Float16,
            vec![4, 5],
            MemoryLayout::ColumnMajor,
            DeviceFamily::MetalM4,
            vec![0u8; 40],
        )
        .unwrap();

        let mut graph = HashGraph::new();
        assert!(graph.is_empty());

        graph.add_tensor(&tensor1).unwrap();
        assert_eq!(graph.len(), 1);

        graph.add_tensor(&tensor2).unwrap();
        assert_eq!(graph.len(), 2);

        let graph_hash = graph.hash();
        assert_ne!(graph_hash, B3Hash::hash(&[]));
    }

    #[test]
    fn test_deterministic_hashing() {
        let tensor = create_test_tensor();

        let hash1 = hash_tensor_with_metadata(&tensor).unwrap();
        let hash2 = hash_tensor_with_metadata(&tensor).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_multiple_tensor_hashing() {
        let tensor1 = create_test_tensor();
        let tensor2 = Tensor::new(
            DataType::Float16,
            vec![1, 2],
            MemoryLayout::RowMajor,
            DeviceFamily::CPU,
            vec![0u8; 4],
        )
        .unwrap();

        let hash1 = hash_tensors(&[&tensor1, &tensor2]).unwrap();
        let hash2 = hash_tensors(&[&tensor1, &tensor2]).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_order_independence() {
        let tensor1 = create_test_tensor();
        let tensor2 = Tensor::new(
            DataType::Float16,
            vec![1, 2],
            MemoryLayout::RowMajor,
            DeviceFamily::CPU,
            vec![0u8; 4],
        )
        .unwrap();

        let hash1 = hash_tensors(&[&tensor1, &tensor2]).unwrap();
        let hash2 = hash_tensors(&[&tensor2, &tensor1]).unwrap();

        // Should be the same due to sorting
        assert_eq!(hash1, hash2);
    }
}
