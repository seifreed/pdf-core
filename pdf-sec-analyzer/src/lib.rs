pub mod report;
pub mod rules;
pub mod scanner;
pub mod threats;

use pdf_ast::{AstNode, NodeType, PdfDictionary, PdfDocument, PdfValue, Visitor, VisitorAction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use report::SecurityReport;
pub use scanner::SecurityScanner;
pub use threats::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub threats: Vec<Threat>,
    pub warnings: Vec<Warning>,
    pub info: Vec<InfoItem>,
    pub metadata: ScanMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanMetadata {
    pub scan_time: String,
    pub rules_applied: Vec<String>,
    pub total_nodes_scanned: usize,
    pub scan_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub category: String,
    pub severity: Severity,
    pub message: String,
    pub location: Option<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoItem {
    pub category: String,
    pub message: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Low => write!(f, "LOW"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

pub fn scan_document(document: &PdfDocument) -> ScanResult {
    let mut scanner = SecurityScanner::new();
    scanner.scan(document)
}

pub fn scan_for_malware(document: &PdfDocument) -> Vec<Threat> {
    let result = scan_document(document);
    result
        .threats
        .into_iter()
        .filter(|t| matches!(t.threat_type, ThreatType::Malware | ThreatType::Exploit))
        .collect()
}

pub fn scan_for_privacy_risks(document: &PdfDocument) -> Vec<Threat> {
    let result = scan_document(document);
    result
        .threats
        .into_iter()
        .filter(|t| {
            matches!(
                t.threat_type,
                ThreatType::PrivacyRisk | ThreatType::DataLeakage
            )
        })
        .collect()
}

pub fn quick_security_check(document: &PdfDocument) -> bool {
    let result = scan_document(document);
    !result.threats.iter().any(|t| t.severity >= Severity::High)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_ast::{NodeType, PdfName, PdfVersion};

    #[test]
    fn test_basic_scan() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        let result = scan_document(&doc);
        assert!(result.threats.is_empty());
    }

    #[test]
    fn test_quick_security_check() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        assert!(quick_security_check(&doc));
    }
}
