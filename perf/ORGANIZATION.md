# Performance Directory Organization

## Structure

The `perf/` directory is organized into focused subdirectories, each containing related performance investigations and analyses.

```
perf/
├── README.md                           # Main entry point
├── INDEX.md                            # Legacy detailed index
├── ORGANIZATION.md                     # This file
│
├── memory-pool-investigation/          # Memory pool deep dive (2026-01-31)
│   ├── README.md                       # Investigation overview
│   ├── START_HERE.md                   # Quick start guide
│   ├── MEMORY_ISSUE_QUICK_REF.md      # One-page reference
│   ├── REPRODUCTION_GUIDE.md          # Step-by-step reproduction
│   ├── 2026-01-31-deepdive.md        # Complete analysis (600+ lines)
│   ├── TRACE_ANALYSIS.md              # Timeline analysis
│   ├── memory_pool_visualization.md   # Visual explanations
│   ├── simulate_real_upload.py        # Main test script
│   ├── prove_pool_vs_rss.py          # Simple proof
│   ├── prove_memory_leak.py          # Baseline tests
│   ├── run_all_tests.sh              # Run all tests
│   └── ... (more scripts and data)
│
├── python-vs-rust-comparison/          # Performance benchmarks
│   ├── README.md                       # Comparison overview
│   ├── 2025-01-24-final-comparison.md # Final comparison
│   ├── 2025-01-24-v1-comparison.md   # Version 1
│   ├── 2025-01-24-v2-comparison.md   # Version 2
│   ├── hash-upload-benchmark-results.md
│   ├── python-benchmark-instructions.md
│   ├── python_benchmark_results_2026-01-24.txt
│   ├── perf-v2-report.txt
│   └── perf-v3-report.txt
│
├── vfs-performance/                    # VFS optimization
│   ├── README.md                       # VFS performance overview
│   ├── vfs-read-perf-analysis.md      # Read path analysis
│   ├── vfs-read-perf-comparison.md    # Before/after comparison
│   ├── vfs-read-path-improvements.md  # Optimization results
│   ├── vfs-write-perf-analysis.md     # Write path analysis
│   ├── vfs-write-optimization-results.md
│   ├── vfs-perf-lock.md               # Lock contention analysis
│   ├── vfs-read-test.sh               # Test script
│   └── ... (performance data files)
│
└── dashmap-analysis/                   # DashMap investigation
    ├── README.md                       # DashMap overview
    ├── dashmap-analysis.md            # Performance analysis
    └── dashmap-improvements.md        # Optimization recommendations
```

## Navigation

### By Topic

**Memory Issues:**
- Start: `memory-pool-investigation/START_HERE.md`
- Quick ref: `memory-pool-investigation/MEMORY_ISSUE_QUICK_REF.md`
- Deep dive: `memory-pool-investigation/2026-01-31-deepdive.md`

**Performance Comparison:**
- Start: `python-vs-rust-comparison/README.md`
- Latest: `python-vs-rust-comparison/2025-01-24-final-comparison.md`

**VFS Optimization:**
- Start: `vfs-performance/README.md`
- Read: `vfs-performance/vfs-read-perf-analysis.md`
- Write: `vfs-performance/vfs-write-perf-analysis.md`

**Concurrent Data Structures:**
- Start: `dashmap-analysis/README.md`

### By Date

- **2026-01-31:** Memory pool investigation
- **2026-01-24:** Python benchmark results
- **2025-01-24:** Python vs Rust comparison
- **2025-01:** VFS performance optimization
- **2025-01:** DashMap analysis

## Quick Commands

### Memory Pool Investigation
```bash
# Run simulation
python3 perf/memory-pool-investigation/simulate_real_upload.py

# View results
cat /tmp/real_upload_simulation_summary.txt

# Read analysis
cat perf/memory-pool-investigation/START_HERE.md
```

### Python vs Rust Comparison
```bash
# View latest comparison
cat perf/python-vs-rust-comparison/2025-01-24-final-comparison.md

# View benchmark results
cat perf/python-vs-rust-comparison/hash-upload-benchmark-results.md
```

### VFS Performance
```bash
# Run read test
bash perf/vfs-performance/vfs-read-test.sh

# View analysis
cat perf/vfs-performance/vfs-read-perf-analysis.md
```

## File Types

### Documentation
- **README.md** - Overview and navigation for each subdirectory
- **Analysis .md files** - Detailed performance analysis
- **Comparison .md files** - Before/after comparisons
- **Guide .md files** - How-to and reproduction guides

### Scripts
- **Python scripts (.py)** - Test and analysis scripts
- **Shell scripts (.sh)** - Test runners and automation

### Data
- **Trace files (.jsonl)** - Structured trace data
- **Report files (.txt)** - Raw output and reports
- **Performance data (.data)** - Profiling data

## Maintenance

### Adding New Investigations

1. Create a new subdirectory with a descriptive name
2. Add a README.md explaining the investigation
3. Include all related files (docs, scripts, data)
4. Update the main `perf/README.md`
5. Add entry to this ORGANIZATION.md

### File Naming Conventions

- **Dates:** Use YYYY-MM-DD format (e.g., `2026-01-31-deepdive.md`)
- **Descriptive names:** Use kebab-case (e.g., `memory-pool-investigation`)
- **README files:** Always `README.md` (not `README_TOPIC.md`)
- **Scripts:** Descriptive names with extension (e.g., `simulate_real_upload.py`)

## Migration Notes

This organization was created on 2026-01-31 to improve navigation and maintainability.

**Previous structure:** All files in flat `perf/` directory

**New structure:** Organized into topic-based subdirectories

**Benefits:**
- Easier to find related files
- Clear separation of concerns
- Better scalability for future investigations
- Each subdirectory is self-contained

## Related Directories

- **`design/`** - Design documents and architecture
- **`crates/`** - Rust implementation
- **`context/deadline-cloud/`** - Python reference implementation
