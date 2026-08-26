#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pdf_ast::parser::lexer::skip_whitespace(data);
    let _ = pdf_ast::parser::lexer::skip_whitespace_and_comments(data);
    let _ = pdf_ast::parser::lexer::comment(data);
    let _ = pdf_ast::parser::lexer::pdf_header(data);
    let _ = pdf_ast::parser::lexer::pdf_eof(data);
    let _ = pdf_ast::parser::lexer::regular_chars(data);
    let _ = pdf_ast::parser::lexer::keyword(data);
    let _ = pdf_ast::parser::lexer::integer(data);
    let _ = pdf_ast::parser::lexer::real(data);
    let _ = pdf_ast::parser::lexer::hex_string(data);
    let _ = pdf_ast::parser::lexer::literal_string(data);
    let _ = pdf_ast::parser::lexer::name(data);
});
