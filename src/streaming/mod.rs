/// Streaming and incremental processing capabilities
///
/// This module provides functionality for processing large PDF documents
/// in a streaming fashion, allowing for memory-efficient handling of
/// large files and real-time processing.
use crate::ast::{AstError, AstNode, AstResult, NodeId, NodeType, PdfDocument};
use crate::parser::PdfParser;
use crate::performance::ResourceBudget;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek};

pub mod chunks;
pub mod incremental;
pub mod pipeline;

pub use chunks::*;
pub use incremental::*;
pub use pipeline::*;

/// Streaming parser for large PDF documents
pub struct StreamingParser<R: Read + Seek + BufRead> {
    reader: R,
    chunk_size: usize,
    buffer_size: usize,
    current_position: u64,
    document: PdfDocument,
    budget: ResourceBudget,
    node_cache: HashMap<NodeId, AstNode>,
    lazy_nodes: HashMap<NodeId, LazyNode>,
}

/// Lazy node that is loaded on demand
#[derive(Debug, Clone)]
pub struct LazyNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub byte_offset: u64,
    pub byte_length: usize,
    pub is_loaded: bool,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

/// Streaming configuration
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    pub chunk_size: usize,
    pub buffer_size: usize,
    pub max_memory_usage: usize,
    pub enable_lazy_loading: bool,
    pub enable_caching: bool,
    pub cache_size_limit: usize,
    pub parallel_processing: bool,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64 * 1024,               // 64KB chunks
            buffer_size: 1024 * 1024,            // 1MB buffer
            max_memory_usage: 512 * 1024 * 1024, // 512MB max memory
            enable_lazy_loading: true,
            enable_caching: true,
            cache_size_limit: 100 * 1024 * 1024, // 100MB cache
            parallel_processing: false,
        }
    }
}

impl<R: Read + Seek + BufRead> StreamingParser<R> {
    /// Create a new streaming parser
    pub fn new(reader: R, config: StreamingConfig) -> Self {
        Self::new_with_budget(reader, config, ResourceBudget::default())
    }

    pub fn new_with_budget(reader: R, config: StreamingConfig, budget: ResourceBudget) -> Self {
        let mut document = PdfDocument::new(crate::ast::PdfVersion { major: 1, minor: 4 });
        document.budget = budget.clone();
        Self {
            reader,
            chunk_size: config.chunk_size.max(1),
            buffer_size: config.buffer_size,
            current_position: 0,
            document,
            budget,
            node_cache: HashMap::new(),
            lazy_nodes: HashMap::new(),
        }
    }

    /// Parse the document incrementally
    pub fn parse_incremental(&mut self) -> AstResult<IncrementalResult> {
        self.budget
            .check()
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        let mut result = IncrementalResult::new();

        // Read header
        let header = self.read_header()?;
        self.budget
            .consume_input(header.len() as u64)
            .map_err(|error| AstError::ParseError(error.to_string()))?;
        result.add_chunk(ProcessedChunk {
            offset: 0,
            size: header.len(),
            chunk_type: ChunkType::Header,
            nodes_found: 0,
            processing_time_ms: 0,
        });

        // Parse in chunks
        while !self.is_eof()? {
            let chunk_start = self.current_position;
            let chunk = self.read_chunk()?;

            let start_time = std::time::Instant::now();
            let nodes = self.parse_chunk(&chunk)?;
            let processing_time = start_time.elapsed().as_millis() as u64;

            result.add_chunk(ProcessedChunk {
                offset: chunk_start,
                size: chunk.len(),
                chunk_type: ChunkType::Content,
                nodes_found: nodes.len(),
                processing_time_ms: processing_time,
            });

            // Add nodes to document
            for node in nodes {
                self.add_node_to_document(node)?;
            }

            // Check memory usage
            if self.get_memory_usage() > self.buffer_size * 10 {
                self.flush_cache()?;
            }
        }

        result.finalize();
        Ok(result)
    }

