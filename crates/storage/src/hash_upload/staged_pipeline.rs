//! 3-stage pipelined hash+upload implementation.
//!
//! Separates file processing into three concurrent stages:
//! 1. **Read Stage**: Reads files from disk into memory buffers
//! 2. **Hash Stage**: Computes XXH128 hash of file contents
//! 3. **Upload Stage**: Uploads data to S3/storage backend
//!
//! Each stage runs concurrently, connected by async channels.
//! Memory is bounded by the memory pool with backpressure.

use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use rusty_attachments_common::Xxh3Hasher;
use rusty_attachments_model::HashAlgorithm;

use crate::error::StorageError;
use crate::hash_cache::HashCache;
use crate::traits::ContentAddressedDataCache;

use super::deduplication::{UploadDeduplicator, UploadIntent, UploadResult};
use super::memory_pool::MemoryPool;
use super::options::HashUploadOptions;
use super::progress::{HashUploadProgress, ProgressTracker};
use super::pipeline::WorkItem;

/// Channel capacity for inter-stage communication.
const CHANNEL_CAPACITY: usize = 256;

/// Result of processing a single file.
#[derive(Debug, Clone)]
pub struct StagedProcessedItem {
    /// Original path.
    pub path: String,
    /// Computed hash.
    pub hash: String,
    /// File size.
    pub size: u64,
    /// Whether upload was skipped.
    pub upload_skipped: bool,
    /// Whether hash was from cache.
    pub hash_cached: bool,
}

/// Configuration for the staged pipeline.
#[derive(Debug, Clone)]
pub struct StagedPipelineConfig {
    /// Maximum concurrent file reads.
    pub read_concurrency: usize,
    /// Maximum concurrent hash computations.
    pub hash_concurrency: usize,
    /// Maximum concurrent uploads.
    pub upload_concurrency: usize,
    /// Maximum memory for in-flight data.
    pub max_memory_bytes: u64,
    /// Memory permit granularity.
    pub permit_size: u64,
}

impl Default for StagedPipelineConfig {
    fn default() -> Self {
        Self {
            read_concurrency: 16,
            hash_concurrency: 16,
            upload_concurrency: 32,
            max_memory_bytes: 5 * 1024 * 1024 * 1024, // 5GB
            permit_size: 64 * 1024 * 1024,            // 64MB
        }
    }
}

impl From<&HashUploadOptions> for StagedPipelineConfig {
    fn from(opts: &HashUploadOptions) -> Self {
        // Distribute 64 total threads across stages
        // Read: 16, Hash: 16, Upload: 32
        Self {
            read_concurrency: 16,
            hash_concurrency: 16,
            upload_concurrency: 32,
            max_memory_bytes: opts.max_memory_bytes,
            permit_size: 64 * 1024 * 1024,
        }
    }
}

/// 3-stage pipelined processor.
///
/// Processes files through read → hash → upload stages concurrently.
pub struct StagedPipeline<C: ContentAddressedDataCache> {
    data_cache: Arc<C>,
    hash_cache: Option<Arc<HashCache>>,
    options: HashUploadOptions,
    config: StagedPipelineConfig,
    memory_pool: Arc<MemoryPool>,
    deduplicator: Arc<UploadDeduplicator>,
    progress: Arc<ProgressTracker>,
}

