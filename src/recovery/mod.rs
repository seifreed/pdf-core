/// Advanced error recovery and resilient parsing
///
/// This module provides sophisticated error recovery mechanisms that allow
/// the parser to continue processing even when encountering malformed or
/// corrupted PDF data, making it suitable for forensic analysis and
/// handling real-world, imperfect PDF documents.
use crate::ast::{AstError, AstNode, AstResult, NodeId, NodeMetadata, NodeType, PdfDocument};
use crate::parser::PdfParser;
use crate::performance::ResourceBudget;
use crate::types::PdfValue;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;
use std::time::{Duration, Instant};

pub mod diagnostics;
pub mod reconstruction;
pub mod strategies;

pub use diagnostics::*;
pub use reconstruction::*;
pub use strategies::*;

/// Recovery-enabled parser that can handle malformed PDFs
pub struct RecoveryParser {
    base_parser: PdfParser,
    recovery_config: RecoveryConfig,
    recovery_strategies: Vec<Box<dyn RecoveryStrategy>>,
    error_log: Vec<RecoveryError>,
    statistics: RecoveryStatistics,
}

/// Configuration for error recovery
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    pub max_errors: usize,
    pub skip_corrupted_objects: bool,
    pub attempt_structure_reconstruction: bool,
    pub use_heuristic_parsing: bool,
    pub preserve_partial_objects: bool,
    pub enable_fuzzy_matching: bool,
    pub recovery_aggressiveness: RecoveryLevel,
    pub timeout_ms: u64,
}

/// Level of recovery aggressiveness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryLevel {
    Conservative, // Only fix obvious errors
    Moderate,     // Apply common fixes and heuristics
    Aggressive,   // Try all available recovery strategies
    Experimental, // Use experimental and risky recovery methods
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_errors: 1000,
            skip_corrupted_objects: true,
            attempt_structure_reconstruction: true,
            use_heuristic_parsing: true,
            preserve_partial_objects: true,
            enable_fuzzy_matching: true,
            recovery_aggressiveness: RecoveryLevel::Moderate,
            timeout_ms: 60000, // 1 minute
        }
    }
}

/// Statistics for recovery operations
#[derive(Debug, Clone, Default)]
pub struct RecoveryStatistics {
    pub total_errors_encountered: usize,
    pub errors_recovered: usize,
    pub objects_skipped: usize,
    pub objects_reconstructed: usize,
    pub heuristic_fixes_applied: usize,
    pub fuzzy_matches: usize,
    pub recovery_time_ms: u64,
    pub success_rate: f64,
}

/// Error encountered during parsing with recovery context
#[derive(Debug, Clone)]
pub struct RecoveryError {
    pub error_type: RecoveryErrorType,
    pub location: ErrorLocation,
    pub original_error: String,
    pub recovery_attempt: Option<RecoveryAttempt>,
    pub severity: ErrorSeverity,
    pub context: ErrorContext,
}

/// Type of recovery error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryErrorType {
    ParseError,
    StructuralError,
    ReferenceError,
    StreamError,
    EncodingError,
    IntegrityError,
    UnknownFormat,
}

/// Location where error occurred
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub byte_offset: u64,
    pub line_number: Option<usize>,
    pub object_number: Option<u32>,
    pub context_description: String,
}

/// Recovery attempt information
#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    pub strategy_used: String,
    pub success: bool,
    pub result_description: String,
    pub time_taken_ms: u64,
}

/// Severity of an error
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
    Fatal,
}

/// Context information for error recovery
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub surrounding_data: Vec<u8>,
    pub object_hierarchy: Vec<String>,
    pub reference_chain: Vec<String>,
    pub hints: HashMap<String, String>,
}

impl RecoveryParser {
    /// Create a new recovery parser
    pub fn new(config: RecoveryConfig) -> Self {
        let mut parser = Self {
            base_parser: PdfParser::new(),
            recovery_config: config.clone(),
            recovery_strategies: Vec::new(),
            error_log: Vec::new(),
            statistics: RecoveryStatistics::default(),
        };

        parser.initialize_strategies(&config);
        parser
    }

    /// Uses an explicit budget for both the initial and recovered parse.
    pub fn with_resource_budget(mut self, budget: ResourceBudget) -> Self {
        self.base_parser = self.base_parser.with_resource_budget(budget);
        self
    }

