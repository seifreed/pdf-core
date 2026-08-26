use crate::ast::{AstNode, NodeType, PdfAstGraph};
use crate::performance::{increment_pages_processed, increment_parallel_tasks, PerformanceConfig};
use dashmap::DashMap;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::sync::Arc;

/// Parallel processing coordinator for PDF operations
pub struct ParallelProcessor {
    config: PerformanceConfig,
    #[cfg(feature = "parallel")]
    thread_pool: rayon::ThreadPool,
}

impl ParallelProcessor {
    pub fn new(config: PerformanceConfig) -> Result<Self, String> {
        let thread_count = config.worker_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });

        #[cfg(feature = "parallel")]
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .thread_name(|i| format!("pdf-worker-{}", i))
            .build()
            .map_err(|e| format!("Failed to create thread pool: {}", e))?;

        Ok(Self {
            config,
            #[cfg(feature = "parallel")]
            thread_pool,
        })
    }

    /// Process multiple pages in parallel
    pub fn process_pages_parallel<F, R>(&self, pages: Vec<AstNode>, processor: F) -> Vec<R>
    where
        F: Fn(&AstNode) -> R + Send + Sync,
        R: Send,
    {
        if !self.config.enable_parallel || pages.len() < 2 {
            // Process sequentially for small workloads
            return pages.iter().map(&processor).collect();
        }

        increment_parallel_tasks();

        self.thread_pool.install(|| {
            pages
                .par_iter()
                .map(|page| {
                    increment_pages_processed(1);
                    processor(page)
                })
                .collect()
        })
    }

    /// Extract text from all pages in parallel
    pub fn extract_text_parallel(&self, ast: &PdfAstGraph) -> Vec<PageText> {
        let pages = self.collect_page_nodes(ast);

        self.process_pages_parallel(pages, |page| {
            PageText {
                page_number: self.extract_page_number(page).unwrap_or(0),
                text: self.extract_page_text(page),
                word_count: 0,      // Will be calculated
                character_count: 0, // Will be calculated
            }
        })
        .into_iter()
        .enumerate()
        .map(|(idx, mut page_text)| {
            page_text.page_number = idx + 1;
            page_text.word_count = page_text.text.split_whitespace().count();
            page_text.character_count = page_text.text.len();
            page_text
        })
        .collect()
    }

    /// Analyze document structure in parallel
    pub fn analyze_structure_parallel(&self, ast: &PdfAstGraph) -> StructureAnalysis {
        let all_nodes = ast.get_all_nodes();

        // Group nodes by type for parallel processing
        let nodes_by_type: DashMap<NodeType, Vec<AstNode>> = DashMap::new();

        for node in &all_nodes {
            nodes_by_type
                .entry(node.node_type.clone())
                .or_default()
                .push((*node).clone());
        }

        // Process each node type in parallel
        let results: Vec<_> = nodes_by_type
            .into_iter()
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(node_type, nodes)| {
                increment_parallel_tasks();
                let analysis = match node_type {
                    NodeType::Page => self.analyze_pages(&nodes),
                    NodeType::Font => self.analyze_fonts(&nodes),
                    NodeType::Image => self.analyze_images(&nodes),
                    _ => TypeAnalysis::default(),
                };
                (node_type, analysis)
            })
            .collect();

        // Combine results
        let mut structure_analysis = StructureAnalysis::default();
        for (node_type, analysis) in results {
            match node_type {
                NodeType::Page => structure_analysis.pages = analysis,
                NodeType::Font => structure_analysis.fonts = analysis,
                NodeType::Image => structure_analysis.images = analysis,
                _ => {}
            }
        }

        structure_analysis
    }

    /// Process filters in parallel for multiple streams
    pub fn process_filters_parallel<F, R>(&self, streams: Vec<StreamData>, processor: F) -> Vec<R>
    where
        F: Fn(&StreamData) -> R + Send + Sync,
        R: Send,
        StreamData: Send + Sync,
    {
        if !self.config.enable_parallel || streams.len() < 2 {
            return streams.iter().map(&processor).collect();
        }

        increment_parallel_tasks();

        self.thread_pool
            .install(|| streams.par_iter().map(&processor).collect())
    }

    /// Parallel validation of PDF objects
    pub fn validate_objects_parallel(&self, objects: Vec<(u32, AstNode)>) -> Vec<ValidationResult> {
        if !self.config.enable_parallel || objects.len() < 10 {
            return objects
                .iter()
                .map(|(id, node)| self.validate_object(*id, node))
                .collect();
        }

        increment_parallel_tasks();

        self.thread_pool.install(|| {
            objects
                .par_iter()
                .map(|(id, node)| self.validate_object(*id, node))
                .collect()
        })
    }

    // Helper methods
    fn collect_page_nodes(&self, ast: &PdfAstGraph) -> Vec<AstNode> {
        ast.get_all_nodes()
            .into_iter()
            .filter_map(|node| {
                if matches!(node.node_type, NodeType::Page) {
                    Some((*node).clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn extract_page_number(&self, _page: &AstNode) -> Option<usize> {
        // Extract page number from page node
        // This would need actual implementation based on PDF structure
        None
    }

    fn extract_page_text(&self, _page: &AstNode) -> String {
        String::new()
    }

    fn analyze_pages(&self, nodes: &[AstNode]) -> TypeAnalysis {
        TypeAnalysis {
            count: nodes.len(),
            total_size: nodes.iter().map(|n| self.estimate_node_size(n)).sum(),
            properties: vec![
                format!("Page count: {}", nodes.len()),
                format!(
                    "Average page size: {} bytes",
                    if nodes.is_empty() {
                        0
                    } else {
                        nodes
                            .iter()
                            .map(|n| self.estimate_node_size(n))
                            .sum::<usize>()
                            / nodes.len()
                    }
                ),
            ],
        }
    }

    fn analyze_fonts(&self, nodes: &[AstNode]) -> TypeAnalysis {
        let embedded_fonts = nodes
            .iter()
            .filter(|node| self.is_font_embedded(node))
            .count();

        TypeAnalysis {
            count: nodes.len(),
            total_size: nodes.iter().map(|n| self.estimate_node_size(n)).sum(),
            properties: vec![
                format!("Total fonts: {}", nodes.len()),
                format!("Embedded fonts: {}", embedded_fonts),
                format!("System fonts: {}", nodes.len() - embedded_fonts),
            ],
        }
    }

    fn analyze_images(&self, nodes: &[AstNode]) -> TypeAnalysis {
        TypeAnalysis {
            count: nodes.len(),
            total_size: nodes.iter().map(|n| self.estimate_node_size(n)).sum(),
            properties: vec![
                format!("Total images: {}", nodes.len()),
                format!(
                    "Average image size: {} bytes",
                    if nodes.is_empty() {
                        0
                    } else {
                        nodes
                            .iter()
                            .map(|n| self.estimate_node_size(n))
                            .sum::<usize>()
                            / nodes.len()
                    }
                ),
            ],
        }
    }

    fn estimate_node_size(&self, _node: &AstNode) -> usize {
        std::mem::size_of::<AstNode>()
    }

    fn is_font_embedded(&self, _node: &AstNode) -> bool {
        // Check if font is embedded - simplified
        false
    }

    fn validate_object(&self, object_id: u32, _node: &AstNode) -> ValidationResult {
        ValidationResult {
            object_id,
            is_valid: false,
            errors: Vec::new(),
            warnings: vec!["Parallel object validation is not implemented".to_string()],
        }
    }
}

// Parallel-safe data structures
pub type StreamData = Vec<u8>;

#[derive(Debug, Clone)]
pub struct PageText {
    pub page_number: usize,
    pub text: String,
    pub word_count: usize,
    pub character_count: usize,
}

#[derive(Debug, Default)]
pub struct StructureAnalysis {
    pub pages: TypeAnalysis,
    pub fonts: TypeAnalysis,
    pub images: TypeAnalysis,
}

#[derive(Debug, Default)]
pub struct TypeAnalysis {
    pub count: usize,
    pub total_size: usize,
    pub properties: Vec<String>,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub object_id: u32,
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Parallel batch processor for large operations
pub struct BatchProcessor {
    processor: Arc<ParallelProcessor>,
    batch_size: usize,
}

impl BatchProcessor {
    pub fn new(processor: ParallelProcessor, batch_size: usize) -> Self {
        Self {
            processor: Arc::new(processor),
            batch_size,
        }
    }

    /// Process items in batches with parallel execution within each batch
    pub fn process_in_batches<T, F, R>(&self, items: Vec<T>, processor: F) -> Vec<R>
    where
        T: Send + Sync,
        F: Fn(&T) -> R + Send + Sync,
        R: Send,
    {
        items
            .chunks(self.batch_size)
            .flat_map(|batch| {
                if batch.len() > 1 && self.processor.config.enable_parallel {
                    batch.par_iter().map(&processor).collect::<Vec<_>>()
                } else {
                    batch.iter().map(&processor).collect::<Vec<_>>()
                }
            })
            .collect()
    }
}

/// Parallel work scheduler
pub struct WorkScheduler {
    processor: Arc<ParallelProcessor>,
}

impl WorkScheduler {
    pub fn new(processor: ParallelProcessor) -> Self {
        Self {
            processor: Arc::new(processor),
        }
    }

    /// Schedule work items with different priorities
    pub fn schedule_work<T, F, R>(&self, work_items: Vec<(WorkPriority, T)>, processor: F) -> Vec<R>
    where
        T: Send + Sync,
        F: Fn(&T) -> R + Send + Sync + Clone,
        R: Send,
    {
        // Sort by priority
        let mut sorted_work = work_items;
        sorted_work.sort_by_key(|(priority, _)| *priority as u8);

        // Process high priority items first, then parallel process others
        let (high_priority, normal_priority): (Vec<_>, Vec<_>) = sorted_work
            .into_iter()
            .partition(|(priority, _)| matches!(priority, WorkPriority::High));

        let mut results = Vec::new();

        // Process high priority items sequentially first
        for (_, item) in high_priority {
            results.push(processor(&item));
        }

        // Process normal priority items in parallel
        if !normal_priority.is_empty() && self.processor.config.enable_parallel {
            let parallel_results: Vec<R> = normal_priority
                .par_iter()
                .map(|(_, item)| processor(item))
                .collect();
            results.extend(parallel_results);
        } else {
            for (_, item) in normal_priority {
                results.push(processor(&item));
            }
        }

        results
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_processor_creation() {
        let config = PerformanceConfig::default();
        let processor = ParallelProcessor::new(config);
        assert!(processor.is_ok());
    }

    #[test]
    fn test_batch_processor() {
        let config = PerformanceConfig {
            enable_parallel: false,
            ..Default::default()
        };
        let processor = ParallelProcessor::new(config).unwrap();
        let batch_processor = BatchProcessor::new(processor, 5);

        let items: Vec<i32> = (1..=20).collect();
        let results = batch_processor.process_in_batches(items, |x| x * 2);

        let expected: Vec<i32> = (1..=20).map(|x| x * 2).collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn test_work_scheduler() {
        let config = PerformanceConfig {
            enable_parallel: false,
            ..Default::default()
        };
        let processor = ParallelProcessor::new(config).unwrap();
        let scheduler = WorkScheduler::new(processor);

        let work_items = vec![
            (WorkPriority::Normal, 1),
            (WorkPriority::High, 2),
            (WorkPriority::Low, 3),
        ];

        let results = scheduler.schedule_work(work_items, |x| x * 10);
        // High priority should be processed first
        assert_eq!(results[0], 20); // High priority item (2 * 10)
    }
}
