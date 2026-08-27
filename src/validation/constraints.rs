use super::*;
use crate::ast::{AstNode, NodeType, PdfDocument};
use crate::types::{PdfDictionary, PdfStream, PdfValue};

fn resolve_node_from_value<'a>(document: &'a PdfDocument, value: &PdfValue) -> Option<&'a AstNode> {
    match value {
        PdfValue::Reference(reference) => document.ast.get_node_by_object(reference.id()),
        _ => None,
    }
}

fn resolve_dict_from_value(document: &PdfDocument, value: &PdfValue) -> Option<PdfDictionary> {
    match value {
        PdfValue::Dictionary(dict) => Some(dict.clone()),
        PdfValue::Stream(stream) => Some(stream.dict.clone()),
        PdfValue::Reference(_) => resolve_node_from_value(document, value).and_then(|node| {
            node.as_dict()
                .cloned()
                .or_else(|| node.as_stream().map(|s| s.dict.clone()))
        }),
        _ => None,
    }
}

fn resolve_stream_from_value(document: &PdfDocument, value: &PdfValue) -> Option<PdfStream> {
    match value {
        PdfValue::Stream(stream) => Some(stream.clone()),
        PdfValue::Reference(_) => {
            resolve_node_from_value(document, value).and_then(|node| node.as_stream().cloned())
        }
        _ => None,
    }
}

fn has_embedded_font_program(
    dict: &PdfDictionary,
    document: &PdfDocument,
    visited: &mut HashSet<NodeId>,
) -> bool {
    for key in ["FontFile", "FontFile2", "FontFile3", "CIDFontFile"] {
        let Some(value) = dict.get(key) else {
            continue;
        };
        let is_stream = match value {
            PdfValue::Stream(_) => true,
            PdfValue::Reference(reference) => match document.ast.get_node_by_object(reference.id())
            {
                Some(node) if visited.insert(node.id) => {
                    let result = matches!(node.value, PdfValue::Stream(_));
                    visited.remove(&node.id);
                    result
                }
                _ => false,
            },
            _ => false,
        };
        if is_stream {
            return true;
        }
    }

    match dict.get("FontDescriptor") {
        Some(PdfValue::Dictionary(descriptor)) => {
            has_embedded_font_program(descriptor, document, visited)
        }
        Some(PdfValue::Reference(reference)) => {
            let Some(node) = document.ast.get_node_by_object(reference.id()) else {
                return false;
            };
            if !visited.insert(node.id) {
                return false;
            }
            let result = node
                .as_dict()
                .is_some_and(|descriptor| has_embedded_font_program(descriptor, document, visited));
            visited.remove(&node.id);
            result
        }
        _ => false,
    }
}

fn value_contains_name(value: &PdfValue, name: &str) -> bool {
    match value {
        PdfValue::Name(n) => n.without_slash() == name || n.as_str() == name,
        PdfValue::Array(arr) => arr.iter().any(|v| value_contains_name(v, name)),
        PdfValue::Dictionary(dict) => dict.values().any(|v| value_contains_name(v, name)),
        PdfValue::Stream(stream) => stream.dict.values().any(|v| value_contains_name(v, name)),
        _ => false,
    }
}

/// Basic constraint: Document must have a catalog
pub struct HasCatalogConstraint;

impl SchemaConstraint for HasCatalogConstraint {
    fn name(&self) -> &str {
        "has-catalog"
    }

    fn description(&self) -> &str {
        "Document must have a catalog dictionary"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Catalog dictionary")
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::Catalog]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let catalog_nodes = document.ast.find_nodes_by_type(NodeType::Catalog);

        if catalog_nodes.is_empty() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Critical,
                code: "CATALOG_MISSING".to_string(),
                message: "Document must contain a catalog dictionary".to_string(),
                node_id: None,
                location: Some("Document root".to_string()),
                suggestion: Some("Add a catalog dictionary to the document".to_string()),
            });
        } else if catalog_nodes.len() > 1 {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "MULTIPLE_CATALOGS".to_string(),
                message: "Document contains multiple catalog dictionaries".to_string(),
                node_id: Some(NodeId::new(catalog_nodes[1].index())),
                location: Some("Document structure".to_string()),
                suggestion: Some("Remove duplicate catalog dictionaries".to_string()),
            });
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: Trailer must have Root entry
pub struct HasTrailerRootConstraint;

impl SchemaConstraint for HasTrailerRootConstraint {
    fn name(&self) -> &str {
        "has-trailer-root"
    }

    fn description(&self) -> &str {
        "Trailer must contain /Root"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 File trailer")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if document.trailer.contains_key("Root") {
            report.add_passed_check();
        } else {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "TRAILER_ROOT_MISSING".to_string(),
                message: "Trailer dictionary missing /Root".to_string(),
                node_id: None,
                location: Some("Trailer".to_string()),
                suggestion: Some("Add /Root entry in trailer".to_string()),
            });
        }
    }
}

/// Constraint: Trailer must have Size entry
pub struct HasTrailerSizeConstraint;