    /// Parse a PDF document with error recovery
    pub fn parse_with_recovery(&mut self, data: &[u8]) -> AstResult<(PdfDocument, RecoveryReport)> {
        let start_time = std::time::Instant::now();

        // First attempt normal parsing
        match self.base_parser.parse(&mut Cursor::new(data)) {
            Ok(document) => {
                // Normal parsing succeeded
                let report = RecoveryReport {
                    success: true,
                    errors_encountered: Vec::new(),
                    recovery_actions: Vec::new(),
                    statistics: self.statistics.clone(),
                    final_document_health: DocumentHealth::Healthy,
                };
                return Ok((document, report));
            }
            Err(initial_error) => {
                if is_resource_limit_error(&initial_error) {
                    return Err(initial_error);
                }
                self.check_timeout(start_time)?;
                // Normal parsing failed, begin recovery
                self.log_error(RecoveryError {
                    error_type: RecoveryErrorType::ParseError,
                    location: ErrorLocation {
                        byte_offset: 0,
                        line_number: None,
                        object_number: None,
                        context_description: "Initial parse attempt".to_string(),
                    },
                    original_error: format!("{:?}", initial_error),
                    recovery_attempt: None,
                    severity: ErrorSeverity::Error,
                    context: ErrorContext {
                        surrounding_data: data.get(0..100).unwrap_or(data).to_vec(),
                        object_hierarchy: Vec::new(),
                        reference_chain: Vec::new(),
                        hints: HashMap::new(),
                    },
                });
            }
        }

        // Begin recovery process
        let recovery_result = self.attempt_recovery(data, start_time)?;
        let elapsed = start_time.elapsed().as_millis() as u64;
        self.statistics.recovery_time_ms = elapsed;

        // Calculate success rate
        if self.statistics.total_errors_encountered > 0 {
            self.statistics.success_rate = self.statistics.errors_recovered as f64
                / self.statistics.total_errors_encountered as f64;
        }

        Ok(recovery_result)
    }

    /// Attempt recovery using all available strategies
    fn attempt_recovery(
        &mut self,
        data: &[u8],
        start_time: Instant,
    ) -> AstResult<(PdfDocument, RecoveryReport)> {
        let mut document = PdfDocument::new(crate::ast::PdfVersion { major: 1, minor: 4 });
        let mut recovery_actions = Vec::new();
        let mut current_data: Cow<'_, [u8]> = Cow::Borrowed(data);

        // Apply recovery strategies in order of preference
        for strategy in &self.recovery_strategies {
            self.check_timeout(start_time)?;
            let context = RecoveryContext {
                original_data: data,
                current_data: current_data.as_ref(),
                document: &document,
                config: &self.recovery_config,
                error_log: &self.error_log,
            };

            match strategy.apply_recovery(context) {
                Ok(result) => {
                    recovery_actions.push(RecoveryAction {
                        strategy_name: strategy.name().to_string(),
                        action_type: result.action_type,
                        description: result.description,
                        success: true,
                        data_modified: result.data_changed,
                    });

                    if result.data_changed {
                        if let Some(modified_data) = result.modified_data {
                            current_data = Cow::Owned(modified_data);
                        }
                    }

                    if result.document_changed {
                        if let Some(new_doc) = result.modified_document {
                            document = new_doc;
                        }
                    }

                    self.statistics.errors_recovered += 1;
                }
                Err(e) => {
                    recovery_actions.push(RecoveryAction {
                        strategy_name: strategy.name().to_string(),
                        action_type: RecoveryActionType::Failed,
                        description: format!("Strategy failed: {:?}", e),
                        success: false,
                        data_modified: false,
                    });
                }
            }
        }

        // Final parsing attempt with recovered data
        self.check_timeout(start_time)?;
        let final_document = match self
            .base_parser
            .parse(&mut Cursor::new(current_data.as_ref()))
        {
            Ok(doc) => doc,
            Err(error) if is_resource_limit_error(&error) => return Err(error),
            Err(_) => {
                // If all recovery failed, return best-effort document
                self.create_best_effort_document(current_data.as_ref())?
            }
        };

        let health = self.assess_document_health(&final_document);

        let report = RecoveryReport {
            success: !recovery_actions.is_empty(),
            errors_encountered: self.error_log.clone(),
            recovery_actions,
            statistics: self.statistics.clone(),
            final_document_health: health,
        };

        Ok((final_document, report))
    }

    fn check_timeout(&self, start_time: Instant) -> AstResult<()> {
        if start_time.elapsed() >= Duration::from_millis(self.recovery_config.timeout_ms) {
            return Err(AstError::ParseError(
                "recovery timeout exceeded".to_string(),
            ));
        }
        Ok(())
    }

