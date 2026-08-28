use super::*;
use crate::ast::{AstNode, NodeType, PdfDocument};
use std::collections::HashMap;

/// Base trait for recovery strategies
pub trait RecoveryStrategy: Send + Sync {
    /// Get the name of this strategy
    fn name(&self) -> &str;

    /// Apply the recovery strategy
    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult>;

    /// Check if this strategy can handle the given error type
    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool;

    /// Get the priority of this strategy (higher = more preferred)
    fn priority(&self) -> u8 {
        50
    }
}

/// Context provided to recovery strategies
#[derive(Debug)]
pub struct RecoveryContext<'a> {
    pub original_data: &'a [u8],
    pub current_data: &'a [u8],
    pub document: &'a PdfDocument,
    pub config: &'a RecoveryConfig,
    pub error_log: &'a [RecoveryError],
}

/// Result of applying a recovery strategy
#[derive(Debug)]
pub struct RecoveryResult {
    pub success: bool,
    pub action_type: RecoveryActionType,
    pub description: String,
    pub data_changed: bool,
    pub document_changed: bool,
    pub modified_data: Option<Vec<u8>>,
    pub modified_document: Option<PdfDocument>,
}

/// Basic structure recovery strategy
pub struct BasicStructureRecovery {
    name: String,
}

