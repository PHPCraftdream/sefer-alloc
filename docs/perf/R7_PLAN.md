# Round7 plan — kill the high-S segment scan and Windows eager commit

**Base revision:** `461fe8f`. This is the working spec handed to implementers.
Strategic API changes and safety-guarantee weakening are deliberately OUT of
scope for Round7.

## Goals

1. Replace the O(S) "find a segment with a free block" scan with a lazy
   per-class **segment directory** + **remote dirty routing**.
2. Cut the Windows first-heap commit from 4.52 MiB to a realistic 0.6–0.9 MiB
   via **incremental commit** of small segments.
3. After the architectural work, run two bounded experiments: TCACHE_CAP and
   pool-cap.
4. Do NOT change the public API and do NOT introduce a `contract-fast`
   safety-weakening profile in Round7.

## Ground rules

- Each phase is its own commit.
- Judge + pre-registered GO/NO-GO criteria FIRST, then implementation.
- New mechanisms land behind experimental feature flags; the feature-OFF path
  stays byte-for-byte semantically identical to today.
- Every perf GO requires simultaneously: the target win; no regression of
  ordinary churn; proven correctness; measured memory overhead.
- Do NOT mix the directory and incremental-commit work in one commit or one A/B.
- Document every rejected variant with numbers; never leave dead experimental
  code in the tree.

## Verified facts (code fact-check at 461fe8f)

These underpin the design and were confirmed against the source:

- **The directory design is semantically complete.** `find_segment_with_free_impl`
  (`src/alloc_core/alloc_core_small.rs`) selects a segment ONLY by
  `bt.head(class_idx) != FREE_LIST_NULL` (line ~399). Carve/bump room is a
  SEPARATE concern living on `small_cur`, never consulted by this scan. So a
  per-class `class_nonempty` bitmap covers the scan's selection criterion
  exactly — no second "has-carve-room" bitmap is needed.
- **The scan is NOT a pure lookup** — it has load-bearing side effects (see P1).
- **The scan has a two-pass NUMA preference** (local-first, foreign-as-fallback,
  lines ~404-425) (see P2).
- A1 sizing: 49 classes × 16 u64 × 8 B = 6.1 KiB; 55 (medium-classes) = 6.9 KiB.
  A4: `dirty_segments[16]` = 128 B/slot ≈ 8 KiB per 64-slot registry chunk.
- `flush_run` exists (`alloc_core_small_magazine.rs`) — the A2 transition-site
  list is correct.
- `SegmentHeader.segment_id: u32` is immutable for a segment's lifetime;
  slot/base/kind/id revalidation on recycle is already established practice.
  `resolve_heap_overflow` (R6) is the ready owner-slot resolver to model the A4
  producer path on.
- vmem commits the whole usable span at once on Windows
  (`crates/vmem/src/lib.rs` `winapi_virtual_commit`, ~line 420); the
  fallible-recommit style (`recommit_pages_impl`) is the precedent for B.
- The 0.9 MiB B-gate is realistic: ~0.52 MiB non-segment (chunk + overflow
  sidecar) + metadata (tens of KiB) + first payload chunk (128–256 KiB).

## Mandatory corrections (P1–P6)

**P1 (blocking, A3): preserve the scan's side effects on the directory path.**
Today's per-segment scan body: (a) lazily drains that segment's remote ring
(Variant-2); (b) on an emptied segment calls `release_or_pool_empty_segment`
(the whole Mechanism-2 decommit/pool hysteresis lives INSIDE the scan); (c) on a
hit calls `unpool_if_present` (skipping it = documented double-pool hazard);
(d) refreshes the `ring_drain_head` cache. A directory hit MUST still
`unpool_if_present`; the A4 dirty-drain MUST reuse the existing scan drain body
(it already carries the decommit flag + pool/release decision), not a fresh loop.

