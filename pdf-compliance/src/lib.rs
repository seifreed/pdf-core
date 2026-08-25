use pdf_ast::{AstNode, PdfDictionary, PdfDocument, Visitor, VisitorAction};
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
    let mut validator = PdfA1bValidator::new();
    let mut walker = pdf_ast::visitor::AstWalker::new(&document.ast);
    walker.walk(&mut validator);
    validator.generate_report()
}

pub fn validate_pdfua1(document: &PdfDocument) -> ComplianceReport {
    let mut validator = PdfUA1Validator::new();
    let mut walker = pdf_ast::visitor::AstWalker::new(&document.ast);
    walker.walk(&mut validator);
    validator.generate_report()
}

struct PdfA1bValidator {
    violations: Vec<Violation>,
    has_embedded_fonts: bool,
    has_color_profile: bool,
}

impl PdfA1bValidator {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            has_embedded_fonts: false,
            has_color_profile: false,
        }
    }

    fn generate_report(&self) -> ComplianceReport {
        let status = if self
            .violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Error)
        {
            ComplianceStatus::NonCompliant
        } else if self
            .violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Warning)
        {
            ComplianceStatus::PartiallyCompliant
        } else {
            ComplianceStatus::Compliant
        };

        ComplianceReport {
            profile: ComplianceProfile::PdfA1b,
            scope: "Experimental preflight checks for selected PDF/A-1b requirements".to_string(),
            status,
            violations: self.violations.clone(),
            recommendations: vec![
                "Ensure all fonts are embedded".to_string(),
                "Include ICC color profile".to_string(),
                "Remove JavaScript and multimedia content".to_string(),
            ],
        }
    }
}

impl Visitor for PdfA1bValidator {
    fn visit_font(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        if dict.get("FontFile").is_none()
            && dict.get("FontFile2").is_none()
            && dict.get("FontFile3").is_none()
        {
            self.violations.push(Violation {
                rule_id: "pdfa-1b.font-embedding".to_string(),
                rule: "PDF/A-1b Font Embedding".to_string(),
                description: "All fonts must be embedded in PDF/A-1b".to_string(),
                location: "Font dictionary".to_string(),
                standard_reference: Some("ISO 19005-1:2005, 6.3.5".to_string()),
                severity: ViolationSeverity::Error,
            });
        } else {
            self.has_embedded_fonts = true;
        }
        VisitorAction::Continue
    }

    fn visit_action(&mut self, _node: &AstNode, _dict: &PdfDictionary) -> VisitorAction {
        self.violations.push(Violation {
            rule_id: "pdfa-1b.interactive-content".to_string(),
            rule: "PDF/A-1b Interactive Content".to_string(),
            description: "Interactive content not allowed in PDF/A-1b".to_string(),
            location: "Action dictionary".to_string(),
            standard_reference: Some("ISO 19005-1:2005, 6.6".to_string()),
            severity: ViolationSeverity::Error,
        });
        VisitorAction::Continue
    }
}

struct PdfUA1Validator {
    violations: Vec<Violation>,
    has_structure_tree: bool,
    has_lang_attribute: bool,
}

impl PdfUA1Validator {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            has_structure_tree: false,
            has_lang_attribute: false,
        }
    }

    fn generate_report(&self) -> ComplianceReport {
        let status = if self
            .violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Error)
        {
            ComplianceStatus::NonCompliant
        } else {
            ComplianceStatus::Compliant
        };

        ComplianceReport {
            profile: ComplianceProfile::PdfUA1,
            scope: "Experimental preflight checks for selected PDF/UA-1 requirements".to_string(),
            status,
            violations: self.violations.clone(),
            recommendations: vec![
                "Add structure tree for accessibility".to_string(),
                "Include language attributes".to_string(),
                "Provide alt text for images".to_string(),
            ],
        }
    }
}

impl Visitor for PdfUA1Validator {
    fn visit_catalog(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        if dict.get("StructTreeRoot").is_some() {
            self.has_structure_tree = true;
        } else {
            self.violations.push(Violation {
                rule_id: "pdfua-1.structure-tree".to_string(),
                rule: "PDF/UA-1 Structure Tree".to_string(),
                description: "Document must have a structure tree".to_string(),
                location: "Catalog".to_string(),
                standard_reference: Some("ISO 14289-1:2014, 7.1".to_string()),
                severity: ViolationSeverity::Error,
            });
        }

        if dict.get("Lang").is_some() {
            self.has_lang_attribute = true;
        } else {
            self.violations.push(Violation {
                rule_id: "pdfua-1.language".to_string(),
                rule: "PDF/UA-1 Language".to_string(),
                description: "Document must specify primary language".to_string(),
                location: "Catalog".to_string(),
                standard_reference: Some("ISO 14289-1:2014, 7.2".to_string()),
                severity: ViolationSeverity::Warning,
            });
        }

        VisitorAction::Continue
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
    }
}