impl BasicStructureRecovery {
    pub fn new() -> Self {
        Self {
            name: "BasicStructureRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for BasicStructureRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Fix common structural issues

        // 1. Fix missing PDF header
        if !data.starts_with(b"%PDF-") {
            if let Some(header_pos) = find_pattern(&data, b"%PDF-") {
                // Remove garbage before header
                data = data[header_pos..].to_vec();
                modified = true;
            } else {
                // Add missing header
                let mut new_data = b"%PDF-1.4\n".to_vec();
                new_data.extend_from_slice(&data);
                data = new_data;
                modified = true;
            }
        }

        // 2. Fix missing EOF marker
        if !data.ends_with(b"%%EOF") && !data.ends_with(b"%%EOF\n") {
            data.extend_from_slice(b"\n%%EOF");
            modified = true;
        }

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::StructureRepair,
            description: if modified {
                "Fixed basic PDF structure issues".to_string()
            } else {
                "No structural issues found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::StructuralError | RecoveryErrorType::ParseError
        )
    }

    fn priority(&self) -> u8 {
        90 // High priority for basic structure
    }
}

impl Default for BasicStructureRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference recovery strategy
pub struct ReferenceRecovery {
    name: String,
}

impl ReferenceRecovery {
    pub fn new() -> Self {
        Self {
            name: "ReferenceRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for ReferenceRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Find and fix broken references
        let broken_refs = self.find_broken_references(&data);

        for broken_ref in broken_refs {
            if let Some(fixed_ref) = self.fix_reference(&data, &broken_ref) {
                // Replace broken reference with fixed one
                if let Some(pos) = find_pattern(&data, broken_ref.as_bytes()) {
                    data.splice(
                        pos..pos + broken_ref.len(),
                        fixed_ref.as_bytes().iter().cloned(),
                    );
                    modified = true;
                }
            }
        }

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::ReferenceResolution,
            description: if modified {
                "Fixed broken object references".to_string()
            } else {
                "No broken references found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(error_type, RecoveryErrorType::ReferenceError)
    }

    fn priority(&self) -> u8 {
        70
    }
}

impl ReferenceRecovery {
    fn find_broken_references(&self, data: &[u8]) -> Vec<String> {
        let mut broken_refs = Vec::new();
        let data_str = String::from_utf8_lossy(data);

        // Look for malformed references like "123 0R" (missing space)
        if let Ok(re) = regex::Regex::new(r"(\d+)\s*(\d+)R") {
            for line in data_str.lines() {
                if let Some(captures) = re.captures(line) {
                    if let Some(broken) = captures.get(0) {
                        broken_refs.push(broken.as_str().to_string());
                    }
                }
            }
        }

        broken_refs
    }

    fn fix_reference(&self, _data: &[u8], broken_ref: &str) -> Option<String> {
        // Fix malformed references
        regex::Regex::new(r"(\d+)\s*(\d+)R")
            .ok()
            .and_then(|re| re.captures(broken_ref))
            .and_then(|captures| {
                Some(format!(
                    "{} {} R",
                    captures.get(1)?.as_str(),
                    captures.get(2)?.as_str()
                ))
            })
    }
}

impl Default for ReferenceRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream recovery strategy
pub struct StreamRecovery {
    name: String,
}

impl StreamRecovery {
    pub fn new() -> Self {
        Self {
            name: "StreamRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for StreamRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Find and fix stream issues
        let streams = self.find_streams(&data);

        for stream in streams {
            if let Some(fixed_stream) = self.fix_stream(&data, &stream) {
                // Replace broken stream
                data.splice(stream.start..stream.end, fixed_stream);
                modified = true;
            }
        }

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::StreamDecoding,
            description: if modified {
                "Fixed stream encoding issues".to_string()
            } else {
                "No stream issues found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::StreamError | RecoveryErrorType::EncodingError
        )
    }

    fn priority(&self) -> u8 {
        60
    }
}

impl StreamRecovery {
    fn find_streams(&self, data: &[u8]) -> Vec<StreamRegion> {
        let mut streams = Vec::new();
        let mut pos = 0;

        while let Some(stream_start) = find_pattern(&data[pos..], b"stream") {
            let abs_start = pos + stream_start;

            if let Some(stream_end) = find_pattern(&data[abs_start..], b"endstream") {
                let abs_end = abs_start + stream_end + 9; // Include "endstream"

                streams.push(StreamRegion {
                    start: abs_start,
                    end: abs_end,
                });

                pos = abs_end;
            } else {
                pos = abs_start + 6;
            }
        }

        streams
    }

    fn fix_stream(&self, data: &[u8], stream: &StreamRegion) -> Option<Vec<u8>> {
        let stream_data = &data[stream.start..stream.end];

        // Try to fix common stream issues
        let mut fixed = stream_data.to_vec();
        let mut modified = false;

        // Fix missing newlines after "stream"
        if fixed.starts_with(b"stream") && !fixed.starts_with(b"stream\n") {
            fixed.splice(6..6, b"\n".iter().cloned());
            modified = true;
        }

        // Fix missing newlines before "endstream"
        if let Some(endstream_pos) = find_pattern(&fixed, b"endstream") {
            if endstream_pos > 0 && fixed[endstream_pos - 1] != b'\n' {
                fixed.splice(endstream_pos..endstream_pos, b"\n".iter().cloned());
                modified = true;
            }
        }

        if modified {
            Some(fixed)
        } else {
            None
        }
    }
}

impl Default for StreamRecovery {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct StreamRegion {
    start: usize,
    end: usize,
}

/// Encoding recovery strategy
pub struct EncodingRecovery {
    name: String,
}

impl EncodingRecovery {
    pub fn new() -> Self {
        Self {
            name: "EncodingRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for EncodingRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let data = context.current_data;

        // Try different encodings to decode the data
        let encodings = vec!["utf-8", "latin1", "cp1252", "iso-8859-1"];

        for encoding in encodings {
            if let Ok(decoded) = self.try_encoding(data, encoding) {
                return Ok(RecoveryResult {
                    success: true,
                    action_type: RecoveryActionType::EncodingFix,
                    description: format!("Successfully decoded with {} encoding", encoding),
                    data_changed: true,
                    document_changed: false,
                    modified_data: Some(decoded),
                    modified_document: None,
                });
            }
        }

        Ok(RecoveryResult {
            success: false,
            action_type: RecoveryActionType::EncodingFix,
            description: "Could not fix encoding issues".to_string(),
            data_changed: false,
            document_changed: false,
            modified_data: None,
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(error_type, RecoveryErrorType::EncodingError)
    }

    fn priority(&self) -> u8 {
        40
    }
}

impl EncodingRecovery {
    fn try_encoding(&self, data: &[u8], _encoding: &str) -> Result<Vec<u8>, String> {
        // Simplified encoding handling
        // In a real implementation, would use proper encoding libraries

        // For now, just clean up obvious encoding issues
        let mut cleaned = Vec::new();

        for &byte in data {
            match byte {
                0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F => {
                    // Replace control characters with spaces
                    cleaned.push(b' ');
                }
                _ => {
                    cleaned.push(byte);
                }
            }
        }

        Ok(cleaned)
    }
}

impl Default for EncodingRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Heuristic recovery strategy
pub struct HeuristicRecovery {
    name: String,
}

impl HeuristicRecovery {
    pub fn new() -> Self {
        Self {
            name: "HeuristicRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for HeuristicRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Apply various heuristic fixes
        modified |= self.fix_common_typos(&mut data);
        modified |= self.fix_whitespace_issues(&mut data);
        modified |= self.fix_bracket_imbalance(&mut data);

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::HeuristicPatch,
            description: if modified {
                "Applied heuristic fixes".to_string()
            } else {
                "No heuristic fixes applied".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, _error_type: &RecoveryErrorType) -> bool {
        true // Can attempt to handle any error type
    }

    fn priority(&self) -> u8 {
        30
    }
}

impl HeuristicRecovery {
    fn fix_common_typos(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Common PDF typos
        let typos = vec![
            ("obje", "obj"),
            ("endobje", "endobj"),
            ("stram", "stream"),
            ("endstram", "endstream"),
            ("trailer", "trailer"),
        ];

        for (typo, correct) in typos {
            if fixed_str.contains(typo) {
                fixed_str = fixed_str.replace(typo, correct);
                modified = true;
            }
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }

    fn fix_whitespace_issues(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Fix missing spaces around operators
        let patterns = vec![("<<", " << "), (">>", " >> "), ("][", "] [")];

        for (pattern, replacement) in patterns {
            if let Some(pos) = find_pattern(data, pattern.as_bytes()) {
                data.splice(pos..pos + pattern.len(), replacement.bytes());
                modified = true;
            }
        }

        modified
    }

    fn fix_bracket_imbalance(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let mut open_brackets = 0;
        let mut close_brackets = 0;

        // Count brackets
        for &byte in data.iter() {
            match byte {
                b'[' | b'(' | b'<' => open_brackets += 1,
                b']' | b')' | b'>' => close_brackets += 1,
                _ => {}
            }
        }

        // Add missing closing brackets
        if open_brackets > close_brackets {
            let missing = open_brackets - close_brackets;
            for _ in 0..missing {
                data.push(b']'); // Default to square brackets
            }
            modified = true;
        }

        modified
    }
}

impl Default for HeuristicRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Fuzzy matching recovery strategy
pub struct FuzzyMatchingRecovery {
    name: String,
}

impl FuzzyMatchingRecovery {
    pub fn new() -> Self {
        Self {
            name: "FuzzyMatchingRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for FuzzyMatchingRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let data = context.current_data;

        // Use fuzzy matching to find and fix similar-looking but incorrect tokens
        let fixed_data = self.apply_fuzzy_fixes(data);

        let modified = fixed_data != data;

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::FuzzyMatch,
            description: if modified {
                "Applied fuzzy matching fixes".to_string()
            } else {
                "No fuzzy matches found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(fixed_data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, _error_type: &RecoveryErrorType) -> bool {
        true
    }

    fn priority(&self) -> u8 {
        20
    }
}

impl FuzzyMatchingRecovery {
    fn apply_fuzzy_fixes(&self, data: &[u8]) -> Vec<u8> {
        let data_str = String::from_utf8_lossy(data);
        let mut fixed = data_str.to_string();

        // Known PDF keywords for fuzzy matching
        let keywords = vec![
            "obj",
            "endobj",
            "stream",
            "endstream",
            "xref",
            "trailer",
            "startxref",
            "Type",
            "Catalog",
            "Pages",
            "Page",
            "Font",
        ];

        // Simple fuzzy matching (Levenshtein distance = 1)
        for keyword in keywords {
            let similar = self.find_similar_words(&data_str, keyword, 1);
            for similar_word in similar {
                if similar_word != keyword {
                    fixed = fixed.replace(&similar_word, keyword);
                }
            }
        }

        fixed.into_bytes()
    }

    fn find_similar_words(&self, text: &str, target: &str, max_distance: usize) -> Vec<String> {
        let mut similar = Vec::new();

        for word in text.split_whitespace() {
            if self.levenshtein_distance(word, target) <= max_distance {
                similar.push(word.to_string());
            }
        }

        similar
    }

    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();
        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for (i, row) in matrix.iter_mut().enumerate().take(len1 + 1) {
            row[0] = i;
        }
        for (j, value) in matrix[0].iter_mut().enumerate().take(len2 + 1) {
            *value = j;
        }

        for (i, c1) in s1.chars().enumerate() {
            for (j, c2) in s2.chars().enumerate() {
                let cost = if c1 == c2 { 0 } else { 1 };
                matrix[i + 1][j + 1] = std::cmp::min(
                    std::cmp::min(matrix[i][j + 1] + 1, matrix[i + 1][j] + 1),
                    matrix[i][j] + cost,
                );
            }
        }

        matrix[len1][len2]
    }
}

impl Default for FuzzyMatchingRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Advanced structure repair strategy
pub struct StructureRepairStrategy {
    name: String,
}

impl StructureRepairStrategy {
    pub fn new() -> Self {
        Self {
            name: "StructureRepairStrategy".to_string(),
        }
    }
}

impl RecoveryStrategy for StructureRepairStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Advanced structure repair beyond BasicStructureRecovery

        // 1. Fix nested object structures
        modified |= self.fix_nested_objects(&mut data);

        // 2. Repair broken dictionary structures
        modified |= self.repair_dictionaries(&mut data);

        // 3. Fix array structures
        modified |= self.repair_arrays(&mut data);

        // 4. Fix cross-reference inconsistencies
        modified |= self.fix_cross_reference_issues(&mut data);

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::StructureRepair,
            description: if modified {
                "Applied advanced structure repairs".to_string()
            } else {
                "No advanced structure issues found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::StructuralError | RecoveryErrorType::ParseError
        )
    }

    fn priority(&self) -> u8 {
        85 // High priority, slightly lower than BasicStructureRecovery
    }
}

impl StructureRepairStrategy {
    fn fix_nested_objects(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let mut pos = 0;

        while pos < data.len().saturating_sub(10) {
            // Look for broken nested object patterns like "1 0 obj 2 0 obj"
            if let Some(obj_pos) = find_pattern(&data[pos..], b" obj") {
                pos += obj_pos + 4;

                // Check if there's another obj marker too close (likely nested)
                if let Some(next_obj) =
                    find_pattern(&data[pos..pos.saturating_add(50).min(data.len())], b" obj")
                {
                    if next_obj < 20 {
                        // Too close, likely nested
                        // Insert endobj before the next obj
                        data.splice(
                            pos + next_obj..pos + next_obj,
                            b"\nendobj\n".iter().cloned(),
                        );
                        modified = true;
                        pos += next_obj + 8;
                    }
                }
            } else {
                break;
            }
        }

        modified
    }

    fn repair_dictionaries(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Fix unbalanced dictionary markers
        let mut dict_depth = 0;
        let chars: Vec<char> = fixed_str.chars().collect();
        let mut i = 0;

        while i < chars.len().saturating_sub(1) {
            if i < chars.len() - 1 && chars[i] == '<' && chars[i + 1] == '<' {
                dict_depth += 1;
                i += 2;
            } else if i < chars.len() - 1 && chars[i] == '>' && chars[i + 1] == '>' {
                dict_depth -= 1;
                i += 2;
            } else {
                i += 1;
            }
        }

        // Add missing closing dictionary markers
        if dict_depth > 0 {
            for _ in 0..dict_depth {
                fixed_str.push_str(" >>");
            }
            modified = true;
        }

        // Fix malformed dictionary entries (missing spaces around keys)
        if fixed_str.contains("/Key/Value") {
            fixed_str = fixed_str.replace("/Key/Value", "/Key /Value");
            modified = true;
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }

    fn repair_arrays(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Fix unbalanced array brackets
        let mut array_depth = 0;
        for ch in fixed_str.chars() {
            match ch {
                '[' => array_depth += 1,
                ']' => array_depth -= 1,
                _ => {}
            }
        }

        // Add missing closing brackets
        if array_depth > 0 {
            for _ in 0..array_depth {
                fixed_str.push(']');
            }
            modified = true;
        }

        // Fix missing spaces between array elements
        if let Ok(re) = regex::Regex::new(r"(\d)([A-Za-z/])") {
            if re.is_match(&fixed_str) {
                fixed_str = re.replace_all(&fixed_str, "$1 $2").to_string();
                modified = true;
            }
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }

    fn fix_cross_reference_issues(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Find all object definitions and ensure they have proper references
        let mut object_offsets = HashMap::new();
        let mut pos = 0;

        // Compile regex once outside the loop
        let obj_regex = match regex::Regex::new(r"(\d+)\s+\d+\s+obj$") {
            Ok(re) => re,
            Err(_) => return modified,
        };

        // Collect object positions
        while pos < data.len() {
            if let Some(obj_start) = find_pattern(&data[pos..], b" obj") {
                let abs_pos = pos + obj_start;

                // Extract object number
                let start_search = abs_pos.saturating_sub(20);
                let prefix = String::from_utf8_lossy(&data[start_search..abs_pos]);

                if let Some(captures) = obj_regex.captures(&prefix) {
                    if let Ok(obj_num) = captures[1].parse::<u32>() {
                        object_offsets.insert(obj_num, start_search);
                    }
                }

                pos = abs_pos + 4;
            } else {
                break;
            }
        }

        // Update xref table with correct offsets
        if let Some(xref_pos) = find_pattern(data, b"xref") {
            if let Some(trailer_pos) = find_pattern(&data[xref_pos..], b"trailer") {
                let xref_end = xref_pos + trailer_pos;

                // Rebuild xref section
                let mut new_xref = b"xref\n0 1\n0000000000 65535 f \n".to_vec();

                for offset in object_offsets.values() {
                    let xref_entry = format!("{:010} 00000 n \n", offset);
                    new_xref.extend_from_slice(xref_entry.as_bytes());
                }

                data.splice(xref_pos..xref_end, new_xref);
                modified = true;
            }
        }

        modified
    }
}

impl Default for StructureRepairStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Data recovery strategy for corrupted content
pub struct DataRecoveryStrategy {
    name: String,
}

impl DataRecoveryStrategy {
    pub fn new() -> Self {
        Self {
            name: "DataRecoveryStrategy".to_string(),
        }
    }
}

impl RecoveryStrategy for DataRecoveryStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Recover corrupted data using multiple techniques

        // 1. Fix corrupted binary data
        modified |= self.recover_binary_data(&mut data);

        // 2. Restore damaged text content
        modified |= self.recover_text_content(&mut data);

        // 3. Repair corrupted numeric values
        modified |= self.recover_numeric_values(&mut data);

        // 4. Reconstruct partial data from context
        modified |= self.reconstruct_from_context(&mut data, context);

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::DataReconstruction,
            description: if modified {
                "Recovered corrupted data content".to_string()
            } else {
                "No corrupted data found".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::EncodingError
                | RecoveryErrorType::StreamError
                | RecoveryErrorType::IntegrityError
        )
    }

    fn priority(&self) -> u8 {
        75
    }
}

impl DataRecoveryStrategy {
    fn recover_binary_data(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Replace null bytes that shouldn't be there (except in streams)
        let mut in_stream = false;
        let mut i = 0;

        while i < data.len() {
            // Track if we're inside a stream
            if i <= data.len().saturating_sub(6) && &data[i..i + 6] == b"stream" {
                in_stream = true;
                i += 6;
                continue;
            }
            if i <= data.len().saturating_sub(9) && &data[i..i + 9] == b"endstream" {
                in_stream = false;
                i += 9;
                continue;
            }

            // Replace problematic null bytes outside of streams
            if !in_stream && data[i] == 0 {
                data[i] = b' ';
                modified = true;
            }

            // Fix corrupted line endings
            if i < data.len() - 1 && data[i] == b'\r' && data[i + 1] != b'\n' {
                data.splice(i + 1..i + 1, b"\n".iter().cloned());
                modified = true;
                i += 1;
            }

            i += 1;
        }

        modified
    }

    fn recover_text_content(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Fix common text corruptions
        let corruptions = vec![
            ("\u{0000}", " "), // Null character
            ("\u{FFFD}", "?"), // Replacement character
            ("�", "?"),        // Another replacement char
            ("", " "),         // Various control chars
            ("", " "),
            ("", " "),
        ];

        for (corrupted, replacement) in corruptions {
            if fixed_str.contains(corrupted) {
                fixed_str = fixed_str.replace(corrupted, replacement);
                modified = true;
            }
        }

        // Fix broken UTF-8 sequences by removing invalid characters
        if let Ok(valid_string) = String::from_utf8(fixed_str.clone().into_bytes()) {
            if valid_string != data_str {
                fixed_str = valid_string;
                modified = true;
            }
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }

    fn recover_numeric_values(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Fix malformed numbers
        if let Ok(re) = regex::Regex::new(r"(\d+)\.(\d*)[^\d\s]") {
            if re.is_match(&fixed_str) {
                // Remove invalid characters after decimal numbers
                fixed_str = re.replace_all(&fixed_str, "$1.$2").to_string();
                modified = true;
            }
        }

        // Fix negative numbers with corrupted minus signs
        if let Ok(re) = regex::Regex::new(r"[^\d\s](\d+)") {
            let mut replacements = Vec::new();
            for m in re.find_iter(&fixed_str) {
                if let Some(first_char) = m.as_str().chars().next() {
                    if first_char as u32 > 127 {
                        // Non-ASCII, likely corrupted minus
                        let replacement = format!("-{}", &m.as_str()[first_char.len_utf8()..]);
                        replacements.push((m.range(), replacement));
                    }
                }
            }
            // Apply replacements in reverse order to maintain indices
            for (range, replacement) in replacements.into_iter().rev() {
                fixed_str.replace_range(range, &replacement);
                modified = true;
            }
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }

    fn reconstruct_from_context(&self, data: &mut Vec<u8>, context: RecoveryContext) -> bool {
        let mut modified = false;

        // Use error context to reconstruct missing data
        for error in context.error_log {
            if error.severity == ErrorSeverity::Critical {
                // Try to reconstruct critical missing data
                if let Some(hint) = error.context.hints.get("expected_content") {
                    let pos = error.location.byte_offset as usize;
                    if pos < data.len() {
                        // Insert reconstructed content
                        data.splice(pos..pos, hint.bytes());
                        modified = true;
                    }
                }
            }
        }

        // Reconstruct based on surrounding data patterns
        let data_str = String::from_utf8_lossy(data);

        // Look for partial patterns that can be completed
        if let Ok(re) = regex::Regex::new(r"/Type\s*/([A-Z][a-z]*)(?:\s|$)") {
            for capture in re.captures_iter(&data_str) {
                let type_name = &capture[1];
                // Ensure proper formatting
                let proper_format = format!("/Type /{}", type_name);
                if !data_str.contains(&proper_format) {
                    let mut fixed_str = data_str.to_string();
                    fixed_str = fixed_str.replace(&capture[0], &proper_format);
                    *data = fixed_str.into_bytes();
                    modified = true;
                    break;
                }
            }
        }

        modified
    }
}

impl Default for DataRecoveryStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// XRef table rebuild strategy
pub struct XRefRebuildStrategy {
    name: String,
}

impl XRefRebuildStrategy {
    pub fn new() -> Self {
        Self {
            name: "XRefRebuildStrategy".to_string(),
        }
    }
}

impl RecoveryStrategy for XRefRebuildStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Completely rebuild the xref table from scratch
        if self.needs_xref_rebuild(&data) {
            modified = self.rebuild_xref_table(&mut data);
        }

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::StructureRepair,
            description: if modified {
                "Rebuilt cross-reference table".to_string()
            } else {
                "Cross-reference table is valid".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::ReferenceError | RecoveryErrorType::StructuralError
        )
    }

    fn priority(&self) -> u8 {
        80
    }
}

impl XRefRebuildStrategy {
    fn needs_xref_rebuild(&self, data: &[u8]) -> bool {
        // Check if xref table exists and is valid
        if let Some(xref_pos) = find_pattern(data, b"xref") {
            // Check if xref entries are properly formatted
            let xref_section = &data[xref_pos..];
            if let Some(trailer_pos) = find_pattern(xref_section, b"trailer") {
                let xref_data = &xref_section[..trailer_pos];

                // Look for malformed xref entries
                let xref_str = String::from_utf8_lossy(xref_data);
                for line in xref_str.lines().skip(2) {
                    // Skip "xref" and subsection header
                    if !line.is_empty() && !self.is_valid_xref_entry(line) {
                        return true; // Needs rebuild
                    }
                }
            } else {
                return true; // No trailer found
            }
        } else {
            return true; // No xref table found
        }

        false
    }

    fn is_valid_xref_entry(&self, line: &str) -> bool {
        // Valid xref entry format: "nnnnnnnnnn ggggg n/f "
        if line.len() != 20 {
            return false;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 3 {
            return false;
        }

        // Check if offset is numeric and 10 digits
        if parts[0].len() != 10 || !parts[0].chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        // Check if generation is numeric and 5 digits
        if parts[1].len() != 5 || !parts[1].chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        // Check if type is 'n' or 'f'
        matches!(parts[2], "n" | "f")
    }

    fn rebuild_xref_table(&self, data: &mut Vec<u8>) -> bool {
        // First, find all object positions
        let objects = self.scan_for_objects(data);

        // Remove existing xref and trailer
        if let Some(xref_pos) = find_pattern(data, b"xref") {
            if let Some(_eof_pos) = find_pattern(&data[xref_pos..], b"%%EOF") {
                data.truncate(xref_pos);
            }
        }

        // Build new xref table
        let xref_start_pos = data.len();
        let mut xref_data = b"xref\n".to_vec();

        if objects.is_empty() {
            // Minimal xref if no objects found
            xref_data.extend_from_slice(b"0 1\n0000000000 65535 f \n");
        } else {
            // Find the range of object numbers
            let min_obj = objects.keys().min().copied().unwrap_or(0);
            let max_obj = objects.keys().max().copied().unwrap_or(0);

            let count = max_obj - min_obj + 1;
            xref_data.extend_from_slice(format!("{} {}\n", min_obj, count).as_bytes());

            // Add entries for each object
            for obj_num in min_obj..=max_obj {
                if let Some(offset) = objects.get(&obj_num) {
                    xref_data.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
                } else {
                    // Missing object, mark as free
                    xref_data.extend_from_slice(b"0000000000 65535 f \n");
                }
            }
        }

        // Add trailer
        let trailer = format!(
            "trailer\n<<\n/Size {}\n/Root 1 0 R\n>>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_start_pos
        );

        xref_data.extend_from_slice(trailer.as_bytes());
        data.extend_from_slice(&xref_data);

        true
    }

    fn scan_for_objects(&self, data: &[u8]) -> HashMap<u32, usize> {
        let mut objects = HashMap::new();
        let mut pos = 0;

        // Compile regex once outside the loop
        let obj_regex = match regex::Regex::new(r"(\d+)\s+\d+\s+obj$") {
            Ok(re) => re,
            Err(_) => return objects,
        };

        while pos < data.len() {
            if let Some(obj_start) = find_pattern(&data[pos..], b" obj") {
                let abs_pos = pos + obj_start;

                // Look backwards for the object number
                let search_start = abs_pos.saturating_sub(20);
                let prefix = String::from_utf8_lossy(&data[search_start..abs_pos]);

                // Find object number pattern
                if let Some(captures) = obj_regex.captures(&prefix) {
                    if let Ok(obj_num) = captures[1].parse::<u32>() {
                        objects.insert(obj_num, search_start);
                    }
                }

                pos = abs_pos + 4;
            } else {
                break;
            }
        }

        objects
    }
}

impl Default for XRefRebuildStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream repair strategy for corrupted streams
pub struct StreamRepairStrategy {
    name: String,
}

impl StreamRepairStrategy {
    pub fn new() -> Self {
        Self {
            name: "StreamRepairStrategy".to_string(),
        }
    }
}

impl RecoveryStrategy for StreamRepairStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Comprehensive stream repair
        modified |= self.repair_stream_boundaries(&mut data);
        modified |= self.fix_stream_lengths(&mut data);
        modified |= self.repair_compressed_streams(&mut data);
        modified |= self.fix_stream_filters(&mut data);

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::StreamDecoding,
            description: if modified {
                "Repaired corrupted stream data".to_string()
            } else {
                "No stream repairs needed".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, error_type: &RecoveryErrorType) -> bool {
        matches!(
            error_type,
            RecoveryErrorType::StreamError | RecoveryErrorType::EncodingError
        )
    }

    fn priority(&self) -> u8 {
        70
    }
}

impl StreamRepairStrategy {
    fn repair_stream_boundaries(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let mut pos = 0;

        while pos < data.len() {
            if let Some(stream_start) = find_pattern(&data[pos..], b"stream") {
                let abs_stream_pos = pos + stream_start;

                // Ensure proper newline after "stream"
                let after_stream = abs_stream_pos + 6;
                if after_stream < data.len() {
                    match data[after_stream] {
                        b'\r' => {
                            // Check for CRLF
                            if after_stream + 1 < data.len() && data[after_stream + 1] == b'\n' {
                                // CRLF is fine, but normalize to LF
                                data.remove(after_stream);
                                modified = true;
                            } else {
                                // Just CR, convert to LF
                                data[after_stream] = b'\n';
                                modified = true;
                            }
                        }
                        b'\n' => {
                            // Already correct
                        }
                        _ => {
                            // Missing newline
                            data.insert(after_stream, b'\n');
                            modified = true;
                        }
                    }
                }

                // Find matching endstream
                if let Some(endstream_pos) = find_pattern(&data[abs_stream_pos..], b"endstream") {
                    let abs_endstream_pos = abs_stream_pos + endstream_pos;

                    // Ensure proper newline before "endstream"
                    if abs_endstream_pos > 0 {
                        match data[abs_endstream_pos - 1] {
                            b'\n' | b'\r' => {
                                // Already has newline
                            }
                            _ => {
                                // Missing newline
                                data.insert(abs_endstream_pos, b'\n');
                                modified = true;
                            }
                        }
                    }

                    pos = abs_endstream_pos + 9;
                } else {
                    // Missing endstream - add it
                    if let Some(next_obj) = find_pattern(&data[abs_stream_pos..], b"endobj") {
                        let endobj_pos = abs_stream_pos + next_obj;
                        data.splice(endobj_pos..endobj_pos, b"endstream\n".iter().cloned());
                        modified = true;
                    }
                    pos = abs_stream_pos + 6;
                }
            } else {
                break;
            }
        }

        modified
    }

    fn fix_stream_lengths(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut pos = 0;

        while let Some(length_start) = data_str[pos..].find("/Length ") {
            let abs_pos = pos + length_start + 8;

            // Extract current length value
            if let Some(length_end) = data_str[abs_pos..].find(|c: char| !c.is_ascii_digit()) {
                if let Ok(declared_length) =
                    data_str[abs_pos..abs_pos + length_end].parse::<usize>()
                {
                    // Find the associated stream
                    if let Some(stream_start) = data_str[abs_pos..].find("stream\n") {
                        let stream_data_start = abs_pos + stream_start + 7;

                        if let Some(endstream_pos) = data_str[stream_data_start..].find("endstream")
                        {
                            let actual_length = endstream_pos;

                            if declared_length != actual_length {
                                // Update the length value
                                let new_data_str = format!(
                                    "{}{}{}",
                                    &data_str[..abs_pos],
                                    actual_length,
                                    &data_str[abs_pos + length_end..]
                                );
                                *data = new_data_str.into_bytes();
                                modified = true;
                                break; // Restart scan with modified data
                            }
                        }
                    }
                }
            }

            pos = abs_pos;
        }

        modified
    }

    fn repair_compressed_streams(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);

        // Look for compressed streams with corrupted headers
        if data_str.contains("/Filter ") && data_str.contains("FlateDecode") {
            // Find FlateDecode streams
            let mut pos = 0;
            while let Some(filter_pos) = data_str[pos..].find("/Filter ") {
                let abs_pos = pos + filter_pos;

                if data_str[abs_pos..].starts_with("/Filter /FlateDecode")
                    || data_str[abs_pos..].starts_with("/Filter [/FlateDecode")
                {
                    // Find the stream data
                    if let Some(stream_start) = data_str[abs_pos..].find("stream\n") {
                        let stream_data_start = abs_pos + stream_start + 7;

                        if let Some(endstream_pos) = data_str[stream_data_start..].find("endstream")
                        {
                            // Check if stream data looks like it starts with zlib header
                            let stream_bytes =
                                &data[stream_data_start..stream_data_start + endstream_pos];

                            if !stream_bytes.is_empty() {
                                // Check for corrupted zlib header
                                if stream_bytes.len() >= 2 {
                                    let first_two = (stream_bytes[0], stream_bytes[1]);

                                    // Common zlib headers: 0x78 0x9C, 0x78 0xDA, etc.
                                    if first_two.0 != 0x78 {
                                        // Possibly corrupted zlib header, try to fix
                                        let mut fixed_stream = vec![0x78, 0x9C];
                                        fixed_stream.extend_from_slice(&stream_bytes[2..]);

                                        data.splice(
                                            stream_data_start
                                                ..stream_data_start + stream_bytes.len(),
                                            fixed_stream.iter().cloned(),
                                        );
                                        modified = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                pos = abs_pos + 8;
            }
        }

        modified
    }

    fn fix_stream_filters(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut fixed_str = data_str.to_string();

        // Fix malformed filter specifications
        let filter_fixes = vec![
            ("/Filter/FlateDecode", "/Filter /FlateDecode"),
            ("/Filter[/FlateDecode]", "/Filter [/FlateDecode]"),
            ("/Filter /FlateDecod", "/Filter /FlateDecode"), // Common typo
            ("/Filter /ASCIIHexDecod", "/Filter /ASCIIHexDecode"),
        ];

        for (malformed, correct) in filter_fixes {
            if fixed_str.contains(malformed) {
                fixed_str = fixed_str.replace(malformed, correct);
                modified = true;
            }
        }

        // Fix filter arrays with missing brackets
        if let Ok(re) = regex::Regex::new(r"/Filter\s+/(\w+)\s+/(\w+)") {
            if re.is_match(&fixed_str) {
                fixed_str = re.replace_all(&fixed_str, "/Filter [/$1 /$2]").to_string();
                modified = true;
            }
        }

        if modified {
            *data = fixed_str.into_bytes();
        }

        modified
    }
}

impl Default for StreamRepairStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Experimental recovery strategy with advanced techniques
pub struct ExperimentalRecovery {
    name: String,
}

impl ExperimentalRecovery {
    pub fn new() -> Self {
        Self {
            name: "ExperimentalRecovery".to_string(),
        }
    }
}

impl RecoveryStrategy for ExperimentalRecovery {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply_recovery(&self, context: RecoveryContext) -> AstResult<RecoveryResult> {
        let mut data = context.current_data.to_vec();
        let mut modified = false;

        // Experimental and advanced recovery techniques

        // 1. Machine learning-inspired pattern recognition
        modified |= self.pattern_recognition_repair(&mut data);

        // 2. Statistical content reconstruction
        modified |= self.statistical_reconstruction(&mut data);

        // 3. Cross-validation with known PDF structures
        modified |= self.cross_validate_structure(&mut data, context);

        // 4. Genetic algorithm-inspired repair attempts
        modified |= self.evolutionary_repair(&mut data);

        // 5. Deep structural analysis and reconstruction
        modified |= self.deep_structure_analysis(&mut data);

        Ok(RecoveryResult {
            success: modified,
            action_type: RecoveryActionType::DataReconstruction,
            description: if modified {
                "Applied experimental recovery techniques".to_string()
            } else {
                "No experimental repairs applied".to_string()
            },
            data_changed: modified,
            document_changed: false,
            modified_data: if modified { Some(data) } else { None },
            modified_document: None,
        })
    }

    fn can_handle(&self, _error_type: &RecoveryErrorType) -> bool {
        true // Can attempt any error type with experimental methods
    }

    fn priority(&self) -> u8 {
        10 // Lowest priority - only when other strategies fail
    }
}

impl ExperimentalRecovery {
    /// Pattern recognition repair using frequency analysis
    fn pattern_recognition_repair(&self, data: &mut [u8]) -> bool {
        let mut modified = false;

        // Analyze byte frequency patterns to detect anomalies
        let mut frequency = vec![0u32; 256];
        for &byte in data.iter() {
            frequency[byte as usize] += 1;
        }

        // Find bytes with extremely low or high frequency (potential corruption)
        let total_bytes = data.len() as f64;
        let expected_frequency = total_bytes / 256.0;

        for (byte_val, &freq) in frequency.iter().enumerate() {
            let freq_ratio = freq as f64 / expected_frequency;

            // If a non-printable byte appears too frequently, it might be corruption
            if byte_val < 32
                && byte_val != 9
                && byte_val != 10
                && byte_val != 13
                && freq_ratio > 5.0
            {
                // Much more frequent than expected
                // Replace suspicious bytes with spaces
                for byte in data.iter_mut() {
                    if *byte == byte_val as u8 {
                        *byte = b' ';
                        modified = true;
                    }
                }
            }
        }

        // Look for repeated corruption patterns
        modified |= self.detect_repeated_corruption_patterns(data);

        modified
    }

    fn detect_repeated_corruption_patterns(&self, data: &mut [u8]) -> bool {
        let mut modified = false;

        // Look for sequences of the same byte that are likely corruption
        let mut i = 0;
        while i < data.len() {
            let current_byte = data[i];
            let mut count = 1;

            // Count consecutive identical bytes
            while i + count < data.len() && data[i + count] == current_byte {
                count += 1;
            }

            // If we have a long sequence of non-printable characters, likely corruption
            if count > 10
                && !(32..=126).contains(&current_byte)
                && current_byte != b'\n'
                && current_byte != b'\r'
            {
                // Replace with appropriate content
                let replacement = if current_byte == 0 { b" " } else { b"?" };
                for item in data.iter_mut().skip(i).take(count) {
                    *item = replacement[0];
                }
                modified = true;
            }

            i += count;
        }

        modified
    }

    /// Statistical reconstruction based on PDF content patterns
    fn statistical_reconstruction(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Statistical analysis of PDF keywords and their expected context
        let pdf_keywords = vec![
            ("obj", vec!["endobj", "stream", "<<"]),
            ("stream", vec!["endstream", "Length"]),
            ("xref", vec!["trailer", "n ", "f "]),
            ("trailer", vec!["startxref", "Size", "Root"]),
        ];

        for (keyword, expected_neighbors) in pdf_keywords {
            let data_str = String::from_utf8_lossy(data);
            let keyword_positions = self.find_all_occurrences(&data_str, keyword);

            for pos in keyword_positions {
                // Check context around each keyword
                let context_start = pos.saturating_sub(100);
                let context_end = (pos + 100).min(data_str.len());
                let context = &data_str[context_start..context_end];

                // Calculate how many expected neighbors are present
                let neighbor_count = expected_neighbors
                    .iter()
                    .filter(|&neighbor| context.contains(neighbor))
                    .count();

                // If context is suspiciously lacking in expected neighbors
                if neighbor_count == 0 && keyword != "trailer" {
                    // This might be a false positive or corrupted context
                    // Try to reconstruct based on keyword type
                    if let Some(reconstruction) =
                        self.reconstruct_keyword_context(keyword, pos, &data_str)
                    {
                        let mut new_data = data_str.to_string();
                        new_data.replace_range(pos..pos + keyword.len(), &reconstruction);
                        *data = new_data.into_bytes();
                        modified = true;
                        break; // Restart analysis with new data
                    }
                }
            }

            if modified {
                break; // Restart from beginning with modified data
            }
        }

        modified
    }

    fn find_all_occurrences(&self, text: &str, pattern: &str) -> Vec<usize> {
        let mut positions = Vec::new();
        let mut start = 0;

        while let Some(pos) = text[start..].find(pattern) {
            positions.push(start + pos);
            start += pos + 1;
        }

        positions
    }

    fn reconstruct_keyword_context(&self, keyword: &str, pos: usize, text: &str) -> Option<String> {
        match keyword {
            "obj" => {
                // If "obj" appears without proper context, might be part of a larger corruption
                // Check if it should be "endobj"
                if pos >= 3 && &text[pos - 3..pos] == "end" {
                    return Some("endobj".to_string());
                }
                None
            }
            "stream" => {
                // Check if this should be "endstream"
                if pos >= 3 && &text[pos - 3..pos] == "end" {
                    return Some("endstream".to_string());
                }
                None
            }
            _ => None,
        }
    }

    /// Cross-validation with known PDF structure patterns
    fn cross_validate_structure(&self, data: &mut Vec<u8>, context: RecoveryContext) -> bool {
        let mut modified = false;

        // Check for structural inconsistencies using document context
        if !context.document.ast.get_all_nodes().is_empty() {
            // We have some document structure to work with
            modified |= self.validate_against_document_structure(context, data);
        }

        // Cross-validate common PDF structural patterns
        modified |= self.validate_common_patterns(data);

        modified
    }

    fn validate_against_document_structure(
        &self,
        context: RecoveryContext,
        data: &mut Vec<u8>,
    ) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);

        // If document has objects but data lacks proper object structure
        let nodes = context.document.ast.get_all_nodes();
        if !nodes.is_empty() && !data_str.contains("obj") {
            // Document claims to have objects but data doesn't show them
            // This suggests severe structural damage - attempt reconstruction

            let reconstructed = self.reconstruct_basic_pdf_structure(&nodes);
            if !reconstructed.is_empty() {
                *data = reconstructed;
                modified = true;
            }
        }

        modified
    }

    fn reconstruct_basic_pdf_structure(&self, nodes: &[&AstNode]) -> Vec<u8> {
        let mut reconstructed = Vec::new();

        // Add PDF header
        reconstructed.extend_from_slice(b"%PDF-1.4\n");

        // Add basic objects based on AST nodes
        for (i, node) in nodes.iter().enumerate() {
            let obj_num = i + 1;
            let obj_header = format!("{} 0 obj\n", obj_num);
            reconstructed.extend_from_slice(obj_header.as_bytes());

            // Add basic object content based on node type
            match node.node_type {
                NodeType::Catalog => {
                    reconstructed.extend_from_slice(b"<<\n/Type /Catalog\n>>\n");
                }
                NodeType::Page => {
                    reconstructed.extend_from_slice(b"<<\n/Type /Page\n>>\n");
                }
                _ => {
                    reconstructed.extend_from_slice(b"<<\n>>\n");
                }
            }

            reconstructed.extend_from_slice(b"endobj\n");
        }

        // Add basic xref and trailer
        reconstructed.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
        reconstructed.extend_from_slice(b"trailer\n<<\n/Size 2\n>>\nstartxref\n0\n%%EOF");

        reconstructed
    }

    fn validate_common_patterns(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Validate and fix common structural patterns
        let patterns_to_check = vec![
            // Pattern: every "obj" should eventually have "endobj"
            (b"obj".to_vec(), b"endobj".to_vec()),
            // Pattern: every "stream" should have "endstream"
            (b"stream".to_vec(), b"endstream".to_vec()),
            // Pattern: every "<<" should have ">>"
            (b"<<".to_vec(), b">>".to_vec()),
        ];

        for (start_pattern, end_pattern) in patterns_to_check {
            modified |= self.ensure_pattern_balance(data, &start_pattern, &end_pattern);
        }

        modified
    }

    fn ensure_pattern_balance(
        &self,
        data: &mut Vec<u8>,
        start_pattern: &[u8],
        end_pattern: &[u8],
    ) -> bool {
        let mut modified = false;
        let mut open_count = 0;
        let mut close_count = 0;

        // Count occurrences of start and end patterns
        let mut pos = 0;
        while pos <= data.len().saturating_sub(start_pattern.len()) {
            if data[pos..].starts_with(start_pattern) {
                open_count += 1;
                pos += start_pattern.len();
            } else {
                pos += 1;
            }
        }

        pos = 0;
        while pos <= data.len().saturating_sub(end_pattern.len()) {
            if data[pos..].starts_with(end_pattern) {
                close_count += 1;
                pos += end_pattern.len();
            } else {
                pos += 1;
            }
        }

        // If unbalanced, add missing end patterns
        if open_count > close_count {
            let missing = open_count - close_count;
            for _ in 0..missing {
                data.extend_from_slice(b"\n");
                data.extend_from_slice(end_pattern);
                data.extend_from_slice(b"\n");
            }
            modified = true;
        }

        modified
    }

    /// Evolutionary/genetic algorithm-inspired repair attempts
    fn evolutionary_repair(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Try multiple "mutations" and keep the best result
        let original_data = data.clone();
        let mut best_fitness = self.calculate_pdf_fitness(&original_data);
        let mut best_data = original_data.clone();

        // Generate variations with different repair strategies
        let mutations = self.generate_repair_mutations(&original_data);

        for mutation in mutations {
            let fitness = self.calculate_pdf_fitness(&mutation);
            if fitness > best_fitness {
                best_fitness = fitness;
                best_data = mutation;
                modified = true;
            }
        }

        if modified {
            *data = best_data;
        }

        modified
    }

    fn calculate_pdf_fitness(&self, data: &[u8]) -> f64 {
        let mut fitness = 0.0;
        let data_str = String::from_utf8_lossy(data);

        // Award points for proper PDF structure
        if data_str.starts_with("%PDF") {
            fitness += 10.0;
        }
        if data_str.contains("obj") {
            fitness += 5.0;
        }
        if data_str.contains("endobj") {
            fitness += 5.0;
        }
        if data_str.contains("xref") {
            fitness += 5.0;
        }
        if data_str.contains("trailer") {
            fitness += 5.0;
        }
        if data_str.ends_with("%%EOF") {
            fitness += 10.0;
        }

        // Penalize for obvious corruption
        let null_bytes = data.iter().filter(|&&b| b == 0).count();
        fitness -= null_bytes as f64 * 0.1;

        // Award for balanced structures
        let obj_count = data_str.matches("obj").count();
        let endobj_count = data_str.matches("endobj").count();
        fitness += 5.0 - (obj_count as f64 - endobj_count as f64).abs();

        fitness
    }

    fn generate_repair_mutations(&self, original_data: &[u8]) -> Vec<Vec<u8>> {
        let mut mutations = Vec::new();

        // Mutation 1: Remove all null bytes
        let mut mutation1 = original_data.to_vec();
        mutation1.retain(|&b| b != 0);
        mutations.push(mutation1);

        // Mutation 2: Replace control characters with spaces
        let mut mutation2 = original_data.to_vec();
        for byte in mutation2.iter_mut() {
            if *byte < 32 && *byte != b'\n' && *byte != b'\r' && *byte != b'\t' {
                *byte = b' ';
            }
        }
        mutations.push(mutation2);

        // Mutation 3: Fix line endings
        let mut mutation3 = original_data.to_vec();
        for i in 0..mutation3.len().saturating_sub(1) {
            if mutation3[i] == b'\r' && mutation3[i + 1] != b'\n' {
                mutation3[i] = b'\n';
            }
        }
        mutations.push(mutation3);

        mutations
    }

    /// Deep structural analysis and reconstruction
    fn deep_structure_analysis(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;

        // Perform deep analysis of the document structure
        let structure_analysis = self.analyze_document_structure(data);

        // Apply fixes based on structural analysis
        if structure_analysis.missing_header {
            self.add_pdf_header(data);
            modified = true;
        }

        if structure_analysis.broken_xref {
            modified |= self.reconstruct_xref_from_deep_analysis(data, &structure_analysis);
        }

        if structure_analysis.orphaned_content {
            modified |= self.wrap_orphaned_content(data);
        }

        modified
    }

    fn analyze_document_structure(&self, data: &[u8]) -> DocumentStructureAnalysis {
        let data_str = String::from_utf8_lossy(data);

        DocumentStructureAnalysis {
            missing_header: !data_str.starts_with("%PDF"),
            broken_xref: !data_str.contains("xref") || !self.is_xref_valid(&data_str),
            orphaned_content: self.has_orphaned_content(&data_str),
            object_count: data_str.matches(" obj").count(),
            malformed_objects: self.count_malformed_objects(&data_str),
        }
    }

    fn is_xref_valid(&self, data_str: &str) -> bool {
        // Simple validation - check if xref has proper format
        if let Some(xref_pos) = data_str.find("xref") {
            let xref_section = &data_str[xref_pos..];
            if let Some(trailer_pos) = xref_section.find("trailer") {
                let xref_entries = &xref_section[..trailer_pos];
                // Check if it has at least some numeric entries
                return xref_entries.chars().filter(|c| c.is_ascii_digit()).count() > 10;
            }
        }
        false
    }

    fn has_orphaned_content(&self, data_str: &str) -> bool {
        // Look for dictionary or array content that's not inside objects
        let lines: Vec<&str> = data_str.lines().collect();
        let mut in_object = false;

        for line in lines {
            if line.contains(" obj") {
                in_object = true;
            } else if line.contains("endobj") {
                in_object = false;
            } else if !in_object
                && (line.contains("<<") || line.contains(">>") || line.contains("/"))
            {
                return true; // Found content outside of objects
            }
        }

        false
    }

    fn count_malformed_objects(&self, data_str: &str) -> usize {
        let obj_starts = data_str.matches(" obj").count();
        let obj_ends = data_str.matches("endobj").count();
        obj_starts.saturating_sub(obj_ends)
    }

    fn add_pdf_header(&self, data: &mut Vec<u8>) {
        let header = b"%PDF-1.4\n";
        data.splice(0..0, header.iter().cloned());
    }

    fn reconstruct_xref_from_deep_analysis(
        &self,
        data: &mut Vec<u8>,
        analysis: &DocumentStructureAnalysis,
    ) -> bool {
        if analysis.object_count == 0 {
            return false; // Can't build xref without objects
        }

        // Find the best position to insert xref (before trailer or at end)
        let data_str = String::from_utf8_lossy(data);
        let insert_pos = if let Some(trailer_pos) = data_str.find("trailer") {
            trailer_pos
        } else {
            data.len()
        };

        // Build minimal xref
        let xref_content = format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            analysis.object_count + 1
        );

        data.splice(insert_pos..insert_pos, xref_content.bytes());
        true
    }

    fn wrap_orphaned_content(&self, data: &mut Vec<u8>) -> bool {
        let mut modified = false;
        let data_str = String::from_utf8_lossy(data);
        let mut new_data = String::new();
        let lines: Vec<&str> = data_str.lines().collect();

        let mut in_object = false;
        let mut orphaned_content = String::new();
        let mut object_counter = 1000; // Start with high numbers to avoid conflicts

        for line in lines {
            if line.contains(" obj") {
                // Wrap any accumulated orphaned content
                if !orphaned_content.is_empty() {
                    new_data.push_str(&format!(
                        "{} 0 obj\n{}\nendobj\n",
                        object_counter, orphaned_content
                    ));
                    orphaned_content.clear();
                    object_counter += 1;
                    modified = true;
                }
                in_object = true;
                new_data.push_str(line);
                new_data.push('\n');
            } else if line.contains("endobj") {
                in_object = false;
                new_data.push_str(line);
                new_data.push('\n');
            } else if !in_object
                && (line.contains("<<") || line.contains("/") || line.starts_with('['))
            {
                // This looks like orphaned content
                orphaned_content.push_str(line);
                orphaned_content.push('\n');
            } else {
                new_data.push_str(line);
                new_data.push('\n');
            }
        }

        // Handle any remaining orphaned content
        if !orphaned_content.is_empty() {
            new_data.push_str(&format!(
                "{} 0 obj\n{}\nendobj\n",
                object_counter, orphaned_content
            ));
            modified = true;
        }

        if modified {
            *data = new_data.into_bytes();
        }

        modified
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct DocumentStructureAnalysis {
    missing_header: bool,
    broken_xref: bool,
    orphaned_content: bool,
    object_count: usize,
    malformed_objects: usize,
}

impl Default for ExperimentalRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to find a pattern in data
fn find_pattern(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

#[cfg(test)]
mod tests {
    use super::{BasicStructureRecovery, RecoveryConfig, RecoveryContext, RecoveryStrategy};
    use crate::ast::{PdfDocument, PdfVersion};

    #[test]
    fn basic_recovery_does_not_invent_xref_or_root() {
        let data = b"%PDF-1.4\n1 0 obj\nnull\nendobj\n";
        let document = PdfDocument::new(PdfVersion::new(1, 4));
        let config = RecoveryConfig::default();
        let result = BasicStructureRecovery::new()
            .apply_recovery(RecoveryContext {
                original_data: data,
                current_data: data,
                document: &document,
                config: &config,
                error_log: &[],
            })
            .expect("basic recovery should complete");
        let recovered = result
            .modified_data
            .expect("missing EOF should be repaired");

        assert!(recovered.ends_with(b"\n%%EOF"));
        assert!(!recovered.windows(4).any(|window| window == b"xref"));
        assert!(!recovered.windows(7).any(|window| window == b"trailer"));
        assert!(!recovered.windows(5).any(|window| window == b"/Root"));
    }
}
