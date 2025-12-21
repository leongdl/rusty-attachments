# Example: Incremental Download Implementation in Rusty-Attachments

## Overview

This document provides a detailed analysis of the Python `_incremental_output_download` implementation and maps it to rusty-attachments primitives. The focus is on manifest operations and file downloading, excluding CLI parsing, job state tracking, and checkpoint management.

---

## Python Implementation Flow Analysis

### High-Level Flow

```
1. Get download candidate jobs (SearchJobs API)
2. Categorize jobs (compare with checkpoint)
3. Get sessions and session actions (ListSessions, ListSessionActions APIs)
4. Populate manifest S3 keys from S3 listing
5. Get storage profiles (GetStorageProfileForQueue API)
6. Create path mapping rules (if storage profiles differ)
7. Download all manifests from S3
8. Make manifest paths absolute
9. Apply path mapping (if needed)
10. Merge manifests chronologically
11. Download files from CAS
```

### Key Manifest Operations

#### 1. Manifest Discovery (`_add_output_manifests_from_s3`)

**What it does:**
- Lists S3 objects under the output manifest prefix
- Extracts session action IDs from S3 keys using regex
- Matches manifest keys to job attachment roots using hash
- Populates session actions with `outputManifestPath`

**S3 Key Pattern:**
```
{rootPrefix}/Manifests/{farmId}/{queueId}/{jobId}/{stepId}/{taskId}/{timestamp}_{sessionActionId}/{hash}_output
```

**Rust Equivalent:**
```rust
use rusty_attachments_storage::{
    ManifestLocation, OutputManifestScope, OutputManifestDiscoveryOptions,
    discover_output_manifest_keys,
};

// Discover manifests for a job
let options = OutputManifestDiscoveryOptions {
    scope: OutputManifestScope::Job { job_id: job_id.to_string() },
    select_latest_per_task: false, // Get all, not just latest
};

let manifest_keys = discover_output_manifest_keys(
    &client,
    &location,
    &options,
).await?;
```

#### 2. Manifest Download with Path Transformation (`_download_manifest_and_make_paths_absolute`)

**What it does:**
- Downloads manifest from S3
- Extracts `LastModified` timestamp
- Joins manifest paths with root path
- Applies path mapping if storage profiles differ
- Tracks unmapped paths

**Python Code:**
```python
# Download manifest
_, last_modified, manifest = _get_asset_root_and_manifest_from_s3_with_last_modified(
    manifest_s3_key, s3_bucket, boto3_session
)

# Make paths absolute
for manifest_path in manifest.paths:
    manifest_path.path = source_os_path.normpath(
        source_os_path.join(root_path, manifest_path.path)
    )
    
    # Apply path mapping
    if path_mapping_rule_applier:
        try:
            manifest_path.path = str(
                path_mapping_rule_applier.strict_transform(manifest_path.path)
            )
            new_manifest_paths.append(manifest_path)
        except ValueError:
            output_unmapped_paths.append((job_id, manifest_path.path))
```

**Rust Equivalent:**
```rust
use rusty_attachments_storage::{
    download_manifest, PathMappingApplier, PathFormat,
    resolve_manifest_paths,
};

// Download manifest with metadata
let (manifest, metadata) = download_manifest(&client, bucket, &s3_key).await?;
let last_modified = metadata.last_modified.unwrap_or(0);

// Resolve paths with mapping
let resolved = resolve_manifest_paths(
    &manifest,
    &metadata.asset_root,
    source_path_format,
    applier.as_ref(),
)?;

// Track unmapped paths
for unmapped in &resolved.unmapped {
    unmapped_paths.entry(job_id.clone())
        .or_default()
        .push(unmapped.absolute_source_path.clone());
}
```

#### 3. Manifest Merging (`_merge_absolute_path_manifest_list`)

**What it does:**
- Sorts manifests by `LastModified` timestamp (oldest first)
- Merges paths using normalized path as key
- Later files overwrite earlier ones (last-write-wins)

**Python Code:**
```python
# Sort by last modified
downloaded_manifests.sort(key=lambda item: item[0])

# Merge using dict (later overwrites earlier)
merged_manifest_paths_dict = {}
for _, manifest in downloaded_manifests:
    for manifest_path in manifest.paths:
        merged_manifest_paths_dict[os.path.normcase(manifest_path.path)] = manifest_path

return list(merged_manifest_paths_dict.values())
```

