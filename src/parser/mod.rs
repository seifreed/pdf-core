pub mod accessibility;
pub mod cmap;
pub mod colorspace;
pub mod content_analyzer;
pub mod content_operands;
pub mod content_stream;
pub mod document_parser;
pub mod extgstate;
pub mod functions;
pub mod lazy_stream;
pub mod lexer;
pub mod names_tree;
pub mod object_parser;
pub mod ocg;
pub mod outlines;
pub mod output_intents;
pub mod page_tree;
pub mod pdf_file;
pub mod reference_resolver;
pub mod struct_tree;
pub mod text_extraction;
pub mod xref;

use crate::ast::{AstError, AstResult, ParseDiagnostic, PdfDocument};
use crate::performance::{PerformanceLimits, ResourceBudget};
use crate::types::PdfValue;
use std::io::{BufRead, Read, Seek};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    Strict,
    Tolerant,
    Forensic,
}

impl ParseMode {
    pub(crate) fn is_tolerant(self) -> bool {
        !matches!(self, Self::Strict)
    }

    pub(crate) fn is_forensic(self) -> bool {
        matches!(self, Self::Forensic)
    }
}

#[allow(dead_code)]
pub struct PdfParser {
    mode: ParseMode,
    max_errors: usize,
    limits: PerformanceLimits,
}

impl PdfParser {
    /// Creates a new tolerant PDF parser with default settings.
    ///
    /// Default configuration:
    /// - Tolerant mode enabled (attempts to parse malformed PDFs)
    /// - Maximum nesting depth: 100
    /// - Maximum errors before abort: 1000
    /// - Default performance limits
    ///
    /// # Returns
    /// A new `PdfParser` configured for tolerant parsing
    pub fn new() -> Self {
        PdfParser {
            mode: ParseMode::Tolerant,
            max_errors: 1000,
            limits: PerformanceLimits::default(),
        }
    }

    /// Creates a strict PDF parser that rejects malformed documents.
    ///
    /// Strict configuration:
    /// - Tolerant mode disabled (fails on spec violations)
    /// - Maximum nesting depth: 100
    /// - No error tolerance (max_errors: 0)
    /// - Default performance limits
    ///
    /// # Returns
    /// A new `PdfParser` configured for strict parsing
    pub fn strict() -> Self {
        PdfParser {
            mode: ParseMode::Strict,
            max_errors: 0,
            limits: PerformanceLimits::default(),
        }
    }

    /// Sets the tolerance mode for parsing.
    ///
    /// # Arguments
    /// * `tolerant` - If true, attempts to parse malformed PDFs; if false, strictly follows PDF spec
    ///
    /// # Returns
    /// Self for method chaining
    pub fn with_tolerance(mut self, tolerant: bool) -> Self {
        self.mode = if tolerant {
            ParseMode::Tolerant
        } else {
            ParseMode::Strict
        };
        self
    }

    pub fn with_max_errors(mut self, max_errors: usize) -> Self {
        self.max_errors = max_errors;
        self
    }

    pub fn forensic() -> Self {
        Self {
            mode: ParseMode::Forensic,
            max_errors: 10_000,
            ..Self::new()
        }
    }

    pub fn with_mode(mut self, mode: ParseMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn mode(&self) -> ParseMode {
        self.mode
    }

    pub(crate) fn resource_budget(&self) -> ResourceBudget {
        self.limits.budget.clone()
    }

    /// Sets the maximum nesting depth for PDF objects.
    ///
    /// # Arguments
    /// * `depth` - Maximum allowed nesting level (prevents stack overflow from deeply nested structures)
    ///
    /// # Returns
    /// Self for method chaining
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.limits.max_depth = depth;
        self.limits.refresh_budget();
        self
    }

    /// Sets performance limits for parsing operations.
    ///
    /// # Arguments
    /// * `limits` - Performance limits including timeouts and resource constraints
    ///
    /// # Returns
    /// Self for method chaining
    pub fn with_limits(mut self, limits: PerformanceLimits) -> Self {
        let mut limits = limits;
        limits.refresh_budget();
        self.limits = limits;
        self
    }

