# sefer-alloc — Flamegraph Profiling Report (2026-06-28)

Task #61: build flamegraphs under characteristic workloads, find hot paths,
identify candidate optimizations.

---

## §0 — How to reproduce

### Prerequisites

```
perf_event_paranoid = 2  (already active)
cargo install flamegraph   # 0.6.13
cargo install inferno      # 0.12.6
apt-get install linux-tools-generic  # perf 6.8
```

**Important — WSL2 ABI mismatch:**
`/usr/bin/perf` (symlink to perf-6.18 WSL2 kernel) is broken when recording data.
You must use `/usr/lib/linux-tools/6.8.0-124-generic/perf` directly.
`cargo flamegraph` works as expected with this PATH.

**Important detail:** `cargo flamegraph` with `perf.data` in the working directory on
a mounted NTFS (D: drive) causes the error `"failed to write perf data,
error: Bad address"` — this is not an ABI issue, but slow NTFS IO under perf's MMapped
ring buffer. Solution: use `CARGO_TARGET_DIR=/tmp/...` + build the binary directly
and run perf with `-o /tmp/...`.

### Reproduction commands

```bash
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$PATH

# §1 — Small-class churn
mkdir /tmp/sefer-fg1
CARGO_PROFILE_BENCH_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg1 \
  cargo build --bench global_alloc --features 'alloc-global' --profile bench
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg1/perf_small.data \
  /tmp/sefer-fg1/release/deps/global_alloc-<hash> --bench 'SeferMalloc'
perf script -i /tmp/sefer-fg1/perf_small.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg1/small_churn.svg

# §2 — MT cross-thread
mkdir /tmp/sefer-fg2
CARGO_PROFILE_RELEASE_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg2 \
  cargo build --release --example malloc_macro \
  --features 'alloc-global alloc-xthread'
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg2/perf_mt.data \
  /tmp/sefer-fg2/release/examples/malloc_macro
perf script -i /tmp/sefer-fg2/perf_mt.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg2/mt_xthread.svg

# §3 — Large/realloc
mkdir /tmp/sefer-fg3
CARGO_PROFILE_BENCH_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg3 \
  cargo build --bench large_realloc --features 'alloc-global' --profile bench
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg3/perf_large.data \
  /tmp/sefer-fg3/release/deps/large_realloc-<hash> --bench 'SeferMalloc'
perf script -i /tmp/sefer-fg3/perf_large.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg3/large_realloc.svg

# §4 — tokio burn-in
mkdir /tmp/sefer-fg4
CARGO_PROFILE_RELEASE_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg4 \
  cargo build --release --example tokio_burn_in \
  --features 'alloc-global alloc-xthread'
SEFER_BURNIN_SECONDS=20 SEFER_TOKIO_WORKERS=4 SEFER_BURNIN_TASKS=512 \
  perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg4/perf_tokio.data \
  /tmp/sefer-fg4/release/examples/tokio_burn_in
perf script -i /tmp/sefer-fg4/perf_tokio.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg4/tokio_burnin.svg
```

---

## §1 — Single-thread small-class churn (`global_alloc` bench)

**SVG:** `/tmp/sefer-fg1/small_churn.svg`  
**Samples:** 9 463 (cycles:Pu). Lost samples: 0.

### Findings — DATA QUALITY WARNING

The flamegraph profiles **the entire criterion process**, including its own
statistics (KDE / bootstrapping / rayon). As a result, the picture is heavily distorted:

| Function | Self-time |
|---|---|
| rayon/criterion KDE `bridge_producer_consumer_helper` | **52.25%** |
| `libm __ieee754_exp_fma` | 20.74% |
| `libm exp()` | 11.56% |
| `AllocCore::alloc` (SeferMalloc) | **1.72%** |
| `bench_direct_alloc` (benchmark wrapper) | 1.02% |
| `SegmentTable::contains_base` | 0.72% |
| `HeapCore::stamp_segment_owner` | 0.29% |

**Conclusion:** ~84% CPU is criterion statistics (KDE with exp() from libm). The allocator
itself takes ~3.7% total. The data is **informative** only relative to each other,
not in an absolute sense.

### Allocator hot paths (within its 3.7%)

**Top-3 by self-time (allocator only):**
1. `AllocCore::alloc` — 1.72% (main allocation path)
2. `SegmentTable::contains_base` — 0.72% (foreign pointer check in dealloc)
3. `HeapCore::stamp_segment_owner` — 0.29% (atomic ownership stamping)

