//! Hash cache backend trait.

use std::collections::HashMap;

use async_trait::async_trait;

use super::entry::{HashCacheEntry, HashCacheKey};

/// Pluggable backend for hash cache storage.
///
/// Implementations handle their own error recovery - on failure,
/// methods return None/Ok to allow graceful degradation (rehash the file).
#[async_trait]
pub trait HashCacheBackend: Send + Sync {
    /// Look up a cached hash by file identity.
    ///
    /// # Arguments
    /// * `key` - The cache key (path, size, mtime)
    ///
    /// # Returns
    /// The cached entry if found and not expired, None otherwise.
    /// Returns None on error (graceful degradation).
    async fn get(&self, key: &HashCacheKey) -> Option<HashCacheEntry>;

    /// Store a hash entry.
    ///
    /// # Arguments
    /// * `key` - The cache key (path, size, mtime)
    /// * `entry` - The entry to store
    ///
    /// Silently fails on error (caller will just rehash next time).
    async fn put(&self, key: &HashCacheKey, entry: &HashCacheEntry);

    /// Remove an entry (for explicit invalidation).
    ///
    /// # Arguments
    /// * `key` - The cache key to remove
    async fn delete(&self, key: &HashCacheKey);

    /// Bulk lookup for multiple keys.
    ///
    /// # Arguments
    /// * `keys` - Slice of cache keys to look up
    ///
    /// # Returns
    /// Map of found entries; missing/expired keys are omitted.
    async fn get_batch(&self, keys: &[HashCacheKey]) -> HashMap<HashCacheKey, HashCacheEntry> {
        // Default implementation: sequential gets
        let mut results: HashMap<HashCacheKey, HashCacheEntry> = HashMap::new();
        for key in keys {
            if let Some(entry) = self.get(key).await {
                results.insert(key.clone(), entry);
            }
        }
        results
    }

    /// Bulk store for multiple entries.
    ///
    /// # Arguments
    /// * `entries` - Slice of (key, entry) pairs to store
    async fn put_batch(&self, entries: &[(HashCacheKey, HashCacheEntry)]) {
        // Default implementation: sequential puts
        for (key, entry) in entries {
            self.put(key, entry).await;
        }
    }

    /// Clear all entries (for cache reset).
    async fn clear(&self);
}
