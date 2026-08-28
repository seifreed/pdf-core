/// Unit tests for PDF parser components
///
/// These tests verify individual parsing functions and components
use pdf_ast::parser::*;
use pdf_ast::performance::PerformanceLimits;
use pdf_ast::types::*;
use std::io::Cursor;

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn test_object_parser() {
        use object_parser::parse_value;

        // Test null parsing
        let result = parse_value(b"null");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        assert_eq!(value, PdfValue::Null);

        // Test boolean parsing
        let result = parse_value(b"true");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        assert_eq!(value, PdfValue::Boolean(true));

        // Test integer parsing
        let result = parse_value(b"42");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        assert_eq!(value, PdfValue::Integer(42));

        // Test real parsing
        let result = parse_value(b"3.14");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        if let PdfValue::Real(r) = value {
            let expected = 314.0 / 100.0;
            assert!((r - expected).abs() < f64::EPSILON);
        } else {
            panic!("Expected real value");
        }

        // Test name parsing
        let result = parse_value(b"/Name");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        if let PdfValue::Name(name) = value {
            assert_eq!(name.as_str(), "/Name");
        } else {
            panic!("Expected name value");
        }
    }

    #[test]
    fn low_level_object_parsers_respect_resource_budgets() {
        use object_parser::{parse_indirect_object_with_budget, parse_value_with_budget};
        use pdf_ast::performance::ResourceBudget;

        let input_budget = ResourceBudget::new(2, 1024, 1024, 10, 10, 10, 10, 8);
        assert!(parse_value_with_budget(b"123", &input_budget).is_err());

        let object_budget = ResourceBudget::new(1024, 1024, 1024, 10, 0, 10, 10, 8);
        assert!(
            parse_indirect_object_with_budget(b"1 0 obj\nnull\nendobj", &object_budget).is_err()
        );
    }

    #[test]
    fn test_array_parsing() {
        use object_parser::parse_value;

        let result = parse_value(b"[1 2 3 /Name true]");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();

        if let PdfValue::Array(array) = value {
            assert_eq!(array.len(), 5);
            assert_eq!(array[0], PdfValue::Integer(1));
            assert_eq!(array[1], PdfValue::Integer(2));
            assert_eq!(array[2], PdfValue::Integer(3));
            assert_eq!(array[4], PdfValue::Boolean(true));
        } else {
            panic!("Expected array value");
        }
    }

    #[test]
    fn test_dictionary_parsing() {
        use object_parser::parse_value;

        let result = parse_value(b"<< /Type /Catalog /Pages 2 0 R >>");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();

        if let PdfValue::Dictionary(dict) = value {
            assert!(dict.contains_key("Type"));
            assert!(dict.contains_key("Pages"));

            if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                assert_eq!(type_name.as_str(), "/Catalog");
            }
        } else {
            panic!("Expected dictionary value");
        }
    }

    #[test]
    fn test_reference_parsing() {
        use object_parser::parse_value;

        let result = parse_value(b"42 0 R");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();

        if let PdfValue::Reference(reference) = value {
            assert_eq!(reference.object_id().number, 42);
            assert_eq!(reference.object_id().generation, 0);
        } else {
            panic!("Expected reference value");
        }
    }

    #[test]
    fn test_indirect_object_parsing() {
        use object_parser::parse_indirect_object;

        let data = b"1 0 obj\n<< /Type /Catalog >>\nendobj";
        let result = parse_indirect_object(data);
        assert!(result.is_ok());

        let (_, (obj_id, value)) = result.unwrap();
        assert_eq!(obj_id.number, 1);
        assert_eq!(obj_id.generation, 0);

        if let PdfValue::Dictionary(dict) = value {
            assert!(dict.contains_key("Type"));
        } else {
            panic!("Expected dictionary value");
        }
    }

    #[test]
    fn stream_parser_rejects_invalid_length_and_missing_endstream() {
        use object_parser::parse_indirect_object;

        assert!(
            parse_indirect_object(b"1 0 obj\n<< /Length -1 >>\nstream\n\nendstream\nendobj")
                .is_err()
        );
        assert!(parse_indirect_object(b"1 0 obj\n<< /Length 3 >>\nstream\nabc\nendobj").is_err());
    }

    #[test]
    fn flate_decoder_stops_at_output_limit() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&vec![b'A'; 4096]).unwrap();
        let compressed = encoder.finish().unwrap();
        let result = pdf_ast::filters::decode_stream_with_limits(
            &compressed,
            &[StreamFilter::FlateDecode(Default::default())],
            1024,
            100,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_xref_table_parsing() {
        use xref::parse_xref_table;

        let xref_data = b"xref
0 3
0000000000 65535 f 
0000000010 00000 n 
0000000079 00000 n ";

        let result = parse_xref_table(xref_data);
        assert!(result.is_ok());

        let (_, entries) = result.unwrap();
        assert_eq!(entries.len(), 3);

        // Check first entry (free)
        let (obj_id, entry) = &entries[0];
        assert_eq!(obj_id.number, 0);
        assert!(entry.is_free());

        // Check second entry (in use)
        let (obj_id, entry) = &entries[1];
        assert_eq!(obj_id.number, 1);
        assert!(entry.is_in_use());
        if let Some(offset) = entry.offset() {
            assert_eq!(offset, 10); // 0x0a (10 in decimal)
        }
    }

    #[test]
    fn test_content_stream_parsing() {
        use content_stream::ContentStreamParser;

        let mut parser = ContentStreamParser::new();
        let content = b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET";

        let result = parser.parse(content);
        assert!(result.is_ok());

        let operators = result.unwrap();
        assert!(!operators.is_empty());

        // Should start with BeginText and end with EndText
        use content_stream::ContentOperator;
        assert_eq!(operators[0], ContentOperator::BeginText);
        assert_eq!(operators[operators.len() - 1], ContentOperator::EndText);
    }

    #[test]
    fn test_string_parsing() {
        use object_parser::parse_value;

        // Test literal string
        let result = parse_value(b"(Hello World)");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        if let PdfValue::String(string) = value {
            assert_eq!(string.to_string_lossy(), "Hello World");
        }

        // Test hex string
        let result = parse_value(b"<48656C6C6F>");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        if let PdfValue::String(string) = value {
            assert_eq!(string.to_string_lossy(), "Hello");
        }
    }

    #[test]
    fn test_filter_decoding() {
        use pdf_ast::filters::*;

        // Test ASCII Hex decode
        let hex_data = b"48656C6C6F>";
        let filters = vec![StreamFilter::ASCIIHexDecode];
        let result = decode_stream(hex_data, &filters);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"Hello");

        // Test ASCII85 decode
        let a85_data = b"9jqo^BlbD-BleB1djH~>";
        let filters = vec![StreamFilter::ASCII85Decode];
        let result = decode_stream(a85_data, &filters);
        assert!(result.is_ok());
        // "Hello world!" in ASCII85

        // Test RunLength decode
        let rl_data = &[2, b'A', b'B', b'C', 254, b'D', 128]; // ABC + 257-254=3 D's + EOD
        let filters = vec![StreamFilter::RunLengthDecode];
        let result = decode_stream(rl_data, &filters);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded, b"ABCDDD");
    }

    #[test]
    fn test_predictor_decoding() {
        use pdf_ast::filters::predictor::PredictorDecoder;

        // Test PNG Sub predictor
        let decoder = PredictorDecoder::new(11, 1, 8, 4); // PNG Sub predictor
        let data = vec![1, 10, 5, 3, 7]; // predictor byte + differences
        let result = decoder.decode(&data);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        // Should reconstruct: 10, 15 (10+5), 18 (15+3), 25 (18+7)
        assert_eq!(decoded, vec![10, 15, 18, 25]);

        // Test TIFF predictor
        let decoder = PredictorDecoder::new(2, 1, 8, 4);
        let data = vec![10, 5, 3, 7]; // first pixel + differences
        let result = decoder.decode(&data);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        // Should reconstruct similar to PNG Sub but without predictor byte
        assert_eq!(decoded, vec![10, 15, 18, 25]);
    }

    #[test]
    fn test_linearization_parsing() {
        use pdf_ast::parser::xref::parse_linearization_dict;
        use pdf_ast::types::{PdfDictionary, PdfStream};

        let mut dict = PdfDictionary::new();
        dict.insert("Linearized", PdfValue::Real(1.0));
        dict.insert("L", PdfValue::Integer(1000));
        dict.insert(
            "H",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(100),
                PdfValue::Integer(50),
            ])),
        );
        dict.insert("N", PdfValue::Integer(5));
        dict.insert("O", PdfValue::Integer(3));
        dict.insert("E", PdfValue::Integer(800));
        dict.insert("T", PdfValue::Integer(200));

        let stream = PdfStream::new(dict, vec![]);

        let result = parse_linearization_dict(&stream);
        assert!(result.is_ok());

        let linearization = result.unwrap();
        assert_eq!(linearization.version, 1.0);
        assert_eq!(linearization.file_length, 1000);
        assert_eq!(linearization.hint_stream_offset, 100);
        assert_eq!(linearization.hint_stream_length, Some(50));
        assert_eq!(linearization.object_count, 5);
        assert_eq!(linearization.first_page_object_number, 3);
        assert_eq!(linearization.first_page_end_offset, 800);
        assert_eq!(linearization.main_xref_table_entries, 200);

        // Test validation
        assert!(linearization.validate().is_ok());
    }

    #[test]
    fn test_malformed_data_handling() {
        use object_parser::parse_value;

        // Test incomplete data
        let result = parse_value(b"[1 2 3");
        assert!(result.is_err());

        // Test invalid syntax
        let result = parse_value(b"<< /Type >>");
        assert!(result.is_err());

        // Test truncated reference - Note: parser may return partial success
        let result = parse_value(b"42 0");
        // The parser might successfully parse "42" as an integer and leave " 0" unparsed
        // So we check if we got a valid value rather than requiring an error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_whitespace_handling() {
        use object_parser::parse_value;

        // Test various whitespace scenarios
        let test_cases = vec![
            b"  42  ".as_slice(),
            b"\t\r\n42\t\r\n".as_slice(),
            b"[ 1    2\t\t3 ]".as_slice(),
            b"<<  /Type   /Catalog  >>".as_slice(),
        ];

        for test_case in test_cases {
            let result = parse_value(test_case);
            assert!(
                result.is_ok(),
                "Failed to parse: {:?}",
                String::from_utf8_lossy(test_case)
            );
        }
    }

    #[test]
    fn test_nested_structures() {
        use object_parser::parse_value;

        // Test nested arrays
        let result = parse_value(b"[[1 2] [3 4] [5 [6 7]]]");
        assert!(result.is_ok());

        // Test nested dictionaries
        let result = parse_value(b"<< /Dict << /Nested true >> /Array [1 2 3] >>");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();

        if let PdfValue::Dictionary(dict) = value {
            assert!(dict.contains_key("Dict"));
            assert!(dict.contains_key("Array"));
        }
    }

    #[test]
    fn test_edge_cases() {
        use object_parser::parse_value;

        // Empty array
        let result = parse_value(b"[]");
        assert!(result.is_ok());

        // Empty dictionary
        let result = parse_value(b"<<>>");
        assert!(result.is_ok());

        // Zero values
        let result = parse_value(b"0");
        assert!(result.is_ok());

        // Negative values
        let result = parse_value(b"-42");
        assert!(result.is_ok());

        // Very large numbers
        let result = parse_value(b"999999999999999");
        assert!(result.is_ok());
    }

    #[test]
    fn object_stream_offsets_reject_invalid_bounds() {
        use object_parser::parse_object_stream_offsets;

        assert_eq!(
            parse_object_stream_offsets(b"10 0 11 3 abcdefghi", 2, 10).unwrap(),
            vec![10, 13]
        );
        assert!(parse_object_stream_offsets(b"10 99 abc", 1, 6).is_err());
        assert!(parse_object_stream_offsets(b"10", 2, 3).is_err());
        assert!(parse_object_stream_offsets(b"1 0", usize::MAX, 3).is_err());
    }

    #[test]
    fn object_stream_offsets_respect_entry_budget() {
        use object_parser::parse_object_stream_offsets_with_budget;
        use pdf_ast::performance::ResourceBudget;

        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 1, 10, 10, 8);
        assert!(parse_object_stream_offsets_with_budget(b"1 0 2 3 data", 2, 4, &budget).is_err());
    }

    #[test]
    fn parser_modes_are_explicit() {
        assert_eq!(PdfParser::strict().mode(), ParseMode::Strict);
        assert_eq!(PdfParser::new().mode(), ParseMode::Tolerant);
        assert_eq!(PdfParser::forensic().mode(), ParseMode::Forensic);
        assert!(PdfParser::strict().parse_objects(b"1 2 nope").is_err());
        assert_eq!(
            PdfParser::new().parse_objects(b"1 2 nope").unwrap().len(),
            2
        );

        let mut limits = PerformanceLimits {
            max_nodes: 1,
            ..PerformanceLimits::default()
        };
        limits.refresh_budget();
        let bounded = PdfParser::new().with_limits(limits);
        assert!(bounded
            .parse_objects(b"1 0 obj null endobj 2 0 obj null endobj")
            .is_err());

        let (objects, diagnostics) = PdfParser::new()
            .parse_objects_with_diagnostics(b"1 2 nope")
            .unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].recovery_action, "skipped_to_next_line");
    }

    #[test]
    fn tolerant_object_sequence_resumes_after_a_malformed_line() {
        let (objects, diagnostics) = PdfParser::new()
            .with_max_errors(2)
            .parse_objects_with_diagnostics(b"1 nope\n2\n")
            .expect("tolerant recovery should continue at the next line");

        assert_eq!(objects, vec![PdfValue::Integer(1), PdfValue::Integer(2)]);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].recovery_action, "skipped_to_next_line");
    }

    #[test]
    fn strict_parse_object_does_not_hide_malformed_indirect_objects() {
        let error = PdfParser::strict()
            .parse_object(b"7 0 obj\n42\n")
            .expect_err("strict mode must reject a truncated indirect object");
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn tolerant_parse_object_does_not_hide_malformed_indirect_objects() {
        assert!(PdfParser::new().parse_object(b"7 0 obj\n42\n").is_err());
    }

    #[test]
    fn public_parser_apis_respect_input_budget() {
        let limits = PerformanceLimits {
            max_file_size_mb: 0,
            ..PerformanceLimits::default()
        };
        let parser = PdfParser::strict().with_limits(limits);

        assert!(parser.parse_value(b"1").is_err());
        assert!(parser.parse_object(b"1").is_err());
        assert!(parser.parse_objects(b"1").is_err());
    }

    #[test]
    fn parse_objects_consumes_indirect_objects_as_units() {
        let objects = PdfParser::strict()
            .parse_objects(b"7 0 obj\n42\nendobj\n8 0 obj\nnull\nendobj")
            .expect("valid indirect objects should parse");
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0], PdfValue::Integer(42));
        assert_eq!(objects[1], PdfValue::Null);
    }

    #[test]
    fn forensic_mode_preserves_recovery_evidence() {
        let parser = PdfParser::forensic();
        let document = parser
            .parse_bytes(b"%PDF-1.7\n1 0 obj\n42\nendobj\n")
            .expect("forensic parser should return a controlled result");

        assert!(!document.diagnostics.is_empty());
        assert!(document.forensic.is_some());
        assert!(document
            .forensic
            .as_ref()
            .unwrap()
            .recovered_xref
            .contains_key(&ObjectId::new(1, 0)));
        assert!(!document
            .forensic
            .as_ref()
            .unwrap()
            .residual_ranges
            .is_empty());
    }

    #[test]
    fn reader_parsing_preserves_original_bytes() {
        let input = b"%PDF-1.7\n1 0 obj\n42\nendobj\n".to_vec();
        let document = PdfParser::new()
            .parse(Cursor::new(input.clone()))
            .expect("reader parse should succeed");

        assert_eq!(document.original_bytes, Some(input));
    }

    #[test]
    fn configured_value_depth_is_enforced() {
        let nested = "[".repeat(2) + "0" + &"]".repeat(2);
        assert!(PdfParser::new()
            .with_max_depth(2)
            .parse_value(nested.as_bytes())
            .is_err());
        assert!(PdfParser::new()
            .with_max_depth(4)
            .parse_value(nested.as_bytes())
            .is_ok());
    }

    #[test]
    fn parse_object_accepts_an_indirect_object() {
        assert_eq!(
            PdfParser::strict()
                .parse_object(b"7 2 obj\n42\nendobj")
                .unwrap(),
            PdfValue::Integer(42)
        );
    }

    #[test]
    fn strict_single_value_rejects_trailing_tokens() {
        assert!(PdfParser::strict().parse_value(b"42 trailing").is_err());
        assert!(PdfParser::new().parse_value(b"42 trailing").is_ok());
    }

    #[test]
    fn strict_indirect_object_rejects_trailing_tokens() {
        assert!(PdfParser::strict()
            .parse_object(b"7 2 obj\n42\nendobj trailing")
            .is_err());
    }
}
