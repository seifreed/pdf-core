use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ResourceBudget {
    pub max_input_bytes: u64,
    pub max_decoded_bytes_total: u64,
    pub max_decoded_bytes_per_stream: u64,
    pub max_decode_ratio: u64,
    pub max_objects: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_depth: usize,
    pub deadline: Option<Instant>,
    pub cancellation: CancellationToken,
    input_bytes: Arc<AtomicU64>,
    decoded_bytes_total: Arc<AtomicU64>,
    objects: Arc<AtomicU64>,
    nodes: Arc<AtomicU64>,
    edges: Arc<AtomicU64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceBudgetError {
    InputBytes,
    DecodedBytes,
    Objects,
    Nodes,
    Edges,
    Deadline,
    Cancelled,
}

impl std::fmt::Display for ResourceBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "resource budget exceeded: {:?}", self)
    }
}

impl std::error::Error for ResourceBudgetError {}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self::new(
            100 * 1024 * 1024,
            1024 * 1024 * 1024,
            50 * 1024 * 1024,
            100,
            1_000_000,
            1_000_000,
            5_000_000,
            1000,
        )
    }
}

impl ResourceBudget {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_input_bytes: u64,
        max_decoded_bytes_total: u64,
        max_decoded_bytes_per_stream: u64,
        max_decode_ratio: u64,
        max_objects: usize,
        max_nodes: usize,
        max_edges: usize,
        max_depth: usize,
    ) -> Self {
        Self {
            max_input_bytes,
            max_decoded_bytes_total,
            max_decoded_bytes_per_stream,
            max_decode_ratio,
            max_objects,
            max_nodes,
            max_edges,
            max_depth,
            deadline: None,
            cancellation: CancellationToken::default(),
            input_bytes: Arc::new(AtomicU64::new(0)),
            decoded_bytes_total: Arc::new(AtomicU64::new(0)),
            objects: Arc::new(AtomicU64::new(0)),
            nodes: Arc::new(AtomicU64::new(0)),
            edges: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn check(&self) -> Result<(), ResourceBudgetError> {
        if self.cancellation.is_cancelled() {
            return Err(ResourceBudgetError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ResourceBudgetError::Deadline);
        }
        Ok(())
    }

    pub fn consume_input(&self, bytes: u64) -> Result<(), ResourceBudgetError> {
        self.consume_bytes(
            &self.input_bytes,
            self.max_input_bytes,
            bytes,
            ResourceBudgetError::InputBytes,
        )
    }

    pub fn consume_decoded(&self, bytes: u64) -> Result<(), ResourceBudgetError> {
        self.check()?;
        if bytes > self.max_decoded_bytes_per_stream {
            return Err(ResourceBudgetError::DecodedBytes);
        }
        self.consume_bytes(
            &self.decoded_bytes_total,
            self.max_decoded_bytes_total,
            bytes,
            ResourceBudgetError::DecodedBytes,
        )
    }

    pub fn remaining_decoded_bytes(&self) -> u64 {
        self.max_decoded_bytes_total
            .saturating_sub(self.decoded_bytes_total.load(Ordering::Acquire))
    }

    pub fn consume_object(&self) -> Result<(), ResourceBudgetError> {
        self.consume_counter(
            &self.objects,
            self.max_objects,
            ResourceBudgetError::Objects,
        )
    }

    pub fn consume_node(&self) -> Result<(), ResourceBudgetError> {
        self.consume_counter(&self.nodes, self.max_nodes, ResourceBudgetError::Nodes)
    }

    pub fn consume_edge(&self) -> Result<(), ResourceBudgetError> {
        self.consume_counter(&self.edges, self.max_edges, ResourceBudgetError::Edges)
    }

    fn consume_counter(
        &self,
        counter: &AtomicU64,
        limit: usize,
        error: ResourceBudgetError,
    ) -> Result<(), ResourceBudgetError> {
        self.check()?;
        let limit = limit as u64;
        loop {
            let current = counter.load(Ordering::Acquire);
            if current >= limit {
                return Err(error);
            }
            if counter
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    fn consume_bytes(
        &self,
        counter: &AtomicU64,
        limit: u64,
        bytes: u64,
        error: ResourceBudgetError,
    ) -> Result<(), ResourceBudgetError> {
        self.check()?;
        loop {
            let current = counter.load(Ordering::Acquire);
            let next = current.checked_add(bytes).ok_or_else(|| error.clone())?;
            if next > limit {
                return Err(error);
            }
            if counter
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_memory_mb: usize,
    pub max_parse_time: Duration,
    pub max_query_time: Duration,
    pub max_depth: usize,
    pub max_file_size_mb: usize,
    pub max_object_size_mb: usize,
    pub max_stream_decode_ratio: usize,
    pub max_concurrent_parsers: usize,
    pub enable_timeout_checks: bool,
    pub enable_memory_checks: bool,
    pub enable_recursion_checks: bool,
    pub budget: ResourceBudget,
}

impl Default for PerformanceLimits {
    fn default() -> Self {
        let mut limits = Self {
            max_nodes: 1_000_000,
            max_edges: 5_000_000,
            max_memory_mb: 1024,
            max_parse_time: Duration::from_secs(300), // 5 minutes
            max_query_time: Duration::from_secs(30),
            max_depth: 1000,
            max_file_size_mb: 100,
            max_object_size_mb: 50,
            max_stream_decode_ratio: 100,
            max_concurrent_parsers: 4,
            enable_timeout_checks: true,
            enable_memory_checks: true,
            enable_recursion_checks: true,
            budget: ResourceBudget::default(),
        };
        limits.refresh_budget();
        limits
    }
}

impl PerformanceLimits {
    pub fn conservative() -> Self {
        let mut limits = Self {
            max_nodes: 100_000,
            max_edges: 500_000,
            max_memory_mb: 256,
            max_parse_time: Duration::from_secs(60),
            max_query_time: Duration::from_secs(10),
            max_depth: 100,
            max_file_size_mb: 10,
            max_object_size_mb: 5,
            max_stream_decode_ratio: 50,
            max_concurrent_parsers: 2,
            ..Default::default()
        };
        limits.refresh_budget();
        limits
    }

    pub fn permissive() -> Self {
        let mut limits = Self {
            max_nodes: 10_000_000,
            max_edges: 50_000_000,
            max_memory_mb: 4096,
            max_parse_time: Duration::from_secs(1800), // 30 minutes
            max_query_time: Duration::from_secs(120),
            max_depth: 10000,
            max_file_size_mb: 1000,
            max_object_size_mb: 500,
            max_stream_decode_ratio: 200,
            max_concurrent_parsers: 8,
            ..Default::default()
        };
        limits.refresh_budget();
        limits
    }

    pub fn refresh_budget(&mut self) {
        let deadline = self.budget.deadline;
        let cancellation = self.budget.cancellation.clone();
        self.budget = ResourceBudget::new(
            (self.max_file_size_mb as u64).saturating_mul(1024 * 1024),
            (self.max_memory_mb as u64).saturating_mul(1024 * 1024),
            (self.max_object_size_mb as u64).saturating_mul(1024 * 1024),
            self.max_stream_decode_ratio as u64,
            self.max_nodes,
            self.max_nodes,
            self.max_edges,
            self.max_depth,
        );
        self.budget.deadline = deadline;
        self.budget.cancellation = cancellation;
    }
}

#[derive(Debug)]
pub struct PerformanceGuard {
    limits: PerformanceLimits,
    start_time: Instant,
    node_count: usize,
    edge_count: usize,
    current_depth: usize,
    max_depth_reached: usize,
    memory_usage: Arc<Mutex<usize>>,
    operation_name: String,
}

#[derive(Debug, Clone)]
pub enum PerformanceViolation {
    TooManyNodes(usize, usize),
    TooManyEdges(usize, usize),
    ExcessiveMemory(usize, usize),
    Timeout(Duration, Duration),
    ExcessiveDepth(usize, usize),
    FileTooLarge(usize, usize),
    ObjectTooLarge(usize, usize),
    TooManyConcurrentParsers(usize, usize),
}

impl std::fmt::Display for PerformanceViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerformanceViolation::TooManyNodes(current, max) => {
                write!(f, "Too many nodes: {} > {}", current, max)
            }
            PerformanceViolation::TooManyEdges(current, max) => {
                write!(f, "Too many edges: {} > {}", current, max)
            }
            PerformanceViolation::ExcessiveMemory(current, max) => {
                write!(f, "Excessive memory usage: {}MB > {}MB", current, max)
            }
            PerformanceViolation::Timeout(current, max) => {
                write!(f, "Operation timeout: {:?} > {:?}", current, max)
            }
            PerformanceViolation::ExcessiveDepth(current, max) => {
                write!(f, "Excessive recursion depth: {} > {}", current, max)
            }
            PerformanceViolation::FileTooLarge(current, max) => {
                write!(f, "File too large: {}MB > {}MB", current, max)
            }
            PerformanceViolation::ObjectTooLarge(current, max) => {
                write!(f, "Object too large: {}MB > {}MB", current, max)
            }
            PerformanceViolation::TooManyConcurrentParsers(current, max) => {
                write!(f, "Too many concurrent parsers: {} > {}", current, max)
            }
        }
    }
}

impl std::error::Error for PerformanceViolation {}

impl PerformanceGuard {
    pub fn new(limits: PerformanceLimits, operation_name: &str) -> Self {
        Self {
            limits,
            start_time: Instant::now(),
            node_count: 0,
            edge_count: 0,
            current_depth: 0,
            max_depth_reached: 0,
            memory_usage: Arc::new(Mutex::new(0)),
            operation_name: operation_name.to_string(),
        }
    }

    pub fn check_file_size(&self, size_bytes: usize) -> Result<(), PerformanceViolation> {
        let size_mb = size_bytes.div_ceil(1024 * 1024);
        if size_bytes > self.limits.max_file_size_mb.saturating_mul(1024 * 1024) {
            return Err(PerformanceViolation::FileTooLarge(
                size_mb,
                self.limits.max_file_size_mb,
            ));
        }
        Ok(())
    }

    pub fn check_object_size(&self, size_bytes: usize) -> Result<(), PerformanceViolation> {
        let size_mb = size_bytes.div_ceil(1024 * 1024);
        if size_bytes > self.limits.max_object_size_mb.saturating_mul(1024 * 1024) {
            return Err(PerformanceViolation::ObjectTooLarge(
                size_mb,
                self.limits.max_object_size_mb,
            ));
        }
        Ok(())
    }

    pub fn check_nodes(&self, count: usize) -> Result<(), PerformanceViolation> {
        if count > self.limits.max_nodes {
            return Err(PerformanceViolation::TooManyNodes(
                count,
                self.limits.max_nodes,
            ));
        }
        Ok(())
    }

    pub fn check_edges(&self, count: usize) -> Result<(), PerformanceViolation> {
        if count > self.limits.max_edges {
            return Err(PerformanceViolation::TooManyEdges(
                count,
                self.limits.max_edges,
            ));
        }
        Ok(())
    }

    pub fn check_timeout(&self, max_duration: Duration) -> Result<(), PerformanceViolation> {
        if !self.limits.enable_timeout_checks {
            return Ok(());
        }

        let elapsed = self.start_time.elapsed();
        if elapsed > max_duration {
            return Err(PerformanceViolation::Timeout(elapsed, max_duration));
        }
        Ok(())
    }

    pub fn check_parse_timeout(&self) -> Result<(), PerformanceViolation> {
        self.check_timeout(self.limits.max_parse_time)
    }

    pub fn check_query_timeout(&self) -> Result<(), PerformanceViolation> {
        self.check_timeout(self.limits.max_query_time)
    }

    pub fn enter_recursion(&mut self) -> Result<RecursionGuard<'_>, PerformanceViolation> {
        if !self.limits.enable_recursion_checks {
            return Ok(RecursionGuard::new(self, 0));
        }

        self.current_depth += 1;
        if self.current_depth > self.max_depth_reached {
            self.max_depth_reached = self.current_depth;
        }

        if self.current_depth > self.limits.max_depth {
            return Err(PerformanceViolation::ExcessiveDepth(
                self.current_depth,
                self.limits.max_depth,
            ));
        }

        Ok(RecursionGuard::new(self, self.current_depth))
    }

    pub fn track_memory_allocation(&self, bytes: usize) -> Result<(), PerformanceViolation> {
        if !self.limits.enable_memory_checks {
            return Ok(());
        }

        if let Ok(mut usage) = self.memory_usage.lock() {
            let next_usage = usage.saturating_add(bytes);
            if next_usage > self.limits.max_memory_mb.saturating_mul(1024 * 1024) {
                return Err(PerformanceViolation::ExcessiveMemory(
                    next_usage.div_ceil(1024 * 1024),
                    self.limits.max_memory_mb,
                ));
            }
            *usage = next_usage;
        }
        Ok(())
    }

    pub fn track_memory_deallocation(&self, bytes: usize) {
        if let Ok(mut usage) = self.memory_usage.lock() {
            *usage = usage.saturating_sub(bytes);
        }
    }

    pub fn increment_nodes(&mut self) -> Result<(), PerformanceViolation> {
        self.node_count += 1;
        self.check_nodes(self.node_count)
    }

    pub fn increment_edges(&mut self) -> Result<(), PerformanceViolation> {
        self.edge_count += 1;
        self.check_edges(self.edge_count)
    }

    pub fn get_stats(&self) -> PerformanceStats {
        PerformanceStats {
            operation_name: self.operation_name.clone(),
            elapsed_time: self.start_time.elapsed(),
            node_count: self.node_count,
            edge_count: self.edge_count,
            max_depth_reached: self.max_depth_reached,
            memory_usage_mb: self.memory_usage.lock().map(|guard| *guard).unwrap_or(0)
                / (1024 * 1024),
        }
    }

    fn exit_recursion(&mut self) {
        if self.current_depth > 0 {
            self.current_depth -= 1;
        }
    }
}

pub struct RecursionGuard<'a> {
    guard: &'a mut PerformanceGuard,
    depth: usize,
}

