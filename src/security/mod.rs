pub mod etsi;
pub mod hardening;
pub mod heuristics;
pub mod ltv;
pub mod polyglot;
pub mod quirks;
pub mod report_output;
pub mod signatures;

pub use report_output::{format_security_report, output_format_from_path, SecurityOutputFormat};

use crate::ast::PdfAstGraph;
use crate::performance::ResourceBudget;
use serde::{Deserialize, Serialize};

// Re-export hardening module
pub use hardening::{
    PdfSanitizer, SecurityLimits, SecurityStatistics, SecurityValidator, SecurityViolation,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub signatures: Vec<DigitalSignature>,
    pub encryption: Option<EncryptionInfo>,
    pub permissions: DocumentPermissions,
    pub validation_results: Vec<ValidationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigitalSignature {
    pub field_name: String,
    pub signature_type: SignatureType,
    pub signer: Option<String>,
    pub signing_time: Option<String>,
    pub certificate_info: Option<CertificateInfo>,
    pub validity: SignatureValidity,
    pub location: Option<String>,
    pub reason: Option<String>,
    pub contact_info: Option<String>,
    pub timestamp: Option<TimestampDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureType {
    AdbePkcs7Detached,
    AdbePkcs7Sha1,
    AdbeX509RsaSha1,
    EtsiCadEsDetached,
    EtsiRfc3161,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub issuer: String,
    pub subject: String,
    pub serial_number: String,
    pub valid_from: String,
    pub valid_to: String,
    pub key_usage: Vec<String>,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampDetails {
    pub time: Option<String>,
    pub policy_oid: Option<String>,
    pub hash_algorithm: Option<String>,
    pub signature_valid: bool,
    pub tsa_chain_valid: Option<bool>,
    pub tsa_pin_valid: Option<bool>,
    pub tsa_revocation_events: Vec<crate::crypto::certificates::RevocationEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureValidity {
    Valid,
    Invalid(String),
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    pub algorithm: String,
    pub key_length: u32,
    pub revision: u32,
    pub permissions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPermissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub add_notes: bool,
    pub fill_forms: bool,
    pub accessibility: bool,
    pub assemble: bool,
    pub high_quality_print: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub check_type: String,
    pub status: ValidationStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationStatus {
    Pass,
    Fail,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub report_format_version: String,
    pub generated_at_unix: u64,
    pub security: SecurityInfo,
}

impl Default for DocumentPermissions {
    fn default() -> Self {
        Self {
            print: true,
            modify: true,
            copy: true,
            add_notes: true,
            fill_forms: true,
            accessibility: true,
            assemble: true,
            high_quality_print: true,
        }
    }
}

/// Security analyzer for detecting malicious patterns and indicators of compromise in PDF documents.
pub struct SecurityAnalyzer;

impl SecurityAnalyzer {
    /// Performs security analysis on a PDF AST graph.
    ///
    /// Detects:
    /// - JavaScript actions and embedded scripts
    /// - URI actions and external links
    /// - Launch actions (executable invocation)
    /// - Embedded files
    /// - RichMedia content
    /// - OpenAction and Additional Actions (AA)
    /// - XFA forms
    /// - Suspicious patterns in strings and streams
    ///
    /// # Arguments
    /// * `ast` - The PDF AST graph to analyze
    ///
    /// # Returns
    /// A `SecurityInfo` struct containing validation results for detected indicators of compromise
    pub fn analyze(ast: &PdfAstGraph) -> SecurityInfo {
        Self::analyze_with_budget(ast, &ResourceBudget::default())
    }

    pub fn analyze_with_budget(ast: &PdfAstGraph, budget: &ResourceBudget) -> SecurityInfo {
        let mut results = Vec::new();

        // Direct node-type indicators
        let js_nodes = ast.find_nodes_by_type(crate::ast::NodeType::JavaScriptAction);
        if !js_nodes.is_empty() {
            results.push(ValidationResult {
                check_type: "IOC:JavaScriptAction".to_string(),
                status: ValidationStatus::Fail,
                message: format!("JavaScript actions detected: {}", js_nodes.len()),
            });
        }

        let uri_nodes = ast.find_nodes_by_type(crate::ast::NodeType::URIAction);
        if !uri_nodes.is_empty() {
            results.push(ValidationResult {
                check_type: "IOC:URIAction".to_string(),
                status: ValidationStatus::Warning,
                message: format!("URI actions detected: {}", uri_nodes.len()),
            });
        }

        let launch_nodes = ast.find_nodes_by_type(crate::ast::NodeType::LaunchAction);
        if !launch_nodes.is_empty() {
            results.push(ValidationResult {
                check_type: "IOC:LaunchAction".to_string(),
                status: ValidationStatus::Fail,
                message: format!("Launch actions detected: {}", launch_nodes.len()),
            });
        }

        let embedded_files = ast.find_nodes_by_type(crate::ast::NodeType::EmbeddedFile);
        if !embedded_files.is_empty() {
            results.push(ValidationResult {
                check_type: "IOC:EmbeddedFile".to_string(),
                status: ValidationStatus::Warning,
                message: format!("Embedded files detected: {}", embedded_files.len()),
            });
        }

        let richmedia_nodes = ast.find_nodes_by_type(crate::ast::NodeType::RichMedia);
        if !richmedia_nodes.is_empty() {
            results.push(ValidationResult {
                check_type: "IOC:RichMedia".to_string(),
                status: ValidationStatus::Fail,
                message: format!("RichMedia content detected: {}", richmedia_nodes.len()),
            });
        }

        // Scan all nodes for dictionary-based indicators and patterns
        let mut suspicious_patterns = 0usize;
        let mut uri_hits = Vec::new();
        let mut launch_hits = 0usize;
        let mut xfa_hits = 0usize;
        let mut open_action_hits = 0usize;
        let mut aa_hits = 0usize;
        let mut embedded_hits = 0usize;
        let mut js_string_hits = 0usize;

        for node in ast.get_all_nodes() {
            if let Some(dict) = node.value.as_dict() {
                if dict.contains_key("OpenAction") {
                    open_action_hits += 1;
                }
                if dict.contains_key("AA") {
                    aa_hits += 1;
                }
                if dict.contains_key("XFA") {
                    xfa_hits += 1;
                }
                if dict.contains_key("EF") || dict.contains_key("EmbeddedFiles") {
                    embedded_hits += 1;
                }

                // Action dictionaries
                if let Some(crate::types::PdfValue::Name(s)) = dict.get("S") {
                    let action = s.without_slash();
                    match action {
                        "JavaScript" => js_string_hits += 1,
                        "Launch" => launch_hits += 1,
                        "URI" => {
                            if let Some(uri) = extract_uri(dict) {
                                uri_hits.push(uri);
                            }
                        }
                        _ => {}
                    }
                }

                // URI keys
                if let Some(uri) = dict.get("URI").and_then(extract_string_value) {
                    uri_hits.push(uri);
                }

                // Filespecs
                if let Some(crate::types::PdfValue::Name(t)) = dict.get("Type") {
                    if t.without_slash() == "Filespec" {
                        embedded_hits += 1;
                    }
                }
            }

            // Scan strings/names for suspicious patterns
            let mut extracted = Vec::new();
            collect_textual_values(&node.value, &mut extracted);
            for text in extracted {
                if is_javascript_pattern(&text) {
                    js_string_hits += 1;
                }
                if is_suspicious_pattern(&text) {
                    suspicious_patterns += 1;
                }
            }

            // Stream content scanning (decoded if possible)
            if let Some(stream) = node.value.as_stream() {
                if let Ok(decoded) = stream.decode_with_budget(budget) {
                    let sample = &decoded[..decoded.len().min(1024 * 1024)];
                    let text = String::from_utf8_lossy(sample);
                    if is_javascript_pattern(&text) {
                        js_string_hits += 1;
                    }
                    if is_suspicious_pattern(&text) {
                        suspicious_patterns += 1;
                    }
                }
            }
        }

        if open_action_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:OpenAction".to_string(),
                status: ValidationStatus::Warning,
                message: format!("OpenAction present: {}", open_action_hits),
            });
        }

        if aa_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:AdditionalActions".to_string(),
                status: ValidationStatus::Warning,
                message: format!("Additional Actions (AA) present: {}", aa_hits),
            });
        }

        if xfa_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:XFA".to_string(),
                status: ValidationStatus::Fail,
                message: format!("XFA forms detected: {}", xfa_hits),
            });
        }

        if launch_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:Launch".to_string(),
                status: ValidationStatus::Fail,
                message: format!("Launch actions detected: {}", launch_hits),
            });
        }

        if embedded_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:EmbeddedFiles".to_string(),
                status: ValidationStatus::Warning,
                message: format!("Embedded file indicators: {}", embedded_hits),
            });
        }

        for uri in uri_hits {
            let status = if uri.starts_with("http://")
                || uri.starts_with("ftp://")
                || uri.starts_with("file://")
                || uri.contains("\\\\")
            {
                ValidationStatus::Fail
            } else {
                ValidationStatus::Warning
            };
            results.push(ValidationResult {
                check_type: "IOC:URI".to_string(),
                status,
                message: format!("External URI: {}", uri),
            });
        }

        if js_string_hits > 0 {
            results.push(ValidationResult {
                check_type: "IOC:JavaScriptIndicators".to_string(),
                status: ValidationStatus::Fail,
                message: format!("JavaScript indicators found: {}", js_string_hits),
            });
        }

        if suspicious_patterns > 0 {
            results.push(ValidationResult {
                check_type: "IOC:SuspiciousPatterns".to_string(),
                status: ValidationStatus::Warning,
                message: format!("Suspicious patterns found: {}", suspicious_patterns),
            });
        }

        if results.is_empty() {
            results.push(ValidationResult {
                check_type: "Security".to_string(),
                status: ValidationStatus::Pass,
                message: "No suspicious indicators detected".to_string(),
            });
        }

        SecurityInfo {
            signatures: vec![],
            encryption: None,
            permissions: DocumentPermissions::default(),
            validation_results: results,
        }
    }

    /// Performs comprehensive security analysis on a complete PDF document.
    ///
    /// This method combines:
    /// - AST-based indicator detection (from `analyze()`)
    /// - Digital signature verification with cryptographic validation
    /// - ETSI profile validation (CAdES, PAdES, RFC3161)
    /// - Heuristic analysis for anomalies
    /// - Producer quirk detection for known malware patterns
    ///
    /// # Arguments
    /// * `document` - The parsed PDF document
    /// * `reader` - A seekable reader for accessing raw PDF data (required for signature verification)
    /// * `crypto_config` - Cryptographic configuration for signature validation and certificate verification
    ///
    /// # Returns
    /// A comprehensive `SecurityInfo` struct with all detected issues and signature validation results
    pub fn analyze_document<R: std::io::Read + std::io::Seek>(
        document: &crate::ast::PdfDocument,
        reader: &mut R,
        crypto_config: crate::crypto::CryptoConfig,
    ) -> SecurityInfo {
        let mut info = Self::analyze_with_budget(&document.ast, &document.budget);
        let mut verifier = crate::crypto::signature_verification::SignatureVerifier::new()
            .with_crypto_config(crypto_config);

        let nodes = document.ast.get_all_nodes();
        let mut signatures = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            if node.node_type == crate::ast::NodeType::Signature {
                if let crate::types::PdfValue::Dictionary(dict) = &node.value {
                    let name = extract_signature_name(dict, index);
                    let sig = verifier.verify_signature(dict, &name, reader);
                    signatures.push(crate::security::signatures::to_digital_signature(&sig));
                }
            }
        }

        info.signatures = signatures;

        // ETSI profile checks (CAdES/PAdES/RFC3161)
        let etsi_results = crate::security::etsi::validate_etsi_profiles(
            &info.signatures,
            document.metadata.has_dss,
            crate::security::etsi::EtsiValidationOptions {
                require_dss_for_pades: true,
            },
        );
        info.validation_results.extend(etsi_results);

        if let Ok(mut heuristic_results) =
            crate::security::heuristics::analyze_document_heuristics(document, reader)
        {
            info.validation_results.append(&mut heuristic_results);
        }

        let mut quirk_results = crate::security::quirks::detect_producer_quirks(document);
        info.validation_results.append(&mut quirk_results);
        info
    }
}

