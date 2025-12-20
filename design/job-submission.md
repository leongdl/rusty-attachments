# Rusty Attachments: Job Submission Conversion

## Overview

This document describes the business logic for converting manifests and asset metadata into the job submission format required by Deadline Cloud. This is a pure data transformation layer with no I/O.

---

## Data Structures

### Path Format

```rust
/// Operating system path format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat {
    Windows,
    Posix,
}

impl PathFormat {
    /// Get the path format for the current host.
    pub fn host() -> Self {
        #[cfg(windows)]
        { PathFormat::Windows }
        #[cfg(not(windows))]
        { PathFormat::Posix }
    }
}
```

### Manifest Properties

```rust
/// Properties of a single manifest for job submission.
#[derive(Debug, Clone)]
pub struct ManifestProperties {
    /// Root path the manifest was created from.
    pub root_path: String,
    /// Path format of the root path.
    pub root_path_format: PathFormat,
    /// S3 key path to the uploaded manifest (partial, excludes root prefix).
    pub input_manifest_path: Option<String>,
    /// Hash of the manifest content.
    pub input_manifest_hash: Option<String>,
    /// Relative paths of output directories.
    pub output_relative_directories: Option<Vec<String>>,
    /// File system location name (from storage profile).
    pub file_system_location_name: Option<String>,
}
```

### Attachments

```rust
/// Job attachments payload for job submission.
#[derive(Debug, Clone)]
pub struct Attachments {
    /// List of manifest properties.
    pub manifests: Vec<ManifestProperties>,
    /// File system mode: "COPIED" or "VIRTUAL".
    pub file_system: String,
}

impl Default for Attachments {
    fn default() -> Self {
        Self {
            manifests: Vec::new(),
            file_system: "COPIED".into(),
        }
    }
}
```

### Asset Root Manifest

```rust
/// A manifest with its associated metadata for a single asset root.
#[derive(Debug, Clone)]
pub struct AssetRootManifest {
    /// Root path for this manifest.
    pub root_path: String,
    /// The manifest (None if no input files for this root).
    pub asset_manifest: Option<Manifest>,
    /// Output directories (relative to root_path).
    pub outputs: Vec<PathBuf>,
    /// File system location name (from storage profile).
    pub file_system_location_name: Option<String>,
}
```

---

## Conversion Functions

### Build Manifest Properties

```rust
/// Convert an uploaded manifest into ManifestProperties for job submission.
pub fn build_manifest_properties(
    asset_root_manifest: &AssetRootManifest,
    partial_manifest_key: Option<&str>,
    manifest_hash: Option<&str>,
) -> ManifestProperties {
    let output_rel_paths: Option<Vec<String>> = if asset_root_manifest.outputs.is_empty() {
        None
    } else {
        Some(
            asset_root_manifest.outputs
                .iter()
                .filter_map(|path| {
                    path.strip_prefix(&asset_root_manifest.root_path)
                        .ok()
                        .map(|rel| rel.to_string_lossy().into())
                })
                .collect()
        )
    };

    ManifestProperties {
        root_path: asset_root_manifest.root_path.clone(),
        root_path_format: PathFormat::host(),
        input_manifest_path: partial_manifest_key.map(String::from),
        input_manifest_hash: manifest_hash.map(String::from),
        output_relative_directories: output_rel_paths,
        file_system_location_name: asset_root_manifest.file_system_location_name.clone(),
    }
}
```

### Build Attachments

```rust
/// Convert a list of uploaded manifests into an Attachments payload.
pub fn build_attachments(
    manifest_properties: Vec<ManifestProperties>,
    file_system_mode: &str,
) -> Attachments {
    Attachments {
        manifests: manifest_properties,
        file_system: file_system_mode.into(),
    }
}
```

### Full Conversion Pipeline

```rust
/// Result of uploading a single manifest.
pub struct ManifestUploadInfo {
    pub asset_root_manifest: AssetRootManifest,
    pub partial_manifest_key: Option<String>,
    pub manifest_hash: Option<String>,
}

/// Convert upload results into job submission Attachments.
pub fn to_job_attachments(
    upload_results: &[ManifestUploadInfo],
    file_system_mode: &str,
) -> Attachments {
    let properties: Vec<ManifestProperties> = upload_results
        .iter()
        .map(|info| {
            build_manifest_properties(
                &info.asset_root_manifest,
                info.partial_manifest_key.as_deref(),
                info.manifest_hash.as_deref(),
            )
        })
        .collect();

    build_attachments(properties, file_system_mode)
}
```

---

## JSON Serialization

For API submission, the Attachments struct serializes to JSON:

```rust
impl Attachments {
    /// Serialize to JSON for API submission.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
```

### Example JSON Output

```json
{
  "manifests": [
    {
      "rootPath": "/mnt/projects/job1",
      "rootPathFormat": "posix",
      "inputManifestPath": "farm-123/queue-456/Inputs/abc123_input",
      "inputManifestHash": "def456789...",
      "outputRelativeDirectories": ["renders", "cache"],
      "fileSystemLocationName": "ProjectFiles"
    }
  ],
  "fileSystem": "COPIED"
}
```

---

## Usage Example

```rust
use rusty_attachments_storage::job_submission::{
    to_job_attachments, ManifestUploadInfo, AssetRootManifest,
};

// After uploading manifests...
let upload_results = vec![
    ManifestUploadInfo {
        asset_root_manifest: AssetRootManifest {
            root_path: "/mnt/projects/job1".into(),
            asset_manifest: Some(manifest),
            outputs: vec![PathBuf::from("/mnt/projects/job1/renders")],
            file_system_location_name: Some("ProjectFiles".into()),
        },
        partial_manifest_key: Some("farm-123/queue-456/Inputs/abc123_input".into()),
        manifest_hash: Some("def456789...".into()),
    },
];

let attachments = to_job_attachments(&upload_results, "COPIED");
let json = attachments.to_json()?;

// Use `json` in CreateJob API call
```

---

## Related Documents

- [storage-profiles.md](storage-profiles.md) - Storage profile and file system location types
- [manifest-storage.md](manifest-storage.md) - Manifest upload operations
- [storage-design.md](storage-design.md) - Upload orchestration