**P2 (blocking, A design): preserve the two-pass NUMA preference.** A "first set
bit" directory query silently drops local-first/foreign-fallback. Apply the same
rule the plan sets for B: either node-aware bit selection, or forbid
`numa-aware × alloc-segment-directory` at compile time — never silently lose
placement.

**P3 (A4): set the dirty bit on EVERY successful ring publish.** Including the
retry-path site `push_with_overflow_retry::try_push_uncounted` (rebuilt in
R6-REGRESSION-2), not only happy-path `dealloc` — a missed retry site = a
persistent invisible entry. Add to A4 kill-gates: `regression_paused_owner_wallclock`,
`regression_paused_owner_multisegment`, `remote_fanin_high_contention`
(`exhausted_delta == 0`).

**P4 (doc-note + test): the ring-entry visibility contract changes.** Today ANY
full scan finds any published entry; with dirty routing, a producer stalled
between publish and `fetch_or` is invisible until its bit (or until the fallback
scan on a crashed thread). This is bounded deferral of the same class as the
existing "later drain picks it up" contract, but the contract changes — pin it
in the `remote_free_ring` module doc and cover it with a guarded-fallback test.

**P5 (tuning): the materialization threshold is data, not dogma.** The A4 judge
already shows S=64 at 0.65–3.75 µs and S=32..63 already paying microseconds on
the linear scan. Make the threshold a named constant chosen from A0 data
(32 vs 64); the `S<=16` GO gate (≤2% regression) permits either.

**P6 (consistency): C2 presets are documented recipes, not new API.**
`low-rss`/`balanced`/`throughput` must be documented recipes over the existing
`LargeCacheConfig`/`SmallSegmentPoolConfig` (docs + examples), NOT new public
constructors — Round7's own "no public API" rule. A new API preset is a separate
decision outside Round7.

---

# Workstream A — per-class segment directory

## A0 — baseline + observability
Run `benches/segment_directory_sweep.rs` across S=1,3,16,64,256,1023 × holes
0/25/50/75% × normal+medium-classes × remote dirty density 0/1/10/100%. Capture
mean/p50/p99, segment slots examined, refill misses, remote-ring drains,
non-empty segments actually found. Fix the kill-gate set (iai, criterion churn
16/64/256/1024 B, cold direct, persistent fan-in, Windows first-heap commit).
Add diagnostic counters under `alloc-stats`/bench cfg ONLY: `directory_hits`,
`directory_stale_hits`, `directory_fallback_scans`, `directory_words_examined`,
`dirty_segments_drained`, `full_scan_slots_examined`. Gather S=32..63 data for
the P5 threshold choice. **Deliverable:** `docs/perf/R7_DIRECTORY_BASELINE.md`.

## A1 — lazy owner-only directory sidecar
New experimental feature `alloc-segment-directory`. Struct
`SegmentDirectory { class_nonempty[SMALL_CLASS_COUNT][MAX_SEGMENTS/64] }`.
Materialize ONLY after `table.count() >= THRESHOLD` (named const from A0); below,
keep the linear scan. Reserve M5-clean via the existing direct-VM lazy-sidecar
pattern (reuse the R6 heap_overflow/bootstrap mechanics). Sidecar OOM is NOT
allocator OOM — disable the mechanism, fall back to linear scan. Pointer stable
until heap death; NOT inline in every HeapSlot. On first materialization do one
full rebuild from every registered small/primordial segment's per-class BinTable
head (skip Large + recycled slots). Test: built directory == actual BinTable state.

## A2 — centralize empty↔non-empty transitions
Helpers `publish_nonempty(class,slot)` / `publish_empty(class,slot)` /
`clear_segment(slot)`, wired into EVERY head-mutating path: `dealloc_small`,
`flush_class`, `flush_run`, `reclaim_offset`, `reclaim_offset_checked`,
`pop_free`, `drain_freelist_batch`, pool/unpool, decommit reset, table
recycle/release. Rules: set only on `old_head==NULL && new_head!=NULL`; clear
only on `old_head!=NULL && new_head==NULL`; on recycle clear ALL classes of the
slot; reused slots never inherit old-lifetime bits; owner-only → non-atomic word
ops. The helpers are the single choke point — no ad-hoc maintenance.