    /// Load a lazy node on demand
    pub fn load_lazy_node(&mut self, node_id: NodeId) -> AstResult<AstNode> {
        if let Some(cached_node) = self.node_cache.get(&node_id) {
            return Ok(cached_node.clone());
        }

        if let Some(lazy_node) = self.lazy_nodes.get(&node_id) {
            if lazy_node.byte_length as u64 > self.budget.remaining_input_bytes() {
                return Err(AstError::ParseError(
                    "resource budget exceeded: InputBytes".to_string(),
                ));
            }
            let current_pos = self.reader.stream_position()?;

            // Seek to node position
            self.reader
                .seek(std::io::SeekFrom::Start(lazy_node.byte_offset))?;

            // Read node data
            let mut buffer = vec![0u8; lazy_node.byte_length];
            self.budget
                .consume_memory(buffer.len() as u64)
                .map_err(|error| AstError::ParseError(error.to_string()))?;
            self.reader.read_exact(&mut buffer)?;

            // Parse node
            let parser = PdfParser::new().with_resource_budget(self.budget.clone());
            let pdf_value = parser.parse_object(&buffer)?;

            // Convert PdfValue to AstNode
            self.budget
                .consume_node()
                .map_err(|error| AstError::ParseError(error.to_string()))?;
            let ast_node = AstNode::new(node_id, NodeType::Unknown, pdf_value);

            // Cache the node
            self.node_cache.insert(node_id, ast_node.clone());

            // Restore position
            self.reader.seek(std::io::SeekFrom::Start(current_pos))?;

            Ok(ast_node)
        } else {
            Err(AstError::NodeNotFound(format!("node_{}", node_id.index())))
        }
    }

