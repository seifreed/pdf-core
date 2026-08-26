use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::ResourceBudget;
use crate::types::{PdfStream, PdfValue};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CMap {
    pub name: String,
    pub cid_system_info: CIDSystemInfo,
    pub wmode: i32,
    pub code_space_ranges: Vec<CodeSpaceRange>,
    pub mappings: CMapMappings,
    pub usecmap: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CIDSystemInfo {
    pub registry: String,
    pub ordering: String,
    pub supplement: i32,
}

#[derive(Debug, Clone)]
pub struct CodeSpaceRange {
    pub start: Vec<u8>,
    pub end: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum CMapMappings {
    Char(HashMap<Vec<u8>, Vec<u8>>), // bfchar mappings
    Range(Vec<CharRangeMapping>),    // bfrange mappings
    CID(HashMap<Vec<u8>, u32>),      // cidchar mappings
    CIDRange(Vec<CIDRangeMapping>),  // cidrange mappings
    Mixed {
        chars: HashMap<Vec<u8>, Vec<u8>>,
        ranges: Vec<CharRangeMapping>,
        cid_chars: HashMap<Vec<u8>, u32>,
        cid_ranges: Vec<CIDRangeMapping>,
    },
}

#[derive(Debug, Clone)]
pub struct CharRangeMapping {
    pub start: Vec<u8>,
    pub end: Vec<u8>,
    pub dest: RangeDest,
}

#[derive(Debug, Clone)]
pub enum RangeDest {
    Single(Vec<u8>),     // Maps to single starting point
    Array(Vec<Vec<u8>>), // Maps to array of values
}

#[derive(Debug, Clone)]
pub struct CIDRangeMapping {
    pub start: Vec<u8>,
    pub end: Vec<u8>,
    pub cid: u32,
}

#[allow(dead_code)]
pub struct CMapParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
}

impl<'a> CMapParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        CMapParser {
            ast,
            resolver,
            budget: budget.clone(),
        }
    }

    pub fn parse_cmap_stream(&mut self, stream: &PdfStream) -> Option<(NodeId, CMap)> {
        let data = stream.decode_with_budget(&self.budget).ok()?;
        let cmap = self.parse_cmap_data(&data)?;

        // Create CMap node
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::CMap,
            PdfValue::Stream(stream.clone()),
        );

        // Add metadata
        node.metadata
            .set_property("cmap_name".to_string(), cmap.name.clone());
        node.metadata.set_property(
            "registry".to_string(),
            cmap.cid_system_info.registry.clone(),
        );
        node.metadata.set_property(
            "ordering".to_string(),
            cmap.cid_system_info.ordering.clone(),
        );
        node.metadata.set_property(
            "supplement".to_string(),
            cmap.cid_system_info.supplement.to_string(),
        );
        node.metadata
            .set_property("wmode".to_string(), cmap.wmode.to_string());

        self.budget.consume_node().ok()?;
        let node_id = self.ast.add_node(node);

        Some((node_id, cmap))
    }

    pub fn parse_tounicode_stream(&mut self, stream: &PdfStream) -> Option<NodeId> {
        let data = stream.decode_with_budget(&self.budget).ok()?;
        let cmap = self.parse_cmap_data(&data)?;

        // Create ToUnicode node
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::ToUnicode,
            PdfValue::Stream(stream.clone()),
        );

        // Add metadata
        node.metadata
            .set_property("cmap_name".to_string(), cmap.name.clone());

        // Count mappings
        let mapping_count = match &cmap.mappings {
            CMapMappings::Char(m) => m.len(),
            CMapMappings::Range(r) => r.len(),
            CMapMappings::CID(m) => m.len(),
            CMapMappings::CIDRange(r) => r.len(),
            CMapMappings::Mixed {
                chars,
                ranges,
                cid_chars,
                cid_ranges,
            } => chars.len() + ranges.len() + cid_chars.len() + cid_ranges.len(),
        };

        node.metadata
            .set_property("mapping_count".to_string(), mapping_count.to_string());

        self.budget.consume_node().ok()?;
        let node_id = self.ast.add_node(node);

        Some(node_id)
    }

    fn parse_cmap_data(&self, data: &[u8]) -> Option<CMap> {
        let content = String::from_utf8_lossy(data);
        let mut cmap = CMap {
            name: String::new(),
            cid_system_info: CIDSystemInfo {
                registry: String::new(),
                ordering: String::new(),
                supplement: 0,
            },
            wmode: 0,
            code_space_ranges: Vec::new(),
            mappings: CMapMappings::Char(HashMap::new()),
            usecmap: None,
        };

        let mut chars = HashMap::new();
        let mut ranges = Vec::new();
        let mut cid_chars = HashMap::new();
        let mut cid_ranges = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // CMapName
            if line.starts_with("/CMapName") {
                if let Some(name) = self.extract_name(line) {
                    cmap.name = name;
                }
            }
            // CIDSystemInfo
            else if line.contains("CIDSystemInfo") {
                i += 1;
                while i < lines.len() && !lines[i].contains(">>") {
                    let info_line = lines[i].trim();
                    if info_line.starts_with("/Registry") {
                        if let Some(reg) = self.extract_string(info_line) {
                            cmap.cid_system_info.registry = reg;
                        }
                    } else if info_line.starts_with("/Ordering") {
                        if let Some(ord) = self.extract_string(info_line) {
                            cmap.cid_system_info.ordering = ord;
                        }
                    } else if info_line.starts_with("/Supplement") {
                        if let Some(sup) = self.extract_number(info_line) {
                            cmap.cid_system_info.supplement = sup as i32;
                        }
                    }
                    i += 1;
                }
            }
            // WMode
            else if line.starts_with("/WMode") {
                if let Some(wmode) = self.extract_number(line) {
                    cmap.wmode = wmode as i32;
                }
            }
            // UseCMap
            else if line.starts_with("/UseCMap") {
                if let Some(usecmap) = self.extract_name(line) {
                    cmap.usecmap = Some(usecmap);
                }
            }
            // Code space ranges
            else if line.contains("begincodespacerange") {
                let count = self.extract_count(line).unwrap_or(0);
                i += 1;
                for _ in 0..count {
                    if i >= lines.len() {
                        break;
                    }
                    let range_line = lines[i].trim();
                    if let Some((start, end)) = self.parse_hex_range(range_line) {
                        cmap.code_space_ranges.push(CodeSpaceRange { start, end });
                    }
                    i += 1;
                }
            }
            // Character mappings
            else if line.contains("beginbfchar") {
                let count = self.extract_count(line).unwrap_or(0);
                i += 1;
                for _ in 0..count {
                    if i >= lines.len() {
                        break;
                    }
                    let char_line = lines[i].trim();
                    if let Some((src, dst)) = self.parse_char_mapping(char_line) {
                        chars.insert(src, dst);
                    }
                    i += 1;
                }
            }
            // Range mappings
            else if line.contains("beginbfrange") {
                let count = self.extract_count(line).unwrap_or(0);
                i += 1;
                for _ in 0..count {
                    if i >= lines.len() {
                        break;
                    }
                    let range_line = lines[i].trim();
                    if let Some(mapping) = self.parse_range_mapping(range_line) {
                        ranges.push(mapping);
                    }
                    i += 1;
                }
            }
            // CID character mappings
            else if line.contains("begincidchar") {
                let count = self.extract_count(line).unwrap_or(0);
                i += 1;
                for _ in 0..count {
                    if i >= lines.len() {
                        break;
                    }
                    let cid_line = lines[i].trim();
                    if let Some((src, cid)) = self.parse_cid_char(cid_line) {
                        cid_chars.insert(src, cid);
                    }
                    i += 1;
                }
            }
            // CID range mappings
            else if line.contains("begincidrange") {
                let count = self.extract_count(line).unwrap_or(0);
                i += 1;
                for _ in 0..count {
                    if i >= lines.len() {
                        break;
                    }
                    let cid_range_line = lines[i].trim();
                    if let Some(mapping) = self.parse_cid_range(cid_range_line) {
                        cid_ranges.push(mapping);
                    }
                    i += 1;
                }
            }

            i += 1;
        }

        // Determine mapping type
        cmap.mappings = if !chars.is_empty()
            && ranges.is_empty()
            && cid_chars.is_empty()
            && cid_ranges.is_empty()
        {
            CMapMappings::Char(chars)
        } else if chars.is_empty()
            && !ranges.is_empty()
            && cid_chars.is_empty()
            && cid_ranges.is_empty()
        {
            CMapMappings::Range(ranges)
        } else if chars.is_empty()
            && ranges.is_empty()
            && !cid_chars.is_empty()
            && cid_ranges.is_empty()
        {
            CMapMappings::CID(cid_chars)
        } else if chars.is_empty()
            && ranges.is_empty()
            && cid_chars.is_empty()
            && !cid_ranges.is_empty()
        {
            CMapMappings::CIDRange(cid_ranges)
        } else {
            CMapMappings::Mixed {
                chars,
                ranges,
                cid_chars,
                cid_ranges,
            }
        };

        Some(cmap)
    }

    fn extract_name(&self, line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.get(1).map(|s| s.trim_start_matches('/').to_string())
    }

    fn extract_string(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find('(') {
            if let Some(end) = line.rfind(')') {
                return Some(line[start + 1..end].to_string());
            }
        }
        None
    }

    fn extract_number(&self, line: &str) -> Option<i64> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.get(1).and_then(|s| s.parse().ok())
    }

    fn extract_count(&self, line: &str) -> Option<usize> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.first().and_then(|s| s.parse().ok())
    }

    fn parse_hex_range(&self, line: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let start = self.hex_to_bytes(parts[0])?;
            let end = self.hex_to_bytes(parts[1])?;
            return Some((start, end));
        }
        None
    }

    fn parse_char_mapping(&self, line: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let src = self.hex_to_bytes(parts[0])?;
            let dst = self.hex_to_bytes(parts[1])?;
            return Some((src, dst));
        }
        None
    }

    fn parse_range_mapping(&self, line: &str) -> Option<CharRangeMapping> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let start = self.hex_to_bytes(parts[0])?;
            let end = self.hex_to_bytes(parts[1])?;

            // Check if destination is array
            if parts[2].starts_with('[') {
                // Parse array of destinations
                let mut array_dests = Vec::new();
                let array_str = parts[2..].join(" ");
                let array_content = array_str.trim_start_matches('[').trim_end_matches(']');

                for hex in array_content.split_whitespace() {
                    if let Some(bytes) = self.hex_to_bytes(hex) {
                        array_dests.push(bytes);
                    }
                }

                return Some(CharRangeMapping {
                    start,
                    end,
                    dest: RangeDest::Array(array_dests),
                });
            } else {
                // Single destination
                let dest = self.hex_to_bytes(parts[2])?;
                return Some(CharRangeMapping {
                    start,
                    end,
                    dest: RangeDest::Single(dest),
                });
            }
        }
        None
    }

    fn parse_cid_char(&self, line: &str) -> Option<(Vec<u8>, u32)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let src = self.hex_to_bytes(parts[0])?;
            let cid = parts[1].parse().ok()?;
            return Some((src, cid));
        }
        None
    }

    fn parse_cid_range(&self, line: &str) -> Option<CIDRangeMapping> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let start = self.hex_to_bytes(parts[0])?;
            let end = self.hex_to_bytes(parts[1])?;
            let cid = parts[2].parse().ok()?;
            return Some(CIDRangeMapping { start, end, cid });
        }
        None
    }

    fn hex_to_bytes(&self, hex: &str) -> Option<Vec<u8>> {
        let hex = hex.trim_start_matches('<').trim_end_matches('>');
        if hex.len() % 2 != 0 {
            return None;
        }

        let mut bytes = Vec::new();
        for i in (0..hex.len()).step_by(2) {
            let byte_str = &hex[i..i + 2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            } else {
                return None;
            }
        }

        Some(bytes)
    }

    pub fn map_code_to_unicode(&self, cmap: &CMap, code: &[u8]) -> Option<String> {
        match &cmap.mappings {
            CMapMappings::Char(chars) => chars
                .get(code)
                .and_then(|bytes| self.bytes_to_unicode(bytes)),
            CMapMappings::Range(ranges) => {
                for range in ranges {
                    if self.in_range(code, &range.start, &range.end) {
                        return self.map_range_to_unicode(code, &range.start, &range.dest);
                    }
                }
                None
            }
            CMapMappings::Mixed { chars, ranges, .. } => {
                // Try direct mapping first
                if let Some(unicode) = chars
                    .get(code)
                    .and_then(|bytes| self.bytes_to_unicode(bytes))
                {
                    return Some(unicode);
                }

                // Try range mappings
                for range in ranges {
                    if self.in_range(code, &range.start, &range.end) {
                        return self.map_range_to_unicode(code, &range.start, &range.dest);
                    }
                }

                None
            }
            _ => None,
        }
    }

    fn in_range(&self, code: &[u8], start: &[u8], end: &[u8]) -> bool {
        if code.len() != start.len() || code.len() != end.len() {
            return false;
        }

        code >= start && code <= end
    }

    fn map_range_to_unicode(&self, code: &[u8], start: &[u8], dest: &RangeDest) -> Option<String> {
        match dest {
            RangeDest::Single(base) => {
                // Calculate offset
                let offset = self.bytes_to_u32(code)? - self.bytes_to_u32(start)?;
                let unicode_value = self.bytes_to_u32(base)? + offset;

                // Convert to Unicode character
                char::from_u32(unicode_value).map(|c| c.to_string())
            }
            RangeDest::Array(array) => {
                // Calculate index
                let index = (self.bytes_to_u32(code)? - self.bytes_to_u32(start)?) as usize;
                array
                    .get(index)
                    .and_then(|bytes| self.bytes_to_unicode(bytes))
            }
        }
    }

    fn bytes_to_unicode(&self, bytes: &[u8]) -> Option<String> {
        // Interpret bytes as UTF-16BE Unicode value
        if bytes.len() == 2 {
            let value = ((bytes[0] as u32) << 8) | (bytes[1] as u32);
            char::from_u32(value).map(|c| c.to_string())
        } else if bytes.len() == 4 {
            // Surrogate pair or direct UTF-32
            let value = ((bytes[0] as u32) << 24)
                | ((bytes[1] as u32) << 16)
                | ((bytes[2] as u32) << 8)
                | (bytes[3] as u32);
            char::from_u32(value).map(|c| c.to_string())
        } else {
            None
        }
    }

    fn bytes_to_u32(&self, bytes: &[u8]) -> Option<u32> {
        if bytes.is_empty() || bytes.len() > 4 {
            return None;
        }

        let mut value = 0u32;
        for byte in bytes {
            value = (value << 8) | (*byte as u32);
        }

        Some(value)
    }
}
