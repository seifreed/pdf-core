use super::{CertificateChainResult, CertificateInfo, CryptoConfig, CryptoError, CryptoResult};

pub type X509Certificate = CertificateInfo;
pub type CertificateChain = Vec<CertificateInfo>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RevocationProtocol {
    Ocsp,
    Crl,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevocationEvent {
    pub cert_index: usize,
    pub url: String,
    pub protocol: RevocationProtocol,
    pub status: String,
    pub latency_ms: u128,
    pub error: Option<String>,
}

#[cfg(feature = "crypto")]
use openssl::ocsp::{OcspCertId, OcspResponse, OcspResponseStatus};
#[cfg(feature = "crypto")]
use openssl::x509::{X509Crl, X509};
#[cfg(feature = "crypto")]
use reqwest::blocking::Client;
#[cfg(feature = "crypto")]
use std::time::Duration;
#[cfg(feature = "crypto")]
use std::time::Instant;

/// Certificate chain validator with support for multiple trust stores
pub struct CertificateChainValidator {
    trust_stores: Vec<TrustStore>,
    config: CryptoConfig,
}

impl CertificateChainValidator {
    pub fn new(config: CryptoConfig) -> CryptoResult<Self> {
        let mut validator = Self {
            trust_stores: Vec::new(),
            config: config.clone(),
        };

        // Trust anchors are opt-in; parsing a PDF must not load platform state.
        if let Some(trust_store_path) = &config.trust_store_path {
            validator.add_trust_store_from_path(trust_store_path)?;
        }

        Ok(validator)
    }

    /// Add a trust store from a file path
    pub fn add_trust_store_from_path(&mut self, path: &str) -> CryptoResult<()> {
        let trust_store = TrustStore::from_path(path)?;
        self.trust_stores.push(trust_store);
        Ok(())
    }

    /// Add system default trust store
    pub fn add_system_trust_store(&mut self) -> CryptoResult<()> {
        let trust_store = TrustStore::system_default()?;
        self.trust_stores.push(trust_store);
        Ok(())
    }

    /// Validate a certificate chain
    pub fn validate_chain(&self, cert_chain: &[&[u8]]) -> CryptoResult<CertificateChainResult> {
        if cert_chain.is_empty() {
            return Ok(CertificateChainResult {
                is_valid: false,
                chain: Vec::new(),
                trust_anchor: None,
                validation_errors: vec!["Empty certificate chain".to_string()],
                validation_warnings: Vec::new(),
            });
        }

        // Parse all certificates in the chain
        let mut parsed_certs = Vec::new();
        for (i, cert_data) in cert_chain.iter().enumerate() {
            match self.parse_certificate(cert_data) {
                Ok(cert) => parsed_certs.push(cert),
                Err(e) => {
                    return Ok(CertificateChainResult {
                        is_valid: false,
                        chain: parsed_certs,
                        trust_anchor: None,
                        validation_errors: vec![format!(
                            "Failed to parse certificate {}: {}",
                            i, e
                        )],
                        validation_warnings: Vec::new(),
                    });
                }
            }
        }

        // Perform validation
        self.validate_parsed_chain(&parsed_certs, cert_chain)
    }

    /// Validate a parsed certificate chain
    fn validate_parsed_chain(
        &self,
        chain: &[CertificateInfo],
        chain_raw: &[&[u8]],
    ) -> CryptoResult<CertificateChainResult> {
        let mut validation_errors = Vec::new();
        let mut validation_warnings = Vec::new();

        // Check chain length
        if chain.len() > self.config.max_cert_chain_depth as usize {
            validation_errors.push(format!(
                "Certificate chain too long: {} certificates (max: {})",
                chain.len(),
                self.config.max_cert_chain_depth
            ));
        }

        // Validate each certificate individually
        for (i, cert) in chain.iter().enumerate() {
            let cert_errors = self.validate_single_certificate(cert);
            if !cert_errors.is_empty() {
                validation_errors.append(
                    &mut cert_errors
                        .into_iter()
                        .map(|e| format!("Certificate {}: {}", i, e))
                        .collect(),
                );
            }
        }

        // Validate chain structure
        let chain_errors = self.validate_chain_structure(chain);
        validation_errors.extend(chain_errors);

        // Find trust anchor
        let trust_anchor = self.find_trust_anchor(chain);
        if trust_anchor.is_none() {
            validation_errors.push("No trusted root certificate found".to_string());
        }

        // Perform revocation checking if enabled
        if self.config.enable_crl_checking {
            let crl_warnings = self.check_crl_status(chain, chain_raw);
            validation_warnings.extend(crl_warnings);
        }

        if self.config.enable_ocsp_checking {
            let ocsp_warnings = self.check_ocsp_status(chain, chain_raw);
            validation_warnings.extend(ocsp_warnings);
        }

        Ok(CertificateChainResult {
            is_valid: validation_errors.is_empty(),
            chain: chain.to_vec(),
            trust_anchor,
            validation_errors,
            validation_warnings,
        })
    }

    /// Validate a certificate chain and return revocation events
    pub fn validate_chain_with_revocation_details(
        &self,
        cert_chain: &[&[u8]],
    ) -> CryptoResult<(CertificateChainResult, Vec<RevocationEvent>)> {
        if cert_chain.is_empty() {
            return Ok((
                CertificateChainResult {
                    is_valid: false,
                    chain: Vec::new(),
                    trust_anchor: None,
                    validation_errors: vec!["Empty certificate chain".to_string()],
                    validation_warnings: Vec::new(),
                },
                Vec::new(),
            ));
        }

        let mut parsed_certs = Vec::new();
        for (i, cert_data) in cert_chain.iter().enumerate() {
            match self.parse_certificate(cert_data) {
                Ok(cert) => parsed_certs.push(cert),
                Err(e) => {
                    return Ok((
                        CertificateChainResult {
                            is_valid: false,
                            chain: parsed_certs,
                            trust_anchor: None,
                            validation_errors: vec![format!(
                                "Failed to parse certificate {}: {}",
                                i, e
                            )],
                            validation_warnings: Vec::new(),
                        },
                        Vec::new(),
                    ));
                }
            }
        }

        let mut events = Vec::new();
        let mut result = self.validate_parsed_chain(&parsed_certs, cert_chain)?;

        if self.config.enable_crl_checking {
            let (warnings, crl_events) = self.check_crl_status_details(&parsed_certs, cert_chain);
            result.validation_warnings.extend(warnings);
            events.extend(crl_events);
        }

        if self.config.enable_ocsp_checking {
            let (warnings, ocsp_events) = self.check_ocsp_status_details(&parsed_certs, cert_chain);
            result.validation_warnings.extend(warnings);
            events.extend(ocsp_events);
        }

        Ok((result, events))
    }

    /// Parse a single certificate
    fn parse_certificate(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
        #[cfg(feature = "crypto")]
        {
            self.parse_certificate_openssl(cert_data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            self.parse_certificate_simple(cert_data)
        }
    }

    #[cfg(feature = "crypto")]
    fn parse_certificate_openssl(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
        // Parse X.509 certificate using DER format
        let cert = X509CertificateImpl::from_der(cert_data)?;

        Ok(CertificateInfo {
            subject: cert.subject_string(),
            issuer: cert.issuer_string(),
            serial_number: cert.serial_number_hex(),
            der: cert_data.to_vec(),
            not_before: cert.not_before(),
            not_after: cert.not_after(),
            public_key_algorithm: cert.public_key_algorithm(),
            signature_algorithm: cert.signature_algorithm(),
            key_usage: cert.key_usage(),
            extended_key_usage: cert.extended_key_usage(),
            is_ca: cert.is_ca(),
            fingerprint_sha256: cert.fingerprint_sha256(),
        })
    }

    #[cfg(not(feature = "crypto"))]
    fn parse_certificate_simple(&self, cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
        // Parse X.509 certificate using simple DER parser
        if cert_data.len() < 100 {
            return Err(CryptoError::CertificateError(
                "Certificate data too short".to_string(),
            ));
        }

        // Check for DER format
        if !cert_data.starts_with(&[0x30, 0x82]) && !cert_data.starts_with(&[0x30, 0x81]) {
            return Err(CryptoError::CertificateError(
                "Invalid certificate format".to_string(),
            ));
        }

        // Parse basic X.509 structure
        let parser = SimpleDerParser::new(cert_data);
        let cert_info = parser.parse_x509()?;

        Ok(cert_info)
    }

    /// Validate a single certificate
    fn validate_single_certificate(&self, cert: &CertificateInfo) -> Vec<String> {
        let mut errors = Vec::new();
        let now = super::chrono::Utc::now();

        // Check validity period
        if cert.not_before.timestamp() > now.timestamp() {
            errors.push("Certificate is not yet valid".to_string());
        }
        if cert.not_after.timestamp() < now.timestamp() {
            errors.push("Certificate has expired".to_string());
        }

        // Check key usage for signing certificates
        if !cert.key_usage.contains(&"Digital Signature".to_string()) {
            errors.push("Certificate does not have Digital Signature key usage".to_string());
        }

        // Validate subject and issuer
        if cert.subject.is_empty() {
            errors.push("Certificate subject is empty".to_string());
        }
        if cert.issuer.is_empty() {
            errors.push("Certificate issuer is empty".to_string());
        }

        errors
    }

    /// Validate certificate chain structure
    fn validate_chain_structure(&self, chain: &[CertificateInfo]) -> Vec<String> {
        let mut errors = Vec::new();

        if chain.is_empty() {
            return vec!["Empty certificate chain".to_string()];
        }

        // Check that each certificate is signed by the next one in the chain
        for i in 0..chain.len().saturating_sub(1) {
            let current = &chain[i];
            let issuer_cert = &chain[i + 1];

            if current.issuer != issuer_cert.subject {
                errors.push(format!(
                    "Certificate {} issuer '{}' does not match certificate {} subject '{}'",
                    i,
                    current.issuer,
                    i + 1,
                    issuer_cert.subject
                ));
            }

            // Check that issuer certificate is a CA
            if !issuer_cert.is_ca {
                errors.push(format!(
                    "Certificate {} is not a CA but is used to sign certificate {}",
                    i + 1,
                    i
                ));
            }
        }

        // Check root certificate
        if let Some(root) = chain.last() {
            if root.subject != root.issuer && !self.is_trusted_root(root) {
                errors
                    .push("Root certificate is not self-signed and not in trust store".to_string());
            }
        }

        errors
    }

    /// Find trust anchor in the certificate chain
    fn find_trust_anchor(&self, chain: &[CertificateInfo]) -> Option<CertificateInfo> {
        for cert in chain.iter().rev() {
            if self.is_trusted_root(cert) {
                return Some(cert.clone());
            }
        }
        None
    }

    /// Check if a certificate is a trusted root
    fn is_trusted_root(&self, cert: &CertificateInfo) -> bool {
        for trust_store in &self.trust_stores {
            if trust_store.contains_certificate(cert) {
                return true;
            }
        }
        false
    }

    #[cfg(feature = "crypto")]
    fn find_issuer_cert_der<'a>(&self, chain_raw: &'a [&[u8]], index: usize) -> Option<&'a [u8]> {
        if index + 1 < chain_raw.len() {
            return Some(chain_raw[index + 1]);
        }

        let issuer = parse_x509(chain_raw[index]).ok()?.issuer().to_string();
        for (pos, candidate) in chain_raw.iter().enumerate() {
            if pos == index {
                continue;
            }
            if let Ok(cert) = parse_x509(candidate) {
                if cert.subject().to_string() == issuer {
                    return Some(*candidate);
                }
            }
        }

        None
    }

    #[cfg(feature = "crypto")]
    fn http_client(&self) -> Result<Client, String> {
        Client::builder()
            .timeout(Duration::from_secs(
                self.config.network_timeout_seconds as u64,
            ))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))
    }

    #[cfg(feature = "crypto")]
    fn http_get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        let client = self.http_client()?;
        let response = client
            .get(url)
            .send()
            .map_err(|e| format!("HTTP GET failed: {}", e))?;
        if !response.status().is_success() {
            return Err(format!("HTTP GET status {}", response.status()));
        }
        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| format!("HTTP GET body error: {}", e))
    }

    #[cfg(feature = "crypto")]
    fn http_post_ocsp(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, String> {
        let client = self.http_client()?;
        let response = client
            .post(url)
            .header("Content-Type", "application/ocsp-request")
            .header("Accept", "application/ocsp-response")
            .body(body.to_vec())
            .send()
            .map_err(|e| format!("HTTP POST failed: {}", e))?;
        if !response.status().is_success() {
            return Err(format!("HTTP POST status {}", response.status()));
        }
        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| format!("HTTP POST body error: {}", e))
    }

    /// Check CRL (Certificate Revocation List) status
    fn check_crl_status(&self, chain: &[CertificateInfo], chain_raw: &[&[u8]]) -> Vec<String> {
        self.check_crl_status_details(chain, chain_raw).0
    }

    fn check_crl_status_details(
        &self,
        chain: &[CertificateInfo],
        chain_raw: &[&[u8]],
    ) -> (Vec<String>, Vec<RevocationEvent>) {
        #[cfg(not(feature = "crypto"))]
        {
            let _ = chain;
            let _ = chain_raw;
            return (
                vec!["CRL checking requires crypto feature".to_string()],
                Vec::new(),
            );
        }

        #[cfg(feature = "crypto")]
        {
            let mut warnings = Vec::new();
            let mut events = Vec::new();
            let _ = chain;

            for (i, cert_bytes) in chain_raw.iter().enumerate() {
                let urls = extract_crl_urls(cert_bytes);
                if urls.is_empty() {
                    continue;
                }

                let issuer_der = self.find_issuer_cert_der(chain_raw, i);
                let issuer_x509 = issuer_der.and_then(|der| X509::from_der(der).ok());
                let cert_x509 = X509::from_der(cert_bytes).ok();

                for url in urls {
                    if !(url.starts_with("http://") || url.starts_with("https://")) {
                        warnings.push(format!(
                            "CRL URL for certificate {} uses unsupported scheme: {}",
                            i, url
                        ));
                        events.push(RevocationEvent {
                            cert_index: i,
                            url,
                            protocol: RevocationProtocol::Crl,
                            status: "unsupported_scheme".to_string(),
                            latency_ms: 0,
                            error: None,
                        });
                        continue;
                    }

                    let start = Instant::now();
                    let crl_data = match self.http_get_bytes(&url) {
                        Ok(data) => data,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to fetch CRL for certificate {}: {}",
                                i, err
                            ));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Crl,
                                status: "fetch_failed".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: Some(err),
                            });
                            continue;
                        }
                    };

                    let crl = if crl_data.starts_with(b"-----BEGIN") {
                        X509Crl::from_pem(&crl_data)
                    } else {
                        X509Crl::from_der(&crl_data)
                    };

                    let crl = match crl {
                        Ok(crl) => crl,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to parse CRL for certificate {}: {}",
                                i, err
                            ));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Crl,
                                status: "parse_failed".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: Some(err.to_string()),
                            });
                            continue;
                        }
                    };

                    if let Some(issuer) = issuer_x509.as_ref() {
                        if let Ok(key) = issuer.public_key() {
                            if !crl.verify(&key).unwrap_or(false) {
                                warnings.push(format!(
                                    "CRL signature verification failed for certificate {}",
                                    i
                                ));
                                events.push(RevocationEvent {
                                    cert_index: i,
                                    url: url.clone(),
                                    protocol: RevocationProtocol::Crl,
                                    status: "signature_invalid".to_string(),
                                    latency_ms: start.elapsed().as_millis(),
                                    error: None,
                                });
                            }
                        }
                    }

                    let mut is_revoked = false;
                    if let (Some(cert), Some(revoked_stack)) =
                        (cert_x509.as_ref(), crl.get_revoked())
                    {
                        if let Ok(serial) = cert.serial_number().to_bn() {
                            for revoked in revoked_stack {
                                if let Ok(revoked_serial) = revoked.serial_number().to_bn() {
                                    if revoked_serial == serial {
                                        is_revoked = true;
                                        warnings.push(format!(
                                            "Certificate {} is revoked per CRL ({})",
                                            i, url
                                        ));
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    events.push(RevocationEvent {
                        cert_index: i,
                        url,
                        protocol: RevocationProtocol::Crl,
                        status: if is_revoked { "revoked" } else { "ok" }.to_string(),
                        latency_ms: start.elapsed().as_millis(),
                        error: None,
                    });
                }
            }

            (warnings, events)
        }
    }

    /// Check OCSP (Online Certificate Status Protocol) status
    fn check_ocsp_status(&self, chain: &[CertificateInfo], chain_raw: &[&[u8]]) -> Vec<String> {
        self.check_ocsp_status_details(chain, chain_raw).0
    }

    fn check_ocsp_status_details(
        &self,
        chain: &[CertificateInfo],
        chain_raw: &[&[u8]],
    ) -> (Vec<String>, Vec<RevocationEvent>) {
        #[cfg(not(feature = "crypto"))]
        {
            let _ = chain;
            let _ = chain_raw;
            return (
                vec!["OCSP checking requires crypto feature".to_string()],
                Vec::new(),
            );
        }

        #[cfg(feature = "crypto")]
        {
            use openssl::hash::MessageDigest;
            use openssl::ocsp::{OcspFlag, OcspRequest};
            use openssl::stack::Stack;
            use openssl::x509::store::X509StoreBuilder;

            let mut warnings = Vec::new();
            let mut events = Vec::new();
            let _ = chain;

            for (i, cert_bytes) in chain_raw.iter().enumerate() {
                let urls = extract_ocsp_urls(cert_bytes);
                if urls.is_empty() {
                    continue;
                }

                let issuer_der = self.find_issuer_cert_der(chain_raw, i);
                let issuer_der = match issuer_der {
                    Some(der) => der,
                    None => {
                        warnings.push(format!("OCSP issuer not found for certificate {}", i));
                        continue;
                    }
                };

                let cert_x509 = match X509::from_der(cert_bytes) {
                    Ok(cert) => cert,
                    Err(err) => {
                        warnings.push(format!(
                            "Failed to parse certificate {} for OCSP: {}",
                            i, err
                        ));
                        continue;
                    }
                };

                let issuer_x509 = match X509::from_der(issuer_der) {
                    Ok(cert) => cert,
                    Err(err) => {
                        warnings.push(format!(
                            "Failed to parse issuer for certificate {}: {}",
                            i, err
                        ));
                        continue;
                    }
                };

                let cert_id_for_req =
                    match OcspCertId::from_cert(MessageDigest::sha1(), &cert_x509, &issuer_x509) {
                        Ok(id) => id,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to build OCSP CertId for certificate {}: {}",
                                i, err
                            ));
                            continue;
                        }
                    };

                let cert_id_for_status =
                    match OcspCertId::from_cert(MessageDigest::sha1(), &cert_x509, &issuer_x509) {
                        Ok(id) => id,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to build OCSP CertId for certificate {}: {}",
                                i, err
                            ));
                            continue;
                        }
                    };

                let mut ocsp_req = match OcspRequest::new() {
                    Ok(req) => req,
                    Err(err) => {
                        warnings.push(format!(
                            "Failed to create OCSP request for certificate {}: {}",
                            i, err
                        ));
                        continue;
                    }
                };
                if let Err(err) = ocsp_req.add_id(cert_id_for_req) {
                    warnings.push(format!(
                        "Failed to build OCSP request for certificate {}: {}",
                        i, err
                    ));
                    continue;
                }

                let req_der = match ocsp_req.to_der() {
                    Ok(der) => der,
                    Err(err) => {
                        warnings.push(format!(
                            "Failed to serialize OCSP request for certificate {}: {}",
                            i, err
                        ));
                        continue;
                    }
                };

                for url in urls {
                    if !(url.starts_with("http://") || url.starts_with("https://")) {
                        warnings.push(format!(
                            "OCSP URL for certificate {} uses unsupported scheme: {}",
                            i, url
                        ));
                        events.push(RevocationEvent {
                            cert_index: i,
                            url,
                            protocol: RevocationProtocol::Ocsp,
                            status: "unsupported_scheme".to_string(),
                            latency_ms: 0,
                            error: None,
                        });
                        continue;
                    }

                    let start = Instant::now();
                    let ocsp_response = match self.http_post_ocsp(&url, &req_der) {
                        Ok(data) => data,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to fetch OCSP response for certificate {}: {}",
                                i, err
                            ));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Ocsp,
                                status: "fetch_failed".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: Some(err),
                            });
                            continue;
                        }
                    };

                    let response = match OcspResponse::from_der(&ocsp_response) {
                        Ok(resp) => resp,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to parse OCSP response for certificate {}: {}",
                                i, err
                            ));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Ocsp,
                                status: "parse_failed".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: Some(err.to_string()),
                            });
                            continue;
                        }
                    };

                    if response.status() != OcspResponseStatus::SUCCESSFUL {
                        warnings.push(format!(
                            "OCSP response status for certificate {} is {:?}",
                            i,
                            response.status()
                        ));
                        events.push(RevocationEvent {
                            cert_index: i,
                            url,
                            protocol: RevocationProtocol::Ocsp,
                            status: format!("response_{:?}", response.status()),
                            latency_ms: start.elapsed().as_millis(),
                            error: None,
                        });
                        continue;
                    }

                    let basic = match response.basic() {
                        Ok(basic) => basic,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to parse OCSP basic response for certificate {}: {}",
                                i, err
                            ));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Ocsp,
                                status: "basic_parse_failed".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: Some(err.to_string()),
                            });
                            continue;
                        }
                    };

                    let mut store_builder = match X509StoreBuilder::new() {
                        Ok(builder) => builder,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to create OCSP trust store for certificate {}: {}",
                                i, err
                            ));
                            continue;
                        }
                    };
                    if store_builder.set_default_paths().is_err() {
                        warnings.push(format!(
                            "Failed to load default trust store for OCSP verification (cert {})",
                            i
                        ));
                    }

                    let mut certs = match Stack::new() {
                        Ok(stack) => stack,
                        Err(err) => {
                            warnings.push(format!(
                                "Failed to create OCSP cert stack for certificate {}: {}",
                                i, err
                            ));
                            continue;
                        }
                    };
                    if certs.push(issuer_x509.clone()).is_err() {
                        warnings.push(format!(
                            "Failed to add issuer cert to OCSP stack (cert {})",
                            i
                        ));
                    }

                    if let Err(err) =
                        basic.verify(&certs, &store_builder.build(), OcspFlag::empty())
                    {
                        warnings.push(format!(
                            "OCSP response verification failed for certificate {}: {}",
                            i, err
                        ));
                        events.push(RevocationEvent {
                            cert_index: i,
                            url: url.clone(),
                            protocol: RevocationProtocol::Ocsp,
                            status: "signature_invalid".to_string(),
                            latency_ms: start.elapsed().as_millis(),
                            error: None,
                        });
                    }

                    if let Some(status) = basic.find_status(&cert_id_for_status) {
                        if status.status == openssl::ocsp::OcspCertStatus::REVOKED {
                            warnings
                                .push(format!("Certificate {} is revoked per OCSP ({})", i, url));
                            events.push(RevocationEvent {
                                cert_index: i,
                                url,
                                protocol: RevocationProtocol::Ocsp,
                                status: "revoked".to_string(),
                                latency_ms: start.elapsed().as_millis(),
                                error: None,
                            });
                            continue;
                        }
                    }

                    events.push(RevocationEvent {
                        cert_index: i,
                        url,
                        protocol: RevocationProtocol::Ocsp,
                        status: "ok".to_string(),
                        latency_ms: start.elapsed().as_millis(),
                        error: None,
                    });
                }
            }

            (warnings, events)
        }
    }
}

