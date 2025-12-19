# Rusty Attachments: Manifest Storage Design

## Overview

This document proposes a set of functions for uploading and downloading manifest files to/from S3. These operations are distinct from CAS data transfers - manifests are stored with specific naming conventions, metadata, and content types.

---

## Project Structure

```
rusty-attachments/
├── crates/
│   ├── storage/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manifest_storage.rs   # NEW - Manifest upload/download
│   │       └── ...
```

---

## S3 Key Structure

### Input Manifests
```
s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/Inputs/{guid}/{manifest_hash}_input
```

### Output Manifests (Task-Level)

Used when a session action is associated with a specific task:
```
s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{task_id}/{timestamp}_{session_action_id}/{manifest_hash}_output
```

### Output Manifests (Step-Level)

Used when a session action is associated with a step but not a specific task (e.g., step-level environment actions):
```
s3://{bucket}/{root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{timestamp}_{session_action_id}/{manifest_hash}_output
```

The timestamp format is ISO 8601: `YYYY-MM-DDTHH:MM:SS.ffffffZ` (e.g., `2025-05-22T22:17:03.409012Z`)

---

## S3 Metadata

Manifests are uploaded with S3 user metadata to track the asset root and optional file system location.

### Metadata Fields

| Field | Required | Description |
|-------|----------|-------------|
| `asset-root` | Yes* | The root path the manifest was created from (ASCII paths only) |
| `asset-root-json` | Yes* | JSON-encoded root path (for non-ASCII paths, or as fallback) |
| `file-system-location-name` | No | File system location name for storage profile mapping |

*One of `asset-root` or `asset-root-json` is required. For backward compatibility, when the path contains non-ASCII characters, both fields are populated with the JSON-encoded value.

### Metadata Structure

```rust
/// S3 metadata attached to manifest uploads
#[derive(Debug, Clone, Default)]
pub struct ManifestS3Metadata {
    /// Root path the manifest was created from
    pub asset_root: String,
    
    /// Optional file system location name (for storage profile mapping)
    pub file_system_location_name: Option<String>,
}

impl ManifestS3Metadata {
    /// Build S3 metadata headers for upload
    pub fn to_s3_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        
        // Handle non-ASCII paths with JSON encoding
        if self.asset_root.is_ascii() {
            metadata.insert("asset-root".into(), self.asset_root.clone());
        } else {
            // For non-ASCII paths, populate both fields for backward compatibility
            let json_encoded = serde_json::to_string(&self.asset_root).unwrap();
            metadata.insert("asset-root".into(), json_encoded.clone());
            metadata.insert("asset-root-json".into(), json_encoded);
        }
        
        if let Some(ref location) = self.file_system_location_name {
            metadata.insert("file-system-location-name".into(), location.clone());
        }
        
        metadata
    }
    
    /// Parse S3 metadata from download response
    pub fn from_s3_metadata(metadata: &HashMap<String, String>) -> Option<Self> {
        // Prefer asset-root-json for non-ASCII paths
        let asset_root = metadata.get("asset-root-json")
            .and_then(|v| serde_json::from_str(v).ok())
            .or_else(|| metadata.get("asset-root").cloned())?;
        
        let file_system_location_name = metadata.get("file-system-location-name").cloned();
        
        Some(Self { asset_root, file_system_location_name })
    }
}
```

---

## Data Structures

```rust
/// Location for storing/retrieving manifests
#[derive(Debug, Clone)]
pub struct ManifestLocation {
    pub bucket: String,
    pub root_prefix: String,
    pub farm_id: String,
    pub queue_id: String,
}

/// Identifiers for output manifest path (task-level)
#[derive(Debug, Clone)]
pub struct TaskOutputManifestPath {
    pub job_id: String,
    pub step_id: String,
    pub task_id: String,
    pub session_action_id: String,
    pub timestamp: f64,  // Seconds since epoch, converted to ISO 8601
}

/// Identifiers for output manifest path (step-level, no task)
#[derive(Debug, Clone)]
pub struct StepOutputManifestPath {
    pub job_id: String,
    pub step_id: String,
    pub session_action_id: String,
    pub timestamp: f64,  // Seconds since epoch, converted to ISO 8601
}

/// Metadata returned when downloading a manifest
#[derive(Debug, Clone)]
pub struct ManifestDownloadMetadata {
    /// Root path the manifest was created from
    pub asset_root: String,
    /// Optional file system location name
    pub file_system_location_name: Option<String>,
    /// Content type from S3
    pub content_type: Option<String>,
}

/// Result of uploading a manifest
#[derive(Debug, Clone)]
pub struct ManifestUploadResult {
    pub s3_key: String,
    pub manifest_hash: String,
}
```

---

## Functions

### Upload Functions

