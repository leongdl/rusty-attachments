//! Storage abstraction for Job Attachments S3 operations.
//!
//! This crate provides a platform-agnostic interface for uploading and downloading
//! files to/from S3 Content-Addressable Storage (CAS). It supports multiple backends:
//!
//! - **CRT Backend** - Native Rust using AWS CRT for high-performance operations
//! - **WASM/JS SDK Backend** - WebAssembly using AWS SDK for JavaScript v3

mod cas;
mod error;
mod traits;
mod types;

pub use cas::{
    expected_chunk_count, generate_chunks, needs_chunking, upload_strategy, ChunkInfo,
    DownloadStrategy, UploadStrategy,
};
pub use error::{StorageError, TransferError};
pub use traits::{ObjectInfo, ProgressCallback, StorageClient};
pub use types::{
    AwsCredentials, CasDownloadRequest, CasUploadRequest, ConflictResolution, DataDestination,
    DataSource, DownloadResult, OperationType, RetrySettings, S3Location, StorageSettings,
    TransferProgress, TransferStatistics, UploadResult, CHUNK_SIZE_NONE, CHUNK_SIZE_V2,
};
