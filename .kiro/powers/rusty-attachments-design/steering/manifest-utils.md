# Manifest Utilities Design Summary

**Full doc:** `design/manifest-utils.md`  
**Status:** Partially implemented in `crates/model/`

## Purpose
Diff and merge operations for manifests - comparing, combining, and applying changes.

## Diff Operations

### Compare Manifests
```rust
fn compare_manifests<'a>(
    reference: &'a Manifest,
    current: &'a Manifest,
) -> Result<Vec<DiffEntry<'a>>, VersionNotCompatibleError>;

enum FileStatus { Unchanged, New, Modified, Deleted }

struct DiffEntry<'a> {
    status: FileStatus,
    path: &'a str,
    reference: Option<&'a ManifestFilePath>,
    current: Option<&'a ManifestFilePath>,
}
```

### Fast Diff (No Hashing)
```rust
fn fast_diff_files(
    root: &Path,
    current_files: &[&str],
    manifest: &Manifest,
) -> io::Result<Vec<FastDiffEntry>>;
```
Compares by mtime/size only - faster but may miss content-only changes.

### Create Diff Manifest
```rust
fn create_diff_manifest(
    parent: &Manifest,
    parent_encoded: &str,
    current: &Manifest,
) -> Result<Manifest, ManifestError>;
```
Creates manifest with `manifest_type=Diff` and `parent_manifest_hash`.

### Apply Diff Manifest
```rust
fn apply_diff_manifest(
    base: &Manifest,
    diff: &Manifest,
) -> Result<Manifest, ManifestError>;
```
Inverse of create - applies diff to base to reconstruct current state.

## Merge Operations

### Basic Merge
```rust
fn merge_manifests(manifests: &[&Manifest]) -> Result<Option<Manifest>, ManifestError>;
```
Later manifests override earlier (last-write-wins).

### Chronological Merge
```rust
fn merge_manifests_chronologically(
    manifests_with_timestamps: &mut [(i64, &Manifest)],
) -> Result<Option<Manifest>, ManifestError>;
```
Sorts by timestamp, then merges (newest wins for conflicts).

## ManifestPathGroup

For efficient download aggregation:
```rust
struct ManifestPathGroup {
    total_bytes: u64,
    files_by_hash_alg: HashMap<HashAlgorithm, Vec<ManifestFilePath>>,
}

impl ManifestPathGroup {
    fn add_manifest(&mut self, manifest: &Manifest);
    fn combine(&mut self, other: &ManifestPathGroup);
    fn all_paths(&self) -> Vec<&str>;
    fn file_count(&self) -> usize;
}
```

## When to Read Full Doc
- Implementing manifest comparison
- Understanding diff manifest format
- Merge semantics and ordering
- Download path aggregation
