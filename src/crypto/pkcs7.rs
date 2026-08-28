use super::{
    CertificateInfo, CryptoError, CryptoResult, SignatureVerificationResult, TimestampInfo,
};
use crate::crypto::timestamp;

#[cfg(feature = "crypto")]
use openssl::{
    pkcs7::Pkcs7,
    pkey::PKey,
    x509::{X509Ref, X509},
};

#[cfg(feature = "crypto")]
use x509_parser::prelude::*;

#[cfg(not(feature = "crypto"))]
use sha1::Sha1;
#[cfg(not(feature = "crypto"))]
use sha2::{Digest, Sha256, Sha384, Sha512};

/// PKCS#7 / CMS (Cryptographic Message Syntax) handler
pub struct Pkcs7Handler;
pub type PKCS7Parser = Pkcs7Handler;

pub struct SignedData {
    pub content_info: ContentInfo,
    pub certificates: Vec<Vec<u8>>,
    pub crls: Vec<Vec<u8>>,
    pub signers: Vec<SignerInfo>,
}

#[allow(dead_code)]
impl Default for Pkcs7Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl Pkcs7Handler {
    pub fn new() -> Self {
        Self
    }

    /// Parse and verify a PKCS#7 signature
    pub fn verify_pkcs7(
        &self,
        pkcs7_data: &[u8],
        signed_data: &[u8],
    ) -> CryptoResult<SignatureVerificationResult> {
        // Parse the PKCS#7 structure
        let pkcs7_info = self.parse_pkcs7_structure(pkcs7_data)?;

        // Verify the signature
        #[cfg(feature = "crypto")]
        {
            self.verify_with_openssl(&pkcs7_info, signed_data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            self.verify_simple(&pkcs7_info, signed_data)
        }
    }

    /// Parse PKCS#7 ASN.1 structure
    fn parse_pkcs7_structure(&self, data: &[u8]) -> CryptoResult<Pkcs7Info> {
        if data.len() < 20 {
            return Err(CryptoError::InvalidSignatureFormat(
                "PKCS#7 data too short".to_string(),
            ));
        }

        // Check for ASN.1 SEQUENCE tag
        if data[0] != 0x30 {
            return Err(CryptoError::InvalidSignatureFormat(
                "Invalid PKCS#7 ASN.1 structure".to_string(),
            ));
        }

        // Parse length
        let (content_start, total_length) = self.parse_asn1_length(&data[1..])?;

        if data.len() < content_start + 1 + total_length {
            return Err(CryptoError::InvalidSignatureFormat(
                "PKCS#7 data truncated".to_string(),
            ));
        }

        // Look for ContentInfo OID (1.2.840.113549.1.7.2 for SignedData)
        let signed_data_oid = &[
            0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02,
        ];

        if !self.contains_sequence(data, signed_data_oid) {
            return Err(CryptoError::InvalidSignatureFormat(
                "Not a SignedData PKCS#7 structure".to_string(),
            ));
        }

        // Extract basic information (simplified parsing)
        let pkcs7_info = Pkcs7Info {
            version: self.extract_version(data)?,
            digest_algorithms: self.extract_digest_algorithms(data)?,
            content_info: self.extract_content_info(data)?,
            certificates: self.extract_certificates(data)?,
            signer_infos: self.extract_signer_infos(data)?,
            raw_data: data.to_vec(),
        };

        Ok(pkcs7_info)
    }

    /// Parse ASN.1 length field
    fn parse_asn1_length(&self, data: &[u8]) -> CryptoResult<(usize, usize)> {
        if data.is_empty() {
            return Err(CryptoError::InvalidSignatureFormat(
                "Empty ASN.1 length field".to_string(),
            ));
        }

        let first_byte = data[0];

        if first_byte & 0x80 == 0 {
            // Short form
            Ok((1, first_byte as usize))
        } else {
            // Long form
            let length_octets = (first_byte & 0x7F) as usize;
            if length_octets == 0 || length_octets > 4 || data.len() < 1 + length_octets {
                return Err(CryptoError::InvalidSignatureFormat(
                    "Invalid ASN.1 long form length".to_string(),
                ));
            }

            let mut length = 0usize;
            for i in 0..length_octets {
                length = (length << 8) | data[1 + i] as usize;
            }

            Ok((1 + length_octets, length))
        }
    }

    /// Check if data contains a specific byte sequence
    fn contains_sequence(&self, data: &[u8], sequence: &[u8]) -> bool {
        data.windows(sequence.len())
            .any(|window| window == sequence)
    }

    /// Extract the SignedData version from PKCS#7 data.
    fn extract_version(&self, data: &[u8]) -> CryptoResult<u32> {
        // Look for INTEGER tag (0x02) near the beginning
        for i in 0..data.len().min(50) {
            if data[i] == 0x02 && i + 2 < data.len() && data[i + 1] == 0x01 {
                return Ok(data[i + 2] as u32);
            }
        }
        Err(CryptoError::InvalidSignatureFormat(
            "SignedData version INTEGER not found".to_string(),
        ))
    }

    /// Extract digest algorithms (simplified)
    fn extract_digest_algorithms(&self, data: &[u8]) -> CryptoResult<Vec<String>> {
        #[cfg(feature = "crypto")]
        {
            self.extract_digest_algorithms_asn1(data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = data;
            Err(CryptoError::UnsupportedAlgorithm(
                "PKCS#7 algorithm inspection requires the crypto feature".to_string(),
            ))
        }
    }

    /// Extract digest algorithms using ASN.1 parsing
    #[cfg(feature = "crypto")]
    fn extract_digest_algorithms_asn1(&self, data: &[u8]) -> CryptoResult<Vec<String>> {
        use der_parser::der::*;

        let mut algorithms = Vec::new();

        // Parse the outer PKCS#7 ContentInfo structure
        match parse_der(data) {
            Ok((_remaining, obj)) => {
                if let DerObjectContent::Sequence(seq) = obj.content {
                    // Look for digest algorithms in the SignedData structure
                    for item in seq {
                        if let DerObjectContent::Set(set) = item.content {
                            // This could be the digestAlgorithms set
                            for alg_item in set {
                                if let DerObjectContent::Sequence(alg_seq) = alg_item.content {
                                    if let Some(first) = alg_seq.first() {
                                        if let DerObjectContent::OID(oid) = &first.content {
                                            let algorithm_name = match oid.to_string().as_str() {
                                                "1.3.14.3.2.26" => "SHA-1".to_string(),
                                                "2.16.840.1.101.3.4.2.1" => "SHA-256".to_string(),
                                                "2.16.840.1.101.3.4.2.2" => "SHA-384".to_string(),
                                                "2.16.840.1.101.3.4.2.3" => "SHA-512".to_string(),
                                                _ => format!("Unknown ({})", oid),
                                            };
                                            algorithms.push(algorithm_name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(err) => {
                return Err(CryptoError::InvalidSignatureFormat(format!(
                    "DER parse error: {}",
                    err
                )))
            }
        }

        if algorithms.is_empty() {
            return Err(CryptoError::InvalidSignatureFormat(
                "No digest algorithm found".to_string(),
            ));
        }

        Ok(algorithms)
    }

    /// Extract content info when a complete ASN.1 implementation is available.
    fn extract_content_info(&self, _data: &[u8]) -> CryptoResult<ContentInfo> {
        Err(CryptoError::UnsupportedAlgorithm(
            "PKCS#7 content-info extraction is not implemented".to_string(),
        ))
    }

    /// Extract certificates from PKCS#7 data
    fn extract_certificates(&self, data: &[u8]) -> CryptoResult<Vec<Vec<u8>>> {
        // Look for certificate tag (0xA0) in the PKCS#7 structure
        let mut certificates = Vec::new();

        let mut i = 0;
        while i < data.len().saturating_sub(10) {
            if data[i] == 0xA0 {
                // Found certificates section
                let (len_start, certs_length) = self.parse_asn1_length(&data[i + 1..])?;
                let certs_start = i + 1 + len_start;
                let certs_end = certs_start + certs_length;

                if certs_end <= data.len() {
                    // Extract individual certificates
                    let certs_data = &data[certs_start..certs_end];
                    let extracted = self.extract_individual_certificates(certs_data)?;
                    certificates.extend(extracted);
                }
                break;
            }
            i += 1;
        }

        Ok(certificates)
    }

    /// Extract individual certificates from certificate section
    fn extract_individual_certificates(&self, certs_data: &[u8]) -> CryptoResult<Vec<Vec<u8>>> {
        let mut certificates = Vec::new();
        let mut i = 0;

        while i < certs_data.len().saturating_sub(10) {
            if certs_data[i] == 0x30 && certs_data[i + 1] == 0x82 {
                // Found certificate (DER-encoded)
                let cert_length =
                    ((certs_data[i + 2] as usize) << 8) | (certs_data[i + 3] as usize);
                let total_length = cert_length + 4;

                if i + total_length <= certs_data.len() {
                    certificates.push(certs_data[i..i + total_length].to_vec());
                    i += total_length;
                } else {
                    break;
                }
            } else {
                i += 1;
            }
        }

        Ok(certificates)
    }

    /// Extract signer infos (simplified)
    fn extract_signer_infos(&self, data: &[u8]) -> CryptoResult<Vec<SignerInfo>> {
        #[cfg(feature = "crypto")]
        {
            self.extract_signer_infos_asn1(data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = data;
            Err(CryptoError::UnsupportedAlgorithm(
                "PKCS#7 signer inspection requires the crypto feature".to_string(),
            ))
        }
    }

    /// Extract signer infos using ASN.1 parsing
    #[cfg(feature = "crypto")]
    fn extract_signer_infos_asn1(&self, _data: &[u8]) -> CryptoResult<Vec<SignerInfo>> {
        Err(CryptoError::UnsupportedAlgorithm(
            "PKCS#7 signer-info extraction is not implemented".to_string(),
        ))
    }

    /// Verify PKCS#7 signature using OpenSSL
    #[cfg(feature = "crypto")]
    fn verify_with_openssl(
        &self,
        pkcs7_info: &Pkcs7Info,
        signed_data: &[u8],
    ) -> CryptoResult<SignatureVerificationResult> {
        use openssl::pkcs7::Pkcs7Flags;
        use openssl::stack::Stack;
        use openssl::x509::store::X509StoreBuilder;

        let pkcs7 = match Pkcs7::from_der(&pkcs7_info.raw_data) {
            Ok(p7) => p7,
            Err(e) => {
                return Ok(SignatureVerificationResult {
                    is_valid: false,
                    signer_certificate: None,
                    signing_time: None,
                    algorithm: "PKCS#7".to_string(),
                    error_message: Some(format!("Failed to parse PKCS#7: {}", e)),
                    certificate_chain: Vec::new(),
                    timestamp_info: None,
                });
            }
        };

        let empty_certs = Stack::new().unwrap();
        let certs = pkcs7
            .signed()
            .and_then(|s| s.certificates())
            .unwrap_or(&empty_certs);

        let mut cert_chain = Vec::new();
        for cert in certs.iter() {
            if let Ok(info) = self.parse_x509_certificate_openssl(cert) {
                cert_chain.push(info);
            }
        }

        let mut store_builder = X509StoreBuilder::new()
            .map_err(|e| CryptoError::OpenSsl(format!("Failed to create store builder: {}", e)))?;
        store_builder.set_default_paths().map_err(|e| {
            CryptoError::OpenSsl(format!("Failed to load system trust store: {}", e))
        })?;
        let store = store_builder.build();

        let mut output = Vec::new();
        let verify_result = pkcs7.verify(
            certs,
            &store,
            if signed_data.is_empty() {
                None
            } else {
                Some(signed_data)
            },
            Some(&mut output),
            Pkcs7Flags::empty(),
        );

        let is_valid = verify_result.is_ok();
        let error_message = verify_result
            .err()
            .map(|e| format!("PKCS#7 verification failed: {}", e));

        let signer_cert_info = pkcs7
            .signers(certs, Pkcs7Flags::empty())
            .ok()
            .and_then(|stack| {
                let cert = stack.iter().next()?;
                self.parse_x509_certificate_openssl(cert).ok()
            });

        let signing_time = self.extract_signing_time(pkcs7_info);
        let timestamp_info = self.extract_timestamp_info(pkcs7_info);

        Ok(SignatureVerificationResult {
            is_valid,
            signer_certificate: signer_cert_info,
            signing_time,
            algorithm: "PKCS#7".to_string(),
            error_message,
            certificate_chain: cert_chain,
            timestamp_info,
        })
    }

    /// Simple PKCS#7 verification without crypto libraries
    #[cfg(not(feature = "crypto"))]
    fn verify_simple(
        &self,
        pkcs7_info: &Pkcs7Info,
        signed_data: &[u8],
    ) -> CryptoResult<SignatureVerificationResult> {
        // Extract signer certificate
        let signer_cert = if !pkcs7_info.certificates.is_empty() {
            self.parse_certificate_simple(&pkcs7_info.certificates[0])
                .ok()
        } else {
            None
        };

        // Create basic verification result
        Ok(SignatureVerificationResult {
            is_valid: false,
            signer_certificate: signer_cert,
            signing_time: self.extract_signing_time(pkcs7_info),
            algorithm: "PKCS#7".to_string(),
            error_message: Some("Full PKCS#7 verification requires crypto features".to_string()),
            certificate_chain: self.parse_certificate_chain(&pkcs7_info.certificates),
            timestamp_info: self.extract_timestamp_info(pkcs7_info),
        })
    }

    /// Parse X509 certificate using OpenSSL
    #[cfg(feature = "crypto")]
    fn parse_x509_certificate_openssl(&self, cert: &X509Ref) -> CryptoResult<CertificateInfo> {
        let der = cert
            .to_der()
            .map_err(|e| CryptoError::CertificateError(format!("X509 DER encode error: {}", e)))?;

        let (_, parsed) = X509Certificate::from_der(&der).map_err(|e| {
            CryptoError::CertificateError(format!("Failed to parse certificate: {}", e))
        })?;

        let subject = parsed.subject().to_string();
        let issuer = parsed.issuer().to_string();
        let serial_number = parsed.tbs_certificate.raw_serial_as_string();

        let not_before = self
            .asn1_time_to_chrono_x509(&parsed.validity().not_before)
            .unwrap_or_else(super::chrono::Utc::now);
        let not_after = self
            .asn1_time_to_chrono_x509(&parsed.validity().not_after)
            .unwrap_or_else(super::chrono::Utc::now);

        let public_key_algorithm = parsed.public_key().algorithm.algorithm.to_string();
        let signature_algorithm = parsed.signature_algorithm.algorithm.to_string();

        let fingerprint_sha256 = self
            .compute_sha256(&der)
            .map(|hash| hash.iter().map(|byte| format!("{:02x}", byte)).collect())
            .unwrap_or_else(|_| "unknown".to_string());

        let key_usage = parse_key_usage_from_x509(&parsed);
        let extended_key_usage = parse_extended_key_usage_from_x509(&parsed);
        let is_ca = parse_is_ca_from_x509(&parsed);

        Ok(CertificateInfo {
            subject,
            issuer,
            serial_number,
            der,
            not_before,
            not_after,
            public_key_algorithm,
            signature_algorithm,
            key_usage,
            extended_key_usage,
            is_ca,
            fingerprint_sha256,
        })
    }

    /// Parse certificate using simple method
    #[allow(dead_code)]
    fn parse_certificate_simple(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
        #[cfg(feature = "crypto")]
        {
            self.parse_certificate_with_x509_parser(cert_data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            self.parse_certificate_basic(cert_data)
        }
    }

    /// Parse certificate using x509-parser library
    #[cfg(feature = "crypto")]
    #[allow(dead_code)]
    fn parse_certificate_with_x509_parser(
        &self,
        cert_data: &[u8],
    ) -> CryptoResult<CertificateInfo> {
        // Parse the certificate using x509-parser
        let (_, cert) = X509Certificate::from_der(cert_data).map_err(|e| {
            CryptoError::CertificateError(format!("Failed to parse certificate: {}", e))
        })?;

        let subject = cert.subject().to_string();
        let issuer = cert.issuer().to_string();
        let serial_number = cert.tbs_certificate.raw_serial_as_string();

        let not_before = self
            .asn1_time_to_chrono_x509(&cert.validity().not_before)
            .unwrap_or_else(super::chrono::Utc::now);
        let not_after = self
            .asn1_time_to_chrono_x509(&cert.validity().not_after)
            .unwrap_or_else(super::chrono::Utc::now);

        let public_key_algorithm = cert.public_key().algorithm.algorithm.to_string();
        let signature_algorithm = cert.signature_algorithm.algorithm.to_string();

        let fingerprint_sha256 = self
            .compute_sha256(cert_data)
            .map(|hash| hash.iter().map(|byte| format!("{:02x}", byte)).collect())
            .unwrap_or_else(|_| "unknown".to_string());

        let key_usage = parse_key_usage_from_x509(&cert);
        let extended_key_usage = parse_extended_key_usage_from_x509(&cert);
        let is_ca = parse_is_ca_from_x509(&cert);

        Ok(CertificateInfo {
            subject,
            issuer,
            serial_number,
            der: cert_data.to_vec(),
            not_before,
            not_after,
            public_key_algorithm,
            signature_algorithm,
            key_usage,
            extended_key_usage,
            is_ca,
            fingerprint_sha256,
        })
    }

    /// Convert x509-parser ASN.1 Time to chrono DateTime
    #[cfg(feature = "crypto")]
    #[allow(dead_code)]
    fn asn1_time_to_chrono_x509(
        &self,
        _asn1_time: &ASN1Time,
    ) -> Option<super::chrono::DateTime<super::chrono::Utc>> {
        let time = _asn1_time.to_datetime();
        Some(super::chrono::DateTime::from_timestamp(
            time.unix_timestamp(),
        ))
    }

    /// Basic certificate parsing without crypto libraries
    #[cfg(not(feature = "crypto"))]
    fn parse_certificate_basic(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
        let _ = cert_data;
        Err(CryptoError::UnsupportedAlgorithm(
            "X.509 certificate inspection requires the crypto feature".to_string(),
        ))
    }

    /// Parse certificate chain
    #[allow(dead_code)]
    fn parse_certificate_chain(&self, cert_data_list: &[Vec<u8>]) -> Vec<CertificateInfo> {
        cert_data_list
            .iter()
            .filter_map(|cert_data| self.parse_certificate_simple(cert_data).ok())
            .collect()
    }

    /// Extract signing time from PKCS#7 authenticated attributes
    fn extract_signing_time(
        &self,
        pkcs7_info: &Pkcs7Info,
    ) -> Option<super::chrono::DateTime<super::chrono::Utc>> {
        // Look for signing time in authenticated attributes
        for signer_info in &pkcs7_info.signer_infos {
            for attr in &signer_info.authenticated_attributes {
                if attr.oid == "1.2.840.113549.1.9.5" {
                    // signing-time OID
                    // The attribute is present, but its ASN.1 time value is not
                    // decoded by this parser yet.
                    return None;
                }
            }
        }
        None
    }

    /// Extract timestamp info from PKCS#7 unauthenticated attributes
    fn extract_timestamp_info(&self, pkcs7_info: &Pkcs7Info) -> Option<TimestampInfo> {
        // Look for timestamp in unauthenticated attributes
        for signer_info in &pkcs7_info.signer_infos {
            for attr in &signer_info.unauthenticated_attributes {
                if attr.oid == "1.2.840.113549.1.9.16.2.14" {
                    // timestamp token OID
                    if let Some(value) = attr.values.first() {
                        if let Ok(parsed) = timestamp::parse_timestamp_token(value) {
                            let hash_algorithm = parsed
                                .hash_algorithm
                                .unwrap_or_else(|| "Unknown".to_string());
                            let timestamp_authority = parsed
                                .tsa_certificate_der
                                .as_deref()
                                .and_then(Self::extract_cert_subject)
                                .or(parsed.policy_oid)
                                .unwrap_or_else(|| "Unknown TSA".to_string());
                            let error_message = if parsed.message_imprint.is_some() {
                                Some(
                                    "Timestamp parsed; digest verification requires document bytes"
                                        .to_string(),
                                )
                            } else {
                                Some("Timestamp missing message imprint".to_string())
                            };
                            return Some(TimestampInfo {
                                timestamp: parsed.time,
                                timestamp_authority,
                                hash_algorithm,
                                is_valid: false,
                                error_message,
                            });
                        }
                    }
                }
            }
        }
        None
    }

    #[cfg(feature = "crypto")]
    fn extract_cert_subject(cert_der: &[u8]) -> Option<String> {
        use openssl::x509::X509;

        let cert = X509::from_der(cert_der).ok()?;
        let subject = cert.subject_name();
        subject
            .entries()
            .next()
            .and_then(|entry| entry.data().to_string().ok())
    }

    #[cfg(not(feature = "crypto"))]
    fn extract_cert_subject(_cert_der: &[u8]) -> Option<String> {
        None
    }

    /// Extract digest from signed data
    pub fn compute_digest(&self, data: &[u8], algorithm: &str) -> CryptoResult<Vec<u8>> {
        match algorithm {
            "SHA-1" => self.compute_sha1(data),
            "SHA-256" => self.compute_sha256(data),
            "SHA-384" => self.compute_sha384(data),
            "SHA-512" => self.compute_sha512(data),
            _ => Err(CryptoError::UnsupportedAlgorithm(format!(
                "Unsupported digest algorithm: {}",
                algorithm
            ))),
        }
    }

    #[cfg(feature = "crypto")]
    fn compute_sha1(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        use openssl::hash::{hash, MessageDigest};

        hash(MessageDigest::sha1(), data)
            .map(|digest| digest.to_vec())
            .map_err(|e| {
                CryptoError::UnsupportedAlgorithm(format!("SHA-1 computation failed: {}", e))
            })
    }

    #[cfg(feature = "crypto")]
    fn compute_sha256(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        use openssl::hash::{hash, MessageDigest};

        hash(MessageDigest::sha256(), data)
            .map(|digest| digest.to_vec())
            .map_err(|e| {
                CryptoError::UnsupportedAlgorithm(format!("SHA-256 computation failed: {}", e))
            })
    }

    #[cfg(feature = "crypto")]
    fn compute_sha384(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        use openssl::hash::{hash, MessageDigest};

        hash(MessageDigest::sha384(), data)
            .map(|digest| digest.to_vec())
            .map_err(|e| {
                CryptoError::UnsupportedAlgorithm(format!("SHA-384 computation failed: {}", e))
            })
    }

    #[cfg(feature = "crypto")]
    fn compute_sha512(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        use openssl::hash::{hash, MessageDigest};

        hash(MessageDigest::sha512(), data)
            .map(|digest| digest.to_vec())
            .map_err(|e| {
                CryptoError::UnsupportedAlgorithm(format!("SHA-512 computation failed: {}", e))
            })
    }

    #[cfg(not(feature = "crypto"))]
    fn compute_sha1(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut hasher = Sha1::new();
        hasher.update(data);
        Ok(hasher.finalize().to_vec())
    }

    #[cfg(not(feature = "crypto"))]
    fn compute_sha256(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(hasher.finalize().to_vec())
    }

    #[cfg(not(feature = "crypto"))]
    fn compute_sha384(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut hasher = Sha384::new();
        hasher.update(data);
        Ok(hasher.finalize().to_vec())
    }

    #[cfg(not(feature = "crypto"))]
    fn compute_sha512(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut hasher = Sha512::new();
        hasher.update(data);
        Ok(hasher.finalize().to_vec())
    }

    /// Parse signed data from PKCS#7 contents
    pub fn parse_signed_data(&self, contents: &[u8]) -> CryptoResult<SignedData> {
        let _ = contents;
        Err(CryptoError::UnsupportedAlgorithm(
            "SignedData parsing is not implemented".to_string(),
        ))
    }
}

#[cfg(feature = "crypto")]
fn parse_key_usage_from_x509(cert: &X509Certificate) -> Vec<String> {
    use x509_parser::extensions::ParsedExtension;
    for ext in cert.extensions() {
        if let ParsedExtension::KeyUsage(ku) = &ext.parsed_extension() {
            let mut out = Vec::new();
            if ku.digital_signature() {
                out.push("Digital Signature".to_string());
            }
            if ku.non_repudiation() {
                out.push("Non Repudiation".to_string());
            }
            if ku.key_encipherment() {
                out.push("Key Encipherment".to_string());
            }
            if ku.data_encipherment() {
                out.push("Data Encipherment".to_string());
            }
            if ku.key_agreement() {
                out.push("Key Agreement".to_string());
            }
            if ku.key_cert_sign() {
                out.push("Certificate Signing".to_string());
            }
            if ku.crl_sign() {
                out.push("CRL Signing".to_string());
            }
            if ku.encipher_only() {
                out.push("Encipher Only".to_string());
            }
            if ku.decipher_only() {
                out.push("Decipher Only".to_string());
            }
            return out;
        }
    }
    Vec::new()
}

#[cfg(feature = "crypto")]
fn parse_extended_key_usage_from_x509(cert: &X509Certificate) -> Vec<String> {
    use x509_parser::extensions::ParsedExtension;
    for ext in cert.extensions() {
        if let ParsedExtension::ExtendedKeyUsage(eku) = &ext.parsed_extension() {
            let mut out = Vec::new();
            if eku.any {
                out.push("Any".to_string());
            }
            if eku.server_auth {
                out.push("Server Auth".to_string());
            }
            if eku.client_auth {
                out.push("Client Auth".to_string());
            }
            if eku.code_signing {
                out.push("Code Signing".to_string());
            }
            if eku.email_protection {
                out.push("Email Protection".to_string());
            }
            if eku.time_stamping {
                out.push("Time Stamping".to_string());
            }
            if eku.ocsp_signing {
                out.push("OCSP Signing".to_string());
            }
            for oid in &eku.other {
                out.push(oid.to_string());
            }
            return out;
        }
    }
    Vec::new()
}

#[cfg(feature = "crypto")]
fn parse_is_ca_from_x509(cert: &X509Certificate) -> bool {
    use x509_parser::extensions::ParsedExtension;
    for ext in cert.extensions() {
        if let ParsedExtension::BasicConstraints(bc) = &ext.parsed_extension() {
            return bc.ca;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct Pkcs7Info {
    pub version: u32,
    pub digest_algorithms: Vec<String>,
    pub content_info: ContentInfo,
    pub certificates: Vec<Vec<u8>>,
    pub signer_infos: Vec<SignerInfo>,
    pub raw_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ContentInfo {
    pub content_type: String,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct SignerInfo {
    pub version: u32,
    pub issuer_and_serial: IssuerAndSerial,
    pub digest_algorithm: String,
    pub signature_algorithm: String,
    pub signature: Vec<u8>,
    pub authenticated_attributes: Vec<Attribute>,
    pub unauthenticated_attributes: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub struct IssuerAndSerial {
    pub issuer: String,
    pub serial_number: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub oid: String,
    pub values: Vec<Vec<u8>>,
}

/// PKCS#7 builder for creating signed data
pub struct Pkcs7Builder {
    content: Option<Vec<u8>>,
    certificates: Vec<Vec<u8>>,
    signers: Vec<SignerConfig>,
    detached: bool,
}

impl Default for Pkcs7Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Pkcs7Builder {
    pub fn new() -> Self {
        Self {
            content: None,
            certificates: Vec::new(),
            signers: Vec::new(),
            detached: false,
        }
    }

    pub fn with_content(mut self, content: Vec<u8>) -> Self {
        self.content = Some(content);
        self
    }

    pub fn add_certificate(mut self, cert: Vec<u8>) -> Self {
        self.certificates.push(cert);
        self
    }

    pub fn add_signer(mut self, signer: SignerConfig) -> Self {
        self.signers.push(signer);
        self
    }

    pub fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    pub fn build(self) -> CryptoResult<Vec<u8>> {
        #[cfg(feature = "crypto")]
        {
            self.build_with_openssl()
        }
        #[cfg(not(feature = "crypto"))]
        {
            Err(CryptoError::UnsupportedAlgorithm(
                "PKCS#7 building requires crypto feature".to_string(),
            ))
        }
    }

    /// Build PKCS#7 using OpenSSL
    #[cfg(feature = "crypto")]
    fn build_with_openssl(self) -> CryptoResult<Vec<u8>> {
        use openssl::{
            pkcs7::{Pkcs7, Pkcs7Flags},
            stack::Stack,
        };

        if self.signers.is_empty() {
            return Err(CryptoError::InvalidSignatureFormat(
                "No signers provided".to_string(),
            ));
        }

        // Parse certificates and private keys for signers
        let mut signer_certs = Stack::new().map_err(|e| {
            CryptoError::InvalidSignatureFormat(format!(
                "Failed to create certificate stack: {}",
                e
            ))
        })?;
        let mut signer_keys = Vec::new();

        for signer in &self.signers {
            // Parse certificate
            let cert = X509::from_der(&signer.certificate).map_err(|e| {
                CryptoError::CertificateError(format!("Failed to parse signer certificate: {}", e))
            })?;
            signer_certs.push(cert).map_err(|e| {
                CryptoError::InvalidSignatureFormat(format!(
                    "Failed to add certificate to stack: {}",
                    e
                ))
            })?;

            // Parse private key
            let key = PKey::private_key_from_der(&signer.private_key)
                .or_else(|_| PKey::private_key_from_pem(&signer.private_key))
                .map_err(|e| {
                    CryptoError::InvalidKey(format!("Failed to parse private key: {}", e))
                })?;
            signer_keys.push(key);
        }

        // Parse additional certificates
        let mut certs = Stack::new().map_err(|e| {
            CryptoError::InvalidSignatureFormat(format!(
                "Failed to create certificate stack: {}",
                e
            ))
        })?;
        for cert_data in &self.certificates {
            let cert = X509::from_der(cert_data).map_err(|e| {
                CryptoError::CertificateError(format!("Failed to parse certificate: {}", e))
            })?;
            certs.push(cert).map_err(|e| {
                CryptoError::InvalidSignatureFormat(format!(
                    "Failed to add certificate to stack: {}",
                    e
                ))
            })?;
        }

        // Create PKCS#7 signed data
        let content_bytes;
        let content_slice = if let Some(content_data) = &self.content {
            content_bytes = content_data.as_slice();
            Some(content_bytes)
        } else {
            None
        };

        let flags = if self.detached {
            Pkcs7Flags::DETACHED
        } else {
            Pkcs7Flags::empty()
        };

        // For simplicity, using the first signer
        if let (Some(first_signer_cert), Some(first_signer_key)) =
            (signer_certs.get(0), signer_keys.first())
        {
            let content_for_signing = content_slice.unwrap_or(&[]);

            let pkcs7 = Pkcs7::sign(
                first_signer_cert,
                first_signer_key,
                &certs,
                content_for_signing,
                flags,
            )
            .map_err(|e| {
                CryptoError::InvalidSignatureFormat(format!("Failed to create PKCS#7: {}", e))
            })?;

            // Convert to DER format
            pkcs7.to_der().map_err(|e| {
                CryptoError::InvalidSignatureFormat(format!("Failed to serialize PKCS#7: {}", e))
            })
        } else {
            Err(CryptoError::InvalidSignatureFormat(
                "No valid signers available".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignerConfig {
    pub certificate: Vec<u8>,
    pub private_key: Vec<u8>,
    pub digest_algorithm: String,
    pub signature_algorithm: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkcs7_handler_creation() {
        let handler = Pkcs7Handler::new();
        // Just test that it can be created
        assert_eq!(std::mem::size_of_val(&handler), 0); // Zero-sized struct
    }

    #[test]
    fn test_asn1_length_parsing() {
        let handler = Pkcs7Handler::new();

        // Test short form
        let short_data = [0x05]; // Length = 5
        let (start, length) = handler.parse_asn1_length(&short_data).unwrap();
        assert_eq!(start, 1);
        assert_eq!(length, 5);

        // Test long form
        let long_data = [0x81, 0xFF]; // Length = 255
        let (start, length) = handler.parse_asn1_length(&long_data).unwrap();
        assert_eq!(start, 2);
        assert_eq!(length, 255);
    }

    #[test]
    fn test_sequence_detection() {
        let handler = Pkcs7Handler::new();
        let data = [0x01, 0x02, 0x03, 0x04, 0x05];
        let sequence = [0x03, 0x04];

        assert!(handler.contains_sequence(&data, &sequence));

        let not_found = [0x06, 0x07];
        assert!(!handler.contains_sequence(&data, &not_found));
    }

    #[test]
    fn test_pkcs7_structure_validation() {
        let handler = Pkcs7Handler::new();

        // Test invalid data
        let invalid_data = [0x00, 0x01, 0x02];
        assert!(handler.parse_pkcs7_structure(&invalid_data).is_err());

        // Test data that starts like ASN.1 SEQUENCE
        let sequence_data = [0x30, 0x82, 0x01, 0x00]; // SEQUENCE, length 256
                                                      // This will fail because it doesn't contain SignedData OID, which is expected
        assert!(handler.parse_pkcs7_structure(&sequence_data).is_err());
    }

    #[test]
    fn test_pkcs7_builder() {
        let builder = Pkcs7Builder::new()
            .with_content(b"Hello, World!".to_vec())
            .detached(false);

        // Building will fail without proper certificates/keys, which is expected
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_functions() {
        let handler = Pkcs7Handler::new();
        let test_data = b"test data";

        // Test SHA-256
        let sha256_result = handler.compute_sha256(test_data);
        assert!(sha256_result.is_ok());
        assert_eq!(sha256_result.unwrap().len(), 32); // SHA-256 produces 32 bytes

        // Test SHA-1
        let sha1_result = handler.compute_sha1(test_data);
        assert!(sha1_result.is_ok());
        assert_eq!(sha1_result.unwrap().len(), 20); // SHA-1 produces 20 bytes

        // Test SHA-384
        let sha384_result = handler.compute_sha384(test_data);
        assert!(sha384_result.is_ok());
        assert_eq!(sha384_result.unwrap().len(), 48); // SHA-384 produces 48 bytes

        // Test SHA-512
        let sha512_result = handler.compute_sha512(test_data);
        assert!(sha512_result.is_ok());
        assert_eq!(sha512_result.unwrap().len(), 64); // SHA-512 produces 64 bytes
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn test_certificate_parsing_basic() {
        let handler = Pkcs7Handler::new();

        // Test that the method exists and handles invalid data gracefully
        // Since we're testing with invalid certificate data, we expect it to fail
        // with a proper error message rather than crashing
        let invalid_cert_data = vec![0x30, 0x82, 0x01, 0x00]; // Incomplete DER sequence
        let mut cert_data_extended = invalid_cert_data;
        cert_data_extended.resize(200, 0); // Make it large enough

        let result = handler.parse_certificate_simple(&cert_data_extended);

        // The test should verify that:
        // 1. Invalid certificate data is handled gracefully (returns error)
        // 2. The error message is meaningful
        match result {
            Ok(_) => {
                // If it succeeds, verify the certificate info has reasonable data
                let cert_info = result.unwrap();
                assert!(!cert_info.subject.is_empty());
                assert!(!cert_info.issuer.is_empty());
            }
            Err(e) => {
                // Verify that we get a proper certificate parsing error
                assert!(matches!(e, CryptoError::CertificateError(_)));
                if let CryptoError::CertificateError(msg) = e {
                    assert!(msg.contains("Failed to parse certificate"));
                }
            }
        }

        // Also test that extremely short data is rejected appropriately
        let too_short = vec![0x30, 0x01];
        let result_short = handler.parse_certificate_simple(&too_short);
        assert!(result_short.is_err());
    }
}
