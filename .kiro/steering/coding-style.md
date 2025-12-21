# Rust Coding Style Guidelines

## Documentation

### Function Documentation
Every function must include:
1. A single-line summary of what the function does
2. Documentation for each argument using `# Arguments` section
3. Return value documentation using `# Returns` section (if non-trivial)

```rust
/// Calculate the expected number of chunks for a file.
///
/// # Arguments
/// * `size` - Total file size in bytes
/// * `chunk_size` - Size of each chunk in bytes (0 disables chunking)
///
/// # Returns
/// Number of chunks needed, minimum 1.
pub fn expected_chunk_count(size: u64, chunk_size: u64) -> usize {
    // ...
}
```

## Type Annotations

### Explicit Types on Variables
Always annotate `let` bindings and function-local variables with explicit types so readers can understand the code without IDE assistance:

```rust
// Good - explicit types
let chunks: Vec<ChunkInfo> = Vec::new();
let offset: u64 = 0;
let dir_index: HashMap<&str, usize> = HashMap::new();

// Avoid - implicit types
let chunks = Vec::new();
let offset = 0;
let dir_index = HashMap::new();
```

### Exception: Obvious Types
Type annotations may be omitted when the type is immediately obvious from the right-hand side:

```rust
// OK - type is obvious from constructor
let manifest = AssetManifest::new(paths);
let error = ManifestError::UnknownVersion(version);
```

## Function Design

### Single Focused Purpose
Primitive/utility functions should have a single, focused purpose:

```rust
// Good - single purpose
pub fn needs_chunking(size: u64, chunk_size: u64) -> bool;
pub fn generate_chunks(size: u64, chunk_size: u64) -> Vec<ChunkInfo>;
pub fn cas_key(hash: &str, algorithm: HashAlgorithm) -> String;

// Avoid - multiple responsibilities
pub fn process_and_upload_file(...) -> Result<...>;  // Split into process_file() and upload_file()
```

### Composition Over Complexity
Build complex operations by composing simple primitives:

```rust
// Primitives
fn hash_file(path: &Path) -> Result<String, Error>;
fn check_exists(client: &Client, key: &str) -> Result<bool, Error>;
fn upload_bytes(client: &Client, key: &str, data: &[u8]) -> Result<(), Error>;

// Composed operation
fn upload_if_missing(client: &Client, path: &Path) -> Result<UploadResult, Error> {
    let hash: String = hash_file(path)?;
    let key: String = cas_key(&hash, HashAlgorithm::Xxh128);
    
    if check_exists(client, &key)? {
        return Ok(UploadResult::skipped());
    }
    
    let data: Vec<u8> = std::fs::read(path)?;
    upload_bytes(client, &key, &data)?;
    Ok(UploadResult::uploaded(data.len() as u64))
}
```