```rust
/// Upload an input manifest to S3
///
/// Computes manifest hash, generates S3 key, sets content-type and metadata.
/// Key format: {root_prefix}/Manifests/{farm_id}/{queue_id}/Inputs/{guid}/{hash}_input
pub async fn upload_input_manifest(
    client: &impl StorageClient,
    location: &ManifestLocation,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError>;

/// Upload an output manifest to S3 (task-level)
///
/// Key format: {root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{task_id}/{timestamp}_{session_action_id}/{hash}_output
pub async fn upload_task_output_manifest(
    client: &impl StorageClient,
    location: &ManifestLocation,
    output_path: &TaskOutputManifestPath,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError>;

/// Upload an output manifest to S3 (step-level, no task)
///
/// Key format: {root_prefix}/Manifests/{farm_id}/{queue_id}/{job_id}/{step_id}/{timestamp}_{session_action_id}/{hash}_output
pub async fn upload_step_output_manifest(
    client: &impl StorageClient,
    location: &ManifestLocation,
    output_path: &StepOutputManifestPath,
    manifest: &Manifest,
    asset_root: &str,
    file_system_location_name: Option<&str>,
) -> Result<ManifestUploadResult, StorageError>;
```

### Download Functions

```rust
/// Download a manifest by its S3 key
pub async fn download_manifest(
    client: &impl StorageClient,
    bucket: &str,
    s3_key: &str,
) -> Result<(Manifest, ManifestDownloadMetadata), StorageError>;

/// Download a manifest by its hash (input manifest)
pub async fn download_input_manifest(
    client: &impl StorageClient,
    location: &ManifestLocation,
    manifest_hash: &str,
) -> Result<(Manifest, ManifestDownloadMetadata), StorageError>;
```

---

## Content Types

| Manifest Type | Version | Content-Type |
|---------------|---------|--------------|
| Snapshot | v2023-03-03 | `application/x-deadline-manifest-2023-03-03` |
| Snapshot | v2025-12-04-beta | `application/x-deadline-manifest-2025-12-04-beta` |
| Diff | v2025-12-04-beta | `application/x-deadline-manifest-diff-2025-12-04-beta` |

```rust
fn get_content_type(manifest: &Manifest) -> &'static str {
    match manifest {
        Manifest::V2023_03_03(_) => "application/x-deadline-manifest-2023-03-03",
        Manifest::V2025_12_04_beta(m) => {
            if m.parent_manifest_hash.is_some() {
                "application/x-deadline-manifest-diff-2025-12-04-beta"
            } else {
                "application/x-deadline-manifest-2025-12-04-beta"
            }
        }
    }
}
```

---

## S3 Metadata Handling

See the [S3 Metadata](#s3-metadata) section above for the `ManifestS3Metadata` struct that handles building and parsing S3 metadata.

---

## Usage Example

```rust
use rusty_attachments_storage::{
    upload_input_manifest, download_manifest,
    ManifestLocation, CrtStorageClient,
};

let client = CrtStorageClient::new(settings)?;
let location = ManifestLocation {
    bucket: "my-bucket".into(),
    root_prefix: "DeadlineCloud".into(),
    farm_id: "farm-123".into(),
    queue_id: "queue-456".into(),
};

// Upload input manifest
let result = upload_input_manifest(
    &client,
    &location,
    &manifest,
    "/project/assets",
    None,  // file_system_location_name
).await?;
println!("Uploaded: {} (hash: {})", result.s3_key, result.manifest_hash);

// Download manifest
let (manifest, metadata) = download_manifest(&client, &location.bucket, &result.s3_key).await?;
println!("Asset root: {}", metadata.asset_root);
```

---

## Related Documents

- [storage-design.md](storage-design.md) - StorageClient trait and CAS operations
- [model-design.md](model-design.md) - Manifest data structures
- [file_system.md](file_system.md) - Creating manifests from directories

---

## Reference: deadline-cloud Python Implementation

The following files in the [deadline-cloud](https://github.com/aws-deadline/deadline-cloud) library implement the equivalent functionality:

### Path Construction
- `src/deadline/job_attachments/models.py`
  - `JobAttachmentS3Settings.full_output_prefix()` - Builds task-level output prefix
  - `JobAttachmentS3Settings.partial_session_action_manifest_prefix()` - Task-level partial path
  - `JobAttachmentS3Settings.partial_session_action_manifest_prefix_without_task()` - Step-level partial path
  - `JobAttachmentS3Settings.partial_manifest_prefix()` - Input manifest partial path with GUID

### Upload Operations
- `src/deadline/job_attachments/asset_sync.py`
  - `AssetSync._upload_output_manifest_to_s3()` - Uploads output manifest with metadata

### Download Operations
- `src/deadline/job_attachments/download.py`
  - `get_manifest_from_s3()` - Downloads and decodes a manifest by S3 key
  - `get_output_manifests_by_asset_root()` - Lists and downloads output manifests
  - `_get_output_manifest_prefix()` - Builds prefix for listing output manifests

### Metadata Handling
- `src/deadline/job_attachments/download.py`
  - `_get_asset_root_from_metadata()` - Extracts asset root from S3 metadata (handles both ASCII and JSON-encoded non-ASCII paths)
