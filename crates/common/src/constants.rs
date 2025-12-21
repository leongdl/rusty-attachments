//! Shared constants used across rusty-attachments crates.

/// Chunk size for manifest v2 format (256MB).
/// Files larger than this are split into chunks with individual hashes.
pub const CHUNK_SIZE_V2: u64 = 256 * 1024 * 1024;

/// No chunking (for v1 format or when chunking is disabled).
pub const CHUNK_SIZE_NONE: u64 = 0;

/// Default threshold for small vs large file classification (80MB).
/// Small files benefit from parallel object uploads.
/// Large files benefit from serial processing with parallel multipart.
pub const SMALL_FILE_THRESHOLD: u64 = 80 * 1024 * 1024;

/// Windows MAX_PATH limit.
pub const WINDOWS_MAX_PATH: usize = 260;

/// Default hash cache TTL in days.
pub const DEFAULT_HASH_CACHE_TTL_DAYS: u32 = 30;

/// Default stat cache capacity (number of entries).
pub const DEFAULT_STAT_CACHE_CAPACITY: usize = 1024;
