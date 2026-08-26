#![allow(dead_code)]

use crate::crypto::certificates::{
    CertificateChainValidator, RevocationEvent, TrustStore, X509Certificate,
};
use crate::crypto::chrono::{DateTime, Utc};
use crate::crypto::pkcs7::{SignedData, SignerInfo};
use crate::crypto::timestamp;
use crate::crypto::CryptoConfig;
use crate::types::{PdfDictionary, PdfValue};
use std::io::{Read, Seek};

#[cfg(feature = "crypto")]
use der_parser::asn1_rs::ASN1DateTime;
#[cfg(feature = "crypto")]
use der_parser::ber::BerObjectContent;
#[cfg(feature = "crypto")]
use der_parser::der::parse_der;
#[cfg(feature = "crypto")]
use openssl::hash::{hash, MessageDigest};
#[cfg(feature = "crypto")]
use openssl::sign::Verifier;
#[cfg(feature = "crypto")]
use openssl::x509::X509;

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Complete signature verification for PDF documents
pub struct SignatureVerifier {
    trust_store: TrustStore,
    config: CryptoConfig,
}

#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub field_name: String,
    pub signer: SignerCertificate,
    pub signing_time: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub contact_info: Option<String>,
    pub byte_range: Vec<(u64, u64)>,
    pub filter: String,
    pub sub_filter: String,
    pub validity: SignatureValidity,
    pub certificate_chain: Vec<X509Certificate>,
    pub timestamp: Option<TimestampInfo>,
}

#[derive(Debug, Clone)]
pub struct SignerCertificate {
    pub subject: String,
    pub issuer: String,
    pub serial_number: Vec<u8>,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub key_usage: Vec<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TimestampInfo {
    pub time: DateTime<Utc>,
    pub tsa_certificate: Option<X509Certificate>,
    pub accuracy: Option<TimestampAccuracy>,
    pub policy_oid: Option<String>,
    pub hash_algorithm: Option<String>,
    pub message_imprint: Option<Vec<u8>>,
    pub signature_valid: bool,
    pub tsa_chain_valid: Option<bool>,
    pub tsa_chain_errors: Vec<String>,
    pub tsa_chain_warnings: Vec<String>,
    pub tsa_pin_valid: Option<bool>,
    pub tsa_pin_reason: Option<String>,
    pub tsa_revocation_events: Vec<RevocationEvent>,
}

#[derive(Debug, Clone)]
pub struct TimestampAccuracy {
    pub seconds: u64,
    pub millis: Option<u32>,
    pub micros: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignatureValidity {
    Valid,
    Invalid(String),
    Unknown(String),
    CertificateExpired,
    CertificateNotYetValid,
    CertificateRevoked,
    UntrustedCertificate,
    DocumentModified,
    DigestMismatch,
    SignatureFormatError,
}

impl Default for SignatureVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier {
    pub fn new() -> Self {
        let trust_store = TrustStore::new("No trust store configured".to_string());
        SignatureVerifier {
            trust_store,
            config: CryptoConfig::default(),
        }
    }

    pub fn with_trust_store(mut self, trust_store: TrustStore) -> Self {
        self.trust_store = trust_store;
        self
    }

    pub fn with_crypto_config(mut self, config: CryptoConfig) -> Self {
        self.config = config;
        self
    }

    /// Verify a signature dictionary
    pub fn verify_signature<R: Read + Seek>(
        &mut self,
        sig_dict: &PdfDictionary,
        field_name: &str,
        reader: &mut R,
    ) -> SignatureInfo {
        let mut info = SignatureInfo {
            field_name: field_name.to_string(),
            signer: SignerCertificate {
                subject: String::new(),
                issuer: String::new(),
                serial_number: Vec::new(),
                not_before: Utc::now(),
                not_after: Utc::now(),
                key_usage: Vec::new(),
                email: None,
            },
            signing_time: None,
            reason: self.extract_string(sig_dict, "Reason"),
            location: self.extract_string(sig_dict, "Location"),
            contact_info: self.extract_string(sig_dict, "ContactInfo"),
            byte_range: Vec::new(),
            filter: String::new(),
            sub_filter: String::new(),
            validity: SignatureValidity::Unknown("Not verified".to_string()),
            certificate_chain: Vec::new(),
            timestamp: None,
        };

        // Extract filter and sub-filter
        info.filter = sig_dict
            .get("Filter")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash().to_string()),
                _ => None,
            })
            .unwrap_or_else(|| "Unknown".to_string());

