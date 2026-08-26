use crate::ast::document::XRefEntry;
use crate::ast::linearization::LinearizationInfo;
use crate::filters::decode_stream_with_budget;
use crate::performance::PerformanceLimits;
use crate::types::{ObjectId, PdfStream, PdfValue};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, digit1, multispace0, space1},
    combinator::{map_res, opt},
    multi::many1,
    IResult,
};

type XRefParseResult<'a> = IResult<&'a [u8], (Vec<(ObjectId, XRefEntry)>, Option<PdfStream>)>;

fn parse_u16(input: &[u8]) -> Result<u16, &'static str> {
    std::str::from_utf8(input)
        .map_err(|_| "invalid xref number")?
        .parse::<u16>()
        .map_err(|_| "xref number out of range")
}

fn parse_u32(input: &[u8]) -> Result<u32, &'static str> {
    std::str::from_utf8(input)
        .map_err(|_| "invalid xref number")?
        .parse::<u32>()
        .map_err(|_| "xref number out of range")
}

fn parse_u64(input: &[u8]) -> Result<u64, &'static str> {
    std::str::from_utf8(input)
        .map_err(|_| "invalid xref number")?
        .parse::<u64>()
        .map_err(|_| "xref number out of range")
}

pub fn parse_xref_table(input: &[u8]) -> IResult<&[u8], Vec<(ObjectId, XRefEntry)>> {
    let (input, _) = tag(b"xref")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, sections) = many1(parse_xref_section)(input)?;

    let mut entries = Vec::new();
    for section in sections {
        entries.extend(section);
    }

    Ok((input, entries))
}

fn parse_xref_section(input: &[u8]) -> IResult<&[u8], Vec<(ObjectId, XRefEntry)>> {
    let (input, (start_obj, count)) = parse_xref_subsection_header(input)?;
    let (input, raw_entries) = many1(parse_xref_entry)(input)?;

    let mut entries = Vec::new();
    for (i, entry) in raw_entries.into_iter().take(count as usize).enumerate() {
        let object_number = start_obj.checked_add(i as u32).ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
        })?;
        let obj_id = ObjectId::new(object_number, entry.generation());
        entries.push((obj_id, entry));
    }

    Ok((input, entries))
}

fn parse_xref_subsection_header(input: &[u8]) -> IResult<&[u8], (u32, u32)> {
    let (input, start_obj) = map_res(digit1, parse_u32)(input)?;
    let (input, _) = space1(input)?;
    let (input, count) = map_res(digit1, parse_u32)(input)?;
    let (input, _) = multispace0(input)?;

    Ok((input, (start_obj, count)))
}

fn parse_xref_entry(input: &[u8]) -> IResult<&[u8], XRefEntry> {
    let (input, offset) = map_res(take_while1(|c: u8| c.is_ascii_digit()), parse_u64)(input)?;
    let (input, _) = space1(input)?;
    let (input, generation) = map_res(take_while1(|c: u8| c.is_ascii_digit()), parse_u16)(input)?;
    let (input, _) = space1(input)?;
    let (input, status) = alt((char('n'), char('f')))(input)?;
    let (input, _) = multispace0(input)?;

    let entry = match status {
        'n' => XRefEntry::InUse { offset, generation },
        'f' => XRefEntry::Free {
            next_free_object: offset as u32,
            generation,
        },
        _ => unreachable!(),
    };

    Ok((input, entry))
}

