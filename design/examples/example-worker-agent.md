# Rusty Attachments: Worker Agent Integration Analysis

## Overview

This document analyzes the Python worker-agent implementation of job attachment download/upload to validate that the rusty-attachments design covers all necessary primitives. It documents the code flow, identifies gaps, and provides conclusions about design completeness.

---

## Python Implementation Analysis

### Download Flow (deadline-cloud-worker-agent)

The worker agent download flow follows this sequence:

```
AttachmentDownloadAction.start()
  ↓
1. Aggregate manifests (_aggregate_asset_root_manifests)
   - Fetches input manifests from S3
   - Handles step-step dependencies
   - Merges manifests by asset root
  ↓
2. Generate dynamic path mappings
   - Creates unique session directories
   - Maps source paths to local session paths
   - Updates OpenJD session path mapping rules
  ↓
3. Write local manifests (_check_and_write_local_manifests)
   - Writes merged manifests to session directory
   - Creates manifest S3 key mapping file
  ↓
4. Create WorkerManifestProperties
   - Bundles manifest metadata with local paths
   - Tracks input/output directories
   - Serializes to JSON for subprocess
  ↓
5. Spawn subprocess: attachment_download.py
   ↓
   a. load_worker_manifest_properties()
      - Deserializes WorkerManifestProperties from JSON
   ↓
   b. build_merged_manifests_by_root()
      - Reads local manifest files
      - Decodes to BaseAssetManifest objects
   ↓
   c. perform_download()
      - Calls download_files_from_manifests()
      - Progress tracking with low transfer rate detection
      - Parallel download via ThreadPoolExecutor
      - Per-file: download_file() → boto3 TransferManager
      - Conflict resolution (_get_new_copy_file_path)
      - File permission setting (_set_fs_group)
```

**Key Python Functions:**
- `download_files_from_manifests()` - Main download orchestrator
- `download_file()` - Single file download with progress tracking
- `_download_files_parallel()` - ThreadPoolExecutor coordination
- `_get_new_copy_file_path()` - Atomic conflict resolution with file locking

### Upload Flow (deadline-cloud-worker-agent)

The worker agent upload flow follows this sequence:

```
AttachmentUploadAction.start()
  ↓
1. Get WorkerManifestProperties from session
   - Retrieves all manifest metadata
   - Includes local paths and output directories
  ↓
2. Spawn subprocess: attachment_upload.py
   ↓
   a. parse_worker_manifest_properties()
      - Deserializes from JSON
   ↓
   b. merge()
      - Calls _manifest_merge() for each root
      - Combines input manifests chronologically
      - Creates base manifest for diff comparison
   ↓
   c. snapshot()
      - Calls _manifest_snapshot() for each root
      - Compares output directories against base
      - Creates diff manifest with only changes
      - Uses glob patterns for output filtering
   ↓
   d. upload_output_assets()
      - Calls S3AssetUploader.upload_assets()
      - Verifies S3 check cache integrity
      - Separates files into small/large queues
      - Parallel upload for small files
      - Serial upload for large files (parallel multipart)
      - Uses S3CheckCache to skip existing files
      - Uploads manifest with metadata
```

**Key Python Functions:**
- `S3AssetUploader.upload_assets()` - Main upload orchestrator
- `upload_input_files()` - CAS upload with caching
- `upload_object_to_cas()` - Single object upload
- `_separate_files_by_size()` - Queue splitting for optimization
- `verify_hash_cache_integrity()` - Cache validation with sampling

---

## Design Coverage Assessment

### Core Primitives Coverage