**Rust Equivalent:**
```rust
use rusty_attachments_model::merge::merge_manifests_chronologically;
use std::collections::HashMap;

// Prepare manifests with timestamps
let mut manifests_with_timestamps: Vec<(i64, &Manifest)> = downloaded_manifests
    .iter()
    .map(|(ts, manifest)| (*ts, manifest))
    .collect();

// Merge chronologically (handles last-write-wins)
let merged = merge_manifests_chronologically(&mut manifests_with_timestamps)?
    .expect("non-empty manifest list");

// Extract file paths for download
let files_to_download: Vec<&ManifestFilePath> = merged.files()?.iter().collect();
```

#### 4. File Download (`_download_manifest_paths`)

**What it does:**
- Downloads files from CAS using hash as key
- Uses transfer manager for large files (>1MB)
- Uses get_object for small files
- Handles conflict resolution (skip, overwrite, create copy)
- Sets file mtime from manifest
- Verifies file size after download
- Reports progress

**Python Code:**
```python
def _download_file(file, hash_algorithm, s3_bucket, cas_prefix, ...):
    s3_key = f"{cas_prefix}/{file.hash}.{hash_algorithm.value}"
    
    # Conflict resolution
    if local_file_path.is_file():
        if file_conflict_resolution == FileConflictResolution.SKIP:
            return
        elif file_conflict_resolution == FileConflictResolution.CREATE_COPY:
            local_file_path = _get_new_copy_file_path(...)
    
    # Download based on size
    if (file.size or 0) > 1024 * 1024:
        _download_file_with_transfer_manager(...)
    else:
        _download_file_with_get_object(...)
    
    # Set mtime
    modified_time_override = (file.mtime or 0) / 1000000
    os.utime(local_file_path, (modified_time_override, modified_time_override))
    
    # Verify size
    file_size_on_disk = os.path.getsize(local_file_path)
    if file_size_on_disk != file.size:
        raise JobAttachmentsError(...)
```

**Rust Equivalent:**
```rust
use rusty_attachments_storage::{
    DownloadOrchestrator, ConflictResolution, TransferProgress,
};

// Download to resolved paths
let stats = orchestrator.download_to_resolved_paths(
    &resolved.resolved,
    manifest.hash_alg(),
    ConflictResolution::CreateCopy,
    Some(&progress_callback),
).await?;
```

---

## Rusty-Attachments Implementation

### Core Data Structures

```rust
/// Manifest with metadata for chronological merging
#[derive(Debug, Clone)]
pub struct ManifestWithTimestamp {
    pub manifest: Manifest,
    pub last_modified: i64,  // Unix timestamp (seconds)
    pub asset_root: String,
}

/// Result of downloading and resolving manifests
#[derive(Debug, Clone)]
pub struct ResolvedManifests {
    /// Successfully resolved paths ready for download
    pub resolved: Vec<ResolvedManifestPath>,
    /// Paths that couldn't be mapped
    pub unmapped: HashMap<String, Vec<String>>,  // job_id -> unmapped_paths
}

/// Session action with manifest information
#[derive(Debug, Clone)]
pub struct SessionActionWithManifests {
    pub session_action_id: String,
    pub session_id: String,
    pub job_id: String,
    /// Manifest S3 keys (one per job attachment root)
    pub manifest_keys: Vec<Option<String>>,
}
```

### Step 1: Discover Output Manifests

```rust
use rusty_attachments_storage::{
    ManifestLocation, OutputManifestScope, OutputManifestDiscoveryOptions,
    discover_output_manifest_keys,
};

/// Discover all output manifests for jobs with new outputs
pub async fn discover_job_output_manifests<C: StorageClient>(
    client: &C,
    location: &ManifestLocation,
    job_ids: &[String],
) -> Result<HashMap<String, Vec<String>>, StorageError> {
    let mut manifests_by_job: HashMap<String, Vec<String>> = HashMap::new();
    
    for job_id in job_ids {
        let options = OutputManifestDiscoveryOptions {
            scope: OutputManifestScope::Job { job_id: job_id.clone() },
            select_latest_per_task: false,  // Get all manifests
        };
        
        let keys = discover_output_manifest_keys(client, location, &options).await?;
        manifests_by_job.insert(job_id.clone(), keys);
    }
    
    Ok(manifests_by_job)
}
```

### Step 2: Match Manifests to Session Actions

