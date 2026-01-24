//! Pipelined hash+upload operations.
//!
//! This module provides a combined hash+upload operation that reads files
//! once into memory, computes the hash, and uploads from the same buffer.
//!
//! # Benefits over sequential approach
//!
//! - Single file read (vs. read for hash, then read for upload)
//! - Concurrent hash+upload across different files
//! - Hash deduplication (duplicate files uploaded once)
//! - Memory backpressure (bounded memory usage)
//!
//! # Example
//!
//! ```ignore
//! use rusty_attachments_storage::hash_upload::{hash_upload_abs_manifest, HashUploadOptions};
//!
//! let result = hash_upload_abs_manifest(
//!     manifest,
//!     "/source/root",
//!     &data_cache,
//!     Some(&hash_cache),
//!     HashUploadOptions::default(),
//! ).await?;
//! ```
//!
//! # Preserving Existing APIs
//!
//! This module is an optimization, not a replacement. The existing standalone
//! hash and upload functions remain available:
//!
//! - `rusty_attachments_common::hash_file()` - Hash a single file
//! - `FileSystemScanner::snapshot()` - Hash all files in directory
//! - `UploadOrchestrator::upload_manifest_contents()` - Upload all manifest files

mod deduplication;
mod memory_pool;
mod options;
mod pipeline;
mod progress;
mod staged_pipeline;

pub use options::HashUploadOptions;
pub use pipeline::{ProcessedItem, WorkItem};
pub use progress::{HashUploadProgress, ProgressTracker};
pub use staged_pipeline::{StagedPipeline, StagedPipelineConfig, StagedProcessedItem};

use std::collections::HashMap;

use rusty_attachments_model::{v2023_03_03, v2025_12, Manifest};

use crate::error::StorageError;
use crate::hash_cache::HashCache;
use crate::traits::ContentAddressedDataCache;
use crate::types::TransferStatistics;

use pipeline::HashUploadPipeline;

/// Result of hash_upload_abs_manifest operation.
#[derive(Debug)]
pub struct HashUploadResult {
    /// Updated manifest with all hashes filled in.
    pub manifest: Manifest,
    /// Transfer statistics.
    pub statistics: TransferStatistics,
    /// Detailed progress at completion.
    pub progress: HashUploadProgress,
}

/// Hash and upload manifest contents in a pipelined manner.
///
/// This operation combines hashing and uploading into a single pass over the data,
/// avoiding the need to read files twice.
///
/// # Arguments
/// * `manifest` - Manifest with files to process (hashes may be empty)
/// * `source_root` - Root directory where files are located
/// * `data_cache` - Content-addressable storage destination
/// * `hash_cache` - Optional hash cache for efficiency
/// * `options` - Pipeline configuration options
///
/// # Returns
/// Result containing updated manifest and statistics.
///
/// # Example
///
/// ```ignore
/// let result = hash_upload_abs_manifest(
///     manifest,
///     "/path/to/files",
///     &s3_data_cache,
///     Some(&hash_cache),
///     HashUploadOptions::default(),
/// ).await?;
///
/// println!("Uploaded {} files", result.statistics.files_transferred);
/// ```
pub async fn hash_upload_abs_manifest<C: ContentAddressedDataCache + 'static>(
    manifest: Manifest,
    source_root: &str,
    data_cache: &C,
    hash_cache: Option<&HashCache>,
    options: HashUploadOptions,
) -> Result<HashUploadResult, StorageError> {
    // Collect work items from manifest
    let (work_items, total_bytes): (Vec<WorkItem>, u64) =
        collect_work_items(&manifest, source_root)?;
    let total_files: u64 = work_items.len() as u64;

    // Create pipeline
    let pipeline = HashUploadPipeline::new(data_cache, hash_cache, options, total_files, total_bytes);

    // Process all items
    let processed: Vec<ProcessedItem> = pipeline.process(work_items).await?;

    // Build result manifest with hashes
    let result_manifest: Manifest = build_result_manifest(manifest, source_root, &processed)?;

    // Build statistics
    let progress: HashUploadProgress = pipeline.progress();
    let statistics = TransferStatistics {
        files_processed: progress.hashed_files + progress.hash_skipped_files,
        files_transferred: progress.uploaded_files,
        files_skipped: progress.upload_skipped_files,
        bytes_transferred: progress.uploaded_bytes,
        bytes_skipped: progress.upload_skipped_bytes,
        errors: vec![],
    };

    Ok(HashUploadResult {
        manifest: result_manifest,
        statistics,
        progress,
    })
}

