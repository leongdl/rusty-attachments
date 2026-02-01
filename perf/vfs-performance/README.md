# VFS Performance Analysis

Performance analysis and optimization of the Virtual File System (VFS) read and write paths.

## Quick Start

View the main analysis:
```bash
cat perf/vfs-performance/vfs-read-perf-analysis.md
cat perf/vfs-performance/vfs-write-perf-analysis.md
```

## Files

### Analysis Documents
- **`vfs-read-perf-analysis.md`** - Read path performance analysis
- **`vfs-read-perf-comparison.md`** - Before/after comparison
- **`vfs-read-path-improvements.md`** - Optimization results
- **`vfs-write-perf-analysis.md`** - Write path performance analysis
- **`vfs-write-optimization-results.md`** - Write optimization results
- **`vfs-perf-lock.md`** - Lock contention analysis

### Performance Data
- **`vfs-read-perf.data`** - Read performance profile data
- **`vfs-read-perf2.data`** - Read performance profile data (v2)
- **`vfs-read-perf-analysis.txt`** - Read analysis text output
- **`vfs-read-perf2-analysis.txt`** - Read analysis text output (v2)
- **`vfs-write-perf.data`** - Write performance profile data
- **`vfs-write-perf2.data`** - Write performance profile data (v2)
- **`vfs-write-perf-v3.data`** - Write performance profile data (v3)
- **`vfs-write-perf-v4.data`** - Write performance profile data (v4)
- **`vfs-write-perf-v3-full.txt`** - Write analysis full output (v3)
- **`vfs-write-perf-v4-analysis.txt`** - Write analysis text output (v4)

### Test Scripts
- **`vfs-read-test.sh`** - Read performance test script

## Key Findings

### Read Path
- Identified bottlenecks in file lookup and caching
- Optimized inode management
- Improved cache hit rates

### Write Path
- Reduced lock contention
- Optimized dirty file tracking
- Improved write throughput

### Lock Analysis
- Identified lock contention hotspots
- Reduced critical section sizes
- Improved concurrent access patterns

## Running Tests

### Read Performance Test
```bash
bash perf/vfs-performance/vfs-read-test.sh
```

### Profiling
```bash
# Profile read operations
perf record -g ./target/release/mount_vfs
perf report > vfs-read-perf-analysis.txt

# Profile write operations
perf record -g -o vfs-write-perf.data ./target/release/mount_vfs
perf report -i vfs-write-perf.data > vfs-write-perf-analysis.txt
```

## Related

- **VFS implementation:** `../../crates/vfs/`
- **Design docs:** `../../design/vfs/`
- **FUSE implementation:** `../../crates/vfs/src/fuse.rs`
