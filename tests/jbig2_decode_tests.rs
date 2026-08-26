use pdf_ast::filters::decode_stream_with_limits;
use pdf_ast::filters::jbig2::decode_jbig2;
use pdf_ast::types::{PdfDictionary, PdfName, PdfStream, PdfValue, StreamFilter};

// Minimal sequential JBIG2 file: a 4x4 all-white page using MMR encoding.
#[rustfmt::skip]
const MINIMAL_JBIG2: &[u8] = &[
    0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x26, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x04, 0x01, 0xF0,
    0x00, 0x00, 0x00, 0x02, 0x31, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x03, 0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn decodes_embedded_jbig2_bitmap_with_output_limit() {
    let decoded = decode_jbig2(MINIMAL_JBIG2, None, 4).expect("JBIG2 should decode");
    assert_eq!(decoded, [0, 0, 0, 0]);
    assert!(decode_jbig2(MINIMAL_JBIG2, None, 3).is_err());

    let routed = decode_stream_with_limits(
        MINIMAL_JBIG2,
        &[StreamFilter::JBIG2Decode(Default::default())],
        4,
        100,
    )
    .expect("JBIG2 stream filter should decode");
    assert_eq!(routed, decoded);
}

#[test]
fn preserves_direct_jbig2_globals_from_decode_params() {
    let mut params = PdfDictionary::new();
    params.insert(
        PdfName::new("JBIG2Globals"),
        PdfValue::Stream(PdfStream::new(PdfDictionary::new(), vec![1, 2, 3])),
    );
    let mut dict = PdfDictionary::new();
    dict.insert(
        PdfName::new("Filter"),
        PdfValue::Name(PdfName::new("JBIG2Decode")),
    );
    dict.insert(PdfName::new("DecodeParms"), PdfValue::Dictionary(params));

    let filters = PdfStream::new(dict, Vec::new()).get_filters_with_params();
    assert_eq!(
        filters,
        vec![StreamFilter::JBIG2Decode(
            pdf_ast::types::JBIG2DecodeParams {
                globals: Some(vec![1, 2, 3]),
            }
        )]
    );
}
