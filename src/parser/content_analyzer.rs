use crate::ast::{AstNode, NodeId, NodeMetadata, NodeType};
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfString, PdfValue};
use std::collections::HashMap;

/// Analyzes PDF content streams and extracts operators and potentially malicious content
pub struct ContentAnalyzer {
    suspicious_keywords: Vec<&'static str>,
    js_keywords: Vec<&'static str>,
}

impl ContentAnalyzer {
    pub fn new() -> Self {
        Self {
            suspicious_keywords: vec![
                "eval",
                "unescape",
                "fromCharCode",
                "String.fromCharCode",
                "document.write",
                "innerHTML",
                "createElement",
                "appendChild",
                "exec",
                "system",
                "shell",
                "cmd",
                "powershell",
                "bash",
                "ActiveXObject",
                "WScript",
                "Shell.Application",
                "getAnnots",
                "print",
                "mailDoc",
                "importDataObject",
                "launch",
                "submitForm",
                "resetForm",
                "exportValues",
                "/F ",
                "/FT ",
                "/Ff ",
                "/V ",
                "/DV ",
                "/AA ",
                "/OpenAction",
                "/Names",
                "/AcroForm",
                "/XFA",
            ],
            js_keywords: vec![
                "function",
                "var",
                "let",
                "const",
                "if",
                "for",
                "while",
                "try",
                "catch",
                "throw",
                "return",
                "new",
                "this",
                "app.",
                "doc.",
                "field.",
                "event.",
                "util.",
                "AFNumber_Format",
                "AFPercent_Format",
                "AFDate_Format",
            ],
        }
    }

    /// Analyze a content stream and extract all operators and suspicious content
    pub fn analyze_content_stream(&self, stream_data: &[u8], node_id: usize) -> Vec<AstNode> {
        self.analyze_content_stream_with_budget(stream_data, node_id, &ResourceBudget::default())
            .unwrap_or_default()
    }

    /// Analyze a content stream while charging input and generated nodes.
    pub fn analyze_content_stream_with_budget(
        &self,
        stream_data: &[u8],
        node_id: usize,
        budget: &ResourceBudget,
    ) -> Result<Vec<AstNode>, ResourceBudgetError> {
        budget.consume_input(stream_data.len() as u64)?;
        let mut nodes = Vec::new();
        let mut next_id = node_id;

        // Try to interpret as text first
        if let Ok(content) = std::str::from_utf8(stream_data) {
            nodes.extend(self.parse_text_content_with_budget(content, &mut next_id, budget)?);
        }

        // Parse as PDF operators
        nodes.extend(self.parse_pdf_operators_with_budget(stream_data, &mut next_id, budget)?);

        Ok(nodes)
    }

    /// Parse text content looking for JavaScript and suspicious patterns
    fn parse_text_content_with_budget(
        &self,
        content: &str,
        next_id: &mut usize,
        budget: &ResourceBudget,
    ) -> Result<Vec<AstNode>, ResourceBudgetError> {
        let mut nodes = Vec::new();

        // Check for JavaScript code
        if self.contains_javascript(content) {
            budget.consume_node()?;
            let js_node = self.create_js_node(content, next_id);
            nodes.push(js_node);
        }

        // Check for suspicious patterns
        for suspicious in self.find_suspicious_patterns(content) {
            budget.consume_node()?;
            let suspicious_node = self.create_suspicious_node(&suspicious, next_id);
            nodes.push(suspicious_node);
        }

        // Check for external references (URLs, file paths)
        for external_ref in self.find_external_references(content) {
            budget.consume_node()?;
            let ref_node = self.create_external_ref_node(&external_ref, next_id);
            nodes.push(ref_node);
        }

        Ok(nodes)
    }

    /// Parse PDF content stream operators (BT, ET, Tf, Tj, etc.)
    fn parse_pdf_operators_with_budget(
        &self,
        data: &[u8],
        next_id: &mut usize,
        budget: &ResourceBudget,
    ) -> Result<Vec<AstNode>, ResourceBudgetError> {
        let mut nodes = Vec::new();

        if let Ok(content) = std::str::from_utf8(data) {
            let tokens = self.tokenize_content_stream(content);
            let mut i = 0;

            while i < tokens.len() {
                if let Some(operator) = self.identify_operator(&tokens, i) {
                    budget.consume_node()?;
                    let op_node = self.create_operator_node(&operator, next_id);
                    nodes.push(op_node);
                    i += operator.token_count;
                } else {
                    i += 1;
                }
            }
        }

        Ok(nodes)
    }

