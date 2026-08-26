use crate::performance::{update_memory_peak, PerformanceConfig};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Memory manager with configurable limits and monitoring
pub struct MemoryManager {
    config: PerformanceConfig,
    current_usage: AtomicUsize,
    peak_usage: AtomicUsize,
    allocations: Arc<Mutex<HashMap<String, AllocationInfo>>>,
}

impl MemoryManager {
    pub fn new(config: PerformanceConfig) -> Self {
        Self {
            config,
            current_usage: AtomicUsize::new(0),
            peak_usage: AtomicUsize::new(0),
            allocations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check if we can allocate the requested amount of memory
    pub fn can_allocate(&self, size_bytes: usize) -> bool {
        let current = self.current_usage.load(Ordering::Relaxed);
        let max_bytes = self.config.max_memory_mb * 1024 * 1024;

        current + size_bytes <= max_bytes
    }

    /// Register a memory allocation
    pub fn allocate(&self, category: &str, size_bytes: usize) -> Result<AllocationId, MemoryError> {
        if !self.can_allocate(size_bytes) {
            return Err(MemoryError::OutOfMemory {
                requested: size_bytes,
                available: self.available_bytes(),
            });
        }

        let allocation_id = AllocationId::new();
        let info = AllocationInfo {
            id: allocation_id.clone(),
            category: category.to_string(),
            size_bytes,
            timestamp: std::time::Instant::now(),
        };

        // Update counters
        let new_usage = self.current_usage.fetch_add(size_bytes, Ordering::Relaxed) + size_bytes;

        // Update peak if necessary
        let mut peak = self.peak_usage.load(Ordering::Relaxed);
        while new_usage > peak {
            match self.peak_usage.compare_exchange_weak(
                peak,
                new_usage,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    update_memory_peak(new_usage / (1024 * 1024));
                    break;
                }
                Err(actual) => peak = actual,
            }
        }

        // Register allocation
        self.allocations
            .lock()
            .insert(allocation_id.to_string(), info);

        log::debug!(
            "Allocated {} bytes for {}, total usage: {} MB",
            size_bytes,
            category,
            new_usage / (1024 * 1024)
        );

        Ok(allocation_id)
    }

    /// Release a memory allocation
    pub fn deallocate(&self, allocation_id: AllocationId) -> Result<(), MemoryError> {
        let mut allocations = self.allocations.lock();

        if let Some(info) = allocations.remove(&allocation_id.to_string()) {
            self.current_usage
                .fetch_sub(info.size_bytes, Ordering::Relaxed);

            log::debug!(
                "Deallocated {} bytes for {}, remaining usage: {} MB",
                info.size_bytes,
                info.category,
                self.current_usage.load(Ordering::Relaxed) / (1024 * 1024)
            );

            Ok(())
        } else {
            Err(MemoryError::AllocationNotFound(allocation_id))
        }
    }

    /// Get current memory usage statistics
    pub fn get_stats(&self) -> MemoryStats {
        let allocations = self.allocations.lock();
        let current_bytes = self.current_usage.load(Ordering::Relaxed);
        let peak_bytes = self.peak_usage.load(Ordering::Relaxed);

        let mut by_category = HashMap::new();
        for info in allocations.values() {
            *by_category.entry(info.category.clone()).or_insert(0) += info.size_bytes;
        }

        MemoryStats {
            current_usage_bytes: current_bytes,
            current_usage_mb: current_bytes / (1024 * 1024),
            peak_usage_bytes: peak_bytes,
            peak_usage_mb: peak_bytes / (1024 * 1024),
            limit_bytes: self.config.max_memory_mb * 1024 * 1024,
            limit_mb: self.config.max_memory_mb,
            available_bytes: self.available_bytes(),
            allocation_count: allocations.len(),
            usage_by_category: by_category,
        }
    }

    /// Perform garbage collection of old allocations
    pub fn gc(&self, max_age: std::time::Duration) -> usize {
        let mut allocations = self.allocations.lock();
        let now = std::time::Instant::now();
        let mut freed_bytes = 0;

        allocations.retain(|_, info| {
            if now.duration_since(info.timestamp) > max_age {
                freed_bytes += info.size_bytes;
                false
            } else {
                true
            }
        });

        if freed_bytes > 0 {
            self.current_usage.fetch_sub(freed_bytes, Ordering::Relaxed);
            log::info!(
                "Garbage collected {} bytes from {} old allocations",
                freed_bytes,
                freed_bytes
            );
        }

        freed_bytes
    }

    /// Force cleanup when approaching memory limit
    pub fn emergency_cleanup(&self) -> usize {
        let current_usage = self.current_usage.load(Ordering::Relaxed);
        let limit = self.config.max_memory_mb * 1024 * 1024;

        if current_usage > (limit * 90 / 100) {
            // 90% threshold
            log::warn!(
                "Memory usage at {}%, performing emergency cleanup",
                current_usage * 100 / limit
            );

            // Cleanup old allocations aggressively
            self.gc(std::time::Duration::from_secs(30)) // Clean up anything older than 30s
        } else {
            0
        }
    }

    fn available_bytes(&self) -> usize {
        let current = self.current_usage.load(Ordering::Relaxed);
        let limit = self.config.max_memory_mb * 1024 * 1024;
        limit.saturating_sub(current)
    }
}

/// RAII memory allocation guard
pub struct MemoryGuard {
    manager: Arc<MemoryManager>,
    allocation_id: Option<AllocationId>,
}

impl MemoryGuard {
    pub fn new(
        manager: Arc<MemoryManager>,
        category: &str,
        size_bytes: usize,
    ) -> Result<Self, MemoryError> {
        let allocation_id = manager.allocate(category, size_bytes)?;
        Ok(Self {
            manager,
            allocation_id: Some(allocation_id),
        })
    }

