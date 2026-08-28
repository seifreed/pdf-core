use pdf_ast::filters::jpx::decode_jpx_to_codestream;

#[test]
fn reject_missing_signature() {
    let data = vec![0x00, 0x00, 0x00, 0x08, b'j', b'p', b'2', b'c'];
    assert!(decode_jpx_to_codestream(&data).is_err());
}

#[test]
fn reject_no_codestream() {
    let data = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    assert!(decode_jpx_to_codestream(&data).is_err());
}

#[test]
fn reject_extended_length_that_exceeds_platform_size() {
    let mut data = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    data.extend_from_slice(&[0, 0, 0, 1, b'j', b'p', b'2', b'c']);
    data.extend_from_slice(&u64::MAX.to_be_bytes());

    assert!(decode_jpx_to_codestream(&data).is_err());
}

#[test]
fn reject_trailing_bytes_after_jp2_boxes() {
    let mut data = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x09, b'j', b'p', b'2', b'c', 0xFF]);
    data.push(0x00);

    assert!(decode_jpx_to_codestream(&data)
        .expect_err("trailing bytes must invalidate the JP2 container")
        .to_string()
        .contains("trailing bytes"));
}
