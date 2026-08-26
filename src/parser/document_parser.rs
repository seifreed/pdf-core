use crate::ast::{AstResult, PdfDocument};
use crate::parser::{pdf_file::PdfFileParser, ParseMode};
use crate::performance::PerformanceLimits;
use std::io::{BufRead, Read, Seek};

pub struct DocumentParser<R: Read + Seek + BufRead> {
    reader: R,
    mode: ParseMode,
    max_errors: usize,
    limits: PerformanceLimits,
}

impl<R: Read + Seek + BufRead> DocumentParser<R> {
    pub fn new(reader: R, mode: ParseMode, max_errors: usize) -> Self {
        Self::new_with_limits(reader, mode, max_errors, PerformanceLimits::default())
    }

    pub fn new_with_limits(
        reader: R,
        mode: ParseMode,
        max_errors: usize,
        limits: PerformanceLimits,
    ) -> Self {
        DocumentParser {
            reader,
            mode,
            max_errors,
            limits,
        }
    }

    pub fn parse(self) -> AstResult<PdfDocument> {
        let parser =
            PdfFileParser::new_with_limits(self.reader, self.mode, self.max_errors, self.limits)?;
        parser.parse()
    }
}