impl SchemaConstraint for HasTrailerSizeConstraint {
    fn name(&self) -> &str {
        "has-trailer-size"
    }

    fn description(&self) -> &str {
        "Trailer must contain /Size"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 File trailer")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if let Some(size) = document.trailer.get("Size").and_then(|v| v.as_integer()) {
            if size > 0 {
                report.add_passed_check();
                return;
            }
        }
        report.add_issue(ValidationIssue {
            severity: ValidationSeverity::Error,
            code: "TRAILER_SIZE_MISSING".to_string(),
            message: "Trailer dictionary missing /Size or size <= 0".to_string(),
            node_id: None,
            location: Some("Trailer".to_string()),
            suggestion: Some("Add /Size entry in trailer".to_string()),
        });
    }
}

/// Constraint: PDF 2.0 should declare /Version 2.0
pub struct CatalogVersionConstraint;

impl SchemaConstraint for CatalogVersionConstraint {
    fn name(&self) -> &str {
        "catalog-version"
    }

    fn description(&self) -> &str {
        "Catalog /Version should be 2.0 when validating against PDF 2.0"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Header and catalog version")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let mut has_version = false;
        let mut is_2_0 = false;

        if let Some(catalog_id) = document.catalog {
            if let Some(node) = document.ast.get_node(catalog_id) {
                if let PdfValue::Dictionary(dict) = &node.value {
                    if let Some(version_value) = dict.get("Version") {
                        has_version = true;
                        match version_value {
                            PdfValue::Name(name) => {
                                let v = name.without_slash();
                                if v == "2.0" {
                                    is_2_0 = true;
                                }
                            }
                            PdfValue::String(s) if s.to_string_lossy() == "2.0" => is_2_0 = true,
                            _ => {}
                        }
                    }
                }
            }
        }

        if document.version.major >= 2 {
            if has_version && is_2_0 {
                report.add_passed_check();
            } else {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: "CATALOG_VERSION_MISSING".to_string(),
                    message: "Catalog /Version 2.0 not declared".to_string(),
                    node_id: document.catalog,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Add /Version 2.0 to catalog".to_string()),
                });
            }
        } else if has_version && is_2_0 {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "CATALOG_VERSION_MISMATCH".to_string(),
                message: "Catalog /Version 2.0 but header is older".to_string(),
                node_id: document.catalog,
                location: Some("Catalog".to_string()),
                suggestion: Some("Align header and /Version".to_string()),
            });
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: XRef table must contain entries
pub struct HasXRefEntriesConstraint;

impl SchemaConstraint for HasXRefEntriesConstraint {
    fn name(&self) -> &str {
        "has-xref-entries"
    }

    fn description(&self) -> &str {
        "Document must have at least one xref entry"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Cross-reference table")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if document.xref.entries.is_empty() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "XREF_MISSING".to_string(),
                message: "Cross-reference table has no entries".to_string(),
                node_id: None,
                location: Some("XRef".to_string()),
                suggestion: Some("Add xref entries or recover objects".to_string()),
            });
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: Trailer /Size must align with max object number
pub struct TrailerSizeConsistencyConstraint;

impl SchemaConstraint for TrailerSizeConsistencyConstraint {
    fn name(&self) -> &str {
        "trailer-size-consistency"
    }

    fn description(&self) -> &str {
        "Trailer /Size should be >= max object number + 1"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Cross-reference table and trailer")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let max_obj = document
            .xref
            .entries
            .keys()
            .map(|id| id.number as i64)
            .max()
            .unwrap_or(-1);

        if let Some(size) = document.trailer.get("Size").and_then(|v| v.as_integer()) {
            let expected_min = max_obj + 1;
            if size < expected_min {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: "TRAILER_SIZE_INCONSISTENT".to_string(),
                    message: format!("Trailer /Size {} is less than max object {}", size, max_obj),
                    node_id: None,
                    location: Some("Trailer".to_string()),
                    suggestion: Some("Update /Size to match objects".to_string()),
                });
            } else {
                report.add_passed_check();
            }
        } else {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "TRAILER_SIZE_MISSING".to_string(),
                message: "Trailer /Size missing for consistency check".to_string(),
                node_id: None,
                location: Some("Trailer".to_string()),
                suggestion: Some("Add /Size to trailer".to_string()),
            });
        }
    }
}

/// Constraint: Document must have a pages tree
pub struct HasPagesTreeConstraint;

impl SchemaConstraint for HasPagesTreeConstraint {
    fn name(&self) -> &str {
        "has-pages-tree"
    }

    fn description(&self) -> &str {
        "Document must have a pages tree"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Page tree")
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::Pages]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let pages_nodes = document.ast.find_nodes_by_type(NodeType::Pages);

