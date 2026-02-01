# Hash+Upload Benchmark Results

**Date:** 2025-01-24 (Final)

## Test Environment

- **Machine:** AWS EC2 instance (64 GB RAM)
- **OS:** Linux
- **Rust:** Release build with optimizations
- **S3 Bucket:** s3://adeadlineja/
- **Python:** 3.11 (deadline-cloud)

## Test Dataset

VFX job simulation dataset:
- **Files:** 260
- **Total Size:** ~5.5 GB (5583.26 MB)
- **Composition:**
  - 5 scene files (10-50 MB each)
  - 100 small textures (1-10 KB each)
  - 50 medium textures (100 KB - 5 MB each)
  - 20 large textures (10-100 MB each)
  - 10 geometry caches (50-200 MB each)
  - 5 simulation caches (200 MB - 1 GB each)
  - 20 render outputs (5-50 MB each)
  - 50 config files (1-100 KB each)

## Final Results Summary

### Best Configuration Comparison

| Metric | Python (10 workers) | Rust (32 concurrency) | Rust Advantage |
|--------|---------------------|----------------------|----------------|
| **Total Time** | 17.25s | 13.68s | **1.26x faster** |
| **Throughput** | 323.6 MB/s | 389.5 MB/s | **1.20x higher** |
| **Peak Memory** | 5167 MB | 311 MB | **17x less** |

### Same Concurrency Comparison (32)

| Metric | Python | Rust | Rust Advantage |
|--------|--------|------|----------------|
| **Total Time** | 22.47s | 13.68s | **1.64x faster** |
| **Throughput** | 248.4 MB/s | 389.5 MB/s | **1.57x higher** |
| **Peak Memory** | 5170 MB | 311 MB | **17x less** |

## Rust Benchmark Results (v3 - Recommended)

Settings: 5GB memory, 32 concurrency

| Metric | Run 10 | Run 11 | Run 12 | Average |
|--------|--------|--------|--------|---------|
| Total Time | 13.96s | 13.96s | 13.12s | **13.68s** |
| Throughput | 381.4 MB/s | 381.3 MB/s | 405.8 MB/s | **389.5 MB/s** |
| Peak Memory | 315 MB | 347 MB | 270 MB | **311 MB** |

## Python Benchmark Results

### v1 (10 workers - default)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 17.21s | 17.23s | 17.32s | **17.25s** |
| Throughput | 324.4 MB/s | 324.0 MB/s | 322.4 MB/s | **323.6 MB/s** |
| Peak Memory | 5370 MB | 5285 MB | 4846 MB | **5167 MB** |

### v2 (32 workers)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 22.48s | 22.51s | 22.43s | **22.47s** |
| Throughput | 248.4 MB/s | 248.0 MB/s | 248.9 MB/s | **248.4 MB/s** |
| Peak Memory | 5456 MB | 5286 MB | 4769 MB | **5170 MB** |

## Key Findings

1. **Rust is faster:** 1.26x to 1.64x depending on configuration
2. **Rust uses far less memory:** 17x less (311 MB vs 5.2 GB)
3. **Rust scales better with concurrency:** Performance improves up to ~32 concurrent tasks
4. **Python has GIL limitations:** Performance degrades above 10 workers due to GIL contention

## Concurrency Analysis

### Python Concurrency Scaling
- 10 workers: 323.6 MB/s (optimal)
- 32 workers: 248.4 MB/s (23% slower due to GIL)

### Rust Concurrency Scaling
- 10 concurrency: 372.9 MB/s
- 32 concurrency: 389.5 MB/s (optimal)
- 128 concurrency: 343.0 MB/s (diminishing returns)

## Memory Analysis

Python consistently uses ~5 GB regardless of worker count because it loads entire files into memory across all workers simultaneously without backpressure.

Rust stays under 350 MB even with high concurrency due to:
- Semaphore-based memory pool with 64MB permit granularity
- Backpressure that blocks new reads when memory is full
- Efficient buffer reuse

## Recommended Configuration

### Rust (Production)
```rust
HashUploadOptions {
    max_memory_bytes: 5 * 1024 * 1024 * 1024,  // 5GB
    max_concurrency: 32,
    chunk_size: 256 * 1024 * 1024,  // 256MB
}
```

### Python (Production)
```python
# Use defaults
max_workers = 10  # DEFAULT_MAX_WORKERS
```

## Files

- `perf/2025-01-24-final-comparison.md` - Complete benchmark comparison
- `perf/python_benchmark_results_2026-01-24.txt` - Raw Python v1 output
- `context/deadline-cloud/scripted_tests/run_s3_benchmark.py` - Python benchmark script
- `crates/storage/examples/bench_hash_upload.rs` - Rust benchmark tool
- `crates/storage/src/hash_upload/` - Rust pipelined implementation

## Commands

```bash
# Generate test data
cargo run --release --example bench_hash_upload -- generate --test-dir /tmp/bench_vfx --scenario vfx

# Run Rust benchmark (recommended settings)
source creds.sh
cargo run --release --example bench_hash_upload -- run \
  --test-dir /tmp/bench_vfx \
  --bucket adeadlineja \
  --prefix rusty/bench/test \
  --max-memory-gb 5.0 \
  --max-concurrency 32

# Run Python benchmark
cd context/deadline-cloud
hatch run python scripted_tests/run_s3_benchmark.py
```
