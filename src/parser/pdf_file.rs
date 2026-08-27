#![allow(dead_code)]

use crate::ast::document::XRefEntry;
use crate::ast::{AstError, AstResult, NodeType, PdfDocument, PdfVersion};
use crate::forms::{count_fields_in_acroform, has_hybrid_forms, XfaDocument};
use crate::metadata::XmpMetadata;
use crate::multimedia::av::{extract_audio_info, extract_video_info};
use crate::multimedia::richmedia::extract_richmedia_info;
use crate::multimedia::threed::extract_threed_info;
use crate::parser::lexer::*;
use crate::parser::object_parser;
use crate::parser::xref::parse_xref_table_with_budget;
use crate::parser::ParseMode;
use crate::performance::{PerformanceLimits, ResourceBudget};
use crate::security::ltv::extract_ltv_info;
use crate::types::*;
use std::collections::HashMap;
use std::io::{BufRead, Read, Seek, SeekFrom};

type ParseHeaderResult<'a> = Result<(&'a [u8], ObjectId), nom::Err<nom::error::Error<&'a [u8]>>>;

// Buffer size constants
const LINEARIZATION_BUFFER_SIZE: usize = 1024;
const HEADER_BUFFER_SIZE: usize = 32;
const HEADER_SEARCH_BUFFER_SIZE: usize = 1024;
const XREF_TAIL_BUFFER_SIZE: i64 = 1024;

// PDF structure constants
const MIN_PDF_SIZE: usize = 8;
const PDF_HEADER_SIGNATURE: &[u8] = b"%PDF";
const XREF_KEYWORD: &[u8] = b"xref";
const TRAILER_KEYWORD: &[u8] = b"trailer";
const EOF_MARKER: &[u8] = b"%%EOF";
const STARTXREF_KEYWORD: &[u8] = b"startxref";
const OBJ_KEYWORD: &[u8] = b" obj";
const XREF_TYPE_MARKER: &[u8] = b"/Type /XRef";

// Depth and size limits
const MAX_FORM_FIELD_DEPTH: usize = 64;
const XREF_RECOVERY_SEARCH_RADIUS: u64 = 2048;

#[allow(dead_code)]
pub struct PdfFileParser<R: Read + Seek + BufRead> {
    reader: R,
    mode: ParseMode,
    tolerant: bool,
    max_errors: usize,
    document: PdfDocument,
    object_cache: HashMap<ObjectId, PdfValue>,
    xref_offset: Option<u64>,
    object_offsets: Vec<u64>,
    limits: PerformanceLimits,
    object_load_depth: usize,
}

impl<R: Read + Seek + BufRead> PdfFileParser<R> {
    pub fn new(reader: R, mode: ParseMode, max_errors: usize) -> AstResult<Self> {
        Self::new_with_limits(reader, mode, max_errors, PerformanceLimits::default())
    }

    pub fn new_with_limits(
        mut reader: R,
        mode: ParseMode,
        max_errors: usize,
        limits: PerformanceLimits,
    ) -> AstResult<Self> {
        let tolerant = mode.is_tolerant();
        let version = Self::read_header(&mut reader, tolerant)?;
        let file_size = Self::read_file_size(&mut reader)?;
        let max_file_size = (limits.max_file_size_mb as u64).saturating_mul(1024 * 1024);
        if file_size > max_file_size {
            return Err(AstError::ParseError(format!(
                "File too large: {}MB > {}MB",
                file_size / (1024 * 1024),
                limits.max_file_size_mb
            )));
        }
        limits
            .budget
            .consume_input(file_size)
            .map_err(|err| AstError::ParseError(err.to_string()))?;

        let mut document = PdfDocument::new(version);
        document.budget = limits.budget.clone();
        document.metadata.file_size = Some(file_size);
        if mode.is_forensic() {
            document.forensic = Some(Default::default());
        }

        Ok(PdfFileParser {
            reader,
            mode,
            tolerant,
            max_errors,
            document,
            object_cache: HashMap::new(),
            xref_offset: None,
            object_offsets: Vec::new(),
            limits,
            object_load_depth: 0,
        })
    }

    pub fn parse(mut self) -> AstResult<PdfDocument> {
        self.limits
            .budget
            .check()
            .map_err(|err| AstError::ParseError(err.to_string()))?;
        // Track file size (seek may have moved during checks)
        let _ = self.reader.seek(SeekFrom::Start(0));
        // Check for linearization (must be first object)
        log::debug!("Parsing: checking linearization");
        self.check_linearization()?;

        // Find and parse xref and trailer
        log::debug!("Parsing: locating xref");
        if let Err(err) = self.locate_xref() {
            if self.tolerant {
                self.recover_xref_by_scan()?;
            } else {
                return Err(err);
            }
        } else {
            self.parse_xref_chain()?;
        }
        self.refresh_object_offsets()?;

        // Parse document structure
        log::debug!("Parsing: document structure");
        self.parse_document_structure()?;

        // Resolve all references and build complete AST
        log::debug!("Parsing: resolving references");
        self.resolve_all_references()?;

        // Analyze metadata
        log::debug!("Parsing: metadata analysis");
        self.document.analyze_metadata();

        Ok(self.document)
    }

    fn check_linearization(&mut self) -> AstResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        let mut buffer = vec![0u8; LINEARIZATION_BUFFER_SIZE];
        let n = self.reader.read(&mut buffer)?;
        buffer.truncate(n);

        let pos = Self::skip_pdf_header(&buffer);

        if let Some(linearization) =
            Self::try_parse_linearization_dict_with_budget(&buffer[pos..], &self.limits.budget)
        {
            self.document.set_linearization(linearization);
        }