```rust
use regex::Regex;
use rusty_attachments_common::hash_string;

/// Extract session action ID from manifest S3 key
fn extract_session_action_id(key: &str) -> Option<String> {
    // Pattern: sessionaction-{id}-{index}
    let re = Regex::new(r"(sessionaction-[^/-]+-\d+)").ok()?;
    re.captures(key)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Match manifest keys to job attachment roots using hash
pub fn match_manifests_to_roots(
    manifest_keys: &[String],
    job_attachment_roots: &[JobAttachmentRoot],
) -> Result<HashMap<String, Vec<Option<String>>>, ManifestError> {
    let mut manifests_by_session_action: HashMap<String, Vec<Option<String>>> = HashMap::new();
    
    // Compute root path hashes
    let root_hashes: Vec<(usize, String)> = job_attachment_roots
        .iter()
        .enumerate()
        .map(|(idx, root)| {
            let data = format!(
                "{}{}",
                root.file_system_location_name.as_deref().unwrap_or(""),
                root.root_path
            );
            (idx, hash_string(&data, HashAlgorithm::Xxh128))
        })
        .collect();
    
    for key in manifest_keys {
        // Extract session action ID
        let session_action_id = extract_session_action_id(key)
            .ok_or_else(|| ManifestError::InvalidKey { key: key.clone() })?;
        
        // Find which root this manifest belongs to
        let root_index = root_hashes
            .iter()
            .find(|(_, hash)| key.contains(hash))
            .map(|(idx, _)| *idx)
            .ok_or_else(|| ManifestError::NoMatchingRoot { key: key.clone() })?;
        
        // Initialize entry for this session action
        let entry = manifests_by_session_action
            .entry(session_action_id)
            .or_insert_with(|| vec![None; job_attachment_roots.len()]);
        
        entry[root_index] = Some(key.clone());
    }
    
    Ok(manifests_by_session_action)
}

#[derive(Debug, Clone)]
pub struct JobAttachmentRoot {
    pub root_path: String,
    pub root_path_format: PathFormat,
    pub file_system_location_name: Option<String>,
}
```

### Step 3: Download and Transform Manifests

```rust
use rusty_attachments_storage::{
    download_manifest, PathMappingApplier, resolve_manifest_paths,
};

/// Download a single manifest and resolve its paths
pub async fn download_and_resolve_manifest<C: StorageClient>(
    client: &C,
    bucket: &str,
    s3_key: &str,
    source_path_format: PathFormat,
    applier: Option<&PathMappingApplier>,
) -> Result<ManifestWithTimestamp, StorageError> {
    // Download manifest with metadata
    let (manifest, metadata) = download_manifest(client, bucket, s3_key).await?;
    
    let asset_root = metadata.asset_root
        .ok_or_else(|| StorageError::MissingAssetRoot)?;
    let last_modified = metadata.last_modified.unwrap_or(0);
    
    Ok(ManifestWithTimestamp {
        manifest,
        last_modified,
        asset_root,
    })
}

/// Download all manifests and resolve paths
pub async fn download_all_manifests_with_resolution<C: StorageClient>(
    client: &C,
    bucket: &str,
    manifest_keys: &[(String, String, PathFormat)],  // (job_id, s3_key, source_format)
    path_mapping_appliers: &HashMap<String, Option<PathMappingApplier>>,
) -> Result<ResolvedManifests, StorageError> {
    let mut all_manifests: Vec<ManifestWithTimestamp> = Vec::new();
    let mut unmapped_by_job: HashMap<String, Vec<String>> = HashMap::new();
    
    // Download manifests in parallel
    let futures: Vec<_> = manifest_keys
        .iter()
        .map(|(job_id, s3_key, source_format)| {
            let applier = path_mapping_appliers.get(job_id).and_then(|a| a.as_ref());
            async move {
                let manifest_with_ts = download_and_resolve_manifest(
                    client,
                    bucket,
                    s3_key,
                    *source_format,
                    applier,
                ).await?;
                
                // Resolve paths
                let resolved = resolve_manifest_paths(
                    &manifest_with_ts.manifest,
                    &manifest_with_ts.asset_root,
                    *source_format,
                    applier,
                )?;
                
                Ok::<_, StorageError>((job_id.clone(), manifest_with_ts, resolved))
            }
        })
        .collect();
    
    // Await all downloads
    for result in futures::future::join_all(futures).await {
        let (job_id, manifest_with_ts, resolved) = result?;
        
        all_manifests.push(manifest_with_ts);
        
        // Track unmapped paths
        if !resolved.unmapped.is_empty() {
            unmapped_by_job.entry(job_id)
                .or_default()
                .extend(resolved.unmapped.iter().map(|u| u.absolute_source_path.clone()));
        }
    }
    
    Ok(ResolvedManifests {
        resolved: all_manifests.into_iter()
            .flat_map(|m| {
                // Convert manifest to resolved paths
                // This is simplified - actual implementation would use resolve_manifest_paths
                vec![]
            })
            .collect(),
        unmapped: unmapped_by_job,
    })
}
```

