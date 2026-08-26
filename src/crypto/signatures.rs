use super::{CryptoError, CryptoResult, SignatureVerificationResult};
use crate::types::PdfValue;

/// PDF signature handler with support for multiple signature formats
pub struct PdfSignatureHandler;

impl Default for PdfSignatureHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfSignatureHandler {
    pub fn new() -> Self {
        Self
    }

    /// Verify a PDF digital signature
    pub fn verify_pdf_signature(
        &self,
        signature_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> CryptoResult<SignatureVerificationResult> {
        // Extract signature components
        let filter = self.extract_filter(signature_dict)?;
        let contents = self.extract_contents(signature_dict)?;
        let byte_range = self.extract_byte_range(signature_dict)?;

        match filter.as_str() {
            "Adobe.PPKLite" | "Adobe.PPKMS" => self.verify_pkcs7_signature(&contents, &byte_range),
            "Adobe.PPK" => self.verify_x509_rsa_signature(&contents, &byte_range),
            "ETSI.CAdES.detached" => self.verify_cades_signature(&contents, &byte_range),
            "ETSI.RFC3161" => self.verify_timestamp_signature(&contents, &byte_range),
            _ => Err(CryptoError::UnsupportedAlgorithm(format!(
                "Unsupported signature filter: {}",
                filter
            ))),
        }
    }

    /// Extract signature filter from signature dictionary
    fn extract_filter(
        &self,
        sig_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> CryptoResult<String> {
        match sig_dict.get("Filter") {
            Some(PdfValue::Name(name)) => Ok(name.without_slash().to_string()),
            _ => Err(CryptoError::InvalidSignatureFormat(
                "Missing or invalid Filter".to_string(),
            )),
        }
    }

    /// Extract signature contents (the actual signature bytes)
    fn extract_contents(
        &self,
        sig_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> CryptoResult<Vec<u8>> {
        match sig_dict.get("Contents") {
            Some(PdfValue::String(s)) => {
                // Contents is typically hex-encoded
                self.decode_hex_string(s.as_bytes())
            }
            _ => Err(CryptoError::InvalidSignatureFormat(
                "Missing or invalid Contents".to_string(),
            )),
        }
    }

    /// Extract byte range for signature verification
    fn extract_byte_range(
        &self,
        sig_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> CryptoResult<Vec<u64>> {
        match sig_dict.get("ByteRange") {
            Some(PdfValue::Array(arr)) => {
                let mut byte_range = Vec::new();
                for item in arr.iter() {
                    match item {
                        PdfValue::Integer(i) => byte_range.push(*i as u64),
                        _ => {
                            return Err(CryptoError::InvalidSignatureFormat(
                                "Invalid ByteRange format".to_string(),
                            ))
                        }
                    }
                }
                if byte_range.len() != 4 {
                    return Err(CryptoError::InvalidSignatureFormat(
                        "ByteRange must have 4 elements".to_string(),
                    ));
                }
                Ok(byte_range)
            }
            _ => Err(CryptoError::InvalidSignatureFormat(
                "Missing or invalid ByteRange".to_string(),
            )),
        }
    }

    /// Decode hex string to bytes
    fn decode_hex_string(&self, hex_str: &[u8]) -> CryptoResult<Vec<u8>> {
        let hex_str = std::str::from_utf8(hex_str).map_err(|_| {
            CryptoError::InvalidSignatureFormat("Invalid UTF-8 in hex string".to_string())
        })?;

        let hex_str = hex_str.trim_start_matches('<').trim_end_matches('>');

        if hex_str.len() % 2 != 0 {
            return Err(CryptoError::InvalidSignatureFormat(
                "Hex string length must be even".to_string(),
            ));
        }

        let mut result = Vec::with_capacity(hex_str.len() / 2);
        for chunk in hex_str.as_bytes().as_chunks::<2>().0 {
            let hex_byte = std::str::from_utf8(chunk).map_err(|_| {
                CryptoError::InvalidSignatureFormat("Invalid hex character".to_string())
            })?;
            let byte = u8::from_str_radix(hex_byte, 16).map_err(|_| {
                CryptoError::InvalidSignatureFormat("Invalid hex digit".to_string())
            })?;
            result.push(byte);
        }

        Ok(result)
    }

    /// Verify PKCS#7 signature (Adobe.PPKLite, Adobe.PPKMS)
    fn verify_pkcs7_signature(
        &self,
        contents: &[u8],
        byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        #[cfg(feature = "crypto")]
        {
            self.verify_pkcs7_with_openssl(contents, byte_range)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (contents, byte_range);
            Err(CryptoError::UnsupportedAlgorithm(
                "PKCS#7 verification requires the crypto feature".to_string(),
            ))
        }
    }

    /// Verify X.509 RSA signature (Adobe.PPK)
    fn verify_x509_rsa_signature(
        &self,
        contents: &[u8],
        byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        #[cfg(feature = "crypto")]
        {
            self.verify_x509_with_openssl(contents, byte_range)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (contents, byte_range);
            Err(CryptoError::UnsupportedAlgorithm(
                "X.509 verification requires the crypto feature".to_string(),
            ))
        }
    }

    /// Verify CAdES signature (ETSI.CAdES.detached)
    fn verify_cades_signature(
        &self,
        contents: &[u8],
        byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        #[cfg(feature = "crypto")]
        {
            self.verify_cades_with_openssl(contents, byte_range)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (contents, byte_range);
            Err(CryptoError::UnsupportedAlgorithm(
                "CAdES verification requires the crypto feature".to_string(),
            ))
        }
    }

    /// Verify timestamp signature (ETSI.RFC3161)
    fn verify_timestamp_signature(
        &self,
        contents: &[u8],
        byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        #[cfg(feature = "crypto")]
        {
            self.verify_timestamp_with_openssl(contents, byte_range)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (contents, byte_range);
            Err(CryptoError::UnsupportedAlgorithm(
                "RFC3161 verification requires the crypto feature".to_string(),
            ))
        }
    }

    // OpenSSL-based verification methods (only available with crypto feature)
    #[cfg(feature = "crypto")]
    fn verify_pkcs7_with_openssl(
        &self,
        _contents: &[u8],
        _byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        Err(CryptoError::UnsupportedAlgorithm(
            "OpenSSL PKCS#7 verification is not implemented".to_string(),
        ))
    }

    #[cfg(feature = "crypto")]
    fn verify_x509_with_openssl(
        &self,
        _contents: &[u8],
        _byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        Err(CryptoError::UnsupportedAlgorithm(
            "OpenSSL X.509 verification is not implemented".to_string(),
        ))
    }

    #[cfg(feature = "crypto")]
    fn verify_cades_with_openssl(
        &self,
        _contents: &[u8],
        _byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        Err(CryptoError::UnsupportedAlgorithm(
            "CAdES verification is not implemented".to_string(),
        ))
    }

    #[cfg(feature = "crypto")]
    fn verify_timestamp_with_openssl(
        &self,
        _contents: &[u8],
        _byte_range: &[u64],
    ) -> CryptoResult<SignatureVerificationResult> {
        Err(CryptoError::UnsupportedAlgorithm(
            "RFC3161 verification is not implemented".to_string(),
        ))
    }
}

/// Signature validation utilities
pub struct SignatureValidator;

impl SignatureValidator {
    /// Validate signature dictionary structure
    pub fn validate_signature_dict(
        &self,
        sig_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> Result<Vec<String>, Vec<String>> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Required fields
        if !sig_dict.contains_key("Filter") {
            errors.push("Missing required Filter field".to_string());
        }
        if !sig_dict.contains_key("Contents") {
            errors.push("Missing required Contents field".to_string());
        }
        if !sig_dict.contains_key("ByteRange") {
            errors.push("Missing required ByteRange field".to_string());
        }

        // Optional but recommended fields
        if !sig_dict.contains_key("M") {
            warnings.push("Missing signing time (M field)".to_string());
        }
        if !sig_dict.contains_key("Name") {
            warnings.push("Missing signer name (Name field)".to_string());
        }
        if !sig_dict.contains_key("Reason") {
            warnings.push("Missing signing reason (Reason field)".to_string());
        }

        // Validate ByteRange format
        if let Some(PdfValue::Array(arr)) = sig_dict.get("ByteRange") {
            if arr.len() != 4 {
                errors.push("ByteRange must contain exactly 4 integers".to_string());
            } else {
                for (i, item) in arr.iter().enumerate() {
                    if !matches!(item, PdfValue::Integer(_)) {
                        errors.push(format!("ByteRange[{}] must be an integer", i));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }
    }

    /// Extract signature metadata from dictionary
    pub fn extract_signature_metadata(
        &self,
        sig_dict: &std::collections::HashMap<String, PdfValue>,
    ) -> SignatureMetadata {
        SignatureMetadata {
            filter: sig_dict
                .get("Filter")
                .and_then(|v| v.as_name())
                .map(|n| n.without_slash().to_string()),
            sub_filter: sig_dict
                .get("SubFilter")
                .and_then(|v| v.as_name())
                .map(|n| n.without_slash().to_string()),
            name: sig_dict
                .get("Name")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string_lossy()),
            location: sig_dict
                .get("Location")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string_lossy()),
            reason: sig_dict
                .get("Reason")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string_lossy()),
            contact_info: sig_dict
                .get("ContactInfo")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string_lossy()),
            signing_time: sig_dict
                .get("M")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string_lossy()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignatureMetadata {
    pub filter: Option<String>,
    pub sub_filter: Option<String>,
    pub name: Option<String>,
    pub location: Option<String>,
    pub reason: Option<String>,
    pub contact_info: Option<String>,
    pub signing_time: Option<String>,
}

/// Signature format detector
pub struct SignatureFormatDetector;

impl SignatureFormatDetector {
    /// Detect signature format from binary data
    pub fn detect_format(&self, signature_data: &[u8]) -> SignatureFormat {
        if signature_data.len() < 10 {
            return SignatureFormat::Unknown;
        }

        // Check for ASN.1 DER/BER encoding (PKCS#7, X.509)
        if signature_data[0] == 0x30 {
            if self.looks_like_pkcs7(signature_data) {
                return SignatureFormat::PKCS7;
            } else if self.looks_like_x509_cert(signature_data) {
                return SignatureFormat::X509Certificate;
            }
        }

        // Check for PEM encoding
        if signature_data.starts_with(b"-----BEGIN") {
            if signature_data.starts_with(b"-----BEGIN PKCS7") {
                return SignatureFormat::Pkcs7Pem;
            } else if signature_data.starts_with(b"-----BEGIN CERTIFICATE") {
                return SignatureFormat::X509CertificatePem;
            }
        }

        // Check for timestamp token
        if self.looks_like_timestamp_token(signature_data) {
            return SignatureFormat::Rfc3161Timestamp;
        }

        SignatureFormat::Unknown
    }

    fn looks_like_pkcs7(&self, data: &[u8]) -> bool {
        // Simplified PKCS#7 detection
        // Real implementation would parse ASN.1 structure
        data.len() > 50 && data[0] == 0x30
    }

    fn looks_like_x509_cert(&self, data: &[u8]) -> bool {
        // Simplified X.509 certificate detection
        // Real implementation would parse ASN.1 structure
        data.len() > 100 && data[0] == 0x30
    }

    fn looks_like_timestamp_token(&self, data: &[u8]) -> bool {
        // Simplified timestamp token detection
        // Real implementation would look for specific OIDs
        data.len() > 20 && data[0] == 0x30
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignatureFormat {
    PKCS7,
    Pkcs7Pem,
    X509Certificate,
    X509CertificatePem,
    Rfc3161Timestamp,
    CAdES,
    PAdES,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PdfArray, PdfName, PdfString};
    use std::collections::HashMap;

    #[test]
    fn test_signature_validator() {
        let validator = SignatureValidator;
        let mut sig_dict = HashMap::new();

        // Test missing required fields
        let result = validator.validate_signature_dict(&sig_dict);
        assert!(result.is_err());

        // Add required fields
        sig_dict.insert(
            "Filter".to_string(),
            PdfValue::Name(PdfName::new("Adobe.PPKLite")),
        );
        sig_dict.insert(
            "Contents".to_string(),
            PdfValue::String(PdfString::new_literal(b"<3082...")),
        );

        let mut byte_range = PdfArray::new();
        byte_range.push(PdfValue::Integer(0));
        byte_range.push(PdfValue::Integer(1000));
        byte_range.push(PdfValue::Integer(2000));
        byte_range.push(PdfValue::Integer(3000));
        sig_dict.insert("ByteRange".to_string(), PdfValue::Array(byte_range));

        let result = validator.validate_signature_dict(&sig_dict);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hex_string_decoding() {
        let handler = PdfSignatureHandler::new();

        let hex_str = b"<48656C6C6F>";
        let result = handler.decode_hex_string(hex_str).unwrap();
        assert_eq!(result, b"Hello");

        let invalid_hex = b"<GG>";
        assert!(handler.decode_hex_string(invalid_hex).is_err());
    }

    #[test]
    fn test_signature_format_detection() {
        let detector = SignatureFormatDetector;

        // Test ASN.1 DER format with sufficient length
        // The original test expected either PKCS7 or X509Certificate for ASN.1 data
        let mut der_data = vec![0x30, 0x82, 0x01, 0x00]; // ASN.1 SEQUENCE header
        der_data.resize(60, 0x00); // Make it large enough to be detected
        let format = detector.detect_format(&der_data);
        // The current implementation will detect this as PKCS7 since it checks PKCS7 first
        assert!(matches!(
            format,
            SignatureFormat::PKCS7 | SignatureFormat::X509Certificate
        ));

        // Test with data that's too small to be detected as either PKCS7 or X509
        let small_der_data = [0x30, 0x82, 0x01, 0x00]; // Only 4 bytes
        let format = detector.detect_format(&small_der_data);
        assert_eq!(format, SignatureFormat::Unknown);

        // Test PEM format
        let pem_data = b"-----BEGIN CERTIFICATE-----";
        let format = detector.detect_format(pem_data);
        assert_eq!(format, SignatureFormat::X509CertificatePem);

        // Test unknown format
        let unknown_data = b"unknown";
        let format = detector.detect_format(unknown_data);
        assert_eq!(format, SignatureFormat::Unknown);
    }
}
