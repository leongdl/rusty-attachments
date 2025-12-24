# Rusty Attachments: Virtual File System (VFS) Module Design

**Status: 📋 DESIGN**

## Background Summary

This module ports the C++ Fus3 project to Rust, creating a FUSE-based virtual filesystem that mounts Deadline Cloud job attachment manifests. Files appear as local files but content is fetched on-demand from S3 CAS (Content-Addressable Storage).

### Core Concepts

| Concept | Description |
|---------|-------------|
| **Manifest** | JSON file listing files with paths, sizes, mtimes, and content hashes |
| **CAS** | Content-Addressable Storage - files stored by hash (e.g., `Data/{hash}.xxh128`) |
| **INode** | In-memory representation of a file/directory/symlink with metadata |
| **INodeManager** | Allocates inode IDs and maintains ID→INode and path→INode maps |
| **FileStore** | Trait for content retrieval (S3, disk cache, or memory) |

### Key Operations Flow

```
Mount:
  1. Parse manifest JSON
  2. Build INode tree (INodeManager::AddPathINodes for each entry)
  3. Store content hash in each file INode's FileStorageInfo
  4. Start FUSE session

File Access (e.g., `cat /mnt/vfs/scene.blend`):
  1. lookup("/", "scene.blend") → find INode by name in parent's children
  2. getattr(ino) → return FileAttr from INode
  3. open(ino) → create OpenFileInfo, return file handle
  4. read(ino, fh, offset, size):
     a. Get content hash from INode
     b. Build S3 key: "{CASPrefix}/{hash}.{hashAlg}"
     c. Fetch from cache or S3
     d. Verify xxhash integrity
     e. Return data slice
  5. release(fh) → cleanup OpenFileInfo
```

### S3 CAS Key Format

```
{CASPrefix}/{hash}.{hashAlg}
Example: Data/b9957a169638056faef9f7c45721db91.xxh128
```

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `ROOT_INODE` | 1 | Root directory inode ID |
| `ATTR_TIMEOUT` | 86400s | Kernel cache TTL (24 hours) |
| `DEFAULT_FILE_PERMS` | 0o644 | rw-r--r-- |
| `DEFAULT_DIR_PERMS` | 0o755 | rwxr-xr-x |

---

## Fuser Library Summary

The `fuser` crate provides the Rust FUSE interface. Key points:

### FileAttr Structure
```rust
pub struct FileAttr {
    pub ino: u64,           // Inode number
    pub size: u64,          // Size in bytes
    pub blocks: u64,        // Size in blocks
    pub atime: SystemTime,  // Access time
    pub mtime: SystemTime,  // Modification time
    pub ctime: SystemTime,  // Change time
    pub kind: FileType,     // Directory, RegularFile, Symlink
    pub perm: u16,          // Permissions (0o755, etc.)
    pub nlink: u32,         // Hard link count
    pub uid: u32, pub gid: u32,
    pub blksize: u32,       // Block size (512)
}
```

### Required Filesystem Trait Methods (Read-Only)

| Method | Purpose | Reply |
|--------|---------|-------|
| `lookup(parent, name)` | Resolve path component | `ReplyEntry` with FileAttr |
| `getattr(ino)` | Get file attributes | `ReplyAttr` with FileAttr |
| `readdir(ino, offset)` | List directory | `ReplyDirectory` with entries |
| `open(ino, flags)` | Open file | `ReplyOpen` with file handle |
| `read(ino, fh, offset, size)` | Read file data | `ReplyData` with bytes |
| `release(ino, fh)` | Close file | `ReplyEmpty` |
| `readlink(ino)` | Read symlink target | `ReplyData` with path |

### Mount API
```rust
// Blocking mount (returns on unmount)
fuser::mount2(filesystem, "/mnt/vfs", &[MountOption::RO])?;

// Background mount (returns immediately)
let session = fuser::spawn_mount2(filesystem, "/mnt/vfs", &[MountOption::RO])?;
// session.join() or drop to unmount
```

