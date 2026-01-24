//! Core pipeline implementation for pipelined hash+upload.
//!
//! Reads files once into memory, hashes them, then uploads from the same buffer.
//! Provides concurrent processing with memory backpressure and hash deduplication.

use std::sync::Arc;

use futures::stream::{self, StreamExt};
use rusty_attachments_common::Xxh3Hasher;
use rusty_attachments_model::HashAlgorithm;
use tokio::task::spawn_blocking;

use crate::error::StorageError;
use crate::hash_cache::HashCache;
use crate::traits::ContentAddressedDataCache;

use super::deduplication::{UploadDeduplicator, UploadIntent, UploadResult};
use super::memory_pool::MemoryPool;
use super::options::HashUploadOptions;
use super::progress::{HashUploadProgress, ProgressTracker};

/// Work item for the pipeline.
#[derive(Debug, Clone)]
pub struct WorkItem {
    /// Absolute path to the file.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Modification time in microseconds.
    pub mtime: i64,
}

/// Result of processing a single file.
#[derive(Debug, Clone)]
pub struct ProcessedItem {
    /// Original path.
    pub path: String,
    /// Computed hash.
    pub hash: String,
    /// File size.
    pub size: u64,
    /// Whether upload was skipped (already existed or deduplicated).
    pub upload_skipped: bool,
    /// Whether hash was from cache.
    pub hash_cached: bool,
}

/// Pipelined hash+upload processor.
///
/// Coordinates reading, hashing, and uploading files with:
/// - Memory backpressure via semaphore-based pool
/// - Hash deduplication for duplicate files
/// - Concurrent processing up to configurable limit
pub struct HashUploadPipeline<'a, C: ContentAddressedDataCache> {
    data_cache: &'a C,
    hash_cache: Option<&'a HashCache>,
    options: HashUploadOptions,
    memory_pool: Arc<MemoryPool>,
    deduplicator: Arc<UploadDeduplicator>,
    progress: Arc<ProgressTracker>,
    hash_alg: HashAlgorithm,
}

