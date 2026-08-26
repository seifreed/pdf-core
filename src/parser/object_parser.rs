use crate::parser::lexer::*;
use crate::types::*;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::char,
    combinator::{map, value},
    multi::many0,
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    IResult,
};

pub fn parse_value(input: &[u8]) -> IResult<&[u8], PdfValue> {
    parse_value_with_depth(input, 0)
}

const MAX_NESTING_DEPTH: usize = 256;

fn parse_value_with_depth(input: &[u8], depth: usize) -> IResult<&[u8], PdfValue> {
    if depth > MAX_NESTING_DEPTH {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        )));
    }

    preceded(
        skip_whitespace_and_comments,
        alt((
            parse_null,
            parse_boolean,
            parse_reference,
            parse_real_or_integer,
            parse_string,
            parse_name_value,
            |i| parse_array_with_depth(i, depth + 1),
            |i| parse_dictionary_with_depth(i, depth + 1),
        )),
    )(input)
}

fn parse_null(input: &[u8]) -> IResult<&[u8], PdfValue> {
    value(PdfValue::Null, tag(b"null"))(input)
}

fn parse_boolean(input: &[u8]) -> IResult<&[u8], PdfValue> {
    alt((
        value(PdfValue::Boolean(true), tag(b"true")),
        value(PdfValue::Boolean(false), tag(b"false")),
    ))(input)
}

fn parse_real_or_integer(input: &[u8]) -> IResult<&[u8], PdfValue> {
    alt((map(real, PdfValue::Real), map(integer, PdfValue::Integer)))(input)
}

fn parse_string(input: &[u8]) -> IResult<&[u8], PdfValue> {
    alt((
        map(hex_string, |bytes| {
            PdfValue::String(PdfString::new_hex(bytes))
        }),
        map(literal_string, |bytes| {
            PdfValue::String(PdfString::new_literal(bytes))
        }),
    ))(input)
}

fn parse_name_value(input: &[u8]) -> IResult<&[u8], PdfValue> {
    map(name, |n| PdfValue::Name(PdfName::new(n)))(input)
}

fn parse_array_with_depth(input: &[u8], depth: usize) -> IResult<&[u8], PdfValue> {
    map(
        delimited(
            terminated(char('['), skip_whitespace_and_comments),
            many0(|i| parse_value_with_depth(i, depth + 1)),
            preceded(skip_whitespace_and_comments, char(']')),
        ),
        |values| PdfValue::Array(PdfArray::from(values)),
    )(input)
}

fn parse_dictionary_with_depth(input: &[u8], depth: usize) -> IResult<&[u8], PdfValue> {
    map(
        delimited(
            terminated(tag(b"<<"), skip_whitespace_and_comments),
            many0(preceded(
                skip_whitespace_and_comments,
                separated_pair(name, skip_whitespace_and_comments, |i| {
                    parse_value_with_depth(i, depth + 1)
                }),
            )),
            preceded(skip_whitespace_and_comments, tag(b">>")),
        ),
        |pairs| {
            let mut dict = PdfDictionary::new();
            for (key, value) in pairs {
                dict.insert(key, value);
            }
            PdfValue::Dictionary(dict)
        },
    )(input)
}

fn parse_reference(input: &[u8]) -> IResult<&[u8], PdfValue> {
    map(
        tuple((
            integer,
            preceded(skip_whitespace, integer),
            preceded(skip_whitespace, char('R')),
        )),
        |(obj_num, gen_num, _)| (obj_num, gen_num),
    )(input)
    .and_then(|(remaining, (obj_num, gen_num))| {
        if obj_num < 0 || gen_num < 0 || obj_num > u32::MAX as i64 || gen_num > u16::MAX as i64 {
            return Err(nom::Err::Failure(nom::error::Error::new(
                remaining,
                nom::error::ErrorKind::Verify,
            )));
        }
        Ok((
            remaining,
            PdfValue::Reference(PdfReference::new(obj_num as u32, gen_num as u16)),
        ))
    })
}