### Step 4: Merge Manifests by Asset Root

```rust
use rusty_attachments_model::merge::merge_manifests_chronologically;

/// Group manifests by asset root and merge chronologically
pub fn merge_manifests_by_root(
    manifests: Vec<ManifestWithTimestamp>,
) -> Result<HashMap<String, Manifest>, ManifestError> {
    // Group by asset root
    let mut by_root: HashMap<String, Vec<(i64, Manifest)>> = HashMap::new();
    
    for manifest_with_ts in manifests {
        by_root
            .entry(manifest_with_ts.asset_root.clone())
            .or_default()
            .push((manifest_with_ts.last_modified, manifest_with_ts.manifest));
    }
    
    // Merge each group chronologically
    let mut merged_by_root: HashMap<String, Manifest> = HashMap::new();
    
    for (root, mut manifests) in by_root {
        let merged = merge_manifests_chronologically(&mut manifests)?
            .ok_or_else(|| ManifestError::EmptyManifestList)?;
        
        merged_by_root.insert(root, merged);
    }
    
    Ok(merged_by_root)
}
```

### Step 5: Download Files

```rust
use rusty_attachments_storage::{
    DownloadOrchestrator, ConflictResolution, TransferProgress,
};

/// Download all files from merged manifests
pub async fn download_merged_manifests<C: StorageClient>(
    orchestrator: &DownloadOrchestrator<C>,
    merged_manifests: HashMap<String, Manifest>,
    conflict_resolution: ConflictResolution,
    progress: Option<&dyn ProgressCallback<TransferProgress>>,
) -> Result<TransferStatistics, StorageError> {
    let mut all_resolved: Vec<ResolvedManifestPath> = Vec::new();
    
    // Collect all resolved paths from all manifests
    for (root, manifest) in &merged_manifests {
        let resolved = resolve_manifest_paths_direct(&manifest, Path::new(root))?;
        all_resolved.extend(resolved);
    }
    
    // Download all files
    orchestrator.download_to_resolved_paths(
        &all_resolved,
        HashAlgorithm::Xxh128,  // Assume xxh128 for now
        conflict_resolution,
        progress,
    ).await
}

/// Helper to resolve manifest paths directly (no mapping)
fn resolve_manifest_paths_direct(
    manifest: &Manifest,
    root: &Path,
) -> Result<Vec<ResolvedManifestPath>, StorageError> {
    manifest.files()?
        .iter()
        .filter(|f| !f.deleted)
        .map(|entry| {
            let local_path = root.join(&entry.path);
            Ok(ResolvedManifestPath {
                manifest_entry: entry.clone(),
                local_path,
            })
        })
        .collect()
}
```

---

## Complete Incremental Download Function