impl<'a, C: ContentAddressedDataCache + 'static> HashUploadPipeline<'a, C> {
    /// Create a new pipeline.
    ///
    /// # Arguments
    /// * `data_cache` - Content-addressable storage destination
    /// * `hash_cache` - Optional hash cache for efficiency
    /// * `options` - Pipeline configuration
    /// * `total_files` - Total number of files to process
    /// * `total_bytes` - Total bytes to process
    pub fn new(
        data_cache: &'a C,
        hash_cache: Option<&'a HashCache>,
        options: HashUploadOptions,
        total_files: u64,
        total_bytes: u64,
    ) -> Self {
        let memory_pool = Arc::new(MemoryPool::new(
            options.max_memory_bytes,
            64 * 1024 * 1024, // 64MB permit size
        ));

        Self {
            data_cache,
            hash_cache,
            options,
            memory_pool,
            deduplicator: Arc::new(UploadDeduplicator::new()),
            progress: Arc::new(ProgressTracker::new(total_files, total_bytes)),
            hash_alg: HashAlgorithm::Xxh128,
        }
    }

    /// Process all work items through the pipeline.
    ///
    /// # Arguments
    /// * `items` - Files to process
    ///
    /// # Returns
    /// Processed items with hashes and upload status.
    pub async fn process(&self, items: Vec<WorkItem>) -> Result<Vec<ProcessedItem>, StorageError> {
        let results: Vec<Result<ProcessedItem, StorageError>> = stream::iter(items)
            .map(|item| self.process_item(item))
            .buffer_unordered(self.options.max_concurrency)
            .collect()
            .await;

        // Collect results, propagating first error
        let mut processed: Vec<ProcessedItem> = Vec::with_capacity(results.len());
        for result in results {
            processed.push(result?);
        }

        Ok(processed)
    }

    /// Process a single work item.
    ///
    /// # Arguments
    /// * `item` - File to process
    ///
    /// # Returns
    /// Processed item with hash and upload status.
    async fn process_item(&self, item: WorkItem) -> Result<ProcessedItem, StorageError> {
        // Step 1: Check hash cache
        let cached_hash: Option<String> = self.get_cached_hash(&item).await;

        // Step 2: Check if we can skip entirely (hash cached + exists in data cache)
        if let Some(ref hash) = cached_hash {
            if self.options.use_s3_check_cache {
                if self.data_cache.object_exists(hash, self.hash_alg).await? {
                    self.progress.record_hash_complete(item.size, true);
                    self.progress.record_upload_complete(item.size, true);
                    return Ok(ProcessedItem {
                        path: item.path,
                        hash: hash.clone(),
                        size: item.size,
                        upload_skipped: true,
                        hash_cached: true,
                    });
                }
            }
        }

        // Step 3: Allocate memory and read file
        let _permit = self.memory_pool.allocate(item.size).await;

        let path_clone: String = item.path.clone();
        let data: Vec<u8> = spawn_blocking(move || std::fs::read(&path_clone))
            .await
            .map_err(|e| StorageError::Other {
                message: format!("Task join error: {}", e),
            })?
            .map_err(|e| StorageError::IoError {
                path: item.path.clone(),
                message: e.to_string(),
            })?;

        // Step 4: Compute hash (if not cached)
        let (hash, hash_cached): (String, bool) = if let Some(h) = cached_hash {
            self.progress.record_hash_complete(item.size, true);
            (h, true)
        } else {
            let hash: String = self.compute_hash(&data).await?;
            self.progress.record_hash_complete(item.size, false);

            // Update hash cache
            if let Some(cache) = self.hash_cache {
                cache
                    .put(&item.path, item.size, item.mtime, hash.clone())
                    .await;
            }

            (hash, false)
        };

        // Step 5: Upload (with deduplication)
        let upload_skipped: bool = self.upload_with_dedup(&hash, &data).await?;

        self.progress.record_upload_complete(item.size, upload_skipped);

        Ok(ProcessedItem {
            path: item.path,
            hash,
            size: item.size,
            upload_skipped,
            hash_cached,
        })
    }

    /// Get cached hash if available and caching is enabled.
    ///
    /// # Arguments
    /// * `item` - Work item to look up
    ///
    /// # Returns
    /// Cached hash if found, None otherwise.
    async fn get_cached_hash(&self, item: &WorkItem) -> Option<String> {
        if !self.options.use_hash_cache || self.options.force_rehash {
            return None;
        }

        if let Some(cache) = self.hash_cache {
            cache.get(&item.path, item.size, item.mtime).await
        } else {
            None
        }
    }

    /// Compute hash of data using spawn_blocking.
    ///
    /// # Arguments
    /// * `data` - Bytes to hash
    ///
    /// # Returns
    /// Hex-encoded hash string.
    async fn compute_hash(&self, data: &[u8]) -> Result<String, StorageError> {
        let data_clone: Vec<u8> = data.to_vec();
        spawn_blocking(move || {
            let mut hasher: Xxh3Hasher = Xxh3Hasher::new();
            hasher.update(&data_clone);
            hasher.finish_hex()
        })
        .await
        .map_err(|e| StorageError::Other {
            message: format!("Hash task join error: {}", e),
        })
    }

    /// Upload data with deduplication.
    ///
    /// # Arguments
    /// * `hash` - Content hash
    /// * `data` - Bytes to upload
    ///
    /// # Returns
    /// True if upload was skipped (already existed or deduplicated).
    async fn upload_with_dedup(&self, hash: &str, data: &[u8]) -> Result<bool, StorageError> {
        // Check if already exists
        if self.data_cache.object_exists(hash, self.hash_alg).await? {
            return Ok(true);
        }

        // Register upload intent
        match self.deduplicator.register(hash) {
            UploadIntent::Proceed => {
                // We're the first uploader
                let result = self.data_cache.put_object(hash, self.hash_alg, data).await;

                if result.is_ok() {
                    self.deduplicator.complete(hash);
                } else {
                    self.deduplicator.failed(hash);
                }

                result?;
                Ok(false)
            }
            UploadIntent::Wait(mut receiver) => {
                // Wait for other upload to complete
                match receiver.recv().await {
                    Ok(UploadResult::Success) => Ok(true),
                    Ok(UploadResult::Failed) => {
                        // Other upload failed, try ourselves
                        self.data_cache.put_object(hash, self.hash_alg, data).await?;
                        Ok(false)
                    }
                    Err(_) => {
                        // Channel closed, try ourselves
                        self.data_cache.put_object(hash, self.hash_alg, data).await?;
                        Ok(false)
                    }
                }
            }
        }
    }

    /// Get current progress snapshot.
    pub fn progress(&self) -> HashUploadProgress {
        self.progress.snapshot()
    }

    /// Get the progress tracker for external monitoring.
    #[allow(dead_code)]
    pub fn progress_tracker(&self) -> &Arc<ProgressTracker> {
        &self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Mock data cache for testing.
    struct MockDataCache {
        objects: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MockDataCache {
        fn new() -> Self {
            Self {
                objects: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl ContentAddressedDataCache for MockDataCache {
        fn get_object_key(&self, hash: &str, algorithm: HashAlgorithm) -> String {
            format!("{}.{}", hash, algorithm.extension())
        }

        async fn object_exists(
            &self,
            hash: &str,
            _algorithm: HashAlgorithm,
        ) -> Result<bool, StorageError> {
            let objects = self.objects.lock().unwrap();
            Ok(objects.contains_key(hash))
        }

        async fn object_size(
            &self,
            hash: &str,
            _algorithm: HashAlgorithm,
        ) -> Result<Option<u64>, StorageError> {
            let objects = self.objects.lock().unwrap();
            Ok(objects.get(hash).map(|v| v.len() as u64))
        }

        async fn put_object(
            &self,
            hash: &str,
            _algorithm: HashAlgorithm,
            data: &[u8],
        ) -> Result<(), StorageError> {
            let mut objects = self.objects.lock().unwrap();
            objects.insert(hash.to_string(), data.to_vec());
            Ok(())
        }

        async fn put_object_from_file(
            &self,
            hash: &str,
            algorithm: HashAlgorithm,
            file_path: &Path,
            _progress: Option<&dyn crate::traits::ProgressCallback>,
        ) -> Result<(), StorageError> {
            let data: Vec<u8> = std::fs::read(file_path).map_err(|e| StorageError::IoError {
                path: file_path.display().to_string(),
                message: e.to_string(),
            })?;
            self.put_object(hash, algorithm, &data).await
        }

        async fn get_object(
            &self,
            hash: &str,
            _algorithm: HashAlgorithm,
        ) -> Result<Vec<u8>, StorageError> {
            let objects = self.objects.lock().unwrap();
            objects.get(hash).cloned().ok_or(StorageError::NotFound {
                bucket: "mock".to_string(),
                key: hash.to_string(),
            })
        }

        async fn get_object_to_file(
            &self,
            hash: &str,
            algorithm: HashAlgorithm,
            file_path: &Path,
            _progress: Option<&dyn crate::traits::ProgressCallback>,
        ) -> Result<(), StorageError> {
            let data: Vec<u8> = self.get_object(hash, algorithm).await?;
            std::fs::write(file_path, data).map_err(|e| StorageError::IoError {
                path: file_path.display().to_string(),
                message: e.to_string(),
            })
        }
    }

    fn create_test_file(dir: &TempDir, name: &str, content: &[u8]) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn test_process_single_file() {
        let dir = TempDir::new().unwrap();
        let path: String = create_test_file(&dir, "test.txt", b"hello world");

        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();
        let pipeline = HashUploadPipeline::new(&cache, None, options, 1, 11);

        let items: Vec<WorkItem> = vec![WorkItem {
            path: path.clone(),
            size: 11,
            mtime: 0,
        }];

        let results: Vec<ProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, path);
        assert!(!results[0].hash.is_empty());
        assert!(!results[0].upload_skipped);
        assert!(!results[0].hash_cached);
    }

    #[tokio::test]
    async fn test_process_multiple_files() {
        let dir = TempDir::new().unwrap();
        let path1: String = create_test_file(&dir, "file1.txt", b"content one");
        let path2: String = create_test_file(&dir, "file2.txt", b"content two");
        let path3: String = create_test_file(&dir, "file3.txt", b"content three");

        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();
        let pipeline = HashUploadPipeline::new(&cache, None, options, 3, 35);

        let items: Vec<WorkItem> = vec![
            WorkItem {
                path: path1,
                size: 11,
                mtime: 0,
            },
            WorkItem {
                path: path2,
                size: 11,
                mtime: 0,
            },
            WorkItem {
                path: path3,
                size: 13,
                mtime: 0,
            },
        ];

        let results: Vec<ProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 3);
        // All should have unique hashes
        let hashes: Vec<&String> = results.iter().map(|r| &r.hash).collect();
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
    }

    #[tokio::test]
    async fn test_duplicate_files_deduplicated() {
        let dir = TempDir::new().unwrap();
        // Same content, different names
        let path1: String = create_test_file(&dir, "dup1.txt", b"duplicate content");
        let path2: String = create_test_file(&dir, "dup2.txt", b"duplicate content");

        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();
        let pipeline = HashUploadPipeline::new(&cache, None, options, 2, 34);

        let items: Vec<WorkItem> = vec![
            WorkItem {
                path: path1,
                size: 17,
                mtime: 0,
            },
            WorkItem {
                path: path2,
                size: 17,
                mtime: 0,
            },
        ];

        let results: Vec<ProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 2);
        // Same hash
        assert_eq!(results[0].hash, results[1].hash);
        // One should be uploaded, one should be skipped (or both uploaded due to race)
        // At least one should not be skipped
        assert!(results.iter().any(|r| !r.upload_skipped));
    }

    #[tokio::test]
    async fn test_progress_tracking() {
        let dir = TempDir::new().unwrap();
        let path: String = create_test_file(&dir, "test.txt", b"hello");

        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();
        let pipeline = HashUploadPipeline::new(&cache, None, options, 1, 5);

        let items: Vec<WorkItem> = vec![WorkItem {
            path,
            size: 5,
            mtime: 0,
        }];

        let _results: Vec<ProcessedItem> = pipeline.process(items).await.unwrap();

        let progress: HashUploadProgress = pipeline.progress();
        assert_eq!(progress.total_files, 1);
        assert_eq!(progress.total_bytes, 5);
        assert_eq!(progress.hashed_files, 1);
        assert_eq!(progress.uploaded_files, 1);
    }
}
