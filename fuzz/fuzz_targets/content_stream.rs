#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::parser::content_operands::parse_content_stream(data);
    let _ = pdf_ast::parser::content_operands::parse_content_stream_with_offsets(data);
});
