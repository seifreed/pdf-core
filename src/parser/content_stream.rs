use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentOperator {
    BeginText,
    EndText,

    SetCharSpace(f64),
    SetWordSpace(f64),
    SetHorizontalScale(f64),
    SetLeading(f64),
    SetFont(String, f64),
    SetTextRenderMode(i32),
    SetTextRise(f64),

    MoveText(f64, f64),
    MoveTextNextLine,
    SetTextMatrix(f64, f64, f64, f64, f64, f64),

    ShowText(Vec<u8>),
    ShowTextArray(Vec<TextArrayElement>),
    ShowTextNextLine(Vec<u8>),
    ShowTextWithSpacing(f64, f64, Vec<u8>),

    MoveTo(f64, f64),
    LineTo(f64, f64),
    CurveTo(f64, f64, f64, f64, f64, f64),
    CurveToV(f64, f64, f64, f64),
    CurveToY(f64, f64, f64, f64),
    ClosePath,
    Rectangle(f64, f64, f64, f64),

    Stroke,
    CloseAndStroke,
    Fill,
    FillEvenOdd,
    FillAndStroke,
    FillAndStrokeEvenOdd,
    CloseFillAndStroke,
    CloseFillAndStrokeEvenOdd,
    EndPath,

    Clip,
    ClipEvenOdd,

    SetLineWidth(f64),
    SetLineCap(i32),
    SetLineJoin(i32),
    SetMiterLimit(f64),
    SetDashPattern(Vec<f64>, f64),
    SetRenderingIntent(String),
    SetFlatness(f64),

    Save,
    Restore,
    SetMatrix(f64, f64, f64, f64, f64, f64),

    BeginMarkedContent(String),
    BeginMarkedContentWithProps(String, MarkedContentProps),
    EndMarkedContent,

    SetColorSpace(String),
    SetStrokingColorSpace(String),
    SetColor(Vec<f64>),
    SetStrokingColor(Vec<f64>),
    SetColorN(Vec<f64>, Option<String>),
    SetStrokingColorN(Vec<f64>, Option<String>),
    SetGrayLevel(f64),
    SetStrokingGrayLevel(f64),
    SetRGBColor(f64, f64, f64),
    SetStrokingRGBColor(f64, f64, f64),
    SetCMYKColor(f64, f64, f64, f64),
    SetStrokingCMYKColor(f64, f64, f64, f64),

    PaintXObject(String),
    PaintShading(String),

    BeginInlineImage,
    InlineImageData(InlineImageInfo),
    EndInlineImage,

    SetGraphicsStateParams(String),

    // Additional operators for completeness
    PaintPattern(String),
    BeginShadingPattern(PatternInfo),
    EndShadingPattern,

    // Type 3 font operators
    SetCharWidth(f64, f64),
    SetCacheDevice(f64, f64, f64, f64, f64, f64),

    // Compatibility operators
    BeginCompatibilitySection,
    EndCompatibilitySection,

    Unknown(String, Vec<Operand>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineImageInfo {
    pub width: u32,
    pub height: u32,
    pub color_space: String,
    pub bits_per_component: u8,
    pub filter: Option<String>,
    pub decode_params: Option<HashMap<String, Operand>>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternInfo {
    pub pattern_type: i32,
    pub shading: Option<ShadingInfo>,
    pub matrix: Option<[f64; 6]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadingInfo {
    pub shading_type: i32,
    pub color_space: String,
    pub coords: Vec<f64>,
    pub function: Option<Box<Operand>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextArrayElement {
    Text(Vec<u8>),
    Spacing(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarkedContentProps {
    Dictionary(crate::types::PdfDictionary),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operand {
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Operand>),
    Dictionary(Vec<(String, Operand)>),
}

#[allow(dead_code)]
pub struct ContentStreamParser {
    operators: Vec<ContentOperator>,
}

impl Default for ContentStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentStreamParser {
    pub fn new() -> Self {
        ContentStreamParser {
            operators: Vec::new(),
        }
    }

    pub fn parse(&mut self, data: &[u8]) -> Result<Vec<ContentOperator>, String> {
        let operators = crate::parser::content_operands::parse_content_stream(data);
        self.operators = operators.clone();
        Ok(operators)
    }
}
