#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(document) = pdf_ast::parser::PdfParser::forensic().parse_bytes(data) {
        let _ = pdf_ast::serialization::to_json(&document);
    }
});
