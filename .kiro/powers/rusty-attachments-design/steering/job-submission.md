# Job Submission Design Summary

**Full doc:** `design/job-submission.md`  
**Status:** Not yet implemented (planned for `ja-deadline-utils` crate)

## Purpose
Convert manifests and asset metadata into the job submission format for Deadline Cloud CreateJob API.

## Key Types

```rust
enum PathFormat { Windows, Posix }

struct ManifestProperties {
    root_path: String,
    root_path_format: PathFormat,
    input_manifest_path: Option<String>,    // S3 key (partial)
    input_manifest_hash: Option<String>,
    output_relative_directories: Option<Vec<String>>,
    file_system_location_name: Option<String>,
}

struct Attachments {
    manifests: Vec<ManifestProperties>,
    file_system: String,  // "COPIED" or "VIRTUAL"
}

struct AssetRootManifest {
    root_path: String,
    asset_manifest: Option<Manifest>,
    outputs: Vec<PathBuf>,
    file_system_location_name: Option<String>,
}
```

## Conversion Functions

```rust
fn build_manifest_properties(
    asset_root_manifest: &AssetRootManifest,
    partial_manifest_key: Option<&str>,
    manifest_hash: Option<&str>,
) -> ManifestProperties;

fn build_attachments(
    manifest_properties: Vec<ManifestProperties>,
    file_system_mode: &str,
) -> Attachments;

fn to_job_attachments(
    upload_results: &[ManifestUploadInfo],
    file_system_mode: &str,
) -> Attachments;
```

## JSON Output Format

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

## Usage Flow

1. Group assets by root (`group_asset_paths()`)
2. Create manifests for each root (`snapshot()`)
3. Upload manifest contents (`upload_manifest_contents()`)
4. Upload manifest files (`upload_input_manifest()`)
5. Convert to attachments (`to_job_attachments()`)
6. Serialize for CreateJob API (`attachments.to_json()`)

## When to Read Full Doc
- Implementing job submission
- Understanding attachments JSON format
- Path format handling
- Output directory tracking