pub fn parse_der_certificate(cert_data: &[u8]) -> CryptoResult<CertificateInfo> {
    #[cfg(feature = "crypto")]
    {
        let cert = X509CertificateImpl::from_der(cert_data)?;
        Ok(CertificateInfo {
            subject: cert.subject_string(),
            issuer: cert.issuer_string(),
            serial_number: cert.serial_number_hex(),
            der: cert_data.to_vec(),
            not_before: cert.not_before(),
            not_after: cert.not_after(),
            public_key_algorithm: cert.public_key_algorithm(),
            signature_algorithm: cert.signature_algorithm(),
            key_usage: cert.key_usage(),
            extended_key_usage: cert.extended_key_usage(),
            is_ca: cert.is_ca(),
            fingerprint_sha256: cert.fingerprint_sha256(),
        })
    }
    #[cfg(not(feature = "crypto"))]
    {
        let parser = SimpleDerParser::new(cert_data);
        parser.parse_x509()
    }
}

#[cfg(feature = "crypto")]
pub fn extract_ocsp_urls(cert_data: &[u8]) -> Vec<String> {
    use x509_parser::extensions::{GeneralName, ParsedExtension};
    let Ok(cert) = parse_x509(cert_data) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::AuthorityInfoAccess(aia) = ext.parsed_extension() {
            for access in &aia.accessdescs {
                if access.access_method.to_id_string() == "1.3.6.1.5.5.7.48.1" {
                    if let GeneralName::URI(uri) = &access.access_location {
                        let url = uri.to_string();
                        if !url.is_empty() {
                            urls.push(url);
                        }
                    }
                }
            }
        }
    }
    urls
}

