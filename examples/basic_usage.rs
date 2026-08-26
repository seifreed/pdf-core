use pdf_ast::{
    AstNode, NodeType, PdfDocument, PdfName, PdfValue, QueryBuilder, Visitor, VisitorAction,
};
// Removed unused imports: File, BufReader

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("pdf-core - Basic Usage Example\n");

    demonstrate_ast_creation();
    demonstrate_visitor_pattern();
    demonstrate_query_builder();
    demonstrate_security_analysis();

    Ok(())
}

fn demonstrate_ast_creation() {
    println!("=== Creating PDF AST ===");

    let mut doc = PdfDocument::new(pdf_ast::PdfVersion::new(1, 7));

    let mut catalog_dict = pdf_ast::PdfDictionary::new();
    catalog_dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
    catalog_dict.insert("Version", PdfValue::Name(PdfName::new("1.7")));

    let catalog_id = doc
        .ast
        .create_node(NodeType::Catalog, PdfValue::Dictionary(catalog_dict));
    doc.set_catalog(catalog_id);

    let mut page_dict = pdf_ast::PdfDictionary::new();
    page_dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
    page_dict.insert(
        "MediaBox",
        PdfValue::Array(pdf_ast::PdfArray::from(vec![
            PdfValue::Integer(0),
            PdfValue::Integer(0),
            PdfValue::Integer(612),
            PdfValue::Integer(792),
        ])),
    );

    let page_id = doc
        .ast
        .create_node(NodeType::Page, PdfValue::Dictionary(page_dict));

    doc.ast
        .add_edge(catalog_id, page_id, pdf_ast::ast::EdgeType::Child);

    println!("Created document with {} nodes", doc.ast.node_count());
    println!("PDF Version: {}", doc.version);

    let validation_errors = doc.validate_structure();
    if validation_errors.is_empty() {
        println!("Document structure is valid\n");
    } else {
        println!("Validation errors: {:?}\n", validation_errors);
    }
}

fn demonstrate_visitor_pattern() {
    println!("=== Visitor Pattern ===");

    struct PageCounter {
        count: usize,
    }

    impl Visitor for PageCounter {
        fn visit_page(&mut self, _node: &AstNode, _dict: &pdf_ast::PdfDictionary) -> VisitorAction {
            self.count += 1;
            VisitorAction::Continue
        }
    }

    let doc = create_sample_document();
    let mut counter = PageCounter { count: 0 };
    let mut walker = pdf_ast::visitor::AstWalker::new(&doc.ast);
    walker.walk(&mut counter);

    println!("✓ Found {} pages using visitor pattern\n", counter.count);
}

fn demonstrate_query_builder() {
    println!("=== Query Builder ===");

    let doc = create_sample_document();

    let pages = QueryBuilder::new()
        .with_type(NodeType::Page)
        .execute(&doc.ast);

    println!("✓ Found {} pages using query builder", pages.len());

    let error_nodes = QueryBuilder::new().with_errors(true).execute(&doc.ast);

    println!("✓ Found {} nodes with errors", error_nodes.len());

    let shallow_nodes = QueryBuilder::new().with_max_depth(2).execute(&doc.ast);

    println!("✓ Found {} nodes at depth ≤ 2\n", shallow_nodes.len());
}

