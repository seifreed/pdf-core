use crate::ast::{NodeId, PdfAstGraph};
use crate::types::{PdfDictionary, PdfValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Accessibility attributes for structure elements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityAttributes {
    /// Alternative text description
    pub alt: Option<String>,

    /// Actual text content (what would be spoken)
    pub actual_text: Option<String>,

    /// Expansion of an abbreviation
    pub e: Option<String>,

    /// Language specification
    pub lang: Option<String>,

    /// Title attribute
    pub title: Option<String>,

    /// Additional accessibility properties
    pub custom_attributes: HashMap<String, String>,

    /// ARIA-like roles for web compatibility
    pub aria_role: Option<String>,

    /// ARIA-like labels
    pub aria_label: Option<String>,

    /// ARIA-like descriptions
    pub aria_describedby: Option<String>,

    /// Table-specific attributes
    pub table_attributes: Option<TableAccessibility>,

    /// List-specific attributes
    pub list_attributes: Option<ListAccessibility>,

    /// Form-specific attributes
    pub form_attributes: Option<FormAccessibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableAccessibility {
    /// Table headers
    pub headers: Vec<String>,

    /// Row span
    pub rowspan: Option<u32>,

    /// Column span
    pub colspan: Option<u32>,

    /// Table scope (row, col, rowgroup, colgroup)
    pub scope: Option<TableScope>,

    /// Table summary
    pub summary: Option<String>,

    /// Table caption
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TableScope {
    Row,
    Col,
    RowGroup,
    ColGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAccessibility {
    /// List type (ordered, unordered, definition)
    pub list_type: ListType,

    /// List item numbering
    pub list_numbering: Option<ListNumbering>,

    /// Continuation marker
    pub continuation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListType {
    Ordered,
    Unordered,
    Definition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNumbering {
    pub start: Option<u32>,
    pub numbering_style: NumberingStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NumberingStyle {
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormAccessibility {
    /// Form field description
    pub description: Option<String>,

    /// Required field indicator
    pub required: bool,

    /// Field validation rules
    pub validation: Option<String>,

    /// Error messages
    pub error_message: Option<String>,

    /// Help text
    pub help_text: Option<String>,
}

/// Parser for accessibility attributes in structure elements
pub struct AccessibilityParser;

impl AccessibilityParser {
    /// Parse accessibility attributes from a structure element dictionary
    pub fn parse_attributes(struct_dict: &PdfDictionary) -> AccessibilityAttributes {
        let mut attributes = AccessibilityAttributes {
            alt: None,
            actual_text: None,
            e: None,
            lang: None,
            title: None,
            custom_attributes: HashMap::new(),
            aria_role: None,
            aria_label: None,
            aria_describedby: None,
            table_attributes: None,
            list_attributes: None,
            form_attributes: None,
        };

        // Parse standard PDF accessibility attributes
        if let Some(alt_value) = struct_dict.get("Alt") {
            attributes.alt = Self::extract_string_value(alt_value);
        }

        if let Some(actual_text_value) = struct_dict.get("ActualText") {
            attributes.actual_text = Self::extract_string_value(actual_text_value);
        }

        if let Some(e_value) = struct_dict.get("E") {
            attributes.e = Self::extract_string_value(e_value);
        }

        if let Some(lang_value) = struct_dict.get("Lang") {
            attributes.lang = Self::extract_string_value(lang_value);
        }

        // Parse A (attributes) dictionary for additional properties
        if let Some(PdfValue::Dictionary(a_dict)) = struct_dict.get("A") {
            Self::parse_attributes_dict(a_dict, &mut attributes);
        }

        // Parse C (class) attributes if present
        if let Some(c_value) = struct_dict.get("C") {
            Self::parse_class_attributes(c_value, &mut attributes);
        }

        // Determine content-specific attributes based on structure type
        if let Some(PdfValue::Name(s_type)) = struct_dict.get("S") {
            let struct_type = s_type.without_slash();
            match struct_type {
                "Table" | "THead" | "TBody" | "TFoot" | "TR" | "TH" | "TD" => {
                    attributes.table_attributes = Self::parse_table_attributes(struct_dict);
                }
                "L" | "LI" | "Lbl" | "LBody" => {
                    attributes.list_attributes = Self::parse_list_attributes(struct_dict);
                }
                "Form" | "Widget" => {
                    attributes.form_attributes = Self::parse_form_attributes(struct_dict);
                }
                _ => {}
            }
        }

        attributes
    }

    fn extract_string_value(value: &PdfValue) -> Option<String> {
        match value {
            PdfValue::String(s) => Some(s.to_string_lossy()),
            PdfValue::Name(n) => Some(n.without_slash().to_string()),
            _ => None,
        }
    }

    fn parse_attributes_dict(a_dict: &PdfDictionary, attributes: &mut AccessibilityAttributes) {
        // Parse O (Owner) specific attributes
        if let Some(PdfValue::Name(owner)) = a_dict.get("O") {
            match owner.without_slash() {
                "Layout" => Self::parse_layout_attributes(a_dict, attributes),
                "PrintField" => Self::parse_print_field_attributes(a_dict, attributes),
                "Table" => {
                    attributes.table_attributes =
                        Some(Self::parse_table_attributes_from_dict(a_dict));
                }
                "List" => {
                    attributes.list_attributes = Some(Self::parse_list_attributes_from_dict(
                        a_dict,
                        ListAccessibility {
                            list_type: ListType::Unordered,
                            list_numbering: None,
                            continuation: None,
                        },
                    ));
                }
                _ => {
                    // Store custom owner attributes
                    Self::store_custom_attributes(a_dict, attributes, owner.without_slash());
                }
            }
        }

        // Parse common attributes
        for (key, value) in a_dict.iter() {
            if let Some(string_val) = Self::extract_string_value(value) {
                match key.as_str() {
                    "Title" => attributes.title = Some(string_val),
                    "Lang" => attributes.lang = Some(string_val),
                    _ => {
                        attributes
                            .custom_attributes
                            .insert(key.without_slash().to_string(), string_val);
                    }
                }
            }
        }
    }

    fn parse_layout_attributes(a_dict: &PdfDictionary, attributes: &mut AccessibilityAttributes) {
        // Parse layout-specific attributes that affect accessibility
        if let Some(PdfValue::Name(placement)) = a_dict.get("Placement") {
            attributes.custom_attributes.insert(
                "placement".to_string(),
                placement.without_slash().to_string(),
            );
        }

        if let Some(PdfValue::Name(writing_mode)) = a_dict.get("WritingMode") {
            attributes.custom_attributes.insert(
                "writing_mode".to_string(),
                writing_mode.without_slash().to_string(),
            );
        }
    }

    fn parse_print_field_attributes(
        a_dict: &PdfDictionary,
        attributes: &mut AccessibilityAttributes,
    ) {
        // Parse print field attributes
        if let Some(role_value) = a_dict.get("Role") {
            if let Some(role) = Self::extract_string_value(role_value) {
                attributes.aria_role = Some(role);
            }
        }

        if let Some(PdfValue::Name(checked)) = a_dict.get("checked") {
            attributes
                .custom_attributes
                .insert("checked".to_string(), checked.without_slash().to_string());
        }
    }

    fn parse_class_attributes(c_value: &PdfValue, attributes: &mut AccessibilityAttributes) {
        match c_value {
            PdfValue::Name(class_name) => {
                attributes
                    .custom_attributes
                    .insert("class".to_string(), class_name.without_slash().to_string());
            }
            PdfValue::Array(class_array) => {
                let classes: Vec<String> = class_array
                    .iter()
                    .filter_map(|v| match v {
                        PdfValue::Name(n) => Some(n.without_slash().to_string()),
                        _ => None,
                    })
                    .collect();
                if !classes.is_empty() {
                    attributes
                        .custom_attributes
                        .insert("class".to_string(), classes.join(" "));
                }
            }
            _ => {}
        }
    }

    fn parse_table_attributes(struct_dict: &PdfDictionary) -> Option<TableAccessibility> {
        let mut table_attrs = TableAccessibility {
            headers: Vec::new(),
            rowspan: None,
            colspan: None,
            scope: None,
            summary: None,
            caption: None,
        };

        // Parse from A dictionary if present
        if let Some(PdfValue::Dictionary(a_dict)) = struct_dict.get("A") {
            table_attrs = Self::parse_table_attributes_from_dict(a_dict);
        }

        Some(table_attrs)
    }

    fn parse_table_attributes_from_dict(a_dict: &PdfDictionary) -> TableAccessibility {
        let mut table_attrs = TableAccessibility {
            headers: Vec::new(),
            rowspan: None,
            colspan: None,
            scope: None,
            summary: None,
            caption: None,
        };

        // Parse Headers array
        if let Some(PdfValue::Array(headers_array)) = a_dict.get("Headers") {
            for header in headers_array.iter() {
                if let Some(header_str) = Self::extract_string_value(header) {
                    table_attrs.headers.push(header_str);
                }
            }
        }

        // Parse RowSpan
        if let Some(PdfValue::Integer(rowspan)) = a_dict.get("RowSpan") {
            table_attrs.rowspan = Some(*rowspan as u32);
        }

        // Parse ColSpan
        if let Some(PdfValue::Integer(colspan)) = a_dict.get("ColSpan") {
            table_attrs.colspan = Some(*colspan as u32);
        }

        // Parse Scope
        if let Some(PdfValue::Name(scope)) = a_dict.get("Scope") {
            table_attrs.scope = match scope.without_slash() {
                "Row" => Some(TableScope::Row),
                "Column" => Some(TableScope::Col),
                "RowGroup" => Some(TableScope::RowGroup),
                "ColGroup" => Some(TableScope::ColGroup),
                _ => None,
            };
        }

        // Parse Summary
        if let Some(summary_value) = a_dict.get("Summary") {
            table_attrs.summary = Self::extract_string_value(summary_value);
        }

        table_attrs
    }

    fn parse_list_attributes(struct_dict: &PdfDictionary) -> Option<ListAccessibility> {
        // Determine list type from structure type
        let list_type = if let Some(PdfValue::Name(s_type)) = struct_dict.get("S") {
            match s_type.without_slash() {
                "L" => ListType::Unordered, // Default for generic list
                _ => ListType::Unordered,
            }
        } else {
            ListType::Unordered
        };

        let mut list_attrs = ListAccessibility {
            list_type,
            list_numbering: None,
            continuation: None,
        };

        // Parse from A dictionary if present
        if let Some(PdfValue::Dictionary(a_dict)) = struct_dict.get("A") {
            list_attrs = Self::parse_list_attributes_from_dict(a_dict, list_attrs);
        }

        Some(list_attrs)
    }

    fn parse_list_attributes_from_dict(
        a_dict: &PdfDictionary,
        mut list_attrs: ListAccessibility,
    ) -> ListAccessibility {
        // Parse ListNumbering
        if let Some(PdfValue::Dictionary(ln_dict)) = a_dict.get("ListNumbering") {
            let mut numbering = ListNumbering {
                start: None,
                numbering_style: NumberingStyle::Decimal,
            };

            if let Some(PdfValue::Integer(start)) = ln_dict.get("Start") {
                numbering.start = Some(*start as u32);
            }

            if let Some(PdfValue::Name(style)) = ln_dict.get("Style") {
                numbering.numbering_style = match style.without_slash() {
                    "Decimal" => NumberingStyle::Decimal,
                    "LowerAlpha" => NumberingStyle::LowerAlpha,
                    "UpperAlpha" => NumberingStyle::UpperAlpha,
                    "LowerRoman" => NumberingStyle::LowerRoman,
                    "UpperRoman" => NumberingStyle::UpperRoman,
                    other => NumberingStyle::Custom(other.to_string()),
                };
            }

            list_attrs.list_numbering = Some(numbering);
            list_attrs.list_type = ListType::Ordered; // Has numbering, so ordered
        }

        list_attrs
    }

    fn parse_form_attributes(struct_dict: &PdfDictionary) -> Option<FormAccessibility> {
        let mut form_attrs = FormAccessibility {
            description: None,
            required: false,
            validation: None,
            error_message: None,
            help_text: None,
        };

        // Parse from A dictionary if present
        if let Some(PdfValue::Dictionary(a_dict)) = struct_dict.get("A") {
            if let Some(desc_value) = a_dict.get("Desc") {
                form_attrs.description = Self::extract_string_value(desc_value);
            }

            if let Some(PdfValue::Boolean(required)) = a_dict.get("Required") {
                form_attrs.required = *required;
            }

            if let Some(validation_value) = a_dict.get("Validation") {
                form_attrs.validation = Self::extract_string_value(validation_value);
            }

            if let Some(error_value) = a_dict.get("ErrorMessage") {
                form_attrs.error_message = Self::extract_string_value(error_value);
            }

            if let Some(help_value) = a_dict.get("Help") {
                form_attrs.help_text = Self::extract_string_value(help_value);
            }
        }

        Some(form_attrs)
    }

    fn store_custom_attributes(
        a_dict: &PdfDictionary,
        attributes: &mut AccessibilityAttributes,
        owner: &str,
    ) {
        let prefix = format!("{}:", owner);
        for (key, value) in a_dict.iter() {
            if let Some(string_val) = Self::extract_string_value(value) {
                let full_key = format!("{}{}", prefix, key);
                attributes.custom_attributes.insert(full_key, string_val);
            }
        }
    }
}

/// Accessibility validator for structure trees
pub struct AccessibilityValidator;

impl AccessibilityValidator {
    /// Validate accessibility attributes and generate compliance report
    pub fn validate_structure_accessibility(
        ast: &PdfAstGraph,
        struct_tree_root: NodeId,
    ) -> AccessibilityReport {
        let mut report = AccessibilityReport::new();

        Self::validate_node_accessibility(ast, struct_tree_root, &mut report, 0);

        report.finalize();
        report
    }

    fn validate_node_accessibility(
        ast: &PdfAstGraph,
        node_id: NodeId,
        report: &mut AccessibilityReport,
        _depth: usize,
    ) {
        if let Some(node) = ast.get_node(node_id) {
            if let PdfValue::Dictionary(dict) = &node.value {
                let attributes = AccessibilityParser::parse_attributes(dict);

                // Validate based on structure type
                if let Some(PdfValue::Name(s_type)) = dict.get("S") {
                    let struct_type = s_type.without_slash();
                    Self::validate_by_structure_type(struct_type, &attributes, report, node_id);
                }

                // Check for missing Alt text on images and figures
                Self::validate_alt_text(&attributes, dict, report, node_id);

                // Validate language specification
                Self::validate_language(&attributes, report, node_id);

                // Validate table structure
                if let Some(table_attrs) = &attributes.table_attributes {
                    Self::validate_table_accessibility(table_attrs, report, node_id);
                }

                // Validate list structure
                if let Some(list_attrs) = &attributes.list_attributes {
                    Self::validate_list_accessibility(list_attrs, report, node_id);
                }
            }
        }

        // Recursively validate children
        for child_id in ast.get_children(node_id) {
            Self::validate_node_accessibility(ast, child_id, report, _depth + 1);
        }
    }

    fn validate_by_structure_type(
        struct_type: &str,
        attributes: &AccessibilityAttributes,
        report: &mut AccessibilityReport,
        node_id: NodeId,
    ) {
        match struct_type {
            "Figure" | "Img" => {
                if attributes.alt.is_none() {
                    report.add_issue(AccessibilityIssue {
                        issue_type: AccessibilityIssueType::MissingAltText,
                        severity: AccessibilityIssueSeverity::Error,
                        node_id,
                        description: "Image or figure missing alternative text".to_string(),
                        suggestion: Some("Add Alt attribute with descriptive text".to_string()),
                    });
                }
            }
            "H1" | "H2" | "H3" | "H4" | "H5" | "H6" => {
                if attributes.actual_text.is_none() && attributes.alt.is_none() {
                    report.add_issue(AccessibilityIssue {
                        issue_type: AccessibilityIssueType::MissingHeadingText,
                        severity: AccessibilityIssueSeverity::Warning,
                        node_id,
                        description: format!("Heading {} may need ActualText or Alt", struct_type),
                        suggestion: Some(
                            "Consider adding ActualText for screen readers".to_string(),
                        ),
                    });
                }
            }
            "Artifact" if attributes.alt.is_some() || attributes.actual_text.is_some() => {
                // Artifacts should not have accessibility attributes
                report.add_issue(AccessibilityIssue {
                    issue_type: AccessibilityIssueType::ArtifactWithText,
                    severity: AccessibilityIssueSeverity::Warning,
                    node_id,
                    description: "Artifact has accessibility text".to_string(),
                    suggestion: Some("Remove Alt/ActualText from artifacts".to_string()),
                });
            }
            _ => {}
        }
    }

    fn validate_alt_text(
        attributes: &AccessibilityAttributes,
        _dict: &PdfDictionary,
        report: &mut AccessibilityReport,
        node_id: NodeId,
    ) {
        // Check for empty alt text
        if let Some(alt) = &attributes.alt {
            if alt.trim().is_empty() {
                report.add_issue(AccessibilityIssue {
                    issue_type: AccessibilityIssueType::EmptyAltText,
                    severity: AccessibilityIssueSeverity::Warning,
                    node_id,
                    description: "Alt text is empty".to_string(),
                    suggestion: Some("Provide meaningful alternative text".to_string()),
                });
            }
        }
    }

    fn validate_language(
        attributes: &AccessibilityAttributes,
        report: &mut AccessibilityReport,
        node_id: NodeId,
    ) {
        // Check for proper language specification in multilingual documents
        if let Some(lang) = &attributes.lang {
            if !Self::is_valid_language_code(lang) {
                report.add_issue(AccessibilityIssue {
                    issue_type: AccessibilityIssueType::InvalidLanguageCode,
                    severity: AccessibilityIssueSeverity::Warning,
                    node_id,
                    description: format!("Invalid language code: {}", lang),
                    suggestion: Some("Use ISO 639-1 or BCP 47 language codes".to_string()),
                });
            }
        }
    }

    fn validate_table_accessibility(
        table_attrs: &TableAccessibility,
        report: &mut AccessibilityReport,
        node_id: NodeId,
    ) {
        // Check for table headers
        if table_attrs.headers.is_empty() {
            report.add_issue(AccessibilityIssue {
                issue_type: AccessibilityIssueType::MissingTableHeaders,
                severity: AccessibilityIssueSeverity::Error,
                node_id,
                description: "Table missing headers".to_string(),
                suggestion: Some("Add Headers attribute to table cells".to_string()),
            });
        }

        // Check for table summary
        if table_attrs.summary.is_none() {
            report.add_issue(AccessibilityIssue {
                issue_type: AccessibilityIssueType::MissingTableSummary,
                severity: AccessibilityIssueSeverity::Warning,
                node_id,
                description: "Complex table missing summary".to_string(),
                suggestion: Some("Add Summary attribute for complex tables".to_string()),
            });
        }
    }

    fn validate_list_accessibility(
        list_attrs: &ListAccessibility,
        report: &mut AccessibilityReport,
        node_id: NodeId,
    ) {
        // Validate list structure consistency
        if let ListType::Ordered = list_attrs.list_type {
            if list_attrs.list_numbering.is_none() {
                report.add_issue(AccessibilityIssue {
                    issue_type: AccessibilityIssueType::InconsistentListStructure,
                    severity: AccessibilityIssueSeverity::Warning,
                    node_id,
                    description: "Ordered list missing numbering information".to_string(),
                    suggestion: Some("Add ListNumbering attributes".to_string()),
                });
            }
        }
    }

    fn is_valid_language_code(lang: &str) -> bool {
        // Basic validation for common language codes
        let lang = lang.to_lowercase();
        let iso_codes = [
            "en", "es", "fr", "de", "it", "pt", "ru", "ja", "ko", "zh", "ar",
        ];

        // Check ISO 639-1 codes
        if iso_codes.contains(&lang.as_str()) {
            return true;
        }

        // Check for BCP 47 format (language-region)
        if lang.contains('-') && lang.len() >= 5 {
            let parts: Vec<&str> = lang.split('-').collect();
            if parts.len() == 2 && parts[0].len() == 2 && parts[1].len() == 2 {
                return iso_codes.contains(&parts[0]);
            }
        }

        false
    }
}

/// Accessibility compliance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityReport {
    pub issues: Vec<AccessibilityIssue>,
    pub summary: AccessibilitySummary,
    pub compliance_level: ComplianceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityIssue {
    pub issue_type: AccessibilityIssueType,
    pub severity: AccessibilityIssueSeverity,
    pub node_id: NodeId,
    pub description: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessibilityIssueType {
    MissingAltText,
    EmptyAltText,
    MissingHeadingText,
    ArtifactWithText,
    InvalidLanguageCode,
    MissingTableHeaders,
    MissingTableSummary,
    InconsistentListStructure,
    MissingFormLabel,
    InvalidStructureNesting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessibilityIssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySummary {
    pub total_issues: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub accessibility_score: f64, // 0.0 to 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceLevel {
    AAA, // Highest level
    AA,  // Standard level
    A,   // Minimum level
    NonCompliant,
}

impl Default for AccessibilityReport {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessibilityReport {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            summary: AccessibilitySummary {
                total_issues: 0,
                errors: 0,
                warnings: 0,
                info: 0,
                accessibility_score: 1.0,
            },
            compliance_level: ComplianceLevel::AAA,
        }
    }

    pub fn add_issue(&mut self, issue: AccessibilityIssue) {
        match issue.severity {
            AccessibilityIssueSeverity::Error => self.summary.errors += 1,
            AccessibilityIssueSeverity::Warning => self.summary.warnings += 1,
            AccessibilityIssueSeverity::Info => self.summary.info += 1,
            AccessibilityIssueSeverity::Critical => self.summary.errors += 1,
        }
        self.issues.push(issue);
    }

    pub fn finalize(&mut self) {
        self.summary.total_issues = self.issues.len();

        // Calculate accessibility score
        let error_weight = 0.5;
        let warning_weight = 0.2;
        let max_deduction = error_weight * self.summary.errors as f64
            + warning_weight * self.summary.warnings as f64;

        self.summary.accessibility_score = (1.0 - (max_deduction / 10.0)).max(0.0);

        // Determine compliance level
        self.compliance_level = if self.summary.errors == 0 && self.summary.warnings == 0 {
            ComplianceLevel::AAA
        } else if self.summary.errors == 0 && self.summary.warnings <= 5 {
            ComplianceLevel::AA
        } else if self.summary.errors <= 2 {
            ComplianceLevel::A
        } else {
            ComplianceLevel::NonCompliant
        };
    }
}