impl<'a> RecursionGuard<'a> {
    fn new(guard: &'a mut PerformanceGuard, depth: usize) -> Self {
        Self { guard, depth }
    }

    pub fn depth(&self) -> usize {
        self.depth
    }
}

impl<'a> Drop for RecursionGuard<'a> {
    fn drop(&mut self) {
        self.guard.exit_recursion();
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub operation_name: String,
    pub elapsed_time: Duration,
    pub node_count: usize,
    pub edge_count: usize,
    pub max_depth_reached: usize,
    pub memory_usage_mb: usize,
}

impl PerformanceStats {
    pub fn nodes_per_second(&self) -> f64 {
        if self.elapsed_time.as_secs() == 0 {
            return self.node_count as f64;
        }
        self.node_count as f64 / self.elapsed_time.as_secs_f64()
    }

    pub fn edges_per_second(&self) -> f64 {
        if self.elapsed_time.as_secs() == 0 {
            return self.edge_count as f64;
        }
        self.edge_count as f64 / self.elapsed_time.as_secs_f64()
    }
}

pub struct ConcurrencyGuard {
    limits: PerformanceLimits,
    active_parsers: Arc<Mutex<usize>>,
}

impl ConcurrencyGuard {
    pub fn new(limits: PerformanceLimits) -> Self {
        Self {
            limits,
            active_parsers: Arc::new(Mutex::new(0)),
        }
    }

