use crate::ast::document::XRefEntry;
use crate::ast::{EdgeType, NodeId, NodeType, PdfAstGraph, PdfDocument};
use crate::filters::decode_stream_with_budget;
use crate::parser::{content_operands, content_stream, object_parser};
use crate::performance::PerformanceLimits;
use crate::types::{ObjectId, PdfDictionary, PdfReference, PdfValue, StreamData};
use log::{debug, info, warn};
use nom::IResult;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Read, Seek, SeekFrom};

/// Simple mapping from ObjectId to NodeId for use in parsers
pub struct ObjectNodeMap {
    object_to_node: HashMap<ObjectId, NodeId>,
}

fn stream_uses_jbig2(filter: Option<&PdfValue>) -> bool {
    match filter {
        Some(PdfValue::Name(name)) => name.without_slash() == "JBIG2Decode",
        Some(PdfValue::Array(filters)) => filters.iter().any(|filter| {
            filter
                .as_name()
                .is_some_and(|name| name.without_slash() == "JBIG2Decode")
        }),
        _ => false,
    }
}

impl Default for ObjectNodeMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectNodeMap {
    pub fn new() -> Self {
        ObjectNodeMap {
            object_to_node: HashMap::new(),
        }
    }

    pub fn insert(&mut self, obj_id: ObjectId, node_id: NodeId) {
        self.object_to_node.insert(obj_id, node_id);
    }

    pub fn get_node_id(&self, obj_id: &ObjectId) -> Option<NodeId> {
        self.object_to_node.get(obj_id).copied()
    }

    pub fn get_object_node_map(&self) -> ObjectNodeMap {
        ObjectNodeMap::from_map(self.object_to_node.clone())
    }

    pub fn from_map(map: HashMap<ObjectId, NodeId>) -> Self {
        ObjectNodeMap {
            object_to_node: map,
        }
    }
}

/// Resolves PDF references and builds complete object graph with proper edges
pub struct ReferenceResolver<R: BufRead + Seek> {
    reader: R,
    xref_table: HashMap<ObjectId, u64>,
    compressed_objects: HashMap<ObjectId, (u32, u32)>,
    object_to_node: HashMap<ObjectId, NodeId>, // Maps ObjectId to NodeId
    resolved_objects: HashSet<ObjectId>,
    pending_references: VecDeque<(NodeId, PdfReference)>, // (source_node, reference)
    tolerant: bool,
    limits: PerformanceLimits,
}

impl<R: BufRead + Seek> ReferenceResolver<R> {
    pub fn new(mut reader: R, tolerant: bool, limits: PerformanceLimits) -> Result<Self, String> {
        let xref_table = Self::build_xref_table(&mut reader, &limits)?;

        Ok(Self {
            reader,
            xref_table,
            compressed_objects: HashMap::new(),
            object_to_node: HashMap::new(),
            resolved_objects: HashSet::new(),
            pending_references: VecDeque::new(),
            tolerant,
            limits,
        })
    }

    /// Create resolver using existing document xref information
    pub fn from_document(
        reader: R,
        document: &PdfDocument,
        tolerant: bool,
        limits: PerformanceLimits,
    ) -> Self {
        let mut xref_table = HashMap::new();
        let mut compressed_objects = HashMap::new();

        // Convert document xref entries to our format
        for (obj_id, entry) in &document.xref.entries {
            match entry {
                XRefEntry::InUse { offset, .. } => {
                    xref_table.insert(*obj_id, *offset);
                }
                XRefEntry::Compressed {
                    stream_object,
                    index,
                } => {
                    compressed_objects.insert(*obj_id, (*stream_object, *index));
                    // Track compressed object references
                    debug!(
                        "Object {:?} is compressed in stream {:?} at index {}",
                        obj_id, stream_object, index
                    );
                }
                _ => {}
            }
        }

        info!("Converted {} xref entries from document", xref_table.len());

        Self {
            reader,
            xref_table,
            compressed_objects,
            object_to_node: HashMap::new(),
            resolved_objects: HashSet::new(),
            pending_references: VecDeque::new(),
            tolerant,
            limits,
        }
    }

    /// Build cross-reference table by scanning the PDF
    fn build_xref_table(
        reader: &mut R,
        limits: &PerformanceLimits,
    ) -> Result<HashMap<ObjectId, u64>, String> {
        // Find startxref offset
        let file_size = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| format!("Seek error: {}", e))?;
        let tail_size = file_size.min(1024);
        reader
            .seek(SeekFrom::Start(file_size.saturating_sub(tail_size)))
            .map_err(|e| format!("Seek error: {}", e))?;

        let mut buffer = Vec::new();
        reader
            .read_to_end(&mut buffer)
            .map_err(|e| format!("Read error: {}", e))?;

        let content = String::from_utf8_lossy(&buffer);

        if let Some(startxref_pos) = content.rfind("startxref") {
            let xref_section = &content[startxref_pos..];
            if let Some(offset_str) = xref_section.lines().nth(1) {
                if let Ok(xref_offset) = offset_str.trim().parse::<u64>() {
                    return Self::parse_xref_table(reader, xref_offset, limits);
                }
            }
        }

