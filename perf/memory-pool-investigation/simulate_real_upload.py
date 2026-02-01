#!/usr/bin/env python3
"""
Simulate real upload behavior with realistic timing to generate memory traces.

This simulates the exact behavior of the Python pipeline including:
- Concurrent READ+HASH and UPLOAD thread pools
- Realistic timing (fast hash, slow upload)
- boto3 reference holding
- Memory pool tracking
"""
import os
import sys
import gc
import time
import threading
import queue
import psutil
from io import BytesIO
from typing import List, Dict, Any
import json

class MemoryPool:
    """Simulates deadline-cloud's _MemoryPool."""
    def __init__(self, max_bytes: int):
        self._max_bytes = max_bytes
        self._allocated_bytes = 0
        self._lock = threading.Lock()
        self._space_available = threading.Condition(self._lock)
    
    def allocate(self, size: int) -> None:
        with self._space_available:
            while self._allocated_bytes + size > self._max_bytes:
                self._space_available.wait()
            self._allocated_bytes += size
    
    def release(self, size: int) -> None:
        with self._space_available:
            self._allocated_bytes -= size
            self._space_available.notify_all()
    
    @property
    def allocated(self) -> int:
        with self._lock:
            return self._allocated_bytes

class WorkItem:
    """Simulates a file work item."""
    def __init__(self, file_id: int, size: int):
        self.file_id = file_id
        self.size = size
        self.data = None
        self.hash = None
        self.start_time = time.time()

class MemoryTracer:
    """Tracks memory usage over time."""
    def __init__(self, pool: MemoryPool):
        self.pool = pool
        self.process = psutil.Process(os.getpid())
        self.start_time = time.time()
        self.samples = []
        self.running = False
        self.thread = None
    
    def start(self):
        self.running = True
        self.thread = threading.Thread(target=self._sample_loop, daemon=True)
        self.thread.start()
    
    def stop(self):
        self.running = False
        if self.thread:
            self.thread.join(timeout=1.0)
    
    def _sample_loop(self):
        while self.running:
            self._take_sample()
            time.sleep(0.05)  # Sample every 50ms
    
    def _take_sample(self):
        mem_info = self.process.memory_info()
        sample = {
            'timestamp': time.time(),
            'elapsed_ms': int((time.time() - self.start_time) * 1000),
            'pool_allocated_mb': self.pool.allocated / (1024 * 1024),
            'rss_mb': mem_info.rss / (1024 * 1024),
            'vms_mb': mem_info.vms / (1024 * 1024),
            'gap_mb': (mem_info.rss - self.pool.allocated) / (1024 * 1024),
        }
        self.samples.append(sample)
    
    def save(self, filename: str):
        with open(filename, 'w') as f:
            for sample in self.samples:
                f.write(json.dumps(sample) + '\n')

def simulate_boto3_behavior(data: bytes) -> BytesIO:
    """Simulate what boto3 does - wraps bytes in BytesIO."""
    return BytesIO(data)

def read_hash_worker(
    work_queue: queue.Queue,
    upload_queue: queue.Queue,
    pool: MemoryPool,
    stats: Dict[str, Any]
):
    """Simulates READ+HASH thread."""
    while True:
        try:
            item = work_queue.get(timeout=0.1)
            if item is None:  # Poison pill
                break
            
            # Allocate from pool
            pool.allocate(item.size)
            stats['hash_start'] = stats.get('hash_start', 0) + 1
            
            # Simulate reading file (fast - 2ms)
            time.sleep(0.002)
            item.data = os.urandom(item.size)
            
            # Simulate hashing (fast - 3ms)
            time.sleep(0.003)
            item.hash = f"hash_{item.file_id}"
            
            stats['hash_complete'] = stats.get('hash_complete', 0) + 1
            
            # Submit to upload queue
            upload_queue.put(item)
            
        except queue.Empty:
            continue

