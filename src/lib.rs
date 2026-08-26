//! PDF-AST: An experimental PDF analysis library providing Abstract Syntax Tree representation
//! and security analysis capabilities for PDF documents.
//!
//! This library offers:
//! - PDF parsing with controlled tolerance for malformed documents
//! - AST-based graph representation of PDF structure
//! - Advanced security analysis including signature verification
//! - Multimedia content extraction (audio, video, 3D, RichMedia)
//! - XFA form analysis and script detection
//! - Performance profiling and optimization tools
//! - Multiple output formats (JSON, YAML, TOML)
//!
//! Copyright (C) 2026 Marc Rivero López
//! Licensed under the MIT License
//! See LICENSE file for details

/// Abstract Syntax Tree implementation for PDF documents.
pub mod ast;
/// Language bindings for Python and JavaScript.
pub mod bindings;
/// PDF specification constants and enumerations.
pub mod constants;
/// Cryptographic operations for signatures and encryption.
pub mod crypto;
/// Event hooks for parser/AST instrumentation.
pub mod events;
/// Foreign Function Interface for C interoperability.
pub mod ffi;
/// Stream filters (compression, encoding, decoding).
pub mod filters;
/// AcroForm and XFA form processing.
pub mod forms;
/// Document metadata extraction and parsing.
pub mod metadata;
/// Core PDF parsing functionality.
pub mod parser;
/// Performance monitoring and profiling.
pub mod performance;
/// Plugin architecture for extensibility.
pub mod plugins;
/// Security analysis and signature verification.
pub mod security;
/// Graph serialization and deserialization.
pub mod serialization;
/// Document transformation utilities.
pub mod transform;
/// Traversal helpers and walker traits.
pub mod traversal;
/// Core PDF data types (objects, arrays, dictionaries).
pub mod types;
/// Document validation and compliance checking.
pub mod validation;
/// Visitor pattern for AST traversal.
pub mod visitor;

// Export simplified Python bindings when Python feature is enabled
#[cfg(feature = "python")]
pub use bindings::python_simple::*;
pub mod api;
pub mod compression;
pub mod multimedia;
pub mod recovery;
pub mod schema;
pub mod streaming;

pub use ast::{
    AstError, AstNode, AstResult, NodeId, NodeType, ParseDiagnostic, PdfAstGraph, PdfDocument,
    PdfVersion,
};
pub use compression::{
    create_optimal_compressor, AdvancedCompressor, CompressionConfig, CompressionLevel,
    CompressionResult,
};
pub use events::AstEventListener;
pub use forms::{
    count_fields_in_acroform, has_hybrid_forms, AcroFormStats, XfaDocument, XfaNode, XfaPacket,
    XfaScriptStats,
};
pub use multimedia::av::{AudioInfo, VideoInfo};
pub use multimedia::richmedia::RichMediaInfo;
pub use multimedia::threed::ThreeDInfo;
pub use parser::PdfParser;
pub use performance::{
    get_performance_stats, start_timer, PerformanceAnalyzer, PerformanceConfig, PerformanceReport,
    PerformanceStats,
};
pub use security::etsi::{validate_etsi_profiles, EtsiValidationOptions};
pub use security::ltv::LtvInfo;
pub use security::{
    report_output::format_security_report, report_output::SecurityOutputFormat,
    security_info_to_report, security_report_to_json, security_report_to_toml,
    security_report_to_yaml, DigitalSignature, SecurityAnalyzer, SecurityInfo, SecurityReport,
};
pub use serialization::{GraphDeserializer, SerializableGraph};
pub use traversal::{AstWalker, GraphWalker, TimelineWalker};
pub use types::{
    ObjectId, PdfArray, PdfDictionary, PdfName, PdfReference, PdfStream, PdfString, PdfValue,
};
pub use visitor::{QueryBuilder, Visitor, VisitorAction};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types() {
        let name = PdfName::new("Type");
        assert_eq!(name.as_str(), "/Type");

        let string = PdfString::new_literal(b"Hello PDF");
        assert_eq!(string.to_string_lossy(), "Hello PDF");

        let mut array = PdfArray::new();
        array.push(PdfValue::Integer(42));
        array.push(PdfValue::Boolean(true));
        assert_eq!(array.len(), 2);

        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("Catalog")));
        assert!(dict.contains_key("Type"));
    }

    #[test]
    fn test_ast_graph() {
        let mut graph = PdfAstGraph::new();
        let root_value = PdfValue::Dictionary(PdfDictionary::new());
        let root_id = graph.create_node(NodeType::Root, root_value);
        graph.set_root(root_id);

        let child_value = PdfValue::Dictionary(PdfDictionary::new());
        let child_id = graph.create_node(NodeType::Page, child_value);
        graph.add_edge(root_id, child_id, crate::ast::EdgeType::Child);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        assert!(!graph.is_cyclic());
    }
}