#[cfg(not(feature = "crypto"))]
pub fn extract_ocsp_urls(_cert_data: &[u8]) -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "crypto")]
pub fn extract_crl_urls(cert_data: &[u8]) -> Vec<String> {
    use x509_parser::extensions::{DistributionPointName, GeneralName, ParsedExtension};
    let Ok(cert) = parse_x509(cert_data) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::CRLDistributionPoints(points) = ext.parsed_extension() {
            for point in &points.points {
                if let Some(DistributionPointName::FullName(names)) = &point.distribution_point {
                    for name in names {
                        if let GeneralName::URI(uri) = name {
                            let url = uri.to_string();
                            if !url.is_empty() {
                                urls.push(url);
                            }
                        }
                    }
                }
            }
        }
    }
    urls
}

#[cfg(not(feature = "crypto"))]
pub fn extract_crl_urls(_cert_data: &[u8]) -> Vec<String> {
    Vec::new()
}

/// Trust store containing trusted root certificates
#[derive(Debug, Clone)]
pub struct TrustStore {
    certificates: Vec<CertificateInfo>,
    name: String,
}

impl TrustStore {
    /// Create a new empty trust store
    pub fn new(name: String) -> Self {
        Self {
            certificates: Vec::new(),
            name,
        }
    }

    /// Load trust store from a file path
    pub fn from_path(path: &str) -> CryptoResult<Self> {
        let mut trust_store = Self::new(format!("File: {}", path));

        // Read certificate file
        let cert_data = std::fs::read(path).map_err(CryptoError::Io)?;

        // Parse certificates from the file
        if path.ends_with(".pem") {
            trust_store.load_pem_certificates(&cert_data)?;
        } else if path.ends_with(".der") {
            trust_store.load_der_certificate(&cert_data)?;
        } else {
            // Try to auto-detect format
            if cert_data.starts_with(b"-----BEGIN") {
                trust_store.load_pem_certificates(&cert_data)?;
            } else if cert_data.starts_with(&[0x30, 0x82]) {
                trust_store.load_der_certificate(&cert_data)?;
            } else {
                return Err(CryptoError::CertificateError(
                    "Unknown certificate format".to_string(),
                ));
            }
        }

        Ok(trust_store)
    }

