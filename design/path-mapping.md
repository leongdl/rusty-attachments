# Rusty Attachments: Path Mapping Design

## Overview

Path mapping enables translation between source paths (where files originated) and destination paths (where files should be placed). This is essential for:

1. **Job Submission**: Mapping submitter paths to worker session directories
2. **Download**: Mapping manifest asset roots to local destinations
3. **Cross-Platform**: Handling Windows ↔ POSIX path format differences

---

## Data Structures

### Path Format

```rust
/// Operating system path format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    
    /// Get the path separator for this format.
    pub fn separator(&self) -> char {
        match self {
            PathFormat::Windows => '\\',
            PathFormat::Posix => '/',
        }
    }
}
```

### Path Mapping Rule

```rust
/// A rule for mapping source paths to destination paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMappingRule {
    /// The path format of the source path (windows or posix).
    pub source_path_format: PathFormat,
    /// The source path to match against.
    pub source_path: String,
    /// The destination path to transform to.
    pub destination_path: String,
}

impl PathMappingRule {
    /// Create a new path mapping rule.
    pub fn new(
        source_path_format: PathFormat,
        source_path: impl Into<String>,
        destination_path: impl Into<String>,
    ) -> Self {
        Self {
            source_path_format,
            source_path: source_path.into(),
            destination_path: destination_path.into(),
        }
    }
    
    /// Check if a path matches this rule's source path.
    pub fn matches(&self, path: &str) -> bool {
        self.normalize_for_comparison(path)
            .starts_with(&self.normalize_for_comparison(&self.source_path))
    }
    
    /// Apply this rule to transform a path.
    /// Returns None if the path doesn't match this rule.
    pub fn apply(&self, path: &str) -> Option<String> {
        let normalized_path = self.normalize_for_comparison(path);
        let normalized_source = self.normalize_for_comparison(&self.source_path);
        
        if let Some(relative) = normalized_path.strip_prefix(&normalized_source) {
            let relative = relative.trim_start_matches('/').trim_start_matches('\\');
            if relative.is_empty() {
                Some(self.destination_path.clone())
            } else {
                Some(format!("{}/{}", self.destination_path, relative))
            }
        } else {
            None
        }
    }
    
    /// Normalize a path for comparison (convert separators, lowercase on Windows).
    fn normalize_for_comparison(&self, path: &str) -> String {
        let normalized = path.replace('\\', "/");
        if self.source_path_format == PathFormat::Windows {
            normalized.to_lowercase()
        } else {
            normalized
        }
    }
    
    /// Compute a hash of the source path for manifest naming.
    pub fn hashed_source_path(&self, hash_alg: HashAlgorithm) -> String {
        hash_data(self.source_path.as_bytes(), hash_alg)
    }
}
```

---

## Unique Destination Directory Naming

When downloading files without a storage profile, we generate unique directory names to prevent path collisions across different asset roots.

### Algorithm

```rust
use sha3::{Shake256, digest::{Update, ExtendableOutput, XofReader}};

/// Generate a unique destination directory name from a source root path.
/// 
/// Uses SHAKE-256 to create a short, deterministic hash of the source path.
/// Format: `assetroot-{20_char_hex}`
/// 
/// # Example
/// ```
/// let dir_name = get_unique_dest_dir_name("/mnt/projects/job1");
/// // Returns something like: "assetroot-a1b2c3d4e5f6g7h8i9j0"
/// ```
pub fn get_unique_dest_dir_name(source_root: &str) -> String {
    let mut hasher = Shake256::default();
    hasher.update(source_root.as_bytes());
    
    let mut reader = hasher.finalize_xof();
    let mut hash_bytes = [0u8; 10]; // 10 bytes = 20 hex chars
    reader.read(&mut hash_bytes);
    
    format!("assetroot-{}", hex::encode(hash_bytes))
}
```

---

## Dynamic Path Mapping Generation

For job submissions without storage profiles, we generate path mappings dynamically based on the session directory.

### Generate Dynamic Mappings

```rust
/// Generate path mapping rules for manifests without file system location names.
/// 
/// For each manifest that doesn't have a `fileSystemLocationName`, creates a
/// mapping from the original root path to a unique subdirectory under the
/// session directory.
/// 
/// # Arguments
/// * `session_dir` - The worker's session directory
/// * `manifests` - List of manifest properties from job attachments
/// 
/// # Returns
/// Map of original root path → PathMappingRule
pub fn generate_dynamic_path_mapping(
    session_dir: &Path,
    manifests: &[ManifestProperties],
) -> HashMap<String, PathMappingRule> {
    let mut mappings = HashMap::new();
    
    for manifest in manifests {
        // Skip manifests that have a file system location (handled by storage profiles)
        if manifest.file_system_location_name.is_some() {
            continue;
        }
        
        let dir_name = get_unique_dest_dir_name(&manifest.root_path);
        let local_root = session_dir.join(&dir_name);
        
        mappings.insert(
            manifest.root_path.clone(),
            PathMappingRule::new(
                manifest.root_path_format,
                &manifest.root_path,
                local_root.to_string_lossy(),
            ),
        );
    }
    
    mappings
}
```

### Resolve Local Destination

```rust
/// Resolve the local destination for a manifest.
/// 
/// Checks dynamic mappings first, then falls back to storage profile mappings.
/// 
/// # Arguments
/// * `manifest` - The manifest properties to resolve
/// * `dynamic_mappings` - Mappings generated by `generate_dynamic_path_mapping`
/// * `storage_profile_mappings` - Mappings from storage profile (source → dest)
/// 
/// # Returns
/// The local destination path, or error if no mapping found.
pub fn get_local_destination(
    manifest: &ManifestProperties,
    dynamic_mappings: &HashMap<String, PathMappingRule>,
    storage_profile_mappings: &HashMap<String, String>,
) -> Result<String, PathMappingError> {
    if manifest.file_system_location_name.is_some() {
        // Use storage profile mapping
        storage_profile_mappings
            .get(&manifest.root_path)
            .cloned()
            .ok_or_else(|| PathMappingError::NoMappingFound {
                root_path: manifest.root_path.clone(),
            })
    } else {
        // Use dynamic mapping
        dynamic_mappings
            .get(&manifest.root_path)
            .map(|rule| rule.destination_path.clone())
            .ok_or_else(|| PathMappingError::NoMappingFound {
                root_path: manifest.root_path.clone(),
            })
    }
}
```

---

## Path Transformation

### Transform Manifest Path to Local Path

```rust
/// Transform a relative path from a manifest to a local filesystem path.
/// 
/// Handles path separator conversion between Windows and POSIX formats.
/// 
/// # Arguments
/// * `manifest_path` - Relative path from manifest (always uses `/` separators)
/// * `local_root` - Local root directory
/// * `source_format` - Original path format from manifest
/// 
/// # Returns
/// Absolute local path
pub fn manifest_path_to_local(
    manifest_path: &str,
    local_root: &Path,
    source_format: PathFormat,
) -> PathBuf {
    // Manifest paths always use forward slashes
    let components: Vec<&str> = manifest_path.split('/').collect();
    
    let mut result = local_root.to_path_buf();
    for component in components {
        if !component.is_empty() {
            result.push(component);
        }
    }
    
    result
}

