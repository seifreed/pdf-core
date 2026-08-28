use pdf_ast::filters::decode_stream_with_limits;
use pdf_ast::filters::jpx::{
    decode_jpx_image, decode_jpx_to_codestream, decode_jpx_to_codestream_with_limit,
};
use pdf_ast::parser::PdfParser;
use pdf_ast::types::StreamFilter;

#[test]
fn decode_jpx_raw_codestream() {
    let data = vec![0xFF, 0x4F, 0x00, 0x01, 0x02];
    let out = decode_jpx_to_codestream(&data).expect("decode");
    assert_eq!(out, data);
}

#[test]
fn decode_jp2_container_extracts_codestream() {
    // JP2 signature box (12 bytes)
    let mut data = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    // ftyp box (16 bytes length, 8 bytes header + 8 payload)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x10, b'f', b't', b'y', b'p', 0x6A, 0x70, 0x32, 0x20, 0x00, 0x00, 0x00,
        0x00,
    ]);
    // jp2c box with payload
    let payload = vec![0x11, 0x22, 0x33];
    let len = (8 + payload.len()) as u32;
    data.extend_from_slice(&len.to_be_bytes());
    data.extend_from_slice(b"jp2c");
    data.extend_from_slice(&payload);

    let out = decode_jpx_to_codestream(&data).expect("decode");
    assert_eq!(out, payload);
}

#[test]
fn limited_jpx_decode_rejects_oversized_codestream() {
    let data = vec![0xFF, 0x4F, 0x00, 0x01, 0x02];
    assert!(decode_jpx_to_codestream_with_limit(&data, 4).is_err());
}

#[test]
fn decodes_serialized_jpx_corpus_image() {
    let Some(root) = std::env::var_os("PDF_COMPLIANCE_CORPUS") else {
        eprintln!("Skipping serialized JPX decode: corpus root is not configured");
        return;
    };
    let path = std::path::PathBuf::from(root).join(
        "verapdf-pdfa-2b/6.2 Graphics/6.2.8 Images/6.2.8.3 JPEG2000/\
         veraPDF test suite 6-2-8-3-t01-pass-a.pdf",
    );
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read serialized JPX fixture {}: {error}", path.display()));
    let document = PdfParser::new()
        .parse_bytes(&bytes)
        .expect("serialized JPX fixture should parse");
    let stream = document
        .ast
        .get_all_nodes()
        .into_iter()
        .find_map(|node| {
            node.as_stream().filter(|stream| {
                stream.raw_data().is_some_and(|raw| {
                    raw.starts_with(b"\x00\x00\x00\x0cjP  ") || raw.starts_with(b"\xff\x4f")
                })
            })
        })
        .expect("serialized JPX fixture should expose a raw JPX stream");
    let raw = stream.raw_data().expect("image stream should be resident");
    let decoded = decode_stream_with_limits(raw, &[StreamFilter::JPXDecode], 640 * 480 * 4, 100)
        .expect("JPX stream filter should decode");
    assert!(decoded.len() > raw.len());
    assert!(decode_jpx_image(raw, decoded.len() - 1).is_err());
}