        if pages_nodes.is_empty() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Critical,
                code: "PAGES_TREE_MISSING".to_string(),
                message: "Document must contain a pages tree".to_string(),
                node_id: None,
                location: Some("Document structure".to_string()),
                suggestion: Some("Add a pages tree to the document".to_string()),
            });
        } else {
            // Check that pages tree has correct structure
            let pages_node_id = pages_nodes[0];
            if let Some(pages_node) = document.ast.get_node(pages_node_id) {
                if let PdfValue::Dictionary(dict) = &pages_node.value {
                    if !dict.contains_key("Kids") {
                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            code: "PAGES_NO_KIDS".to_string(),
                            message: "Pages tree must contain Kids array".to_string(),
                            node_id: Some(pages_node_id),
                            location: Some("Pages dictionary".to_string()),
                            suggestion: Some("Add Kids array to pages dictionary".to_string()),
                        });
                    }

                    if !dict.contains_key("Count") {
                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            code: "PAGES_NO_COUNT".to_string(),
                            message: "Pages tree must contain Count entry".to_string(),
                            node_id: Some(pages_node_id),
                            location: Some("Pages dictionary".to_string()),
                            suggestion: Some("Add Count entry to pages dictionary".to_string()),
                        });
                    } else {
                        report.add_passed_check();
                    }
                }
            }
        }
    }
}

/// Constraint: Catalog must include Pages reference
pub struct CatalogHasPagesConstraint;

impl SchemaConstraint for CatalogHasPagesConstraint {
    fn name(&self) -> &str {
        "catalog-has-pages"
    }

    fn description(&self) -> &str {
        "Catalog must contain /Pages reference"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Catalog and pages tree")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if let Some(catalog_id) = document.catalog {
            if let Some(catalog_node) = document.ast.get_node(catalog_id) {
                if let PdfValue::Dictionary(dict) = &catalog_node.value {
                    if dict.contains_key("Pages") {
                        report.add_passed_check();
                        return;
                    }
                }
            }
        }

        report.add_issue(ValidationIssue {
            severity: ValidationSeverity::Error,
            code: "CATALOG_PAGES_MISSING".to_string(),
            message: "Catalog missing /Pages entry".to_string(),
            node_id: document.catalog,
            location: Some("Catalog".to_string()),
            suggestion: Some("Add /Pages reference to catalog".to_string()),
        });
    }
}

/// Constraint: Pages /Count should match actual page nodes
pub struct PageCountConsistencyConstraint;

impl SchemaConstraint for PageCountConsistencyConstraint {
    fn name(&self) -> &str {
        "pages-count-consistency"
    }

    fn description(&self) -> &str {
        "Pages /Count should match number of Page nodes"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Page tree count")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let page_nodes = document.ast.find_nodes_by_type(NodeType::Page);
        let actual = page_nodes.len() as i64;
        let mut reported = None;

        if let Some(pages_id) = document
            .ast
            .find_nodes_by_type(NodeType::Pages)
            .first()
            .copied()
        {
            if let Some(pages_node) = document.ast.get_node(pages_id) {
                if let PdfValue::Dictionary(dict) = &pages_node.value {
                    if let Some(count) = dict.get("Count").and_then(|v| v.as_integer()) {
                        reported = Some(count);
                    }
                }
            }
        }

        if let Some(count) = reported {
            if count != actual {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: "PAGES_COUNT_MISMATCH".to_string(),
                    message: format!(
                        "Pages /Count {} does not match actual pages {}",
                        count, actual
                    ),
                    node_id: document.catalog,
                    location: Some("Pages tree".to_string()),
                    suggestion: Some("Update /Count to match page nodes".to_string()),
                });
            } else {
                report.add_passed_check();
            }
        } else {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "PAGES_COUNT_MISSING".to_string(),
                message: "Pages /Count missing for consistency check".to_string(),
                node_id: document.catalog,
                location: Some("Pages tree".to_string()),
                suggestion: Some("Add /Count to pages tree".to_string()),
            });
        }
    }
}

/// Constraint: Trailer /ID must be an array of two strings
pub struct TrailerIdConstraint;

impl SchemaConstraint for TrailerIdConstraint {
    fn name(&self) -> &str {
        "trailer-id"
    }

    fn description(&self) -> &str {
        "Trailer /ID should be array of two strings"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 File identifiers")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if let Some(value) = document.trailer.get("ID") {
            if let PdfValue::Array(arr) = value {
                if arr.len() == 2 && arr.iter().all(|v| matches!(v, PdfValue::String(_))) {
                    report.add_passed_check();
                    return;
                }
            }

            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "TRAILER_ID_INVALID".to_string(),
                message: "Trailer /ID is malformed".to_string(),
                node_id: None,
                location: Some("Trailer".to_string()),
                suggestion: Some("Set /ID to array of two strings".to_string()),
            });
        } else {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                code: "TRAILER_ID_MISSING".to_string(),
                message: "Trailer /ID missing".to_string(),
                node_id: None,
                location: Some("Trailer".to_string()),
                suggestion: Some("Add /ID to trailer".to_string()),
            });
        }
    }
}

/// Constraint: Metadata stream must be /Subtype /XML
pub struct MetadataStreamConstraint;

impl SchemaConstraint for MetadataStreamConstraint {
    fn name(&self) -> &str {
        "metadata-stream"
    }

    fn description(&self) -> &str {
        "Metadata stream must be XML"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Metadata
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Metadata streams")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let catalog_id = match document.catalog {
            Some(id) => id,
            None => return,
        };

