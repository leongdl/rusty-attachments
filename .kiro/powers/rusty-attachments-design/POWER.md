# Rusty Attachments Design Reference

This power provides quick access to design documentation for the rusty-attachments project - a Rust implementation of AWS Deadline Cloud job attachments.

## Design Document Summaries

Use these summaries to identify which document to read for specific topics. Reference the full document when you need implementation details.

### Core Architecture

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `model-design.md` | Manifest data structures | v2023/v2025 formats, ManifestType (Snapshot/Diff), validation rules, encode/decode |
| `common.md` | Shared utilities | Hash functions, path utilities, constants (CHUNK_SIZE_V2=256MB), ProgressCallback trait |
| `storage-design.md` | S3 storage abstraction | StorageClient trait, UploadOrchestrator, DownloadOrchestrator, CRT backend |

### File Operations

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `file_system.md` | Directory scanning | GlobFilter, SnapshotOptions, DiffOptions, expand_input_paths(), FileSystemScanner |
| `manifest-utils.md` | Manifest operations | compare_manifests(), create_diff_manifest(), merge_manifests(), ManifestPathGroup |
| `hash-cache.md` | Caching layer | HashCache (path,size,mtime→hash), S3CheckCache (s3_key→exists), SQLite backends |

### S3 Operations

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `manifest-storage.md` | Manifest S3 ops | upload_input_manifest(), output manifest discovery, S3 key formats, metadata handling |
| `upload.md` | Upload prototype | Original CAS upload design, chunking logic (superseded by storage-design.md) |

### Path & Profile Management

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `storage-profiles.md` | Storage profiles | FileSystemLocation (Local/Shared), AssetRootGroup, group_asset_paths(), path validation |
| `path-mapping.md` | Path transformation | PathMappingRule, PathMappingApplier (trie-based), cross-platform path handling |

### Job Integration

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `job-submission.md` | Job attachments format | ManifestProperties, Attachments struct, build_attachments() for CreateJob API |
| `bindings.md` | Python bindings (PyO3) | API design, async support, progress callbacks, exception classes |

### Implementation

| Document | Purpose | Key Topics |
|----------|---------|------------|
| `implementation/order.md` | Build order | Phase-by-phase implementation plan, dependency graph, completion status |
| `todo.md` | Remaining work | Feature checklist, skipped features with rationale, Python function mapping |
| `utilities.md` | CLI utilities | filter_redundant_known_paths(), classify_paths(), warning message generation |

### Examples

| Document | Purpose |
|----------|---------|
| `examples/example-bundle-submit.md` | Full job submission workflow |
| `examples/example-worker-agent.md` | Worker input/output sync |
| `examples/example-incremental-download.md` | Incremental download with diff manifests |
| `examples/example-output-download.md` | Output manifest discovery and download |

## Quick Reference

### Manifest Versions
- **v2023-03-03**: Original format, single hash per file, no directories
- **v2025-12-04-beta**: Chunked files, directories, symlinks, diff manifests

### Key Constants
- `CHUNK_SIZE_V2` = 256MB (chunking threshold)
- `SMALL_FILE_THRESHOLD` = 80MB (parallel upload threshold)
- Hash algorithm: XXH128

### Crate Structure
```
crates/
├── common/        # Shared utilities
├── model/         # Manifest structures
├── filesystem/    # Directory scanning
├── profiles/      # Storage profiles
├── storage/       # Upload/download orchestration
├── storage-crt/   # AWS SDK backend
└── python/        # PyO3 bindings
```

## Usage

When working on rusty-attachments code:
1. Check this summary to find the relevant design document
2. Read the full document for implementation details
3. Follow the coding style in `.kiro/steering/coding-style.md`
4. Follow the design principles in `.kiro/steering/design-steering.md`
