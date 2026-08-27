use pdf_ast::forms::XfaDocument;
use pdf_ast::performance::ResourceBudget;
use pdf_ast::types::{PdfArray, PdfDictionary, PdfStream, PdfString, PdfValue};

#[test]
fn parse_xfa_from_stream_packet() {
    let xml = b"<xfa><datasets><data>ok</data></datasets></xfa>".to_vec();
    let stream = PdfStream::new(PdfDictionary::new(), xml);

    let mut acroform = PdfDictionary::new();
    acroform.insert("XFA", PdfValue::Stream(stream));

    let doc = XfaDocument::from_acroform(&acroform).unwrap();
    assert_eq!(doc.packets.len(), 1);
    assert_eq!(doc.packets[0].root.name, "xfa");
}

#[test]
fn parse_xfa_from_array_packets() {
    let xml = PdfString::new_literal(b"<xfa><form>v</form></xfa>");
    let mut arr = PdfArray::new();
    arr.push(PdfValue::Name("form".into()));
    arr.push(PdfValue::String(xml));

    let mut acroform = PdfDictionary::new();
    acroform.insert("XFA", PdfValue::Array(arr));

    let doc = XfaDocument::from_acroform(&acroform).unwrap();
    assert_eq!(doc.packets.len(), 1);
    assert_eq!(doc.packets[0].name, "form");
}

#[test]
fn rejects_xfa_stream_when_filter_decoding_fails() {
    let mut dict = PdfDictionary::new();
    dict.insert("Filter", PdfValue::Name("UnknownFilter".into()));
    let stream = PdfStream::new(dict, b"<xfa/>".to_vec());

    let mut acroform = PdfDictionary::new();
    acroform.insert("XFA", PdfValue::Stream(stream));

    let error = XfaDocument::from_acroform_with_budget(&acroform, &ResourceBudget::default())
        .expect_err("invalid XFA filters must not be parsed as raw XML");
    assert!(error.contains("Unsupported stream filter"));
}
