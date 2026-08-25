use crate::ast::{AstNode, NodeId, PdfDocument};
use crate::bindings::utils;
use crate::parser::PdfParser;
use crate::plugins::api::PluginManager;
use crate::validation::{SchemaRegistry, ValidationReport};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyModule};
use pyo3::{Py, PyResult, Python};
use std::collections::HashMap;
use std::sync::Arc;

/// Python wrapper for PdfDocument
#[pyclass(name = "PdfDocument")]
#[derive(Clone)]
pub struct PyPdfDocument {
    inner: Arc<PdfDocument>,
}

#[pymethods]
impl PyPdfDocument {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(PdfDocument::new(crate::ast::PdfVersion {
                major: 1,
                minor: 7,
            })),
        }
    }

    /// Parse PDF from bytes
    #[staticmethod]
    fn from_bytes(data: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let bytes = data.as_bytes();
        let parser = PdfParser::new();

        match parser.parse_bytes(bytes) {
            Ok(document) => Ok(Self {
                inner: Arc::new(document),
            }),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse PDF: {:?}",
                e
            ))),
        }
    }

    /// Get document version
    fn get_version(&self) -> (u8, u8) {
        (self.inner.version.major, self.inner.version.minor)
    }

    /// Get all nodes in the document
    fn get_all_nodes(&self) -> Vec<PyAstNode> {
        self.inner
            .ast
            .get_all_nodes()
            .into_iter()
            .cloned()
            .map(|node| PyAstNode { inner: node })
            .collect()
    }

    /// Get root node
    fn get_root(&self) -> Option<PyAstNode> {
        self.inner.ast.get_root().and_then(|node_id| {
            self.inner.ast.get_node(node_id).map(|node| PyAstNode {
                inner: node.clone(),
            })
        })
    }

    /// Get node by ID
    fn get_node(&self, node_id: u64) -> Option<PyAstNode> {
        let node_id = usize::try_from(node_id).ok()?;
        self.inner
            .ast
            .get_node(NodeId(node_id))
            .map(|node| PyAstNode {
                inner: node.clone(),
            })
    }

    /// Get children of a node
    fn get_children(&self, node_id: u64) -> Vec<PyAstNode> {
        let node_id = match usize::try_from(node_id) {
            Ok(id) => NodeId(id),
            Err(_) => return Vec::new(),
        };
        self.inner
            .ast
            .get_children(node_id)
            .iter()
            .filter_map(|&child_id| {
                self.inner.ast.get_node(child_id).map(|node| PyAstNode {
                    inner: node.clone(),
                })
            })
            .collect()
    }

    /// Get nodes by type
    fn get_nodes_by_type(&self, node_type: &str) -> Vec<PyAstNode> {
        let Ok(node_type) = utils::parse_node_type(node_type) else {
            return Vec::new();
        };
        self.inner
            .ast
            .get_nodes_by_type(node_type)
            .iter()
            .filter_map(|node_id| {
                self.inner.ast.get_node(*node_id).map(|node| PyAstNode {
                    inner: node.clone(),
                })
            })
            .collect()
    }

    /// Validate document against schema
    fn validate(&self, schema_name: &str) -> PyResult<PyValidationReport> {
        let registry = SchemaRegistry::new();

        match registry.validate(&self.inner, schema_name) {
            Some(report) => Ok(PyValidationReport { inner: report }),
            None => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Schema '{}' not found",
                schema_name
            ))),
        }
    }

    /// Get document statistics
    fn get_statistics(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);

            let total_nodes = self.inner.ast.get_all_nodes().len();
            dict.set_item("total_nodes", total_nodes)?;

            // Count nodes by type
            let mut type_counts = HashMap::new();
            for node in self.inner.ast.get_all_nodes() {
                let type_name = format!("{:?}", node.node_type);
                *type_counts.entry(type_name).or_insert(0) += 1;
            }

            let type_dict = PyDict::new(py);
            for (node_type, count) in type_counts {
                type_dict.set_item(node_type, count)?;
            }
            dict.set_item("node_types", type_dict)?;

            dict.set_item(
                "version",
                format!("{}.{}", self.inner.version.major, self.inner.version.minor),
            )?;

            Ok(dict.into())
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "PdfDocument(version={}.{}, nodes={})",
            self.inner.version.major,
            self.inner.version.minor,
            self.inner.ast.get_all_nodes().len()
        )
    }
}

