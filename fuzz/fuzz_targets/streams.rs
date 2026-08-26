#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::parser::object_parser::parse_indirect_stream_prefix(data);
    let _ = pdf_ast::parser::object_parser::parse_indirect_object_with_stream_length(
        data,
        data.len(),
    );
    let _ = pdf_ast::parser::object_parser::parse_object_stream_offsets(data, 8, data.len() / 2);
});
