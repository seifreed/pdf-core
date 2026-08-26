#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(document) = pdf_ast::parser::PdfParser::forensic().parse_bytes(data) {
        let _ = pdf_ast::serialization::to_json(&document);
    }

    if let Ok(json) = std::str::from_utf8(data) {
        if let Ok(graph) = pdf_ast::serialization::SerializableGraph::from_json(json) {
            let _ = pdf_ast::serialization::GraphDeserializer::deserialize(graph);
        }
    }

    if let Ok(graph) = pdf_ast::serialization::SerializableGraph::from_cbor(data) {
        let _ = pdf_ast::serialization::GraphDeserializer::deserialize(graph);
    }
});
