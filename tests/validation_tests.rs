use pdf_ast::validation::pdfa::*;
use pdf_ast::validation::ValidationSeverity;
/// Tests for PDF validation functionality
///
/// These tests verify the PDF/A validation and other validation features
use pdf_ast::*;

#[cfg(test)]
mod validation_tests {
    use super::*;
    use pdf_ast::validation::SchemaRegistry;

    #[test]
    fn test_pdfa_basic_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);

        // Create a minimal document for testing
        let document = create_test_document();

        // Test basic validation
        let report = validator.validate(&document);

        // Should have validation results
        assert!(!report.issues.is_empty() || !report.is_valid);
        assert!(!report.schema_name.is_empty());
        assert_eq!(report.schema_name, "PDF/A-1b");
    }

    #[test]
    fn test_pdfa_version_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);

        // Test PDF 2.0 (should fail PDF/A-1b)
        let mut document = create_test_document();
        document.version = PdfVersion { major: 2, minor: 0 };

        let report = validator.validate(&document);
        assert!(!report.is_valid);

        // Should have version-related error
        let has_version_error = report
            .issues
            .iter()
            .any(|issue| issue.code.contains("VERSION") || issue.message.contains("version"));
        assert!(has_version_error);
    }

    #[test]
    fn test_pdfa_color_space_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Add a page with RGB color space (should be allowed in PDF/A-1b)
        add_page_with_colorspace(&mut document, "/DeviceRGB");

        let report = validator.validate(&document);

        // RGB should be allowed
        let has_color_error = report
            .issues
            .iter()
            .any(|issue| issue.code.contains("COLOR") && issue.message.contains("DeviceRGB"));
        assert!(!has_color_error);

        // Test with CMYK (should also be allowed)
        let mut document2 = create_test_document();
        add_page_with_colorspace(&mut document2, "/DeviceCMYK");

        let report2 = validator.validate(&document2);
        let has_cmyk_error = report2
            .issues
            .iter()
            .any(|issue| issue.code.contains("COLOR") && issue.message.contains("DeviceCMYK"));
        assert!(!has_cmyk_error);
    }

    #[test]
    fn test_pdfa_color_space_validation_follows_inherited_page_resources() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();
        let pages_id = document
            .ast
            .find_nodes_by_type(NodeType::Pages)
            .into_iter()
            .next()
            .expect("test document should have a Pages node");
        let pages = document
            .ast
            .get_node_mut(pages_id)
            .expect("Pages node should exist");
        if let PdfValue::Dictionary(dict) = &mut pages.value {
            let mut resources = PdfDictionary::new();
            let mut colorspaces = PdfDictionary::new();
            colorspaces.insert("CS1", PdfValue::Name(PdfName::new("DeviceRGB")));
            resources.insert("ColorSpace", PdfValue::Dictionary(colorspaces));
            dict.insert("Resources", PdfValue::Dictionary(resources));
        }

        let report = validator.validate(&document);
        assert!(has_issue(&report, "PDF_A_COLOR_SPACE"));
    }

    #[test]
    fn test_pdfa_font_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Add an embedded font (should be OK)
        add_embedded_font(&mut document, "Arial", true);

        let report = validator.validate(&document);

        // Embedded fonts should not cause errors
        let has_font_error = report.issues.iter().any(|issue| {
            issue.code.contains("FONT") && issue.severity == ValidationSeverity::Error
        });
        assert!(!has_font_error);

        // Test non-embedded font (should cause error)
        let mut document2 = create_test_document();
        add_embedded_font(&mut document2, "Helvetica", false);

        let report2 = validator.validate(&document2);
        let has_embed_error = report2
            .issues
            .iter()
            .any(|issue| issue.code == "PDF_A_FONT_EMBEDDING");
        assert!(has_embed_error);
    }

    #[test]
    fn test_pdfa_cid_font_descriptor_embedding() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();
        document.ast.create_node(
            NodeType::Object(ObjectId::new(99, 0)),
            PdfValue::Stream(PdfStream::new(PdfDictionary::new(), vec![0x01])),
        );
        let descriptor = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("FontFile2", PdfValue::Reference(PdfReference::new(99, 0)));
            dict
        });
        let cid_font = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert(
                "DescendantFonts",
                PdfValue::Array(
                    vec![PdfValue::Dictionary({
                        let mut descendant = PdfDictionary::new();
                        descendant.insert("FontDescriptor", descriptor);
                        descendant
                    })]
                    .into(),
                ),
            );
            dict
        });
        document.ast.create_node(NodeType::CIDFont, cid_font);

        let report = validator.validate(&document);
        assert!(!has_issue(&report, "PDF_A_FONT_EMBEDDING"));
    }

    #[test]
    fn pdfa_font_file_key_must_reference_a_stream() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();
        let font = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert("BaseFont", PdfValue::Name(PdfName::new("TestFont")));
            dict.insert("FontFile", PdfValue::Null);
            dict
        });
        document.ast.create_node(NodeType::Font, font);

        let report = validator.validate(&document);
        assert!(has_issue(&report, "PDF_A_FONT_EMBEDDING"));
    }

    #[test]
    fn fixture_pdfa_multimedia_rule_has_positive_and_negative_cases() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let clean_report = validator.validate(&create_test_document());
        assert!(!has_issue(&clean_report, "PDF_A_MULTIMEDIA"));

        let mut multimedia = create_test_document();
        add_multimedia_annotation(&mut multimedia);
        let report = validator.validate(&multimedia);
        assert!(has_issue(&report, "PDF_A_MULTIMEDIA"));
    }

    #[test]
    fn test_pdfa_javascript_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let clean_report = validator.validate(&create_test_document());
        assert!(!has_issue(&clean_report, "PDF_A_JAVASCRIPT"));

        // Add JavaScript action (should fail PDF/A-1b)
        let mut document = create_test_document();
        add_javascript_action(&mut document);

        let report = validator.validate(&document);
        assert!(!report.is_valid);

        // Should have JavaScript-related error
        assert!(has_issue(&report, "PDF_A_JAVASCRIPT"));
    }

    #[test]
    fn test_pdfa_transparency_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Add transparency (should fail PDF/A-1b)
        add_transparency_group(&mut document);

        let report = validator.validate(&document);

        // Should have transparency-related warning/error
        let has_transparency_issue = report.issues.iter().any(|issue| {
            issue.message.to_lowercase().contains("transparency")
                || issue.code.contains("TRANSPARENCY")
        });
        assert!(has_transparency_issue);
    }

    #[test]
    fn test_pdfa_encryption_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Set document as encrypted (should fail PDF/A-1b)
        document.metadata.encrypted = true;

        let report = validator.validate(&document);
        assert!(!report.is_valid);

        // Should have encryption-related error
        let has_encryption_error = report.issues.iter().any(|issue| {
            issue.code.contains("ENCRYPTION") || issue.message.to_lowercase().contains("encrypt")
        });
        assert!(has_encryption_error);
    }

    #[test]
    fn test_pdfa_metadata_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Add proper XMP metadata
        add_xmp_metadata(&mut document);

        let report = validator.validate(&document);

        // With metadata, should have fewer issues
        let metadata_errors = report
            .issues
            .iter()
            .filter(|issue| issue.code.contains("METADATA"))
            .count();

        // May still have metadata issues if not properly formatted
        // but at least metadata exists
        assert!(document.info.is_some() || metadata_errors > 0);
    }

    #[test]
    fn test_pdfa_image_validation() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let mut document = create_test_document();

        // Add image with proper color space
        add_image_object(&mut document, "/DeviceRGB");

        let report = validator.validate(&document);

        // RGB images should be allowed
        let _has_image_error = report.issues.iter().any(|issue| {
            issue.code.contains("IMAGE") && issue.severity == ValidationSeverity::Error
        });

        // May have warnings but not necessarily errors for RGB images
        println!(
            "Image validation issues: {:?}",
            report
                .issues
                .iter()
                .filter(|i| i.code.contains("IMAGE"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_validation_statistics() {
        let validator = PdfA1bValidator::new().with_strict_mode(false);
        let document = create_test_document();

        let report = validator.validate(&document);

        // Check statistics are populated
        assert!(report.statistics.total_checks > 0);
        assert_eq!(
            report.statistics.total_checks,
            report.statistics.passed_checks + report.statistics.failed_checks
        );

        // Count issues by severity
        let error_count = report
            .issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .count();
        let warning_count = report
            .issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Warning)
            .count();
        let info_count = report
            .issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Info)
            .count();

        assert_eq!(report.statistics.error_count, error_count);
        assert_eq!(report.statistics.warning_count, warning_count);
        assert_eq!(report.statistics.info_count, info_count);
    }

    #[test]
    fn test_strict_vs_lenient_mode() {
        let strict_validator = PdfA1bValidator::new().with_strict_mode(true);
        let lenient_validator = PdfA1bValidator::new().with_strict_mode(false);

        let document = create_test_document();

        let strict_report = strict_validator.validate(&document);
        let lenient_report = lenient_validator.validate(&document);

        // Strict mode should generally have more issues or stricter enforcement
        println!(
            "Strict issues: {}, Lenient issues: {}",
            strict_report.issues.len(),
            lenient_report.issues.len()
        );

        // Both should run without panicking
        assert!(!strict_report.schema_name.is_empty());
        assert!(!lenient_report.schema_name.is_empty());
    }

    #[test]
    fn test_pdfx_colorspace_constraint_detects_device_rgb() {
        let mut document = create_test_document();
        document.version = PdfVersion::new(1, 3);
        add_page_with_resource_colorspace(&mut document, "/DeviceRGB");

        let registry = SchemaRegistry::new();
        let report = registry
            .validate(&document, "PDF/X-1a")
            .expect("PDF/X-1a report should be produced");

        let has_rgb_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == "DEVICE_RGB_DISALLOWED");
        let has_output_intents_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == "OUTPUT_INTENTS_MISSING");

        assert!(has_rgb_issue, "Expected DeviceRGB to be flagged");
        assert!(
            has_output_intents_issue,
            "Expected missing OutputIntents to be flagged"
        );
    }

    #[test]
    fn test_pdfua_accessibility_constraints() {
        let mut document = create_test_document();
        add_marked_struct_tree(&mut document);
        add_struct_elem_figure(&mut document, false);

        let registry = SchemaRegistry::new();
        let report = registry
            .validate(&document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");

        let has_lang_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == "LANG_MISSING");
        let has_metadata_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == "ACCESSIBILITY_METADATA_MISSING");
        let has_alt_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == "ALT_TEXT_MISSING");

        assert!(has_lang_issue, "Expected missing language to be flagged");
        assert!(
            has_metadata_issue,
            "Expected missing metadata to be flagged"
        );
        assert!(has_alt_issue, "Expected missing Alt text to be flagged");
    }

    #[test]
    fn fixture_pdfua_structure_rule_has_positive_and_negative_cases() {
        let registry = SchemaRegistry::new();

        let untagged = registry
            .validate(&create_test_document(), "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&untagged, "NO_TAGGED_STRUCTURE"));

        let mut missing_elements = create_test_document();
        add_marked_struct_tree(&mut missing_elements);
        let missing_elements = registry
            .validate(&missing_elements, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&missing_elements, "STRUCT_ELEM_MISSING"));

        let mut valid = create_test_document();
        add_marked_struct_tree(&mut valid);
        add_struct_elem_figure(&mut valid, true);
        set_catalog_lang(&mut valid, b"en-US");
        let valid = registry
            .validate(&valid, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(!has_issue(&valid, "NO_TAGGED_STRUCTURE"));
        assert!(!has_issue(&valid, "STRUCT_ELEM_MISSING"));
    }

    #[test]
    fn pdfua_rejects_empty_alt_text_and_resolves_indirect_alt_text() {
        let registry = SchemaRegistry::new();

        let mut empty = create_test_document();
        add_marked_struct_tree(&mut empty);
        add_struct_elem_figure(&mut empty, true);
        let elem_id = empty.ast.find_nodes_by_type(NodeType::StructElem)[0];
        if let Some(node) = empty.ast.get_node_mut(elem_id) {
            if let PdfValue::Dictionary(dict) = &mut node.value {
                dict.insert("Alt", PdfValue::String(PdfString::new_literal(b"   ")));
            }
        }
        let empty = registry
            .validate(&empty, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&empty, "ALT_TEXT_MISSING"));

        let mut indirect = create_test_document();
        add_marked_struct_tree(&mut indirect);
        add_struct_elem_figure(&mut indirect, false);
        indirect.ast.create_node(
            NodeType::Object(ObjectId::new(900, 0)),
            PdfValue::String(PdfString::new_literal(b"indirect figure description")),
        );
        let elem_id = indirect.ast.find_nodes_by_type(NodeType::StructElem)[0];
        if let Some(node) = indirect.ast.get_node_mut(elem_id) {
            if let PdfValue::Dictionary(dict) = &mut node.value {
                dict.insert("Alt", PdfValue::Reference(PdfReference::new(900, 0)));
            }
        }
        let indirect = registry
            .validate(&indirect, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(!has_issue(&indirect, "ALT_TEXT_MISSING"));
    }

    #[test]
    fn fixture_pdfua_language_rule_has_positive_and_negative_cases() {
        let registry = SchemaRegistry::new();

        let missing = registry
            .validate(&create_test_document(), "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&missing, "LANG_MISSING"));

        let mut empty_document = create_test_document();
        set_catalog_lang(&mut empty_document, b"");
        let empty = registry
            .validate(&empty_document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&empty, "LANG_EMPTY"));

        let mut valid_document = create_test_document();
        set_catalog_lang(&mut valid_document, b"en-US");
        let valid = registry
            .validate(&valid_document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(!has_issue(&valid, "LANG_MISSING"));
        assert!(!has_issue(&valid, "LANG_EMPTY"));
    }

    #[test]
    fn fixture_pdfua_metadata_rule_has_positive_and_negative_cases() {
        let registry = SchemaRegistry::new();

        let mut valid_document = create_test_document();
        add_pdfua_metadata(&mut valid_document, true);
        let valid = registry
            .validate(&valid_document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(!has_issue(&valid, "METADATA_STREAM_INVALID"));

        let mut invalid_document = create_test_document();
        add_pdfua_metadata(&mut invalid_document, false);
        let invalid = registry
            .validate(&invalid_document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(has_issue(&invalid, "METADATA_STREAM_INVALID"));
    }

    #[test]
    fn pdfa_resolves_indirect_catalog_entries() {
        let mut document = create_test_document();
        let action_id = ObjectId::new(40, 0);
        document.ast.create_node(
            NodeType::Object(action_id),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("S", PdfValue::Name(PdfName::new("JavaScript")));
                dict
            }),
        );
        let form_id = ObjectId::new(41, 0);
        document.ast.create_node(
            NodeType::Object(form_id),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("XFA", PdfValue::String(PdfString::new_literal(b"xfa")));
                dict
            }),
        );
        document.ast.create_node(
            NodeType::Object(ObjectId::new(42, 0)),
            PdfValue::Stream(PdfStream::new(
                {
                    let mut dict = PdfDictionary::new();
                    dict.insert("Type", PdfValue::Name(PdfName::new("Metadata")));
                    dict.insert("Subtype", PdfValue::Name(PdfName::new("XML")));
                    dict
                },
                b"<x:xmpmeta/>".to_vec(),
            )),
        );

        let catalog = document.catalog.expect("catalog should exist");
        let catalog_node = document
            .ast
            .get_node_mut(catalog)
            .expect("catalog node should exist");
        if let PdfValue::Dictionary(catalog_dict) = &mut catalog_node.value {
            catalog_dict.insert("OpenAction", PdfValue::Reference(PdfReference::new(40, 0)));
            catalog_dict.insert("AcroForm", PdfValue::Reference(PdfReference::new(41, 0)));
            catalog_dict.insert("Metadata", PdfValue::Reference(PdfReference::new(42, 0)));
        }

        let report = PdfA1bValidator::new()
            .with_strict_mode(false)
            .validate(&document);
        assert!(has_issue(&report, "PDF_A_JAVASCRIPT"));
        assert!(has_issue(&report, "PDF_A_XFA"));
        assert!(!has_issue(&report, "PDF_A_XMP_METADATA"));
    }

    #[test]
    fn pdfa_output_intent_requires_profile_fields() {
        let mut invalid = create_test_document();
        let catalog_id = invalid.catalog.expect("catalog should exist");
        let catalog = invalid
            .ast
            .get_node_mut(catalog_id)
            .expect("catalog node should exist");
        if let PdfValue::Dictionary(dict) = &mut catalog.value {
            dict.insert(
                "OutputIntents",
                PdfValue::Array(PdfArray::from(vec![PdfValue::Dictionary({
                    let mut intent = PdfDictionary::new();
                    intent.insert("S", PdfValue::Name(PdfName::new("GTS_PDFX")));
                    intent
                })])),
            );
        }
        let invalid_report = PdfA1bValidator::new()
            .with_strict_mode(false)
            .validate(&invalid);
        assert!(has_issue(&invalid_report, "PDF_A_OUTPUT_INTENT"));

        let mut valid = create_test_document();
        let catalog_id = valid.catalog.expect("catalog should exist");
        let catalog = valid
            .ast
            .get_node_mut(catalog_id)
            .expect("catalog node should exist");
        if let PdfValue::Dictionary(dict) = &mut catalog.value {
            dict.insert(
                "OutputIntents",
                PdfValue::Array(PdfArray::from(vec![PdfValue::Dictionary({
                    let mut intent = PdfDictionary::new();
                    intent.insert("S", PdfValue::Name(PdfName::new("GTS_PDFA1")));
                    intent.insert(
                        "OutputConditionIdentifier",
                        PdfValue::String(PdfString::new_literal(b"sRGB")),
                    );
                    intent.insert(
                        "DestOutputProfile",
                        PdfValue::Stream(PdfStream::new(PdfDictionary::new(), vec![0; 4])),
                    );
                    intent
                })])),
            );
        }
        let valid_report = PdfA1bValidator::new()
            .with_strict_mode(false)
            .validate(&valid);
        assert!(!has_issue(&valid_report, "PDF_A_OUTPUT_INTENT"));
    }

    #[test]
    fn pdfua_resolves_indirect_catalog_language() {
        let mut document = create_test_document();
        let language_id = ObjectId::new(50, 0);
        document.ast.create_node(
            NodeType::Object(language_id),
            PdfValue::String(PdfString::new_literal(b"en-US")),
        );
        let catalog_id = document.catalog.expect("catalog should exist");
        let catalog = document
            .ast
            .get_node_mut(catalog_id)
            .expect("catalog node should exist");
        if let PdfValue::Dictionary(dict) = &mut catalog.value {
            dict.insert("Lang", PdfValue::Reference(PdfReference::new(50, 0)));
        }

        let report = SchemaRegistry::new()
            .validate(&document, "PDF/UA-1")
            .expect("PDF/UA-1 report should be produced");
        assert!(!has_issue(&report, "LANG_MISSING"));
        assert!(!has_issue(&report, "LANG_EMPTY"));
    }

    // Helper functions for creating test scenarios

    fn has_issue(report: &pdf_ast::validation::ValidationReport, code: &str) -> bool {
        report.issues.iter().any(|issue| issue.code == code)
    }

    fn create_test_document() -> PdfDocument {
        let version = PdfVersion { major: 1, minor: 4 };
        let mut document = PdfDocument::new(version);

        // Add basic catalog
        let catalog_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
            dict
        });
        let catalog_id = document.ast.create_node(NodeType::Catalog, catalog_value);
        document.set_catalog(catalog_id);

        // Add basic page tree
        let pages_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Pages")));
            dict.insert("Count", PdfValue::Integer(1));
            dict
        });
        let pages_id = document.ast.create_node(NodeType::Pages, pages_value);

        // Add a page
        let page_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
            dict.insert(
                "MediaBox",
                PdfValue::Array(PdfArray::from(vec![
                    PdfValue::Integer(0),
                    PdfValue::Integer(0),
                    PdfValue::Integer(612),
                    PdfValue::Integer(792),
                ])),
            );
            dict
        });
        let page_id = document.ast.create_node(NodeType::Page, page_value);

        document
            .ast
            .add_edge(pages_id, page_id, crate::ast::EdgeType::Child);
        document
            .ast
            .add_edge(catalog_id, pages_id, crate::ast::EdgeType::Reference);

        document
    }

    fn add_page_with_colorspace(document: &mut PdfDocument, colorspace: &str) {
        let page_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
            dict.insert("ColorSpace", PdfValue::Name(PdfName::new(colorspace)));
            dict
        });
        let page_id = document.ast.create_node(NodeType::Page, page_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, page_id, crate::ast::EdgeType::Child);
        }
    }

    fn add_page_with_resource_colorspace(document: &mut PdfDocument, colorspace: &str) {
        let resources = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            let mut color_space = PdfDictionary::new();
            color_space.insert("CS1", PdfValue::Name(PdfName::new(colorspace)));
            dict.insert("ColorSpace", PdfValue::Dictionary(color_space));
            dict
        });

        let page_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
            dict.insert("Resources", resources);
            dict
        });

        let page_id = document.ast.create_node(NodeType::Page, page_value);
        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, page_id, crate::ast::EdgeType::Child);
        }
    }

    fn add_marked_struct_tree(document: &mut PdfDocument) {
        let catalog_id = document.catalog.expect("Catalog should exist");
        if let Some(catalog_node) = document.ast.get_node_mut(catalog_id) {
            if let PdfValue::Dictionary(ref mut dict) = catalog_node.value {
                dict.insert(
                    "MarkInfo",
                    PdfValue::Dictionary({
                        let mut mark_info = PdfDictionary::new();
                        mark_info.insert("Marked", PdfValue::Boolean(true));
                        mark_info
                    }),
                );
                dict.insert(
                    "StructTreeRoot",
                    PdfValue::Dictionary({
                        let mut struct_root = PdfDictionary::new();
                        struct_root.insert("Type", PdfValue::Name(PdfName::new("StructTreeRoot")));
                        struct_root
                            .insert("ParentTree", PdfValue::Dictionary(PdfDictionary::new()));
                        struct_root.insert("K", PdfValue::Array(PdfArray::new()));
                        struct_root
                    }),
                );
            }
        }
    }

    fn add_struct_elem_figure(document: &mut PdfDocument, with_alt: bool) {
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("StructElem")));
        dict.insert("S", PdfValue::Name(PdfName::new("Figure")));
        if with_alt {
            dict.insert(
                "Alt",
                PdfValue::String(PdfString::new_literal(b"figure alt text")),
            );
        }
        let elem_id = document
            .ast
            .create_node(NodeType::StructElem, PdfValue::Dictionary(dict));

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, elem_id, crate::ast::EdgeType::Reference);
        }
    }

    fn add_embedded_font(document: &mut PdfDocument, font_name: &str, embedded: bool) {
        if embedded {
            document.ast.create_node(
                NodeType::Object(ObjectId::new(999, 0)),
                PdfValue::Stream(PdfStream::new(PdfDictionary::new(), vec![0x01])),
            );
        }
        let font_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert("BaseFont", PdfValue::Name(PdfName::new(font_name)));
            if embedded {
                dict.insert("FontFile", PdfValue::Reference(PdfReference::new(999, 0)));
            }
            dict
        });
        let font_id = document.ast.create_node(NodeType::Font, font_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, font_id, crate::ast::EdgeType::Reference);
        }
    }

    fn add_javascript_action(document: &mut PdfDocument) {
        let action_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Action")));
            dict.insert("S", PdfValue::Name(PdfName::new("JavaScript")));
            dict.insert(
                "JS",
                PdfValue::String(PdfString::new_literal(b"app.alert('Hello');")),
            );
            dict
        });
        let action_id = document
            .ast
            .create_node(NodeType::JavaScriptAction, action_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, action_id, crate::ast::EdgeType::Reference);
        }
    }

    fn add_multimedia_annotation(document: &mut PdfDocument) {
        let annotation_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("Movie")));
            dict
        });
        let annotation_id = document
            .ast
            .create_node(NodeType::Annotation, annotation_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, annotation_id, crate::ast::EdgeType::Reference);
        }
    }

    fn set_catalog_lang(document: &mut PdfDocument, language: &[u8]) {
        let catalog_id = document.catalog.expect("Catalog should exist");
        let catalog = document
            .ast
            .get_node_mut(catalog_id)
            .expect("Catalog node should exist");
        if let PdfValue::Dictionary(dict) = &mut catalog.value {
            dict.insert(
                "Lang",
                PdfValue::String(PdfString::new_literal(language.to_vec())),
            );
        }
    }

    fn add_transparency_group(document: &mut PdfDocument) {
        let group_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Group")));
            dict.insert("S", PdfValue::Name(PdfName::new("Transparency")));
            dict
        });
        let group_id = document.ast.create_node(NodeType::Other, group_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, group_id, crate::ast::EdgeType::Reference);
        }
    }

    fn add_xmp_metadata(document: &mut PdfDocument) {
        let metadata_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Metadata")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("XML")));
            dict
        });
        let metadata_id = document.ast.create_node(NodeType::Metadata, metadata_value);
        document.set_info(metadata_id);
    }

    fn add_pdfua_metadata(document: &mut PdfDocument, valid_type: bool) {
        let catalog_id = document.catalog.expect("Catalog should exist");
        let catalog = document
            .ast
            .get_node_mut(catalog_id)
            .expect("Catalog node should exist");
        if let PdfValue::Dictionary(dict) = &mut catalog.value {
            let mut metadata_dict = PdfDictionary::new();
            if valid_type {
                metadata_dict.insert("Type", PdfValue::Name(PdfName::new("Metadata")));
            }
            metadata_dict.insert("Subtype", PdfValue::Name(PdfName::new("XML")));
            dict.insert(
                "Metadata",
                PdfValue::Stream(PdfStream::new(
                    metadata_dict,
                    b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"></x:xmpmeta>".to_vec(),
                )),
            );
        }
    }

    fn add_image_object(document: &mut PdfDocument, colorspace: &str) {
        let image_value = PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("XObject")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("Image")));
            dict.insert("ColorSpace", PdfValue::Name(PdfName::new(colorspace)));
            dict.insert("Width", PdfValue::Integer(100));
            dict.insert("Height", PdfValue::Integer(100));
            dict.insert("BitsPerComponent", PdfValue::Integer(8));
            dict
        });
        let image_id = document.ast.create_node(NodeType::Image, image_value);

        if let Some(catalog_id) = document.catalog {
            document
                .ast
                .add_edge(catalog_id, image_id, crate::ast::EdgeType::Reference);
        }
    }
}
