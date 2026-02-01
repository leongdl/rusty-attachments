#!/usr/bin/env python3
"""
Deep dive into boto3 memory behavior during put_object calls.

This script instruments boto3 to understand how it handles the Body parameter
and whether it makes internal copies of the data.
"""
import sys
import os
import gc
import tracemalloc
from typing import Any, Dict

# Start memory tracking BEFORE any imports
tracemalloc.start()

sys.path.insert(0, 'context/deadline-cloud/src')

import boto3
from botocore.client import BaseClient

# Patch boto3's put_object to trace memory
_original_make_request = None

def patched_make_request(self, operation_model, request_dict, request_context):
    """Patched _make_request to trace memory during S3 operations."""
    
    # Take snapshot before
    snapshot_before = tracemalloc.take_snapshot()
    mem_before = tracemalloc.get_traced_memory()
    
    # Check if this is a put_object with Body
    body_size = 0
    if 'body' in request_dict and request_dict['body'] is not None:
        body = request_dict['body']
        if isinstance(body, bytes):
            body_size = len(body)
        elif hasattr(body, 'read'):
            # File-like object
            if hasattr(body, 'seek') and hasattr(body, 'tell'):
                current_pos = body.tell()
                body.seek(0, 2)  # Seek to end
                body_size = body.tell()
                body.seek(current_pos)  # Restore position
    
    # Call original
    result = _original_make_request(self, operation_model, request_dict, request_context)
    
    # Take snapshot after
    snapshot_after = tracemalloc.take_snapshot()
    mem_after = tracemalloc.get_traced_memory()
    
    # Calculate difference
    mem_delta = mem_after[0] - mem_before[0]
    
    if body_size > 0:
        print(f"\n[BOTO3 TRACE] {operation_model.name}", file=sys.stderr)
        print(f"  Body size: {body_size / (1024*1024):.2f} MB", file=sys.stderr)
        print(f"  Memory before: {mem_before[0] / (1024*1024):.2f} MB", file=sys.stderr)
        print(f"  Memory after: {mem_after[0] / (1024*1024):.2f} MB", file=sys.stderr)
        print(f"  Memory delta: {mem_delta / (1024*1024):.2f} MB", file=sys.stderr)
        
        if mem_delta > body_size * 0.5:  # More than 50% overhead
            print(f"  ⚠️  MEMORY OVERHEAD: {(mem_delta / body_size - 1) * 100:.1f}% extra!", file=sys.stderr)
            
            # Show top allocations
            top_stats = snapshot_after.compare_to(snapshot_before, 'lineno')
            print(f"  Top 5 allocations:", file=sys.stderr)
            for stat in top_stats[:5]:
                print(f"    {stat}", file=sys.stderr)
    
    return result

def patch_boto3():
    """Patch boto3 to trace memory usage."""
    global _original_make_request
    
    # Patch the BaseClient._make_request method
    from botocore.client import BaseClient
    _original_make_request = BaseClient._make_request
    BaseClient._make_request = patched_make_request
    
    print("✓ Patched boto3 BaseClient._make_request", file=sys.stderr)

def test_simple_upload():
    """Test a simple S3 upload to see memory behavior."""
    import tempfile
    
    print("\n" + "=" * 80, file=sys.stderr)
    print("SIMPLE UPLOAD TEST", file=sys.stderr)
    print("=" * 80, file=sys.stderr)
    
    # Check for S3 credentials
    if 'AWS_PROFILE' not in os.environ and 'AWS_ACCESS_KEY_ID' not in os.environ:
        print("\n⚠️  No AWS credentials found", file=sys.stderr)
        return
    
    s3_bucket = os.environ.get('S3_BUCKET')
    if not s3_bucket:
        print("\n⚠️  S3_BUCKET environment variable not set", file=sys.stderr)
        return
    
    # Create test data
    test_sizes = [1, 10, 50, 100]  # MB
    
    s3_client = boto3.client('s3')
    
    for size_mb in test_sizes:
        print(f"\n--- Testing {size_mb} MB upload ---", file=sys.stderr)
        
        # Create data
        data = os.urandom(size_mb * 1024 * 1024)
        print(f"Created {len(data) / (1024*1024):.2f} MB of test data", file=sys.stderr)
        
        # Force garbage collection
        gc.collect()
        
        # Upload
        key = f"memory-test/test-{size_mb}mb.bin"
        try:
            s3_client.put_object(
                Bucket=s3_bucket,
                Key=key,
                Body=data,
            )
            print(f"✓ Upload complete", file=sys.stderr)
        except Exception as e:
            print(f"✗ Upload failed: {e}", file=sys.stderr)
        
        # Clean up
        del data
        gc.collect()

def main():
    print(f"PID: {os.getpid()}", file=sys.stderr)
    print(f"Python version: {sys.version}", file=sys.stderr)
    print(f"boto3 version: {boto3.__version__}", file=sys.stderr)
    
    # Patch boto3
    patch_boto3()
    
    # Run test
    test_simple_upload()
    
    # Show final memory stats
    print("\n" + "=" * 80, file=sys.stderr)
    print("FINAL MEMORY STATS", file=sys.stderr)
    print("=" * 80, file=sys.stderr)
    
    current, peak = tracemalloc.get_traced_memory()
    print(f"Current memory: {current / (1024*1024):.2f} MB", file=sys.stderr)
    print(f"Peak memory: {peak / (1024*1024):.2f} MB", file=sys.stderr)
    
    tracemalloc.stop()

if __name__ == '__main__':
    main()
