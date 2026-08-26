#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let handler = pdf_ast::crypto::pkcs7::Pkcs7Handler::new();
    let _ = handler.parse_signed_data(data);
    let _ = handler.compute_digest(data, "SHA256");
});
