#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let xml = String::from_utf8_lossy(data);
    let _ = pdf_ast::metadata::xmp::parse_xmp(&xml);
});
