#![allow(dead_code)]
#![allow(clippy::items_after_test_module)]

use super::{CryptoError, CryptoResult};
use crate::types::PdfDictionary;
use std::collections::HashMap;

const PDF_PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn pad_password(password: &[u8]) -> [u8; 32] {
    let mut padded = PDF_PASSWORD_PADDING;
    let len = password.len().min(32);
    padded[..len].copy_from_slice(&password[..len]);
    padded
}

#[derive(Debug, Clone)]
pub struct CryptFilter {
    pub cfm: String,        // Crypt filter method (V2, AESV2, AESV3)
    pub auth_event: String, // DocOpen or EFOpen
    pub length: u32,        // Key length in bits
}

pub trait EncryptionHandler {
    fn decrypt(
        &self,
        encrypted_data: &[u8],
        encrypt_dict: &PdfDictionary,
        user_password: Option<&str>,
        owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>>;
    fn encrypt(
        &self,
        data: &[u8],
        encrypt_dict: &PdfDictionary,
        password: &str,
    ) -> CryptoResult<Vec<u8>>;
}

pub trait SecurityHandler {
    fn authenticate(&mut self, password: &str) -> bool;
    fn decrypt_object(
        &self,
        obj_data: &[u8],
        obj_id: u32,
        generation: u16,
    ) -> CryptoResult<Vec<u8>>;
    fn compute_object_key(&self, obj_id: u32, file_id: &[u8]) -> Vec<u8>;
    fn decrypt_string(&self, data: &str, key: &[u8]) -> CryptoResult<String>;
    fn decrypt_stream(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>>;
    fn get_permissions(&self) -> u32;
    fn is_authenticated(&self) -> bool;
}

#[allow(dead_code)]
pub struct StandardSecurityHandler {
    pub v: u32,                                      // Encryption algorithm version
    pub r: u32,                                      // Revision number
    pub length: u32,                                 // Length of encryption key in bits
    pub p: i32,                                      // Permission flags
    pub o: Vec<u8>,                                  // Owner password entry
    pub u: Vec<u8>,                                  // User password entry
    pub oe: Option<Vec<u8>>,                         // Owner encryption key (V5)
    pub ue: Option<Vec<u8>>,                         // User encryption key (V5)
    pub perms: Option<Vec<u8>>,                      // Encrypted permissions (V5)
    pub encrypt_metadata: bool,                      // Whether to encrypt metadata
    pub stream_filter: String,                       // Default crypt filter for streams
    pub string_filter: String,                       // Default crypt filter for strings
    pub crypt_filters: HashMap<String, CryptFilter>, // Named crypt filters
    file_id: Vec<u8>,
    encryption_key: Option<Vec<u8>>,
    version: i32,
    revision: i32,
    permissions: u32,
    authenticated: bool,
}

impl Default for StandardSecurityHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardSecurityHandler {
    pub fn new() -> Self {
        Self {
            v: 0,
            r: 0,
            length: 128,
            p: 0,
            o: Vec::new(),
            u: Vec::new(),
            oe: None,
            ue: None,
            perms: None,
            encrypt_metadata: true,
            stream_filter: "StdCF".to_string(),
            string_filter: "StdCF".to_string(),
            crypt_filters: HashMap::new(),
            file_id: Vec::new(),
            encryption_key: None,
            version: 0,
            revision: 0,
            permissions: 0,
            authenticated: false,
        }
    }

    pub fn new_with_params(v: u32, r: u32, length: u32, p: i32, o: Vec<u8>, u: Vec<u8>) -> Self {
        Self {
            v,
            r,
            length,
            p,
            o,
            u,
            oe: None,
            ue: None,
            perms: None,
            encrypt_metadata: true,
            stream_filter: "StdCF".to_string(),
            string_filter: "StdCF".to_string(),
            crypt_filters: HashMap::new(),
            file_id: Vec::new(),
            encryption_key: None,
            version: v as i32,
            revision: r as i32,
            permissions: p as u32,
            authenticated: false,
        }
    }