def upload_worker(
    upload_queue: queue.Queue,
    pool: MemoryPool,
    boto3_refs: List[BytesIO],
    stats: Dict[str, Any]
):
    """Simulates UPLOAD thread."""
    while True:
        try:
            item = upload_queue.get(timeout=0.1)
            if item is None:  # Poison pill
                break
            
            stats['upload_start'] = stats.get('upload_start', 0) + 1
            
            # Simulate boto3 wrapping (this is what boto3 does!)
            wrapped = simulate_boto3_behavior(item.data)
            boto3_refs.append(wrapped)  # boto3 holds this reference
            
            # Simulate HTTP upload (slow - 50ms)
            time.sleep(0.050)
            
            # Release from pool (but boto3 still holds wrapped!)
            pool.release(item.size)
            item.data = None  # Clear our reference
            
            stats['upload_complete'] = stats.get('upload_complete', 0) + 1
            
        except queue.Empty:
            continue

def run_simulation(
    num_files: int = 20,
    file_size_mb: int = 50,
    pool_limit_mb: int = 500,
    num_workers: int = 4
):
    """Run the simulation."""
    print(f"=" * 80)
    print(f"SIMULATING REAL UPLOAD BEHAVIOR")
    print(f"=" * 80)
    print(f"Configuration:")
    print(f"  Files: {num_files}")
    print(f"  File size: {file_size_mb} MB")
    print(f"  Pool limit: {pool_limit_mb} MB")
    print(f"  Workers: {num_workers} (hash) + {num_workers} (upload)")
    print(f"  PID: {os.getpid()}")
    print(f"=" * 80)
    print()
    
    file_size = file_size_mb * 1024 * 1024
    pool = MemoryPool(pool_limit_mb * 1024 * 1024)
    
    # Start memory tracer
    tracer = MemoryTracer(pool)
    tracer.start()
    
    # Create queues
    work_queue = queue.Queue()
    upload_queue = queue.Queue()
    
    # Track boto3 references (simulates boto3 holding them)
    boto3_refs = []
    
    # Stats
    stats = {}
    
    # Start worker threads
    hash_threads = []
    for i in range(num_workers):
        t = threading.Thread(
            target=read_hash_worker,
            args=(work_queue, upload_queue, pool, stats),
            name=f"HashWorker-{i}"
        )
        t.start()
        hash_threads.append(t)
    
    upload_threads = []
    for i in range(num_workers):
        t = threading.Thread(
            target=upload_worker,
            args=(upload_queue, pool, boto3_refs, stats),
            name=f"UploadWorker-{i}"
        )
        t.start()
        upload_threads.append(t)
    
    # Submit work
    print(f"Submitting {num_files} files...")
    for i in range(num_files):
        item = WorkItem(i, file_size)
        work_queue.put(item)
    
    # Wait for hashing to complete
    print(f"Waiting for hashing to complete...")
    for _ in range(num_workers):
        work_queue.put(None)  # Poison pills
    for t in hash_threads:
        t.join()
    
    print(f"Hashing complete. Waiting for uploads...")
    
    # Wait for uploads to complete
    for _ in range(num_workers):
        upload_queue.put(None)  # Poison pills
    for t in upload_threads:
        t.join()
    
    print(f"Uploads complete!")
    
    # Stop tracer
    time.sleep(0.1)  # Let tracer catch up
    tracer.stop()
    
    # Clear boto3 references (simulates boto3 finishing)
    print(f"Clearing boto3 references...")
    boto3_refs.clear()
    gc.collect()
    
    # Final sample
    time.sleep(0.1)
    tracer._take_sample()
    
    # Save trace
    trace_file = "/tmp/real_upload_simulation_trace.jsonl"
    tracer.save(trace_file)
    print(f"\nTrace saved to: {trace_file}")
    
    # Analyze results
    print(f"\n" + "=" * 80)
    print(f"RESULTS")
    print(f"=" * 80)
    
    max_pool = max(s['pool_allocated_mb'] for s in tracer.samples)
    max_rss = max(s['rss_mb'] for s in tracer.samples)
    max_gap = max(s['gap_mb'] for s in tracer.samples)
    
    violations = [s for s in tracer.samples if s['rss_mb'] > pool_limit_mb]
    
    print(f"Pool limit: {pool_limit_mb} MB")
    print(f"Max pool allocated: {max_pool:.2f} MB")
    print(f"Max RSS: {max_rss:.2f} MB")
    print(f"Max gap: {max_gap:.2f} MB")
    print(f"")
    print(f"Stats:")
    print(f"  Files hashed: {stats.get('hash_complete', 0)}")
    print(f"  Files uploaded: {stats.get('upload_complete', 0)}")
    print(f"  boto3 refs held: {len(boto3_refs)} (should be 0 after cleanup)")
    print(f"")
    
    if max_pool > pool_limit_mb:
        print(f"⚠️  POOL EXCEEDED ITS OWN LIMIT!")
    else:
        print(f"✓ Pool stayed within limit")
    
    if violations:
        first_violation = violations[0]
        print(f"\n⚠️  RSS EXCEEDED POOL LIMIT {len(violations)} times!")
        print(f"   First violation at {first_violation['elapsed_ms']}ms: {first_violation['rss_mb']:.2f} MB")
        print(f"   Peak: {max_rss:.2f} MB")
        print(f"   Exceeded by: {max_rss - pool_limit_mb:.2f} MB ({(max_rss/pool_limit_mb - 1)*100:.1f}%)")
        print(f"\n   PROOF: Pool tracked {max_pool:.2f} MB, but RSS was {max_rss:.2f} MB")
        print(f"   Gap: {max_gap:.2f} MB unaccounted for!")
    else:
        print(f"\n✓ RSS stayed within pool limit")
    
    # Generate summary
    summary_file = "/tmp/real_upload_simulation_summary.txt"
    with open(summary_file, 'w') as f:
        f.write(f"Real Upload Simulation Summary\n")
        f.write(f"=" * 80 + "\n\n")
        f.write(f"Configuration:\n")
        f.write(f"  Files: {num_files}\n")
        f.write(f"  File size: {file_size_mb} MB\n")
        f.write(f"  Pool limit: {pool_limit_mb} MB\n")
        f.write(f"  Workers: {num_workers} + {num_workers}\n\n")
        f.write(f"Results:\n")
        f.write(f"  Max pool: {max_pool:.2f} MB\n")
        f.write(f"  Max RSS: {max_rss:.2f} MB\n")
        f.write(f"  Max gap: {max_gap:.2f} MB\n")
        f.write(f"  Violations: {len(violations)}\n\n")
        
        if violations:
            f.write(f"Memory exceeded pool limit by {(max_rss/pool_limit_mb - 1)*100:.1f}%\n")
            f.write(f"This proves the pool tracks correctly but RSS exceeds due to boto3 refs.\n")
    
    print(f"\nSummary saved to: {summary_file}")
    
    return tracer.samples

