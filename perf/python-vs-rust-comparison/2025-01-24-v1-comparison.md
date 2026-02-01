# Python vs Rust Hash+Upload Benchmark Comparison

**Date:** 2025-01-24

## Test Environment

- **Machine:** AWS EC2 instance
- **OS:** Linux
- **Dataset:** 5.5 GB VFX dataset (260 files)
- **S3 Bucket:** s3://adeadlineja/
- **Python:** 3.11 (deadline-cloud from context/deadline-cloud)
- **Rust:** Release build with optimizations

## Results Summary

| Metric | Python | Rust | Winner | Ratio |
|--------|--------|------|--------|-------|
| **Total Time** | 17.25s | 32.60s | Python | 1.89x faster |
| **Throughput** | 323.6 MB/s | 163.3 MB/s | Python | 1.98x higher |
| **Peak Memory** | 5167 MB | 210 MB | Rust | 24.6x less |

## Detailed Results

### Python (deadline-cloud)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 17.21s | 17.23s | 17.32s | **17.25s** |
| Throughput | 324.4 MB/s | 324.0 MB/s | 322.4 MB/s | **323.6 MB/s** |
| Peak Memory | 5370 MB | 5285 MB | 4846 MB | **5167 MB** |
| Files Hashed | 268 | 268 | 268 | 268 |
| Files Uploaded | 268 | 268 | 268 | 268 |

### Rust (rusty-attachments)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 32.94s | 32.40s | 32.47s | **32.60s** |
| Throughput | 161.7 MB/s | 164.3 MB/s | 164.0 MB/s | **163.3 MB/s** |
| Peak Memory | 236 MB | 218 MB | 175 MB | **210 MB** |
| Files Hashed | 260 | 260 | 260 | 260 |
| Files Uploaded | 260 | 260 | 260 | 260 |

## Analysis

### Why Python is Faster

1. **No Memory Backpressure:** Python loads more data into memory concurrently, saturating the network better
2. **Higher Concurrency:** Without memory limits, more uploads happen in parallel
3. **Network Saturation:** The ~324 MB/s throughput suggests Python is closer to saturating the EC2-to-S3 bandwidth

### Why Rust Uses Less Memory

1. **Semaphore-based Memory Pool:** Limits concurrent in-flight data to ~256 MB
2. **Backpressure:** Slows down file reads when uploads are backed up
3. **Bounded Concurrency:** Default 64 concurrent operations with memory limits

### Trade-offs

| Aspect | Python | Rust |
|--------|--------|------|
| Speed | ✅ 2x faster | ❌ Slower |
| Memory | ❌ 5+ GB peak | ✅ ~200 MB peak |
| Predictability | ❌ Memory varies with dataset | ✅ Bounded memory |
| Large Datasets | ❌ May OOM on constrained systems | ✅ Works on any system |

## Recommendations

### For Rust Implementation

To match Python's throughput while maintaining memory bounds:

1. **Increase Memory Pool Size:** Current ~256 MB may be too conservative
2. **Tune Concurrency:** Experiment with higher concurrent upload limits
3. **Profile Network Utilization:** Check if we're CPU-bound or network-bound
4. **Consider Adaptive Backpressure:** Increase memory allowance when network is the bottleneck

### Potential Optimizations

```
Current Rust settings:
- Memory pool: ~256 MB
- Concurrent operations: 64

Suggested experiments:
- Memory pool: 512 MB, 1 GB, 2 GB
- Concurrent operations: 128, 256
```

## Test Commands

### Python Benchmark
```bash
cd context/deadline-cloud
source ../creds.sh
hatch run python scripted_tests/run_s3_benchmark.py
```

### Rust Benchmark
```bash
source creds.sh
cargo run --release --example bench_hash_upload -- run \
  --test-dir /tmp/bench_vfx \
  --bucket adeadlineja \
  --prefix rusty/bench/test1
```

## Files

- `perf/python_benchmark_results_2026-01-24.txt` - Raw Python benchmark output
- `perf/hash-upload-benchmark-results.md` - Rust benchmark details
- `context/deadline-cloud/scripted_tests/run_s3_benchmark.py` - Python benchmark script
- `crates/storage/examples/bench_hash_upload.rs` - Rust benchmark tool