        // Fallback: scan entire file
        Self::scan_for_objects(reader, limits)
    }

    /// Parse xref table at given offset
    fn parse_xref_table(
        reader: &mut R,
        offset: u64,
        limits: &PerformanceLimits,
    ) -> Result<HashMap<ObjectId, u64>, String> {
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Seek error: {}", e))?;

        let buffer = Self::read_limited(reader, limits.budget.max_input_bytes)?;

        // Try to parse as xref stream first (PDF 1.5+)
        if buffer.starts_with(b"<<") || buffer.iter().take(20).any(|&b| b.is_ascii_digit()) {
            // Might be xref stream object
            if let Ok((_, (_obj_id, PdfValue::Stream(stream)))) =
                object_parser::parse_indirect_object_with_max_depth(&buffer, limits.max_depth)
            {
                return crate::parser::xref::parse_xref_stream_with_limits(&stream, limits).map(
                    |entries| {
                        entries
                            .into_iter()
                            .filter_map(|(id, entry)| {
                                if let XRefEntry::InUse { offset, .. } = entry {
                                    Some((id, offset))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    },
                );
            }
        }

        // Parse traditional xref table
        let mut xref_table = HashMap::new();
        let content = String::from_utf8_lossy(&buffer);

        if content.starts_with("xref") {
            let mut lines = content.lines().skip(1); // Skip "xref"

            while let Some(line) = lines.next() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("trailer") {
                    break;
                }

                // Parse subsection header
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() == 2 {
                    if let (Ok(start), Ok(count)) =
                        (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
                        for i in 0..count {
                            if let Some(entry_line) = lines.next() {
                                let entry_parts: Vec<&str> =
                                    entry_line.split_whitespace().collect();
                                if entry_parts.len() >= 3 && entry_parts[2] == "n" {
                                    if let (Ok(offset), Ok(gen)) = (
                                        entry_parts[0].parse::<u64>(),
                                        entry_parts[1].parse::<u16>(),
                                    ) {
                                        let obj_id = ObjectId::new(start + i, gen);
                                        xref_table.insert(obj_id, offset);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(xref_table)
    }

    /// Scan entire file for object definitions
    fn scan_for_objects(
        reader: &mut R,
        limits: &PerformanceLimits,
    ) -> Result<HashMap<ObjectId, u64>, String> {
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| format!("Seek error: {}", e))?;

        let content = Self::read_limited(reader, limits.budget.max_input_bytes)?;

        let mut xref_table = HashMap::new();
        let mut pos = 0;

        // Find all "n m obj" patterns
        while pos < content.len() {
            if let Some(obj_pos) = Self::find_next_object(&content[pos..]) {
                let absolute_pos = pos + obj_pos;

                // Parse object header
                if let Ok((_, obj_id)) = Self::parse_object_header(&content[absolute_pos..]) {
                    xref_table.insert(obj_id, absolute_pos as u64);
                }

                pos = absolute_pos + 1;
            } else {
                break;
            }
        }

        info!("Found {} objects by scanning", xref_table.len());
        Ok(xref_table)
    }

    fn read_limited(reader: &mut R, max_bytes: u64) -> Result<Vec<u8>, String> {
        let mut content = Vec::new();
        reader
            .by_ref()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut content)
            .map_err(|e| format!("Read error: {}", e))?;
        if content.len() as u64 > max_bytes {
            return Err(format!(
                "Input exceeds resource limit of {} bytes",
                max_bytes
            ));
        }
        Ok(content)
    }

    fn find_next_object(data: &[u8]) -> Option<usize> {
        for i in 0..data.len().saturating_sub(10) {
            // Look for pattern: digit(s) space digit(s) space "obj"
            if data[i].is_ascii_digit() {
                let mut j = i;
                while j < data.len() && data[j].is_ascii_digit() {
                    j += 1;
                }
                if j < data.len() && data[j].is_ascii_whitespace() {
                    j += 1;
                    while j < data.len() && data[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < data.len() && data[j].is_ascii_digit() {
                        let _k = j;
                        while j < data.len() && data[j].is_ascii_digit() {
                            j += 1;
                        }
                        if j + 4 <= data.len() && &data[j..j + 4] == b" obj" {
                            return Some(i);
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_object_header(data: &[u8]) -> IResult<&[u8], ObjectId> {
        use nom::{
            bytes::complete::tag,
            character::complete::{digit1, space1},
            combinator::map_opt,
            sequence::tuple,
        };

        map_opt(
            tuple((digit1, space1, digit1, space1, tag(b"obj"))),
            |(num, _, gen, _, _)| {
                let num = std::str::from_utf8(num).ok()?.parse().ok()?;
                let gen = std::str::from_utf8(gen).ok()?.parse().ok()?;
                Some(ObjectId::new(num, gen))
            },
        )(data)
    }

    /// Resolve all references in the AST with proper edge creation
    pub fn resolve_references(&mut self, ast: &mut PdfAstGraph) -> Result<(), String> {
        // First pass: collect all references from existing nodes
        let nodes = ast.get_all_nodes();
        for node in &nodes {
            self.collect_references_from_node(node.id, &node.value);
        }

        // Second pass: resolve references and create edges
        while let Some((source_node, pdf_ref)) = self.pending_references.pop_front() {
            self.limits.budget.check().map_err(|err| err.to_string())?;
            let obj_id = pdf_ref.id();

            // Check if we already have this object as a node
            let target_node = if let Some(&existing_node) = self.object_to_node.get(&obj_id) {
                existing_node
            } else if !self.resolved_objects.contains(&obj_id) {
                // Resolve the object
                match self.resolve_object(obj_id, ast) {
                    Ok(node_id) => {
                        self.resolved_objects.insert(obj_id);
                        self.object_to_node.insert(obj_id, node_id);
                        // Newly loaded indirect objects can contain references of their own.
                        // Queue them immediately so nested resources are resolved transitively.
                        let value = ast.get_node(node_id).map(|node| node.value.clone());
                        if let Some(value) = value {
                            self.collect_references_from_node(node_id, &value);
                        }
                        node_id
                    }
                    Err(e) => {
                        if self.tolerant {
                            warn!("Failed to resolve reference {}: {}", obj_id, e);
                            continue;
                        }
                        return Err(e);
                    }
                }
            } else {
                continue; // Already resolved but not found in map
            };

            // Create reference edge from source to target
            self.limits
                .budget
                .consume_edge()
                .map_err(|err| err.to_string())?;
            self.add_edge(ast, source_node, target_node, EdgeType::Reference)?;
            debug!(
                "Created reference edge from {:?} to {:?} for object {}",
                source_node, target_node, obj_id
            );
        }

        // Third pass: resolve indirect Length references in streams
        self.resolve_stream_lengths(ast)?;

        // Resolve direct stream values for JBIG2 globals before any stream AST
        // decoding pass consumes the filter parameters.
        self.resolve_jbig2_globals(ast);

        // Fourth pass: build page resource nodes (colorspaces, ICC profiles)
        self.build_page_resources(ast)?;

        // Fifth pass: build font-related AST nodes
        self.build_font_resources(ast)?;

        // Sixth pass: build AST from content streams
        self.build_content_stream_ast(ast)?;

        // Seventh pass: attach JavaScript nodes from action dictionaries
        self.build_javascript_nodes(ast)?;

        Ok(())
    }

    fn build_page_resources(&self, ast: &mut PdfAstGraph) -> Result<(), String> {
        use crate::parser::colorspace::ColorSpaceParser;

        let resolver_map = ObjectNodeMap::from_map(self.object_to_node.clone());
        let node_ids: Vec<NodeId> = ast.get_all_nodes().iter().map(|n| n.id).collect();
        for node_id in node_ids {
            let node = match ast.get_node(node_id) {
                Some(node) => node,
                None => continue,
            };
            if node.node_type != NodeType::Page {
                continue;
            }

            let page_dict = match node.as_dict() {
                Some(dict) => dict.clone(),
                None => continue,
            };

            let resources = match page_dict.get("Resources") {
                Some(PdfValue::Dictionary(dict)) => Some(dict.clone()),
                Some(PdfValue::Reference(res_ref)) => self
                    .object_to_node
                    .get(&res_ref.id())
                    .and_then(|res_id| ast.get_node(*res_id))
                    .and_then(|res_node| res_node.as_dict().cloned()),
                _ => None,
            };

            let resources = match resources {
                Some(res) => res,
                None => continue,
            };

            if let Some(PdfValue::Dictionary(colorspaces)) = resources.get("ColorSpace") {
                for (cs_name, cs_value) in colorspaces.iter() {
                    let mut parser =
                        ColorSpaceParser::new_with_limits(ast, &resolver_map, &self.limits);
                    if let Some(cs_id) = parser.parse_colorspace(cs_value) {
                        self.add_edge(ast, node_id, cs_id, EdgeType::Resource)?;
                        if let Some(cs_node) = ast.get_node_mut(cs_id) {
                            cs_node
                                .metadata
                                .set_property("resource_name".to_string(), cs_name.to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn collect_references_from_node(&mut self, node_id: NodeId, value: &PdfValue) {
        let mut stack = vec![value];
        while let Some(current) = stack.pop() {
            match current {
                PdfValue::Reference(pdf_ref) => {
                    self.pending_references.push_back((node_id, *pdf_ref));
                }
                PdfValue::Array(array) => {
                    for item in array.iter() {
                        stack.push(item);
                    }
                }
                PdfValue::Dictionary(dict) => {
                    for (_, val) in dict.iter() {
                        stack.push(val);
                    }
                }
                PdfValue::Stream(stream) => {
                    for (_, val) in stream.dict.iter() {
                        stack.push(val);
                    }
                }
                _ => {}
            }
        }
    }

    /// Resolve a specific object and create its node
    fn resolve_object(
        &mut self,
        obj_id: ObjectId,
        ast: &mut PdfAstGraph,
    ) -> Result<NodeId, String> {
        self.limits
            .budget
            .consume_object()
            .map_err(|err| err.to_string())?;
        if let Some(&offset) = self.xref_table.get(&obj_id) {
            let buffer = self.read_object_buffer(offset)?;

            // Resolve an indirect stream length before parsing stream bytes.
            let parsed = match object_parser::parse_indirect_stream_prefix_with_max_depth(
                &buffer,
                self.limits.max_depth,
            ) {
                Ok((_, (_, dict))) => {
                    if let Some(PdfValue::Reference(length_ref)) = dict.get("Length") {
                        match self.load_object_value(length_ref.id()) {
                            Some(PdfValue::Integer(length)) if length >= 0 => {
                                match usize::try_from(length) {
                                    Ok(length) => {
                                        object_parser::parse_indirect_object_with_stream_length_and_max_depth(
                                            &buffer,
                                            length,
                                            self.limits.max_depth,
                                        )
                                    }
                                    Err(_) if self.tolerant => {
                                        object_parser::parse_indirect_object_with_max_depth(
                                            &buffer,
                                            self.limits.max_depth,
                                        )
                                    }
                                    Err(_) => {
                                        return Err(
                                            "Indirect stream Length is too large".to_string()
                                        )
                                    }
                                }
                            }
                            Some(_) if self.tolerant => {
                                object_parser::parse_indirect_object_with_max_depth(
                                    &buffer,
                                    self.limits.max_depth,
                                )
                            }
                            Some(_) => {
                                return Err("Indirect stream Length is not an integer".to_string())
                            }
                            None if self.tolerant => {
                                object_parser::parse_indirect_object_with_max_depth(
                                    &buffer,
                                    self.limits.max_depth,
                                )
                            }
                            None => {
                                return Err("Failed to resolve indirect stream Length".to_string())
                            }
                        }
                    } else {
                        object_parser::parse_indirect_object_with_max_depth(
                            &buffer,
                            self.limits.max_depth,
                        )
                    }
                }
                Err(_) => object_parser::parse_indirect_object_with_max_depth(
                    &buffer,
                    self.limits.max_depth,
                ),
            };

            // Try to parse the object
            match parsed {
                Ok((rest, (parsed_obj_id, value))) => {
                    if parsed_obj_id != obj_id {
                        let message = format!(
                            "Object ID mismatch: expected {} {}, got {} {}",
                            obj_id.number,
                            obj_id.generation,
                            parsed_obj_id.number,
                            parsed_obj_id.generation
                        );
                        if !self.tolerant {
                            return Err(message);
                        }
                        warn!("{message}");

                        let node_id =
                            self.create_node(ast, NodeType::Object(obj_id), PdfValue::Null)?;
                        if let Some(node) = ast.get_node_mut(node_id) {
                            node.metadata.offset = Some(offset);
                            node.metadata.size = Some(buffer.len() - rest.len());
                            node.metadata.errors.push(crate::ast::node::ParseError {
                                code: crate::ast::node::ErrorCode::InvalidReference,
                                message,
                                offset: Some(offset),
                                recoverable: true,
                            });
                            node.metadata
                                .warnings
                                .push("Discarded object bytes after ObjectId mismatch".to_string());
                            node.metadata.properties.insert(
                                "object_id".to_string(),
                                format!("{} {} R", obj_id.number, obj_id.generation),
                            );
                            node.metadata
                                .properties
                                .insert("recovery".to_string(), "object_id_mismatch".to_string());
                        }
                        return Ok(node_id);
                    }

                    // Create node with proper type
                    let node_type = self.determine_node_type(&value, obj_id);
                    let node_id = self.create_node(ast, node_type, value)?;

                    // Add metadata
                    if let Some(node) = ast.get_node_mut(node_id) {
                        node.metadata.offset = Some(offset);
                        node.metadata.size = Some(buffer.len() - rest.len());
                        node.metadata.properties.insert(
                            "object_id".to_string(),
                            format!("{} {} R", obj_id.number, obj_id.generation),
                        );
                        if let PdfValue::Stream(stream) = &node.value {
                            let declared_length = match stream.dict.get("Length") {
                                Some(PdfValue::Integer(length)) if *length >= 0 => {
                                    length.to_string()
                                }
                                Some(PdfValue::Reference(reference)) => {
                                    format!(
                                        "{} {} R",
                                        reference.object_number, reference.generation_number
                                    )
                                }
                                Some(_) => "invalid".to_string(),
                                None => "missing".to_string(),
                            };
                            node.metadata
                                .properties
                                .insert("declared_length".to_string(), declared_length);
                            node.metadata.properties.insert(
                                "observed_length".to_string(),
                                stream.data.len().to_string(),
                            );
                            node.metadata.properties.insert(
                                "decode_state".to_string(),
                                match &stream.data {
                                    StreamData::Raw(_) => "raw",
                                    StreamData::Decoded(_) => "decoded",
                                    StreamData::Lazy(_) => "lazy",
                                }
                                .to_string(),
                            );
                            node.metadata
                                .properties
                                .insert("stream_length".to_string(), stream.data.len().to_string());
                            node.metadata.properties.insert(
                                "stream_filters".to_string(),
                                stream
                                    .get_filters()
                                    .iter()
                                    .map(|f| f.name())
                                    .collect::<Vec<_>>()
                                    .join(","),
                            );
                        }
                    }

                    Ok(node_id)
                }
                Err(e) => {
                    if self.tolerant {
                        if let Some(recovered) = self.parse_object_value_fallback(&buffer) {
                            let node_type = self.determine_node_type(&recovered, obj_id);
                            let node_id = self.create_node(ast, node_type, recovered)?;
                            if let Some(node) = ast.get_node_mut(node_id) {
                                node.metadata.offset = Some(offset);
                                node.metadata.size = Some(buffer.len());
                                node.metadata.warnings.push(
                                    "Recovered object by parsing value after obj keyword"
                                        .to_string(),
                                );
                                node.metadata.properties.insert(
                                    "recovery".to_string(),
                                    "parse_value_after_obj".to_string(),
                                );
                            }
                            return Ok(node_id);
                        }

                        let node_id =
                            self.create_node(ast, NodeType::Object(obj_id), PdfValue::Null)?;
                        if let Some(node) = ast.get_node_mut(node_id) {
                            node.metadata.offset = Some(offset);
                            node.metadata.size = Some(buffer.len());
                            node.metadata.errors.push(crate::ast::node::ParseError {
                                code: crate::ast::node::ErrorCode::InvalidSyntax,
                                message: format!("Failed to parse object: {:?}", e),
                                offset: Some(offset),
                                recoverable: true,
                            });
                            node.metadata
                                .warnings
                                .push("Recovered from parse error".to_string());
                            node.metadata.properties.insert(
                                "recovery".to_string(),
                                "parse_indirect_object_failed".to_string(),
                            );
                        }
                        Ok(node_id)
                    } else {
                        Err(format!(
                            "Failed to parse object at offset {}: {:?}",
                            offset, e
                        ))
                    }
                }
            }
        } else if let Some(&(stream_object, index)) = self.compressed_objects.get(&obj_id) {
            let (value, meta) = self
                .resolve_compressed_object(stream_object, index)
                .map_err(|e| format!("Compressed object {} error: {}", obj_id.number, e))?;
            let node_type = self.determine_node_type(&value, obj_id);
            let node_id = self.create_node(ast, node_type, value)?;

            if let Some(node) = ast.get_node_mut(node_id) {
                node.metadata.offset = meta.file_offset;
                node.metadata.size = meta.object_length;
                node.metadata.properties.insert(
                    "object_id".to_string(),
                    format!("{} {} R", obj_id.number, obj_id.generation),
                );
                node.metadata.properties.insert(
                    "container_stream".to_string(),
                    format!("{} 0 R", stream_object),
                );
                if let Some(offset) = meta.container_offset {
                    node.metadata
                        .properties
                        .insert("container_stream_offset".to_string(), offset.to_string());
                }
                if let Some(stream_offset) = meta.object_offset {
                    node.metadata.properties.insert(
                        "object_stream_offset".to_string(),
                        stream_offset.to_string(),
                    );
                }
                if let Some(stream_length) = meta.object_length {
                    node.metadata.properties.insert(
                        "object_stream_length".to_string(),
                        stream_length.to_string(),
                    );
                }
                node.metadata
                    .properties
                    .insert("object_stream_index".to_string(), index.to_string());
            }

            Ok(node_id)
        } else if self.tolerant {
            let node_id = self.create_node(ast, NodeType::Object(obj_id), PdfValue::Null)?;
            if let Some(node) = ast.get_node_mut(node_id) {
                node.metadata.errors.push(crate::ast::node::ParseError {
                    code: crate::ast::node::ErrorCode::MissingObject,
                    message: "Object not found in xref table".to_string(),
                    offset: None,
                    recoverable: true,
                });
                node.metadata
                    .warnings
                    .push("Recovered missing object reference".to_string());
                node.metadata
                    .properties
                    .insert("recovery".to_string(), "xref_missing_object".to_string());
            }
            Ok(node_id)
        } else {
            Err(format!("Object {} not found in xref table", obj_id))
        }
    }

    fn parse_object_value_fallback(&self, buffer: &[u8]) -> Option<PdfValue> {
        let obj_pos = buffer.windows(3).position(|w| w == b"obj")?;
        let mut pos = obj_pos + 3;
        while pos < buffer.len() && buffer[pos].is_ascii_whitespace() {
            pos += 1;
        }
        object_parser::parse_value_with_max_depth(&buffer[pos..], self.limits.max_depth)
            .ok()
            .map(|(_, value)| value)
    }

    fn resolve_compressed_object(
        &mut self,
        stream_object: u32,
        index: u32,
    ) -> Result<(PdfValue, CompressedObjectMeta), String> {
        let stream_id = ObjectId::new(stream_object, 0);
        let stream_offset = self.xref_table.get(&stream_id).copied();
        let (stream, dict) = self.load_object_stream(stream_object)?;
        let (value, object_offset, object_length) =
            self.parse_object_stream_entry(&stream, &dict, index)?;

        Ok((
            value,
            CompressedObjectMeta {
                file_offset: stream_offset,
                container_offset: stream_offset,
                object_offset: Some(object_offset as u64),
                object_length: Some(object_length),
            },
        ))
    }

    fn load_object_stream(
        &mut self,
        stream_object: u32,
    ) -> Result<(Vec<u8>, PdfDictionary), String> {
        let stream_id = ObjectId::new(stream_object, 0);
        let offset = self
            .xref_table
            .get(&stream_id)
            .copied()
            .ok_or_else(|| format!("Object stream {} offset missing", stream_object))?;
        let buffer = self.read_object_buffer(offset)?;

        let (_, (_obj_id, value)) =
            object_parser::parse_indirect_object_with_max_depth(&buffer, self.limits.max_depth)
                .map_err(|e| format!("Failed to parse object stream: {:?}", e))?;
        let stream = match value {
            PdfValue::Stream(stream) => stream,
            _ => return Err("Object stream is not a stream".to_string()),
        };

        let filters = stream.get_filters();
        let raw = stream
            .raw_data()
            .ok_or_else(|| "Object stream has no data".to_string())?;

        let decoded = decode_stream_with_budget(raw, &filters, &self.limits.budget)
            .map_err(|e| format!("Failed to decode object stream: {}", e))?;

        Ok((decoded, stream.dict))
    }

    fn parse_object_stream_entry(
        &self,
        data: &[u8],
        dict: &PdfDictionary,
        index: u32,
    ) -> Result<(PdfValue, usize, usize), String> {
        let n = dict
            .get("N")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| "Missing N in object stream".to_string())?
            .try_into()
            .map_err(|_| "Invalid N in object stream".to_string())?;
        let first = dict
            .get("First")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| "Missing First in object stream".to_string())?
            .try_into()
            .map_err(|_| "Invalid First in object stream".to_string())?;
        let index: usize = index
            .try_into()
            .map_err(|_| "Invalid object stream index".to_string())?;

        if index >= n {
            return Err("Object stream index out of range".to_string());
        }

        let offsets = object_parser::parse_object_stream_offsets_with_budget(
            data,
            n,
            first,
            &self.limits.budget,
        )?;
        let start = offsets[index];
        let next_offset = offsets
            .iter()
            .copied()
            .filter(|candidate| *candidate > start)
            .min()
            .unwrap_or(data.len());

        if start >= data.len() || start >= next_offset {
            return Err("Invalid object stream offsets".to_string());
        }

        let slice = data
            .get(start..next_offset)
            .ok_or_else(|| "Invalid object stream offsets".to_string())?;
        let (_, value) = object_parser::parse_value_with_max_depth(slice, self.limits.max_depth)
            .map_err(|e| format!("Parse value error: {:?}", e))?;
        Ok((value, start, next_offset - start))
    }

    fn determine_node_type(&self, value: &PdfValue, obj_id: ObjectId) -> NodeType {
        if let PdfValue::Dictionary(dict) = value {
            if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                match type_name.as_str() {
                    "/Catalog" => return NodeType::Catalog,
                    "/Pages" => return NodeType::Pages,
                    "/Page" => return NodeType::Page,
                    "/StructTreeRoot" => return NodeType::StructTreeRoot,
                    "/StructElem" => return NodeType::StructElem,
                    "/Font" => return NodeType::Font,
                    "/XObject" => {
                        if let Some(PdfValue::Name(subtype)) = dict.get("Subtype") {
                            if subtype.as_str() == "/Image" {
                                return NodeType::Image;
                            }
                        }
                        return NodeType::XObject;
                    }
                    "/Annot" => return NodeType::Annotation,
                    "/Metadata" => return NodeType::Metadata,
                    _ => {}
                }
            }
        }

        if let PdfValue::Stream(_) = value {
            return NodeType::ContentStream;
        }

        NodeType::Object(obj_id)
    }

    /// Resolve indirect Length references in streams
    fn resolve_stream_lengths(&mut self, ast: &mut PdfAstGraph) -> Result<(), String> {
        let nodes = ast.get_all_nodes();
        let mut updates = Vec::new();

        for node in nodes {
            if let PdfValue::Stream(stream) = &node.value {
                if let Some(PdfValue::Reference(length_ref)) = stream.dict.get("Length") {
                    // Resolve the length reference
                    let length_obj_id = length_ref.id();

                    if let Some(&offset) = self.xref_table.get(&length_obj_id) {
                        self.reader
                            .seek(SeekFrom::Start(offset))
                            .map_err(|e| format!("Seek error: {}", e))?;

                        let mut buffer = vec![0u8; 1024];
                        let bytes_read = self
                            .reader
                            .read(&mut buffer)
                            .map_err(|e| format!("Read error: {}", e))?;

                        if let Ok((_, (_, PdfValue::Integer(length)))) =
                            object_parser::parse_indirect_object_with_max_depth(
                                &buffer[..bytes_read],
                                self.limits.max_depth,
                            )
                        {
                            let length = usize::try_from(length)
                                .map_err(|_| "Indirect stream Length must be non-negative")?;
                            updates.push((node.id, length));
                            info!(
                                "Resolved indirect Length {} for stream in node {:?}",
                                length, node.id
                            );
                        }
                    }
                }
            }
        }

        // Keep the declared /Length and raw bytes intact; record the resolved value separately.
        for (node_id, length) in updates {
            if let Some(node) = ast.get_node_mut(node_id) {
                if let PdfValue::Stream(ref mut stream) = node.value {
                    node.metadata
                        .properties
                        .insert("resolved_length".to_string(), length.to_string());
                    node.metadata
                        .properties
                        .insert("observed_length".to_string(), stream.data.len().to_string());
                }
            }
        }

        Ok(())
    }

    fn resolve_jbig2_globals(&mut self, ast: &mut PdfAstGraph) {
        let node_ids: Vec<NodeId> = ast.get_all_nodes().iter().map(|node| node.id).collect();

        for node_id in node_ids {
            let mut dict = match ast
                .get_node(node_id)
                .and_then(|node| node.as_stream())
                .filter(|stream| stream_uses_jbig2(stream.dict.get("Filter")))
                .map(|stream| stream.dict.clone())
            {
                Some(dict) => dict,
                None => continue,
            };

            let changed = dict
                .get_mut("DecodeParms")
                .map(|params| self.resolve_jbig2_globals_value(params, ast))
                .unwrap_or(false);
            if changed {
                if let Some(node) = ast.get_node_mut(node_id) {
                    if let PdfValue::Stream(stream) = &mut node.value {
                        stream.dict = dict;
                    }
                }
            }
        }
    }

    fn resolve_jbig2_globals_value(&mut self, value: &mut PdfValue, ast: &PdfAstGraph) -> bool {
        match value {
            PdfValue::Dictionary(dict) => self.resolve_jbig2_globals_dict(dict, ast),
            PdfValue::Array(values) => values
                .iter_mut()
                .any(|value| self.resolve_jbig2_globals_value(value, ast)),
            _ => false,
        }
    }

    fn resolve_jbig2_globals_dict(&mut self, dict: &mut PdfDictionary, ast: &PdfAstGraph) -> bool {
        let reference = match dict.get("JBIG2Globals") {
            Some(PdfValue::Reference(reference)) => *reference,
            _ => return false,
        };
        let Some(mut stream) = self
            .object_to_node
            .get(&reference.id())
            .and_then(|node_id| ast.get_node(*node_id))
            .and_then(|node| node.as_stream())
            .cloned()
        else {
            return false;
        };

        if let Some(raw) = stream.raw_data() {
            let filters = stream.get_filters_with_params();
            if let Ok(decoded) = decode_stream_with_budget(raw, &filters, &self.limits.budget) {
                stream.data = StreamData::Decoded(decoded);
            }
        }
        dict.insert("JBIG2Globals", PdfValue::Stream(stream));
        true
    }

    /// Build AST nodes from content streams
    fn build_content_stream_ast(&mut self, ast: &mut PdfAstGraph) -> Result<(), String> {
        let nodes = ast.get_all_nodes();
        let mut content_streams = Vec::new();

        // Find all content streams
        for node in nodes {
            if matches!(node.node_type, NodeType::ContentStream)
                || (matches!(node.node_type, NodeType::Page) && node.as_dict().is_some())
            {
                content_streams.push(node.id);
            }
        }

        // Process each content stream
        for stream_node_id in content_streams {
            if let Some(node) = ast.get_node(stream_node_id) {
                let stream_data = if let PdfValue::Stream(stream) = &node.value {
                    // Decode the stream if needed
                    // Get stream data and filters
                    let data = match &stream.data {
                        crate::types::stream::StreamData::Raw(data) => data,
                        crate::types::stream::StreamData::Decoded(data) => data,
                        _ => continue, // Skip lazy streams for now
                    };
                    let filters = stream.get_filters();
                    match decode_stream_with_budget(data, &filters, &self.limits.budget) {
                        Ok(decoded) => decoded,
                        Err(e) => {
                            let message = format!("Failed to decode stream: {}", e);
                            self.record_stream_issue(
                                ast,
                                stream_node_id,
                                crate::ast::ErrorCode::CorruptedStream,
                                message.clone(),
                                "stream_decode_skipped",
                            );
                            if !self.tolerant {
                                return Err(message);
                            }
                            continue;
                        }
                    }
                } else if let PdfValue::Dictionary(dict) = &node.value {
                    // Page dictionary - look for Contents
                    if let Some(PdfValue::Reference(_)) = dict.get("Contents") {
                        continue; // Will be resolved separately
                    }
                    continue;
                } else {
                    continue;
                };

                let indexed = match content_operands::parse_content_stream_with_offsets_with_budget(
                    &stream_data,
                    &self.limits.budget,
                ) {
                    Ok(indexed) => indexed,
                    Err(error) => {
                        let message = format!("Failed to parse content stream: {}", error);
                        self.record_stream_issue(
                            ast,
                            stream_node_id,
                            crate::ast::ErrorCode::InvalidSyntax,
                            message.clone(),
                            "content_stream_skipped",
                        );
                        if !self.tolerant {
                            return Err(message);
                        }
                        continue;
                    }
                };

                for (i, item) in indexed.iter().enumerate() {
                    let op_node_id = self.create_operator_node(ast, &item.operator, i)?;
                    if let Some(node) = ast.get_node_mut(op_node_id) {
                        node.metadata.offset = Some(item.offset as u64);
                        node.metadata
                            .properties
                            .insert("stream_local_offset".to_string(), item.offset.to_string());
                        node.metadata
                            .properties
                            .insert("content_operator_index".to_string(), i.to_string());
                    }
                    self.add_edge(ast, stream_node_id, op_node_id, EdgeType::Child)?;
                }
                info!(
                    "Created {} operator nodes with offsets for stream {:?}",
                    indexed.len(),
                    stream_node_id
                );
            }
        }

        Ok(())
    }

    fn record_stream_issue(
        &self,
        ast: &mut PdfAstGraph,
        stream_node_id: NodeId,
        code: crate::ast::ErrorCode,
        message: String,
        recovery: &str,
    ) {
        if let Some(node) = ast.get_node_mut(stream_node_id) {
            let offset = node.metadata.offset;
            node.metadata.errors.push(crate::ast::ParseError {
                code,
                message: message.clone(),
                offset,
                recoverable: self.tolerant,
            });
            node.metadata.warnings.push(message);
            node.metadata
                .properties
                .insert("recovery".to_string(), recovery.to_string());
        }
    }

    fn build_javascript_nodes(&mut self, ast: &mut PdfAstGraph) -> Result<(), String> {
        let node_ids: Vec<NodeId> = ast.get_all_nodes().iter().map(|n| n.id).collect();

        for node_id in node_ids {
            let dict = match ast.get_node(node_id).and_then(|node| node.as_dict()) {
                Some(d) => d.clone(),
                None => continue,
            };

            let Some(js_value) = dict.get("JS").or_else(|| dict.get("JavaScript")) else {
                continue;
            };

            let existing_js = ast.get_children(node_id).into_iter().any(|child| {
                ast.get_node(child)
                    .map(|n| n.node_type == NodeType::JavaScript)
                    .unwrap_or(false)
            });
            if existing_js {
                continue;
            }

            let resolved = match js_value {
                PdfValue::Reference(r) => match self.load_object_value(r.id()) {
                    Some(value) => value,
                    None if self.tolerant => PdfValue::Null,
                    None => return Err(format!("Failed to resolve JavaScript object {}", r.id())),
                },
                value => value.clone(),
            };

            let js_id = self.create_node(ast, NodeType::JavaScript, resolved)?;
            self.add_edge(ast, node_id, js_id, EdgeType::Child)?;
        }

        Ok(())
    }

    fn build_font_resources(&mut self, ast: &mut PdfAstGraph) -> Result<(), String> {
        let nodes = ast.get_all_nodes();
        let mut fonts = Vec::new();

        for node in nodes {
            if matches!(
                node.node_type,
                NodeType::Font
                    | NodeType::Type1Font
                    | NodeType::TrueTypeFont
                    | NodeType::Type3Font
                    | NodeType::CIDFont
            ) {
                fonts.push(node.id);
            }
        }

        for font_id in fonts {
            let dict = match ast.get_node(font_id).and_then(|n| n.as_dict()).cloned() {
                Some(d) => d,
                None => continue,
            };

            if let Some(encoding_val) = dict.get("Encoding") {
                self.attach_encoding_node(ast, font_id, encoding_val)?;
            }

            if let Some(to_unicode_val) = dict.get("ToUnicode") {
                self.attach_tounicode_node(ast, font_id, to_unicode_val)?;
            }

            if let Some(cid_info) = dict.get("CIDSystemInfo") {
                let cid_id = self.create_node(ast, NodeType::Metadata, cid_info.clone())?;
                if let Some(node) = ast.get_node_mut(cid_id) {
                    node.metadata
                        .set_property("metadata_kind".to_string(), "cid_system_info".to_string());
                }
                self.add_edge(ast, font_id, cid_id, EdgeType::Child)?;
            }
        }

        Ok(())
    }

    fn attach_encoding_node(
        &mut self,
        ast: &mut PdfAstGraph,
        font_id: NodeId,
        value: &PdfValue,
    ) -> Result<(), String> {
        let resolved = match value {
            PdfValue::Reference(r) => match self.load_object_value(r.id()) {
                Some(value) => value,
                None if self.tolerant => PdfValue::Null,
                None => return Err(format!("Failed to resolve font encoding object {}", r.id())),
            },
            _ => value.clone(),
        };

        let encoding_id = self.create_node(ast, NodeType::Encoding, resolved)?;
        if let Some(node) = ast.get_node_mut(encoding_id) {
            node.metadata
                .set_property("metadata_kind".to_string(), "font_encoding".to_string());
        }
        self.add_edge(ast, font_id, encoding_id, EdgeType::Child)?;
        Ok(())
    }

    fn attach_tounicode_node(
        &mut self,
        ast: &mut PdfAstGraph,
        font_id: NodeId,
        value: &PdfValue,
    ) -> Result<(), String> {
        let resolved = match value {
            PdfValue::Reference(r) => match self.load_object_value(r.id()) {
                Some(value) => value,
                None if self.tolerant => PdfValue::Null,
                None => return Err(format!("Failed to resolve ToUnicode object {}", r.id())),
            },
            _ => value.clone(),
        };

        let stream = match resolved {
            PdfValue::Stream(stream) => stream,
            _ => {
                let node_id = self.create_node(ast, NodeType::ToUnicode, resolved)?;
                self.add_edge(ast, font_id, node_id, EdgeType::Child)?;
                return Ok(());
            }
        };

        let map = self.object_to_node.clone();
        let resolver_map = ObjectNodeMap::from_map(map);
        let mut cmap_parser = crate::parser::cmap::CMapParser::new_with_budget(
            ast,
            &resolver_map,
            &self.limits.budget,
        );
        if let Some(node_id) = cmap_parser.parse_tounicode_stream(&stream) {
            self.add_edge(ast, font_id, node_id, EdgeType::Child)?;
        }
        Ok(())
    }

    fn load_object_value(&mut self, obj_id: ObjectId) -> Option<PdfValue> {
        self.limits.budget.consume_object().ok()?;
        let offset = self.xref_table.get(&obj_id).copied()?;
        let buffer = self.read_object_buffer(offset).ok()?;

        object_parser::parse_indirect_object_with_max_depth(&buffer, self.limits.max_depth)
            .ok()
            .map(|(_, (_, value))| value)
    }

    fn read_object_buffer(&mut self, offset: u64) -> Result<Vec<u8>, String> {
        let file_size = self
            .reader
            .seek(SeekFrom::End(0))
            .map_err(|e| format!("Seek error: {}", e))?;
        if offset >= file_size {
            return Err(format!("Object offset {} is outside the file", offset));
        }

        let max_bytes =
            self.limits
                .max_object_size_mb
                .checked_mul(1024 * 1024)
                .ok_or_else(|| "Object size limit overflow".to_string())? as u64;
        let next_offset = self
            .xref_table
            .values()
            .copied()
            .filter(|candidate| *candidate > offset)
            .min()
            .unwrap_or(file_size);
        let bound = next_offset
            .saturating_sub(offset)
            .min(file_size.saturating_sub(offset))
            .min(max_bytes);

        self.reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Seek error: {}", e))?;
        let mut buffer = Vec::new();
        self.reader
            .by_ref()
            .take(bound)
            .read_to_end(&mut buffer)
            .map_err(|e| format!("Read error: {}", e))?;
        Ok(buffer)
    }

    fn create_operator_node(
        &self,
        ast: &mut PdfAstGraph,
        operator: &content_stream::ContentOperator,
        index: usize,
    ) -> Result<NodeId, String> {
        use content_stream::ContentOperator;

        // Create appropriate value for the operator
        let value = match operator {
            ContentOperator::BeginText => PdfValue::Name(crate::types::PdfName::new("BT")),
            ContentOperator::EndText => PdfValue::Name(crate::types::PdfName::new("ET")),
            ContentOperator::SetFont(name, size) => {
                let mut dict = PdfDictionary::new();
                dict.insert("Font", PdfValue::Name(crate::types::PdfName::new(name)));
                dict.insert("Size", PdfValue::Real(*size));
                PdfValue::Dictionary(dict)
            }
            ContentOperator::ShowText(text) => {
                PdfValue::String(crate::types::PdfString::new_literal(text.clone()))
            }
            ContentOperator::MoveText(x, y) => {
                let mut dict = PdfDictionary::new();
                dict.insert("X", PdfValue::Real(*x));
                dict.insert("Y", PdfValue::Real(*y));
                PdfValue::Dictionary(dict)
            }
            ContentOperator::PaintXObject(name) => PdfValue::Name(crate::types::PdfName::new(name)),
            _ => {
                // For other operators, create a simple name value
                PdfValue::Name(crate::types::PdfName::new(format!("Op_{}", index)))
            }
        };

        let node_id = self.create_node(ast, NodeType::ContentOperator, value)?;

        // Add metadata
        if let Some(node) = ast.get_node_mut(node_id) {
            node.metadata
                .properties
                .insert("operator_type".to_string(), format!("{:?}", operator));
            node.metadata
                .properties
                .insert("index".to_string(), index.to_string());
        }

        Ok(node_id)
    }

    fn create_node(
        &self,
        ast: &mut PdfAstGraph,
        node_type: NodeType,
        value: PdfValue,
    ) -> Result<NodeId, String> {
        self.limits
            .budget
            .consume_node()
            .map_err(|err| err.to_string())?;
        Ok(ast.create_node(node_type, value))
    }

    fn add_edge(
        &self,
        ast: &mut PdfAstGraph,
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
    ) -> Result<(), String> {
        self.limits
            .budget
            .consume_edge()
            .map_err(|err| err.to_string())?;
        if ast.add_edge(from, to, edge_type) {
            Ok(())
        } else {
            Err("Cannot add AST edge: node endpoint is missing".to_string())
        }
    }
}

#[derive(Debug, Clone)]
struct CompressedObjectMeta {
    file_offset: Option<u64>,
    container_offset: Option<u64>,
    object_offset: Option<u64>,
    object_length: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_object_header_parsing() {
        let data = b"123 0 obj";
        let result = ReferenceResolver::<Cursor<Vec<u8>>>::parse_object_header(data);
        assert!(result.is_ok());
        let (_, obj_id) = result.unwrap();
        assert_eq!(obj_id.number, 123);
        assert_eq!(obj_id.generation, 0);
    }

    #[test]
    fn rejects_object_header_numeric_overflow() {
        assert!(
            ReferenceResolver::<Cursor<Vec<u8>>>::parse_object_header(b"4294967296 0 obj").is_err()
        );
        assert!(ReferenceResolver::<Cursor<Vec<u8>>>::parse_object_header(b"1 65536 obj").is_err());
    }

    #[test]
    fn test_find_next_object() {
        let data = b"some text 42 0 obj more text";
        let pos = ReferenceResolver::<Cursor<Vec<u8>>>::find_next_object(data);
        assert_eq!(pos, Some(10)); // Position of "42"
    }

    #[test]
    fn test_reference_collection() {
        // Create a small PDF-like buffer to satisfy seek logic
        let pdf_data = vec![0u8; 2048]; // At least 1024 bytes so seek doesn't fail
        let mut resolver = ReferenceResolver::new(
            Cursor::new(pdf_data),
            true,
            crate::performance::PerformanceLimits::default(),
        )
        .unwrap();
        let mut ast = PdfAstGraph::new();

        // Create a node with a reference
        let mut dict = PdfDictionary::new();
        dict.insert("Ref", PdfValue::Reference(PdfReference::new(5, 0)));
        let node_id = ast.create_node(NodeType::Root, PdfValue::Dictionary(dict));

        // Collect references
        if let Some(node) = ast.get_node(node_id) {
            resolver.collect_references_from_node(node_id, &node.value);
        }

        assert_eq!(resolver.pending_references.len(), 1);
    }

    #[test]
    fn content_stream_failures_are_recorded_or_rejected_by_mode() {
        let mut dictionary = PdfDictionary::new();
        dictionary.insert(
            "Filter",
            PdfValue::Name(crate::types::PdfName::new("FlateDecode")),
        );

        let make_ast = || {
            let mut ast = PdfAstGraph::new();
            ast.create_node(
                NodeType::ContentStream,
                PdfValue::Stream(crate::types::PdfStream::new(
                    dictionary.clone(),
                    b"not a flate stream".to_vec(),
                )),
            );
            ast
        };
        let document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));

        let mut tolerant = ReferenceResolver::from_document(
            Cursor::new(Vec::new()),
            &document,
            true,
            crate::performance::PerformanceLimits::default(),
        );
        let mut tolerant_ast = make_ast();
        tolerant
            .build_content_stream_ast(&mut tolerant_ast)
            .expect("tolerant mode should preserve the failure and continue");
        let tolerant_node = tolerant_ast.get_node(crate::ast::NodeId::new(0)).unwrap();
        assert!(tolerant_node.is_error());
        assert_eq!(
            tolerant_node.metadata.properties.get("recovery"),
            Some(&"stream_decode_skipped".to_string())
        );

        let mut strict = ReferenceResolver::from_document(
            Cursor::new(Vec::new()),
            &document,
            false,
            crate::performance::PerformanceLimits::default(),
        );
        let mut strict_ast = make_ast();
        assert!(strict.build_content_stream_ast(&mut strict_ast).is_err());
    }

    #[test]
    fn xref_scan_respects_input_budget() {
        let mut limits = crate::performance::PerformanceLimits::default();
        limits.budget.max_input_bytes = 1024;
        let result = ReferenceResolver::new(Cursor::new(vec![0u8; 2048]), true, limits);

        assert!(matches!(
            result,
            Err(error) if error.contains("Input exceeds resource limit")
        ));
    }

    #[test]
    fn resolver_handles_inputs_smaller_than_xref_tail_window() {
        let result = ReferenceResolver::new(
            Cursor::new(vec![0u8; 16]),
            true,
            crate::performance::PerformanceLimits::default(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_negative_indirect_stream_length() {
        let document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));
        let mut resolver = ReferenceResolver::from_document(
            Cursor::new(b"2 0 obj\n-1\nendobj\n".to_vec()),
            &document,
            true,
            crate::performance::PerformanceLimits::default(),
        );
        resolver.xref_table.insert(ObjectId::new(2, 0), 0);

        let mut dict = PdfDictionary::new();
        dict.insert("Length", PdfValue::Reference(PdfReference::new(2, 0)));
        let mut ast = PdfAstGraph::new();
        ast.create_node(
            NodeType::ContentStream,
            PdfValue::Stream(crate::types::PdfStream::new(dict, Vec::new())),
        );

        let error = resolver.resolve_stream_lengths(&mut ast).unwrap_err();
        assert!(error.contains("non-negative"));
    }

    #[test]
    fn resolves_indirect_length_without_mutating_lossless_stream_state() {
        let document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));
        let mut resolver = ReferenceResolver::from_document(
            Cursor::new(b"2 0 obj\n3\nendobj\n".to_vec()),
            &document,
            true,
            crate::performance::PerformanceLimits::default(),
        );
        resolver.xref_table.insert(ObjectId::new(2, 0), 0);

        let mut dict = PdfDictionary::new();
        let length_ref = PdfReference::new(2, 0);
        dict.insert("Length", PdfValue::Reference(length_ref));
        let mut ast = PdfAstGraph::new();
        let node_id = ast.create_node(
            NodeType::ContentStream,
            PdfValue::Stream(crate::types::PdfStream::new(dict, b"abcd".to_vec())),
        );

        resolver
            .resolve_stream_lengths(&mut ast)
            .expect("indirect length should resolve");
        let node = ast.get_node(node_id).expect("stream node");
        let stream = node.value.as_stream().expect("stream value");
        assert_eq!(
            stream.dict.get("Length"),
            Some(&PdfValue::Reference(length_ref))
        );
        assert_eq!(stream.raw_data(), Some(b"abcd".as_slice()));
        assert_eq!(
            node.metadata.properties.get("resolved_length"),
            Some(&"3".to_string())
        );
        assert_eq!(
            node.metadata.properties.get("observed_length"),
            Some(&"4".to_string())
        );
    }

    #[test]
    fn object_id_mismatch_is_strict_or_null_recovery() {
        let data = b"2 0 obj\n<< /Type /Page >>\nendobj\n".to_vec();
        let mut document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));
        document.xref.entries.insert(
            ObjectId::new(1, 0),
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );

        let mut strict = ReferenceResolver::from_document(
            Cursor::new(data.clone()),
            &document,
            false,
            crate::performance::PerformanceLimits::default(),
        );
        let mut strict_ast = PdfAstGraph::new();
        assert!(strict
            .resolve_object(ObjectId::new(1, 0), &mut strict_ast)
            .is_err());

        let mut tolerant = ReferenceResolver::from_document(
            Cursor::new(data),
            &document,
            true,
            crate::performance::PerformanceLimits::default(),
        );
        let mut tolerant_ast = PdfAstGraph::new();
        let node_id = tolerant
            .resolve_object(ObjectId::new(1, 0), &mut tolerant_ast)
            .expect("tolerant mismatch should recover");
        let node = tolerant_ast.get_node(node_id).expect("recovery node");
        assert!(matches!(node.value, PdfValue::Null));
        assert!(node.is_error());
        assert_eq!(
            node.metadata.properties.get("recovery"),
            Some(&"object_id_mismatch".to_string())
        );
    }

    #[test]
    fn stream_data_can_contain_endobj_without_truncating_resolution() {
        let mut data =
            b"2 0 obj\n<< /Length 12 >>\nstream\nabcendobjxyz\nendstream\nendobj\n".to_vec();
        let next_offset = data.len() as u64;
        data.extend_from_slice(b"3 0 obj\n42\nendobj\n");

        let mut document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));
        document.xref.entries.insert(
            ObjectId::new(2, 0),
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );
        document.xref.entries.insert(
            ObjectId::new(3, 0),
            XRefEntry::InUse {
                offset: next_offset,
                generation: 0,
            },
        );

        let mut resolver = ReferenceResolver::from_document(
            Cursor::new(data),
            &document,
            false,
            crate::performance::PerformanceLimits::default(),
        );
        let mut ast = PdfAstGraph::new();
        let node_id = resolver
            .resolve_object(ObjectId::new(2, 0), &mut ast)
            .expect("stream object should resolve");
        let node = ast.get_node(node_id).expect("stream node");
        let stream = node.value.as_stream().expect("stream value");
        assert_eq!(stream.raw_data(), Some(b"abcendobjxyz".as_slice()));
    }

    #[test]
    fn strict_reference_resolution_rejects_missing_objects() {
        let document = PdfDocument::new(crate::ast::PdfVersion::new(1, 7));
        let mut resolver = ReferenceResolver::from_document(
            Cursor::new(Vec::new()),
            &document,
            false,
            crate::performance::PerformanceLimits::default(),
        );
        let mut dict = PdfDictionary::new();
        dict.insert("Missing", PdfValue::Reference(PdfReference::new(9, 0)));
        let mut ast = PdfAstGraph::new();
        let root = ast.create_node(NodeType::Root, PdfValue::Dictionary(dict));
        resolver.collect_references_from_node(root, &ast.get_node(root).unwrap().value.clone());

        let error = resolver
            .resolve_references(&mut ast)
            .expect_err("strict resolution must reject a missing object");
        assert!(error.contains("not found in xref table"));
    }
}