        let catalog = match document.ast.get_node(catalog_id) {
            Some(node) => node,
            None => return,
        };

        if let PdfValue::Dictionary(dict) = &catalog.value {
            if let Some(metadata) = dict.get("Metadata") {
                if let Some(stream) = resolve_stream_from_value(document, metadata) {
                    if let Some(PdfValue::Name(subtype)) = stream.dict.get("Subtype") {
                        if subtype.without_slash() == "XML" {
                            report.add_passed_check();
                            return;
                        }
                    }
                    report.add_issue(ValidationIssue {
                        severity: ValidationSeverity::Warning,
                        code: "METADATA_SUBTYPE_INVALID".to_string(),
                        message: "Metadata stream is not /Subtype /XML".to_string(),
                        node_id: Some(catalog_id),
                        location: Some("Metadata".to_string()),
                        suggestion: Some("Set metadata stream /Subtype to /XML".to_string()),
                    });
                } else {
                    report.add_issue(ValidationIssue {
                        severity: ValidationSeverity::Warning,
                        code: "METADATA_NOT_STREAM".to_string(),
                        message: "Metadata entry is not a stream".to_string(),
                        node_id: Some(catalog_id),
                        location: Some("Metadata".to_string()),
                        suggestion: Some("Use XMP metadata stream".to_string()),
                    });
                }
            }
        }
    }
}

/// Constraint: No encryption allowed
pub struct NoEncryptionConstraint;

impl SchemaConstraint for NoEncryptionConstraint {
    fn name(&self) -> &str {
        "no-encryption"
    }

    fn description(&self) -> &str {
        "Document must not be encrypted"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Security
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        if document.metadata.encrypted {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "ENCRYPTION_NOT_ALLOWED".to_string(),
                message: "Document encryption is not allowed in this profile".to_string(),
                node_id: None,
                location: Some("Document trailer".to_string()),
                suggestion: Some("Remove encryption from the document".to_string()),
            });
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: No JavaScript allowed
pub struct NoJavaScriptConstraint;

impl SchemaConstraint for NoJavaScriptConstraint {
    fn name(&self) -> &str {
        "no-javascript"
    }

    fn description(&self) -> &str {
        "Document must not contain JavaScript"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::JavaScript
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::JavaScriptAction, NodeType::EmbeddedJS]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let js_nodes = document.ast.find_nodes_by_type(NodeType::JavaScriptAction);
        let embedded_js_nodes = document.ast.find_nodes_by_type(NodeType::EmbeddedJS);

        if !js_nodes.is_empty() || !embedded_js_nodes.is_empty() {
            for node in js_nodes.iter().chain(embedded_js_nodes.iter()) {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "JAVASCRIPT_NOT_ALLOWED".to_string(),
                    message: "JavaScript is not allowed in this profile".to_string(),
                    node_id: Some(*node),
                    location: Some("JavaScript action or embedded script".to_string()),
                    suggestion: Some("Remove JavaScript code from the document".to_string()),
                });
            }
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: No external references allowed
pub struct NoExternalReferencesConstraint;

impl SchemaConstraint for NoExternalReferencesConstraint {
    fn name(&self) -> &str {
        "no-external-references"
    }

    fn description(&self) -> &str {
        "Document must not contain external references"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Security
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::ExternalReference, NodeType::URIAction]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let external_refs = document.ast.find_nodes_by_type(NodeType::ExternalReference);
        let uri_actions = document.ast.find_nodes_by_type(NodeType::URIAction);

        for node in external_refs.iter().chain(uri_actions.iter()) {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "EXTERNAL_REFERENCE_NOT_ALLOWED".to_string(),
                message: "External references are not allowed in this profile".to_string(),
                node_id: Some(*node),
                location: Some("External reference or URI action".to_string()),
                suggestion: Some("Remove external references from the document".to_string()),
            });
        }

        if external_refs.is_empty() && uri_actions.is_empty() {
            report.add_passed_check();
        }
    }
}

/// Constraint: All fonts must be embedded
pub struct EmbeddedFontsConstraint;

impl SchemaConstraint for EmbeddedFontsConstraint {
    fn name(&self) -> &str {
        "embedded-fonts"
    }

    fn description(&self) -> &str {
        "All fonts must be embedded in the document"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Fonts
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![
            NodeType::Font,
            NodeType::Type1Font,
            NodeType::TrueTypeFont,
            NodeType::Type3Font,
            NodeType::CIDFont,
        ]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let font_types = vec![
            NodeType::Font,
            NodeType::Type1Font,
            NodeType::TrueTypeFont,
            NodeType::Type3Font,
            NodeType::CIDFont,
        ];
        let mut all_embedded = true;

        for font_type in font_types {
            let fonts = document.ast.find_nodes_by_type(font_type);

            for font_id in fonts {
                if let Some(font) = document.ast.get_node(font_id) {
                    if let PdfValue::Dictionary(dict) = &font.value {
                        let has_font_file =
                            has_embedded_font_program(dict, document, &mut HashSet::new())
                                || dict
                                    .get("DescendantFonts")
                                    .and_then(PdfValue::as_array)
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|value| resolve_dict_from_value(document, value))
                                    .any(|descendant| {
                                        has_embedded_font_program(
                                            &descendant,
                                            document,
                                            &mut HashSet::new(),
                                        )
                                    });

                        if !has_font_file {
                            all_embedded = false;
                            report.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                code: "FONT_NOT_EMBEDDED".to_string(),
                                message: "Font is not embedded in the document".to_string(),
                                node_id: Some(font_id),
                                location: Some("Font dictionary".to_string()),
                                suggestion: Some("Embed the font in the document".to_string()),
                            });
                        }
                    }
                }
            }
        }

        if all_embedded {
            report.add_passed_check();
        }
    }
}

