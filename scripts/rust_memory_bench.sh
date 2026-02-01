#!/bin/bash
# Run Rust benchmark with memory monitoring

source creds.sh

# Clear caches
rm -f ~/.cache/rusty-attachments/hash_cache.db
rm -f ~/.cache/rusty-attachments/s3_check_cache.db

OUTPUT_FILE=/tmp/rust_memory_bench_results.txt

echo "=== Rust S3 Upload Benchmark with Memory Monitoring ===" > $OUTPUT_FILE
echo "Date: $(date)" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

# Run with 1GB memory limit on VFX dataset
echo "Running Rust benchmark (1GB pool limit, 5.5GB data)..." >> $OUTPUT_FILE
cargo run --release --example bench_hash_upload -- run \
    --test-dir /tmp/bench_s3_large \
    --bucket adeadlineja \
    --prefix rusty/bench-memory-test-$(date +%s) \
    --staged \
    --max-memory-gb 1.0 >> $OUTPUT_FILE 2>&1

echo "" >> $OUTPUT_FILE
echo "=== Complete ===" >> $OUTPUT_FILE
