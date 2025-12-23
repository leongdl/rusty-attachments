# Storage Design Summary

**Full doc:** `design/storage-design.md`  
**Status:** ✅ IMPLEMENTED in `crates/storage/` and `crates/storage-crt/`

## Purpose
Platform-agnostic storage abstraction for S3 CAS operations with CRT and WASM backends.

## Key Types

### Configuration
- `StorageSettings`: region, credentials, expected_bucket_owner, chunk_size, retry settings
- `S3Location`: bucket, root_prefix, cas_prefix, manifest_prefix
- `UploadOptions` / `DownloadOptions`: concurrency, thresholds, verification flags

### Transfer Types
- `CasUploadRequest` / `CasDownloadRequest`: hash, algorithm, size, source/destination
- `DataSource`: FilePath, FileRange, Bytes
- `DataDestination`: FilePath, FileOffset, Memory
- `TransferProgress`: status, operation, current_key, bytes, overall progress

### Results
- `UploadResult` / `DownloadResult`: hash, bytes_transferred, was_uploaded/data
- `TransferStatistics`: files processed/transferred/skipped, bytes, errors
- `SummaryStatistics`: totals, time, transfer_rate, status

## Core Trait
```rust
#[async_trait]
pub trait StorageClient: Send + Sync {
    fn expected_bucket_owner(&self) -> Option<&str>;
    async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<u64>, StorageError>;
    async fn put_object(...) -> Result<(), StorageError>;
    async fn put_object_from_file(...) -> Result<(), StorageError>;
    async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn get_object_to_file(...) -> Result<(), StorageError>;
    async fn list_objects(&self, bucket: &str, prefix: &str) -> Result<Vec<ObjectInfo>, StorageError>;
}
```

## Orchestrators
- `UploadOrchestrator`: upload_manifest_contents(), parallel small files, CRT multipart for large
- `DownloadOrchestrator`: download_manifest_contents(), conflict resolution, post-download verification

## Conflict Resolution
- `Skip`: Don't download if exists
- `Overwrite`: Replace existing
- `CreateCopy`: Generate "file (1).ext" style names

## When to Read Full Doc
- Implementing new storage backend
- Modifying upload/download logic
- Understanding progress reporting
- Adding new transfer options
