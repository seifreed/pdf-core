use pdf_ast::parser::content_operands::parse_content_stream;
use pdf_ast::{AstNode, PdfDictionary, PdfDocument, PdfValue, Visitor, VisitorAction};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextExtractionResult {
    pub pages: Vec<PageText>,
    pub metadata: ExtractionMetadata,
    pub fonts: Vec<FontInfo>,
    pub structure: DocumentStructure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageText {
    pub page_number: usize,
    pub text: String,
    pub formatted_text: String,
    pub text_blocks: Vec<TextBlock>,
    pub images: Vec<ImageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font: String,
    pub font_size: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    pub name: String,
    pub base_font: String,
    pub encoding: String,
    pub embedded: bool,
    pub font_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStructure {
    pub headings: Vec<Heading>,
    pub tables: Vec<TableInfo>,
    pub lists: Vec<ListInfo>,
    pub links: Vec<LinkInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub page: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub rows: usize,
    pub columns: usize,
    pub page: usize,
    pub data: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListInfo {
    pub list_type: ListType,
    pub items: Vec<String>,
    pub page: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListType {
    Bullet,
    Numbered,
    Definition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    pub page: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionMetadata {
    pub total_pages: usize,
    pub total_words: usize,
    pub total_characters: usize,
    pub fonts_used: usize,
    pub images_found: usize,
    pub extraction_time_ms: u64,
}

pub struct TextExtractor {
    pages: Vec<PageText>,
    fonts: Vec<FontInfo>,
    structure: DocumentStructure,
    current_page: usize,
    extraction_stats: ExtractionStats,
}

struct ExtractionStats {
    total_words: usize,
    total_characters: usize,
    fonts_found: usize,
    images_found: usize,
}

impl TextExtractor {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            fonts: Vec::new(),
            structure: DocumentStructure {
                headings: Vec::new(),
                tables: Vec::new(),
                lists: Vec::new(),
                links: Vec::new(),
            },
            current_page: 0,
            extraction_stats: ExtractionStats {
                total_words: 0,
                total_characters: 0,
                fonts_found: 0,
                images_found: 0,
            },
        }
    }

    pub fn extract(&mut self, document: &PdfDocument) -> TextExtractionResult {
        let start = std::time::Instant::now();

        self.pages.clear();
        self.fonts.clear();
        self.current_page = 0;
        self.extraction_stats = ExtractionStats {
            total_words: 0,
            total_characters: 0,
            fonts_found: 0,
            images_found: 0,
        };

        // Initialize pages
        for i in 0..document.metadata.page_count {
            self.pages.push(PageText {
                page_number: i + 1,
                text: String::new(),
                formatted_text: String::new(),
                text_blocks: Vec::new(),
                images: Vec::new(),
            });
        }

        for (page_index, page_id) in document.get_pages().into_iter().enumerate() {
            if let Some(page) = document
                .ast
                .get_node(page_id)
                .and_then(|node| node.as_dict())
            {
                self.extract_page(document, page, page_index);
            }
        }

        // Collect font and image metadata from the resolved AST.
        let mut walker = pdf_ast::visitor::AstWalker::new(&document.ast);
        walker.walk(self);

        // Calculate statistics
        for page in &self.pages {
            self.extraction_stats.total_words += page.text.split_whitespace().count();
            self.extraction_stats.total_characters += page.text.len();
        }

        let extraction_time = start.elapsed().as_millis() as u64;

        TextExtractionResult {
            pages: self.pages.clone(),
            fonts: self.fonts.clone(),
            structure: self.structure.clone(),
            metadata: ExtractionMetadata {
                total_pages: self.pages.len(),
                total_words: self.extraction_stats.total_words,
                total_characters: self.extraction_stats.total_characters,
                fonts_used: self.fonts.len(),
                images_found: self.extraction_stats.images_found,
                extraction_time_ms: extraction_time,
            },
        }
    }

    fn extract_page(&mut self, document: &PdfDocument, page: &PdfDictionary, page_index: usize) {
        let resources = page
            .get("Resources")
            .and_then(|value| Self::resolve_dictionary(document, value))
            .unwrap_or_default();
        let mut streams = Vec::new();
        let mut seen = HashSet::new();
        if let Some(contents) = page.get("Contents") {
            Self::collect_streams(document, contents, &mut streams, &mut seen);
        }

        for stream in streams {
            let bytes = match stream.decode_with_limits(5 * 1024 * 1024, 50) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let operators = parse_content_stream(&bytes);
            let mut extractor =
                pdf_ast::parser::text_extraction::TextExtractor::new(&document.ast, &resources);
            for span in extractor.extract_text(&operators) {
                if let Some(page_text) = self.pages.get_mut(page_index) {
                    page_text.text.push_str(&span.text);
                    page_text.formatted_text.push_str(&span.text);
                    page_text.text_blocks.push(TextBlock {
                        text: span.text,
                        x: span.x,
                        y: span.y,
                        width: span.width,
                        height: span.height,
                        font: span.font_name,
                        font_size: span.font_size,
                        color: "unknown".to_string(),
                    });
                }
            }
        }
    }

    fn resolve_dictionary(document: &PdfDocument, value: &PdfValue) -> Option<PdfDictionary> {
        match value {
            PdfValue::Dictionary(dict) => Some(dict.clone()),
            PdfValue::Reference(reference) => document
                .ast
                .get_node_by_object(reference.id())
                .and_then(|node| node.as_dict().cloned()),
            _ => None,
        }
    }

    fn collect_streams(
        document: &PdfDocument,
        value: &PdfValue,
        streams: &mut Vec<pdf_ast::types::PdfStream>,
        seen: &mut HashSet<pdf_ast::types::ObjectId>,
    ) {
        match value {
            PdfValue::Stream(stream) => streams.push(stream.clone()),
            PdfValue::Array(items) => {
                for item in items {
                    Self::collect_streams(document, item, streams, seen);
                }
            }
            PdfValue::Reference(reference) if seen.insert(reference.id()) => {
                if let Some(node) = document.ast.get_node_by_object(reference.id()) {
                    Self::collect_streams(document, &node.value, streams, seen);
                }
            }
            _ => {}
        }
    }
}

impl Visitor for TextExtractor {
    fn visit_page(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.current_page += 1;
        VisitorAction::Continue
    }

    fn visit_font(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let font_name = dict
            .get("BaseFont")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let encoding = dict
            .get("Encoding")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let embedded = dict.get("FontFile").is_some()
            || dict.get("FontFile2").is_some()
            || dict.get("FontFile3").is_some();

        let font_type = dict
            .get("Subtype")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        self.fonts.push(FontInfo {
            name: font_name.clone(),
            base_font: font_name,
            encoding,
            embedded,
            font_type,
        });

        self.extraction_stats.fonts_found += 1;
        VisitorAction::Continue
    }

    fn visit_image(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let image_name = dict
            .get("Name")
            .and_then(|value| value.as_name())
            .map(|name| name.without_slash().to_string())
            .unwrap_or_else(|| format!("Image_{}", self.extraction_stats.images_found + 1));
        let width = dict
            .get("Width")
            .and_then(|value| value.as_integer())
            .unwrap_or(0) as f64;
        let height = dict
            .get("Height")
            .and_then(|value| value.as_integer())
            .unwrap_or(0) as f64;

        if self.current_page > 0 && self.current_page <= self.pages.len() {
            self.pages[self.current_page - 1].images.push(ImageInfo {
                name: image_name,
                x: 0.0,
                y: 0.0,
                width,
                height,
                alt_text: None,
            });
        }

        self.extraction_stats.images_found += 1;
        VisitorAction::Continue
    }
}

impl Default for TextExtractor {
    fn default() -> Self {
        Self::new()
    }
}

pub fn extract_text(document: &PdfDocument) -> TextExtractionResult {
    let mut extractor = TextExtractor::new();
    extractor.extract(document)
}

pub fn extract_plain_text(document: &PdfDocument) -> String {
    let result = extract_text(document);
    result
        .pages
        .iter()
        .map(|page| page.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn search_text(document: &PdfDocument, query: &str) -> Vec<(usize, String)> {
    let result = extract_text(document);
    let mut matches = Vec::new();

    for (page_num, page) in result.pages.iter().enumerate() {
        for line in page.text.lines() {
            if line.to_lowercase().contains(&query.to_lowercase()) {
                matches.push((page_num + 1, line.to_string()));
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_ast::PdfVersion;

    #[test]
    fn test_text_extraction() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        let result = extract_text(&doc);
        assert_eq!(result.metadata.total_pages, 0);
    }

    #[test]
    fn test_plain_text_extraction() {
        let doc = PdfDocument::new(PdfVersion::new(1, 7));
        let text = extract_plain_text(&doc);
        assert!(text.is_empty());
    }

    #[test]
    fn extracts_text_from_a_real_content_stream() {
        use pdf_ast::ast::NodeType;
        use pdf_ast::types::{ObjectId, PdfName, PdfReference, PdfStream, StreamData};

        let mut document = PdfDocument::new(PdfVersion::new(1, 7));
        let mut font = PdfDictionary::new();
        font.insert("BaseFont", PdfValue::Name(PdfName::new("Helvetica")));
        document.ast.create_node(
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Dictionary(font),
        );

        let mut resources = PdfDictionary::new();
        let mut fonts = PdfDictionary::new();
        fonts.insert("F1", PdfValue::Reference(PdfReference::new(1, 0)));
        resources.insert("Font", PdfValue::Dictionary(fonts));

        let mut page = PdfDictionary::new();
        page.insert("Resources", PdfValue::Dictionary(resources));
        page.insert(
            "Contents",
            PdfValue::Stream(PdfStream {
                dict: PdfDictionary::new(),
                data: StreamData::Raw(b"BT /F1 12 Tf 10 10 Td (Hello) Tj ET".to_vec()),
            }),
        );
        document
            .ast
            .create_node(NodeType::Page, PdfValue::Dictionary(page));
        document.metadata.page_count = 1;

        assert_eq!(document.get_pages().len(), 1);
        let operators = parse_content_stream(b"BT /F1 12 Tf 10 10 Td (Hello) Tj ET");
        assert!(!operators.is_empty());
        let page_id = document.get_pages()[0];
        let page = document.ast.get_node(page_id).unwrap();
        let resources = page
            .as_dict()
            .unwrap()
            .get("Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let mut core_extractor =
            pdf_ast::parser::text_extraction::TextExtractor::new(&document.ast, resources);
        assert_eq!(core_extractor.extract_text(&operators).len(), 1);
        let result = extract_text(&document);
        assert_eq!(result.pages[0].text, "Hello");
    }
}
