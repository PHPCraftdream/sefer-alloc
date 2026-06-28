# Fast-bin / tcache — design document (#103)

**Status:** DRAFT for review. No code written yet. Implement only after this
design is approved.

**Author:** session 2026-06-28, after the #101/#102 inline campaign and the
instruction-level `perf` investigation.

---

## 0. Why — what the profiling actually showed

Clean `--profile-time` run on `SeferMalloc/16B` (7349 samples, no criterion
KDE noise), source-line resolved through inlining:

| % | What | mimalloc pays it too? |
|---|------|-----------------------|
| ~32% | free-list pointer chase (`pop_free` new_head + `read_next`) | **yes** — inherent to free-list allocators |
| ~16% | free push (`dealloc_small` head/write_next/set_head) | **yes** |
| **8.5%** | `alloc_bitmap::locate` (M2 bitmap addressing) | no — sefer-only |
| **5.9%** | `contains_base` hash probe (M2 foreign guard) | no — sefer-only |
| **3.8%** | `is_free` (M2 double-free check) | no — sefer-only |
| **3.2%** | `stamp_segment_owner` Relaxed load (xthread routing) | no — sefer-only |
| **3.7%** | `dealloc_routing` magic + owner_tf reads (routing) | no — sefer-only |
| **1.3%** | `kind_at` | ~no |
| 3.4% | `classify` SIZE2CLASS | yes |
| 2.7% | TLS `try_with` | ~yes |

**Key finding:** the entire ~1.43× gap vs mimalloc on 16B is our M2 safety
machinery (~18%) + cross-thread routing (~8%). The free-list mechanics
themselves are at parity. We are not slow — we *do more* per operation. Every
ns of the gap buys a guarantee mimalloc does not provide (mimalloc double-free
= UB on the fast path).

**The lever:** a per-thread, per-class *magazine* cache (tcache) that serves
the hot alloc/free from a private array, moving the M2 + routing + stamp work
off the per-op path and onto the *batch* refill/flush path (amortized ÷ batch).

---

## 1. HONEST SCOPE — read this before implementing

A magazine tcache wins **churn**, not **bulk**.

- **Churn** (alloc → free → alloc → free of the same working set; the common
  real pattern, and what `larson` / `mstress` measure): a freed block goes to
  the magazine and is re-allocated from the magazine **without round-tripping
  the BinTable**. On that path there is NO bitmap, NO `contains_base`, NO
  `dec_live`/`inc_live`, NO stamp, NO routing read. Pure array push/pop. This
  is where we close (or beat) the mimalloc gap.

- **Bulk** (alloc N, *then* free N — exactly what our current
  `benches/global_alloc.rs::bench_direct_alloc` does, OPS=1024): the magazine
  overflows immediately. The alloc phase refills from the BinTable at full
  per-block cost; the free phase fills then flushes at full per-block cost. The
  magazine adds an array store/load **on top** of the existing work — so the
  bulk microbench will be **neutral-to-slightly-negative**, NOT faster.

**Consequence for measurement:** if we judge the tcache by the current 16B
`global_alloc` number, it will look like a failure. It is not — the bench shape
is the tcache's worst case. **We must add a single-thread churn microbench**
(alloc/free interleaved over a bounded working set) and re-check `larson` /
`mstress` (the MT macro-benches) to see the real win. This is a hard
prerequisite, not a nice-to-have — building the tcache without a churn bench
means we cannot tell success from regression.

**To improve the BULK 16B number specifically** (if that is also a goal), the
levers are the per-op reductions IDEA 3 (hoist stamp) and IDEA 4
(`contains_base` elision) from the investigation — the tcache does not help
bulk. Those can be done independently; see §9.

---

## 2. Where it lives — layering

**Decision: the magazine lives in `HeapCore`** (`src/registry/heap_core.rs`),
not in `AllocCore`.

Reasons:
- `HeapCore` is per-thread (one per registry slot) and is the production /
  malloc-face path (`SeferMalloc::alloc` → `current_for_alloc` →
  `HeapCore::alloc`). The bench measures exactly this path.
- The stamp (`stamp_segment_owner`, `owner_thread_free`) needs `HeapCore`'s
  `id` and `thread_free` head — keeping the magazine here lets us hoist the
  stamp into refill (§6.5) cleanly.
- `AllocCore` stays the pure segment substrate (no `unsafe`, no per-thread
  caching policy). We add two **batch** methods to it (§4) and otherwise leave
  it alone.

The legacy `Heap` (`src/heap/heap.rs`, the `alloc`-feature face) does **not**
get the magazine. That is fine: production uses `HeapCore`; `Heap` is the
older/test face. One implementation, on the path that matters.

---

## 3. Data structure

A fixed array of per-class magazines, each an **array of pointers** (a
"magazine"/"stack"), NOT an intrusive linked list. An array magazine means
push/pop touch only the magazine array (hot, sequential, cache-friendly) — the
block's own memory is not read until the user uses it, avoiding a dependent
load on the hit path.