    pub fn acquire_parser_slot(&self) -> Result<ParserSlot, PerformanceViolation> {
        let mut active = self
            .active_parsers
            .lock()
            .map_err(|_| PerformanceViolation::TooManyConcurrentParsers(0, 0))?;

        if *active >= self.limits.max_concurrent_parsers {
            return Err(PerformanceViolation::TooManyConcurrentParsers(
                *active,
                self.limits.max_concurrent_parsers,
            ));
        }

        *active += 1;
        Ok(ParserSlot {
            active_parsers: self.active_parsers.clone(),
        })
    }
}

pub struct ParserSlot {
    active_parsers: Arc<Mutex<usize>>,
}

impl Drop for ParserSlot {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active_parsers.lock() {
            *active = active.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_performance_limits() {
        let limits = PerformanceLimits::conservative();
        let mut guard = PerformanceGuard::new(limits, "test");

        // Test node limit
        for _ in 0..100_000 {
            guard.increment_nodes().unwrap();
        }

        // Should fail on the next increment
        assert!(guard.increment_nodes().is_err());
    }

    #[test]
    fn test_recursion_guard() {
        let limits = PerformanceLimits::conservative();
        let mut guard = PerformanceGuard::new(limits, "test");

        // Test that we can enter recursion initially
        let rguard1 = guard
            .enter_recursion()
            .expect("Should be able to enter recursion");

        // Test depth tracking
        assert_eq!(rguard1.depth(), 1);

        // Drop the guard to allow further recursion
        drop(rguard1);

        // Should be able to enter again after dropping
        let _rguard2 = guard
            .enter_recursion()
            .expect("Should be able to enter recursion again");
    }

