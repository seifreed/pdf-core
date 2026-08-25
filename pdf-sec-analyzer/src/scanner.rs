use crate::threats::ThreatType;
use crate::{InfoItem, ScanMetadata, ScanResult, Severity, Threat, Warning};
use pdf_ast::{
    AstNode, NodeType, PdfDictionary, PdfDocument, PdfString, PdfValue, Visitor, VisitorAction,
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct SecurityScanner {
    threats: Vec<Threat>,
    warnings: Vec<Warning>,
    info: Vec<InfoItem>,
    nodes_scanned: usize,
    rules_applied: Vec<String>,
}

impl SecurityScanner {
    pub fn new() -> Self {
        SecurityScanner {
            threats: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
            nodes_scanned: 0,
            rules_applied: Vec::new(),
        }
    }

    pub fn scan(&mut self, document: &PdfDocument) -> ScanResult {
        let start_time = Instant::now();

        // Reset state
        self.threats.clear();
        self.warnings.clear();
        self.info.clear();
        self.nodes_scanned = 0;
        self.rules_applied.clear();

        // Scan document metadata
        self.scan_document_metadata(document);

        // Scan AST nodes
        let mut walker = pdf_ast::visitor::AstWalker::new(&document.ast);
        walker.walk(self);

        // Generate additional analysis
        self.analyze_patterns();

        let scan_duration = start_time.elapsed().as_millis() as u64;

        ScanResult {
            threats: self.threats.clone(),
            warnings: self.warnings.clone(),
            info: self.info.clone(),
            metadata: ScanMetadata {
                scan_time: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
                rules_applied: self.rules_applied.clone(),
                total_nodes_scanned: self.nodes_scanned,
                scan_duration_ms: scan_duration,
            },
        }
    }

    fn scan_document_metadata(&mut self, document: &PdfDocument) {
        self.rules_applied
            .push("document_metadata_scan".to_string());

        // Check PDF version
        if document.version.major > 1 || (document.version.major == 1 && document.version.minor > 7)
        {
            self.info.push(InfoItem {
                category: "Version".to_string(),
                message: "PDF version".to_string(),
                value: document.version.to_string(),
            });
        }

        // Document statistics
        self.info.push(InfoItem {
            category: "Structure".to_string(),
            message: "Total nodes in AST".to_string(),
            value: document.ast.node_count().to_string(),
        });

        self.info.push(InfoItem {
            category: "Structure".to_string(),
            message: "Total pages".to_string(),
            value: document.metadata.page_count.to_string(),
        });

        // Security features
        if document.metadata.encrypted {
            self.info.push(InfoItem {
                category: "Security".to_string(),
                message: "Document is encrypted".to_string(),
                value: "true".to_string(),
            });
        }

        if document.metadata.has_signatures {
            self.info.push(InfoItem {
                category: "Security".to_string(),
                message: "Contains digital signatures".to_string(),
                value: "true".to_string(),
            });
        }

        // Risk indicators
        if document.metadata.has_javascript {
            self.warnings.push(Warning {
                category: "JavaScript".to_string(),
                severity: Severity::Medium,
                message: "Document contains JavaScript".to_string(),
                location: Some("Document level".to_string()),
                recommendation: "Review JavaScript for malicious content".to_string(),
            });
        }

        if document.metadata.has_embedded_files {
            self.warnings.push(Warning {
                category: "Embedded Files".to_string(),
                severity: Severity::Medium,
                message: "Document contains embedded files".to_string(),
                location: Some("Document level".to_string()),
                recommendation: "Scan embedded files for malware".to_string(),
            });
        }

        if document.metadata.has_forms {
            self.info.push(InfoItem {
                category: "Forms".to_string(),
                message: "Document contains forms".to_string(),
                value: "true".to_string(),
            });
        }
    }

    fn analyze_patterns(&mut self) {
        self.rules_applied.push("pattern_analysis".to_string());

        // Check for multiple high-risk features
        let high_risk_count = self
            .threats
            .iter()
            .filter(|t| t.severity >= Severity::High)
            .count();

        if high_risk_count > 2 {
            self.threats.push(Threat {
                threat_type: ThreatType::Malware,
                severity: Severity::Critical,
                title: "Multiple High-Risk Features Detected".to_string(),
                description: format!(
                    "Document contains {} high-risk features, indicating possible malware.",
                    high_risk_count
                ),
                location: "Document-wide pattern".to_string(),
                evidence: vec![format!("{} high-risk threats found", high_risk_count)],
                mitigation:
                    "Exercise extreme caution. Consider this document potentially malicious."
                        .to_string(),
                cve_references: vec![],
            });
        }

        // Check for suspicious combinations
        let has_js = self
            .threats
            .iter()
            .any(|t| t.threat_type == ThreatType::JavaScript);
        let has_network = self
            .threats
            .iter()
            .any(|t| t.threat_type == ThreatType::OutboundConnection);
        let has_executable = self
            .threats
            .iter()
            .any(|t| t.threat_type == ThreatType::EmbeddedExecutable);

        if has_js && has_network {
            self.threats.push(Threat {
                threat_type: ThreatType::Malware,
                severity: Severity::High,
                title: "JavaScript + Network Access Combination".to_string(),
                description:
                    "Document combines JavaScript execution with network access capabilities."
                        .to_string(),
                location: "Pattern analysis".to_string(),
                evidence: vec!["JavaScript and network access both present".to_string()],
                mitigation: "Block network access and disable JavaScript execution.".to_string(),
                cve_references: vec![],
            });
        }

        if has_executable && has_js {
            self.threats.push(Threat {
                threat_type: ThreatType::Exploit,
                severity: Severity::Critical,
                title: "Executable + JavaScript Combination".to_string(),
                description: "Document contains both embedded executables and JavaScript - classic exploit pattern.".to_string(),
                location: "Pattern analysis".to_string(),
                evidence: vec!["Both executable files and JavaScript present".to_string()],
                mitigation: "Do not open this document. High likelihood of exploit attempt.".to_string(),
                cve_references: vec!["CVE-2010-0188".to_string(), "CVE-2009-0927".to_string()],
            });
        }
    }
}

impl Visitor for SecurityScanner {
    fn visit_node(&mut self, node: &AstNode) -> VisitorAction {
        self.nodes_scanned += 1;
        VisitorAction::Continue
    }

    fn visit_action(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        self.rules_applied.push("action_scan".to_string());

        let location = format!("Action node {}", node.id.0);

        if let Some(PdfValue::Name(action_type)) = dict.get("S") {
            match action_type.without_slash() {
                "JavaScript" | "JS" => {
                    if let Some(PdfValue::String(js_code)) = dict.get("JS") {
                        let script_content = js_code.to_string_lossy();
                        self.threats
                            .push(Threat::javascript(location, &script_content));
                    } else {
                        self.threats
                            .push(Threat::javascript(location, "<script content not found>"));
                    }
                }
                "Launch" => {
                    let file_spec = dict
                        .get("F")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_else(|| "<unknown>".to_string());

                    self.threats.push(Threat::embedded_executable(
                        location,
                        &file_spec,
                        "executable",
                    ));
                }
                "URI" => {
                    if let Some(PdfValue::String(uri)) = dict.get("URI") {
                        let url = uri.to_string_lossy();
                        self.threats
                            .push(Threat::outbound_connection(location, &url));
                    }
                }
                "SubmitForm" => {
                    if let Some(PdfValue::String(url)) = dict.get("F") {
                        let submit_url = url.to_string_lossy();
                        self.threats
                            .push(Threat::form_data_exfiltration(location, &submit_url));
                    }
                }
                "ImportData" => {
                    self.threats.push(Threat::suspicious_action(
                        location,
                        "ImportData",
                        "Document attempts to import external data",
                    ));
                }
                "GoToR" => {
                    self.warnings.push(Warning {
                        category: "Navigation".to_string(),
                        severity: Severity::Low,
                        message: "Document contains remote navigation action".to_string(),
                        location: Some(location),
                        recommendation: "Verify destination document is trusted".to_string(),
                    });
                }
                _ => {
                    self.info.push(InfoItem {
                        category: "Actions".to_string(),
                        message: format!("Action type: {}", action_type.without_slash()),
                        value: "detected".to_string(),
                    });
                }
            }
        }

        VisitorAction::Continue
    }

    fn visit_embedded_file(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        self.rules_applied.push("embedded_file_scan".to_string());

        let location = format!("Embedded file node {}", node.id.0);

        let file_name = dict
            .get("F")
            .or_else(|| dict.get("UF"))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "<unnamed>".to_string());

        // Check file extension for executables
        let file_name_lower = file_name.to_lowercase();
        let executable_extensions = [
            ".exe", ".com", ".bat", ".cmd", ".scr", ".pif", ".dll", ".jar", ".class", ".sh",
            ".ps1", ".vbs", ".js",
        ];

        if executable_extensions
            .iter()
            .any(|ext| file_name_lower.ends_with(ext))
        {
            self.threats.push(Threat::embedded_executable(
                location,
                &file_name,
                "executable",
            ));
        } else {
            self.info.push(InfoItem {
                category: "Embedded Files".to_string(),
                message: format!("Embedded file: {}", file_name),
                value: "non-executable".to_string(),
            });
        }

        VisitorAction::Continue
    }

    fn visit_annotation(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        self.rules_applied.push("annotation_scan".to_string());

        // Check for widget annotations (forms)
        if let Some(PdfValue::Name(subtype)) = dict.get("Subtype") {
            if subtype.without_slash() == "Widget" {
                // Check for JavaScript in widget
                if let Some(PdfValue::Dictionary(aa_dict)) = dict.get("AA") {
                    // Additional Actions dictionary
                    for (key, value) in aa_dict.iter() {
                        if let PdfValue::Dictionary(action_dict) = value {
                            if let Some(PdfValue::Name(action_type)) = action_dict.get("S") {
                                if action_type.without_slash() == "JavaScript" {
                                    let location = format!("Widget annotation {}", node.id.0);
                                    if let Some(PdfValue::String(js_code)) = action_dict.get("JS") {
                                        let script_content = js_code.to_string_lossy();
                                        self.threats
                                            .push(Threat::javascript(location, &script_content));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        VisitorAction::Continue
    }
}

impl Default for SecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}
