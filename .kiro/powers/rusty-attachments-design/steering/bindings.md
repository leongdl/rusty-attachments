# Python Bindings Design Summary

**Full doc:** `design/bindings.md`  
**Status:** ✅ Phase 1 IMPLEMENTED in `crates/python/`

## Purpose
PyO3 bindings for Python CLI layer on top of Rust job attachments library.

## Implementation Status

### ✅ Implemented (Phase 1 - MVP)
- `S3Location`, `ManifestLocation`
- `AssetReferences`, `BundleSubmitOptions`, `BundleSubmitResult`
- `SummaryStatistics`, `FileSystemLocation`, `StorageProfile`
- `Manifest` (decode/encode)
- `submit_bundle_attachments_py()`
- `decode_manifest()`
- Exception classes: `AttachmentError`, `StorageError`, `ValidationError`

### ❌ Not Yet Implemented (Phase 2+)
- `sync_inputs()`, `sync_outputs()`
- `create_manifest()`, `diff_manifests()`
- Upload progress callback (separate from scan)

## Key API

```python
from rusty_attachments import (
    submit_bundle_attachments,
    S3Location, ManifestLocation, AssetReferences,
    BundleSubmitOptions, StorageProfile,
)

result = await submit_bundle_attachments(
    region="us-west-2",
    s3_location=S3Location(...),
    manifest_location=ManifestLocation(...),
    asset_references=AssetReferences(
        input_filenames=[...],
        output_directories=[...],
        referenced_paths=[...],
    ),
    storage_profile=None,
    options=BundleSubmitOptions(
        require_paths_exist=False,
        file_system_mode="COPIED",
    ),
    progress_callback=lambda p: print(f"{p.phase} {p.files_processed}/{p.total_files}"),
)

attachments_json = result.attachments_json
```

## Async Support
Uses `pyo3-asyncio` with tokio runtime for async functions.

## Progress Callbacks
Accept Python callables, wrap in `PythonProgressCallback` implementing `ProgressCallback<T>`.

## Module Structure
```
rusty_attachments/
├── __init__.py          # Re-exports
├── _rusty_attachments.pyd  # Compiled Rust
└── py.typed             # PEP 561 marker
```

## When to Read Full Doc
- Adding new Python bindings
- Understanding async patterns
- Progress callback implementation
- Type stub (.pyi) updates