    pub fn set_file_id(&mut self, file_id: Vec<u8>) {
        self.file_id = file_id;
    }
}

impl SecurityHandler for StandardSecurityHandler {
    fn authenticate(&mut self, password: &str) -> bool {
        if !(2..=4).contains(&self.r) {
            self.authenticated = false;
            return false;
        }

        let info = EncryptionInfo {
            algorithm: EncryptionAlgorithm::RC4,
            version: self.v,
            revision: self.r,
            key_length: self.length,
            permissions: self.p as u32,
            owner_key: self.o.clone(),
            user_key: self.u.clone(),
            filter: "Standard".to_string(),
            file_id: Some(self.file_id.clone()),
        };
        let validator = PasswordValidator;
        self.authenticated = validator.validate_user_password(password, &info);
        if self.authenticated {
            self.encryption_key = PdfEncryptionHandler::new()
                .compute_key_standard(&info, Some(password), None)
                .ok();
            self.authenticated = self.encryption_key.is_some();
        }
        self.authenticated
    }

    fn decrypt_object(
        &self,
        obj_data: &[u8],
        obj_id: u32,
        generation: u16,
    ) -> CryptoResult<Vec<u8>> {
        if !self.authenticated {
            return Err(CryptoError::AuthenticationRequired);
        }

        if let Some(key) = &self.encryption_key {
            // Create object-specific key
            let mut object_key = key.clone();
            object_key.extend_from_slice(&obj_id.to_le_bytes()[..3]);
            object_key.extend_from_slice(&generation.to_le_bytes()[..2]);

            let hash_key = md5(&object_key);
            Ok(rc4_decrypt(obj_data, &hash_key))
        } else {
            Ok(obj_data.to_vec())
        }
    }

    fn compute_object_key(&self, obj_id: u32, file_id: &[u8]) -> Vec<u8> {
        let mut key = Vec::new();
        if let Some(encryption_key) = &self.encryption_key {
            key.extend_from_slice(encryption_key);
        }
        key.extend_from_slice(&obj_id.to_le_bytes()[..3]);
        key.extend_from_slice(file_id);
        md5(&key)
    }

    fn decrypt_string(&self, data: &str, key: &[u8]) -> CryptoResult<String> {
        let decrypted_bytes = rc4_decrypt(data.as_bytes(), key);
        String::from_utf8(decrypted_bytes)
            .map_err(|_| CryptoError::InvalidFormat("Invalid UTF-8 string".to_string()))
    }

    fn decrypt_stream(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        Ok(rc4_decrypt(data, key))
    }

    fn get_permissions(&self) -> u32 {
        self.permissions
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }
}

impl EncryptionHandler for PdfEncryptionHandler {
    fn decrypt(
        &self,
        encrypted_data: &[u8],
        encrypt_dict: &PdfDictionary,
        user_password: Option<&str>,
        owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>> {
        self.decrypt(encrypted_data, encrypt_dict, user_password, owner_password)
    }

    fn encrypt(
        &self,
        data: &[u8],
        encrypt_dict: &PdfDictionary,
        password: &str,
    ) -> CryptoResult<Vec<u8>> {
        self.encrypt(data, encrypt_dict, password)
    }
}

/// PDF encryption handler supporting multiple encryption algorithms
pub struct PdfEncryptionHandler;

impl Default for PdfEncryptionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfEncryptionHandler {
    pub fn new() -> Self {
        Self
    }