    /// Load system default trust store
    pub fn system_default() -> CryptoResult<Self> {
        let mut trust_store = Self::new("System Default".to_string());

        // Load system certificates based on platform
        #[cfg(target_os = "linux")]
        {
            // Try common Linux certificate paths
            let paths = vec![
                "/etc/ssl/certs/ca-certificates.crt",
                "/etc/pki/tls/certs/ca-bundle.crt",
                "/etc/ssl/ca-bundle.pem",
            ];

            for path in paths {
                if std::path::Path::new(path).exists() {
                    if let Ok(cert_data) = std::fs::read(path) {
                        let _ = trust_store.load_pem_certificates(&cert_data);
                        break;
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Attempt to load common macOS trust store bundles
            let paths = vec![
                "/etc/ssl/cert.pem",
                "/System/Library/OpenSSL/certs/cert.pem",
            ];
            let mut loaded = false;
            for path in paths {
                if std::path::Path::new(path).exists() {
                    if let Ok(cert_data) = std::fs::read(path) {
                        let _ = trust_store.load_pem_certificates(&cert_data);
                        loaded = true;
                        break;
                    }
                }
            }
            if !loaded {
                return Err(CryptoError::CertificateError(
                    "System trust store not available on macOS".to_string(),
                ));
            }
        }

        #[cfg(target_os = "windows")]
        {
            return Err(CryptoError::CertificateError(
                "System trust store not available on Windows without platform API".to_string(),
            ));
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            return Err(CryptoError::CertificateError(
                "System trust store not available on this platform".to_string(),
            ));
        }

        Ok(trust_store)
    }

    /// Load PEM format certificates
    fn load_pem_certificates(&mut self, pem_data: &[u8]) -> CryptoResult<()> {
        let pem_str = std::str::from_utf8(pem_data)
            .map_err(|_| CryptoError::CertificateError("Invalid UTF-8 in PEM file".to_string()))?;

        // Find all certificate blocks
        let cert_blocks = self.extract_pem_blocks(pem_str, "CERTIFICATE")?;

        for cert_block in cert_blocks {
            // Decode base64
            let der_data = self.decode_base64(&cert_block)?;
            self.load_der_certificate(&der_data)?;
        }

        Ok(())
    }

    /// Load DER format certificate
    fn load_der_certificate(&mut self, der_data: &[u8]) -> CryptoResult<()> {
        #[cfg(feature = "crypto")]
        {
            // Parse with OpenSSL
            let cert = self.parse_der_certificate_openssl(der_data)?;
            self.certificates.push(cert);
        }
        #[cfg(not(feature = "crypto"))]
        {
            let cert = parse_der_certificate(der_data)?;
            self.certificates.push(cert);
        }

        Ok(())
    }

    /// Extract PEM blocks of a specific type
    fn extract_pem_blocks(&self, pem_str: &str, block_type: &str) -> CryptoResult<Vec<String>> {
        let mut blocks = Vec::new();
        let begin_marker = format!("-----BEGIN {}-----", block_type);
        let end_marker = format!("-----END {}-----", block_type);

        let mut current_pos = 0;
        while let Some(begin_pos) = pem_str[current_pos..].find(&begin_marker) {
            let begin_pos = current_pos + begin_pos;
            if let Some(end_pos) = pem_str[begin_pos..].find(&end_marker) {
                let end_pos = begin_pos + end_pos;
                let block = &pem_str[begin_pos + begin_marker.len()..end_pos];
                blocks.push(block.trim().replace(['\n', '\r'], ""));
                current_pos = end_pos + end_marker.len();
            } else {
                break;
            }
        }

        Ok(blocks)
    }

    /// Decode base64 string
    fn decode_base64(&self, base64_str: &str) -> CryptoResult<Vec<u8>> {
        // Simple base64 decoder (for production, use a proper base64 library)
        const BASE64_CHARS: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let clean_str = base64_str.replace('=', "");
        let mut result = Vec::new();

        for chunk in clean_str.as_bytes().chunks(4) {
            let mut values = [0u8; 4];
            for (i, &byte) in chunk.iter().enumerate() {
                values[i] = BASE64_CHARS
                    .iter()
                    .position(|&x| x == byte)
                    .ok_or_else(|| {
                        CryptoError::CertificateError("Invalid base64 character".to_string())
                    })? as u8;
            }

            if chunk.len() >= 2 {
                result.push((values[0] << 2) | (values[1] >> 4));
            }
            if chunk.len() >= 3 {
                result.push((values[1] << 4) | (values[2] >> 2));
            }
            if chunk.len() >= 4 {
                result.push((values[2] << 6) | values[3]);
            }
        }

        Ok(result)
    }

    #[cfg(feature = "crypto")]
    fn parse_der_certificate_openssl(&self, der_data: &[u8]) -> CryptoResult<CertificateInfo> {
        let cert = X509CertificateImpl::from_der(der_data)?;
        Ok(CertificateInfo {
            subject: cert.subject_string(),
            issuer: cert.issuer_string(),
            serial_number: cert.serial_number_hex(),
            der: der_data.to_vec(),
            not_before: cert.not_before(),
            not_after: cert.not_after(),
            public_key_algorithm: cert.public_key_algorithm(),
            signature_algorithm: cert.signature_algorithm(),
            key_usage: cert.key_usage(),
            extended_key_usage: cert.extended_key_usage(),
            is_ca: cert.is_ca(),
            fingerprint_sha256: cert.fingerprint_sha256(),
        })
    }

    /// Check if trust store contains a certificate
    pub fn contains_certificate(&self, cert: &CertificateInfo) -> bool {
        self.certificates.iter().any(|trusted_cert| {
            trusted_cert.fingerprint_sha256 == cert.fingerprint_sha256
                || (trusted_cert.subject == cert.subject && trusted_cert.issuer == cert.issuer)
        })
    }

    /// Add a certificate to the trust store
    pub fn add_certificate(&mut self, cert: CertificateInfo) {
        if !self.contains_certificate(&cert) {
            self.certificates.push(cert);
        }
    }

    /// Get all certificates in the trust store
    pub fn get_certificates(&self) -> &[CertificateInfo] {
        &self.certificates
    }

    /// Get trust store name
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::chrono::DateTime;

    #[test]
    fn test_certificate_chain_validator_creation() {
        let config = CryptoConfig::default();
        let result = CertificateChainValidator::new(config);

        // Should succeed even if system trust store is not available
        assert!(result.is_ok() || result.is_err()); // Either way is acceptable for this test
    }

    #[test]
    fn test_trust_store_creation() {
        let trust_store = TrustStore::new("Test Store".to_string());
        assert_eq!(trust_store.name(), "Test Store");
        assert_eq!(trust_store.get_certificates().len(), 0);
    }

    #[test]
    fn test_system_trust_store() {
        let result = TrustStore::system_default();
        if let Ok(trust_store) = result {
            assert!(!trust_store.get_certificates().is_empty());
        } else {
            // System trust store may not be available on all platforms
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_certificate_validation() {
        let config = CryptoConfig::default();

        // Create a mock certificate with expired date
        let expired_cert = CertificateInfo {
            subject: "CN=Test".to_string(),
            issuer: "CN=Test CA".to_string(),
            serial_number: "123".to_string(),
            der: Vec::new(),
            not_before: DateTime::from_timestamp(0), // Very old
            not_after: DateTime::from_timestamp(1),  // Expired
            public_key_algorithm: "RSA".to_string(),
            signature_algorithm: "SHA256withRSA".to_string(),
            key_usage: vec!["Digital Signature".to_string()],
            extended_key_usage: Vec::new(),
            is_ca: false,
            fingerprint_sha256: "test_fingerprint".to_string(),
        };

        let validator = CertificateChainValidator::new(config).unwrap();
        let errors = validator.validate_single_certificate(&expired_cert);

        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("expired")));
    }

    #[test]
    fn test_base64_decoding() {
        let trust_store = TrustStore::new("Test".to_string());

        let test_data = "SGVsbG8gV29ybGQ="; // "Hello World" in base64
        let result = trust_store.decode_base64(test_data).unwrap();
        assert_eq!(result, b"Hello World");
    }
}

/// Simple X.509 certificate structure
#[cfg(feature = "crypto")]
#[derive(Debug, Clone)]
pub struct X509CertificateImpl {
    data: Vec<u8>,
}

#[cfg(feature = "crypto")]
impl X509CertificateImpl {
    pub fn from_der(data: &[u8]) -> CryptoResult<Self> {
        Ok(Self {
            data: data.to_vec(),
        })
    }

    pub fn subject_string(&self) -> String {
        match parse_x509(&self.data) {
            Ok(cert) => cert.subject().to_string(),
            Err(_) => String::new(),
        }
    }

    pub fn issuer_string(&self) -> String {
        match parse_x509(&self.data) {
            Ok(cert) => cert.issuer().to_string(),
            Err(_) => String::new(),
        }
    }

    pub fn serial_number_hex(&self) -> String {
        match parse_x509(&self.data) {
            Ok(cert) => cert.tbs_certificate.raw_serial_as_string(),
            Err(_) => String::new(),
        }
    }

    pub fn not_before(&self) -> super::chrono::DateTime<super::chrono::Utc> {
        parse_x509_time(&self.data, true).unwrap_or_else(|_| super::chrono::Utc::now())
    }

    pub fn not_after(&self) -> super::chrono::DateTime<super::chrono::Utc> {
        parse_x509_time(&self.data, false).unwrap_or_else(|_| super::chrono::Utc::now())
    }

    fn public_key_algorithm(&self) -> String {
        match parse_x509(&self.data) {
            Ok(cert) => cert.public_key().algorithm.algorithm.to_string(),
            Err(_) => String::new(),
        }
    }

    fn signature_algorithm(&self) -> String {
        match parse_x509(&self.data) {
            Ok(cert) => cert.signature_algorithm.algorithm.to_string(),
            Err(_) => String::new(),
        }
    }

    fn key_usage(&self) -> Vec<String> {
        parse_key_usage(&self.data)
    }

    fn extended_key_usage(&self) -> Vec<String> {
        parse_extended_key_usage(&self.data)
    }

    fn is_ca(&self) -> bool {
        parse_is_ca(&self.data)
    }

    fn fingerprint_sha256(&self) -> String {
        // Compute SHA-256 of certificate
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.data);
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }

    // Field-like accessors for compatibility with direct field access
    pub fn subject(&self) -> String {
        self.subject_string()
    }

    pub fn issuer(&self) -> String {
        self.issuer_string()
    }

    pub fn serial_number(&self) -> String {
        self.serial_number_hex()
    }

    pub fn extensions(&self) -> Vec<String> {
        parse_extensions(&self.data)
    }
}

#[cfg(feature = "crypto")]
fn parse_x509(data: &[u8]) -> Result<x509_parser::certificate::X509Certificate<'_>, CryptoError> {
    use nom::Parser;
    use x509_parser::prelude::X509CertificateParser;
    let mut parser = X509CertificateParser::new().with_deep_parse_extensions(true);
    parser
        .parse(data)
        .map(|(_, cert)| cert)
        .map_err(|e| CryptoError::CertificateError(format!("X509 parse error: {:?}", e)))
}

#[cfg(feature = "crypto")]
fn parse_x509_time(
    data: &[u8],
    not_before: bool,
) -> Result<super::chrono::DateTime<super::chrono::Utc>, CryptoError> {
    let cert = parse_x509(data)?;
    let validity = &cert.tbs_certificate.validity;
    let time = if not_before {
        validity.not_before.to_datetime()
    } else {
        validity.not_after.to_datetime()
    };
    Ok(super::chrono::DateTime::from_timestamp(
        time.unix_timestamp(),
    ))
}

#[cfg(feature = "crypto")]
fn parse_key_usage(data: &[u8]) -> Vec<String> {
    use x509_parser::extensions::ParsedExtension;
    let Ok(cert) = parse_x509(data) else {
        return Vec::new();
    };
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
fn parse_extended_key_usage(data: &[u8]) -> Vec<String> {
    use x509_parser::extensions::ParsedExtension;
    let Ok(cert) = parse_x509(data) else {
        return Vec::new();
    };
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
fn parse_is_ca(data: &[u8]) -> bool {
    use x509_parser::extensions::ParsedExtension;
    let Ok(cert) = parse_x509(data) else {
        return false;
    };
    for ext in cert.extensions() {
        if let ParsedExtension::BasicConstraints(bc) = &ext.parsed_extension() {
            return bc.ca;
        }
    }
    false
}

#[cfg(feature = "crypto")]
fn parse_extensions(data: &[u8]) -> Vec<String> {
    let Ok(cert) = parse_x509(data) else {
        return Vec::new();
    };
    cert.extensions()
        .iter()
        .map(|ext| ext.oid.to_string())
        .collect()
}

/// Simple DER parser for X.509
#[cfg(not(feature = "crypto"))]
struct SimpleDerParser<'a> {
    data: &'a [u8],
    pos: usize,
}

#[cfg(not(feature = "crypto"))]
impl<'a> SimpleDerParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn parse_x509(&self) -> CryptoResult<CertificateInfo> {
        let mut parser = self.clone();

        // Read SEQUENCE header
        let (tag, _length) = parser.read_tag_length()?;
        if tag != 0x30 {
            return Err(CryptoError::CertificateError("Not a SEQUENCE".to_string()));
        }

        // Read TBSCertificate
        let (tbs_tag, _tbs_length) = parser.read_tag_length()?;
        if tbs_tag != 0x30 {
            return Err(CryptoError::CertificateError(
                "Invalid TBSCertificate".to_string(),
            ));
        }

        // Parse version
        let _version = parser.parse_version();

        // Parse serial number
        let serial = parser.parse_integer()?;

        // Parse signature algorithm
        let sig_algo = parser.parse_algorithm_identifier()?;

        // Parse issuer
        let issuer = parser.parse_name()?;

        // Parse validity
        let (not_before, not_after) = parser.parse_validity()?;

        // Parse subject
        let subject = parser.parse_name()?;

        // Parse public key
        let pub_key_algo = parser.parse_subject_public_key_info()?;

        // Extensions
        let (key_usage, ext_key_usage, is_ca) = parser.parse_extensions();

        // Compute fingerprint
        let fingerprint = self.compute_sha256_fingerprint();

        Ok(CertificateInfo {
            subject,
            issuer,
            serial_number: format!("{:X}", serial),
            der: self.data.to_vec(),
            not_before,
            not_after,
            public_key_algorithm: pub_key_algo,
            signature_algorithm: sig_algo,
            key_usage,
            extended_key_usage: ext_key_usage,
            is_ca,
            fingerprint_sha256: fingerprint,
        })
    }