    #[test]
    fn test_concurrency_guard() {
        let limits = PerformanceLimits::conservative();
        let guard = ConcurrencyGuard::new(limits);

        // Acquire max slots
        let mut slots = Vec::new();
        for _ in 0..2 {
            slots.push(guard.acquire_parser_slot().unwrap());
        }

        // Should fail to acquire another slot
        assert!(guard.acquire_parser_slot().is_err());
    }

    #[test]
    fn test_timeout_check() {
        let limits = PerformanceLimits {
            max_parse_time: Duration::from_millis(10),
            ..Default::default()
        };
        let guard = PerformanceGuard::new(limits, "test");

        thread::sleep(Duration::from_millis(20));
        assert!(guard.check_parse_timeout().is_err());
    }

    #[test]
    fn byte_limits_reject_partial_megabytes() {
        let limits = PerformanceLimits {
            max_file_size_mb: 1,
            max_object_size_mb: 1,
            ..Default::default()
        };
        let guard = PerformanceGuard::new(limits, "test");
        let one_mb = 1024 * 1024;

        assert!(guard.check_file_size(one_mb).is_ok());
        assert!(guard.check_file_size(one_mb + 1).is_err());
        assert!(guard.check_object_size(one_mb).is_ok());
        assert!(guard.check_object_size(one_mb + 1).is_err());
    }