---

## Goals

1. **Mount manifests as filesystems** - Read-only mounted directories from V1/V2 manifests
2. **Lazy content retrieval** - Fetch from S3 CAS on-demand when files are read
3. **Pyramid architecture** - Single-responsibility primitives → composition → FUSE interface
4. **Cross-platform** - Linux and macOS (FUSE), potential Windows (WinFSP)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Layer 3: FUSE Interface                                   │
│                    (fuser::Filesystem impl)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Layer 2: VFS Operations                                   │
│                    (lookup, read, readdir)                                   │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Layer 1: Primitives                                       │
│         INodeManager          │           FileStore                          │
│    (INode, INodeDir, etc.)    │    (S3CasStore, DiskCache)                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Existing Crates                                           │
│              model (Manifest)  │  storage (S3 client)                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Project Structure

```
crates/vfs/src/
├── lib.rs
├── error.rs              # VfsError enum
├── memory_pool.rs        # Layer 1: Memory pool for V2 chunks
│
├── inode/                # Layer 1: INode primitives
│   ├── mod.rs
│   ├── types.rs          # INodeId, INodeType, FileAttr
│   ├── file.rs           # INodeFile (hash, size, mtime)
│   ├── dir.rs            # INodeDir (children map)
│   ├── symlink.rs        # INodeSymlink (target path)
│   └── manager.rs        # INodeManager (allocation, lookup)
│
├── content/              # Layer 1: Content retrieval
│   ├── mod.rs
│   ├── store.rs          # FileStore trait
│   ├── s3_cas.rs         # S3CasStore implementation
│   └── cached.rs         # CachedStore wrapper with DiskCache
│
├── builder.rs            # Layer 2: Build INode tree from Manifest
│
└── fuse.rs               # Layer 3: fuser::Filesystem implementation
```

---

## Core Types

### INode Types

```rust
pub type INodeId = u64;
pub const ROOT_INODE: INodeId = 1;

pub enum INodeType { File, Directory, Symlink }

pub struct INodeFile {
    id: INodeId,
    parent_id: INodeId,
    name: String,
    path: String,
    size: u64,
    mtime: SystemTime,
    hash: String,              // Content hash for CAS lookup
    hash_algorithm: HashAlgorithm,
    executable: bool,
}

pub struct INodeDir {
    id: INodeId,
    parent_id: INodeId,
    name: String,
    path: String,
    children: RwLock<HashMap<String, INodeId>>,  // name → child inode
}

pub struct INodeSymlink {
    id: INodeId,
    parent_id: INodeId,
    name: String,
    path: String,
    target: String,            // Symlink target path
}
```

### INodeManager

```rust
pub struct INodeManager {
    next_id: AtomicU64,
    inodes: RwLock<HashMap<INodeId, Arc<dyn INode>>>,
    path_index: RwLock<HashMap<String, INodeId>>,
}

impl INodeManager {
    pub fn new() -> Self;                                    // Creates root dir
    pub fn get(&self, id: INodeId) -> Option<Arc<dyn INode>>;
    pub fn get_by_path(&self, path: &str) -> Option<Arc<dyn INode>>;
    pub fn add_file(&self, parent: INodeId, entry: &ManifestFilePath) -> INodeId;
    pub fn add_directory(&self, parent: INodeId, name: &str, path: &str) -> INodeId;
    pub fn add_symlink(&self, parent: INodeId, name: &str, path: &str, target: &str) -> INodeId;
}
```

### FileStore Trait

```rust
#[async_trait]
pub trait FileStore: Send + Sync {
    async fn retrieve(&self, hash: &str, algorithm: HashAlgorithm) -> Result<Vec<u8>, VfsError>;
    async fn retrieve_range(&self, hash: &str, algorithm: HashAlgorithm, 
                            offset: u64, size: u64) -> Result<Vec<u8>, VfsError>;
}

pub struct S3CasStore {
    client: S3Client,
    bucket: String,
    cas_prefix: String,  // e.g., "Data"
}

pub struct CachedStore<S: FileStore> {
    inner: S,
    cache_dir: PathBuf,
    max_size: u64,
}
```

