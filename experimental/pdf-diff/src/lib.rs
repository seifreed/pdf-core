use pdf_ast::{NodeType, PdfDocument, PdfValue};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfDiffResult {
    pub summary: DiffSummary,
    pub structure_changes: Vec<StructureChange>,
    pub content_changes: Vec<ContentChange>,
    pub metadata_changes: Vec<MetadataChange>,
    pub page_changes: Vec<PageChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub similarity_score: f64,
    pub total_changes: usize,
    pub structure_changes: usize,
    pub content_changes: usize,
    pub metadata_changes: usize,
    pub pages_added: usize,
    pub pages_removed: usize,
    pub pages_modified: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureChange {
    pub change_type: ChangeType,
    pub node_type: String,
    pub location: String,
    pub description: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentChange {
    pub change_type: ChangeType,
    pub page: usize,
    pub location: String,
    pub old_content: String,
    pub new_content: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataChange {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageChange {
    pub page_number: usize,
    pub change_type: ChangeType,
    pub description: String,
    pub text_changes: Vec<TextChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChange {
    pub line_number: usize,
    pub change_type: ChangeType,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Removed,
    Modified,
    Moved,
}

pub struct PdfDiffer;

impl PdfDiffer {
    pub fn compare(doc1: &PdfDocument, doc2: &PdfDocument) -> PdfDiffResult {
        let mut differ = DocumentDiffer::new(doc1, doc2);
        differ.analyze()
    }

    pub fn quick_compare(doc1: &PdfDocument, doc2: &PdfDocument) -> f64 {
        let result = Self::compare(doc1, doc2);
        result.summary.similarity_score
    }

    pub fn find_text_differences(text1: &str, text2: &str) -> Vec<TextChange> {
        let diff = TextDiff::from_lines(text1, text2);
        let mut changes = Vec::new();
        let mut line_num = 1;

        for change in diff.iter_all_changes() {
            let change_type = match change.tag() {
                ChangeTag::Delete => ChangeType::Removed,
                ChangeTag::Insert => ChangeType::Added,
                ChangeTag::Equal => {
                    line_num += 1;
                    continue;
                }
            };

            changes.push(TextChange {
                line_number: line_num,
                change_type,
                old_text: if change.tag() == ChangeTag::Delete {
                    change.value().to_string()
                } else {
                    String::new()
                },
                new_text: if change.tag() == ChangeTag::Insert {
                    change.value().to_string()
                } else {
                    String::new()
                },
            });

            line_num += 1;
        }

        changes
    }
}

struct DocumentDiffer<'a> {
    doc1: &'a PdfDocument,
    doc2: &'a PdfDocument,
    structure_changes: Vec<StructureChange>,
    content_changes: Vec<ContentChange>,
    metadata_changes: Vec<MetadataChange>,
    page_changes: Vec<PageChange>,
}

impl<'a> DocumentDiffer<'a> {
    fn new(doc1: &'a PdfDocument, doc2: &'a PdfDocument) -> Self {
        Self {
            doc1,
            doc2,
            structure_changes: Vec::new(),
            content_changes: Vec::new(),
            metadata_changes: Vec::new(),
            page_changes: Vec::new(),
        }
    }

    fn analyze(&mut self) -> PdfDiffResult {
        self.compare_metadata();
        self.compare_structure();
        self.compare_pages();

        let total_changes = self.structure_changes.len()
            + self.content_changes.len()
            + self.metadata_changes.len()
            + self.page_changes.len();

        let similarity_score = self.calculate_similarity();

        PdfDiffResult {
            summary: DiffSummary {
                similarity_score,
                total_changes,
                structure_changes: self.structure_changes.len(),
                content_changes: self.content_changes.len(),
                metadata_changes: self.metadata_changes.len(),
                pages_added: self.count_pages_added(),
                pages_removed: self.count_pages_removed(),
                pages_modified: self.page_changes.len(),
            },
            structure_changes: self.structure_changes.clone(),
            content_changes: self.content_changes.clone(),
            metadata_changes: self.metadata_changes.clone(),
            page_changes: self.page_changes.clone(),
        }
    }

    fn compare_metadata(&mut self) {
        // Compare PDF versions
        if self.doc1.version != self.doc2.version {
            self.metadata_changes.push(MetadataChange {
                field: "PDF Version".to_string(),
                old_value: Some(self.doc1.version.to_string()),
                new_value: Some(self.doc2.version.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        // Compare page counts
        if self.doc1.metadata.page_count != self.doc2.metadata.page_count {
            self.metadata_changes.push(MetadataChange {
                field: "Page Count".to_string(),
                old_value: Some(self.doc1.metadata.page_count.to_string()),
                new_value: Some(self.doc2.metadata.page_count.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        // Compare security features
        if self.doc1.metadata.encrypted != self.doc2.metadata.encrypted {
            self.metadata_changes.push(MetadataChange {
                field: "Encryption".to_string(),
                old_value: Some(self.doc1.metadata.encrypted.to_string()),
                new_value: Some(self.doc2.metadata.encrypted.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        if self.doc1.metadata.has_javascript != self.doc2.metadata.has_javascript {
            self.metadata_changes.push(MetadataChange {
                field: "JavaScript".to_string(),
                old_value: Some(self.doc1.metadata.has_javascript.to_string()),
                new_value: Some(self.doc2.metadata.has_javascript.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        if self.doc1.metadata.has_embedded_files != self.doc2.metadata.has_embedded_files {
            self.metadata_changes.push(MetadataChange {
                field: "Embedded Files".to_string(),
                old_value: Some(self.doc1.metadata.has_embedded_files.to_string()),
                new_value: Some(self.doc2.metadata.has_embedded_files.to_string()),
                change_type: ChangeType::Modified,
            });
        }
    }

    fn compare_structure(&mut self) {
        // Compare AST node counts
        let nodes1 = self.doc1.ast.node_count();
        let nodes2 = self.doc2.ast.node_count();

        if nodes1 != nodes2 {
            self.structure_changes.push(StructureChange {
                change_type: ChangeType::Modified,
                node_type: "Document".to_string(),
                location: "AST Root".to_string(),
                description: "Total node count changed".to_string(),
                old_value: Some(nodes1.to_string()),
                new_value: Some(nodes2.to_string()),
            });
        }

        // Compare specific node types
        let page_nodes1 = self.doc1.ast.find_nodes_by_type(NodeType::Page);
        let page_nodes2 = self.doc2.ast.find_nodes_by_type(NodeType::Page);

        if page_nodes1.len() != page_nodes2.len() {
            self.structure_changes.push(StructureChange {
                change_type: ChangeType::Modified,
                node_type: "Page".to_string(),
                location: "Document Structure".to_string(),
                description: "Number of pages changed".to_string(),
                old_value: Some(page_nodes1.len().to_string()),
                new_value: Some(page_nodes2.len().to_string()),
            });
        }

        let font_nodes1 = self.doc1.ast.find_nodes_by_type(NodeType::Font);
        let font_nodes2 = self.doc2.ast.find_nodes_by_type(NodeType::Font);

        if font_nodes1.len() != font_nodes2.len() {
            self.structure_changes.push(StructureChange {
                change_type: ChangeType::Modified,
                node_type: "Font".to_string(),
                location: "Resource Dictionary".to_string(),
                description: "Number of fonts changed".to_string(),
                old_value: Some(font_nodes1.len().to_string()),
                new_value: Some(font_nodes2.len().to_string()),
            });
        }
    }

    fn compare_pages(&mut self) {
        let max_pages = std::cmp::max(self.doc1.metadata.page_count, self.doc2.metadata.page_count);

        for page_num in 1..=max_pages {
            let has_page1 = page_num <= self.doc1.metadata.page_count;
            let has_page2 = page_num <= self.doc2.metadata.page_count;

            match (has_page1, has_page2) {
                (true, false) => {
                    self.page_changes.push(PageChange {
                        page_number: page_num,
                        change_type: ChangeType::Removed,
                        description: "Page removed".to_string(),
                        text_changes: Vec::new(),
                    });
                }
                (false, true) => {
                    self.page_changes.push(PageChange {
                        page_number: page_num,
                        change_type: ChangeType::Added,
                        description: "Page added".to_string(),
                        text_changes: Vec::new(),
                    });
                }
                (true, true) => {
                    let page_text1 = self.page_content(self.doc1, page_num);
                    let page_text2 = self.page_content(self.doc2, page_num);

                    if page_text1 != page_text2 {
                        let text_changes =
                            PdfDiffer::find_text_differences(&page_text1, &page_text2);

                        if !text_changes.is_empty() {
                            self.page_changes.push(PageChange {
                                page_number: page_num,
                                change_type: ChangeType::Modified,
                                description: "Page content modified".to_string(),
                                text_changes,
                            });
                        }
                    }
                }
                (false, false) => unreachable!(),
            }
        }
    }

    fn page_content(&self, document: &PdfDocument, page_number: usize) -> String {
        let Some(page_id) = document.get_page(page_number.saturating_sub(1)) else {
            return String::new();
        };
        let Some(page) = document.ast.get_node(page_id) else {
            return String::new();
        };
        let mut content = String::new();
        let mut seen = HashSet::new();
        if let PdfValue::Dictionary(dict) = &page.value {
            if let Some(contents) = dict.get("Contents") {
                Self::collect_content(document, contents, &mut seen, &mut content);
            }
        }
        content
    }

    fn collect_content(
        document: &PdfDocument,
        value: &PdfValue,
        seen: &mut HashSet<pdf_ast::types::ObjectId>,
        output: &mut String,
    ) {
        match value {
            PdfValue::Stream(stream) => {
                if let Ok(bytes) = stream.decode_with_limits(5 * 1024 * 1024, 50) {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    output.push('\n');
                }
            }
            PdfValue::Array(items) => {
                for item in items {
                    Self::collect_content(document, item, seen, output);
                }
            }
            PdfValue::Reference(reference) if seen.insert(reference.id()) => {
                if let Some(node) = document.ast.get_node_by_object(reference.id()) {
                    Self::collect_content(document, &node.value, seen, output);
                }
            }
            _ => {}
        }
    }

    fn calculate_similarity(&self) -> f64 {
        let total_changes = self.structure_changes.len()
            + self.content_changes.len()
            + self.metadata_changes.len()
            + self.page_changes.len();

        if total_changes == 0 {
            return 1.0; // Identical documents
        }

        let total_items = (self.doc1.ast.node_count() + self.doc1.metadata.page_count + 10) as f64;

        let similarity = 1.0 - (total_changes as f64 / total_items);
        similarity.clamp(0.0, 1.0)
    }

    fn count_pages_added(&self) -> usize {
        self.page_changes
            .iter()
            .filter(|pc| pc.change_type == ChangeType::Added)
            .count()
    }

    fn count_pages_removed(&self) -> usize {
        self.page_changes
            .iter()
            .filter(|pc| pc.change_type == ChangeType::Removed)
            .count()
    }
}

pub fn generate_diff_report(result: &PdfDiffResult) -> String {
    let mut report = String::new();

    report.push_str("PDF Document Comparison Report\n");
    report.push_str("==============================\n\n");

    report.push_str(&format!(
        "Similarity Score: {:.2}%\n",
        result.summary.similarity_score * 100.0
    ));
    report.push_str(&format!(
        "Total Changes: {}\n",
        result.summary.total_changes
    ));
    report.push_str(&format!(
        "Structure Changes: {}\n",
        result.summary.structure_changes
    ));
    report.push_str(&format!(
        "Content Changes: {}\n",
        result.summary.content_changes
    ));
    report.push_str(&format!(
        "Metadata Changes: {}\n",
        result.summary.metadata_changes
    ));
    report.push_str(&format!("Pages Added: {}\n", result.summary.pages_added));
    report.push_str(&format!(
        "Pages Removed: {}\n",
        result.summary.pages_removed
    ));
    report.push_str(&format!(
        "Pages Modified: {}\n\n",
        result.summary.pages_modified
    ));

    if !result.metadata_changes.is_empty() {
        report.push_str("METADATA CHANGES:\n");
        report.push_str("-----------------\n");
        for change in &result.metadata_changes {
            report.push_str(&format!(
                "• {} changed from {:?} to {:?}\n",
                change.field, change.old_value, change.new_value
            ));
        }
        report.push_str("\n");
    }

    if !result.structure_changes.is_empty() {
        report.push_str("STRUCTURE CHANGES:\n");
        report.push_str("------------------\n");
        for change in &result.structure_changes {
            report.push_str(&format!(
                "• {} in {}: {}\n",
                change.node_type, change.location, change.description
            ));
        }
        report.push_str("\n");
    }

    if !result.page_changes.is_empty() {
        report.push_str("PAGE CHANGES:\n");
        report.push_str("-------------\n");
        for change in &result.page_changes {
            report.push_str(&format!(
                "• Page {}: {:?} - {}\n",
                change.page_number, change.change_type, change.description
            ));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_ast::PdfVersion;

    #[test]
    fn test_identical_documents() {
        let doc1 = PdfDocument::new(PdfVersion::new(1, 7));
        let doc2 = PdfDocument::new(PdfVersion::new(1, 7));

        let result = PdfDiffer::compare(&doc1, &doc2);
        assert_eq!(result.summary.similarity_score, 1.0);
        assert_eq!(result.summary.total_changes, 0);
    }

    #[test]
    fn test_different_versions() {
        let doc1 = PdfDocument::new(PdfVersion::new(1, 4));
        let doc2 = PdfDocument::new(PdfVersion::new(1, 7));

        let result = PdfDiffer::compare(&doc1, &doc2);
        assert!(result.summary.similarity_score < 1.0);
        assert!(result.summary.total_changes > 0);
    }
}
