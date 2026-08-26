#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut graph = pdf_ast::ast::PdfAstGraph::new();
    let resolver = pdf_ast::parser::reference_resolver::ObjectNodeMap::new();
    let mut parser = pdf_ast::parser::cmap::CMapParser::new(&mut graph, &resolver);
    let stream = pdf_ast::types::PdfStream::new(pdf_ast::types::PdfDictionary::new(), data.to_vec());
    let _ = parser.parse_cmap_stream(&stream);
    let _ = parser.parse_tounicode_stream(&stream);
});
