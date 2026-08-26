pub mod certificates;
pub mod decryption;
pub mod encryption;
pub mod pkcs7;
pub mod signature_verification;
pub mod signatures;
pub mod timestamp;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid signature format: {0}")]
    InvalidSignatureFormat(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Certificate parsing error: {0}")]
    CertificateError(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Authentication required")]
    AuthenticationRequired,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "crypto")]
    #[error("OpenSSL error: {0}")]
    OpenSsl(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;

/// Cryptographic configuration
#[derive(Debug, Clone)]
pub struct CryptoConfig {
    /// Enable signature verification
    pub enable_signature_verification: bool,

    /// Enable certificate chain validation
    pub enable_cert_chain_validation: bool,

    /// Trust store path for certificate validation
    pub trust_store_path: Option<String>,

    /// Maximum certificate chain depth
    pub max_cert_chain_depth: u32,

    /// Enable CRL checking
    pub enable_crl_checking: bool,

    /// Enable OCSP checking
    pub enable_ocsp_checking: bool,

    /// Timeout for network operations (CRL, OCSP)
    pub network_timeout_seconds: u32,

    /// Enable TSA chain validation for RFC3161 timestamps
    pub enable_tsa_chain_validation: bool,

    /// Enable TSA revocation checks (OCSP/CRL) during timestamp validation
    pub enable_tsa_revocation_checks: bool,

    /// Allowed TSA certificate fingerprints (SHA-256). Empty means allow all.
    pub tsa_allow_fingerprints: Vec<String>,

    /// Blocked TSA certificate fingerprints (SHA-256).
    pub tsa_block_fingerprints: Vec<String>,
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            enable_signature_verification: true,
            enable_cert_chain_validation: true,
            trust_store_path: None,
            max_cert_chain_depth: 5,
            enable_crl_checking: false, // Disabled by default for performance
            enable_ocsp_checking: false, // Disabled by default for performance
            network_timeout_seconds: 30,
            enable_tsa_chain_validation: false,
            enable_tsa_revocation_checks: false,
            tsa_allow_fingerprints: Vec::new(),
            tsa_block_fingerprints: Vec::new(),
        }
    }
}

/// Main cryptographic provider interface
pub trait CryptoProvider {
    /// Verify a digital signature
    fn verify_signature(
        &self,
        _signature_data: &[u8],
        _signed_data: &[u8],
        _config: &CryptoConfig,
    ) -> CryptoResult<SignatureVerificationResult>;

    /// Parse and validate a certificate
    fn parse_certificate(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo>;

    /// Verify a certificate chain
    fn verify_certificate_chain(
        &self,
        cert_chain: &[&[u8]],
        _config: &CryptoConfig,
    ) -> CryptoResult<CertificateChainResult>;

    /// Decrypt encrypted data
    fn decrypt(&self, encrypted_data: &[u8], key: &[u8], algorithm: &str) -> CryptoResult<Vec<u8>>;

    /// Encrypt data
    fn encrypt(&self, data: &[u8], key: &[u8], algorithm: &str) -> CryptoResult<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub struct SignatureVerificationResult {
    pub is_valid: bool,
    pub signer_certificate: Option<CertificateInfo>,
    pub signing_time: Option<chrono::DateTime<chrono::Utc>>,
    pub algorithm: String,
    pub error_message: Option<String>,
    pub certificate_chain: Vec<CertificateInfo>,
    pub timestamp_info: Option<TimestampInfo>,
}

#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub der: Vec<u8>,
    pub not_before: chrono::DateTime<chrono::Utc>,
    pub not_after: chrono::DateTime<chrono::Utc>,
    pub public_key_algorithm: String,
    pub signature_algorithm: String,
    pub key_usage: Vec<String>,
    pub extended_key_usage: Vec<String>,
    pub is_ca: bool,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Clone)]
pub struct CertificateChainResult {
    pub is_valid: bool,
    pub chain: Vec<CertificateInfo>,
    pub trust_anchor: Option<CertificateInfo>,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TimestampInfo {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub timestamp_authority: String,
    pub hash_algorithm: String,
    pub is_valid: bool,
    pub error_message: Option<String>,
}

/// Get the default crypto provider based on available features
pub fn get_default_crypto_provider() -> Box<dyn CryptoProvider + Send + Sync> {
    #[cfg(feature = "crypto")]
    {
        Box::new(openssl_provider::OpenSslCryptoProvider::new())
    }
    #[cfg(not(feature = "crypto"))]
    {
        Box::new(mock_provider::MockCryptoProvider::new())
    }
}

// Mock provider for when crypto features are disabled
#[cfg(not(feature = "crypto"))]
mod mock_provider {
    use super::*;

    pub struct MockCryptoProvider;

    impl MockCryptoProvider {
        pub fn new() -> Self {
            Self
        }
    }

    impl CryptoProvider for MockCryptoProvider {
        fn verify_signature(
            &self,
            __signature_data: &[u8],
            __signed_data: &[u8],
            __config: &CryptoConfig,
        ) -> CryptoResult<SignatureVerificationResult> {
            Ok(SignatureVerificationResult {
                is_valid: false,
                signer_certificate: None,
                signing_time: None,
                algorithm: "Mock".to_string(),
                error_message: Some("Cryptographic features not enabled".to_string()),
                certificate_chain: Vec::new(),
                timestamp_info: None,
            })
        }

