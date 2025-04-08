//! Model Cache Manager - Versioned AI Model Storage with Blockchain Validation

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use {
    anchor_lang::{prelude::*, solana_program::hash::hashv},
    serde::{Deserialize, Serialize},
    solana_program::clock::Clock,
    std::{
        collections::HashMap,
        fs,
        io::{self, Read, Write},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    },
    sha2::{Digest, Sha256},
};

/// Model metadata structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_hash: [u8; 32],
    pub version: u32,
    pub timestamp: i64,
    pub owner: Pubkey,
    pub storage_uri: String,
    pub encrypted: bool,
}

/// Model cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_versions: usize,
    pub cache_dir: PathBuf,
    pub validate_hash: bool,
}

/// Local model cache manager
pub struct ModelCache {
    config: CacheConfig,
    versions: HashMap<u32, ModelMetadata>,
    current_version: u32,
}

impl ModelCache {
    /// Initialize new model cache
    pub fn new(config: CacheConfig) -> Result<Self> {
        fs::create_dir_all(&config.cache_dir)?;
        
        let mut cache = Self {
            config,
            versions: HashMap::new(),
            current_version: 0,
        };
        
        cache.load_existing()?;
        Ok(cache)
    }

    /// Add new model version to cache
    pub fn add_model(&mut self, data: &[u8], metadata: ModelMetadata) -> Result<()> {
        if self.config.validate_hash {
            let calculated_hash = self.calculate_hash(data);
            if calculated_hash != metadata.model_hash {
                return Err(ErrorCode::HashMismatch.into());
            }
        }

        let version = metadata.version;
        let file_path = self.model_path(version);
        
        // Write model data
        let mut file = fs::File::create(&file_path)?;
        file.write_all(data)?;
        
        // Store metadata
        self.versions.insert(version, metadata);
        self.current_version = version;
        
        // Cleanup old versions
        self.cleanup_old_versions()?;
        
        Ok(())
    }

    /// Get model data for specific version
    pub fn get_model(&self, version: u32) -> Result<Vec<u8>> {
        let metadata = self.versions.get(&version)
            .ok_or(ErrorCode::ModelNotFound)?;
            
        let file_path = self.model_path(version);
        let mut file = fs::File::open(&file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        
        if self.config.validate_hash {
            let calculated_hash = self.calculate_hash(&buffer);
            if calculated_hash != metadata.model_hash {
                return Err(ErrorCode::HashMismatch.into());
            }
        }
        
        Ok(buffer)
    }

    /// Verify model integrity against blockchain
    pub fn verify_on_chain(&self, program: &Program, version: u32) -> Result<bool> {
        let metadata = self.versions.get(&version)
            .ok_or(ErrorCode::ModelNotFound)?;
        
        let on_chain_meta: ModelMetadata = program.account(metadata.owner)?;
        
        Ok(metadata.model_hash == on_chain_meta.model_hash &&
           metadata.storage_uri == on_chain_meta.storage_uri)
    }

    /// Calculate SHA256 hash of model data
    pub fn calculate_hash(&self, data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Get path for model storage
    fn model_path(&self, version: u32) -> PathBuf {
        self.config.cache_dir.join(format!("model_v{}.bin", version))
    }

    /// Load existing models from cache directory
    fn load_existing(&mut self) -> Result<()> {
        for entry in fs::read_dir(&self.config.cache_dir)? {
            let path = entry?.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("bin") {
                let version = self.parse_version(&path)?;
                let metadata_path = self.metadata_path(version);
                let metadata_file = fs::File::open(metadata_path)?;
                let metadata: ModelMetadata = serde_json::from_reader(metadata_file)?;
                
                self.versions.insert(version, metadata);
                if version > self.current_version {
                    self.current_version = version;
                }
            }
        }
        Ok(())
    }

    /// Cleanup old model versions
    fn cleanup_old_versions(&mut self) -> Result<()> {
        let mut versions: Vec<u32> = self.versions.keys().cloned().collect();
        versions.sort_unstable();
        
        while versions.len() > self.config.max_versions {
            if let Some(oldest) = versions.first() {
                let path = self.model_path(*oldest);
                fs::remove_file(path)?;
                self.versions.remove(oldest);
                versions.remove(0);
            }
        }
        Ok(())
    }

    /// Get metadata file path
    fn metadata_path(&self, version: u32) -> PathBuf {
        self.config.cache_dir.join(format!("meta_v{}.json", version))
    }

    /// Parse version from filename
    fn parse_version(&self, path: &Path) -> Result<u32> {
        let filename = path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or(ErrorCode::InvalidFilename)?;
        
        let version_str = filename.trim_start_matches("model_v")
            .trim_end_matches(".bin");
        
        version_str.parse()
            .map_err(|_| ErrorCode::InvalidVersion.into())
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Model data hash mismatch")]
    HashMismatch,
    #[msg("Model version not found")]
    ModelNotFound,
    #[msg("Invalid cache directory path")]
    InvalidCacheDir,
    #[msg("Invalid filename format")]
    InvalidFilename,
    #[msg("Invalid version number")]
    InvalidVersion,
    #[msg("Storage I/O error")]
    StorageError,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config() -> CacheConfig {
        CacheConfig {
            max_versions: 3,
            cache_dir: tempdir().unwrap().into_path(),
            validate_hash: true,
        }
    }

    fn test_metadata(version: u32) -> ModelMetadata {
        ModelMetadata {
            model_hash: [0; 32],
            version,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            owner: Pubkey::new_unique(),
            storage_uri: "ipfs://test".to_string(),
            encrypted: false,
        }
    }

    #[test]
    fn test_add_and_retrieve() {
        let mut cache = ModelCache::new(test_config()).unwrap();
        let data = vec![1,2,3];
        let mut meta = test_metadata(1);
        meta.model_hash = cache.calculate_hash(&data);
        
        cache.add_model(&data, meta.clone()).unwrap();
        let retrieved = cache.get_model(1).unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_version_cleanup() {
        let config = test_config();
        let mut cache = ModelCache::new(config.clone()).unwrap();
        
        for v in 1..=5 {
            let data = vec![v as u8];
            let mut meta = test_metadata(v);
            meta.model_hash = cache.calculate_hash(&data);
            cache.add_model(&data, meta).unwrap();
        }
        
        assert_eq!(cache.versions.len(), config.max_versions);
        assert!(cache.get_model(1).is_err());
        assert!(cache.get_model(5).is_ok());
    }

    #[test]
    fn test_hash_validation() {
        let mut cache = ModelCache::new(test_config()).unwrap();
        let data = vec![1,2,3];
        let mut meta = test_metadata(1);
        meta.model_hash = [0; 32]; // Incorrect hash
        
        let result = cache.add_model(&data, meta);
        assert!(matches!(result, Err(ErrorCode::HashMismatch.into())));
    }
}