impl<C: ContentAddressedDataCache + Send + Sync + 'static> StagedPipeline<C> {
    /// Create a new staged pipeline.
    ///
    /// # Arguments
    /// * `data_cache` - Content-addressable storage destination
    /// * `hash_cache` - Optional hash cache
    /// * `options` - Pipeline options
    /// * `total_files` - Total files to process
    /// * `total_bytes` - Total bytes to process
    pub fn new(
        data_cache: Arc<C>,
        hash_cache: Option<Arc<HashCache>>,
        options: HashUploadOptions,
        total_files: u64,
        total_bytes: u64,
    ) -> Self {
        let config = StagedPipelineConfig::from(&options);
        let memory_pool = Arc::new(MemoryPool::new(config.max_memory_bytes, config.permit_size));

        Self {
            data_cache,
            hash_cache,
            options,
            config,
            memory_pool,
            deduplicator: Arc::new(UploadDeduplicator::new()),
            progress: Arc::new(ProgressTracker::new(total_files, total_bytes)),
        }
    }

    /// Process all work items through the 3-stage pipeline.
    ///
    /// # Arguments
    /// * `items` - Files to process
    ///
    /// # Returns
    /// Processed items with hashes and upload status.
    pub async fn process(
        self: Arc<Self>,
        items: Vec<WorkItem>,
    ) -> Result<Vec<StagedProcessedItem>, StorageError> {
        let total_items: usize = items.len();

        // Create channels between stages
        let (read_tx, read_rx) = mpsc::channel::<ReadOutputOwned>(CHANNEL_CAPACITY);
        let (hash_tx, hash_rx) = mpsc::channel::<HashOutputOwned>(CHANNEL_CAPACITY);
        let (result_tx, mut result_rx) = mpsc::channel::<StagedProcessedItem>(CHANNEL_CAPACITY);

        // Spawn read stage
        let read_handle: JoinHandle<Result<(), StorageError>> = {
            let pipeline = Arc::clone(&self);
            tokio::spawn(async move {
                pipeline.run_read_stage(items, read_tx).await
            })
        };

        // Spawn hash stage
        let hash_handle: JoinHandle<Result<(), StorageError>> = {
            let pipeline = Arc::clone(&self);
            tokio::spawn(async move {
                pipeline.run_hash_stage(read_rx, hash_tx).await
            })
        };

        // Spawn upload stage
        let upload_handle: JoinHandle<Result<(), StorageError>> = {
            let pipeline = Arc::clone(&self);
            tokio::spawn(async move {
                pipeline.run_upload_stage(hash_rx, result_tx).await
            })
        };

        // Collect results
        let mut results: Vec<StagedProcessedItem> = Vec::with_capacity(total_items);
        while let Some(item) = result_rx.recv().await {
            results.push(item);
        }

        // Wait for all stages to complete
        read_handle.await.map_err(|e| StorageError::Other {
            message: format!("Read stage join error: {}", e),
        })??;

        hash_handle.await.map_err(|e| StorageError::Other {
            message: format!("Hash stage join error: {}", e),
        })??;

        upload_handle.await.map_err(|e| StorageError::Other {
            message: format!("Upload stage join error: {}", e),
        })??;

        Ok(results)
    }

    /// Get current progress snapshot.
    pub fn progress(&self) -> HashUploadProgress {
        self.progress.snapshot()
    }
}

// Owned versions for sending across channels (no lifetime issues)
struct ReadOutputOwned {
    item: WorkItem,
    data: Option<Bytes>,
    cached_hash: Option<String>,
    skip_entirely: bool,
}

struct HashOutputOwned {
    item: WorkItem,
    data: Option<Bytes>,
    hash: String,
    hash_cached: bool,
    skip_upload: bool,
}

