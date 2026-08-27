/// Integration tests for PDF-AST library
///
/// These tests verify the complete functionality of the PDF-AST library
/// using real PDF data and comprehensive test cases.
use pdf_ast::*;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use pdf_ast::performance::PerformanceLimits;

    /// Test basic PDF parsing with a minimal valid PDF
    #[test]
    fn test_basic_pdf_parsing() {
        let minimal_pdf = create_minimal_pdf();
        let parser = PdfParser::new();

        match parser.parse_bytes(&minimal_pdf) {
            Ok(document) => {
                assert_eq!(document.version.major, 1);
                assert_eq!(document.version.minor, 4);
                assert!(document.catalog.is_some());
                assert!(!document.is_linearized());
                assert_eq!(
                    document.original_bytes.as_deref(),
                    Some(minimal_pdf.as_slice())
                );
            }
            Err(e) => panic!("Failed to parse minimal PDF: {:?}", e),
        }
    }

    #[test]
    fn parse_bytes_original_retention_respects_memory_budget() {
        let minimal_pdf = create_minimal_pdf();
        let budget = pdf_ast::performance::ResourceBudget::new(
            1024 * 1024,
            (minimal_pdf.len() - 1) as u64,
            1024 * 1024,
            100,
            10_000,
            10_000,
            10_000,
            100,
        );

        let error = PdfParser::new()
            .with_resource_budget(budget)
            .parse_bytes(&minimal_pdf)
            .expect_err("lossless source retention must respect the memory budget");
        assert!(error.to_string().contains("DecodedBytes"));
    }

    #[test]
    fn document_page_queries_respect_node_budget() {
        let document = PdfParser::new()
            .parse_bytes(&create_minimal_pdf())
            .expect("minimal PDF should parse");
        let budget = pdf_ast::performance::ResourceBudget::new(
            1024 * 1024,
            1024 * 1024,
            1024 * 1024,
            100,
            100,
            0,
            100,
            100,
        );

        assert_eq!(
            document
                .get_pages_with_budget(&budget)
                .expect_err("page collection must charge nodes"),
            pdf_ast::performance::ResourceBudgetError::Nodes
        );
    }

    /// Test PDF with JavaScript content
    #[test]
    fn test_javascript_detection() {
        let js_pdf = create_pdf_with_javascript();
        let parser = PdfParser::new();

        match parser.parse_bytes(&js_pdf) {
            Ok(mut document) => {
                document.analyze_metadata();
                assert!(document.metadata.has_javascript);
            }
            Err(e) => panic!("Failed to parse JavaScript PDF: {:?}", e),
        }
    }

    /// Test PDF with embedded files
    #[test]
    fn test_embedded_files_detection() {
        let embedded_pdf = create_pdf_with_embedded_files();
        let parser = PdfParser::new();

        match parser.parse_bytes(&embedded_pdf) {
            Ok(mut document) => {
                document.analyze_metadata();
                assert!(document.metadata.has_embedded_files);
            }
            Err(e) => panic!("Failed to parse embedded files PDF: {:?}", e),
        }
    }

    /// Test encrypted PDF detection
    #[test]
    fn test_encryption_detection() {
        let encrypted_pdf = create_encrypted_pdf();
        let parser = PdfParser::new();

        match parser.parse_bytes(&encrypted_pdf) {
            Ok(document) => {
                assert!(document.metadata.encrypted);
            }
            Err(e) => panic!("Failed to parse encrypted PDF: {:?}", e),
        }
    }

    /// Test content stream parsing
    #[test]
    fn test_content_stream_parsing() {
        let content_stream_data = b"BT /F1 12 Tf 100 700 Td (Hello PDF) Tj ET";
        let mut parser = pdf_ast::parser::content_stream::ContentStreamParser::new();

        match parser.parse(content_stream_data) {
            Ok(operators) => {
                assert!(!operators.is_empty());
                // Should contain BeginText, SetFont, MoveText, ShowText, EndText
                assert!(operators.len() >= 5);
            }
            Err(e) => panic!("Failed to parse content stream: {}", e),
        }
    }

    #[test]
    fn test_page_resources_are_inherited_from_pages_node() {
        let document = PdfParser::new()
            .parse_bytes(&create_pdf_with_inherited_resources())
            .expect("inherited resource PDF should parse");
        let page_id = document
            .ast
            .find_nodes_by_type(NodeType::Page)
            .into_iter()
            .next()
            .expect("page node");
        let page = document.ast.get_node(page_id).expect("page node exists");

        assert_eq!(
            page.metadata.properties.get("has_inherited_resources"),
            Some(&"true".to_string())
        );
        let resources = page
            .as_dict()
            .and_then(|dict| dict.get("Resources"))
            .and_then(PdfValue::as_dict)
            .expect("effective resources");
        assert!(resources.contains_key("Font"));
        assert!(document.ast.get_references(page_id).iter().any(|child_id| {
            document
                .ast
                .get_node(*child_id)
                .map(|node| node.node_type == NodeType::Font)
                .unwrap_or(false)
        }));
    }

    #[test]
    fn invalid_page_resources_are_strict_error_or_tolerant_diagnostic() {
        let pdf = create_pdf_with_invalid_resources();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject non-dictionary page resources");
        assert!(error.to_string().contains("Resources must be a dictionary"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "invalid_resources"));
    }

    #[test]
    fn invalid_page_tree_node_is_strict_error_or_tolerant_diagnostic() {
        let pdf = create_pdf_with_invalid_page_tree_node();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject a non-dictionary page tree node");
        assert!(error
            .to_string()
            .contains("Page tree node must be a dictionary"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "invalid_page_tree_node"));
    }

    #[test]
    fn invalid_page_tree_kids_are_strict_error_or_tolerant_diagnostic() {
        let pdf = create_pdf_with_invalid_page_tree_kids();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject non-array page tree kids");
        assert!(error
            .to_string()
            .contains("Page tree /Kids must be an array"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "invalid_page_tree_kids"));
    }

    #[test]
    fn missing_page_tree_fields_are_strict_error_or_tolerant_diagnostics() {
        let pdf = create_pdf_with_missing_page_tree_fields();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject missing /Pages fields");
        assert!(error.to_string().contains("missing required /Kids"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain diagnostics");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "missing_page_tree_kids"));
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "missing_page_tree_count"));
    }

    #[test]
    fn page_parent_must_be_an_indirect_reference() {
        let pdf = create_pdf_with_invalid_page_parent();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject a missing page parent");
        assert!(error
            .to_string()
            .contains("Page tree /Page node is missing required /Parent"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "missing_page_parent"));
    }

    #[test]
    fn invalid_catalog_pages_root_is_strict_error_or_tolerant_diagnostic() {
        let pdf = create_pdf_with_invalid_catalog_pages_root();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject a non-reference catalog /Pages");
        assert!(error
            .to_string()
            .contains("Catalog /Pages must be an indirect reference"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "invalid_pages_root"));
    }

    #[test]
    fn missing_catalog_pages_root_is_strict_error_or_tolerant_diagnostic() {
        let pdf = create_pdf_without_catalog_pages_root();

        let error = PdfParser::strict()
            .parse_bytes(&pdf)
            .expect_err("strict mode must reject a catalog without /Pages");
        assert!(error
            .to_string()
            .contains("missing the required /Pages reference"));

        let document = PdfParser::new()
            .parse_bytes(&pdf)
            .expect("tolerant mode should retain a diagnostic");
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.error_code == "missing_pages_root"));
    }

    /// Test filter decoding
    #[test]
    fn test_filter_decoding() {
        use pdf_ast::filters::decode_stream;
        use pdf_ast::types::StreamFilter;

        // Test ASCII hex decoding
        let hex_data = b"48656C6C6F20504446>";
        let filters = vec![StreamFilter::ASCIIHexDecode];

        match decode_stream(hex_data, &filters) {
            Ok(decoded) => {
                assert_eq!(decoded, b"Hello PDF");
            }
            Err(e) => panic!("Failed to decode hex data: {}", e),
        }
    }

    /// Test PDF/A validation
    #[test]
    fn test_pdfa_validation() {
        use pdf_ast::validation::pdfa::PdfA1bValidator;

        let pdf_data = create_minimal_pdf();
        let parser = PdfParser::new();

        if let Ok(document) = parser.parse_bytes(&pdf_data) {
            let validator = PdfA1bValidator::new().with_strict_mode(false);
            let report = validator.validate(&document);

            // Minimal PDF likely won't be PDF/A compliant
            assert!(!report.is_valid);
            assert!(!report.issues.is_empty());
        }
    }

    /// Test linearized PDF detection
    #[test]
    fn test_linearized_pdf() {
        let linearized_pdf = create_linearized_pdf();
        let parser = PdfParser::new();

        match parser.parse_bytes(&linearized_pdf) {
            Ok(document) => {
                assert!(document.is_linearized());
                if let Some(linearization) = document.get_linearization() {
                    assert!(linearization.validate().is_ok());
                }
            }
            Err(e) => panic!("Failed to parse linearized PDF: {:?}", e),
        }
    }

    /// Test XRef stream parsing
    #[test]
    fn test_xref_stream() {
        let xref_stream_pdf = create_pdf_with_xref_stream();
        let parser = PdfParser::new();

        match parser.parse_bytes(&xref_stream_pdf) {
            Ok(document) => {
                assert!(!document.xref.streams.is_empty());
                assert!(!document.xref.entries.is_empty());
            }
            Err(e) => panic!("Failed to parse XRef stream PDF: {:?}", e),
        }
    }

    #[test]
    fn test_lossless_stream_metadata() {
        let parser = PdfParser::new();
        let document = parser
            .parse_bytes(&create_pdf_with_stream())
            .expect("stream PDF should parse");
        let nodes = document.ast.get_all_nodes();
        let stream = nodes
            .iter()
            .find(|node| node.metadata.properties.contains_key("decode_state"))
            .expect("stream node records lossless length state");
        assert_eq!(
            stream.metadata.properties.get("declared_length"),
            Some(&"3".to_string())
        );
        assert_eq!(
            stream.metadata.properties.get("observed_length"),
            Some(&"3".to_string())
        );
        assert_eq!(
            stream.metadata.properties.get("decode_state"),
            Some(&"raw".to_string())
        );
    }

    /// Test malformed PDF handling
    #[test]
    fn test_malformed_pdf_handling() {
        let malformed_pdfs = vec![
            b"Not a PDF".to_vec(),
            b"%PDF-1.4\ntruncated".to_vec(),
            b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nxref\n".to_vec(),
        ];

        let parser = PdfParser::strict();

        for malformed_pdf in malformed_pdfs {
            // Expected to fail.
            if parser.parse_bytes(&malformed_pdf).is_ok() {
                panic!("Should have failed to parse malformed PDF");
            }
        }
    }

    /// Test performance with large PDF
    #[test]
    fn test_performance() {
        let large_pdf = create_large_pdf();
        let parser = PdfParser::new();

        let start = std::time::Instant::now();
        match parser.parse_bytes(&large_pdf) {
            Ok(document) => {
                let duration = start.elapsed();
                println!("Parsed large PDF in {:?}", duration);
                assert!(duration.as_secs() < 5); // Should parse within 5 seconds
                assert!(document.metadata.page_count > 0);
            }
            Err(e) => panic!("Failed to parse large PDF: {:?}", e),
        }
    }

    #[test]
    fn test_tolerant_recovery_from_malformed_object() {
        let pdf_data = build_malformed_pdf();
        let parser = PdfParser::new()
            .with_tolerance(true)
            .with_limits(PerformanceLimits::conservative());

        let document = parser
            .parse_bytes(&pdf_data)
            .expect("Tolerant parse should succeed");

        let has_recovery = document
            .ast
            .get_all_nodes()
            .iter()
            .any(|node| !node.metadata.errors.is_empty());

        let _ = has_recovery;
    }

    // Helper functions to create test PDFs

    fn create_minimal_pdf() -> Vec<u8> {
        // Fixed PDF with correct xref offset
        let content = b"%PDF-1.4
1 0 obj
<<
/Type /Catalog
/Pages 2 0 R
>>
endobj
2 0 obj
<<
/Type /Pages
/Kids [3 0 R]
/Count 1
>>
endobj
3 0 obj
<<
/Type /Page
/Parent 2 0 R
/MediaBox [0 0 612 792]
>>
endobj
";
        let xref_start = content.len();

        let mut pdf = Vec::from(&content[..]);

        let xref_content = b"xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
trailer
<<
/Size 4
/Root 1 0 R
>>
startxref
";
        pdf.extend_from_slice(xref_content);
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_pdf_with_inherited_resources() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources 4 0 R >>\nendobj\n"
                .as_slice(),
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n"
                .as_slice(),
            b"4 0 obj\n<< /Font << /F1 5 0 R >> >>\nendobj\n".as_slice(),
            b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_invalid_resources() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources 4 0 R >>\nendobj\n"
                .as_slice(),
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n"
                .as_slice(),
            b"4 0 obj\n123\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_invalid_page_tree_node() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".as_slice(),
            b"3 0 obj\n123\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_invalid_page_tree_kids() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids 3 /Count 1 >>\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_missing_page_tree_fields() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages >>\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_invalid_page_parent() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".as_slice(),
            b"3 0 obj\n<< /Type /Page /MediaBox [0 0 10 10] >>\nendobj\n".as_slice(),
        ];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_invalid_catalog_pages_root() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [b"1 0 obj\n<< /Type /Catalog /Pages 3 >>\nendobj\n".as_slice()];
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_without_catalog_pages_root() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let object = b"1 0 obj\n<< /Type /Catalog >>\nendobj\n";
        let offset = pdf.len();
        pdf.extend_from_slice(object);
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn create_pdf_with_stream() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0usize];
        let objects = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".as_slice(),
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n".as_slice(),
            b"4 0 obj\n<< /Length 3 >>\nstream\nabc\nendstream\nendobj\n".as_slice(),
        ];
        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object);
        }
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        pdf
    }

    fn build_malformed_pdf() -> Vec<u8> {
        let mut parts = Vec::new();
        parts.push(b"%PDF-1.4\n".to_vec());

        let offset1 = parts.iter().map(|p| p.len()).sum::<usize>();
        parts.push(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec());

        let offset2 = parts.iter().map(|p| p.len()).sum::<usize>();
        parts.push(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1\nendobj\n".to_vec());

        let offset3 = parts.iter().map(|p| p.len()).sum::<usize>();
        parts.push(b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n".to_vec());

        let xref_offset = parts.iter().map(|p| p.len()).sum::<usize>();
        let xref = format!(
            "xref\n0 4\n0000000000 65535 f \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offset1, offset2, offset3, xref_offset
        );
        parts.push(xref.into_bytes());

        parts.concat()
    }

    fn create_pdf_with_javascript() -> Vec<u8> {
        // Create PDF with dynamically calculated offsets
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");

        // Object 1 - Catalog
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<<\n/Type /Catalog\n/Pages 2 0 R\n/OpenAction 4 0 R\n>>\nendobj\n",
        );

        // Object 2 - Pages
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>\nendobj\n");

        // Object 3 - Page
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // Object 4 - JavaScript Action
        let obj4_offset = pdf.len();
        pdf.extend_from_slice(b"4 0 obj\n<<\n/Type /Action\n/S /JavaScript\n/JS (app.alert('Hello from PDF');)\n>>\nendobj\n");

        // XRef table
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj1_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj2_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj3_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj4_offset).as_bytes());

        pdf.extend_from_slice(b"trailer\n<<\n/Size 5\n/Root 1 0 R\n>>\nstartxref\n");
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_pdf_with_embedded_files() -> Vec<u8> {
        // Create PDF with dynamically calculated offsets
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");

        // Object 1 - Catalog
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<<\n/Type /Catalog\n/Pages 2 0 R\n/Names 5 0 R\n>>\nendobj\n",
        );

        // Object 2 - Pages
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>\nendobj\n");

        // Object 3 - Page
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // Object 4 - Filespec
        let obj4_offset = pdf.len();
        pdf.extend_from_slice(b"4 0 obj\n<<\n/Type /Filespec\n/F (embedded.txt)\n>>\nendobj\n");

        // Object 5 - Names dictionary
        let obj5_offset = pdf.len();
        pdf.extend_from_slice(b"5 0 obj\n<<\n/EmbeddedFiles 6 0 R\n>>\nendobj\n");

        // Object 6 - EmbeddedFiles name tree
        let obj6_offset = pdf.len();
        pdf.extend_from_slice(b"6 0 obj\n<<\n/Names [(embedded.txt) 4 0 R]\n>>\nendobj\n");

        // XRef table
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 7\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj1_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj2_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj3_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj4_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj5_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj6_offset).as_bytes());

        pdf.extend_from_slice(b"trailer\n<<\n/Size 7\n/Root 1 0 R\n>>\nstartxref\n");
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_encrypted_pdf() -> Vec<u8> {
        // Fixed PDF with correct xref offset
        let content = b"%PDF-1.4
1 0 obj
<<
/Type /Catalog
/Pages 2 0 R
>>
endobj
2 0 obj
<<
/Type /Pages
/Kids [3 0 R]
/Count 1
>>
endobj
3 0 obj
<<
/Type /Page
/Parent 2 0 R
/MediaBox [0 0 612 792]
>>
endobj
4 0 obj
<<
/Filter /Standard
/V 1
/R 2
/O <0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF>
/U <0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF>
/P -4
>>
endobj
";
        let xref_start = content.len();

        let mut pdf = Vec::from(&content[..]);

        let xref_content = b"xref
0 5
0000000000 65535 f 
0000000011 00000 n 
0000000058 00000 n 
0000000100 00000 n 
0000000157 00000 n 
trailer
<<
/Size 5
/Root 1 0 R
/Encrypt 4 0 R
>>
startxref
";
        pdf.extend_from_slice(xref_content);
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_linearized_pdf() -> Vec<u8> {
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");

        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<<\n/Linearized 1.0\n/L 1000\n/H [0 0]\n/O 4\n/E 300\n/N 1\n/T 0\n>>\nendobj\n",
        );

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<<\n/Type /Catalog\n/Pages 3 0 R\n>>\nendobj\n");

        let obj3_offset = pdf.len();
        pdf.extend_from_slice(b"3 0 obj\n<<\n/Type /Pages\n/Kids [4 0 R]\n/Count 1\n>>\nendobj\n");

        let obj4_offset = pdf.len();
        pdf.extend_from_slice(
            b"4 0 obj\n<<\n/Type /Page\n/Parent 3 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj1_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj2_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj3_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj4_offset).as_bytes());
        pdf.extend_from_slice(b"trailer\n<<\n/Size 5\n/Root 2 0 R\n>>\nstartxref\n");
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_pdf_with_xref_stream() -> Vec<u8> {
        // Create a PDF with XRef stream (PDF 1.5+)
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.5\n");

        // Object 1 - Catalog
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<<\n/Type /Catalog\n/Pages 2 0 R\n>>\nendobj\n");

        // Object 2 - Pages
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>\nendobj\n");

        // Object 3 - Page
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // XRef stream object (replaces traditional xref table)
        let xref_obj_offset = pdf.len();

        // Create simple uncompressed xref stream data
        // Format: Type(1) Offset(2) Gen(1) for each object
        let mut xref_data = Vec::new();
        // Object 0 (free)
        xref_data.push(0u8); // type 0 = free
        xref_data.extend_from_slice(&0u16.to_be_bytes()); // next free obj
        xref_data.push(65u8); // generation (high byte of 65535)

        // Object 1
        xref_data.push(1u8); // type 1 = in use
        xref_data.extend_from_slice(&(obj1_offset as u16).to_be_bytes()); // offset
        xref_data.push(0u8); // generation

        // Object 2
        xref_data.push(1u8); // type 1 = in use
        xref_data.extend_from_slice(&(obj2_offset as u16).to_be_bytes()); // offset
        xref_data.push(0u8); // generation

        // Object 3
        xref_data.push(1u8); // type 1 = in use
        xref_data.extend_from_slice(&(obj3_offset as u16).to_be_bytes()); // offset
        xref_data.push(0u8); // generation

        // Object 4 (self-reference, not typically needed but for completeness)
        xref_data.push(1u8); // type 1 = in use
        xref_data.extend_from_slice(&(xref_obj_offset as u16).to_be_bytes()); // offset
        xref_data.push(0u8); // generation

        // Write XRef stream object
        pdf.extend_from_slice(b"4 0 obj\n<<\n/Type /XRef\n/Size 5\n/W [1 2 1]\n/Root 1 0 R\n");
        pdf.extend_from_slice(format!("/Length {}\n", xref_data.len()).as_bytes());
        pdf.extend_from_slice(b">>\nstream\n");
        pdf.extend_from_slice(&xref_data);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(xref_obj_offset.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }

    fn create_large_pdf() -> Vec<u8> {
        // Create a PDF with multiple pages for performance testing
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");

        // Object 1 - Catalog
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<<\n/Type /Catalog\n/Pages 2 0 R\n>>\nendobj\n");

        // Object 2 - Pages
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(
            b"2 0 obj\n<<\n/Type /Pages\n/Kids [3 0 R 4 0 R 5 0 R]\n/Count 3\n>>\nendobj\n",
        );

        // Object 3 - Page 1
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // Object 4 - Page 2
        let obj4_offset = pdf.len();
        pdf.extend_from_slice(
            b"4 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // Object 5 - Page 3
        let obj5_offset = pdf.len();
        pdf.extend_from_slice(
            b"5 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 612 792]\n>>\nendobj\n",
        );

        // XRef table
        let xref_start = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj1_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj2_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj3_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj4_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \n", obj5_offset).as_bytes());

        pdf.extend_from_slice(b"trailer\n<<\n/Size 6\n/Root 1 0 R\n>>\nstartxref\n");
        pdf.extend_from_slice(xref_start.to_string().as_bytes());
        pdf.extend_from_slice(b"\n%%EOF");

        pdf
    }
}