pub fn parse_indirect_object(input: &[u8]) -> IResult<&[u8], (ObjectId, PdfValue)> {
    let (input, obj_id) = parse_indirect_object_header(input)?;
    let (input, _) = skip_whitespace_and_comments(input)?;
    let (input, value) = parse_value(input)?;
    let (input, _) = skip_whitespace_and_comments(input)?;

    let (input, value) =
        if let Ok((input2, _)) = tag::<_, _, nom::error::Error<_>>(b"stream")(input) {
            let (input3, stream_value) = parse_stream_data(input2, value)?;
            (input3, stream_value)
        } else {
            (input, value)
        };

    let (input, _) = skip_whitespace_and_comments(input)?;
    let (input, _) = tag(b"endobj")(input)?;

    Ok((input, (obj_id, value)))
}

pub(crate) fn parse_indirect_object_header(input: &[u8]) -> IResult<&[u8], ObjectId> {
    let (input, obj_num) = integer(input)?;
    let (input, _) = skip_whitespace(input)?;
    let (input, gen_num) = integer(input)?;
    let (input, _) = skip_whitespace(input)?;
    let (input, _) = tag(b"obj")(input)?;
    if obj_num < 0 || gen_num < 0 || obj_num > u32::MAX as i64 || gen_num > u16::MAX as i64 {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    Ok((input, ObjectId::new(obj_num as u32, gen_num as u16)))
}

pub fn parse_indirect_stream_prefix(input: &[u8]) -> IResult<&[u8], (ObjectId, PdfDictionary)> {
    let (input, obj_num) = integer(input)?;
    let (input, _) = skip_whitespace(input)?;
    let (input, gen_num) = integer(input)?;
    let (input, _) = skip_whitespace(input)?;
    let (input, _) = tag(b"obj")(input)?;
    if obj_num < 0 || gen_num < 0 || obj_num > u32::MAX as i64 || gen_num > u16::MAX as i64 {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    let (input, _) = skip_whitespace_and_comments(input)?;
    let (input, value) = parse_value(input)?;
    let dict = match value {
        PdfValue::Dictionary(dict) => dict,
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    };
    let (input, _) = skip_whitespace_and_comments(input)?;
    let (input, _) = tag(b"stream")(input)?;
    let (input, _) = alt((tag(b"\r\n"), tag(b"\n")))(input)?;

    Ok((input, (ObjectId::new(obj_num as u32, gen_num as u16), dict)))
}

pub fn parse_indirect_object_with_stream_length(
    input: &[u8],
    length: usize,
) -> IResult<&[u8], (ObjectId, PdfValue)> {
    let (input, (obj_id, dict)) = parse_indirect_stream_prefix(input)?;
    let (input, data) = nom::bytes::complete::take(length)(input)?;
    let (input, _) = skip_whitespace(input)?;
    let (input, _) = tag(b"endstream")(input)?;
    let (input, _) = skip_whitespace_and_comments(input)?;
    let (input, _) = tag(b"endobj")(input)?;

    Ok((
        input,
        (
            obj_id,
            PdfValue::Stream(PdfStream::new(dict, data.to_vec())),
        ),
    ))
}

fn parse_stream_data(input: &[u8], dict_value: PdfValue) -> IResult<&[u8], PdfValue> {
    parse_stream_data_with_resolver(input, dict_value, None)
}

fn parse_stream_data_with_resolver<'a>(
    input: &'a [u8],
    dict_value: PdfValue,
    _resolver: Option<
        &'a crate::parser::reference_resolver::ReferenceResolver<
            std::io::BufReader<std::io::Cursor<Vec<u8>>>,
        >,
    >,
) -> IResult<&'a [u8], PdfValue> {
    if let PdfValue::Dictionary(dict) = dict_value {
        let (input, _) = alt((tag(b"\r\n"), tag(b"\n")))(input)?;

        // Try to resolve Length - could be direct integer or indirect reference
        let length = match dict.get("Length") {
            Some(PdfValue::Integer(len)) if *len >= 0 => *len as usize,
            Some(PdfValue::Integer(_)) => {
                return Err(nom::Err::Failure(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Verify,
                )))
            }
            Some(PdfValue::Reference(pdf_ref)) => {
                let _ = pdf_ref;
                return parse_stream_with_endstream_detection(input, dict);
            }
            _ => {
                return parse_stream_with_endstream_detection(input, dict);
            }
        };

        let (input, data) = nom::bytes::complete::take(length)(input)?;
        let (input, _) = skip_whitespace(input)?;
        let (input, _) = tag(b"endstream")(input)?;

        let stream = PdfStream::new(dict, data.to_vec());
        Ok((input, PdfValue::Stream(stream)))
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )))
    }
}

