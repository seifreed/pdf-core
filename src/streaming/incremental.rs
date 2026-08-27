use crate::ast::{AstNode, AstResult, NodeId, NodeType, PdfDocument, PdfVersion};
use crate::types::{ObjectId, PdfReference, PdfValue};
use std::collections::{HashMap, HashSet, VecDeque};

/// Incremental document processor that builds the AST progressively
pub struct IncrementalProcessor {
    document: PdfDocument,
    processing_queue: VecDeque<ProcessingTask>,
    completed_nodes: HashMap<NodeId, AstNode>,
    object_index: HashMap<ObjectId, NodeId>,
    pending_references: HashMap<ObjectId, Vec<NodeId>>,
    processing_statistics: ProcessingStatistics,
    tolerant: bool,
}

/// A task for processing a specific part of the document
#[derive(Debug, Clone)]
pub struct ProcessingTask {
    pub task_id: String,
    pub task_type: TaskType,
    pub priority: Priority,
    pub data: Vec<u8>,
    pub context: TaskContext,
}

/// Type of processing task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    ParseObject,
    ResolveReference,
    BuildNode,
    ValidateStructure,
    IndexContent,
}

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Context information for a processing task
#[derive(Debug, Clone)]
pub struct TaskContext {
    pub byte_offset: u64,
    pub parent_node: Option<NodeId>,
    pub expected_type: Option<NodeType>,
    pub reference_chain: Vec<String>,
}

/// Statistics for incremental processing
#[derive(Debug, Clone, Default)]
pub struct ProcessingStatistics {
    pub tasks_queued: usize,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub nodes_created: usize,
    pub references_resolved: usize,
    pub bytes_processed: u64,
    pub processing_time_ms: u64,
    pub memory_usage_mb: f64,
}

impl IncrementalProcessor {
    /// Create a new incremental processor
    pub fn new() -> Self {
        Self {
            document: PdfDocument::new(PdfVersion { major: 1, minor: 4 }),
            processing_queue: VecDeque::new(),
            completed_nodes: HashMap::new(),
            object_index: HashMap::new(),
            pending_references: HashMap::new(),
            processing_statistics: ProcessingStatistics::default(),
            tolerant: true,
        }
    }

    pub fn new_with_tolerance(tolerant: bool) -> Self {
        Self {
            tolerant,
            ..Self::new()
        }
    }

    /// Add a processing task to the queue
    pub fn queue_task(&mut self, task: ProcessingTask) {
        // Insert task based on priority
        let insert_pos = self
            .processing_queue
            .iter()
            .position(|t| t.priority < task.priority)
            .unwrap_or(self.processing_queue.len());

        self.processing_queue.insert(insert_pos, task);
        self.processing_statistics.tasks_queued += 1;
    }

