use crate::ast::{NodeId, NodeType, PdfDocument};
use crate::types::{PdfArray, PdfDictionary, PdfStream, PdfValue};
use crate::validation::{
    ConstraintCategory, SchemaConstraint, ValidationIssue, ValidationReport, ValidationSeverity,
};
use std::collections::HashSet;

/// PDF/A-1b validator implementing ISO 19005-1:2005 Level B requirements
pub struct PdfA1bValidator {
    strict_mode: bool,
}

#[derive(Debug, Clone, Copy)]
enum PdfA1bConstraintKind {
    Version,
    ColorSpaces,
    Fonts,
    Images,
    Multimedia,
    JavaScript,
    Annotations,
    Forms,
    Encryption,
    Metadata,
    Transparency,
    FileSpecification,
    CrossReference,
}

#[derive(Debug, Clone, Copy)]
struct PdfA1bConstraint {
    strict_mode: bool,
    kind: PdfA1bConstraintKind,
}

impl PdfA1bConstraint {
    fn new(strict_mode: bool, kind: PdfA1bConstraintKind) -> Self {
        Self { strict_mode, kind }
    }
}

impl SchemaConstraint for PdfA1bConstraint {
    fn name(&self) -> &str {
        match self.kind {
            PdfA1bConstraintKind::Version => "pdfa-1b-version",
            PdfA1bConstraintKind::ColorSpaces => "pdfa-1b-color-spaces",
            PdfA1bConstraintKind::Fonts => "pdfa-1b-fonts",
            PdfA1bConstraintKind::Images => "pdfa-1b-images",
            PdfA1bConstraintKind::Multimedia => "pdfa-1b-multimedia",
            PdfA1bConstraintKind::JavaScript => "pdfa-1b-javascript",
            PdfA1bConstraintKind::Annotations => "pdfa-1b-annotations",
            PdfA1bConstraintKind::Forms => "pdfa-1b-forms",
            PdfA1bConstraintKind::Encryption => "pdfa-1b-encryption",
            PdfA1bConstraintKind::Metadata => "pdfa-1b-metadata",
            PdfA1bConstraintKind::Transparency => "pdfa-1b-transparency",
            PdfA1bConstraintKind::FileSpecification => "pdfa-1b-file-specification",
            PdfA1bConstraintKind::CrossReference => "pdfa-1b-cross-reference",
        }
    }

    fn description(&self) -> &str {
        "Selected PDF/A-1b preflight constraint"
    }

    fn category(&self) -> ConstraintCategory {
        match self.kind {
            PdfA1bConstraintKind::Version
            | PdfA1bConstraintKind::CrossReference
            | PdfA1bConstraintKind::FileSpecification => ConstraintCategory::Structure,
            PdfA1bConstraintKind::ColorSpaces
            | PdfA1bConstraintKind::Images
            | PdfA1bConstraintKind::Transparency => ConstraintCategory::Graphics,
            PdfA1bConstraintKind::Fonts => ConstraintCategory::Fonts,
            PdfA1bConstraintKind::Multimedia | PdfA1bConstraintKind::Annotations => {
                ConstraintCategory::Annotations
            }
            PdfA1bConstraintKind::JavaScript => ConstraintCategory::JavaScript,
            PdfA1bConstraintKind::Forms => ConstraintCategory::Forms,
            PdfA1bConstraintKind::Encryption => ConstraintCategory::Security,
            PdfA1bConstraintKind::Metadata => ConstraintCategory::Metadata,
        }
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let validator = PdfA1bValidator {
            strict_mode: self.strict_mode,
        };
        let failed_before = report.statistics.failed_checks;

        match self.kind {
            PdfA1bConstraintKind::Version => validator.validate_version(report, document),
            PdfA1bConstraintKind::ColorSpaces => validator.validate_color_spaces(report, document),
            PdfA1bConstraintKind::Fonts => validator.validate_fonts(report, document),
            PdfA1bConstraintKind::Images => validator.validate_images(report, document),
            PdfA1bConstraintKind::Multimedia => {
                validator.validate_multimedia_content(report, document)
            }
            PdfA1bConstraintKind::JavaScript => validator.validate_javascript(report, document),
            PdfA1bConstraintKind::Annotations => validator.validate_annotations(report, document),
            PdfA1bConstraintKind::Forms => validator.validate_forms(report, document),
            PdfA1bConstraintKind::Encryption => validator.validate_encryption(report, document),
            PdfA1bConstraintKind::Metadata => validator.validate_metadata(report, document),
            PdfA1bConstraintKind::Transparency => validator.validate_transparency(report, document),
            PdfA1bConstraintKind::FileSpecification => {
                validator.validate_file_specification(report, document)
            }
            PdfA1bConstraintKind::CrossReference => {
                validator.validate_cross_reference(report, document)
            }
        }

        if report.statistics.failed_checks == failed_before {
            report.add_passed_check();
        } else {
            report.statistics.total_checks += 1;
        }
    }
}