fn demonstrate_security_analysis() {
    println!("=== Security Analysis ===");

    struct SecurityAnalyzer {
        javascript_found: bool,
        embedded_files: Vec<String>,
        suspicious_actions: Vec<String>,
    }

    impl Visitor for SecurityAnalyzer {
        fn visit_action(
            &mut self,
            _node: &AstNode,
            dict: &pdf_ast::PdfDictionary,
        ) -> VisitorAction {
            if let Some(PdfValue::Name(action_type)) = dict.get("S") {
                match action_type.without_slash() {
                    "JavaScript" | "JS" => {
                        self.javascript_found = true;
                        self.suspicious_actions
                            .push("JavaScript action detected".to_string());
                    }
                    "Launch" => {
                        self.suspicious_actions
                            .push("Launch action detected".to_string());
                    }
                    "URI" => {
                        if let Some(PdfValue::String(uri)) = dict.get("URI") {
                            self.suspicious_actions
                                .push(format!("URI action: {}", uri.to_string_lossy()));
                        }
                    }
                    _ => {}
                }
            }
            VisitorAction::Continue
        }

        fn visit_embedded_file(
            &mut self,
            _node: &AstNode,
            dict: &pdf_ast::PdfDictionary,
        ) -> VisitorAction {
            if let Some(PdfValue::String(name)) = dict.get("F") {
                self.embedded_files.push(name.to_string_lossy());
            }
            VisitorAction::Continue
        }
    }

    let doc = create_sample_document_with_actions();
    let mut analyzer = SecurityAnalyzer {
        javascript_found: false,
        embedded_files: Vec::new(),
        suspicious_actions: Vec::new(),
    };

    let mut walker = pdf_ast::visitor::AstWalker::new(&doc.ast);
    walker.walk(&mut analyzer);

    println!("Security Analysis Results:");
    println!("  JavaScript detected: {}", analyzer.javascript_found);
    println!("  Embedded files: {}", analyzer.embedded_files.len());
    println!(
        "  Suspicious actions: {}",
        analyzer.suspicious_actions.len()
    );

    for action in &analyzer.suspicious_actions {
        println!("    - {}", action);
    }

    println!();
}

fn create_sample_document() -> PdfDocument {
    let mut doc = PdfDocument::new(pdf_ast::PdfVersion::new(1, 7));

    let mut catalog_dict = pdf_ast::PdfDictionary::new();
    catalog_dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));

    let catalog_id = doc
        .ast
        .create_node(NodeType::Catalog, PdfValue::Dictionary(catalog_dict));
    doc.set_catalog(catalog_id);

    for i in 0..3 {
        let mut page_dict = pdf_ast::PdfDictionary::new();
        page_dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
        page_dict.insert("PageNumber", PdfValue::Integer(i + 1));

        let page_id = doc
            .ast
            .create_node(NodeType::Page, PdfValue::Dictionary(page_dict));

        doc.ast
            .add_edge(catalog_id, page_id, pdf_ast::ast::EdgeType::Child);
    }

    doc
}

fn create_sample_document_with_actions() -> PdfDocument {
    let mut doc = create_sample_document();

    let mut js_action = pdf_ast::PdfDictionary::new();
    js_action.insert("Type", PdfValue::Name(PdfName::new("Action")));
    js_action.insert("S", PdfValue::Name(PdfName::new("JavaScript")));
    js_action.insert(
        "JS",
        PdfValue::String(pdf_ast::PdfString::new_literal(
            b"app.alert('Hello from PDF!');",
        )),
    );

    let js_id = doc
        .ast
        .create_node(NodeType::Action, PdfValue::Dictionary(js_action));

    if let Some(catalog_id) = doc.catalog {
        doc.ast
            .add_edge(catalog_id, js_id, pdf_ast::ast::EdgeType::Child);
    }

    let mut uri_action = pdf_ast::PdfDictionary::new();
    uri_action.insert("Type", PdfValue::Name(PdfName::new("Action")));
    uri_action.insert("S", PdfValue::Name(PdfName::new("URI")));
    uri_action.insert(
        "URI",
        PdfValue::String(pdf_ast::PdfString::new_literal(b"https://example.com")),
    );

    let uri_id = doc
        .ast
        .create_node(NodeType::Action, PdfValue::Dictionary(uri_action));

    if let Some(catalog_id) = doc.catalog {
        doc.ast
            .add_edge(catalog_id, uri_id, pdf_ast::ast::EdgeType::Child);
    }

    doc
}
