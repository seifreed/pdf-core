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

impl ComplianceProfile {
    fn schema_name(&self) -> &'static str {
        match self {
            Self::PdfA1a => "PDF/A-1a",
            Self::PdfA1b => "PDF/A-1b",
            Self::PdfA2a => "PDF/A-2a",
            Self::PdfA2b => "PDF/A-2b",
            Self::PdfA2u => "PDF/A-2u",
            Self::PdfA3a => "PDF/A-3a",
            Self::PdfA3b => "PDF/A-3b",
            Self::PdfA3u => "PDF/A-3u",
            Self::PdfUA1 => "PDF/UA-1",
        }
    }
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
    pub node_id: Option<usize>,
    pub offset: Option<u64>,
    pub expected: Option<String>,
    pub standard_reference: Option<String>,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Error,
    Warning,
    Info,
}

pub fn validate_profile(
    document: &PdfDocument,
    profile: ComplianceProfile,
) -> Option<ComplianceReport> {
    let report =
        pdf_ast::validation::SchemaRegistry::new().validate(document, profile.schema_name())?;
    Some(convert_report(profile, document, report))
}

pub fn validate_pdfa1b(document: &PdfDocument) -> ComplianceReport {
    validate_profile(document, ComplianceProfile::PdfA1b)
        .expect("PDF/A-1b is registered by the root validation registry")
}

pub fn validate_pdfua1(document: &PdfDocument) -> ComplianceReport {
    validate_profile(document, ComplianceProfile::PdfUA1)
        .expect("PDF/UA-1 is registered by the root validation registry")
}

fn convert_report(
    profile: ComplianceProfile,
    document: &PdfDocument,
    report: ValidationReport,
) -> ComplianceReport {
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

    let violations = report
        .issues
        .into_iter()
        .map(|issue| convert_issue(issue, document))
        .collect();

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

fn convert_issue(issue: ValidationIssue, document: &PdfDocument) -> Violation {
    let node_id = issue.node_id.map(|id| id.index());
    let offset = issue
        .node_id
        .and_then(|id| document.ast.get_node(id))
        .and_then(|node| node.metadata.offset);
    let standard_reference = match issue.code.as_str() {
        "PDF_A_FONT_EMBEDDING" => Some("ISO 19005-1:2005, 6.3.4"),
        "PDF_A_MULTIMEDIA" => Some("ISO 19005-1:2005, 6.5.2"),
        "PDF_A_JAVASCRIPT" => Some("ISO 19005-1:2005, 6.6.1"),
        "NO_TAGGED_STRUCTURE" | "STRUCT_ELEM_MISSING" => Some("ISO 14289-1:2014, 7.1"),
        "LANG_MISSING" | "LANG_EMPTY" => Some("ISO 14289-1:2014, 7.2"),
        "ALT_TEXT_MISSING" => Some("ISO 14289-1:2014, 7.3"),
        _ => None,
    };

    Violation {
        rule_id: issue.code.clone(),
        rule: issue.code,
        description: issue.message,
        location: issue.location.unwrap_or_else(|| "Document".to_string()),
        node_id,
        offset,
        expected: issue.suggestion,
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
    use pdf_ast::types::PdfValue;
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

    #[test]
    fn validates_all_exposed_profiles_through_the_root_registry() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        for profile in [
            ComplianceProfile::PdfA1a,
            ComplianceProfile::PdfA1b,
            ComplianceProfile::PdfA2a,
            ComplianceProfile::PdfA2b,
            ComplianceProfile::PdfA2u,
            ComplianceProfile::PdfA3a,
            ComplianceProfile::PdfA3b,
            ComplianceProfile::PdfA3u,
            ComplianceProfile::PdfUA1,
        ] {
            assert!(validate_profile(&doc, profile).is_some());
        }
    }

    #[test]
    fn preserves_issue_location_and_expectation_in_adapter_output() {
        let mut doc = PdfDocument::new(PdfVersion::new(1, 7));
        let node_id = doc
            .ast
            .create_node(pdf_ast::ast::NodeType::Root, PdfValue::Null);
        doc.ast
            .get_node_mut(node_id)
            .expect("created node exists")
            .metadata
            .offset = Some(123);
        let issue = ValidationIssue {
            severity: ValidationSeverity::Error,
            code: "PDF_A_FONT_EMBEDDING".to_string(),
            message: "font is not embedded".to_string(),
            node_id: Some(node_id),
            location: Some("Font".to_string()),
            suggestion: Some("Embed the font".to_string()),
        };

        let violation = convert_issue(issue, &doc);
        assert_eq!(violation.node_id, Some(node_id.index()));
        assert_eq!(violation.offset, Some(123));
        assert_eq!(violation.expected.as_deref(), Some("Embed the font"));
        assert_eq!(
            violation.standard_reference.as_deref(),
            Some("ISO 19005-1:2005, 6.3.4")
        );
    }

    #[test]
    fn preserves_pdfua_alt_text_reference() {
        let violation = convert_issue(
            ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "ALT_TEXT_MISSING".to_string(),
                message: "missing alternative text".to_string(),
                node_id: None,
                location: None,
                suggestion: None,
            },
            &PdfDocument::new(PdfVersion::new(1, 7)),
        );

        assert_eq!(
            violation.standard_reference.as_deref(),
            Some("ISO 14289-1:2014, 7.3")
        );
    }
}
