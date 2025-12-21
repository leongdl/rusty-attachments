//! Shared error types used across rusty-attachments crates.

use thiserror::Error;

/// Path-related errors shared across crates.
#[derive(Debug, Error, Clone)]
pub enum PathError {
    /// Path is outside the expected root directory.
    #[error("Path is outside root: {path} not in {root}")]
    PathOutsideRoot {
        /// The path that was checked.
        path: String,
        /// The root directory it should be within.
        root: String,
    },

    /// Path is invalid or malformed.
    #[error("Invalid path: {path}")]
    InvalidPath {
        /// The invalid path.
        path: String,
    },

    /// Path does not match expected value.
    #[error("Path mismatch - expected {expected}, got {actual}")]
    PathMismatch {
        /// Expected path.
        expected: String,
        /// Actual path.
        actual: String,
    },

    /// IO error occurred while accessing path.
    #[error("IO error at {path}: {message}")]
    IoError {
        /// Path where error occurred.
        path: String,
        /// Error message.
        message: String,
    },
}

impl PathError {
    /// Create an IoError from std::io::Error.
    ///
    /// # Arguments
    /// * `path` - Path where the error occurred
    /// * `err` - The underlying IO error
    pub fn from_io(path: impl Into<String>, err: std::io::Error) -> Self {
        Self::IoError {
            path: path.into(),
            message: err.to_string(),
        }
    }
}

/// Version compatibility error for features not supported in older manifest versions.
///
/// Per steering guidelines, unsupported versions should fail explicitly
/// rather than silently degrading.
#[derive(Debug, Error, Clone)]
#[error("{feature} requires manifest version {min_version} or later")]
pub struct VersionNotCompatibleError {
    /// The feature that was requested.
    pub feature: &'static str,
    /// Minimum version required for this feature.
    pub min_version: &'static str,
}

impl VersionNotCompatibleError {
    /// Create a new version compatibility error.
    ///
    /// # Arguments
    /// * `feature` - Name of the feature that requires a newer version
    /// * `min_version` - Minimum manifest version required
    pub fn new(feature: &'static str, min_version: &'static str) -> Self {
        Self {
            feature,
            min_version,
        }
    }
}