## A3 — directory-driven lookup
Rewire `find_segment_with_free_impl`: directory disabled → current scan; else →
drain dirty routing → query `class_nonempty[class]` → validate slot → hit
returns → directory miss → GUARDED fallback scan. **P1:** the hit path preserves
`unpool_if_present` and validates `base_at(slot)!=null` + kind small/primordial +
BinTable head still non-null (stale positive → clear bit + continue). **P2:**
preserve the NUMA two-pass preference. First GO must be achieved WITH the
fallback scan present (correctness oracle + OOM degradation path — do not remove
prematurely; whether the directory becomes authoritative is decided only after
A5 property tests).

## A4 — remote dirty routing
Stable per-slot `dirty_segments[16]: AtomicU64` (128 B/slot, ~8 KiB/chunk).
Producer: after a successful `RemoteFreeRing` publish, read immutable
`segment_id`, `dirty_segments[word].fetch_or(bit, Release)`. **P3:** every
publish site, including `push_with_overflow_retry::try_push_uncounted`. Owner
drain: `swap(0, Acquire)` each dirty word; per bit `base_at(slot)`; revalidate
slot/base/kind/id; **P1:** REUSE the existing scan drain body; publish reclaimed
blocks into the directory. Lost-wakeup: bit after publish; late producer re-sets
after owner swap; slot reuse always revalidated; heap-overflow fallback stays a
separate existing channel. If the stable remote-control struct can't carry the
bitmap without extending the header stamp, write a design note and choose
inline-bitmap vs separate-stable-pointer; never a raw base queue without
generation/revalidation. **P4:** pin the changed visibility contract in the
`remote_free_ring` module doc.

## A5 — correctness gates
Property (directory bits == actual non-empty BinTable heads);
empty→non-empty; non-empty→empty; multiple classes per segment; recycle+reuse
same/different base; pool/unpool; decommit/reset/recommit; medium-classes;
sidecar OOM → fallback; stale positive → clear+continue; NO false negative after
local free; remote push before/after owner swap; multiple producers set one bit;
producer during drain; dirty word with multiple segments; slot recycle with stale
dirty bit. **P4** guarded-fallback test. **P3** kill-gate re-run
(paused_owner_wallclock, paused_owner_multisegment, remote_fanin_high_contention).
Tools: loom (dirty publication/swap/lost-wakeup), miri strict-provenance
(sidecar + slot/base lookup), TSan (real remote fan-in), ASan (directory
index/recycle), production + all-features suites.

## A6 — GO/NO-GO
GO only if ALL hold: S=256/1023 holes=0 → ≥10× mean AND p99 refill-miss with
≤16 directory words examined (checked via `directory_words_examined`); remote
density 1–100% → ≥5× high-S miss AND zero lost remote frees; S≤16 → not worse
than 2%; IAI churn → not worse than 1% (ideally the raw-Ir kill gate); sidecar
absent at S<64; high-S heap overhead ≤ ~8 KiB directory + agreed dirty control.
Expected: S=1023 refill-miss tens/hundreds µs → sub-µs / low-single-digit µs;
headline churn unchanged. NO-GO → document with numbers + remove dead impl.

---

# Workstream B — incremental Windows commit

**Start only after A's verdict (A6) is fixed. Do not mix A/B.**

## B0 — vmem design + accounting
Experimental Windows-only feature `alloc-lazy-commit`. New op
`commit_range(base, start, end) -> bool`. Reservation stays a single object
freed once; aligned usable base preserved; metadata pages committed before first
write; Unix + Miri keep the eager impl; Large stays eager in phase 1; NUMA-aware
Windows handled explicitly (node-aware commit OR forbid the combination at
compile time — never silently lose placement). Reuse the fallible-recommit style.

