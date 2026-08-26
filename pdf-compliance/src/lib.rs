use pdf_ast::validation::{ValidationIssue, ValidationReport, ValidationSeverity};
use pdf_ast::PdfDocument;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub profile: ComplianceProfile,
    pub scope: String,
    pub status: ComplianceStatus,
    pub violations: Vec<Violation>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComplianceProfile {
    PdfA1a,
    PdfA1b,
    PdfA2a,
    PdfA2b,
    PdfA2u,
    PdfA3a,
    PdfA3b,
    PdfA3u,
    PdfUA1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComplianceStatus {
    Compliant,
    NonCompliant,
    PartiallyCompliant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub rule_id: String,
    pub rule: String,
    pub description: String,
    pub location: String,
    pub standard_reference: Option<String>,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Error,
    Warning,
    Info,
}

pub fn validate_pdfa1b(document: &PdfDocument) -> ComplianceReport {
    let report = pdf_ast::validation::pdfa::PdfA1bValidator::new().validate(document);
    convert_report(ComplianceProfile::PdfA1b, report)
}

pub fn validate_pdfua1(document: &PdfDocument) -> ComplianceReport {
    let report = pdf_ast::validation::SchemaRegistry::new()
        .validate(document, "PDF/UA-1")
        .expect("PDF/UA-1 is registered by the root validation registry");
    convert_report(ComplianceProfile::PdfUA1, report)
}

fn convert_report(profile: ComplianceProfile, report: ValidationReport) -> ComplianceReport {
    let status = if report.issues.iter().any(|issue| {
        matches!(
            issue.severity,
            ValidationSeverity::Error | ValidationSeverity::Critical
        )
    }) {
        ComplianceStatus::NonCompliant
    } else if report
        .issues
        .iter()
        .any(|issue| issue.severity == ValidationSeverity::Warning)
    {
        ComplianceStatus::PartiallyCompliant
    } else {
        ComplianceStatus::Compliant
    };

    let violations = report.issues.into_iter().map(convert_issue).collect();

    ComplianceReport {
        profile,
        scope: "Experimental preflight checks for selected requirements".to_string(),
        status,
        violations,
        recommendations: vec![
            "Review every reported rule against the target profile".to_string(),
            "Do not treat a clean report as full conformance".to_string(),
        ],
    }
}

fn convert_issue(issue: ValidationIssue) -> Violation {
    let standard_reference = match issue.code.as_str() {
        "PDF_A_FONT_EMBEDDING" => Some("ISO 19005-1:2005, 6.3.5"),
        "PDF_A_MULTIMEDIA" | "PDF_A_JAVASCRIPT" => Some("ISO 19005-1:2005, 6.6"),
        "NO_TAGGED_STRUCTURE" | "STRUCT_ELEM_MISSING" => Some("ISO 14289-1:2014, 7.1"),
        "LANG_MISSING" | "LANG_EMPTY" => Some("ISO 14289-1:2014, 7.2"),
        _ => None,
    };

    Violation {
        rule_id: issue.code.clone(),
        rule: issue.code,
        description: issue.message,
        location: issue.location.unwrap_or_else(|| "Document".to_string()),
        standard_reference: standard_reference.map(str::to_string),
        severity: match issue.severity {
            ValidationSeverity::Critical | ValidationSeverity::Error => ViolationSeverity::Error,
            ValidationSeverity::Warning => ViolationSeverity::Warning,
            ValidationSeverity::Info => ViolationSeverity::Info,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_ast::PdfVersion;

    #[test]
    fn test_pdfa1b_validation() {
        let doc = PdfDocument::new(PdfVersion::new(1, 4));
        let report = validate_pdfa1b(&doc);
        assert_eq!(report.profile, ComplianceProfile::PdfA1b);
    }

    #[test]
    fn test_pdfua1_validation() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        let report = validate_pdfua1(&doc);
        assert_eq!(report.profile, ComplianceProfile::PdfUA1);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.rule_id == "CATALOG_MISSING"));
    }
}
