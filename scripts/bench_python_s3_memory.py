#!/usr/bin/env python3
"""
Python S3 upload benchmark with memory monitoring.

This script benchmarks the Python pipelined hash+upload implementation
with real S3 uploads and tracks actual RSS memory usage.
"""

from __future__ import annotations

import os
import sys
import time
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

# Enable snapshots library
os.environ["ENABLE_SNAPSHOTS_LIBRARY"] = "1"

import psutil


@dataclass
class MemorySample:
    """A single memory sample."""
    timestamp: float
    rss_mb: float
    vms_mb: float
    pool_mb: float


class MemoryMonitor:
    """Monitor process memory in background thread."""

    def __init__(self, interval: float = 0.5) -> None:
        self.interval: float = interval
        self.samples: List[MemorySample] = []
        self._stop: bool = False
        self._thread: Optional[threading.Thread] = None
        self._pool_allocated: float = 0.0
        self._lock = threading.Lock()

    def set_pool_allocated(self, bytes_val: int) -> None:
        """Update current pool allocation."""
        with self._lock:
            self._pool_allocated = bytes_val / (1024 * 1024)

    def start(self) -> None:
        """Start monitoring."""
        self._stop = False
        self._thread = threading.Thread(target=self._monitor_loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        """Stop monitoring."""
        self._stop = True
        if self._thread:
            self._thread.join(timeout=2.0)

    def _monitor_loop(self) -> None:
        """Background monitoring loop."""
        process = psutil.Process()
        while not self._stop:
            mem = process.memory_info()
            with self._lock:
                pool_mb: float = self._pool_allocated
            sample = MemorySample(
                timestamp=time.time(),
                rss_mb=mem.rss / (1024 * 1024),
                vms_mb=mem.vms / (1024 * 1024),
                pool_mb=pool_mb,
            )
            self.samples.append(sample)
            time.sleep(self.interval)

    def get_stats(self) -> dict:
        """Get memory statistics."""
        if not self.samples:
            return {}
        rss_values: List[float] = [s.rss_mb for s in self.samples]
        pool_values: List[float] = [s.pool_mb for s in self.samples]
        return {
            "min_rss_mb": min(rss_values),
            "max_rss_mb": max(rss_values),
            "avg_rss_mb": sum(rss_values) / len(rss_values),
            "max_pool_mb": max(pool_values) if pool_values else 0,
            "samples": len(self.samples),
        }


def run_s3_benchmark(
    test_dir: Path,
    bucket: str,
    prefix: str,
    max_memory_mb: int = 1024,
    max_workers: int = 4,
) -> dict:
    """
    Run Python hash+upload benchmark with real S3.

    Args:
        test_dir: Directory containing test files
        bucket: S3 bucket name
        prefix: S3 key prefix
        max_memory_mb: Maximum memory pool size in MB
        max_workers: Maximum parallel workers

    Returns:
        Dictionary with benchmark results
    """
    from deadline.job_attachments._snapshots._operations import (
        collect_abs_snapshot,
        hash_upload_abs_manifest,
    )
    from deadline.job_attachments._snapshots._content_addressed_data_cache import S3DataCache
    from deadline.job_attachments.caches.hash_cache import HashCache

    # Clear hash cache
    cache_dir: Path = Path.home() / ".deadline" / "cache"
    hash_cache_path: Path = cache_dir / "hash_cache.db"
    if hash_cache_path.exists():
        hash_cache_path.unlink()
        print(f"Cleared hash cache: {hash_cache_path}")

    # Start memory monitor
    monitor = MemoryMonitor(interval=0.25)
    monitor.start()

    start_time: float = time.time()

    # Collect snapshot
    print(f"\nCollecting snapshot from {test_dir}...")
    snapshot = collect_abs_snapshot(
        directories=[test_dir],
        filenames=[],
    )
    print(f"  Files: {len(snapshot.files)}")
    print(f"  Total size: {snapshot.totalSize / (1024 * 1024):.2f} MB")

    # Create S3 data cache
    import boto3
    s3_client = boto3.client("s3")
    data_cache = S3DataCache(
        s3_client=s3_client,
        s3_bucket=bucket,
        s3_key_prefix=prefix,
    )

    # Create hash cache
    hash_cache = HashCache()

    # Progress callback
    def on_progress(metadata) -> bool:
        """Progress callback."""
        return True

    # Run hash+upload
    print(f"\nRunning hash+upload to s3://{bucket}/{prefix}...")
    print(f"  Max memory: {max_memory_mb} MB")
    print(f"  Max workers: {max_workers}")

    max_memory_bytes: int = max_memory_mb * 1024 * 1024

    result = hash_upload_abs_manifest(
        manifest=snapshot,
        data_cache=data_cache,
        hash_cache=hash_cache,
        on_progress=on_progress,
        max_memory_bytes=max_memory_bytes,
        max_workers=max_workers,
    )

    end_time: float = time.time()
    total_time: float = end_time - start_time

    # Stop monitor
    monitor.stop()

    # Get stats
    stats = result.statistics
    mem_stats = monitor.get_stats()

    print(f"\nCompleted in {total_time:.2f}s")
    print(f"  {stats.progressMessage}")
    print(f"\nMemory stats:")
    print(f"  Pool limit: {max_memory_mb} MB")
    print(f"  Max RSS: {mem_stats.get('max_rss_mb', 0):.2f} MB")
    print(f"  Min RSS: {mem_stats.get('min_rss_mb', 0):.2f} MB")
    print(f"  Avg RSS: {mem_stats.get('avg_rss_mb', 0):.2f} MB")

    # Calculate ratio
    max_rss: float = mem_stats.get("max_rss_mb", 0)
    ratio: float = max_rss / max_memory_mb if max_memory_mb > 0 else 0

    print(f"\n  RSS/Pool ratio: {ratio:.2f}x")
    if ratio > 1.5:
        print(f"  ⚠️  RSS exceeded pool limit by {(ratio - 1) * 100:.0f}%!")

    # Write trace
    trace_file: Path = Path("/tmp/python_s3_memory_trace.csv")
    with open(trace_file, "w") as f:
        f.write("timestamp,rss_mb,vms_mb,pool_mb\n")
        for s in monitor.samples:
            f.write(f"{s.timestamp},{s.rss_mb:.2f},{s.vms_mb:.2f},{s.pool_mb:.2f}\n")
    print(f"\nTrace written to {trace_file}")

    return {
        "total_time": total_time,
        "total_bytes": snapshot.totalSize,
        "files": len(snapshot.files),
        "pool_limit_mb": max_memory_mb,
        "max_rss_mb": max_rss,
        "ratio": ratio,
        "throughput_mb_s": snapshot.totalSize / (1024 * 1024) / total_time,
    }


def main() -> None:
    """Main entry point."""
    import argparse

    parser = argparse.ArgumentParser(description="Python S3 upload benchmark")
    parser.add_argument("--test-dir", type=Path, required=True, help="Test directory")
    parser.add_argument("--bucket", type=str, required=True, help="S3 bucket")
    parser.add_argument("--prefix", type=str, required=True, help="S3 prefix")
    parser.add_argument("--max-memory-mb", type=int, default=1024, help="Pool limit MB")
    parser.add_argument("--max-workers", type=int, default=4, help="Max workers")

    args = parser.parse_args()

    if not args.test_dir.exists():
        print(f"Error: Test directory does not exist: {args.test_dir}")
        sys.exit(1)

    print("Python S3 Upload Benchmark with Memory Monitoring")
    print("=" * 60)

    results = run_s3_benchmark(
        test_dir=args.test_dir,
        bucket=args.bucket,
        prefix=args.prefix,
        max_memory_mb=args.max_memory_mb,
        max_workers=args.max_workers,
    )

    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"Total time:     {results['total_time']:.2f}s")
    print(f"Total bytes:    {results['total_bytes'] / (1024 * 1024):.2f} MB")
    print(f"Files:          {results['files']}")
    print(f"Pool limit:     {results['pool_limit_mb']} MB")
    print(f"Max RSS:        {results['max_rss_mb']:.2f} MB")
    print(f"RSS/Pool ratio: {results['ratio']:.2f}x")
    print(f"Throughput:     {results['throughput_mb_s']:.2f} MB/s")


if __name__ == "__main__":
    main()
