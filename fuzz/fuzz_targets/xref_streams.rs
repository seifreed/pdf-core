#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut dictionary = pdf_ast::types::PdfDictionary::new();
    dictionary.insert(
        "W",
        pdf_ast::types::PdfValue::Array(pdf_ast::types::PdfArray::from(vec![
            pdf_ast::types::PdfValue::Integer(1),
            pdf_ast::types::PdfValue::Integer(4),
            pdf_ast::types::PdfValue::Integer(2),
        ])),
    );
    dictionary.insert("Size", pdf_ast::types::PdfValue::Integer(8));

    let stream = pdf_ast::types::PdfStream::new(dictionary, data.to_vec());
    let _ = pdf_ast::parser::xref::parse_xref_stream(
        &stream,
        &pdf_ast::performance::PerformanceLimits::default(),
    );
});