| Python Component | Rusty Design Document | Coverage Status |
|-----------------|----------------------|-----------------|
| **Data Structures** | | |
| Manifest v2023-03-03 | `model-design.md` | ✅ Complete |
| Manifest v2025-12-04-beta | `model-design.md` | ✅ Complete |
| ManifestProperties | `job-submission.md` | ✅ Complete |
| PathMappingRule | `path-mapping.md` | ✅ Complete |
| StorageProfile | `storage-profiles.md` | ✅ Complete |
| **Manifest Operations** | | |
| Encode/Decode | `model-design.md` | ✅ Complete |
| Diff (compare_manifest) | `manifest-utils.md` | ✅ Complete |
| Merge (merge_asset_manifests) | `manifest-utils.md` | ✅ Complete |
| Snapshot (_manifest_snapshot) | `file_system.md` | ✅ Complete |
| **File System Operations** | | |
| Directory walking | `file_system.md` | ✅ Complete |
| Glob filtering | `file_system.md` | ✅ Complete |
| File hashing | `common.md` | ✅ Complete |
| Symlink handling | `file_system.md` | ✅ Complete |
| **Storage Operations** | | |
| S3 upload (CAS) | `storage-design.md` | ✅ Complete |
| S3 download (CAS) | `storage-design.md` | ✅ Complete |
| Manifest upload | `manifest-storage.md` | ✅ Complete |
| Manifest download | `manifest-storage.md` | ✅ Complete |
| Output manifest discovery | `manifest-storage.md` | ✅ Complete |
| **Caching** | | |
| Hash cache (local) | `hash-cache.md` | ✅ Complete |
| S3 check cache | `hash-cache.md` | ✅ Complete |
| Cache integrity verification | `hash-cache.md` | ✅ Complete |
| **Path Operations** | | |
| Path normalization | `common.md` | ✅ Complete |
| Path mapping rules | `path-mapping.md` | ✅ Complete |
| Dynamic path mapping | `path-mapping.md` | ✅ Complete |
| Unique directory naming | `path-mapping.md` | ✅ Complete |
| **Progress & Error Handling** | | |
| Progress callbacks | `common.md` + `storage-design.md` | ✅ Complete |
| Cancellation support | `storage-design.md` | ✅ Complete |
| Error types | All design docs | ✅ Complete |
| **Download Features** | | |
| Conflict resolution | `storage-design.md` | ✅ Complete |
| File permissions | `file_system.md` | ✅ Complete |
| Parallel downloads | `storage-design.md` | ✅ Complete |
| **Upload Features** | | |
| Small/large file separation | `storage-design.md` | ✅ Complete |
| Parallel small file upload | `storage-design.md` | ✅ Complete |
| Serial large file upload | `storage-design.md` | ✅ Complete |

---

## Identified Gaps

### 1. WorkerManifestProperties Data Structure

**Python Implementation:**
```python
class WorkerManifestProperties:
    """Worker-specific manifest properties with local paths."""
    
    def __init__(
        self,
        manifest_properties: ManifestProperties,
        local_root_path: str,
        local_manifest_paths: Optional[List[str]] = None,
        local_input_manifest_path: Optional[str] = None,
    ):
        self.manifest_properties = manifest_properties
        self.local_root_path = local_root_path
        self.local_manifest_paths = local_manifest_paths or []
        self.local_input_manifest_path = local_input_manifest_path
    
    def as_output_metadata(self) -> dict:
        """Generate S3 metadata for output manifest uploads."""
        # Handles ASCII/non-ASCII paths
        # Returns {"Metadata": {"asset-root": ..., "file-system-location-name": ...}}
    
    def to_path_mapping_rule(self) -> PathMappingRule:
        """Convert to OpenJD path mapping rule."""
    
    def get_hashed_source_path(self) -> str:
        """Generate hash for manifest naming."""
    
    def local_output_relative_directories(self) -> Optional[List[str]]:
        """Convert output dirs to local path format."""
```

**Rust Design Status:** ⚠️ **Not explicitly designed**

**Analysis:**
- This is an **orchestration-level data structure**, not a core primitive
- Bundles job submission format (`ManifestProperties`) with worker execution state
- Provides convenience methods for common transformations
- Used for serialization between worker-agent process and subprocess

**Recommendation:**
- **Option A:** Document as an example orchestration pattern in this file
- **Option B:** Leave to consuming applications (worker-agent can define it)
- **Option C:** Add to `job-submission.md` as an optional extension