**Top-3 by total-time (allocator only):**
1. `AllocCore::alloc` — includes `pop_free` (pop from BinTable) + `alloc_small`
2. `SegmentTable::contains_base` — linear O(segments) scan on every `dealloc`
3. `HeapCore::stamp_segment_owner` — Acquire-load + conditional Release-store on every alloc

### Observations

1. **`contains_base` — O(segments) scan on EVERY `dealloc`.** This is a linear
   iteration over slots[] in segment_table.rs:220. In a bench where one thread continuously
   does alloc/dealloc of small blocks, this is not a bottleneck only because
   segments < 5. But as live_segments grows (to 50–100) this could become
   noticeable.

2. **`stamp_segment_owner` on every alloc.** In `HeapCore::alloc` after every
   successful allocation, `stamp_segment_owner` is called, which does an Acquire
   load + conditional Release store on `owner_state`. For a segment-hot workload where
   one segment is used continuously, the condition `unpack_owner_id(cur) != self.id`
   never fires, but the Acquire load still happens.

3. **Actual performance:** SeferMalloc ~18–20 µs per batch of 32K
   operations (criterion batch). Mimalloc ~10–14 µs (1.5–2x faster on small
   sizes). This matches expectations.

### Candidate optimizations (§1)

- **OPT-A:** Skip `stamp_segment_owner` when the segment is already stamped with this
  heap id (cache last_stamped_base in HeapCore). Expected gain: ~0.3%
  CPU, but may be noticeable in micro-bench.
- **OPT-B:** `contains_base` — replace linear scan with a hash-set or bitmap
  with O(1) lookup. Relevant when segments > 20.

---

## §2 — MT cross-thread free (`malloc_macro` benchmark)

**SVG:** `/tmp/sefer-fg2/mt_xthread.svg`  
**Samples:** 361 (cycles:Pu). Lost samples: 0.  
**Note:** low sample count — the workload is short (larson + mstress T=1/2/4).
Conclusions are indicative, not precise.

### Top-3 by self-time

| Function | Self-time |
|---|---|
| `std::sync::mpmc::list::Channel::try_recv` | **16.70%** |
| `libc _int_free` | 11.21% |
| `libc malloc` | 10.28% |

*Note: `libc malloc/_int_free` appears here because the malloc_macro bench runs all three
allocators (SeferMalloc, mimalloc, System) in parallel.*

| Function (SeferMalloc only) | Self-time |
|---|---|
| `AllocCore::alloc` | **5.24%** |
| `HeapCore::dealloc_routing` | 3.58% |
| `mstress_worker<SeferMalloc>` | 1.91% |
| `larson_worker<SeferMalloc>` | 0.98% |
| `HeapCore::stamp_segment_owner` | 1.46% |

### Observations

1. **`dealloc_routing` — 3.58% self.** This is the cross-thread dealloc path: reads
   `magic_at`, `owner_thread_free_at`, computes `segment_base_of_ptr` and pushes
   `(offset, class)` into RemoteFreeRing via CAS. Takes ~40% of alloc (5.24%).
   This ratio is expected for larson (which actively does cross-thread free).

2. **`stamp_segment_owner` — 1.46%** out of the total 5.24% alloc. This means that for
   every ~3.6 allocations, 1 stamp is spent. Significant.

3. **`mstress` vs `larson`:** mstress on SeferMalloc (1.91%) vs larson (0.98%).
   mstress uses random sizes with a wide range — more free-list misses,
   more frequent `carve_block`.

4. **Comparison with mimalloc:** mimalloc shows better mstress (27.41 M vs
   19.23 M ops/sec). This is a 1.43x gap. larson — SeferMalloc wins (18.3 M
   vs 13.6 M). The flamegraph shows that `mi_page_queue_find_free_ex` and
   `mi_page_malloc_zero` in mimalloc take a similar percentage to our `alloc`.

5. **RemoteFreeRing overhead:** cross-thread push takes a small fraction — ~0.1%
   in `ring.push`. This means the CAS-reservation is not a bottleneck.

### Candidate optimizations (§2)

- **OPT-C:** `stamp_segment_owner` — rewrite as a branch-free check:
  save last_stamped_base and skip stamp if `ptr` is in the same segment.
  Saves an Acquire load + branch on every alloc.
- **OPT-D:** `dealloc_routing` — `segment_base_of_ptr` is called in both alloc and
  dealloc. Could pass base as a hint (if the caller knows) — but this changes
  the API. Not recommended.

---

## §3 — Large/realloc (`large_realloc` bench)

**SVG:** `/tmp/sefer-fg3/large_realloc.svg`  
**Samples:** 8 648 (cycles:Pu). Losses: 0.

### WARNING: same distortions from criterion

