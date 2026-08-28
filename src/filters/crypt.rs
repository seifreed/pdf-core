use super::{FilterError, FilterResult};
use crate::crypto::{get_default_crypto_provider, CryptoConfig, CryptoProvider};
use crate::performance::ResourceBudget;
use crate::types::CryptFilterParams;

/// Crypt filter for decrypting encrypted PDF streams
#[allow(dead_code)]
pub struct CryptFilter {
    crypto_provider: Box<dyn CryptoProvider + Send + Sync>,
    config: CryptoConfig,
}

impl CryptFilter {
    /// Create a new Crypt filter
    pub fn new() -> Self {
        Self {
            crypto_provider: get_default_crypto_provider(),
            config: CryptoConfig::default(),
        }
    }

    /// Create a Crypt filter with custom configuration
    pub fn with_config(config: CryptoConfig) -> Self {
        Self {
            crypto_provider: get_default_crypto_provider(),
            config,
        }
    }

    /// Decrypt data using the Crypt filter
    pub fn decrypt(
        &self,
        data: &[u8],
        params: &CryptFilterParams,
        key: &[u8],
    ) -> FilterResult<Vec<u8>> {
        match params {
            CryptFilterParams::Identity => {
                // Identity filter - no encryption
                Ok(data.to_vec())
            }
            CryptFilterParams::V2 { name } => self.decrypt_v2(data, name, key),
            CryptFilterParams::AESV2 { name } => self.decrypt_aes_v2(data, name, key),
            CryptFilterParams::AESV3 { name } => self.decrypt_aes_v3(data, name, key),
        }
    }

    /// Decrypt data while charging the shared resource budget.
    pub fn decrypt_with_budget(
        &self,
        data: &[u8],
        params: &CryptFilterParams,
        key: &[u8],
        budget: &ResourceBudget,
    ) -> FilterResult<Vec<u8>> {
        budget
            .check()
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        budget
            .consume_input(data.len() as u64)
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        let limit = usize::try_from(
            budget
                .max_decoded_bytes_per_stream
                .min(budget.remaining_decoded_bytes()),
        )
        .unwrap_or(usize::MAX);
        if data.len() > limit {
            return Err(FilterError::CryptError(
                "decrypted output exceeds resource budget".to_string(),
            ));
        }
        let output = self.decrypt(data, params, key)?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        Ok(output)
    }

    /// Encrypt data using the Crypt filter
    pub fn encrypt(
        &self,
        data: &[u8],
        params: &CryptFilterParams,
        key: &[u8],
    ) -> FilterResult<Vec<u8>> {
        match params {
            CryptFilterParams::Identity => {
                // Identity filter - no encryption
                Ok(data.to_vec())
            }
            CryptFilterParams::V2 { name } => self.encrypt_v2(data, name, key),
            CryptFilterParams::AESV2 { name } => self.encrypt_aes_v2(data, name, key),
            CryptFilterParams::AESV3 { name } => self.encrypt_aes_v3(data, name, key),
        }
    }