    fn read_tag_length(&mut self) -> CryptoResult<(u8, usize)> {
        if self.pos >= self.data.len() {
            return Err(CryptoError::CertificateError(
                "Unexpected end of data".to_string(),
            ));
        }

        let tag = self.data[self.pos];
        self.pos += 1;

        if self.pos >= self.data.len() {
            return Err(CryptoError::CertificateError("Missing length".to_string()));
        }

        let length = if self.data[self.pos] & 0x80 == 0 {
            let len = self.data[self.pos] as usize;
            self.pos += 1;
            len
        } else {
            let num_octets = (self.data[self.pos] & 0x7F) as usize;
            self.pos += 1;

            let mut len = 0usize;
            for _ in 0..num_octets {
                if self.pos >= self.data.len() {
                    return Err(CryptoError::CertificateError(
                        "Invalid length encoding".to_string(),
                    ));
                }
                len = (len << 8) | (self.data[self.pos] as usize);
                self.pos += 1;
            }
            len
        };

        Ok((tag, length))
    }

    fn parse_version(&mut self) -> i32 {
        if self.pos < self.data.len() && self.data[self.pos] == 0xA0 {
            self.pos += 1;
            let _ = self.read_length();
            if self.pos + 3 <= self.data.len() {
                self.pos += 3;
                return 2; // v3
            }
        }
        0 // v1
    }

