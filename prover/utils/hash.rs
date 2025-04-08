//! Umazen Cryptographic Hash Utilities - Multi-Algorithm Hash System

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use std::{
    fmt,
    io::{self, Read},
    marker::PhantomData,
};

use digest::{
    core_api::BlockSizeUser,
    typenum::{U32, U64},
    FixedOutput,
    HashMarker,
    Output,
    OutputSizeUser,
    Update,
};
use sha2::{Sha256, Sha512};
use sha3::{Keccak256, Keccak512};
use thiserror::Error;
use generic_array::GenericArray;

/// Cryptographic Hash Algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-256 (FIPS 180-4)
    SHA256,
    /// SHA-512 (FIPS 180-4)
    SHA512,
    /// Keccak-256 (Ethereum)
    KECCAK256,
    /// Keccak-512 
    KECCAK512,
    /// BLAKE3 (Modern hash)
    BLAKE3,
    /// Poseidon (ZK-friendly)
    POSEIDON,
}

/// Hash Error Types
#[derive(Debug, Error)]
pub enum HashError {
    #[error("Input data exceeds maximum size")]
    InputTooLarge,
    #[error("Serialization failed: {0}")]
    SerializationError(#[from] bincode::Error),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Invalid hash length")]
    InvalidHashLength,
    #[error("Unsupported algorithm")]
    UnsupportedAlgorithm,
}

/// Universal Hash Output
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashOutput {
    algorithm: HashAlgorithm,
    bytes: GenericArray<u8, U64>,
}

impl HashOutput {
    /// Create from raw bytes
    pub fn new(algorithm: HashAlgorithm, bytes: &[u8]) -> Result<Self, HashError> {
        let mut output = GenericArray::default();
        let len = bytes.len();
        
        match algorithm {
            HashAlgorithm::SHA256 | HashAlgorithm::KECCAK256 => {
                if len != 32 {
                    return Err(HashError::InvalidHashLength);
                }
                output[..32].copy_from_slice(bytes);
            }
            HashAlgorithm::SHA512 | HashAlgorithm::KECCAK512 | HashAlgorithm::BLAKE3 => {
                if len != 64 {
                    return Err(HashError::InvalidHashLength);
                }
                output.copy_from_slice(bytes);
            }
            HashAlgorithm::POSEIDON => {
                if len != 32 {
                    return Err(HashError::InvalidHashLength);
                }
                output[..32].copy_from_slice(bytes);
            }
        }

        Ok(Self {
            algorithm,
            bytes: output,
        })
    }

    /// Convert to byte array
    pub fn as_bytes(&self) -> &[u8] {
        match self.algorithm {
            HashAlgorithm::SHA256 | HashAlgorithm::KECCAK256 | HashAlgorithm::POSEIDON => &self.bytes[..32],
            _ => &self.bytes[..]
        }
    }
}

/// Stream Hasher Trait
pub trait StreamHasher: Update + FixedOutput + Default + Clone {}

/// Generic Hash Processor
pub struct HashProcessor<H: StreamHasher> {
    hasher: H,
    algorithm: HashAlgorithm,
    _marker: PhantomData<H>,
}

impl<H> HashProcessor<H>
where
    H: StreamHasher,
{
    /// Create new processor
    pub fn new(algorithm: HashAlgorithm) -> Self {
        Self {
            hasher: H::default(),
            algorithm,
            _marker: PhantomData,
        }
    }

    /// Process data in chunks
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Finalize hash
    pub fn finalize(self) -> HashOutput {
        let result = self.hasher.finalize_fixed();
        HashOutput {
            algorithm: self.algorithm,
            bytes: result,
        }
    }
}

/// Unified Hash Context
pub struct UniversalHasher {
    processor: Box<dyn UniversalHashImpl>,
    algorithm: HashAlgorithm,
}

impl UniversalHasher {
    /// Create new hasher
    pub fn new(algorithm: HashAlgorithm) -> Result<Self, HashError> {
        let processor: Box<dyn UniversalHashImpl> = match algorithm {
            HashAlgorithm::SHA256 => Box::new(Sha256Processor::new()),
            HashAlgorithm::SHA512 => Box::new(Sha512Processor::new()),
            HashAlgorithm::KECCAK256 => Box::new(Keccak256Processor::new()),
            HashAlgorithm::KECCAK512 => Box::new(Keccak512Processor::new()),
            HashAlgorithm::BLAKE3 => Box::new(Blake3Processor::new()),
            HashAlgorithm::POSEIDON => Box::new(PoseidonProcessor::new()?),
        };

        Ok(Self {
            processor,
            algorithm,
        })
    }

    /// Update with data
    pub fn update(&mut self, data: &[u8]) -> Result<(), HashError> {
        self.processor.update(data)
    }

    /// Finalize hash
    pub fn finalize(self) -> Result<HashOutput, HashError> {
        self.processor.finalize(self.algorithm)
    }
}

/// Universal Hash Implementation Trait
trait UniversalHashImpl {
    fn update(&mut self, data: &[u8]) -> Result<(), HashError>;
    fn finalize(&mut self, algorithm: HashAlgorithm) -> Result<HashOutput, HashError>;
}

// SHA-256 Implementation
struct Sha256Processor {
    hasher: Sha256,
}

impl Sha256Processor {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }
}

