use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::types::{PdfArray, PdfDictionary, PdfName, PdfString, PdfValue};

/// Builder for creating PDF document structures
pub struct DocumentBuilder {
    graph: PdfAstGraph,
    catalog_id: Option<NodeId>,
    pages_root_id: Option<NodeId>,
    current_page: Option<NodeId>,
    info_id: Option<NodeId>,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            graph: PdfAstGraph::new(),
            catalog_id: None,
            pages_root_id: None,
            current_page: None,
            info_id: None,
        }
    }

    /// Create document catalog
    pub fn with_catalog(&mut self) -> &mut Self {
        let mut catalog_dict = PdfDictionary::new();
        catalog_dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));

        let catalog_node = AstNode::new(
            NodeId(1),
            NodeType::Catalog,
            PdfValue::Dictionary(catalog_dict),
        );

        let catalog_id = self
            .graph
            .create_node(NodeType::Catalog, catalog_node.value);
        self.graph.set_root(catalog_id);
        self.catalog_id = Some(catalog_id);

        self
    }

    /// Create pages tree
    pub fn with_pages_tree(&mut self) -> &mut Self {
        if self.catalog_id.is_none() {
            self.with_catalog();
        }

        let mut pages_dict = PdfDictionary::new();
        pages_dict.insert("Type", PdfValue::Name(PdfName::new("Pages")));
        pages_dict.insert("Count", PdfValue::Integer(0));
        pages_dict.insert("Kids", PdfValue::Array(PdfArray::new()));

        let pages_id = self
            .graph
            .create_node(NodeType::Pages, PdfValue::Dictionary(pages_dict));

        if let Some(catalog_id) = self.catalog_id {
            self.graph
                .add_edge(catalog_id, pages_id, crate::ast::EdgeType::Child);
        }

        self.pages_root_id = Some(pages_id);
        self
    }

    /// Add a page
    pub fn add_page(&mut self, width: f32, height: f32) -> &mut Self {
        if self.pages_root_id.is_none() {
            self.with_pages_tree();
        }

        let mut page_dict = PdfDictionary::new();
        page_dict.insert("Type", PdfValue::Name(PdfName::new("Page")));

        // MediaBox
        let mut media_box = PdfArray::new();
        media_box.push(PdfValue::Integer(0));
        media_box.push(PdfValue::Integer(0));
        media_box.push(PdfValue::Real(width as f64));
        media_box.push(PdfValue::Real(height as f64));
        page_dict.insert("MediaBox", PdfValue::Array(media_box));

        // Resources
        let resources_dict = PdfDictionary::new();
        page_dict.insert("Resources", PdfValue::Dictionary(resources_dict));

        let page_id = self
            .graph
            .create_node(NodeType::Page, PdfValue::Dictionary(page_dict));

        if let Some(pages_id) = self.pages_root_id {
            self.graph
                .add_edge(pages_id, page_id, crate::ast::EdgeType::Child);

            // Update page count in pages tree
            if let Some(pages_node) = self.graph.get_node_mut(pages_id) {
                if let PdfValue::Dictionary(dict) = &mut pages_node.value {
                    if let Some(PdfValue::Integer(count)) = dict.get_mut("Count") {
                        *count += 1;
                    }

                    // Add to Kids array
                    if let Some(PdfValue::Array(kids)) = dict.get_mut("Kids") {
                        kids.push(PdfValue::Reference(crate::types::PdfReference::new(
                            page_id.0 as u32,
                            0,
                        )));
                    }
                }
            }
        }

        self.current_page = Some(page_id);
        self
    }

    /// Add content stream to current page
    pub fn add_content_stream(&mut self, content: &str) -> &mut Self {
        if let Some(page_id) = self.current_page {
            let content_data = content.as_bytes().to_vec();
            let stream = crate::types::PdfStream::new(
                {
                    let mut dict = PdfDictionary::new();
                    dict.insert("Length", PdfValue::Integer(content_data.len() as i64));
                    dict
                },
                content_data,
            );

            let stream_id = self
                .graph
                .create_node(NodeType::ContentStream, PdfValue::Stream(stream));

            self.graph
                .add_edge(page_id, stream_id, crate::ast::EdgeType::Child);

            // Update page dictionary to reference content stream
            if let Some(page_node) = self.graph.get_node_mut(page_id) {
                if let PdfValue::Dictionary(dict) = &mut page_node.value {
                    dict.insert(
                        "Contents",
                        PdfValue::Reference(crate::types::PdfReference::new(stream_id.0 as u32, 0)),
                    );
                }
            }
        }

        self
    }

    /// Add font to current page resources
    pub fn add_font(&mut self, name: &str, font_type: FontType, base_font: &str) -> &mut Self {
        if let Some(page_id) = self.current_page {
            let mut font_dict = PdfDictionary::new();
            font_dict.insert("Type", PdfValue::Name(PdfName::new("Font")));
            font_dict.insert("Subtype", PdfValue::Name(PdfName::new(font_type.as_str())));
            font_dict.insert("BaseFont", PdfValue::Name(PdfName::new(base_font)));

            let font_id = self.graph.create_node(
                match font_type {
                    FontType::Type1 => NodeType::Type1Font,
                    FontType::TrueType => NodeType::TrueTypeFont,
                    FontType::Type3 => NodeType::Type3Font,
                },
                PdfValue::Dictionary(font_dict),
            );

            self.graph
                .add_edge(page_id, font_id, crate::ast::EdgeType::Child);

            // Update page resources
            if let Some(page_node) = self.graph.get_node_mut(page_id) {
                if let PdfValue::Dictionary(page_dict) = &mut page_node.value {
                    if let Some(PdfValue::Dictionary(resources)) = page_dict.get_mut("Resources") {
                        // Get or create Font dictionary
                        let font_dict = resources
                            .entry("Font")
                            .or_insert_with(|| PdfValue::Dictionary(PdfDictionary::new()));

                        if let PdfValue::Dictionary(fonts) = font_dict {
                            fonts.insert(
                                name,
                                PdfValue::Reference(crate::types::PdfReference::new(
                                    font_id.0 as u32,
                                    0,
                                )),
                            );
                        }
                    }
                }
            }
        }

        self
    }

    /// Add document info
    pub fn with_info(
        &mut self,
        title: Option<&str>,
        author: Option<&str>,
        creator: Option<&str>,
    ) -> &mut Self {
        let mut info_dict = PdfDictionary::new();

        if let Some(title) = title {
            info_dict.insert(
                "Title",
                PdfValue::String(PdfString::new_literal(title.as_bytes())),
            );
        }

        if let Some(author) = author {
            info_dict.insert(
                "Author",
                PdfValue::String(PdfString::new_literal(author.as_bytes())),
            );
        }

        if let Some(creator) = creator {
            info_dict.insert(
                "Creator",
                PdfValue::String(PdfString::new_literal(creator.as_bytes())),
            );
        }

        // Add creation date
        let now = chrono::Utc::now().format("D:%Y%m%d%H%M%S%z").to_string();
        info_dict.insert(
            "CreationDate",
            PdfValue::String(PdfString::new_literal(now.as_bytes())),
        );

        let info_id = self
            .graph
            .create_node(NodeType::Metadata, PdfValue::Dictionary(info_dict));

        if let Some(catalog_id) = self.catalog_id {
            self.graph
                .add_edge(catalog_id, info_id, crate::ast::EdgeType::Child);
        }

        self.info_id = Some(info_id);
        self
    }

    /// Add annotation to current page
    pub fn add_annotation(&mut self, annotation_type: AnnotationType, rect: [f32; 4]) -> &mut Self {
        if let Some(page_id) = self.current_page {
            let mut annot_dict = PdfDictionary::new();
            annot_dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
            annot_dict.insert(
                "Subtype",
                PdfValue::Name(PdfName::new(annotation_type.as_str())),
            );

            // Rect
            let mut rect_array = PdfArray::new();
            for &coord in &rect {
                rect_array.push(PdfValue::Real(coord as f64));
            }
            annot_dict.insert("Rect", PdfValue::Array(rect_array));

            let annot_id = self
                .graph
                .create_node(NodeType::Annotation, PdfValue::Dictionary(annot_dict));

            self.graph
                .add_edge(page_id, annot_id, crate::ast::EdgeType::Child);
        }

        self
    }

    /// Build the final document
    pub fn build(self) -> PdfAstGraph {
        self.graph
    }

    /// Get the current graph reference
    pub fn graph(&self) -> &PdfAstGraph {
        &self.graph
    }

    /// Get mutable graph reference
    pub fn graph_mut(&mut self) -> &mut PdfAstGraph {
        &mut self.graph
    }
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FontType {
    Type1,
    TrueType,
    Type3,
}