    /// Decrypt PDF data using the encryption dictionary
    pub fn decrypt(
        &self,
        encrypted_data: &[u8],
        encrypt_dict: &PdfDictionary,
        user_password: Option<&str>,
        owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>> {
        let encryption_info = self.parse_encryption_dict(encrypt_dict)?;

        // Compute encryption key
        let key = self.compute_encryption_key(&encryption_info, user_password, owner_password)?;

        // Decrypt based on algorithm
        match encryption_info.algorithm {
            EncryptionAlgorithm::RC4 => self.decrypt_rc4(encrypted_data, &key),
            EncryptionAlgorithm::AES128 => self.decrypt_aes(encrypted_data, &key, 128),
            EncryptionAlgorithm::AES256 => self.decrypt_aes(encrypted_data, &key, 256),
        }
    }

    /// Encrypt PDF data
    pub fn encrypt(
        &self,
        data: &[u8],
        encrypt_dict: &PdfDictionary,
        password: &str,
    ) -> CryptoResult<Vec<u8>> {
        let encryption_info = self.parse_encryption_dict(encrypt_dict)?;
        let key = self.compute_encryption_key(&encryption_info, Some(password), None)?;

        match encryption_info.algorithm {
            EncryptionAlgorithm::RC4 => self.encrypt_rc4(data, &key),
            EncryptionAlgorithm::AES128 => self.encrypt_aes(data, &key, 128),
            EncryptionAlgorithm::AES256 => self.encrypt_aes(data, &key, 256),
        }
    }

    /// Parse encryption dictionary to extract parameters
    fn parse_encryption_dict(&self, encrypt_dict: &PdfDictionary) -> CryptoResult<EncryptionInfo> {
        let filter = encrypt_dict
            .get("Filter")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash())
            .unwrap_or("Standard");

        if filter != "Standard" {
            return Err(CryptoError::UnsupportedAlgorithm(format!(
                "Unsupported encryption filter: {}",
                filter
            )));
        }

        let version = encrypt_dict
            .get("V")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u32;

        let revision = encrypt_dict
            .get("R")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u32;

        let key_length = encrypt_dict
            .get("Length")
            .and_then(|v| v.as_integer())
            .unwrap_or(40) as u32;

        let permissions = encrypt_dict
            .get("P")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u32;

        let owner_key = encrypt_dict
            .get("O")
            .and_then(|v| v.as_string())
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default();

        let user_key = encrypt_dict
            .get("U")
            .and_then(|v| v.as_string())
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default();

        let file_id = encrypt_dict
            .get("ID")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|v| v.as_string())
            .map(|s| s.as_bytes().to_vec());

        let algorithm = match (version, key_length) {
            (1..=3, _) => EncryptionAlgorithm::RC4,
            (4, 128) => EncryptionAlgorithm::AES128,
            (5, 256) => EncryptionAlgorithm::AES256,
            _ => {
                return Err(CryptoError::UnsupportedAlgorithm(format!(
                    "Unsupported encryption version/length: V={}, Length={}",
                    version, key_length
                )))
            }
        };

        Ok(EncryptionInfo {
            algorithm,
            version,
            revision,
            key_length,
            permissions,
            owner_key,
            user_key,
            filter: filter.to_string(),
            file_id,
        })
    }

