# Rusty Attachments: Utilities

## Overview

This document defines utility functions that support CLI and application-level operations. These are convenience functions that compose core primitives for common use cases.

---

## Known Asset Path Filtering

When collecting known asset paths from multiple sources (CLI arguments, storage profiles, config files), paths may overlap. This utility removes redundant paths where one is a prefix of another.

### Algorithm

The algorithm uses a TRIE (prefix tree) to efficiently detect prefix relationships:

1. Sort paths from shortest to longest (prefixes always come first)
2. For each path, split into components and insert into TRIE
3. While inserting, detect if another path is already a prefix
4. Filter out paths that have another path as a prefix

### Implementation

```rust
use std::collections::HashMap;
use std::path::Path;

/// Filter out redundant paths from a list of known asset paths.
///
/// Removes any path that has another path in the list as a prefix.
/// This reduces processing overhead and produces shorter warning messages.
///
/// # Example
/// ```
/// use rusty_attachments_storage::utilities::filter_redundant_known_paths;
///
/// let paths = vec![
///     "/mnt/projects".to_string(),
///     "/mnt/projects/job1".to_string(),      // redundant (prefix: /mnt/projects)
///     "/mnt/projects/job1/assets".to_string(), // redundant (prefix: /mnt/projects)
///     "/mnt/shared".to_string(),
/// ];
///
/// let filtered = filter_redundant_known_paths(&paths);
/// assert_eq!(filtered, vec!["/mnt/projects", "/mnt/shared"]);
/// ```
pub fn filter_redundant_known_paths(known_asset_paths: &[String]) -> Vec<String> {
    // TRIE node: maps path component -> child node
    // A `true` value indicates a complete path ends at this node
    type TrieNode = HashMap<String, TrieValue>;
    
    enum TrieValue {
        Terminal,           // A known path ends here
        Children(TrieNode), // More path components follow
    }
    
    let mut trie: TrieNode = HashMap::new();
    let mut filtered_paths: Vec<String> = Vec::new();
    
    // Process paths from shortest to longest so prefixes are seen first
    let mut sorted_paths: Vec<&String> = known_asset_paths.iter().collect();
    sorted_paths.sort_by_key(|p| p.len());
    
    for path in sorted_paths {
        let parts: Vec<&str> = Path::new(path)
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        
        if parts.is_empty() {
            continue;
        }
        
        let mut current = &mut trie;
        let mut is_prefix_of_existing = false;
        
        // Walk the TRIE, checking for existing prefixes
        for (i, part) in parts.iter().enumerate() {
            let part_string = part.to_string();
            
            match current.get(&part_string) {
                Some(TrieValue::Terminal) => {
                    // Another path is a prefix of this one - skip it
                    is_prefix_of_existing = true;
                    break;
                }
                Some(TrieValue::Children(_)) => {
                    // Continue traversing
                    if let Some(TrieValue::Children(children)) = current.get_mut(&part_string) {
                        current = children;
                    }
                }
                None => {
                    // New path component - insert it
                    if i == parts.len() - 1 {
                        // Last component - mark as terminal
                        current.insert(part_string, TrieValue::Terminal);
                    } else {
                        // Intermediate component - create children node
                        current.insert(part_string.clone(), TrieValue::Children(HashMap::new()));
                        if let Some(TrieValue::Children(children)) = current.get_mut(&part_string) {
                            current = children;
                        }
                    }
                }
            }
        }
        
        if !is_prefix_of_existing {
            filtered_paths.push(path.clone());
        }
    }
    
    filtered_paths
}
```

### Simplified Implementation

For simpler use cases, a straightforward O(n²) implementation:

```rust
/// Filter redundant paths using simple prefix checking.
///
/// Less efficient than TRIE-based approach but simpler.
/// Suitable for small path lists (< 100 paths).
pub fn filter_redundant_known_paths_simple(known_asset_paths: &[String]) -> Vec<String> {
    let mut sorted: Vec<&String> = known_asset_paths.iter().collect();
    sorted.sort_by_key(|p| p.len());
    
    let mut result: Vec<String> = Vec::new();
    
    for path in sorted {
        let path_obj = Path::new(path);
        
        // Check if any existing result is a prefix of this path
        let has_prefix = result.iter().any(|existing| {
            let existing_path = Path::new(existing);
            path_obj.starts_with(existing_path)
        });
        
        if !has_prefix {
            result.push(path.clone());
        }
    }
    
    result
}
```

---

## Path Classification for Warnings

When submitting jobs, paths outside known locations generate warnings. This utility classifies paths for user feedback.

### Implementation

```rust
use std::collections::HashSet;
use std::path::Path;