    /// Encrypt data while charging the shared resource budget.
    pub fn encrypt_with_budget(
        &self,
        data: &[u8],
        params: &CryptFilterParams,
        key: &[u8],
        budget: &ResourceBudget,
    ) -> FilterResult<Vec<u8>> {
        budget
            .check()
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        budget
            .consume_input(data.len() as u64)
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        let limit = usize::try_from(
            budget
                .max_decoded_bytes_per_stream
                .min(budget.remaining_decoded_bytes()),
        )
        .unwrap_or(usize::MAX);
        let worst_case = match params {
            CryptFilterParams::AESV2 { .. } | CryptFilterParams::AESV3 { .. } => {
                data.len().saturating_add(32)
            }
            _ => data.len(),
        };
        if worst_case > limit {
            return Err(FilterError::CryptError(
                "encrypted output exceeds resource budget".to_string(),
            ));
        }
        let output = self.encrypt(data, params, key)?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| FilterError::CryptError(error.to_string()))?;
        Ok(output)
    }

    /// Decrypt using V2 standard security handler (RC4)
    fn decrypt_v2(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .decrypt(data, key, "RC4")
            .map_err(|e| FilterError::CryptError(format!("V2 decryption failed: {}", e)))
    }

    /// Encrypt using V2 standard security handler (RC4)
    fn encrypt_v2(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .encrypt(data, key, "RC4")
            .map_err(|e| FilterError::CryptError(format!("V2 encryption failed: {}", e)))
    }

    /// Decrypt using AESV2 (AES-128 in CBC mode)
    fn decrypt_aes_v2(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .decrypt(data, key, "AES-128-CBC")
            .map_err(|e| FilterError::CryptError(format!("AES-128 decryption failed: {}", e)))
    }

    /// Encrypt using AESV2 (AES-128 in CBC mode)
    fn encrypt_aes_v2(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .encrypt(data, key, "AES-128-CBC")
            .map_err(|e| FilterError::CryptError(format!("AES-128 encryption failed: {}", e)))
    }

    /// Decrypt using AESV3 (AES-256 in CBC mode)
    fn decrypt_aes_v3(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .decrypt(data, key, "AES-256-CBC")
            .map_err(|e| FilterError::CryptError(format!("AES-256 decryption failed: {}", e)))
    }

    /// Encrypt using AESV3 (AES-256 in CBC mode)
    fn encrypt_aes_v3(&self, data: &[u8], _name: &str, key: &[u8]) -> FilterResult<Vec<u8>> {
        self.crypto_provider
            .encrypt(data, key, "AES-256-CBC")
            .map_err(|e| FilterError::CryptError(format!("AES-256 encryption failed: {}", e)))
    }

    /// Derive encryption key from object number and generation
    pub fn derive_object_key(
        &self,
        base_key: &[u8],
        object_number: u32,
        generation: u16,
    ) -> Vec<u8> {
        self.derive_object_key_internal(base_key, object_number, generation, false)
    }

    /// Derive encryption key for AES-encrypted streams (adds sAlT per spec)
    pub fn derive_object_key_aes(
        &self,
        base_key: &[u8],
        object_number: u32,
        generation: u16,
    ) -> Vec<u8> {
        self.derive_object_key_internal(base_key, object_number, generation, true)
    }

    fn derive_object_key_internal(
        &self,
        base_key: &[u8],
        object_number: u32,
        generation: u16,
        aes: bool,
    ) -> Vec<u8> {
        let mut key_data = Vec::with_capacity(base_key.len() + 9);
        key_data.extend_from_slice(base_key);
        key_data.extend_from_slice(&object_number.to_le_bytes()[..3]);
        key_data.extend_from_slice(&generation.to_le_bytes());
        if aes {
            key_data.extend_from_slice(b"sAlT");
        }

        let digest = crate::crypto::encryption::md5(&key_data);
        let key_len = std::cmp::min(16, base_key.len() + 5);
        digest[..key_len].to_vec()
    }

    /// Check if filter supports a given encryption method
    pub fn supports_method(&self, method: &str) -> bool {
        matches!(method, "V2" | "AESV2" | "AESV3" | "Identity")
    }
}

impl Default for CryptFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Crypt filter manager for handling multiple encryption methods
pub struct CryptFilterManager {
    filters: std::collections::HashMap<String, CryptFilter>,
    default_filter: String,
}

impl CryptFilterManager {
    /// Create a new Crypt filter manager
    pub fn new() -> Self {
        let mut manager = Self {
            filters: std::collections::HashMap::new(),
            default_filter: "Identity".to_string(),
        };

        // Add default filters
        manager.add_filter("Identity".to_string(), CryptFilter::new());
        manager.add_filter("StdCF".to_string(), CryptFilter::new());

        manager
    }

    /// Add a custom Crypt filter
    pub fn add_filter(&mut self, name: String, filter: CryptFilter) {
        self.filters.insert(name, filter);
    }

