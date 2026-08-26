#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let filters = [pdf_ast::types::StreamFilter::ASCIIHexDecode];
    let _ = pdf_ast::filters::decode_stream_with_limits(data, &filters, 5 * 1024 * 1024, 50);
});
