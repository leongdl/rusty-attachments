#!/usr/bin/env python3
"""Analyze memory trace data."""
import json
import sys

def analyze_trace(trace_file, pool_limit_mb=1024):
    samples = []
    with open(trace_file, 'r') as f:
        for line in f:
            samples.append(json.loads(line))
    
    if not samples:
        print("No samples found")
        return
    
    max_rss = max(s['rss_mb'] for s in samples)
    max_vms = max(s['vms_mb'] for s in samples)
    avg_rss = sum(s['rss_mb'] for s in samples) / len(samples)
    
    # Find when pool limit was exceeded
    exceeded = [s for s in samples if s['rss_mb'] > pool_limit_mb]
    
    print(f"=== Memory Analysis ===")
    print(f"Total samples: {len(samples)}")
    print(f"Duration: {samples[-1]['elapsed']:.2f}s")
    print(f"Pool limit: {pool_limit_mb} MB")
    print(f"Max RSS: {max_rss:.2f} MB")
    print(f"Max VMS: {max_vms:.2f} MB")
    print(f"Avg RSS: {avg_rss:.2f} MB")
    print(f"Exceeded pool limit: {len(exceeded)} times ({len(exceeded)/len(samples)*100:.1f}%)")
    
    if exceeded:
        print(f"\nFirst exceeded at: {exceeded[0]['elapsed']:.2f}s ({exceeded[0]['rss_mb']:.2f} MB)")
        print(f"Peak exceeded at: {max(exceeded, key=lambda s: s['rss_mb'])['elapsed']:.2f}s ({max_rss:.2f} MB)")
        print(f"Exceeded by: {max_rss - pool_limit_mb:.2f} MB ({(max_rss/pool_limit_mb - 1)*100:.1f}%)")

if __name__ == '__main__':
    trace_file = sys.argv[1] if len(sys.argv) > 1 else "/tmp/memory_trace.jsonl"
    pool_limit = int(sys.argv[2]) if len(sys.argv) > 2 else 1024
    analyze_trace(trace_file, pool_limit)