        info.sub_filter = sig_dict
            .get("SubFilter")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash().to_string()),
                _ => None,
            })
            .unwrap_or_else(|| "Unknown".to_string());

        info.signing_time = self.extract_pdf_signing_time(sig_dict);

        // Extract ByteRange
        info.byte_range = self.extract_byte_range(sig_dict);

        // Extract Contents (PKCS#7 signature)
        let contents = match sig_dict.get("Contents") {
            Some(PdfValue::String(s)) => s.clone(),
            _ => {
                info.validity = SignatureValidity::SignatureFormatError;
                return info;
            }
        };

        // Verify based on SubFilter
        match info.sub_filter.as_str() {
            "adbe.pkcs7.detached" => {
                self.verify_pkcs7_detached(&mut info, contents.as_bytes(), reader);
            }
            "adbe.pkcs7.sha1" => {
                self.verify_pkcs7_sha1(&mut info, contents.as_bytes(), reader);
            }
            "adbe.x509.rsa_sha1" => {
                self.verify_x509_rsa_sha1(&mut info, contents.as_bytes(), sig_dict, reader);
            }
            "ETSI.CAdES.detached" => {
                self.verify_cades(&mut info, contents.as_bytes(), reader);
            }
            "ETSI.RFC3161" => {
                self.verify_timestamp(&mut info, contents.as_bytes(), reader);
            }
            _ => {
                info.validity = SignatureValidity::Unknown(format!(
                    "Unsupported SubFilter: {}",
                    info.sub_filter
                ));
            }
        }

        info
    }

    fn verify_pkcs7_detached(
        &mut self,
        info: &mut SignatureInfo,
        contents: &[u8],
        reader: &mut dyn ReadSeek,
    ) {
        let signed_bytes = match self.read_byte_ranges(info, reader) {
            Ok(bytes) => bytes,
            Err(e) => {
                info.validity = SignatureValidity::Invalid(e);
                return;
            }
        };

        let handler = crate::crypto::pkcs7::Pkcs7Handler::new();
        let result = match handler.verify_pkcs7(contents, &signed_bytes) {
            Ok(r) => r,
            Err(e) => {
                info.validity =
                    SignatureValidity::Invalid(format!("PKCS#7 verification error: {}", e));
                return;
            }
        };

        info.certificate_chain = result.certificate_chain.clone();
        if let Some(signer_cert) = result.signer_certificate.clone() {
            info.signer = self.extract_signer_info(&signer_cert);
        }
        info.signing_time = result.signing_time;

        if result.is_valid {
            info.validity = self.verify_certificate_chain(&info.certificate_chain);
        } else {
            info.validity = SignatureValidity::Invalid(
                result
                    .error_message
                    .unwrap_or_else(|| "PKCS#7 signature invalid".to_string()),
            );
        }
    }

    fn verify_pkcs7_sha1(
        &mut self,
        info: &mut SignatureInfo,
        contents: &[u8],
        reader: &mut dyn ReadSeek,
    ) {
        // Similar to detached but with SHA-1 digest included
        self.verify_pkcs7_detached(info, contents, reader);
    }

    fn verify_x509_rsa_sha1(
        &mut self,
        info: &mut SignatureInfo,
        contents: &[u8],
        cert_data: &PdfDictionary,
        reader: &mut dyn ReadSeek,
    ) {
        // Extract certificate
        let cert_bytes = match cert_data.get("Cert") {
            Some(PdfValue::String(s)) => s,
            Some(PdfValue::Array(arr)) if !arr.is_empty() => match &arr[0] {
                PdfValue::String(s) => s,
                _ => {
                    info.validity = SignatureValidity::SignatureFormatError;
                    return;
                }
            },
            _ => {
                info.validity = SignatureValidity::SignatureFormatError;
                return;
            }
        };

        let cert_der = match parse_cert_der(cert_bytes.as_bytes()) {
            Ok(der) => der,
            Err(err) => {
                info.validity = SignatureValidity::Invalid(err);
                return;
            }
        };

        // Parse X.509 certificate
        let cert = match crate::crypto::certificates::parse_der_certificate(&cert_der) {
            Ok(c) => c,
            Err(_) => {
                info.validity = SignatureValidity::SignatureFormatError;
                return;
            }
        };

        info.certificate_chain = vec![cert.clone()];
        info.signer = self.extract_signer_info(&cert);

        let signed_bytes = match self.read_byte_ranges(info, reader) {
            Ok(bytes) => bytes,
            Err(err) => {
                info.validity = SignatureValidity::Invalid(err);
                return;
            }
        };

        let verified = self.verify_rsa_signature(contents, &signed_bytes, &cert, "SHA-1");
        if verified {
            info.validity = self.verify_certificate_chain(&info.certificate_chain);
        } else {
            info.validity = SignatureValidity::Invalid("X.509 RSA signature invalid".to_string());
        }
    }

    fn verify_cades(
        &mut self,
        info: &mut SignatureInfo,
        contents: &[u8],
        reader: &mut dyn ReadSeek,
    ) {
        // CAdES is PKCS#7 with additional requirements
        self.verify_pkcs7_detached(info, contents, reader);

        // Additional CAdES validation would go here
        // - Signing certificate reference
        // - Signature policy identifier
        // - Commitment type indication
    }

    fn verify_timestamp(
        &mut self,
        info: &mut SignatureInfo,
        contents: &[u8],
        reader: &mut dyn ReadSeek,
    ) {
        // Parse RFC 3161 TimeStampToken
        let tst = match self.parse_timestamp_token(contents) {
            Ok(t) => t,
            Err(_) => {
                info.validity = SignatureValidity::SignatureFormatError;
                return;
            }
        };

        let mut timestamp_info = tst;
        timestamp_info.signature_valid = false;
        #[cfg(feature = "crypto")]
        {
            if timestamp::verify_timestamp_signature(contents).is_ok() {
                timestamp_info.signature_valid = true;
            }
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = contents;
            info.validity = SignatureValidity::Unknown(
                "Timestamp verification requires crypto feature".to_string(),
            );
            info.timestamp = Some(timestamp_info);
            return;
        }

        if !timestamp_info.signature_valid {
            info.validity = SignatureValidity::Invalid("Timestamp signature invalid".to_string());
            info.timestamp = Some(timestamp_info);
            return;
        }

        let digest_algorithm = timestamp_info
            .hash_algorithm
            .clone()
            .unwrap_or_else(|| "SHA-256".to_string());

        info.timestamp = Some(timestamp_info);

        // Compute document digest
        let document_digest = match self.compute_document_digest(info, reader, &digest_algorithm) {
            Ok(d) => d,
            Err(_) => {
                info.validity = SignatureValidity::Invalid("Failed to compute digest".to_string());
                return;
            }
        };

        // Verify timestamp covers document digest
        if !self.verify_timestamp_digest(info.timestamp.as_ref().unwrap(), &document_digest) {
            info.validity = SignatureValidity::Invalid("Timestamp digest mismatch".to_string());
            return;
        }

        // Validate TSA certificate chain if available
        if let Some(ts) = info.timestamp.as_mut() {
            if let Some(tsa_cert) = ts.tsa_certificate.as_ref() {
                if let Some((valid, reason)) =
                    check_tsa_pinning(&self.config, &tsa_cert.fingerprint_sha256)
                {
                    ts.tsa_pin_valid = Some(valid);
                    ts.tsa_pin_reason = reason.clone();
                    if !valid {
                        info.validity = SignatureValidity::Invalid(
                            reason.unwrap_or_else(|| "TSA pinning failed".to_string()),
                        );
                        return;
                    }
                }

                let mut chain = Vec::new();
                chain.push(tsa_cert.der.clone());
                #[cfg(feature = "crypto")]
                {
                    let mut extra = timestamp::extract_tsa_certificates_der(contents);
                    extra.retain(|c| c != &tsa_cert.der);
                    chain.extend(extra);
                }

                if !chain.is_empty()
                    && self.config.enable_cert_chain_validation
                    && self.config.enable_tsa_chain_validation
                {
                    let chain_refs: Vec<&[u8]> = chain.iter().map(|c| c.as_slice()).collect();
                    let mut config = self.config.clone();
                    if config.enable_tsa_revocation_checks {
                        config.enable_ocsp_checking = true;
                        config.enable_crl_checking = true;
                    }
                    if let Ok((result, events)) = CertificateChainValidator::new(config)
                        .and_then(|v| v.validate_chain_with_revocation_details(&chain_refs))
                    {
                        let valid = result.is_valid;
                        ts.tsa_chain_valid = Some(valid);
                        ts.tsa_chain_errors = result.validation_errors;
                        ts.tsa_chain_warnings = result.validation_warnings;
                        ts.tsa_revocation_events = events;
                        if !valid {
                            info.validity = SignatureValidity::UntrustedCertificate;
                            return;
                        }
                    }
                }
            }
        }

        info.validity = SignatureValidity::Valid;
    }

    fn compute_document_digest(
        &self,
        info: &SignatureInfo,
        reader: &mut dyn ReadSeek,
        algorithm: &str,
    ) -> Result<Vec<u8>, String> {
        let bytes = self.read_byte_ranges(info, reader)?;
        #[cfg(feature = "crypto")]
        {
            let md = match algorithm.to_ascii_uppercase().as_str() {
                "SHA-1" | "SHA1" | "1.3.14.3.2.26" => MessageDigest::sha1(),
                "SHA-256" | "SHA256" | "2.16.840.1.101.3.4.2.1" => MessageDigest::sha256(),
                "SHA-384" | "SHA384" | "2.16.840.1.101.3.4.2.2" => MessageDigest::sha384(),
                "SHA-512" | "SHA512" | "2.16.840.1.101.3.4.2.3" => MessageDigest::sha512(),
                _ => return Err("Unsupported digest algorithm".to_string()),
            };
            hash(md, &bytes)
                .map(|digest| digest.to_vec())
                .map_err(|e| format!("Digest error: {}", e))
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = algorithm;
            Err("Digest computation requires crypto feature".to_string())
        }
    }

    fn verify_message_digest(
        &self,
        auth_attrs: &[crate::crypto::pkcs7::Attribute],
        expected: &[u8],
    ) -> bool {
        for attr in auth_attrs {
            if attr.oid == "1.2.840.113549.1.9.4" {
                if let Some(value) = attr.values.first() {
                    return value.as_slice() == expected;
                }
            }
        }
        false
    }

    fn verify_signature_value(
        &self,
        signer_info: &SignerInfo,
        cert: &X509Certificate,
        signed_data: &SignedData,
    ) -> bool {
        let data = match signed_data.content_info.content.as_deref() {
            Some(content) if !content.is_empty() => content,
            _ => return false,
        };
        self.verify_rsa_signature(
            &signer_info.signature,
            data,
            cert,
            signer_info.digest_algorithm.as_str(),
        )
    }

    fn verify_rsa_signature(
        &self,
        signature: &[u8],
        data: &[u8],
        cert: &X509Certificate,
        digest_algorithm: &str,
    ) -> bool {
        #[cfg(feature = "crypto")]
        {
            let cert_x509 = match X509::from_der(&cert.der) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let pkey = match cert_x509.public_key() {
                Ok(k) => k,
                Err(_) => return false,
            };
            let md = match digest_algorithm.to_ascii_uppercase().as_str() {
                "SHA-1" | "SHA1" => MessageDigest::sha1(),
                "SHA-256" | "SHA256" => MessageDigest::sha256(),
                "SHA-384" | "SHA384" => MessageDigest::sha384(),
                "SHA-512" | "SHA512" => MessageDigest::sha512(),
                _ => return false,
            };
            let mut verifier = match Verifier::new(md, &pkey) {
                Ok(v) => v,
                Err(_) => return false,
            };
            verifier.verify_oneshot(signature, data).unwrap_or(false)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (signature, data, cert, digest_algorithm);
            false
        }
    }

    fn find_signer_certificate(
        &self,
        signer_info: &SignerInfo,
        certificates: &[X509Certificate],
    ) -> Option<X509Certificate> {
        // Find certificate matching signer info
        for cert in certificates {
            if self.matches_signer_info(cert, signer_info) {
                return Some(cert.clone());
            }
        }
        None
    }

    fn matches_signer_info(&self, cert: &X509Certificate, signer_info: &SignerInfo) -> bool {
        let issuer_match = cert.issuer == signer_info.issuer_and_serial.issuer;
        let serial_match =
            cert.serial_number.as_bytes() == signer_info.issuer_and_serial.serial_number.as_slice();
        issuer_match && serial_match
    }

    fn verify_certificate_chain(&mut self, chain: &[X509Certificate]) -> SignatureValidity {
        if chain.is_empty() {
            return SignatureValidity::Invalid("No certificates".to_string());
        }

        let now = Utc::now();

        // Check certificate validity periods
        for cert in chain {
            let not_before = cert.not_before;
            let not_after = cert.not_after;

            if not_before.timestamp() > now.timestamp() {
                return SignatureValidity::CertificateNotYetValid;
            }
            if not_after.timestamp() < now.timestamp() {
                return SignatureValidity::CertificateExpired;
            }
        }

        if let Some(root) = chain.last() {
            if self.trust_store.contains_certificate(root) {
                return SignatureValidity::Valid;
            }
        }

        SignatureValidity::UntrustedCertificate
    }

    fn extract_signer_info(&self, cert: &X509Certificate) -> SignerCertificate {
        SignerCertificate {
            subject: cert.subject.clone(),
            issuer: cert.issuer.clone(),
            serial_number: cert.serial_number.as_bytes().to_vec(),
            not_before: cert.not_before,
            not_after: cert.not_after,
            key_usage: self.extract_key_usage(cert),
            email: self.extract_email(cert),
        }
    }

    fn extract_key_usage(&self, _cert: &X509Certificate) -> Vec<String> {
        _cert.key_usage.clone()
    }

    fn extract_email(&self, _cert: &X509Certificate) -> Option<String> {
        for part in _cert.subject.split(',') {
            let item = part.trim();
            if let Some(value) = item.strip_prefix("emailAddress=") {
                return Some(value.trim().to_string());
            }
            if let Some(value) = item.strip_prefix("E=") {
                return Some(value.trim().to_string());
            }
            if let Some(value) = item.strip_prefix("EMAIL=") {
                return Some(value.trim().to_string());
            }
        }
        None
    }

    fn extract_signing_time(
        &self,
        auth_attrs: &[crate::crypto::pkcs7::Attribute],
    ) -> Option<DateTime<Utc>> {
        #[cfg(feature = "crypto")]
        {
            for attr in auth_attrs {
                if attr.oid == "1.2.840.113549.1.9.5" {
                    if let Some(value) = attr.values.first() {
                        if let Some(ts) = parse_asn1_time(value) {
                            return Some(ts);
                        }
                    }
                }
            }
            None
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = auth_attrs;
            None
        }
    }

    fn extract_timestamp(
        &self,
        unsigned_attrs: &[crate::crypto::pkcs7::Attribute],
    ) -> Option<TimestampInfo> {
        for attr in unsigned_attrs {
            if attr.oid == "1.2.840.113549.1.9.16.2.14" {
                if let Some(value) = attr.values.first() {
                    if let Ok(ts) = self.parse_timestamp_token(value) {
                        return Some(ts);
                    }
                }
            }
        }
        None
    }

    fn parse_timestamp_token(&self, _data: &[u8]) -> Result<TimestampInfo, String> {
        #[cfg(feature = "crypto")]
        {
            let parsed = timestamp::parse_timestamp_token(_data)?;
            let tsa_certificate = parsed
                .tsa_certificate_der
                .as_deref()
                .and_then(|der| crate::crypto::certificates::parse_der_certificate(der).ok());

            Ok(TimestampInfo {
                time: parsed.time,
                tsa_certificate,
                accuracy: parsed.accuracy.map(|acc| TimestampAccuracy {
                    seconds: acc.seconds,
                    millis: acc.millis,
                    micros: acc.micros,
                }),
                policy_oid: parsed.policy_oid,
                hash_algorithm: parsed.hash_algorithm,
                message_imprint: parsed.message_imprint,
                signature_valid: false,
                tsa_chain_valid: None,
                tsa_chain_errors: Vec::new(),
                tsa_chain_warnings: Vec::new(),
                tsa_pin_valid: None,
                tsa_pin_reason: None,
                tsa_revocation_events: Vec::new(),
            })
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = _data;
            Err("Timestamp parsing requires crypto feature".to_string())
        }
    }

    fn read_byte_ranges(
        &self,
        info: &SignatureInfo,
        reader: &mut dyn ReadSeek,
    ) -> Result<Vec<u8>, String> {
        use std::io::SeekFrom;

        let total: u64 = info.byte_range.iter().map(|(_, len)| *len).sum();
        let mut output = Vec::with_capacity(total as usize);

        for (offset, length) in &info.byte_range {
            reader
                .seek(SeekFrom::Start(*offset))
                .map_err(|_| "Failed to seek byte range".to_string())?;
            let mut buffer = vec![0u8; *length as usize];
            let read = reader
                .read(&mut buffer)
                .map_err(|_| "Failed to read byte range".to_string())?;
            if read != *length as usize {
                return Err("Failed to read full byte range".to_string());
            }
            output.extend_from_slice(&buffer);
        }

        Ok(output)
    }

    fn verify_timestamp_digest(&self, _timestamp: &TimestampInfo, _digest: &[u8]) -> bool {
        if let Some(imprint) = &_timestamp.message_imprint {
            imprint.as_slice() == _digest
        } else {
            false
        }
    }

    fn extract_byte_range(&self, sig_dict: &PdfDictionary) -> Vec<(u64, u64)> {
        let mut ranges = Vec::new();

        if let Some(PdfValue::Array(br)) = sig_dict.get("ByteRange") {
            let mut i = 0;
            while i + 1 < br.len() {
                if let (Some(offset), Some(length)) = (br[i].as_integer(), br[i + 1].as_integer()) {
                    ranges.push((offset as u64, length as u64));
                }
                i += 2;
            }
        }

        ranges
    }

    fn extract_string(&self, dict: &PdfDictionary, key: &str) -> Option<String> {
        dict.get(key).and_then(|v| match v {
            PdfValue::String(s) => Some(s.to_string_lossy()),
            _ => None,
        })
    }

    fn extract_pdf_signing_time(&self, sig_dict: &PdfDictionary) -> Option<DateTime<Utc>> {
        let raw = sig_dict.get("M").and_then(|v| match v {
            PdfValue::String(s) => Some(s.to_string_lossy()),
            _ => None,
        })?;
        parse_pdf_date_to_datetime(&raw)
    }
}