/// Result of classifying paths against known locations.
#[derive(Debug, Clone, Default)]
pub struct PathClassification {
    /// Paths that are under known locations (no warning needed)
    pub known_paths: HashSet<String>,
    /// Paths that are outside all known locations (generate warning)
    pub unknown_paths: HashSet<String>,
}

/// Classify paths as known or unknown based on known asset path prefixes.
///
/// # Arguments
/// * `paths` - Paths to classify
/// * `known_prefixes` - Known asset path prefixes (already filtered for redundancy)
///
/// # Example
/// ```
/// use rusty_attachments_storage::utilities::classify_paths;
///
/// let paths = vec![
///     "/mnt/projects/job1/scene.blend",
///     "/home/user/downloads/texture.png",
/// ];
/// let known = vec!["/mnt/projects".to_string()];
///
/// let result = classify_paths(&paths, &known);
/// assert!(result.known_paths.contains("/mnt/projects/job1/scene.blend"));
/// assert!(result.unknown_paths.contains("/home/user/downloads/texture.png"));
/// ```
pub fn classify_paths(
    paths: &[impl AsRef<str>],
    known_prefixes: &[String],
) -> PathClassification {
    let mut result = PathClassification::default();
    
    for path in paths {
        let path_str = path.as_ref();
        let path_obj = Path::new(path_str);
        
        let is_known = known_prefixes.iter().any(|prefix| {
            path_obj.starts_with(Path::new(prefix))
        });
        
        if is_known {
            result.known_paths.insert(path_str.to_string());
        } else {
            result.unknown_paths.insert(path_str.to_string());
        }
    }
    
    result
}
```

---

## Collect Known Asset Paths

Aggregate known asset paths from multiple sources for job submission.

### Implementation

```rust
use std::path::Path;

use crate::storage_profiles::{StorageProfile, FileSystemLocationType};

/// Sources for known asset paths.
#[derive(Debug, Clone, Default)]
pub struct KnownAssetPathSources<'a> {
    /// Paths explicitly provided (e.g., CLI arguments)
    pub explicit_paths: &'a [String],
    /// Job bundle directory (always known)
    pub job_bundle_dir: Option<&'a Path>,
    /// Storage profile with LOCAL locations
    pub storage_profile: Option<&'a StorageProfile>,
    /// Paths from configuration file
    pub config_paths: &'a [String],
    /// PATH-type job parameter values
    pub parameter_paths: &'a [String],
}

