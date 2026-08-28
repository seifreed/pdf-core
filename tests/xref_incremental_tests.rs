use pdf_ast::parser::PdfParser;
use pdf_ast::types::ObjectId;

struct IncrementalPdf {
    data: Vec<u8>,
    xref1_offset: u64,
    xref2_offset: u64,
}

fn build_incremental_pdf() -> IncrementalPdf {
    let mut pdf = String::new();
    pdf.push_str("%PDF-1.4\n");

    let obj1_offset = pdf.len();
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.push_str("3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>\nendobj\n");

    let xref1_offset = pdf.len();
    pdf.push_str("xref\n0 4\n");
    pdf.push_str("0000000000 65535 f \n");
    pdf.push_str(&format!("{:010} 00000 n \n", obj1_offset));
    pdf.push_str(&format!("{:010} 00000 n \n", obj2_offset));
    pdf.push_str(&format!("{:010} 00000 n \n", obj3_offset));
    pdf.push_str("trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.push_str("startxref\n");
    pdf.push_str(&format!("{}\n", xref1_offset));
    pdf.push_str("%%EOF\n");

    let obj4_offset = pdf.len();
    pdf.push_str("4 0 obj\n<< /Producer (Incremental) >>\nendobj\n");

    let xref2_offset = pdf.len();
    pdf.push_str("xref\n0 5\n");
    pdf.push_str("0000000000 65535 f \n");
    pdf.push_str(&format!("{:010} 00000 n \n", obj1_offset));
    pdf.push_str(&format!("{:010} 00000 n \n", obj2_offset));
    pdf.push_str(&format!("{:010} 00000 n \n", obj3_offset));
    pdf.push_str(&format!("{:010} 00000 n \n", obj4_offset));
    pdf.push_str("trailer\n<< /Size 5 /Root 1 0 R /Info 4 0 R /Prev ");
    pdf.push_str(&format!("{}", xref1_offset));
    pdf.push_str(" >>\n");
    pdf.push_str("startxref\n");
    pdf.push_str(&format!("{}\n", xref2_offset));
    pdf.push_str("%%EOF\n");

    IncrementalPdf {
        data: pdf.into_bytes(),
        xref1_offset: xref1_offset as u64,
        xref2_offset: xref2_offset as u64,
    }
}

#[test]
fn test_incremental_xref_chain() {
    let pdf = build_incremental_pdf();
    let parser = PdfParser::new();
    let document = parser
        .parse_bytes(&pdf.data)
        .expect("parse incremental pdf");

    assert!(document.xref.entries.contains_key(&ObjectId::new(4, 0)));
    assert_eq!(document.revisions.len(), 2);
    assert_eq!(document.revisions[0].xref_offset, pdf.xref2_offset);
    assert_eq!(document.revisions[1].xref_offset, pdf.xref1_offset);
    assert!(document.revisions[0]
        .added_objects
        .contains(&ObjectId::new(4, 0)));
}

#[test]
fn invalid_prev_is_an_error_in_strict_mode_and_a_diagnostic_in_tolerant_mode() {
    let pdf = build_incremental_pdf();
    let old_prev = format!("/Prev {}", pdf.xref1_offset);
    let new_prev = "/Prev -1";
    let mut text = String::from_utf8(pdf.data).expect("test PDF should be UTF-8");
    text = text.replace(&old_prev, new_prev);
    let data = text.into_bytes();

    assert!(PdfParser::strict().parse_bytes(&data).is_err());
    let document = PdfParser::new()
        .parse_bytes(&data)
        .expect("tolerant parser should retain the current revision");
    assert!(document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.error_code == "invalid_prev"));
}

#[test]
fn invalid_prev_type_is_an_error_in_strict_mode_and_a_diagnostic_in_tolerant_mode() {
    let pdf = build_incremental_pdf();
    let old_prev = format!("/Prev {}", pdf.xref1_offset);
    let mut text = String::from_utf8(pdf.data).expect("test PDF should be UTF-8");
    text = text.replace(&old_prev, "/Prev (invalid)");
    let data = text.into_bytes();

    assert!(PdfParser::strict().parse_bytes(&data).is_err());
    let document = PdfParser::new()
        .parse_bytes(&data)
        .expect("tolerant parser should retain the current revision");
    assert!(document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.error_code == "invalid_prev_type"));
}

fn build_hybrid_pdf() -> (Vec<u8>, u64, u64) {
    let mut pdf = b"%PDF-1.5\n".to_vec();
    let objects = [
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".as_slice(),
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n".as_slice(),
    ];
    let mut offsets = Vec::new();
    for object in objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object);
    }

    let xref_stream_offset = pdf.len() as u64;
    let xref_stream_length = 7;
    let mut xref_stream = vec![1u8];
    xref_stream.extend_from_slice(&(xref_stream_offset as u32).to_be_bytes());
    xref_stream.extend_from_slice(&0u16.to_be_bytes());
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XRef /Size 5 /Index [4 1] /W [1 4 2] /Root 1 0 R /Length {xref_stream_length} >>\nstream\n"
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&xref_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let classic_xref_offset = pdf.len() as u64;
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size 5 /Root 1 0 R /XRefStm {xref_stream_offset} >>\nstartxref\n{classic_xref_offset}\n%%EOF"
        )
        .as_bytes(),
    );
    (pdf, xref_stream_offset, classic_xref_offset)
}

#[test]
fn test_hybrid_xref_table_and_stream() {
    let (data, xref_stream_offset, classic_xref_offset) = build_hybrid_pdf();
    let document = PdfParser::new()
        .parse_bytes(&data)
        .expect("parse hybrid xref PDF");

    assert!(document.xref.hybrid_mode);
    assert_eq!(document.revisions.len(), 1);
    assert_eq!(document.revisions[0].xref_offset, classic_xref_offset);
    assert!(document.xref.entries.contains_key(&ObjectId::new(4, 0)));
    assert!(document
        .xref
        .streams
        .iter()
        .any(|stream| stream.object_id == ObjectId::new(4, 0)));
    assert!(data.len() > xref_stream_offset as usize);
}

#[test]
fn invalid_xref_stream_offset_is_an_error_in_strict_mode_and_a_diagnostic_in_tolerant_mode() {
    let (data, xref_stream_offset, _) = build_hybrid_pdf();
    let marker = format!("/XRefStm {}", xref_stream_offset);
    let replacement = b"/XRefStm (invalid)";
    let mut data = data;
    let start = data
        .windows(marker.len())
        .position(|window| window == marker.as_bytes())
        .expect("hybrid trailer should contain XRefStm");
    data.splice(start..start + marker.len(), replacement.iter().copied());

    assert!(PdfParser::strict().parse_bytes(&data).is_err());
    let document = PdfParser::new()
        .parse_bytes(&data)
        .expect("tolerant parser should retain the classic xref table");
    assert!(document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.error_code == "invalid_xref_stm_type"));
}
