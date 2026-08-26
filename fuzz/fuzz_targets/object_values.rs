#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::parser::object_parser::parse_value(data);
    let _ = pdf_ast::parser::object_parser::parse_indirect_object(data);
});