    /// Get nodes by type with lazy loading
    pub fn get_nodes_by_type_lazy(&mut self, node_type: &NodeType) -> AstResult<Vec<AstNode>> {
        let mut nodes = Vec::new();

        // Get already loaded nodes
        for node_id in self.document.ast.get_nodes_by_type(node_type.clone()) {
            if let Some(node) = self.document.ast.get_node(node_id) {
                nodes.push(node.clone());
            }
        }

        // Get lazy nodes of the requested type
        let lazy_node_ids: Vec<NodeId> = self
            .lazy_nodes
            .iter()
            .filter_map(|(node_id, lazy_node)| {
                if lazy_node.node_type == *node_type && !lazy_node.is_loaded {
                    Some(*node_id)
                } else {
                    None
                }
            })
            .collect();

        for node_id in lazy_node_ids {
            let node = self.load_lazy_node(node_id)?;
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Process document in streaming fashion with callback
    pub fn process_streaming<F>(&mut self, mut callback: F) -> AstResult<()>
    where
        F: FnMut(&AstNode, &StreamingContext) -> AstResult<bool>,
    {
        let mut context = StreamingContext::new();

        while !self.is_eof()? {
            let chunk = self.read_chunk()?;
            let nodes = self.parse_chunk(&chunk)?;

            for node in nodes {
                context.update(&node);

                // Call callback - if it returns false, stop processing
                if !callback(&node, &context)? {
                    return Ok(());
                }

                self.add_node_to_document(node)?;
            }
        }

        Ok(())
    }

    // Helper methods
    fn read_header(&mut self) -> AstResult<Vec<u8>> {
        let mut header = vec![0u8; 8];
        self.reader.read_exact(&mut header)?;
        self.current_position += 8;
        Ok(header)
    }

    fn read_chunk(&mut self) -> AstResult<Vec<u8>> {
        let remaining = self.budget.remaining_input_bytes();
        if remaining == 0 {
            return Err(AstError::ParseError(
                "resource budget exceeded: InputBytes".to_string(),
            ));
        }
        let chunk_size = self
            .chunk_size
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let mut buffer = vec![0u8; chunk_size.max(1)];
        let bytes_read = self.reader.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        self.current_position += bytes_read as u64;
        Ok(buffer)
    }

    fn parse_chunk(&self, chunk: &[u8]) -> AstResult<Vec<AstNode>> {
        let parser = PdfParser::new().with_resource_budget(self.budget.clone());
        // This is a simplified implementation
        // In practice, would need more sophisticated chunk parsing
        match parser.parse_objects(chunk) {
            Ok(pdf_values) => {
                let mut nodes = Vec::new();
                for (i, pdf_value) in pdf_values.into_iter().enumerate() {
                    self.budget
                        .consume_node()
                        .map_err(|error| AstError::ParseError(error.to_string()))?;
                    let node_id = NodeId::new(i);
                    let ast_node = AstNode::new(node_id, NodeType::Unknown, pdf_value);
                    nodes.push(ast_node);
                }
                Ok(nodes)
            }
            Err(error) if is_resource_budget_error(&error) => Err(error),
            Err(_) => Ok(Vec::new()), // Skip malformed chunks in streaming mode
        }
    }

    fn add_node_to_document(&mut self, node: AstNode) -> AstResult<()> {
        let node_id = self.document.ast.create_node(node.node_type, node.value);

        // Update lazy node if it exists
        if let Some(lazy_node) = self.lazy_nodes.get_mut(&node_id) {
            lazy_node.is_loaded = true;
        }

        Ok(())
    }

    fn is_eof(&mut self) -> AstResult<bool> {
        // Check if we can read one more byte
        let current_pos = self.reader.stream_position()?;
        let mut temp_buffer = [0u8; 1];
        match self.reader.read(&mut temp_buffer) {
            Ok(0) => Ok(true), // EOF
            Ok(_) => {
                // Reset position
                self.reader.seek(std::io::SeekFrom::Start(current_pos))?;
                Ok(false)
            }
            Err(_) => Ok(true), // Treat errors as EOF for streaming
        }
    }

    fn get_memory_usage(&self) -> usize {
        // Estimate memory usage
        self.node_cache.len() * std::mem::size_of::<AstNode>()
            + self.lazy_nodes.len() * std::mem::size_of::<LazyNode>()
    }

    fn flush_cache(&mut self) -> AstResult<()> {
        // Remove least recently used nodes from cache
        // This is a simplified LRU implementation
        if self.node_cache.len() > 1000 {
            let keys_to_remove: Vec<_> = self.node_cache.keys().take(100).cloned().collect();
            for key in keys_to_remove {
                self.node_cache.remove(&key);
            }
        }
        Ok(())
    }
}

fn is_resource_budget_error(error: &AstError) -> bool {
    matches!(error, AstError::ParseError(message) if message.starts_with("resource budget exceeded:"))
}

/// Context information during streaming processing
#[derive(Debug, Clone)]
pub struct StreamingContext {
    pub total_nodes_processed: usize,
    pub current_chunk: usize,
    pub bytes_processed: u64,
    pub processing_start_time: std::time::Instant,
    pub node_type_counts: HashMap<NodeType, usize>,
}

impl StreamingContext {
    pub fn new() -> Self {
        Self {
            total_nodes_processed: 0,
            current_chunk: 0,
            bytes_processed: 0,
            processing_start_time: std::time::Instant::now(),
            node_type_counts: HashMap::new(),
        }
    }

    pub fn update(&mut self, node: &AstNode) {
        self.total_nodes_processed += 1;
        *self
            .node_type_counts
            .entry(node.node_type.clone())
            .or_insert(0) += 1;
    }

    pub fn elapsed_time(&self) -> std::time::Duration {
        self.processing_start_time.elapsed()
    }

    pub fn processing_rate(&self) -> f64 {
        let elapsed_secs = self.elapsed_time().as_secs_f64();
        if elapsed_secs > 0.0 {
            self.total_nodes_processed as f64 / elapsed_secs
        } else {
            0.0
        }
    }
}

impl Default for StreamingContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of incremental parsing
#[derive(Debug, Clone)]
pub struct IncrementalResult {
    pub chunks_processed: Vec<ProcessedChunk>,
    pub total_bytes: u64,
    pub total_nodes: usize,
    pub processing_time_ms: u64,
    pub memory_peak_mb: f64,
}

impl IncrementalResult {
    pub fn new() -> Self {
        Self {
            chunks_processed: Vec::new(),
            total_bytes: 0,
            total_nodes: 0,
            processing_time_ms: 0,
            memory_peak_mb: 0.0,
        }
    }

    pub fn add_chunk(&mut self, chunk: ProcessedChunk) {
        self.total_bytes += chunk.size as u64;
        self.total_nodes += chunk.nodes_found;
        self.processing_time_ms += chunk.processing_time_ms;
        self.chunks_processed.push(chunk);
    }

    pub fn finalize(&mut self) {
        // Calculate final statistics
        self.memory_peak_mb = self.estimate_memory_usage();
    }

    fn estimate_memory_usage(&self) -> f64 {
        // Rough estimate based on nodes and chunks
        (self.total_nodes * 1024 + self.chunks_processed.len() * 256) as f64 / (1024.0 * 1024.0)
    }
}

impl Default for IncrementalResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a processed chunk
#[derive(Debug, Clone)]
pub struct ProcessedChunk {
    pub offset: u64,
    pub size: usize,
    pub chunk_type: ChunkType,
    pub nodes_found: usize,
    pub processing_time_ms: u64,
}

/// Type of chunk being processed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkType {
    Header,
    Content,
    XrefTable,
    Trailer,
    Stream,
}

/// Create a streaming parser from a file
pub fn create_streaming_parser(
    file_path: &str,
    config: StreamingConfig,
) -> AstResult<StreamingParser<BufReader<std::fs::File>>> {
    let file = std::fs::File::open(file_path)?;
    let reader = BufReader::new(file);
    Ok(StreamingParser::new(reader, config))
}

/// Parse a large PDF file in streaming mode
pub fn parse_large_pdf(file_path: &str) -> AstResult<(PdfDocument, IncrementalResult)> {
    let config = StreamingConfig::default();
    let mut parser = create_streaming_parser(file_path, config)?;
    let result = parser.parse_incremental()?;
    Ok((parser.document, result))
}

#[cfg(test)]
mod tests {
    use super::{StreamingConfig, StreamingParser};
    use crate::performance::ResourceBudget;
    use std::io::{BufReader, Cursor};