### Memory Pool

The memory pool manages fixed-size blocks (256MB, matching V2 chunk size) with LRU eviction.
Designed for efficient handling of V2 manifest chunks where large files are split into
individually-hashed chunks.

```
┌─────────────────────────────────────────────────────────────────┐
│                      MemoryPool                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  blocks: HashMap<BlockId, Arc<PoolBlock>>               │    │
│  │  key_index: HashMap<BlockKey, BlockId>                  │    │
│  │  pending_fetches: HashMap<BlockKey, Shared<Future>>     │    │
│  │  lru_order: VecDeque<BlockId>  (front=oldest)           │    │
│  │  current_size / max_size                                │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘

PoolBlock:
  - data: Arc<Vec<u8>>  (shared, lock-free reads)
  - key: BlockKey
  - ref_count: AtomicUsize
```

#### Core Types

```rust
/// Key identifying a unique block (hash + chunk_index).
pub struct BlockKey {
    pub hash: String,
    pub chunk_index: u32,
}

/// Configuration for the memory pool.
pub struct MemoryPoolConfig {
    pub max_size: u64,    // Default: 8GB
    pub block_size: u64,  // Default: 256MB (CHUNK_SIZE_V2)
}

/// RAII handle - holds Arc to data, auto-releases on drop.
pub struct BlockHandle {
    data: Arc<Vec<u8>>,   // Direct access, no lock needed
    pool: Arc<...>,       // For ref_count decrement on drop
    block_id: BlockId,
}

/// Trait for content providers (S3, disk, etc.).
#[async_trait]
pub trait BlockContentProvider: Send + Sync {
    async fn fetch(&self, key: &BlockKey) -> Result<Vec<u8>, MemoryPoolError>;
}
```

### VFS Options

Configuration for VFS behavior including caching, prefetching, and performance tuning.

```rust
/// Main configuration struct for the VFS.
pub struct VfsOptions {
    pub pool: MemoryPoolConfig,       // Memory pool settings
    pub prefetch: PrefetchStrategy,   // Chunk prefetch behavior
    pub kernel_cache: KernelCacheOptions,
    pub read_ahead: ReadAheadOptions,
    pub timeouts: TimeoutOptions,
}

/// Strategy for prefetching chunks.
pub enum PrefetchStrategy {
    /// No prefetching - lazy load on read (default).
    None,
    /// Prefetch first N chunks on file open.
    OnOpen { chunks: u32 },
    /// Prefetch next chunk during sequential reads.
    Sequential { look_ahead: u32 },
    /// Prefetch all chunks on open (use with caution).
    Eager,
}

/// Kernel-level cache settings (FUSE).
pub struct KernelCacheOptions {
    pub enable_page_cache: bool,   // Default: true
    pub enable_attr_cache: bool,   // Default: true
    pub attr_timeout_secs: u64,    // Default: 86400 (24h)
    pub entry_timeout_secs: u64,   // Default: 86400 (24h)
}

/// Read-ahead behavior for sequential access.
pub struct ReadAheadOptions {
    pub detect_sequential: bool,      // Default: true
    pub sequential_threshold: u32,    // Default: 2 reads
    pub max_concurrent_prefetch: u32, // Default: 4
}

/// Timeout settings.
pub struct TimeoutOptions {
    pub fetch_timeout_secs: u64,  // Default: 300 (5 min)
    pub open_timeout_secs: u64,   // Default: 60 (1 min)
}
```

#### Usage Example

```rust
let options = VfsOptions::default()
    .with_prefetch(PrefetchStrategy::OnOpen { chunks: 2 })
    .with_pool_config(MemoryPoolConfig::with_max_size(16 * GB))
    .with_read_ahead(ReadAheadOptions::aggressive());

let vfs = DeadlineVfs::new(manifest, store, options)?;
fuser::mount2(vfs, "/mnt/assets", &[MountOption::RO])?;
```

