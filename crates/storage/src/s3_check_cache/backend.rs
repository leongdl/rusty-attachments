//! S3 check cache backend trait.

use async_trait::async_trait;

use super::entry::{S3CheckCacheEntry, S3CheckCacheKey};

/// Pluggable backend for S3 check cache storage.
///
/// Same graceful degradation philosophy as HashCacheBackend -
/// implementations handle their own error recovery.
#[async_trait]
pub trait S3CheckCacheBackend: Send + Sync {
    /// Check if an S3 key was previously seen as uploaded.
    ///
    /// # Arguments
    /// * `key` - The cache key
    ///
    /// # Returns
    /// The cached entry if found, None otherwise.
    /// Returns None on error (graceful degradation).
    async fn get(&self, key: &S3CheckCacheKey) -> Option<S3CheckCacheEntry>;

    /// Record that an S3 key exists.
    ///
    /// # Arguments
    /// * `entry` - The entry to store
    ///
    /// Silently fails on error.
    async fn put(&self, entry: &S3CheckCacheEntry);

    /// Remove an entry (for cache invalidation).
    ///
    /// # Arguments
    /// * `key` - The cache key to remove
    async fn delete(&self, key: &S3CheckCacheKey);

    /// Clear all entries (for cache reset).
    async fn clear(&self);

    /// Bulk lookup for multiple keys.
    ///
    /// # Arguments
    /// * `keys` - Slice of cache keys to look up
    ///
    /// # Returns
    /// Vector of found entries; missing keys are omitted.
    async fn get_batch(&self, keys: &[S3CheckCacheKey]) -> Vec<S3CheckCacheEntry> {
        // Default implementation: sequential gets
        let mut results: Vec<S3CheckCacheEntry> = Vec::new();
        for key in keys {
            if let Some(entry) = self.get(key).await {
                results.push(entry);
            }
        }
        results
    }

    /// Bulk store for multiple entries.
    ///
    /// # Arguments
    /// * `entries` - Slice of entries to store
    async fn put_batch(&self, entries: &[S3CheckCacheEntry]) {
        // Default implementation: sequential puts
        for entry in entries {
            self.put(entry).await;
        }
    }
}
