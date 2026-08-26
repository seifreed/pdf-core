#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let object_count = data.first().copied().unwrap_or(0) as usize % 32;
    let first = data.get(1).copied().unwrap_or(0) as usize;
    let _ = pdf_ast::parser::object_parser::parse_object_stream_offsets(
        data,
        object_count,
        first,
    );
});
