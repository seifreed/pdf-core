use crate::parser::content_stream::{ContentOperator, InlineImageInfo, TextArrayElement};
use crate::performance::{ResourceBudget, ResourceBudgetError};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{char, digit1, multispace0, multispace1, one_of},
    combinator::{map, opt, recognize},
    multi::separated_list0,
    sequence::{delimited, preceded, tuple},
    IResult,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Operand>),
    Dictionary(HashMap<String, Operand>),
    Boolean(bool),
    Null,
}

impl Operand {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Operand::Integer(i) => Some(*i as f64),
            Operand::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            Operand::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&str> {
        match self {
            Operand::Name(n) => Some(n),
            _ => None,
        }
    }
}

/// Parse complete content stream with operands
pub fn parse_content_stream(input: &[u8]) -> Vec<ContentOperator> {
    parse_content_stream_with_budget(input, &ResourceBudget::default()).unwrap_or_default()
}

pub fn parse_content_stream_with_budget(
    input: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<ContentOperator>, ResourceBudgetError> {
    budget.consume_input(input.len() as u64)?;
    let mut operators = Vec::new();
    let mut operand_stack: Vec<Operand> = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        // Skip whitespace
        if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
            remaining = rest;
        }

        if remaining.is_empty() {
            break;
        }

        // Try to parse operand
        if let Ok((rest, operand)) = parse_operand(remaining) {
            operand_stack.push(operand);
            remaining = rest;
        }
        // Try to parse operator
        else if let Ok((rest, op)) = parse_operator_with_operands(remaining, &mut operand_stack) {
            budget.consume_node()?;
            operators.push(op);
            remaining = rest;
        }
        // Skip unrecognized byte
        else {
            remaining = &remaining[1..];
        }
    }

    Ok(operators)
}

#[derive(Debug, Clone)]
pub struct ContentOperatorWithOffset {
    pub operator: ContentOperator,
    pub offset: usize,
}

/// Parse content stream and capture operator byte offsets.
pub fn parse_content_stream_with_offsets(input: &[u8]) -> Vec<ContentOperatorWithOffset> {
    parse_content_stream_with_offsets_with_budget(input, &ResourceBudget::default())
        .unwrap_or_default()
}

pub fn parse_content_stream_with_offsets_with_budget(
    input: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<ContentOperatorWithOffset>, ResourceBudgetError> {
    budget.consume_input(input.len() as u64)?;
    let mut operators = Vec::new();
    let mut operand_stack: Vec<Operand> = Vec::new();
    let mut remaining = input;
    let base_len = input.len();

    while !remaining.is_empty() {
        if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
            remaining = rest;
        }
        if remaining.is_empty() {
            break;
        }

        if let Ok((rest, operand)) = parse_operand(remaining) {
            operand_stack.push(operand);
            remaining = rest;
        } else if let Ok((rest, op)) = parse_operator_with_operands(remaining, &mut operand_stack) {
            budget.consume_node()?;
            let offset = base_len.saturating_sub(remaining.len());
            operators.push(ContentOperatorWithOffset {
                operator: op,
                offset,
            });
            remaining = rest;
        } else {
            remaining = &remaining[1..];
        }
    }

    Ok(operators)
}

