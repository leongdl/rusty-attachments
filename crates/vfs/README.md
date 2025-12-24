# rusty-attachments-vfs

FUSE-based virtual filesystem for mounting Deadline Cloud job attachment manifests. Files appear as local files but content is fetched on-demand from S3 CAS (Content-Addressable Storage).

## Features

- Mount V1/V2 manifests as read-only filesystems
- Lazy content retrieval from S3 CAS
- Memory pool with LRU eviction for V2 256MB chunks
- Cross-platform: Linux and macOS (FUSE)

## Building

### Without FUSE (memory pool only)

```bash
cargo build -p rusty-attachments-vfs
```

### With FUSE support

```bash
cargo build -p rusty-attachments-vfs --features fuse
```

## Platform Setup

### macOS

1. Install macFUSE:
   ```bash
   brew install --cask macfuse
   ```

2. Reboot your Mac

3. Allow the kernel extension:
   - Go to System Settings → Privacy & Security
   - Scroll down and click "Allow" for the macFUSE system extension

4. Set pkg-config path (add to your shell profile):
   ```bash
   export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:$PKG_CONFIG_PATH"
   ```

5. Build with FUSE:
   ```bash
   cargo build -p rusty-attachments-vfs --features fuse
   ```

### Linux

1. Install FUSE development libraries:
   ```bash
   # Debian/Ubuntu
   sudo apt-get install libfuse-dev pkg-config

   # Fedora/RHEL
   sudo dnf install fuse-devel pkg-config

   # Arch
   sudo pacman -S fuse2 pkg-config
   ```

2. Build with FUSE:
   ```bash
   cargo build -p rusty-attachments-vfs --features fuse
   ```

## Testing

```bash
# Run all tests (no FUSE required)
cargo test -p rusty-attachments-vfs

# Run memory pool tests only
cargo test -p rusty-attachments-vfs memory_pool
```

## Architecture

```
Layer 3: FUSE Interface (fuser::Filesystem impl)
Layer 2: VFS Operations (lookup, read, readdir)
Layer 1: Primitives (INodeManager, FileStore, MemoryPool)
```

See [design/vfs.md](../../design/vfs.md) for detailed design documentation.
