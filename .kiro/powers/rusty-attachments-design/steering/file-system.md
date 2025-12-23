# File System Design Summary

**Full doc:** `design/file_system.md`  
**Status:** ✅ IMPLEMENTED in `crates/filesystem/`

## Purpose
Directory scanning, manifest creation, and diff operations with glob filtering.

## Key Types

### Filtering
```rust
struct GlobFilter {
    include: Vec<String>,  // Empty = include all
    exclude: Vec<String>,
}
```
Patterns: `*`, `**`, `?`, `[abc]`, `[!abc]`, `{a,b}`

### Snapshot Options
```rust
struct SnapshotOptions {
    root: PathBuf,
    input_files: Option<Vec<PathBuf>>,  // None = walk directory
    version: ManifestVersion,
    filter: GlobFilter,
    hash_algorithm: HashAlgorithm,
    follow_symlinks: bool,
    include_empty_dirs: bool,
    hash_cache: Option<PathBuf>,
    parallelism: usize,
}
```

### Diff Options
```rust
struct DiffOptions {
    root: PathBuf,
    filter: GlobFilter,
    mode: DiffMode,  // Fast (mtime/size) or Hash
    hash_cache: Option<PathBuf>,
    parallelism: usize,
}
```

### Results
- `DiffResult`: added, modified, deleted, unchanged files/dirs, stats
- `ExpandedInputPaths`: files, expanded_directories, missing, total_size

## Key Functions

### Input Path Handling
- `expand_input_paths()`: Expand directories to files, apply filters
- `validate_input_paths()`: Categorize as valid_files, missing, directories

### Scanner Operations
```rust
impl FileSystemScanner {
    fn snapshot(&self, options: &SnapshotOptions, progress: Option<...>) -> Result<Manifest>;
    fn snapshot_structure(&self, options: &SnapshotOptions, progress: Option<...>) -> Result<Manifest>;
    fn diff(&self, manifest: &Manifest, options: &DiffOptions, progress: Option<...>) -> Result<DiffResult>;
    fn create_diff_manifest(&self, parent: &Manifest, parent_bytes: &[u8], diff: &DiffResult, options: &DiffOptions) -> Result<Manifest>;
}
```

## Security
- Symlink validation: target must be within root
- Path traversal prevention via `is_within_root()`

## When to Read Full Doc
- Implementing directory scanning
- Adding new filter patterns
- Understanding diff algorithms
- Symlink security validation