    /// Process the next task in the queue
    pub fn process_next_task(&mut self) -> AstResult<Option<ProcessingResult>> {
        if let Some(task) = self.processing_queue.pop_front() {
            let start_time = std::time::Instant::now();

            let result = match task.task_type {
                TaskType::ParseObject => self.process_parse_object(task),
                TaskType::ResolveReference => self.process_resolve_reference(task),
                TaskType::BuildNode => self.process_build_node(task),
                TaskType::ValidateStructure => self.process_validate_structure(task),
                TaskType::IndexContent => self.process_index_content(task),
            };

            let processing_time = start_time.elapsed().as_millis() as u64;
            self.processing_statistics.processing_time_ms += processing_time;

            match result {
                Ok(result) => {
                    self.processing_statistics.tasks_completed += 1;
                    Ok(Some(result))
                }
                Err(e) => {
                    self.processing_statistics.tasks_failed += 1;
                    Err(e)
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Process all queued tasks
    pub fn process_all_tasks(&mut self) -> AstResult<Vec<ProcessingResult>> {
        let mut results = Vec::new();

        while !self.processing_queue.is_empty() {
            if let Some(result) = self.process_next_task()? {
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Add raw PDF data for incremental parsing
    pub fn add_data_chunk(&mut self, data: Vec<u8>, offset: u64) -> AstResult<()> {
        // Analyze the data chunk and create appropriate tasks
        let tasks = self.analyze_chunk(&data, offset)?;

        for task in tasks {
            self.queue_task(task);
        }

        self.processing_statistics.bytes_processed += data.len() as u64;
        Ok(())
    }

    /// Get the current document state
    pub fn get_document(&self) -> &PdfDocument {
        &self.document
    }

    /// Get processing statistics
    pub fn get_statistics(&self) -> &ProcessingStatistics {
        &self.processing_statistics
    }

    /// Check if processing is complete
    pub fn is_complete(&self) -> bool {
        self.processing_queue.is_empty() && self.pending_references.is_empty()
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        if self.processing_statistics.tasks_queued == 0 {
            100.0
        } else {
            (self.processing_statistics.tasks_completed as f64
                / self.processing_statistics.tasks_queued as f64)
                * 100.0
        }
    }

    // Private methods for task processing
    fn process_parse_object(&mut self, task: ProcessingTask) -> AstResult<ProcessingResult> {
        // Parse indirect object if present
        if let Ok((rest, (obj_id, pdf_value))) =
            crate::parser::object_parser::parse_indirect_object_with_budget(
                &task.data,
                &self.document.budget,
            )
        {
            let node_type = determine_node_type(&pdf_value, obj_id);
            let node_id = self
                .document
                .ast
                .create_node(node_type.clone(), pdf_value.clone());

            if let Some(node) = self.document.ast.get_node_mut(node_id) {
                node.metadata.offset = Some(task.context.byte_offset);
                node.metadata.size = Some(task.data.len().saturating_sub(rest.len()));
                node.metadata.properties.insert(
                    "object_id".to_string(),
                    format!("{} {} R", obj_id.number, obj_id.generation),
                );
            }

            let ast_node = AstNode::new(node_id, node_type, pdf_value.clone());
            self.completed_nodes.insert(node_id, ast_node);
            self.object_index.insert(obj_id, node_id);
            self.processing_statistics.nodes_created += 1;

            // Resolve pending references to this object
            if let Some(pending) = self.pending_references.remove(&obj_id) {
                for source in pending {
                    self.document
                        .ast
                        .add_edge(source, node_id, crate::ast::EdgeType::Reference);
                    self.processing_statistics.references_resolved += 1;
                }
            }

            // Collect references from this node and queue unresolved
            let mut references = Vec::new();
            collect_references_from_value(&pdf_value, &mut references);
            for reference in references {
                self.track_reference(node_id, reference);
            }

            return Ok(ProcessingResult {
                task_id: task.task_id,
                result_type: ResultType::NodeCreated,
                node_id: Some(node_id),
                data: None,
            });
        }

        // Fallback: parse as value
        let parser = crate::parser::PdfParser::new();
        match parser.parse_object(&task.data) {
            Ok(pdf_value) => {
                let node_id = self
                    .document
                    .ast
                    .create_node(NodeType::Unknown, pdf_value.clone());

                if let Some(node) = self.document.ast.get_node_mut(node_id) {
                    node.metadata.offset = Some(task.context.byte_offset);
                    node.metadata.size = Some(task.data.len());
                    node.metadata
                        .warnings
                        .push("Parsed value without object header".to_string());
                }

                let ast_node = AstNode::new(node_id, NodeType::Unknown, pdf_value);
                self.completed_nodes.insert(node_id, ast_node);
                self.processing_statistics.nodes_created += 1;

                Ok(ProcessingResult {
                    task_id: task.task_id,
                    result_type: ResultType::NodeCreated,
                    node_id: Some(node_id),
                    data: None,
                })
            }
            Err(e) => {
                if self.tolerant {
                    Ok(ProcessingResult {
                        task_id: task.task_id,
                        result_type: ResultType::ParseError,
                        node_id: None,
                        data: Some(format!("Parse error (tolerant): {:?}", e)),
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    fn process_resolve_reference(&mut self, task: ProcessingTask) -> AstResult<ProcessingResult> {
        // Resolve PDF object references
        let reference_id = String::from_utf8_lossy(&task.data).to_string();
        let obj_id = match parse_reference_id(&reference_id) {
            Some(id) => id,
            None => {
                return Ok(ProcessingResult {
                    task_id: task.task_id,
                    result_type: ResultType::ReferencePending,
                    node_id: None,
                    data: Some(reference_id),
                })
            }
        };

        // Check if we have a node for this reference
        if let Some(node_id) = self.object_index.get(&obj_id).copied() {
            // Update pending references
            if let Some(pending) = self.pending_references.remove(&obj_id) {
                for pending_node in pending {
                    // Link the nodes
                    self.document.ast.add_edge(
                        pending_node,
                        node_id,
                        crate::ast::EdgeType::Reference,
                    );
                }
            }

            self.processing_statistics.references_resolved += 1;

            Ok(ProcessingResult {
                task_id: task.task_id,
                result_type: ResultType::ReferenceResolved,
                node_id: Some(node_id),
                data: Some(reference_id),
            })
        } else {
            // Add to pending references
            self.pending_references.entry(obj_id).or_default();

            Ok(ProcessingResult {
                task_id: task.task_id,
                result_type: ResultType::ReferencePending,
                node_id: None,
                data: Some(reference_id),
            })
        }
    }

    fn process_build_node(&mut self, task: ProcessingTask) -> AstResult<ProcessingResult> {
        if task.data.is_empty() {
            return Err(crate::ast::AstError::ParseError(
                "BuildNode requires non-empty data".to_string(),
            ));
        }

        let parser = crate::parser::PdfParser::new();
        let pdf_value = parser.parse_object(&task.data)?;
        let node_type = task.context.expected_type.unwrap_or(NodeType::Unknown);
        let node_id = self.document.ast.create_node(node_type, pdf_value);

        // Link to parent if specified
        if let Some(parent_id) = task.context.parent_node {
            self.document
                .ast
                .add_edge(parent_id, node_id, crate::ast::EdgeType::Child);
        }

        Ok(ProcessingResult {
            task_id: task.task_id,
            result_type: ResultType::NodeCreated,
            node_id: Some(node_id),
            data: None,
        })
    }

    fn process_validate_structure(&mut self, task: ProcessingTask) -> AstResult<ProcessingResult> {
        let validation_count = self.document.ast.get_all_nodes().len();
        let registry = crate::validation::SchemaRegistry::new();
        let schema_name = task
            .context
            .reference_chain
            .first()
            .map(|s| s.as_str())
            .unwrap_or("PDF-2.0");
        let report = registry.validate(&self.document, schema_name);

        Ok(ProcessingResult {
            task_id: task.task_id,
            result_type: if report.as_ref().map(|r| r.is_valid).unwrap_or(false) {
                ResultType::ValidationPassed
            } else {
                ResultType::ValidationFailed
            },
            node_id: None,
            data: Some(format!(
                "Validated {} nodes (schema: {})",
                validation_count, schema_name
            )),
        })
    }

    fn process_index_content(&mut self, task: ProcessingTask) -> AstResult<ProcessingResult> {
        // Index content for faster searching
        let content = String::from_utf8_lossy(&task.data);
        let mut keywords = HashSet::new();
        for token in content
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
        {
            keywords.insert(token.to_lowercase());
        }

        Ok(ProcessingResult {
            task_id: task.task_id,
            result_type: ResultType::ContentIndexed,
            node_id: None,
            data: Some(format!(
                "Indexed {} bytes ({} keywords)",
                task.data.len(),
                keywords.len()
            )),
        })
    }

    fn analyze_chunk(&self, data: &[u8], offset: u64) -> AstResult<Vec<ProcessingTask>> {
        let mut tasks = Vec::new();

        // Simple heuristic to identify PDF objects in the chunk
        if data.starts_with(b"%PDF") {
            // PDF header
            tasks.push(ProcessingTask {
                task_id: format!("header_{}", offset),
                task_type: TaskType::ParseObject,
                priority: Priority::Critical,
                data: data[..8.min(data.len())].to_vec(),
                context: TaskContext {
                    byte_offset: offset,
                    parent_node: None,
                    expected_type: Some(NodeType::Metadata),
                    reference_chain: Vec::new(),
                },
            });
        }

        let mut cursor = 0usize;
        while let Some(pos) = find_next_object(data, cursor) {
            let slice = &data[pos..];
            if let Ok((rest, (_obj_id, _value))) =
                crate::parser::object_parser::parse_indirect_object_with_budget(
                    slice,
                    &self.document.budget,
                )
            {
                let consumed = slice.len().saturating_sub(rest.len());
                let obj_bytes = slice[..consumed].to_vec();
                tasks.push(ProcessingTask {
                    task_id: format!("object_{}", offset + pos as u64),
                    task_type: TaskType::ParseObject,
                    priority: Priority::Normal,
                    data: obj_bytes,
                    context: TaskContext {
                        byte_offset: offset + pos as u64,
                        parent_node: None,
                        expected_type: None,
                        reference_chain: Vec::new(),
                    },
                });
                cursor = pos + consumed;
            } else {
                cursor = pos + 1;
            }
        }

        Ok(tasks)
    }

    fn track_reference(&mut self, source_node: NodeId, reference: PdfReference) {
        let obj_id = reference.id();
        if let Some(target) = self.object_index.get(&obj_id).copied() {
            self.document
                .ast
                .add_edge(source_node, target, crate::ast::EdgeType::Reference);
            self.processing_statistics.references_resolved += 1;
        } else {
            self.pending_references
                .entry(obj_id)
                .or_default()
                .push(source_node);
        }
    }
}

fn parse_reference_id(value: &str) -> Option<ObjectId> {
    let mut parts = value.split_whitespace();
    let num = parts.next()?.parse::<u32>().ok()?;
    let gen = parts.next()?.parse::<u16>().ok()?;
    let r = parts.next()?;
    if r != "R" {
        return None;
    }
    Some(ObjectId::new(num, gen))
}

fn collect_references_from_value(value: &PdfValue, out: &mut Vec<PdfReference>) {
    match value {
        PdfValue::Reference(r) => out.push(*r),
        PdfValue::Array(arr) => {
            for v in arr.iter() {
                collect_references_from_value(v, out);
            }
        }
        PdfValue::Dictionary(dict) => {
            for (_, v) in dict.iter() {
                collect_references_from_value(v, out);
            }
        }
        PdfValue::Stream(stream) => {
            collect_references_from_value(&PdfValue::Dictionary(stream.dict.clone()), out);
        }
        _ => {}
    }
}

fn determine_node_type(value: &PdfValue, obj_id: ObjectId) -> NodeType {
    if let PdfValue::Dictionary(dict) = value {
        if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
            match type_name.as_str() {
                "/Catalog" => return NodeType::Catalog,
                "/Pages" => return NodeType::Pages,
                "/Page" => return NodeType::Page,
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

fn find_next_object(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 4 < data.len() {
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
                    let mut k = j;
                    while k < data.len() && data[k].is_ascii_digit() {
                        k += 1;
                    }
                    if k + 4 <= data.len() && &data[k..k + 4] == b" obj" {
                        return Some(i);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

impl Default for IncrementalProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of processing a task
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub task_id: String,
    pub result_type: ResultType,
    pub node_id: Option<NodeId>,
    pub data: Option<String>,
}

/// Type of processing result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultType {
    NodeCreated,
    ReferenceResolved,
    ReferencePending,
    ParseError,
    ValidationPassed,
    ValidationFailed,
    ContentIndexed,
}

/// Builder for creating processing tasks
pub struct TaskBuilder {
    task_id: String,
    task_type: TaskType,
    priority: Priority,
    data: Vec<u8>,
    context: TaskContext,
}

impl TaskBuilder {
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            task_type: TaskType::ParseObject,
            priority: Priority::Normal,
            data: Vec::new(),
            context: TaskContext {
                byte_offset: 0,
                parent_node: None,
                expected_type: None,
                reference_chain: Vec::new(),
            },
        }
    }

    pub fn task_type(mut self, task_type: TaskType) -> Self {
        self.task_type = task_type;
        self
    }

    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn byte_offset(mut self, offset: u64) -> Self {
        self.context.byte_offset = offset;
        self
    }

    pub fn parent_node(mut self, parent: NodeId) -> Self {
        self.context.parent_node = Some(parent);
        self
    }

    pub fn expected_type(mut self, node_type: NodeType) -> Self {
        self.context.expected_type = Some(node_type);
        self
    }

    pub fn build(self) -> ProcessingTask {
        ProcessingTask {
            task_id: self.task_id,
            task_type: self.task_type,
            priority: self.priority,
            data: self.data,
            context: self.context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_resolves_references() {
        let data = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Count 0 >>\nendobj\n";
        let mut processor = IncrementalProcessor::new();
        processor.add_data_chunk(data.to_vec(), 0).unwrap();
        let _ = processor.process_all_tasks().unwrap();

        let nodes = processor.document.ast.get_all_nodes();
        assert!(nodes.len() >= 2);

        let mut has_ref_edge = false;
        for edge in processor.document.ast.get_all_edges() {
            if edge.edge_type == crate::ast::EdgeType::Reference {
                has_ref_edge = true;
                break;
            }
        }
        assert!(has_ref_edge, "Expected at least one reference edge");
    }
}