/// Parse XRef Stream (PDF 1.5+)
/// XRef streams are compressed streams that contain the cross-reference information
pub fn parse_xref_stream(
    stream: &PdfStream,
    limits: &PerformanceLimits,
) -> Result<Vec<(ObjectId, XRefEntry)>, String> {
    let dict = &stream.dict;

    // Get W array - widths of the three fields in each entry
    let w_array = dict
        .get("W")
        .and_then(|v| v.as_array())
        .ok_or("Missing W array in XRef stream")?;

    if w_array.len() != 3 {
        return Err("W array must have exactly 3 elements".to_string());
    }

    let width = |index: usize| -> Result<usize, String> {
        let value = w_array[index]
            .as_integer()
            .ok_or_else(|| format!("XRef /W[{}] must be an integer", index))?;
        usize::try_from(value).map_err(|_| format!("XRef /W[{}] must be non-negative", index))
    };
    let w1 = width(0)?; // Type field width
    let w2 = width(1)?; // Field 2 width
    let w3 = width(2)?; // Field 3 width

    if [w1, w2, w3].iter().any(|&width| width > 8) {
        return Err("XRef field widths cannot exceed 8 bytes".to_string());
    }

    let entry_size = w1
        .checked_add(w2)
        .and_then(|size| size.checked_add(w3))
        .ok_or_else(|| "XRef entry width overflow".to_string())?;
    if entry_size == 0 {
        return Err("Invalid W array - all widths are zero".to_string());
    }

    // Get Index array (or default to [0, Size])
    let index_array = if let Some(value) = dict.get("Index") {
        let array = value
            .as_array()
            .ok_or_else(|| "XRef /Index must be an array".to_string())?;
        array
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = value
                    .as_integer()
                    .ok_or_else(|| format!("XRef /Index[{}] must be an integer", index))?;
                u64::try_from(value)
                    .map_err(|_| format!("XRef /Index[{}] must be non-negative", index))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let size = dict
            .get("Size")
            .and_then(|v| v.as_integer())
            .ok_or("Missing XRef /Size")?;
        vec![
            0,
            u64::try_from(size).map_err(|_| "XRef /Size must be non-negative")?,
        ]
    };

    if index_array.len() % 2 != 0 {
        return Err("XRef /Index must contain start/count pairs".to_string());
    }

    // Decode the stream data
    let filters = stream.get_filters();
    let raw_data = match &stream.data {
        crate::types::StreamData::Raw(data) => data,
        crate::types::StreamData::Decoded(data) => data,
        crate::types::StreamData::Lazy(_) => {
            return Err("Cannot decode lazy stream data".to_string());
        }
    };

    let decoded_data = decode_stream_with_budget(raw_data, &filters, &limits.budget)
        .map_err(|e| format!("Failed to decode XRef stream: {}", e))?;

    let mut entries = Vec::new();
    let mut data_offset: usize = 0;
    let entry_size = w1 + w2 + w3;

    // Process each subsection defined in Index array
    for chunk in index_array.chunks(2) {
        if chunk.len() != 2 {
            continue;
        }

        let start = u32::try_from(chunk[0]).map_err(|_| "XRef object number overflow")?;
        let count = u32::try_from(chunk[1]).map_err(|_| "XRef object count overflow")?;

        for i in 0..count {
            let end = data_offset
                .checked_add(entry_size)
                .ok_or_else(|| "XRef data offset overflow".to_string())?;
            if end > decoded_data.len() {
                break; // Not enough data for another entry
            }

            let entry_data = decoded_data
                .get(data_offset..end)
                .ok_or_else(|| "XRef entry is outside decoded data".to_string())?;

            let entry = parse_xref_stream_entry(entry_data, w1, w2, w3)?;
            let object_number = start
                .checked_add(i)
                .ok_or_else(|| "XRef object number overflow".to_string())?;
            let obj_id = ObjectId::new(object_number, entry.generation());

            entries.push((obj_id, entry));
            data_offset += entry_size;
        }
    }

    Ok(entries)
}

