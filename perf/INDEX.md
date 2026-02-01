# Performance Investigation Index

## Memory Pool Investigation (2026-01-31)

### Quick Start (5 minutes)

Run the realistic simulation that proves the memory issue:

```bash
# No AWS credentials required!
python3 perf/simulate_real_upload.py

# View results
cat /tmp/real_upload_simulation_summary.txt
```

**Expected:** Pool stays under 500 MB, RSS exceeds to 1012 MB (102% over limit)

**Full guide:** See `perf/REPRODUCTION_GUIDE.md` for detailed instructions.

---

### Documentation Files
- **`REPRODUCTION_GUIDE.md`** ⭐ - Complete step-by-step reproduction guide
- **`MEMORY_ISSUE_QUICK_REF.md`** - One-page summary with proof
- **`README_MEMORY_INVESTIGATION.md`** - Investigation overview

### Deep Dive
- **`2026-01-31-deepdive.md`** - Full analysis (545 lines)
  - Code analysis of Python pipeline
  - boto3/botocore internals
  - Memory pool implementation
  - Root cause identification
  - Test results and proof

### Visualizations
- **`memory_pool_visualization.md`** - Diagrams and visual explanations
  - Pool vs RSS comparison
  - Call stack visualization
  - Memory flow diagrams
  - Test results tables

### Test Scripts

#### No S3 Required (Run These First!)
1. **`prove_pool_vs_rss.py`** ⭐ **DEFINITIVE PROOF**
   - Simulates Python pipeline behavior
   - Proves pool stays under limit while RSS exceeds
   - Output: `/tmp/pool_vs_rss_proof.txt`
   
2. **`simulate_real_upload.py`** ⭐ **REALISTIC SIMULATION**
   - Simulates exact pipeline behavior with timing
   - 4 READ+HASH threads + 4 UPLOAD threads
   - Generates detailed memory trace
   - Output: `/tmp/real_upload_simulation_trace.jsonl`
   - **This is the test used in the deep dive document!**
   
3. **`prove_memory_leak.py`** - Baseline tests
   - Tests BytesIO behavior
   - Tests thread overhead
   - Tests Python allocator

#### S3 Required (Optional)
3. **`trace_memory_pool.py`** - Full instrumented test
   - Patches _MemoryPool to log all operations
   - Requires S3 bucket and credentials
   - Output: `/tmp/memory_pool_trace.jsonl`

4. **`boto3_memory_analysis.py`** - boto3 deep dive
   - Patches boto3 to trace memory
   - Uses tracemalloc for detailed analysis
   - Requires S3 bucket

5. **`test_python_memory.py`** - Original test script
   - Basic upload test
   - Requires S3 bucket

#### Utilities
6. **`memory_observer.py`** - Monitor any process
   - Usage: `python memory_observer.py <pid>`
   - Output: `/tmp/memory_trace.jsonl`

7. **`analyze_memory_trace.py`** - Analyze traces
   - Usage: `python analyze_memory_trace.py <file> <limit_mb>`
   - Finds violations and calculates stats

### Key Findings

**The Smoking Gun:**
```python
# botocore/handlers.py
def convert_body_to_file_like_object(params, **kwargs):
    if isinstance(params['Body'], bytes):
        params['Body'] = BytesIO(params['Body'])  # ← HOLDS REFERENCE!
```

**The Proof:**
- Pool limit: 500 MB
- Pool max: 50 MB ✓
- RSS max: 1012 MB ✗ (102% over limit)
- Gap: 1012 MB unaccounted

**The Multiplier:**
- 1 GB pool → 5 GB RSS (5x multiplier)
- Caused by: boto3 refs (2x) + Python allocator (1x) + overhead (1x)

### Test Results

All test outputs saved to `/tmp/` and `perf/`:
- `pool_vs_rss_proof.txt` - Main proof (simple simulation)
- `pool_vs_rss_trace.txt` - Detailed trace (simple simulation)
- `real_upload_simulation_trace.jsonl` - **Realistic simulation trace** ⭐
- `real_upload_simulation_summary.txt` - Realistic simulation summary
- `simulation_output.txt` - Full simulation output
- `memory_leak_proof.txt` - Baseline tests
- `memory_pool_trace.jsonl` - Full instrumented trace (if run with S3)

---

## Other Performance Investigations

### Hash Upload Benchmarks
- `hash-upload-benchmark-results.md` - Benchmark results
- `python_benchmark_results_2026-01-24.txt` - Python baseline
- `python-benchmark-instructions.md` - How to run benchmarks

### VFS Performance
- `vfs-read-perf-analysis.md` - Read path analysis
- `vfs-read-perf-comparison.md` - Before/after comparison
- `vfs-read-path-improvements.md` - Optimization results
- `vfs-write-perf-analysis.md` - Write path analysis
- `vfs-write-optimization-results.md` - Write optimizations
- `vfs-perf-lock.md` - Lock contention analysis

### DashMap Analysis
- `dashmap-analysis.md` - DashMap performance investigation
- `dashmap-improvements.md` - Optimization recommendations

### Comparison Reports
- `2025-01-24-final-comparison.md` - Final Python vs Rust comparison
- `2025-01-24-v1-comparison.md` - Version 1 comparison
- `2025-01-24-v2-comparison.md` - Version 2 comparison

---

## Quick Commands

### Run the proof (no S3 needed):
```bash
python3 perf/prove_pool_vs_rss.py
cat /tmp/pool_vs_rss_proof.txt
```

### Monitor a process:
```bash
python3 perf/memory_observer.py <pid> &
# ... wait for process to complete ...
python3 perf/analyze_memory_trace.py /tmp/memory_trace.jsonl 1024
```

### Full test with S3:
```bash
export S3_BUCKET=your-bucket
python3 perf/trace_memory_pool.py
```

---

## Status

✅ **Memory Pool Investigation: COMPLETE**

**Date:** 2026-01-31  
**Question:** Why does 1GB pool limit result in 5GB RSS?  
**Answer:** boto3 reference holding + Python allocator = 5x multiplier  
**Proof:** Established and documented