    pub fn release(mut self) {
        if let Some(id) = self.allocation_id.take() {
            let _ = self.manager.deallocate(id);
        }
    }
}

impl Drop for MemoryGuard {
    fn drop(&mut self) {
        if let Some(id) = self.allocation_id.take() {
            let _ = self.manager.deallocate(id);
        }
    }
}

/// Memory pool for frequently allocated objects
pub struct MemoryPool<T> {
    pool: Mutex<Vec<Box<T>>>,
    max_size: usize,
    create_fn: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T: Send + 'static> MemoryPool<T> {
    pub fn new<F>(max_size: usize, create_fn: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            pool: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
            create_fn: Box::new(create_fn),
        }
    }

    /// Get an object from the pool or create a new one
    pub fn get(&self) -> PooledObject<'_, T>
    where
        T: Send + 'static,
    {
        let mut pool = self.pool.lock();
        let object = pool.pop().unwrap_or_else(|| Box::new((self.create_fn)()));

        PooledObject {
            object: Some(object),
            pool: self,
        }
    }

    fn return_object(&self, object: Box<T>) {
        let mut pool = self.pool.lock();
        if pool.len() < self.max_size {
            pool.push(object);
        }
        // Otherwise let it drop
    }
}

/// RAII wrapper for pooled objects
pub struct PooledObject<'a, T: Send + 'static> {
    object: Option<Box<T>>,
    pool: &'a MemoryPool<T>,
}

impl<'a, T: Send + 'static> PooledObject<'a, T> {
    pub fn get(&self) -> &T {
        self.object.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<'a, T: Send + 'static> AsRef<T> for PooledObject<'a, T> {
    fn as_ref(&self) -> &T {
        self.object.as_ref().unwrap()
    }
}

impl<'a, T: Send + 'static> AsMut<T> for PooledObject<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<'a, T: Send + 'static> Drop for PooledObject<'a, T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            self.pool.return_object(object);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AllocationId {
    id: uuid::Uuid,
}

impl AllocationId {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
        }
    }
}

impl std::fmt::Display for AllocationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

#[derive(Debug, Clone)]
pub struct AllocationInfo {
    pub id: AllocationId,
    pub category: String,
    pub size_bytes: usize,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub current_usage_bytes: usize,
    pub current_usage_mb: usize,
    pub peak_usage_bytes: usize,
    pub peak_usage_mb: usize,
    pub limit_bytes: usize,
    pub limit_mb: usize,
    pub available_bytes: usize,
    pub allocation_count: usize,
    pub usage_by_category: HashMap<String, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Out of memory: requested {requested} bytes, only {available} bytes available")]
    OutOfMemory { requested: usize, available: usize },
    #[error("Allocation not found: {0}")]
    AllocationNotFound(AllocationId),
}

