//! Shared types and utilities for rusty-attachments.
//!
//! This crate provides common functionality used across all rusty-attachments crates:
//! - Path normalization utilities
//! - Hash computation functions
//! - Generic progress callback trait
//! - Shared constants and error types
//! - Platform-specific machine identifier

pub mod constants;
pub mod error;
pub mod hash;
pub mod machine_id;
pub mod path_utils;
pub mod progress;

// Re-export commonly used items at crate root
pub use constants::*;
pub use error::{PathError, VersionNotCompatibleError};
pub use hash::{hash_bytes, hash_file, hash_string, Xxh3Hasher};
pub use machine_id::get_machine_id;
pub use path_utils::{
    from_posix_path, is_within_root, lexical_normalize, normalize_for_manifest, to_absolute,
    to_long_path, to_posix_path,
};
pub use progress::{progress_fn, FnProgress, NoOpProgress, ProgressCallback};
