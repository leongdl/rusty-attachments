# Real Upload Simulation - Trace Analysis

## Test Configuration

- **Files:** 20 files × 50 MB = 1000 MB total
- **Pool limit:** 500 MB
- **Workers:** 4 READ+HASH threads + 4 UPLOAD threads
- **Hash time:** ~5ms per file (fast - CPU bound)
- **Upload time:** ~50ms per file (slow - network I/O bound)

## Memory Timeline

### Visual Representation

```
Time (ms) →
0    100   200   300   400   500   600   700   800   900   1000  1100
│    │     │     │     │     │     │     │     │     │     │     │
├────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┤
│                                                                  │
│ Pool Limit: 500 MB ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│                                                                  │
│ Pool Allocated (MB):                                            │
│   0 ─┐                                                          │
│     200 ─┐  ┌─ 200 ─┐  ┌─ 200 ─┐  ┌─ 200 ─┐  ┌─ 200 ─┐       │
│         400 ┘       400 ┘       400 ┘       400 ┘       └─ 0   │
│                                                                  │
│ RSS (MB):                                                       │
│   12 ─┐                                                    ┌─ 12│
│      69 ─┐                                                 │    │
│         131 ─┐                                             │    │
│             192 ─┐                                         │    │
│                 247 ─┐                                     │    │
│                     309 ─┐                                 │    │
│                         371 ─┐                             │    │
│                             426 ─┐                         │    │
│                                 488 ─┐                     │    │
│                                     550 ─┐ ← VIOLATION!    │    │
│                                         611 ─┐             │    │
│                                             667 ─┐         │    │
│                                                 728 ─┐     │    │
│                                                     790 ─┐ │    │
│                                                         845│    │
│                                                         907│    │
│                                                         969│    │
│                                                        1012│    │
│                                                        1012└────┘
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Detailed Trace Data

### Phase 1: Initial Files (0-200ms)

```
Time  Pool   RSS    Gap    Event
────────────────────────────────────────────────────────────────
0ms   0 MB   12 MB  12 MB  Startup
50ms  200MB  69 MB  -131   Files 1-4: Hashing (4 threads busy)
100ms 200MB  131MB  -69    Files 1-4: Still hashing
150ms 200MB  192MB  -8     Files 1-4: Hash complete, queued for upload
200ms 400MB  247MB  -153   Files 5-8: Hashing starts (pool reused)
```

**Analysis:**
- Pool oscillates between 200-400 MB (4-8 files being hashed)
- RSS grows steadily as files are read into memory
- Negative gap means pool accounting is ahead of actual RSS

### Phase 2: Pipeline Fills (200-450ms)

```
Time  Pool   RSS    Gap    Event
────────────────────────────────────────────────────────────────
250ms 200MB  309MB  109MB  Files 1-4: Uploading (boto3 holds refs)
301ms 200MB  371MB  171MB  Files 5-8: Uploading
351ms 400MB  426MB  26 MB  Files 9-12: Hashing
401ms 200MB  488MB  288MB  Gap growing rapidly
451ms 200MB  550MB  350MB  ⚠️ FIRST VIOLATION: RSS > 500 MB!
```

**Analysis:**
- Gap turns positive and grows (boto3 holding references)
- Pool releases memory after hashing, but RSS keeps growing
- First violation at 451ms when 11th file batch completes
- **This is the race condition in action!**

### Phase 3: Peak Memory (450-900ms)

```
Time  Pool   RSS    Gap    Event
────────────────────────────────────────────────────────────────
501ms 200MB  611MB  411MB  Violation continues
551ms 400MB  667MB  267MB  Files 13-16: Hashing
601ms 200MB  728MB  528MB  Gap = 528 MB (more than pool limit!)
651ms 200MB  790MB  590MB  Gap = 590 MB
701ms 400MB  845MB  445MB  Files 17-20: Hashing (last batch)
752ms 200MB  907MB  707MB  Gap = 707 MB
802ms 200MB  969MB  769MB  Gap = 769 MB
852ms 200MB  1012MB 812MB  ⚠️ PEAK: RSS = 1012 MB (2x over limit!)
```

**Analysis:**
- RSS continues growing even as pool stays under limit
- Gap reaches 812 MB (larger than pool limit itself!)
- Peak RSS = 1012 MB while pool = 200 MB
- **Pool thinks only 4 files in memory, but 20 files worth of data is in RSS!**

### Phase 4: Cleanup (900-1200ms)

```
Time  Pool   RSS     Gap     Event
────────────────────────────────────────────────────────────────
902ms 0 MB   1012MB  1012MB  All hashing done, pool empty
952ms 0 MB   1012MB  1012MB  Uploads finishing, boto3 still holds refs
1173ms 0MB   12 MB   12 MB   boto3 refs cleared, memory freed!
```

**Analysis:**
- Pool drops to 0 MB (all files released)
- RSS stays at 1012 MB (boto3 still processing uploads)
- After boto3 finishes and refs are cleared, RSS drops to 12 MB
- **This proves boto3 was holding the memory!**

## Key Metrics

### Pool Behavior
- **Max allocated:** 400 MB (8 files × 50 MB)
- **Never exceeded:** 500 MB limit ✓
- **Oscillation:** 0 → 200 → 400 → 200 → 0 (as expected)

### RSS Behavior
- **Peak:** 1012 MB
- **Exceeded limit by:** 512 MB (102%)
- **Violations:** 11 samples (from 451ms to 952ms)
- **Duration over limit:** 501ms (55% of test time)

### Gap Analysis
- **Max gap:** 1012 MB (at 902ms when pool = 0)
- **Gap > pool limit:** Yes! (1012 MB > 500 MB)
- **Cause:** boto3 holding BytesIO references

## The Race Condition

### Why This Happens

```
READ+HASH threads (fast):
  File 1: 0-5ms    ████
  File 2: 5-10ms       ████
  File 3: 10-15ms          ████
  File 4: 15-20ms              ████
  ...
  File 20: 95-100ms                                        ████