        Ok(())
    }

    fn skip_pdf_header(buffer: &[u8]) -> usize {
        let mut pos = 0;
        while pos < buffer.len() && buffer[pos] != b'\n' && buffer[pos] != b'\r' {
            pos += 1;
        }
        while pos < buffer.len() && (buffer[pos] == b'\n' || buffer[pos] == b'\r') {
            pos += 1;
        }
        pos
    }

    fn try_parse_linearization_dict(
        data: &[u8],
    ) -> Option<crate::ast::linearization::LinearizationInfo> {
        Self::try_parse_linearization_dict_with_budget(data, &ResourceBudget::default())
    }

    fn try_parse_linearization_dict_with_budget(
        data: &[u8],
        budget: &ResourceBudget,
    ) -> Option<crate::ast::linearization::LinearizationInfo> {
        let (_, (obj_id, value)) =
            object_parser::parse_indirect_object_with_budget(data, budget).ok()?;

        if obj_id != ObjectId::new(1, 0) {
            return None;
        }

        let dict = match value {
            PdfValue::Dictionary(d) => d,
            _ => return None,
        };

        if !dict.contains_key("Linearized") {
            return None;
        }

        let stream = PdfStream::new(dict, Vec::new());
        let linearization = crate::parser::xref::parse_linearization_dict(&stream).ok()?;
        linearization.validate().ok()?;
        Some(linearization)
    }

    fn read_header(reader: &mut R, tolerant: bool) -> AstResult<PdfVersion> {
        reader.seek(SeekFrom::Start(0))?;
        let mut buffer = [0u8; HEADER_BUFFER_SIZE];
        let n = reader.read(&mut buffer)?;

        if n < MIN_PDF_SIZE {
            return Self::handle_small_file(tolerant);
        }

        if let Ok((_, (major, minor))) = pdf_header(&buffer[..n]) {
            return Ok(PdfVersion::new(major, minor));
        }

        if tolerant {
            Self::search_header_in_buffer(reader)
        } else {
            Err(AstError::ParseError("Invalid PDF header".to_string()))
        }
    }

    fn handle_small_file(tolerant: bool) -> AstResult<PdfVersion> {
        if tolerant {
            Ok(PdfVersion::new(1, 7))
        } else {
            Err(AstError::ParseError(
                "File too small to be a PDF".to_string(),
            ))
        }
    }

    fn search_header_in_buffer(reader: &mut R) -> AstResult<PdfVersion> {
        reader.seek(SeekFrom::Start(0))?;
        let mut search_buffer = [0u8; HEADER_SEARCH_BUFFER_SIZE];
        let search_n = reader.read(&mut search_buffer)?;

        for i in 0..search_n.saturating_sub(MIN_PDF_SIZE) {
            if &search_buffer[i..i + 4] == PDF_HEADER_SIGNATURE {
                if let Ok((_, (major, minor))) = pdf_header(&search_buffer[i..]) {
                    return Ok(PdfVersion::new(major, minor));
                }
            }
        }

        Ok(PdfVersion::new(1, 7))
    }

    fn read_file_size(reader: &mut R) -> AstResult<u64> {
        let current = reader.stream_position()?;
        let end = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(current))?;
        Ok(end)
    }

    fn locate_xref(&mut self) -> AstResult<()> {
        let file_size = self.reader.seek(SeekFrom::End(0))?;
        let read_size = file_size.min(XREF_TAIL_BUFFER_SIZE as u64);
        self.reader.seek(SeekFrom::End(-(read_size as i64)))?;

        let mut buffer = vec![0u8; read_size as usize];
        self.reader.read_exact(&mut buffer)?;

        let eof_pos = Self::rfind_pattern(&buffer, EOF_MARKER)
            .ok_or_else(|| AstError::ParseError("No %%EOF marker found".to_string()))?;

        let startxref_pos = Self::rfind_pattern(&buffer[..eof_pos], STARTXREF_KEYWORD)
            .ok_or_else(|| AstError::ParseError("No startxref found".to_string()))?;

        let xref_data = &buffer[startxref_pos + STARTXREF_KEYWORD.len()..eof_pos];
        let xref_data = Self::skip_whitespace(xref_data);

        if let Ok((_, offset)) = integer(xref_data) {
            self.xref_offset = Some(
                u64::try_from(offset)
                    .map_err(|_| AstError::ParseError("Negative xref offset".to_string()))?,
            );
            log::debug!("Parsing: xref offset {}", offset);
            Ok(())
        } else {
            Err(AstError::ParseError("Invalid xref offset".to_string()))
        }
    }

    fn parse_xref_and_trailer(&mut self) -> AstResult<()> {
        let xref_offset = self
            .xref_offset
            .ok_or_else(|| AstError::ParseError("No xref offset".to_string()))?;

        let buffer = self.read_xref_buffer(xref_offset)?;

        if Self::starts_with_xref_keyword(&buffer) {
            self.parse_xref_table(&buffer)?;
        } else {
            self.parse_xref_stream(&buffer)?;
        }

        Ok(())
    }

    fn starts_with_xref_keyword(buffer: &[u8]) -> bool {
        buffer.len() >= XREF_KEYWORD.len() && &buffer[..XREF_KEYWORD.len()] == XREF_KEYWORD
    }

    fn parse_xref_chain(&mut self) -> AstResult<()> {
        log::debug!("Parsing: parsing xref chain");
        let mut offset = self
            .xref_offset
            .ok_or_else(|| AstError::ParseError("No xref offset".to_string()))?;
        let mut seen = std::collections::HashSet::new();
        let mut revision_number = 0u32;
        let mut aggregated = std::collections::HashMap::new();

        loop {
            if !seen.insert(offset) {
                let message = "Detected cycle in xref /Prev chain";
                if self.tolerant {
                    self.record_anomaly("xref_prev_cycle", message, Some(offset))?;
                    break;
                }
                return Err(AstError::ParseError(message.to_string()));
            }

            let (entries, trailer) = match self.parse_single_xref_at(offset) {
                Ok(result) => result,
                Err(err) => {
                    if self.tolerant {
                        self.record_anomaly(
                            "xref_parse_failed",
                            "Failed to parse xref section; falling back to scan",
                            Some(offset),
                        )?;
                        self.recover_xref_by_scan()?;
                        break;
                    } else {
                        return Err(err);
                    }
                }
            };
            let (added, modified, deleted) = self.compute_revision_deltas(&aggregated, &entries);

            for (obj_id, entry) in &entries {
                if let Some(previous_entry) = aggregated.get(obj_id) {
                    if previous_entry != entry {
                        if let Some(forensic) = self.document.forensic.as_mut() {
                            if !forensic.overwritten_objects.contains(obj_id) {
                                forensic.overwritten_objects.push(*obj_id);
                            }
                        }
                    }
                }
                if !self.document.xref.entries.contains_key(obj_id) {
                    self.document.add_xref_entry(*obj_id, *entry);
                }
                aggregated.entry(*obj_id).or_insert(*entry);
            }

            if revision_number == 0 {
                self.document.set_trailer(trailer.clone());
            }

            self.document.revisions.push(crate::ast::DocumentRevision {
                revision_number,
                xref_offset: offset,
                trailer: trailer.clone(),
                modified_objects: modified,
                added_objects: added,
                deleted_objects: deleted,
            });

            revision_number = revision_number.saturating_add(1);

            match trailer.get("Prev") {
                None => break,
                Some(value) => {
                    let Some(prev) = value.as_integer() else {
                        let message = "Invalid /Prev xref offset type".to_string();
                        if self.tolerant {
                            self.record_anomaly("invalid_prev_type", &message, Some(offset))?;
                            break;
                        }
                        return Err(AstError::ParseError(message));
                    };
                    if prev <= 0 {
                        let message = format!("Invalid /Prev xref offset: {prev}");
                        if self.tolerant {
                            self.record_anomaly("invalid_prev", &message, Some(offset))?;
                            break;
                        }
                        return Err(AstError::ParseError(message));
                    }
                    offset = u64::try_from(prev).map_err(|_| {
                        AstError::ParseError("Negative /Prev xref offset".to_string())
                    })?;
                }
            }
        }

        Ok(())
    }

    fn parse_single_xref_at(
        &mut self,
        offset: u64,
    ) -> AstResult<(
        std::collections::HashMap<ObjectId, XRefEntry>,
        PdfDictionary,
    )> {
        log::debug!("Parsing: parse_single_xref_at offset {}", offset);

        let buffer = self.read_xref_buffer(offset)?;

        let result = if Self::starts_with_xref_keyword(&buffer) {
            self.parse_xref_table_section(&buffer)?
        } else {
            self.try_parse_xref_stream_section(&buffer, offset)?
        };

        let (mut entries, mut trailer, mut parsed) = match result {
            Some((e, t)) => (e, t, true),
            None => (
                std::collections::HashMap::new(),
                PdfDictionary::new(),
                false,
            ),
        };
        let mut recovered = false;

        if !parsed && self.tolerant {
            if let Some((fallback_entries, fallback_trailer)) =
                self.recover_xref_near_offset(offset)?
            {
                entries.extend(fallback_entries);
                trailer = fallback_trailer;
                parsed = true;
                recovered = true;
                self.record_anomaly(
                    "xref_recovered_near_offset",
                    "Recovered xref by scanning near declared offset",
                    Some(offset),
                )?;
            }
        }

        if !parsed {
            return Err(AstError::ParseError(
                "Failed to parse xref section".to_string(),
            ));
        }

        if entries.is_empty() && self.tolerant {
            self.recover_xref_by_scan()?;
        }

        if let Some(forensic) = self.document.forensic.as_mut() {
            let target = if recovered {
                &mut forensic.recovered_xref
            } else {
                &mut forensic.declared_xref
            };
            target.extend(entries.iter().map(|(id, entry)| (*id, *entry)));
        }

        Ok((entries, trailer))
    }

    fn read_xref_buffer(&mut self, offset: u64) -> AstResult<Vec<u8>> {
        let file_size = Self::read_file_size(&mut self.reader)?;
        if offset >= file_size {
            return Err(AstError::ParseError(format!(
                "Xref offset {} is outside the file",
                offset
            )));
        }
        let remaining = file_size - offset;
        if remaining > self.limits.budget.max_input_bytes {
            return Err(AstError::ParseError(format!(
                "Xref data exceeds resource limit of {} bytes",
                self.limits.budget.max_input_bytes
            )));
        }
        self.limits
            .budget
            .consume_memory(remaining)
            .map_err(|error| AstError::ParseError(error.to_string()))?;

        self.reader.seek(SeekFrom::Start(offset))?;
        let mut buffer = Vec::new();
        self.reader
            .by_ref()
            .take(remaining)
            .read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn parse_xref_table_section(
        &mut self,
        buffer: &[u8],
    ) -> AstResult<
        Option<(
            std::collections::HashMap<ObjectId, XRefEntry>,
            PdfDictionary,
        )>,
    > {
        log::debug!("Parsing: detected xref table");

        let (remaining, table_entries) =
            match parse_xref_table_with_budget(buffer, &self.limits.budget) {
                Ok(result) => result,
                Err(nom::Err::Failure(error)) if error.code == nom::error::ErrorKind::TooLarge => {
                    return Err(AstError::ParseError(
                        "XRef table exceeds the shared object budget".to_string(),
                    ));
                }
                Err(_) => return Ok(None),
            };

        let mut entries: std::collections::HashMap<ObjectId, XRefEntry> =
            table_entries.into_iter().collect();

        if !Self::skip_whitespace(remaining).starts_with(TRAILER_KEYWORD) {
            return Ok(None);
        }
        let trailer = match Self::extract_trailer_dict_with_budget(
            remaining,
            self.limits.max_depth,
            &self.limits.budget,
        ) {
            Some(dict) => dict,
            None => return Ok(None),
        };

        if let Some(value) = trailer.get("XRefStm") {
            let Some(xref_stm) = value.as_integer() else {
                let message = "Invalid /XRefStm offset type".to_string();
                if self.tolerant {
                    self.record_anomaly("invalid_xref_stm_type", &message, None)?;
                } else {
                    return Err(AstError::ParseError(message));
                }
                return Ok(Some((entries, trailer)));
            };
            if xref_stm <= 0 {
                let message = format!("Invalid /XRefStm offset: {xref_stm}");
                if self.tolerant {
                    self.record_anomaly("invalid_xref_stm", &message, None)?;
                    return Ok(Some((entries, trailer)));
                }
                return Err(AstError::ParseError(message));
            }
            self.document.xref.hybrid_mode = true;
            let xref_stm = u64::try_from(xref_stm)
                .map_err(|_| AstError::ParseError("Negative /XRefStm offset".to_string()))?;
            let (stream_entries, _) = self.parse_xref_stream_at(xref_stm)?;
            entries.extend(stream_entries);
        }

        Ok(Some((entries, trailer)))
    }

    fn extract_trailer_dict(data: &[u8], max_depth: usize) -> Option<PdfDictionary> {
        Self::extract_trailer_dict_with_budget(data, max_depth, &ResourceBudget::default())
    }

    fn extract_trailer_dict_with_budget(
        data: &[u8],
        max_depth: usize,
        budget: &ResourceBudget,
    ) -> Option<PdfDictionary> {
        let trailer_pos = Self::find_pattern(data, TRAILER_KEYWORD)?;
        let trailer_data = &data[trailer_pos + TRAILER_KEYWORD.len()..];
        let trailer_data = Self::skip_whitespace(trailer_data);

        match object_parser::parse_value_with_max_depth_and_budget(trailer_data, max_depth, budget)
        {
            Ok((_, PdfValue::Dictionary(dict))) => Some(dict),
            _ => None,
        }
    }

    fn try_parse_xref_stream_section(
        &mut self,
        buffer: &[u8],
        offset: u64,
    ) -> AstResult<
        Option<(
            std::collections::HashMap<ObjectId, XRefEntry>,
            PdfDictionary,
        )>,
    > {
        let (obj_id, stream) = match object_parser::parse_indirect_object_with_max_depth_and_budget(
            buffer,
            self.limits.max_depth,
            &self.limits.budget,
        ) {
            Ok((_, (id, PdfValue::Stream(s)))) => (id, s),
            _ => return Ok(None),
        };

        if !Self::is_xref_stream(&stream) {
            return Ok(None);
        }

        log::debug!("Parsing: detected xref stream object {}", obj_id.number);

        let (entries, trailer) = self.parse_xref_stream_at(offset)?;

        self.document.add_xref_stream(crate::ast::XRefStream {
            object_id: obj_id,
            dict: trailer.clone(),
            entries: Vec::new(),
        });

        Ok(Some((entries, trailer)))
    }

    fn is_xref_stream(stream: &PdfStream) -> bool {
        stream
            .dict
            .get("Type")
            .and_then(|v| v.as_name())
            .map(|n| n == "XRef")
            .unwrap_or(false)
    }

    fn parse_xref_stream_at(
        &mut self,
        offset: u64,
    ) -> AstResult<(
        std::collections::HashMap<ObjectId, XRefEntry>,
        PdfDictionary,
    )> {
        let buffer = self.read_xref_buffer(offset)?;

        let (obj_id, stream) = match object_parser::parse_indirect_object_with_max_depth_and_budget(
            &buffer,
            self.limits.max_depth,
            &self.limits.budget,
        ) {
            Ok((_, (id, PdfValue::Stream(s)))) => (id, s),
            _ => return Err(AstError::ParseError("Invalid xref stream".to_string())),
        };

        if !Self::is_xref_stream(&stream) {
            return Err(AstError::ParseError("Not an xref stream".to_string()));
        }

        let entries = self.decode_xref_stream_entries(&stream)?;

        self.document.add_xref_stream(crate::ast::XRefStream {
            object_id: obj_id,
            dict: stream.dict.clone(),
            entries: Vec::new(),
        });

        Ok((entries, stream.dict))
    }

    fn decode_xref_stream_entries(
        &mut self,
        stream: &PdfStream,
    ) -> AstResult<std::collections::HashMap<ObjectId, XRefEntry>> {
        let mut entries = std::collections::HashMap::new();

        let raw_data = match stream.raw_data() {
            Some(data) => data,
            None => return Ok(entries),
        };

        let filters = match stream.get_filters_with_params_checked() {
            Ok(filters) => filters,
            Err(err) if self.tolerant => {
                self.record_diagnostic(
                    None,
                    None,
                    "xref_stream_filter",
                    "continued with empty xref entries",
                    0.9,
                    raw_data.len() as u64,
                    &err,
                )?;
                return Ok(entries);
            }
            Err(err) => {
                return Err(AstError::ParseError(format!(
                    "Invalid xref stream filters: {err}"
                )))
            }
        };
        let decoded = match crate::filters::decode_stream_with_budget(
            raw_data,
            &filters,
            &self.limits.budget,
        ) {
            Ok(data) => data,
            Err(err) => {
                if self.tolerant {
                    self.record_diagnostic(
                        None,
                        None,
                        "xref_stream_decode",
                        "continued with empty xref entries",
                        0.9,
                        raw_data.len() as u64,
                        &err.to_string(),
                    )?;
                    return Ok(entries);
                }
                return Err(AstError::ParseError(format!(
                    "Failed to decode xref stream: {}",
                    err
                )));
            }
        };

        let parsed_entries = self.parse_xref_stream_entries(&decoded, &stream.dict)?;
        for (obj_id, entry) in parsed_entries {
            entries.insert(obj_id, entry);
        }

        Ok(entries)
    }

    fn parse_xref_stream_entries(
        &self,
        data: &[u8],
        dict: &PdfDictionary,
    ) -> AstResult<Vec<(ObjectId, XRefEntry)>> {
        let widths = Self::extract_xref_field_widths(dict)?;
        let index = Self::extract_xref_index_ranges(dict)?;

        let entry_size = widths
            .iter()
            .try_fold(0usize, |total, width| total.checked_add(*width))
            .ok_or_else(|| AstError::ParseError("Xref entry size overflow".to_string()))?;
        if entry_size == 0 {
            return Ok(Vec::new());
        }

        self.parse_xref_entries_from_data(data, &widths, entry_size, &index)
    }

    fn extract_xref_field_widths(dict: &PdfDictionary) -> AstResult<[usize; 3]> {
        let w_array = dict
            .get("W")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AstError::ParseError("Missing W in xref stream".to_string()))?;
        if w_array.len() != 3 {
            return Err(AstError::ParseError(
                "Xref field widths must contain exactly 3 entries".to_string(),
            ));
        }

        let mut widths = [0usize; 3];
        for (i, w) in w_array.iter().enumerate() {
            let width = w
                .as_integer()
                .ok_or_else(|| AstError::ParseError("Invalid xref field width".to_string()))?;
            widths[i] = usize::try_from(width)
                .map_err(|_| AstError::ParseError("Xref field width is invalid".to_string()))?;
            if widths[i] > 8 {
                return Err(AstError::ParseError(
                    "Xref field width cannot exceed 8 bytes".to_string(),
                ));
            }
        }
        Ok(widths)
    }

    fn extract_xref_index_ranges(dict: &PdfDictionary) -> AstResult<Vec<(u32, u32)>> {
        let size = dict
            .get("Size")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| AstError::ParseError("Missing xref Size".to_string()))?;
        let size = u32::try_from(size)
            .map_err(|_| AstError::ParseError("Invalid xref Size".to_string()))?;
        let default_range = || -> AstResult<Vec<(u32, u32)>> { Ok(vec![(0, size)]) };

        let index_array = match dict.get("Index").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return default_range(),
        };

        let mut pairs = Vec::new();
        let mut iter = index_array.iter();
        while let Some(start) = iter.next() {
            let count = iter.next().ok_or_else(|| {
                AstError::ParseError("Xref Index must contain start/count pairs".to_string())
            })?;
            let start = u32::try_from(
                start
                    .as_integer()
                    .ok_or_else(|| AstError::ParseError("Invalid xref Index start".to_string()))?,
            )
            .map_err(|_| AstError::ParseError("Invalid xref Index start".to_string()))?;
            let count = u32::try_from(
                count
                    .as_integer()
                    .ok_or_else(|| AstError::ParseError("Invalid xref Index count".to_string()))?,
            )
            .map_err(|_| AstError::ParseError("Invalid xref Index count".to_string()))?;
            pairs.push((start, count));
        }

        if pairs.is_empty() {
            default_range()
        } else {
            Ok(pairs)
        }
    }

    fn parse_xref_entries_from_data(
        &self,
        data: &[u8],
        widths: &[usize; 3],
        entry_size: usize,
        index: &[(u32, u32)],
    ) -> AstResult<Vec<(ObjectId, XRefEntry)>> {
        let mut entries = Vec::new();
        let mut offset = 0usize;

        for (start, count) in index {
            for i in 0..*count {
                let end = offset.checked_add(entry_size).ok_or_else(|| {
                    AstError::ParseError("Xref entry offset overflow".to_string())
                })?;
                if end > data.len() {
                    return Err(AstError::ParseError(
                        "Xref stream data is truncated".to_string(),
                    ));
                }
                let object_number = start.checked_add(i).ok_or_else(|| {
                    AstError::ParseError("Xref object number overflow".to_string())
                })?;
                let obj_id = ObjectId::new(object_number, 0);
                let entry_data = &data[offset..end];
                let entry = self.parse_xref_stream_entry(entry_data, widths)?;
                entries.push((obj_id, entry));
                offset = end;
            }
        }

        Ok(entries)
    }

    fn compute_revision_deltas(
        &self,
        previous: &std::collections::HashMap<ObjectId, XRefEntry>,
        current: &std::collections::HashMap<ObjectId, XRefEntry>,
    ) -> (Vec<ObjectId>, Vec<ObjectId>, Vec<ObjectId>) {
        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        for (obj_id, entry) in current {
            match entry {
                XRefEntry::Free { .. } => deleted.push(*obj_id),
                _ => {
                    if let Some(prev_entry) = previous.get(obj_id) {
                        if prev_entry != entry {
                            modified.push(*obj_id);
                        }
                    } else {
                        added.push(*obj_id);
                    }
                }
            }
        }

        (added, modified, deleted)
    }

    fn recover_xref_near_offset(
        &mut self,
        offset: u64,
    ) -> AstResult<
        Option<(
            std::collections::HashMap<ObjectId, XRefEntry>,
            PdfDictionary,
        )>,
    > {
        let file_size = Self::read_file_size(&mut self.reader)?;
        let start = offset.saturating_sub(XREF_RECOVERY_SEARCH_RADIUS);
        let end = std::cmp::min(
            offset.saturating_add(XREF_RECOVERY_SEARCH_RADIUS),
            file_size,
        );
        if end <= start {
            return Ok(None);
        }

        let buffer = self.read_recovery_buffer(start, end)?;

        if let Some(result) = self.try_recover_xref_table(&buffer, start, offset)? {
            return Ok(Some(result));
        }

        self.try_recover_xref_stream(&buffer, start, offset)
    }

    fn read_recovery_buffer(&mut self, start: u64, end: u64) -> AstResult<Vec<u8>> {
        let size = end.saturating_sub(start);
        self.limits
            .budget
            .consume_memory(size)
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        self.reader.seek(SeekFrom::Start(start))?;
        let mut buffer = vec![
            0u8;
            usize::try_from(size).map_err(|_| {
                AstError::ParseError("Recovery buffer size does not fit in usize".to_string())
            })?
        ];
        let n = self.reader.read(&mut buffer)?;
        buffer.truncate(n);
        Ok(buffer)
    }

    fn try_recover_xref_table(
        &mut self,
        buffer: &[u8],
        start: u64,
        original_offset: u64,
    ) -> AstResult<
        Option<(
            std::collections::HashMap<ObjectId, XRefEntry>,
            PdfDictionary,
        )>,
    > {
        let pos = match Self::find_pattern(buffer, XREF_KEYWORD) {
            Some(p) => p,
            None => return Ok(None),
        };

        let absolute = start + pos as u64;
        if absolute == original_offset {
            return Ok(None);
        }

        match self.parse_single_xref_at(absolute) {
            Ok((entries, trailer)) => Ok(Some((entries, trailer))),
            Err(_) => Ok(None),
        }
    }

    fn try_recover_xref_stream(
        &mut self,
        buffer: &[u8],
        start: u64,
        original_offset: u64,
    ) -> AstResult<
        Option<(
            std::collections::HashMap<ObjectId, XRefEntry>,
            PdfDictionary,
        )>,
    > {
        for i in 0..buffer.len().saturating_sub(OBJ_KEYWORD.len() + 1) {
            if &buffer[i..i + OBJ_KEYWORD.len()] == OBJ_KEYWORD {
                let absolute = start + i as u64;
                if absolute == original_offset {
                    continue;
                }
                if let Ok((entries, trailer)) = self.parse_xref_stream_at(absolute) {
                    return Ok(Some((entries, trailer)));
                }
            }
        }
        Ok(None)
    }

    fn recover_xref_by_scan(&mut self) -> AstResult<()> {
        let content = self.read_limited_input()?;

        let mut pos = 0;
        let mut count = 0usize;
        let mut covered_end = 0usize;
        while pos < content.len() {
            if let Some(obj_pos) = Self::find_next_object(&content[pos..]) {
                let absolute_pos = pos + obj_pos;
                self.record_forensic_residual(covered_end, absolute_pos);
                if let Ok((_, obj_id)) = Self::parse_object_header(&content[absolute_pos..]) {
                    let object_end = object_parser::parse_indirect_object_with_budget(
                        &content[absolute_pos..],
                        &self.limits.budget,
                    )
                    .ok()
                    .map(|(remaining, _)| content.len() - remaining.len())
                    .unwrap_or_else(|| absolute_pos.saturating_add(1));
                    let entry = XRefEntry::InUse {
                        offset: absolute_pos as u64,
                        generation: obj_id.generation,
                    };
                    if self.document.xref.entries.contains_key(&obj_id) {
                        if let Some(forensic) = self.document.forensic.as_mut() {
                            if !forensic.duplicate_objects.contains(&obj_id) {
                                forensic.duplicate_objects.push(obj_id);
                            }
                        }
                    }
                    self.document.xref.entries.insert(obj_id, entry);
                    if let Some(forensic) = self.document.forensic.as_mut() {
                        forensic.recovered_xref.insert(obj_id, entry);
                    }
                    count += 1;
                    covered_end = covered_end.max(object_end);
                    pos = object_end.max(absolute_pos.saturating_add(1));
                } else {
                    pos = absolute_pos.saturating_add(1);
                }
            } else {
                break;
            }
        }
        self.record_forensic_residual(covered_end, content.len());

        if count == 0 {
            return Err(AstError::ParseError(
                "Failed to recover xref entries".to_string(),
            ));
        }

        self.record_anomaly(
            "xref_recovered_by_scan",
            "Recovered xref entries by scanning for objects",
            None,
        )?;

        Ok(())
    }

    fn record_forensic_residual(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        if let Some(forensic) = self.document.forensic.as_mut() {
            let range = (start as u64, end as u64);
            if let Some((_, previous_end)) = forensic.residual_ranges.last_mut() {
                if *previous_end == range.0 {
                    *previous_end = range.1;
                    return;
                }
            }
            forensic.residual_ranges.push(range);
        }
    }

    fn record_anomaly(&mut self, code: &str, message: &str, offset: Option<u64>) -> AstResult<()> {
        self.record_diagnostic(None, offset, code, "xref recovery", 0.5, 0, message)?;
        let node_id = self.add_to_ast(PdfValue::Null, NodeType::Other)?;
        if let Some(node) = self.document.ast.get_node_mut(node_id) {
            node.metadata.errors.push(crate::ast::node::ParseError {
                code: crate::ast::node::ErrorCode::MalformedStructure,
                message: message.to_string(),
                offset,
                recoverable: true,
            });
            node.metadata
                .properties
                .insert("anomaly_code".to_string(), code.to_string());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_diagnostic(
        &mut self,
        object_id: Option<ObjectId>,
        offset: Option<u64>,
        error_code: &str,
        recovery_action: &str,
        confidence: f32,
        bytes_consumed: u64,
        message: &str,
    ) -> AstResult<()> {
        if self.document.diagnostics.len() >= self.max_errors {
            return Err(AstError::ParseError(format!(
                "Maximum parser diagnostics exceeded: {}",
                self.max_errors
            )));
        }
        self.document.diagnostics.push(crate::ast::ParseDiagnostic {
            object_id,
            offset,
            error_code: error_code.to_string(),
            recovery_action: recovery_action.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
            bytes_consumed,
            message: message.to_string(),
        });
        Ok(())
    }

    fn find_next_object(data: &[u8]) -> Option<usize> {
        for i in 0..data.len().saturating_sub(10) {
            if data[i].is_ascii_digit() {
                let mut j = i;
                while j < data.len() && data[j].is_ascii_digit() {
                    j += 1;
                }
                if j < data.len() && data[j].is_ascii_whitespace() {
                    j += 1;
                    while j < data.len() && data[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < data.len() && data[j].is_ascii_digit() {
                        let start = j;
                        while j < data.len() && data[j].is_ascii_digit() {
                            j += 1;
                        }
                        if j + 4 <= data.len() && &data[j..j + 4] == b" obj" && start > i {
                            return Some(i);
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_object_header(input: &[u8]) -> ParseHeaderResult<'_> {
        let (input, obj_num) = integer(input)?;
        let (input, _) = nom::character::complete::multispace1(input)?;
        let (input, gen_num) = integer(input)?;
        let (input, _) = nom::bytes::complete::tag(" obj")(input)?;
        if obj_num < 0 || gen_num < 0 || obj_num > u32::MAX as i64 || gen_num > u16::MAX as i64 {
            return Err(nom::Err::Failure(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
        Ok((input, ObjectId::new(obj_num as u32, gen_num as u16)))
    }

    fn parse_xref_table(&mut self, data: &[u8]) -> AstResult<()> {
        // Parse xref entries
        let (remaining, entries) = match parse_xref_table_with_budget(data, &self.limits.budget) {
            Ok(result) => result,
            Err(nom::Err::Failure(error)) if error.code == nom::error::ErrorKind::TooLarge => {
                return Err(AstError::ParseError(
                    "XRef table exceeds the shared object budget".to_string(),
                ));
            }
            Err(_) => return Ok(()),
        };
        for (obj_id, entry) in entries {
            self.document.add_xref_entry(obj_id, entry);
        }

        // Find and parse trailer
        let remaining = Self::skip_whitespace(remaining);
        if remaining.starts_with(TRAILER_KEYWORD) {
            let trailer_pos = 0;
            let trailer_data = &remaining[trailer_pos + 7..];
            let trailer_data = Self::skip_whitespace(trailer_data);

            if let Ok((_, PdfValue::Dictionary(dict))) =
                object_parser::parse_value_with_max_depth_and_budget(
                    trailer_data,
                    self.limits.max_depth,
                    &self.limits.budget,
                )
            {
                self.document.set_trailer(dict);
            }
        }

        Ok(())
    }

    fn parse_xref_stream(&mut self, data: &[u8]) -> AstResult<()> {
        // Parse as indirect object
        match object_parser::parse_indirect_object_with_max_depth_and_budget(
            data,
            self.limits.max_depth,
            &self.limits.budget,
        ) {
            Ok((_, (obj_id, value))) => {
                if let PdfValue::Stream(stream) = value {
                    // Decode stream data
                    let filters = match stream.get_filters_with_params_checked() {
                        Ok(filters) => Some(filters),
                        Err(err) if self.tolerant => {
                            self.record_diagnostic(
                                Some(obj_id),
                                self.xref_offset,
                                "xref_stream_filter",
                                "continued with empty xref entries",
                                0.9,
                                stream.raw_data().map_or(0, |data| data.len() as u64),
                                &err,
                            )?;
                            None
                        }
                        Err(err) => {
                            return Err(AstError::ParseError(format!(
                                "Invalid xref stream filters: {err}"
                            )))
                        }
                    };

                    if let (Some(filters), Some(raw_data)) = (filters, stream.raw_data()) {
                        match crate::filters::decode_stream_with_budget(
                            raw_data,
                            &filters,
                            &self.limits.budget,
                        ) {
                            Ok(decoded) => {
                                self.parse_xref_stream_data(&decoded, &stream.dict)?;
                            }
                            Err(err) if self.tolerant => {
                                self.record_diagnostic(
                                    Some(obj_id),
                                    self.xref_offset,
                                    "xref_stream_decode",
                                    "continued with empty xref entries",
                                    0.9,
                                    raw_data.len() as u64,
                                    &err.to_string(),
                                )?;
                            }
                            Err(err) => {
                                return Err(AstError::ParseError(format!(
                                    "Failed to decode xref stream: {}",
                                    err
                                )));
                            }
                        }
                    }

                    // Use stream dictionary as trailer
                    self.document.set_trailer(stream.dict.clone());

                    // Store xref stream
                    self.document.add_xref_stream(crate::ast::XRefStream {
                        object_id: obj_id,
                        dict: stream.dict,
                        entries: Vec::new(),
                    });
                }
            }
            Err(err) => {
                return Err(AstError::ParseError(format!(
                    "Failed to parse xref stream: {:?}",
                    err
                )));
            }
        }

        Ok(())
    }

    fn parse_xref_stream_data(&mut self, data: &[u8], dict: &PdfDictionary) -> AstResult<()> {
        let widths = Self::extract_xref_field_widths(dict)?;

        // Get Index array (object number ranges)
        let index = Self::extract_xref_index_ranges(dict)?;

        let entry_size = widths
            .iter()
            .try_fold(0usize, |total, width| total.checked_add(*width))
            .ok_or_else(|| AstError::ParseError("Xref entry size overflow".to_string()))?;
        for (obj_id, entry) in
            self.parse_xref_entries_from_data(data, &widths, entry_size, &index)?
        {
            self.document.add_xref_entry(obj_id, entry);
        }

        Ok(())
    }

    fn parse_xref_stream_entry(&self, data: &[u8], widths: &[usize; 3]) -> AstResult<XRefEntry> {
        let mut offset = 0;

        let width_total = widths
            .iter()
            .try_fold(0usize, |total, width| total.checked_add(*width))
            .ok_or_else(|| AstError::ParseError("Xref entry size overflow".to_string()))?;
        if data.len() < width_total {
            return Err(AstError::ParseError(
                "Truncated xref stream entry".to_string(),
            ));
        }

        // Field 1: Type
        let entry_type = if widths[0] > 0 {
            let end = offset + widths[0];
            Self::read_integer(&data[offset..end])
        } else {
            1 // Default type
        };
        offset += widths[0];

        // Field 2: Second field
        let field2 = if widths[1] > 0 {
            let end = offset + widths[1];
            Self::read_integer(&data[offset..end])
        } else {
            0
        };
        offset += widths[1];

        // Field 3: Third field
        let field3 = if widths[2] > 0 {
            let end = offset + widths[2];
            Self::read_integer(&data[offset..end])
        } else {
            0
        };

        let entry = match entry_type {
            0 => XRefEntry::Free {
                next_free_object: u32::try_from(field2).map_err(|_| {
                    AstError::ParseError("XRef free-object number overflow".to_string())
                })?,
                generation: u16::try_from(field3)
                    .map_err(|_| AstError::ParseError("XRef generation overflow".to_string()))?,
            },
            1 => XRefEntry::InUse {
                offset: field2,
                generation: u16::try_from(field3)
                    .map_err(|_| AstError::ParseError("XRef generation overflow".to_string()))?,
            },
            2 => XRefEntry::Compressed {
                stream_object: u32::try_from(field2).map_err(|_| {
                    AstError::ParseError("XRef object stream number overflow".to_string())
                })?,
                index: u32::try_from(field3).map_err(|_| {
                    AstError::ParseError("XRef object stream index overflow".to_string())
                })?,
            },
            entry_type => {
                if self.tolerant {
                    XRefEntry::Free {
                        next_free_object: 0,
                        generation: 65535,
                    }
                } else {
                    return Err(AstError::ParseError(format!(
                        "Invalid XRef entry type: {entry_type}"
                    )));
                }
            }
        };

        Ok(entry)
    }

    fn parse_document_structure(&mut self) -> AstResult<()> {
        let root_ref = match self.document.trailer.get("Root").cloned() {
            Some(root_value) => match root_value.as_reference().cloned() {
                Some(root_ref) => Some(root_ref),
                None if self.tolerant => {
                    self.record_diagnostic(
                        None,
                        None,
                        "invalid_root",
                        "skipped_catalog",
                        1.0,
                        0,
                        "Trailer /Root is not an indirect reference",
                    )?;
                    None
                }
                None => {
                    return Err(AstError::ParseError(
                        "Trailer /Root is not an indirect reference".to_string(),
                    ));
                }
            },
            None if self.tolerant => {
                self.record_diagnostic(
                    None,
                    None,
                    "missing_root",
                    "skipped_catalog",
                    1.0,
                    0,
                    "Trailer does not contain a /Root reference",
                )?;
                None
            }
            None => {
                return Err(AstError::ParseError(
                    "Trailer does not contain a /Root reference".to_string(),
                ));
            }
        };

        // Parse catalog
        if let Some(root_ref) = root_ref {
            let catalog_value = self.load_object(&root_ref.id())?;
            let catalog_id = match catalog_value {
                PdfValue::Dictionary(_) => {
                    let catalog_id = self.add_to_ast(catalog_value, NodeType::Catalog)?;
                    self.document.set_catalog(catalog_id);
                    self.document
                        .ast
                        .register_object_node(root_ref.id(), catalog_id);
                    Some(catalog_id)
                }
                _ if self.tolerant => {
                    self.record_diagnostic(
                        Some(root_ref.id()),
                        None,
                        "invalid_catalog",
                        "skipped_catalog",
                        1.0,
                        0,
                        "Trailer /Root does not resolve to a dictionary",
                    )?;
                    None
                }
                _ => {
                    return Err(AstError::ParseError(
                        "Trailer /Root does not resolve to a dictionary".to_string(),
                    ));
                }
            };

            if let Some(catalog_id) = catalog_id {
                // Parse catalog sub-structures
                self.parse_catalog_references(catalog_id)?;
                // Parse page tree
                let pages_ref = if let Some(catalog_node) = self.document.ast.get_node(catalog_id) {
                    catalog_node
                        .as_dict()
                        .and_then(|dict| dict.get("Pages"))
                        .and_then(|v| v.as_reference())
                        .cloned()
                } else {
                    None
                };

                if let Some(pages_ref) = pages_ref {
                    self.parse_page_tree(&pages_ref, catalog_id)?;
                }
            }
        }

        // Parse info dictionary
        if let Some(info_ref) = self
            .document
            .trailer
            .get("Info")
            .and_then(|v| v.as_reference())
            .cloned()
        {
            let info_value = self.load_object(&info_ref.id())?;
            let info_id = self.add_to_ast(info_value, NodeType::Metadata)?;
            self.document.set_info(info_id);
            self.document
                .ast
                .register_object_node(info_ref.id(), info_id);
        }

        // Parse encryption dictionary
        if let Some(encrypt_ref) = self
            .document
            .trailer
            .get("Encrypt")
            .and_then(|v| v.as_reference())
            .cloned()
        {
            let encrypt_value = self.load_object(&encrypt_ref.id())?;
            let encrypt_id = self.add_to_ast(encrypt_value, NodeType::Encrypt)?;
            self.document
                .ast
                .register_object_node(encrypt_ref.id(), encrypt_id);
        }

        Ok(())
    }

    fn parse_catalog_references(&mut self, catalog_id: crate::ast::NodeId) -> AstResult<()> {
        let catalog_dict = match self.get_catalog_dict(catalog_id) {
            Some(dict) => dict,
            None => return Ok(()),
        };

        self.parse_open_action(&catalog_dict, catalog_id)?;

        if let Some(aa_value) = catalog_dict.get("AA") {
            self.parse_additional_actions(aa_value, catalog_id)?;
        }

        self.parse_names_dictionary(&catalog_dict)?;

        if let Some(metadata_value) = catalog_dict.get("Metadata") {
            self.parse_xmp_metadata(metadata_value, catalog_id)?;
        }

        self.parse_acroform(&catalog_dict, catalog_id)?;
        self.parse_dss(&catalog_dict)?;

        Ok(())
    }

    fn get_catalog_dict(&self, catalog_id: crate::ast::NodeId) -> Option<PdfDictionary> {
        self.document.ast.get_node(catalog_id)?.as_dict().cloned()
    }

    fn parse_open_action(
        &mut self,
        catalog_dict: &PdfDictionary,
        catalog_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let open_action = match catalog_dict.get("OpenAction") {
            Some(action) => action,
            None => return Ok(()),
        };

        match open_action {
            PdfValue::Reference(open_action_ref) => {
                let action_value = self.load_object(&open_action_ref.id())?;
                let action_id = self.add_to_ast(action_value, NodeType::Action)?;
                self.add_edge(catalog_id, action_id, crate::ast::EdgeType::Reference)?;
            }
            PdfValue::Dictionary(_) => {
                let action_id = self.add_to_ast(open_action.clone(), NodeType::Action)?;
                self.add_edge(catalog_id, action_id, crate::ast::EdgeType::Child)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn parse_names_dictionary(&mut self, catalog_dict: &PdfDictionary) -> AstResult<()> {
        let names_ref = match catalog_dict.get("Names").and_then(|v| v.as_reference()) {
            Some(r) => r,
            None => return Ok(()),
        };

        let names_value = self.load_object(&names_ref.id())?;
        let names_dict = match &names_value {
            PdfValue::Dictionary(d) => d,
            _ => return Ok(()),
        };

        if let Some(embedded_ref) = names_dict
            .get("EmbeddedFiles")
            .and_then(|v| v.as_reference())
        {
            let embedded_value = self.load_object(&embedded_ref.id())?;
            let _ = self.add_to_ast(embedded_value, NodeType::EmbeddedFile)?;
        }

        if let Some(js_ref) = names_dict.get("JavaScript").and_then(|v| v.as_reference()) {
            let js_value = self.load_object(&js_ref.id())?;
            let _ = self.add_to_ast(js_value, NodeType::JavaScriptAction)?;
        }

        Ok(())
    }

    fn parse_acroform(
        &mut self,
        catalog_dict: &PdfDictionary,
        catalog_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let acroform_value = match catalog_dict.get("AcroForm") {
            Some(v) => v,
            None => return Ok(()),
        };

        let acroform_loaded = match acroform_value {
            PdfValue::Reference(acro_ref) => Some(self.load_object(&acro_ref.id())?),
            PdfValue::Dictionary(_) => Some(acroform_value.clone()),
            _ => None,
        };

        let mut acro_dict = match acroform_loaded {
            Some(PdfValue::Dictionary(d)) => d,
            _ => return Ok(()),
        };

        self.document.metadata.has_forms = true;

        if let Some(xfa_value) = acro_dict.get("XFA").cloned() {
            let resolved = self.resolve_xfa_value(&xfa_value)?;
            acro_dict.insert("XFA", resolved);
        }

        let acro_id =
            self.add_to_ast(PdfValue::Dictionary(acro_dict.clone()), NodeType::AcroForm)?;
        self.add_edge(catalog_id, acro_id, crate::ast::EdgeType::Child)?;

        self.parse_form_fields(&acro_dict, acro_id)?;

        if let Some(aa_value) = acro_dict.get("AA") {
            self.parse_additional_actions(aa_value, acro_id)?;
        }

        self.process_xfa_document(&acro_dict)?;
        self.update_form_field_stats(&acro_dict);

        Ok(())
    }

    fn process_xfa_document(&mut self, acro_dict: &PdfDictionary) -> AstResult<()> {
        match XfaDocument::from_acroform_with_budget(acro_dict, &self.limits.budget) {
            Ok(xfa_doc) => {
                if !xfa_doc.is_empty() {
                    self.document.metadata.has_xfa = true;
                    self.document.metadata.xfa_packets = xfa_doc.packets.len();
                    let stats = xfa_doc.script_stats();
                    self.document.metadata.has_xfa_scripts = stats.has_scripts;
                    self.document.metadata.xfa_script_nodes = stats.script_nodes;
                    self.document.xfa = Some(xfa_doc);
                }
            }
            Err(err) if self.tolerant => {
                self.record_diagnostic(None, None, "xfa_parse", "skipped_xfa", 0.8, 0, &err)?;
                log::warn!("Failed to parse XFA data (tolerant): {}", err);
            }
            Err(err) => return Err(AstError::ParseError(err)),
        }
        Ok(())
    }

    fn update_form_field_stats(&mut self, acro_dict: &PdfDictionary) {
        let stats = count_fields_in_acroform(acro_dict);
        self.document.metadata.form_field_count = stats.field_count;
        self.document.metadata.has_hybrid_forms =
            has_hybrid_forms(self.document.metadata.has_xfa, acro_dict);
    }

    fn parse_dss(&mut self, catalog_dict: &PdfDictionary) -> AstResult<()> {
        let dss_value = match catalog_dict.get("DSS") {
            Some(v) => v,
            None => return Ok(()),
        };

        let dss_resolved = match dss_value {
            PdfValue::Reference(reference) => Some(self.load_object(&reference.id())?),
            _ => Some(dss_value.clone()),
        };

        if let Some(PdfValue::Dictionary(dss_dict)) = dss_resolved {
            let info = extract_ltv_info(&dss_dict);
            self.document.metadata.has_dss = info.has_dss;
            self.document.metadata.dss_vri_count = info.vri_count;
            self.document.metadata.dss_certs = info.certs_count;
            self.document.metadata.dss_ocsp = info.ocsp_count;
            self.document.metadata.dss_crl = info.crl_count;
            self.document.metadata.dss_timestamps = info.timestamp_count;
        }

        Ok(())
    }

    fn resolve_xfa_value(&mut self, value: &PdfValue) -> AstResult<PdfValue> {
        self.resolve_xfa_value_at(value, 0)
    }

    fn resolve_xfa_value_at(&mut self, value: &PdfValue, depth: usize) -> AstResult<PdfValue> {
        self.limits
            .budget
            .check()
            .map_err(|err| AstError::ParseError(err.to_string()))?;
        if depth > self.limits.max_depth {
            if self.tolerant {
                self.record_diagnostic(
                    None,
                    None,
                    "xfa_value_depth",
                    "returned_null",
                    1.0,
                    0,
                    "Maximum XFA value depth exceeded",
                )?;
                return Ok(PdfValue::Null);
            }
            return Err(AstError::ParseError(format!(
                "Exceeded max XFA value depth: {}",
                self.limits.max_depth
            )));
        }
        match value {
            PdfValue::Reference(reference) => self.load_object(&reference.id()),
            PdfValue::Array(items) => {
                let mut resolved = PdfArray::new();
                for item in items.iter() {
                    resolved.push(self.resolve_xfa_value_at(item, depth + 1)?);
                }
                Ok(PdfValue::Array(resolved))
            }
            _ => Ok(value.clone()),
        }
    }

    fn parse_form_fields(
        &mut self,
        acroform: &PdfDictionary,
        parent_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        if let Some(fields_value) = acroform.get("Fields") {
            self.parse_form_field_value(fields_value, parent_id, 0)?;
        }
        Ok(())
    }

    fn parse_form_field_value(
        &mut self,
        value: &PdfValue,
        parent_id: crate::ast::NodeId,
        depth: usize,
    ) -> AstResult<()> {
        if depth > MAX_FORM_FIELD_DEPTH {
            if !self.tolerant {
                return Err(AstError::ParseError(format!(
                    "Maximum form field depth exceeded: {}",
                    MAX_FORM_FIELD_DEPTH
                )));
            }
            self.record_diagnostic(
                None,
                None,
                "max_form_field_depth",
                "skipped_form_field_branch",
                1.0,
                0,
                "Maximum form field depth exceeded; branch skipped",
            )?;
            return Ok(());
        }

        match value {
            PdfValue::Array(items) => {
                for item in items.iter() {
                    self.parse_form_field_value(item, parent_id, depth + 1)?;
                }
            }
            PdfValue::Reference(reference) => {
                let field_value = self.load_object(&reference.id())?;
                self.parse_form_field_value(&field_value, parent_id, depth + 1)?;
            }
            PdfValue::Dictionary(dict) => {
                let node_id =
                    self.add_to_ast(PdfValue::Dictionary(dict.clone()), NodeType::Field)?;
                self.add_edge(parent_id, node_id, crate::ast::EdgeType::Child)?;

                if let Some(name) = dict.get("T").and_then(|v| v.as_string()) {
                    if let Some(node) = self.document.ast.get_node_mut(node_id) {
                        node.metadata
                            .properties
                            .insert("field_name".to_string(), name.decode_pdf_encoding());
                    }
                }

                if let Some(ft) = dict.get("FT").and_then(|v| v.as_name()) {
                    if let Some(node) = self.document.ast.get_node_mut(node_id) {
                        node.metadata
                            .properties
                            .insert("field_type".to_string(), ft.without_slash().to_string());
                    }
                }

                if let Some(flags) = dict.get("Ff").and_then(|v| v.as_integer()) {
                    if let Some(node) = self.document.ast.get_node_mut(node_id) {
                        node.metadata
                            .properties
                            .insert("field_flags".to_string(), flags.to_string());
                    }
                }

                if let Some(kids) = dict.get("Kids") {
                    self.parse_form_field_value(kids, node_id, depth + 1)?;
                }

                if let Some(action) = dict.get("A") {
                    self.parse_action_value(action, node_id, None)?;
                }
                if let Some(aa_value) = dict.get("AA") {
                    self.parse_additional_actions(aa_value, node_id)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn parse_page_annotations(
        &mut self,
        page_dict: &PdfDictionary,
        page_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let annots = match page_dict.get("Annots") {
            Some(PdfValue::Array(array)) => array,
            _ => return Ok(()),
        };

        for annot in annots.iter() {
            self.process_single_annotation(annot, page_id)?;
        }

        Ok(())
    }

    fn process_single_annotation(
        &mut self,
        annot: &PdfValue,
        page_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let resolved = self.resolve_annotation_value(annot)?;
        let dict = match resolved {
            PdfValue::Dictionary(d) => d,
            _ => return Ok(()),
        };

        let subtype = Self::get_annotation_subtype(&dict);
        let node_type = Self::annotation_subtype_to_node_type(&subtype);

        let annot_id = self.add_to_ast(PdfValue::Dictionary(dict.clone()), node_type)?;
        self.add_edge(page_id, annot_id, crate::ast::EdgeType::Child)?;

        self.set_annotation_subtype_property(annot_id, &subtype);
        self.parse_annotation_actions(&dict, annot_id)?;
        self.process_annotation_by_subtype(&dict, &subtype, annot_id)?;

        Ok(())
    }

    fn get_annotation_subtype(dict: &PdfDictionary) -> String {
        dict.get("Subtype")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash().to_string())
            .unwrap_or_default()
    }

    fn annotation_subtype_to_node_type(subtype: &str) -> NodeType {
        match subtype {
            "RichMedia" => NodeType::RichMedia,
            "3D" => NodeType::ThreeD,
            _ => NodeType::Annotation,
        }
    }

    fn set_annotation_subtype_property(&mut self, annot_id: crate::ast::NodeId, subtype: &str) {
        if let Some(node) = self.document.ast.get_node_mut(annot_id) {
            node.metadata
                .properties
                .insert("annotation_subtype".to_string(), subtype.to_string());
        }
    }

    fn parse_annotation_actions(
        &mut self,
        dict: &PdfDictionary,
        annot_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        if let Some(action) = dict.get("A") {
            self.parse_action_value(action, annot_id, None)?;
        }
        if let Some(aa_value) = dict.get("AA") {
            self.parse_additional_actions(aa_value, annot_id)?;
        }
        Ok(())
    }

    fn process_annotation_by_subtype(
        &mut self,
        dict: &PdfDictionary,
        subtype: &str,
        annot_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        match subtype {
            "RichMedia" => self.process_richmedia_annotation(dict, annot_id)?,
            "3D" => self.process_threed_annotation(dict, annot_id)?,
            "Sound" => self.process_sound_annotation(dict, annot_id),
            "Movie" => self.process_movie_annotation(dict, annot_id),
            _ => {}
        }
        Ok(())
    }

    fn process_richmedia_annotation(
        &mut self,
        dict: &PdfDictionary,
        annot_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let content = self.resolve_dictionary_from_value(dict.get("RichMediaContent"))?;
        let settings = self.resolve_dictionary_from_value(dict.get("RichMediaSettings"))?;
        let info = extract_richmedia_info(dict, content.as_ref(), settings.as_ref());

        self.document.metadata.has_richmedia = true;
        self.document.metadata.richmedia_annotations += 1;
        self.document.metadata.richmedia_assets += info.assets_count;
        self.document.metadata.richmedia_scripts += info.script_count;

        if let Some(node) = self.document.ast.get_node_mut(annot_id) {
            node.metadata.properties.insert(
                "richmedia_assets".to_string(),
                info.assets_count.to_string(),
            );
            node.metadata.properties.insert(
                "richmedia_configurations".to_string(),
                info.configuration_count.to_string(),
            );
            node.metadata.properties.insert(
                "richmedia_scripts".to_string(),
                info.script_count.to_string(),
            );
            if !info.asset_names.is_empty() {
                node.metadata.properties.insert(
                    "richmedia_asset_names".to_string(),
                    info.asset_names.join(","),
                );
            }
        }

        Ok(())
    }

    fn process_threed_annotation(
        &mut self,
        dict: &PdfDictionary,
        annot_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let stream = self.resolve_stream_from_value(dict.get("3DD"))?;
        let info = extract_threed_info(dict, stream.as_ref());

        self.document.metadata.has_3d = true;
        self.document.metadata.threed_annotations += 1;

        match info.format.as_deref() {
            Some("U3D") => self.document.metadata.threed_u3d += 1,
            Some("PRC") => self.document.metadata.threed_prc += 1,
            _ => {}
        }

        if let Some(node) = self.document.ast.get_node_mut(annot_id) {
            if let Some(fmt) = info.format.clone() {
                node.metadata
                    .properties
                    .insert("threed_format".to_string(), fmt);
            }
            node.metadata
                .properties
                .insert("threed_bytes".to_string(), info.byte_len.to_string());
            node.metadata
                .properties
                .insert("threed_has_view".to_string(), info.has_view.to_string());
            node.metadata.properties.insert(
                "threed_has_activation".to_string(),
                info.has_activation.to_string(),
            );
        }

        Ok(())
    }

    fn process_sound_annotation(&mut self, dict: &PdfDictionary, annot_id: crate::ast::NodeId) {
        let info = match extract_audio_info(dict) {
            Some(i) => i,
            None => return,
        };

        self.document.metadata.has_audio = true;
        self.document.metadata.audio_annotations += 1;

        if let Some(node) = self.document.ast.get_node_mut(annot_id) {
            if let Some(fmt) = info.format {
                node.metadata
                    .properties
                    .insert("audio_format".to_string(), fmt);
            }
            if let Some(ch) = info.channels {
                node.metadata
                    .properties
                    .insert("audio_channels".to_string(), ch.to_string());
            }
            if let Some(rate) = info.sample_rate {
                node.metadata
                    .properties
                    .insert("audio_sample_rate".to_string(), rate.to_string());
            }
            if let Some(bits) = info.bits_per_sample {
                node.metadata
                    .properties
                    .insert("audio_bits_per_sample".to_string(), bits.to_string());
            }
            node.metadata
                .properties
                .insert("audio_bytes".to_string(), info.byte_len.to_string());
        }
    }

    fn process_movie_annotation(&mut self, dict: &PdfDictionary, annot_id: crate::ast::NodeId) {
        let info = match extract_video_info(dict) {
            Some(i) => i,
            None => return,
        };

        self.document.metadata.has_video = true;
        self.document.metadata.video_annotations += 1;

        if let Some(node) = self.document.ast.get_node_mut(annot_id) {
            if let Some(fmt) = info.format {
                node.metadata
                    .properties
                    .insert("video_format".to_string(), fmt);
            }
            if let Some(w) = info.width {
                node.metadata
                    .properties
                    .insert("video_width".to_string(), w.to_string());
            }
            if let Some(h) = info.height {
                node.metadata
                    .properties
                    .insert("video_height".to_string(), h.to_string());
            }
            if let Some(d) = info.duration {
                node.metadata
                    .properties
                    .insert("video_duration".to_string(), d.to_string());
            }
            node.metadata
                .properties
                .insert("video_bytes".to_string(), info.byte_len.to_string());
        }
    }

    fn resolve_annotation_value(&mut self, value: &PdfValue) -> AstResult<PdfValue> {
        match value {
            PdfValue::Reference(reference) => self.load_object(&reference.id()),
            _ => Ok(value.clone()),
        }
    }

    fn resolve_dictionary_from_value(
        &mut self,
        value: Option<&PdfValue>,
    ) -> AstResult<Option<PdfDictionary>> {
        let Some(val) = value else { return Ok(None) };
        let resolved = match val {
            PdfValue::Reference(reference) => self.load_object(&reference.id())?,
            _ => val.clone(),
        };
        match resolved {
            PdfValue::Dictionary(dict) => Ok(Some(dict)),
            _ => Ok(None),
        }
    }

    fn resolve_stream_from_value(
        &mut self,
        value: Option<&PdfValue>,
    ) -> AstResult<Option<PdfStream>> {
        let Some(val) = value else { return Ok(None) };
        let resolved = match val {
            PdfValue::Reference(reference) => self.load_object(&reference.id())?,
            _ => val.clone(),
        };
        match resolved {
            PdfValue::Stream(stream) => Ok(Some(stream)),
            _ => Ok(None),
        }
    }

    fn parse_page_tree(
        &mut self,
        pages_ref: &PdfReference,
        parent_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let mut stack = vec![(*pages_ref, parent_id, PdfDictionary::new())];
        let mut visited = std::collections::HashSet::new();

        while let Some((current_ref, current_parent, inherited_resources)) = stack.pop() {
            let obj_id = current_ref.id();
            if !visited.insert(obj_id) {
                if !self.tolerant {
                    return Err(AstError::ParseError(format!(
                        "Cycle detected in page tree at object {} {}",
                        obj_id.number, obj_id.generation
                    )));
                }
                self.record_diagnostic(
                    Some(obj_id),
                    None,
                    "page_tree_cycle",
                    "skipped_cyclic_page_tree",
                    1.0,
                    0,
                    "Cycle detected in page tree; branch skipped",
                )?;
                continue;
            }

            let pages_value = self.load_object(&obj_id)?;
            match pages_value {
                PdfValue::Dictionary(ref pages_dict) => {
                    let own_resources =
                        self.resolve_resource_dictionary(pages_dict.get("Resources"))?;
                    let effective_resources =
                        Self::merge_resource_dictionaries(&inherited_resources, &own_resources);
                    let node_type = if let Some(type_name) = pages_dict.get_type() {
                        match type_name.without_slash() {
                            "Pages" => NodeType::Pages,
                            "Page" => NodeType::Page,
                            _ => NodeType::Unknown,
                        }
                    } else {
                        NodeType::Unknown
                    };

                    if node_type == NodeType::Unknown {
                        let message = format!(
                            "Page tree node {} {} has missing or unknown /Type",
                            obj_id.number, obj_id.generation
                        );
                        if !self.tolerant {
                            return Err(AstError::ParseError(message));
                        }
                        self.record_diagnostic(
                            Some(obj_id),
                            None,
                            "invalid_page_tree_type",
                            "skipped_invalid_page_tree_node",
                            1.0,
                            0,
                            &message,
                        )?;
                        continue;
                    }

                    let is_page = node_type == NodeType::Page;
                    let mut node_value = pages_value.clone();
                    if !inherited_resources.is_empty() {
                        if let PdfValue::Dictionary(node_dict) = &mut node_value {
                            node_dict.insert(
                                "Resources",
                                PdfValue::Dictionary(effective_resources.clone()),
                            );
                        }
                    }
                    let pages_id = self.add_to_ast(node_value, node_type)?;
                    self.document.ast.register_object_node(obj_id, pages_id);
                    self.add_edge(current_parent, pages_id, crate::ast::EdgeType::Child)?;

                    if !inherited_resources.is_empty() {
                        if let Some(node) = self.document.ast.get_node_mut(pages_id) {
                            node.metadata.set_property(
                                "has_inherited_resources".to_string(),
                                "true".to_string(),
                            );
                        }
                    }

                    if let Some(kids_value) = pages_dict.get("Kids") {
                        match kids_value {
                            PdfValue::Array(kids) => {
                                for kid in kids.iter() {
                                    if let Some(kid_ref) = kid.as_reference() {
                                        stack.push((
                                            *kid_ref,
                                            pages_id,
                                            effective_resources.clone(),
                                        ));
                                    } else {
                                        let message = format!(
                                            "Page tree /Kids entry must be a reference, got {}",
                                            kid.type_name()
                                        );
                                        if !self.tolerant {
                                            return Err(AstError::ParseError(message));
                                        }
                                        self.record_diagnostic(
                                            Some(obj_id),
                                            None,
                                            "invalid_page_tree_kid",
                                            "skipped_invalid_page_tree_kid",
                                            1.0,
                                            0,
                                            &message,
                                        )?;
                                    }
                                }
                            }
                            invalid => {
                                let message = format!(
                                    "Page tree /Kids must be an array, got {}",
                                    invalid.type_name()
                                );
                                if !self.tolerant {
                                    return Err(AstError::ParseError(message));
                                }
                                self.record_diagnostic(
                                    Some(obj_id),
                                    None,
                                    "invalid_page_tree_kids",
                                    "skipped_invalid_page_tree_kids",
                                    1.0,
                                    0,
                                    &message,
                                )?;
                            }
                        }
                    }

                    if is_page {
                        if let Some(aa_value) = pages_dict.get("AA") {
                            self.parse_additional_actions(aa_value, pages_id)?;
                        }
                        self.parse_page_annotations(pages_dict, pages_id)?;
                    }
                }
                invalid => {
                    let message = format!(
                        "Page tree node must be a dictionary, got {}",
                        invalid.type_name()
                    );
                    if !self.tolerant {
                        return Err(AstError::ParseError(message));
                    }
                    self.record_diagnostic(
                        Some(obj_id),
                        None,
                        "invalid_page_tree_node",
                        "skipped_invalid_page_tree_node",
                        1.0,
                        0,
                        &message,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn resolve_resource_dictionary(
        &mut self,
        value: Option<&PdfValue>,
    ) -> AstResult<PdfDictionary> {
        let Some(value) = value else {
            return Ok(PdfDictionary::new());
        };
        let resolved = match value {
            PdfValue::Reference(reference) => self.load_object(&reference.id())?,
            value => value.clone(),
        };
        match resolved {
            PdfValue::Dictionary(dict) => Ok(dict),
            invalid => {
                let message = format!(
                    "Page Resources must be a dictionary, got {}",
                    invalid.type_name()
                );
                if !self.tolerant {
                    return Err(AstError::ParseError(message));
                }
                self.record_diagnostic(
                    None,
                    None,
                    "invalid_resources",
                    "ignored_invalid_resources",
                    1.0,
                    0,
                    &message,
                )?;
                Ok(PdfDictionary::new())
            }
        }
    }

    fn merge_resource_dictionaries(parent: &PdfDictionary, child: &PdfDictionary) -> PdfDictionary {
        const CATEGORIES: &[&str] = &[
            "ColorSpace",
            "ExtGState",
            "Font",
            "Pattern",
            "Properties",
            "Shading",
            "XObject",
        ];

        let mut merged = parent.clone();
        for (name, value) in child.iter() {
            if CATEGORIES.contains(&name.as_str()) {
                let mut category = parent
                    .get(name.as_str())
                    .and_then(PdfValue::as_dict)
                    .cloned()
                    .unwrap_or_default();
                if let Some(child_category) = value.as_dict() {
                    for (resource_name, resource_value) in child_category {
                        category.insert(resource_name.clone(), resource_value.clone());
                    }
                    merged.insert(name.clone(), PdfValue::Dictionary(category));
                } else {
                    merged.insert(name.clone(), value.clone());
                }
            } else {
                merged.insert(name.clone(), value.clone());
            }
        }
        merged
    }

    fn load_object(&mut self, obj_id: &ObjectId) -> AstResult<PdfValue> {
        if self.limits.enable_recursion_checks {
            self.object_load_depth += 1;
            if self.object_load_depth > self.limits.max_depth {
                self.object_load_depth -= 1;
                if self.tolerant {
                    self.record_diagnostic(
                        Some(*obj_id),
                        None,
                        "max_depth",
                        "returned Null",
                        1.0,
                        0,
                        "Maximum object resolution depth exceeded",
                    )?;
                    return Ok(PdfValue::Null);
                }
                return Err(AstError::ParseError(format!(
                    "Exceeded max object load depth: {}",
                    self.limits.max_depth
                )));
            }
        }

        let result = (|| {
            self.limits
                .budget
                .check()
                .map_err(|err| AstError::ParseError(err.to_string()))?;
            // Check cache
            if let Some(cached) = self.object_cache.get(obj_id).cloned() {
                return Ok(cached);
            }
            // Get object location from xref
            let entry = match self.document.xref.entries.get(obj_id).copied() {
                Some(entry) => entry,
                None => {
                    if !self.tolerant {
                        return Err(AstError::ParseError(format!(
                            "Missing xref entry for object {} {}",
                            obj_id.number, obj_id.generation
                        )));
                    }
                    self.record_diagnostic(
                        Some(*obj_id),
                        None,
                        "missing_xref",
                        "returned Null",
                        1.0,
                        0,
                        "No cross-reference entry exists for the requested object",
                    )?;
                    return Ok(PdfValue::Null);
                }
            };

            let value = match entry {
                XRefEntry::InUse { offset, .. } => {
                    let buffer = self.read_object_buffer(offset)?;

                    let parsed =
                        match object_parser::parse_indirect_stream_prefix_with_max_depth_and_budget(
                            &buffer,
                            self.limits.max_depth,
                            &self.limits.budget,
                        ) {
                            Ok((_, (_, dict))) => {
                                if let Some(PdfValue::Reference(length_ref)) = dict.get("Length") {
                                    match self.load_object(&length_ref.id()) {
                                    Ok(PdfValue::Integer(length)) if length >= 0 => {
                                        match usize::try_from(length) {
                                            Ok(length) => {
                                                object_parser::parse_indirect_object_with_stream_length_and_max_depth_and_budget(
                                                    &buffer,
                                                    length,
                                                    self.limits.max_depth,
                                                    &self.limits.budget,
                                                )
                                            }
                                            Err(_) if self.tolerant => {
                                                object_parser::parse_indirect_object_with_max_depth_and_budget(
                                                    &buffer,
                                                    self.limits.max_depth,
                                                    &self.limits.budget,
                                                )
                                            }
                                            Err(_) => {
                                                return Err(AstError::ParseError(
                                                    "Indirect stream Length is too large".to_string(),
                                                ));
                                            }
                                        }
                                    }
                                    Ok(_) if self.tolerant => {
                                        object_parser::parse_indirect_object_with_max_depth_and_budget(
                                            &buffer,
                                            self.limits.max_depth,
                                            &self.limits.budget,
                                        )
                                    }
                                    Ok(_) => {
                                        return Err(AstError::ParseError(
                                            "Indirect stream Length is not an integer".to_string(),
                                        ));
                                    }
                                    Err(err) if self.tolerant => {
                                        log::warn!(
                                            "Failed to resolve indirect stream Length for object {}: {}",
                                            obj_id.number,
                                            err
                                        );
                                        object_parser::parse_indirect_object_with_max_depth_and_budget(
                                            &buffer,
                                            self.limits.max_depth,
                                            &self.limits.budget,
                                        )
                                    }
                                    Err(err) => return Err(err),
                                }
                                } else {
                                    object_parser::parse_indirect_object_with_max_depth_and_budget(
                                        &buffer,
                                        self.limits.max_depth,
                                        &self.limits.budget,
                                    )
                                }
                            }
                            Err(_) => {
                                object_parser::parse_indirect_object_with_max_depth_and_budget(
                                    &buffer,
                                    self.limits.max_depth,
                                    &self.limits.budget,
                                )
                            }
                        };

                    match parsed {
                        Ok((_, (parsed_id, mut value))) => {
                            if parsed_id == *obj_id {
                                if let PdfValue::Stream(stream) = &mut value {
                                    let result = self.resolve_indirect_stream_length(stream);
                                    if let Err(err) = result {
                                        if !self.tolerant {
                                            return Err(err);
                                        }
                                        self.record_diagnostic(
                                            Some(*obj_id),
                                            Some(offset),
                                            "stream_length",
                                            "kept observed stream bytes",
                                            0.8,
                                            stream.data.len() as u64,
                                            &err.to_string(),
                                        )?;
                                        log::warn!(
                                            "Failed to resolve stream length for object {}: {}",
                                            obj_id.number,
                                            err
                                        );
                                    }
                                }
                                value
                            } else if self.tolerant {
                                self.record_diagnostic(
                                    Some(*obj_id),
                                    Some(offset),
                                    "object_id_mismatch",
                                    "returned Null",
                                    1.0,
                                    buffer.len() as u64,
                                    &format!(
                                        "Object ID mismatch: expected {} {}, got {} {}",
                                        obj_id.number,
                                        obj_id.generation,
                                        parsed_id.number,
                                        parsed_id.generation
                                    ),
                                )?;
                                PdfValue::Null
                            } else {
                                return Err(AstError::ParseError(format!(
                                    "Object ID mismatch: expected {} {}, got {} {}",
                                    obj_id.number,
                                    obj_id.generation,
                                    parsed_id.number,
                                    parsed_id.generation
                                )));
                            }
                        }
                        Err(err) if self.tolerant => {
                            self.record_diagnostic(
                                Some(*obj_id),
                                Some(offset),
                                "object_parse",
                                "returned Null",
                                1.0,
                                buffer.len() as u64,
                                &format!("Failed to parse object: {:?}", err),
                            )?;
                            log::warn!("Failed to parse object {}: {:?}", obj_id.number, err);
                            PdfValue::Null
                        }
                        Err(err) => {
                            return Err(AstError::ParseError(format!(
                                "Failed to parse object {} {}: {:?}",
                                obj_id.number, obj_id.generation, err
                            )))
                        }
                    }
                }
                XRefEntry::Compressed {
                    stream_object,
                    index,
                } => {
                    // Load from object stream
                    self.load_from_object_stream(stream_object, index)?
                }
                XRefEntry::Free { .. } => PdfValue::Null,
            };

            self.object_cache.insert(*obj_id, value.clone());
            Ok(value)
        })();

        if self.limits.enable_recursion_checks {
            self.object_load_depth = self.object_load_depth.saturating_sub(1);
        }

        result
    }

    fn read_object_buffer(&mut self, offset: u64) -> AstResult<Vec<u8>> {
        let file_size = Self::read_file_size(&mut self.reader)?;
        if offset >= file_size {
            return Err(AstError::ParseError(format!(
                "Object offset {} is outside the file",
                offset
            )));
        }

        let max_bytes = self
            .limits
            .max_object_size_mb
            .checked_mul(1024 * 1024)
            .ok_or_else(|| AstError::ParseError("Object size limit overflow".to_string()))?
            as u64;
        if self.object_offsets.is_empty() {
            self.refresh_object_offsets()?;
        }
        let next_offset = self
            .object_offsets
            .get(
                self.object_offsets
                    .partition_point(|candidate| *candidate <= offset),
            )
            .copied()
            .unwrap_or(file_size);
        let bound = next_offset
            .saturating_sub(offset)
            .min(file_size.saturating_sub(offset))
            .min(max_bytes);
        self.limits
            .budget
            .consume_decoded(bound)
            .map_err(|error| AstError::ParseError(error.to_string()))?;

        self.reader.seek(SeekFrom::Start(offset))?;
        let mut buffer = Vec::new();
        self.reader.by_ref().take(bound).read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn refresh_object_offsets(&mut self) -> AstResult<()> {
        let mut offsets: Vec<u64> = self
            .document
            .xref
            .entries
            .values()
            .filter_map(|entry| match entry {
                XRefEntry::InUse { offset, .. } => Some(*offset),
                _ => None,
            })
            .collect();
        offsets.sort_unstable();
        offsets.dedup();
        let bytes = offsets
            .len()
            .checked_mul(std::mem::size_of::<u64>())
            .ok_or_else(|| AstError::ParseError("Object offset index is too large".to_string()))?;
        self.limits
            .budget
            .consume_memory(bytes as u64)
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        self.object_offsets = offsets;
        Ok(())
    }

    fn read_limited_input(&mut self) -> AstResult<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(0))?;
        let max_bytes = self.limits.budget.max_input_bytes;
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        let mut limited = self.reader.by_ref().take(max_bytes.saturating_add(1));
        loop {
            let bytes_read = limited.read(&mut chunk)?;
            if bytes_read == 0 {
                break;
            }
            self.limits
                .budget
                .consume_decoded(bytes_read as u64)
                .map_err(|error| AstError::ParseError(error.to_string()))?;
            buffer.extend_from_slice(&chunk[..bytes_read]);
        }
        if buffer.len() as u64 > max_bytes {
            return Err(AstError::ParseError(format!(
                "Input exceeds resource limit of {} bytes",
                max_bytes
            )));
        }
        Ok(buffer)
    }

    fn resolve_indirect_stream_length(&mut self, stream: &mut PdfStream) -> AstResult<()> {
        let length_id = match stream.dict.get("Length") {
            Some(PdfValue::Reference(reference)) => reference.id(),
            _ => return Ok(()),
        };
        let length = match self.load_object(&length_id)? {
            PdfValue::Integer(length) if length >= 0 => usize::try_from(length)
                .map_err(|_| AstError::ParseError("Stream length is too large".to_string()))?,
            _ => {
                return Err(AstError::ParseError(
                    "Indirect stream Length is not a non-negative integer".to_string(),
                ))
            }
        };
        if length > stream.data.len() {
            return Err(AstError::ParseError(format!(
                "Declared stream length {} exceeds observed data {}",
                length,
                stream.data.len()
            )));
        }
        Ok(())
    }

    fn load_from_object_stream(&mut self, stream_obj: u32, index: u32) -> AstResult<PdfValue> {
        let stream_id = ObjectId::new(stream_obj, 0);
        let stream_value = self.load_object(&stream_id)?;

        if let PdfValue::Stream(stream) = stream_value {
            // Decode stream
            let filters = match stream.get_filters_with_params_checked() {
                Ok(filters) => filters,
                Err(err) if self.tolerant => {
                    self.record_diagnostic(
                        Some(ObjectId::new(stream_obj, 0)),
                        None,
                        "object_stream_filter",
                        "returned Null",
                        1.0,
                        stream.raw_data().map_or(0, |data| data.len() as u64),
                        &err,
                    )?;
                    return Ok(PdfValue::Null);
                }
                Err(err) => {
                    return Err(AstError::ParseError(format!(
                        "Invalid object stream filters: {err}"
                    )))
                }
            };
            if let Some(raw_data) = stream.raw_data() {
                match crate::filters::decode_stream_with_budget(
                    raw_data,
                    &filters,
                    &self.limits.budget,
                ) {
                    Ok(decoded) => {
                        return match self.parse_object_from_stream(&decoded, index, &stream.dict) {
                            Ok(value) => Ok(value),
                            Err(err) if self.tolerant => {
                                self.record_diagnostic(
                                    Some(ObjectId::new(stream_obj, 0)),
                                    None,
                                    "object_stream_parse",
                                    "returned Null",
                                    1.0,
                                    decoded.len() as u64,
                                    &err.to_string(),
                                )?;
                                Ok(PdfValue::Null)
                            }
                            Err(err) => Err(err),
                        };
                    }
                    Err(err) if !self.tolerant => {
                        return Err(AstError::ParseError(format!(
                            "Failed to decode object stream: {}",
                            err
                        )));
                    }
                    Err(err) => {
                        self.record_diagnostic(
                            Some(ObjectId::new(stream_obj, 0)),
                            None,
                            "object_stream_decode",
                            "returned Null",
                            1.0,
                            0,
                            &err.to_string(),
                        )?;
                    }
                }
            } else if !self.tolerant {
                return Err(AstError::ParseError(
                    "Object stream has no raw data".to_string(),
                ));
            } else {
                self.record_diagnostic(
                    Some(ObjectId::new(stream_obj, 0)),
                    None,
                    "object_stream_data",
                    "returned Null",
                    1.0,
                    0,
                    "Object stream has no raw data",
                )?;
            }
        } else if !self.tolerant {
            return Err(AstError::ParseError(
                "Xref compressed entry is not an object stream".to_string(),
            ));
        } else {
            self.record_diagnostic(
                Some(ObjectId::new(stream_obj, 0)),
                None,
                "object_stream_type",
                "returned Null",
                1.0,
                0,
                "Compressed xref entry does not reference an object stream",
            )?;
        }

        Ok(PdfValue::Null)
    }

    fn parse_object_from_stream(
        &self,
        data: &[u8],
        index: u32,
        dict: &PdfDictionary,
    ) -> AstResult<PdfValue> {
        // Get N (number of objects) and First (offset to first object)
        let n = dict
            .get("N")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| AstError::ParseError("Missing N in object stream".to_string()))
            .and_then(|n| {
                usize::try_from(n)
                    .map_err(|_| AstError::ParseError("Invalid N in object stream".to_string()))
            })?;
        let first = dict
            .get("First")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| AstError::ParseError("Missing First in object stream".to_string()))
            .and_then(|first| {
                usize::try_from(first)
                    .map_err(|_| AstError::ParseError("Invalid First in object stream".to_string()))
            })?;

        let index = usize::try_from(index)
            .map_err(|_| AstError::ParseError("Invalid object stream index".to_string()))?;
        if index >= n {
            if !self.tolerant {
                return Err(AstError::ParseError(
                    "Object stream index out of range".to_string(),
                ));
            }
            return Ok(PdfValue::Null);
        }

        let offsets = object_parser::parse_object_stream_offsets_with_budget(
            data,
            n,
            first,
            &self.limits.budget,
        )
        .map_err(AstError::ParseError)?;

        // Find the object at the requested index
        let obj_offset = offsets[index];
        let next_offset = offsets
            .iter()
            .copied()
            .filter(|candidate| *candidate > obj_offset)
            .min()
            .unwrap_or(data.len());
        let obj_data = data
            .get(obj_offset..next_offset)
            .ok_or_else(|| AstError::ParseError("Invalid object stream offsets".to_string()))?;
        match object_parser::parse_value_with_max_depth_and_budget(
            obj_data,
            self.limits.max_depth,
            &self.limits.budget,
        ) {
            Ok((remaining, value)) => {
                let remaining = crate::parser::lexer::skip_whitespace_and_comments(remaining)
                    .map(|(remaining, _)| remaining)
                    .unwrap_or(remaining);
                if !self.tolerant && !remaining.is_empty() {
                    return Err(AstError::ParseError(
                        "Residual bytes after compressed object".to_string(),
                    ));
                }
                Ok(value)
            }
            Err(err) if self.tolerant => {
                log::warn!("Failed to parse compressed object: {:?}", err);
                Ok(PdfValue::Null)
            }
            Err(err) => Err(AstError::ParseError(format!(
                "Failed to parse compressed object: {:?}",
                err
            ))),
        }
    }

    fn add_to_ast(
        &mut self,
        value: PdfValue,
        node_type: NodeType,
    ) -> AstResult<crate::ast::NodeId> {
        self.limits
            .budget
            .consume_node()
            .map_err(|err| AstError::ParseError(err.to_string()))?;
        // Auto-detect more specific node types based on the value
        let refined_node_type = self.refine_node_type(&value, node_type);
        let node_id = self.document.ast.create_node(refined_node_type, value);
        Ok(node_id)
    }

    fn add_edge(
        &mut self,
        from: crate::ast::NodeId,
        to: crate::ast::NodeId,
        edge_type: crate::ast::EdgeType,
    ) -> AstResult<()> {
        self.limits
            .budget
            .consume_edge()
            .map_err(|err| AstError::ParseError(err.to_string()))?;
        if self.document.ast.add_edge(from, to, edge_type) {
            Ok(())
        } else {
            Err(AstError::ParseError(
                "Cannot add AST edge: node endpoint is missing".to_string(),
            ))
        }
    }

    fn parse_xmp_metadata(
        &mut self,
        metadata_value: &PdfValue,
        catalog_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let stream = match self.resolve_metadata_stream(metadata_value)? {
            Some(s) => s,
            None => return Ok(()),
        };

        let metadata_id = self.create_xmp_stream_node(&stream, catalog_id)?;

        let decoded = match self.decode_xmp_stream(&stream) {
            Ok(data) => data,
            Err(err) if self.tolerant => {
                self.record_diagnostic(None, None, "xmp_decode", "skipped_xmp", 0.8, 0, &err)?;
                return Ok(());
            }
            Err(err) => return Err(AstError::ParseError(err)),
        };

        let xmp = match XmpMetadata::parse_from_stream_with_budget(&decoded, &self.limits.budget) {
            Ok(metadata) => metadata,
            Err(err) if self.tolerant => {
                self.record_diagnostic(
                    None,
                    None,
                    "xmp_parse",
                    "skipped_xmp",
                    0.8,
                    decoded.len() as u64,
                    &err,
                )?;
                return Ok(());
            }
            Err(err) => return Err(AstError::ParseError(err)),
        };

        let packet_id = self.create_xmp_packet_node(&xmp, metadata_id)?;
        self.create_xmp_namespace_nodes(&xmp, packet_id)?;

        let namespace_missing = self.create_xmp_property_nodes(&xmp, packet_id)?;
        self.add_namespace_warning_if_needed(packet_id, namespace_missing);

        self.check_xmp_info_coherence(&xmp, packet_id);
        Ok(())
    }

    fn resolve_metadata_stream(
        &mut self,
        metadata_value: &PdfValue,
    ) -> AstResult<Option<PdfStream>> {
        let resolved = match metadata_value {
            PdfValue::Reference(reference) => self.load_object(&reference.id())?,
            _ => metadata_value.clone(),
        };

        match resolved {
            PdfValue::Stream(stream) => Ok(Some(stream)),
            _ => Ok(None),
        }
    }

    fn create_xmp_stream_node(
        &mut self,
        stream: &PdfStream,
        catalog_id: crate::ast::NodeId,
    ) -> AstResult<crate::ast::NodeId> {
        let metadata_id = self.add_to_ast(PdfValue::Stream(stream.clone()), NodeType::Metadata)?;
        self.add_edge(catalog_id, metadata_id, crate::ast::EdgeType::Child)?;

        if let Some(node) = self.document.ast.get_node_mut(metadata_id) {
            node.metadata
                .properties
                .insert("metadata_kind".to_string(), "xmp_stream".to_string());
        }

        Ok(metadata_id)
    }

    fn decode_xmp_stream(&self, stream: &PdfStream) -> Result<Vec<u8>, String> {
        stream.decode_with_budget(&self.limits.budget)
    }

    fn create_xmp_packet_node(
        &mut self,
        xmp: &XmpMetadata,
        metadata_id: crate::ast::NodeId,
    ) -> AstResult<crate::ast::NodeId> {
        let packet_id = self.add_to_ast(PdfValue::Null, NodeType::Metadata)?;
        self.add_edge(metadata_id, packet_id, crate::ast::EdgeType::Child)?;

        if let Some(node) = self.document.ast.get_node_mut(packet_id) {
            node.metadata
                .properties
                .insert("metadata_kind".to_string(), "xmp_packet".to_string());
            node.metadata
                .properties
                .insert("xmp_raw_length".to_string(), xmp.raw_xml.len().to_string());
        }

        Ok(packet_id)
    }

    fn create_xmp_namespace_nodes(
        &mut self,
        xmp: &XmpMetadata,
        packet_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        for (prefix, uri) in &xmp.namespaces {
            let ns_id = self.add_to_ast(PdfValue::Null, NodeType::Metadata)?;
            self.add_edge(packet_id, ns_id, crate::ast::EdgeType::Child)?;

            if let Some(node) = self.document.ast.get_node_mut(ns_id) {
                node.metadata
                    .properties
                    .insert("metadata_kind".to_string(), "xmp_namespace".to_string());
                node.metadata
                    .properties
                    .insert("xmp_namespace_prefix".to_string(), prefix.clone());
                node.metadata
                    .properties
                    .insert("xmp_namespace_uri".to_string(), uri.clone());
            }
        }

        Ok(())
    }

    fn create_xmp_property_nodes(
        &mut self,
        xmp: &XmpMetadata,
        packet_id: crate::ast::NodeId,
    ) -> AstResult<usize> {
        let mut namespace_missing = 0usize;

        for (key, value) in &xmp.properties {
            let prop_id = self.add_to_ast(PdfValue::Null, NodeType::Metadata)?;
            self.add_edge(packet_id, prop_id, crate::ast::EdgeType::Child)?;

            let (prefix, name) = Self::split_xmp_property_key(key);
            if !prefix.is_empty() && !xmp.namespaces.contains_key(&prefix) {
                namespace_missing += 1;
            }

            self.set_xmp_property_metadata(prop_id, key, value, &prefix, &name);
        }

        Ok(namespace_missing)
    }

    fn split_xmp_property_key(key: &str) -> (String, String) {
        match key.split_once(':') {
            Some((p, n)) => (p.to_string(), n.to_string()),
            None => (String::new(), key.to_string()),
        }
    }

    fn set_xmp_property_metadata(
        &mut self,
        prop_id: crate::ast::NodeId,
        key: &str,
        value: &str,
        prefix: &str,
        name: &str,
    ) {
        if let Some(node) = self.document.ast.get_node_mut(prop_id) {
            node.metadata
                .properties
                .insert("metadata_kind".to_string(), "xmp_property".to_string());
            node.metadata
                .properties
                .insert("xmp_key".to_string(), key.to_string());
            node.metadata
                .properties
                .insert("xmp_value".to_string(), value.to_string());

            if !prefix.is_empty() {
                node.metadata
                    .properties
                    .insert("xmp_namespace".to_string(), prefix.to_string());
                node.metadata
                    .properties
                    .insert("xmp_property".to_string(), name.to_string());
            }
        }
    }

    fn add_namespace_warning_if_needed(&mut self, packet_id: crate::ast::NodeId, count: usize) {
        if count > 0 {
            if let Some(node) = self.document.ast.get_node_mut(packet_id) {
                node.metadata
                    .warnings
                    .push(format!("XMP properties with missing namespaces: {}", count));
            }
        }
    }

    fn check_xmp_info_coherence(&mut self, xmp: &XmpMetadata, packet_id: crate::ast::NodeId) {
        let info = self.document.get_info();
        let mut mismatches = Vec::new();

        let compare = |label: &str, info_val: Option<String>, xmp_val: Option<&String>| match (
            info_val, xmp_val,
        ) {
            (Some(i), Some(x)) if i != *x => Some(label.to_string()),
            _ => None,
        };

        if let Some(info_dict) = info {
            let info_title = info_dict
                .get("Title")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_author = info_dict
                .get("Author")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_subject = info_dict
                .get("Subject")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_keywords = info_dict
                .get("Keywords")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_creator = info_dict
                .get("Creator")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_producer = info_dict
                .get("Producer")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_creation = info_dict
                .get("CreationDate")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());
            let info_mod = info_dict
                .get("ModDate")
                .and_then(|v| v.as_string())
                .map(|s| s.decode_pdf_encoding());

            if let Some(label) = compare("Title", info_title, xmp.title()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("Author", info_author, xmp.author()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("Subject", info_subject, xmp.subject()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("Keywords", info_keywords, xmp.keywords()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("Creator", info_creator, xmp.creator()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("Producer", info_producer, xmp.producer()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("CreationDate", info_creation, xmp.creation_date()) {
                mismatches.push(label);
            }
            if let Some(label) = compare("ModDate", info_mod, xmp.modification_date()) {
                mismatches.push(label);
            }
        }

        if !mismatches.is_empty() {
            if let Some(node) = self.document.ast.get_node_mut(packet_id) {
                node.metadata
                    .warnings
                    .push(format!("XMP/Info mismatch: {}", mismatches.join(", ")));
            }
        }
    }

    fn parse_additional_actions(
        &mut self,
        aa_value: &PdfValue,
        parent_id: crate::ast::NodeId,
    ) -> AstResult<()> {
        let resolved = match aa_value {
            PdfValue::Reference(reference) => self.load_object(&reference.id())?,
            _ => aa_value.clone(),
        };

        if let PdfValue::Dictionary(dict) = resolved {
            for (event, action) in dict.iter() {
                self.parse_action_value(action, parent_id, Some(event.to_string()))?;
            }
        }

        Ok(())
    }

    fn parse_action_value(
        &mut self,
        value: &PdfValue,
        parent_id: crate::ast::NodeId,
        event: Option<String>,
    ) -> AstResult<()> {
        match value {
            PdfValue::Reference(reference) => {
                let action_value = self.load_object(&reference.id())?;
                self.parse_action_value(&action_value, parent_id, event)?;
            }
            PdfValue::Array(items) => {
                for item in items.iter() {
                    self.parse_action_value(item, parent_id, event.clone())?;
                }
            }
            PdfValue::Dictionary(dict) => {
                let action_id =
                    self.add_to_ast(PdfValue::Dictionary(dict.clone()), NodeType::Action)?;
                self.add_edge(parent_id, action_id, crate::ast::EdgeType::Child)?;

                if let Some(event_name) = event.clone() {
                    if let Some(node) = self.document.ast.get_node_mut(action_id) {
                        node.metadata
                            .properties
                            .insert("action_event".to_string(), event_name);
                    }
                }

                if let Some(PdfValue::Name(s)) = dict.get("S") {
                    if let Some(node) = self.document.ast.get_node_mut(action_id) {
                        node.metadata
                            .properties
                            .insert("action_type".to_string(), s.without_slash().to_string());
                    }
                }

                if let Some(js_value) = dict.get("JS").or_else(|| dict.get("JavaScript")).cloned() {
                    let resolved_js = match js_value {
                        PdfValue::Reference(reference) => self.load_object(&reference.id())?,
                        _ => js_value,
                    };
                    let js_id = self.add_to_ast(resolved_js, NodeType::JavaScript)?;
                    self.add_edge(action_id, js_id, crate::ast::EdgeType::Child)?;
                }

                if let Some(next_value) = dict.get("Next") {
                    self.parse_action_value(next_value, action_id, None)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn refine_node_type(&self, value: &PdfValue, default_type: NodeType) -> NodeType {
        if let PdfValue::Dictionary(dict) = value {
            // Check for Type entry to determine more specific node type
            if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                match type_name.without_slash() {
                    "Catalog" => return NodeType::Catalog,
                    "Pages" => return NodeType::Pages,
                    "Page" => return NodeType::Page,
                    "StructTreeRoot" => return NodeType::StructTreeRoot,
                    "StructElem" => return NodeType::StructElem,
                    "Font" => return NodeType::Font,
                    "XObject" => return NodeType::XObject,
                    "Annot" => return NodeType::Annotation,
                    "Action" => {
                        // Further refine action types
                        if let Some(PdfValue::Name(subtype)) = dict.get("S") {
                            match subtype.without_slash() {
                                "JavaScript" => return NodeType::JavaScriptAction,
                                "GoTo" => return NodeType::GoToAction,
                                "URI" => return NodeType::URIAction,
                                "Launch" => return NodeType::LaunchAction,
                                "SubmitForm" => return NodeType::SubmitFormAction,
                                _ => return NodeType::Action,
                            }
                        }
                        return NodeType::Action;
                    }
                    "Filespec" => return NodeType::EmbeddedFile,
                    "Encrypt" => return NodeType::Encrypt,
                    _ => {}
                }
            }

            // Check for specific dictionary patterns
            if dict.contains_key("JS") || dict.contains_key("JavaScript") {
                return NodeType::JavaScriptAction;
            }

            if dict.contains_key("Filter") && dict.get("Type").is_none() {
                return NodeType::Stream;
            }

            // Check for embedded files patterns
            if dict.contains_key("F") && dict.contains_key("EF") {
                return NodeType::EmbeddedFile;
            }

            // Check for linearization dictionary
            if dict.contains_key("Linearized") {
                return NodeType::Metadata;
            }
        }

        default_type
    }

    fn find_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn rfind_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .rposition(|window| window == needle)
    }

    fn skip_whitespace(data: &[u8]) -> &[u8] {
        let mut i = 0;
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        &data[i..]
    }

    fn read_integer(data: &[u8]) -> u64 {
        let mut result = 0u64;
        for &byte in data {
            result = (result << 8) | (byte as u64);
        }
        result
    }

    fn resolve_all_references(&mut self) -> AstResult<()> {
        use crate::parser::reference_resolver::ReferenceResolver;
        use std::io::Cursor;

        // Create a new reader for the reference resolver
        let buffer = self.read_limited_input()?;
        self.limits
            .budget
            .consume_memory(buffer.len() as u64)
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        self.document.original_bytes = Some(buffer.clone());
        let cursor = Cursor::new(buffer);

        // Create reference resolver using existing document xref information
        let mut resolver = ReferenceResolver::from_document(
            cursor,
            &self.document,
            self.tolerant,
            self.limits.clone(),
        );

        // Resolve all references in the AST
        if let Err(err) = resolver.resolve_references(&mut self.document.ast) {
            if self.tolerant {
                log::warn!("Reference resolution error (tolerant): {}", err);
            } else {
                return Err(AstError::ParseError(err));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PdfFileParser, XRefEntry, MAX_FORM_FIELD_DEPTH};
    use crate::ast::NodeType;
    use crate::parser::ParseMode;
    use crate::performance::PerformanceLimits;
    use crate::types::{ObjectId, PdfArray, PdfDictionary, PdfReference, PdfStream, PdfValue};
    use std::io::{BufReader, Cursor};

    #[test]
    fn rejects_wrapped_recovery_object_ids() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        assert!(Parser::parse_object_header(b"-1 0 obj").is_err());
        assert!(Parser::parse_object_header(b"1 -1 obj").is_err());
    }

    #[test]
    fn strict_xref_stream_rejects_unknown_entry_type() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("valid header should construct parser");

        let error = parser
            .parse_xref_stream_entry(&[3, 0, 0], &[1, 1, 1])
            .expect_err("strict mode must reject unknown XRef entry types");
        assert!(error.to_string().contains("Invalid XRef entry type"));
    }

    #[test]
    fn tolerant_xfa_parse_records_diagnostic() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Tolerant,
            10,
            PerformanceLimits::default(),
        )
        .expect("valid header should construct parser");
        let mut acroform = PdfDictionary::new();
        acroform.insert(
            "XFA",
            PdfValue::Stream(PdfStream::new(PdfDictionary::new(), b"<xfa>".to_vec())),
        );

        parser
            .process_xfa_document(&acroform)
            .expect("tolerant XFA parsing should continue");
        assert!(parser
            .document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "xfa_parse"));
    }

    #[test]
    fn strict_xfa_value_resolution_rejects_excessive_depth() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut limits = PerformanceLimits {
            max_depth: 1,
            ..PerformanceLimits::default()
        };
        limits.refresh_budget();
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            limits,
        )
        .expect("valid header should construct parser");
        let nested = PdfValue::Array(PdfArray::from(vec![PdfValue::Array(PdfArray::from(vec![
            PdfValue::Integer(1),
        ]))]));

        let error = parser
            .resolve_xfa_value(&nested)
            .expect_err("strict mode must bound XFA value recursion");
        assert!(error.to_string().contains("XFA value depth"));
    }

    #[test]
    fn strict_xmp_parse_error_is_not_silenced() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("valid header should construct parser");
        let catalog_id = parser
            .add_to_ast(
                PdfValue::Dictionary(PdfDictionary::new()),
                NodeType::Catalog,
            )
            .expect("catalog node should be created");
        let stream = PdfStream::new(PdfDictionary::new(), b"<".to_vec());

        let error = parser
            .parse_xmp_metadata(&PdfValue::Stream(stream), catalog_id)
            .expect_err("strict mode must reject malformed XMP");
        assert!(error.to_string().contains("XML parse error"));
    }

    #[test]
    fn resolves_indirect_stream_length_before_parsing_stream_data() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let stream_data = b"abcendstreamxyz";
        let mut data = b"%PDF-1.7\n".to_vec();
        let stream_offset = data.len() as u64;
        data.extend_from_slice(b"4 0 obj\n<< /Length 5 0 R >>\nstream\n");
        data.extend_from_slice(stream_data);
        data.extend_from_slice(b"\nendstream\nendobj\n");
        let length_offset = data.len() as u64;
        data.extend_from_slice(b"5 0 obj\n15\nendobj\n");

        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(data)),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("parser should initialize");
        parser.document.xref.entries.insert(
            ObjectId::new(4, 0),
            XRefEntry::InUse {
                offset: stream_offset,
                generation: 0,
            },
        );
        parser.document.xref.entries.insert(
            ObjectId::new(5, 0),
            XRefEntry::InUse {
                offset: length_offset,
                generation: 0,
            },
        );

        let value = parser
            .load_object(&ObjectId::new(4, 0))
            .expect("stream object should load");
        let PdfValue::Stream(stream) = value else {
            panic!("expected stream");
        };
        assert_eq!(stream.raw_data(), Some(stream_data.as_slice()));
    }

    #[test]
    fn indirect_stream_length_validation_preserves_observed_bytes() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let data = b"%PDF-1.7\n5 0 obj\n3\nendobj\n".to_vec();
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(data)),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("parser should initialize");
        parser.document.xref.entries.insert(
            ObjectId::new(5, 0),
            XRefEntry::InUse {
                offset: 9,
                generation: 0,
            },
        );

        let length_ref = PdfReference::new(5, 0);
        let mut dict = PdfDictionary::new();
        dict.insert("Length", PdfValue::Reference(length_ref));
        let mut stream = PdfStream::new(dict, b"abcd".to_vec());
        parser
            .resolve_indirect_stream_length(&mut stream)
            .expect("declared length should validate");

        assert_eq!(
            stream.dict.get("Length"),
            Some(&PdfValue::Reference(length_ref))
        );
        assert_eq!(stream.raw_data(), Some(b"abcd".as_slice()));
    }

    #[test]
    fn reads_xref_data_beyond_the_legacy_buffer_size() {
        let mut data = b"%PDF-1.7\n".to_vec();
        data.resize(300 * 1024, b'x');
        let parser = PdfFileParser::new_with_limits(
            BufReader::new(Cursor::new(data.clone())),
            ParseMode::Tolerant,
            100,
            PerformanceLimits::default(),
        )
        .expect("parser should initialize");
        let mut parser = parser;
        let buffer = parser.read_xref_buffer(0).expect("xref data should read");
        assert_eq!(buffer.len(), data.len());
    }

    #[test]
    fn ignores_invalid_linearization_dictionaries() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let negative_length =
            b"1 0 obj\n<< /Linearized 1.0 /L -1 /H [0 0] /O 1 /E 1 /N 1 /T 1 >>\nendobj\n";
        let missing_length =
            b"1 0 obj\n<< /Linearized 1.0 /H [0 0] /O 1 /E 1 /N 1 /T 1 >>\nendobj\n";

        assert!(Parser::try_parse_linearization_dict(negative_length).is_none());
        assert!(Parser::try_parse_linearization_dict(missing_length).is_none());
    }

    #[test]
    fn rejects_missing_or_negative_xref_stream_size() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let missing_size = PdfDictionary::new();
        assert!(Parser::extract_xref_index_ranges(&missing_size).is_err());

        let mut negative_size = PdfDictionary::new();
        negative_size.insert("Size", PdfValue::Integer(-1));
        assert!(Parser::extract_xref_index_ranges(&negative_size).is_err());
    }

    #[test]
    fn rejects_xref_stream_width_arrays_with_wrong_length() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut dict = PdfDictionary::new();
        dict.insert("W", PdfValue::Array(vec![PdfValue::Integer(1)].into()));
        assert!(Parser::extract_xref_field_widths(&dict).is_err());

        dict.insert(
            "W",
            PdfValue::Array(
                vec![
                    PdfValue::Integer(1),
                    PdfValue::Integer(2),
                    PdfValue::Integer(3),
                    PdfValue::Integer(4),
                ]
                .into(),
            ),
        );
        assert!(Parser::extract_xref_field_widths(&dict).is_err());
    }

    #[test]
    fn trailer_parsing_honors_configured_nesting_limit() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let trailer = b"trailer << /Nested [1] >>";
        assert!(Parser::extract_trailer_dict(trailer, 0).is_none());
        assert!(Parser::extract_trailer_dict(trailer, 256).is_some());
    }

    #[test]
    fn trailer_parsing_respects_shared_input_budget() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let trailer = b"trailer << /Name (long value) >>";
        let budget = crate::performance::ResourceBudget::new(1, 1024, 1024, 100, 10, 10, 10, 8);
        assert!(Parser::extract_trailer_dict_with_budget(trailer, 256, &budget).is_none());
    }

    #[test]
    fn xref_table_rejects_residual_entries_before_trailer() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Tolerant,
            10,
            PerformanceLimits::default(),
        )
        .expect("parser should initialize");
        let malformed =
            b"xref\n0 1\n0000000000 65535 f \n0000000010 00000 n \ntrailer\n<< /Size 1 >>";

        assert!(parser
            .parse_xref_table_section(malformed)
            .expect("xref parsing should be controlled")
            .is_none());
    }

    #[test]
    fn strict_mode_does_not_recover_malformed_xref() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let valid_xref = b"xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 >>\n";
        let malformed_xref =
            b"xref\n0 1\n0000000000 65535 f \n0000000010 00000 n \ntrailer\n<< /Size 1 >>\n";
        let mut data = b"%PDF-1.7\n".to_vec();
        data.extend_from_slice(valid_xref);
        data.resize(100, b'\n');
        let malformed_offset = data.len() as u64;
        data.extend_from_slice(malformed_xref);

        let mut strict = Parser::new_with_limits(
            BufReader::new(Cursor::new(data.clone())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        assert!(strict.parse_single_xref_at(malformed_offset).is_err());

        let mut tolerant = Parser::new_with_limits(
            BufReader::new(Cursor::new(data)),
            ParseMode::Tolerant,
            10,
            PerformanceLimits::default(),
        )
        .expect("tolerant parser should initialize");
        assert!(tolerant.parse_single_xref_at(malformed_offset).is_ok());
    }

    #[test]
    fn strict_mode_rejects_xref_prev_cycles() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let data = b"%PDF-1.7\nxref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 /Prev 9 >>\n";
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(data.to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        parser.xref_offset = Some(9);

        assert!(parser.parse_xref_chain().is_err());
    }

    #[test]
    fn strict_mode_rejects_page_tree_cycles() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let data = b"%PDF-1.7\n1 0 obj\n<< /Type /Pages /Kids [1 0 R] >>\nendobj\n";
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(data.to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        parser.document.xref.entries.insert(
            ObjectId::new(1, 0),
            XRefEntry::InUse {
                offset: 9,
                generation: 0,
            },
        );
        let root = parser
            .document
            .ast
            .create_node(crate::ast::NodeType::Root, PdfValue::Null);

        let error = parser
            .parse_page_tree(&PdfReference::new(1, 0), root)
            .expect_err("strict page-tree parsing must reject cycles");
        assert!(error.to_string().contains("Cycle detected"));

        let mut tolerant = Parser::new_with_limits(
            BufReader::new(Cursor::new(data.to_vec())),
            ParseMode::Tolerant,
            10,
            PerformanceLimits::default(),
        )
        .expect("tolerant parser should initialize");
        tolerant.document.xref.entries.insert(
            ObjectId::new(1, 0),
            XRefEntry::InUse {
                offset: 9,
                generation: 0,
            },
        );
        let tolerant_root = tolerant
            .document
            .ast
            .create_node(crate::ast::NodeType::Root, PdfValue::Null);
        tolerant
            .parse_page_tree(&PdfReference::new(1, 0), tolerant_root)
            .expect("tolerant page-tree parsing should cut cycles");
        assert!(tolerant
            .document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "page_tree_cycle"));
    }

    #[test]
    fn strict_mode_rejects_form_field_depth_overflow() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut value = PdfValue::Null;
        for _ in 0..=MAX_FORM_FIELD_DEPTH {
            value = PdfValue::Array(PdfArray::from(vec![value]));
        }
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        let root = parser
            .document
            .ast
            .create_node(crate::ast::NodeType::Root, PdfValue::Null);

        let error = parser
            .parse_form_field_value(&value, root, 0)
            .expect_err("strict form parsing must reject excessive depth");
        assert!(error.to_string().contains("Maximum form field depth"));
    }

    #[test]
    fn strict_document_structure_requires_valid_root_reference() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        let error = parser
            .parse_document_structure()
            .expect_err("strict parsing must reject a missing /Root");
        assert!(error.to_string().contains("does not contain a /Root"));

        let mut parser = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        parser.document.trailer.insert("Root", PdfValue::Integer(1));
        let error = parser
            .parse_document_structure()
            .expect_err("strict parsing must reject a non-reference /Root");
        assert!(error.to_string().contains("not an indirect reference"));
    }

    #[test]
    fn strict_object_stream_objects_reject_residual_bytes() {
        type Parser = PdfFileParser<BufReader<Cursor<Vec<u8>>>>;
        let mut dict = PdfDictionary::new();
        dict.insert("N", PdfValue::Integer(1));
        dict.insert("First", PdfValue::Integer(4));
        let data = b"1 0 42 trailing";

        let strict = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Strict,
            0,
            PerformanceLimits::default(),
        )
        .expect("strict parser should initialize");
        assert!(strict
            .parse_object_from_stream(data, 0, &dict)
            .expect_err("strict object streams must reject residual bytes")
            .to_string()
            .contains("Residual bytes"));

        let tolerant = Parser::new_with_limits(
            BufReader::new(Cursor::new(b"%PDF-1.7\n".to_vec())),
            ParseMode::Tolerant,
            10,
            PerformanceLimits::default(),
        )
        .expect("tolerant parser should initialize");
        assert_eq!(
            tolerant
                .parse_object_from_stream(data, 0, &dict)
                .expect("tolerant object streams should recover"),
            PdfValue::Integer(42)
        );
    }
}