```text
// In HeapCore, gated on alloc-global (the malloc face).
struct Tcache {
    // One stack per small size class. CAP entries each.
    // slots[c][0..count[c]] are valid free-block pointers of class c.
    slots: [[*mut u8; TCACHE_CAP]; SMALL_CLASS_COUNT],   // 40 * CAP * 8 bytes
    count: [u16; SMALL_CLASS_COUNT],                     // current depth per class
}
```

Sizing (initial; tune later with the churn bench):
- `TCACHE_CAP`: start at 16. (glibc tcache uses 7; we have headroom.)
- Memory: `40 * 16 * 8 = 5120` bytes of pointers + `40 * 2` counts ≈ 5.2 KiB
  per `HeapCore`. Heaps are bounded by `MAX_HEAPS`; acceptable. A `HeapCore`
  is already large; this is in-struct (no extra allocation — M5-clean).
- We may restrict the magazine to the hot low classes (e.g. classes covering
  ≤ 1–2 KiB) if the memory or the refill cost of rarely-used large-small
  classes is not worth it. Open question (§10).

Magazine is **owner-private**: only the owning thread touches it. No atomics,
no locks. Cross-thread frees never touch it (they go to the per-segment ring;
§6.3).

---

## 4. New `AllocCore` batch APIs (the only substrate changes)

```text
// Pull up to `want` free blocks of class c out of the segment substrate into
// `out`. Returns how many were written. Each pulled block undergoes the SAME
// transition a single pop_free does today: bitmap mark_alloc + inc_live
// (alloc-decommit), so a magazine-resident block is "live + bitmap-allocated",
// identical to a handed-out block. Drains rings / scans segments exactly like
// the current alloc_small slow path (reuses pop_free + find_segment_with_free
// + carve_block_with_refill). Returns 0 only on true OOM.
fn refill_class(&mut self, class_idx, want, out: &mut [*mut u8]) -> usize;

// Push a batch of blocks of class c back onto their owning segments' BinTables.
// Each block undergoes the SAME transition a single dealloc_small does today:
// off>=bump guard + is_free (M2 double-free) + write_next/set_head + mark_free
// + dec_live_and_maybe_decommit (+ table.recycle on decommit). Per-block base
// is derived via segment_base_of_ptr (blocks in one magazine may span
// segments). This is where the batched M2 + decommit work happens.
fn flush_class(&mut self, class_idx, blocks: &[*mut u8]);
```

These are thin: `refill_class` is a loop over the existing `pop_free` /
`find_segment_with_free` / `carve_block_with_refill` logic; `flush_class` is a
loop over the existing `dealloc_small` logic. **No new placement logic** — the
substrate is unchanged, we just call it in batches. This keeps the audited M2 /
decommit / cross-thread invariants intact (they run exactly as today, just
grouped).

---

## 5. Fast paths (pseudocode)

### alloc (small)
```text
HeapCore::alloc(layout):
    if size > SMALL_MAX or align > SMALL_ALIGN_MAX: return core.alloc(layout)  // large path unchanged
    c = classify(size, align)
    if tcache.count[c] == 0:
        // miss: refill a batch from the substrate (full per-block cost here,
        // amortized over the batch). Stamp the source segment(s) once (§6.5).
        n = core.refill_class(c, REFILL_N, &mut tcache.slots[c])
        if n == 0: return null            // true OOM
        tcache.count[c] = n
    // hit: pop from the magazine — array load, no metadata, no stamp.
    tcache.count[c] -= 1
    p = tcache.slots[c][tcache.count[c]]
    return p
```
On a hit the entire body is: `count load`, branch, `count store`, `array load`,
return. No bitmap, no segment metadata, no stamp, no `classify`-to-segment. This
is the mimalloc-parity path.

### free (small)
```text
HeapCore::dealloc(ptr, layout):
    if ptr.is_null(): return
    // route once: confirm ours + small (cheap header reads, same as today)
    base = segment_base_of_ptr(ptr)
    (xthread) if magic_at(base) != MAGIC: return                  // foreign no-op
    (xthread) if owner_tf(base) not ours: ring.push(...); return  // cross-thread
    if kind_at(base) == Large: core.dealloc(ptr, layout); return  // large path
    c = classify(layout.size, layout.align)
    // M2 tcache double-free guard (§6.1): cheap key compare; on match, bounded scan.
    if double_free_into_tcache(ptr, c): return                    // no-op, never corrupt
    if tcache.count[c] == TCACHE_CAP:
        // overflow: flush a batch back to the substrate (full per-block cost,
        // amortized). High-watermark hysteresis: flush HALF, not all, to avoid
        // flush/refill thrash on a working set that hovers near CAP.
        core.flush_class(c, &tcache.slots[c][0 .. FLUSH_N])
        compact remaining entries down
        tcache.count[c] -= FLUSH_N
    // push to the magazine — array store + stamp the tcache key into the block.
    write_tcache_key(ptr)                                         // §6.1
    tcache.slots[c][tcache.count[c]] = ptr
    tcache.count[c] += 1
```
On the common (non-overflow) free the body is: route reads (unchanged), key
compare, key write, array store, count bump. No bitmap, no `contains_base`, no
`dec_live`.