/// Constraint: Document must have tagged structure
pub struct TaggedStructureConstraint;

impl SchemaConstraint for TaggedStructureConstraint {
    fn name(&self) -> &str {
        "tagged-structure"
    }

    fn description(&self) -> &str {
        "Document must have tagged structure for accessibility"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Accessibility
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let catalog_nodes = document.ast.find_nodes_by_type(NodeType::Catalog);

        if let Some(catalog_id) = catalog_nodes.first() {
            if let Some(catalog) = document.ast.get_node(*catalog_id) {
                if let PdfValue::Dictionary(dict) = &catalog.value {
                    if let Some(PdfValue::Dictionary(mark_info)) = dict.get("MarkInfo") {
                        if let Some(PdfValue::Boolean(marked)) = mark_info.get("Marked") {
                            if *marked {
                                // Check for StructTreeRoot
                                if dict.contains_key("StructTreeRoot") {
                                    report.add_passed_check();
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }

        report.add_issue(ValidationIssue {
            severity: ValidationSeverity::Error,
            code: "NO_TAGGED_STRUCTURE".to_string(),
            message: "Document must have tagged structure for accessibility".to_string(),
            node_id: catalog_nodes.first().copied(),
            location: Some("Document catalog".to_string()),
            suggestion: Some("Add tagged structure to the document".to_string()),
        });
    }
}

/// Constraint: No transparency allowed
pub struct NoTransparencyConstraint;

impl SchemaConstraint for NoTransparencyConstraint {
    fn name(&self) -> &str {
        "no-transparency"
    }

    fn description(&self) -> &str {
        "Document must not use transparency features"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Graphics
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        // This is a simplified check - in practice would need to examine graphics states,
        // blend modes, transparency groups, etc.
        let mut transparency_found = false;

        // Check for ExtGState entries that might contain transparency
        for node in document.ast.get_all_nodes() {
            if let PdfValue::Dictionary(dict) = &node.value {
                if let Some(PdfValue::Dictionary(resources)) = dict.get("Resources") {
                    if let Some(PdfValue::Dictionary(ext_gstate)) = resources.get("ExtGState") {
                        for (_, gstate_value) in ext_gstate {
                            if let PdfValue::Dictionary(gstate_dict) = gstate_value {
                                if gstate_dict.contains_key("ca")
                                    || gstate_dict.contains_key("CA")
                                    || gstate_dict.contains_key("BM")
                                    || gstate_dict.contains_key("SMask")
                                {
                                    transparency_found = true;
                                    report.add_issue(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        code: "TRANSPARENCY_NOT_ALLOWED".to_string(),
                                        message:
                                            "Transparency features are not allowed in this profile"
                                                .to_string(),
                                        node_id: Some(node.id),
                                        location: Some("Graphics state".to_string()),
                                        suggestion: Some(
                                            "Remove transparency effects from the document"
                                                .to_string(),
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if !transparency_found {
            report.add_passed_check();
        }
    }
}

/// Constraint: No embedded files allowed
pub struct NoEmbeddedFilesConstraint;

impl SchemaConstraint for NoEmbeddedFilesConstraint {
    fn name(&self) -> &str {
        "no-embedded-files"
    }

    fn description(&self) -> &str {
        "Document must not contain embedded files"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Content
    }

    fn required_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::EmbeddedFile]
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let embedded_files = document.ast.find_nodes_by_type(NodeType::EmbeddedFile);

        if !embedded_files.is_empty() {
            for file_node in embedded_files {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "EMBEDDED_FILE_NOT_ALLOWED".to_string(),
                    message: "Embedded files are not allowed in this profile".to_string(),
                    node_id: Some(file_node),
                    location: Some("Embedded file".to_string()),
                    suggestion: Some("Remove embedded files from the document".to_string()),
                });
            }
        } else {
            report.add_passed_check();
        }
    }
}

/// Constraint: Fonts should define Encoding/ToUnicode appropriately
pub struct FontCMapEncodingConstraint;

impl SchemaConstraint for FontCMapEncodingConstraint {
    fn name(&self) -> &str {
        "font-encoding-cmap"
    }

    fn description(&self) -> &str {
        "Fonts should define Encoding and/or ToUnicode mappings"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Fonts
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Font dictionaries and ToUnicode CMaps")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let font_nodes = document.ast.find_nodes_by_type(NodeType::Font);
        let cid_nodes = document.ast.find_nodes_by_type(NodeType::CIDFont);

        for font_id in font_nodes.into_iter().chain(cid_nodes) {
            let mut has_encoding = false;
            let mut has_tounicode = false;

            if let Some(node) = document.ast.get_node(font_id) {
                if let Some(dict) = node.as_dict() {
                    if dict.contains_key("Encoding") {
                        has_encoding = true;
                    }
                    if dict.contains_key("ToUnicode") {
                        has_tounicode = true;
                    }
                    if let Some(PdfValue::Name(subtype)) = dict.get("Subtype") {
                        if subtype.without_slash() == "Type0" && !dict.contains_key("ToUnicode") {
                            has_tounicode = false;
                        }
                    }
                }

                let children = document.ast.get_children(font_id);
                for child in children {
                    if let Some(child_node) = document.ast.get_node(child) {
                        if child_node.node_type == NodeType::Encoding {
                            has_encoding = true;
                        }
                        if child_node.node_type == NodeType::ToUnicode {
                            has_tounicode = true;
                            if !matches!(child_node.value, PdfValue::Stream(_)) {
                                report.add_issue(ValidationIssue {
                                    severity: ValidationSeverity::Warning,
                                    code: "TOUNICODE_NOT_STREAM".to_string(),
                                    message: "ToUnicode node is not a stream".to_string(),
                                    node_id: Some(child),
                                    location: Some("Font ToUnicode".to_string()),
                                    suggestion: Some("Ensure ToUnicode is a stream".to_string()),
                                });
                            }
                        }
                    }
                }
            }

            if !has_encoding && !has_tounicode {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: "FONT_ENCODING_MISSING".to_string(),
                    message: "Font missing Encoding/ToUnicode mappings".to_string(),
                    node_id: Some(font_id),
                    location: Some("Font dictionary".to_string()),
                    suggestion: Some("Provide Encoding or ToUnicode mapping".to_string()),
                });
            } else {
                report.add_passed_check();
            }
        }
    }
}

/// Constraint: Document structure must be valid
pub struct ValidStructureConstraint;

impl SchemaConstraint for ValidStructureConstraint {
    fn name(&self) -> &str {
        "valid-structure"
    }

    fn description(&self) -> &str {
        "Document must have valid PDF structure"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Structure
    }

    fn iso_reference(&self) -> Option<&str> {
        Some("ISO 32000-2:2020 Document structure")
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        // Check for cycles in the graph
        if document.ast.is_cyclic() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "CYCLIC_STRUCTURE".to_string(),
                message: "Document structure contains cycles".to_string(),
                node_id: None,
                location: Some("Document structure".to_string()),
                suggestion: Some("Remove circular references from the document".to_string()),
            });
        } else {
            report.add_passed_check();
        }
    }
}

// Additional constraints for specific profiles...

/// Constraint: Color space restrictions for PDF/X
pub struct ColorSpaceConstraint;

impl SchemaConstraint for ColorSpaceConstraint {
    fn name(&self) -> &str {
        "color-space"
    }

    fn description(&self) -> &str {
        "Color spaces must comply with PDF/X requirements"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Graphics
    }

    fn check(&self, _document: &PdfDocument, report: &mut ValidationReport) {
        let document = _document;
        let mut has_issue = false;

        let catalog_dict = document.get_catalog().cloned();
        let mut has_output_intents = false;
        if let Some(catalog) = &catalog_dict {
            if catalog.contains_key("OutputIntents") {
                has_output_intents = true;
            }
        }

        if !has_output_intents {
            has_issue = true;
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "OUTPUT_INTENTS_MISSING".to_string(),
                message: "PDF/X requires OutputIntents for color management".to_string(),
                node_id: document.catalog,
                location: Some("Catalog".to_string()),
                suggestion: Some("Add OutputIntents to the catalog".to_string()),
            });
        }

        let pages = document.ast.find_nodes_by_type(NodeType::Page);
        for page_id in pages {
            if let Some(page) = document.ast.get_node(page_id) {
                if let PdfValue::Dictionary(dict) = &page.value {
                    if let Some(resources_value) = dict.get("Resources") {
                        if let Some(resources) = resolve_dict_from_value(document, resources_value)
                        {
                            if let Some(colorspaces_value) = resources.get("ColorSpace") {
                                if value_contains_name(colorspaces_value, "DeviceRGB") {
                                    has_issue = true;
                                    report.add_issue(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        code: "DEVICE_RGB_DISALLOWED".to_string(),
                                        message: "DeviceRGB color space is not permitted in PDF/X"
                                            .to_string(),
                                        node_id: Some(page_id),
                                        location: Some("Page resources ColorSpace".to_string()),
                                        suggestion: Some(
                                            "Use DeviceCMYK/Separation/ICCBased with OutputIntent"
                                                .to_string(),
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        let image_nodes = document.ast.find_nodes_by_type(NodeType::ImageXObject);
        for image_id in image_nodes {
            if let Some(image) = document.ast.get_node(image_id) {
                if let PdfValue::Dictionary(dict) = &image.value {
                    if let Some(colorspace_value) = dict.get("ColorSpace") {
                        if value_contains_name(colorspace_value, "DeviceRGB") {
                            has_issue = true;
                            report.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                code: "IMAGE_DEVICE_RGB_DISALLOWED".to_string(),
                                message: "Image uses DeviceRGB which is not permitted in PDF/X"
                                    .to_string(),
                                node_id: Some(image_id),
                                location: Some("Image XObject ColorSpace".to_string()),
                                suggestion: Some(
                                    "Convert images to CMYK or ICCBased with OutputIntent"
                                        .to_string(),
                                ),
                            });
                        }
                    }
                }
            }
        }

        if !has_issue {
            report.add_passed_check();
        }
    }
}

/// Constraint: TrimBox required for PDF/X
pub struct TrimBoxConstraint;

impl SchemaConstraint for TrimBoxConstraint {
    fn name(&self) -> &str {
        "trim-box"
    }

    fn description(&self) -> &str {
        "Pages must have TrimBox for print production"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Graphics
    }

    fn check(&self, document: &PdfDocument, report: &mut ValidationReport) {
        let pages = document.ast.find_nodes_by_type(NodeType::Page);

        for page_id in pages {
            if let Some(page) = document.ast.get_node(page_id) {
                if let PdfValue::Dictionary(dict) = &page.value {
                    if !dict.contains_key("TrimBox") {
                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Warning,
                            code: "TRIM_BOX_MISSING".to_string(),
                            message: "Page should have TrimBox for print production".to_string(),
                            node_id: Some(page_id),
                            location: Some("Page dictionary".to_string()),
                            suggestion: Some("Add TrimBox to page dictionary".to_string()),
                        });
                    }
                }
            }
        }

        report.add_passed_check();
    }
}

// PDF/UA specific constraints

/// Constraint: Accessibility metadata required
pub struct AccessibilityMetadataConstraint;

impl SchemaConstraint for AccessibilityMetadataConstraint {
    fn name(&self) -> &str {
        "accessibility-metadata"
    }

    fn description(&self) -> &str {
        "Document must contain accessibility metadata"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Accessibility
    }

    fn check(&self, _document: &PdfDocument, report: &mut ValidationReport) {
        let document = _document;
        let catalog = match document.get_catalog() {
            Some(catalog) => catalog,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "CATALOG_MISSING".to_string(),
                    message: "Catalog missing; cannot validate accessibility metadata".to_string(),
                    node_id: None,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Ensure document has a catalog dictionary".to_string()),
                });
                return;
            }
        };

        let metadata_value = match catalog.get("Metadata") {
            Some(value) => value,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "ACCESSIBILITY_METADATA_MISSING".to_string(),
                    message: "PDF/UA requires XMP metadata stream in catalog".to_string(),
                    node_id: document.catalog,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Add Metadata stream with XMP packet".to_string()),
                });
                return;
            }
        };

        let stream = match resolve_stream_from_value(document, metadata_value) {
            Some(stream) => stream,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "METADATA_NOT_STREAM".to_string(),
                    message: "Catalog Metadata entry must be a stream".to_string(),
                    node_id: document.catalog,
                    location: Some("Catalog Metadata".to_string()),
                    suggestion: Some("Ensure Metadata is a stream object".to_string()),
                });
                return;
            }
        };

        let subtype_ok = stream
            .dict
            .get("Subtype")
            .and_then(PdfValue::as_name)
            .map(|name| name.without_slash() == "XML")
            .unwrap_or(false);

        let type_ok = stream
            .dict
            .get("Type")
            .and_then(PdfValue::as_name)
            .map(|name| name.without_slash() == "Metadata")
            .unwrap_or(false);

        if !subtype_ok || !type_ok {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "METADATA_STREAM_INVALID".to_string(),
                message: "Metadata stream must have Type=Metadata and Subtype=XML".to_string(),
                node_id: document.catalog,
                location: Some("Metadata stream".to_string()),
                suggestion: Some("Fix Metadata stream dictionary entries".to_string()),
            });
            return;
        }

        if let Some(bytes) = stream.data.as_bytes() {
            if !bytes.windows(9).any(|w| w == b"x:xmpmeta")
                && !bytes.windows(10).any(|w| w == b"<x:xmpmeta")
            {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: "XMP_PACKET_MISSING".to_string(),
                    message: "Metadata stream does not appear to contain an XMP packet".to_string(),
                    node_id: document.catalog,
                    location: Some("Metadata stream".to_string()),
                    suggestion: Some("Embed a valid XMP packet".to_string()),
                });
                return;
            }
        }

        report.add_passed_check();
    }
}

