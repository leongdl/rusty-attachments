# Python vs Rust Performance Comparison

Benchmarks and performance comparisons between Python's deadline-cloud implementation and Rust's rusty-attachments.

## Quick Start

View the latest comparison:
```bash
cat perf/python-vs-rust-comparison/2025-01-24-final-comparison.md
```

## Files

### Comparison Reports
- **`2025-01-24-final-comparison.md`** - Final comprehensive comparison
- **`2025-01-24-v1-comparison.md`** - Version 1 comparison
- **`2025-01-24-v2-comparison.md`** - Version 2 comparison

### Benchmark Results
- **`hash-upload-benchmark-results.md`** - Hash+upload performance results
- **`python_benchmark_results_2026-01-24.txt`** - Raw Python benchmark data
- **`perf-v2-report.txt`** - Performance report v2
- **`perf-v3-report.txt`** - Performance report v3

### Instructions
- **`python-benchmark-instructions.md`** - How to run Python benchmarks

## Key Findings

### Performance Comparison
- Rust shows significant performance improvements over Python
- Better memory efficiency
- Lower latency
- Higher throughput

### Architecture Differences
- **Python:** Uses ThreadPoolExecutor with explicit memory pool
- **Rust:** Uses tokio async runtime with AWS CRT

See the comparison documents for detailed analysis.

## Running Benchmarks

### Python Benchmarks
Follow instructions in `python-benchmark-instructions.md`

### Rust Benchmarks
```bash
# From repository root
cargo bench --package storage
```

## Related

- **Memory pool investigation:** `../memory-pool-investigation/`
- **Design docs:** `../../design/pipelining.md`