impl FontType {
    fn as_str(&self) -> &'static str {
        match self {
            FontType::Type1 => "Type1",
            FontType::TrueType => "TrueType",
            FontType::Type3 => "Type3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnnotationType {
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    ThreeD,
    Redact,
}

impl AnnotationType {
    fn as_str(&self) -> &'static str {
        match self {
            AnnotationType::Text => "Text",
            AnnotationType::Link => "Link",
            AnnotationType::FreeText => "FreeText",
            AnnotationType::Line => "Line",
            AnnotationType::Square => "Square",
            AnnotationType::Circle => "Circle",
            AnnotationType::Highlight => "Highlight",
            AnnotationType::Underline => "Underline",
            AnnotationType::Squiggly => "Squiggly",
            AnnotationType::StrikeOut => "StrikeOut",
            AnnotationType::Stamp => "Stamp",
            AnnotationType::Caret => "Caret",
            AnnotationType::Ink => "Ink",
            AnnotationType::Popup => "Popup",
            AnnotationType::FileAttachment => "FileAttachment",
            AnnotationType::Sound => "Sound",
            AnnotationType::Movie => "Movie",
            AnnotationType::Widget => "Widget",
            AnnotationType::Screen => "Screen",
            AnnotationType::PrinterMark => "PrinterMark",
            AnnotationType::TrapNet => "TrapNet",
            AnnotationType::Watermark => "Watermark",
            AnnotationType::ThreeD => "3D",
            AnnotationType::Redact => "Redact",
        }
    }
}