```rust
use rusty_attachments_storage::{
    ManifestLocation, OutputManifestScope, OutputManifestDiscoveryOptions,
    discover_output_manifest_keys, download_manifest,
    PathMappingApplier, PathFormat, resolve_manifest_paths,
    DownloadOrchestrator, ConflictResolution, TransferProgress,
    StorageClient, S3Location,
};
use rusty_attachments_model::{Manifest, merge::merge_manifests_chronologically};
use std::collections::HashMap;

/// Incremental download for a set of jobs
pub async fn incremental_download_jobs<C: StorageClient>(
    client: &C,
    s3_location: &S3Location,
    manifest_location: &ManifestLocation,
    job_ids: &[String],
    job_attachment_roots: &HashMap<String, Vec<JobAttachmentRoot>>,
    path_mapping_appliers: &HashMap<String, Option<PathMappingApplier>>,
    conflict_resolution: ConflictResolution,
    progress: Option<&dyn ProgressCallback<TransferProgress>>,
) -> Result<IncrementalDownloadResult, StorageError> {
    // 1. Discover all output manifests
    let mut all_manifest_keys: Vec<(String, String, PathFormat)> = Vec::new();
    
    for job_id in job_ids {
        let options = OutputManifestDiscoveryOptions {
            scope: OutputManifestScope::Job { job_id: job_id.clone() },
            select_latest_per_task: false,
        };
        
        let keys = discover_output_manifest_keys(client, manifest_location, &options).await?;
        
        // Get source path format from job's storage profile
        let source_format = get_job_source_format(job_id, job_attachment_roots)?;
        
        for key in keys {
            all_manifest_keys.push((job_id.clone(), key, source_format));
        }
    }
    
    // 2. Download all manifests with path resolution
    let mut manifests_with_ts: Vec<ManifestWithTimestamp> = Vec::new();
    let mut unmapped_by_job: HashMap<String, Vec<String>> = HashMap::new();
    
    for (job_id, s3_key, source_format) in &all_manifest_keys {
        let applier = path_mapping_appliers
            .get(job_id)
            .and_then(|a| a.as_ref());
        
        // Download manifest
        let (manifest, metadata) = download_manifest(
            client,
            &manifest_location.bucket,
            s3_key,
        ).await?;
        
        let asset_root = metadata.asset_root
            .ok_or_else(|| StorageError::MissingAssetRoot)?;
        let last_modified = metadata.last_modified.unwrap_or(0);
        
        // Resolve paths
        let resolved = resolve_manifest_paths(
            &manifest,
            &asset_root,
            *source_format,
            applier,
        )?;
        
        // Track unmapped
        if !resolved.unmapped.is_empty() {
            unmapped_by_job
                .entry(job_id.clone())
                .or_default()
                .extend(resolved.unmapped.iter().map(|u| u.absolute_source_path.clone()));
        }
        
        manifests_with_ts.push(ManifestWithTimestamp {
            manifest,
            last_modified,
            asset_root,
        });
    }
    
    // 3. Group by asset root and merge chronologically
    let mut by_root: HashMap<String, Vec<(i64, Manifest)>> = HashMap::new();
    
    for manifest_with_ts in manifests_with_ts {
        by_root
            .entry(manifest_with_ts.asset_root.clone())
            .or_default()
            .push((manifest_with_ts.last_modified, manifest_with_ts.manifest));
    }
    
    let mut all_resolved: Vec<ResolvedManifestPath> = Vec::new();
    
    for (root, mut manifests) in by_root {
        // Merge chronologically
        let merged = merge_manifests_chronologically(&mut manifests)?
            .ok_or_else(|| ManifestError::EmptyManifestList)?;
        
        // Resolve to absolute paths
        let resolved = resolve_manifest_paths_direct(&merged, Path::new(&root))?;
        all_resolved.extend(resolved);
    }
    
    // 4. Download all files
    let orchestrator = DownloadOrchestrator::new(client.clone(), s3_location.clone());
    
    let stats = orchestrator.download_to_resolved_paths(
        &all_resolved,
        HashAlgorithm::Xxh128,
        conflict_resolution,
        progress,
    ).await?;
    
    Ok(IncrementalDownloadResult {
        stats,
        unmapped_paths: unmapped_by_job,
        files_downloaded: all_resolved.len(),
    })
}

#[derive(Debug, Clone)]
pub struct IncrementalDownloadResult {
    pub stats: TransferStatistics,
    pub unmapped_paths: HashMap<String, Vec<String>>,
    pub files_downloaded: usize,
}

fn get_job_source_format(
    job_id: &str,
    job_attachment_roots: &HashMap<String, Vec<JobAttachmentRoot>>,
) -> Result<PathFormat, StorageError> {
    job_attachment_roots
        .get(job_id)
        .and_then(|roots| roots.first())
        .map(|root| root.root_path_format)
        .ok_or_else(|| StorageError::MissingJobInfo { job_id: job_id.to_string() })
}
```

---

## Key Differences from Python

### 1. Manifest Discovery

**Python:** Lists all S3 objects, extracts session action IDs with regex, matches to roots
**Rust:** Uses `discover_output_manifest_keys()` primitive which handles S3 listing and filtering

### 2. Path Resolution

**Python:** Manually joins paths, applies path mapping in-place
**Rust:** Uses `resolve_manifest_paths()` which returns resolved and unmapped paths separately

