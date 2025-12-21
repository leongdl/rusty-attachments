# Example: Incremental Download Use Case

This example demonstrates how to use rusty-attachments primitives to implement incremental output downloading from Deadline Cloud jobs.

## Overview

Incremental download continuously polls for new job outputs and downloads them to the local machine. Key challenges:
- Jobs may use different storage profiles than the local machine
- Multiple manifests need to be merged chronologically
- Paths must be transformed from job's storage profile to local paths

## Primitives Used

| Primitive | Module | Purpose |
|-----------|--------|---------|
| `generate_path_mapping_rules()` | `storage-profiles` | Create rules from storage profile pairs |
| `PathMappingApplier` | `path-mapping` | Efficient trie-based path transformation |
| `resolve_manifest_paths()` | `path-mapping` | Convert manifest paths to absolute local paths |
| `download_to_resolved_paths()` | `storage-design` | Download files to pre-resolved paths |
| `discover_output_manifest_keys()` | `manifest-storage` | Find output manifests in S3 |
| `download_manifest()` | `manifest-storage` | Download manifest with metadata |
| `merge_manifests_chronologically()` | `manifest-utils` | Merge manifests by timestamp |

## Complete Example

```rust
use rusty_attachments_storage::{
    // Storage profiles
    StorageProfileWithId, StorageProfileOsFamily,
    generate_path_mapping_rules,
    // Path mapping
    PathMappingApplier, PathFormat,
    resolve_manifest_paths, ResolvedManifestPaths,
    // Manifest operations
    ManifestLocation, OutputManifestScope, OutputManifestDiscoveryOptions,
    discover_output_manifest_keys, download_manifest, ManifestDownloadMetadata,
    // Download
    DownloadOrchestrator, S3Location, ConflictResolution,
    TransferProgress, ProgressCallback,
};
use rusty_attachments_model::{Manifest, merge::merge_manifests_chronologically};
use std::collections::HashMap;

/// Download outputs for a specific job, handling storage profile differences.
async fn download_job_outputs<C: StorageClient>(
    orchestrator: &DownloadOrchestrator<C>,
    location: &ManifestLocation,
    job_id: &str,
    job_storage_profile: &StorageProfileWithId,
    local_storage_profile: &StorageProfileWithId,
    conflict_resolution: ConflictResolution,
    progress: Option<&dyn ProgressCallback<TransferProgress>>,
) -> Result<DownloadResult, StorageError> {
    // 1. Generate path mapping rules between storage profiles
    let rules = generate_path_mapping_rules(job_storage_profile, local_storage_profile);
    
    let applier = if rules.is_empty() {
        None
    } else {
        Some(PathMappingApplier::new(&rules)?)
    };
    
    let source_format = job_storage_profile.os_family.to_path_format();
    
    // 2. Discover output manifests for the job
    let options = OutputManifestDiscoveryOptions {
        scope: OutputManifestScope::Job { job_id: job_id.to_string() },
        select_latest_per_task: true,
    };
    
    let manifest_keys = discover_output_manifest_keys(
        &orchestrator.client(),
        location,
        &options,
    ).await?;
    
    // 3. Download all manifests with metadata
    let mut manifests_with_timestamps: Vec<(i64, Manifest, String)> = Vec::new();
    
    for key in &manifest_keys {
        let (manifest, metadata) = download_manifest(
            &orchestrator.client(),
            &location.bucket,
            key,
        ).await?;
        
        let timestamp = metadata.last_modified.unwrap_or(0);
        manifests_with_timestamps.push((timestamp, manifest, metadata.asset_root));
    }
    
    // 4. Group manifests by asset root and merge chronologically
    let mut by_root: HashMap<String, Vec<(i64, &Manifest)>> = HashMap::new();
    for (ts, manifest, root) in &manifests_with_timestamps {
        by_root.entry(root.clone())
            .or_default()
            .push((*ts, manifest));
    }
    
    let mut all_resolved: Vec<ResolvedManifestPath> = Vec::new();
    let mut all_unmapped: Vec<UnmappedPath> = Vec::new();
    
    for (root_path, mut manifests) in by_root {
        // Merge manifests for this root (later overwrites earlier)
        let merged = merge_manifests_chronologically(&mut manifests)?
            .expect("non-empty manifest list");
        
        // 5. Resolve paths with mapping
        let resolved = resolve_manifest_paths(
            &merged,
            &root_path,
            source_format,
            applier.as_ref(),
        )?;
        
        all_resolved.extend(resolved.resolved);
        all_unmapped.extend(resolved.unmapped);
    }
    
    // 6. Report unmapped paths
    if !all_unmapped.is_empty() {
        eprintln!("WARNING: {} paths could not be mapped:", all_unmapped.len());
        for unmapped in &all_unmapped {
            eprintln!("  {}", unmapped.absolute_source_path);
        }
    }
    
    // 7. Download to resolved paths
    let stats = orchestrator.download_to_resolved_paths(
        &all_resolved,
        merged.hash_alg(),
        conflict_resolution,
        progress,
    ).await?;
    
    Ok(DownloadResult {
        stats,
        unmapped_count: all_unmapped.len(),
    })
}

pub struct DownloadResult {
    pub stats: TransferStatistics,
    pub unmapped_count: usize,
}
```

