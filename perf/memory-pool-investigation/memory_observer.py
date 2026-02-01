#!/usr/bin/env python3
"""Memory Observer Script - Monitors memory usage of a target process over time."""
import psutil
import time
import sys
import json

def monitor_process(pid, interval=0.1, output_file="/tmp/memory_trace.jsonl"):
    try:
        process = psutil.Process(pid)
        print(f"Monitoring PID {pid}: {process.name()}", file=sys.stderr)
        print(f"Output: {output_file}", file=sys.stderr)
        
        with open(output_file, 'w') as f:
            start_time = time.time()
            sample_count = 0
            
            while True:
                try:
                    mem_info = process.memory_info()
                    mem_percent = process.memory_percent()
                    num_threads = process.num_threads()
                    
                    data = {
                        'timestamp': time.time(),
                        'elapsed': time.time() - start_time,
                        'rss_bytes': mem_info.rss,
                        'rss_mb': mem_info.rss / (1024 * 1024),
                        'vms_bytes': mem_info.vms,
                        'vms_mb': mem_info.vms / (1024 * 1024),
                        'percent': mem_percent,
                        'threads': num_threads,
                    }
                    
                    f.write(json.dumps(data) + '\n')
                    f.flush()
                    
                    sample_count += 1
                    if sample_count % 10 == 0:
                        print(f"[{data['elapsed']:.1f}s] RSS: {data['rss_mb']:.1f} MB, VMS: {data['vms_mb']:.1f} MB, Threads: {num_threads}", file=sys.stderr)
                    
                    time.sleep(interval)
                    
                except psutil.NoSuchProcess:
                    print(f"Process {pid} terminated", file=sys.stderr)
                    break
                    
        print(f"Monitoring complete. Samples: {sample_count}", file=sys.stderr)
        
    except psutil.NoSuchProcess:
        print(f"Process {pid} not found", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nMonitoring interrupted", file=sys.stderr)
        sys.exit(0)

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: memory_observer.py <pid>", file=sys.stderr)
        sys.exit(1)
    
    pid = int(sys.argv[1])
    monitor_process(pid)