#### Pool Operations

```rust
impl MemoryPool {
    /// Create pool with configuration.
    pub fn new(config: MemoryPoolConfig) -> Self;
    
    /// Acquire block, fetching if not cached. Returns RAII handle.
    /// Uses fetch coordination to prevent duplicate fetches.
    pub async fn acquire<F, Fut>(&self, key: &BlockKey, fetch: F) 
        -> Result<BlockHandle, MemoryPoolError>;
    
    /// Try to get cached block without fetching.
    pub fn try_get(&self, key: &BlockKey) -> Option<BlockHandle>;
    
    /// Get pool statistics.
    pub fn stats(&self) -> MemoryPoolStats;
}
```

#### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `DEFAULT_MAX_POOL_SIZE` | 8GB | Maximum pool memory |
| `DEFAULT_BLOCK_SIZE` | 256MB | Block size (matches V2 chunk) |

#### Multi-threaded Access Design

The memory pool is designed for concurrent access from multiple FUSE read operations:

1. **Lock-Free Data Access**: Block data is stored in `Arc<Vec<u8>>`.
   - `BlockHandle` holds a clone of the Arc
   - Reading data requires no locks - just dereference the Arc
   - Multiple readers can access the same block simultaneously

2. **Atomic Reference Counting**: `ref_count` uses `AtomicUsize`.
   - Increment/decrement without holding pool lock
   - Blocks with `ref_count > 0` cannot be evicted
   - `BlockHandle::drop()` decrements atomically

3. **Fetch Coordination**: Prevents thundering herd / duplicate fetches.
   - `pending_fetches: HashMap<BlockKey, Shared<Future>>` tracks in-flight fetches
   - First thread to request a key starts the fetch and inserts a shared future
   - Subsequent threads for the same key await the existing future
   - After fetch completes, future is removed and block is inserted

   ```
   Thread A: acquire(key) → cache miss → start fetch, insert pending future
   Thread B: acquire(key) → cache miss → find pending future → await it
   Thread C: acquire(key) → cache miss → find pending future → await it
   [fetch completes]
   All threads: receive same data, only one S3 request made
   ```

4. **Lock Granularity**:
   - Pool metadata protected by `Mutex<MemoryPoolInner>` (not RwLock - mutations are common)
   - Lock held only for quick HashMap operations
   - Async fetch happens outside the lock
   - Data reads are lock-free via Arc

5. **Deadlock Prevention**:
   - Single lock (no nested locks)
   - No lock held across await points
   - Atomic operations for ref_count

6. **Memory Pressure**: When pool is full and all blocks are in use:
   - Returns `MemoryPoolError::PoolExhausted`
   - Caller should retry or fail the read operation

### DeadlineVfs (FUSE Implementation)

```rust
pub struct DeadlineVfs {
    inodes: INodeManager,
    store: Arc<dyn FileStore>,
    handles: RwLock<HashMap<u64, OpenHandle>>,
    next_handle: AtomicU64,
}

impl fuser::Filesystem for DeadlineVfs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_dir = self.inodes.get(parent)?.as_dir()?;
        let child_id = parent_dir.get_child(name.to_str()?)?;
        let child = self.inodes.get(child_id)?;
        reply.entry(&TTL, &child.to_fuser_attr(), 0);
    }
    
    fn read(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, 
            size: u32, _flags: i32, _lock: Option<u64>, reply: ReplyData) {
        let file = self.inodes.get(ino)?.as_file()?;
        let data = self.store.retrieve(&file.hash, file.hash_algorithm)?;
        reply.data(&data[offset as usize..][..size as usize]);
    }
    // ... other methods
}
```

---

## Usage Example