## B1 — commit frontier
Owner-only page-aligned `committed_payload_end` in always-committed metadata.
Fresh small segment: reserve 4 MiB without full commit → commit
`[0, metadata_end)` → commit first payload chunk → init metadata → carve. First
chunk size = named const to sweep in B5 (64/128/256/512 KiB; likely 128/256).

## B2 — fallible bump growth
All bump-carve paths: compute carve/batch end; if above frontier, commit the
rounded chunk range first; only after success advance bump + live count + page
map + hand out pointer. On failure: everything unchanged, block not published,
try another segment or return null. `carve_batch` must do ONE commit for the
whole batch, not a syscall per block.

## B3 — decommit/recommit integration
Lazy mode: after decommit reset frontier to metadata/initial chunk; on reuse grow
in chunks again; never read/write above frontier; keep stale-free guards; never
decommit metadata or the remote ring. Tests: alloc_zeroed; recommit after
decommit; partial chunk; commit failure + retry.

## B4 — correctness gates
First block of each chunk; block on a chunk boundary; batch crossing one/several
boundaries; commit failure before bump update; retry after failure; decommit →
partial recommit; pool retain/reuse; pool eviction/release; cross-thread free in a
partially-committed segment; allocator drop/release reservation; primordial
metadata; medium classes; zeroed allocations; Windows NUMA combination.
Fault-injection hook to fail a specific N-th commit (test-only).

## B5 — GO/NO-GO + chunk sweep
Primary judge: process-per-sample A1 (`first_alloc_process`). GO: first-heap
Windows commit 4.52 MiB → ≤0.9 MiB (target 0.6–0.8); first-alloc latency not
worse than 10% (ideally better); dense cold alloc not worse than 3%; steady churn
no regression; commit-syscall count scales with chunk growth not allocation
count; commit failure fully recoverable; Linux + Miri no regression. Sweep chunk
size, pick by Pareto (commit vs wall-clock). **Deliverable:**
`docs/perf/R7_INCREMENTAL_COMMIT.md`.

---

# Workstream C — bounded experiments (interleave; do not delay A/B)

## C1 — TCACHE_CAP re-sweep (16/32/64)
The old NO-GO (`PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`, 2026-07-07) predates
MagazineBitmap/RAD-5 (2026-07-12) which removed the O(count) M2 scan that
penalized larger caps — cost model changed. Test 16/32/64 + optionally a
"flush-incoming-directly-when-full" policy without raising cap. Measure cold
direct 16/64/256 B; churn/write-churn; refill/flush count; first registry chunk
commit; HeapSlot size; IAI; p99. GO: cold direct ≥−10%; churn not worse than 2%;
first-heap commit not up beyond an agreed limit; no growth of remote/magazine
correctness surface. Magazine arrays scale as `TCACHE_CAP × 49 × 8 B × 64 slots`
per chunk → cap=64 adds ~1.2 MiB to the first chunk; if it eats the R6
commit-charge win, NO-GO regardless of a small wall-clock win.

## C2 — pool-cap sweep → documented presets
Without algorithm changes, sweep pool cap 0/1/4/8/16 (default 4) each with a
matched `pool_byte_cap`. Measure working-set cycle 256/1024 B;
decommit/recommit/reserve/release counts; retained commit/RSS; latency after
oscillation; OOM pool drain. **P6:** outcome is documented recipes
(`low-rss`/`balanced`/`throughput`) over the existing config types, not new
constructors.

---

# Execution notes

- **One cargo/bench process at a time** (16-thread host, standing constraint).
- **C interleaves with A/B** = sequenced between A phases on this single host,
  not run concurrently.
- Both workstream deliverables (`R7_DIRECTORY_BASELINE.md`,
  `R7_INCREMENTAL_COMMIT.md`) live under `docs/perf/`.