    /// Compute encryption key from password and encryption parameters
    fn compute_encryption_key(
        &self,
        info: &EncryptionInfo,
        user_password: Option<&str>,
        owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>> {
        match info.revision {
            2..=4 => self.compute_key_standard(info, user_password, owner_password),
            5 => self.compute_key_aes(info, user_password, owner_password),
            _ => Err(CryptoError::UnsupportedAlgorithm(format!(
                "Unsupported revision: {}",
                info.revision
            ))),
        }
    }

    /// Compute encryption key for standard security handler (R 2-4)
    fn compute_key_standard(
        &self,
        info: &EncryptionInfo,
        user_password: Option<&str>,
        _owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>> {
        let password = user_password.unwrap_or("").as_bytes();
        let padded_password = pad_password(password);
        self.compute_key_standard_with_padded(info, &padded_password)
    }

    fn compute_key_standard_with_padded(
        &self,
        info: &EncryptionInfo,
        padded_password: &[u8; 32],
    ) -> CryptoResult<Vec<u8>> {
        // PDF key derivation algorithm (Algorithm 2 from PDF spec)
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(padded_password);
        hash_input.extend_from_slice(&info.owner_key);
        hash_input.extend_from_slice(&info.permissions.to_le_bytes());
        if let Some(file_id) = info.file_id.as_ref() {
            hash_input.extend_from_slice(file_id);
        }

        // File ID would be added here if available
        // hash_input.extend_from_slice(&file_id);

        // Compute MD5 hash
        let mut key = self.md5_hash(&hash_input);

        // For revision 3 or greater, do additional processing
        if info.revision >= 3 {
            let n = info.key_length as usize / 8;
            for _ in 0..50 {
                key = self.md5_hash(&key[..n]);
            }
        }

        // Truncate to key length
        let key_bytes = (info.key_length as usize / 8).min(16);
        Ok(key[..key_bytes].to_vec())
    }

    pub fn compute_owner_key_standard(
        &self,
        info: &EncryptionInfo,
        owner_password: &[u8],
        user_password: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        let owner_padded = pad_password(owner_password);
        let mut digest = self.md5_hash(&owner_padded);

        if info.revision >= 3 {
            for _ in 0..50 {
                digest = self.md5_hash(&digest);
            }
        }

        let key_len = (info.key_length as usize / 8).min(16);
        let key = &digest[..key_len];
        let mut data = pad_password(user_password).to_vec();

        if info.revision == 2 {
            data = self.rc4_cipher(&data, key)?;
        } else {
            for i in 0..20u8 {
                let mut k = key.to_vec();
                for b in &mut k {
                    *b ^= i;
                }
                data = self.rc4_cipher(&data, &k)?;
            }
        }

        Ok(data)
    }

    pub fn compute_user_key_standard(
        &self,
        info: &EncryptionInfo,
        user_password: &[u8],
        encryption_key: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        let padded_user = pad_password(user_password);
        if info.revision == 2 {
            return self.rc4_cipher(&padded_user, encryption_key);
        }

        let mut input = Vec::new();
        input.extend_from_slice(&PDF_PASSWORD_PADDING);
        if let Some(file_id) = info.file_id.as_ref() {
            input.extend_from_slice(file_id);
        }

        let mut hash = self.md5_hash(&input);
        hash = self.rc4_cipher(&hash, encryption_key)?;
        for i in 1..20u8 {
            let mut k = encryption_key.to_vec();
            for b in &mut k {
                *b ^= i;
            }
            hash = self.rc4_cipher(&hash, &k)?;
        }

        let mut result = Vec::with_capacity(32);
        result.extend_from_slice(&hash);
        result.resize(32, 0u8);
        Ok(result)
    }

    /// Compute encryption key for AES security handler (R 5+)
    fn compute_key_aes(
        &self,
        _info: &EncryptionInfo,
        _user_password: Option<&str>,
        _owner_password: Option<&str>,
    ) -> CryptoResult<Vec<u8>> {
        #[cfg(feature = "crypto")]
        {
            // AES key derivation using SHA-256
            let password = _user_password.unwrap_or("").as_bytes();
            self.compute_sha256_key(password, &_info.user_key, _info.key_length)
        }
        #[cfg(not(feature = "crypto"))]
        {
            Err(CryptoError::UnsupportedAlgorithm(
                "AES encryption requires crypto feature".to_string(),
            ))
        }
    }

    /// MD5 hash implementation for PDF key derivation
    fn md5_hash(&self, data: &[u8]) -> Vec<u8> {
        md5::compute(data).0.to_vec()
    }

    /// SHA-256 hash for AES key derivation
    fn sha256_hash(&self, data: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hasher.finalize().to_vec()
    }

    /// Compute SHA-256 based key for AES encryption
    fn compute_sha256_key(
        &self,
        password: &[u8],
        user_key: &[u8],
        key_length: u32,
    ) -> CryptoResult<Vec<u8>> {
        let mut key_data = Vec::new();
        key_data.extend_from_slice(password);
        key_data.extend_from_slice(user_key);

        let hash = self.sha256_hash(&key_data);
        let key_bytes = (key_length / 8) as usize;

        Ok(hash[..key_bytes.min(hash.len())].to_vec())
    }

    /// Decrypt data using RC4
    fn decrypt_rc4(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        // Always use our RC4 implementation for PDF compatibility
        self.rc4_cipher(data, key)
    }

    /// Encrypt data using RC4
    fn encrypt_rc4(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        // RC4 is symmetric, so encryption = decryption
        self.decrypt_rc4(data, key)
    }

    /// Decrypt data using AES
    fn decrypt_aes(&self, data: &[u8], key: &[u8], _key_bits: u32) -> CryptoResult<Vec<u8>> {
        if data.len() < 16 {
            return Err(CryptoError::InvalidKey(
                "AES data too short for IV".to_string(),
            ));
        }

        // First 16 bytes are the IV
        let iv = &data[..16];
        let encrypted = &data[16..];

        // Decrypt using AES-CBC
        aes_decrypt_cbc(encrypted, key, Some(iv))
            .map_err(|e| CryptoError::InvalidKey(format!("AES decrypt error: {}", e)))
    }

    /// Encrypt data using AES
    fn encrypt_aes(&self, data: &[u8], key: &[u8], _key_bits: u32) -> CryptoResult<Vec<u8>> {
        // Generate random IV
        let iv = self.generate_random_bytes(16)?;

        // Encrypt using AES-CBC
        let encrypted = aes_encrypt_cbc(data, key, Some(&iv))
            .map_err(|e| CryptoError::InvalidKey(format!("AES encrypt error: {}", e)))?;

        // Prepend IV to encrypted data
        let mut result = Vec::with_capacity(16 + encrypted.len());
        result.extend_from_slice(&iv);
        result.extend_from_slice(&encrypted);
        Ok(result)
    }

    /// Generate random bytes for IV/salt
    fn generate_random_bytes(&self, len: usize) -> CryptoResult<Vec<u8>> {
        use rand::TryRngCore;
        let mut buf = vec![0u8; len];
        let mut rng = rand::rngs::OsRng;
        rng.try_fill_bytes(&mut buf).map_err(|err| {
            CryptoError::EncryptionError(format!("OS randomness unavailable: {err}"))
        })?;
        Ok(buf)
    }

    /// RC4 cipher implementation
    fn rc4_cipher(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        if key.is_empty() {
            return Err(CryptoError::InvalidKey("Empty RC4 key".to_string()));
        }

        // Initialize S array
        let mut s = [0u8; 256];
        for (i, item) in s.iter_mut().enumerate() {
            *item = i as u8;
        }

        // Key scheduling algorithm (KSA)
        let mut j = 0;
        for i in 0..256 {
            j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
            s.swap(i, j);
        }

        // Pseudo-random generation algorithm (PRGA)
        let mut result = Vec::with_capacity(data.len());
        let mut i = 0;
        let mut j = 0;

        for &byte in data {
            i = (i + 1) % 256;
            j = (j + s[i] as usize) % 256;
            s.swap(i, j);
            let k = s[(s[i] as usize + s[j] as usize) % 256];
            result.push(byte ^ k);
        }

        Ok(result)
    }
}

/// PDF permission flags
#[derive(Debug, Clone)]
pub struct PdfPermissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub add_notes: bool,
    pub fill_forms: bool,
    pub extract_for_accessibility: bool,
    pub assemble: bool,
    pub print_high_quality: bool,
}

impl PdfPermissions {
    pub fn from_flags(flags: u32) -> Self {
        Self {
            print: (flags & 0x04) != 0,
            modify: (flags & 0x08) != 0,
            copy: (flags & 0x10) != 0,
            add_notes: (flags & 0x20) != 0,
            fill_forms: (flags & 0x100) != 0,
            extract_for_accessibility: (flags & 0x200) != 0,
            assemble: (flags & 0x400) != 0,
            print_high_quality: (flags & 0x800) != 0,
        }
    }

