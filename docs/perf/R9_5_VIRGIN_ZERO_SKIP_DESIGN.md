# R9-5 — Small-path `alloc_zeroed` "virgin zero-skip": DESIGN-ONLY (no code change)

**Task:** design (and, only if airtight, prototype) a virgin zero-skip optimization
for the SMALL allocation path's `alloc_zeroed`, plus measure the potential win for
sizes 4 KiB–1 MiB.
**Outcome:** **DESIGN-ONLY.** No `src/`, `Cargo.toml`, or `tests/` file is modified.
The deliverable is this doc. §10 states precisely *why* no prototype is shipped
this session, and §11 gives the staged plan that would land it.
**Date:** 2026-07-20
**Base revision:** `main` @ `f469343` (R9-4 just landed; the small-path substrate
under analysis is R8-10 task #223 @ `852828e`, which landed 2026-07-20 — see §6).
**Platform:** Windows 10 Pro x86-64 (analysis host). The correctness argument is
platform-parametric (Windows `VirtualAlloc` MEM_COMMIT vs Unix anonymous `mmap` vs
miri `std::alloc` fallback) and is resolved per-platform in §4; no measurement
here is host-dependent because no measurement is performed (§8 is analytical).

---

## 0. TL;DR

The optimization is **structurally feasible and, against the *current* (post-R8-10)
small-path substrate, airtight** for all five risk areas the task brief enumerates.
It is the SAME class of optimization as the already-shipped Large-path skip
(R8-8 task #221 / R9-1 task #232, `alloc_large_slow` returning a freshness bool,
consumed by `alloc_zeroed`), except the freshness signal is **per-carve** rather
than per-segment-reservation, because one small segment holds many same-class
blocks carved incrementally by a monotonic bump cursor.

**However, this exact idea was a documented NO-GO on 2026-07-10** (P4(b)/S3,
`docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md`), and a later
deep-audit (`docs/reviews/2026-07-17-deep-audit/07-perf-optimizations.md` P2-3,
landed 2026-07-19 00:12 UTC) re-proposed it as a "high-water virgin frontier per
segment" but **explicitly left the macOS `MADV_DONTNEED` risk UNRESOLVED**, rating
it medium-risk and saying "переиспользованные mapping'и НЕ считать нулевыми"
(reused mappings must NOT be assumed zero). **R8-10 (task #223, commit `852828e`,
landed 2026-07-20 10:57 UTC — the day AFTER the deep-audit) is what dissolved
that risk**: it removed the B3 "decommit the pooled segment's payload on pool
admission" path, which was the *only* production code that ever produced the
macOS-dangerous "decommitted but still registered" small-segment state. With R8-10
in place, the dangerous path is provably unreachable in production
(`decommit_empty_segment_impl(_, _, false)` has zero callers — verified by grep,
§4.3), so the P4(b) reason #2 (macOS `MADV_DONTNEED` laziness) can no longer fire
on the small path. This report's contribution over P2-3 is showing that R8-10
closed the gap P2-3 named.

**No prototype is shipped** (§10): a substrate-only prototype (modifying
`AllocCore::alloc_zeroed`'s small arm) would be fully testable but **production-
inert** — the production `HeapCore::alloc_zeroed` small arm does NOT delegate to
`AllocCore::alloc_zeroed` (it calls `self.alloc` + unconditional `Node::zero`,
`src/registry/heap_core_alloc.rs:318-333`; confirmed by grep — zero call sites of
`self.core.alloc_zeroed`). The real production win requires plumbing the virgin
bit through the magazine refill path (`refill_class_bump` → `carve_batch` →
magazine slot → magazine hit → `HeapCore::alloc_zeroed`), which is a larger
surface with its own open design question (per-magazine-slot virgin storage in
`PerClass.slots: [*mut u8; TCACHE_CAP]`). That is staged in §11, not rushed here.

The estimated win ceiling (§8, analytical from memset bandwidth, no code to
measure): for a *genuinely first-touch, never-reused* `alloc_zeroed` call the
skip saves the full `memset(N)` — roughly **~130 ns at 4 KiB → ~530 ns at 16 KiB
→ ~2.5 µs at 64 KiB → ~12-17 µs at 256 KiB → ~70-90 µs at 1 MiB** per call on a
modern x86-64 core. This is a *ceiling* that applies only to cold/calloc-first-
touch traffic; steady-state churn (the pattern the perf campaign actually
diagnoses as hot) reuses blocks and gets **zero** benefit, because a reused block
is by definition not virgin and MUST be zeroed explicitly. That narrowness is the
same narrowness P4(b) reason #3 named, and it is the second reason (after the
production-plumbing cost) a prototype is not rushed.

---

## 1. Scope recap — what the Large path already does, and why Small is harder

R8-8/R9-1 ships a Large-path zero-skip (`src/alloc_core/alloc_core_large.rs:39-56`,
57, 267-360; `src/alloc_core/alloc_core.rs:825-833`; `src/registry/heap_core_alloc.rs:
335-365`). `alloc_large` returns `(*mut u8, bool)`; `alloc_large_slow` (the only
fresh-span producer) yields `cfg!(not(miri))`; a `large_cache` HIT yields `false`
everywhere. The signal is **SEGMENT-granular**: one whole 4 MiB (or larger)
dedicated span is either genuinely fresh (OS-zero-guaranteed) or reused. There is
exactly ONE block per Large segment, so per-segment and per-block granularity
coincide. The Large test (`tests/alloc_zeroed_fresh_large_skip.rs`) pins this with
a per-platform delta-assertion against `dbg_large_zero_pass_count`.

The Small path is **fundamentally multi-block-per-segment**: one 4 MiB segment
holds many same-class blocks, carved incrementally by a per-segment monotonic
bump cursor (`carve_block`, `src/alloc_core/alloc_core_small.rs:1052-1162`;
batched sibling `carve_batch`, `alloc_core_small.rs:1209-1287`). The block at
range `[old_bump, old_bump + block_size)` is carved exactly once; later, after
`dealloc`, it may return to the segment's free list and be re-served by `pop_free`
— bit-for-bit indistinguishable at the call site from a virgin carve *unless new
metadata distinguishes them*. This is P4(b)'s reason #1 ("no per-block virgin
state exists"). The design below resolves it **without per-block metadata**, by
exploiting (a) bump-cursor monotonicity within a committed lifetime and (b) the
carve-vs-`pop_free` dispatch distinction — both verified against the current code.

---

## 2. The invariant (precise, byte-range level)

**Definition (virgin byte range).** A byte range `[lo, hi)` inside a small
segment's payload (`[small_meta_end, SEGMENT)`) is **virgin** iff, at the moment
`alloc_zeroed` consults it, *every* byte in `[lo, hi)` satisfies: "this byte was
last made readable by an OS zero-fill on the current registered segment's current
committed lifetime, and has never been written by any allocator or caller code
since."

**Corollary (OS-zero guarantee).** The OS zero-fill is guaranteed on:
- **Fresh reserve** (`Segment::reserve` / `numa::reserve_aligned_on_node`):
  Windows `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` demand-zero; Unix anonymous
  `mmap` zero-fill — both including macOS fresh `mmap` (the `MADV_DONTNEED`
  exception is decommit-then-reuse, NOT fresh reserve, §6).
- **Incremental first-time commit** (`os::commit_pages`, the lazy-commit grow-on-
  carve path, `alloc_core_small.rs:1122-1144`): Windows `VirtualAlloc(MEM_COMMIT)`
  on previously-reserved-but-uncommitted pages is demand-zero. (Lazy-commit is
  Windows-only by cfg, `alloc_core_small.rs:1635-1638`, so this branch never runs
  on macOS — see §4.2.)
- **NOT guaranteed** on decommit-then-recommit on macOS/XNU/*BSD (`MADV_DONTNEED`
  is advisory+lazy, no zero-fill — `crates/vmem/src/lib.rs` §decommit note). This
  is the P4(b) killer — and it is the reason the design hinges on §4.3.
- **NOT guaranteed under `cfg!(miri)`**: miri's `std::alloc` fallback does not
  zero (`crates/vmem`'s miri aperture), exactly as R9-1 documented for the Large
  path. The Small design withholds the signal identically (§5).

**Operational test for "is the block I am about to hand out virgin?".** A block
served by `alloc_small` reaches the caller through exactly one of two dispatches
(verified, `src/alloc_core/alloc_core_small.rs:103-150`):

1. **`pop_free` (free-list pop)** — the block was previously carved, handed out,
   and freed. By construction **never virgin**, regardless of OS state.
2. **`carve_block` / `carve_batch` (bump advance)** — the block range
   `[aligned_bump, aligned_bump + block_size)` was never previously handed out
   *in this committed lifetime* (bump is monotonic; `pop_free` does not move it;
   only a decommit/reset moves it back, §4.3). Whether its bytes are OS-zero-
   guaranteed is then a property of the segment's committed lifetime — see §3.

So virginity decomposes into a **dispatch test** (carve vs pop, already
distinguished at the call site — no new metadata) AND a **segment-lifetime test**
(one new per-segment bit, §3).

---

## 3. The per-segment field — `payload_virgin: bool`

Add ONE bit of per-segment state to `SegmentHeader`
(`src/alloc_core/segment_header.rs`, alongside the existing owner-only
`committed_payload_end: usize` at line 547 and `bump: usize` at line 315):

```text
payload_virgin: bool   // owner-only. true iff this segment's payload bytes
                       // [small_meta_end, bump) have NEVER been made readable
                       // by any path other than a fresh-OS-reserve or an
                       // incremental first-time commit on THIS registered
                       // lifetime (i.e. no in-place decommit+recommit has
                       // ever occurred on this registered segment).
```

**Why a bool and not the deep-audit's "high-water `virgin_frontier: usize`"?**
Within a committed lifetime the bump cursor is monotonic (carve advances it;
`pop_free`/remote-free reclaim do not touch it; only `decommit_empty_segment_impl`
resets it to `payload_start`). Therefore the high-water frontier equals the
current `bump` at all times in steady state, and "is this carve's range past the
frontier" is *always true* for a carve. The bool captures the only thing that
actually matters — "has an in-place decommit ever occurred on this registered
segment?" — at one quarter the storage (1 bit vs `usize`) and with a simpler
invariant. The frontier formulation is strictly more general but buys nothing
under the current substrate; it is recorded as a hedge in §11 if a future change
re-introduces mid-lifetime bump resets.

**Reset rules (the load-bearing part — justified against the code in §4):**

| Event | `payload_virgin` after | Site | Justification |
|---|---|---|---|
| Fresh `reserve_small_segment` | **`true`** (real OS) / **`false`** (`cfg!(miri)`) | `alloc_core_small.rs:1428-1721` | Genuinely fresh OS reservation; OS-zero-guaranteed on every real backend. miri withheld per §5. |
| `carve_block` / `carve_batch` success | unchanged (stays whatever it was) | `alloc_core_small.rs:1052-1162`, `1209-1287` | A carve does not decommit; lifetime virginity is unaffected. |
| `pop_free` success | unchanged | `alloc_core_small.rs` (pop path) | Free-list reuse does not reset lifetime state; the served block is reported non-virgin via the *dispatch* test (§2), not via this bit. |
| Pool admission (`pool_push_front`) | unchanged | `alloc_core_small_pool.rs:266-273` | R8-10: pool admission NEVER decommits or resets metadata (`alloc_core_small_pool.rs:256-265`). Lifetime virginity survives pooling. (But a pooled segment is fully-carved — bump near SEGMENT — so carve never runs on it again and the bit is never consulted; pool reuse is via `pop_free`, reported non-virgin by dispatch.) |
| `release_or_pool_empty_segment` → release branch → `release_empty_segment_now` (`release_follows=true`) | **irrelevant** | `alloc_core_small_pool.rs:283-284, 358-360, 620-622` | The reservation is fully released (`os::release_segment`) and the slot `recycle`d (NULLed) immediately after. The segment ceases to exist as a registered segment; a future reservation at this address is a fresh `reserve_small_segment`, which resets the bit to `true`. No special handling. |
| `decommit_empty_segment_impl(_, _, release_follows=false)` | **`false`** (defensive — DEAD in production today, §4.3) | `alloc_core_small_pool.rs:631-730` | This is the ONLY path that decommits a payload while leaving the segment registered. On macOS the subsequent recommit (via `carve_block`'s `is_decommitted()` branch, `alloc_core_small.rs:1070-1110`) is NOT zero-guaranteed. Setting `payload_virgin=false` here makes the skip permanently conservative for such a segment — correct on every platform. |

**Read site.** `carve_block` / `carve_batch` read `payload_virgin` once per carve
run and OR it with `cfg!(not(miri))` to produce the per-block (or per-batch)
virgin signal handed back to `alloc_small`. `pop_free` produces `false`
unconditionally. `alloc_small` propagates the signal up to `alloc_zeroed`'s small
arm, which skips `Node::zero` iff the signal is `true` and bumps a new
`SMALL_ZERO_PASS_CALLS` counter iff it is `false` (mirroring `LARGE_ZERO_PASS_CALLS`,
`src/alloc_core/alloc_core.rs:214`, `src/alloc_core/alloc_core_core_diag.rs:409-410`).

**Storage / layout cost.** One byte (one bit, but stored as a `bool` field for
the same field-atomicity discipline `committed_payload_end`/`bump` use). Inline
in `SegmentHeader`, which is per-segment (one per 4 MiB), not per-block — the
metadata overhead is `1 byte / 4 MiB`, i.e. negligible, and far below P4(b)'s
"new per-block virgin metadata" objection (this design adds NO per-block state,
resolving P4(b) reason #1 — §6).

---

## 4. The five risk areas — each handled, with file:line evidence

### 4.1 Risk area 1 — Pooling (R8-10): pooled/reused blocks must never read virgin

**Argument.** A pooled segment is reused via `find_segment_with_free`'s **free-
list** path, which calls `pop_free` — and `pop_free` produces the virgin signal
`false` unconditionally (§3 read site). It is NEVER re-inserted as a fresh carve
target: `reserve_small_segment`'s own doc (`alloc_core_small.rs:1429-1448`)
states "a pooled segment is fully-carved (bump near SEGMENT, fully carved) … it
cannot serve as a FRESH carve target … drawn from via
`find_segment_with_free`'s free-list reuse (the hysteresis win: the emptied
segment's blocks are re-served with no OS work), NOT popped as `small_cur` here."
`carve_block` therefore never runs on a pooled segment, so its `payload_virgin`
bit (whatever its value) is never consulted for a pooled-segment block. The
dispatch test alone makes pooled-segment blocks non-virgin, *independently* of
the bit. **Cannot mark a pooled block virgin.** ✓

### 4.2 Risk area 2 — `alloc-lazy-commit` incremental commit (every commit call site)

**Argument.** There are exactly two commit call sites that can precede a bump
advance:

1. **Grow-on-carve** (`alloc_core_small.rs:1122-1144`): `os::commit_pages(segment,
   frontier, new_frontier)` commits `[frontier, new_frontier)` — a range that was
   *reserved-but-never-committed* since the segment's fresh reserve (the lazy
   reserve commits only `[0, meta_end + LAZY_FIRST_CHUNK)`, `alloc_core_small.rs:
   1506-1514`; the frontier only ever advances, never retreats, within a
   committed lifetime). This is a **first-time commit** of bytes that no code has
   ever written. On Windows `VirtualAlloc(MEM_COMMIT)` is demand-zero → zero-
   guaranteed. The lazy-commit feature is cfg'd Windows-only (`alloc_core_small.rs:
   1635: `all(not(feature = "numa-aware"), windows, not(miri))`), so this branch
   does not exist on macOS; the macOS `MADV_DONTNEED` exception cannot apply
   here. ✓
2. **Recommit-on-`is_decommitted`** (`alloc_core_small.rs:1070-1110`): fires only
   when `meta.is_decommitted()` is true. §4.3 proves this is never true on a live
   registered segment in production, so this branch is **defensive dead code**
   today; if it ever fires, the design sets `payload_virgin=false` at the
   `release_follows=false` decommit site (§3 reset table), so the skip stays
   conservative. ✓

**No commit call site can precede a bump advance on a path that would let a
dirty byte survive into a virgin-marked block.** ✓

### 4.3 Risk area 3 — Segment release + re-reserve vs decommit+recommit (the macOS crux)

**This is the load-bearing risk area and the one P4(b)/P2-3 flagged.** Two
sub-paths must be distinguished:

**(a) Full release + fresh re-reserve** — `release_or_pool_empty_segment`'s
release branch (`alloc_core_small_pool.rs:274-285`) calls
`release_empty_segment_now` → `decommit_empty_segment_for_release` (which passes
`release_follows=true` unconditionally, `alloc_core_small_pool.rs:621`) →
`set_bump(payload_start)` + `set_decommitted(true)`, *then* `self.table.recycle(base)`
which NULLs the slot, *then* the caller (or the slot-recycle machinery) runs
`os::release_segment` → `MEM_RELEASE`/`munmap` on the WHOLE reservation. The
segment ceases to be registered. A future allocation at any base goes through
`reserve_small_segment` → a genuinely fresh `Segment::reserve` (fresh
`mmap`/`VirtualAlloc`) → **zero-guaranteed on every real OS including macOS
fresh `mmap`** (the `MADV_DONTNEED` exception is decommit-then-reuse, NOT fresh
reserve). The `payload_virgin` bit is reset to `true` by
`reserve_small_segment`. ✓

**(b) Decommit-in-place + recommit-on-reuse** — `decommit_empty_segment_impl(_,
_, release_follows=false)` is the path that would decommit the payload *while
leaving the segment registered*, creating the macOS-dangerous state. **Grep
verification (run this session):**

```text
=== callers of decommit_empty_segment_impl ===
src/alloc_core/alloc_core_small_pool.rs:621:  Self::decommit_empty_segment_impl(meta, base, true);   ← ONLY caller, hard-coded true
src/alloc_core/alloc_core_small_pool.rs:631:  fn decommit_empty_segment_impl(...) {                  ← the definition
(+ doc/comment references only)

=== callers of decommit_empty_segment_for_release ===
src/alloc_core/alloc_core_small_pool.rs:359:  Self::decommit_empty_segment_for_release(meta, base);  ← ONLY production caller
(+ the definition + doc references)
```

`release_follows=false` has **zero production callers**. It exists only as a
defensive fallback for a decommit style (B3, R7 Workstream B) that R8-10
*removed from the pool-admission path* (`alloc_core_small_pool.rs:256-265`:
"pool admission never decommits or resets metadata"). With R8-10 in place,
`is_decommitted()` is **never true on a live registered small segment in
production**, so `carve_block`'s recommit branch (`alloc_core_small.rs:1070-
1110`) is also defensive dead code in production, and the macOS `MADV_DONTNEED`
failure mode cannot be reached on the small path. The design still sets
`payload_virgin=false` in `decommit_empty_segment_impl(release_follows=false)`
so that *if* that path is ever re-enabled (e.g. a future decommit policy
change), the skip stays conservative automatically — no silent regression.
**Cannot mark a decommit-reused macOS block virgin: the path is unreachable in
production, and the design is robust to its re-introduction.** ✓

### 4.4 Risk area 4 — Batched bump-carve (`carve_batch`)

**Argument.** `carve_batch` (`alloc_core_small.rs:1209-1287`) carves a RUN of up
to `out.len()` `block_size`-strided blocks in one call, all monotonic past the
entry bump. Because the bump cursor never retreats within a committed lifetime
(§3), every block in the run `[aligned_start + k*block_size, aligned_start +
(k+1)*block_size)` for `k in 0..n` is past the entry frontier, i.e. virgin iff
the segment's `payload_virgin` bit is true. The batched signal is therefore the
SAME single bit OR'd across the run: either all `n` blocks are virgin (signal
`true && cfg!(not(miri))`) or none are. There is no intra-run transition.
`carve_batch` reads the bit once at run start and stamps the same signal on all
`n` output blocks. **Cannot mark a non-virgin block virgin within a batch.** ✓

### 4.5 Risk area 5 — Remote-free reclaim (cross-thread freed blocks)

**Argument.** A block freed by a non-owner thread and reclaimed via the remote-
free ring is, by definition, previously carved → never virgin. It re-enters the
allocator through `reclaim_offset` → free-list push, and is later served by
`pop_free` → dispatch test reports `false` (§2, §4.1). The remote-free path
writes only to per-segment metadata that is disjoint from the owner-only
`payload_virgin` bit (it writes the ring + the bin-table free-list head + the
alloc bitmap; it does not write `bump`, `committed_payload_end`, or — in this
design — `payload_virgin`). This mirrors the **established owner-only discipline
the codebase already documents for `bump`** (`alloc_core_small.rs:1055-1060`:
"the Owner touches ONLY the `bump` field, never the cross-thread-read header
fields … `bump` is owner-only (no Remote reads it), so a plain field write is
race-free"). `payload_virgin` is owner-only by the same argument: written only
in `reserve_small_segment` and (defensively) `decommit_empty_segment_impl`, both
owner-thread; read only in `carve_block`/`carve_batch`, both owner-thread. **The
remote-free path cannot race or corrupt the field, and cannot elevate a remote-
freed block to virgin.** ✓

---

## 5. The miri caveat (mirrors R9-1 exactly)

Under `cfg!(miri)`, `crates/vmem`'s aperture falls back to bare
`std::alloc::alloc`, which does **not** zero freshly-reserved memory (vmem's own
`leak_zeroed_pages` documents and works around exactly this — cited in
`alloc_core_large.rs:43-48`). Therefore:

- `reserve_small_segment` sets `payload_virgin = cfg!(not(miri))` (i.e. `false`
  under miri), exactly as `alloc_large_slow` returns `cfg!(not(miri))` as its
  freshness bool (`alloc_core_large.rs:359`).
- The carve signal is `payload_virgin && cfg!(not(miri))`, which is always
  `false` under miri.
- `alloc_zeroed`'s small arm therefore always runs `Node::zero` under miri,
  preserving the `alloc_zeroed` contract there. The miri-gated test assertion
  mirrors `tests/alloc_zeroed_fresh_large_skip.rs:137-148` verbatim: under
  `cfg!(not(miri))` the virgin skip fires (`SMALL_ZERO_PASS_CALLS` delta 0 on a
  genuinely fresh carve); under `cfg!(miri)` it must not (`SMALL_ZERO_PASS_CALLS`
  delta 1). No re-invention of the pattern.

**Miri itself is NOT run in this session** (per task constraint — the repo's
recent experience is 17+ minutes per miri run even after aggressive workload
shrinkage). The miri code path is verified by reading: the `cfg!(not(miri))`
short-circuit is compile-time, structurally identical to R9-1's proven-correct
Large-path gate, and the existing `tests/alloc_zeroed_fresh_large_skip.rs` already
exercises the identical per-platform delta-assertion pattern under both `cfg`s.

---

## 6. Rebuttal of the 2026-07-10 P4(b) NO-GO — point by point

The P4(b) NO-GO (`docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md`)
gave three reasons. With the R8-10 substrate in place, the first two no longer
hold for THIS design; the third (narrow win) stands and is acknowledged.

| P4(b) reason (2026-07-10) | Status under R8-10 (2026-07-20) |
|---|---|
| **#1 — "No per-block virgin state exists; virgin-ness of a specific block inside an already-committed segment is a strictly finer question than any flag we keep."** | **Rebutted.** This design adds NO per-block metadata. It resolves the "is this specific block virgin" question via the **dispatch test** (carve vs `pop_free`, already distinguished at the call site, §2) plus ONE owner-only per-segment bit (§3). A carve-served block in a clean-lifetime segment is virgin; a `pop_free`-served block never is. No finer-than-segment metadata is needed because the bump cursor's monotonicity within a committed lifetime makes "is this carve past the frontier" trivially true, collapsing the per-block question to the per-segment lifetime question. |
| **#2 — "The recycled path legitimately holds non-zero garbage on macOS/XNU/*BSD (`MADV_DONTNEED` is advisory+lazy, no zero-fill guarantee)."** | **Rebutted, by R8-10.** The macOS danger requires a decommit-then-reuse cycle on a *registered* segment. R8-10 (commit `852828e`, 2026-07-20) removed the B3 decommit-on-pool-admission path — the only production code that produced that state. Verified by grep (§4.3): `decommit_empty_segment_impl(release_follows=false)` has zero callers; `is_decommitted()` is never true on a live registered segment in production; `carve_block`'s recommit branch is dead code. A pooled segment is reused via `pop_free` (never virgin by dispatch); a released segment is fully returned to the OS and any re-reserve is a fresh `mmap` (zero-guaranteed even on macOS). The deep-audit P2-3 (`docs/reviews/2026-07-17-deep-audit/07-perf-optimizations.md`, landed 2026-07-19 00:12 UTC, one day BEFORE R8-10) re-proposed this exact design but flagged the macOS risk as unresolved — correctly, given the substrate *at that time*. R8-10 landed the next day and closed the gap. **This is the dated, verifiable contribution of this report over P2-3.** |
| **#3 — "The extractable win is narrow (only the first `alloc_zeroed` touch of a genuinely fresh, never-reused bump slice)."** | **Stands.** Acknowledged in §0 and §8. The win is a cold/calloc-first-touch ceiling; steady-state churn reuses blocks and gets zero benefit. This is the second reason (after the production-plumbing cost, §10) no prototype is rushed. |

The P4(b) "what would flip this to GO" criteria were: (i) a deterministic
measurement proving the memset is a real cost (§8 supplies the analytical
estimate; a dedicated `alloc_zeroed` cold bench is the natural follow-up, §11),
(ii) a *segment-level* (never per-block) virgin signal that provably cannot be
true for any freed/recycled/recommitted block on ANY target OS (this design, §3-
§5), and (iii) the poison-counterfactual test green (specified in §11's test
plan, mirroring `tests/alloc_zeroed_fresh_large_skip.rs` exactly). Criteria (ii)
is met by this design; (i) and (iii) are the implementation work §11 stages.

---

## 7. Why no prototype this session (the production-path gap)

A prototype scoped to `AllocCore::alloc_zeroed`'s small arm (the minimal surface
to make the five required tests pass at the substrate layer, mirroring how
`tests/alloc_zeroed_fresh_large_skip.rs` tests the Large path at the substrate
layer) is **fully testable but production-inert**. Verified this session:

```text
=== HeapCore::alloc_zeroed delegation to AllocCore::alloc_zeroed ===
$ rg -n "self\.core\.alloc_zeroed|core\.alloc_zeroed" src/registry/
(no matches — exit 1)
```

The production entry point `HeapCore::alloc_zeroed`
(`src/registry/heap_core_alloc.rs:318-333`) handles the small arm by calling
`self.alloc(layout)` (which under `production`/`fastbin` routes through the
magazine fast path: `HeapCore::alloc` → magazine pop → `refill_magazine_slow` →
`refill_class_bump_checked` → `carve_batch`, `src/alloc_core/alloc_core_small_magazine.rs`)
and then applying an *unconditional* `Node::zero`. It never calls
`AllocCore::alloc_zeroed`. So a substrate-only prototype:

- **Would** make tests (a)–(e) pass against `AllocCore` directly — exactly the
  layer `tests/alloc_zeroed_fresh_large_skip.rs` tests, so the test scaffolding
  is proven and reusable.
- **Would NOT** benefit any production caller of `HeapCore::alloc_zeroed` /
  `SeferAlloc::alloc_zeroed` (the `GlobalAlloc::alloc_zeroed` path,
  `src/global/sefer_alloc.rs:569-575`) under the default `production` feature,
  because that path bypasses the substrate entry entirely.

To benefit production, the virgin bit must be **plumbed through the magazine**:
`carve_batch` produces the bit per carved block → the refill path stores it
alongside the slot in `PerClass.slots: [*mut u8; TCACHE_CAP]`
(`src/registry/tcache.rs:138`) → a magazine hit propagates the bit to
`HeapCore::alloc_zeroed`'s small arm, which then skips `Node::zero` iff true.
That storage question (a parallel `[bool; TCACHE_CAP]`, or a tag bit stolen from
the slot pointer given the alignment invariant, or a per-`PerClass` "all slots
virgin" short-circuit bit) is its own open design decision with cache-line and
hardening (`hardened`'s tagged-pointer scheme) interaction, and it is the
genuine remaining work — not a quick add-on. Rushing a substrate prototype that
does not touch it would give a green test suite for code that helps no production
caller, while consuming the correctness-review budget the orchestrator has
flagged as especially high for this task. **Design-only is the honest call.**

---

## 8. Win estimate — analytical from memset bandwidth (no code to measure)

For a *genuinely first-touch, never-reused* `alloc_zeroed` call (the only regime
the skip benefits), the optimization saves the full explicit `Node::zero` /
`memset(N)`. `Node::zero` is `core::ptr::write_bytes(ptr, 0, len)`
(`src/alloc_core/node.rs:131-151`), i.e. a standard memset.

**Defensible memset throughput on a modern x86-64 core** (single thread,
non-temporal for sizes exceeding L3, hot for cache-resident). The honest
bandwidth limits on commodity x86-64 (Zen 4 / Golden Cove / similar) for a
single-threaded `memset(0)`:

- **L1-resident (≤ 32 KiB): ~30-40 GB/s** effective (limited by L1 write
  bandwidth; vectorized store loop). Representative: `glibc` `memset` benchmark
  figures and the widely-cited ~32 KiB L1 ceiling.
- **L2-resident (32 KiB – 1 MiB): ~20-30 GB/s**.
- **L3 / DRAM-bound (≥ 1 MiB): ~10-15 GB/s** (non-temporal stores to DRAM; the
  allocator's `Node::zero` is a regular store, so for 1 MiB it straddles L3 and
  the effective rate is toward the lower end).

**Per-call savings (memset time = N / bandwidth), the win CEILING:**

| Size | Bandwidth (assumed) | memset time (saved per virgin call) |
|---|---|---|
| 4 KiB | 30 GB/s (L1) | ~130 ns |
| 16 KiB | 30 GB/s (L1/L2) | ~530 ns |
| 64 KiB | 25 GB/s (L2) | ~2.5 µs |
| 256 KiB | 15-20 GB/s (L2/L3 boundary) | ~12-17 µs |
| 1 MiB | 10-15 GB/s (L3/DRAM) | ~70-90 µs |

**Reading the table.** These are *best-case per-call* savings, realized only
when (a) the block is genuinely virgin (fresh carve in a clean-lifetime segment)
AND (b) the call would otherwise have dirtied the full N bytes. They are NOT
steady-state savings. A `vec![0u8; N]`-style workload that allocates, fills,
frees, and re-allocates the same size reuses blocks via the free list and gets
**zero** benefit (those blocks are served by `pop_free`, dispatch-reported non-
virgin, §2). The real win lives in:

- **Cold-start / first-heap calloc bursts** (interpreter startup, JIT code
  buffer zeroing, fresh page-cache buffers) — the same regime R8-8's Large skip
  targets, just at smaller sizes.
- **Genuinely append-only zeroed buffers** that grow into fresh carve territory
  and never recycle (e.g. a `Vec::with_capacity`-then-`resize(0)` pattern at
  medium sizes).

The 4 KiB row (~130 ns) is comparable to the allocator's own per-op metadata
cost (the `small_churn_16b` iai gate measures ~75 Ir/op, well under 100 ns of
pure-CPU work at ~3 GHz), so the skip is *not free* to invoke — the diagnostic
counter and the bit read cost a few cycles, which at 4 KiB eats into the ~130 ns
margin. At 16 KiB and up the skip is a clear net win per virgin call. **Below
~4 KiB the win is marginal; the interesting range is 16 KiB – 1 MiB**, which
overlaps the `medium-classes` target range (256 KiB – 1 MiB) — exactly where a
calloc-heavy workload pays the most memset bandwidth.

**Cross-check against P4(b) reason #3.** P4(b) called the win "narrow" because
"the churn/cold benches measure plain `alloc`, not `alloc_zeroed`." That remains
true: this report's numbers are a *ceiling* on a path the existing perf-gate
does not exercise. Closing that gap is the §11 first-stage follow-up (a dedicated
`alloc_zeroed` cold bench) — *before* any code lands, matching P4(b)'s flip-to-GO
criterion (i).

---

## 9. Kill-gate / verdict

| # | Criterion (the task's five risk areas + design completeness) | Target | Finding (this report) | Verdict |
|---|---|---|---|---|
| K1 | Risk 1 (pooling): can a pooled/reused block be marked virgin? | provably no | Dispatch test: pool reuse is via `pop_free` → signal `false` unconditionally. `carve_block` never runs on a pooled segment (§4.1). | **PASS** |
| K2 | Risk 2 (lazy-commit): is every commit-call-site that precedes a bump advance zero-guaranteed? | provably yes | Grow-on-carve is first-time commit, Windows-only (demand-zero). Recommit-on-`is_decommitted` is defensive dead code; design sets bit false if ever fired (§4.2). | **PASS** |
| K3 | Risk 3 (release vs decommit+recommit, macOS crux): can a decommit-reused macOS block be marked virgin? | provably no | `decommit_empty_segment_impl(release_follows=false)` has ZERO production callers (grep-verified, §4.3). R8-10 removed the only path that created the macOS-dangerous state. Design sets bit false defensively. | **PASS** |
| K4 | Risk 4 (batched carve): can a non-virgin block slip into a virgin-marked batch? | provably no | Bump monotonic within a lifetime → all blocks in a `carve_batch` run share the same single-bit signal; no intra-run transition (§4.4). | **PASS** |
| K5 | Risk 5 (remote-free): can the remote-free path race/corrupt the bit or elevate a remote-freed block? | provably no | Bit is owner-only (same discipline as `bump`, documented at `alloc_core_small.rs:1055-1060`). Remote-free writes disjoint metadata; reclaimed blocks re-enter via `pop_free` → signal `false` (§4.5). | **PASS** |
| K6 | miri caveat: does the design withhold the signal under `cfg!(miri)`? | yes, mirroring R9-1 | Bit = `cfg!(not(miri))` on fresh reserve; carve signal AND'd with `cfg!(not(miri))`; structurally identical to R9-1's Large-path gate (§5). | **PASS** |
| K7 | Design distinguishes itself from the 2026-07-10 P4(b) NO-GO? | yes, point-by-point | §6 rebuts reasons #1 (no per-block state — resolved by dispatch) and #2 (macOS — resolved by R8-10, dated 07-20 vs deep-audit 07-19); acknowledges reason #3 (narrow win). | **PASS** |
| K8 | Is the design airtight enough to prototype THIS session? | yes for substrate; **no for production** | Substrate prototype is testable but production-inert (`HeapCore::alloc_zeroed` does not delegate to `AllocCore::alloc_zeroed`, grep-verified §7). Production win requires magazine plumbing — an open design question, §11. | **DESIGN-ONLY** |

### Verdict

**DESIGN-ONLY, SHIPPED. GO for staged implementation (§11) on a future task;
NO-GO for rushing a prototype into this session.**

All five risk areas are resolved against the current (post-R8-10) substrate with
file:line evidence (K1–K5 PASS). The miri caveat mirrors R9-1 (K6 PASS). The
design explicitly rebuts the 2026-07-10 P4(b) NO-GO's first two reasons using
the R8-10 substrate that landed the day after the deep-audit re-flagged the risk
(K7 PASS). The reason no code ships is K8: a substrate-only prototype would be
production-inert under `production`/`fastbin`, and the production win requires
plumbing the bit through the magazine — a larger surface with its own open
storage-design question (§7, §11). Given the prior NO-GO history and the
orchestrator's flag of an "especially careful line-by-line safety review" for
this task, design-only is the honest, high-value outcome.

---

## 10. Explicitly NOT done this session

- **No `src/` change.** No `payload_virgin` field is added; no `carve_block` /
  `carve_batch` / `alloc_small` / `alloc_zeroed` signature is touched; no
  `SMALL_ZERO_PASS_CALLS` counter is added; no `cfg`-gating `virgin-zero-skip`
  feature is added to `Cargo.toml`.
- **No `tests/` change.** No virgin-skip regression test is added (the test plan
  is specified in §11.3, ready to lift verbatim when the implementation lands).
- **No `Cargo.toml` change.**
- **No miri run.** Per task constraint (miri is 17+ minutes per run on this
  repo). The miri code path is verified by reading (§5), not by execution.
- **No commit, no push, no `git add`.** Per task constraint — the orchestrator
  will review any future diff.

---

## 11. Staged implementation recommendation (for a future task)

**Stage 0 (measurement gate, ~1h, no `src/` change):** add a dedicated
`alloc_zeroed` cold-first-touch criterion bench (criterion fast profile, mirroring
`benches/perf_gate_iai.rs`'s shape) at sizes 4 KiB / 16 KiB / 64 KiB / 256 KiB /
1 MiB, comparing current `alloc_zeroed` against a `Node::zero`-only baseline.
**Exit criterion:** the measured memset cost at ≥ 16 KiB exceeds ~500 ns/op (the
§8 prediction), justifying the implementation cost. If the real number is below
that (host `memset` faster than estimated, or the magazine fast-path already
amortizes), **re-evaluate whether to proceed at all** — P4(b) reason #3 stands.

**Stage 1 (substrate prototype behind `virgin-zero-skip`, ~2-3h):** the minimal
airtight change. Add `payload_virgin: bool` to `SegmentHeader` with the §3 reset
rules; thread `(ptr, is_virgin)` through `carve_block` / `carve_batch` /
`alloc_small` (or a sibling `alloc_small_with_virgin` to avoid disturbing the
plain `alloc_small` signature — the cleaner option given there are 3 callers,
`alloc_core.rs:792,819` and `alloc_core_small_magazine.rs:57`); gate
`AllocCore::alloc_zeroed`'s small arm on the signal; add `SMALL_ZERO_PASS_CALLS`.
This stage is testable but production-inert (§7) — its value is proving the
invariant on the real substrate.

**Stage 2 (magazine plumbing, ~3-4h, the production win):** carry the bit from
`carve_batch` through `refill_class_bump_checked` into the magazine. Open design
question: per-slot storage. Three candidates, in rising complexity:
(a) a parallel `[bool; TCACHE_CAP]` in `PerClass` (simplest, +16 bytes/class,
cache-line pressure); (b) a single per-`PerClass` "all resident slots virgin"
short-circuit bit (cheapest, but must be cleared on the first non-virgin refill
into the class — a batched carve after a `pop_free`-sourced refill would need to
track this); (c) a tag bit stolen from the slot pointer (the slots are
`MIN_BLOCK`-aligned ≥ 16 B, so 4 low bits are free — but `hardened`'s tagged-
pointer scheme may already consume them; needs audit). Recommendation: start
with (a), measure the cache-line cost, consider (b) only if (a) shows regression.
Then wire `HeapCore::alloc_zeroed`'s small arm (`heap_core_alloc.rs:325-333`) to
consume the bit and skip `Node::zero` iff true. This is the stage that delivers
the §8 win to production callers.

**Stage 3 (promotion gate, ~1h):** re-run the Stage 0 bench with the Stage 2
plumbing live; confirm the measured win matches §8's ceiling at the cold-first-
touch regime and is ~zero on steady-state churn (the honest expectation). Only
on a green Stage 3 consider promoting `virgin-zero-skip` into `production` — and
even then, the prior P4(b) NO-GO history argues for a long soak under the
differential / miri / loom gates first.

### 11.3 Test plan (lift verbatim when Stage 1 lands)

New test file `tests/alloc_zeroed_virgin_small_skip.rs`, whole-file gated on
`#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]`, serialised
by a file-wide `Mutex` (mirroring `tests/alloc_zeroed_fresh_large_skip.rs:42-51`
— `SMALL_ZERO_PASS_CALLS` will be process-wide). Five tests mapped to the task's
§2 checklist:

- **(a)** `fresh_small_alloc_zeroed_is_all_zero_and_skips_zero_pass` — virgin
  carve via `AllocCore::alloc_zeroed` on a fresh segment reads back all-zero AND
  `SMALL_ZERO_PASS_CALLS` delta is 0 (real OS) / 1 (miri), mirroring
  `tests/alloc_zeroed_fresh_large_skip.rs:137-148`.
- **(b)** `dirty_freed_reallocd_small_still_zeroes` — the R9-1-style regression
  guard: `alloc` → write 0xAA to every byte → `dealloc` → `alloc_zeroed` same
  shape (must `pop_free`) → reads back all-zero AND delta is 1. Mirrors
  `tests/alloc_zeroed_fresh_large_skip.rs:156-242`.
- **(c)** `pooled_segment_alloc_zeroed_never_claims_virgin` — drive a segment
  through empty → pool → reuse (R8-10 path, `tests/small_segment_pool.rs` has
  the scaffolding), then `alloc_zeroed` from the pooled segment's free list:
  every block reads back all-zero AND delta increments per block (never virgin).
- **(d)** `interleaved_virgin_and_reuse_always_zero` — stress test mixing virgin
  carves (fresh segment) and reused blocks (same size class) in the same
  `AllocCore`, asserting every `alloc_zeroed` result reads back all-zero
  regardless of which path served it. Mirrors
  `tests/alloc_zeroed_fresh_large_skip.rs:244-306`.
- **(e)** the miri-gated delta assertion is inline in (a) (and a `cfg(miri)`
  constant-size shrink, mirroring `tests/alloc_zeroed_fresh_large_skip.rs:69-72`
  — full-buffer touches are byte-by-byte under miri).

---

## 12. Caveats

- **Single analysis host, no measurement performed.** §8's numbers are
  analytical from published-class x86-64 memset bandwidth, not measured on the
  analysis host; they are a ceiling with ~±30% uncertainty (the L1/L2/L3
  straddle regions are the least certain). Stage 0 (§11) is the gate that turns
  them into a measured figure.
- **The design's correctness hinges on §4.3's grep result.** The claim
  "`decommit_empty_segment_impl(release_follows=false)` has zero production
  callers" is the load-bearing fact that dissolves P4(b) reason #2. It was
  verified by grep this session against `main` @ `f469343`. **Any future change
  that re-introduces an in-place decommit of a registered small segment (e.g.
  re-enabling a B3-style decommit-on-pool-admission, or a new decommit policy)
  MUST also set `payload_virgin=false`** (per the §3 reset table), or the skip
  becomes unsound on macOS. The §11.3 test (c) does NOT catch this by itself
  (it exercises pooling, not in-place decommit); a dedicated test that drives
  the `release_follows=false` path (currently dead — would need a test-only
  hook) and asserts the bit goes false is the correct guard, and is noted for
  Stage 1.
- **The magazine-plumbing storage question (Stage 2) is genuinely open.** §11's
  three candidates are sketched, not analyzed; the `hardened` feature's tagged-
  pointer scheme (`src/alloc_core/remote_free_ring.rs`, `src/alloc_core/segment_header_gen_table.rs`)
  may interact with the tag-bit-in-pointer option. This is the real remaining
  design work and is why a production prototype is not rushed.
- **No `src/` or `Cargo.toml` was modified.** This is a documentation-only
  deliverable. The design is staged for a future code-change task (§11).
- **The prior NO-GO history is a feature, not a bug.** P4(b) (2026-07-10) and
  P2-3 (2026-07-19) are both cited and rebutted/updated here; this report does
  not pretend they didn't happen. R8-10 (2026-07-20) is the substrate change
  that re-opens the question, and the dating (P2-3 one day before R8-10) is the
  evidence that the re-opening is real, not a re-litigation of a settled call.
