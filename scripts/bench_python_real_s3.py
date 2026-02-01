#!/usr/bin/env python3
"""
Real Python S3 upload benchmark using deadline-cloud library.

This runs the actual deadline-cloud hash_upload pipeline against real S3,
matching the Rust benchmark configuration exactly.
"""

from __future__ import annotations

import os
import sys
import time
import threading
from pathlib import Path
from typing import List, Optional

import psutil

os.environ["ENABLE_SNAPSHOTS_LIBRARY"] = "1"


class MemoryMonitor:
    """Background memory monitor."""

    def __init__(self, interval: float = 0.1) -> None:
        self.interval: float = interval
        self.samples: List[float] = []
        self._stop: bool = False
        self._thread: Optional[threading.Thread] = None

    def start(self) -> None:
        self._stop = False
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop = True
        if self._thread:
            self._thread.join(timeout=2.0)

    def _loop(self) -> None:
        process = psutil.Process()
        while not self._stop:
            mem = process.memory_info()
            self.samples.append(mem.rss / (1024 * 1024))
            time.sleep(self.interval)

    def max_rss(self) -> float:
        return max(self.samples) if self.samples else 0.0

    def min_rss(self) -> float:
        return min(self.samples) if self.samples else 0.0


def run_benchmark(
    test_dir: Path,
    bucket: str,
    prefix: str,
    max_memory_gb: float,
    max_workers: int,
) -> dict:
    """Run the benchmark using installed deadline-cloud library."""
    from deadline.job_attachments._snapshots._operations import (
        collect_abs_snapshot,
        hash_upload_abs_manifest,
    )
    from deadline.job_attachments._snapshots._content_addressed_data_cache import S3DataCache

    # Clear caches
    cache_dir: Path = Path.home() / ".deadline" / "cache"
    hash_cache_path: Path = cache_dir / "hash_cache.db"
    if hash_cache_path.exists():
        hash_cache_path.unlink()
        print(f"Cleared hash cache: {hash_cache_path}")

    # Collect snapshot using the library's API
    print(f"\nCollecting snapshot from {test_dir}...")
    snapshot = collect_abs_snapshot(
        directories=[test_dir],
        filenames=[],
    )
    file_count: int = len(snapshot.files)
    total_bytes: int = snapshot.totalSize
    print(f"Found {file_count} files ({total_bytes / (1024 * 1024):.2f} MB)")

    # Create S3 data cache
    print(f"\nCreating S3 data cache: s3://{bucket}/{prefix}/")
    import boto3
    s3_client = boto3.client("s3")
    data_cache = S3DataCache(
        s3_bucket=bucket,
        s3_key_prefix=prefix,
        s3_client=s3_client,
    )

    # Start memory monitor
    monitor = MemoryMonitor(interval=0.1)
    monitor.start()

    # Run benchmark
    max_memory_bytes: int = int(max_memory_gb * 1024 * 1024 * 1024)
    print(f"\nRunning hash+upload...")
    print(f"  Max memory: {max_memory_gb} GB ({max_memory_bytes // (1024*1024)} MB)")
    print(f"  Max workers: {max_workers}")

    start_time: float = time.perf_counter()

    result = hash_upload_abs_manifest(
        manifest=snapshot,
        data_cache=data_cache,
        hash_cache=None,
        force_rehash=True,
        max_memory_bytes=max_memory_bytes,
        max_workers=max_workers,
    )

    total_time: float = time.perf_counter() - start_time

    # Stop monitor
    monitor.stop()

    # Get stats
    stats = result.statistics
    max_rss: float = monitor.max_rss()
    min_rss: float = monitor.min_rss()
    throughput: float = total_bytes / total_time / (1024 * 1024)

    print(f"\nCompleted:")
    print(f"  Hashed: {stats.hashed_file_chunks} chunks ({stats.hashed_bytes / (1024 * 1024):.2f} MB)")
    print(f"  Uploaded: {stats.uploaded_file_chunks} chunks ({stats.uploaded_bytes / (1024 * 1024):.2f} MB)")

    return {
        "total_time": total_time,
        "total_bytes": total_bytes,
        "file_count": file_count,
        "max_memory_gb": max_memory_gb,
        "max_workers": max_workers,
        "max_rss_mb": max_rss,
        "min_rss_mb": min_rss,
        "throughput_mb_s": throughput,
        "hashed_bytes": stats.hashed_bytes,
        "uploaded_bytes": stats.uploaded_bytes,
    }


def main() -> None:
    import argparse

    parser = argparse.ArgumentParser(description="Python S3 benchmark")
    parser.add_argument("--test-dir", type=Path, required=True)
    parser.add_argument("--bucket", type=str, required=True)
    parser.add_argument("--prefix", type=str, required=True)
    parser.add_argument("--max-memory-gb", type=float, default=1.0)
    parser.add_argument("--max-workers", type=int, default=10)

    args = parser.parse_args()

    if not args.test_dir.exists():
        print(f"Error: Test directory does not exist: {args.test_dir}")
        sys.exit(1)

    print("=" * 70)
    print("PYTHON S3 UPLOAD BENCHMARK (Real deadline-cloud)")
    print("=" * 70)
    print(f"Test dir:    {args.test_dir}")
    print(f"Bucket:      {args.bucket}")
    print(f"Prefix:      {args.prefix}")
    print(f"Max memory:  {args.max_memory_gb} GB")
    print(f"Max workers: {args.max_workers}")

    results = run_benchmark(
        test_dir=args.test_dir,
        bucket=args.bucket,
        prefix=args.prefix,
        max_memory_gb=args.max_memory_gb,
        max_workers=args.max_workers,
    )

    print("\n" + "=" * 70)
    print("RESULTS")
    print("=" * 70)
    print(f"Total time:      {results['total_time']:.2f}s")
    print(f"Total bytes:     {results['total_bytes'] / (1024 * 1024):.2f} MB")
    print(f"Files:           {results['file_count']}")
    print(f"Pool limit:      {results['max_memory_gb'] * 1024:.0f} MB")
    print(f"Peak RSS:        {results['max_rss_mb']:.2f} MB")
    print(f"Throughput:      {results['throughput_mb_s']:.2f} MB/s")

    pool_limit_mb: float = results['max_memory_gb'] * 1024
    ratio: float = results['max_rss_mb'] / pool_limit_mb
    print(f"RSS/Pool ratio:  {ratio:.2f}x")

    if results['max_rss_mb'] > pool_limit_mb:
        exceeded: float = results['max_rss_mb'] - pool_limit_mb
        pct: float = (exceeded / pool_limit_mb) * 100
        print(f"\n⚠️  RSS exceeded pool limit by {exceeded:.0f} MB ({pct:.1f}%)")


if __name__ == "__main__":
    main()