**Proposed Rust Equivalent:**
```rust
/// Worker-specific manifest properties with local paths.
/// 
/// This structure extends ManifestProperties with worker execution state,
/// bridging the gap between job submission format and worker operations.
pub struct WorkerManifestProperties {
    /// Original manifest properties from job submission
    pub manifest_properties: ManifestProperties,
    /// Local root path where files are downloaded
    pub local_root_path: PathBuf,
    /// Local paths to all manifest files for this root
    pub local_manifest_paths: Vec<PathBuf>,
    /// Local path to the input manifest file
    pub local_input_manifest_path: Option<PathBuf>,
}

impl WorkerManifestProperties {
    /// Generate S3 metadata for output manifest uploads.
    pub fn as_output_metadata(&self) -> ManifestS3Metadata {
        ManifestS3Metadata {
            asset_root: self.manifest_properties.root_path.clone(),
            file_system_location_name: self.manifest_properties.file_system_location_name.clone(),
        }
    }
    
    /// Convert to path mapping rule for OpenJD session.
    pub fn to_path_mapping_rule(&self) -> PathMappingRule {
        PathMappingRule::new(
            self.manifest_properties.root_path_format,
            &self.manifest_properties.root_path,
            &self.local_root_path,
        )
    }
    
    /// Generate hashed source path for manifest naming.
    pub fn get_hashed_source_path(&self) -> String {
        let source = format!(
            "{}{}",
            self.manifest_properties.file_system_location_name.as_deref().unwrap_or(""),
            self.manifest_properties.root_path
        );
        hash_string(&source, HashAlgorithm::Xxh128)
    }
    
    /// Convert output directories to local path format.
    pub fn local_output_relative_directories(&self) -> Option<Vec<PathBuf>> {
        self.manifest_properties.output_relative_directories.as_ref().map(|dirs| {
            dirs.iter()
                .map(|dir| {
                    // Convert from source format to local format
                    if self.manifest_properties.root_path_format != PathFormat::host() {
                        // Path format conversion logic
                        convert_path_format(dir, self.manifest_properties.root_path_format, PathFormat::host())
                    } else {
                        PathBuf::from(dir)
                    }
                })
                .collect()
        })
    }
}
```

### 2. Low Transfer Rate Detection

**Python Implementation:**
```python
LOW_TRANSFER_RATE_THRESHOLD = 10 * 10**3  # 10 KB/s
LOW_TRANSFER_COUNT_THRESHOLD = 60  # 60 reports = ~1 minute

def progress_handler(status: ProgressReportMetadata) -> bool:
    """Cancel download if transfer rate stays below threshold."""
    nonlocal low_transfer_count, last_processed_files
    
    # File completion acts as heartbeat
    if status.processedFiles > last_processed_files:
        low_transfer_count = 0
    last_processed_files = status.processedFiles
    
    if status.transferRate < LOW_TRANSFER_RATE_THRESHOLD:
        low_transfer_count += 1
    else:
        low_transfer_count = 0
        
    if low_transfer_count >= LOW_TRANSFER_COUNT_THRESHOLD:
        print(f"openjd_fail: Transfer rate too low")
        return False  # Cancel
    
    return True  # Continue
```

**Rust Design Status:** ⚠️ **Not explicitly designed**

**Analysis:**
- This is **application-level policy**, not a core primitive
- The Rust design provides `ProgressCallback<TransferProgress>` which includes `transfer_rate`
- Applications can implement this logic in their progress callback

**Recommendation:**
- Document as an example progress callback implementation
- Not needed in core library

**Proposed Rust Example:**
```rust
struct LowTransferRateDetector {
    threshold: u64,  // bytes per second
    count_threshold: u32,
    low_count: u32,
    last_processed_files: u64,
}

impl ProgressCallback<TransferProgress> for LowTransferRateDetector {
    fn on_progress(&self, progress: &TransferProgress) -> bool {
        // File completion acts as heartbeat
        if progress.overall_completed > self.last_processed_files {
            self.low_count = 0;
        }
        self.last_processed_files = progress.overall_completed;
        
        let transfer_rate = if progress.elapsed_seconds > 0.0 {
            progress.overall_bytes / progress.elapsed_seconds as u64
        } else {
            0
        };
        
        if transfer_rate < self.threshold {
            self.low_count += 1;
        } else {
            self.low_count = 0;
        }
        
        if self.low_count >= self.count_threshold {
            eprintln!("Transfer rate too low, cancelling");
            return false;  // Cancel
        }
        
        true  // Continue
    }
}
```

### 3. Subprocess Execution Model

**Python Implementation:**
- Worker agent spawns `attachment_download.py` and `attachment_upload.py` as subprocesses
- Uses OpenJD `StepScript` to execute Python scripts
- Passes `WorkerManifestProperties` via JSON embedded files
- Captures stdout for progress reporting (`openjd_progress:`, `openjd_status:`)

**Rust Design Status:** ✅ **Architectural choice, not a gap**

**Analysis:**
- Python uses subprocess for **isolation** and **OpenJD integration**
- Rust design assumes **direct library calls** from worker agent
- Both approaches are valid:
  - **Subprocess:** Better isolation, easier to update independently
  - **Direct calls:** Better performance, simpler error handling

**Recommendation:**
- Not a design gap - this is an architectural decision for the worker agent
- If Rust worker agent wants subprocess model, it can spawn Rust binaries
- If using PyO3 bindings, Python worker agent can call Rust directly