/// Constraint: Alternative text required
pub struct AltTextConstraint;

impl SchemaConstraint for AltTextConstraint {
    fn name(&self) -> &str {
        "alt-text"
    }

    fn description(&self) -> &str {
        "Images and figures must have alternative text"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Accessibility
    }

    fn check(&self, _document: &PdfDocument, report: &mut ValidationReport) {
        let document = _document;
        let struct_elems = document.ast.find_nodes_by_type(NodeType::StructElem);

        if struct_elems.is_empty() {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "STRUCT_ELEM_MISSING".to_string(),
                message: "PDF/UA requires structure elements for alternative text".to_string(),
                node_id: document.catalog,
                location: Some("Structure tree".to_string()),
                suggestion: Some("Add StructTreeRoot and StructElem entries".to_string()),
            });
            return;
        }

        let mut missing_alt = false;
        for elem_id in struct_elems {
            if let Some(elem) = document.ast.get_node(elem_id) {
                if let PdfValue::Dictionary(dict) = &elem.value {
                    let is_figure = dict
                        .get("S")
                        .and_then(PdfValue::as_name)
                        .map(|name| {
                            name.without_slash() == "Figure"
                                || name.without_slash() == "Formula"
                                || name.without_slash() == "Table"
                        })
                        .unwrap_or(false);
                    if is_figure && !dict.contains_key("Alt") {
                        missing_alt = true;
                        report.add_issue(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            code: "ALT_TEXT_MISSING".to_string(),
                            message: "Figure/Table structure element missing Alt text".to_string(),
                            node_id: Some(elem_id),
                            location: Some("StructElem".to_string()),
                            suggestion: Some("Add Alt entry to StructElem".to_string()),
                        });
                    }
                }
            }
        }

        if !missing_alt {
            report.add_passed_check();
        }
    }
}

