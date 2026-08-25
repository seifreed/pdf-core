#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let parser = pdf_ast::parser::PdfParser::forensic();
    let _ = parser.parse_bytes(data);
    let _ = pdf_ast::parser::object_parser::parse_value(data);
});