### 3. Manifest Merging

**Python:** Uses dict with normcased path as key for last-write-wins
**Rust:** Uses `merge_manifests_chronologically()` which handles merging at manifest level

### 4. File Download

**Python:** Manual thread pool, size-based download strategy, conflict resolution
**Rust:** `DownloadOrchestrator.download_to_resolved_paths()` handles all of this

---

## Primitives Used

| Operation | Rust Primitive | Module |
|-----------|---------------|--------|
| Discover manifests | `discover_output_manifest_keys()` | `manifest-storage` |
| Download manifest | `download_manifest()` | `manifest-storage` |
| Resolve paths | `resolve_manifest_paths()` | `path-mapping` |
| Merge manifests | `merge_manifests_chronologically()` | `manifest-utils` |
| Download files | `download_to_resolved_paths()` | `storage-design` |
| Path mapping | `PathMappingApplier` | `path-mapping` |

---

## Related Documents

- [manifest-storage.md](../manifest-storage.md) - Manifest discovery and download
- [manifest-utils.md](../manifest-utils.md) - Manifest merging
- [path-mapping.md](../path-mapping.md) - Path transformation
- [storage-design.md](../storage-design.md) - File download orchestration
- [example-incremental-download.md](example-incremental-download.md) - Simplified example


---

## Analysis: Comparison with deadline-cloud Implementation

This section analyzes the rusty-attachments design against the actual deadline-cloud Python implementation in `_incremental_download.py` and `_manifest_s3_downloads.py`.

### Summary

The rusty-attachments design **covers all core primitives** needed for incremental download. However, there are some **gaps and refinements** worth noting.

---

### ✅ Well-Covered Areas

#### 1. Manifest Discovery from S3

**Python (`_add_output_manifests_from_s3`):**
- Lists S3 objects under output manifest prefix
- Extracts session action IDs using regex: `(sessionaction-[^/-]+-[^/-]+)/`
- Matches manifest keys to job attachment roots using hash of `{fileSystemLocationName}{rootPath}`

**Rust Design (`manifest-storage.md`):**
- `discover_output_manifest_keys()` handles S3 listing and filtering
- `parse_manifest_keys()` extracts session action IDs
- `group_manifests_by_task()` and `select_latest_manifests_per_task()` for filtering

**Assessment:** ✅ Complete - The Rust design provides equivalent primitives.

#### 2. Manifest Download with Metadata

**Python (`_get_asset_root_and_manifest_from_s3_with_last_modified`):**
- Downloads manifest from S3
- Extracts `LastModified` timestamp from S3 response
- Parses `asset-root` and `asset-root-json` from S3 metadata

**Rust Design (`manifest-storage.md`):**
- `download_manifest()` returns `(Manifest, ManifestDownloadMetadata)`
- `ManifestDownloadMetadata` includes `asset_root`, `file_system_location_name`, `last_modified`
- `ManifestS3Metadata::from_s3_metadata()` handles both ASCII and JSON-encoded paths

**Assessment:** ✅ Complete

#### 3. Path Resolution and Mapping

**Python (`_download_manifest_and_make_paths_absolute`):**
- Joins manifest paths with root using source OS path module (`ntpath` or `posixpath`)
- Applies `_PathMappingRuleApplier.strict_transform()` for cross-profile mapping
- Tracks unmapped paths separately

**Rust Design (`path-mapping.md`):**
- `resolve_manifest_paths()` handles joining and mapping
- `PathMappingApplier` with trie-based matching
- `ResolvedManifestPaths` separates `resolved` and `unmapped` paths

**Assessment:** ✅ Complete

#### 4. Chronological Manifest Merging

**Python (`_merge_absolute_path_manifest_list`):**
- Sorts manifests by `LastModified` timestamp (oldest first)
- Uses dict with `os.path.normcase(path)` as key for last-write-wins

**Rust Design (`manifest-utils.md`):**
- `merge_manifests_chronologically()` sorts by timestamp and merges
- Uses `HashMap` with path as key for last-write-wins semantics

**Assessment:** ✅ Complete

#### 5. File Download with Conflict Resolution

**Python (`_download_file`):**
- Size-based strategy: `>1MB` uses transfer manager, else `get_object`
- Conflict resolution: SKIP, OVERWRITE, CREATE_COPY
- `_get_new_copy_file_path()` generates unique names with file locking
- Sets mtime from manifest after download
- Verifies file size matches manifest