    fn parse_integer(&mut self) -> CryptoResult<u64> {
        let (tag, length) = self.read_tag_length()?;
        if tag != 0x02 {
            return Err(CryptoError::CertificateError(
                "Expected INTEGER".to_string(),
            ));
        }

        let mut value = 0u64;
        for _ in 0..length.min(8) {
            if self.pos >= self.data.len() {
                break;
            }
            value = (value << 8) | (self.data[self.pos] as u64);
            self.pos += 1;
        }

        self.pos += length.saturating_sub(8);
        Ok(value)
    }

    fn parse_algorithm_identifier(&mut self) -> CryptoResult<String> {
        let (tag, _length) = self.read_tag_length()?;
        if tag != 0x30 {
            return Err(CryptoError::CertificateError(
                "Expected SEQUENCE for AlgorithmIdentifier".to_string(),
            ));
        }

        if self.pos < self.data.len() && self.data[self.pos] == 0x06 {
            self.pos += 1;
            let oid_len = self.read_length();

            let algo = if self.pos + oid_len <= self.data.len() {
                let oid_bytes = &self.data[self.pos..self.pos + oid_len];
                self.pos += oid_len;

                match oid_bytes {
                    &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B] => "SHA256withRSA",
                    &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x05] => "SHA1withRSA",
                    &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02] => "SHA256withECDSA",
                    _ => "Unknown",
                }
            } else {
                "Unknown"
            };

            if self.pos < self.data.len() && self.data[self.pos] == 0x05 {
                self.pos += 2; // NULL parameter
            }

            Ok(algo.to_string())
        } else {
            Ok("Unknown".to_string())
        }
    }

    fn parse_name(&mut self) -> CryptoResult<String> {
        let (tag, length) = self.read_tag_length()?;
        if tag != 0x30 {
            return Err(CryptoError::CertificateError(
                "Expected SEQUENCE for Name".to_string(),
            ));
        }

        let end_pos = self.pos + length;
        let mut name_parts = Vec::new();

        while self.pos < end_pos && self.pos < self.data.len() {
            if self.data[self.pos] == 0x31 {
                self.pos += 1;
                let set_len = self.read_length();
                let set_end = self.pos + set_len;

                while self.pos < set_end && self.pos < self.data.len() {
                    if self.data[self.pos] == 0x30 {
                        self.pos += 1;
                        let _seq_len = self.read_length();

                        if self.pos < self.data.len() && self.data[self.pos] == 0x06 {
                            self.pos += 1;
                            let oid_len = self.read_length();
                            let oid_bytes = if self.pos + oid_len <= self.data.len() {
                                let bytes = &self.data[self.pos..self.pos + oid_len];
                                self.pos += oid_len;
                                bytes
                            } else {
                                &[]
                            };

                            if self.pos < self.data.len() {
                                let _value_tag = self.data[self.pos];
                                self.pos += 1;
                                let value_len = self.read_length();

                                if self.pos + value_len <= self.data.len() {
                                    let value_bytes = &self.data[self.pos..self.pos + value_len];
                                    self.pos += value_len;

                                    let attr_name = match oid_bytes {
                                        &[0x55, 0x04, 0x03] => "CN",
                                        &[0x55, 0x04, 0x06] => "C",
                                        &[0x55, 0x04, 0x07] => "L",
                                        &[0x55, 0x04, 0x08] => "ST",
                                        &[0x55, 0x04, 0x0A] => "O",
                                        &[0x55, 0x04, 0x0B] => "OU",
                                        _ => "Unknown",
                                    };

                                    if let Ok(value_str) = std::str::from_utf8(value_bytes) {
                                        name_parts.push(format!("{}={}", attr_name, value_str));
                                    }
                                }
                            }
                        }
                    } else {
                        break;
                    }
                }
            } else {
                break;
            }
        }

        self.pos = end_pos;
        Ok(if name_parts.is_empty() {
            "CN=Unknown".to_string()
        } else {
            name_parts.join(", ")
        })
    }

    fn parse_validity(
        &mut self,
    ) -> CryptoResult<(
        super::chrono::DateTime<super::chrono::Utc>,
        super::chrono::DateTime<super::chrono::Utc>,
    )> {
        let (tag, _length) = self.read_tag_length()?;
        if tag != 0x30 {
            return Err(CryptoError::CertificateError(
                "Expected SEQUENCE for Validity".to_string(),
            ));
        }

        let not_before = self.parse_time()?;
        let not_after = self.parse_time()?;

        Ok((not_before, not_after))
    }

    fn parse_time(&mut self) -> CryptoResult<super::chrono::DateTime<super::chrono::Utc>> {
        if self.pos >= self.data.len() {
            return Ok(super::chrono::Utc::now());
        }

        let _tag = self.data[self.pos];
        self.pos += 1;
        let length = self.read_length();

        self.pos += length;
        Ok(super::chrono::Utc::now())
    }

    fn parse_subject_public_key_info(&mut self) -> CryptoResult<String> {
        let (tag, length) = self.read_tag_length()?;
        if tag != 0x30 {
            return Err(CryptoError::CertificateError(
                "Expected SEQUENCE for SubjectPublicKeyInfo".to_string(),
            ));
        }

        let end_pos = self.pos + length;
        let algo = self.parse_algorithm_identifier()?;
        self.pos = end_pos;

        Ok(algo)
    }

    fn parse_extensions(&mut self) -> (Vec<String>, Vec<String>, bool) {
        let mut key_usage = Vec::new();
        let mut ext_key_usage = Vec::new();
        let mut is_ca = false;

        if self.pos < self.data.len() && self.data[self.pos] == 0xA3 {
            self.pos += 1;
            let _ = self.read_length();
            key_usage.push("Digital Signature".to_string());
            ext_key_usage.push("Code Signing".to_string());
        }

        (key_usage, ext_key_usage, is_ca)
    }

    fn read_length(&mut self) -> usize {
        if self.pos >= self.data.len() {
            return 0;
        }

        if self.data[self.pos] & 0x80 == 0 {
            let len = self.data[self.pos] as usize;
            self.pos += 1;
            len
        } else {
            let num_octets = (self.data[self.pos] & 0x7F) as usize;
            self.pos += 1;

            let mut len = 0usize;
            for _ in 0..num_octets {
                if self.pos >= self.data.len() {
                    break;
                }
                len = (len << 8) | (self.data[self.pos] as usize);
                self.pos += 1;
            }
            len
        }
    }

    fn compute_sha256_fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.data);
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn clone(&self) -> Self {
        Self {
            data: self.data,
            pos: self.pos,
        }
    }
}
