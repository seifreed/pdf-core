use crate::ast::{NodeId, PdfAstGraph};
use crate::parser::cmap::{CMap, CMapParser};
use crate::parser::content_stream::ContentOperator;
use crate::types::{PdfDictionary, PdfValue};
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
        // Pre-load fonts from resources
        self.load_fonts();

        // Process operators
        for op in operators {
            self.process_operator(op);
        }

        // Sort spans by position
        self.text_spans
            .sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));

        self.text_spans.clone()
    }

    fn load_fonts(&mut self) {
        if let Some(PdfValue::Dictionary(fonts)) = self.page_resources.get("Font") {
            for (name, font_ref) in fonts.iter() {
                if let PdfValue::Reference(_obj_id) = font_ref {
                    // Get font node from AST
                    // Parse font info
                    let font_info = self.parse_font_info(name.as_str(), font_ref);
                    self.fonts
                        .insert(name.without_slash().to_string(), font_info);
                }
            }
        }
    }

    fn parse_font_info(&mut self, name: &str, _font_value: &PdfValue) -> FontInfo {
        // Default font info

        // Parse font dictionary if available
        // This would need access to the actual font object
        // For now, return default

        FontInfo {
            font_type: "Type1".to_string(),
            base_font: name.to_string(),
            encoding: "StandardEncoding".to_string(),
            to_unicode: None,
            width_map: HashMap::new(),
            default_width: 1000.0,
            font_matrix: [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
        }
    }

    fn process_operator(&mut self, op: &ContentOperator) {
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
                if let Some(font_info) = self.fonts.get(name) {
                    self.text_state.current_font = Some(font_info.clone());
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
                self.process_operator(&ContentOperator::MoveText(0.0, -leading));
            }

            ContentOperator::SetTextMatrix(a, b, c, d, e, f) => {
                self.graphics_state.text_matrix = [*a, *b, *c, *d, *e, *f];
                self.graphics_state.text_line_matrix = [*a, *b, *c, *d, *e, *f];
            }

            ContentOperator::ShowText(text) => {
                self.show_text(text);
            }

            ContentOperator::ShowTextArray(array) => {
                for element in array {
                    match element {
                        crate::parser::content_stream::TextArrayElement::Text(text) => {
                            self.show_text(text);
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
                self.process_operator(&ContentOperator::MoveTextNextLine);
                self.show_text(text);
            }

            ContentOperator::ShowTextWithSpacing(tw, tc, text) => {
                self.graphics_state.word_space = *tw;
                self.graphics_state.char_space = *tc;
                self.process_operator(&ContentOperator::MoveTextNextLine);
                self.show_text(text);
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
    }

    fn show_text(&mut self, text_bytes: &[u8]) {
        if self.text_state.current_font.is_none() {
            return;
        }

        let Some(font) = self.text_state.current_font.as_ref() else {
            return;
        };
        let mut chars = Vec::new();
        let mut total_width = 0.0;

        // Decode text using font encoding/ToUnicode
        let decoded = self.decode_text(text_bytes, font);

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
    }

    fn decode_text(&self, text_bytes: &[u8], font: &FontInfo) -> String {
        // Try ToUnicode CMap first
        if let Some(cmap) = &self.text_state.current_cmap {
            return self.decode_with_cmap(text_bytes, cmap);
        }

        // Fallback to encoding
        match font.encoding.as_str() {
            "WinAnsiEncoding" => self.decode_win_ansi(text_bytes),
            "MacRomanEncoding" => self.decode_mac_roman(text_bytes),
            "StandardEncoding" => {
                // Simple ASCII decoding
                String::from_utf8_lossy(text_bytes).to_string()
            }
            _ => {
                // Default fallback encoding
                String::from_utf8_lossy(text_bytes).to_string()
            }
        }
    }

    fn decode_with_cmap(&self, text_bytes: &[u8], cmap: &CMap) -> String {
        let mut result = String::new();
        let mut i = 0;

        while i < text_bytes.len() {
            // Try 2-byte code first
            if i + 1 < text_bytes.len() {
                let code = &text_bytes[i..i + 2];
                if let Some(unicode) = CMapParser::new(
                    &mut PdfAstGraph::new(),
                    &crate::parser::reference_resolver::ObjectNodeMap::new(),
                )
                .map_code_to_unicode(cmap, code)
                {
                    result.push_str(&unicode);
                    i += 2;
                    continue;
                }
            }

            // Try 1-byte code
            let code = &text_bytes[i..i + 1];
            if let Some(unicode) = CMapParser::new(
                &mut PdfAstGraph::new(),
                &crate::parser::reference_resolver::ObjectNodeMap::new(),
            )
            .map_code_to_unicode(cmap, code)
            {
                result.push_str(&unicode);
            } else {
                // Fallback to direct mapping
                result.push(text_bytes[i] as char);
            }

            i += 1;
        }

        result
    }

    fn decode_win_ansi(&self, text_bytes: &[u8]) -> String {
        text_bytes
            .iter()
            .map(|&b| {
                if b < 128 {
                    b as char
                } else {
                    // Windows-1252 mapping for 128-255
                    match b {
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
                        _ => b as char,
                    }
                }
            })
            .collect()
    }

    fn decode_mac_roman(&self, text_bytes: &[u8]) -> String {
        // Simplified MacRoman decoding
        String::from_utf8_lossy(text_bytes).to_string()
    }

    fn get_char_width(&self, ch: char, font: &FontInfo) -> f64 {
        // Get width from font metrics
        let code = ch as u32;
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