/// Hash and upload manifest contents using 3-stage pipeline.
///
/// This version uses separate read, hash, and upload stages that run
/// concurrently, connected by async channels. This allows better utilization
/// of the memory budget and higher throughput.
///
/// # Arguments
/// * `manifest` - Manifest with files to process (hashes may be empty)
/// * `source_root` - Root directory where files are located
/// * `data_cache` - Content-addressable storage destination (must be Arc-wrapped)
/// * `hash_cache` - Optional hash cache for efficiency (must be Arc-wrapped)
/// * `options` - Pipeline configuration options
///
/// # Returns
/// Result containing updated manifest and statistics.
///
/// # Example
///
/// ```ignore
/// let result = hash_upload_abs_manifest_staged(
///     manifest,
///     "/path/to/files",
///     Arc::new(s3_data_cache),
///     Some(Arc::new(hash_cache)),
///     HashUploadOptions::default(),
/// ).await?;
///
/// println!("Uploaded {} files", result.statistics.files_transferred);
/// ```
pub async fn hash_upload_abs_manifest_staged<C: ContentAddressedDataCache + Send + Sync + 'static>(
    manifest: Manifest,
    source_root: &str,
    data_cache: std::sync::Arc<C>,
    hash_cache: Option<std::sync::Arc<HashCache>>,
    options: HashUploadOptions,
) -> Result<HashUploadResult, StorageError> {
    // Collect work items from manifest
    let (work_items, total_bytes): (Vec<WorkItem>, u64) =
        collect_work_items(&manifest, source_root)?;
    let total_files: u64 = work_items.len() as u64;

    // Create staged pipeline
    let pipeline = std::sync::Arc::new(StagedPipeline::new(
        data_cache,
        hash_cache,
        options,
        total_files,
        total_bytes,
    ));

    // Process all items through 3-stage pipeline
    let processed: Vec<StagedProcessedItem> = pipeline.clone().process(work_items).await?;

    // Build result manifest with hashes
    let result_manifest: Manifest = build_result_manifest_staged(manifest, source_root, &processed)?;

    // Build statistics
    let progress: HashUploadProgress = pipeline.progress();
    let statistics = TransferStatistics {
        files_processed: progress.hashed_files + progress.hash_skipped_files,
        files_transferred: progress.uploaded_files,
        files_skipped: progress.upload_skipped_files,
        bytes_transferred: progress.uploaded_bytes,
        bytes_skipped: progress.upload_skipped_bytes,
        errors: vec![],
    };

    Ok(HashUploadResult {
        manifest: result_manifest,
        statistics,
        progress,
    })
}

/// Build result manifest with hashes from staged processed items.
///
/// # Arguments
/// * `original` - Original manifest
/// * `source_root` - Root directory for files
/// * `processed` - Processed items with computed hashes
///
/// # Returns
/// Updated manifest with hashes filled in.
fn build_result_manifest_staged(
    original: Manifest,
    source_root: &str,
    processed: &[StagedProcessedItem],
) -> Result<Manifest, StorageError> {
    // Create a map of full path -> hash for quick lookup
    let hash_map: HashMap<&str, &str> = processed
        .iter()
        .map(|p| (p.path.as_str(), p.hash.as_str()))
        .collect();

    match original {
        Manifest::V2023_03_03(m) => {
            let paths: Vec<v2023_03_03::ManifestPath> = m
                .paths
                .into_iter()
                .map(|mut p| {
                    let full_path: String = format!("{}/{}", source_root, p.path);
                    if let Some(hash) = hash_map.get(full_path.as_str()) {
                        p.hash = hash.to_string();
                    }
                    p
                })
                .collect();

            Ok(Manifest::V2023_03_03(v2023_03_03::AssetManifest::new(
                paths,
            )))
        }
        Manifest::V2025_12(m) => {
            let files: Vec<v2025_12::ManifestFilePath> = m
                .files
                .into_iter()
                .map(|mut f| {
                    if f.symlink_target.is_none() && !f.deleted {
                        let full_path: String = format!("{}/{}", source_root, f.path);
                        if let Some(hash) = hash_map.get(full_path.as_str()) {
                            f.hash = Some(hash.to_string());
                        }
                    }
                    f
                })
                .collect();

            Ok(Manifest::V2025_12(v2025_12::AssetManifest::with_spec(
                m.dirs,
                files,
                m.spec_version,
                m.parent_manifest_hash,
            )))
        }
    }
}

