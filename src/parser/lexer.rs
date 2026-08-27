use crate::performance::{ResourceBudget, ResourceBudgetError};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{char, digit1, multispace0, multispace1, one_of},
    combinator::{map, map_res, opt, recognize, value},
    multi::many0,
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};

fn with_budget<'a, O, E, F>(
    input: &'a [u8],
    budget: &ResourceBudget,
    parser: F,
) -> Result<IResult<&'a [u8], O, E>, ResourceBudgetError>
where
    F: Fn(&'a [u8]) -> IResult<&'a [u8], O, E>,
{
    // Charge before parsing so token decoders cannot allocate past the limit.
    budget.consume_input(input.len() as u64)?;
    Ok(parser(input))
}

fn parse_u8(input: &[u8]) -> Result<u8, &'static str> {
    std::str::from_utf8(input)
        .map_err(|_| "invalid numeric token")?
        .parse::<u8>()
        .map_err(|_| "numeric token out of range")
}

fn parse_i64(input: &[u8]) -> Result<i64, &'static str> {
    std::str::from_utf8(input)
        .map_err(|_| "invalid numeric token")?
        .parse::<i64>()
        .map_err(|_| "numeric token out of range")
}

fn parse_f64(input: &[u8]) -> Result<f64, &'static str> {
    let value = std::str::from_utf8(input)
        .map_err(|_| "invalid numeric token")?
        .parse::<f64>()
        .map_err(|_| "invalid real number")?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("real number is not finite")
    }
}

pub fn skip_whitespace(input: &[u8]) -> IResult<&[u8], ()> {
    value((), multispace0)(input)
}

pub fn skip_whitespace_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], ()>, ResourceBudgetError> {
    with_budget(input, budget, skip_whitespace)
}

pub fn skip_whitespace_and_comments(input: &[u8]) -> IResult<&[u8], ()> {
    value((), many0(alt((value((), multispace1), value((), comment)))))(input)
}

pub fn skip_whitespace_and_comments_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], ()>, ResourceBudgetError> {
    with_budget(input, budget, skip_whitespace_and_comments)
}

pub fn comment(input: &[u8]) -> IResult<&[u8], &[u8]> {
    preceded(
        char('%'),
        alt((take_until("\n"), take_until("\r"), nom::combinator::rest)),
    )(input)
}

pub fn comment_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], &'a [u8]>, ResourceBudgetError> {
    with_budget(input, budget, comment)
}

pub fn pdf_header(input: &[u8]) -> IResult<&[u8], (u8, u8)> {
    let (input, _) = tag(b"%PDF-")(input)?;
    let (input, major) = map_res(digit1, parse_u8)(input)?;
    let (input, _) = char('.')(input)?;
    let (input, minor) = map_res(digit1, parse_u8)(input)?;
    Ok((input, (major, minor)))
}

pub fn pdf_header_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], (u8, u8)>, ResourceBudgetError> {
    with_budget(input, budget, pdf_header)
}

pub fn pdf_eof(input: &[u8]) -> IResult<&[u8], &[u8]> {
    tag(b"%%EOF")(input)
}

pub fn pdf_eof_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], &'a [u8]>, ResourceBudgetError> {
    with_budget(input, budget, pdf_eof)
}

pub fn is_whitespace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'\0')
}

pub fn is_delimiter(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

pub fn is_regular_char(c: u8) -> bool {
    !is_whitespace(c) && !is_delimiter(c)
}

pub fn regular_chars(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while1(is_regular_char)(input)
}

pub fn regular_chars_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], &'a [u8]>, ResourceBudgetError> {
    with_budget(input, budget, regular_chars)
}

pub fn keyword(input: &[u8]) -> IResult<&[u8], &[u8]> {
    alt((
        tag(b"true"),
        tag(b"false"),
        tag(b"null"),
        tag(b"obj"),
        tag(b"endobj"),
        tag(b"stream"),
        tag(b"endstream"),
        tag(b"xref"),
        tag(b"startxref"),
        tag(b"trailer"),
        tag(b"R"),
        tag(b"n"),
        tag(b"f"),
    ))(input)
}

pub fn keyword_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], &'a [u8]>, ResourceBudgetError> {
    with_budget(input, budget, keyword)
}

pub fn integer(input: &[u8]) -> IResult<&[u8], i64> {
    map_res(recognize(pair(opt(one_of("+-")), digit1)), parse_i64)(input)
}

pub fn integer_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], i64>, ResourceBudgetError> {
    with_budget(input, budget, integer)
}

