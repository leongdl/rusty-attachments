//! Manifest storage operations for S3.
//!
//! This module provides functions for uploading and downloading manifest files
//! to/from S3. Manifests are stored with specific naming conventions, metadata,
//! and content types.
//!
//! # S3 Key Structure
//!
//! ## Input Manifests
//! ```text
//! s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/Inputs/{guid}/{manifest_hash}_input
//! ```
//!
//! ## Output Manifests (Task-Level)
//! ```text
//! s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{task_id}/{timestamp}_{session_action_id}/{manifest_hash}_output
//! ```
//!
//! ## Output Manifests (Step-Level)
//! ```text
//! s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{timestamp}_{session_action_id}/{manifest_hash}_output
//! ```

use std::collections::HashMap;

use rusty_attachments_common::hash_bytes;
use rusty_attachments_model::Manifest;

use crate::error::StorageError;
use crate::traits::StorageClient;
use crate::types::S3Location;

// ============================================================================
// Constants
// ============================================================================

/// Content type for v2023-03-03 snapshot manifests.
pub const CONTENT_TYPE_V2023_03_03: &str = "application/x-deadline-manifest-2023-03-03";

/// Content type for v2025-12-04-beta snapshot manifests.
pub const CONTENT_TYPE_V2025_12_04_BETA: &str = "application/x-deadline-manifest-2025-12-04-beta";

/// Content type for v2025-12-04-beta diff manifests.
pub const CONTENT_TYPE_V2025_12_04_BETA_DIFF: &str =
    "application/x-deadline-manifest-diff-2025-12-04-beta";

/// S3 metadata key for asset root (ASCII paths).
pub const METADATA_KEY_ASSET_ROOT: &str = "asset-root";

/// S3 metadata key for asset root (JSON-encoded, for non-ASCII paths).
pub const METADATA_KEY_ASSET_ROOT_JSON: &str = "asset-root-json";

/// S3 metadata key for file system location name.
pub const METADATA_KEY_FILE_SYSTEM_LOCATION_NAME: &str = "file-system-location-name";

// ============================================================================
// Data Structures
// ============================================================================

/// Location for storing/retrieving manifests.
#[derive(Debug, Clone)]
pub struct ManifestLocation {
    /// S3 bucket name.
    pub bucket: String,
    /// Root prefix for all operations (e.g., "DeadlineCloud").
    pub root_prefix: String,
    /// Farm ID.
    pub farm_id: String,
    /// Queue ID.
    pub queue_id: String,
}

impl ManifestLocation {
    /// Create a new manifest location.
    ///
    /// # Arguments
    /// * `bucket` - S3 bucket name
    /// * `root_prefix` - Root prefix for all operations
    /// * `farm_id` - Farm ID
    /// * `queue_id` - Queue ID
    pub fn new(
        bucket: impl Into<String>,
        root_prefix: impl Into<String>,
        farm_id: impl Into<String>,
        queue_id: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            root_prefix: root_prefix.into(),
            farm_id: farm_id.into(),
            queue_id: queue_id.into(),
        }
    }

    /// Create from an S3Location and farm/queue IDs.
    pub fn from_s3_location(location: &S3Location, farm_id: &str, queue_id: &str) -> Self {
        Self {
            bucket: location.bucket.clone(),
            root_prefix: location.root_prefix.clone(),
            farm_id: farm_id.to_string(),
            queue_id: queue_id.to_string(),
        }
    }

    /// Build the full manifest prefix.
    ///
    /// Returns: `{root_prefix}/Manifests`
    pub fn manifest_prefix(&self) -> String {
        if self.root_prefix.is_empty() {
            "Manifests".to_string()
        } else {
            format!("{}/Manifests", self.root_prefix)
        }
    }

    /// Build the input manifest prefix for this farm/queue.
    ///
    /// Returns: `{root_prefix}/Manifests/{farm_id}/{queue_id}/Inputs`
    pub fn input_manifest_prefix(&self) -> String {
        format!(
            "{}/{}/{}/Inputs",
            self.manifest_prefix(),
            self.farm_id,
            self.queue_id
        )
    }
}

/// Identifiers for output manifest path (task-level).
#[derive(Debug, Clone)]
pub struct TaskOutputManifestPath {
    /// Job ID.
    pub job_id: String,
    /// Step ID.
    pub step_id: String,
    /// Task ID.
    pub task_id: String,
    /// Session action ID.
    pub session_action_id: String,
    /// Timestamp as seconds since epoch (converted to ISO 8601).
    pub timestamp: f64,
}