/// Constraint: Language specification required
pub struct LanguageSpecificationConstraint;

impl SchemaConstraint for LanguageSpecificationConstraint {
    fn name(&self) -> &str {
        "language-specification"
    }

    fn description(&self) -> &str {
        "Document must specify primary language"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Accessibility
    }

    fn check(&self, _document: &PdfDocument, report: &mut ValidationReport) {
        let document = _document;
        let catalog = match document.get_catalog() {
            Some(catalog) => catalog,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "CATALOG_MISSING".to_string(),
                    message: "Catalog missing; cannot validate language".to_string(),
                    node_id: None,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Ensure document has a catalog dictionary".to_string()),
                });
                return;
            }
        };

        let lang_value = catalog.get("Lang").and_then(PdfValue::as_string);
        if let Some(lang) = lang_value {
            if lang.as_bytes().is_empty() {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "LANG_EMPTY".to_string(),
                    message: "Catalog Lang entry must not be empty".to_string(),
                    node_id: document.catalog,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Set a valid language code (e.g. en-US)".to_string()),
                });
                return;
            }
            report.add_passed_check();
        } else {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "LANG_MISSING".to_string(),
                message: "PDF/UA requires catalog Lang entry".to_string(),
                node_id: document.catalog,
                location: Some("Catalog".to_string()),
                suggestion: Some("Set Lang in catalog (e.g. en-US)".to_string()),
            });
        }
    }
}

