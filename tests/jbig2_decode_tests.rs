use pdf_ast::filters::decode_stream_with_limits;
use pdf_ast::filters::jbig2::decode_jbig2;
use pdf_ast::parser::PdfParser;
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

#[test]
fn resolves_indirect_jbig2_globals_in_parsed_streams() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize; 6];
    append_object(
        &mut pdf,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    append_object(
        &mut pdf,
        &mut offsets,
        2,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    append_object(
        &mut pdf,
        &mut offsets,
        3,
        b"<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 4 0 R >> >> /MediaBox [0 0 4 4] >>",
    );

    let image_data = b"not-decoded-in-this-test";
    let image_dict = format!(
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode /DecodeParms << /JBIG2Globals 5 0 R >> /Length {} >>\nstream\n",
        image_data.len()
    );
    let mut image_body = image_dict.into_bytes();
    image_body.extend_from_slice(image_data);
    image_body.extend_from_slice(b"\nendstream");
    append_object(&mut pdf, &mut offsets, 4, &image_body);

    let global_body = b"<< /Length 3 >>\nstream\n\x01\x02\x03\nendstream";
    append_object(&mut pdf, &mut offsets, 5, global_body);

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );

    let document = PdfParser::new()
        .parse_bytes(&pdf)
        .expect("PDF with indirect JBIG2 globals should parse");
    let stream = document
        .ast
        .get_all_nodes()
        .into_iter()
        .find_map(|node| {
            node.as_stream().filter(|stream| {
                stream
                    .dict
                    .get("Subtype")
                    .and_then(PdfValue::as_name)
                    .is_some_and(|name| name.without_slash() == "Image")
            })
        })
        .expect("parsed image stream should be present");
    assert_eq!(
        stream.get_filters_with_params(),
        vec![StreamFilter::JBIG2Decode(
            pdf_ast::types::JBIG2DecodeParams {
                globals: Some(vec![1, 2, 3]),
            }
        )]
    );
}

fn append_object(pdf: &mut Vec<u8>, offsets: &mut [usize], id: usize, body: &[u8]) {
    offsets[id] = pdf.len();
    pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    pdf.extend_from_slice(body);
    pdf.extend_from_slice(b"\nendobj\n");
}