fn parse_stream_with_endstream_detection(
    input: &[u8],
    dict: PdfDictionary,
) -> IResult<&[u8], PdfValue> {
    // Find "endstream" marker
    let endstream_marker = b"endstream";
    let mut pos = 0;

    while pos + endstream_marker.len() <= input.len() {
        if &input[pos..pos + endstream_marker.len()] == endstream_marker {
            // Found endstream, check if it's properly delimited
            let before_endstream =
                if pos > 0 && (input[pos - 1] == b'\r' || input[pos - 1] == b'\n') {
                    pos - 1
                } else {
                    pos
                };

            let data = if before_endstream > 0
                && input[before_endstream - 1] == b'\r'
                && input[before_endstream] == b'\n'
            {
                input[0..before_endstream - 1].to_vec()
            } else {
                input[0..before_endstream].to_vec()
            };

            let remaining = &input[pos + endstream_marker.len()..];
            let stream = PdfStream::new(dict, data);
            return Ok((remaining, PdfValue::Stream(stream)));
        }
        pos += 1;
    }

    Err(nom::Err::Failure(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

/// Return validated absolute offsets for objects stored in an object stream.
pub fn parse_object_stream_offsets(
    data: &[u8],
    object_count: usize,
    first: usize,
) -> Result<Vec<usize>, String> {
    if object_count == 0 || first == 0 {
        return Err("Invalid object stream header".to_string());
    }
    let header = data
        .get(..first)
        .ok_or_else(|| "Object stream header exceeds decoded data".to_string())?;
    let mut cursor = 0;
    let mut offsets = Vec::with_capacity(object_count);

    for _ in 0..object_count {
        let _object_number = next_decimal(header, &mut cursor)?;
        let relative_offset = next_decimal(header, &mut cursor)?;
        let absolute_offset = first
            .checked_add(relative_offset)
            .ok_or_else(|| "Object stream offset overflow".to_string())?;
        if absolute_offset >= data.len() {
            return Err("Object stream object offset exceeds decoded data".to_string());
        }
        offsets.push(absolute_offset);
    }

    Ok(offsets)
}

fn next_decimal(data: &[u8], cursor: &mut usize) -> Result<usize, String> {
    while *cursor < data.len() && data[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    let start = *cursor;
    let mut value = 0usize;
    while *cursor < data.len() && data[*cursor].is_ascii_digit() {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add((data[*cursor] - b'0') as usize))
            .ok_or_else(|| "Object stream header number overflow".to_string())?;
        *cursor += 1;
    }
    if *cursor == start {
        return Err("Incomplete object stream header".to_string());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_indirect_object, parse_indirect_object_with_stream_length,
        parse_indirect_stream_prefix, parse_value,
    };
    use crate::types::PdfValue;

    #[test]
    fn rejects_negative_references() {
        assert!(parse_value(b"-1 0 R").is_err());
        assert!(parse_value(b"1 -1 R").is_err());
    }

    #[test]
    fn rejects_negative_indirect_object_ids() {
        assert!(parse_indirect_object(b"-1 0 obj null endobj").is_err());
        assert!(parse_indirect_object(b"1 -1 obj null endobj").is_err());
    }

    #[test]
    fn keeps_valid_references() {
        let (_, value) = parse_value(b"12 3 R").expect("valid reference");
        assert!(matches!(value, PdfValue::Reference(_)));
    }

    #[test]
    fn parses_indirect_stream_length_without_scanning_for_endstream() {
        let data = b"1 0 obj\n<< /Length 9 0 R >>\nstream\nabcendstreamxyz\nendstream\nendobj";
        let (remaining, (_, _dict)) = parse_indirect_stream_prefix(data).unwrap();
        assert_eq!(remaining, b"abcendstreamxyz\nendstream\nendobj");

        let (_, (_, value)) = parse_indirect_object_with_stream_length(data, 15).unwrap();
        let PdfValue::Stream(stream) = value else {
            panic!("expected stream");
        };
        assert_eq!(stream.raw_data(), Some(b"abcendstreamxyz".as_slice()));
    }
}