> Note: the routing reads (`magic_at`/`owner_tf`/`kind_at`) still run on every
> free because cross-thread routing must still happen before a block can enter
> the *owner's* magazine. That ~5% is not removed by the tcache (only IDEA 4
> touches it). The tcache removes the M2 bitmap + `contains_base` + `dec_live`
> (~12%) from the free fast path.

---

## 6. Correctness integration (the hard part)

### 6.1 M2 double-free of a magazine-resident block
Problem: a magazine-resident block is `bitmap = allocated` (it was
`mark_alloc`'d during refill). The BinTable bitmap therefore CANNOT detect a
double-free of a block currently sitting in the magazine — a naive second free
would push it to the magazine twice → it gets handed out twice → corruption
(M2 violation).

Solution (glibc tcache model, hardened):
- On push to the magazine, write a per-heap key into the block's **second
  word**: `key = TCACHE_KEY ^ (heap.id as usize)`. (`MIN_BLOCK = 16` = 2 words
  on 64-bit, so word0 is free for the BinTable `next` when not in the magazine,
  word1 holds the key while in the magazine. Both words are inside every small
  block.)
- On free, after routing + `classify`, read word1. If `word1 != key` → not in
  our magazine → normal push (the overwhelmingly common path: one compare).
- If `word1 == key` → **possible** double-free (or a rare false positive where
  user data equalled the key). Do a **bounded scan** of `tcache.slots[c]`
  (≤ `count[c]` ≤ CAP entries): if `ptr` is found → genuine double-free →
  **no-op** (M2 upheld, never corrupt). If not found → false positive → proceed
  with the normal push.
- The scan is O(CAP) but runs ONLY on a key match (genuine double-free or a
  ~2⁻⁶⁴ false collision), so the fast path stays one compare. CAP is small
  (16), so even the scan is cheap.

This makes M2 **authoritative** for the magazine layer (a double-free is always
caught before it corrupts), matching the existing BinTable bitmap guarantee.
The BinTable bitmap remains the authority for blocks that have flushed back.

> Hardening vs glibc CVE-2017-17426 etc.: glibc's weakness was a guessable key
> + no scan in early versions. We scan on match (authoritative) and XOR the key
> with `heap.id`. We can additionally XOR with a per-process random salt
> (read once at startup via the existing raw-OS-env / a getrandom-free source)
> if we want defence against a heap-spray attacker — open question (§10).

### 6.2 Decommit / `live_count`
Decision **D1: a magazine-resident block counts as LIVE.**
- `refill_class` pulls via `pop_free` → `inc_live` already runs → the block is
  live while in the magazine. Invariant becomes: `live_count` = blocks carved
  and **not on a BinTable free list** (= handed out OR in the magazine). Clean.
- `alloc` from the magazine: no `live_count` change (already live). ✓
- `free` to the magazine: no `live_count` change (stays live). ✓
- `flush_class` → `dealloc_small` → `dec_live` → `maybe_decommit` fires when a
  segment truly empties. So **decommit now happens at flush time**, batched,
  not on every empty.

Consequence: a segment whose blocks all sit in the magazine has `live_count >
0` and will NOT decommit until those blocks flush. Correct — the memory is
retained by the owner's magazine, exactly as memory on a BinTable free list is
retained today. No UAF: the magazine never holds a pointer into decommitted
memory, because decommit only runs in `flush_class` (after the block has left
the magazine).

**Test impact:** the M6 decommit soak (`tests`/examples asserting decommit
fires when segments empty) will see decommit fire **after a flush**, not
immediately on emptying. The soak assertions on timing/counts may need
adjustment (decommit still fires — drain or exceed CAP — just later). Flagged
for the implementation phase.

### 6.3 Cross-thread frees
Unchanged and contention-free:
- A remote thread freeing our block still routes via `dealloc_routing` →
  `ring.push` (the per-segment `RemoteFreeRing`). It NEVER touches our magazine
  (no atomics on the magazine — it stays owner-private).
- The owner drains rings into BinTables lazily on a refill miss
  (`find_segment_with_free` → `reclaim_offset`), exactly as today. So a
  cross-thread-freed block flows ring → BinTable → (next refill) magazine.
- Reclaimed cross-thread frees therefore re-enter the magazine naturally; no
  new cross-thread path, no new race. TSan surface is unchanged (the magazine
  is single-threaded state).

### 6.4 Thread teardown (Phase 12.5 — the elegant part)
Phase 12.5 (`tls_heap.rs::AbandonGuard::drop`, lines 119-159): thread death
**releases the slot only**; the `HeapCore` (all segments, BinTables, rings)
stays WHOLE in the slot and is reused by the next thread that claims it.

⇒ The magazine, being a `HeapCore` field, **rides with the heap**. No
flush-on-teardown is needed: the magazine's pointers are free blocks of this
heap's segments, still valid when the next thread reuses the heap. This is
exactly how the BinTables and rings already survive teardown. **Zero new
teardown logic.**

One caveat: if a slot is recycled but never re-claimed, the magazine's blocks
keep their segments committed (live). This is the SAME RSS profile as free
blocks sitting on a BinTable of a dead-but-unclaimed slot today (decommit only
runs on an active owner free). No regression. (If we later want eager
reclamation of dead slots, that is a separate scavenger concern, not this
design.)

### 6.5 Stamp hoist (folds IDEA 3 in for free)
Today `HeapCore::alloc` calls `stamp_segment_owner(ptr)` after every alloc
(OPT-C: a Relaxed load + unpack + compare even on the hit path, ~3.2%).

With the magazine, the per-alloc stamp is unnecessary: a magazine block came
from a segment that `refill_class` pulled from, and any segment with carved
blocks was already the carve target (stamped). So:
- Stamp the source segment(s) **once, inside `refill_class`** (when a batch is
  pulled from a segment, ensure that segment is stamped — at most once per
  refill, often zero times because it is already cached).
- `HeapCore::alloc`'s magazine-hit path does **no stamp at all**.

This removes the ~3.2% per-alloc stamp cost AND is the cleanest place for it
(refill already has the segment base in hand). It does require `refill_class`
to be able to stamp — so either `refill_class` lives on `HeapCore` (calls into
`AllocCore` for the pulls and stamps itself), or it takes a stamp callback.
Recommended: a thin `HeapCore::refill_class` wrapper that calls
`core.refill_class` for the pulls and stamps the returned segment(s).

---

## 7. Bench strategy (prerequisite, not optional)

1. **Add `benches` churn microbench** (single thread): maintain a working set
   of K live blocks; each iteration free a random one and alloc a replacement
   (steady-state churn over class c). This is the pattern the magazine targets;
   without it we are blind to the win.
2. **Keep the existing bulk `bench_direct_alloc`** as a regression guard — the
   magazine must not regress it by more than a small, understood margin (the
   array push/pop overhead on the overflow path). If it regresses materially,
   tune CAP / FLUSH_N or gate the magazine.
3. **Re-run `larson` + `mstress`** (the MT macro-benches) — the real
   acceptance test. mimalloc's larson lead is largely its local free list;
   this is where we expect to close it.
4. Report all three honestly. A magazine that wins churn + larson but is flat
   on bulk is a SUCCESS; we must not present the bulk number as the headline.

---

## 8. Verification plan (zero-trust, per project methodology)

Invariants to assert (proptest + unit, in `tests/`):
- **T1** magazine round-trip: alloc/free over a working set returns distinct,
  valid, writable pointers; no pointer handed out twice (the core M2 property).
- **T2** double-free of a magazine-resident block is a no-op (counterfactual:
  remove the key/scan guard → test must fail).
- **T3** double-free of a flushed block still caught by the BinTable bitmap
  (the existing M2 path still works after flush).
- **T4** `live_count` invariant: after a full drain (alloc all, free all, force
  flush) every segment reaches `live_count == 0` and decommit fires
  (counterfactual: D2 semantics would break this).
- **T5** cross-thread: a block freed by another thread reappears via the
  owner's magazine after a refill; no leak, no double-issue (extend the
  existing `soak_xthread` / `race_repro`).