```rust
use rusty_attachments_vfs::{DeadlineVfs, S3CasStore, CachedStore};
use rusty_attachments_model::Manifest;

// Load manifest
let manifest = Manifest::from_file("manifest.json")?;

// Create content store with caching
let s3_store = S3CasStore::new(s3_client, "my-bucket", "Data");
let cached_store = CachedStore::new(s3_store, "/var/cache/vfs", 10_GB);

// Build VFS and mount
let vfs = DeadlineVfs::from_manifest(&manifest, Arc::new(cached_store))?;
fuser::mount2(vfs, "/mnt/assets", &[MountOption::RO, MountOption::AutoUnmount])?;
```

---

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("Inode not found: {0}")]
    InodeNotFound(INodeId),
    
    #[error("Not a directory: {0}")]
    NotADirectory(INodeId),
    
    #[error("Content retrieval failed for hash {hash}")]
    ContentRetrievalFailed { hash: String, source: Box<dyn std::error::Error + Send + Sync> },
    
    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    
    #[error("Mount failed: {0}")]
    MountFailed(String),
}
```

---

## Fus3 C++ Reference (for porting)

Key files and functions to reference when implementing:

| Rust Component | Fus3 C++ Reference |
|----------------|-------------------|
| `INodeManager::add_file` | `INodeManager::AddPathINodes()` in `inode_manager.cpp:237` |
| `DeadlineVfs::lookup` | `HandleLookup()` in `simurgh_fuse_low.cpp:903` |
| `DeadlineVfs::read` | `HandleRead()` + `ReadFUSE()` in `simurgh_fuse_low.cpp:137` |
| `S3CasStore::retrieve` | `AWSVFS::GetFileInfoFromBucket()` in `aws_vfs.cpp:378` |
| `S3CasStore::cas_key` | `DeadlineVFS::GetS3Key()` in `deadline_vfs.cpp:137` |
| Hash verification | `DeadlineVFS::IsObjectIntegrityValid()` in `deadline_vfs.cpp:109` |

---

## Dependencies

```toml
[dependencies]
fuser = "0.14"
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }
async-trait = "0.1"
thiserror = "1"
tracing = "0.1"

# Internal crates
rusty-attachments-model = { path = "../model" }
rusty-attachments-storage = { path = "../storage" }
rusty-attachments-common = { path = "../common" }
```

---

## Future Considerations

1. **Chunk-based retrieval** - Use V2 chunk hashes for parallel/partial downloads
2. **Multiple manifests** - Mount multiple manifests as subdirectories
3. **Windows support** - WinFSP or Dokan integration
4. **Memory pool enhancements**:
   - Sharded locks (`DashMap`) for reduced contention under high concurrency
   - Prefetching adjacent chunks for sequential read patterns
   - Tiered eviction (prioritize evicting cold chunks over recently-accessed)

---

## Write Support Design (Future)

The current memory pool is optimized for read-only access. If write support is needed in the future, here's the recommended approach:

### Current Limitations

The read-optimized design has these constraints for writes:

| Aspect | Current Design | Write Limitation |
|--------|----------------|------------------|
| Data storage | `Arc<Vec<u8>>` | Immutable once created - shared readers |
| Block API | `data(&self) -> &[u8]` | Read-only, no `data_mut()` |
| CAS model | Content identified by hash | Modified data = different hash = different block |
| Dirty tracking | None | No mechanism to track modified blocks |

### Recommended Approach: Copy-on-Write with Dirty Cache

```
┌─────────────────────────────────────────────────────────────────┐
│                      WritableMemoryPool                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  read_pool: MemoryPool (existing, immutable)            │    │
│  │  dirty_blocks: HashMap<BlockKey, DirtyBlock>            │    │
│  │  write_log: Vec<WriteOperation>                         │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘

DirtyBlock:
  - data: Vec<u8>  (mutable, owned)
  - original_hash: Option<String>  (for COW tracking)
  - modified_ranges: Vec<Range<u64>>
```

### Write Flow

```
write(key, offset, data):
  1. Check dirty_blocks for existing dirty copy
  2. If not dirty:
     a. Fetch original from read_pool (or S3 if not cached)
     b. Copy to new Vec<u8> in dirty_blocks (COW)
  3. Apply write to dirty block
  4. Track modified range for partial flush optimization