    pub fn to_flags(&self) -> u32 {
        let mut flags = 0u32;
        if self.print {
            flags |= 0x04;
        }
        if self.modify {
            flags |= 0x08;
        }
        if self.copy {
            flags |= 0x10;
        }
        if self.add_notes {
            flags |= 0x20;
        }
        if self.fill_forms {
            flags |= 0x100;
        }
        if self.extract_for_accessibility {
            flags |= 0x200;
        }
        if self.assemble {
            flags |= 0x400;
        }
        if self.print_high_quality {
            flags |= 0x800;
        }
        flags
    }

    pub fn is_restricted(&self) -> bool {
        !(self.print
            && self.modify
            && self.copy
            && self.add_notes
            && self.fill_forms
            && self.assemble
            && self.print_high_quality)
    }
}

#[derive(Debug, Clone)]
pub struct EncryptionInfo {
    pub algorithm: EncryptionAlgorithm,
    pub version: u32,
    pub revision: u32,
    pub key_length: u32,
    pub permissions: u32,
    pub owner_key: Vec<u8>,
    pub user_key: Vec<u8>,
    pub filter: String,
    pub file_id: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EncryptionAlgorithm {
    RC4,
    AES128,
    AES256,
}

impl EncryptionInfo {
    pub fn get_permissions(&self) -> PdfPermissions {
        PdfPermissions::from_flags(self.permissions)
    }

