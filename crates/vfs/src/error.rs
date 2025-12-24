//! Error types for the VFS crate.

use std::fmt;

/// Errors that can occur during VFS operations.
#[derive(Debug)]
pub enum VfsError {
    /// Inode not found.
    InodeNotFound(u64),

    /// Not a directory.
    NotADirectory(u64),

    /// Content retrieval failed.
    ContentRetrievalFailed {
        hash: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Hash mismatch after retrieval.
    HashMismatch { expected: String, actual: String },

    /// Mount operation failed.
    MountFailed(String),
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::InodeNotFound(id) => write!(f, "Inode not found: {}", id),
            VfsError::NotADirectory(id) => write!(f, "Not a directory: {}", id),
            VfsError::ContentRetrievalFailed { hash, source } => {
                write!(f, "Content retrieval failed for hash {}: {}", hash, source)
            }
            VfsError::HashMismatch { expected, actual } => {
                write!(f, "Hash mismatch: expected {}, got {}", expected, actual)
            }
            VfsError::MountFailed(msg) => write!(f, "Mount failed: {}", msg),
        }
    }
}

impl std::error::Error for VfsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VfsError::ContentRetrievalFailed { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
