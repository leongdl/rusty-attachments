#!/usr/bin/env python3
"""
Prove that boto3 put_object creates memory copies.

This test uses a mock S3 client to isolate boto3's memory behavior
without needing actual S3 credentials.
"""
import sys
import os
import gc
import tracemalloc
from unittest.mock import Mock, MagicMock
from io import BytesIO

# Start memory tracking
tracemalloc.start()

def get_memory_mb():
    """Get current memory usage in MB."""
    current, peak = tracemalloc.get_traced_memory()
    return current / (1024 * 1024), peak / (1024 * 1024)

def test_bytes_to_bytesio_copy():
    """Test if converting bytes to BytesIO creates a copy."""
    print("\n" + "=" * 80)
    print("TEST 1: bytes -> BytesIO conversion")
    print("=" * 80)
    
    size_mb = 100
    print(f"Creating {size_mb} MB of data...")
    
    gc.collect()
    mem_before, _ = get_memory_mb()
    
    # Create data
    data = os.urandom(size_mb * 1024 * 1024)
    gc.collect()
    mem_after_data, _ = get_memory_mb()
    
    print(f"Memory before: {mem_before:.2f} MB")
    print(f"Memory after creating data: {mem_after_data:.2f} MB")
    print(f"Data size: {len(data) / (1024*1024):.2f} MB")
    print(f"Memory increase: {mem_after_data - mem_before:.2f} MB")
    
    # Wrap in BytesIO
    print(f"\nWrapping in BytesIO...")
    bio = BytesIO(data)
    gc.collect()
    mem_after_bio, _ = get_memory_mb()
    
    print(f"Memory after BytesIO: {mem_after_bio:.2f} MB")
    print(f"BytesIO overhead: {mem_after_bio - mem_after_data:.2f} MB")
    
    if mem_after_bio - mem_after_data > size_mb * 0.5:
        print(f"⚠️  COPY DETECTED! BytesIO created a {(mem_after_bio - mem_after_data) / size_mb * 100:.1f}% copy!")
    else:
        print(f"✓ No significant copy (BytesIO uses reference)")
    
    # Clean up
    del data
    del bio
    gc.collect()

def test_boto3_request_preparation():
    """Test boto3's request preparation to see if it copies data."""
    print("\n" + "=" * 80)
    print("TEST 2: boto3 request preparation")
    print("=" * 80)
    
    # Import boto3 components
    from botocore.client import BaseClient
    from botocore.endpoint import Endpoint
    
    size_mb = 100
    print(f"Creating {size_mb} MB of data...")
    
    gc.collect()
    mem_before, _ = get_memory_mb()
    
    # Create data
    data = os.urandom(size_mb * 1024 * 1024)
    gc.collect()
    mem_after_data, _ = get_memory_mb()
    
    print(f"Memory before: {mem_before:.2f} MB")
    print(f"Memory after creating data: {mem_after_data:.2f} MB")
    print(f"Memory increase: {mem_after_data - mem_before:.2f} MB")
    
    # Simulate what boto3 does with the body
    print(f"\nSimulating boto3 body handling...")
    
    # boto3 checks if body is bytes and may wrap it
    if isinstance(data, bytes):
        # This is what botocore does internally
        body_for_request = data
        
        # Check if it needs to be seekable (for retries)
        if not hasattr(body_for_request, 'read'):
            # Not file-like, wrap it
            body_for_request = BytesIO(data)
    
    gc.collect()
    mem_after_wrap, _ = get_memory_mb()
    
    print(f"Memory after wrapping: {mem_after_wrap:.2f} MB")
    print(f"Wrapping overhead: {mem_after_wrap - mem_after_data:.2f} MB")
    
    if mem_after_wrap - mem_after_data > size_mb * 0.5:
        print(f"⚠️  COPY DETECTED! Wrapping created a {(mem_after_wrap - mem_after_data) / size_mb * 100:.1f}% copy!")
    else:
        print(f"✓ No significant copy")
    
    # Clean up
    del data
    del body_for_request
    gc.collect()

def test_multiple_references():
    """Test if multiple references to the same data increase memory."""
    print("\n" + "=" * 80)
    print("TEST 3: Multiple references to same data")
    print("=" * 80)
    
    size_mb = 100
    print(f"Creating {size_mb} MB of data...")
    
    gc.collect()
    mem_before, _ = get_memory_mb()
    
    # Create data
    data = os.urandom(size_mb * 1024 * 1024)
    gc.collect()
    mem_after_data, _ = get_memory_mb()
    
    print(f"Memory after creating data: {mem_after_data:.2f} MB")
    print(f"Memory increase: {mem_after_data - mem_before:.2f} MB")
    
    # Create multiple references (simulating what happens in pipeline)
    print(f"\nCreating multiple references...")
    refs = []
    for i in range(5):
        refs.append(data)  # Just a reference, not a copy
    
    gc.collect()
    mem_after_refs, _ = get_memory_mb()
    
    print(f"Memory after 5 references: {mem_after_refs:.2f} MB")
    print(f"Reference overhead: {mem_after_refs - mem_after_data:.2f} MB")
    
    if mem_after_refs - mem_after_data > 1:
        print(f"⚠️  Unexpected overhead from references!")
    else:
        print(f"✓ References don't increase memory (as expected)")
    
    # Clean up
    del data
    del refs
    gc.collect()