flush(key):
  1. Compute new hash of dirty block
  2. Upload to S3 CAS with new hash
  3. Update manifest entry (new hash, size, mtime)
  4. Move block to read_pool with new key
  5. Remove from dirty_blocks
```

### Read Flow with Dirty Blocks

```
read(key, offset, size):
  1. Check dirty_blocks first (dirty data takes precedence)
  2. If dirty: return from dirty block
  3. Else: return from read_pool (existing flow)
```

### Key Design Decisions

1. **Separate dirty cache**: Keep read pool immutable for lock-free reads. Dirty blocks are separate and use standard `Vec<u8>` for mutability.

2. **Copy-on-Write**: Only copy when first write occurs. Reads of unmodified data still use the efficient `Arc<Vec<u8>>` path.

3. **Write-back, not write-through**: Buffer writes locally, flush on `fsync()` or `release()`. Reduces S3 round-trips.

4. **New hash on flush**: Since CAS uses content hashes, modified blocks get new hashes. Original blocks remain valid for other readers.

### Alternative Approaches (Not Recommended)

| Approach | Pros | Cons |
|----------|------|------|
| `Arc<RwLock<Vec<u8>>>` | Simple API | Lock overhead on every read |
| In-place mutation | No copy | Breaks shared readers, unsafe |
| Write-through | Simple consistency | High latency, many S3 calls |


---

## Fus3 vs Rust VFS: File Open/Read/Close Flow Comparison

This section traces through the file lifecycle in both implementations to identify differences and potential performance implications.

### Fus3 (C++) Flow

```
Thread A: open("/file.txt")
├─ HandleOpen(ino)
│  ├─ GetINode(ino) → iNode
│  └─ OpenObject(iNode, isOpen=true, createNotExists=false)
│     ├─ LOCK(m_openFileMutex)
│     ├─ Check m_openFileINodeMap[ino] → miss
│     ├─ Check m_pendingOpenFileINodeMap[ino] → miss
│     ├─ Create OpenFileInfo, add to m_pendingOpenFileINodeMap
│     ├─ LOCK(fileBuffer.bufferMutex) ← held during download
│     ├─ UNLOCK(m_openFileMutex)
│     ├─ m_fileStorage->Retrieve() or GetFileInfoFromBucket()
│     │  └─ S3 GetObject (blocking, ~100ms-10s)
│     ├─ TransferComplete()
│     │  ├─ LOCK(m_openFileMutex)
│     │  ├─ Move from pending → openFileINodeMap
│     │  ├─ UNLOCK(m_openFileMutex)
│     │  └─ SetOpen() → notify waiters
│     └─ UNLOCK(fileBuffer.bufferMutex)
└─ Return file handle

Thread B: open("/file.txt") [while Thread A downloading]
├─ HandleOpen(ino)
│  └─ OpenObject(iNode)
│     ├─ LOCK(m_openFileMutex)
│     ├─ Check m_openFileINodeMap[ino] → miss
│     ├─ Check m_pendingOpenFileINodeMap[ino] → HIT!
│     ├─ UNLOCK(m_openFileMutex)
│     ├─ WaitForOpen() ← blocks until Thread A completes
│     ├─ Check IsInitialized() → true
│     ├─ IncrementHandleCount()
│     └─ Return same OpenFileInfo
└─ Return file handle (same underlying buffer)
```

**Key Fus3 Characteristics:**
- Per-file `OpenFileInfo` with handle count (multiple opens share one buffer)
- `m_pendingOpenFileINodeMap` for in-flight downloads
- `WaitForOpen()` blocks concurrent openers until download completes
- Single S3 request per file, regardless of concurrent opens
- Buffer mutex held during entire download

### Proposed Rust VFS Flow

```
Thread A: open("/file.txt")
├─ lookup(parent, "file.txt") → ino
├─ open(ino) → create OpenHandle, return fh
└─ [no download yet - lazy]

