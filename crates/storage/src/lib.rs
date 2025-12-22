//! Storage abstraction for Job Attachments S3 operations.
//!
//! This crate provides a platform-agnostic interface for uploading and downloading
//! files to/from S3 Content-Addressable Storage (CAS). It supports multiple backends:
//!
//! - **CRT Backend** - Native Rust using AWS CRT for high-performance operations
//! - **WASM/JS SDK Backend** - WebAssembly using AWS SDK for JavaScript v3
//!
//! # Caching
//!
//! The crate provides two caching layers:
//!
//! - **Hash Cache** - Caches file hashes to avoid re-hashing unchanged files
//! - **S3 Check Cache** - Caches S3 existence checks to avoid redundant HEAD requests
//!
//! # Manifest Storage
//!
//! The `manifest_storage` module provides functions for uploading and downloading
//! manifest files to/from S3 with proper metadata and content types.

mod cas;
mod error;
pub mod hash_cache;
pub mod manifest_storage;
pub mod s3_check_cache;
mod traits;
mod types;

pub use cas::{
    expected_chunk_count, generate_chunks, needs_chunking, upload_strategy, ChunkInfo,
    DownloadStrategy, UploadStrategy,
};
pub use error::{StorageError, TransferError};
pub use hash_cache::{
    HashCache, HashCacheBackend, HashCacheEntry, HashCacheError, HashCacheKey, SqliteHashCache,
    DEFAULT_HASH_CACHE_TTL_DAYS,
};
pub use manifest_storage::{
    build_partial_input_manifest_prefix, compute_manifest_name_hash, compute_root_path_hash,
    float_to_iso_datetime_string, format_input_manifest_s3_key,
    format_step_output_manifest_s3_key, format_task_output_manifest_s3_key, generate_random_guid,
    get_manifest_content_type, upload_input_manifest, upload_step_output_manifest,
    upload_task_output_manifest, ManifestDownloadMetadata, ManifestLocation, ManifestS3Metadata,
    ManifestUploadResult, StepOutputManifestPath, TaskOutputManifestPath,
    CONTENT_TYPE_V2023_03_03, CONTENT_TYPE_V2025_12_04_BETA, CONTENT_TYPE_V2025_12_04_BETA_DIFF,
};
pub use s3_check_cache::{
    S3CheckCache, S3CheckCacheBackend, S3CheckCacheEntry, S3CheckCacheError, S3CheckCacheKey,
    SqliteS3CheckCache,
};
pub use traits::{ObjectInfo, ProgressCallback, StorageClient};
pub use types::{
    AwsCredentials, CasDownloadRequest, CasUploadRequest, ConflictResolution, DataDestination,
    DataSource, DownloadResult, OperationType, RetrySettings, S3Location, StorageSettings,
    TransferProgress, TransferStatistics, UploadResult, CHUNK_SIZE_NONE, CHUNK_SIZE_V2,
};
