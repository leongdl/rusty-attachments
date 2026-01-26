# Python vs Rust Hash+Upload Benchmark Comparison (v2)

**Date:** 2025-01-24

## Summary

After tuning Rust to use the same concurrency as Python (10 workers) and increasing memory allowance to 5GB, **Rust is now 1.2x faster than Python while using 22x less memory**.

## Results Summary

| Metric | Python | Rust v1 | Rust v2 | Winner |
|--------|--------|---------|---------|--------|
| **Total Time** | 17.25s | 32.60s | **14.53s** | Rust v2 |
| **Throughput** | 323.6 MB/s | 163.3 MB/s | **372.9 MB/s** | Rust v2 |
| **Peak Memory** | 5167 MB | 210 MB | **235 MB** | Rust (22x less) |

## Configuration Changes (v1 → v2)

| Setting | v1 | v2 |
|---------|----|----|
| Max Memory | 1 GB | 5 GB |
| Max Concurrency | 64 | 10 |

The key insight: Python uses 10 workers (`DEFAULT_MAX_WORKERS = 10`), not 64. Matching this concurrency level improved Rust performance significantly.

## Test Environment

- **Machine:** AWS EC2 instance
- **OS:** Linux
- **Dataset:** 5.5 GB VFX dataset (260 files)
- **S3 Bucket:** s3://adeadlineja/
- **Python:** 3.11 (deadline-cloud)
- **Rust:** Release build with optimizations

## Detailed Results

### Python (deadline-cloud)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 17.21s | 17.23s | 17.32s | **17.25s** |
| Throughput | 324.4 MB/s | 324.0 MB/s | 322.4 MB/s | **323.6 MB/s** |
| Peak Memory | 5370 MB | 5285 MB | 4846 MB | **5167 MB** |

### Rust v2 (5GB memory, 10 concurrency)

| Metric | Run 4 | Run 5 | Run 6 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 13.01s | 13.23s | 17.34s | **14.53s** |
| Throughput | 409.4 MB/s | 402.4 MB/s | 307.1 MB/s | **372.9 MB/s** |
| Peak Memory | 237 MB | 221 MB | 248 MB | **235 MB** |

### Rust v1 (1GB memory, 64 concurrency) - Previous

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 32.94s | 32.40s | 32.47s | **32.60s** |
| Throughput | 161.7 MB/s | 164.3 MB/s | 164.0 MB/s | **163.3 MB/s** |
| Peak Memory | 236 MB | 218 MB | 175 MB | **210 MB** |

## Analysis

### Why Rust v2 is Faster

1. **Matching Concurrency:** Using 10 workers (same as Python) instead of 64 reduces contention
2. **Higher Memory Allowance:** 5GB allows more data in-flight without backpressure blocking
3. **Better Pipeline Efficiency:** Less context switching with fewer concurrent tasks

### Memory Efficiency

Despite allowing 5GB, Rust only used ~235 MB peak:
- Rust's memory pool provides backpressure but doesn't force usage
- The semaphore-based allocation is efficient
- Python's ThreadPoolExecutor has higher overhead per task

### Variance in Run 6

Run 6 was slower (17.34s vs ~13s). This could be due to:
- Network variability
- S3 throttling
- Background system activity

## S3 Locations

- Python: `s3://adeadlineja/python/bench/run{1,2,3}/`
- Rust v1: `s3://adeadlineja/rusty/bench/run{1,2,3}/`
- Rust v2: `s3://adeadlineja/rusty/bench/run{4,5,6}/`

## Test Commands

### Python Benchmark
```bash
cd context/deadline-cloud
source ../creds.sh
hatch run python scripted_tests/run_s3_benchmark.py
```

### Rust Benchmark (v2 settings)
```bash
source creds.sh
cargo run --release --example bench_hash_upload -- run \
  --test-dir /tmp/bench_vfx \
  --bucket adeadlineja \
  --prefix rusty/bench/run4 \
  --max-memory-gb 5.0 \
  --max-concurrency 10
```

## Conclusion

With proper tuning, Rust achieves:
- **1.2x faster** than Python (14.5s vs 17.3s)
- **22x less memory** (235 MB vs 5.2 GB)
- **15% higher throughput** (373 MB/s vs 324 MB/s)

The Rust implementation is now both faster AND more memory-efficient than Python.
