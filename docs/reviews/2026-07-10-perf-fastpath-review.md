# Performance review — small/medium fast-path alloc/free (2026-07-10)

**Scope:** cold-direct alloc/free fast path for 16B–256B blocks. **Method:**
fxx (Fable-5, effort=max) research agent, read-only investigation of
`src/registry/heap_core.rs`, `src/alloc_core/node.rs`,
`src/alloc_core/size_classes.rs`, `src/registry/tcache.rs`,
`src/global/sefer_alloc.rs`, `src/global/tls_heap.rs`. No source files were
modified; no findings below have been implemented yet.

## Trigger

Fresh wall-clock benchmarks (this session, `npm run bench:table`,
`production` feature set) showed SeferAlloc consistently ~2x slower than
mimalloc on the small/medium cold-direct path:

| Size | SeferAlloc | mimalloc | gap |
|---|---:|---:|---:|
| 16B | 36.3ns | 18.0ns | 2.01x slower |
| 64B | 45.1ns | 21.4ns | 2.11x slower |
| 256B | 50.3ns | 26.8ns | 1.87x slower |
| 1024B | 47.5ns | 54.0ns | 1.14x **faster** |

Deterministic reference (iai-callgrind, not wall-clock-noisy):
`small_churn_16b` ≈ 131.7 Ir/op marginal; `cold_alloc_free_256x16b` ≈ 204.5
Ir/op marginal.

## Ruled out

- Atomics: **zero** `Ordering::` ops fire on a magazine hit/push in the
  production feature set; `tcache_hits` counter is gated behind
  `alloc-stats`, stamping is hoisted into refill (OPT-C).
- Size-class lookup is already an O(1) compile-time LUT.
- Inlining is thorough (`#[inline(always)]` on the hot chain).
- TLS resolution is one `Cell` load + one sentinel compare (already
  optimized in a prior task, #145).

Conclusion: the 2x gap is **structural work per op**, not a missing
`#[inline]` or a stray atomic.

## Ranked findings

### 1. O(count) in-magazine double-free scan on every free — HIGH confidence, ~25–35% of the gap

- **File:** `src/registry/heap_core.rs:1005-1044` (`dealloc_own_thread_with_base`'s
  M2 oracle scan), fed by the magazine push at `:1073-1077`.
- Every own-thread small free linearly scans `tcache.slots[c][0..cnt]` for
  `ptr` to guard against double-free. The in-code comment claims
  "cnt is 1–3 in churn" — measured wrong for the cold-direct/churn pattern
  actually benchmarked: the first miss refills 16 blocks and pops 1, so
  `count[c]` oscillates 15↔16 forever after. **Every free compares ~15
  pointers** (~15 loads + 15 cmps across 2 cache lines) — mimalloc's free is
  `block->next = page->local_free; page->local_free = block`, no oracle at
  all.
- **Fix direction:** fold magazine membership into the existing per-segment
  `AllocBitmap` (already loaded on this path for a second oracle): redefine
  bit `1` as "block not owned by the user" (in magazine **or** on a BinTable
  free list) — set on magazine push, clear on magazine pop. The double-free
  oracle collapses to the `is_free(off)` bit test already being performed.
  Bonus: the decommit-reset stale-free `bump_of()` guard
  (`heap_core.rs:1065-1068`, one extra header cache-line load per free)
  becomes removable if segment reset sets payload bits to 1 instead of 0.
- **Risk: medium** — touches the M2/D1 double-free-guard invariant chain,
  `refill_class_bump`, `flush_class`, and decommit-reset semantics. Stays in
  safe code (bitmap ops go through the node seam), no new `unsafe`, no
  cross-thread protocol change (bitmap is owner-only). Needs the
  counterfactual double-free regression tests re-run after the change.

### 2. Free-path cache-line footprint — ~6 distinct lines per op pair vs mimalloc's ~3 — MEDIUM confidence

- **File:** `src/registry/tcache.rs:105-112` (`Tcache` layout: `slots` array,
  then `count` array ~6KB away — never share a line); plus per free:
  own_cache line, header `bump` line, bitmap line, slots line, count line.
- **Fix direction:** restructure `Tcache` as
  `[PerClass; SMALL_CLASS_COUNT]` where `PerClass { count: u16, slots: [*mut
  u8; CAP] }`, padded/aligned so count and top-of-stack live on one line.
  Finding 1 removes the `bump` header line as a side effect.
- **Risk: low** — pure private-struct layout, single-threaded field, no
  invariants.

### 3. Cross-thread routing tail fully inlined into every free call site — MEDIUM confidence (I-cache/code-bloat)

- **File:** `src/registry/heap_core.rs:1285-1433` — `dealloc_routing` is
  `#[inline(always)]` and carries the entire cold cross-thread tail (magic
  check, owner compare, Large deferred-push, ring push) inline behind the
  `contains_base` branch.
- **Fix direction:** outline everything after the `contains_base(base)` hit
  into `#[cold] #[inline(never)] fn dealloc_foreign_slow(...)`, mirroring
  the existing `refill_magazine_slow` outlining pattern.
- **Risk: low** — pure code motion, no semantic change.

### 4. Alloc-side classification does more branch work than mimalloc's direct LUT — MEDIUM confidence, ~5-10 Ir/alloc

- **File:** `src/registry/heap_core.rs:576-603` + `src/alloc_core/size_classes.rs:161-182`.
- Per alloc: size/align clamp, `need > SMALL_MAX` branch, LUT load,
  `align <= 16` branch, then the `class.is_none()` Large-drain branch, then
  `if let Some(c)` again. mimalloc's `mi_heap_malloc_small` is one LUT load
  + a single null-check branch.
- **Fix direction:** collapse the dominant `align <= 16 && size <= SMALL_MAX`
  case to a single guarded LUT index; verify in asm whether the
  `is_none()`/`Some(c)` branches already merge.
- **Risk: low-medium** — hot but pure arithmetic; the Large-drain check must
  keep firing for `class == None`.

### 5. Double classification per logical alloc/free pair — LOW confidence as a lever

`class_for` runs once in alloc and once in free — inherent to the design
(free is Layout-keyed, avoiding a dependent header load). Not worth
changing standalone; finding 4 shrinks both call sites.

### 6. Confirmed non-issues (documented to save re-investigation)

- TLS resolution, `contains_base` direct-mapped cache hit, absence of
  `Instant::now()`/stats/bounds-checks on the small hot path in release —
  all already comparable to mimalloc.

## Summary recommendation

Eliminate the O(count) in-magazine double-free scan (finding 1) by folding
magazine membership into the existing per-segment `AllocBitmap`. In the
benchmarked cold-direct/churn workload the magazine sits at 15-16 entries,
so today's "exact" guard is really ~15 pointer compares per free — the
single largest line-item separating SeferAlloc's ~131 Ir/op from mimalloc's
~40-50. Preserves M2 exactly (arguably strengthens it, subsumes the
decommit-reset `bump` guard), stays in safe owner-only code. Medium risk,
highest expected reward of the five findings; validate with the existing
counterfactual double-free tests plus the iai `small_churn_16b` judge.
