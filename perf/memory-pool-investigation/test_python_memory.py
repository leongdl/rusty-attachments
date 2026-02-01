#!/usr/bin/env python3
"""
Test script to measure Python hash+upload memory usage with explicit pool limit.
"""
import os
import sys
import tempfile
import shutil
from pathlib import Path

# Add deadline-cloud to path
sys.path.insert(0, 'context/deadline-cloud/src')

from deadline.job_attachments._snapshots._operations._hash_upload_abs_manifest import hash_upload_abs_manifest
from deadline.job_attachments._snapshots._abs_manifest import AbsManifest, AbsFile
from deadline.job_attachments._snapshots._cas_data_cache import ContentAddressedDataCache

def create_test_files(base_dir, num_files=100, file_size_mb=10):
    """Create test files for upload."""
    print(f"Creating {num_files} files of {file_size_mb}MB each in {base_dir}")
    files = []
    
    for i in range(num_files):
        file_path = os.path.join(base_dir, f"testfile_{i:04d}.bin")
        
        # Create file with random data
        with open(file_path, 'wb') as f:
            # Write in chunks to avoid memory spike
            chunk_size = 1024 * 1024  # 1MB chunks
            for _ in range(file_size_mb):
                f.write(os.urandom(chunk_size))
        
        files.append(file_path)
        if (i + 1) % 10 == 0:
            print(f"  Created {i + 1}/{num_files} files")
    
    return files

def main():
    print(f"PID: {os.getpid()}")
    print("=" * 80)
    
    # Configuration
    NUM_FILES = 100
    FILE_SIZE_MB = 10
    MEMORY_POOL_MB = 1024  # 1GB limit
    MAX_WORKERS = 4
    
    print(f"Configuration:")
    print(f"  Files: {NUM_FILES}")
    print(f"  File size: {FILE_SIZE_MB} MB")
    print(f"  Total data: {NUM_FILES * FILE_SIZE_MB} MB")
    print(f"  Memory pool limit: {MEMORY_POOL_MB} MB")
    print(f"  Max workers: {MAX_WORKERS}")
    print("=" * 80)
    
    # Create temporary directory for test files
    test_dir = tempfile.mkdtemp(prefix="python_upload_test_")
    print(f"Test directory: {test_dir}")
    
    try:
        # Create test files
        files = create_test_files(test_dir, NUM_FILES, FILE_SIZE_MB)
        print(f"Created {len(files)} test files")
        print("=" * 80)
        
        # Create manifest
        abs_files = [
            AbsFile(path=f, hash=None, size=os.path.getsize(f), mtime=os.path.getmtime(f))
            for f in files
        ]
        manifest = AbsManifest(files=abs_files, root=test_dir)
        print(f"Created manifest with {len(manifest.files)} files")
        
        # Create S3 cache (mock for now - we'll use local cache)
        cache_dir = tempfile.mkdtemp(prefix="python_cache_")
        print(f"Cache directory: {cache_dir}")
        
        # Note: We need actual S3 credentials for real test
        # For now, this will fail but we can observe memory behavior
        
        print("=" * 80)
        print("Starting hash+upload operation...")
        print(f"MEMORY POOL LIMIT: {MEMORY_POOL_MB} MB")
        print("Watch memory usage - it should NOT exceed the pool limit!")
        print("=" * 80)
        
        # This is where we'd call hash_upload_abs_manifest
        # For now, let's trace through the code to understand memory pool
        
        print("\nNOTE: This test requires S3 credentials to run fully.")
        print("However, we can trace through the code to understand memory pool behavior.")
        
    finally:
        # Cleanup
        print(f"\nCleaning up test directory: {test_dir}")
        shutil.rmtree(test_dir, ignore_errors=True)

if __name__ == '__main__':
    main()