/// Parse a single entry from an XRef stream
fn parse_xref_stream_entry(
    data: &[u8],
    w1: usize,
    w2: usize,
    w3: usize,
) -> Result<XRefEntry, String> {
    let entry_size = w1
        .checked_add(w2)
        .and_then(|size| size.checked_add(w3))
        .ok_or_else(|| "XRef entry width overflow".to_string())?;
    if [w1, w2, w3].iter().any(|&width| width > 8) || data.len() < entry_size {
        return Err("XRef entry has invalid field widths or length".to_string());
    }

    let mut offset: usize = 0;

    // Field 1: Type (0 = free, 1 = normal, 2 = compressed)
    let type_field = if w1 > 0 {
        let end = offset
            .checked_add(w1)
            .ok_or_else(|| "XRef type field offset overflow".to_string())?;
        read_int_field(
            data.get(offset..end)
                .ok_or_else(|| "XRef type field is outside entry".to_string())?,
        )
    } else {
        1 // Default type is 1 (normal object)
    };
    offset += w1;

    // Field 2: Object number or offset
    let field2 = if w2 > 0 {
        let end = offset
            .checked_add(w2)
            .ok_or_else(|| "XRef field 2 offset overflow".to_string())?;
        read_int_field(
            data.get(offset..end)
                .ok_or_else(|| "XRef field 2 is outside entry".to_string())?,
        )
    } else {
        0
    };
    offset += w2;

    // Field 3: Generation or index
    let field3 = if w3 > 0 {
        let end = offset
            .checked_add(w3)
            .ok_or_else(|| "XRef field 3 offset overflow".to_string())?;
        read_int_field(
            data.get(offset..end)
                .ok_or_else(|| "XRef field 3 is outside entry".to_string())?,
        )
    } else {
        0
    };

    match type_field {
        0 => {
            // Free object entry
            Ok(XRefEntry::Free {
                next_free_object: u32::try_from(field2)
                    .map_err(|_| "XRef free-object number overflow".to_string())?,
                generation: u16::try_from(field3)
                    .map_err(|_| "XRef generation overflow".to_string())?,
            })
        }
        1 => {
            // Normal object entry
            Ok(XRefEntry::InUse {
                offset: field2,
                generation: u16::try_from(field3)
                    .map_err(|_| "XRef generation overflow".to_string())?,
            })
        }
        2 => {
            // Compressed object entry
            Ok(XRefEntry::Compressed {
                stream_object: u32::try_from(field2)
                    .map_err(|_| "XRef object stream number overflow".to_string())?,
                index: u32::try_from(field3)
                    .map_err(|_| "XRef object stream index overflow".to_string())?,
            })
        }
        _ => Err(format!("Invalid XRef entry type: {}", type_field)),
    }
}

/// Read an integer field from bytes (big-endian)
fn read_int_field(data: &[u8]) -> u64 {
    let mut result = 0u64;
    for &byte in data {
        result = (result << 8) | (byte as u64);
    }
    result
}

