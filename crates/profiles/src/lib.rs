//! Storage profiles and path grouping for Job Attachments.
//!
//! This crate provides storage profile management and path grouping logic:
//!
//! - **Storage Profiles** - Define how files are organized based on location (LOCAL vs SHARED)
//! - **Path Grouping** - Group files by asset root respecting storage profile locations
//!
//! # Storage Profile Types
//!
//! - `LOCAL` - Files uploaded with the job
//! - `SHARED` - Files on shared storage accessible to workers (skip upload)

mod grouping;
mod types;

pub use grouping::{
    group_asset_paths, group_asset_paths_validated, AssetRootGroup, PathGroupingError,
    PathGroupingResult, PathValidationMode,
};
pub use types::{
    FileSystemLocation, FileSystemLocationType, StorageProfile, StorageProfileOsFamily,
    StorageProfileWithId,
};
