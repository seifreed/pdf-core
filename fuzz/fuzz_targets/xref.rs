#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::parser::xref::parse_xref_table(data);
});