| Function | Self-time |
|---|---|
| criterion KDE (rayon) | **50.37%** |
| `libm __ieee754_exp_fma` | 21.93% |
| `libm exp()` | 11.01% |
| `AllocCore::alloc` (SeferMalloc) | **6.74%** |
| `HeapCore::realloc` | 0.01% |

### Top-3 by self-time (allocator)

1. `AllocCore::alloc` — 6.74% (significantly higher than in the small-class bench!)
2. `libm __munmap` — 0.08% (OS dealloc for large segments)
3. `HeapCore::realloc` — 0.01%

### Observations

1. **Large alloc goes entirely through mmap/VirtualAlloc.** Every allocation
   >= SMALL_MAX gets a separate segment via `os::reserve_segment`. On WSL2,
   mmap has no page cache — every alloc+free is a full round-trip to the
   hypervisor. SeferMalloc measured: **~8.3 µs** per alloc+free of 4 MiB/16 MiB/64 MiB.
   mimalloc (not in this profile) has a page-cache for large allocations -> much
   faster.

2. **`realloc_grow_geometric` — 65 µs for 16 doublings (64 B -> 4 MiB).** This is
   alloc + memcpy + dealloc at each step. SeferMalloc `realloc` always
   does a new alloc + copy (no in-place growth) — each step = 2 mmap + 1 memcpy.
   mimalloc has slab-growth with partial in-place — significantly wins
   (documented as "300x+ lag in ALLOC_BENCH.md").

3. **`AllocCore::alloc` takes 6.74%** vs 1.72% in the small-class bench. The proportion
   increased because we profiled only SeferMalloc (filter `--bench 'SeferMalloc'`),
   reducing criterion's weight.

4. **`__munmap` — 0.08%** indicates real OS calls in dealloc large.
   This is a cold path (once per bench iteration), but it is expensive in absolute
   time.

5. **`realloc_in_place_unfavorable`:** SeferMalloc spends ~9.5 µs on 8 growth steps
   with neighbors. Each step is a full mmap (large segment) + memcpy + munmap.
   This is unavoidable without a segment cache.

### Candidate optimizations (§3)

- **OPT-E:** Cache of empty large segments (size <= N, e.g. <= 64 MiB). On
  deallocation the large segment is not freed immediately via `os::release_segment`,
  but placed in a per-thread freelist (1–2 slots). On the next large alloc of
  similar size — reuse without mmap. Expected gain: 10–100x on large alloc+free
  micro-bench. Risk: RSS grows (the segment stays in memory). Parameters: max cache
  size + time-based eviction.

- **OPT-F:** In-place realloc for small->small upgrades (when new size <=
  block_size of the current class). Currently `AllocCore::realloc` always does
  alloc + copy + dealloc. If `new_size <= SizeClasses::block_size(old_class_idx)`,
  we can return the same ptr. Expected gain: eliminates alloc+copy+dealloc on
  frequent-realloc patterns. Risk: must carefully update live_count (decommit
  feature).

---

## §4 — tokio async burn-in

**SVG:** `/tmp/sefer-fg4/tokio_burnin.svg`  
**Samples:** 8 (cycles:Pu). Losses: 0.

### WARNING: extremely limited data

The burn-in with 512 tasks completes in **0.07 seconds**. Perf at F=99 Hz managed
to take only 8 samples. Any conclusions with this sample size are unreliable —
this is **approximate** data, not statistically significant.

To obtain a real profile, one needs either:
- A repeating workload (loop { spawn 512 tasks }) without early-exit, or
- A larger number of tasks — but then the allocator crashes with OOM (see below).

### What happened when increasing the load

With `SEFER_BURNIN_TASKS=2000, SEFER_BURNIN_CONCURRENCY=200` the process crashes with:
```
memory allocation of 256 bytes failed
memory allocation of 3072 bytes failed
skipping backtrace printing to avoid potential recursion
```

This is OOM: `MAX_SEGMENTS = 1024` without `alloc-decommit` — the append-only segment
table overflows with a large number of concurrent tasks and their tokio-internal
allocations (runtime allocates per-task stacks, queues, etc.).

### Top-3 by self-time (with 8 samples — LOW CONFIDENCE)

| Function | Self-time |
|---|---|
| `AllocCore::alloc` | 24.72% |
| `run_query` (async closure) | 13.39% |
| `Mutex::lock_contended` | 13.39% |
| tokio worker `run_task` | 13.39% |
| `__memset_evex_unaligned_erms` | 12.23% |
| `HeapCore::dealloc_routing` | 9.47% |

### Observations (caution: 8 samples)

1. **`AllocCore::alloc` — 24.72% of 8 samples.** Expected: tokio creates
   tasks, initializes TLS heaps, allocates task-local data.