**Rust Design (`storage-design.md`):**
- `download_to_resolved_paths()` handles batch downloads
- `ConflictResolution` enum with Skip, Overwrite, CreateCopy
- `generate_unique_copy_path()` with atomic file creation
- File metadata setting mentioned in download orchestrator

**Assessment:** ✅ Complete

---

### ⚠️ Gaps and Refinements Needed

#### 1. Session Action ID Matching to Manifest Roots

**Python Implementation Detail:**
```python
# Hash computation for matching manifests to roots
job_indexed_root_path_hash = [
    (index, ja_hash_data(
        f"{manifest.get('fileSystemLocationName', '')}{manifest['rootPath']}".encode(),
        DEFAULT_HASH_ALG,
    ))
    for index, manifest in enumerate(job["attachments"]["manifests"])
]
```

The Python code matches manifest S3 keys to job attachment roots by:
1. Computing hash of `{fileSystemLocationName}{rootPath}` for each root
2. Checking if the hash appears in the S3 key

**Rust Design Gap:**
The `match_manifests_to_roots()` function in the example document shows this pattern, but it's not explicitly documented in `manifest-storage.md`. The design should clarify:
- The hash algorithm used (xxh128)
- The exact string format: `"{fileSystemLocationName}{rootPath}"` (empty string if no location name)

**Recommendation:** Add a `compute_root_path_hash()` function to `manifest-storage.md`:
```rust
/// Compute the hash used to match manifest S3 keys to job attachment roots.
pub fn compute_root_path_hash(
    file_system_location_name: Option<&str>,
    root_path: &str,
) -> String {
    let data = format!(
        "{}{}",
        file_system_location_name.unwrap_or(""),
        root_path
    );
    hash_string(&data, HashAlgorithm::Xxh128)
}
```

#### 2. Manifest Index Correspondence

**Python Implementation Detail:**
```python
# The manifests lists from the job and session action correspond, so we can zip them
for job_manifest, session_action_manifest in zip(
    job["attachments"]["manifests"], session_action["manifests"]
):
```

Each session action has a `manifests` array that corresponds 1:1 with the job's `attachments.manifests` array. This is critical for:
- Knowing which root path each output manifest belongs to
- Handling jobs with multiple asset roots

**Rust Design Gap:**
This correspondence isn't explicitly documented. The design assumes manifests are matched by hash, but the index-based correspondence is also important for the orchestration layer.

**Recommendation:** Document this in `manifest-storage.md` or a new `incremental-download.md` design doc:
```rust
/// Session action manifest information.
/// 
/// The `manifests` array corresponds 1:1 with the job's `attachments.manifests` array.
/// Each entry contains the output manifest path for that asset root, or None if
/// the session action didn't produce output for that root.
pub struct SessionActionManifests {
    pub session_action_id: String,
    /// One entry per job attachment root, in the same order as job.attachments.manifests
    pub manifests: Vec<Option<SessionActionManifestInfo>>,
}

pub struct SessionActionManifestInfo {
    pub output_manifest_path: String,
}
```

#### 3. Source Path Format Handling in Path Resolution

**Python Implementation Detail:**
```python
if path_mapping_rule_applier:
    if path_mapping_rule_applier.source_path_format == PathFormat.WINDOWS.value:
        source_os_path: Any = ntpath
    else:
        source_os_path = posixpath
else:
    source_os_path = os.path

# Join using source format
manifest_path.path = source_os_path.normpath(
    source_os_path.join(root_path, manifest_path.path)
)
```