/// Collect work items from manifest.
///
/// # Arguments
/// * `manifest` - Source manifest
/// * `source_root` - Root directory for files
///
/// # Returns
/// Tuple of (work items, total bytes).
fn collect_work_items(
    manifest: &Manifest,
    source_root: &str,
) -> Result<(Vec<WorkItem>, u64), StorageError> {
    let mut items: Vec<WorkItem> = Vec::new();
    let mut total_bytes: u64 = 0;

    match manifest {
        Manifest::V2023_03_03(m) => {
            for path in &m.paths {
                let full_path: String = format!("{}/{}", source_root, path.path);
                items.push(WorkItem {
                    path: full_path,
                    size: path.size,
                    mtime: path.mtime,
                });
                total_bytes += path.size;
            }
        }
        Manifest::V2025_12(m) => {
            for file in &m.files {
                // Skip symlinks and deleted entries
                if file.symlink_target.is_some() || file.deleted {
                    continue;
                }

                let size: u64 = file.size.unwrap_or(0);
                let full_path: String = format!("{}/{}", source_root, file.path);
                items.push(WorkItem {
                    path: full_path,
                    size,
                    mtime: file.mtime.unwrap_or(0),
                });
                total_bytes += size;
            }
        }
    }

    Ok((items, total_bytes))
}