/// Python wrapper for AstNode
#[pyclass(name = "AstNode")]
#[derive(Clone)]
pub struct PyAstNode {
    inner: AstNode,
}

#[pymethods]
impl PyAstNode {
    /// Get node ID
    fn get_id(&self) -> u64 {
        self.inner.id.0 as u64
    }

    /// Get node type
    fn get_type(&self) -> String {
        format!("{:?}", self.inner.node_type)
    }

    /// Get node value as string representation
    fn get_value(&self) -> String {
        format!("{:?}", self.inner.value)
    }

    /// Get metadata
    fn get_metadata(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);

            dict.set_item("offset", self.inner.metadata.offset)?;
            dict.set_item("size", self.inner.metadata.size)?;
            dict.set_item("warnings", self.inner.metadata.warnings.clone())?;
            dict.set_item("errors", self.inner.metadata.errors.len())?;
            dict.set_item("properties", self.inner.metadata.properties.clone())?;

            Ok(dict.into())
        })
    }

    /// Check if node has specific property
    fn has_property(&self, key: &str) -> bool {
        match &self.inner.value {
            crate::types::PdfValue::Dictionary(dict) => dict.contains_key(key),
            _ => false,
        }
    }

    /// Get property value
    fn get_property(&self, key: &str) -> Option<String> {
        match &self.inner.value {
            crate::types::PdfValue::Dictionary(dict) => {
                dict.get(key).map(|value| format!("{:?}", value))
            }
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "AstNode(id={}, type={:?})",
            self.inner.id.0, self.inner.node_type
        )
    }
}

/// Python wrapper for ValidationReport
#[pyclass(name = "ValidationReport")]
#[derive(Clone)]
pub struct PyValidationReport {
    inner: ValidationReport,
}

#[pymethods]
impl PyValidationReport {
    /// Check if document is valid
    fn is_valid(&self) -> bool {
        self.inner.is_valid
    }

    /// Get schema name
    fn get_schema_name(&self) -> &str {
        &self.inner.schema_name
    }

    /// Get schema version
    fn get_schema_version(&self) -> &str {
        &self.inner.schema_version
    }

    /// Get validation issues
    fn get_issues(&self) -> Vec<PyValidationIssue> {
        self.inner
            .issues
            .iter()
            .map(|issue| PyValidationIssue {
                inner: issue.clone(),
            })
            .collect()
    }

    /// Get statistics
    fn get_statistics(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);

            dict.set_item("total_checks", self.inner.statistics.total_checks)?;
            dict.set_item("passed_checks", self.inner.statistics.passed_checks)?;
            dict.set_item("failed_checks", self.inner.statistics.failed_checks)?;
            dict.set_item("info_count", self.inner.statistics.info_count)?;
            dict.set_item("warning_count", self.inner.statistics.warning_count)?;
            dict.set_item("error_count", self.inner.statistics.error_count)?;
            dict.set_item("critical_count", self.inner.statistics.critical_count)?;

            Ok(dict.into())
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationReport(schema={}, valid={}, issues={})",
            self.inner.schema_name,
            self.inner.is_valid,
            self.inner.issues.len()
        )
    }
}

/// Python wrapper for ValidationIssue
#[pyclass(name = "ValidationIssue")]
#[derive(Clone)]
pub struct PyValidationIssue {
    inner: crate::validation::ValidationIssue,
}

#[pymethods]
impl PyValidationIssue {
    /// Get severity
    fn get_severity(&self) -> String {
        format!("{:?}", self.inner.severity)
    }

    /// Get error code
    fn get_code(&self) -> &str {
        &self.inner.code
    }

    /// Get message
    fn get_message(&self) -> &str {
        &self.inner.message
    }

