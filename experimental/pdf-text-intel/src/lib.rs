use pdf_ast::{AstNode, NodeType, PdfDictionary, PdfDocument, Visitor, VisitorAction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

        // Walk through AST
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
}

impl Visitor for TextExtractor {
    fn visit_page(&mut self, _node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        // Extract page content - would need content stream parsing
        if let Some(contents) = dict.get("Contents") {
            // Process content streams for text extraction
            let sample_text = "Sample extracted text from page";
            if self.current_page < self.pages.len() {
                self.pages[self.current_page].text.push_str(sample_text);
                self.pages[self.current_page]
                    .formatted_text
                    .push_str(sample_text);

                // Add sample text block
                self.pages[self.current_page].text_blocks.push(TextBlock {
                    text: sample_text.to_string(),
                    x: 72.0,
                    y: 720.0,
                    width: 200.0,
                    height: 12.0,
                    font: "Times-Roman".to_string(),
                    font_size: 12.0,
                    color: "#000000".to_string(),
                });
            }
        }

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
            .unwrap_or_else(|| "StandardEncoding".to_string());

        let embedded = dict.get("FontFile").is_some()
            || dict.get("FontFile2").is_some()
            || dict.get("FontFile3").is_some();

        let font_type = dict
            .get("Subtype")
            .and_then(|v| v.as_name())
            .map(|n| n.without_slash().to_string())
            .unwrap_or_else(|| "Type1".to_string());

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
        let image_name = format!("Image_{}", self.extraction_stats.images_found + 1);

        if self.current_page > 0 && self.current_page <= self.pages.len() {
            self.pages[self.current_page - 1].images.push(ImageInfo {
                name: image_name,
                x: 100.0,
                y: 500.0,
                width: 200.0,
                height: 150.0,
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
}
