use crate::ast::{NodeId, NodeType, PdfDocument};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod constraints;
pub mod pdf_standards;
pub mod pdfa;
pub mod schema;

pub use constraints::*;
pub use pdf_standards::*;
pub use pdfa::*;
pub use schema::*;

/// Validation severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Validation result for a single check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub code: String,
    pub message: String,
    pub node_id: Option<NodeId>,
    pub location: Option<String>,
    pub suggestion: Option<String>,
}

/// Complete validation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub schema_name: String,
    pub schema_version: String,
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub statistics: ValidationStatistics,
    pub metadata: HashMap<String, String>,
}

/// Versioned validation report envelope for stable exports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReportEnvelope {
    pub report_format_version: String,
    pub generated_at_unix: u64,
    pub report: ValidationReport,
}

/// Validation statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationStatistics {
    pub total_checks: usize,
    pub passed_checks: usize,
    pub failed_checks: usize,
    pub info_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub critical_count: usize,
}

impl ValidationReport {
    pub fn new(schema_name: String, schema_version: String) -> Self {
        Self {
            schema_name,
            schema_version,
            is_valid: true,
            issues: Vec::new(),
            statistics: ValidationStatistics::default(),
            metadata: HashMap::new(),
        }
    }

    pub fn add_issue(&mut self, issue: ValidationIssue) {
        match issue.severity {
            ValidationSeverity::Info => self.statistics.info_count += 1,
            ValidationSeverity::Warning => self.statistics.warning_count += 1,
            ValidationSeverity::Error => {
                self.statistics.error_count += 1;
                self.is_valid = false;
            }
            ValidationSeverity::Critical => {
                self.statistics.critical_count += 1;
                self.is_valid = false;
            }
        }

        self.statistics.failed_checks += 1;
        self.issues.push(issue);
    }

    pub fn add_passed_check(&mut self) {
        self.statistics.passed_checks += 1;
        self.statistics.total_checks += 1;
    }

    pub fn finalize(&mut self) {
        self.statistics.total_checks =
            self.statistics.passed_checks + self.statistics.failed_checks;
    }

    pub fn into_envelope(self) -> ValidationReportEnvelope {
        let generated_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        ValidationReportEnvelope {
            report_format_version: "1.0".to_string(),
            generated_at_unix,
            report: self,
        }
    }
}

/// Base trait for PDF schema validation
pub trait PdfSchema: Send + Sync {
    /// Get schema name
    fn name(&self) -> &str;

    /// Get schema version
    fn version(&self) -> &str;

    /// Validate a complete document
    fn validate(&self, document: &PdfDocument) -> ValidationReport;

    /// Get all constraints for this schema
    fn get_constraints(&self) -> Vec<Box<dyn SchemaConstraint>>;

    /// Check if schema supports specific PDF version
    fn supports_pdf_version(&self, version: &crate::ast::PdfVersion) -> bool;

    /// Get schema description
    fn description(&self) -> &str {
        ""
    }

    /// Get schema URL/reference
    fn reference_url(&self) -> Option<&str> {
        None
    }
}

/// Base trait for schema constraints
pub trait SchemaConstraint: Send + Sync {
    /// Get constraint name
    fn name(&self) -> &str;

    /// Get constraint description
    fn description(&self) -> &str;

    /// Check constraint against a document
    fn check(&self, document: &PdfDocument, report: &mut ValidationReport);

    /// Get constraint category
    fn category(&self) -> ConstraintCategory {
        ConstraintCategory::General
    }

    /// Get required node types for this constraint
    fn required_node_types(&self) -> Vec<NodeType> {
        Vec::new()
    }

    /// ISO 32000-2 reference for audit mapping
    fn iso_reference(&self) -> Option<&str> {
        None
    }
}

/// Constraint categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintCategory {
    General,
    Structure,
    Content,
    Metadata,
    Security,
    Accessibility,
    Graphics,
    Fonts,
    Images,
    Annotations,
    Forms,
    JavaScript,
}

/// PDF schema registry
pub struct SchemaRegistry {
    schemas: HashMap<String, Box<dyn PdfSchema>>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            schemas: HashMap::new(),
        };

        // Register standard schemas
        registry.register_standard_schemas();
        registry
    }

    fn register_standard_schemas(&mut self) {
        // PDF 2.0 base schema
        self.register(Box::new(Pdf20Schema::new()));

        // PDF/A schemas
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA1a)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA1b)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA2a)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA2b)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA2u)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA3a)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA3b)));
        self.register(Box::new(PdfASchema::new(PdfALevel::PdfA3u)));

        // PDF/X schemas
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX1a)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX3)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX4)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX4p)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX5g)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX5n)));
        self.register(Box::new(PdfXSchema::new(PdfXLevel::PdfX5pg)));

        // PDF/UA schema
        self.register(Box::new(PdfUASchema::new(PdfUALevel::PdfUA1)));
        self.register(Box::new(PdfUASchema::new(PdfUALevel::PdfUA2)));
    }

    pub fn register(&mut self, schema: Box<dyn PdfSchema>) {
        let name = schema.name().to_string();
        self.schemas.insert(name, schema);
    }

    pub fn get_schema(&self, name: &str) -> Option<&dyn PdfSchema> {
        self.schemas.get(name).map(|s| s.as_ref())
    }

    pub fn list_schemas(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
    }

    /// Validates a document only when the selected schema supports its PDF
    /// version. Returns `None` for an unknown or incompatible schema.
    pub fn validate(&self, document: &PdfDocument, schema_name: &str) -> Option<ValidationReport> {
        self.get_schema(schema_name)
            .filter(|schema| schema.supports_pdf_version(&document.version))
            .map(|schema| schema.validate(document))
    }

    pub fn validate_all(&self, document: &PdfDocument) -> HashMap<String, ValidationReport> {
        let mut results = HashMap::new();

        for (name, schema) in &self.schemas {
            if schema.supports_pdf_version(&document.version) {
                let report = schema.validate(document);
                results.insert(name.clone(), report);
            }
        }

        results
    }

    pub fn verify_report(&self, report: &ValidationReport) -> bool {
        self.get_schema(&report.schema_name)
            .map(|schema| schema.version() == report.schema_version)
            .unwrap_or(false)
    }

    pub fn verify_envelope(&self, envelope: &ValidationReportEnvelope) -> bool {
        envelope.report_format_version == "1.0" && self.verify_report(&envelope.report)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation context for passing state between constraints
pub struct ValidationContext<'a> {
    pub document: &'a PdfDocument,
    pub report: &'a mut ValidationReport,
    pub visited_nodes: HashSet<NodeId>,
    pub context_data: HashMap<String, String>,
}

impl<'a> ValidationContext<'a> {
    pub fn new(document: &'a PdfDocument, report: &'a mut ValidationReport) -> Self {
        Self {
            document,
            report,
            visited_nodes: HashSet::new(),
            context_data: HashMap::new(),
        }
    }

    pub fn mark_visited(&mut self, node_id: NodeId) {
        self.visited_nodes.insert(node_id);
    }

    pub fn is_visited(&self, node_id: NodeId) -> bool {
        self.visited_nodes.contains(&node_id)
    }

    pub fn set_context(&mut self, key: String, value: String) {
        self.context_data.insert(key, value);
    }

    pub fn get_context(&self, key: &str) -> Option<&String> {
        self.context_data.get(key)
    }
}