/// Identifiers for output manifest path (step-level, no task).
#[derive(Debug, Clone)]
pub struct StepOutputManifestPath {
    /// Job ID.
    pub job_id: String,
    /// Step ID.
    pub step_id: String,
    /// Session action ID.
    pub session_action_id: String,
    /// Timestamp as seconds since epoch (converted to ISO 8601).
    pub timestamp: f64,
}

/// S3 metadata attached to manifest uploads.
#[derive(Debug, Clone, Default)]
pub struct ManifestS3Metadata {
    /// Root path the manifest was created from.
    pub asset_root: String,
    /// Optional file system location name (for storage profile mapping).
    pub file_system_location_name: Option<String>,
}

impl ManifestS3Metadata {
    /// Create new metadata with just an asset root.
    ///
    /// # Arguments
    /// * `asset_root` - The root path the manifest was created from
    pub fn new(asset_root: impl Into<String>) -> Self {
        Self {
            asset_root: asset_root.into(),
            file_system_location_name: None,
        }
    }

    /// Create new metadata with asset root and file system location.
    ///
    /// # Arguments
    /// * `asset_root` - The root path the manifest was created from
    /// * `file_system_location_name` - File system location name for storage profile mapping
    pub fn with_location(
        asset_root: impl Into<String>,
        file_system_location_name: impl Into<String>,
    ) -> Self {
        Self {
            asset_root: asset_root.into(),
            file_system_location_name: Some(file_system_location_name.into()),
        }
    }

    /// Build S3 metadata headers for upload.
    ///
    /// Handles non-ASCII paths by JSON-encoding them. For backward compatibility,
    /// when the path contains non-ASCII characters, both `asset-root` and
    /// `asset-root-json` fields are populated with the JSON-encoded value.
    pub fn to_s3_metadata(&self) -> HashMap<String, String> {
        let mut metadata: HashMap<String, String> = HashMap::new();

        // Check if asset_root is ASCII
        if self.asset_root.is_ascii() {
            metadata.insert(METADATA_KEY_ASSET_ROOT.to_string(), self.asset_root.clone());
        } else {
            // For non-ASCII paths, JSON-encode and populate both fields for backward compatibility
            let json_encoded: String =
                serde_json::to_string(&self.asset_root).unwrap_or_else(|_| self.asset_root.clone());
            metadata.insert(METADATA_KEY_ASSET_ROOT.to_string(), json_encoded.clone());
            metadata.insert(METADATA_KEY_ASSET_ROOT_JSON.to_string(), json_encoded);
        }

        if let Some(ref location) = self.file_system_location_name {
            metadata.insert(
                METADATA_KEY_FILE_SYSTEM_LOCATION_NAME.to_string(),
                location.clone(),
            );
        }

        metadata
    }

    /// Parse S3 metadata from download response.
    ///
    /// Prefers `asset-root-json` for non-ASCII paths, falls back to `asset-root`.
    ///
    /// # Arguments
    /// * `metadata` - S3 metadata from HEAD or GET response
    ///
    /// # Returns
    /// Parsed metadata, or None if required fields are missing.
    pub fn from_s3_metadata(metadata: &HashMap<String, String>) -> Option<Self> {
        // Prefer asset-root-json for non-ASCII paths
        let asset_root: String = metadata
            .get(METADATA_KEY_ASSET_ROOT_JSON)
            .and_then(|v| serde_json::from_str(v).ok())
            .or_else(|| metadata.get(METADATA_KEY_ASSET_ROOT).cloned())?;

        let file_system_location_name: Option<String> =
            metadata.get(METADATA_KEY_FILE_SYSTEM_LOCATION_NAME).cloned();

        Some(Self {
            asset_root,
            file_system_location_name,
        })
    }
}

/// Metadata returned when downloading a manifest.
#[derive(Debug, Clone)]
pub struct ManifestDownloadMetadata {
    /// Root path the manifest was created from.
    pub asset_root: String,
    /// Optional file system location name.
    pub file_system_location_name: Option<String>,
    /// Content type from S3.
    pub content_type: Option<String>,
    /// S3 LastModified timestamp (seconds since epoch).
    /// Used for chronological manifest merging.
    pub last_modified: Option<i64>,
}