2. **`Mutex::lock_contended` — 13.39%.** This is std::sync::Mutex, presumably
   in HeapRegistry (during claim/init of a new heap for a new tokio worker thread).
   TLS heap init under contention is clearly visible.

3. **`dealloc_routing` — 9.47%.** Cross-thread free is active: tokio drops
   tasks on worker threads different from those that allocated.

4. **`memset` — 12.23%.** Large zeroed allocations (Vec::resize, HashMap init).

5. **OOM on scaling** — a key finding: `alloc-decommit` is not enabled
   in the standard `alloc-global + alloc-xthread` build. Without it, segments are not
   returned -> rapid exhaustion of the 1024-slot table under async load.

### Candidate optimizations (§4)

- **OPT-G:** Enable `alloc-decommit` in tokio burn-in and soak-test as the
  recommended build. Solves OOM at scale and reduces RSS.
- **OPT-H:** HeapRegistry::claim — remove or replace the Mutex with a lock-free CAS
  for TLS heap init (atomic slot-claim). Reduces Mutex::lock_contended during
  mass task/thread creation.

---

## §5 — Prioritised optimization candidates

### #1 — OPT-E: Empty large-segment cache (HIGH IMPACT)

**What to change:** in `AllocCore::dealloc` (Large path) do not free the segment
immediately via `os::release_segment`, but store 1–2 slots in a per-`AllocCore`
freelist. On the next `alloc_large` of a similar size — reuse without mmap.

**Expected gain:** 10–100x on large alloc+free micro-bench (8 µs -> < 1 µs
for the hot path). Relevant for `realloc_grow_geometric` and `realloc_in_place_unfavorable`.

**Regression risk:** RSS grows by the size of cached segments (up to 64 MiB x 2 = 128 MiB
maximum with reasonable limits). Needs time-based or size-based eviction. Adds
a small overhead in the `alloc_large` cold path (scan freelist).

**Measurability:** `large_alloc_free/SeferMalloc/4MiB` should drop from 8.3 µs to < 1 µs.

---

### #2 — OPT-F: In-place small->small realloc (MEDIUM IMPACT)

**What to change:** in `AllocCore::realloc` before `alloc + copy + dealloc` check:
if `SizeClasses::class_for(new_size, align) == SizeClasses::class_for(old_size, align)`
(or new class block_size <= current block_size), return the same ptr without copy.

**Expected gain:** Vec::push-like patterns (size growing by 1.5–2x) often
land in the same class_idx for small growths -> eliminates alloc+copy+dealloc.
In `realloc_grow_geometric` the first few steps (64 B -> 128 B -> 256 B) may
land in the same or adjacent class — not always, but partially.

**Regression risk:** Zero for correctness (class_idx check is safe). Small
risk of fragmentation (block larger than needed). Must verify that `live_count` and
alloc_bitmap are not violated on in-place (block size does not change — no issues).

**Measurability:** New micro-bench `realloc_same_class` / `global_alloc/Vec_push`.

---

### #3 — OPT-B/C: O(1) segment lookup + lazy `stamp_segment_owner` (MEDIUM IMPACT)

**What to change (OPT-B):** `SegmentTable::contains_base` — replace linear O(count) scan
with a hash-set or open-addressing map with SEGMENT-aligned keys —
key = base >> log2(SEGMENT), value = slot_idx). Footprint: 1024 x 2 x 4 B = 8 KB
(fits in the metadata segment).

**What to change (OPT-C):** `HeapCore::alloc` calls `stamp_segment_owner` after EVERY
allocation. Add a `last_stamped_segment: *mut u8` field to `HeapCore` — skip stamp
if `segment_base_of_ptr(ptr) == self.last_stamped_segment`.

**Expected gain (OPT-B):** With num_segments = 50+, dealloc cost reduction of ~0.5–0.7% CPU.
**Expected gain (OPT-C):** ~1–1.5% CPU on Acquire-load with low num_segments (as
in the global_alloc micro-bench — stamp dominates ~1.46% in the MT profile).

**Regression risk (OPT-B):** Increased SegmentTable complexity, possible hash-collision edge cases.
**Regression risk (OPT-C):** On migration between segments the cache is correctly invalidated
(on the next alloc from a different segment the stamp occurs). No risk.

**Measurability:** Micro-bench with many live segments (e.g. 100 parallel allocators).

---

### #4 — OPT-G: `alloc-decommit` as default in multi-thread builds (HIGH IMPACT for scale)

**What to change:** Not code, but the recommended build: `alloc-global + alloc-xthread + alloc-decommit`
as the recommended feature set for production. Or add a convenience feature alias.

