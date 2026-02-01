#!/bin/bash
# Run all memory pool investigation tests
# No AWS credentials required!

set -e  # Exit on error

echo "================================================================================"
echo "Memory Pool Investigation - Test Suite"
echo "================================================================================"
echo ""
echo "This will run all tests to prove the memory pool issue."
echo "Estimated time: ~30 seconds"
echo ""

# Check Python version
PYTHON_VERSION=$(python3 --version 2>&1 | awk '{print $2}')
echo "Python version: $PYTHON_VERSION"

# Check psutil
if python3 -c "import psutil" 2>/dev/null; then
    PSUTIL_VERSION=$(python3 -c "import psutil; print(psutil.__version__)")
    echo "psutil version: $PSUTIL_VERSION"
else
    echo "ERROR: psutil not installed"
    echo "Install with: pip install psutil"
    exit 1
fi

echo ""
echo "================================================================================"
echo "Test 1: Realistic Simulation (Main Proof)"
echo "================================================================================"
echo ""

python3 perf/simulate_real_upload.py

echo ""
echo "✓ Test 1 complete"
echo ""
echo "Results saved to:"
echo "  - /tmp/real_upload_simulation_trace.jsonl"
echo "  - /tmp/real_upload_simulation_summary.txt"
echo "  - /tmp/simulation_output.txt"
echo ""

echo "================================================================================"
echo "Test 2: Simple Pool vs RSS Test"
echo "================================================================================"
echo ""

python3 perf/prove_pool_vs_rss.py > /dev/null 2>&1

echo ""
echo "✓ Test 2 complete"
echo ""
echo "Results saved to:"
echo "  - /tmp/pool_vs_rss_proof.txt"
echo "  - /tmp/pool_vs_rss_trace.txt"
echo ""

echo "================================================================================"
echo "Test 3: Baseline Memory Tests"
echo "================================================================================"
echo ""

python3 perf/prove_memory_leak.py > /dev/null 2>&1

echo ""
echo "✓ Test 3 complete"
echo ""
echo "Results saved to:"
echo "  - /tmp/memory_leak_proof.txt"
echo ""

echo "================================================================================"
echo "ALL TESTS COMPLETE"
echo "================================================================================"
echo ""
echo "Summary of findings:"
echo ""

# Extract key metrics from simulation
if [ -f /tmp/real_upload_simulation_summary.txt ]; then
    echo "From realistic simulation:"
    grep "Max pool:" /tmp/real_upload_simulation_summary.txt || true
    grep "Max RSS:" /tmp/real_upload_simulation_summary.txt || true
    grep "Violations:" /tmp/real_upload_simulation_summary.txt || true
    grep "exceeded" /tmp/real_upload_simulation_summary.txt || true
fi

echo ""
echo "Detailed analysis available in:"
echo "  - perf/2026-01-31-deepdive.md"
echo "  - perf/TRACE_ANALYSIS.md"
echo "  - perf/REPRODUCTION_GUIDE.md"
echo ""
echo "View results:"
echo "  cat /tmp/real_upload_simulation_summary.txt"
echo "  cat /tmp/simulation_output.txt"
echo ""
