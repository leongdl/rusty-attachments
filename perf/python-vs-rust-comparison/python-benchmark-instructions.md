# Python (deadline-cloud) Hash+Upload Benchmark Instructions

## Context for New Shell

This document provides instructions to run the Python deadline-cloud hash+upload benchmark for comparison with the Rust implementation.

### Rust Baseline Results (Target to Compare Against)

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | 32.94s | 32.40s | 32.47s | **32.60s** |
| Throughput | 161.7 MB/s | 164.3 MB/s | 164.0 MB/s | **163.3 MB/s** |
| Peak Memory | 236 MB | 218 MB | 175 MB | **210 MB** |

- **Dataset:** 5.5 GB VFX dataset, 260 files at `/tmp/bench_vfx`
- **S3 Bucket:** `s3://adeadlineja/rusty/bench/run{1,2,3}/`
- **Implementation:** Pipelined hash+upload with Tokio async

---

## Setup Instructions

### 1. Environment Setup

```bash
# Navigate to deadline-cloud directory
cd context/deadline-cloud

# Check Python version (needs 3.10+)
python3.11 --version

# Set up hatch environment
python3.11 -m pip install --user hatch
export PATH="$HOME/.local/bin:$PATH"

# Create hatch environment
hatch env create
```

### 2. Clear ALL Caches (Critical for Fair Comparison)

```bash
# Clear Python deadline-cloud caches
rm -rf ~/.deadline/cache/
rm -rf ~/.deadline/job_attachments/

# Clear any S3 check caches
rm -rf ~/.deadline/cache/s3_check_cache.db

# Verify caches are cleared
ls -la ~/.deadline/ 2>/dev/null || echo "No .deadline directory"
```

### 3. Clear S3 Test Prefix

```bash
# Source credentials
source /home/ssm-user/rusty-attachments/creds.sh

# Clear Python test prefix (use different prefix than Rust)
aws s3 rm s3://adeadlineja/python/bench/ --recursive
```

### 4. Verify Test Data Exists

```bash
# Check test data
du -sh /tmp/bench_vfx
find /tmp/bench_vfx -type f | wc -l
# Expected: ~5.3G, 260 files
```

---

## Running the Benchmark

### Option A: Using the Custom Benchmark Script

The script at `utils/bench_python_hash_upload.py` uses local filesystem. For S3 comparison, we need to use the deadline-cloud S3AssetManager directly.

### Option B: Using deadline-cloud S3AssetManager (Recommended for S3 Comparison)

Create and run this benchmark script:

```bash
cd context/deadline-cloud

# Create benchmark script
cat > /tmp/python_s3_bench.py << 'EOF'
#!/usr/bin/env python3
"""
Python S3 hash+upload benchmark for comparison with Rust implementation.
Uses deadline-cloud S3AssetManager for direct S3 uploads.
"""
import os
import sys
import time
from pathlib import Path

# Configuration
TEST_DIR = "/tmp/bench_vfx"
S3_BUCKET = "adeadlineja"
S3_PREFIX = "python/bench"

def get_memory_mb():
    """Get current RSS memory in MB."""
    try:
        with open('/proc/self/status', 'r') as f:
            for line in f:
                if line.startswith('VmRSS:'):
                    return int(line.split()[1]) // 1024
    except:
        pass
    return 0

def run_benchmark(run_number: int):
    """Run a single benchmark iteration."""
    from deadline.job_attachments.upload import S3AssetManager
    from deadline.job_attachments.models import JobAttachmentS3Settings
    from deadline.job_attachments.caches.hash_cache import HashCache
    
    # Collect files
    files = []
    for root, _, filenames in os.walk(TEST_DIR):
        for name in filenames:
            files.append(os.path.join(root, name))
    
    total_bytes = sum(os.path.getsize(f) for f in files)
    print(f"Found {len(files)} files ({total_bytes / 1_000_000:.2f} MB)")
    
    # Create S3 settings (direct S3 access, no Deadline service)
    s3_settings = JobAttachmentS3Settings(
        s3BucketName=S3_BUCKET,
        rootPrefix=f"{S3_PREFIX}/run{run_number}",
    )
    
    # Create asset manager
    asset_manager = S3AssetManager(
        farm_id="benchmark",
        queue_id="benchmark", 
        job_attachment_settings=s3_settings,
    )
    
    # Track memory
    start_memory = get_memory_mb()
    peak_memory = start_memory
    
    print(f"\nStarting hash+upload to s3://{S3_BUCKET}/{S3_PREFIX}/run{run_number}/...")
    start = time.perf_counter()
    
    # Prepare paths
    upload_group = asset_manager.prepare_paths_for_upload(
        TEST_DIR, files, [Path(TEST_DIR) / "outputs"], []
    )
    
    # Hash and create manifest
    (hash_stats, manifests) = asset_manager.hash_assets_and_create_manifest(
        asset_groups=upload_group.asset_groups,
        total_input_files=upload_group.total_input_files,
        total_input_bytes=upload_group.total_input_bytes,
    )
    
    hash_time = time.perf_counter() - start
    peak_memory = max(peak_memory, get_memory_mb())
    print(f"Hashing completed in {hash_time:.2f}s")
    print(f"  {hash_stats}")
    
    # Upload to S3
    upload_start = time.perf_counter()
    (upload_stats, attachments) = asset_manager.upload_assets(manifests)
    upload_time = time.perf_counter() - upload_start
    
    total_time = time.perf_counter() - start
    peak_memory = max(peak_memory, get_memory_mb())
    
    print(f"Upload completed in {upload_time:.2f}s")
    print(f"  {upload_stats}")
    
    throughput = total_bytes / total_time / 1_000_000
    
    print(f"\n=== Run {run_number} Results ===")
    print(f"Total time:    {total_time:.2f}s")
    print(f"  Hash time:   {hash_time:.2f}s")
    print(f"  Upload time: {upload_time:.2f}s")
    print(f"Throughput:    {throughput:.2f} MB/s")
    print(f"Peak memory:   {peak_memory - start_memory} MB")
    print(f"Files:         {len(files)}")
    print(f"Total size:    {total_bytes / 1_000_000:.2f} MB")
    
    return {
        'total_time': total_time,
        'hash_time': hash_time,
        'upload_time': upload_time,
        'throughput': throughput,
        'peak_memory': peak_memory - start_memory,
        'files': len(files),
        'bytes': total_bytes,
    }

def clear_caches():
    """Clear all Python caches."""
    import shutil
    cache_dirs = [
        Path.home() / ".deadline" / "cache",
        Path.home() / ".deadline" / "job_attachments",
    ]
    for cache_dir in cache_dirs:
        if cache_dir.exists():
            shutil.rmtree(cache_dir)
            print(f"Cleared: {cache_dir}")

if __name__ == "__main__":
    print("Python S3 Hash+Upload Benchmark")
    print("================================")
    print(f"Test dir: {TEST_DIR}")
    print(f"S3 dest:  s3://{S3_BUCKET}/{S3_PREFIX}/")
    print()
    
    results = []
    for run in range(1, 4):
        print(f"\n{'='*60}")
        print(f"Run {run}/3")
        print("="*60)
        
        # Clear caches before each run
        clear_caches()
        
        result = run_benchmark(run)
        results.append(result)
    
    # Summary
    print(f"\n{'='*60}")
    print("SUMMARY")
    print("="*60)
    avg_time = sum(r['total_time'] for r in results) / len(results)
    avg_throughput = sum(r['throughput'] for r in results) / len(results)
    print(f"Average time:       {avg_time:.2f}s")
    print(f"Average throughput: {avg_throughput:.2f} MB/s")
EOF

# Run with hatch
hatch run python /tmp/python_s3_bench.py 2>&1 | tee /tmp/python_s3_results.txt
```

---

## Expected Output Format

```
=== Run X Results ===
Total time:    XX.XXs
  Hash time:   XX.XXs
  Upload time: XX.XXs
Throughput:    XXX.XX MB/s
Peak memory:   XXX MB
Files:         260
Total size:    5583.26 MB
```

---

## Results Template

Fill in after running:

| Metric | Run 1 | Run 2 | Run 3 | Average |
|--------|-------|-------|-------|---------|
| Total Time | | | | |
| Hash Time | | | | |
| Upload Time | | | | |
| Throughput (MB/s) | | | | |
| Peak Memory (MB) | | | | |

---

## Comparison Table

| Metric | Python | Rust | Ratio (Rust/Python) |
|--------|--------|------|---------------------|
| Total Time | ? | 32.6s | |
| Throughput | ? | 163.3 MB/s | |
| Peak Memory | ? | 210 MB | |

---

## Notes

- Python 3.11 is available at `/usr/bin/python3.11`
- Credentials: `source /home/ssm-user/rusty-attachments/creds.sh`
- Test data: `/tmp/bench_vfx` (5.5 GB, 260 files)
- The Python implementation uses ThreadPoolExecutor for concurrency
- The Rust implementation uses Tokio async + spawn_blocking
- Both implementations use pipelined hash+upload (read once, hash, upload from same buffer)