/// Constraint: Logical reading order required
pub struct LogicalReadingOrderConstraint;

impl SchemaConstraint for LogicalReadingOrderConstraint {
    fn name(&self) -> &str {
        "logical-reading-order"
    }

    fn description(&self) -> &str {
        "Content must have logical reading order"
    }

    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::Accessibility
    }

    fn check(&self, _document: &PdfDocument, report: &mut ValidationReport) {
        let document = _document;
        let catalog = match document.get_catalog() {
            Some(catalog) => catalog,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "CATALOG_MISSING".to_string(),
                    message: "Catalog missing; cannot validate reading order".to_string(),
                    node_id: None,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Ensure document has a catalog dictionary".to_string()),
                });
                return;
            }
        };

        let struct_root_value = match catalog.get("StructTreeRoot") {
            Some(value) => value,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "STRUCT_TREE_ROOT_MISSING".to_string(),
                    message: "PDF/UA requires StructTreeRoot".to_string(),
                    node_id: document.catalog,
                    location: Some("Catalog".to_string()),
                    suggestion: Some("Add StructTreeRoot to catalog".to_string()),
                });
                return;
            }
        };

        let struct_root = match resolve_dict_from_value(document, struct_root_value) {
            Some(dict) => dict,
            None => {
                report.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "STRUCT_TREE_ROOT_INVALID".to_string(),
                    message: "StructTreeRoot must be a dictionary".to_string(),
                    node_id: document.catalog,
                    location: Some("StructTreeRoot".to_string()),
                    suggestion: Some("Ensure StructTreeRoot is a valid dictionary".to_string()),
                });
                return;
            }
        };

        let has_parent_tree = struct_root.contains_key("ParentTree");
        let has_k = struct_root.contains_key("K");

        if !has_parent_tree || !has_k {
            report.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                code: "READING_ORDER_INCOMPLETE".to_string(),
                message: "StructTreeRoot must define ParentTree and K for reading order"
                    .to_string(),
                node_id: document.catalog,
                location: Some("StructTreeRoot".to_string()),
                suggestion: Some("Populate StructTreeRoot ParentTree and K entries".to_string()),
            });
            return;
        }

        report.add_passed_check();
    }
}
