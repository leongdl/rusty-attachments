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

mod cas;
mod error;
pub mod hash_cache;
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