impl PdfA1bValidator {
    pub fn new() -> Self {
        Self { strict_mode: true }
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub(crate) fn constraints(&self) -> Vec<Box<dyn SchemaConstraint>> {
        [
            PdfA1bConstraintKind::Version,
            PdfA1bConstraintKind::ColorSpaces,
            PdfA1bConstraintKind::Fonts,
            PdfA1bConstraintKind::Images,
            PdfA1bConstraintKind::Multimedia,
            PdfA1bConstraintKind::JavaScript,
            PdfA1bConstraintKind::Annotations,
            PdfA1bConstraintKind::Forms,
            PdfA1bConstraintKind::Encryption,
            PdfA1bConstraintKind::Metadata,
            PdfA1bConstraintKind::Transparency,
            PdfA1bConstraintKind::FileSpecification,
            PdfA1bConstraintKind::CrossReference,
        ]
        .into_iter()
        .map(|kind| Box::new(PdfA1bConstraint::new(self.strict_mode, kind)) as _)
        .collect()
    }

    pub fn validate(&self, document: &PdfDocument) -> ValidationReport {
        let mut report = ValidationReport::new("PDF/A-1b".to_string(), "1.0".to_string());
        for constraint in self.constraints() {
            constraint.check(document, &mut report);
        }
        report.finalize();

        report
    }

    fn validate_version(&self, report: &mut ValidationReport, document: &PdfDocument) {
        if document.version.major != 1 || document.version.minor > 4 {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_VERSION".to_string(),
                message: "PDF/A-1 must be based on PDF version 1.4 or earlier".to_string(),
                node_id: None,
                location: Some("Document version".to_string()),
                suggestion: Some(format!(
                    "Found version {}.{}",
                    document.version.major, document.version.minor
                )),
            });
        }
    }

    fn validate_color_spaces(&self, report: &mut ValidationReport, document: &PdfDocument) {
        let mut has_device_colors = false;
        let mut missing_output_intent = true;

        if let Some(catalog_dict) = document.get_catalog() {
            if let Some(output_intents) = catalog_dict.get("OutputIntents") {
                if let Some(intents) = Self::resolve_array(document, output_intents) {
                    if !intents.is_empty() {
                        missing_output_intent = false;
                    }
                    self.validate_output_intents(report, document, intents);
                } else {
                    report.add_issue(ValidationIssue {
                        severity: ValidationSeverity::Error,
                        code: "PDF_A_OUTPUT_INTENT".to_string(),
                        message: "OutputIntents must be an array".to_string(),
                        node_id: None,
                        location: Some("Catalog OutputIntents".to_string()),
                        suggestion: Some("Provide a non-empty OutputIntents array".to_string()),
                    });
                }
            }
        }

        for node in document.ast.get_all_nodes() {
            match &node.node_type {
                NodeType::Image => {
                    if let Some(dict) = node.as_dict() {
                        if let Some(colorspace_value) = dict.get("ColorSpace") {
                            if let Some(colorspace_name) = colorspace_value.as_name() {
                                match colorspace_name.without_slash() {
                                    "DeviceRGB" | "DeviceGray" | "DeviceCMYK" => {
                                        has_device_colors = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                NodeType::Page => {
                    self.check_inherited_page_resources(document, node.id, &mut has_device_colors);
                }
                _ => {}
            }
        }

        if has_device_colors && missing_output_intent {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_COLOR_SPACE".to_string(),
                message: "Device color spaces require OutputIntent specification".to_string(),
                node_id: None,
                location: Some("Color management".to_string()),
                suggestion: Some(
                    "Found device color spaces but no OutputIntents in catalog".to_string(),
                ),
            });
        }

        if missing_output_intent && self.strict_mode {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "PDF_A_OUTPUT_INTENT".to_string(),
                message: "PDF/A-1b should include OutputIntents for color management".to_string(),
                node_id: None,
                location: Some("Color management".to_string()),
                suggestion: None,
            });
        }
    }

    fn validate_output_intents(
        &self,
        report: &mut ValidationReport,
        document: &PdfDocument,
        intents: &PdfArray,
    ) {
        if intents.is_empty() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_OUTPUT_INTENT".to_string(),
                message: "PDF/A-1b requires a non-empty OutputIntents array".to_string(),
                node_id: None,
                location: Some("Catalog OutputIntents".to_string()),
                suggestion: None,
            });
            return;
        }

        for intent in intents {
            let valid = Self::resolve_dictionary(document, intent).is_some_and(|dict| {
                matches!(
                    dict.get("S").and_then(PdfValue::as_name),
                    Some(name) if name.without_slash() == "GTS_PDFA1"
                ) && matches!(
                    dict.get("OutputConditionIdentifier")
                        .and_then(PdfValue::as_string),
                    Some(identifier) if !identifier.as_bytes().is_empty()
                ) && dict
                    .get("DestOutputProfile")
                    .and_then(|value| Self::resolve_stream(document, value))
                    .is_some()
            });
            if !valid {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "PDF_A_OUTPUT_INTENT".to_string(),
                    message: "OutputIntent must define GTS_PDFA1, an identifier, and an ICC profile"
                        .to_string(),
                    node_id: None,
                    location: Some("Catalog OutputIntents".to_string()),
                    suggestion: Some(
                        "Add OutputConditionIdentifier and DestOutputProfile to the PDF/A output intent"
                            .to_string(),
                    ),
                });
            }
        }
    }

    fn check_resources_for_device_colors(
        &self,
        document: &PdfDocument,
        page_dict: &PdfDictionary,
        has_device_colors: &mut bool,
    ) {
        if let Some(resources_value) = page_dict.get("Resources") {
            let resources = match resources_value {
                PdfValue::Dictionary(dict) => Some(dict),
                PdfValue::Reference(reference) => document
                    .ast
                    .get_node_by_object(reference.id())
                    .and_then(|node| node.as_dict()),
                _ => None,
            };
            if let Some(resources_dict) = resources {
                if let Some(colorspaces_value) = resources_dict.get("ColorSpace") {
                    let mut visited = HashSet::new();
                    *has_device_colors |=
                        Self::contains_device_colors(document, colorspaces_value, &mut visited);
                }
            }
        }
    }

    fn check_inherited_page_resources(
        &self,
        document: &PdfDocument,
        page_id: NodeId,
        has_device_colors: &mut bool,
    ) {
        let mut current = Some(page_id);
        let mut visited = HashSet::new();

        while let Some(node_id) = current {
            if !visited.insert(node_id) {
                break;
            }
            if let Some(node) = document.ast.get_node(node_id) {
                if let Some(dict) = node.as_dict() {
                    self.check_resources_for_device_colors(document, dict, has_device_colors);
                }
            }
            current = document.ast.get_parent(node_id);
        }
    }

    fn contains_device_colors(
        document: &PdfDocument,
        value: &PdfValue,
        visited: &mut HashSet<NodeId>,
    ) -> bool {
        match value {
            PdfValue::Name(name) => matches!(
                name.without_slash(),
                "DeviceRGB" | "DeviceGray" | "DeviceCMYK"
            ),
            PdfValue::Array(values) => values
                .iter()
                .any(|value| Self::contains_device_colors(document, value, visited)),
            PdfValue::Dictionary(dict) => dict
                .values()
                .any(|value| Self::contains_device_colors(document, value, visited)),
            PdfValue::Reference(reference) => {
                let Some(node) = document.ast.get_node_by_object(reference.id()) else {
                    return false;
                };
                if !visited.insert(node.id) {
                    return false;
                }
                Self::contains_device_colors(document, &node.value, visited)
            }
            _ => false,
        }
    }

    fn validate_fonts(&self, report: &mut ValidationReport, document: &PdfDocument) {
        let mut unembedded_fonts = Vec::new();
        let mut invalid_encodings = Vec::new();

        for node in document.ast.get_all_nodes() {
            if matches!(
                node.node_type,
                NodeType::Font
                    | NodeType::Type1Font
                    | NodeType::TrueTypeFont
                    | NodeType::Type3Font
                    | NodeType::CIDFont
            ) {
                if let Some(font_dict) = node.as_dict() {
                    let font_name = font_dict
                        .get("BaseFont")
                        .and_then(|v| v.as_name())
                        .map(|n| n.without_slash())
                        .unwrap_or("Unknown");

                    // PDF/A-1b requires ALL fonts to be embedded, including the standard 14 fonts
                    let is_embedded = self.is_font_embedded(font_dict, document);
                    if !is_embedded {
                        unembedded_fonts.push(font_name.to_string());

                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            code: "PDF_A_FONT_EMBEDDING".to_string(),
                            message: "All fonts must be embedded in PDF/A-1b".to_string(),
                            node_id: Some(node.id),
                            location: Some("Font embedding".to_string()),
                            suggestion: Some(format!("Font '{}' is not embedded", font_name)),
                        });
                    }

                    if let Some(subtype) = font_dict.get("Subtype").and_then(|v| v.as_name()) {
                        if subtype.without_slash() != "Type3" {
                            self.validate_font_encoding(
                                font_dict,
                                font_name,
                                &mut invalid_encodings,
                            );
                        }
                    }
                }
            }
        }

        for encoding_issue in invalid_encodings {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_FONT_ENCODING".to_string(),
                message: "Font encoding must be specified or use standard encoding".to_string(),
                node_id: None,
                location: Some("Font encoding".to_string()),
                suggestion: Some(encoding_issue),
            });
        }
    }

    fn is_font_embedded(&self, font_dict: &PdfDictionary, document: &PdfDocument) -> bool {
        if Self::has_embedded_font_program(font_dict, document) {
            return true;
        }

        font_dict
            .get("DescendantFonts")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| match value {
                PdfValue::Dictionary(dict) => Some(dict.clone()),
                PdfValue::Reference(reference) => document
                    .ast
                    .get_node_by_object(reference.id())
                    .and_then(|node| node.as_dict().cloned()),
                _ => None,
            })
            .any(|dict| Self::has_embedded_font_program(&dict, document))
    }

    fn has_embedded_font_program(dict: &PdfDictionary, document: &PdfDocument) -> bool {
        Self::has_embedded_font_program_with_visited(dict, document, &mut HashSet::new())
    }

    fn has_embedded_font_program_with_visited(
        dict: &PdfDictionary,
        document: &PdfDocument,
        visited: &mut HashSet<NodeId>,
    ) -> bool {
        for key in ["FontFile", "FontFile2", "FontFile3", "CIDFontFile"] {
            if let Some(value) = dict.get(key) {
                if Self::is_embedded_font_program(value, document, visited) {
                    return true;
                }
            }
        }

        match dict.get("FontDescriptor") {
            Some(PdfValue::Dictionary(descriptor)) => {
                Self::has_embedded_font_program_with_visited(descriptor, document, visited)
            }
            Some(PdfValue::Reference(reference)) => {
                let Some(node_id) = document
                    .ast
                    .get_node_by_object(reference.id())
                    .map(|node| node.id)
                else {
                    return false;
                };
                if !visited.insert(node_id) {
                    return false;
                }
                let result = document
                    .ast
                    .get_node(node_id)
                    .and_then(|node| node.as_dict())
                    .is_some_and(|descriptor| {
                        Self::has_embedded_font_program_with_visited(descriptor, document, visited)
                    });
                visited.remove(&node_id);
                result
            }
            _ => false,
        }
    }

    fn is_embedded_font_program(
        value: &PdfValue,
        document: &PdfDocument,
        visited: &mut HashSet<NodeId>,
    ) -> bool {
        match value {
            PdfValue::Stream(_) => true,
            PdfValue::Reference(reference) => {
                let Some(node) = document.ast.get_node_by_object(reference.id()) else {
                    return false;
                };
                if !visited.insert(node.id) {
                    return false;
                }
                let result = matches!(node.value, PdfValue::Stream(_));
                visited.remove(&node.id);
                result
            }
            _ => false,
        }
    }

    fn is_standard_font(&self, font_name: &str) -> bool {
        matches!(
            font_name,
            "Times-Roman"
                | "Times-Bold"
                | "Times-Italic"
                | "Times-BoldItalic"
                | "Helvetica"
                | "Helvetica-Bold"
                | "Helvetica-Oblique"
                | "Helvetica-BoldOblique"
                | "Courier"
                | "Courier-Bold"
                | "Courier-Oblique"
                | "Courier-BoldOblique"
                | "Symbol"
                | "ZapfDingbats"
        )
    }

    fn validate_font_encoding(
        &self,
        font_dict: &PdfDictionary,
        font_name: &str,
        invalid_encodings: &mut Vec<String>,
    ) {
        if !font_dict.contains_key("Encoding") && !self.is_standard_font(font_name) {
            if let Some(subtype) = font_dict.get("Subtype").and_then(|v| v.as_name()) {
                if matches!(subtype.without_slash(), "Type1" | "MMType1" | "TrueType") {
                    invalid_encodings
                        .push(format!("Font '{}' lacks encoding specification", font_name));
                }
            }
        }
    }

    fn validate_images(&self, report: &mut ValidationReport, document: &PdfDocument) {
        for node in document.ast.get_all_nodes() {
            if matches!(node.node_type, NodeType::Image | NodeType::ImageXObject) {
                if let Some(image_dict) = node.as_dict() {
                    if let Some(filter_value) = image_dict.get("Filter") {
                        let has_lzw = match filter_value {
                            PdfValue::Name(name) => name.without_slash() == "LZWDecode",
                            PdfValue::Array(filters) => filters.iter().any(|f| {
                                f.as_name()
                                    .map(|n| n.without_slash() == "LZWDecode")
                                    .unwrap_or(false)
                            }),
                            _ => false,
                        };

                        if has_lzw && self.strict_mode {
                            report.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                code: "PDF_A_LZW_DECODE".to_string(),
                                message: "LZWDecode filter should be avoided in PDF/A-1"
                                    .to_string(),
                                node_id: None,
                                location: Some("Image compression".to_string()),
                                suggestion: Some("Consider using FlateDecode instead".to_string()),
                            });
                        }
                    }
                }
            }
        }
    }

    fn validate_multimedia_content(&self, report: &mut ValidationReport, document: &PdfDocument) {
        let mut has_multimedia = false;

        for node in document.ast.get_all_nodes() {
            if node.node_type == NodeType::Annotation {
                if let Some(annot_dict) = node.as_dict() {
                    if let Some(subtype) = annot_dict.get("Subtype").and_then(|v| v.as_name()) {
                        match subtype.without_slash() {
                            "3D" | "Movie" | "Sound" | "Screen" | "RichMedia" => {
                                has_multimedia = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if has_multimedia {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_MULTIMEDIA".to_string(),
                message: "PDF/A-1b does not permit multimedia content".to_string(),
                node_id: None,
                location: Some("Multimedia restrictions".to_string()),
                suggestion: Some(
                    "Remove multimedia annotations like Movie, Sound, or Screen".to_string(),
                ),
            });
        }
    }

    fn validate_javascript(&self, report: &mut ValidationReport, document: &PdfDocument) {
        for node in document.ast.get_all_nodes() {
            if matches!(node.node_type, NodeType::JavaScriptAction) {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "PDF_A_JAVASCRIPT".to_string(),
                    message: "JavaScript is not permitted in PDF/A-1b".to_string(),
                    node_id: Some(node.id),
                    location: Some("JavaScript action node".to_string()),
                    suggestion: Some("Remove all JavaScript actions".to_string()),
                });
                return;
            }
        }

        let mut has_javascript = false;

        if let Some(catalog_dict) = document.get_catalog() {
            if let Some(names_value) = catalog_dict.get("Names") {
                if let Some(names_dict) = Self::resolve_dictionary(document, names_value) {
                    if names_dict.contains_key("JavaScript") {
                        has_javascript = true;
                    }
                }
            }

            if let Some(open_action) = catalog_dict.get("OpenAction") {
                if let Some(action_dict) = Self::resolve_dictionary(document, open_action) {
                    if let Some(s_value) = action_dict.get("S") {
                        if let Some(s_name) = s_value.as_name() {
                            if s_name.without_slash() == "JavaScript" {
                                has_javascript = true;
                            }
                        }
                    }
                }
            }
        }

        for node in document.ast.get_all_nodes() {
            if let Some(dict) = node.as_dict() {
                if let Some(type_value) = dict.get("Type") {
                    if let Some(type_name) = type_value.as_name() {
                        if type_name.without_slash() == "Action" {
                            if let Some(s_value) = dict.get("S") {
                                if let Some(s_name) = s_value.as_name() {
                                    if s_name.without_slash() == "JavaScript" {
                                        has_javascript = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if matches!(node.node_type, NodeType::Annotation | NodeType::Action) {
                    if let Some(s_value) = dict.get("S") {
                        if let Some(s_name) = s_value.as_name() {
                            if s_name.without_slash() == "JavaScript" {
                                has_javascript = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        if has_javascript {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_JAVASCRIPT".to_string(),
                message: "PDF/A-1b does not permit JavaScript".to_string(),
                node_id: None,
                location: Some("JavaScript restrictions".to_string()),
                suggestion: Some("Remove all JavaScript actions and scripts".to_string()),
            });
        }
    }

    fn validate_annotations(&self, report: &mut ValidationReport, document: &PdfDocument) {
        let prohibited_subtypes = ["Movie", "Sound", "FileAttachment"];

        for node in document.ast.get_all_nodes() {
            if matches!(node.node_type, NodeType::Annotation) {
                if let Some(annot_dict) = node.as_dict() {
                    if let Some(subtype) = annot_dict.get("Subtype").and_then(|v| v.as_name()) {
                        let subtype_str = subtype.without_slash();
                        if prohibited_subtypes.contains(&subtype_str) {
                            report.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                code: "PDF_A_ANNOTATION_TYPE".to_string(),
                                message: format!(
                                    "Annotation subtype '{}' not permitted in PDF/A-1b",
                                    subtype_str
                                ),
                                node_id: None,
                                location: Some("Annotation restrictions".to_string()),
                                suggestion: None,
                            });
                        }

                        if !annot_dict.contains_key("AP") && subtype_str != "Popup" {
                            report.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                code: "PDF_A_ANNOTATION_APPEARANCE".to_string(),
                                message: "Annotations should have appearance streams in PDF/A-1b"
                                    .to_string(),
                                node_id: None,
                                location: Some("Annotation appearance".to_string()),
                                suggestion: Some(format!(
                                    "Annotation of type '{}' lacks appearance",
                                    subtype_str
                                )),
                            });
                        }
                    }
                }
            }
        }
    }

    fn validate_forms(&self, report: &mut ValidationReport, document: &PdfDocument) {
        if let Some(catalog_dict) = document.get_catalog() {
            if let Some(acroform_value) = catalog_dict.get("AcroForm") {
                if let Some(acroform_dict) = Self::resolve_dictionary(document, acroform_value) {
                    if acroform_dict.contains_key("XFA") {
                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            code: "PDF_A_XFA".to_string(),
                            message: "XFA forms are not permitted in PDF/A-1b".to_string(),
                            node_id: None,
                            location: Some("Form restrictions".to_string()),
                            suggestion: Some("Use AcroForm instead of XFA".to_string()),
                        });
                    }
                }
            }
        }
    }

    fn validate_encryption(&self, report: &mut ValidationReport, document: &PdfDocument) {
        if document.metadata.encrypted {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_ENCRYPTION".to_string(),
                message: "PDF/A-1b documents must not be encrypted".to_string(),
                node_id: None,
                location: Some("Encryption restrictions".to_string()),
                suggestion: Some("Remove all encryption from the document".to_string()),
            });
        }
    }

    fn validate_metadata(&self, report: &mut ValidationReport, document: &PdfDocument) {
        let metadata_stream = document
            .get_catalog()
            .and_then(|catalog| catalog.get("Metadata"))
            .and_then(|value| Self::resolve_stream(document, value));

        let has_valid_xmp_metadata = metadata_stream.is_some_and(|stream| {
            matches!(
                stream.dict.get("Type").and_then(PdfValue::as_name),
                Some(name) if name.without_slash() == "Metadata"
            ) && matches!(
                stream.dict.get("Subtype").and_then(PdfValue::as_name),
                Some(name) if name.without_slash() == "XML"
            )
        });

        if !has_valid_xmp_metadata {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_XMP_METADATA".to_string(),
                message: "PDF/A-1b requires a Metadata XML stream in catalog".to_string(),
                node_id: None,
                location: Some("Metadata requirements".to_string()),
                suggestion: Some("Add XMP metadata stream to document catalog".to_string()),
            });
        }

        // Full XMP-Info synchronization requires parsing XMP content
        if self.strict_mode {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "PDF_A_METADATA_SYNC".to_string(),
                message: "Verify XMP metadata synchronization with Info dictionary".to_string(),
                node_id: None,
                location: Some("Metadata synchronization".to_string()),
                suggestion: None,
            });
        }
    }

    fn resolve_dictionary<'a>(
        document: &'a PdfDocument,
        value: &'a PdfValue,
    ) -> Option<&'a PdfDictionary> {
        match value {
            PdfValue::Dictionary(dict) => Some(dict),
            PdfValue::Reference(reference) => document
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_dict()),
            _ => None,
        }
    }

    fn resolve_array<'a>(document: &'a PdfDocument, value: &'a PdfValue) -> Option<&'a PdfArray> {
        match value {
            PdfValue::Array(array) => Some(array),
            PdfValue::Reference(reference) => document
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| match &node.value {
                    PdfValue::Array(array) => Some(array),
                    _ => None,
                }),
            _ => None,
        }
    }

    fn resolve_stream<'a>(document: &'a PdfDocument, value: &'a PdfValue) -> Option<&'a PdfStream> {
        match value {
            PdfValue::Stream(stream) => Some(stream),
            PdfValue::Reference(reference) => document
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_stream()),
            _ => None,
        }
    }

    fn validate_transparency(&self, report: &mut ValidationReport, document: &PdfDocument) {
        for node in document.ast.get_all_nodes() {
            if let Some(dict) = node.as_dict() {
                // BM=blend mode, CA/ca=opacity, SMask=soft mask - all indicate transparency
                if dict.contains_key("BM")
                    || dict.contains_key("CA")
                    || dict.contains_key("ca")
                    || dict.contains_key("SMask")
                {
                    report.add_issue(ValidationIssue {
                        severity: ValidationSeverity::Error,
                        code: "PDF_A_TRANSPARENCY".to_string(),
                        message: "PDF/A-1b does not permit transparency in graphics states"
                            .to_string(),
                        node_id: Some(node.id),
                        location: Some("Graphics state".to_string()),
                        suggestion: Some("Remove transparency effects from ExtGState".to_string()),
                    });
                    return; // Found transparency, report and exit
                }

                if let Some(type_value) = dict.get("Type") {
                    if let Some(type_name) = type_value.as_name() {
                        if type_name.without_slash() == "Group" {
                            if let Some(s_value) = dict.get("S") {
                                if let Some(s_name) = s_value.as_name() {
                                    if s_name.without_slash() == "Transparency" {
                                        report.add_issue(ValidationIssue {
                                            severity: ValidationSeverity::Error,
                                            code: "PDF_A_TRANSPARENCY".to_string(),
                                            message: "PDF/A-1b does not permit transparency groups"
                                                .to_string(),
                                            node_id: Some(node.id),
                                            location: Some("Transparency group".to_string()),
                                            suggestion: Some(
                                                "Remove transparency group specification"
                                                    .to_string(),
                                            ),
                                        });
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(s_value) = dict.get("S") {
                    if let Some(s_name) = s_value.as_name() {
                        if s_name.without_slash() == "Transparency" {
                            if let Some(type_value) = dict.get("Type") {
                                if let Some(type_name) = type_value.as_name() {
                                    if type_name.without_slash() == "Group" {
                                        report.add_issue(ValidationIssue {
                                            severity: ValidationSeverity::Error,
                                            code: "PDF_A_TRANSPARENCY".to_string(),
                                            message: "PDF/A-1b does not permit transparency groups"
                                                .to_string(),
                                            node_id: Some(node.id),
                                            location: Some("Transparency group".to_string()),
                                            suggestion: Some(
                                                "Remove transparency group specification"
                                                    .to_string(),
                                            ),
                                        });
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(group_value) = dict.get("Group") {
                    if let Some(group_dict) = group_value.as_dict() {
                        if let Some(s_value) = group_dict.get("S") {
                            if let Some(s_name) = s_value.as_name() {
                                if s_name.without_slash() == "Transparency" {
                                    report.add_issue(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        code: "PDF_A_TRANSPARENCY".to_string(),
                                        message: "PDF/A-1b does not permit transparency groups"
                                            .to_string(),
                                        node_id: Some(node.id),
                                        location: Some("Transparency group".to_string()),
                                        suggestion: Some(
                                            "Remove transparency group specification".to_string(),
                                        ),
                                    });
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn validate_file_specification(&self, report: &mut ValidationReport, document: &PdfDocument) {
        if document.metadata.has_embedded_files {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "PDF_A_EMBEDDED_FILES".to_string(),
                message: "PDF/A-1b does not permit embedded files".to_string(),
                node_id: None,
                location: Some("File specification restrictions".to_string()),
                suggestion: Some("Remove all embedded file attachments".to_string()),
            });
        }
    }

    fn validate_cross_reference(&self, report: &mut ValidationReport, document: &PdfDocument) {
        // PDF/A-1b allows tables or streams, but mixing both is discouraged
        let has_xref_tables = !document.xref.entries.is_empty();
        let has_xref_streams = !document.xref.streams.is_empty();

        if has_xref_tables && has_xref_streams {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "PDF_A_XREF_FORMAT".to_string(),
                message: "Mixed cross-reference formats detected".to_string(),
                node_id: None,
                location: Some("Cross-reference validation".to_string()),
                suggestion: Some(
                    "Consider using consistent cross-reference format throughout".to_string(),
                ),
            });
        }
    }
}

impl Default for PdfA1bValidator {
    fn default() -> Self {
        Self::new()
    }
}
