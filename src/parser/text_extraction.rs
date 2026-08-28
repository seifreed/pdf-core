use crate::ast::{NodeId, PdfAstGraph};
use crate::parser::cmap::{CMap, CMapParser};
use crate::parser::content_stream::ContentOperator;
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfArray, PdfDictionary, PdfValue};
use std::collections::HashMap;

/// Text extraction state machine
#[allow(dead_code)]
pub struct TextExtractor<'a> {
    ast: &'a PdfAstGraph,
    page_resources: &'a PdfDictionary,
    fonts: HashMap<String, FontInfo>,
    cmaps: HashMap<String, CMap>,
    text_spans: Vec<TextSpan>,
    graphics_state: GraphicsState,
    text_state: TextState,
}

#[derive(Debug, Clone)]
pub struct FontInfo {
    pub font_type: String,
    pub base_font: String,
    pub encoding: String,
    pub differences: HashMap<u8, String>,
    pub to_unicode: Option<NodeId>,
    pub width_map: HashMap<u32, f64>,
    pub default_width: f64,
    pub font_matrix: [f64; 6],
}

#[derive(Debug, Clone)]
pub struct GraphicsState {
    pub ctm: [f64; 6], // Current Transformation Matrix
    pub text_matrix: [f64; 6],
    pub text_line_matrix: [f64; 6],
    pub leading: f64,
    pub char_space: f64,
    pub word_space: f64,
    pub horizontal_scale: f64,
    pub text_rise: f64,
    pub font: Option<String>,
    pub font_size: f64,
    pub render_mode: i32,
}

#[derive(Debug, Clone)]
pub struct TextState {
    pub current_font: Option<FontInfo>,
    pub current_cmap: Option<CMap>,
}

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_name: String,
    pub font_size: f64,
    pub space_width: f64,
    pub chars: Vec<CharInfo>,
}

#[derive(Debug, Clone)]
pub struct CharInfo {
    pub unicode: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl<'a> TextExtractor<'a> {
    pub fn new(ast: &'a PdfAstGraph, page_resources: &'a PdfDictionary) -> Self {
        TextExtractor {
            ast,
            page_resources,
            fonts: HashMap::new(),
            cmaps: HashMap::new(),
            text_spans: Vec::new(),
            graphics_state: GraphicsState::default(),
            text_state: TextState {
                current_font: None,
                current_cmap: None,
            },
        }
    }

    pub fn extract_text(&mut self, operators: &[ContentOperator]) -> Vec<TextSpan> {
        self.extract_text_with_budget(operators, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn extract_text_with_budget(
        &mut self,
        operators: &[ContentOperator],
        budget: &ResourceBudget,
    ) -> Result<Vec<TextSpan>, ResourceBudgetError> {
        // Pre-load fonts from resources
        self.load_fonts_with_budget(budget)?;

        // Process operators
        for op in operators {
            budget.consume_node()?;
            self.process_operator_with_budget(op, budget)?;
        }

        // Sort spans by position
        self.text_spans
            .sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));

        for span in &self.text_spans {
            budget.consume_decoded(span.text.len() as u64)?;
        }

        Ok(self.text_spans.clone())
    }

    fn load_fonts_with_budget(
        &mut self,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        let Some(PdfValue::Dictionary(fonts)) = self.page_resources.get("Font") else {
            return Ok(());
        };
        let fonts: Vec<(String, PdfValue)> = fonts
            .iter()
            .map(|(name, value)| (name.without_slash().to_string(), value.clone()))
            .collect();
        for (name, font_value) in fonts {
            budget.consume_node()?;
            let font_info = self.parse_font_info(&name, &font_value, budget);
            self.fonts.insert(name, font_info);
        }
        Ok(())
    }

    fn resolve_dict(&self, value: &PdfValue) -> Option<PdfDictionary> {
        match value {
            PdfValue::Dictionary(dict) => Some(dict.clone()),
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_dict())
                .cloned(),
            _ => None,
        }
    }