    #[test]
    fn streaming_parser_propagates_shared_input_budget() {
        let data = b"%PDF-1.7\n1 0 obj\nnull\nendobj\n";
        let budget = ResourceBudget::new(8, 1024, 1024, 10, 10, 10, 10, 8);
        let mut parser = StreamingParser::new_with_budget(
            BufReader::new(Cursor::new(data)),
            StreamingConfig {
                chunk_size: 0,
                ..StreamingConfig::default()
            },
            budget,
        );

        let error = parser
            .parse_incremental()
            .expect_err("input budget must stop streaming parse");
        assert!(error.to_string().contains("InputBytes"));
    }

    #[test]
    fn lazy_node_buffer_uses_memory_budget() {
        let data = b"null";
        let budget = ResourceBudget::new(16, 3, 3, 10, 10, 10, 10, 8);
        let mut parser = StreamingParser::new_with_budget(
            BufReader::new(Cursor::new(data)),
            StreamingConfig::default(),
            budget,
        );
        parser.lazy_nodes.insert(
            crate::ast::NodeId::new(0),
            super::LazyNode {
                id: crate::ast::NodeId::new(0),
                node_type: crate::ast::NodeType::Unknown,
                byte_offset: 0,
                byte_length: data.len(),
                is_loaded: false,
                parent: None,
                children: Vec::new(),
            },
        );

        let error = parser
            .load_lazy_node(crate::ast::NodeId::new(0))
            .expect_err("lazy node buffer must respect memory budget");
        assert!(error.to_string().contains("DecodedBytes"));
    }
}
