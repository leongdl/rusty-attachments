#!/bin/bash
# Clear all caches for clean benchmark runs

set -e

CACHE_DIR="${HOME}/.cache/rusty-attachments"

echo "Clearing rusty-attachments caches..."

# Hash cache
if [ -f "${CACHE_DIR}/hash_cache.db" ]; then
    rm -f "${CACHE_DIR}/hash_cache.db"
    echo "  Removed hash_cache.db"
fi

# S3 check cache
if [ -f "${CACHE_DIR}/s3_check_cache.db" ]; then
    rm -f "${CACHE_DIR}/s3_check_cache.db"
    echo "  Removed s3_check_cache.db"
fi

# Data cache directory
if [ -d "${CACHE_DIR}/data_cache" ]; then
    rm -rf "${CACHE_DIR}/data_cache"
    echo "  Removed data_cache/"
fi

# S3 test prefix (optional - clears uploaded test data)
if [ "$1" == "--s3" ]; then
    echo "Clearing S3 test prefix..."
    aws s3 rm s3://adeadlineja/rusty/bench/ --recursive
    echo "  Removed s3://adeadlineja/rusty/bench/"
fi

echo "Done."
