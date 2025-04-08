//! Umazen Merkle Tree - High-Performance Merklized Data Structure

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    iter,
    marker::PhantomData,
    mem,
    ops::Range,
};

use sha3::{Digest, Keccak256};
use thiserror::Error;

/// Merkle Tree Configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerkleConfig {
    /// Hash function to use
    pub hash_algorithm: HashAlgorithm,
    /// Enable parallel hashing
    pub parallel: bool,
    /// Cache intermediate nodes
    pub caching: bool,
}

/// Supported Hash Algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// Keccak-256 (Ethereum compatible)
    Keccak256,
    /// SHA-256 (Solana compatible)
    Sha256,
}

/// Merkle Proof
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof<T> {
    /// Leaf index
    pub index: usize,
    /// Leaf hash
    pub leaf_hash: Vec<u8>,
    /// Proof hashes
    pub proof_hashes: Vec<Vec<u8>>,
    /// Tree depth
    pub tree_depth: usize,
    _marker: PhantomData<T>,
}

/// Merkle Tree
#[derive(Debug, Clone)]
pub struct MerkleTree<T> {
    leaves: Vec<Vec<u8>>,
    nodes: Vec<Vec<u8>>,
    depth: usize,
    config: MerkleConfig,
    cache: HashMap<(usize, usize), Vec<u8>>,
    _marker: PhantomData<T>,
}

/// Merkle Tree Error
#[derive(Debug, Error)]
pub enum MerkleError {
    #[error("Empty leaves")]
    EmptyLeaves,
    #[error("Invalid index")]
    InvalidIndex,
    #[error("Invalid proof")]
    InvalidProof,
    #[error("Hash computation error")]
    HashError,
    #[error("Serialization error")]
    SerializationError,
}

impl<T> MerkleTree<T>
where
    T: AsRef<[u8]>,
{
    /// Construct new Merkle Tree
    pub fn new(leaves: Vec<T>, config: MerkleConfig) -> Result<Self, MerkleError> {
        if leaves.is_empty() {
            return Err(MerkleError::EmptyLeaves);
        }

        let leaves_hashed: Vec<Vec<u8>> = if config.parallel {
            leaves.par_iter()
                .map(|leaf| Self::hash_leaf(leaf, config.hash_algorithm))
                .collect()
        } else {
            leaves.iter()
                .map(|leaf| Self::hash_leaf(leaf, config.hash_algorithm))
                .collect()
        };

        let mut tree = Self {
            leaves: leaves_hashed,
            nodes: Vec::new(),
            depth: 0,
            config,
            cache: HashMap::new(),
            _marker: PhantomData,
        };

        tree.build_tree()?;
        Ok(tree)
    }

    /// Build complete Merkle Tree
    fn build_tree(&mut self) -> Result<(), MerkleError> {
        let mut current_level = self.leaves.clone();
        self.depth = 0;

        while current_level.len() > 1 {
            let mut next_level = Vec::with_capacity((current_level.len() + 1) / 2);
            let mut i = 0;
            
            while i < current_level.len() {
                let right = if i + 1 < current_level.len() {
                    &current_level[i + 1]
                } else {
                    &current_level[i]
                };

                let hash = Self::hash_nodes(¤t_level[i], right, self.config.hash_algorithm)?;
                next_level.push(hash);
                i += 2;
            }

            if self.config.caching {
                self.cache_level(self.depth, ¤t_level);
            }

            self.nodes.extend(current_level);
            current_level = next_level;
            self.depth += 1;
        }

        self.nodes.extend(current_level);
        Ok(())
    }

    /// Get Merkle Root
    pub fn root(&self) -> Option<&[u8]> {
        self.nodes.last().map(|v| v.as_slice())
    }

    /// Generate Merkle Proof
    pub fn proof(&self, index: usize) -> Result<MerkleProof<T>, MerkleError> {
        if index >= self.leaves.len() {
            return Err(MerkleError::InvalidIndex);
        }

        let mut proof_hashes = Vec::with_capacity(self.depth);
        let mut current_index = index;
        let mut current_level_size = self.leaves.len();
        let mut level_start = 0;

        for level in 0..self.depth {
            let sibling_index = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };

            if sibling_index < current_level_size {
                let sibling_hash = if self.config.caching {
                    self.get_cached_hash(level, sibling_index)?
                } else {
                    self.nodes[level_start + sibling_index].clone()
                };
                
                proof_hashes.push(sibling_hash);
            }

            level_start += current_level_size;
            current_index /= 2;
            current_level_size = (current_level_size + 1) / 2;
        }

        Ok(MerkleProof {
            index,
            leaf_hash: self.leaves[index].clone(),
            proof_hashes,
            tree_depth: self.depth,
            _marker: PhantomData,
        })
    }

    /// Verify Merkle Proof
    pub fn verify(
        root: &[u8],
        proof: &MerkleProof<T>,
        hash_algorithm: HashAlgorithm,
    ) -> Result<bool, MerkleError> {
        let mut computed_hash = proof.leaf_hash.clone();
        let mut current_index = proof.index;

        for sibling_hash in &proof.proof_hashes {
            let (left, right) = if current_index % 2 == 0 {
                (&computed_hash, sibling_hash)
            } else {
                (sibling_hash, &computed_hash)
            };

            computed_hash = Self::hash_nodes(left, right, hash_algorithm)?;
            current_index /= 2;
        }

        Ok(computed_hash == root)
    }

    /// Update leaf and recompute tree
    pub fn update_leaf(&mut self, index: usize, new_leaf: T) -> Result<(), MerkleError> {
        if index >= self.leaves.len() {
            return Err(MerkleError::InvalidIndex);
        }

        // Update leaf
        self.leaves[index] = Self::hash_leaf(&new_leaf, self.config.hash_algorithm);

        // Rebuild tree from updated leaf
        let mut level_start = 0;
        let mut current_index = index;
        let mut current_level_size = self.leaves.len();

        for level in 0..self.depth {
            let node_index = level_start + current_index;
            
            // Compute parent hash
            let sibling_index = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };

            let sibling_hash = if sibling_index < current_level_size {
                &self.nodes[level_start + sibling_index]
            } else {
                &self.nodes[node_index]
            };

            let new_hash = if current_index % 2 == 0 {
                Self::hash_nodes(&self.nodes[node_index], sibling_hash, self.config.hash_algorithm)?
            } else {
                Self::hash_nodes(sibling_hash, &self.nodes[node_index], self.config.hash_algorithm)?
            };

            // Update parent node
            let parent_level_start = level_start + current_level_size;
            let parent_index = current_index / 2;
            self.nodes[parent_level_start + parent_index] = new_hash;

            // Move up the tree
            level_start += current_level_size;
            current_index = parent_index;
            current_level_size = (current_level_size + 1) / 2;
        }

        Ok(())
    }

    /// Batch update leaves
    pub fn batch_update(&mut self, updates: HashMap<usize, T>) -> Result<(), MerkleError> {
        for (index, leaf) in updates {
            self.update_leaf(index, leaf)?;
        }
        Ok(())
    }

    /// Cache level nodes
    fn cache_level(&mut self, level: usize, nodes: &[Vec<u8>]) {
        for (idx, node) in nodes.iter().enumerate() {
            self.cache.insert((level, idx), node.clone());
        }
    }

    /// Get cached hash
    fn get_cached_hash(&self, level: usize, index: usize) -> Result<Vec<u8>, MerkleError> {
        self.cache
            .get(&(level, index))
            .cloned()
            .ok_or(MerkleError::HashError)
    }

    /// Hash leaf node
    fn hash_leaf(leaf: &T, algorithm: HashAlgorithm) -> Vec<u8> {
        match algorithm {
            HashAlgorithm::Keccak256 => {
                let mut hasher = Keccak256::new();
                hasher.update(leaf.as_ref());
                hasher.finalize().to_vec()
            }
            HashAlgorithm::Sha256 => {
                let mut hasher = sha2::Sha256::new();
                hasher.update(leaf.as_ref());
                hasher.finalize().to_vec()
            }
        }
    }

    /// Hash two nodes
    fn hash_nodes(
        left: &[u8],
        right: &[u8],
        algorithm: HashAlgorithm,
    ) -> Result<Vec<u8>, MerkleError> {
        let mut combined = Vec::with_capacity(left.len() + right.len());
        combined.extend_from_slice(left);
        combined.extend_from_slice(right);

        Ok(match algorithm {
            HashAlgorithm::Keccak256 => {
                let mut hasher = Keccak256::new();
                hasher.update(&combined);
                hasher.finalize().to_vec()
            }
            HashAlgorithm::Sha256 => {
                let mut hasher = sha2::Sha256::new();
                hasher.update(&combined);
                hasher.finalize().to_vec()
            }
        })
    }
}