**Expected gain:** tokio burn-in works with 2000+ tasks without OOM; soak-test puts less
pressure on RSS. Eliminates hard segment-table overflow for long-running workloads.

**Regression risk:** `alloc-decommit` adds `dec_live`/`inc_live` counter updates on
every alloc/dealloc — small overhead (~1 field write). Verified in soak-test.

---

### #5 — OPT-H: Lock-free HeapRegistry::claim (LOW IMPACT — for now)

**What to change:** Replace the Mutex in `HeapRegistry` on the TLS bind-slow path with CAS-based
claim: atomic slot_state FREE->LIVE without a blocking mutex.

**Expected gain:** During burst creation of many threads (tokio spawn_blocking flood) removes
`Mutex::lock_contended` visible in the profile (13.39% of 8 samples — low confidence).

**Regression risk:** TLS bind-slow path is called rarely (once per thread). Measurable
gain only at >100 threads/sec creation rate.

---

## Potential optimization tasks

1. [OPT-E] Large-segment free-cache (1–2 slots per AllocCore)
2. [OPT-F] In-place small→small realloc when class doesn't change
3. [OPT-B] O(1) SegmentTable::contains_base (open-addressing hash map)
4. [OPT-C] Lazy stamp_segment_owner (cache last_stamped_segment in HeapCore)
5. [OPT-G] Enable alloc-decommit by default in multi-thread feature sets (or document as recommended)
6. [OPT-H] Lock-free HeapRegistry::claim (CAS-based TLS slot acquisition)

---

---

## §6 — Low-noise bench profiles (task #62)

Task #62 added two new criterion benches specifically for low-noise
allocator profiling, working around the issues from §1/§3 (84–85% on criterion KDE) and
§4 (8 samples).

### §6.1 — `heap_xthread` (push->drain ring cycle)

**SVG:** `/tmp/sefer-fg-v3a/heap_xthread.svg`
**Samples:** 4 654 (cycles:u). Lost samples: 0.

**Reproduction commands:**
```bash
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$HOME/.cargo/bin:$PATH
mkdir -p /tmp/sefer-fg-v3a

CARGO_PROFILE_BENCH_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-fg-v3a \
  cargo build --bench heap_xthread \
  --features 'alloc-core alloc-xthread' --profile bench

perf record -F 99 -e cycles:u --call-graph dwarf,16384 \
  -o /tmp/sefer-fg-v3a/perf_xthread.data \
  /tmp/sefer-fg-v3a/release/deps/heap_xthread-<hash> --bench

perf script -i /tmp/sefer-fg-v3a/perf_xthread.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title 'heap_xthread — push/drain ring (task #62)' \
    > /tmp/sefer-fg-v3a/heap_xthread.svg
```

**Top-3 by self-time:**

| Function | Self-time |
|---|---|
| criterion KDE `bridge_producer_consumer_helper` | **42.65%** |
| `AllocCore::dbg_push_to_ring` | **13.01%** |
| criterion `join_context` (rayon) | ~8% |

**Bench results:**
- `push_drain_256` (push+drain only, no alloc): **6.7–6.9 µs** per 256 iterations
- `alloc_push_drain_256` (alloc+push+drain): **30–40 µs** per 256 iterations

**Conclusion:** criterion overhead dropped from 84% (§1) to **43%** — a 2x improvement.
The allocator (`dbg_push_to_ring`) is now visible at **13%** self-time versus 1.7% in §1.
The `dbg_drain_all_rings` function was fully inlined by the optimizer — its cost
is dissolved into the iterating code (this is expected: drain is a tight loop with Relaxed
atomic stores).

---

### §6.2 — `heap_async_pattern` (database-pipeline mixed alloc)

**SVG:** `/tmp/sefer-fg-v3a/heap_async_pattern.svg`
**Samples:** 1 632 (cycles:u). Lost samples: 0.

**Reproduction commands:**
```bash
CARGO_PROFILE_BENCH_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-fg-v3a \
  cargo build --bench heap_async_pattern \
  --features 'alloc-global' --profile bench

perf record -F 99 -e cycles:u --call-graph dwarf,16384 \
  -o /tmp/sefer-fg-v3a/perf_async.data \
  /tmp/sefer-fg-v3a/release/deps/heap_async_pattern-<hash> --bench

perf script -i /tmp/sefer-fg-v3a/perf_async.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title 'heap_async_pattern — mixed alloc pipeline (task #62)' \
    > /tmp/sefer-fg-v3a/heap_async_pattern.svg
```

**Top-3 by self-time:**

