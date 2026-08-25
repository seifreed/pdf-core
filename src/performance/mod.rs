pub mod enhanced_lazy;
pub mod lazy_loading;
pub mod limits;
pub mod memory;
pub mod parallel;
pub mod progress;
pub mod streaming;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Re-export limits module
pub use limits::{
    CancellationToken, ConcurrencyGuard, ParserSlot, PerformanceGuard, PerformanceLimits,
    PerformanceViolation, RecursionGuard, ResourceBudget, ResourceBudgetError,
};

/// Configuration for performance optimizations
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Maximum memory usage before triggering streaming mode
    pub max_memory_mb: usize,

    /// Enable parallel processing
    pub enable_parallel: bool,

    /// Number of parallel workers (None = auto-detect)
    pub worker_threads: Option<usize>,

    /// Enable lazy loading of streams
    pub enable_lazy_loading: bool,

    /// Chunk size for streaming operations
    pub stream_chunk_size: usize,

    /// Progress callback interval
    pub progress_interval: Duration,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 512, // 512MB default limit
            enable_parallel: true,
            worker_threads: None, // Auto-detect
            enable_lazy_loading: true,
            stream_chunk_size: 8192, // 8KB chunks
            progress_interval: Duration::from_millis(100),
        }
    }
}

/// Global performance statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub bytes_processed: u64,
    pub objects_parsed: u64,
    pub pages_processed: u64,
    pub memory_peak_mb: usize,
    pub memory_current_mb: usize,
    pub parallel_tasks_spawned: u64,
    pub lazy_loads_performed: u64,
    pub parse_time_ms: u64,
    pub filter_time_ms: u64,
    pub validation_time_ms: u64,
    pub serialization_time_ms: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub io_operations: u64,
    pub compression_ratio: f64,
    pub operation_times: HashMap<String, Vec<u64>>,
}

use std::sync::OnceLock;

static PERF_STATS_INSTANCE: OnceLock<RwLock<PerformanceStats>> = OnceLock::new();

fn get_perf_stats() -> &'static RwLock<PerformanceStats> {
    PERF_STATS_INSTANCE.get_or_init(|| RwLock::new(PerformanceStats::default()))
}

pub fn get_performance_stats() -> PerformanceStats {
    get_perf_stats().read().clone()
}

pub fn increment_bytes_processed(bytes: u64) {
    get_perf_stats().write().bytes_processed += bytes;
}

pub fn increment_objects_parsed(count: u64) {
    get_perf_stats().write().objects_parsed += count;
}

pub fn increment_pages_processed(count: u64) {
    get_perf_stats().write().pages_processed += count;
}

pub fn update_memory_peak(mb: usize) {
    let mut stats = get_perf_stats().write();
    if mb > stats.memory_peak_mb {
        stats.memory_peak_mb = mb;
    }
}

pub fn increment_parallel_tasks() {
    get_perf_stats().write().parallel_tasks_spawned += 1;
}

pub fn increment_lazy_loads() {
    get_perf_stats().write().lazy_loads_performed += 1;
}

pub fn reset_performance_stats() {
    *get_perf_stats().write() = PerformanceStats::default();
}

pub fn update_current_memory(mb: usize) {
    get_perf_stats().write().memory_current_mb = mb;
    update_memory_peak(mb);
}

pub fn add_parse_time(ms: u64) {
    get_perf_stats().write().parse_time_ms += ms;
}

pub fn add_filter_time(ms: u64) {
    get_perf_stats().write().filter_time_ms += ms;
}

pub fn add_validation_time(ms: u64) {
    get_perf_stats().write().validation_time_ms += ms;
}

pub fn add_serialization_time(ms: u64) {
    get_perf_stats().write().serialization_time_ms += ms;
}

pub fn increment_cache_hits() {
    get_perf_stats().write().cache_hits += 1;
}

