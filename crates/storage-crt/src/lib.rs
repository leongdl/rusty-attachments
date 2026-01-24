//! AWS SDK S3 backend for rusty-attachments storage.
//!
//! This crate provides `StorageClient` implementations using the AWS SDK for Rust.
//! It supports all S3 operations required for CAS-based file transfers.
//!
//! # Clients
//!
//! - `CrtStorageClient`: Standard S3 client using simple put_object/get_object
//! - `TransferManagerClient`: High-performance client using AWS S3 Transfer Manager
//!   with automatic multipart uploads and parallel byte-range downloads
//!
//! # Example
//!
//! ```ignore
//! use rusty_attachments_storage_crt::{CrtStorageClient, TransferManagerClient};
//! use rusty_attachments_storage::{StorageSettings, UploadOrchestrator, S3Location};
//!
//! let settings = StorageSettings::default();
//!
//! // Standard client
//! let client = CrtStorageClient::new(settings.clone()).await?;
//!
//! // High-performance transfer manager client
//! let tm_client = TransferManagerClient::new(settings).await?;
//! ```

mod client;
mod error;
mod transfer_manager;

pub use client::CrtStorageClient;
pub use error::CrtError;
pub use transfer_manager::TransferManagerClient;