pub fn parse_content_stream_strict_with_budget(
    input: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<ContentOperator>, String> {
    parse_content_stream_with_offsets_strict_with_budget(input, budget).map(|operators| {
        operators
            .into_iter()
            .map(|operator| operator.operator)
            .collect()
    })
}

pub fn parse_content_stream_with_offsets_strict_with_budget(
    input: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<ContentOperatorWithOffset>, String> {
    budget
        .consume_input(input.len() as u64)
        .map_err(|error| error.to_string())?;
    let mut operators = Vec::new();
    let mut operand_stack: Vec<Operand> = Vec::new();
    let mut remaining = input;
    let base_len = input.len();

    while !remaining.is_empty() {
        let (rest, _) = multispace0::<_, nom::error::Error<_>>(remaining)
            .map_err(|error| format!("Invalid content stream whitespace: {error:?}"))?;
        remaining = rest;
        if remaining.is_empty() {
            break;
        }

        if remaining.starts_with(b"BI")
            && remaining
                .get(2)
                .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            let offset = base_len.saturating_sub(remaining.len());
            let (rest, image) = parse_inline_image_after_input_budget(remaining, budget)
                .map_err(|error| format!("Invalid inline image: {error:?}"))?;
            for operator in [
                ContentOperator::BeginInlineImage,
                ContentOperator::InlineImageData(image.clone()),
                ContentOperator::EndInlineImage,
            ] {
                budget.consume_node().map_err(|error| error.to_string())?;
                operators.push(ContentOperatorWithOffset { operator, offset });
            }
            remaining = rest;
            continue;
        }

        if let Ok((rest, operand)) = parse_operand(remaining) {
            operand_stack.push(operand);
            remaining = rest;
            continue;
        }

        if let Ok((rest, operator)) = parse_operator_with_operands(remaining, &mut operand_stack) {
            budget.consume_node().map_err(|error| error.to_string())?;
            let offset = base_len.saturating_sub(remaining.len());
            operators.push(ContentOperatorWithOffset { operator, offset });
            remaining = rest;
            continue;
        }

        let preview_len = remaining.len().min(16);
        return Err(format!(
            "Invalid content stream token at offset {}: {:?}",
            base_len.saturating_sub(remaining.len()),
            &remaining[..preview_len]
        ));
    }

    if !operand_stack.is_empty() {
        return Err("Content stream ended with unconsumed operands".to_string());
    }

    Ok(operators)
}

/// Parse a single operand
fn parse_operand(input: &[u8]) -> IResult<&[u8], Operand> {
    alt((
        map(parse_number, |n| match n {
            Number::Integer(i) => Operand::Integer(i),
            Number::Real(r) => Operand::Real(r),
        }),
        map(parse_string, Operand::String),
        map(parse_hex_string, Operand::String),
        map(parse_name, Operand::Name),
        map(parse_array, Operand::Array),
        map(parse_dictionary, Operand::Dictionary),
        map(tag(b"true"), |_| Operand::Boolean(true)),
        map(tag(b"false"), |_| Operand::Boolean(false)),
        map(tag(b"null"), |_| Operand::Null),
    ))(input)
}

#[derive(Debug)]
enum Number {
    Integer(i64),
    Real(f64),
}

fn parse_number(input: &[u8]) -> IResult<&[u8], Number> {
    let (input, sign) = opt(one_of("+-"))(input)?;
    let (input, num_str) = recognize(tuple((digit1, opt(tuple((char('.'), digit1))))))(input)?;

    let num_string = std::str::from_utf8(num_str).map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;

    let sign_mult = if sign == Some('-') { -1.0 } else { 1.0 };

    if num_string.contains('.') {
        let value: f64 = num_string.parse().map_err(|_| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        Ok((input, Number::Real(value * sign_mult)))
    } else {
        let value: i64 = num_string.parse().map_err(|_| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        Ok((input, Number::Integer((value as f64 * sign_mult) as i64)))
    }
}

fn parse_string(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (input, _) = char('(')(input)?;
    let mut result = Vec::new();
    let mut remaining = input;
    let mut paren_depth = 1;

    while paren_depth > 0 && !remaining.is_empty() {
        match remaining[0] {
            b'(' => {
                paren_depth += 1;
                result.push(b'(');
                remaining = &remaining[1..];
            }
            b')' => {
                paren_depth -= 1;
                if paren_depth > 0 {
                    result.push(b')');
                }
                remaining = &remaining[1..];
            }
            b'\\' if remaining.len() > 1 => {
                // Handle escape sequences
                match remaining[1] {
                    b'n' => result.push(b'\n'),
                    b'r' => result.push(b'\r'),
                    b't' => result.push(b'\t'),
                    b'b' => result.push(b'\x08'),
                    b'f' => result.push(b'\x0C'),
                    b'(' => result.push(b'('),
                    b')' => result.push(b')'),
                    b'\\' => result.push(b'\\'),
                    c if c.is_ascii_digit() => {
                        // Octal escape
                        let mut octal = vec![c];
                        let mut idx = 2;
                        while idx < remaining.len() && idx < 4 && remaining[idx].is_ascii_digit() {
                            octal.push(remaining[idx]);
                            idx += 1;
                        }
                        if let Ok(s) = std::str::from_utf8(&octal) {
                            if let Ok(n) = u8::from_str_radix(s, 8) {
                                result.push(n);
                            }
                        }
                        remaining = &remaining[idx..];
                        continue;
                    }
                    _ => {
                        result.push(remaining[1]);
                    }
                }
                remaining = &remaining[2..];
            }
            c => {
                result.push(c);
                remaining = &remaining[1..];
            }
        }
    }

    Ok((remaining, result))
}

fn parse_hex_string(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (input, _) = char('<')(input)?;
    let (input, hex) = take_while(|c: u8| c.is_ascii_hexdigit() || c.is_ascii_whitespace())(input)?;
    let (input, _) = char('>')(input)?;

    let hex_clean: Vec<u8> = hex
        .iter()
        .filter(|c| c.is_ascii_hexdigit())
        .copied()
        .collect();

    let mut result = Vec::new();
    for chunk in hex_clean.chunks(2) {
        let high = chunk[0];
        let low = if chunk.len() > 1 { chunk[1] } else { b'0' };

        let h = if high.is_ascii_digit() {
            high - b'0'
        } else {
            (high.to_ascii_uppercase() - b'A') + 10
        };
        let l = if low.is_ascii_digit() {
            low - b'0'
        } else {
            (low.to_ascii_uppercase() - b'A') + 10
        };

        result.push((h << 4) | l);
    }

    Ok((input, result))
}

fn parse_name(input: &[u8]) -> IResult<&[u8], String> {
    let (input, _) = char('/')(input)?;
    let (input, name) = take_while(|c: u8| {
        !c.is_ascii_whitespace()
            && c != b'/'
            && c != b'['
            && c != b']'
            && c != b'('
            && c != b')'
            && c != b'<'
            && c != b'>'
    })(input)?;

    // Decode # escapes
    let mut result = String::new();
    let mut i = 0;
    let name_bytes = name;

    while i < name_bytes.len() {
        if name_bytes[i] == b'#' && i + 2 < name_bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&name_bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    result.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        }
        result.push(name_bytes[i] as char);
        i += 1;
    }

    Ok((input, result))
}

fn parse_array(input: &[u8]) -> IResult<&[u8], Vec<Operand>> {
    delimited(
        preceded(char('['), multispace0),
        separated_list0(multispace1, parse_operand),
        preceded(multispace0, char(']')),
    )(input)
}

fn parse_dictionary(input: &[u8]) -> IResult<&[u8], HashMap<String, Operand>> {
    let (input, _) = preceded(tag(b"<<"), multispace0)(input)?;
    let mut dict = HashMap::new();
    let mut remaining = input;

    loop {
        // Skip whitespace
        if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
            remaining = rest;
        }

        // Check for end
        if let Ok((rest, _)) = tag::<_, _, nom::error::Error<_>>(b">>")(remaining) {
            return Ok((rest, dict));
        }

        // Parse name
        if let Ok((rest, name)) = parse_name(remaining) {
            remaining = rest;

            // Skip whitespace
            if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
                remaining = rest;
            }

            // Parse value
            if let Ok((rest, value)) = parse_operand(remaining) {
                dict.insert(name, value);
                remaining = rest;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

/// Parse operator with its operands from the stack
fn parse_operator_with_operands<'a>(
    input: &'a [u8],
    operand_stack: &mut Vec<Operand>,
) -> IResult<&'a [u8], ContentOperator> {
    let (input, op_name) =
        take_while1(|c: u8| c.is_ascii_alphabetic() || c == b'*' || c == b'\'' || c == b'"')(
            input,
        )?;

    let operator = match op_name {
        // Text operators
        b"BT" => {
            operand_stack.clear();
            ContentOperator::BeginText
        }
        b"ET" => {
            operand_stack.clear();
            ContentOperator::EndText
        }
        b"Tc" => {
            let spacing = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetCharSpace(spacing)
        }
        b"Tw" => {
            let spacing = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetWordSpace(spacing)
        }
        b"Tz" => {
            let scale = pop_number(operand_stack).unwrap_or(100.0);
            ContentOperator::SetHorizontalScale(scale)
        }
        b"TL" => {
            let leading = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetLeading(leading)
        }
        b"Tf" => {
            let size = pop_number(operand_stack).unwrap_or(12.0);
            let font = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::SetFont(font, size)
        }
        b"Tr" => {
            let mode = pop_number(operand_stack).unwrap_or(0.0) as i32;
            ContentOperator::SetTextRenderMode(mode)
        }
        b"Ts" => {
            let rise = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetTextRise(rise)
        }
        b"Td" => {
            let ty = pop_number(operand_stack).unwrap_or(0.0);
            let tx = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::MoveText(tx, ty)
        }
        b"TD" => {
            let ty = pop_number(operand_stack).unwrap_or(0.0);
            let tx = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::MoveText(tx, ty)
        }
        b"Tm" => {
            let f = pop_number(operand_stack).unwrap_or(0.0);
            let e = pop_number(operand_stack).unwrap_or(0.0);
            let d = pop_number(operand_stack).unwrap_or(1.0);
            let c = pop_number(operand_stack).unwrap_or(0.0);
            let b = pop_number(operand_stack).unwrap_or(0.0);
            let a = pop_number(operand_stack).unwrap_or(1.0);
            ContentOperator::SetTextMatrix(a, b, c, d, e, f)
        }
        b"T*" => {
            operand_stack.clear();
            ContentOperator::MoveTextNextLine
        }
        b"Tj" => {
            let text = pop_string(operand_stack).unwrap_or_default();
            ContentOperator::ShowText(text)
        }
        b"TJ" => {
            let array = pop_text_array(operand_stack);
            ContentOperator::ShowTextArray(array)
        }
        b"'" => {
            let text = pop_string(operand_stack).unwrap_or_default();
            ContentOperator::ShowTextNextLine(text)
        }
        b"\"" => {
            let text = pop_string(operand_stack).unwrap_or_default();
            let tc = pop_number(operand_stack).unwrap_or(0.0);
            let tw = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::ShowTextWithSpacing(tw, tc, text)
        }

        // Graphics operators
        b"m" => {
            let y = pop_number(operand_stack).unwrap_or(0.0);
            let x = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::MoveTo(x, y)
        }
        b"l" => {
            let y = pop_number(operand_stack).unwrap_or(0.0);
            let x = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::LineTo(x, y)
        }
        b"c" => {
            let y3 = pop_number(operand_stack).unwrap_or(0.0);
            let x3 = pop_number(operand_stack).unwrap_or(0.0);
            let y2 = pop_number(operand_stack).unwrap_or(0.0);
            let x2 = pop_number(operand_stack).unwrap_or(0.0);
            let y1 = pop_number(operand_stack).unwrap_or(0.0);
            let x1 = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::CurveTo(x1, y1, x2, y2, x3, y3)
        }
        b"v" => {
            let y3 = pop_number(operand_stack).unwrap_or(0.0);
            let x3 = pop_number(operand_stack).unwrap_or(0.0);
            let y2 = pop_number(operand_stack).unwrap_or(0.0);
            let x2 = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::CurveToV(x2, y2, x3, y3)
        }
        b"y" => {
            let y3 = pop_number(operand_stack).unwrap_or(0.0);
            let x3 = pop_number(operand_stack).unwrap_or(0.0);
            let y1 = pop_number(operand_stack).unwrap_or(0.0);
            let x1 = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::CurveToY(x1, y1, x3, y3)
        }
        b"h" => {
            operand_stack.clear();
            ContentOperator::ClosePath
        }
        b"re" => {
            let h = pop_number(operand_stack).unwrap_or(0.0);
            let w = pop_number(operand_stack).unwrap_or(0.0);
            let y = pop_number(operand_stack).unwrap_or(0.0);
            let x = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::Rectangle(x, y, w, h)
        }

        // Path painting
        b"S" => {
            operand_stack.clear();
            ContentOperator::Stroke
        }
        b"s" => {
            operand_stack.clear();
            ContentOperator::CloseAndStroke
        }
        b"f" | b"F" => {
            operand_stack.clear();
            ContentOperator::Fill
        }
        b"f*" => {
            operand_stack.clear();
            ContentOperator::FillEvenOdd
        }
        b"B" => {
            operand_stack.clear();
            ContentOperator::FillAndStroke
        }
        b"B*" => {
            operand_stack.clear();
            ContentOperator::FillAndStrokeEvenOdd
        }
        b"b" => {
            operand_stack.clear();
            ContentOperator::CloseFillAndStroke
        }
        b"b*" => {
            operand_stack.clear();
            ContentOperator::CloseFillAndStrokeEvenOdd
        }
        b"n" => {
            operand_stack.clear();
            ContentOperator::EndPath
        }

        // Clipping
        b"W" => {
            operand_stack.clear();
            ContentOperator::Clip
        }
        b"W*" => {
            operand_stack.clear();
            ContentOperator::ClipEvenOdd
        }

        // Graphics state
        b"q" => {
            operand_stack.clear();
            ContentOperator::Save
        }
        b"Q" => {
            operand_stack.clear();
            ContentOperator::Restore
        }
        b"cm" => {
            let f = pop_number(operand_stack).unwrap_or(0.0);
            let e = pop_number(operand_stack).unwrap_or(0.0);
            let d = pop_number(operand_stack).unwrap_or(1.0);
            let c = pop_number(operand_stack).unwrap_or(0.0);
            let b = pop_number(operand_stack).unwrap_or(0.0);
            let a = pop_number(operand_stack).unwrap_or(1.0);
            ContentOperator::SetMatrix(a, b, c, d, e, f)
        }
        b"w" => {
            let width = pop_number(operand_stack).unwrap_or(1.0);
            ContentOperator::SetLineWidth(width)
        }
        b"J" => {
            let cap = pop_number(operand_stack).unwrap_or(0.0) as i32;
            ContentOperator::SetLineCap(cap)
        }
        b"j" => {
            let join = pop_number(operand_stack).unwrap_or(0.0) as i32;
            ContentOperator::SetLineJoin(join)
        }
        b"M" => {
            let limit = pop_number(operand_stack).unwrap_or(10.0);
            ContentOperator::SetMiterLimit(limit)
        }
        b"d" => {
            let phase = pop_number(operand_stack).unwrap_or(0.0);
            let pattern = pop_array(operand_stack);
            ContentOperator::SetDashPattern(pattern, phase)
        }
        b"ri" => {
            let intent = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::SetRenderingIntent(intent)
        }
        b"i" => {
            let flatness = pop_number(operand_stack).unwrap_or(1.0);
            ContentOperator::SetFlatness(flatness)
        }
        b"gs" => {
            let name = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::SetGraphicsStateParams(name)
        }

        // Color
        b"CS" => {
            let name = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::SetStrokingColorSpace(name)
        }
        b"cs" => {
            let name = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::SetColorSpace(name)
        }
        b"SC" | b"SCN" => {
            let mut colors = Vec::new();
            let mut pattern_name = None;

            // Check if last operand is a name (pattern)
            if let Some(Operand::Name(n)) = operand_stack.last() {
                pattern_name = Some(n.clone());
                operand_stack.pop();
            }

            // Collect color components
            while let Some(n) = pop_number(operand_stack) {
                colors.insert(0, n);
            }

            if op_name == b"SCN" {
                ContentOperator::SetStrokingColorN(colors, pattern_name)
            } else {
                ContentOperator::SetStrokingColor(colors)
            }
        }
        b"sc" | b"scn" => {
            let mut colors = Vec::new();
            let mut pattern_name = None;

            // Check if last operand is a name (pattern)
            if let Some(Operand::Name(n)) = operand_stack.last() {
                pattern_name = Some(n.clone());
                operand_stack.pop();
            }

            // Collect color components
            while let Some(n) = pop_number(operand_stack) {
                colors.insert(0, n);
            }

            if op_name == b"scn" {
                ContentOperator::SetColorN(colors, pattern_name)
            } else {
                ContentOperator::SetColor(colors)
            }
        }
        b"G" => {
            let gray = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetStrokingGrayLevel(gray)
        }
        b"g" => {
            let gray = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetGrayLevel(gray)
        }
        b"RG" => {
            let b = pop_number(operand_stack).unwrap_or(0.0);
            let g = pop_number(operand_stack).unwrap_or(0.0);
            let r = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetStrokingRGBColor(r, g, b)
        }
        b"rg" => {
            let b = pop_number(operand_stack).unwrap_or(0.0);
            let g = pop_number(operand_stack).unwrap_or(0.0);
            let r = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetRGBColor(r, g, b)
        }
        b"K" => {
            let k = pop_number(operand_stack).unwrap_or(0.0);
            let y = pop_number(operand_stack).unwrap_or(0.0);
            let m = pop_number(operand_stack).unwrap_or(0.0);
            let c = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetStrokingCMYKColor(c, m, y, k)
        }
        b"k" => {
            let k = pop_number(operand_stack).unwrap_or(0.0);
            let y = pop_number(operand_stack).unwrap_or(0.0);
            let m = pop_number(operand_stack).unwrap_or(0.0);
            let c = pop_number(operand_stack).unwrap_or(0.0);
            ContentOperator::SetCMYKColor(c, m, y, k)
        }

        // XObject
        b"Do" => {
            let name = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::PaintXObject(name)
        }

        // Shading
        b"sh" => {
            let name = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::PaintShading(name)
        }

        // Inline images
        b"BI" => {
            operand_stack.clear();
            ContentOperator::BeginInlineImage
        }

        // Marked content
        b"BMC" => {
            let tag = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::BeginMarkedContent(tag)
        }
        b"BDC" => {
            let props = if let Some(Operand::Dictionary(d)) = operand_stack.pop() {
                crate::parser::content_stream::MarkedContentProps::Dictionary(dict_to_pdf_dict(d))
            } else if let Some(Operand::Name(n)) = operand_stack.pop() {
                crate::parser::content_stream::MarkedContentProps::Name(n)
            } else {
                crate::parser::content_stream::MarkedContentProps::Name(String::new())
            };
            let tag = pop_name(operand_stack).unwrap_or_default();
            ContentOperator::BeginMarkedContentWithProps(tag, props)
        }
        b"EMC" => {
            operand_stack.clear();
            ContentOperator::EndMarkedContent
        }

        _ => {
            // Unknown operator - collect operands
            let operands: Vec<_> = operand_stack
                .drain(..)
                .map(convert_operand_to_content_stream)
                .collect();
            ContentOperator::Unknown(String::from_utf8_lossy(op_name).to_string(), operands)
        }
    };

    Ok((input, operator))
}

// Helper functions to pop operands
fn pop_number(stack: &mut Vec<Operand>) -> Option<f64> {
    stack.pop().and_then(|op| op.as_number())
}

fn pop_name(stack: &mut Vec<Operand>) -> Option<String> {
    stack.pop().and_then(|op| match op {
        Operand::Name(n) => Some(n),
        _ => None,
    })
}

fn pop_string(stack: &mut Vec<Operand>) -> Option<Vec<u8>> {
    stack.pop().and_then(|op| match op {
        Operand::String(s) => Some(s),
        _ => None,
    })
}

fn pop_array(stack: &mut Vec<Operand>) -> Vec<f64> {
    if let Some(Operand::Array(arr)) = stack.pop() {
        arr.into_iter().filter_map(|op| op.as_number()).collect()
    } else {
        Vec::new()
    }
}

fn pop_text_array(stack: &mut Vec<Operand>) -> Vec<TextArrayElement> {
    if let Some(Operand::Array(arr)) = stack.pop() {
        arr.into_iter()
            .map(|op| match op {
                Operand::String(s) => TextArrayElement::Text(s),
                Operand::Integer(i) => TextArrayElement::Spacing(i as f64),
                Operand::Real(r) => TextArrayElement::Spacing(r),
                _ => TextArrayElement::Spacing(0.0),
            })
            .collect()
    } else {
        Vec::new()
    }
}

fn dict_to_pdf_dict(dict: HashMap<String, Operand>) -> crate::types::PdfDictionary {
    let mut pdf_dict = crate::types::PdfDictionary::new();
    for (key, value) in dict {
        pdf_dict.insert(key, operand_to_pdf_value(value));
    }
    pdf_dict
}

fn operand_to_pdf_value(op: Operand) -> crate::types::PdfValue {
    match op {
        Operand::Integer(i) => crate::types::PdfValue::Integer(i),
        Operand::Real(r) => crate::types::PdfValue::Real(r),
        Operand::String(s) => {
            crate::types::PdfValue::String(crate::types::primitive::PdfString::new_literal(s))
        }
        Operand::Name(n) => crate::types::PdfValue::Name(crate::types::primitive::PdfName::new(n)),
        Operand::Boolean(b) => crate::types::PdfValue::Boolean(b),
        Operand::Null => crate::types::PdfValue::Null,
        Operand::Array(arr) => {
            let pdf_arr: Vec<_> = arr.into_iter().map(operand_to_pdf_value).collect();
            crate::types::PdfValue::Array(crate::types::object::PdfArray::from(pdf_arr))
        }
        Operand::Dictionary(dict) => crate::types::PdfValue::Dictionary(dict_to_pdf_dict(dict)),
    }
}

/// Parse inline image with dictionary and data
pub fn parse_inline_image(input: &[u8]) -> IResult<&[u8], InlineImageInfo> {
    parse_inline_image_with_budget(input, &ResourceBudget::default())
}

pub fn parse_inline_image_with_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> IResult<&'a [u8], InlineImageInfo> {
    if budget.consume_input(input.len() as u64).is_err() {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        )));
    }

    parse_inline_image_impl(input, budget)
}

