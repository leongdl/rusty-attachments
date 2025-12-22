# Rusty Attachments

Rust implementation of the Job Attachments manifest model for AWS Deadline Cloud, with Python and WASM bindings.

## Features

- **v2023-03-03**: Original manifest format with files only
- **v2025-12-04-beta**: Extended format with directories, symlinks, chunking, execute bit

## Project Structure

```
rusty-attachments/
├── crates/
│   ├── common/     # Shared utilities (path, hash, progress)
│   ├── model/      # Core manifest model
│   ├── filesystem/ # Directory scanning, diff operations
│   ├── profiles/   # Storage profiles, path grouping
│   ├── storage/    # S3 storage abstraction, caching layers
│   ├── storage-crt/# AWS SDK S3 backend implementation
│   ├── python/     # PyO3 bindings
│   └── wasm/       # WASM bindings
└── design/         # Design documents
```

## Crates

| Crate | Description |
|-------|-------------|
| `rusty-attachments-common` | Path utilities, hash functions, progress callbacks, constants |
| `rusty-attachments-model` | Manifest structures, encode/decode, validation |
| `rusty-attachments-filesystem` | Directory scanning, snapshot/diff operations, glob filtering |
| `rusty-attachments-profiles` | Storage profiles, path grouping, asset root management |
| `rusty-attachments-storage` | S3 storage traits, upload/download orchestration, caching |
| `rusty-attachments-storage-crt` | AWS SDK S3 backend (`StorageClient` implementation) |

## Building

```bash
# Build all crates
cargo build

# Build a specific crate
cargo build -p rusty-attachments-common
cargo build -p rusty-attachments-model
cargo build -p rusty-attachments-filesystem
cargo build -p rusty-attachments-profiles
cargo build -p rusty-attachments-storage
cargo build -p rusty-attachments-storage-crt

# Check without building (faster)
cargo check
```

## Testing

```bash
# Run all tests
cargo test

# Test a specific crate
cargo test -p rusty-attachments-common
cargo test -p rusty-attachments-model
cargo test -p rusty-attachments-filesystem
cargo test -p rusty-attachments-profiles
cargo test -p rusty-attachments-storage
cargo test -p rusty-attachments-storage-crt

# Run tests with output
cargo test -- --nocapture
```

## Usage (Rust)

```rust
use rusty_attachments_model::Manifest;
use rusty_attachments_common::{hash_file, normalize_for_manifest};
use rusty_attachments_filesystem::{FileSystemScanner, SnapshotOptions, DiffEngine, DiffOptions};

// Decode a manifest
let json = r#"{"hashAlg":"xxh128","manifestVersion":"2023-03-03","paths":[],"totalSize":0}"#;
let manifest = Manifest::decode(json)?;
println!("Version: {}", manifest.version());

// Hash a file
let hash = hash_file(Path::new("file.txt"))?;

// Normalize path for manifest storage
let manifest_path = normalize_for_manifest(Path::new("/project/assets/file.txt"), Path::new("/project"))?;
assert_eq!(manifest_path, "assets/file.txt");

// Create a snapshot manifest from a directory
let scanner = FileSystemScanner::new();
let options = SnapshotOptions {
    root: PathBuf::from("/project/assets"),
    ..Default::default()
};
let manifest = scanner.snapshot(&options, None)?;

// Diff a directory against a manifest
let engine = DiffEngine::new();
let diff_options = DiffOptions {
    root: PathBuf::from("/project/assets"),
    ..Default::default()
};
let diff_result = engine.diff(&manifest, &diff_options, None)?;
println!("Added: {}, Modified: {}, Deleted: {}", 
    diff_result.added.len(), 
    diff_result.modified.len(), 
    diff_result.deleted.len());
```

## Python/WASM Bindings

```bash
# Build Python wheel (requires maturin)
cd crates/python && maturin build

# Build WASM (requires wasm-pack)
cd crates/wasm && wasm-pack build
```

## License

Apache-2.0