/// Transform a local filesystem path to a manifest-relative path.
/// 
/// # Arguments
/// * `local_path` - Absolute local path
/// * `local_root` - Local root directory
/// 
/// # Returns
/// Relative path using forward slashes (manifest format)
pub fn local_path_to_manifest(
    local_path: &Path,
    local_root: &Path,
) -> Result<String, PathMappingError> {
    let relative = local_path
        .strip_prefix(local_root)
        .map_err(|_| PathMappingError::PathOutsideRoot {
            path: local_path.display().to_string(),
            root: local_root.display().to_string(),
        })?;
    
    // Convert to forward slashes for manifest format
    Ok(relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}
```

---

## Cross-Platform Path Handling

### Convert Output Directory Paths

When processing output directories from manifests, handle path format differences:

```rust
/// Convert an output directory path to the current platform format.
/// 
/// # Arguments
/// * `output_dir` - Output directory path from manifest
/// * `source_format` - Path format the directory was created with
/// 
/// # Returns
/// Path converted to current platform format
pub fn convert_output_dir_path(
    output_dir: &str,
    source_format: PathFormat,
) -> String {
    let current_format = PathFormat::host();
    
    if source_format == current_format {
        return output_dir.to_string();
    }
    
    match source_format {
        PathFormat::Windows => output_dir.replace('\\', "/"),
        PathFormat::Posix => output_dir.replace('/', "\\"),
    }
}
```

---

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum PathMappingError {
    #[error("No path mapping rule found for root path: {root_path}")]
    NoMappingFound { root_path: String },
    
    #[error("Path {path} is outside root {root}")]
    PathOutsideRoot { path: String, root: String },
    
    #[error("Invalid path format: {path}")]
    InvalidPath { path: String },
}
```

---

## Usage Examples

### Job Submission Path Mapping

```rust
use rusty_attachments_storage::path_mapping::{
    generate_dynamic_path_mapping, get_local_destination, PathMappingRule,
};

// On worker: generate mappings for session
let session_dir = Path::new("/var/lib/deadline/session-123");
let dynamic_mappings = generate_dynamic_path_mapping(session_dir, &attachments.manifests);

// Resolve destination for each manifest
for manifest in &attachments.manifests {
    let local_dest = get_local_destination(
        manifest,
        &dynamic_mappings,
        &storage_profile_mappings,
    )?;
    
    println!("Manifest root {} -> local {}", manifest.root_path, local_dest);
}

// Convert mappings to job submission format
let path_mapping_rules: Vec<_> = dynamic_mappings
    .values()
    .map(|rule| serde_json::json!({
        "sourcePathFormat": rule.source_path_format,
        "sourcePath": rule.source_path,
        "destinationPath": rule.destination_path,
    }))
    .collect();
```

### Download Path Resolution

```rust
use rusty_attachments_storage::path_mapping::manifest_path_to_local;

let manifest = download_manifest(&client, bucket, key).await?;
let local_root = Path::new("/var/lib/deadline/session-123/assetroot-abc123");

for file in manifest.files() {
    let local_path = manifest_path_to_local(
        &file.path,
        local_root,
        manifest_properties.root_path_format,
    );
    
    // Download file to local_path
    download_file(&client, &file.hash, &local_path).await?;
}
```

---

## Project Structure

```
rusty-attachments/
├── crates/
│   ├── storage/
│   │   └── src/
│   │       ├── path_mapping/
│   │       │   ├── mod.rs           # Public API
│   │       │   ├── rule.rs          # PathMappingRule
│   │       │   ├── dynamic.rs       # Dynamic mapping generation
│   │       │   ├── transform.rs     # Path transformation utilities
│   │       │   └── error.rs         # Error types
│   │       └── ...
```

---

## Related Documents

- [storage-profiles.md](storage-profiles.md) - Storage profile and file system location types
- [job-submission.md](job-submission.md) - Converting manifests to job attachments format
- [manifest-storage.md](manifest-storage.md) - Manifest upload/download operations