    /// Get a Crypt filter by name
    pub fn get_filter(&self, name: &str) -> Option<&CryptFilter> {
        self.filters
            .get(name)
            .or_else(|| self.filters.get(&self.default_filter))
    }

    /// Set the default filter name
    pub fn set_default_filter(&mut self, name: String) {
        self.default_filter = name;
    }

    /// Decrypt data using a named filter
    pub fn decrypt(
        &self,
        data: &[u8],
        filter_name: &str,
        params: &CryptFilterParams,
        key: &[u8],
    ) -> FilterResult<Vec<u8>> {
        let filter = self.get_filter(filter_name).ok_or_else(|| {
            FilterError::CryptError(format!("Unknown crypt filter: {}", filter_name))
        })?;

        filter.decrypt(data, params, key)
    }

    /// Decrypt data through a named filter while charging a shared budget.
    pub fn decrypt_with_budget(
        &self,
        data: &[u8],
        filter_name: &str,
        params: &CryptFilterParams,
        key: &[u8],
        budget: &ResourceBudget,
    ) -> FilterResult<Vec<u8>> {
        let filter = self.get_filter(filter_name).ok_or_else(|| {
            FilterError::CryptError(format!("Unknown crypt filter: {}", filter_name))
        })?;

        filter.decrypt_with_budget(data, params, key, budget)
    }

    /// Encrypt data using a named filter
    pub fn encrypt(
        &self,
        data: &[u8],
        filter_name: &str,
        params: &CryptFilterParams,
        key: &[u8],
    ) -> FilterResult<Vec<u8>> {
        let filter = self.get_filter(filter_name).ok_or_else(|| {
            FilterError::CryptError(format!("Unknown crypt filter: {}", filter_name))
        })?;

        filter.encrypt(data, params, key)
    }

    /// Encrypt data through a named filter while charging a shared budget.
    pub fn encrypt_with_budget(
        &self,
        data: &[u8],
        filter_name: &str,
        params: &CryptFilterParams,
        key: &[u8],
        budget: &ResourceBudget,
    ) -> FilterResult<Vec<u8>> {
        let filter = self.get_filter(filter_name).ok_or_else(|| {
            FilterError::CryptError(format!("Unknown crypt filter: {}", filter_name))
        })?;

        filter.encrypt_with_budget(data, params, key, budget)
    }

    /// List all available filter names
    pub fn list_filters(&self) -> Vec<&String> {
        self.filters.keys().collect()
    }
}

impl Default for CryptFilterManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Encryption key cache for performance
pub struct EncryptionKeyCache {
    cache: std::collections::HashMap<(u32, u16), Vec<u8>>,
    base_key: Vec<u8>,
    max_size: usize,
}