impl<C: ContentAddressedDataCache + Send + Sync + 'static> StagedPipeline<C> {
    /// Run the read stage.
    ///
    /// Reads files from disk, checks hash cache, and sends to hash stage.
    async fn run_read_stage(
        &self,
        items: Vec<WorkItem>,
        tx: mpsc::Sender<ReadOutputOwned>,
    ) -> Result<(), StorageError> {
        use futures::stream::{self, StreamExt};

        let results: Vec<Result<(), StorageError>> = stream::iter(items)
            .map(|item| {
                let tx = tx.clone();
                async move {
                    let output: ReadOutputOwned = self.read_file(item).await?;
                    tx.send(output).await.map_err(|_| StorageError::Other {
                        message: "Read stage channel closed".to_string(),
                    })?;
                    Ok(())
                }
            })
            .buffer_unordered(self.config.read_concurrency)
            .collect()
            .await;

        // Check for errors
        for result in results {
            result?;
        }

        Ok(())
    }

    /// Read a single file.
    async fn read_file(&self, item: WorkItem) -> Result<ReadOutputOwned, StorageError> {
        // Check hash cache first
        let cached_hash: Option<String> = self.get_cached_hash(&item).await;

        // If hash is cached, check if object exists in S3
        if let Some(ref hash) = cached_hash {
            if self.options.use_s3_check_cache {
                if self.data_cache.object_exists(hash, HashAlgorithm::Xxh128).await? {
                    // Can skip entirely - no need to read file
                    self.progress.record_hash_complete(item.size, true);
                    self.progress.record_upload_complete(item.size, true);
                    return Ok(ReadOutputOwned {
                        item,
                        data: None,
                        cached_hash,
                        skip_entirely: true,
                    });
                }
            }
        }

        // Allocate memory permit before reading
        let _permit = self.memory_pool.allocate(item.size).await;

        // Read file
        let path_clone: String = item.path.clone();
        let data: Vec<u8> = tokio::task::spawn_blocking(move || std::fs::read(&path_clone))
            .await
            .map_err(|e| StorageError::Other {
                message: format!("Read task join error: {}", e),
            })?
            .map_err(|e| StorageError::IoError {
                path: item.path.clone(),
                message: e.to_string(),
            })?;

        Ok(ReadOutputOwned {
            item,
            data: Some(Bytes::from(data)),
            cached_hash,
            skip_entirely: false,
        })
    }

    /// Get cached hash if available.
    async fn get_cached_hash(&self, item: &WorkItem) -> Option<String> {
        if !self.options.use_hash_cache || self.options.force_rehash {
            return None;
        }

        if let Some(cache) = &self.hash_cache {
            cache.get(&item.path, item.size, item.mtime).await
        } else {
            None
        }
    }

    /// Run the hash stage.
    ///
    /// Computes hashes for file data and sends to upload stage.
    async fn run_hash_stage(
        &self,
        mut rx: mpsc::Receiver<ReadOutputOwned>,
        tx: mpsc::Sender<HashOutputOwned>,
    ) -> Result<(), StorageError> {
        use futures::stream::{self, StreamExt};

        // Collect items and process concurrently
        let mut pending: Vec<ReadOutputOwned> = Vec::new();

        while let Some(input) = rx.recv().await {
            pending.push(input);

            // Process in batches when we have enough or channel is empty
            if pending.len() >= self.config.hash_concurrency || rx.is_empty() {
                let batch: Vec<ReadOutputOwned> = std::mem::take(&mut pending);
                let results: Vec<Result<HashOutputOwned, StorageError>> = stream::iter(batch)
                    .map(|input| self.hash_data(input))
                    .buffer_unordered(self.config.hash_concurrency)
                    .collect()
                    .await;

                for result in results {
                    let output: HashOutputOwned = result?;
                    tx.send(output).await.map_err(|_| StorageError::Other {
                        message: "Hash stage channel closed".to_string(),
                    })?;
                }
            }
        }

        // Process remaining items
        if !pending.is_empty() {
            let results: Vec<Result<HashOutputOwned, StorageError>> = stream::iter(pending)
                .map(|input| self.hash_data(input))
                .buffer_unordered(self.config.hash_concurrency)
                .collect()
                .await;

            for result in results {
                let output: HashOutputOwned = result?;
                tx.send(output).await.map_err(|_| StorageError::Other {
                    message: "Hash stage channel closed".to_string(),
                })?;
            }
        }

        Ok(())
    }

    /// Hash a single file's data.
    async fn hash_data(&self, input: ReadOutputOwned) -> Result<HashOutputOwned, StorageError> {
        // If skipping entirely, pass through
        if input.skip_entirely {
            return Ok(HashOutputOwned {
                item: input.item,
                data: None,
                hash: input.cached_hash.unwrap_or_default(),
                hash_cached: true,
                skip_upload: true,
            });
        }

        let data: Bytes = input.data.expect("Data should be present if not skipping");

        // Use cached hash if available
        let (hash, hash_cached): (String, bool) = if let Some(h) = input.cached_hash {
            self.progress.record_hash_complete(input.item.size, true);
            (h, true)
        } else {
            // Compute hash
            let data_clone: Bytes = data.clone();
            let hash: String = tokio::task::spawn_blocking(move || {
                let mut hasher: Xxh3Hasher = Xxh3Hasher::new();
                hasher.update(&data_clone);
                hasher.finish_hex()
            })
            .await
            .map_err(|e| StorageError::Other {
                message: format!("Hash task join error: {}", e),
            })?;

            self.progress.record_hash_complete(input.item.size, false);

            // Update hash cache
            if let Some(cache) = &self.hash_cache {
                cache
                    .put(&input.item.path, input.item.size, input.item.mtime, hash.clone())
                    .await;
            }

            (hash, false)
        };

        // Check if object already exists
        let exists: bool = self.data_cache.object_exists(&hash, HashAlgorithm::Xxh128).await?;

        Ok(HashOutputOwned {
            item: input.item,
            data: Some(data),
            hash,
            hash_cached,
            skip_upload: exists,
        })
    }

    /// Run the upload stage.
    ///
    /// Uploads data to storage backend with deduplication.
    async fn run_upload_stage(
        &self,
        mut rx: mpsc::Receiver<HashOutputOwned>,
        tx: mpsc::Sender<StagedProcessedItem>,
    ) -> Result<(), StorageError> {
        use futures::stream::{self, StreamExt};

        let mut pending: Vec<HashOutputOwned> = Vec::new();

        while let Some(input) = rx.recv().await {
            pending.push(input);

            // Process in batches
            if pending.len() >= self.config.upload_concurrency || rx.is_empty() {
                let batch: Vec<HashOutputOwned> = std::mem::take(&mut pending);
                let results: Vec<Result<StagedProcessedItem, StorageError>> = stream::iter(batch)
                    .map(|input| self.upload_data(input))
                    .buffer_unordered(self.config.upload_concurrency)
                    .collect()
                    .await;

                for result in results {
                    let output: StagedProcessedItem = result?;
                    tx.send(output).await.map_err(|_| StorageError::Other {
                        message: "Upload stage channel closed".to_string(),
                    })?;
                }
            }
        }

        // Process remaining items
        if !pending.is_empty() {
            let results: Vec<Result<StagedProcessedItem, StorageError>> = stream::iter(pending)
                .map(|input| self.upload_data(input))
                .buffer_unordered(self.config.upload_concurrency)
                .collect()
                .await;

            for result in results {
                let output: StagedProcessedItem = result?;
                tx.send(output).await.map_err(|_| StorageError::Other {
                    message: "Upload stage channel closed".to_string(),
                })?;
            }
        }

        Ok(())
    }

    /// Upload a single file's data.
    async fn upload_data(&self, input: HashOutputOwned) -> Result<StagedProcessedItem, StorageError> {
        let upload_skipped: bool = if input.skip_upload {
            self.progress.record_upload_complete(input.item.size, true);
            true
        } else {
            // Upload with deduplication
            let skipped: bool = self.upload_with_dedup(&input.hash, input.data.as_ref()).await?;
            self.progress.record_upload_complete(input.item.size, skipped);
            skipped
        };

        // Memory permit is released when input goes out of scope

        Ok(StagedProcessedItem {
            path: input.item.path,
            hash: input.hash,
            size: input.item.size,
            upload_skipped,
            hash_cached: input.hash_cached,
        })
    }

    /// Upload data with deduplication.
    async fn upload_with_dedup(
        &self,
        hash: &str,
        data: Option<&Bytes>,
    ) -> Result<bool, StorageError> {
        let data: &Bytes = match data {
            Some(d) => d,
            None => return Ok(true), // No data means skip
        };

        // Check if already exists
        if self.data_cache.object_exists(hash, HashAlgorithm::Xxh128).await? {
            return Ok(true);
        }

        // Register upload intent for deduplication
        match self.deduplicator.register(hash) {
            UploadIntent::Proceed => {
                // We're the first uploader
                let result = self.data_cache.put_object(hash, HashAlgorithm::Xxh128, data).await;

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
                        self.data_cache.put_object(hash, HashAlgorithm::Xxh128, data).await?;
                        Ok(false)
                    }
                    Err(_) => {
                        // Channel closed, try ourselves
                        self.data_cache.put_object(hash, HashAlgorithm::Xxh128, data).await?;
                        Ok(false)
                    }
                }
            }
        }
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
    async fn test_staged_pipeline_single_file() {
        let dir = TempDir::new().unwrap();
        let path: String = create_test_file(&dir, "test.txt", b"hello world");

        let cache = Arc::new(MockDataCache::new());
        let options = HashUploadOptions::default();
        let pipeline = Arc::new(StagedPipeline::new(
            cache,
            None,
            options,
            1,
            11,
        ));

        let items: Vec<WorkItem> = vec![WorkItem {
            path: path.clone(),
            size: 11,
            mtime: 0,
        }];

        let results: Vec<StagedProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, path);
        assert!(!results[0].hash.is_empty());
        assert!(!results[0].upload_skipped);
    }

    #[tokio::test]
    async fn test_staged_pipeline_multiple_files() {
        let dir = TempDir::new().unwrap();
        let path1: String = create_test_file(&dir, "file1.txt", b"content one");
        let path2: String = create_test_file(&dir, "file2.txt", b"content two");
        let path3: String = create_test_file(&dir, "file3.txt", b"content three");

        let cache = Arc::new(MockDataCache::new());
        let options = HashUploadOptions::default();
        let pipeline = Arc::new(StagedPipeline::new(
            cache,
            None,
            options,
            3,
            35,
        ));

        let items: Vec<WorkItem> = vec![
            WorkItem { path: path1, size: 11, mtime: 0 },
            WorkItem { path: path2, size: 11, mtime: 0 },
            WorkItem { path: path3, size: 13, mtime: 0 },
        ];

        let results: Vec<StagedProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 3);
        // All should have unique hashes
        let hashes: Vec<&String> = results.iter().map(|r| &r.hash).collect();
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
    }

    #[tokio::test]
    async fn test_staged_pipeline_deduplication() {
        let dir = TempDir::new().unwrap();
        // Same content, different names
        let path1: String = create_test_file(&dir, "dup1.txt", b"duplicate content");
        let path2: String = create_test_file(&dir, "dup2.txt", b"duplicate content");

        let cache = Arc::new(MockDataCache::new());
        let options = HashUploadOptions::default();
        let pipeline = Arc::new(StagedPipeline::new(
            cache,
            None,
            options,
            2,
            34,
        ));

        let items: Vec<WorkItem> = vec![
            WorkItem { path: path1, size: 17, mtime: 0 },
            WorkItem { path: path2, size: 17, mtime: 0 },
        ];

        let results: Vec<StagedProcessedItem> = pipeline.process(items).await.unwrap();

        assert_eq!(results.len(), 2);
        // Same hash
        assert_eq!(results[0].hash, results[1].hash);
    }
}