    pub fn is_encrypted(&self) -> bool {
        true // If we have EncryptionInfo, document is encrypted
    }

    pub fn supports_algorithm(&self, algorithm: &str) -> bool {
        matches!(
            (algorithm, &self.algorithm),
            ("RC4", EncryptionAlgorithm::RC4)
                | ("AES-128", EncryptionAlgorithm::AES128)
                | ("AES-256", EncryptionAlgorithm::AES256)
        )
    }
}

/// Password validator for PDF encryption
pub struct PasswordValidator;

impl PasswordValidator {
    /// Validate user password against encryption parameters
    pub fn validate_user_password(&self, password: &str, _encrypt_info: &EncryptionInfo) -> bool {
        if !(2..=4).contains(&_encrypt_info.revision) {
            return false;
        }

        let handler = PdfEncryptionHandler::new();
        let key = match handler.compute_key_standard(_encrypt_info, Some(password), None) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let expected =
            match handler.compute_user_key_standard(_encrypt_info, password.as_bytes(), &key) {
                Ok(u) => u,
                Err(_) => return false,
            };

        if _encrypt_info.revision >= 3 {
            _encrypt_info.user_key.len() >= 16
                && expected.len() >= 16
                && _encrypt_info.user_key[..16] == expected[..16]
        } else {
            _encrypt_info.user_key == expected
        }
    }

    /// Validate owner password against encryption parameters
    pub fn validate_owner_password(&self, password: &str, _encrypt_info: &EncryptionInfo) -> bool {
        if !(2..=4).contains(&_encrypt_info.revision) {
            return false;
        }

        {
            let handler = PdfEncryptionHandler::new();
            let owner_padded = pad_password(password.as_bytes());
            let mut digest = handler.md5_hash(&owner_padded);

            if _encrypt_info.revision >= 3 {
                for _ in 0..50 {
                    digest = handler.md5_hash(&digest);
                }
            }

            let key_len = (_encrypt_info.key_length as usize / 8).min(16);
            let key = &digest[..key_len];
            let mut data = _encrypt_info.owner_key.clone();

            if _encrypt_info.revision == 2 {
                data = match handler.rc4_cipher(&data, key) {
                    Ok(out) => out,
                    Err(_) => return false,
                };
            } else {
                for i in (0..20u8).rev() {
                    let mut k = key.to_vec();
                    for b in &mut k {
                        *b ^= i;
                    }
                    data = match handler.rc4_cipher(&data, &k) {
                        Ok(out) => out,
                        Err(_) => return false,
                    };
                }
            }

            if data.len() < 32 {
                return false;
            }
            let padded: [u8; 32] = match data[..32].try_into() {
                Ok(p) => p,
                Err(_) => return false,
            };
            let key = match handler.compute_key_standard_with_padded(_encrypt_info, &padded) {
                Ok(k) => k,
                Err(_) => return false,
            };
            let expected = match handler.compute_user_key_standard(_encrypt_info, &data[..32], &key)
            {
                Ok(u) => u,
                Err(_) => return false,
            };

            if _encrypt_info.revision >= 3 {
                _encrypt_info.user_key.len() >= 16
                    && expected.len() >= 16
                    && _encrypt_info.user_key[..16] == expected[..16]
            } else {
                _encrypt_info.user_key == expected
            }
        }
    }

