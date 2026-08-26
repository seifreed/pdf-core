use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule};
use std::sync::Arc;

use pdf_ast_core::ast::{AstNode, NodeId, PdfDocument, PdfVersion};
use pdf_ast_core::parser::PdfParser;
use pdf_ast_core::plugins::api::PluginManager;
use pdf_ast_core::validation::{SchemaRegistry, ValidationReport};

#[pyclass(name = "PdfDocument", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPdfDocument {
    inner: Arc<PdfDocument>,
}

#[pymethods]
impl PyPdfDocument {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(PdfDocument::new(PdfVersion { major: 1, minor: 7 })),
        }
    }

    #[staticmethod]
    fn from_bytes(data: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let bytes = data.as_bytes();
        let parser = PdfParser::new();
        parser
            .parse_bytes(bytes)
            .map(|document| Self {
                inner: Arc::new(document),
            })
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {:?}", e)))
    }

    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| PyValueError::new_err(format!("Failed to open file: {}", e)))?;
        let reader = std::io::BufReader::new(file);
        let parser = PdfParser::new();
        parser
            .parse(reader)
            .map(|document| Self {
                inner: Arc::new(document),
            })
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {:?}", e)))
    }

    fn get_version(&self) -> (u8, u8) {
        (self.inner.version.major, self.inner.version.minor)
    }

    fn get_all_nodes(&self) -> Vec<PyAstNode> {
        self.inner
            .ast
            .get_all_nodes()
            .iter()
            .map(|node| PyAstNode {
                inner: (*node).clone(),
            })
            .collect()
    }

    fn get_root(&self) -> Option<PyAstNode> {
        self.inner
            .ast
            .get_root()
            .and_then(|node_id| self.inner.ast.get_node(node_id))
            .map(|node| PyAstNode {
                inner: node.clone(),
            })
    }

    fn get_node(&self, node_id: u64) -> Option<PyAstNode> {
        let node_id = usize::try_from(node_id).ok()?;
        self.inner
            .ast
            .get_node(NodeId(node_id))
            .map(|node| PyAstNode {
                inner: node.clone(),
            })
    }

    fn get_children(&self, node_id: u64) -> Vec<PyAstNode> {
        let Ok(node_id) = usize::try_from(node_id) else {
            return Vec::new();
        };
        self.inner
            .ast
            .get_children(NodeId(node_id))
            .iter()
            .filter_map(|child_id| self.inner.ast.get_node(*child_id))
            .map(|node| PyAstNode {
                inner: node.clone(),
            })
            .collect()
    }

    fn get_nodes_by_type(&self, node_type: &str) -> Vec<PyAstNode> {
        match pdf_ast_core::bindings::utils::parse_node_type(node_type) {
            Ok(nt) => self
                .inner
                .ast
                .get_nodes_by_type(nt)
                .iter()
                .filter_map(|node_id| self.inner.ast.get_node(*node_id))
                .map(|node| PyAstNode {
                    inner: node.clone(),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn validate(&self, schema_name: &str) -> PyResult<PyValidationReport> {
        let registry = SchemaRegistry::new();
        match registry.validate(&self.inner, schema_name) {
            Some(report) => Ok(PyValidationReport { inner: report }),
            None => Err(PyValueError::new_err(format!(
                "Schema '{}' not found",
                schema_name
            ))),
        }
    }

    fn get_statistics(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);
            let total_nodes = self.inner.ast.get_all_nodes().len();
            dict.set_item("total_nodes", total_nodes)?;
            dict.set_item(
                "version",
                format!("{}.{}", self.inner.version.major, self.inner.version.minor),
            )?;
            dict.set_item("page_count", self.inner.metadata.page_count)?;
            dict.set_item("has_javascript", self.inner.metadata.has_javascript)?;
            dict.set_item("has_embedded_files", self.inner.metadata.has_embedded_files)?;
            dict.set_item("has_signatures", self.inner.metadata.has_signatures)?;
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

#[pyclass(name = "AstNode", skip_from_py_object)]
#[derive(Clone)]
pub struct PyAstNode {
    inner: AstNode,
}

#[pymethods]
impl PyAstNode {
    fn get_id(&self) -> u64 {
        self.inner.id.0 as u64
    }

    fn get_type(&self) -> String {
        format!("{:?}", self.inner.node_type)
    }

    fn get_value(&self) -> String {
        format!("{:?}", self.inner.value)
    }

    fn get_metadata(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("offset", self.inner.metadata.offset)?;
            dict.set_item("size", self.inner.metadata.size)?;
            dict.set_item("warnings", self.inner.metadata.warnings.clone())?;
            dict.set_item("error_count", self.inner.metadata.errors.len())?;
            dict.set_item("properties", self.inner.metadata.properties.clone())?;
            Ok(dict.into())
        })
    }

    fn has_property(&self, key: &str) -> bool {
        match &self.inner.value {
            pdf_ast_core::types::PdfValue::Dictionary(dict) => dict.contains_key(key),
            _ => false,
        }
    }

    fn get_property(&self, key: &str) -> Option<String> {
        match &self.inner.value {
            pdf_ast_core::types::PdfValue::Dictionary(dict) => {
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

#[pyclass(name = "ValidationReport", skip_from_py_object)]
#[derive(Clone)]
pub struct PyValidationReport {
    inner: ValidationReport,
}

#[pymethods]
impl PyValidationReport {
    fn is_valid(&self) -> bool {
        self.inner.is_valid
    }

    fn get_schema_name(&self) -> &str {
        &self.inner.schema_name
    }

    fn get_schema_version(&self) -> &str {
        &self.inner.schema_version
    }

    fn get_issues(&self) -> Vec<PyValidationIssue> {
        self.inner
            .issues
            .iter()
            .map(|issue| PyValidationIssue {
                inner: issue.clone(),
            })
            .collect()
    }

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

#[pyclass(name = "ValidationIssue", skip_from_py_object)]
#[derive(Clone)]
pub struct PyValidationIssue {
    inner: pdf_ast_core::validation::ValidationIssue,
}

#[pymethods]
impl PyValidationIssue {
    fn get_severity(&self) -> String {
        format!("{:?}", self.inner.severity)
    }

    fn get_code(&self) -> &str {
        &self.inner.code
    }

    fn get_message(&self) -> &str {
        &self.inner.message
    }

    fn get_node_id(&self) -> Option<u64> {
        self.inner.node_id.map(|id| id.0 as u64)
    }
}

#[pyclass(name = "PluginManager")]
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

    fn load_plugins_from_file(&mut self, path: &str) -> PyResult<Vec<String>> {
        match self.inner.load_plugins_from_file(path) {
            Ok(results) => Ok(results
                .into_iter()
                .map(|res| format!("{:?}", res))
                .collect()),
            Err(err) => Err(PyValueError::new_err(format!(
                "Failed to load plugins: {}",
                err
            ))),
        }
    }

    fn execute_plugins(&self, document: &PyPdfDocument) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let mut doc = (*document.inner).clone();
            let summary = self.inner.execute_plugins(&mut doc);
            let dict = PyDict::new(py);
            dict.set_item("total_plugins", summary.total_plugins)?;
            dict.set_item("successful_plugins", summary.successful_plugins)?;
            dict.set_item("failed_plugins", summary.failed_plugins)?;
            let results = PyDict::new(py);
            for (name, result) in summary.plugin_results {
                results.set_item(name, format!("{:?}", result))?;
            }
            dict.set_item("plugin_results", results)?;
            Ok(dict.into())
        })
    }

    fn list_plugins(&self) -> Vec<Py<PyDict>> {
        Python::attach(|py| {
            self.inner
                .list_plugins()
                .iter()
                .map(|meta| {
                    let dict = PyDict::new(py);
                    dict.set_item("name", meta.name.clone()).ok();
                    dict.set_item("version", meta.version.clone()).ok();
                    dict.set_item("description", meta.description.clone()).ok();
                    dict.set_item("author", meta.author.clone()).ok();
                    let tags = meta.tags.clone();
                    dict.set_item("tags", tags).ok();
                    dict.into()
                })
                .collect()
        })
    }
}

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
    let registry = SchemaRegistry::new();
    match registry.validate(&document.inner, schema_name) {
        Some(report) => Ok(PyValidationReport { inner: report }),
        None => Err(PyValueError::new_err(format!(
            "Schema '{}' not found",
            schema_name
        ))),
    }
}

#[pymodule]
fn pdf_ast(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPdfDocument>()?;
    m.add_class::<PyAstNode>()?;
    m.add_class::<PyValidationReport>()?;
    m.add_class::<PyValidationIssue>()?;
    m.add_class::<PyPluginManager>()?;
    m.add_function(wrap_pyfunction!(parse_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(get_available_schemas, m)?)?;
    m.add_function(wrap_pyfunction!(validate_document, m)?)?;
    Ok(())
}