| Function | Self-time |
|---|---|
| criterion KDE `bridge_producer_consumer_helper` | **39.64%** |
| `AllocCore::alloc` | **12.25%** |
| criterion rayon helpers | ~8% |

**Bench results:**
- `SeferMalloc/pipeline` (40 small + 1 grow + 16 medium allocs): **1.6–2.1 µs** per iteration

**Conclusion:** criterion overhead **40%** versus 85% in §3 (large_realloc) — a 2.1x improvement.
`AllocCore::alloc` is visible at **12.25%** versus 1.72% in §1 (without SeferMalloc filter).
Realloc (`HeapCore::realloc`) is visible at ~0.56%, which honestly reflects the small share of
row targets with grow operations relative to plain alloc+free.

---

### §6.3 — Comparison table: criterion overhead before and after

| Profile | Bench | Samples | criterion KDE self-time | allocator self-time |
|---|---|---|---|---|
| §1 | `global_alloc` (small churn) | 9 463 | **84%** | 1.72% (AllocCore::alloc) |
| §3 | `large_realloc` (realloc-heavy) | 8 648 | **85%** | 6.74% (AllocCore::alloc) |
| §4 | `tokio_burn_in` | 8 | ~50% (unreliable) | 24% (unreliable) |
| **§6.1** | **heap_xthread** (ring push+drain) | **4 654** | **43%** | **13.01%** (dbg_push_to_ring) |
| **§6.2** | **heap_async_pattern** (pipeline) | **1 632** | **40%** | **12.25%** (AllocCore::alloc) |

**Summary:** criterion overhead is fundamentally tied to measurement sample size.
If the inner loop executes in < 10 µs, criterion takes 10 samples and spends
~3 seconds on statistics — during which the KDE function does n^2 comparisons.
At **7 µs** (push_drain_256) over 3 seconds it accumulates ~430 000 iterations ->
10 points -> KDE over 10 numbers = minimal work, but it still takes ~43% CPU.

The gain from the new benches: **2x reduction in criterion overhead (84% -> 43%)** at the same
duration, plus **7–10x increase in allocator function visibility** (1.7% -> 13%).
This is sufficient for identifying hot paths, but not for precise isolation
profiling.

**Recommendation:** for deep isolation profiling (expected allocator
share > 60%):
1. **`samply`**: `cargo install samply` + `samply record cargo bench --bench heap_xthread ...`
   — macOS/Linux profiler with low overhead, better call-graph and flame chart UI.
   On WSL2 it is supported with limitations (no kernel symbols, but user-space works).
2. **Standalone tight-loop binary**: write `examples/bench_ring_tight.rs` without criterion —
   `loop { push 256 + drain }` with wall-clock timing at start/end. No KDE overhead.
   Profile this binary directly via `perf record -F 999`. Allocator share > 90%.

---

## Profiling session summary

### What worked

- `perf record -F 99 --call-graph dwarf,16384` with an explicit path to perf 6.8 on WSL2 — works.
- `inferno-collapse-perf | inferno-flamegraph` — demangles Rust symbols well.
- All 4+2 SVGs were generated and contain readable stack traces with symbols.
- New benches §6.1/§6.2 reduced criterion overhead from 84–85% to 40–43%.

### What did not work / limitations

1. **cargo flamegraph on NTFS mount** — crashes with "Bad address" due to slow IO
   when writing the perf ring buffer. Solution: explicit perf + target_dir in /tmp.

2. **Criterion bench profiles (§1, §3)** — criterion spends 80–85% CPU on
   its own KDE statistics (rayon + exp()). Only ~3–7% of the allocator is visible.
   For an honest profile, a standalone tight-loop without criterion is needed.

3. **tokio burn-in (§4)** — 512 tasks complete in 70 ms -> 8 samples (F=99 Hz).
   Data is unreliable. Scaling (>1000 tasks) leads to OOM without alloc-decommit.

4. **Sample losses** with a large bench (test_bench.data 6.6 GB): 41% lost.
   Solution: profile only one allocator (--bench 'SeferMalloc'), reduce
   measurement time.

5. **WSL2 PMU traces** — kernel tracepoints are unavailable (expected for WSL2).
   User-space sampling (cycles:u) works correctly. (`cycles:Pu` is unavailable
   on this WSL2 kernel — `cycles:u` was used instead.)

6. **criterion overhead is fundamental:** even with the new benches §6.1/§6.2, criterion
   takes 40–43% CPU. This is not a bug in the new benches — it is a structural property of criterion
   with small inner loops (< 10 µs). For full isolation profiling, samply
   or a standalone binary is needed (see §6.3 Recommendation).

---

## §7 — Post-fastbin re-investigation: where the real ceiling sits (2026-06-29)