    /// Uses an explicit shared resource budget for every parser phase.
    pub fn with_resource_budget(mut self, budget: ResourceBudget) -> Self {
        self.limits.max_file_size_mb = budget_megabytes(budget.max_input_bytes);
        self.limits.max_memory_mb = budget_megabytes(budget.max_decoded_bytes_total);
        self.limits.max_object_size_mb = budget_megabytes(budget.max_decoded_bytes_per_stream);
        self.limits.max_stream_decode_ratio =
            usize::try_from(budget.max_decode_ratio).unwrap_or(usize::MAX);
        self.limits.max_nodes = budget.max_nodes;
        self.limits.max_edges = budget.max_edges;
        self.limits.max_depth = budget.max_depth;
        self.limits.budget = budget;
        self
    }

    /// Parses a PDF document from a reader.
    ///
    /// # Arguments
    /// * `reader` - A reader implementing Read, Seek, and BufRead (e.g., File, Cursor)
    ///
    /// # Returns
    /// A parsed `PdfDocument` with populated AST graph
    ///
    /// # Errors
    /// Returns `AstError::ParseError` if the PDF cannot be parsed
    pub fn parse<R: Read + Seek + BufRead>(&self, reader: R) -> AstResult<PdfDocument> {
        let parser = document_parser::DocumentParser::new_with_limits(
            reader,
            self.mode,
            self.max_errors,
            self.limits.clone(),
        );
        parser.parse()
    }

    /// Parses a PDF document from a byte slice.
    ///
    /// # Arguments
    /// * `data` - Raw PDF file bytes
    ///
    /// # Returns
    /// A parsed `PdfDocument` with populated AST graph
    ///
    /// # Errors
    /// Returns `AstError::ParseError` if the PDF is malformed
    pub fn parse_bytes(&self, data: &[u8]) -> AstResult<PdfDocument> {
        use std::io::Cursor;
        let cursor = Cursor::new(data);
        let mut document = self.parse(cursor)?;
        document.original_bytes = Some(data.to_vec());
        Ok(document)
    }

    /// Parses a single PDF value from bytes.
    ///
    /// # Arguments
    /// * `input` - Byte slice containing a PDF value (number, string, array, dictionary, etc.)
    ///
    /// # Returns
    /// The parsed `PdfValue`
    ///
    /// # Errors
    /// Returns `AstError::ParseError` if the value cannot be parsed
    pub fn parse_value(&self, input: &[u8]) -> AstResult<PdfValue> {
        self.consume_input(input)?;
        self.consume_object()?;
        self.parse_value_unbudgeted(input)
    }

    fn parse_value_unbudgeted(&self, input: &[u8]) -> AstResult<PdfValue> {
        let (remaining, value) =
            object_parser::parse_value_with_max_depth_unbudgeted(input, self.limits.max_depth)
                .map_err(|e| AstError::ParseError(format!("{:?}", e)))?;
        self.ensure_strictly_consumed(remaining)?;
        Ok(value)
    }

    /// Parses a PDF object from bytes.
    ///
    /// # Arguments
    /// * `input` - Byte slice containing a PDF object
    ///
    /// # Returns
    /// The parsed `PdfValue`
    ///
    /// # Errors
    /// Returns `AstError::ParseError` if the object cannot be parsed
    pub fn parse_object(&self, input: &[u8]) -> AstResult<PdfValue> {
        self.consume_input(input)?;
        self.consume_object()?;
        let object_input = crate::parser::lexer::skip_whitespace_and_comments(input)
            .map(|(remaining, _)| remaining)
            .unwrap_or(input);
        if self.mode == ParseMode::Strict
            && object_parser::parse_indirect_object_header(object_input).is_ok()
        {
            let (remaining, (_, value)) =
                object_parser::parse_indirect_object_with_max_depth_unbudgeted(
                    object_input,
                    self.limits.max_depth,
                )
                .map_err(|e| AstError::ParseError(format!("{:?}", e)))?;
            self.ensure_strictly_consumed(remaining)?;
            return Ok(value);
        }
        if let Ok((_, (_, value))) = object_parser::parse_indirect_object_with_max_depth_unbudgeted(
            object_input,
            self.limits.max_depth,
        ) {
            return Ok(value);
        }
        self.parse_value_unbudgeted(input)
    }

