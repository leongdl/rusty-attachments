#!/usr/bin/env python3
"""
Instrumented test to trace memory pool usage and prove it exceeds limits.

This script patches the _MemoryPool class to log all allocations/releases
and compares against actual process memory usage.
"""
import os
import sys
import time
import threading
import psutil
from typing import Optional

# Add deadline-cloud to path
sys.path.insert(0, 'context/deadline-cloud/src')

# Import and patch BEFORE any other imports
from deadline.job_attachments._snapshots._operations import _hash_upload_abs_manifest_pipeline

# Store original methods
_original_allocate = _hash_upload_abs_manifest_pipeline._MemoryPool.allocate
_original_release = _hash_upload_abs_manifest_pipeline._MemoryPool.release

# Global tracking
allocation_log = []
allocation_lock = threading.Lock()
process = psutil.Process(os.getpid())
start_time = time.time()

def patched_allocate(self, size: int) -> None:
    """Patched allocate that logs all allocations."""
    mem_before = process.memory_info().rss
    allocated_before = self._allocated_bytes
    
    # Call original
    _original_allocate(self, size)
    
    mem_after = process.memory_info().rss
    allocated_after = self._allocated_bytes
    
    with allocation_lock:
        allocation_log.append({
            'timestamp': time.time() - start_time,
            'action': 'allocate',
            'size': size,
            'pool_before': allocated_before,
            'pool_after': allocated_after,
            'rss_before_mb': mem_before / (1024 * 1024),
            'rss_after_mb': mem_after / (1024 * 1024),
            'rss_delta_mb': (mem_after - mem_before) / (1024 * 1024),
            'thread': threading.current_thread().name,
        })

def patched_release(self, size: int) -> None:
    """Patched release that logs all releases."""
    mem_before = process.memory_info().rss
    allocated_before = self._allocated_bytes
    
    # Call original
    _original_release(self, size)
    
    mem_after = process.memory_info().rss
    allocated_after = self._allocated_bytes
    
    with allocation_lock:
        allocation_log.append({
            'timestamp': time.time() - start_time,
            'action': 'release',
            'size': size,
            'pool_before': allocated_before,
            'pool_after': allocated_after,
            'rss_before_mb': mem_before / (1024 * 1024),
            'rss_after_mb': mem_after / (1024 * 1024),
            'rss_delta_mb': (mem_after - mem_before) / (1024 * 1024),
            'thread': threading.current_thread().name,
        })

# Apply patches
_hash_upload_abs_manifest_pipeline._MemoryPool.allocate = patched_allocate
_hash_upload_abs_manifest_pipeline._MemoryPool.release = patched_release

print(f"✓ Patched _MemoryPool to trace allocations/releases", file=sys.stderr)

# Now import the rest
import tempfile
import shutil
import json
from deadline.job_attachments._snapshots._operations._hash_upload_abs_manifest import hash_upload_abs_manifest
from deadline.job_attachments._snapshots._abs_manifest import AbsManifest, AbsFile
from deadline.job_attachments._snapshots._cas_data_cache import S3DataCache
import boto3

def create_test_files(base_dir: str, num_files: int = 50, file_size_mb: int = 20) -> list:
    """Create test files for upload."""
    print(f"Creating {num_files} files of {file_size_mb}MB each", file=sys.stderr)
    files = []
    
    for i in range(num_files):
        file_path = os.path.join(base_dir, f"testfile_{i:04d}.bin")
        
        # Create file with random data
        with open(file_path, 'wb') as f:
            chunk_size = 1024 * 1024  # 1MB chunks
            for _ in range(file_size_mb):
                f.write(os.urandom(chunk_size))
        
        files.append(file_path)
        if (i + 1) % 10 == 0:
            print(f"  Created {i + 1}/{num_files} files", file=sys.stderr)
    
    return files

def analyze_log(pool_limit_mb: int):
    """Analyze the allocation log to find memory violations."""
    print("\n" + "=" * 80, file=sys.stderr)
    print("MEMORY POOL ANALYSIS", file=sys.stderr)
    print("=" * 80, file=sys.stderr)
    
    pool_limit_bytes = pool_limit_mb * 1024 * 1024
    max_pool_allocated = 0
    max_rss = 0
    violations = []
    
    for entry in allocation_log:
        pool_allocated = entry['pool_after']
        rss_mb = entry['rss_after_mb']
        
        if pool_allocated > max_pool_allocated:
            max_pool_allocated = pool_allocated
        
        if rss_mb > max_rss:
            max_rss = rss_mb
        
        # Check if RSS exceeds pool limit
        if rss_mb > pool_limit_mb:
            violations.append(entry)
    
    print(f"Pool limit: {pool_limit_mb} MB ({pool_limit_bytes:,} bytes)", file=sys.stderr)
    print(f"Max pool allocated: {max_pool_allocated / (1024 * 1024):.2f} MB ({max_pool_allocated:,} bytes)", file=sys.stderr)
    print(f"Max RSS: {max_rss:.2f} MB", file=sys.stderr)
    print(f"Total allocations: {sum(1 for e in allocation_log if e['action'] == 'allocate')}", file=sys.stderr)
    print(f"Total releases: {sum(1 for e in allocation_log if e['action'] == 'release')}", file=sys.stderr)
    
    if max_pool_allocated > pool_limit_bytes:
        print(f"\n⚠️  POOL LIMIT EXCEEDED BY POOL ITSELF!", file=sys.stderr)
        print(f"   Exceeded by: {(max_pool_allocated - pool_limit_bytes) / (1024 * 1024):.2f} MB", file=sys.stderr)
    
    if violations:
        print(f"\n⚠️  RSS EXCEEDED POOL LIMIT {len(violations)} times!", file=sys.stderr)
        print(f"   First violation at {violations[0]['timestamp']:.2f}s: {violations[0]['rss_after_mb']:.2f} MB", file=sys.stderr)
        print(f"   Peak violation: {max_rss:.2f} MB (exceeded by {max_rss - pool_limit_mb:.2f} MB)", file=sys.stderr)
        print(f"   Percentage over limit: {(max_rss / pool_limit_mb - 1) * 100:.1f}%", file=sys.stderr)
    else:
        print(f"\n✓ RSS stayed within pool limit", file=sys.stderr)
    
    # Save detailed log
    log_file = "/tmp/memory_pool_trace.jsonl"
    with open(log_file, 'w') as f:
        for entry in allocation_log:
            f.write(json.dumps(entry) + '\n')
    print(f"\nDetailed log saved to: {log_file}", file=sys.stderr)
    
    return violations