    /// Check if document can be opened without password (empty user password)
    pub fn can_open_without_password(&self, encrypt_info: &EncryptionInfo) -> bool {
        self.validate_user_password("", encrypt_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PdfName, PdfString};
    use crate::PdfValue;

    #[test]
    fn test_permissions_flags() {
        let permissions = PdfPermissions {
            print: true,
            modify: false,
            copy: true,
            add_notes: false,
            fill_forms: true,
            extract_for_accessibility: true,
            assemble: false,
            print_high_quality: true,
        };

        let flags = permissions.to_flags();
        let restored = PdfPermissions::from_flags(flags);

        assert_eq!(permissions.print, restored.print);
        assert_eq!(permissions.modify, restored.modify);
        assert_eq!(permissions.copy, restored.copy);
        assert_eq!(permissions.fill_forms, restored.fill_forms);
    }

    #[test]
    fn test_encryption_info_parsing() {
        let handler = PdfEncryptionHandler::new();
        let mut encrypt_dict = PdfDictionary::new();

        encrypt_dict.insert("Filter", PdfValue::Name(PdfName::new("Standard")));
        encrypt_dict.insert("V", PdfValue::Integer(1));
        encrypt_dict.insert("R", PdfValue::Integer(2));
        encrypt_dict.insert("Length", PdfValue::Integer(40));
        encrypt_dict.insert("P", PdfValue::Integer(-4));
        encrypt_dict.insert(
            "O",
            PdfValue::String(PdfString::new_literal(b"owner_key_data")),
        );
        encrypt_dict.insert(
            "U",
            PdfValue::String(PdfString::new_literal(b"user_key_data")),
        );

        let result = handler.parse_encryption_dict(&encrypt_dict);
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.algorithm, EncryptionAlgorithm::RC4);
        assert_eq!(info.version, 1);
        assert_eq!(info.revision, 2);
        assert_eq!(info.key_length, 40);
    }

    #[cfg(not(feature = "crypto"))]
    #[test]
    fn test_simple_rc4() {
        let key = b"key";
        let data = b"Hello, World!";

        let encrypted = rc4_encrypt(data, key);
        let decrypted = rc4_encrypt(&encrypted, key);

        assert_eq!(data, &decrypted[..]);
    }

    #[test]
    fn test_password_validator() {
        let _validator = PasswordValidator;
        let _encrypt_info = EncryptionInfo {
            algorithm: EncryptionAlgorithm::RC4,
            version: 1,
            revision: 2,
            key_length: 40,
            permissions: 0xFFFFFFFC,
            owner_key: vec![0; 32],
            user_key: vec![0; 32],
            filter: "Standard".to_string(),
            file_id: None,
        };

        // Test empty password (should work for this test)
        #[cfg(not(feature = "crypto"))]
        {
            assert!(!_validator.validate_user_password("test", &_encrypt_info));
            assert!(!_validator.validate_user_password("", &_encrypt_info));
        }
    }