/// Collect and deduplicate known asset paths from all sources.
///
/// # Example
/// ```
/// use rusty_attachments_storage::utilities::{
///     collect_known_asset_paths, KnownAssetPathSources,
/// };
///
/// let sources = KnownAssetPathSources {
///     explicit_paths: &["--known-asset-path".to_string()],
///     job_bundle_dir: Some(Path::new("/path/to/bundle")),
///     storage_profile: Some(&profile),
///     config_paths: &[],
///     parameter_paths: &[],
/// };
///
/// let known_paths = collect_known_asset_paths(&sources);
/// ```
pub fn collect_known_asset_paths(sources: &KnownAssetPathSources<'_>) -> Vec<String> {
    let mut all_paths: Vec<String> = Vec::new();
    
    // Add explicit paths
    all_paths.extend(sources.explicit_paths.iter().cloned());
    
    // Add job bundle directory
    if let Some(bundle_dir) = sources.job_bundle_dir {
        if let Some(abs_path) = bundle_dir.canonicalize().ok() {
            all_paths.push(abs_path.to_string_lossy().to_string());
        }
    }
    
    // Add LOCAL storage profile locations
    if let Some(profile) = sources.storage_profile {
        for location in &profile.file_system_locations {
            if location.location_type == FileSystemLocationType::Local {
                all_paths.push(location.path.clone());
            }
        }
    }
    
    // Add config paths
    all_paths.extend(sources.config_paths.iter().cloned());
    
    // Add parameter paths
    all_paths.extend(sources.parameter_paths.iter().cloned());
    
    // Filter redundant paths
    filter_redundant_known_paths(&all_paths)
}
```

---

## Project Structure

```
rusty-attachments/
├── crates/
│   ├── storage/
│   │   └── src/
│   │       ├── utilities/
│   │       │   ├── mod.rs              # Public API
│   │       │   ├── known_paths.rs      # Known path filtering
│   │       │   └── classification.rs   # Path classification
│   │       └── ...
```

---

## Warning Message Generation

When submitting jobs, paths outside known locations generate user-facing warnings. This utility formats those warnings for CLI output.

### Implementation

```rust
/// Generate a warning message for asset paths outside known locations.
///
/// # Arguments
/// * `unknown_paths` - Paths that are outside all known asset locations
/// * `known_prefixes` - The known asset path prefixes that were checked
///
/// # Returns
/// A formatted warning message, or None if there are no unknown paths.
///
/// # Example
/// ```
/// use rusty_attachments_storage::utilities::generate_message_for_asset_paths;
///
/// let unknown = vec![
///     "/home/user/downloads/texture.png".to_string(),
///     "/tmp/cache/model.obj".to_string(),
/// ];
/// let known = vec!["/mnt/projects".to_string()];
///
/// if let Some(msg) = generate_message_for_asset_paths(&unknown, &known) {
///     eprintln!("Warning: {}", msg);
/// }
/// ```
pub fn generate_message_for_asset_paths(
    unknown_paths: &[String],
    known_prefixes: &[String],
) -> Option<String> {
    if unknown_paths.is_empty() {
        return None;
    }
    
    let mut message = String::new();
    
    // Header
    message.push_str(&format!(
        "Found {} path(s) outside of known asset locations:\n",
        unknown_paths.len()
    ));
    
    // List unknown paths (limit to first 10 to avoid overwhelming output)
    let display_count = unknown_paths.len().min(10);
    for path in &unknown_paths[..display_count] {
        message.push_str(&format!("  - {}\n", path));
    }
    if unknown_paths.len() > 10 {
        message.push_str(&format!("  ... and {} more\n", unknown_paths.len() - 10));
    }
    
    // Show known locations for context
    if !known_prefixes.is_empty() {
        message.push_str("\nKnown asset locations:\n");
        for prefix in known_prefixes {
            message.push_str(&format!("  - {}\n", prefix));
        }
    }
    
    // Suggestion
    message.push_str("\nConsider adding these paths to your storage profile or using --known-asset-path.");
    
    Some(message)
}

/// Format a summary of path classification results for CLI output.
///
/// # Arguments
/// * `classification` - Result from `classify_paths()`
/// * `known_prefixes` - The known asset path prefixes
///
/// # Returns
/// A formatted summary suitable for verbose CLI output.
pub fn format_path_classification_summary(
    classification: &PathClassification,
    known_prefixes: &[String],
) -> String {
    let mut summary = String::new();
    
    summary.push_str(&format!(
        "Path classification: {} known, {} unknown\n",
        classification.known_paths.len(),
        classification.unknown_paths.len()
    ));
    
    if !classification.unknown_paths.is_empty() {
        if let Some(warning) = generate_message_for_asset_paths(
            &classification.unknown_paths.iter().cloned().collect::<Vec<_>>(),
            known_prefixes,
        ) {
            summary.push_str(&warning);
        }
    }
    
    summary
}
```

---

## Related Documents

- [storage-profiles.md](storage-profiles.md) - Storage profile types used in path collection
- [examples/example-bundle-submit.md](examples/example-bundle-submit.md) - Usage in bundle submit flow