impl<T> fmt::Display for MerkleTree<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MerkleTree(depth={}, leaves={})", self.depth, self.leaves.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    const TEST_DATA: &[&str] = &["a", "b", "c", "d", "e", "f", "g", "h"];

    #[test]
    fn test_tree_construction() {
        let config = MerkleConfig {
            hash_algorithm: HashAlgorithm::Keccak256,
            parallel: false,
            caching: false,
        };
        
        let tree = MerkleTree::new(TEST_DATA.to_vec(), config).unwrap();
        assert_eq!(tree.depth, 3);
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_proof_generation() {
        let config = MerkleConfig {
            hash_algorithm: HashAlgorithm::Keccak256,
            parallel: false,
            caching: false,
        };
        
        let tree = MerkleTree::new(TEST_DATA.to_vec(), config).unwrap();
        let proof = tree.proof(0).unwrap();
        assert_eq!(proof.proof_hashes.len(), 3);
    }

    #[test]
    fn test_proof_verification() {
        let config = MerkleConfig {
            hash_algorithm: HashAlgorithm::Keccak256,
            parallel: false,
            caching: false,
        };
        
        let tree = MerkleTree::new(TEST_DATA.to_vec(), config).unwrap();
        let root = tree.root().unwrap();
        let proof = tree.proof(0).unwrap();
        
        assert!(MerkleTree::verify(root, &proof, HashAlgorithm::Keccak256).unwrap());
    }

    #[test]
    fn test_leaf_update() {
        let config = MerkleConfig {
            hash_algorithm: HashAlgorithm::Keccak256,
            parallel: false,
            caching: false,
        };
        
        let mut tree = MerkleTree::new(TEST_DATA.to_vec(), config).unwrap();
        let original_root = tree.root().unwrap().to_vec();
        
        tree.update_leaf(0, "new_value").unwrap();
        let new_root = tree.root().unwrap();
        
        assert_ne!(original_root, new_root);
    }
}
