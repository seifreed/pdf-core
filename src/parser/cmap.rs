use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
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

type MappingParts = (
    HashMap<Vec<u8>, Vec<u8>>,
    Vec<CharRangeMapping>,
    HashMap<Vec<u8>, u32>,
    Vec<CIDRangeMapping>,
);

#[allow(dead_code)]
pub struct CMapParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
    parsed_cmaps: HashMap<String, CMap>,
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
            parsed_cmaps: HashMap::new(),
        }
    }

    pub fn parse_cmap_stream(&mut self, stream: &PdfStream) -> Option<(NodeId, CMap)> {
        let data = stream.decode_with_budget(&self.budget).ok()?;
        let mut cmap = self.parse_cmap_data(&data)?;
        self.resolve_usecmap(&mut cmap);
        if !cmap.name.is_empty() {
            self.parsed_cmaps.insert(cmap.name.clone(), cmap.clone());
        }

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
        let mut cmap = self.parse_cmap_data(&data)?;
        self.resolve_usecmap(&mut cmap);
        if !cmap.name.is_empty() {
            self.parsed_cmaps.insert(cmap.name.clone(), cmap.clone());
        }

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

    fn resolve_usecmap(&self, cmap: &mut CMap) {
        let Some(base_name) = cmap.usecmap.as_deref() else {
            return;
        };
        let Some(base) = self.parsed_cmaps.get(base_name) else {
            return;
        };

        if cmap.code_space_ranges.is_empty() {
            cmap.code_space_ranges = base.code_space_ranges.clone();
        }
        if cmap.name.is_empty() {
            cmap.name = base.name.clone();
        }
        if cmap.cid_system_info.registry.is_empty() {
            cmap.cid_system_info.registry = base.cid_system_info.registry.clone();
        }
        if cmap.cid_system_info.ordering.is_empty() {
            cmap.cid_system_info.ordering = base.cid_system_info.ordering.clone();
        }
        if cmap.cid_system_info.supplement == 0 {
            cmap.cid_system_info.supplement = base.cid_system_info.supplement;
        }
        if cmap.wmode == 0 {
            cmap.wmode = base.wmode;
        }

        let (mut chars, mut ranges, mut cid_chars, mut cid_ranges) = mapping_parts(&base.mappings);
        let (derived_chars, derived_ranges, derived_cid_chars, derived_cid_ranges) =
            mapping_parts(&cmap.mappings);
        chars.extend(derived_chars);
        cid_chars.extend(derived_cid_chars);
        ranges = derived_ranges.into_iter().chain(ranges).collect();
        cid_ranges = derived_cid_ranges.into_iter().chain(cid_ranges).collect();
        cmap.mappings = CMapMappings::Mixed {
            chars,
            ranges,
            cid_chars,
            cid_ranges,
        };
    }

    fn parse_cmap_data(&self, data: &[u8]) -> Option<CMap> {
        let content = std::str::from_utf8(data).ok()?;
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
                    self.budget.consume_node().ok()?;
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
                    self.budget.consume_node().ok()?;
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
                    self.budget.consume_node().ok()?;
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
                    self.budget.consume_node().ok()?;
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
                    self.budget.consume_node().ok()?;
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
        if !hex.is_ascii() {
            return None;
        }

        let digits = hex.as_bytes();
        let mut bytes = Vec::with_capacity(digits.len().div_ceil(2));
        for index in (0..digits.len()).step_by(2) {
            let high = hex_digit(digits[index])?;
            let low = match digits.get(index + 1) {
                Some(digit) => hex_digit(*digit)?,
                None => 0,
            };
            bytes.push((high << 4) | low);
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

    /// Decode a text byte sequence using the CMap code-space widths.
    pub fn decode_bytes(&self, cmap: &CMap, text: &[u8]) -> String {
        self.decode_bytes_with_budget(cmap, text, &ResourceBudget::default())
            .unwrap_or_default()
    }

    /// Decode a text byte sequence while charging input and decoded output.
    pub fn decode_bytes_with_budget(
        &self,
        cmap: &CMap,
        text: &[u8],
        budget: &ResourceBudget,
    ) -> Result<String, ResourceBudgetError> {
        budget.consume_input(text.len() as u64)?;
        let mut lengths: Vec<usize> = cmap
            .code_space_ranges
            .iter()
            .map(|range| range.start.len())
            .filter(|length| *length > 0)
            .collect();
        if lengths.is_empty() {
            lengths.extend([4, 3, 2, 1]);
        }
        lengths.sort_unstable_by(|left, right| right.cmp(left));
        lengths.dedup();

        let mut result = String::new();
        let mut offset = 0;
        while offset < text.len() {
            let mut decoded = None;
            let mut consumed = 0;

            for length in &lengths {
                let end = offset.saturating_add(*length);
                if end > text.len() {
                    continue;
                }
                let code = &text[offset..end];
                if !cmap.code_space_ranges.is_empty()
                    && !cmap.code_space_ranges.iter().any(|range| {
                        range.start.len() == code.len()
                            && self.in_range(code, &range.start, &range.end)
                    })
                {
                    continue;
                }
                if let Some(unicode) = self.map_code_to_unicode(cmap, code) {
                    decoded = Some(unicode);
                    consumed = *length;
                    break;
                }
            }

            if let Some(unicode) = decoded {
                budget.consume_decoded(unicode.len() as u64)?;
                result.push_str(&unicode);
                offset += consumed;
            } else {
                let character = text[offset] as char;
                budget.consume_decoded(character.len_utf8() as u64)?;
                result.push(character);
                offset += 1;
            }
        }
        Ok(result)
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
                let offset = self
                    .bytes_to_u32(code)?
                    .checked_sub(self.bytes_to_u32(start)?)?;
                self.increment_utf16_destination(base, offset)
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

    fn increment_utf16_destination(&self, base: &[u8], offset: u32) -> Option<String> {
        if base.is_empty() || !base.len().is_multiple_of(2) {
            return None;
        }
        let mut units: Vec<u16> = base
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        let last = units.last_mut()?;
        *last = (*last as u32).checked_add(offset)?.try_into().ok()?;
        String::from_utf16(&units).ok()
    }

    fn bytes_to_unicode(&self, bytes: &[u8]) -> Option<String> {
        if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        String::from_utf16(&units).ok()
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

fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn mapping_parts(mappings: &CMapMappings) -> MappingParts {
    match mappings {
        CMapMappings::Char(chars) => (chars.clone(), Vec::new(), HashMap::new(), Vec::new()),
        CMapMappings::Range(ranges) => (HashMap::new(), ranges.clone(), HashMap::new(), Vec::new()),
        CMapMappings::CID(cid_chars) => (HashMap::new(), Vec::new(), cid_chars.clone(), Vec::new()),
        CMapMappings::CIDRange(cid_ranges) => (
            HashMap::new(),
            Vec::new(),
            HashMap::new(),
            cid_ranges.clone(),
        ),
        CMapMappings::Mixed {
            chars,
            ranges,
            cid_chars,
            cid_ranges,
        } => (
            chars.clone(),
            ranges.clone(),
            cid_chars.clone(),
            cid_ranges.clone(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{CMapMappings, CMapParser};
    use crate::ast::PdfAstGraph;
    use crate::parser::reference_resolver::ObjectNodeMap;
    use crate::performance::ResourceBudgetError;
    use crate::types::{PdfDictionary, PdfStream};

    #[test]
    fn decodes_code_space_widths_and_utf16_surrogates() {
        let data = b"/CMapName /Test def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n2 beginbfchar\n<0041> <0041>\n<0042> <D83DDE00>\nendbfchar";
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);
        let cmap = parser.parse_cmap_data(data).expect("CMap should parse");

        assert!(matches!(cmap.mappings, CMapMappings::Char(_)));
        assert_eq!(parser.decode_bytes(&cmap, b"\x00A\x00B"), "A😀");
    }

    #[test]
    fn decodes_bfrange_destinations() {
        let data = b"/CMapName /Test def\n1 begincodespacerange\n<01> <02>\nendcodespacerange\n1 beginbfrange\n<01> <02> <0041>\nendbfrange";
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);
        let cmap = parser.parse_cmap_data(data).expect("CMap should parse");

        assert_eq!(parser.decode_bytes(&cmap, b"\x01\x02"), "AB");
    }

    #[test]
    fn rejects_bfrange_unicode_overflow() {
        let data = b"/CMapName /Test def\n1 begincodespacerange\n<00000000> <00000001>\nendcodespacerange\n1 beginbfrange\n<00000000> <00000001> <FFFFFFFF>\nendbfrange";
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);
        let cmap = parser.parse_cmap_data(data).expect("CMap should parse");

        assert!(parser.map_code_to_unicode(&cmap, &[0, 0, 0, 1]).is_none());
    }

    #[test]
    fn decodes_bfrange_multi_code_unit_destinations() {
        let data = b"/CMapName /Test def\n1 begincodespacerange\n<01> <02>\nendcodespacerange\n1 beginbfrange\n<01> <02> <006100620063>\nendbfrange";
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);
        let cmap = parser.parse_cmap_data(data).expect("CMap should parse");

        assert_eq!(parser.decode_bytes(&cmap, b"\x01\x02"), "abcabd");
    }

    #[test]
    fn rejects_invalid_cmap_utf8() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);

        assert!(parser
            .parse_cmap_data(b"begincmap\n\xff\nendcmap")
            .is_none());
    }

    #[test]
    fn cmap_decode_charges_input_and_output() {
        let data = b"/CMapName /Test def\n1 begincodespacerange\n<01> <01>\nendcodespacerange\n1 beginbfchar\n<01> <0041>\nendbfchar";
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);
        let cmap = parser.parse_cmap_data(data).expect("CMap should parse");

        let input_budget =
            crate::performance::ResourceBudget::new(0, 1024, 1024, 100, 10, 10, 10, 10);
        assert_eq!(
            parser
                .decode_bytes_with_budget(&cmap, b"\x01", &input_budget)
                .expect_err("text input must respect the budget"),
            ResourceBudgetError::InputBytes
        );

        let output_budget =
            crate::performance::ResourceBudget::new(1024, 0, 1024, 100, 10, 10, 10, 10);
        assert_eq!(
            parser
                .decode_bytes_with_budget(&cmap, b"\x01", &output_budget)
                .expect_err("decoded output must respect the budget"),
            ResourceBudgetError::DecodedBytes
        );
    }

    #[test]
    fn rejects_non_ascii_hex_without_panicking() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);

        assert!(parser.hex_to_bytes("\u{fffd}0").is_none());
    }

    #[test]
    fn pads_odd_length_pdf_hex_strings() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = CMapParser::new(&mut ast, &resolver);

        assert_eq!(parser.hex_to_bytes("<A>"), Some(vec![0xA0]));
    }

    #[test]
    fn inherits_usecmap_mappings_from_a_parsed_base() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let mut parser = CMapParser::new(&mut ast, &resolver);
        let base = PdfStream::new(
            PdfDictionary::new(),
            b"/CMapName /Base def\n1 begincodespacerange\n<01> <02>\nendcodespacerange\n1 beginbfchar\n<01> <0041>\nendbfchar".to_vec(),
        );
        parser
            .parse_cmap_stream(&base)
            .expect("base CMap should parse");

        let derived = PdfStream::new(
            PdfDictionary::new(),
            b"/CMapName /Derived def\n/UseCMap /Base usecmap\n1 beginbfchar\n<02> <0042>\nendbfchar".to_vec(),
        );
        let (_, cmap) = parser
            .parse_cmap_stream(&derived)
            .expect("derived CMap should parse");

        assert_eq!(parser.decode_bytes(&cmap, b"\x01\x02"), "AB");
        assert!(matches!(cmap.mappings, CMapMappings::Mixed { .. }));
    }

    #[test]
    fn rejects_cmap_mapping_count_over_budget() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = crate::performance::ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);
        let parser = CMapParser::new_with_budget(&mut ast, &resolver, &budget);
        let data = b"2 beginbfchar\n<01> <0041>\n<02> <0042>\nendbfchar";

        assert!(parser.parse_cmap_data(data).is_none());
    }
}
