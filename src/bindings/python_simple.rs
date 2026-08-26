use pyo3::exceptions::PyValueError;
/// Simplified Python bindings for PDF-AST
///
/// This module provides basic Python bindings that work with the current codebase state
/// and can be incrementally improved as the rest of the library stabilizes.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyModule};
use std::collections::HashMap;

/// Simplified PDF document wrapper
#[pyclass(name = "SimplePdfDocument")]
pub struct PySimplePdfDocument {
    version: (u8, u8),
    objects: Vec<u8>,
    metadata: HashMap<String, String>,
}

#[pymethods]
impl PySimplePdfDocument {
    #[new]
    fn new() -> Self {
        Self {
            version: (1, 4),
            objects: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Parse basic PDF information from bytes
    #[staticmethod]
    fn from_bytes(data: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let bytes = data.as_bytes();

        // Basic PDF validation
        if bytes.len() < 4 || &bytes[0..4] != b"%PDF" {
            return Err(PyValueError::new_err("Not a valid PDF file"));
        }

        // Extract version from header
        let version = if bytes.len() >= 8 {
            match (bytes[5], bytes[7]) {
                (b'1', b'0') => (1, 0),
                (b'1', b'1') => (1, 1),
                (b'1', b'2') => (1, 2),
                (b'1', b'3') => (1, 3),
                (b'1', b'4') => (1, 4),
                (b'1', b'5') => (1, 5),
                (b'1', b'6') => (1, 6),
                (b'1', b'7') => (1, 7),
                (b'2', b'0') => (2, 0),
                _ => (1, 4), // Default fallback
            }
        } else {
            (1, 4)
        };

        let mut metadata = HashMap::new();
        metadata.insert("file_size".to_string(), bytes.len().to_string());

        // Basic object counting (simplified)
        let obj_count = bytes.windows(4).filter(|w| w == b" obj").count();
        metadata.insert("object_count_estimate".to_string(), obj_count.to_string());

        Ok(Self {
            version,
            objects: bytes.to_vec(),
            metadata,
        })
    }

    /// Get PDF version
    fn get_version(&self) -> (u8, u8) {
        self.version
    }

    /// Get file size
    fn get_file_size(&self) -> usize {
        self.objects.len()
    }

    /// Get basic statistics
    fn get_statistics(&self) -> PyResult<Py<PyDict>> {
        Python::attach(|py| {
            let dict = PyDict::new(py);

            dict.set_item("version", format!("{}.{}", self.version.0, self.version.1))?;
            dict.set_item("file_size", self.objects.len())?;

            for (key, value) in &self.metadata {
                dict.set_item(key, value)?;
            }

            Ok(dict.into())
        })
    }

    /// Check if file contains JavaScript
    fn has_javascript(&self) -> bool {
        let data = &self.objects;
        // Look for common JavaScript indicators
        data.windows(10).any(|w| w == b"/JS" || w == b"/JavaScript")
            || data.windows(6).any(|w| w == b"/JS <<")
    }

    /// Check if file has embedded files
    fn has_embedded_files(&self) -> bool {
        let data = &self.objects;
        data.windows(15).any(|w| w == b"/EmbeddedFiles")
            || data.windows(9).any(|w| w == b"/Filespec")
    }

    /// Check if file is encrypted
    fn is_encrypted(&self) -> bool {
        let data = &self.objects;
        data.windows(8).any(|w| w == b"/Encrypt")
    }

    /// Count approximate number of pages
    fn get_page_count(&self) -> usize {
        let data = &self.objects;
        // Count /Type /Page occurrences
        let mut count = 0;
        let mut i = 0;
        while i < data.len().saturating_sub(10) {
            if &data[i..i + 5] == b"/Type" {
                // Look for /Page in the next 50 bytes
                let end = std::cmp::min(i + 50, data.len());
                if data[i..end].windows(5).any(|w| w == b"/Page") {
                    count += 1;
                }
            }
            i += 1;
        }
        count
    }

    /// Extract strings (simplified)
    fn extract_strings(&self, min_length: Option<usize>) -> Vec<String> {
        let min_len = min_length.unwrap_or(4);
        let data = &self.objects;
        let mut strings = Vec::new();
        let mut current_string = Vec::new();

        for &byte in data {
            if byte.is_ascii_graphic() || byte == b' ' {
                current_string.push(byte);
            } else {
                if current_string.len() >= min_len {
                    if let Ok(s) = String::from_utf8(current_string.clone()) {
                        strings.push(s);
                    }
                }
                current_string.clear();
            }
        }

        // Don't return too many strings to avoid memory issues
        strings.truncate(1000);
        strings
    }

    fn __repr__(&self) -> String {
        format!(
            "SimplePdfDocument(version={}.{}, size={} bytes)",
            self.version.0,
            self.version.1,
            self.objects.len()
        )
    }
}

/// Basic PDF validation result
#[pyclass(name = "ValidationResult")]
pub struct PyValidationResult {
    is_valid: bool,
    issues: Vec<String>,
    warnings: Vec<String>,
}

#[pymethods]
impl PyValidationResult {
    /// Check if the PDF is valid
    fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Get validation issues
    fn get_issues(&self) -> Vec<String> {
        self.issues.clone()
    }

    /// Get validation warnings
    fn get_warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationResult(valid={}, issues={}, warnings={})",
            self.is_valid,
            self.issues.len(),
            self.warnings.len()
        )
    }
}

/// Simple validation function
#[pyfunction]
fn validate_pdf_basic(document: &PySimplePdfDocument) -> PyValidationResult {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    // Basic validation checks
    if document.objects.len() < 100 {
        issues.push("File is too small to be a valid PDF".to_string());
    }

    if !document.objects.starts_with(b"%PDF") {
        issues.push("Missing PDF header".to_string());
    }

    // Check for EOF marker
    if !document.objects.ends_with(b"%%EOF")
        && !document.objects.ends_with(b"%%EOF\n")
        && !document.objects.ends_with(b"%%EOF\r\n")
    {
        warnings.push("Missing or malformed EOF marker".to_string());
    }

    // Look for xref table
    if !document.objects.windows(4).any(|w| w == b"xref") {
        warnings.push("No xref table found".to_string());
    }

    // Look for trailer
    if !document.objects.windows(7).any(|w| w == b"trailer") {
        warnings.push("No trailer found".to_string());
    }

    let is_valid = issues.is_empty();

    PyValidationResult {
        is_valid,
        issues,
        warnings,
    }
}

/// Parse PDF from bytes (convenience function)
#[pyfunction]
fn parse_pdf_simple(data: &Bound<'_, PyBytes>) -> PyResult<PySimplePdfDocument> {
    PySimplePdfDocument::from_bytes(data)
}

/// Get library version
#[pyfunction]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Simple Python module
#[pymodule]
fn pdf_ast_simple(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySimplePdfDocument>()?;
    m.add_class::<PyValidationResult>()?;

    m.add_function(wrap_pyfunction!(parse_pdf_simple, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pdf_basic, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;

    // Module constants
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add(
        "__doc__",
        "Simplified PDF-AST Python bindings for basic PDF analysis",
    )?;

    Ok(())
}