    #[test]
    fn password_validator_accepts_only_derived_user_key() {
        let handler = PdfEncryptionHandler::new();
        let mut info = EncryptionInfo {
            algorithm: EncryptionAlgorithm::RC4,
            version: 1,
            revision: 2,
            key_length: 40,
            permissions: 0xFFFFFFFC,
            owner_key: Vec::new(),
            user_key: Vec::new(),
            filter: "Standard".to_string(),
            file_id: Some(b"file-id".to_vec()),
        };
        info.owner_key = handler
            .compute_owner_key_standard(&info, b"owner", b"user")
            .unwrap();
        let key = handler
            .compute_key_standard(&info, Some("user"), None)
            .unwrap();
        info.user_key = handler
            .compute_user_key_standard(&info, b"user", &key)
            .unwrap();

        let validator = PasswordValidator;
        assert!(validator.validate_user_password("user", &info));
        assert!(!validator.validate_user_password("wrong", &info));
    }
}

/// Public hash functions
pub fn md5(data: &[u8]) -> Vec<u8> {
    md5::compute(data).0.to_vec()
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// RC4 encryption/decryption (self-inverse)
pub fn rc4_decrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    rc4_encrypt(data, key)
}

pub fn rc4_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut s = [0u8; 256];
    for (i, item) in s.iter_mut().enumerate() {
        *item = i as u8;
    }

    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }

    let mut result = Vec::with_capacity(data.len());
    let mut i = 0u8;
    let mut j = 0u8;

    for byte in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        result.push(byte ^ k);
    }

    result
}

/// AES CBC mode decryption
pub fn aes_decrypt_cbc(data: &[u8], key: &[u8], iv: Option<&[u8]>) -> Result<Vec<u8>, String> {
    #[cfg(feature = "crypto")]
    {
        use openssl::symm::{Cipher, Crypter, Mode};
        let cipher = match key.len() {
            16 => Cipher::aes_128_cbc(),
            32 => Cipher::aes_256_cbc(),
            _ => return Err(format!("AES key must be 16 or 32 bytes, got {}", key.len())),
        };
        let default_iv = [0u8; 16];
        let iv = iv.unwrap_or(&default_iv);
        if iv.len() != 16 {
            return Err("AES requires 16-byte IV".to_string());
        }
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, key, Some(iv))
            .map_err(|e| format!("AES cipher error: {}", e))?;
        crypter.pad(true);
        let mut out = vec![0u8; data.len() + cipher.block_size()];
        let mut count = crypter
            .update(data, &mut out)
            .map_err(|e| format!("AES decrypt update error: {}", e))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| format!("AES decrypt finalize error: {}", e))?;
        out.truncate(count);
        Ok(out)
    }
    #[cfg(not(feature = "crypto"))]
    {
        let _ = (data, key, iv);
        Err("AES decryption requires crypto feature".to_string())
    }
}

/// AES CBC mode encryption
pub fn aes_encrypt_cbc(data: &[u8], key: &[u8], iv: Option<&[u8]>) -> Result<Vec<u8>, String> {
    #[cfg(feature = "crypto")]
    {
        use openssl::symm::{Cipher, Crypter, Mode};
        let cipher = match key.len() {
            16 => Cipher::aes_128_cbc(),
            32 => Cipher::aes_256_cbc(),
            _ => return Err(format!("AES key must be 16 or 32 bytes, got {}", key.len())),
        };
        let default_iv = [0u8; 16];
        let iv = iv.unwrap_or(&default_iv);
        if iv.len() != 16 {
            return Err("AES requires 16-byte IV".to_string());
        }
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(iv))
            .map_err(|e| format!("AES cipher error: {}", e))?;
        crypter.pad(true);
        let mut out = vec![0u8; data.len() + cipher.block_size()];
        let mut count = crypter
            .update(data, &mut out)
            .map_err(|e| format!("AES encrypt update error: {}", e))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| format!("AES encrypt finalize error: {}", e))?;
        out.truncate(count);
        Ok(out)
    }
    #[cfg(not(feature = "crypto"))]
    {
        let _ = (data, key, iv);
        Err("AES encryption requires crypto feature".to_string())
    }
}