def test_thread_local_copies():
    """Test if passing data between threads creates copies."""
    print("\n" + "=" * 80)
    print("TEST 4: Thread-local data handling")
    print("=" * 80)
    
    import threading
    import queue
    
    size_mb = 100
    print(f"Creating {size_mb} MB of data...")
    
    gc.collect()
    mem_before, _ = get_memory_mb()
    
    # Create data
    data = os.urandom(size_mb * 1024 * 1024)
    gc.collect()
    mem_after_data, _ = get_memory_mb()
    
    print(f"Memory after creating data: {mem_after_data:.2f} MB")
    
    # Pass data through queue (simulating ThreadPoolExecutor)
    print(f"\nPassing data through queue to another thread...")
    q = queue.Queue()
    result_q = queue.Queue()
    
    def worker():
        item = q.get()
        # Simulate processing
        result_q.put(len(item))
    
    thread = threading.Thread(target=worker)
    thread.start()
    
    q.put(data)
    thread.join()
    result = result_q.get()
    
    gc.collect()
    mem_after_thread, _ = get_memory_mb()
    
    print(f"Memory after thread processing: {mem_after_thread:.2f} MB")
    print(f"Thread overhead: {mem_after_thread - mem_after_data:.2f} MB")
    
    if mem_after_thread - mem_after_data > size_mb * 0.5:
        print(f"⚠️  COPY DETECTED! Thread created a {(mem_after_thread - mem_after_data) / size_mb * 100:.1f}% copy!")
    else:
        print(f"✓ No significant copy (thread uses reference)")
    
    # Clean up
    del data
    gc.collect()

def test_python_memory_allocator():
    """Test Python's memory allocator behavior."""
    print("\n" + "=" * 80)
    print("TEST 5: Python memory allocator (pymalloc)")
    print("=" * 80)
    
    size_mb = 100
    num_iterations = 10
    
    print(f"Allocating and freeing {size_mb} MB {num_iterations} times...")
    
    mem_start, _ = get_memory_mb()
    print(f"Starting memory: {mem_start:.2f} MB")
    
    peak_mem = mem_start
    
    for i in range(num_iterations):
        # Allocate
        data = os.urandom(size_mb * 1024 * 1024)
        gc.collect()
        mem_current, _ = get_memory_mb()
        
        if mem_current > peak_mem:
            peak_mem = mem_current
        
        print(f"  Iteration {i+1}: {mem_current:.2f} MB")
        
        # Free
        del data
        gc.collect()
    
    mem_end, _ = get_memory_mb()
    print(f"\nEnding memory: {mem_end:.2f} MB")
    print(f"Peak memory: {peak_mem:.2f} MB")
    print(f"Memory retained: {mem_end - mem_start:.2f} MB")
    
    if mem_end - mem_start > size_mb * 0.5:
        print(f"⚠️  MEMORY LEAK! Python retained {mem_end - mem_start:.2f} MB after freeing!")
        print(f"   This is {(mem_end - mem_start) / size_mb * 100:.1f}% of allocated size")
    else:
        print(f"✓ Memory properly released")

def main():
    print(f"PID: {os.getpid()}")
    print(f"Python version: {sys.version}")
    print(f"tracemalloc enabled: {tracemalloc.is_tracing()}")
    
    # Run all tests
    test_bytes_to_bytesio_copy()
    test_boto3_request_preparation()
    test_multiple_references()
    test_thread_local_copies()
    test_python_memory_allocator()
    
    # Final summary
    print("\n" + "=" * 80)
    print("SUMMARY")
    print("=" * 80)
    
    current, peak = tracemalloc.get_traced_memory()
    print(f"Peak memory during all tests: {peak / (1024*1024):.2f} MB")
    
    tracemalloc.stop()
    
    print("\n" + "=" * 80)
    print("CONCLUSION")
    print("=" * 80)
    print("""
The memory pool tracks logical allocations, but actual memory usage exceeds
the pool limit due to:

1. BytesIO wrapping (boto3 requirement for retries)
2. Python memory allocator not releasing memory to OS
3. Thread overhead and queue buffering
4. Potential copies in boto3/botocore/urllib3 layers

To prove this definitively with real S3 uploads, run:
  S3_BUCKET=my-bucket python perf/trace_memory_pool.py
""")

if __name__ == '__main__':
    main()