/// Linearized PDF parsing support
/// Linearized PDFs are optimized for web viewing and have a special structure
pub fn parse_linearization_dict(stream: &PdfStream) -> Result<LinearizationInfo, String> {
    let dict = &stream.dict;

    // Linearized PDFs must have a /Linearized entry
    if !dict.contains_key("Linearized") {
        return Err("Not a linearized PDF - missing /Linearized entry".to_string());
    }

    let linearized_version = dict
        .get("Linearized")
        .and_then(|v| v.as_real())
        .unwrap_or(1.0);

    let length = dict
        .get("L")
        .and_then(|v| v.as_integer())
        .ok_or("Missing /L (file length) in linearization dict")?;

    let hint_offset = dict
        .get("H")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|v| v.as_integer())
        .ok_or("Missing /H (hint stream offset) in linearization dict")?;

    let hint_length = dict
        .get("H")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_integer());

    let object_count = dict
        .get("N")
        .and_then(|v| v.as_integer())
        .ok_or("Missing /N (object count) in linearization dict")?;

    let first_page_offset = dict
        .get("O")
        .and_then(|v| v.as_integer())
        .ok_or("Missing /O (first page offset) in linearization dict")?;

    let first_page_end = dict
        .get("E")
        .and_then(|v| v.as_integer())
        .ok_or("Missing /E (first page end) in linearization dict")?;

    let main_xref_entries = dict
        .get("T")
        .and_then(|v| v.as_integer())
        .ok_or("Missing /T (main xref table entries) in linearization dict")?;

    let non_negative = |value: i64, name: &str| {
        u64::try_from(value).map_err(|_| format!("Linearization /{} must be non-negative", name))
    };
    let length = non_negative(length, "L")?;
    let hint_offset = non_negative(hint_offset, "H")?;
    let hint_length = hint_length
        .map(|value| non_negative(value, "H"))
        .transpose()?;
    let object_count = u32::try_from(non_negative(object_count, "N")?)
        .map_err(|_| "Linearization /N exceeds u32".to_string())?;
    let first_page_offset = u32::try_from(non_negative(first_page_offset, "O")?)
        .map_err(|_| "Linearization /O exceeds u32".to_string())?;
    let first_page_end = non_negative(first_page_end, "E")?;
    let main_xref_entries = u32::try_from(non_negative(main_xref_entries, "T")?)
        .map_err(|_| "Linearization /T exceeds u32".to_string())?;

    Ok(LinearizationInfo {
        version: linearized_version,
        file_length: length,
        hint_stream_offset: hint_offset,
        hint_stream_length: hint_length,
        object_count,
        first_page_object_number: first_page_offset,
        first_page_end_offset: first_page_end,
        main_xref_table_entries: main_xref_entries,
    })
}

/// Parse hybrid XRef table/stream
/// Some PDFs use both traditional xref tables and xref streams
pub fn parse_hybrid_xref(input: &[u8]) -> XRefParseResult<'_> {
    // Try to parse traditional xref table first
    if let Ok((remaining, table_entries)) = parse_xref_table(input) {
        // Check if there's an xref stream following
        let (remaining, xref_stream) = opt(parse_xref_stream_object)(remaining)?;
        return Ok((remaining, (table_entries, xref_stream)));
    }

    // If no traditional table, try xref stream
    let (remaining, xref_stream) = parse_xref_stream_object(input)?;
    Ok((remaining, (Vec::new(), Some(xref_stream))))
}

/// Parse an XRef stream object
fn parse_xref_stream_object(input: &[u8]) -> IResult<&[u8], PdfStream> {
    // This is a simplified implementation - in practice, you'd use the full object parser
    use crate::parser::object_parser::parse_indirect_object;

    let (input, (_obj_id, value)) = parse_indirect_object(input)?;

    if let PdfValue::Stream(stream) = value {
        // Verify it's an XRef stream by checking for required entries
        if stream
            .dict
            .get("Type")
            .and_then(|v| v.as_name())
            .map(|n| n.as_str())
            == Some("/XRef")
        {
            Ok((input, stream))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )))
    }
}

impl XRefEntry {
    pub fn generation(&self) -> u16 {
        match self {
            XRefEntry::InUse { generation, .. } | XRefEntry::Free { generation, .. } => *generation,
            XRefEntry::Compressed { .. } => 0,
        }
    }

    /// Check if this entry represents an object in use
    pub fn is_in_use(&self) -> bool {
        matches!(self, XRefEntry::InUse { .. } | XRefEntry::Compressed { .. })
    }

    /// Check if this entry represents a free object
    pub fn is_free(&self) -> bool {
        matches!(self, XRefEntry::Free { .. })
    }