---

## Architectural Patterns

### Pattern 1: Direct Library Integration (Recommended for Rust)

```rust
// Worker agent calls Rust library directly
impl Session {
    async fn download_attachments(&mut self) -> Result<(), Error> {
        // 1. Aggregate manifests
        let manifests_by_root = self.aggregate_manifests().await?;
        
        // 2. Generate path mappings
        let path_mappings = generate_dynamic_path_mapping(
            &self.working_directory,
            &self.job_attachment_details.manifests,
        );
        
        // 3. Download files
        let orchestrator = DownloadOrchestrator::new(client, location);
        let stats = orchestrator.download_manifest(
            &manifests_by_root,
            &self.working_directory,
            ConflictResolution::CreateCopy,
            Some(&progress_callback),
        ).await?;
        
        Ok(())
    }
}
```

### Pattern 2: Subprocess Model (Python-compatible)

```rust
// Worker agent spawns Rust binary as subprocess
impl Session {
    async fn download_attachments(&mut self) -> Result<(), Error> {
        // 1. Prepare WorkerManifestProperties
        let worker_props = self.create_worker_manifest_properties();
        let props_json = serde_json::to_string(&worker_props)?;
        
        // 2. Write to temp file
        let temp_file = NamedTempFile::new()?;
        std::fs::write(&temp_file, props_json)?;
        
        // 3. Spawn subprocess
        let output = Command::new("rusty-attachments-download")
            .arg("--worker-properties")
            .arg(temp_file.path())
            .arg("--s3-uri")
            .arg(&s3_uri)
            .output()?;
        
        // 4. Parse progress from stdout
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.starts_with("openjd_progress:") {
                // Parse and report progress
            }
        }
        
        Ok(())
    }
}
```

---

## Conclusions

### Design Completeness: ✅ **COMPLETE**

The rusty-attachments design documents provide **all core primitives** necessary to implement job attachment download/upload functionality equivalent to the Python implementation.

### Coverage Summary

**✅ Fully Covered (100%):**
- Manifest data structures (v2023, v2025)
- Manifest operations (encode, decode, diff, merge)
- File system operations (scan, hash, glob, symlinks)
- Storage operations (upload, download, CAS)
- Caching (hash cache, S3 check cache)
- Path operations (normalization, mapping, transformation)
- Progress reporting and cancellation
- Error handling

**⚠️ Orchestration Patterns (Not Core Primitives):**
- `WorkerManifestProperties` - Application-level data structure
- Low transfer rate detection - Application-level policy
- Subprocess execution - Architectural choice

**🚫 Intentionally Excluded:**
- VFS (Virtual File System) - Separate system component
- AssetSync orchestrator - Application-level coordination

### Recommendations

1. **For Core Library:**
   - No changes needed - design is complete
   - All primitives are well-defined and composable

2. **For Worker Agent Integration:**
   - Define `WorkerManifestProperties` in worker-agent crate (not core library)
   - Implement low transfer rate detection in progress callback
   - Choose direct library calls over subprocess for better performance

3. **For PyO3 Bindings:**
   - Expose core primitives as Python classes
   - Python worker-agent can continue using subprocess model
   - Gradually migrate to direct Rust calls for performance

4. **Documentation:**
   - Add `WorkerManifestProperties` example to this file (see above)
   - Add progress callback examples to `storage-design.md`
   - Document integration patterns in worker-agent documentation

### Implementation Priority

The Python code analysis confirms that the Rust design is **ready for implementation**. The identified "gaps" are not missing primitives but rather:
- **Orchestration patterns** that should be defined by consuming applications
- **Application policies** that can be implemented via callbacks
- **Architectural choices** that don't affect core library design

No design changes are required before beginning implementation.

---

## Related Documents

- [model-design.md](../model-design.md) - Manifest data structures
- [storage-design.md](../storage-design.md) - Upload/download orchestration
- [manifest-storage.md](../manifest-storage.md) - Manifest S3 operations
- [file_system.md](../file_system.md) - Directory scanning and hashing
- [hash-cache.md](../hash-cache.md) - Local caching
- [path-mapping.md](../path-mapping.md) - Path transformation
- [storage-profiles.md](../storage-profiles.md) - File system locations
- [job-submission.md](../job-submission.md) - Job attachments format
- [manifest-utils.md](../manifest-utils.md) - Diff/merge operations
- [common.md](../common.md) - Shared utilities
- [todo.md](../todo.md) - Implementation roadmap
