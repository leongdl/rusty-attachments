#!/usr/bin/env python3
"""
Simulate Python hash+upload with deadline-cloud default configuration.

Deadline-cloud defaults:
- max_workers: 10 (per pool, so 10 hash + 10 upload = 20 threads)
- max_memory_bytes: min(16GB, max(256MB, quarter_of_total, available-1GB))

This simulation mimics the exact behavior of the Python pipeline with boto3.
"""

from __future__ import annotations

import os
import sys
import time
import threading
import queue
from dataclasses import dataclass
from io import BytesIO
from typing import List, Optional, Dict
import concurrent.futures

import psutil


# Deadline-cloud defaults
DEFAULT_MAX_WORKERS = 10
MIN_MEMORY_BYTES = 256 * 1024 * 1024  # 256MB
MAX_MEMORY_BYTES = 16 * 1024 * 1024 * 1024  # 16GB


def get_default_max_memory_bytes() -> int:
    """Calculate default memory limit matching deadline-cloud."""
    mem = psutil.virtual_memory()
    quarter_of_total = mem.total // 4
    available_minus_1gb = mem.available - (1024 * 1024 * 1024)
    return min(MAX_MEMORY_BYTES, max(MIN_MEMORY_BYTES, quarter_of_total, available_minus_1gb))


@dataclass
class MemorySample:
    """Memory sample at a point in time."""
    elapsed_ms: int
    pool_allocated_mb: float
    rss_mb: float
    vms_mb: float


class MemoryPool:
    """Thread-safe memory pool matching deadline-cloud implementation."""

    def __init__(self, max_bytes: int) -> None:
        self._max_bytes: int = max_bytes
        self._allocated_bytes: int = 0
        self._lock = threading.Lock()
        self._space_available = threading.Condition(self._lock)

    def allocate(self, size: int) -> None:
        """Allocate memory from pool, blocking if necessary."""
        with self._space_available:
            while self._allocated_bytes + size > self._max_bytes:
                self._space_available.wait()
            self._allocated_bytes += size

    def release(self, size: int) -> None:
        """Release memory back to pool."""
        with self._space_available:
            self._allocated_bytes -= size
            self._space_available.notify_all()

    @property
    def allocated(self) -> int:
        with self._lock:
            return self._allocated_bytes


@dataclass
class WorkItem:
    """Work item for the pipeline."""
    file_id: int
    size: int
    data: Optional[bytes] = None
    hash_value: Optional[str] = None


class Boto3RefHolder:
    """Simulates boto3 holding BytesIO references during HTTP upload."""

    def __init__(self) -> None:
        self._refs: Dict[int, BytesIO] = {}
        self._lock = threading.Lock()

    def hold(self, file_id: int, data: bytes) -> None:
        """Simulate boto3 wrapping data in BytesIO."""
        with self._lock:
            self._refs[file_id] = BytesIO(data)

    def release(self, file_id: int) -> None:
        """Release the reference after upload completes."""
        with self._lock:
            if file_id in self._refs:
                del self._refs[file_id]

    def count(self) -> int:
        with self._lock:
            return len(self._refs)