#[cfg(feature = "crypto")]
fn parse_cert_der(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.starts_with(b"-----BEGIN") {
        let cert = X509::from_pem(data).map_err(|e| format!("Invalid PEM cert: {}", e))?;
        cert.to_der()
            .map_err(|e| format!("PEM to DER failed: {}", e))
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(not(feature = "crypto"))]
fn parse_cert_der(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

#[cfg(feature = "crypto")]
fn parse_asn1_time(data: &[u8]) -> Option<DateTime<Utc>> {
    let (_, obj) = parse_der(data).ok()?;
    let time = match obj.content {
        BerObjectContent::UTCTime(t) | BerObjectContent::GeneralizedTime(t) => t,
        _ => return None,
    };
    asn1_datetime_to_utc(time)
}

#[cfg(feature = "crypto")]
fn asn1_datetime_to_utc(time: ASN1DateTime) -> Option<DateTime<Utc>> {
    use der_parser::asn1_rs::ASN1TimeZone;
    let date =
        ::chrono::NaiveDate::from_ymd_opt(time.year as i32, time.month as u32, time.day as u32)?;
    let dt = date.and_hms_opt(time.hour as u32, time.minute as u32, time.second as u32)?;
    let mut ts = dt.and_utc().timestamp();

    match time.tz {
        ASN1TimeZone::Offset(h, m) => {
            let offset = (h as i64) * 3600 + (m as i64) * 60;
            ts -= offset;
        }
        ASN1TimeZone::Z | ASN1TimeZone::Undefined => {}
    }

    Some(DateTime::from_timestamp(ts))
}

fn parse_pdf_date_to_datetime(input: &str) -> Option<DateTime<Utc>> {
    let trimmed = input.trim();
    let date_str = trimmed.strip_prefix("D:").unwrap_or(trimmed);
    if date_str.len() < 14 {
        return None;
    }

    let year: i32 = date_str.get(0..4)?.parse().ok()?;
    let month: u32 = date_str.get(4..6)?.parse().ok()?;
    let day: u32 = date_str.get(6..8)?.parse().ok()?;
    let hour: u32 = date_str.get(8..10)?.parse().ok()?;
    let minute: u32 = date_str.get(10..12)?.parse().ok()?;
    let second: u32 = date_str.get(12..14)?.parse().ok()?;

    let date = ::chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let dt = date.and_hms_opt(hour, minute, second)?;
    Some(DateTime::from_timestamp(dt.and_utc().timestamp()))
}

#[cfg(feature = "crypto")]
pub fn verify_rsa_signature_with_cert_der(
    signature: &[u8],
    data: &[u8],
    cert_der: &[u8],
    digest_algorithm: &str,
) -> Result<bool, String> {
    let cert = X509::from_der(cert_der).map_err(|e| format!("Invalid cert DER: {}", e))?;
    let pkey = cert
        .public_key()
        .map_err(|e| format!("Public key error: {}", e))?;

    let md = match digest_algorithm.to_ascii_uppercase().as_str() {
        "SHA-1" | "SHA1" => MessageDigest::sha1(),
        "SHA-256" | "SHA256" => MessageDigest::sha256(),
        "SHA-384" | "SHA384" => MessageDigest::sha384(),
        "SHA-512" | "SHA512" => MessageDigest::sha512(),
        _ => return Err("Unsupported digest algorithm".to_string()),
    };

    let mut verifier =
        Verifier::new(md, &pkey).map_err(|e| format!("Verifier init error: {}", e))?;
    verifier
        .verify_oneshot(signature, data)
        .map_err(|e| format!("Verify error: {}", e))
}

fn check_tsa_pinning(config: &CryptoConfig, fingerprint: &str) -> Option<(bool, Option<String>)> {
    let allow = &config.tsa_allow_fingerprints;
    let block = &config.tsa_block_fingerprints;

    if !block.is_empty() && block.iter().any(|f| f.eq_ignore_ascii_case(fingerprint)) {
        return Some((false, Some("TSA fingerprint blocked".to_string())));
    }
    if !allow.is_empty() && !allow.iter().any(|f| f.eq_ignore_ascii_case(fingerprint)) {
        return Some((false, Some("TSA fingerprint not in allow list".to_string())));
    }
    if !allow.is_empty() || !block.is_empty() {
        return Some((true, None));
    }
    None
}

pub fn check_tsa_pinning_for_test(
    config: &CryptoConfig,
    fingerprint: &str,
) -> Option<(bool, Option<String>)> {
    check_tsa_pinning(config, fingerprint)
}

/// Document signature manager
#[allow(dead_code)]
pub struct DocumentSignatures {
    signatures: Vec<SignatureInfo>,
    verifier: SignatureVerifier,
}

impl Default for DocumentSignatures {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentSignatures {
    pub fn new() -> Self {
        DocumentSignatures {
            signatures: Vec::new(),
            verifier: SignatureVerifier::new(),
        }
    }

    pub fn extract_from_acroform(&mut self, acroform: &PdfDictionary) {
        // Extract signature fields from AcroForm
        if let Some(PdfValue::Array(fields)) = acroform.get("Fields") {
            for field in fields {
                if let PdfValue::Dictionary(field_dict) = field {
                    if self.is_signature_field(field_dict) {
                        // Extract and verify signature
                        let _field_name = self.extract_field_name(field_dict);
                        if let Some(PdfValue::Dictionary(_v_dict)) = field_dict.get("V") {
                            // Verify signature would need document reader
                            // let info = self.verifier.verify_signature(v_dict, &field_name, reader);
                            // self.signatures.push(info);
                        }
                    }
                }
            }
        }
    }

    fn is_signature_field(&self, field: &PdfDictionary) -> bool {
        field
            .get("FT")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash() == "Sig"),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn extract_field_name(&self, field: &PdfDictionary) -> String {
        field
            .get("T")
            .and_then(|v| match v {
                PdfValue::String(s) => Some(s.to_string_lossy()),
                _ => None,
            })
            .unwrap_or_else(|| "Unnamed".to_string())
    }

    pub fn get_signatures(&self) -> &[SignatureInfo] {
        &self.signatures
    }

    pub fn validate_all(&mut self) -> Vec<SignatureValidity> {
        self.signatures
            .iter()
            .map(|sig| sig.validity.clone())
            .collect()
    }

    pub fn has_valid_signatures(&self) -> bool {
        self.signatures
            .iter()
            .any(|sig| sig.validity == SignatureValidity::Valid)
    }

    pub fn get_document_certification_level(&self) -> CertificationLevel {
        // Check for DocMDP signature
        for sig in &self.signatures {
            if sig.field_name.contains("DocMDP") {
                // Would check permissions
                return CertificationLevel::CertifiedNoChanges;
            }
        }

        CertificationLevel::NotCertified
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CertificationLevel {
    NotCertified,
    CertifiedNoChanges,
    CertifiedFormFilling,
    CertifiedFormFillingAndAnnotations,
}