The Python code uses the source path format (from the job's storage profile) to determine how to join paths, even when no path mapping is applied.

**Rust Design Status:**
The `resolve_manifest_paths()` function in `path-mapping.md` takes `source_path_format` as a parameter and uses `join_path()` which handles this correctly.

**Assessment:** ✅ Covered, but worth verifying the `join_path()` implementation handles edge cases like:
- Windows UNC paths (`\\server\share`)
- Mixed separators in input
- Trailing separators on root

#### 4. Progress Callback with Cancellation

**Python Implementation:**
```python
def _update_download_progress(download_metadata: ProgressReportMetadata) -> bool:
    # ... progress reporting ...
    return sigint_handler.continue_operation  # Return False to cancel
```

The progress callback returns a boolean to signal cancellation.

**Rust Design:**
The `ProgressCallback<T>` trait in `common.md` returns `bool` from `on_progress()`:
```rust
pub trait ProgressCallback<T>: Send + Sync {
    fn on_progress(&self, progress: &T) -> bool;  // false = cancel
}
```

**Assessment:** ✅ Complete

#### 5. File Size Verification After Download

**Python Implementation:**
```python
# Verify that what we downloaded has the correct file size from the manifest.
file_size_on_disk = os.path.getsize(local_file_path)
if file_size_on_disk != file.size:
    raise JobAttachmentsError(
        f"File from S3 for {file.path} had incorrect size {file_size_on_disk}. Required size: {file.size}"
    )
```

**Rust Design Gap:**
The `storage-design.md` mentions size validation in `StorageError::SizeMismatch`, but doesn't explicitly show post-download verification in the download orchestrator.

**Recommendation:** Ensure `download_to_resolved_paths()` verifies file size after download:
```rust
// After download completes
let actual_size = std::fs::metadata(&local_path)?.len();
if actual_size != expected_size {
    return Err(StorageError::SizeMismatch {
        key: hash.clone(),
        expected: expected_size,
        actual: actual_size,
    });
}
```

#### 6. mtime Setting After Download

**Python Implementation:**
```python
# The modified time in the manifest is in microseconds
modified_time_override = (file.mtime or 0) / 1000000
os.utime(local_file_path, (modified_time_override, modified_time_override))
```

**Rust Design Gap:**
The design mentions setting file permissions but doesn't explicitly document mtime handling. The manifest stores mtime in microseconds, which needs conversion.

**Recommendation:** Add to `storage-design.md` download logic:
```rust
/// Set file modification time from manifest entry.
/// 
/// Manifest mtime is in microseconds since epoch.
fn set_file_mtime(path: &Path, mtime_us: i64) -> std::io::Result<()> {
    let mtime_secs = mtime_us / 1_000_000;
    let mtime_nanos = ((mtime_us % 1_000_000) * 1000) as u32;
    
    let mtime = std::time::UNIX_EPOCH + std::time::Duration::new(
        mtime_secs as u64,
        mtime_nanos,
    );
    
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(mtime))
}
```

---

### 🔍 Design Observations

#### 1. Orchestration vs Primitives

The rusty-attachments design correctly separates:
- **Primitives** (manifest-storage, path-mapping, manifest-utils): Low-level operations
- **Orchestration** (this example document): How to compose primitives

The Python `_incremental_download.py` is primarily orchestration code that:
- Calls Deadline APIs (SearchJobs, ListSessions, ListSessionActions)
- Manages checkpoint state
- Coordinates manifest discovery and download

This orchestration logic should remain in the consuming application (worker-agent), not in rusty-attachments core.

#### 2. Parallel Download Strategy

**Python:**
```python
max_workers = S3_DOWNLOAD_MAX_CONCURRENCY  # Typically 10
with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:
    futures = [executor.submit(_download_file, ...) for manifest_path in manifest_paths_to_download]
```

**Rust (CRT):**
The CRT handles parallelism internally, so the Rust design doesn't need explicit thread pool management. This is correctly noted in `storage-design.md`.

#### 3. Error Handling Granularity

The Python implementation has detailed error types for S3 operations:
- `JobAttachmentsS3ClientError` with status code guidance
- `JobAttachmentS3BotoCoreError` for boto errors
- `AssetSyncCancelledError` for cancellation

The Rust `StorageError` enum covers these cases but could benefit from more detailed guidance messages for common errors (403, 404).

---

### Conclusions

The rusty-attachments design is **well-suited for implementing incremental download**. The identified gaps are minor:

1. **Document root path hash computation** - Add `compute_root_path_hash()` to manifest-storage
2. **Document manifest index correspondence** - Clarify the 1:1 relationship between job manifests and session action manifests
3. **Ensure post-download size verification** - Verify in download orchestrator
4. **Document mtime handling** - Add mtime conversion and setting to download logic

No fundamental design changes are required. The primitives are complete and composable.

---

## Related Documents

- [manifest-storage.md](../manifest-storage.md) - Manifest discovery and download
- [manifest-utils.md](../manifest-utils.md) - Manifest merging
- [path-mapping.md](../path-mapping.md) - Path transformation
- [storage-design.md](../storage-design.md) - File download orchestration
- [example-worker-agent.md](example-worker-agent.md) - Worker agent integration analysis
