#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::crypto::certificates::parse_der_certificate(data);
    let _ = pdf_ast::crypto::certificates::extract_ocsp_urls(data);
    let _ = pdf_ast::crypto::certificates::extract_crl_urls(data);
});