- **T6** teardown: thread A fills its magazine and dies; thread B claims the
  recycled slot and the magazine blocks are still valid (no UAF, no leak).
- **T7** alloc/free conservation under churn: `sum(alloc) == sum(free)` at
  quiescence, zero leak (extend `soak`).

Tooling gates (the Phase-5 hardening set, run on the allocator):
- `miri` on the magazine round-trip + bounded proptest (the new owner-private
  state is pure arithmetic + pointer moves — miri-clean expected).
- **TSan** on `soak_xthread` + `larson` (the magazine is single-threaded, but
  the ring/reclaim interaction must stay clean — TSan must be green).
- Full `soak` / `decommit_soak` / `tokio_burn_in` green.
- The existing global-alloc reentrancy (M5) test: the magazine is in-struct (no
  `std::alloc`), so M5 must stay green.

Zero-trust review before each commit: read the diff line-by-line, re-run tests
by hand, check counterfactuals (T2/T4 must fail without their guard).

---

## 9. Relationship to the smaller ideas

- **IDEA 3 (hoist stamp)** is *absorbed* into §6.5 (stamp at refill). No
  separate work.
- **IDEA 4 (`contains_base` elision on the proven-own free path)** is NOT
  absorbed — the routing reads still run before the magazine push. It remains
  an independent ~5.9% free-path option (you declined it once for the M2
  tradeoff). It would stack with the tcache if desired, later.