## Handling Multiple Jobs

For incremental download across multiple jobs (like the Python `_incremental_output_download`):

```rust
/// Download outputs from multiple jobs with different storage profiles.
async fn download_multiple_jobs<C: StorageClient>(
    orchestrator: &DownloadOrchestrator<C>,
    location: &ManifestLocation,
    jobs: &[JobInfo],
    local_storage_profile: &StorageProfileWithId,
    storage_profiles: &HashMap<String, StorageProfileWithId>,
    conflict_resolution: ConflictResolution,
) -> Result<MultiJobDownloadResult, StorageError> {
    // Pre-compute path mapping appliers for each unique storage profile
    let mut appliers: HashMap<String, Option<PathMappingApplier>> = HashMap::new();
    
    for (profile_id, profile) in storage_profiles {
        let rules = generate_path_mapping_rules(profile, local_storage_profile);
        let applier = if rules.is_empty() {
            None
        } else {
            Some(PathMappingApplier::new(&rules)?)
        };
        appliers.insert(profile_id.clone(), applier);
    }
    
    let mut total_stats = TransferStatistics::default();
    let mut jobs_with_unmapped: Vec<String> = Vec::new();
    
    for job in jobs {
        let applier = appliers.get(&job.storage_profile_id)
            .and_then(|a| a.as_ref());
        
        let source_format = storage_profiles
            .get(&job.storage_profile_id)
            .map(|p| p.os_family.to_path_format())
            .unwrap_or(PathFormat::Posix);
        
        // Discover and download for this job...
        let result = download_job_outputs_inner(
            orchestrator,
            location,
            &job.job_id,
            source_format,
            applier,
            conflict_resolution,
            None,
        ).await?;
        
        total_stats.merge(&result.stats);
        if result.unmapped_count > 0 {
            jobs_with_unmapped.push(job.job_id.clone());
        }
    }
    
    Ok(MultiJobDownloadResult {
        stats: total_stats,
        jobs_with_unmapped,
    })
}

pub struct JobInfo {
    pub job_id: String,
    pub storage_profile_id: String,
}

pub struct MultiJobDownloadResult {
    pub stats: TransferStatistics,
    pub jobs_with_unmapped: Vec<String>,
}
```

## Session Action Filtering

For true incremental download, filter manifests by session action:

```rust
/// Filter manifest keys to only those from specific session actions.
fn filter_manifests_by_session_actions(
    manifest_keys: &[String],
    session_action_ids: &HashSet<String>,
) -> Vec<String> {
    manifest_keys
        .iter()
        .filter(|key| {
            // Extract session action ID from key path
            // Format: .../timestamp_sessionaction-xxx-N/hash_output
            if let Some(session_action_id) = extract_session_action_id(key) {
                session_action_ids.contains(&session_action_id)
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

fn extract_session_action_id(key: &str) -> Option<String> {
    // Pattern: sessionaction-{id}-{index}
    let re = regex::Regex::new(r"(sessionaction-[^/-]+-\d+)").ok()?;
    re.captures(key)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}
```

## Related Documents

- [storage-profiles.md](../storage-profiles.md) - `generate_path_mapping_rules()`
- [path-mapping.md](../path-mapping.md) - `PathMappingApplier`, `resolve_manifest_paths()`
- [storage-design.md](../storage-design.md) - `download_to_resolved_paths()`
- [manifest-storage.md](../manifest-storage.md) - Manifest discovery and download
- [manifest-utils.md](../manifest-utils.md) - Manifest merging