def run_simulation(
    num_files: int,
    file_size_mb: int,
    pool_limit_mb: int,
    max_workers: int,
    hash_time_ms: int = 5,
    upload_time_ms: int = 50,
) -> Dict:
    """
    Run the simulation with specified configuration.

    Args:
        num_files: Number of files to process
        file_size_mb: Size of each file in MB
        pool_limit_mb: Memory pool limit in MB
        max_workers: Number of workers per pool
        hash_time_ms: Simulated hash time per file
        upload_time_ms: Simulated upload time per file

    Returns:
        Dictionary with results
    """
    file_size: int = file_size_mb * 1024 * 1024
    pool_limit: int = pool_limit_mb * 1024 * 1024

    pool = MemoryPool(pool_limit)
    boto3_refs = Boto3RefHolder()
    samples: List[MemorySample] = []
    process = psutil.Process()

    # Monitoring
    stop_monitor = threading.Event()
    start_time: float = time.time()

    def monitor() -> None:
        while not stop_monitor.is_set():
            mem = process.memory_info()
            elapsed_ms: int = int((time.time() - start_time) * 1000)
            samples.append(MemorySample(
                elapsed_ms=elapsed_ms,
                pool_allocated_mb=pool.allocated / (1024 * 1024),
                rss_mb=mem.rss / (1024 * 1024),
                vms_mb=mem.vms / (1024 * 1024),
            ))
            time.sleep(0.05)

    monitor_thread = threading.Thread(target=monitor, daemon=True)
    monitor_thread.start()

    # Work queues
    upload_queue: queue.Queue = queue.Queue()
    completed_files: List[int] = []
    completed_lock = threading.Lock()

    def hash_worker(item: WorkItem) -> WorkItem:
        """Hash a file (simulated)."""
        pool.allocate(item.size)
        try:
            # Simulate reading file
            item.data = b"x" * item.size
            # Simulate hashing
            time.sleep(hash_time_ms / 1000)
            item.hash_value = f"hash_{item.file_id}"
        except Exception:
            pool.release(item.size)
            raise
        return item

    def upload_worker(item: WorkItem) -> int:
        """Upload a file (simulated with boto3 behavior)."""
        try:
            # Simulate boto3 wrapping in BytesIO (this is what causes memory accumulation)
            boto3_refs.hold(item.file_id, item.data)

            # Simulate HTTP upload (slow)
            time.sleep(upload_time_ms / 1000)

            # Release boto3 reference after upload completes
            boto3_refs.release(item.file_id)
        finally:
            # Release from pool
            pool.release(item.size)
            item.data = None

        with completed_lock:
            completed_files.append(item.file_id)

        return item.file_id

    # Create work items
    work_items: List[WorkItem] = [
        WorkItem(file_id=i, size=file_size) for i in range(num_files)
    ]

    # Run pipeline
    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as hash_executor:
        with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as upload_executor:
            # Submit all hash jobs
            hash_futures = {hash_executor.submit(hash_worker, item): item for item in work_items}

            # As hash jobs complete, submit upload jobs
            upload_futures = []
            for future in concurrent.futures.as_completed(hash_futures):
                item = future.result()
                upload_future = upload_executor.submit(upload_worker, item)
                upload_futures.append(upload_future)

            # Wait for all uploads
            concurrent.futures.wait(upload_futures)

    # Stop monitoring
    stop_monitor.set()
    monitor_thread.join(timeout=1.0)

    # Calculate results
    max_pool: float = max(s.pool_allocated_mb for s in samples)
    max_rss: float = max(s.rss_mb for s in samples)
    min_rss: float = min(s.rss_mb for s in samples)

    violations: List[MemorySample] = [s for s in samples if s.rss_mb > pool_limit_mb]
    first_violation = violations[0] if violations else None

    return {
        "num_files": num_files,
        "file_size_mb": file_size_mb,
        "pool_limit_mb": pool_limit_mb,
        "max_workers": max_workers,
        "max_pool_mb": max_pool,
        "max_rss_mb": max_rss,
        "min_rss_mb": min_rss,
        "rss_pool_ratio": max_rss / pool_limit_mb,
        "violations": len(violations),
        "first_violation_ms": first_violation.elapsed_ms if first_violation else None,
        "samples": samples,
    }


def main() -> None:
    """Run simulation with deadline-cloud defaults."""
    print("=" * 70)
    print("PYTHON HASH+UPLOAD SIMULATION (Deadline-Cloud Default Config)")
    print("=" * 70)

    # Get system defaults
    default_memory: int = get_default_max_memory_bytes()
    default_memory_mb: int = default_memory // (1024 * 1024)

    print(f"\nDeadline-Cloud Defaults:")
    print(f"  max_workers: {DEFAULT_MAX_WORKERS}")
    print(f"  max_memory: {default_memory_mb} MB ({default_memory_mb / 1024:.1f} GB)")

    # For simulation, use a smaller pool to demonstrate the issue
    # (we can't actually allocate 16GB of fake data)
    sim_pool_mb: int = 500
    sim_file_mb: int = 50
    sim_files: int = 20

    print(f"\nSimulation Config (scaled down):")
    print(f"  Files: {sim_files} x {sim_file_mb} MB = {sim_files * sim_file_mb} MB total")
    print(f"  Pool limit: {sim_pool_mb} MB")
    print(f"  Workers: {DEFAULT_MAX_WORKERS} (hash) + {DEFAULT_MAX_WORKERS} (upload)")

    print("\nRunning simulation...")
    results = run_simulation(
        num_files=sim_files,
        file_size_mb=sim_file_mb,
        pool_limit_mb=sim_pool_mb,
        max_workers=DEFAULT_MAX_WORKERS,
    )

    print("\n" + "=" * 70)
    print("RESULTS")
    print("=" * 70)
    print(f"Pool limit:        {results['pool_limit_mb']} MB")
    print(f"Max pool used:     {results['max_pool_mb']:.1f} MB")
    print(f"Max RSS:           {results['max_rss_mb']:.1f} MB")
    print(f"RSS/Pool ratio:    {results['rss_pool_ratio']:.2f}x")
    print(f"Violations:        {results['violations']}")
    if results['first_violation_ms']:
        print(f"First violation:   {results['first_violation_ms']} ms")

    exceeded_by: float = results['max_rss_mb'] - results['pool_limit_mb']
    exceeded_pct: float = (exceeded_by / results['pool_limit_mb']) * 100

    if exceeded_by > 0:
        print(f"\n⚠️  RSS exceeded pool limit by {exceeded_by:.1f} MB ({exceeded_pct:.1f}%)")
    else:
        print(f"\n✓ RSS stayed within pool limit")

    # Save trace
    trace_file = "/tmp/python_default_config_trace.csv"
    with open(trace_file, "w") as f:
        f.write("elapsed_ms,pool_mb,rss_mb\n")
        for s in results['samples']:
            f.write(f"{s.elapsed_ms},{s.pool_allocated_mb:.2f},{s.rss_mb:.2f}\n")
    print(f"\nTrace saved to: {trace_file}")


if __name__ == "__main__":
    main()