    #[test]
    fn rejected_memory_allocation_is_not_recorded() {
        let limits = PerformanceLimits {
            max_memory_mb: 1,
            ..Default::default()
        };
        let guard = PerformanceGuard::new(limits, "test");
        let one_mb = 1024 * 1024;

        guard.track_memory_allocation(one_mb).unwrap();
        assert!(guard.track_memory_allocation(1).is_err());
        assert_eq!(guard.get_stats().memory_usage_mb, 1);
    }

    #[test]
    fn test_resource_budget_is_shared_and_cancelable() {
        let budget = ResourceBudget::new(4, 8, 8, 10, 1, 1, 1, 1);
        let clone = budget.clone();

        budget.consume_input(4).unwrap();
        assert!(clone.consume_input(1).is_err());

        clone.cancellation.cancel();
        assert_eq!(budget.check(), Err(ResourceBudgetError::Cancelled));
    }

    #[test]
    fn resource_budget_does_not_overconsume_decoded_bytes() {
        let budget = ResourceBudget::new(100, 8, 8, 10, 1, 1, 1, 1);

        budget.consume_decoded(6).unwrap();
        assert_eq!(budget.remaining_decoded_bytes(), 2);
        assert_eq!(
            budget.consume_decoded(3),
            Err(ResourceBudgetError::DecodedBytes)
        );
        assert_eq!(budget.remaining_decoded_bytes(), 2);
    }
}