Thread A: read(ino, fh, offset=0, size=256MB)
├─ Get chunk_key from INode (hash + chunk_index)
├─ pool.acquire(&chunk_key, fetch_fn)
│  ├─ LOCK(pool.inner)
│  ├─ Check key_index → miss
│  ├─ Check pending_fetches → miss
│  ├─ Insert shared future into pending_fetches
│  ├─ UNLOCK(pool.inner)
│  ├─ fetch_fn() → S3 GetObject (async, ~100ms-10s)
│  ├─ LOCK(pool.inner)
│  ├─ Remove from pending_fetches
│  ├─ Insert block, acquire ref
│  ├─ UNLOCK(pool.inner)
│  └─ Return BlockHandle
└─ Return data slice

Thread B: read(ino, fh, offset=0, size=256MB) [while Thread A downloading]
├─ pool.acquire(&chunk_key, fetch_fn)
│  ├─ LOCK(pool.inner)
│  ├─ Check key_index → miss
│  ├─ Check pending_fetches → HIT!
│  ├─ Clone shared future
│  ├─ UNLOCK(pool.inner)
│  ├─ await shared_future ← waits for Thread A
│  ├─ LOCK(pool.inner)
│  ├─ Lookup block, acquire ref
│  ├─ UNLOCK(pool.inner)
│  └─ Return BlockHandle
└─ Return data slice
```

### Key Differences

| Aspect | Fus3 (C++) | Rust VFS |
|--------|------------|----------|
| **Download trigger** | `open()` - eager | `read()` - lazy |
| **Granularity** | Per-file buffer | Per-chunk (256MB blocks) |
| **Concurrent open handling** | `WaitForOpen()` on pending map | Shared future in `pending_fetches` |
| **Data sharing** | Single `OpenFileInfo` per file | `Arc<Vec<u8>>` per chunk |
| **Lock during download** | Buffer mutex held | No lock held (async) |
| **Handle model** | Handle count on `OpenFileInfo` | Separate `BlockHandle` per acquire |

### Performance Implications

**Advantages of Rust Design:**

1. **Lazy download**: Files opened but never read don't trigger S3 requests. Fus3 downloads on open even if file is never read.

2. **Chunk-level caching**: For V2 manifests, only accessed chunks are downloaded. A 10GB file with 40 chunks only downloads the chunks actually read.

3. **No lock during download**: Fus3 holds `bufferMutex` during the entire S3 download. Rust releases the pool lock before async fetch.

4. **Better parallelism for large files**: Multiple chunks of the same file can be fetched in parallel by different threads.

**Potential Disadvantages:**

1. **More S3 requests for small files**: Fus3 downloads entire file once. If Rust VFS reads a small file in multiple `read()` calls, it's still one chunk, but the lazy approach means the first read has full latency.

2. **No prefetching**: Fus3's eager download means data is ready when `read()` is called. Rust's lazy approach means first `read()` always waits.

3. **Chunk boundary overhead**: Reading across chunk boundaries requires acquiring multiple blocks.

### Recommended Enhancements

1. **Prefetch on open (optional)**: Add a config option to prefetch first N chunks on `open()` for latency-sensitive workloads.

2. **Sequential read detection**: If reads are sequential, prefetch next chunk while current chunk is being read.

3. **Small file optimization**: For files < 256MB (single chunk), behavior is equivalent to Fus3.

```rust
// Optional prefetch on open
fn open(&mut self, ino: u64, flags: i32) -> Result<OpenHandle> {
    let handle = self.create_handle(ino, flags)?;
    
    if self.config.prefetch_on_open {
        let file = self.inodes.get(ino)?;
        let first_chunk = BlockKey::new(&file.hash, 0);
        // Fire-and-forget prefetch
        tokio::spawn(self.pool.acquire(&first_chunk, || self.fetch_chunk(&first_chunk)));
    }
    
    Ok(handle)
}
```
