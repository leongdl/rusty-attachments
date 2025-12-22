//! Error types for CRT storage operations.

use rusty_attachments_storage::StorageError;
use thiserror::Error;

/// Errors specific to the CRT storage client.
#[derive(Error, Debug)]
pub enum CrtError {
    /// AWS SDK error.
    #[error("AWS SDK error: {message}")]
    SdkError { message: String, retryable: bool },

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<CrtError> for StorageError {
    fn from(err: CrtError) -> Self {
        match err {
            CrtError::SdkError { message, retryable } => {
                StorageError::NetworkError { message, retryable }
            }
            CrtError::ConfigError(message) => StorageError::InvalidConfig { message },
            CrtError::IoError(e) => StorageError::IoError {
                path: String::new(),
                message: e.to_string(),
            },
        }
    }
}