/// Build result manifest with hashes from processed items.
///
/// # Arguments
/// * `original` - Original manifest
/// * `source_root` - Root directory for files
/// * `processed` - Processed items with computed hashes
///
/// # Returns
/// Updated manifest with hashes filled in.
fn build_result_manifest(
    original: Manifest,
    source_root: &str,
    processed: &[ProcessedItem],
) -> Result<Manifest, StorageError> {
    // Create a map of full path -> hash for quick lookup
    let hash_map: HashMap<&str, &str> = processed
        .iter()
        .map(|p| (p.path.as_str(), p.hash.as_str()))
        .collect();

    match original {
        Manifest::V2023_03_03(m) => {
            let paths: Vec<v2023_03_03::ManifestPath> = m
                .paths
                .into_iter()
                .map(|mut p| {
                    let full_path: String = format!("{}/{}", source_root, p.path);
                    if let Some(hash) = hash_map.get(full_path.as_str()) {
                        p.hash = hash.to_string();
                    }
                    p
                })
                .collect();

            Ok(Manifest::V2023_03_03(v2023_03_03::AssetManifest::new(
                paths,
            )))
        }
        Manifest::V2025_12(m) => {
            let files: Vec<v2025_12::ManifestFilePath> = m
                .files
                .into_iter()
                .map(|mut f| {
                    if f.symlink_target.is_none() && !f.deleted {
                        let full_path: String = format!("{}/{}", source_root, f.path);
                        if let Some(hash) = hash_map.get(full_path.as_str()) {
                            f.hash = Some(hash.to_string());
                        }
                    }
                    f
                })
                .collect();

            Ok(Manifest::V2025_12(v2025_12::AssetManifest::with_spec(
                m.dirs,
                files,
                m.spec_version,
                m.parent_manifest_hash,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
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
            _progress: Option<&crate::traits::ProgressCallback>,
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
            _progress: Option<&crate::traits::ProgressCallback>,
        ) -> Result<(), StorageError> {
            let data: Vec<u8> = self.get_object(hash, algorithm).await?;
            std::fs::write(file_path, data).map_err(|e| StorageError::IoError {
                path: file_path.display().to_string(),
                message: e.to_string(),
            })
        }
    }

    fn create_test_files(dir: &TempDir) -> (String, String, String) {
        let file1 = dir.path().join("file1.txt");
        let file2 = dir.path().join("file2.txt");
        let file3 = dir.path().join("subdir/file3.txt");

        std::fs::write(&file1, b"content one").unwrap();
        std::fs::write(&file2, b"content two").unwrap();
        std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
        std::fs::write(&file3, b"content three").unwrap();

        (
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "subdir/file3.txt".to_string(),
        )
    }

    #[tokio::test]
    async fn test_hash_upload_v2023_manifest() {
        let dir = TempDir::new().unwrap();
        let (path1, path2, path3) = create_test_files(&dir);

        let paths: Vec<v2023_03_03::ManifestPath> = vec![
            v2023_03_03::ManifestPath {
                path: path1,
                hash: String::new(),
                size: 11,
                mtime: 0,
            },
            v2023_03_03::ManifestPath {
                path: path2,
                hash: String::new(),
                size: 11,
                mtime: 0,
            },
            v2023_03_03::ManifestPath {
                path: path3,
                hash: String::new(),
                size: 13,
                mtime: 0,
            },
        ];

        let manifest = Manifest::V2023_03_03(v2023_03_03::AssetManifest::new(paths));
        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();

        let result: HashUploadResult = hash_upload_abs_manifest(
            manifest,
            dir.path().to_str().unwrap(),
            &cache,
            None,
            options,
        )
        .await
        .unwrap();

        // Check manifest has hashes
        if let Manifest::V2023_03_03(m) = &result.manifest {
            assert_eq!(m.paths.len(), 3);
            for path in &m.paths {
                assert!(!path.hash.is_empty(), "Hash should be filled in");
            }
        } else {
            panic!("Expected V2023_03_03 manifest");
        }

        // Check statistics
        assert_eq!(result.statistics.files_processed, 3);
        assert_eq!(result.statistics.files_transferred, 3);
    }

    #[tokio::test]
    async fn test_hash_upload_v2025_manifest() {
        let dir = TempDir::new().unwrap();
        let (path1, path2, _) = create_test_files(&dir);

        let files: Vec<v2025_12::ManifestFilePath> = vec![
            v2025_12::ManifestFilePath::file(path1, "", 11, 0),
            v2025_12::ManifestFilePath::file(path2, "", 11, 0),
        ];

        let manifest = Manifest::V2025_12(v2025_12::AssetManifest::snapshot(vec![], files));
        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();

        let result: HashUploadResult = hash_upload_abs_manifest(
            manifest,
            dir.path().to_str().unwrap(),
            &cache,
            None,
            options,
        )
        .await
        .unwrap();

        // Check manifest has hashes
        if let Manifest::V2025_12(m) = &result.manifest {
            assert_eq!(m.files.len(), 2);
            for file in &m.files {
                assert!(file.hash.is_some(), "Hash should be filled in");
                assert!(!file.hash.as_ref().unwrap().is_empty());
            }
        } else {
            panic!("Expected V2025_12 manifest");
        }
    }

    #[tokio::test]
    async fn test_hash_upload_skips_symlinks() {
        let dir = TempDir::new().unwrap();
        let (path1, _, _) = create_test_files(&dir);

        let files: Vec<v2025_12::ManifestFilePath> = vec![
            v2025_12::ManifestFilePath::file(path1, "", 11, 0),
            v2025_12::ManifestFilePath::symlink("link.txt".to_string(), "target.txt".to_string()),
        ];

        let manifest = Manifest::V2025_12(v2025_12::AssetManifest::snapshot(vec![], files));
        let cache = MockDataCache::new();
        let options = HashUploadOptions::default();

        let result: HashUploadResult = hash_upload_abs_manifest(
            manifest,
            dir.path().to_str().unwrap(),
            &cache,
            None,
            options,
        )
        .await
        .unwrap();

        // Only 1 file should be processed (symlink skipped)
        assert_eq!(result.statistics.files_processed, 1);
    }

    #[tokio::test]
    async fn test_collect_work_items_v2023() {
        let paths: Vec<v2023_03_03::ManifestPath> = vec![
            v2023_03_03::ManifestPath {
                path: "file1.txt".to_string(),
                hash: "abc".to_string(),
                size: 100,
                mtime: 1000,
            },
            v2023_03_03::ManifestPath {
                path: "file2.txt".to_string(),
                hash: "def".to_string(),
                size: 200,
                mtime: 2000,
            },
        ];

        let manifest = Manifest::V2023_03_03(v2023_03_03::AssetManifest::new(paths));
        let (items, total): (Vec<WorkItem>, u64) =
            collect_work_items(&manifest, "/root").unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, "/root/file1.txt");
        assert_eq!(items[0].size, 100);
        assert_eq!(items[1].path, "/root/file2.txt");
        assert_eq!(items[1].size, 200);
        assert_eq!(total, 300);
    }
}