def main():
    print(f"PID: {os.getpid()}", file=sys.stderr)
    print("=" * 80, file=sys.stderr)
    
    # Configuration
    NUM_FILES = 50
    FILE_SIZE_MB = 20  # 20MB files
    MEMORY_POOL_MB = 1024  # 1GB limit
    MAX_WORKERS = 4
    
    print(f"Configuration:", file=sys.stderr)
    print(f"  Files: {NUM_FILES}", file=sys.stderr)
    print(f"  File size: {FILE_SIZE_MB} MB", file=sys.stderr)
    print(f"  Total data: {NUM_FILES * FILE_SIZE_MB} MB", file=sys.stderr)
    print(f"  Memory pool limit: {MEMORY_POOL_MB} MB", file=sys.stderr)
    print(f"  Max workers: {MAX_WORKERS}", file=sys.stderr)
    print("=" * 80, file=sys.stderr)
    
    # Check for S3 credentials
    if 'AWS_PROFILE' not in os.environ and 'AWS_ACCESS_KEY_ID' not in os.environ:
        print("\n⚠️  No AWS credentials found. Set AWS_PROFILE or AWS_ACCESS_KEY_ID", file=sys.stderr)
        print("   This test requires S3 access to run.", file=sys.stderr)
        return 1
    
    # Get S3 bucket from environment
    s3_bucket = os.environ.get('S3_BUCKET')
    if not s3_bucket:
        print("\n⚠️  S3_BUCKET environment variable not set", file=sys.stderr)
        print("   Usage: S3_BUCKET=my-bucket python trace_memory_pool.py", file=sys.stderr)
        return 1
    
    # Create temporary directory for test files
    test_dir = tempfile.mkdtemp(prefix="python_upload_test_")
    print(f"Test directory: {test_dir}", file=sys.stderr)
    
    try:
        # Create test files
        files = create_test_files(test_dir, NUM_FILES, FILE_SIZE_MB)
        print(f"✓ Created {len(files)} test files", file=sys.stderr)
        print("=" * 80, file=sys.stderr)
        
        # Create manifest
        abs_files = [
            AbsFile(path=f, hash=None, size=os.path.getsize(f), mtime=os.path.getmtime(f))
            for f in files
        ]
        manifest = AbsManifest(files=abs_files, root=test_dir)
        print(f"✓ Created manifest with {len(manifest.files)} files", file=sys.stderr)
        
        # Create S3 cache
        s3_client = boto3.client('s3')
        data_cache = S3DataCache(
            s3_client=s3_client,
            s3_bucket=s3_bucket,
            s3_prefix="memory-pool-test/",
        )
        print(f"✓ Created S3 cache (bucket: {s3_bucket})", file=sys.stderr)
        
        print("=" * 80, file=sys.stderr)
        print("Starting hash+upload operation...", file=sys.stderr)
        print(f"MEMORY POOL LIMIT: {MEMORY_POOL_MB} MB", file=sys.stderr)
        print("=" * 80, file=sys.stderr)
        
        # Run the upload
        result = hash_upload_abs_manifest(
            manifest=manifest,
            data_cache=data_cache,
            max_memory_bytes=MEMORY_POOL_MB * 1024 * 1024,
            max_workers=MAX_WORKERS,
        )
        
        print(f"\n✓ Upload complete!", file=sys.stderr)
        print(f"  Files processed: {result.total_files}", file=sys.stderr)
        print(f"  Bytes uploaded: {result.total_bytes:,}", file=sys.stderr)
        
        # Analyze the log
        violations = analyze_log(MEMORY_POOL_MB)
        
        if violations:
            print("\n" + "=" * 80, file=sys.stderr)
            print("PROOF: Memory pool limit was exceeded!", file=sys.stderr)
            print("=" * 80, file=sys.stderr)
            return 1
        else:
            print("\n" + "=" * 80, file=sys.stderr)
            print("Memory pool limit was respected", file=sys.stderr)
            print("=" * 80, file=sys.stderr)
            return 0
        
    finally:
        # Cleanup
        print(f"\nCleaning up test directory: {test_dir}", file=sys.stderr)
        shutil.rmtree(test_dir, ignore_errors=True)

if __name__ == '__main__':
    sys.exit(main())