    fn resolve_stream(&self, value: &PdfValue) -> Option<crate::types::PdfStream> {
        match value {
            PdfValue::Stream(stream) => Some(stream.clone()),
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_stream())
                .cloned(),
            _ => None,
        }
    }

    fn resolve_array(&self, value: &PdfValue) -> Option<PdfArray> {
        match value {
            PdfValue::Array(array) => Some(array.clone()),
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_array())
                .cloned(),
            _ => None,
        }
    }

    fn resolve_integer(&self, value: &PdfValue) -> Option<i64> {
        match value {
            PdfValue::Integer(integer) => Some(*integer),
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.value.as_integer()),
            _ => None,
        }
    }

    fn resolve_number(&self, value: Option<&PdfValue>) -> Option<f64> {
        let value = value?;
        match value {
            PdfValue::Integer(_) | PdfValue::Real(_) => value.as_real(),
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.value.as_real()),
            _ => None,
        }
    }

    fn effective_value<'b>(
        primary: &'b PdfDictionary,
        fallback: &'b PdfDictionary,
        key: &str,
    ) -> Option<&'b PdfValue> {
        primary.get(key).or_else(|| fallback.get(key))
    }

    fn parse_font_info(
        &mut self,
        name: &str,
        font_value: &PdfValue,
        budget: &ResourceBudget,
    ) -> FontInfo {
        let top_dict = self.resolve_dict(font_value).unwrap_or_default();
        let descendant_dict = top_dict
            .get("DescendantFonts")
            .and_then(|value| self.resolve_array(value))
            .and_then(|fonts| fonts.into_iter().next())
            .and_then(|value| self.resolve_dict(&value))
            .unwrap_or_default();
        let effective_dict = if descendant_dict.is_empty() {
            &top_dict
        } else {
            &descendant_dict
        };

        let font_type = top_dict
            .get("Subtype")
            .and_then(PdfValue::as_name)
            .map(|value| value.without_slash().to_string())
            .or_else(|| {
                effective_dict
                    .get("Subtype")
                    .and_then(PdfValue::as_name)
                    .map(|value| value.without_slash().to_string())
            })
            .unwrap_or_else(|| "Unknown".to_string());
        let base_font = top_dict
            .get("BaseFont")
            .or_else(|| effective_dict.get("BaseFont"))
            .and_then(PdfValue::as_name)
            .map(|value| value.without_slash().to_string())
            .unwrap_or_else(|| name.to_string());
        let (encoding, differences) = match top_dict.get("Encoding") {
            Some(PdfValue::Name(value)) => (value.without_slash().to_string(), HashMap::new()),
            Some(value) => {
                let dict = self.resolve_dict(value).unwrap_or_default();
                let encoding = dict
                    .get("BaseEncoding")
                    .and_then(PdfValue::as_name)
                    .map(|value| value.without_slash().to_string())
                    .unwrap_or_else(|| "StandardEncoding".to_string());
                (encoding, Self::parse_differences(dict.get("Differences")))
            }
            None => ("StandardEncoding".to_string(), HashMap::new()),
        };

        let mut width_map = HashMap::new();
        self.parse_simple_widths(&top_dict, &mut width_map);
        if !descendant_dict.is_empty() {
            self.parse_cid_widths(&descendant_dict, &mut width_map);
        }
        let default_width = self
            .resolve_number(Self::effective_value(&top_dict, effective_dict, "DW"))
            .unwrap_or(1000.0);
        let font_matrix = Self::parse_font_matrix(Self::effective_value(
            &top_dict,
            effective_dict,
            "FontMatrix",
        ))
        .unwrap_or([0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);

        let to_unicode = top_dict.get("ToUnicode").and_then(|value| match value {
            PdfValue::Reference(reference) => self
                .ast
                .get_node_by_object(reference.id())
                .map(|node| node.id),
            _ => None,
        });
        if let Some(to_unicode_value) = top_dict.get("ToUnicode") {
            if let Some(stream) = self.resolve_stream(to_unicode_value) {
                let mut cmap_ast = PdfAstGraph::new();
                let resolver = ObjectNodeMap::new();
                if let Some((_, cmap)) =
                    CMapParser::new_with_budget(&mut cmap_ast, &resolver, budget)
                        .parse_cmap_stream(&stream)
                {
                    self.cmaps.insert(name.to_string(), cmap);
                }
            }
        }

        FontInfo {
            font_type,
            base_font,
            encoding,
            differences,
            to_unicode,
            width_map,
            default_width,
            font_matrix,
        }
    }

    fn number_value(value: Option<&PdfValue>) -> Option<f64> {
        value.and_then(PdfValue::as_real)
    }

    fn parse_simple_widths(&self, dict: &PdfDictionary, width_map: &mut HashMap<u32, f64>) {
        let Some(first_char) = dict
            .get("FirstChar")
            .and_then(|value| self.resolve_integer(value))
        else {
            return;
        };
        let Some(widths) = dict
            .get("Widths")
            .and_then(|value| self.resolve_array(value))
        else {
            return;
        };
        for (index, width) in widths.iter().enumerate() {
            let Ok(code) = u32::try_from(first_char.saturating_add(index as i64)) else {
                continue;
            };
            if let Some(width) = Self::number_value(Some(width)) {
                width_map.insert(code, width);
            }
        }
    }

    fn parse_cid_widths(&self, dict: &PdfDictionary, width_map: &mut HashMap<u32, f64>) {
        let Some(widths) = dict.get("W").and_then(|value| self.resolve_array(value)) else {
            return;
        };
        let mut index = 0;
        while index < widths.len() {
            let Some(start) = widths[index]
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
            else {
                index += 1;
                continue;
            };
            let Some(next) = widths.get(index + 1) else {
                break;
            };
            if let Some(values) = next.as_array() {
                for (offset, width) in values.iter().enumerate() {
                    if let Some(width) = Self::number_value(Some(width)) {
                        if let Some(code) = start.checked_add(offset as u32) {
                            width_map.insert(code, width);
                        }
                    }
                }
                index += 2;
            } else if let (Some(end), Some(width)) = (
                next.as_integer()
                    .and_then(|value| u32::try_from(value).ok()),
                widths
                    .get(index + 2)
                    .and_then(|value| Self::number_value(Some(value))),
            ) {
                for code in start..=end {
                    width_map.insert(code, width);
                }
                index += 3;
            } else {
                index += 1;
            }
        }
    }

    fn parse_font_matrix(value: Option<&PdfValue>) -> Option<[f64; 6]> {
        let PdfValue::Array(values) = value? else {
            return None;
        };
        let numbers: Vec<f64> = values.iter().filter_map(PdfValue::as_real).collect();
        (numbers.len() == 6)
            .then(|| numbers.try_into().ok())
            .flatten()
    }

    fn parse_differences(value: Option<&PdfValue>) -> HashMap<u8, String> {
        let Some(PdfValue::Array(values)) = value else {
            return HashMap::new();
        };
        let mut code = None;
        let mut differences = HashMap::new();
        for value in values {
            match value {
                PdfValue::Integer(value) => code = u8::try_from(*value).ok(),
                PdfValue::Name(name) => {
                    if let Some(current_code) = code {
                        if let Some(unicode) = Self::glyph_name_to_unicode(name.without_slash()) {
                            differences.insert(current_code, unicode);
                            code = current_code.checked_add(1);
                        }
                    }
                }
                _ => {}
            }
        }
        differences
    }

    fn glyph_name_to_unicode(name: &str) -> Option<String> {
        let unicode = match name {
            "space" => " ",
            "nbspace" | "nonbreakingspace" => "\u{00a0}",
            "exclam" => "!",
            "quotedbl" => "\"",
            "numbersign" => "#",
            "dollar" => "$",
            "percent" => "%",
            "ampersand" => "&",
            "quotesingle" => "'",
            "parenleft" => "(",
            "parenright" => ")",
            "asterisk" => "*",
            "plus" => "+",
            "comma" => ",",
            "hyphen" => "-",
            "period" => ".",
            "slash" => "/",
            "colon" => ":",
            "semicolon" => ";",
            "less" => "<",
            "equal" => "=",
            "greater" => ">",
            "question" => "?",
            "at" => "@",
            "bracketleft" => "[",
            "backslash" => "\\",
            "bracketright" => "]",
            "asciicircum" => "^",
            "underscore" => "_",
            "grave" => "`",
            "braceleft" => "{",
            "bar" => "|",
            "braceright" => "}",
            "asciitilde" => "~",
            "Euro" => "€",
            "bullet" => "•",
            "endash" => "–",
            "emdash" => "—",
            "Aacute" => "Á",
            "aacute" => "á",
            "Adieresis" => "Ä",
            "adieresis" => "ä",
            "Ntilde" => "Ñ",
            "ntilde" => "ñ",
            "Oslash" => "Ø",
            "oslash" => "ø",
            "fi" => "ﬁ",
            "fl" => "ﬂ",
            _ if name.chars().count() == 1 && name.is_ascii() => name,
            _ => return None,
        };
        Some(unicode.to_string())
    }

    fn process_operator_with_budget(
        &mut self,
        op: &ContentOperator,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        match op {
            ContentOperator::BeginText => {
                self.graphics_state.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                self.graphics_state.text_line_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }

            ContentOperator::EndText => {
                // Reset text state
            }

            ContentOperator::SetFont(name, size) => {
                self.graphics_state.font = Some(name.clone());
                self.graphics_state.font_size = *size;

                // Update current font
                let resource_name = name.trim_start_matches('/');
                if let Some(font_info) = self.fonts.get(resource_name) {
                    self.text_state.current_font = Some(font_info.clone());
                    self.text_state.current_cmap = self.cmaps.get(resource_name).cloned();
                }
            }

            ContentOperator::SetCharSpace(spacing) => {
                self.graphics_state.char_space = *spacing;
            }

            ContentOperator::SetWordSpace(spacing) => {
                self.graphics_state.word_space = *spacing;
            }

            ContentOperator::SetHorizontalScale(scale) => {
                self.graphics_state.horizontal_scale = *scale;
            }

            ContentOperator::SetLeading(leading) => {
                self.graphics_state.leading = *leading;
            }

            ContentOperator::SetTextRise(rise) => {
                self.graphics_state.text_rise = *rise;
            }

            ContentOperator::MoveText(tx, ty) => {
                let tm = &mut self.graphics_state.text_line_matrix;
                tm[4] += tx;
                tm[5] += ty;
                self.graphics_state.text_matrix = *tm;
            }

            ContentOperator::MoveTextNextLine => {
                let leading = self.graphics_state.leading;
                self.process_operator_with_budget(
                    &ContentOperator::MoveText(0.0, -leading),
                    budget,
                )?;
            }

            ContentOperator::SetTextMatrix(a, b, c, d, e, f) => {
                self.graphics_state.text_matrix = [*a, *b, *c, *d, *e, *f];
                self.graphics_state.text_line_matrix = [*a, *b, *c, *d, *e, *f];
            }

            ContentOperator::ShowText(text) => {
                self.show_text_with_budget(text, budget)?;
            }

            ContentOperator::ShowTextArray(array) => {
                for element in array {
                    match element {
                        crate::parser::content_stream::TextArrayElement::Text(text) => {
                            self.show_text_with_budget(text, budget)?;
                        }
                        crate::parser::content_stream::TextArrayElement::Spacing(spacing) => {
                            // Adjust text matrix by spacing
                            let adj = -spacing / 1000.0
                                * self.graphics_state.font_size
                                * self.graphics_state.horizontal_scale
                                / 100.0;
                            self.graphics_state.text_matrix[4] -= adj;
                        }
                    }
                }
            }

            ContentOperator::ShowTextNextLine(text) => {
                self.process_operator_with_budget(&ContentOperator::MoveTextNextLine, budget)?;
                self.show_text_with_budget(text, budget)?;
            }

            ContentOperator::ShowTextWithSpacing(tw, tc, text) => {
                self.graphics_state.word_space = *tw;
                self.graphics_state.char_space = *tc;
                self.process_operator_with_budget(&ContentOperator::MoveTextNextLine, budget)?;
                self.show_text_with_budget(text, budget)?;
            }

            ContentOperator::Save => {
                // Push graphics state
            }

            ContentOperator::Restore => {
                // Pop graphics state
            }

            ContentOperator::SetMatrix(a, b, c, d, e, f) => {
                self.graphics_state.ctm = [*a, *b, *c, *d, *e, *f];
            }

            _ => {
                // Other operators don't affect text extraction
            }
        }
        Ok(())
    }

    fn show_text_with_budget(
        &mut self,
        text_bytes: &[u8],
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        if self.text_state.current_font.is_none() {
            return Ok(());
        }

        let Some(font) = self.text_state.current_font.as_ref() else {
            return Ok(());
        };
        budget.consume_node()?;
        let mut chars = Vec::new();
        let mut total_width = 0.0;

        // Decode text using font encoding/ToUnicode
        let decoded = self.decode_text_with_budget(text_bytes, font, budget)?;

        // Calculate position for each character
        let tm = &self.graphics_state.text_matrix;
        let ctm = &self.graphics_state.ctm;

        // Transform text space to device space
        let (x, y) = self.transform_point(0.0, 0.0, tm, ctm);

        for ch in decoded.chars() {
            let char_width = self.get_char_width(ch, font);

            let char_info = CharInfo {
                unicode: ch.to_string(),
                x: x + total_width,
                y,
                width: char_width * self.graphics_state.font_size,
                height: self.graphics_state.font_size,
            };

            chars.push(char_info);

            // Update position
            total_width += char_width * self.graphics_state.font_size;
            total_width += self.graphics_state.char_space;

            if ch == ' ' {
                total_width += self.graphics_state.word_space;
            }
        }

        // Update text matrix
        self.graphics_state.text_matrix[4] += total_width;

        // Create text span
        if !chars.is_empty() {
            let span = TextSpan {
                text: decoded,
                x,
                y,
                width: total_width,
                height: self.graphics_state.font_size,
                font_name: self.graphics_state.font.clone().unwrap_or_default(),
                font_size: self.graphics_state.font_size,
                space_width: self.get_char_width(' ', font) * self.graphics_state.font_size,
                chars,
            };

            self.text_spans.push(span);
        }
        Ok(())
    }

    fn decode_text_with_budget(
        &self,
        text_bytes: &[u8],
        font: &FontInfo,
        budget: &ResourceBudget,
    ) -> Result<String, ResourceBudgetError> {
        // Try ToUnicode CMap first
        if let Some(cmap) = &self.text_state.current_cmap {
            return self.decode_with_cmap_with_budget(text_bytes, cmap, budget);
        }

        // Fallback to encoding
        budget.consume_input(text_bytes.len() as u64)?;
        let decoded = text_bytes
            .iter()
            .map(|byte| {
                font.differences.get(byte).cloned().unwrap_or_else(|| {
                    match font.encoding.as_str() {
                        "WinAnsiEncoding" => self.decode_win_ansi_byte(*byte).to_string(),
                        "MacRomanEncoding" => self.decode_mac_roman_byte(*byte).to_string(),
                        _ => (*byte as char).to_string(),
                    }
                })
            })
            .collect::<String>();
        budget.consume_decoded(decoded.len() as u64)?;
        Ok(decoded)
    }

    fn decode_with_cmap_with_budget(
        &self,
        text_bytes: &[u8],
        cmap: &CMap,
        budget: &ResourceBudget,
    ) -> Result<String, ResourceBudgetError> {
        let mut ast = PdfAstGraph::new();
        let resolver = crate::parser::reference_resolver::ObjectNodeMap::new();
        CMapParser::new_with_budget(&mut ast, &resolver, budget)
            .decode_bytes_with_budget(cmap, text_bytes, budget)
    }

    fn decode_win_ansi_byte(&self, byte: u8) -> char {
        if byte < 128 {
            return byte as char;
        }
        match byte {
            0x80 => '€',
            0x82 => '‚',
            0x83 => 'ƒ',
            0x84 => '„',
            0x85 => '…',
            0x86 => '†',
            0x87 => '‡',
            0x88 => 'ˆ',
            0x89 => '‰',
            0x8A => 'Š',
            0x8B => '‹',
            0x8C => 'Œ',
            0x8E => 'Ž',
            0x91 => '\'',
            0x92 => '\'',
            0x93 => '"',
            0x94 => '"',
            0x95 => '•',
            0x96 => '–',
            0x97 => '—',
            0x98 => '˜',
            0x99 => '™',
            0x9A => 'š',
            0x9B => '›',
            0x9C => 'œ',
            0x9E => 'ž',
            0x9F => 'Ÿ',
            _ => byte as char,
        }
    }

    fn decode_mac_roman_byte(&self, byte: u8) -> char {
        const MAC_ROMAN_HIGH: &str =
            "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»…\u{00a0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{f8ff}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
        if byte < 0x80 {
            byte as char
        } else {
            MAC_ROMAN_HIGH
                .chars()
                .nth(usize::from(byte - 0x80))
                .unwrap_or('\u{fffd}')
        }
    }

    fn get_char_width(&self, ch: char, font: &FontInfo) -> f64 {
        let code = font
            .width_map
            .contains_key(&(ch as u32))
            .then_some(ch as u32)
            .or_else(|| {
                font.differences.iter().find_map(|(code, unicode)| {
                    (unicode.chars().count() == 1 && unicode.starts_with(ch))
                        .then_some(u32::from(*code))
                })
            })
            .or_else(|| {
                (0..=u8::MAX).find_map(|code| {
                    let decoded = match font.encoding.as_str() {
                        "WinAnsiEncoding" => self.decode_win_ansi_byte(code),
                        "MacRomanEncoding" => self.decode_mac_roman_byte(code),
                        _ => code as char,
                    };
                    (decoded == ch).then_some(u32::from(code))
                })
            })
            .unwrap_or(ch as u32);
        font.width_map
            .get(&code)
            .copied()
            .unwrap_or(font.default_width)
            * font.font_matrix[0]
    }

    fn transform_point(&self, x: f64, y: f64, tm: &[f64; 6], ctm: &[f64; 6]) -> (f64, f64) {
        // Apply text matrix
        let tx = tm[0] * x + tm[2] * y + tm[4];
        let ty = tm[1] * x + tm[3] * y + tm[5];

        // Apply CTM
        let dx = ctm[0] * tx + ctm[2] * ty + ctm[4];
        let dy = ctm[1] * tx + ctm[3] * ty + ctm[5];

        (dx, dy)
    }

    pub fn merge_spans(&mut self) -> Vec<TextLine> {
        let mut lines = Vec::new();
        let mut current_line = TextLine::new();

        for span in &self.text_spans {
            if current_line.should_add_span(span) {
                current_line.add_span(span.clone());
            } else {
                if !current_line.spans.is_empty() {
                    lines.push(current_line);
                }
                current_line = TextLine::new();
                current_line.add_span(span.clone());
            }
        }

        if !current_line.spans.is_empty() {
            lines.push(current_line);
        }

        lines
    }
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            leading: 0.0,
            char_space: 0.0,
            word_space: 0.0,
            horizontal_scale: 100.0,
            text_rise: 0.0,
            font: None,
            font_size: 12.0,
            render_mode: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub spans: Vec<TextSpan>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for TextLine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLine {
    pub fn new() -> Self {
        TextLine {
            spans: Vec::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }

    pub fn should_add_span(&self, span: &TextSpan) -> bool {
        if self.spans.is_empty() {
            return true;
        }

        let last = &self.spans[self.spans.len() - 1];

        // Check if on same line (within tolerance)
        let y_diff = (span.y - last.y).abs();
        if y_diff > last.height * 0.3 {
            return false;
        }

        // Check horizontal distance
        let expected_x = last.x + last.width;
        let x_diff = span.x - expected_x;

        // Allow reasonable spacing
        x_diff < last.space_width * 3.0
    }

    pub fn add_span(&mut self, span: TextSpan) {
        if self.spans.is_empty() {
            self.x = span.x;
            self.y = span.y;
            self.height = span.height;
        }

        self.width = (span.x + span.width) - self.x;
        self.spans.push(span);
    }

    pub fn get_text(&self) -> String {
        self.spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNode, NodeType};
    use crate::types::{ObjectId, PdfArray, PdfName, PdfStream};

    #[test]
    fn extracts_indirect_font_with_tounicode_and_widths() {
        let cmap = b"begincmap\n/CMapName /Test def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n1 beginbfchar\n<41> <0041>\nendbfchar\nendcmap";

        let mut ast = PdfAstGraph::new();
        let mut font = PdfDictionary::new();
        font.insert("Subtype", PdfValue::Name(PdfName::new("Type0")));
        font.insert("BaseFont", PdfValue::Name(PdfName::new("Test")));
        font.insert("Encoding", PdfValue::Name(PdfName::new("Identity-H")));
        font.insert(
            "ToUnicode",
            PdfValue::Reference(crate::types::PdfReference::new(2, 0)),
        );
        font.insert(
            "DescendantFonts",
            PdfValue::Reference(crate::types::PdfReference::new(4, 0)),
        );
        ast.add_node(AstNode::new(
            NodeId::new(1),
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Dictionary(font),
        ));

        let mut cmap_dict = PdfDictionary::new();
        cmap_dict.insert("Length", PdfValue::Integer(cmap.len() as i64));
        ast.add_node(AstNode::new(
            NodeId::new(2),
            NodeType::Object(ObjectId::new(2, 0)),
            PdfValue::Stream(PdfStream::new(cmap_dict, cmap.to_vec())),
        ));

        let mut descendant = PdfDictionary::new();
        descendant.insert("Subtype", PdfValue::Name(PdfName::new("CIDFontType0")));
        descendant.insert(
            "DW",
            PdfValue::Reference(crate::types::PdfReference::new(6, 0)),
        );
        descendant.insert(
            "W",
            PdfValue::Reference(crate::types::PdfReference::new(5, 0)),
        );
        ast.add_node(AstNode::new(
            NodeId::new(5),
            NodeType::Object(ObjectId::new(5, 0)),
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(65),
                PdfValue::Array(PdfArray::from(vec![PdfValue::Integer(600)])),
            ])),
        ));
        ast.add_node(AstNode::new(
            NodeId::new(3),
            NodeType::Object(ObjectId::new(3, 0)),
            PdfValue::Dictionary(descendant),
        ));
        ast.add_node(AstNode::new(
            NodeId::new(4),
            NodeType::Object(ObjectId::new(4, 0)),
            PdfValue::Array(PdfArray::from(vec![PdfValue::Reference(
                crate::types::PdfReference::new(3, 0),
            )])),
        ));
        ast.add_node(AstNode::new(
            NodeId::new(6),
            NodeType::Object(ObjectId::new(6, 0)),
            PdfValue::Integer(500),
        ));

        let mut fonts = PdfDictionary::new();
        fonts.insert(
            "F1",
            PdfValue::Reference(crate::types::PdfReference::new(1, 0)),
        );
        let mut resources = PdfDictionary::new();
        resources.insert("Font", PdfValue::Dictionary(fonts));

        let operators = [
            ContentOperator::BeginText,
            ContentOperator::SetFont("/F1".to_string(), 10.0),
            ContentOperator::ShowText(vec![0x41, 0x42]),
            ContentOperator::EndText,
        ];
        let spans = TextExtractor::new(&ast, &resources).extract_text(&operators);

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "AB");
        assert!((spans[0].width - 11.0).abs() < f64::EPSILON);
    }

    #[test]
    fn text_extraction_charges_operator_traversal() {
        let ast = PdfAstGraph::new();
        let resources = PdfDictionary::new();
        let operators = [ContentOperator::BeginText, ContentOperator::EndText];
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);

        let error = TextExtractor::new(&ast, &resources)
            .extract_text_with_budget(&operators, &budget)
            .expect_err("operator traversal must respect node budget");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }

    #[test]
    fn decodes_mac_roman_bytes() {
        let ast = PdfAstGraph::new();
        let resources = PdfDictionary::new();
        let extractor = TextExtractor::new(&ast, &resources);

        let decoded: String = [0x8e, 0xdb, 0xff]
            .iter()
            .map(|byte| extractor.decode_mac_roman_byte(*byte))
            .collect();
        assert_eq!(decoded, "é€ˇ");
    }

    #[test]
    fn applies_simple_font_encoding_differences() {
        let ast = PdfAstGraph::new();
        let mut encoding = PdfDictionary::new();
        encoding.insert(
            "BaseEncoding",
            PdfValue::Name(PdfName::new("WinAnsiEncoding")),
        );
        encoding.insert(
            "Differences",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(65),
                PdfValue::Name(PdfName::new("Euro")),
                PdfValue::Name(PdfName::new("Aacute")),
            ])),
        );
        let mut font = PdfDictionary::new();
        font.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
        font.insert("Encoding", PdfValue::Dictionary(encoding));

        let mut fonts = PdfDictionary::new();
        fonts.insert("F1", PdfValue::Dictionary(font));
        let mut resources = PdfDictionary::new();
        resources.insert("Font", PdfValue::Dictionary(fonts));
        let operators = [
            ContentOperator::BeginText,
            ContentOperator::SetFont("/F1".to_string(), 10.0),
            ContentOperator::ShowText(vec![65, 66]),
            ContentOperator::EndText,
        ];

        let spans = TextExtractor::new(&ast, &resources).extract_text(&operators);
        assert_eq!(spans[0].text, "€Á");
    }

    #[test]
    fn uses_pdf_width_for_encoded_byte_not_unicode_codepoint() {
        let mut ast = PdfAstGraph::new();
        let mut font = PdfDictionary::new();
        font.insert("Subtype", PdfValue::Name(PdfName::new("Type1")));
        font.insert("Encoding", PdfValue::Name(PdfName::new("WinAnsiEncoding")));
        font.insert(
            "FirstChar",
            PdfValue::Reference(crate::types::PdfReference::new(2, 0)),
        );
        font.insert(
            "Widths",
            PdfValue::Reference(crate::types::PdfReference::new(1, 0)),
        );
        ast.add_node(AstNode::new(
            NodeId::new(1),
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Array(PdfArray::from(vec![PdfValue::Integer(700)])),
        ));
        ast.add_node(AstNode::new(
            NodeId::new(2),
            NodeType::Object(ObjectId::new(2, 0)),
            PdfValue::Integer(128),
        ));
        let mut fonts = PdfDictionary::new();
        fonts.insert("F1", PdfValue::Dictionary(font));
        let mut resources = PdfDictionary::new();
        resources.insert("Font", PdfValue::Dictionary(fonts));
        let operators = [
            ContentOperator::BeginText,
            ContentOperator::SetFont("/F1".to_string(), 10.0),
            ContentOperator::ShowText(vec![0x80]),
            ContentOperator::EndText,
        ];

        let spans = TextExtractor::new(&ast, &resources).extract_text(&operators);
        assert_eq!(spans[0].text, "€");
        assert!((spans[0].width - 7.0).abs() < 1e-9);
    }
}