impl UniversalHashImpl for Sha256Processor {
    fn update(&mut self, data: &[u8]) -> Result<(), HashError> {
        self.hasher.update(data);
        Ok(())
    }

    fn finalize(&mut self, algorithm: HashAlgorithm) -> Result<HashOutput, HashError> {
        let result = self.hasher.finalize_reset();
        HashOutput::new(algorithm, &result)
    }
}

// Keccak-256 Implementation
struct Keccak256Processor {
    hasher: Keccak256,
}

impl Keccak256Processor {
    fn new() -> Self {
        Self {
            hasher: Keccak256::new(),
        }
    }
}

impl UniversalHashImpl for Keccak256Processor {
    fn update(&mut self, data: &[u8]) -> Result<(), HashError> {
        self.hasher.update(data);
        Ok(())
    }

    fn finalize(&mut self, algorithm: HashAlgorithm) -> Result<HashOutput, HashError> {
        let result = self.hasher.finalize_reset();
        HashOutput::new(algorithm, &result)
    }
}

// Blake3 Implementation
struct Blake3Processor {
    hasher: blake3::Hasher,
}

impl Blake3Processor {
    fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }
}

impl UniversalHashImpl for Blake3Processor {
    fn update(&mut self, data: &[u8]) -> Result<(), HashError> {
        self.hasher.update(data);
        Ok(())
    }

    fn finalize(&mut self, algorithm: HashAlgorithm) -> Result<HashOutput, HashError> {
        let result = self.hasher.finalize_reset();
        HashOutput::new(algorithm, result.as_bytes())
    }
}

// Poseidon Implementation (Simplified)
struct PoseidonProcessor {
    state: [u64; 4],
}

impl PoseidonProcessor {
    fn new() -> Result<Self, HashError> {
        Ok(Self {
            state: [0u64; 4],
        })
    }

    fn poseidon_round(&mut self, input: &[u64]) {
        // Simplified round implementation
        // Actual implementation would use proper field operations
        for i in 0..4 {
            self.state[i] = self.state[i].wrapping_add(input.get(i).copied().unwrap_or(0));
        }
    }
}

impl UniversalHashImpl for PoseidonProcessor {
    fn update(&mut self, data: &[u8]) -> Result<(), HashError> {
        let chunks = data.chunks_exact(32);
        for chunk in chunks {
            let mut input = [0u64; 4];
            for i in 0..4 {
                let bytes = chunk.get(i*8..(i+1)*8).unwrap_or_default();
                input[i] = u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]));
            }
            self.poseidon_round(&input);
        }
        Ok(())
    }

    fn finalize(&mut self, algorithm: HashAlgorithm) -> Result<HashOutput, HashError> {
        let mut output = [0u8; 32];
        for i in 0..4 {
            let bytes = self.state[i].to_le_bytes();
            output[i*8..(i+1)*8].copy_from_slice(&bytes);
        }
        HashOutput::new(algorithm, &output)
    }
}

/// Helper Functions

/// Compute single hash
pub fn hash_data(algorithm: HashAlgorithm, data: &[u8]) -> Result<HashOutput, HashError> {
    let mut hasher = UniversalHasher::new(algorithm)?;
    hasher.update(data)?;
    hasher.finalize()
}

/// Hash large data streams
pub fn hash_stream<R: Read>(
    algorithm: HashAlgorithm,
    mut reader: R,
) -> Result<HashOutput, HashError> {
    let mut hasher = UniversalHasher::new(algorithm)?;
    let mut buffer = [0; 4096];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count])?;
    }

    hasher.finalize()
}

/// Verify hash consistency
pub fn verify_hash(
    data: &[u8],
    expected_hash: &HashOutput,
) -> Result<bool, HashError> {
    let actual_hash = hash_data(expected_hash.algorithm, data)?;
    Ok(actual_hash == *expected_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATA: &[u8] = b"umazen.ai";

    #[test]
    fn test_sha256_hashing() {
        let hash = hash_data(HashAlgorithm::SHA256, TEST_DATA).unwrap();
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_keccak256_hashing() {
        let hash = hash_data(HashAlgorithm::KECCAK256, TEST_DATA).unwrap();
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_blake3_hashing() {
        let hash = hash_data(HashAlgorithm::BLAKE3, TEST_DATA).unwrap();
        assert_eq!(hash.as_bytes().len(), 64);
    }

    #[test]
    fn test_poseidon_hashing() {
        let hash = hash_data(HashAlgorithm::POSEIDON, TEST_DATA).unwrap();
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_stream_hashing() {
        let data = vec![b"hello", b" ", b"world"];
        let mut hasher = UniversalHasher::new(HashAlgorithm::SHA256).unwrap();
        
        for chunk in &data {
            hasher.update(chunk).unwrap();
        }
        
        let hash = hasher.finalize().unwrap();
        assert_eq!(hash.as_bytes().len(), 32);
    }
}