    fn consume_input(&self, input: &[u8]) -> AstResult<()> {
        self.limits
            .budget
            .consume_input(input.len() as u64)
            .map_err(|error| AstError::ParseError(error.to_string()))
    }

    fn consume_object(&self) -> AstResult<()> {
        self.limits
            .budget
            .consume_object()
            .map_err(|error| AstError::ParseError(error.to_string()))
    }

    fn ensure_strictly_consumed(&self, remaining: &[u8]) -> AstResult<()> {
        if self.mode == ParseMode::Strict {
            let remaining = crate::parser::lexer::skip_whitespace_and_comments(remaining)
                .map(|(remaining, _)| remaining)
                .unwrap_or(remaining);
            if !remaining.is_empty() {
                return Err(AstError::ParseError(format!(
                    "Trailing bytes after PDF value: {:?}",
                    remaining
                )));
            }
        }
        Ok(())
    }

    /// Parses multiple consecutive PDF objects from bytes.
    ///
    /// # Arguments
    /// * `input` - Byte slice containing multiple PDF objects
    ///
    /// # Returns
    /// A vector of parsed `PdfValue` objects
    ///
    /// # Errors
    /// Returns `AstError` when an object cannot be parsed within the configured error policy.
    pub fn parse_objects(&self, input: &[u8]) -> AstResult<Vec<PdfValue>> {
        self.parse_objects_with_diagnostics(input)
            .map(|(objects, _)| objects)
    }

    /// Parses multiple objects and returns structured recovery diagnostics.
    pub fn parse_objects_with_diagnostics(
        &self,
        input: &[u8],
    ) -> AstResult<(Vec<PdfValue>, Vec<ParseDiagnostic>)> {
        self.consume_input(input)?;
        let mut objects = Vec::new();
        let mut diagnostics = Vec::new();
        let mut remaining = input;
        let mut errors = 0;

        while !remaining.is_empty() {
            let object_input = crate::parser::lexer::skip_whitespace_and_comments(remaining)
                .map(|(remaining, _)| remaining)
                .unwrap_or(remaining);
            if object_input.is_empty() {
                break;
            }
            self.consume_object()?;
            let parsed = if object_parser::parse_indirect_object_header(object_input).is_ok() {
                object_parser::parse_indirect_object_with_max_depth_unbudgeted(
                    object_input,
                    self.limits.max_depth,
                )
                .map(|(rest, (_, value))| (rest, value))
            } else {
                object_parser::parse_value_with_max_depth_unbudgeted(
                    object_input,
                    self.limits.max_depth,
                )
            };

            match parsed {
                Ok((rest, value)) => {
                    objects.push(value);
                    remaining = rest;
                }
                Err(err) => {
                    errors += 1;
                    if !self.mode.is_tolerant() || errors > self.max_errors {
                        return Err(AstError::ParseError(format!(
                            "Failed to parse object: {:?}",
                            err
                        )));
                    }
                    let bytes_consumed = input.len().saturating_sub(remaining.len()) as u64;
                    diagnostics.push(ParseDiagnostic {
                        object_id: None,
                        offset: Some(bytes_consumed),
                        error_code: "standalone_object_parse".to_string(),
                        recovery_action: "skipped_to_next_line".to_string(),
                        confidence: 0.0,
                        bytes_consumed,
                        message: format!("Failed to parse object: {:?}", err),
                    });
                    let skip = remaining
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map(|position| position + 1)
                        .unwrap_or(remaining.len());
                    if skip == 0 || skip == remaining.len() {
                        break;
                    }
                    remaining = &remaining[skip..];
                }
            }
        }

        Ok((objects, diagnostics))
    }
}

fn budget_megabytes(bytes: u64) -> usize {
    usize::try_from(bytes.div_ceil(1024 * 1024)).unwrap_or(usize::MAX)
}

impl Default for PdfParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::PdfParser;
    use crate::performance::{ResourceBudget, ResourceBudgetError};

    #[test]
    fn public_value_parsing_charges_the_object_budget() {
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 0, 10, 10, 10);
        let error = PdfParser::new()
            .with_resource_budget(budget)
            .parse_value(b"null")
            .expect_err("top-level values must consume object budget");
        assert!(error
            .to_string()
            .contains(&format!("{:?}", ResourceBudgetError::Objects)));
    }
}
