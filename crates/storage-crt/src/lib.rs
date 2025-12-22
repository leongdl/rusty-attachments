//! AWS SDK S3 backend for rusty-attachments storage.
//!
//! This crate provides a `StorageClient` implementation using the AWS SDK for Rust.
//! It supports all S3 operations required for CAS-based file transfers.
//!
//! # Example
//!
//! ```ignore
//! use rusty_attachments_storage_crt::CrtStorageClient;
//! use rusty_attachments_storage::{StorageSettings, UploadOrchestrator, S3Location};
//!
//! let settings = StorageSettings::default();
//! let client = CrtStorageClient::new(settings).await?;
//!
//! let location = S3Location::new("my-bucket", "DeadlineCloud", "Data", "Manifests");
//! let orchestrator = UploadOrchestrator::new(&client, location);
//! ```

mod client;
mod error;

pub use client::CrtStorageClient;
pub use error::CrtError;