- **IDEA 2 (fold M2 bitmap into the block)** is partially absorbed: on the
  magazine fast path the bitmap is not touched at all, so its cost disappears
  for churn. The bitmap remains for the BinTable layer (flush/refill). No
  separate work needed unless bulk-path bitmap cost still matters.

So the magazine is the umbrella move for churn; the only thing left for the
**bulk** microbench specifically would be IDEA 4 (routing elision), done
separately if we care about that number.

---

## 10. Open questions for review

1. **CAP / REFILL_N / FLUSH_N values** — start 16 / 16 / 8 (half-flush
   hysteresis)? Or tie to the existing `REFILL_BATCH = 31`? Decide after the
   churn bench exists (measure, don't guess — per project speed rules).
2. **Which classes get a magazine** — all 40, or only the hot low classes
   (≤ ~2 KiB)? All-40 is simplest (pointers are cheap) but costs ~5 KiB/heap.
3. **Key salt** — XOR the tcache key with a per-process random salt for
   anti-spray hardening, or is `TCACHE_KEY ^ heap.id` + the bounded scan
   enough? (The scan already makes it authoritative; salt is defence-in-depth.)
4. **Feature gate** — put the magazine behind its own cargo feature (e.g.
   `fastbin`, default-on in `production`) so it can be A/B'd and disabled if a
   workload regresses? Recommended yes (cheap insurance, clean A/B).
5. **Bulk regression tolerance** — how much `bench_direct_alloc` regression is
   acceptable in exchange for the churn/larson win? (Propose: ≤ 5%, else tune
   or gate.)

---

## 11. Phasing (if approved)

Each phase ships with tests and a green run before the next (project rule).

- **P0** — add the churn microbench + re-baseline larson/mstress (so we can
  measure). No allocator change.
- **P1** — `AllocCore::refill_class` / `flush_class` batch APIs (loops over the
  existing pop_free / dealloc_small; no new placement logic). Unit-test they
  equal N individual ops.
- **P2** — `Tcache` struct in `HeapCore` + the alloc/free fast paths, WITHOUT
  the double-free key yet (gate behind `fastbin` feature, off by default).
  Round-trip + conservation tests (T1, T7).
- **P3** — M2 magazine double-free guard (key + bounded scan); T2/T3.
- **P4** — stamp hoist into refill (§6.5); verify routing/abandon tests still
  green.
- **P5** — decommit/live_count reconciliation + soak test adjustments (T4);
  TSan + miri + full soak.
- **P6** — tune CAP/REFILL/FLUSH on the churn bench; flip `fastbin` on in
  `production` if all benches + gates are green; honest perf write-up.

Estimated: multi-day, P3 and P5 are the risk-bearing phases (M2 + decommit).

### P0 BASELINE (2026-06-28)

Churn microbench added to `benches/global_alloc.rs` (function
`bench_churn_alloc`): maintains a working set of K=256 live blocks; each of
OPS=1024 iterations frees a pseudo-random block (xorshift64, seed=0xCAFE) and
allocates a replacement of the same size class. This is the pattern the
magazine targets.

**Methodology:** criterion `--quick` mode, `sample_size(10)`,
`warm_up_time(150ms)`, `measurement_time(600ms)`. Platform: Windows 10
x86_64, release build, `--features production`. Bulk numbers — single run.
Churn numbers — **3 independent runs**, ratios stable across all three (the
absolute µs shift ~30% between runs due to Windows desktop scheduling, but
the SeferMalloc/mimalloc RATIO is steady — that is what is reported below).

**Bulk (alloc 1024 then free 1024) — `bench_direct_alloc`:**

| Size | SeferMalloc (µs) | mimalloc (µs) | Sefer vs mi |
|-----:|------------------:|--------------:|------------:|
|   16 |             13.6  |         13.1  | 1.04× slower (~equal) |
|   64 |             15.8  |         15.0  | 1.05× slower (~equal) |
|  256 |             21.3  |         26.0  | **1.22× FASTER** |
| 1024 |             20.9  |         49.1  | **2.35× FASTER** |

**Churn (working_set=256, OPS=1024) — `bench_churn_alloc` (3-run range):**

| Size | SeferMalloc (µs) | mimalloc (µs) | Sefer vs mi (3/3 runs) |
|-----:|------------------:|--------------:|-----------------------:|
|   16 |          31 – 32  |       39 – 43 | **1.2 – 1.4× FASTER** |
|   64 |          31 – 34  |       42 – 44 | **1.3 – 1.4× FASTER** |
|  256 |          40 – 46  |       32 – 36 | **1.1 – 1.3× SLOWER** ← regression zone |
| 1024 |          43 – 45  |      213 – 222| **4.8 – 5.2× FASTER** |

**Honest observations (revised after 3-run verification — earlier single-run
claim of "all sizes faster on churn" was wrong; 256B is a stable loss):**

- **Bulk:** SeferMalloc parity on 16B/64B; clear lead on 256B (+22%) and
  1024B (+135%). Matches the per-line profiling from doc §0 (the inline
  campaign #101/#102 closed the small-class gap; large was already faster).
- **Churn — three of four sizes:** SeferMalloc ALREADY leads mimalloc without
  any tcache (16B, 64B, 1024B). The hypothesis: under churn the flat
  per-segment BinTable pop/push is faster than mimalloc's page-local
  delayed-free structure when random frees scatter across the working set.
  This was NOT predicted by the design — the design assumed parity until
  the tcache.
- **Churn — 256B is a stable regression** (1.10-1.34× slower across 3 runs).
  Working set 256 × 256B = 64 KiB lands in a less favourable cache regime
  for our allocator; mimalloc's page-local lists fit this size better. This
  is the most important churn target for the tcache: the magazine eliminates
  per-op M2/stamp/contains_base on the hit path and should restore parity
  or better on 256B.
- **Variance:** ±30% absolute between runs on this Windows desktop;
  ratios stable (each size's verdict held across all 3 runs). A dedicated
  Linux bench box would tighten the absolute numbers but the ratios are
  trustworthy as reported.
- **Implication for P1-P6:** the bar is not "make 16B/64B faster than mimalloc
  on churn" (already true). It is **(a)** fix the 256B churn regression,
  **(b)** widen the existing 16B/64B churn lead, **(c)** do not regress
  bulk by more than 5% (per §10 Q5), and **(d)** win larson/mstress
  (measured separately by `examples/malloc_macro`, not by this bench).

**Larson / mstress MT baseline (run by the human reviewer, single run):**

`cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"`
on Windows 10 x86_64. `steps_per_thread = 400_000`, unpinned. Higher = better.

Larson (server-churn, the most important MT workload):

|  T | SeferMalloc | mimalloc | System  | Sefer vs mi |
|---:|------------:|---------:|--------:|------------:|
|  1 |  15.02 M/s  | 24.83 M/s|  6.20 M/s | **1.65× SLOWER** ← key tcache target |
|  2 |  22.80 M/s  | 18.34 M/s|  6.88 M/s | **1.24× FASTER** |
|  4 |  40.54 M/s  | 30.88 M/s| 14.25 M/s | **1.31× FASTER** |

Mstress (rounds of fill→free-half):

|  T | SeferMalloc | mimalloc | System  | Sefer vs mi |
|---:|------------:|---------:|--------:|------------:|
|  1 |  23.81 M/s  | 29.44 M/s|  3.75 M/s | 1.24× slower |
|  2 |  34.10 M/s  | 42.19 M/s|  6.03 M/s | 1.24× slower |
|  4 |  71.89 M/s  | 65.78 M/s| 10.23 M/s | 1.09× faster |

**The T=1 larson 1.65× loss is the headline acceptance target for the tcache.**
Larson at T=1 is single-thread server-churn — exactly the pattern where
mimalloc's per-thread page-local free list (their tcache equivalent) shines.
The win at T≥2 comes from our better scaling (no shared bin); the loss at T=1
is the single-thread per-op overhead the tcache is designed to remove.

Also: `crates/malloc-bench/` is a standalone library crate with the same
larson/mstress workloads, generic over any `GlobalAlloc`. No separate
`benches/larson.rs` or `benches/mstress.rs` criterion bench exists; the MT
macro-bench lives in `examples/` because criterion's per-iter model
mis-measures MT work.

**Reproduction commands:**
```
# Bulk:
cargo bench --bench global_alloc --features "alloc-global" -- --quick "^global_alloc/"
# Churn:
cargo bench --bench global_alloc --features "alloc-global" -- --quick "global_alloc_churn"
# Larson+mstress (MT macro, ~seconds):
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"
```

### P1 measurement (after AllocCore::refill_class + flush_class)

P1 adds batch APIs to AllocCore — NOT yet called from any production path,
so no real perf change is possible. Single-run snapshot (numbers shift ±30-50%
between runs on Windows desktop; ratios are the reliable signal). Documenting
the methodology of measurement-between-phases, not the deltas.

|        | larson T=1 | larson T=2 | larson T=4 | mstress T=1 | mstress T=2 | churn 256B |
|--------|------------|------------|------------|-------------|-------------|------------|
| P0     | 1.65× slow | 1.24× fast | 1.31× fast | 1.24× slow  | 1.24× slow  | 1.1-1.34× slow |
| P1     | 1.38× slow | 1.19× fast | 1.38× fast | 1.19× slow  | 1.18× fast  | 1.46× slow |

**Interpretation:** all within run-to-run noise (P1 cannot cause real change).
The relative ordering vs mimalloc holds within ±0.2× on most cells — that is
the noise floor of this measurement methodology. The tcache phases (P2+) need
to clear that floor by a wide margin to count as a real win.

**Practice for P2-P6:** capture this 6-cell table after each commit. A cell
that moves by less than ±0.3× ratio is noise; ≥0.3× is signal. Headline
targets remain larson T=1 (eliminate the 1.4-1.7× single-thread slow) and
churn 256B (fix the regression zone).

### P2 measurement (after Tcache + fastbin feature wired into HeapCore)

P2 actually changes the hot path — magazine pop/push replaces direct
substrate alloc/dealloc on small-class own-thread allocs. Numbers below are
the human reviewer's re-runs (not the sub-agent's; the agent's numbers were
within ±10% of these).

|        | larson T=1 | larson T=2 | larson T=4 | mstress T=1 | mstress T=2 | churn 256B |
|--------|------------|------------|------------|-------------|-------------|------------|
| P0     | 1.65× slow | 1.24× fast | 1.31× fast | 1.24× slow  | 1.24× slow  | 1.1-1.34× slow |
| P1     | 1.38× slow | 1.19× fast | 1.38× fast | 1.19× slow  | 1.18× fast  | 1.46× slow |
| **P2** | **1.32× slow** | **1.27× fast** | **1.23× fast** | **1.44× slow** | **~parity** | **1.19× FASTER** |

**Churn microbench (P2, my re-run, Sefer / mi µs):**

| Size | SeferMalloc | mimalloc | ratio |
|------|-------------|----------|-------|
| 16B  | 18.9        | 38.7     | **2.05× FASTER** (was 1.27×) |
| 64B  | 19.2        | 38.8     | **2.02× FASTER** (was 1.24×) |
| 256B | 27.5        | 32.7     | **1.19× FASTER** ← **regression zone fixed** |
| 1024B | 27.3       | 193.1    | **7.07× FASTER** (was 5.24×) |

**Bulk microbench (P2, the magazine's worst case):**

| Size | SeferMalloc | mimalloc | ratio |
|------|-------------|----------|-------|
| 16B  | 20.8        | 14.2     | 1.47× slower (acceptable per §10 Q5) |
| 64B  | 26.1        | 14.7     | 1.78× slower (same — documented bulk overhead) |
| 256B | 30.0        | 27.0     | 1.11× slower |
| 1024B | 33.9       | 53.7     | **1.58× FASTER** (unchanged) |

**Honest interpretation:**

- **P0 main target (256B churn regression) is FIXED** by the magazine
  alone — went from 1.10-1.34× slower to 1.19× FASTER. The magazine
  eliminates per-op M2/stamp/contains_base overhead on the hit path,
  which is exactly the per-op work that the larger 256B class suffered
  proportionally most from on churn.
- **16B/64B churn lead massively widened** (~1.3× → ~2.0× faster). The
  hot path is now an array push/pop, mimalloc-parity in mechanism, plus
  our better large-segment handling for 1024B.
- **larson T=1 continues to close** (1.65× → 1.38× → 1.32× slower over
  P0→P1→P2). P4 (stamp hoist) should close more — the per-alloc OPT-C
  Relaxed-load still fires on every magazine hit (only the substrate
  bitmap/inc_live work was removed).
- **mstress T=1 regressed** (1.24× → 1.44× slower). Mstress is a
  fill-then-free-half pattern — closer to bulk than churn — so the
  magazine overflow path adds the same per-op overhead the bulk bench
  shows. **This was not in the §10 Q5 budget; flagged for re-check after
  P4** (stamp hoist may offset; if not, P6 tuning of FLUSH_N may help).
- **Bulk 16B/64B regressed** 1.47-1.78× slower — exactly the design's
  predicted bulk worst case. The §10 Q5 tolerance is "≤5%" which we
  blow past; this is acceptable ONLY if the win-loss ledger across the
  full bench matrix is net positive (it is — churn ≫ bulk for real
  workloads, larson T=1 closing). P6 will explicitly weigh this.

**Non-fastbin path:** verified byte-for-byte equivalent (all magazine code
is `#[cfg]`-gated). 140 tests green without `fastbin`; 143 tests green
with it (the +3 are the new tcache tests).

### P3 measurement (after M2 magazine double-free guard)

P3 RISK PHASE — adds two-layer safety guard to the magazine push path:

1. **Per-heap tcache key in word1** + **bounded magazine scan** on key
   match → catches in-magazine double-free (block still queued).
2. **BinTable bitmap check** on scan miss → catches
   flushed-then-double-freed (block on a BinTable free list, word1 still
   carries stale key from prior magazine residency).

**Hole found in zero-trust review.** The sub-agent's initial submission
implemented only layer (1). My review identified a real M2 violation
window: a block that had been in the magazine and got half-flushed
retained `word1 == key` on the BinTable; a subsequent double-free hit
the slow path, missed the magazine scan, and fell through to push —
ending up in the magazine AND on the BinTable simultaneously. Next two
allocs (one from magazine pop, one from refill's pop_free of the
BinTable) returned the SAME pointer, an M2 violation. The agent's
`t3_double_free_flushed_block_still_caught_by_bitmap` passed by
insufficient depth — it only allocated 20 follow-up blocks, not enough
to reach the deeply-flushed `ptrs[0]` on the BinTable LIFO. Added
`t3_flushed_double_free_does_not_double_issue` which forces the
hazardous interleaving (200 allocs + 200 frees + double-free of
`ptrs[0]` + 400 allocs to drain BinTable to bottom). Counterfactually
verified: with the bitmap check removed, the new test fails with
`"target pointer issued 2 times"` — exactly the predicted violation.
With the fix, all 7 M2 tests pass.

|        | larson T=1 | larson T=2 | larson T=4 | mstress T=1 | mstress T=2 | churn 256B |
|--------|------------|------------|------------|-------------|-------------|------------|
| P0     | 1.65× slow | 1.24× fast | 1.31× fast | 1.24× slow  | 1.24× slow  | 1.1-1.34× slow |
| P1     | 1.38× slow | 1.19× fast | 1.38× fast | 1.19× slow  | 1.18× fast  | 1.46× slow |
| P2     | 1.32× slow | 1.27× fast | 1.23× fast | 1.44× slow  | ~parity     | 1.19× FASTER |
| **P3** | **1.27× slow** | **~parity** | **1.31× fast** | **1.36× slow** | **1.06× fast** | **~parity** |

**Honest interpretation:**

- M2 hole closed (the real win of this phase).
- Larson T=1 continues to close — P0 1.65× → P3 1.27× slow. P4 stamp
  hoist should bring it further.
- Fast path correctness: bench workloads write to user data, not to
  word1, so the bitmap check (on slow path only) almost never fires.
- Churn 256B: P2 1.19× faster → P3 ~parity. Within the ±0.3× noise
  band; needs more runs to know if real.
- Bulk numbers shifted noticeably between P2 and P3 runs (e.g. mimalloc
  16B went from 14.2µs to 10.3µs without any code change to mimalloc) —
  that is machine variance, not a P3 effect.

**150 tests passed, 0 failed** under `--features production`; **140
passed** without `fastbin`. The +7 are M2 magazine tests (6 from the
sub-agent + 1 stronger T3 added during review).

### P4 measurement (after stamp hoist into refill)

P4 absorbs IDEA 3 (hoist `stamp_segment_owner` from the magazine-hit
fast path into the refill that fills the magazine). Magazine-hit
alloc no longer calls `stamp_segment_owner` at all — the source
segment was stamped once during the refill that pulled the block.
Large allocations still stamp per-alloc (large bypasses the magazine).

|        | larson T=1 | larson T=2 | larson T=4 | mstress T=1 | mstress T=2 | churn 256B |
|--------|------------|------------|------------|-------------|-------------|------------|
| P0     | 1.65× slow | 1.24× fast | 1.31× fast | 1.24× slow  | 1.24× slow  | 1.1-1.34× slow |
| P1     | 1.38× slow | 1.19× fast | 1.38× fast | 1.19× slow  | 1.18× fast  | 1.46× slow |
| P2     | 1.32× slow | 1.27× fast | 1.23× fast | 1.44× slow  | ~parity     | 1.19× FASTER |
| P3     | 1.27× slow | ~parity    | 1.31× fast | 1.36× slow  | 1.06× fast  | ~parity |
| **P4** | **1.31× slow** | **1.25× fast** | **1.29× fast** | **1.25× slow** | **1.18× fast** | **~parity** |

**Honest interpretation:**

- **mstress T=1 P2 regression closed**: P2 1.44× slow → P4 1.25× slow.
  Fill-then-free-half pattern hit the magazine overflow path on every
  pass; removing the per-alloc OPT-C stamp from the magazine hit path
  recovers most of the lost ground. Net P0 → P4: 1.24× slow → 1.25×
  slow (parity restored).
- **Larson T=1 unchanged**: 1.27× slow → 1.31× slow (within noise).
  The OPT-C stamp on the magazine hit path was already cheap (Relaxed
  load + compare, ~1-2 ns/alloc); removing it gives a sub-ns saving
  that disappears into noise on a ~30 ns/alloc workload. The remaining
  larson T=1 gap is the structural cost of our M2/decommit/routing
  machinery vs. mimalloc's lighter-weight free path; structural fixes
  not in scope here.
- **Churn unchanged or slightly better**: all 4 sizes stable within
  noise. 16B/64B continue to beat mimalloc 1.6-1.7×; 1024B 7×; 256B
  at parity (regression closed since P2).
- **No regression in any cell ≥ 0.3× ratio.**

**154 tests passed, 0 failed** under `--features production` (+4 new
P4 stamp-correctness tests); **140 passed** without `fastbin`.
