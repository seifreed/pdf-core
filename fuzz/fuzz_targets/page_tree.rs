#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok((_, pdf_ast::types::PdfValue::Dictionary(dict))) =
        pdf_ast::parser::object_parser::parse_value(data)
    else {
        return;
    };

    let mut graph = pdf_ast::ast::PdfAstGraph::new();
    let parent_id = graph.create_node(
        pdf_ast::ast::NodeType::Pages,
        pdf_ast::types::PdfValue::Dictionary(dict.clone()),
    );
    let resolver = pdf_ast::parser::reference_resolver::ObjectNodeMap::new();
    let mut parser = pdf_ast::parser::page_tree::PageTreeParser::new(&mut graph, &resolver);
    let _ = parser.parse_page_tree(&dict, parent_id);
});