/// Serializes a security report to JSON format.
///
/// # Arguments
/// * `report` - The security report to serialize
///
/// # Returns
/// Formatted JSON string on success
///
/// # Errors
/// Returns an error message if JSON serialization fails
pub fn security_report_to_json(report: &SecurityReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| format!("JSON serialization error: {}", e))
}

/// Serializes a security report to YAML format.
///
/// # Arguments
/// * `report` - The security report to serialize
///
/// # Returns
/// Formatted YAML string on success
///
/// # Errors
/// Returns an error message if YAML serialization fails
pub fn security_report_to_yaml(report: &SecurityReport) -> Result<String, String> {
    serde_yaml::to_string(report).map_err(|e| format!("YAML serialization error: {}", e))
}

/// Serializes a security report to TOML format.
///
/// # Arguments
/// * `report` - The security report to serialize
///
/// # Returns
/// Formatted TOML string on success
///
/// # Errors
/// Returns an error message if TOML serialization fails
pub fn security_report_to_toml(report: &SecurityReport) -> Result<String, String> {
    let converted = SecurityReportToml::from(report);
    toml::to_string_pretty(&converted).map_err(|e| format!("TOML serialization error: {}", e))
}

/// Converts security analysis results to a timestamped security report.
///
/// # Arguments
/// * `info` - The security information to wrap in a report
///
/// # Returns
/// A `SecurityReport` with current timestamp and format version
pub fn security_info_to_report(info: SecurityInfo) -> SecurityReport {
    let generated_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    SecurityReport {
        report_format_version: "1.0".to_string(),
        generated_at_unix,
        security: info,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecurityReportToml {
    report_format_version: String,
    generated_at_unix: u64,
    security: SecurityInfoToml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecurityInfoToml {
    signatures: Vec<DigitalSignatureToml>,
    encryption: Option<EncryptionInfo>,
    permissions: DocumentPermissions,
    validation_results: Vec<ValidationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DigitalSignatureToml {
    field_name: String,
    signature_type: SignatureType,
    signer: Option<String>,
    signing_time: Option<String>,
    certificate_info: Option<CertificateInfo>,
    validity: SignatureValidity,
    location: Option<String>,
    reason: Option<String>,
    contact_info: Option<String>,
    timestamp: Option<TimestampDetailsToml>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimestampDetailsToml {
    time: Option<String>,
    policy_oid: Option<String>,
    hash_algorithm: Option<String>,
    signature_valid: bool,
    tsa_chain_valid: Option<bool>,
    tsa_pin_valid: Option<bool>,
    tsa_revocation_events: Vec<RevocationEventToml>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevocationEventToml {
    cert_index: usize,
    url: String,
    protocol: crate::crypto::certificates::RevocationProtocol,
    status: String,
    latency_ms: u64,
    error: Option<String>,
}

impl From<&SecurityReport> for SecurityReportToml {
    fn from(report: &SecurityReport) -> Self {
        SecurityReportToml {
            report_format_version: report.report_format_version.clone(),
            generated_at_unix: report.generated_at_unix,
            security: SecurityInfoToml::from(&report.security),
        }
    }
}

impl From<&SecurityInfo> for SecurityInfoToml {
    fn from(info: &SecurityInfo) -> Self {
        SecurityInfoToml {
            signatures: info
                .signatures
                .iter()
                .map(DigitalSignatureToml::from)
                .collect(),
            encryption: info.encryption.clone(),
            permissions: info.permissions.clone(),
            validation_results: info.validation_results.clone(),
        }
    }
}

impl From<&DigitalSignature> for DigitalSignatureToml {
    fn from(sig: &DigitalSignature) -> Self {
        DigitalSignatureToml {
            field_name: sig.field_name.clone(),
            signature_type: sig.signature_type.clone(),
            signer: sig.signer.clone(),
            signing_time: sig.signing_time.clone(),
            certificate_info: sig.certificate_info.clone(),
            validity: sig.validity.clone(),
            location: sig.location.clone(),
            reason: sig.reason.clone(),
            contact_info: sig.contact_info.clone(),
            timestamp: sig.timestamp.as_ref().map(TimestampDetailsToml::from),
        }
    }
}

impl From<&TimestampDetails> for TimestampDetailsToml {
    fn from(ts: &TimestampDetails) -> Self {
        TimestampDetailsToml {
            time: ts.time.clone(),
            policy_oid: ts.policy_oid.clone(),
            hash_algorithm: ts.hash_algorithm.clone(),
            signature_valid: ts.signature_valid,
            tsa_chain_valid: ts.tsa_chain_valid,
            tsa_pin_valid: ts.tsa_pin_valid,
            tsa_revocation_events: ts
                .tsa_revocation_events
                .iter()
                .map(RevocationEventToml::from)
                .collect(),
        }
    }
}

impl From<&crate::crypto::certificates::RevocationEvent> for RevocationEventToml {
    fn from(ev: &crate::crypto::certificates::RevocationEvent) -> Self {
        RevocationEventToml {
            cert_index: ev.cert_index,
            url: ev.url.clone(),
            protocol: ev.protocol.clone(),
            status: ev.status.clone(),
            latency_ms: ev.latency_ms.min(u128::from(u64::MAX)) as u64,
            error: ev.error.clone(),
        }
    }
}

fn extract_signature_name(dict: &crate::types::PdfDictionary, index: usize) -> String {
    match dict.get("T") {
        Some(crate::types::PdfValue::String(s)) => s.to_string_lossy(),
        _ => match dict.get("Name") {
            Some(crate::types::PdfValue::String(s)) => s.to_string_lossy(),
            _ => format!("Signature_{}", index),
        },
    }
}

fn extract_string_value(value: &crate::types::PdfValue) -> Option<String> {
    match value {
        crate::types::PdfValue::String(s) => Some(s.to_string_lossy()),
        crate::types::PdfValue::Name(n) => Some(n.without_slash().to_string()),
        _ => None,
    }
}

fn extract_uri(dict: &crate::types::PdfDictionary) -> Option<String> {
    dict.get("URI").and_then(extract_string_value)
}

fn collect_textual_values(value: &crate::types::PdfValue, out: &mut Vec<String>) {
    match value {
        crate::types::PdfValue::String(s) => out.push(s.to_string_lossy()),
        crate::types::PdfValue::Name(n) => out.push(n.without_slash().to_string()),
        crate::types::PdfValue::Array(arr) => {
            for v in arr.iter() {
                collect_textual_values(v, out);
            }
        }
        crate::types::PdfValue::Dictionary(dict) => {
            for (k, v) in dict.iter() {
                out.push(k.without_slash().to_string());
                collect_textual_values(v, out);
            }
        }
        crate::types::PdfValue::Stream(stream) => {
            collect_textual_values(
                &crate::types::PdfValue::Dictionary(stream.dict.clone()),
                out,
            );
        }
        _ => {}
    }
}

fn is_javascript_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("javascript")
        || lower.contains("eval(")
        || lower.contains("unescape(")
        || lower.contains("fromcharcode")
        || lower.contains("app.launchurl")
        || lower.contains("this.exportdataobject")
        || lower.contains("submitform")
}

fn is_suspicious_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("/openaction")
        || lower.contains("/aa")
        || lower.contains("/launch")
        || lower.contains("/uri")
        || lower.contains("/xfa")
        || lower.contains("cmd.exe")
        || lower.contains("powershell")
        || lower.contains("javascript:")
        || lower.contains("file://")
        || lower.contains("http://")
        || lower.contains("https://")
}