pub(crate) fn parse_inline_image_after_input_budget<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> IResult<&'a [u8], InlineImageInfo> {
    parse_inline_image_impl(input, budget)
}

fn parse_inline_image_impl<'a>(
    input: &'a [u8],
    budget: &ResourceBudget,
) -> IResult<&'a [u8], InlineImageInfo> {
    // Skip BI
    let (input, _) = tag(b"BI")(input)?;
    let (input, _) = multispace0(input)?;

    // Parse inline image dictionary
    let mut dict = HashMap::new();
    let mut remaining = input;

    loop {
        // Skip whitespace
        if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
            remaining = rest;
        }

        // Check for ID (start of image data)
        if remaining.starts_with(b"ID") && remaining.len() > 2 && remaining[2].is_ascii_whitespace()
        {
            remaining = &remaining[3..]; // Skip "ID" and whitespace
            break;
        }

        // Parse abbreviated name
        if let Ok((rest, abbrev)) = parse_inline_image_key(remaining) {
            remaining = rest;

            // Skip whitespace
            if let Ok((rest, _)) = multispace0::<_, nom::error::Error<_>>(remaining) {
                remaining = rest;
            }

            // Parse value
            if let Ok((rest, value)) = parse_operand(remaining) {
                dict.insert(expand_inline_image_key(&abbrev), value);
                remaining = rest;
            }
        } else {
            break;
        }
    }

    // Find EI to determine data length
    let mut data_end = 0;
    for i in 0..remaining.len() {
        if remaining[i..].starts_with(b"EI") {
            // Check if EI is followed by whitespace or end
            if i + 2 >= remaining.len() || remaining[i + 2].is_ascii_whitespace() {
                data_end = i;
                break;
            }
        }
    }

    if budget.consume_decoded(data_end as u64).is_err() {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        )));
    }
    let data = remaining[..data_end].to_vec();
    let remaining = &remaining[data_end..];

    // Skip EI
    let (remaining, _) = tag(b"EI")(remaining)?;

    // Build InlineImageInfo
    let width = dict.get("Width").and_then(|v| v.as_number()).unwrap_or(1.0) as u32;

    let height = dict
        .get("Height")
        .and_then(|v| v.as_number())
        .unwrap_or(1.0) as u32;

    let color_space = dict
        .get("ColorSpace")
        .and_then(|v| v.as_name())
        .unwrap_or("DeviceGray")
        .to_string();

    let bits_per_component = dict
        .get("BitsPerComponent")
        .and_then(|v| v.as_number())
        .unwrap_or(8.0) as u8;

    let filter = dict
        .get("Filter")
        .and_then(|v| v.as_name())
        .map(|s| s.to_string());

    let decode_params = if dict.contains_key("DecodeParms") {
        let mut params = HashMap::new();
        if let Some(Operand::Dictionary(d)) = dict.get("DecodeParms") {
            for (k, v) in d {
                params.insert(
                    k.clone(),
                    pdf_value_to_content_operand(operand_to_pdf_value(v.clone())),
                );
            }
        }
        Some(params)
    } else {
        None
    };

    Ok((
        remaining,
        InlineImageInfo {
            width,
            height,
            color_space,
            bits_per_component,
            filter,
            decode_params,
            data,
        },
    ))
}

