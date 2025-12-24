//! FUSE-based virtual filesystem for Deadline Cloud job attachments.
//!
//! This crate provides a read-only FUSE filesystem that mounts Deadline Cloud
//! job attachment manifests. Files appear as local files but content is fetched
//! on-demand from S3 CAS (Content-Addressable Storage).
//!
//! # Architecture
//!
//! ```text
//! Layer 3: FUSE Interface (fuser::Filesystem impl)
//! Layer 2: VFS Operations (lookup, read, readdir)
//! Layer 1: Primitives (INodeManager, FileStore, MemoryPool)
//! ```

pub mod error;
pub mod memory_pool;
pub mod options;

pub use error::VfsError;
pub use memory_pool::{
    BlockContentProvider, BlockHandle, BlockKey, MemoryPool, MemoryPoolConfig, MemoryPoolError,
    MemoryPoolStats,
};
pub use options::{
    KernelCacheOptions, PrefetchStrategy, ReadAheadOptions, TimeoutOptions, VfsOptions,
};