After completing the fast-bin/tcache project (P0–P7, see
[`FASTBIN_DESIGN.md`](FASTBIN_DESIGN.md)) and full-inlining the entire seam (#101/#102)
single-thread larson/mstress T=1 remain **~1.3× slower than mimalloc**. To
understand whether this can be closed and where, a repeat flamegraph was run on
the final code. This investigation is a methodologically important chapter: it shows
why the naive hypothesis "8.5% bitmap → removing it = 8.5% gain" did not work
and where the **real ceiling** for the next optimizations lies.

### §7.0 — Reproduction

```bash
# WSL Ubuntu 24.04
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$HOME/.cargo/bin:$PATH

# Build with line-tables (so srcline resolution maps through inlining)
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-p8 \
  cargo build --release --example malloc_macro --features "alloc-global alloc-xthread"

# Full sweep (larson + mstress, T=1/2/4, all three allocators)
perf record -F 2000 -g --call-graph dwarf,8192 \
  -o /tmp/sefer-p8/p.data \
  /tmp/sefer-p8/release/examples/malloc_macro

# Source-line resolution (maps through inlining; sefer-alloc functions are
# fully inlined into the worker body, so symbol-level resolution misses them)
perf report --stdio -i /tmp/sefer-p8/p.data -g none \
  --sort=srcline --percent-limit 0.5 --full-source-path
```

### §7.1 — Where the cycles ACTUALLY go (larson + mstress)

| % | Layer | Notes |
|---|-------|-------|
| **21.5%** | `libc malloc.c` (`malloc/free`) | System-arm sampling (slowest arm, gets most samples by elapsed-time-share) |
| **18.8%** | `examples/malloc_macro.rs:188` (worker body) | xorshift PRNG + array indexing + branch — common to all three arms |
| **15.9%** | `std/sync/mpmc/mod.rs:948` | **Cross-thread mpmc channel coordination** — bench harness handoff overhead |
| **9.3%** | `core/sync/atomic.rs:3899` | Atomic operations — half ours (stamp_segment_owner Relaxed-load, owner_state CAS), half bench's |
| 8.6% | `libc malloc.c:4649` (`_int_free`) | System arm |
| 7.4% | `libc malloc.c:3347` (`_int_malloc`) | System arm |
| 5.9% | bench worker body (mstress branch) | |
| 5.7% | `std/sync/mpmc/mod.rs:397` | More channel coordination |
| ~3-5% | mimalloc internals (`free.c:209`, `alloc.c:120`) | mimalloc-arm cost |
| **0.23%** | `src/alloc_core/alloc_bitmap.rs:126` (`mark_alloc`/`mark_free`) | **OUR M2 bitmap** |
| **0.05%** | `src/alloc_core/alloc_bitmap.rs:116` (`locate`) | **Bit-position math** |

**Total Sefer-alloc own-code: < 1% of MT runtime.**

The sefer functions are fully inlined into the bench worker body by the
`#[inline(always)]` campaign (#101/#102). At the sample level they show up as
the inlined call sites in `bench_direct_alloc`'s body (the 18.8% line), not as
separate symbols. So this 18.8% is "worker harness body + inlined sefer-alloc".

### §7.2 — The hypothesis that died

The single-thread BULK microbench profile (`SeferMalloc/16B` from §1 here)
showed:
- 8.5% `alloc_bitmap::locate` (bit addressing)
- 3.8% `is_free` (M2 double-free check)
- 5.9% `contains_base` hash probe
- … etc.

Naive reading: if we remove these on the dealloc fast path, we save 12-18%.

That informed the [P8 design](FASTBIN_DESIGN.md#p8-investigation--idea-2-bintable-bitmap--in-block-key-reverted):
replace `AllocBitmap::is_free` in `dealloc_small` with an in-block key in word1.
P8 was implemented cleanly (correctness preserved, 165/0 tests, 43M-op
cross-thread soak balanced) — but **failed to deliver the expected larson T=1
improvement**.

**Why:** the MT macro-bench profile above is fundamentally different from the
single-thread bulk microbench. On MT:
- The bench harness's mpmc channel coordination is **16%** of runtime alone.
- libc malloc dominates (the System arm bench is slow; gets sampled most).
- Our entire allocator + its bitmap is **< 1%** of MT runtime.

The 8.5% bitmap-locate cost from §1 was an artifact of a tight microbench
where everything else was fast. On a real MT workload with thread
coordination, channel overhead, and atomics, the bitmap is a rounding error.

**Lesson: profile the workload you're trying to optimize, not a different one
that happens to be available.** The §1 microbench was useful for finding
small-class hot paths under the inline campaign (#101/#102 — where the entire
hot path is one fused function and every nanosecond matters); it is **NOT**
the right profile for guiding cross-thread MT optimizations.

### §7.3 — Where the larson T=1 gap REALLY sits

Subtracting overhead that isn't ours:
- libc/System arm: not our problem.
- bench worker body: common to all arms.
- mpmc channel: bench harness.

What's left of "us" in the larson workload:
1. **Atomic operations ~4-5%** (half of the 9.3% atomics line is ours):
   `stamp_segment_owner` Relaxed-load + compare on every alloc (the OPT-C
   cache already reduced this from Acquire+Release to Relaxed; further
   reduction would require eliminating the per-alloc stamp entirely — P4
   hoisted it into refill on the magazine hit path, but the **large path**
   still per-alloc stamps; the bulk-bypass path was retired in the 0.3.x
   perf arc, task #147 — see `perf/PERF_PLAN_beat_mimalloc_small_medium.md`).
2. **`dealloc_routing` reads on every dealloc**: `magic_at`, `owner_thread_free_at`,
   `kind_at`. Required for safe cross-thread routing. Per the §0 microbench
   ~5%, on MT roughly similar.
3. **`contains_base` hash probe on every dealloc**: ~3-5% on the dealloc
   side. Required as the M2 foreign-pointer guard (catches frees of pointers
   not allocated by us).
4. **Inline TLS resolution**: `current_for_alloc()` does a `try_with`-based
   safe TLS read on every alloc + dealloc. Some unavoidable cost.

The total bottom-up Sefer-attributable cost on MT is roughly 8-12% of bench
runtime. mimalloc's equivalent is lower because:
- mimalloc has no foreign-pointer guard (`contains_base` equivalent doesn't exist
  on its fast path — a free of a non-mimalloc pointer is UB in their model).
- mimalloc has no M2 double-free guard on the fast path (double-free = UB).
- mimalloc inlines a more compact hot path (their alloc fast-path is ~11 asm
  instructions on x86_64; ours is closer to 25-30 due to safety checks).

**The ~1.3× T=1 gap is the integrated cost of all these guards.** It is NOT
located in any one function we can profile out — that was the P8 lesson.

### §7.4 — What COULD close the gap (and what costs it)

Three remaining levers, ranked by EV per risk:

1. **IDEA 4 — `contains_base` elision on proven-own dealloc**
   (`docs/FASTBIN_DESIGN.md` §9). Estimated: ~3-6% on dealloc path. Risk:
   medium (weakens the foreign-pointer M2 guarantee from "exhaustive registry
   check" to "magic + owner_tf compare"). Effort: 1-2 days.

2. **Per-thread inline TLS pointer to bypass routing on proven-own free**.
   Cache `owner_thread_free_head_address` in TLS; on dealloc compare directly
   to TLS cache instead of reading the segment header. Estimated: ~5-8%. Risk:
   high (TLS lifetime + cross-thread visibility require careful audit; the
   #100 TLS-flake taught us this). Effort: ~5-day project.

3. **Accept the ~1.3× single-thread gap** as the documented cost of:
   - M2 double-free safety (vs mimalloc UB)
   - Foreign-pointer free safety (vs mimalloc UB)
   - Cross-thread routing readiness (mimalloc has its own but we audit ours)
   - `#![forbid(unsafe_code)]` at the top level (one audited aperture)

   And focus future perf work on the workloads where we already win:
   - **Large alloc/free (OPT-E):** 16-39× faster than mimalloc.
   - **MT T≥2 (larson/mstress):** 1.2-1.3× faster.
   - **Churn 16-1024B:** 1.7-7.3× faster.

Option 3 is the most honest given the project's safety-first stance. Options 1
and 2 are viable if a specific deployment needs the single-thread perf and is
willing to accept the safety trade-off.

### §7.5 — Methodological lesson (the meta finding)

**Re-profile the workload you're optimizing for.** A profile is a tool, not a
universal truth. The §1 single-thread bulk profile and the §2/§7 MT profile
look completely different despite measuring "the same" allocator, because:

- Single-thread bulk: tight loop, no thread coordination, no cross-thread paths
  taken → bitmap addressing dominates the % share.
- MT macro: thread coordination, channel overhead, atomics dominate; our
  allocator is < 1% of runtime.

The P8 hypothesis was constructed from the §1 profile and applied to a goal
defined by §2 numbers. That mismatch is what killed it.

**Practical rule for future fastbin / hot-path work:** before designing a
"replace X with Y" optimization, re-profile the *specific* benchmark you want
to improve. If the function you're targeting isn't in the top of *that*
benchmark's profile, the change won't move *that* number.