/// Memory-aware buffer that can grow within limits
#[allow(dead_code)]
pub struct BoundedBuffer {
    data: Vec<u8>,
    max_size: usize,
    manager: Arc<MemoryManager>,
    allocation_guard: Option<MemoryGuard>,
}

impl BoundedBuffer {
    pub fn new(
        initial_capacity: usize,
        max_size: usize,
        manager: Arc<MemoryManager>,
    ) -> Result<Self, MemoryError> {
        let guard = MemoryGuard::new(manager.clone(), "bounded_buffer", initial_capacity)?;

        Ok(Self {
            data: Vec::with_capacity(initial_capacity),
            max_size,
            manager,
            allocation_guard: Some(guard),
        })
    }

    pub fn push(&mut self, value: u8) -> Result<(), MemoryError> {
        if self.data.len() >= self.max_size {
            return Err(MemoryError::OutOfMemory {
                requested: 1,
                available: 0,
            });
        }

        // Check if we need to grow the capacity
        if self.data.len() == self.data.capacity() {
            let new_capacity = std::cmp::min(self.data.capacity() * 2, self.max_size);
            let additional_bytes =
                (new_capacity - self.data.capacity()) * std::mem::size_of::<u8>();

            if !self.manager.can_allocate(additional_bytes) {
                return Err(MemoryError::OutOfMemory {
                    requested: additional_bytes,
                    available: self.manager.available_bytes(),
                });
            }
        }

        self.data.push(value);
        Ok(())
    }

    pub fn extend_from_slice(&mut self, slice: &[u8]) -> Result<(), MemoryError> {
        if self.data.len() + slice.len() > self.max_size {
            return Err(MemoryError::OutOfMemory {
                requested: slice.len(),
                available: self.max_size - self.data.len(),
            });
        }

        let required_capacity = self.data.len() + slice.len();
        if required_capacity > self.data.capacity() {
            let additional_bytes =
                (required_capacity - self.data.capacity()) * std::mem::size_of::<u8>();

            if !self.manager.can_allocate(additional_bytes) {
                return Err(MemoryError::OutOfMemory {
                    requested: additional_bytes,
                    available: self.manager.available_bytes(),
                });
            }
        }

        self.data.extend_from_slice(slice);
        Ok(())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager() {
        let config = PerformanceConfig {
            max_memory_mb: 1, // 1MB limit for testing
            ..Default::default()
        };

        let manager = MemoryManager::new(config);

        // Test successful allocation
        let alloc1 = manager.allocate("test", 1024).unwrap();
        assert_eq!(manager.current_usage.load(Ordering::Relaxed), 1024);

        // Test deallocation
        manager.deallocate(alloc1).unwrap();
        assert_eq!(manager.current_usage.load(Ordering::Relaxed), 0);

        // Test out of memory
        let result = manager.allocate("test", 2 * 1024 * 1024); // 2MB > 1MB limit
        assert!(matches!(result, Err(MemoryError::OutOfMemory { .. })));
    }

    #[test]
    fn test_memory_pool() {
        let pool = MemoryPool::new(5, || vec![0u8; 1024]);

        {
            let obj1 = pool.get();
            assert_eq!(obj1.as_ref().len(), 1024);
        } // obj1 returned to pool

        {
            let obj2 = pool.get(); // Should reuse the returned object
            assert_eq!(obj2.as_ref().len(), 1024);
        }
    }

    #[test]
    fn test_bounded_buffer() {
        let config = PerformanceConfig {
            max_memory_mb: 1,
            ..Default::default()
        };
        let manager = Arc::new(MemoryManager::new(config));

        let mut buffer = BoundedBuffer::new(10, 20, manager).unwrap();

        // Test normal operation
        for i in 0..15 {
            buffer.push(i).unwrap();
        }
        assert_eq!(buffer.len(), 15);

        // Test capacity limit
        for i in 0..10 {
            if buffer.len() < 20 {
                buffer.push(i).unwrap();
            }
        }

        // Should not be able to exceed max size
        assert!(buffer.push(255).is_err());
    }
}