    /// Initialize recovery strategies based on configuration
    fn initialize_strategies(&mut self, config: &RecoveryConfig) {
        // Add strategies based on recovery level
        match config.recovery_aggressiveness {
            RecoveryLevel::Conservative => {
                self.recovery_strategies
                    .push(Box::new(BasicStructureRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(ReferenceRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StructureRepairStrategy::new()));
            }
            RecoveryLevel::Moderate => {
                self.recovery_strategies
                    .push(Box::new(BasicStructureRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StructureRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(XRefRebuildStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(ReferenceRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(DataRecoveryStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(EncodingRecovery::new()));
            }
            RecoveryLevel::Aggressive => {
                self.recovery_strategies
                    .push(Box::new(BasicStructureRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StructureRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(XRefRebuildStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(ReferenceRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(DataRecoveryStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(EncodingRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(HeuristicRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(FuzzyMatchingRecovery::new()));
            }
            RecoveryLevel::Experimental => {
                // Add all strategies including experimental ones
                self.recovery_strategies
                    .push(Box::new(BasicStructureRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StructureRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(XRefRebuildStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(ReferenceRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(StreamRepairStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(DataRecoveryStrategy::new()));
                self.recovery_strategies
                    .push(Box::new(EncodingRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(HeuristicRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(FuzzyMatchingRecovery::new()));
                self.recovery_strategies
                    .push(Box::new(ExperimentalRecovery::new()));
            }
        }
    }

    /// Log a recovery error
    fn log_error(&mut self, error: RecoveryError) {
        self.statistics.total_errors_encountered += 1;
        self.error_log.push(error);

        // Limit error log size
        if self.error_log.len() > self.recovery_config.max_errors {
            self.error_log.remove(0);
        }
    }

    /// Create a best-effort document when all recovery fails
    fn create_best_effort_document(&self, data: &[u8]) -> AstResult<PdfDocument> {
        let budget = self.base_parser.resource_budget();
        budget
            .consume_node()
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        let mut document = PdfDocument::new(crate::ast::PdfVersion { major: 1, minor: 4 });
        document.budget = budget.clone();

        // Create minimal document structure
        let catalog_id = document.ast.create_node(
            NodeType::Catalog,
            PdfValue::Dictionary({
                let mut dict = crate::types::PdfDictionary::new();
                dict.insert(
                    "Type",
                    PdfValue::Name(crate::types::PdfName::new("Catalog")),
                );
                dict
            }),
        );
        document.ast.set_root(catalog_id);

        // Try to extract any recognizable objects
        let objects = self.extract_salvageable_objects(data, &budget)?;
        for object in objects {
            budget
                .consume_edge()
                .map_err(|error| AstError::ParseError(error.to_string()))?;
            let node_id = document.ast.create_node(object.node_type, object.value);
            // Link to catalog if possible
            document
                .ast
                .add_edge(catalog_id, node_id, crate::ast::EdgeType::Child);
        }

        Ok(document)
    }

    /// Extract any objects that can be salvaged from corrupted data
    fn extract_salvageable_objects(
        &self,
        data: &[u8],
        budget: &ResourceBudget,
    ) -> AstResult<Vec<AstNode>> {
        let mut objects = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            // Look for object markers
            if let Some(obj_start) = self.find_object_start(&data[pos..]) {
                pos += obj_start;

                if let Some(obj_end) = self.find_object_end(&data[pos..]) {
                    let obj_data = &data[pos..pos + obj_end];

                    // Try to parse this object
                    budget
                        .consume_node()
                        .map_err(|error| AstError::ParseError(error.to_string()))?;
                    budget
                        .consume_decoded(obj_data.len() as u64)
                        .map_err(|error| AstError::ParseError(error.to_string()))?;
                    if let Ok(node) = self.parse_partial_object(obj_data) {
                        objects.push(node);
                    }

                    pos += obj_end;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(objects)
    }

    /// Find the start of a PDF object
    fn find_object_start(&self, data: &[u8]) -> Option<usize> {
        // Look for pattern like "123 0 obj"
        (0..data.len().saturating_sub(6)).find(|&i| {
            data[i..].starts_with(b" obj") || (i > 0 && data[i - 1..].starts_with(b" obj"))
        })
    }

    /// Find the end of a PDF object
    fn find_object_end(&self, data: &[u8]) -> Option<usize> {
        // Look for "endobj"
        for i in 0..data.len().saturating_sub(6) {
            if data[i..].starts_with(b"endobj") {
                return Some(i + 6);
            }
        }
        None
    }

    /// Parse a partial object with lenient rules
    fn parse_partial_object(&self, data: &[u8]) -> AstResult<AstNode> {
        // Very simplified object parsing for recovery
        let node_id = NodeId(rand::random::<u64>() as usize);

        // Try to determine object type from content
        let node_type = if data.windows(4).any(|w| w == b"Type") {
            if data.windows(7).any(|w| w == b"Catalog") {
                NodeType::Catalog
            } else if data.windows(4).any(|w| w == b"Page") {
                NodeType::Page
            } else if data.windows(4).any(|w| w == b"Font") {
                NodeType::Font
            } else {
                NodeType::Other
            }
        } else {
            NodeType::Other
        };

        Ok(AstNode {
            id: node_id,
            node_type,
            value: PdfValue::String(crate::types::PdfString::new_literal(data)),
            metadata: NodeMetadata::default(),
            children: Vec::new(),
            references: Vec::new(),
        })
    }

    /// Assess the health of the recovered document
    fn assess_document_health(&self, document: &PdfDocument) -> DocumentHealth {
        let nodes = document.ast.get_all_nodes();
        let has_catalog = document.ast.get_root().is_some();
        let error_rate = if self.statistics.total_errors_encountered > 0 {
            1.0 - self.statistics.success_rate
        } else {
            0.0
        };

        if !has_catalog || nodes.is_empty() {
            DocumentHealth::SeverelyDamaged
        } else if error_rate > 0.5 {
            DocumentHealth::Damaged
        } else if error_rate > 0.1 {
            DocumentHealth::PartiallyRecovered
        } else {
            DocumentHealth::Healthy
        }
    }

    /// Get recovery statistics
    pub fn get_statistics(&self) -> &RecoveryStatistics {
        &self.statistics
    }

    /// Get error log
    pub fn get_error_log(&self) -> &[RecoveryError] {
        &self.error_log
    }
}

fn is_resource_limit_error(error: &AstError) -> bool {
    matches!(error, AstError::ParseError(message) if message.contains("resource budget exceeded") || message.contains("File too large"))
}

/// Report generated after recovery attempt
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub success: bool,
    pub errors_encountered: Vec<RecoveryError>,
    pub recovery_actions: Vec<RecoveryAction>,
    pub statistics: RecoveryStatistics,
    pub final_document_health: DocumentHealth,
}

/// Action taken during recovery
#[derive(Debug, Clone)]
pub struct RecoveryAction {
    pub strategy_name: String,
    pub action_type: RecoveryActionType,
    pub description: String,
    pub success: bool,
    pub data_modified: bool,
}

/// Type of recovery action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryActionType {
    StructureRepair,
    ReferenceResolution,
    StreamDecoding,
    EncodingFix,
    HeuristicPatch,
    FuzzyMatch,
    DataReconstruction,
    Failed,
}

/// Overall health of the document after recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentHealth {
    Healthy,
    PartiallyRecovered,
    Damaged,
    SeverelyDamaged,
}

/// Parse a PDF with automatic error recovery
pub fn parse_with_automatic_recovery(data: &[u8]) -> AstResult<(PdfDocument, RecoveryReport)> {
    let mut parser = RecoveryParser::new(RecoveryConfig::default());
    parser.parse_with_recovery(data)
}

#[cfg(test)]
mod tests {
    use super::{RecoveryConfig, RecoveryParser};
    use crate::performance::ResourceBudget;

    #[test]
    fn recovery_does_not_bypass_input_budget() {
        let budget = ResourceBudget::new(8, 1024, 1024, 10, 10, 10, 10, 8);
        let error = RecoveryParser::new(RecoveryConfig::default())
            .with_resource_budget(budget)
            .parse_with_recovery(b"%PDF-1.7\n")
            .expect_err("oversized input must not enter best-effort recovery");
        assert!(error.to_string().contains("InputBytes"));
    }

    #[test]
    fn recovery_honors_configured_timeout() {
        let config = RecoveryConfig {
            timeout_ms: 0,
            ..RecoveryConfig::default()
        };
        let error = RecoveryParser::new(config)
            .parse_with_recovery(b"not a pdf")
            .expect_err("zero timeout must stop recovery");
        assert!(error.to_string().contains("timeout"));
    }

    #[test]
    fn best_effort_recovery_respects_node_budget() {
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 10, 8);
        let parser = RecoveryParser::new(RecoveryConfig::default()).with_resource_budget(budget);

        let error = parser
            .create_best_effort_document(b"not a pdf")
            .expect_err("best-effort recovery must not bypass node limits");
        assert!(error.to_string().contains("Nodes"));
    }
}