    /// Tokenize content stream into PDF tokens
    fn tokenize_content_stream(&self, content: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut in_string = false;
        let mut in_hex_string = false;
        let mut escape_next = false;

        for ch in content.chars() {
            match ch {
                '(' if !in_hex_string && !escape_next => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                    in_string = true;
                    current_token.push(ch);
                }
                ')' if in_string && !escape_next => {
                    current_token.push(ch);
                    tokens.push(current_token.clone());
                    current_token.clear();
                    in_string = false;
                }
                '<' if !in_string => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                    in_hex_string = true;
                    current_token.push(ch);
                }
                '>' if in_hex_string => {
                    current_token.push(ch);
                    tokens.push(current_token.clone());
                    current_token.clear();
                    in_hex_string = false;
                }
                '\\' if in_string => {
                    current_token.push(ch);
                    escape_next = true;
                }
                c if c.is_whitespace() && !in_string && !in_hex_string => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                }
                _ => {
                    current_token.push(ch);
                    if escape_next {
                        escape_next = false;
                    }
                }
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        tokens
    }

    /// Identify PDF operators and their operands
    fn identify_operator(&self, tokens: &[String], index: usize) -> Option<ContentOperator> {
        if index >= tokens.len() {
            return None;
        }

        let token = &tokens[index];

        match token.as_str() {
            // Text operators
            "BT" => Some(ContentOperator {
                operator: "BT".to_string(),
                operands: vec![],
                operator_type: OperatorType::TextBegin,
                token_count: 1,
                suspicious: false,
            }),
            "ET" => Some(ContentOperator {
                operator: "ET".to_string(),
                operands: vec![],
                operator_type: OperatorType::TextEnd,
                token_count: 1,
                suspicious: false,
            }),
            "Tf" if index >= 2 => Some(ContentOperator {
                operator: "Tf".to_string(),
                operands: vec![tokens[index - 2].clone(), tokens[index - 1].clone()],
                operator_type: OperatorType::TextFont,
                token_count: 3,
                suspicious: false,
            }),
            "Tj" if index >= 1 => Some(ContentOperator {
                operator: "Tj".to_string(),
                operands: vec![tokens[index - 1].clone()],
                operator_type: OperatorType::TextShow,
                token_count: 2,
                suspicious: self.is_suspicious_text(&tokens[index - 1]),
            }),
            "TJ" if index >= 1 => Some(ContentOperator {
                operator: "TJ".to_string(),
                operands: vec![tokens[index - 1].clone()],
                operator_type: OperatorType::TextShowArray,
                token_count: 2,
                suspicious: self.is_suspicious_text(&tokens[index - 1]),
            }),
            // Graphics operators
            "q" => Some(ContentOperator {
                operator: "q".to_string(),
                operands: vec![],
                operator_type: OperatorType::GraphicsSave,
                token_count: 1,
                suspicious: false,
            }),
            "Q" => Some(ContentOperator {
                operator: "Q".to_string(),
                operands: vec![],
                operator_type: OperatorType::GraphicsRestore,
                token_count: 1,
                suspicious: false,
            }),
            // XObject operators
            "Do" if index >= 1 => Some(ContentOperator {
                operator: "Do".to_string(),
                operands: vec![tokens[index - 1].clone()],
                operator_type: OperatorType::XObject,
                token_count: 2,
                suspicious: false,
            }),
            _ => None,
        }
    }

    /// Check if text content contains JavaScript
    fn contains_javascript(&self, content: &str) -> bool {
        let js_count = self
            .js_keywords
            .iter()
            .filter(|&keyword| content.contains(keyword))
            .count();

        js_count >= 2 || content.contains("function") || content.contains("eval")
    }

    /// Find suspicious patterns in content
    fn find_suspicious_patterns(&self, content: &str) -> Vec<SuspiciousPattern> {
        let mut patterns = Vec::new();

        for &keyword in &self.suspicious_keywords {
            if content.contains(keyword) {
                patterns.push(SuspiciousPattern {
                    pattern: keyword.to_string(),
                    content: content.to_string(),
                    risk_level: self.assess_risk_level(keyword),
                });
            }
        }

        patterns
    }

    /// Find external references (URLs, file paths)
    fn find_external_references(&self, content: &str) -> Vec<ExternalReference> {
        let mut refs = Vec::new();

        // Simple URL detection
        if content.contains("http://") || content.contains("https://") {
            refs.push(ExternalReference {
                ref_type: "URL".to_string(),
                target: content.to_string(),
                suspicious: true,
            });
        }

        // File path detection
        if content.contains("file://") || content.contains("\\\\") || content.contains("C:\\") {
            refs.push(ExternalReference {
                ref_type: "FilePath".to_string(),
                target: content.to_string(),
                suspicious: true,
            });
        }

        refs
    }

    fn is_suspicious_text(&self, text: &str) -> bool {
        self.suspicious_keywords
            .iter()
            .any(|&keyword| text.contains(keyword))
    }

    fn assess_risk_level(&self, keyword: &str) -> RiskLevel {
        match keyword {
            k if k.contains("eval") || k.contains("exec") || k.contains("shell") => RiskLevel::High,
            k if k.contains("ActiveX") || k.contains("launch") || k.contains("system") => {
                RiskLevel::High
            }
            k if k.contains("JavaScript") || k.contains("unescape") => RiskLevel::Medium,
            _ => RiskLevel::Low,
        }
    }

    fn create_js_node(&self, content: &str, next_id: &mut usize) -> AstNode {
        let node_id = NodeId(*next_id);
        *next_id += 1;

        let mut properties = HashMap::new();
        properties.insert("js_content".to_string(), content.to_string());
        properties.insert("risk_level".to_string(), "high".to_string());

        AstNode::new(
            node_id,
            NodeType::EmbeddedJS,
            PdfValue::String(PdfString::new_literal(content.as_bytes())),
        )
        .with_metadata(NodeMetadata {
            properties,
            ..Default::default()
        })
    }

    fn create_suspicious_node(&self, pattern: &SuspiciousPattern, next_id: &mut usize) -> AstNode {
        let node_id = NodeId(*next_id);
        *next_id += 1;

        let mut properties = HashMap::new();
        properties.insert("pattern".to_string(), pattern.pattern.clone());
        properties.insert(
            "risk_level".to_string(),
            format!("{:?}", pattern.risk_level),
        );

        AstNode::new(
            node_id,
            NodeType::SuspiciousAction,
            PdfValue::String(PdfString::new_literal(pattern.content.as_bytes())),
        )
        .with_metadata(NodeMetadata {
            properties,
            ..Default::default()
        })
    }

    fn create_external_ref_node(
        &self,
        ext_ref: &ExternalReference,
        next_id: &mut usize,
    ) -> AstNode {
        let node_id = NodeId(*next_id);
        *next_id += 1;

        let mut properties = HashMap::new();
        properties.insert("ref_type".to_string(), ext_ref.ref_type.clone());
        properties.insert("target".to_string(), ext_ref.target.clone());
        properties.insert("suspicious".to_string(), ext_ref.suspicious.to_string());

        AstNode::new(
            node_id,
            NodeType::ExternalReference,
            PdfValue::String(PdfString::new_literal(ext_ref.target.as_bytes())),
        )
        .with_metadata(NodeMetadata {
            properties,
            ..Default::default()
        })
    }

    fn create_operator_node(&self, operator: &ContentOperator, next_id: &mut usize) -> AstNode {
        let node_id = NodeId(*next_id);
        *next_id += 1;

        let mut properties = HashMap::new();
        properties.insert("operator".to_string(), operator.operator.clone());
        properties.insert("operands".to_string(), operator.operands.join(" "));
        properties.insert("type".to_string(), format!("{:?}", operator.operator_type));
        properties.insert("suspicious".to_string(), operator.suspicious.to_string());

        let node_type = match operator.operator_type {
            OperatorType::TextBegin
            | OperatorType::TextEnd
            | OperatorType::TextFont
            | OperatorType::TextShow
            | OperatorType::TextShowArray => NodeType::TextOperator,
            OperatorType::GraphicsSave | OperatorType::GraphicsRestore => {
                NodeType::GraphicsOperator
            }
            _ => NodeType::ContentOperator,
        };

        AstNode::new(
            node_id,
            node_type,
            PdfValue::String(PdfString::new_literal(operator.operator.as_bytes())),
        )
        .with_metadata(NodeMetadata {
            properties,
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone)]
pub struct ContentOperator {
    pub operator: String,
    pub operands: Vec<String>,
    pub operator_type: OperatorType,
    pub token_count: usize,
    pub suspicious: bool,
}

#[derive(Debug, Clone)]
pub enum OperatorType {
    TextBegin,
    TextEnd,
    TextFont,
    TextShow,
    TextShowArray,
    GraphicsSave,
    GraphicsRestore,
    XObject,
    Other,
}

#[derive(Debug, Clone)]
pub struct SuspiciousPattern {
    pub pattern: String,
    pub content: String,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone)]
pub struct ExternalReference {
    pub ref_type: String,
    pub target: String,
    pub suspicious: bool,
}

#[derive(Debug, Clone)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for ContentAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ContentAnalyzer;
    use crate::performance::{ResourceBudget, ResourceBudgetError};

    #[test]
    fn content_analysis_charges_input_before_work() {
        let budget = ResourceBudget::new(3, 1024, 1024, 100, 10, 10, 10, 10);
        let error = ContentAnalyzer::new()
            .analyze_content_stream_with_budget(b"BT ET", 0, &budget)
            .expect_err("content input must respect the shared budget");
        assert_eq!(error, ResourceBudgetError::InputBytes);
    }

    #[test]
    fn content_analysis_charges_generated_nodes() {
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);
        let error = ContentAnalyzer::new()
            .analyze_content_stream_with_budget(b"BT ET", 0, &budget)
            .expect_err("each generated operator must consume node budget");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }
}