    /// Get the offset for in-use objects
    pub fn offset(&self) -> Option<u64> {
        match self {
            XRefEntry::InUse { offset, .. } => Some(*offset),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_xref_stream, parse_xref_stream_entry, parse_xref_table};
    use crate::performance::PerformanceLimits;
    use crate::types::{PdfArray, PdfDictionary, PdfStream, PdfValue};

    fn xref_stream(w: Vec<PdfValue>, index: Option<Vec<PdfValue>>) -> PdfStream {
        let mut dict = PdfDictionary::new();
        dict.insert("W", PdfValue::Array(PdfArray::from(w)));
        dict.insert("Size", PdfValue::Integer(1));
        if let Some(index) = index {
            dict.insert("Index", PdfValue::Array(PdfArray::from(index)));
        }
        PdfStream::new(dict, vec![1, 0, 0, 0, 0, 0, 0, 0, 0])
    }

    #[test]
    fn rejects_negative_xref_widths() {
        let stream = xref_stream(
            vec![
                PdfValue::Integer(-1),
                PdfValue::Integer(1),
                PdfValue::Integer(1),
            ],
            None,
        );

        let error = parse_xref_stream(&stream, &PerformanceLimits::default())
            .expect_err("negative width must be rejected");
        assert!(error.contains("non-negative"));
    }

    #[test]
    fn rejects_negative_xref_index_values() {
        let stream = xref_stream(
            vec![
                PdfValue::Integer(1),
                PdfValue::Integer(1),
                PdfValue::Integer(1),
            ],
            Some(vec![PdfValue::Integer(-1), PdfValue::Integer(1)]),
        );

        let error = parse_xref_stream(&stream, &PerformanceLimits::default())
            .expect_err("negative index must be rejected");
        assert!(error.contains("non-negative"));
    }

    #[test]
    fn rejects_xref_fields_wider_than_u64() {
        let stream = xref_stream(
            vec![
                PdfValue::Integer(9),
                PdfValue::Integer(0),
                PdfValue::Integer(0),
            ],
            None,
        );

        let error = parse_xref_stream(&stream, &PerformanceLimits::default())
            .expect_err("wide field must be rejected");
        assert!(error.contains("8 bytes"));
    }

    #[test]
    fn rejects_xref_numeric_overflow_without_panicking() {
        assert!(
            parse_xref_table(b"xref\n999999999999999999999999 1\n0000000000 00000 n\n").is_err()
        );
        assert!(parse_xref_table(b"xref\n0 1\n999999999999999999999999 00000 n\n").is_err());
        assert!(parse_xref_table(b"xref\n0 1\n0000000000 99999 n\n").is_err());
    }

    #[test]
    fn rejects_xref_stream_field_truncation() {
        let error = parse_xref_stream_entry(&[1, 0, 0, 0, 0, 0, 1, 0, 0, 0], 1, 5, 4)
            .expect_err("generation values wider than u16 must be rejected");
        assert!(error.contains("generation"));
    }

    #[test]
    fn rejects_xref_data_over_shared_decode_budget() {
        let stream = xref_stream(
            vec![
                PdfValue::Integer(1),
                PdfValue::Integer(1),
                PdfValue::Integer(1),
            ],
            None,
        );
        let mut limits = PerformanceLimits::default();
        limits.max_object_size_mb = 0;
        limits.refresh_budget();

        let error = parse_xref_stream(&stream, &limits)
            .expect_err("xref data over the shared budget must be rejected");
        assert!(error.contains("DecodedBytes"));
    }

    #[test]
    fn rejects_negative_linearization_offsets() {
        let mut dict = PdfDictionary::new();
        dict.insert("Linearized", PdfValue::Real(1.0));
        dict.insert("L", PdfValue::Integer(-1));
        dict.insert(
            "H",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(0),
                PdfValue::Integer(0),
            ])),
        );
        dict.insert("N", PdfValue::Integer(1));
        dict.insert("O", PdfValue::Integer(1));
        dict.insert("E", PdfValue::Integer(1));
        dict.insert("T", PdfValue::Integer(1));
        let stream = PdfStream::new(dict, Vec::new());

        let error = super::parse_linearization_dict(&stream)
            .expect_err("negative linearization length must be rejected");
        assert!(error.contains("non-negative"));
    }
}
