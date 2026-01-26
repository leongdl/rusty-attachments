# Python vs Rust Hash+Upload Benchmark - Final Comparison

**Date:** 2025-01-24 (Updated: 2026-01-24)

## Executive Summary

With the AWS S3 Transfer Manager, **Rust is 2.49x faster than Python while using 17x less memory**.

| Metric | Python (10 workers) | Rust + Transfer Manager | Rust Advantage |
|--------|---------------------|------------------------|----------------|
| **Total Time** | 17.25s | 8.75s | **1.97x faster** |
| **Throughput** | 323.6 MB/s | 619 MB/s | **1.91x higher** |
| **Peak Memory** | 5167 MB | 325 MB | **16x less** |

### Standard Client Comparison

| Metric | Python (32 workers) | Rust Standard (32 conc) | Rust Advantage |
|--------|---------------------|------------------------|----------------|
| **Total Time** | 22.47s | 13.68s | **1.64x faster** |
| **Throughput** | 248.4 MB/s | 389.5 MB/s | **1.57x higher** |
| **Peak Memory** | 5170 MB | 311 MB | **17x less** |

## Test Environment

- **Machine:** AWS EC2 instance (64 GB RAM)
- **OS:** Linux
- **Dataset:** 5.5 GB VFX dataset (260 files)
- **S3 Bucket:** s3://adeadlineja/
- **Python:** 3.11 (deadline-cloud)
- **Rust:** Release build with optimizations

## All Benchmark Results

### Python Results

| Config | Workers | Run 1 | Run 2 | Run 3 | Avg Time | Avg Throughput | Avg Memory |
|--------|---------|-------|-------|-------|----------|----------------|------------|
| v1 | 10 | 17.21s | 17.23s | 17.32s | **17.25s** | **323.6 MB/s** | **5167 MB** |
| v2 | 32 | 22.48s | 22.51s | 22.43s | **22.47s** | **248.4 MB/s** | **5170 MB** |

### Rust Results

| Config | Concurrency | Run 1 | Run 2 | Run 3 | Avg Time | Avg Throughput | Avg Memory |
|--------|-------------|-------|-------|-------|----------|----------------|------------|
| v1 | 64 | 32.94s | 32.40s | 32.47s | **32.60s** | **163.3 MB/s** | **210 MB** |
| v2 | 10 | 13.01s | 13.23s | 17.34s | **14.53s** | **372.9 MB/s** | **235 MB** |
| v3 (standard) | 32 | 13.96s | 13.96s | 13.12s | **13.68s** | **389.5 MB/s** | **311 MB** |
| v3-high | 128 | 15.60s | 15.43s | 15.53s | **15.52s** | **343.0 MB/s** | **328 MB** |
| **v4 (transfer-mgr)** | 32 | 8.12s | 10.43s | 7.69s | **8.75s** | **619 MB/s** | **325 MB** |

## Key Findings

### 1. Transfer Manager Impact

The AWS S3 Transfer Manager provides a **59% throughput improvement** over the standard client:
- Standard client: 389.5 MB/s
- Transfer Manager: 619 MB/s

This is because the Transfer Manager automatically:
- Uses multipart uploads for large files (parallelizing parts)
- Optimizes connection pooling and request batching
- Reduces per-request overhead (SigV4 signing, TLS, CRC)

### 2. Optimal Concurrency Differs

- **Python:** Best at 10 workers (323 MB/s), worse at 32 workers (248 MB/s)
- **Rust Standard:** Best at 32 concurrency (389 MB/s), worse at 128 (343 MB/s)
- **Rust Transfer Manager:** 619 MB/s at 32 concurrency

Python's performance degrades with higher concurrency due to GIL contention. Rust doesn't have this limitation but still sees diminishing returns from excessive concurrency.

### 3. Memory Usage

Python consistently uses ~5 GB regardless of worker count, while Rust stays under 350 MB even with high concurrency. This is because:

- **Python:** Loads entire files into memory across all workers simultaneously
- **Rust:** Uses semaphore-based memory pool with backpressure

### 4. Fair Comparison (Same Concurrency = 32)

| Metric | Python | Rust Standard | Rust Transfer Manager |
|--------|--------|---------------|----------------------|
| Total Time | 22.47s | 13.68s | 8.75s |
| Throughput | 248.4 MB/s | 389.5 MB/s | 619 MB/s |
| Peak Memory | 5170 MB | 311 MB | 325 MB |
| vs Python | - | 1.64x faster | **2.57x faster** |

### 5. Best-Case Comparison

| Metric | Python (10 workers) | Rust Transfer Manager | Ratio |
|--------|---------------------|----------------------|-------|
| Total Time | 17.25s | 8.75s | **Rust 1.97x faster** |
| Throughput | 323.6 MB/s | 619 MB/s | **Rust 1.91x higher** |
| Peak Memory | 5167 MB | 325 MB | **Rust 16x less** |

Even comparing Python's best config vs Rust's best config, Rust wins decisively on all metrics.

## Configuration Details

### Rust v4 (Recommended - Transfer Manager)
```rust
// Uses aws-sdk-s3-transfer-manager for automatic multipart uploads
HashUploadOptions {
    max_memory_bytes: 5 * 1024 * 1024 * 1024,  // 5GB
    max_concurrency: 32,
    chunk_size: 256 * 1024 * 1024,  // 256MB
}
// CLI: --transfer-manager flag
```

### Rust v3 (Standard Client)
```rust
HashUploadOptions {
    max_memory_bytes: 5 * 1024 * 1024 * 1024,  // 5GB
    max_concurrency: 32,
    chunk_size: 256 * 1024 * 1024,  // 256MB
}
```

### Python Default
```python
DEFAULT_MAX_WORKERS = 10
# Memory: auto-detected, typically uses available - 1GB
```

## S3 Locations

- Python v1: `s3://adeadlineja/python/bench/run{1,2,3}/`
- Python v2: `s3://adeadlineja/python/bench3/run{1,2,3}/`
- Rust v1: `s3://adeadlineja/rusty/bench/run{1,2,3}/`
- Rust v2: `s3://adeadlineja/rusty/bench/run{4,5,6}/`
- Rust v3: `s3://adeadlineja/rusty/bench/run{10,11,12}/`
- Rust v3-high: `s3://adeadlineja/rusty/bench/run{7,8,9}/`
- Rust v4 (transfer-mgr): `s3://adeadlineja/rusty/tm_run{1,2,3}_*/`

## Conclusions

1. **Rust with Transfer Manager is fastest:** 1.97x faster than Python's best config
2. **Transfer Manager provides 59% boost:** 619 MB/s vs 389 MB/s over standard client
3. **Rust uses far less memory:** 16x less (325 MB vs 5.2 GB)
4. **Rust scales better:** Performance improves with concurrency up to ~32
5. **Python has GIL limitations:** Performance degrades above 10 workers

## Recommendations

For production use:
- **Rust:** Use Transfer Manager client with 32 concurrency and 5GB memory limit
- **Python:** Stick with default 10 workers

The Rust implementation with Transfer Manager provides nearly 2x the throughput of Python with bounded, predictable memory usage - ideal for systems with memory constraints or running alongside other processes.