pub fn increment_cache_misses() {
    get_perf_stats().write().cache_misses += 1;
}

pub fn increment_io_operations() {
    get_perf_stats().write().io_operations += 1;
}

pub fn update_compression_ratio(ratio: f64) {
    let mut stats = get_perf_stats().write();
    stats.compression_ratio = (stats.compression_ratio + ratio) / 2.0;
}

pub fn record_operation_time(operation: &str, ms: u64) {
    let mut stats = get_perf_stats().write();
    stats
        .operation_times
        .entry(operation.to_string())
        .or_default()
        .push(ms);
}

#[derive(Debug)]
pub struct PerformanceTimer {
    operation: String,
    start: Instant,
}

impl PerformanceTimer {
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            start: Instant::now(),
        }
    }

    pub fn finish(self) -> u64 {
        let elapsed = self.start.elapsed().as_millis() as u64;
        record_operation_time(&self.operation, elapsed);
        elapsed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub stats: PerformanceStats,
    pub analysis: PerformanceAnalysis,
    pub recommendations: Vec<String>,
    pub bottlenecks: Vec<Bottleneck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub throughput_mb_per_sec: f64,
    pub objects_per_sec: f64,
    pub memory_efficiency: f64,
    pub cache_hit_ratio: f64,
    pub parallel_efficiency: f64,
    pub operation_breakdown: HashMap<String, OperationStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStats {
    pub total_time_ms: u64,
    pub avg_time_ms: f64,
    pub min_time_ms: u64,
    pub max_time_ms: u64,
    pub count: usize,
    pub percentage_of_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    pub category: String,
    pub description: String,
    pub severity: BottleneckSeverity,
    pub impact_score: f64,
    pub suggested_fix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BottleneckSeverity {
    Low,
    Medium,
    High,
    Critical,
}

pub struct PerformanceAnalyzer;

impl PerformanceAnalyzer {
    pub fn generate_report() -> PerformanceReport {
        let stats = get_performance_stats();
        let analysis = Self::analyze_performance(&stats);
        let recommendations = Self::generate_recommendations(&stats, &analysis);
        let bottlenecks = Self::identify_bottlenecks(&stats, &analysis);

        PerformanceReport {
            stats,
            analysis,
            recommendations,
            bottlenecks,
        }
    }

    fn analyze_performance(stats: &PerformanceStats) -> PerformanceAnalysis {
        let total_time = stats.parse_time_ms
            + stats.filter_time_ms
            + stats.validation_time_ms
            + stats.serialization_time_ms;

        let throughput = if total_time > 0 {
            (stats.bytes_processed as f64 / 1_048_576.0) / (total_time as f64 / 1000.0)
        } else {
            0.0
        };

        let objects_per_sec = if total_time > 0 {
            stats.objects_parsed as f64 / (total_time as f64 / 1000.0)
        } else {
            0.0
        };

        let memory_efficiency = if stats.bytes_processed > 0 {
            (stats.bytes_processed as f64 / 1_048_576.0) / stats.memory_peak_mb as f64
        } else {
            0.0
        };

        let cache_hit_ratio = if stats.cache_hits + stats.cache_misses > 0 {
            stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64
        } else {
            0.0
        };

        let parallel_efficiency = if stats.parallel_tasks_spawned > 0 {
            stats.objects_parsed as f64 / stats.parallel_tasks_spawned as f64
        } else {
            0.0
        };

        let mut operation_breakdown = HashMap::new();

        for (operation, times) in &stats.operation_times {
            if !times.is_empty() {
                let total: u64 = times.iter().sum();
                let avg = total as f64 / times.len() as f64;
                let min = *times.iter().min().unwrap_or(&0);
                let max = *times.iter().max().unwrap_or(&0);
                let percentage = if total_time > 0 {
                    total as f64 / total_time as f64 * 100.0
                } else {
                    0.0
                };

                operation_breakdown.insert(
                    operation.clone(),
                    OperationStats {
                        total_time_ms: total,
                        avg_time_ms: avg,
                        min_time_ms: min,
                        max_time_ms: max,
                        count: times.len(),
                        percentage_of_total: percentage,
                    },
                );
            }
        }

        PerformanceAnalysis {
            throughput_mb_per_sec: throughput,
            objects_per_sec,
            memory_efficiency,
            cache_hit_ratio,
            parallel_efficiency,
            operation_breakdown,
        }
    }

    fn generate_recommendations(
        stats: &PerformanceStats,
        analysis: &PerformanceAnalysis,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        if analysis.throughput_mb_per_sec < 10.0 {
            recommendations
                .push("Consider enabling parallel processing to improve throughput".to_string());
        }

        if analysis.cache_hit_ratio < 0.5 {
            recommendations
                .push("Low cache hit ratio detected. Consider increasing cache size".to_string());
        }

        if stats.memory_peak_mb > 1024 {
            recommendations
                .push("High memory usage detected. Consider enabling streaming mode".to_string());
        }

        if analysis.parallel_efficiency < 5.0 && stats.parallel_tasks_spawned > 0 {
            recommendations.push(
                "Parallel processing efficiency is low. Consider reducing thread count".to_string(),
            );
        }

        if stats.lazy_loads_performed == 0 && stats.bytes_processed > 50_000_000 {
            recommendations
                .push("Enable lazy loading for large documents to reduce memory usage".to_string());
        }

        recommendations
    }

    fn identify_bottlenecks(
        stats: &PerformanceStats,
        analysis: &PerformanceAnalysis,
    ) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        if analysis.memory_efficiency < 1.0 {
            bottlenecks.push(Bottleneck {
                category: "Memory".to_string(),
                description: "Poor memory efficiency detected".to_string(),
                severity: BottleneckSeverity::High,
                impact_score: 1.0 - analysis.memory_efficiency,
                suggested_fix: "Enable streaming mode or increase chunk sizes".to_string(),
            });
        }

        if analysis.cache_hit_ratio < 0.3 {
            bottlenecks.push(Bottleneck {
                category: "Caching".to_string(),
                description: "Very low cache hit ratio".to_string(),
                severity: BottleneckSeverity::Medium,
                impact_score: 0.3 - analysis.cache_hit_ratio,
                suggested_fix: "Increase cache size or implement better caching strategy"
                    .to_string(),
            });
        }

        for (operation, op_stats) in &analysis.operation_breakdown {
            if op_stats.percentage_of_total > 50.0 {
                bottlenecks.push(Bottleneck {
                    category: "Operation".to_string(),
                    description: format!(
                        "{} operation consuming {}% of total time",
                        operation, op_stats.percentage_of_total
                    ),
                    severity: BottleneckSeverity::High,
                    impact_score: op_stats.percentage_of_total / 100.0,
                    suggested_fix: format!("Optimize {} operation or parallelize it", operation),
                });
            }
        }

        if stats.io_operations > stats.objects_parsed * 2 {
            bottlenecks.push(Bottleneck {
                category: "I/O".to_string(),
                description: "Excessive I/O operations detected".to_string(),
                severity: BottleneckSeverity::Medium,
                impact_score: 0.5,
                suggested_fix: "Implement better buffering or batch I/O operations".to_string(),
            });
        }

        bottlenecks
    }
}

pub fn start_timer(operation: &str) -> PerformanceTimer {
    PerformanceTimer::new(operation)
}

pub fn get_memory_usage() -> usize {
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(contents) = fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
        {
            if let Ok(rss_str) = String::from_utf8(output.stdout) {
                if let Ok(kb) = rss_str.trim().parse::<usize>() {
                    return kb / 1024;
                }
            }
        }
    }

    0
}

pub fn monitor_memory() {
    let current_mb = get_memory_usage();
    update_current_memory(current_mb);
}
