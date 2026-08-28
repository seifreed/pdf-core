use pdf_ast::ast::{NodeType, PdfDocument, PdfVersion};
use pdf_ast::types::{PdfDictionary, PdfName, PdfReference, PdfValue};
use pdf_ast::validation::{SchemaRegistry, ValidationSeverity};

#[test]
fn font_encoding_missing_warns() {
    let mut document = PdfDocument::new(PdfVersion { major: 2, minor: 0 });

    let font_dict = PdfValue::Dictionary({
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
        dict.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
        dict
    });
    let font_id = document.ast.create_node(NodeType::Font, font_dict);

    let catalog_value = PdfValue::Dictionary({
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
        dict.insert("Pages", PdfValue::Reference(PdfReference::new(2, 0)));
        dict
    });
    let catalog_id = document.ast.create_node(NodeType::Catalog, catalog_value);
    document.set_catalog(catalog_id);

    document
        .ast
        .add_edge(catalog_id, font_id, pdf_ast::ast::EdgeType::Child);

    let registry = SchemaRegistry::new();
    let report = registry.validate(&document, "PDF-2.0").expect("report");

    let has_warning = report.issues.iter().any(|issue| {
        issue.code == "FONT_ENCODING_MISSING" && issue.severity == ValidationSeverity::Warning
    });
    assert!(has_warning, "expected missing font encoding warning");
}

#[test]
fn cid_font_does_not_require_parent_font_mapping() {
    let mut document = PdfDocument::new(PdfVersion { major: 2, minor: 0 });
    let cid_font = document.ast.create_node(
        NodeType::CIDFont,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("CIDFontType2")));
            dict
        }),
    );
    let catalog_id = document.ast.create_node(
        NodeType::Catalog,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
            dict.insert("Pages", PdfValue::Reference(PdfReference::new(2, 0)));
            dict
        }),
    );
    document.set_catalog(catalog_id);
    document
        .ast
        .add_edge(catalog_id, cid_font, pdf_ast::ast::EdgeType::Child);

    let report = SchemaRegistry::new()
        .validate(&document, "PDF-2.0")
        .expect("report");
    assert!(!report
        .issues
        .iter()
        .any(|issue| issue.code == "FONT_ENCODING_MISSING"));
}

#[test]
fn invalid_tounicode_reference_warns() {
    let mut document = PdfDocument::new(PdfVersion { major: 2, minor: 0 });
    let invalid_tounicode = document.ast.create_node(
        NodeType::Object(pdf_ast::types::ObjectId::new(10, 0)),
        PdfValue::Integer(1),
    );
    let font_id = document.ast.create_node(
        NodeType::Font,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
            dict.insert("ToUnicode", PdfValue::Reference(PdfReference::new(10, 0)));
            dict
        }),
    );
    let catalog_id = document.ast.create_node(
        NodeType::Catalog,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
            dict.insert("Pages", PdfValue::Reference(PdfReference::new(2, 0)));
            dict
        }),
    );
    document.set_catalog(catalog_id);
    document
        .ast
        .add_edge(catalog_id, font_id, pdf_ast::ast::EdgeType::Child);
    assert!(document.ast.get_node(invalid_tounicode).is_some());

    let report = SchemaRegistry::new()
        .validate(&document, "PDF-2.0")
        .expect("report");
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "TOUNICODE_NOT_STREAM"));
}

#[test]
fn invalid_encoding_type_warns() {
    let mut document = PdfDocument::new(PdfVersion { major: 2, minor: 0 });
    let font_id = document.ast.create_node(
        NodeType::Font,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            dict.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
            dict.insert("Encoding", PdfValue::Integer(1));
            dict
        }),
    );
    let catalog_id = document.ast.create_node(
        NodeType::Catalog,
        PdfValue::Dictionary({
            let mut dict = PdfDictionary::new();
            dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
            dict.insert("Pages", PdfValue::Reference(PdfReference::new(2, 0)));
            dict
        }),
    );
    document.set_catalog(catalog_id);
    document
        .ast
        .add_edge(catalog_id, font_id, pdf_ast::ast::EdgeType::Child);

    let report = SchemaRegistry::new()
        .validate(&document, "PDF-2.0")
        .expect("report");
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "ENCODING_INVALID"));
}