UPLOAD threads (slow):
  File 1: 5-55ms   ██████████████████████████████████████████████
  File 2: 10-60ms      ██████████████████████████████████████████████
  File 3: 15-65ms          ██████████████████████████████████████████████
  File 4: 20-70ms              ██████████████████████████████████████████████
  ...
  File 20: 100-150ms                                       ██████████████████████████████████████████████
```

**The problem:**
- READ+HASH processes 20 files in 100ms
- UPLOAD processes 20 files in 150ms (50ms lag)
- During the lag, all 20 files are in memory simultaneously
- Pool releases after hashing, but boto3 holds until upload completes
- Result: 20 × 50 MB = 1000 MB in memory, but pool shows 0-400 MB

## Comparison: Pool vs Reality

| Metric | Pool Tracking | Actual RSS | Ratio |
|--------|---------------|------------|-------|
| Max memory | 400 MB | 1012 MB | 2.5x |
| Violations | 0 | 11 | ∞ |
| Over limit | 0 MB | 512 MB | ∞ |
| Duration | 0ms | 501ms | ∞ |

**Conclusion:** The pool is working correctly, but it's tracking the wrong thing. It tracks "allocated" memory, but actual memory includes boto3's internal references.

## Recommendations

### Short-term Fix
Set pool limit to 20-30% of available memory to account for the 2.5-5x multiplier:

```python
available_memory = psutil.virtual_memory().available
pool_limit = available_memory * 0.2  # 20% instead of 50%
```

### Medium-term Fix
Add RSS monitoring alongside pool tracking:

```python
if psutil.Process().memory_info().rss > threshold:
    # Block even if pool has space
    wait_for_memory_to_decrease()
```

### Long-term Fix
Use Rust + AWS CRT which streams from disk instead of buffering in memory.

## Files

- **Trace data:** `perf/real_upload_simulation_trace.jsonl`
- **Summary:** `perf/real_upload_simulation_summary.txt`
- **Full output:** `perf/simulation_output.txt`
- **Test script:** `perf/simulate_real_upload.py`
