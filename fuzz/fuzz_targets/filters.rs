#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use pdf_ast::types::{CryptFilter, PdfName, StreamFilter};

    let filters = [
        vec![StreamFilter::ASCIIHexDecode],
        vec![StreamFilter::ASCII85Decode],
        vec![StreamFilter::FlateDecode(Default::default())],
        vec![StreamFilter::LZWDecode(Default::default())],
        vec![StreamFilter::RunLengthDecode],
        vec![StreamFilter::CCITTFaxDecode(Default::default())],
        vec![StreamFilter::JBIG2Decode],
        vec![StreamFilter::DCTDecode],
        vec![StreamFilter::JPXDecode],
        vec![StreamFilter::Crypt(CryptFilter {
            name: PdfName::new("Identity"),
        })],
    ];

    for filter in filters {
        let _ = pdf_ast::filters::decode_stream_with_limits(
            data,
            &filter,
            5 * 1024 * 1024,
            50,
        );
    }
});