        fn parse_certificate(&self, _cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
            Err(CryptoError::UnsupportedAlgorithm(
                "Cryptographic features not enabled".to_string(),
            ))
        }

        fn verify_certificate_chain(
            &self,
            _cert_chain: &[&[u8]],
            __config: &CryptoConfig,
        ) -> CryptoResult<CertificateChainResult> {
            Ok(CertificateChainResult {
                is_valid: false,
                chain: Vec::new(),
                trust_anchor: None,
                validation_errors: vec!["Cryptographic features not enabled".to_string()],
                validation_warnings: Vec::new(),
            })
        }

        fn decrypt(
            &self,
            _encrypted_data: &[u8],
            _key: &[u8],
            _algorithm: &str,
        ) -> CryptoResult<Vec<u8>> {
            Err(CryptoError::UnsupportedAlgorithm(
                "Cryptographic features not enabled".to_string(),
            ))
        }

        fn encrypt(&self, _data: &[u8], _key: &[u8], _algorithm: &str) -> CryptoResult<Vec<u8>> {
            Err(CryptoError::UnsupportedAlgorithm(
                "Cryptographic features not enabled".to_string(),
            ))
        }
    }
}

// OpenSSL provider implementation
#[cfg(feature = "crypto")]
mod openssl_provider {
    use super::*;

    pub struct OpenSslCryptoProvider;

    impl OpenSslCryptoProvider {
        pub fn new() -> Self {
            Self
        }
    }

    impl CryptoProvider for OpenSslCryptoProvider {
        fn verify_signature(
            &self,
            signature_data: &[u8],
            signed_data: &[u8],
            _config: &CryptoConfig,
        ) -> CryptoResult<SignatureVerificationResult> {
            let handler = crate::crypto::pkcs7::Pkcs7Handler::new();
            handler.verify_pkcs7(signature_data, signed_data)
        }

        fn parse_certificate(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
            crate::crypto::certificates::parse_der_certificate(cert_data)
        }

        fn verify_certificate_chain(
            &self,
            cert_chain: &[&[u8]],
            config: &CryptoConfig,
        ) -> CryptoResult<CertificateChainResult> {
            let validator =
                crate::crypto::certificates::CertificateChainValidator::new(config.clone())?;
            validator.validate_chain(cert_chain)
        }

        fn decrypt(
            &self,
            encrypted_data: &[u8],
            key: &[u8],
            algorithm: &str,
        ) -> CryptoResult<Vec<u8>> {
            match algorithm {
                "RC4" => Ok(crate::crypto::encryption::rc4_decrypt(encrypted_data, key)),
                "AES-128-CBC" | "AES-256-CBC" => {
                    if encrypted_data.len() < 16 {
                        return Err(CryptoError::InvalidKey(
                            "AES data too short for IV".to_string(),
                        ));
                    }
                    let iv = &encrypted_data[..16];
                    let ciphertext = &encrypted_data[16..];
                    crate::crypto::encryption::aes_decrypt_cbc(ciphertext, key, Some(iv)).map_err(
                        |e| CryptoError::EncryptionError(format!("AES decrypt error: {}", e)),
                    )
                }
                _ => Err(CryptoError::UnsupportedAlgorithm(format!(
                    "Unsupported algorithm: {}",
                    algorithm
                ))),
            }
        }

        fn encrypt(&self, data: &[u8], key: &[u8], algorithm: &str) -> CryptoResult<Vec<u8>> {
            match algorithm {
                "RC4" => Ok(crate::crypto::encryption::rc4_encrypt(data, key)),
                "AES-128-CBC" | "AES-256-CBC" => {
                    use rand::TryRngCore;
                    let mut iv = [0u8; 16];
                    let mut rng = rand::rngs::OsRng;
                    rng.try_fill_bytes(&mut iv).map_err(|e| {
                        CryptoError::EncryptionError(format!("OS randomness unavailable: {}", e))
                    })?;
                    let encrypted =
                        crate::crypto::encryption::aes_encrypt_cbc(data, key, Some(&iv)).map_err(
                            |e| CryptoError::EncryptionError(format!("AES encrypt error: {}", e)),
                        )?;
                    let mut out = Vec::with_capacity(16 + encrypted.len());
                    out.extend_from_slice(&iv);
                    out.extend_from_slice(&encrypted);
                    Ok(out)
                }
                _ => Err(CryptoError::UnsupportedAlgorithm(format!(
                    "Unsupported algorithm: {}",
                    algorithm
                ))),
            }
        }
    }
}

// Add chrono for date/time handling (would need to be added to Cargo.toml)
mod chrono {
    use std::fmt;

    #[derive(Debug, Clone, Copy)]
    pub struct DateTime<Tz> {
        timestamp: i64,
        _tz: std::marker::PhantomData<Tz>,
    }

    impl<Tz> DateTime<Tz> {
        pub fn from_timestamp(ts: i64) -> Self {
            Self {
                timestamp: ts,
                _tz: std::marker::PhantomData,
            }
        }

        pub fn timestamp(&self) -> i64 {
            self.timestamp
        }
    }

    impl<Tz> fmt::Display for DateTime<Tz> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "DateTime({})", self.timestamp)
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct Utc;

    impl Utc {
        pub fn now() -> DateTime<Utc> {
            DateTime::from_timestamp(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            )
        }
    }
}
