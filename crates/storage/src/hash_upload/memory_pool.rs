//! Memory pool with backpressure for pipelined operations.
//!
//! Controls memory usage by limiting concurrent in-flight data.
//! When the pool is exhausted, new allocations block until
//! memory is released.

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Semaphore, SemaphorePermit};

/// Memory pool with backpressure for pipelined operations.
///
/// Controls memory usage by limiting concurrent in-flight data.
/// When the pool is exhausted, new allocations block until
/// memory is released.
pub struct MemoryPool {
    /// Maximum bytes allowed.
    #[allow(dead_code)]
    max_bytes: u64,
    /// Currently allocated bytes (for monitoring).
    allocated: AtomicU64,
    /// Semaphore for blocking when full.
    semaphore: Semaphore,
    /// Permit size (granularity of allocation).
    permit_size: u64,
}

impl MemoryPool {
    /// Create a new memory pool.
    ///
    /// # Arguments
    /// * `max_bytes` - Maximum memory to allow
    /// * `permit_size` - Size of each permit (e.g., 64MB)
    pub fn new(max_bytes: u64, permit_size: u64) -> Self {
        let permits: usize = (max_bytes / permit_size).max(1) as usize;
        Self {
            max_bytes,
            allocated: AtomicU64::new(0),
            semaphore: Semaphore::new(permits),
            permit_size,
        }
    }

    /// Allocate memory from the pool.
    ///
    /// Blocks if the pool is exhausted until memory is released.
    ///
    /// # Arguments
    /// * `size` - Number of bytes to allocate
    ///
    /// # Returns
    /// A permit that releases memory when dropped.
    pub async fn allocate(&self, size: u64) -> MemoryPermit<'_> {
        // Calculate permits needed (round up)
        let permits_needed: u32 = ((size + self.permit_size - 1) / self.permit_size).max(1) as u32;

        // Acquire permits (blocks if not available)
        let permit: SemaphorePermit<'_> = self
            .semaphore
            .acquire_many(permits_needed)
            .await
            .expect("semaphore closed");

        self.allocated.fetch_add(size, Ordering::Relaxed);

        MemoryPermit {
            pool: self,
            size,
            _permit: permit,
        }
    }

    /// Try to allocate without blocking.
    ///
    /// # Arguments
    /// * `size` - Number of bytes to allocate
    ///
    /// # Returns
    /// Some(permit) if allocation succeeded, None if pool is exhausted.
    #[allow(dead_code)]
    pub fn try_allocate(&self, size: u64) -> Option<MemoryPermit<'_>> {
        let permits_needed: u32 = ((size + self.permit_size - 1) / self.permit_size).max(1) as u32;

        match self.semaphore.try_acquire_many(permits_needed) {
            Ok(permit) => {
                self.allocated.fetch_add(size, Ordering::Relaxed);
                Some(MemoryPermit {
                    pool: self,
                    size,
                    _permit: permit,
                })
            }
            Err(_) => None,
        }
    }

    /// Get current allocated bytes.
    #[allow(dead_code)]
    pub fn allocated(&self) -> u64 {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Get maximum bytes.
    #[allow(dead_code)]
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Get available permits count.
    #[allow(dead_code)]
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// RAII guard for allocated memory.
///
/// Releases memory back to the pool when dropped.
pub struct MemoryPermit<'a> {
    pool: &'a MemoryPool,
    size: u64,
    _permit: SemaphorePermit<'a>,
}

impl<'a> MemoryPermit<'a> {
    /// Get the size of this allocation.
    #[allow(dead_code)]
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for MemoryPermit<'_> {
    fn drop(&mut self) {
        self.pool.allocated.fetch_sub(self.size, Ordering::Relaxed);
        // SemaphorePermit is automatically released when dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allocate_and_release() {
        let pool = MemoryPool::new(128, 64);

        assert_eq!(pool.allocated(), 0);
        assert_eq!(pool.available_permits(), 2);

        {
            let _permit = pool.allocate(50).await;
            assert_eq!(pool.allocated(), 50);
            assert_eq!(pool.available_permits(), 1);
        }

        // After drop, memory should be released
        assert_eq!(pool.allocated(), 0);
        assert_eq!(pool.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_try_allocate_success() {
        let pool = MemoryPool::new(128, 64);

        let permit = pool.try_allocate(50);
        assert!(permit.is_some());
        assert_eq!(pool.allocated(), 50);
    }

    #[tokio::test]
    async fn test_try_allocate_exhausted() {
        let pool = MemoryPool::new(64, 64);

        let _permit1 = pool.allocate(64).await;
        let permit2 = pool.try_allocate(64);
        assert!(permit2.is_none());
    }

    #[tokio::test]
    async fn test_multiple_allocations() {
        let pool = MemoryPool::new(256, 64);

        let permit1 = pool.allocate(64).await;
        let permit2 = pool.allocate(64).await;

        assert_eq!(pool.allocated(), 128);
        assert_eq!(pool.available_permits(), 2);

        drop(permit1);
        assert_eq!(pool.allocated(), 64);
        assert_eq!(pool.available_permits(), 3);

        drop(permit2);
        assert_eq!(pool.allocated(), 0);
        assert_eq!(pool.available_permits(), 4);
    }
}