    /// Get node ID if available
    fn get_node_id(&self) -> Option<u64> {
        self.inner.node_id.map(|id| id.0 as u64)
    }

    /// Get location if available
    fn get_location(&self) -> Option<&str> {
        self.inner.location.as_deref()
    }

    /// Get suggestion if available
    fn get_suggestion(&self) -> Option<&str> {
        self.inner.suggestion.as_deref()
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationIssue(severity={:?}, code={}, message={})",
            self.inner.severity, self.inner.code, self.inner.message
        )
    }
}

/// Python wrapper for PluginManager
#[pyclass(name = "PluginManager", unsendable)]
pub struct PyPluginManager {
    inner: PluginManager,
}

#[pymethods]
impl PyPluginManager {
    #[new]
    fn new() -> Self {
        Self {
            inner: PluginManager::new(),
        }
    }

    /// Load plugins from configuration file
    fn load_plugins_from_file(&mut self, path: &str) -> PyResult<Vec<String>> {
        match self.inner.load_plugins_from_file(path) {
            Ok(results) => Ok(results
                .into_iter()
                .map(|result| format!("{:?}", result))
                .collect()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                "Failed to load plugins: {}",
                e
            ))),
        }
    }

    /// Execute plugins on document
    fn execute_plugins(&self, py_document: &mut PyPdfDocument) -> PyResult<Py<PyDict>> {
        // Create mutable reference from Arc
        let mut document = (*py_document.inner).clone();
        let summary = self.inner.execute_plugins(&mut document);

        // Update the document
        py_document.inner = Arc::new(document);

        Python::attach(|py| {
            let dict = PyDict::new(py);

            dict.set_item("total_plugins", summary.total_plugins)?;
            dict.set_item("successful_plugins", summary.successful_plugins)?;
            dict.set_item("failed_plugins", summary.failed_plugins)?;
            dict.set_item("execution_time_ms", summary.total_execution_time_ms)?;

            // Convert plugin results
            let results_dict = PyDict::new(py);
            for (name, result) in summary.plugin_results {
                results_dict.set_item(name, format!("{:?}", result))?;
            }
            dict.set_item("plugin_results", results_dict)?;

            Ok(dict.into())
        })
    }

    /// List available plugins
    fn list_plugins(&self) -> Vec<Py<PyDict>> {
        Python::attach(|py| {
            self.inner
                .list_plugins()
                .into_iter()
                .map(|metadata| {
                    let dict = PyDict::new(py);
                    let _ = dict.set_item("name", &metadata.name);
                    let _ = dict.set_item("version", &metadata.version);
                    let _ = dict.set_item("description", &metadata.description);
                    let _ = dict.set_item("author", &metadata.author);
                    let _ = dict.set_item("tags", &metadata.tags);
                    dict.into()
                })
                .collect()
        })
    }

    fn __repr__(&self) -> String {
        "PluginManager()".to_string()
    }
}

/// Module-level functions
#[pyfunction]
fn parse_pdf(data: &Bound<'_, PyBytes>) -> PyResult<PyPdfDocument> {
    PyPdfDocument::from_bytes(data)
}

#[pyfunction]
fn get_available_schemas() -> Vec<String> {
    let registry = SchemaRegistry::new();
    registry
        .list_schemas()
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

#[pyfunction]
fn validate_document(document: &PyPdfDocument, schema_name: &str) -> PyResult<PyValidationReport> {
    document.validate(schema_name)
}

/// Python module
#[pymodule]
fn pdf_ast(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPdfDocument>()?;
    m.add_class::<PyAstNode>()?;
    m.add_class::<PyValidationReport>()?;
    m.add_class::<PyValidationIssue>()?;
    m.add_class::<PyPluginManager>()?;

    m.add_function(wrap_pyfunction!(parse_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(get_available_schemas, m)?)?;
    m.add_function(wrap_pyfunction!(validate_document, m)?)?;

    // Module constants
    m.add("__version__", "0.1.0")?;
    m.add("__author__", "PDF-AST Project")?;

    Ok(())
}
