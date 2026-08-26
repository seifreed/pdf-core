use pdf_ast::filters::jpx::{decode_jpx_to_codestream, decode_jpx_to_codestream_with_limit};

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