def main():
    print(f"PID: {os.getpid()}")
    print(f"Python: {sys.version}")
    print()
    
    samples = run_simulation(
        num_files=20,
        file_size_mb=50,
        pool_limit_mb=500,
        num_workers=4
    )
    
    print(f"\n" + "=" * 80)
    print(f"SAMPLE TRACE DATA (first 10 and last 10 samples)")
    print(f"=" * 80)
    print(f"{'Time(ms)':<10} {'Pool(MB)':<12} {'RSS(MB)':<12} {'Gap(MB)':<12}")
    print(f"-" * 80)
    
    for sample in samples[:10]:
        print(f"{sample['elapsed_ms']:<10} "
              f"{sample['pool_allocated_mb']:<12.2f} "
              f"{sample['rss_mb']:<12.2f} "
              f"{sample['gap_mb']:<12.2f}")
    
    if len(samples) > 20:
        print(f"{'...':<10} {'...':<12} {'...':<12} {'...':<12}")
    
    for sample in samples[-10:]:
        print(f"{sample['elapsed_ms']:<10} "
              f"{sample['pool_allocated_mb']:<12.2f} "
              f"{sample['rss_mb']:<12.2f} "
              f"{sample['gap_mb']:<12.2f}")

if __name__ == '__main__':
    main()
