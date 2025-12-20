# Rusty Attachments: TODO

## Design Documents Completed
- [x] Model design (manifest structures)
- [x] Storage design (upload/download orchestration)
- [x] Manifest storage (S3 manifest upload/download with metadata)
- [x] File system (snapshot/diff operations)
- [x] Hash cache (local file hash caching)
- [x] S3 check cache (S3 existence caching)
- [x] Storage profiles (file system locations)
- [x] Job submission (manifest to job attachments conversion)
- [x] Manifest utilities (diff/merge operations)

## Implementation TODO

### Core
- [ ] Implement business logic to upload a manifest
- [ ] Implement logic to upload the manifest file
- [ ] File folder scanning, snapshot folder, diff a folder
- [ ] Manifest utilities: diff manifest, merge manifest

### Caches
- [ ] Hash cache SQLite backend
- [ ] S3 check cache SQLite backend
- [ ] Cache integrity verification

### Testing
- [ ] Fuzz testing with weird file paths (unicode, special chars, long paths)
- [ ] Edge cases: merging manifests with time ordering
- [ ] Compatibility with Python manifest file names
- [ ] Roundtrip tests: Python create → Rust read → Python read

### Features
- [ ] S3 object tags for manifests
- [ ] Path mapping utilities
- [ ] Storage profile utilities
- [ ] CLI snapshot mode (local copy instead of S3 upload)

### Bindings
- [ ] Python bindings (PyO3)
- [ ] WASM bindings
