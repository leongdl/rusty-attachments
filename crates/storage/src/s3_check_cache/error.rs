//! S3 check cache error types.

use thiserror::Error;

/// Errors that can occur during S3 check cache operations.
#[derive(Error, Debug)]
pub enum S3CheckCacheError {
    /// SQLite database error.
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// DynamoDB error.
    #[error("DynamoDB error: {0}")]
    DynamoDb(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid cache entry.
    #[error("Invalid cache entry: {0}")]
    InvalidEntry(String),
}