pub fn real(input: &[u8]) -> IResult<&[u8], f64> {
    map_res(
        recognize(tuple((
            opt(one_of("+-")),
            alt((
                recognize(tuple((digit1, char('.'), opt(digit1)))),
                recognize(tuple((opt(digit1), char('.'), digit1))),
            )),
        ))),
        parse_f64,
    )(input)
}

pub fn real_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], f64>, ResourceBudgetError> {
    with_budget(input, budget, real)
}

pub fn hex_string(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    delimited(
        char('<'),
        map(
            take_while(|c: u8| c.is_ascii_hexdigit() || is_whitespace(c)),
            |hex: &[u8]| {
                let hex_str: String = hex
                    .iter()
                    .filter(|&&c| !is_whitespace(c))
                    .map(|&c| c as char)
                    .collect();

                let mut result = Vec::new();
                let mut chars = hex_str.chars();

                while let Some(c1) = chars.next() {
                    let c2 = chars.next().unwrap_or('0');
                    if let Ok(byte) = u8::from_str_radix(&format!("{}{}", c1, c2), 16) {
                        result.push(byte);
                    }
                }

                result
            },
        ),
        char('>'),
    )(input)
}

pub fn hex_string_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], Vec<u8>>, ResourceBudgetError> {
    with_budget(input, budget, hex_string)
}

pub fn literal_string(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    delimited(
        char('('),
        map(
            many0(alt((
                preceded(char('\\'), escape_sequence),
                map(take_while1(|c| c != b')' && c != b'\\'), |s: &[u8]| {
                    s.to_vec()
                }),
            ))),
            |parts| parts.into_iter().flatten().collect(),
        ),
        char(')'),
    )(input)
}

pub fn literal_string_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], Vec<u8>>, ResourceBudgetError> {
    with_budget(input, budget, literal_string)
}

fn escape_sequence(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    alt((
        value(vec![b'\n'], char('n')),
        value(vec![b'\r'], char('r')),
        value(vec![b'\t'], char('t')),
        value(vec![b'\x08'], char('b')),
        value(vec![b'\x0C'], char('f')),
        value(vec![b'('], char('(')),
        value(vec![b')'], char(')')),
        value(vec![b'\\'], char('\\')),
        map(octal_escape, |b| vec![b]),
    ))(input)
}

fn octal_escape(input: &[u8]) -> IResult<&[u8], u8> {
    map_res(
        recognize(tuple((
            one_of("01234567"),
            opt(one_of("01234567")),
            opt(one_of("01234567")),
        ))),
        |s: &[u8]| {
            let text = std::str::from_utf8(s).map_err(|_| "invalid octal escape")?;
            u8::from_str_radix(text, 8).map_err(|_| "octal escape out of range")
        },
    )(input)
}

pub fn name(input: &[u8]) -> IResult<&[u8], String> {
    preceded(
        char('/'),
        map(
            take_while(|c: u8| !is_whitespace(c) && !is_delimiter(c)),
            |bytes: &[u8]| {
                let mut result = String::new();
                let mut chars = bytes.iter();

                while let Some(&c) = chars.next() {
                    if c == b'#' {
                        if let (Some(&c1), Some(&c2)) = (chars.next(), chars.next()) {
                            if let Ok(byte) =
                                u8::from_str_radix(&format!("{}{}", c1 as char, c2 as char), 16)
                            {
                                result.push(byte as char);
                                continue;
                            }
                        }
                        result.push('#');
                    } else {
                        result.push(c as char);
                    }
                }

                format!("/{}", result)
            },
        ),
    )(input)
}

pub fn name_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> Result<IResult<&'a [u8], String>, ResourceBudgetError> {
    with_budget(input, budget, name)
}

#[cfg(test)]
mod tests {
    use super::{integer, integer_with_budget, pdf_header, real};
    use crate::performance::{ResourceBudget, ResourceBudgetError};

    #[test]
    fn numeric_overflow_is_a_parse_error() {
        assert!(integer(b"999999999999999999999999").is_err());
        assert!(real(b"1e999999999999999999999").is_err());
        assert!(pdf_header(b"%PDF-999.0").is_err());
    }

    #[test]
    fn budgeted_lexer_charges_input_before_parsing() {
        let budget = ResourceBudget::new(4, 1024, 1024, 100, 10, 10, 10, 10);
        let (remaining, value) = integer_with_budget(b"12 3", &budget).unwrap().unwrap();

        assert_eq!(remaining, b" 3");
        assert_eq!(value, 12);
        assert_eq!(
            integer_with_budget(b"3", &budget),
            Err(ResourceBudgetError::InputBytes)
        );
    }
}
