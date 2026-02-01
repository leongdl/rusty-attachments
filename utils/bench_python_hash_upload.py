#!/usr/bin/env python3
"""
Python benchmark for hash+upload pipeline using deadline-cloud library.

This script benchmarks the Python pipelined hash+upload implementation
for comparison with the Rust implementation.

Usage:
    # Generate test data first (using Rust tool):
    cargo run --release --example bench_hash_upload -- generate --test-dir /tmp/bench --scenario vfx

    # Run Python benchmark:
    export ENABLE_SNAPSHOTS_LIBRARY=1
    python utils/bench_python_hash_upload.py \
        --test-dir /tmp/bench \
        --output-dir /tmp/python_data_cache

Requirements:
    - deadline-cloud library (pip install from context/deadline-cloud/dist/*.whl)
    - psutil for memory monitoring
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

# Enable snapshots library
os.environ["ENABLE_SNAPSHOTS_LIBRARY"] = "1"

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    print("Warning: psutil not installed, memory tracking disabled")
    HAS_PSUTIL = False
    psutil = None


@dataclass
class BenchmarkMetrics:
    """Metrics collected during benchmark."""

    total_time: float
    hash_time: Optional[float]
    upload_time: Optional[float]
    peak_memory_bytes: int
    total_bytes: int
    files_processed: int
    files_skipped: int
    throughput_bytes_per_sec: float

    def print(self, name: str) -> None:
        """Print benchmark results in a formatted table."""
        print(f"\n=== {name} ===")
        print(f"Total time:      {self.total_time:.2f}s")
        if self.hash_time is not None:
            print(f"  Hash time:     {self.hash_time:.2f}s")
        if self.upload_time is not None:
            print(f"  Upload time:   {self.upload_time:.2f}s")
        print(f"Peak memory:     {self.peak_memory_bytes // (1024 * 1024)} MB")
        print(f"Total bytes:     {self.total_bytes // (1024 * 1024)} MB")
        print(f"Files processed: {self.files_processed}")
        print(f"Files skipped:   {self.files_skipped}")
        print(f"Throughput:      {self.throughput_bytes_per_sec / (1024 * 1024):.2f} MB/s")


def get_memory_usage() -> int:
    """Get current process memory usage in bytes."""
    if not HAS_PSUTIL:
        return 0
    process = psutil.Process()
    return process.memory_info().rss


def collect_test_files(test_dir: Path) -> List[Path]:
    """Collect all files in test directory."""
    files: List[Path] = []
    for root, _, filenames in os.walk(test_dir):
        for name in filenames:
            files.append(Path(root) / name)
    return files


def run_benchmark_local(
    test_dir: Path,
    output_dir: Path,
    clear_cache: bool = True,
    max_memory_bytes: Optional[int] = None,
    max_workers: Optional[int] = None,
) -> BenchmarkMetrics:
    """
    Run the Python hash+upload benchmark with local filesystem backend.

    Args:
        test_dir: Directory containing test files
        output_dir: Directory for output data cache
        clear_cache: Whether to clear caches before run
        max_memory_bytes: Maximum memory for pipeline
        max_workers: Maximum parallel workers

    Returns:
        BenchmarkMetrics with results
    """
    # Import deadline-cloud modules
    from deadline.job_attachments._snapshots._operations import (
        collect_abs_snapshot,
        hash_upload_abs_manifest,
    )
    from deadline.job_attachments._snapshots._content_addressed_data_cache import FileSystemDataCache
    from deadline.job_attachments.caches.hash_cache import HashCache

    # Clear caches if requested
    if clear_cache:
        cache_dir = Path.home() / ".deadline" / "cache"
        hash_cache_path = cache_dir / "hash_cache.db"

        if hash_cache_path.exists():
            hash_cache_path.unlink()
            print(f"Cleared hash cache: {hash_cache_path}")

        # Clear output directory
        if output_dir.exists():
            import shutil
            shutil.rmtree(output_dir)
            print(f"Cleared output dir: {output_dir}")

    # Ensure output directory exists
    output_dir.mkdir(parents=True, exist_ok=True)

    # Collect test files
    print(f"Collecting files from {test_dir}...")
    files = collect_test_files(test_dir)
    print(f"Found {len(files)} files")

    # Calculate total size
    total_bytes = sum(f.stat().st_size for f in files)
    print(f"Total size: {total_bytes / (1024 * 1024):.2f} MB")

    # Create filesystem data cache
    data_cache = FileSystemDataCache(root_path=output_dir)

    # Create hash cache
    hash_cache = HashCache()

    # Track memory
    start_memory = get_memory_usage()
    peak_memory = start_memory

    # Step 1: Collect snapshot (no hashing yet)
    print("\nCollecting snapshot...")
    collect_start = time.perf_counter()

    # Get directories from test_dir
    directories = [test_dir]

    snapshot = collect_abs_snapshot(
        directories=directories,
        filenames=[],
    )
    collect_time = time.perf_counter() - collect_start
    print(f"Snapshot collected in {collect_time:.2f}s")
    print(f"  Files: {len(snapshot.files)}")
    print(f"  Dirs: {len(snapshot.dirs)}")
    print(f"  Total size: {snapshot.totalSize / (1024 * 1024):.2f} MB")

    peak_memory = max(peak_memory, get_memory_usage())

    # Step 2: Hash and upload (pipelined)
    print("\nRunning hash+upload pipeline...")
    pipeline_start = time.perf_counter()

    # Progress callback
    last_progress_time = [time.perf_counter()]

    def on_progress(metadata) -> bool:
        """Progress callback."""
        nonlocal peak_memory
        peak_memory = max(peak_memory, get_memory_usage())

        # Print progress every 2 seconds
        now = time.perf_counter()
        if now - last_progress_time[0] >= 2.0:
            print(f"  Progress: {metadata.progress:.1f}% - {metadata.progressMessage}")
            last_progress_time[0] = now
        return True

    kwargs = {
        "manifest": snapshot,
        "data_cache": data_cache,
        "hash_cache": hash_cache,
        "on_progress": on_progress,
    }
    if max_memory_bytes is not None:
        kwargs["max_memory_bytes"] = max_memory_bytes
    if max_workers is not None:
        kwargs["max_workers"] = max_workers

    result = hash_upload_abs_manifest(**kwargs)

    pipeline_time = time.perf_counter() - pipeline_start
    total_time = time.perf_counter() - collect_start

    peak_memory = max(peak_memory, get_memory_usage())

    # Extract statistics
    stats = result.statistics
    print(f"\nPipeline completed in {pipeline_time:.2f}s")
    print(f"  {stats.progressMessage}")

    # Calculate metrics
    throughput = total_bytes / total_time if total_time > 0 else 0

    return BenchmarkMetrics(
        total_time=total_time,
        hash_time=None,  # Pipelined, can't separate
        upload_time=None,  # Pipelined, can't separate
        peak_memory_bytes=peak_memory - start_memory,
        total_bytes=total_bytes,
        files_processed=stats.hashed_file_chunks + stats.hash_skipped_file_chunks,
        files_skipped=stats.upload_skipped_file_chunks,
        throughput_bytes_per_sec=throughput,
    )


def main() -> None:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Python hash+upload benchmark using deadline-cloud library"
    )
    parser.add_argument(
        "--test-dir",
        type=Path,
        default=Path("/tmp/hash_upload_bench"),
        help="Directory containing test files",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("/tmp/python_data_cache"),
        help="Directory for output data cache (local mode)",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=1,
        help="Number of iterations",
    )
    parser.add_argument(
        "--clear-cache",
        action="store_true",
        default=True,
        help="Clear caches before each run",
    )
    parser.add_argument(
        "--no-clear-cache",
        action="store_false",
        dest="clear_cache",
        help="Don't clear caches",
    )
    parser.add_argument(
        "--max-memory-mb",
        type=int,
        default=None,
        help="Maximum memory in MB (default: auto)",
    )
    parser.add_argument(
        "--max-workers",
        type=int,
        default=None,
        help="Maximum parallel workers (default: 10)",
    )

    args = parser.parse_args()

    # Validate test directory
    if not args.test_dir.exists():
        print(f"Error: Test directory does not exist: {args.test_dir}")
        print("Generate test data first with:")
        print(f"  cargo run --release --example bench_hash_upload -- generate --test-dir {args.test_dir}")
        sys.exit(1)

    print("Python Hash+Upload Benchmark")
    print("============================")
    print(f"Test dir:    {args.test_dir}")
    print(f"Output dir:  {args.output_dir}")
    print(f"Iterations:  {args.iterations}")
    print(f"Clear cache: {args.clear_cache}")
    if args.max_memory_mb:
        print(f"Max memory:  {args.max_memory_mb} MB")
    if args.max_workers:
        print(f"Max workers: {args.max_workers}")

    # Convert max memory to bytes
    max_memory_bytes = args.max_memory_mb * 1024 * 1024 if args.max_memory_mb else None

    # Run benchmarks
    all_metrics: List[BenchmarkMetrics] = []

    for i in range(args.iterations):
        print(f"\n{'='*60}")
        print(f"Iteration {i + 1}/{args.iterations}")
        print("=" * 60)

        metrics = run_benchmark_local(
            test_dir=args.test_dir,
            output_dir=args.output_dir,
            clear_cache=args.clear_cache,
            max_memory_bytes=max_memory_bytes,
            max_workers=args.max_workers,
        )
        metrics.print(f"Iteration {i + 1}")
        all_metrics.append(metrics)

    # Print summary
    if len(all_metrics) > 1:
        print("\n" + "=" * 60)
        print("SUMMARY")
        print("=" * 60)

        avg_time = sum(m.total_time for m in all_metrics) / len(all_metrics)
        avg_throughput = sum(m.throughput_bytes_per_sec for m in all_metrics) / len(all_metrics)
        max_memory = max(m.peak_memory_bytes for m in all_metrics)

        print(f"Average time:       {avg_time:.2f}s")
        print(f"Average throughput: {avg_throughput / (1024 * 1024):.2f} MB/s")
        print(f"Peak memory:        {max_memory // (1024 * 1024)} MB")


if __name__ == "__main__":
    main()