fn parse_inline_image_key(input: &[u8]) -> IResult<&[u8], String> {
    let (input, key) = take_while1(|c: u8| c.is_ascii_alphabetic())(input)?;
    Ok((input, String::from_utf8_lossy(key).to_string()))
}

fn expand_inline_image_key(abbrev: &str) -> String {
    match abbrev {
        "BPC" => "BitsPerComponent",
        "CS" => "ColorSpace",
        "D" => "Decode",
        "DP" => "DecodeParms",
        "F" => "Filter",
        "H" => "Height",
        "IM" => "ImageMask",
        "I" => "Interpolate",
        "W" => "Width",
        _ => abbrev,
    }
    .to_string()
}

fn convert_operand_to_content_stream(op: Operand) -> crate::parser::content_stream::Operand {
    match op {
        Operand::Integer(i) => crate::parser::content_stream::Operand::Integer(i),
        Operand::Real(r) => crate::parser::content_stream::Operand::Real(r),
        Operand::String(s) => crate::parser::content_stream::Operand::String(s),
        Operand::Name(n) => crate::parser::content_stream::Operand::Name(n),
        Operand::Boolean(b) => {
            // Convert boolean to integer (0 or 1)
            crate::parser::content_stream::Operand::Integer(if b { 1 } else { 0 })
        }
        Operand::Null => {
            // Convert null to integer 0
            crate::parser::content_stream::Operand::Integer(0)
        }
        Operand::Array(arr) => crate::parser::content_stream::Operand::Array(
            arr.into_iter()
                .map(convert_operand_to_content_stream)
                .collect(),
        ),
        Operand::Dictionary(dict) => crate::parser::content_stream::Operand::Dictionary(
            dict.into_iter()
                .map(|(k, v)| (k, convert_operand_to_content_stream(v)))
                .collect(),
        ),
    }
}

