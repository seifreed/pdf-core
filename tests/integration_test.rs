use pdf_ast::*;

#[test]
fn test_document_creation() {
    let mut doc = PdfDocument::new(PdfVersion::new(1, 7));

    let mut catalog = PdfDictionary::new();
    catalog.insert("Type", PdfValue::Name(PdfName::new("Catalog")));

    let catalog_id = doc
        .ast
        .create_node(NodeType::Catalog, PdfValue::Dictionary(catalog));

    doc.set_catalog(catalog_id);

    assert_eq!(doc.version.major, 1);
    assert_eq!(doc.version.minor, 7);
    assert!(doc.catalog.is_some());
    assert_eq!(doc.ast.node_count(), 1);
}

#[test]
fn test_pdf_types() {
    let name = PdfName::new("Type");
    assert_eq!(name.as_str(), "/Type");
    assert_eq!(name.without_slash(), "Type");

    let string = PdfString::new_literal(b"Hello PDF");
    assert_eq!(string.to_string_lossy(), "Hello PDF");

    let mut array = PdfArray::new();
    array.push(PdfValue::Integer(42));
    array.push(PdfValue::Boolean(true));
    assert_eq!(array.len(), 2);

    let mut dict = PdfDictionary::new();
    dict.insert("Type", PdfValue::Name(PdfName::new("Page")));
    assert!(dict.contains_key("Type"));
}

#[test]
fn test_ast_graph() {
    let mut graph = PdfAstGraph::new();

    let root = graph.create_node(NodeType::Root, PdfValue::Dictionary(PdfDictionary::new()));

    let page = graph.create_node(NodeType::Page, PdfValue::Dictionary(PdfDictionary::new()));

    graph.set_root(root);
    graph.add_edge(root, page, ast::EdgeType::Child);

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
    assert!(!graph.is_cyclic());

    let children = graph.get_children(root);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], page);
}

#[test]
fn test_visitor_pattern() {
    struct NodeCounter {
        count: usize,
    }

    impl Visitor for NodeCounter {
        fn visit_node(&mut self, _node: &AstNode) -> VisitorAction {
            self.count += 1;
            VisitorAction::Continue
        }
    }

    let mut doc = PdfDocument::new(PdfVersion::new(1, 7));

    let catalog_id = doc.ast.create_node(
        NodeType::Catalog,
        PdfValue::Dictionary(PdfDictionary::new()),
    );
    doc.set_catalog(catalog_id);

    for _ in 0..3 {
        let page_id = doc
            .ast
            .create_node(NodeType::Page, PdfValue::Dictionary(PdfDictionary::new()));
        doc.ast.add_edge(catalog_id, page_id, ast::EdgeType::Child);
    }

    let mut counter = NodeCounter { count: 0 };
    let mut walker = visitor::AstWalker::new(&doc.ast);
    walker.walk(&mut counter);

    assert_eq!(counter.count, 4);
}

#[test]
fn test_visitor_dispatches_structure_and_cmap_nodes() {
    struct SpecializedCounter {
        struct_roots: usize,
        struct_elements: usize,
        cmaps: usize,
        tounicode: usize,
    }

    impl Visitor for SpecializedCounter {
        fn visit_struct_tree_root(
            &mut self,
            _node: &AstNode,
            _dict: &PdfDictionary,
        ) -> VisitorAction {
            self.struct_roots += 1;
            VisitorAction::Continue
        }

        fn visit_struct_elem(&mut self, _node: &AstNode, _dict: &PdfDictionary) -> VisitorAction {
            self.struct_elements += 1;
            VisitorAction::Continue
        }

        fn visit_cmap(&mut self, _node: &AstNode, _dict: &PdfDictionary) -> VisitorAction {
            self.cmaps += 1;
            VisitorAction::Continue
        }

        fn visit_tounicode(&mut self, _node: &AstNode, _dict: &PdfDictionary) -> VisitorAction {
            self.tounicode += 1;
            VisitorAction::Continue
        }
    }

    let mut doc = PdfDocument::new(PdfVersion::new(1, 7));
    let root = doc
        .ast
        .create_node(NodeType::Root, PdfValue::Dictionary(PdfDictionary::new()));
    doc.ast.set_root(root);
    for node_type in [
        NodeType::StructTreeRoot,
        NodeType::StructElem,
        NodeType::CMap,
        NodeType::ToUnicode,
    ] {
        let child = doc
            .ast
            .create_node(node_type, PdfValue::Dictionary(PdfDictionary::new()));
        doc.ast.add_edge(root, child, ast::EdgeType::Child);
    }

    let mut counter = SpecializedCounter {
        struct_roots: 0,
        struct_elements: 0,
        cmaps: 0,
        tounicode: 0,
    };
    visitor::AstWalker::new(&doc.ast).walk(&mut counter);

    assert_eq!(counter.struct_roots, 1);
    assert_eq!(counter.struct_elements, 1);
    assert_eq!(counter.cmaps, 1);
    assert_eq!(counter.tounicode, 1);
}

#[test]
fn test_query_builder() {
    let mut doc = PdfDocument::new(PdfVersion::new(1, 7));

    let catalog_id = doc.ast.create_node(
        NodeType::Catalog,
        PdfValue::Dictionary(PdfDictionary::new()),
    );
    doc.set_catalog(catalog_id);

    for _ in 0..5 {
        let page_id = doc
            .ast
            .create_node(NodeType::Page, PdfValue::Dictionary(PdfDictionary::new()));
        doc.ast.add_edge(catalog_id, page_id, ast::EdgeType::Child);
    }

    let pages = QueryBuilder::new()
        .with_type(NodeType::Page)
        .execute(&doc.ast);

    assert_eq!(pages.len(), 5);
}

#[test]
fn test_parser_basic() {
    let parser = PdfParser::new();

    let test_value = b"42";
    let result = parser.parse_value(test_value).unwrap();
    assert_eq!(result, PdfValue::Integer(42));

    let test_bool = b"true";
    let result = parser.parse_value(test_bool).unwrap();
    assert_eq!(result, PdfValue::Boolean(true));

    let test_name = b"/Type";
    let result = parser.parse_value(test_name).unwrap();
    match result {
        PdfValue::Name(name) => assert_eq!(name.as_str(), "/Type"),
        _ => panic!("Expected Name"),
    }
}