/// Result of uploading a manifest.
#[derive(Debug, Clone)]
pub struct ManifestUploadResult {
    /// Full S3 key where the manifest was uploaded.
    pub s3_key: String,
    /// Hash of the manifest content.
    pub manifest_hash: String,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Convert a float timestamp (seconds since epoch) to ISO 8601 format.
///
/// Format: `YYYY-MM-DDTHH:MM:SS.ffffffZ`
/// Example: `2025-05-22T22:17:03.409012Z`
///
/// # Arguments
/// * `timestamp` - Seconds since Unix epoch as f64
///
/// # Returns
/// ISO 8601 formatted string with microsecond precision.
pub fn float_to_iso_datetime_string(timestamp: f64) -> String {
    let seconds: i64 = timestamp.trunc() as i64;
    let microseconds: u32 = ((timestamp.fract()) * 1_000_000.0).abs() as u32;

    // Create datetime from timestamp
    let dt: chrono::DateTime<chrono::Utc> =
        chrono::DateTime::from_timestamp(seconds, microseconds * 1000)
            .unwrap_or(chrono::DateTime::UNIX_EPOCH);

    dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// Generate a random GUID (32 hex characters, no dashes).
///
/// Used for input manifest path generation.
pub fn generate_random_guid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Compute the hash used for manifest naming.
///
/// The hash is computed from the source root path (as bytes) using XXH128.
/// This is used to generate unique manifest names based on the asset root.
///
/// # Arguments
/// * `source_root` - The source root path
///
/// # Returns
/// 32-character hex hash string.
pub fn compute_manifest_name_hash(source_root: &str) -> String {
    hash_bytes(source_root.as_bytes())
}

/// Compute the hash used to match manifest S3 keys to job attachment roots.
///
/// The hash is computed from the concatenation of the file system location name
/// (if any) and the root path. This hash appears in the manifest S3 key filename.
///
/// # Arguments
/// * `file_system_location_name` - Optional file system location name from storage profile
/// * `root_path` - The root path of the job attachment
///
/// # Returns
/// The xxh128 hash as a hex string.
pub fn compute_root_path_hash(
    file_system_location_name: Option<&str>,
    root_path: &str,
) -> String {
    let data: String = format!("{}{}", file_system_location_name.unwrap_or(""), root_path);
    hash_bytes(data.as_bytes())
}

/// Get the content type for a manifest.
///
/// # Arguments
/// * `manifest` - The manifest to get content type for
///
/// # Returns
/// Content type string for S3 upload.
pub fn get_manifest_content_type(manifest: &Manifest) -> &'static str {
    match manifest {
        Manifest::V2023_03_03(_) => CONTENT_TYPE_V2023_03_03,
        Manifest::V2025_12_04_beta(m) => {
            if m.parent_manifest_hash.is_some() {
                CONTENT_TYPE_V2025_12_04_BETA_DIFF
            } else {
                CONTENT_TYPE_V2025_12_04_BETA
            }
        }
    }
}

/// Build the partial input manifest prefix (without root prefix).
///
/// Returns: `{farm_id}/{queue_id}/Inputs/{guid}`
///
/// # Arguments
/// * `farm_id` - Farm ID
/// * `queue_id` - Queue ID
pub fn build_partial_input_manifest_prefix(farm_id: &str, queue_id: &str) -> String {
    let guid: String = generate_random_guid();
    format!("{}/{}/Inputs/{}", farm_id, queue_id, guid)
}

// ============================================================================
// S3 Key Format Functions
// ============================================================================

/// Format the full S3 key for an input manifest.
///
/// Key format: `{manifest_prefix}/{farm_id}/{queue_id}/Inputs/{guid}/{manifest_name}`
///
/// # Arguments
/// * `manifest_prefix` - The manifest prefix (e.g., "DeadlineCloud/Manifests")
/// * `farm_id` - Farm ID
/// * `queue_id` - Queue ID
/// * `guid` - Random GUID for uniqueness
/// * `manifest_name` - Manifest filename (e.g., "{hash}_input")
///
/// # Returns
/// Full S3 key for the input manifest.
pub fn format_input_manifest_s3_key(
    manifest_prefix: &str,
    farm_id: &str,
    queue_id: &str,
    guid: &str,
    manifest_name: &str,
) -> String {
    format!(
        "{}/{}/{}/Inputs/{}/{}",
        manifest_prefix, farm_id, queue_id, guid, manifest_name
    )
}

/// Format the full S3 key for a task-level output manifest.
///
/// Key format: `{manifest_prefix}/{farm_id}/{queue_id}/{job_id}/{step_id}/{task_id}/{timestamp}_{session_action_id}/{manifest_name}`
///
/// # Arguments
/// * `manifest_prefix` - The manifest prefix (e.g., "DeadlineCloud/Manifests")
/// * `farm_id` - Farm ID
/// * `queue_id` - Queue ID
/// * `job_id` - Job ID
/// * `step_id` - Step ID
/// * `task_id` - Task ID
/// * `timestamp_str` - ISO 8601 formatted timestamp
/// * `session_action_id` - Session action ID
/// * `manifest_name` - Manifest filename (e.g., "{hash}_output")
///
/// # Returns
/// Full S3 key for the task-level output manifest.
#[allow(clippy::too_many_arguments)]
pub fn format_task_output_manifest_s3_key(
    manifest_prefix: &str,
    farm_id: &str,
    queue_id: &str,
    job_id: &str,
    step_id: &str,
    task_id: &str,
    timestamp_str: &str,
    session_action_id: &str,
    manifest_name: &str,
) -> String {
    format!(
        "{}/{}/{}/{}/{}/{}/{}_{}/{}",
        manifest_prefix,
        farm_id,
        queue_id,
        job_id,
        step_id,
        task_id,
        timestamp_str,
        session_action_id,
        manifest_name
    )
}

/// Format the full S3 key for a step-level output manifest (no task).
///
/// Key format: `{manifest_prefix}/{farm_id}/{queue_id}/{job_id}/{step_id}/{timestamp}_{session_action_id}/{manifest_name}`
///
/// # Arguments
/// * `manifest_prefix` - The manifest prefix (e.g., "DeadlineCloud/Manifests")
/// * `farm_id` - Farm ID
/// * `queue_id` - Queue ID
/// * `job_id` - Job ID
/// * `step_id` - Step ID
/// * `timestamp_str` - ISO 8601 formatted timestamp
/// * `session_action_id` - Session action ID
/// * `manifest_name` - Manifest filename (e.g., "{hash}_output")
///
/// # Returns
/// Full S3 key for the step-level output manifest.
#[allow(clippy::too_many_arguments)]
pub fn format_step_output_manifest_s3_key(
    manifest_prefix: &str,
    farm_id: &str,
    queue_id: &str,
    job_id: &str,
    step_id: &str,
    timestamp_str: &str,
    session_action_id: &str,
    manifest_name: &str,
) -> String {
    format!(
        "{}/{}/{}/{}/{}/{}_{}/{}",
        manifest_prefix,
        farm_id,
        queue_id,
        job_id,
        step_id,
        timestamp_str,
        session_action_id,
        manifest_name
    )
}

// ============================================================================
// Upload Functions
// ============================================================================

/// Upload an input manifest to S3.
///
/// Computes manifest hash, generates S3 key, sets content-type and metadata.
///
/// Key format: `{root_prefix}/Manifests/{farm_id}/{queue_id}/Inputs/{guid}/{hash}_input`
///
/// # Arguments
/// * `client` - Storage client for S3 operations
/// * `location` - Manifest location configuration
/// * `manifest` - The manifest to upload
/// * `asset_root` - The root path the manifest was created from
/// * `file_system_location_name` - Optional file system location name
///
/// # Returns
/// Upload result with S3 key and manifest hash.
pub async fn upload_input_manifest<C: StorageClient>(
    client: &C,
    location: &ManifestLocation,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError> {
    // Encode manifest to JSON
    let manifest_bytes: Vec<u8> = manifest
        .encode()
        .map_err(|e| StorageError::Other {
            message: format!("Failed to encode manifest: {}", e),
        })?
        .into_bytes();

    // Compute manifest hash
    let manifest_hash: String = hash_bytes(&manifest_bytes);

    // Generate manifest name from source root hash
    let name_hash: String = compute_manifest_name_hash(asset_root);
    let manifest_name: String = format!("{}_input", name_hash);

    // Generate GUID for uniqueness
    let guid: String = generate_random_guid();

    // Build full S3 key using format function
    let full_key: String = format_input_manifest_s3_key(
        &location.manifest_prefix(),
        &location.farm_id,
        &location.queue_id,
        &guid,
        &manifest_name,
    );

    // Build metadata
    let s3_metadata: ManifestS3Metadata = match file_system_location_name {
        Some(loc) => ManifestS3Metadata::with_location(asset_root, loc),
        None => ManifestS3Metadata::new(asset_root),
    };
    let metadata: HashMap<String, String> = s3_metadata.to_s3_metadata();

    // Get content type
    let content_type: &str = get_manifest_content_type(manifest);

    // Upload to S3
    client
        .put_object(
            &location.bucket,
            &full_key,
            &manifest_bytes,
            Some(content_type),
            Some(&metadata),
        )
        .await?;

    Ok(ManifestUploadResult {
        s3_key: full_key,
        manifest_hash,
    })
}

/// Upload a task-level output manifest to S3.
///
/// Key format: `{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{task_id}/{timestamp}_{session_action_id}/{hash}_output`
///
/// # Arguments
/// * `client` - Storage client for S3 operations
/// * `location` - Manifest location configuration
/// * `output_path` - Task output path identifiers
/// * `manifest` - The manifest to upload
/// * `asset_root` - The root path the manifest was created from
/// * `file_system_location_name` - Optional file system location name
///
/// # Returns
/// Upload result with S3 key and manifest hash.
pub async fn upload_task_output_manifest<C: StorageClient>(
    client: &C,
    location: &ManifestLocation,
    output_path: &TaskOutputManifestPath,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError> {
    upload_output_manifest_impl(
        client,
        location,
        &output_path.job_id,
        &output_path.step_id,
        Some(&output_path.task_id),
        &output_path.session_action_id,
        output_path.timestamp,
        manifest,
        asset_root,
        file_system_location_name,
    )
    .await
}

/// Upload a step-level output manifest to S3 (no task).
///
/// Key format: `{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{timestamp}_{session_action_id}/{hash}_output`
///
/// # Arguments
/// * `client` - Storage client for S3 operations
/// * `location` - Manifest location configuration
/// * `output_path` - Step output path identifiers
/// * `manifest` - The manifest to upload
/// * `asset_root` - The root path the manifest was created from
/// * `file_system_location_name` - Optional file system location name
///
/// # Returns
/// Upload result with S3 key and manifest hash.
pub async fn upload_step_output_manifest<C: StorageClient>(
    client: &C,
    location: &ManifestLocation,
    output_path: &StepOutputManifestPath,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError> {
    upload_output_manifest_impl(
        client,
        location,
        &output_path.job_id,
        &output_path.step_id,
        None,
        &output_path.session_action_id,
        output_path.timestamp,
        manifest,
        asset_root,
        file_system_location_name,
    )
    .await
}

/// Internal implementation for output manifest upload.
async fn upload_output_manifest_impl<C: StorageClient>(
    client: &C,
    location: &ManifestLocation,
    job_id: &str,
    step_id: &str,
    task_id: Option<&str>,
    session_action_id: &str,
    timestamp: f64,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError> {
    // Encode manifest to JSON
    let manifest_bytes: Vec<u8> = manifest
        .encode()
        .map_err(|e| StorageError::Other {
            message: format!("Failed to encode manifest: {}", e),
        })?
        .into_bytes();

    // Compute manifest hash for naming
    let name_hash: String = compute_root_path_hash(file_system_location_name, asset_root);
    let manifest_name: String = format!("{}_output", name_hash);

    // Build timestamp string
    let timestamp_str: String = float_to_iso_datetime_string(timestamp);

    // Build full S3 key using format functions
    let full_key: String = match task_id {
        Some(tid) => format_task_output_manifest_s3_key(
            &location.manifest_prefix(),
            &location.farm_id,
            &location.queue_id,
            job_id,
            step_id,
            tid,
            &timestamp_str,
            session_action_id,
            &manifest_name,
        ),
        None => format_step_output_manifest_s3_key(
            &location.manifest_prefix(),
            &location.farm_id,
            &location.queue_id,
            job_id,
            step_id,
            &timestamp_str,
            session_action_id,
            &manifest_name,
        ),
    };

    // Build metadata
    let s3_metadata: ManifestS3Metadata = match file_system_location_name {
        Some(loc) => ManifestS3Metadata::with_location(asset_root, loc),
        None => ManifestS3Metadata::new(asset_root),
    };
    let metadata: HashMap<String, String> = s3_metadata.to_s3_metadata();

    // Get content type
    let content_type: &str = get_manifest_content_type(manifest);

    // Compute manifest content hash for result
    let manifest_hash: String = hash_bytes(&manifest_bytes);

    // Upload to S3
    client
        .put_object(
            &location.bucket,
            &full_key,
            &manifest_bytes,
            Some(content_type),
            Some(&metadata),
        )
        .await?;

    Ok(ManifestUploadResult {
        s3_key: full_key,
        manifest_hash,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_to_iso_datetime_string() {
        // Test with a known timestamp
        let timestamp: f64 = 1716414026.409012;
        let result: String = float_to_iso_datetime_string(timestamp);

        // Should be in format YYYY-MM-DDTHH:MM:SS.ffffffZ
        assert!(result.ends_with('Z'));
        assert!(result.contains('T'));
        assert_eq!(result.len(), 27); // Fixed length for microsecond precision
    }

    #[test]
    fn test_float_to_iso_datetime_string_zero() {
        let result: String = float_to_iso_datetime_string(0.0);
        assert_eq!(result, "1970-01-01T00:00:00.000000Z");
    }

    #[test]
    fn test_generate_random_guid() {
        let guid1: String = generate_random_guid();
        let guid2: String = generate_random_guid();

        // Should be 32 hex characters
        assert_eq!(guid1.len(), 32);
        assert_eq!(guid2.len(), 32);

        // Should be different
        assert_ne!(guid1, guid2);

        // Should be valid hex
        assert!(guid1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_manifest_name_hash() {
        let hash1: String = compute_manifest_name_hash("/path/to/assets");
        let hash2: String = compute_manifest_name_hash("/path/to/assets");
        let hash3: String = compute_manifest_name_hash("/different/path");

        // Same input should produce same hash
        assert_eq!(hash1, hash2);

        // Different input should produce different hash
        assert_ne!(hash1, hash3);

        // Should be 32 hex characters
        assert_eq!(hash1.len(), 32);
    }

    #[test]
    fn test_compute_root_path_hash() {
        // Without file system location
        let hash1: String = compute_root_path_hash(None, "/mnt/projects/job1");

        // With file system location
        let hash2: String = compute_root_path_hash(Some("ProjectShare"), "/mnt/projects/job1");

        // Should be different
        assert_ne!(hash1, hash2);

        // Should be 32 hex characters
        assert_eq!(hash1.len(), 32);
        assert_eq!(hash2.len(), 32);
    }

    #[test]
    fn test_manifest_s3_metadata_ascii() {
        let metadata: ManifestS3Metadata = ManifestS3Metadata::new("/ascii/path");
        let s3_meta: HashMap<String, String> = metadata.to_s3_metadata();

        assert_eq!(s3_meta.get(METADATA_KEY_ASSET_ROOT), Some(&"/ascii/path".to_string()));
        assert!(!s3_meta.contains_key(METADATA_KEY_ASSET_ROOT_JSON));
    }

    #[test]
    fn test_manifest_s3_metadata_non_ascii() {
        let metadata: ManifestS3Metadata = ManifestS3Metadata::new("/path/日本語");
        let s3_meta: HashMap<String, String> = metadata.to_s3_metadata();

        // Both fields should be populated for non-ASCII
        assert!(s3_meta.contains_key(METADATA_KEY_ASSET_ROOT));
        assert!(s3_meta.contains_key(METADATA_KEY_ASSET_ROOT_JSON));

        // Values should be JSON-encoded
        let json_value: &String = s3_meta.get(METADATA_KEY_ASSET_ROOT_JSON).unwrap();
        assert!(json_value.starts_with('"'));
    }

    #[test]
    fn test_manifest_s3_metadata_with_location() {
        let metadata: ManifestS3Metadata =
            ManifestS3Metadata::with_location("/path", "MyLocation");
        let s3_meta: HashMap<String, String> = metadata.to_s3_metadata();

        assert_eq!(
            s3_meta.get(METADATA_KEY_FILE_SYSTEM_LOCATION_NAME),
            Some(&"MyLocation".to_string())
        );
    }

    #[test]
    fn test_manifest_s3_metadata_roundtrip() {
        let original: ManifestS3Metadata =
            ManifestS3Metadata::with_location("/path/to/assets", "SharedDrive");
        let s3_meta: HashMap<String, String> = original.to_s3_metadata();
        let parsed: ManifestS3Metadata = ManifestS3Metadata::from_s3_metadata(&s3_meta).unwrap();

        assert_eq!(parsed.asset_root, original.asset_root);
        assert_eq!(
            parsed.file_system_location_name,
            original.file_system_location_name
        );
    }

    #[test]
    fn test_manifest_location_prefixes() {
        let location: ManifestLocation =
            ManifestLocation::new("my-bucket", "DeadlineCloud", "farm-123", "queue-456");

        assert_eq!(location.manifest_prefix(), "DeadlineCloud/Manifests");
        assert_eq!(
            location.input_manifest_prefix(),
            "DeadlineCloud/Manifests/farm-123/queue-456/Inputs"
        );
    }

    #[test]
    fn test_manifest_location_empty_prefix() {
        let location: ManifestLocation = ManifestLocation::new("my-bucket", "", "farm-123", "queue-456");

        assert_eq!(location.manifest_prefix(), "Manifests");
        assert_eq!(
            location.input_manifest_prefix(),
            "Manifests/farm-123/queue-456/Inputs"
        );
    }

    #[test]
    fn test_format_input_manifest_s3_key() {
        let key: String = format_input_manifest_s3_key(
            "DeadlineCloud/Manifests",
            "farm-123",
            "queue-456",
            "abcd1234",
            "e5f6g7h8_input",
        );

        assert_eq!(
            key,
            "DeadlineCloud/Manifests/farm-123/queue-456/Inputs/abcd1234/e5f6g7h8_input"
        );
    }

    #[test]
    fn test_format_input_manifest_s3_key_empty_prefix() {
        let key: String = format_input_manifest_s3_key(
            "Manifests",
            "farm-123",
            "queue-456",
            "guid1234",
            "hash_input",
        );

        assert_eq!(key, "Manifests/farm-123/queue-456/Inputs/guid1234/hash_input");
    }

    #[test]
    fn test_format_task_output_manifest_s3_key() {
        let key: String = format_task_output_manifest_s3_key(
            "DeadlineCloud/Manifests",
            "farm-123",
            "queue-456",
            "job-789",
            "step-abc",
            "task-def",
            "2025-05-22T22:17:03.409012Z",
            "sessionaction-xyz",
            "hash123_output",
        );

        assert_eq!(
            key,
            "DeadlineCloud/Manifests/farm-123/queue-456/job-789/step-abc/task-def/2025-05-22T22:17:03.409012Z_sessionaction-xyz/hash123_output"
        );
    }

    #[test]
    fn test_format_step_output_manifest_s3_key() {
        let key: String = format_step_output_manifest_s3_key(
            "DeadlineCloud/Manifests",
            "farm-123",
            "queue-456",
            "job-789",
            "step-abc",
            "2025-05-22T22:17:03.409012Z",
            "sessionaction-xyz",
            "hash123_output",
        );

        assert_eq!(
            key,
            "DeadlineCloud/Manifests/farm-123/queue-456/job-789/step-abc/2025-05-22T22:17:03.409012Z_sessionaction-xyz/hash123_output"
        );
    }

    #[test]
    fn test_format_task_vs_step_output_manifest_s3_key_difference() {
        // Task-level key should have task_id segment
        let task_key: String = format_task_output_manifest_s3_key(
            "Manifests",
            "farm",
            "queue",
            "job",
            "step",
            "task",
            "2025-01-01T00:00:00.000000Z",
            "session",
            "hash_output",
        );

        // Step-level key should NOT have task_id segment
        let step_key: String = format_step_output_manifest_s3_key(
            "Manifests",
            "farm",
            "queue",
            "job",
            "step",
            "2025-01-01T00:00:00.000000Z",
            "session",
            "hash_output",
        );

        // Task key should contain "task" segment
        assert!(task_key.contains("/task/"));

        // Step key should NOT contain "task" segment
        assert!(!step_key.contains("/task/"));

        // Both should end with the same manifest name
        assert!(task_key.ends_with("hash_output"));
        assert!(step_key.ends_with("hash_output"));
    }
}