fn pdf_value_to_content_operand(
    val: crate::types::PdfValue,
) -> crate::parser::content_stream::Operand {
    match val {
        crate::types::PdfValue::Integer(i) => crate::parser::content_stream::Operand::Integer(i),
        crate::types::PdfValue::Real(r) => crate::parser::content_stream::Operand::Real(r),
        crate::types::PdfValue::String(s) => {
            crate::parser::content_stream::Operand::String(s.as_bytes().to_vec())
        }
        crate::types::PdfValue::Name(n) => {
            crate::parser::content_stream::Operand::Name(n.without_slash().to_string())
        }
        crate::types::PdfValue::Boolean(value) => {
            crate::parser::content_stream::Operand::Integer(i64::from(value))
        }
        crate::types::PdfValue::Null => crate::parser::content_stream::Operand::Integer(0), // Simplified
        crate::types::PdfValue::Array(arr) => {
            let operands: Vec<_> = arr
                .iter()
                .map(|v| pdf_value_to_content_operand(v.clone()))
                .collect();
            crate::parser::content_stream::Operand::Array(operands)
        }
        _ => crate::parser::content_stream::Operand::Integer(0), // Default for unsupported types
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_content_stream_strict_with_budget, parse_inline_image_with_budget,
        pdf_value_to_content_operand,
    };
    use crate::performance::ResourceBudget;
    use crate::types::PdfValue;

    #[test]
    fn inline_image_parser_respects_input_budget() {
        let budget = ResourceBudget::new(3, 1024, 1024, 10, 10, 10, 10, 8);
        assert!(parse_inline_image_with_budget(b"BI ID x EI", &budget).is_err());
    }

    #[test]
    fn strict_content_parser_rejects_residual_tokens() {
        let budget = ResourceBudget::default();
        assert!(parse_content_stream_strict_with_budget(b"1 0 m", &budget).is_ok());
        assert!(parse_content_stream_strict_with_budget(b"1", &budget).is_err());
        assert!(parse_content_stream_strict_with_budget(b"q @", &budget).is_err());
        assert!(parse_content_stream_strict_with_budget(
            b"BI /W 1 /H 1 /BPC 8 /CS /G ID 00 EI",
            &budget
        )
        .is_ok());
    }

    #[test]
    fn pdf_boolean_operand_preserves_false_value() {
        assert_eq!(
            pdf_value_to_content_operand(PdfValue::Boolean(false)),
            crate::parser::content_stream::Operand::Integer(0)
        );
        assert_eq!(
            pdf_value_to_content_operand(PdfValue::Boolean(true)),
            crate::parser::content_stream::Operand::Integer(1)
        );
    }
}