impl EncryptionKeyCache {
    /// Create a new encryption key cache
    pub fn new(base_key: Vec<u8>) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            base_key,
            max_size: 1000, // Cache up to 1000 object keys
        }
    }

    /// Get or compute an object key
    pub fn get_object_key(&mut self, object_number: u32, generation: u16) -> &Vec<u8> {
        let key = (object_number, generation);

        if !self.cache.contains_key(&key) {
            // Evict oldest entries if cache is full
            if self.cache.len() >= self.max_size {
                self.cache.clear(); // Simple eviction - clear all
            }

            // Derive new key
            let filter = CryptFilter::new();
            let derived_key = filter.derive_object_key(&self.base_key, object_number, generation);
            self.cache.insert(key, derived_key);
        }

        &self.cache[&key]
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Update the base key
    pub fn update_base_key(&mut self, new_base_key: Vec<u8>) {
        self.base_key = new_base_key;
        self.cache.clear(); // Invalidate all cached keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::ResourceBudget;

    #[test]
    fn test_crypt_filter_creation() {
        let filter = CryptFilter::new();
        assert!(filter.supports_method("V2"));
        assert!(filter.supports_method("AESV2"));
        assert!(filter.supports_method("AESV3"));
        assert!(!filter.supports_method("Unknown"));
    }

    #[test]
    fn test_identity_filter() {
        let filter = CryptFilter::new();
        let data = b"Hello, World!";
        let params = CryptFilterParams::Identity;
        let key = b"dummy_key";

        let encrypted = filter.encrypt(data, &params, key).unwrap();
        assert_eq!(encrypted, data);

        let decrypted = filter.decrypt(&encrypted, &params, key).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn budgeted_identity_filter_charges_input_before_copying() {
        let filter = CryptFilter::new();
        let budget = ResourceBudget::new(0, 1024, 1024, 100, 10, 10, 10, 10);
        let error = filter
            .decrypt_with_budget(b"data", &CryptFilterParams::Identity, b"key", &budget)
            .expect_err("crypt input must respect the budget");

        assert!(error.to_string().contains("InputBytes"));
    }

    #[test]
    fn budgeted_aes_encryption_rejects_worst_case_output_before_provider_call() {
        let filter = CryptFilter::new();
        let budget = ResourceBudget::new(1024, 1024, 16, 100, 10, 10, 10, 10);
        let error = filter
            .encrypt_with_budget(
                b"data",
                &CryptFilterParams::AESV2 {
                    name: "StdCF".to_string(),
                },
                b"key",
                &budget,
            )
            .expect_err("AES output must respect the budget before encryption");

        assert!(error.to_string().contains("exceeds resource budget"));
    }

    #[test]
    fn test_key_derivation() {
        let filter = CryptFilter::new();
        // Use a shorter base key that will fit in the final derived key after adding object info
        let base_key = b"base_key"; // 8 bytes, so total will be 8+3+2=13 bytes (< 16)
        let object_number = 42;
        let generation = 0;

        let derived_key = filter.derive_object_key(base_key, object_number, generation);
        assert!(derived_key.len() <= 16);

        // base_key + 3 bytes object + 2 bytes generation = 13
        assert_eq!(derived_key.len(), 13);

        // Test with a key that would exceed 16 bytes to verify truncation
        let long_base_key = b"very_long_encryption_key_that_exceeds_limit"; // > 16 bytes
        let long_derived_key = filter.derive_object_key(long_base_key, object_number, generation);
        assert_eq!(long_derived_key.len(), 16); // Should be truncated to 16
    }

    #[test]
    fn test_crypt_filter_manager() {
        let manager = CryptFilterManager::new();
        assert!(manager.get_filter("Identity").is_some());
        assert!(manager.get_filter("StdCF").is_some());
        assert!(manager.get_filter("NonExistent").is_some()); // Falls back to default

        let filter_names = manager.list_filters();
        assert!(filter_names.contains(&&"Identity".to_string()));
        assert!(filter_names.contains(&&"StdCF".to_string()));
    }

    #[test]
    fn test_encryption_key_cache() {
        let base_key = b"test_base_key".to_vec();
        let mut cache = EncryptionKeyCache::new(base_key);

        let key1 = cache.get_object_key(1, 0).clone();
        let key2 = cache.get_object_key(1, 0).clone();
        assert_eq!(key1, key2); // Should be cached

        let key3 = cache.get_object_key(2, 0).clone();
        assert_ne!(key1, key3); // Different object should have different key

        cache.clear();
        // After clear, should still work
        let key4 = cache.get_object_key(1, 0).clone();
        assert_eq!(key1, key4); // Should be same as before (same derivation)
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn test_random_iv_generation() {
        let filter = CryptFilter::new();
        let params = CryptFilterParams::AESV2 {
            name: "StdCF".to_string(),
        };
        let key = [0x11u8; 16];
        let data = b"iv-generation-test";

        let mut ivs = Vec::new();
        for _ in 0..10 {
            let encrypted = filter
                .encrypt(data, &params, &key)
                .expect("AES encrypt with IV");
            assert!(encrypted.len() >= 16);
            ivs.push(encrypted[..16].to_vec());
        }

        let first = &ivs[0];
        let all_same = ivs.iter().all(|iv| iv == first);
        assert!(
            !all_same,
            "AES encryption produced identical IVs across runs"
        );
    }
}
