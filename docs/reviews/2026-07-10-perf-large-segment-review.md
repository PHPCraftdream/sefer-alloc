# Performance review — large-allocation & segment lifecycle headroom post-PERF-4 (2026-07-10)

**Scope:** perf headroom in the large-allocation and segment lifecycle
path (segment creation, decommit, release, recommit, large-object cache)
now that PERF-4's specific bug is fixed. **Method:** fxx (Fable-5,
effort=max) research agent, read-only investigation of
`src/alloc_core/alloc_core.rs` (alloc_large/dealloc/reserve_small_segment/
carve/decommit), `src/alloc_core/large_cache_config.rs`,
`crates/vmem/src/lib.rs`, `src/alloc_core/os.rs`,
`src/alloc_core/segment_table.rs`, `src/alloc_core/alloc_bitmap.rs`,
`src/alloc_core/segment_header.rs`, plus `docs/perf/IAI_BASELINE.md`,
`docs/perf/FAULT_PROBE.md`, both PERF-4 checkpoints, and the iai benches.
No source files were modified; no findings below have been implemented.

## Already ruled out / not re-proposed

- Segment lookup (`contains_base`) is already O(1) — hash + own-cache +
  tombstone rebuild. No headroom there.
- `find_segment_with_free` O(n_segments) scan is a **known X5
  honest-reject at n=3** (`IAI_BASELINE.md` §X5) — gated on a ≥64-segment
  bench that doesn't exist yet. Not re-proposed.
- P4(b) `alloc_zeroed` virgin-skip (NO-GO 2026-07-10), X5 per-class
  segment queues/free-class bitmap (honest-reject at n=3), X4
  `TCACHE_CAP=32` and bloom-gate (rejected), X6 `clz` class_for
  (rejected), E2 REFILL_N LUT (rejected), fault-axis CI judge (declined,
  review F10) — all explicitly not re-proposed here.

## Ranked findings

### 1. Deferred small-segment recycle — keep the emptied segment registered as-is, release on decay — HIGH confidence on existence, MEDIUM confidence as top lever, MEDIUM risk

- **Files:** `src/alloc_core/alloc_core.rs:1070-1097`
  (`dec_live_and_maybe_decommit`), `:1121-1139` (batch variant), `:4053`
  (`reserve_small_segment`), recycle at `segment_table.rs:381`.
- The instant a non-current small segment's `live_count` hits 0, it is
  bump-reset and the ENTIRE reservation is released to the OS
  (`recycle` → `MEM_RELEASE`/`munmap`). The next allocation burst that
  needs a third segment pays a full OS reserve (3 syscalls, see finding
  2/4), `register` + hash insert, header write, PageMap init (1024B), a
  **32KiB AllocBitmap memset**, ring init, and first-touch page faults
  across the payload. `seg_cycle_decommit_256k` pays this once per round:
  marginal ~672 Ir/op vs ~131 Ir/op for pure churn — and the
  syscall/page-fault cost is mostly *invisible to Ir*, so the wall-clock
  gap is larger than the judge shows.
- **Fix direction:** don't recycle at `live == 0`. Cheapest sound form:
  leave the segment in the table untouched — its per-class freelists are
  fully populated at the moment it empties, so reuse via
  `find_segment_with_free` is literally zero extra work. Track "empty
  since tick T" (or "N empty segments retained", N=1-2) and release from
  the existing lazy decay tick (`maybe_decay_large_cache` already owns the
  clock) or when the count exceeds N. The existing
  `regression_c3_unbounded_recycle` test guards the unbounded-retention
  failure mode.
- **This IS the documented "Mechanism 2 (hysteresis)"** from
  `docs/checkpoints/2026-07-09-perf4-decommit-mechanism1-fix.md` —
  identified but never designed or attempted, with explicit instruction
  to re-evaluate the premise (the "500x gap" was a bench artifact;
  corrected wall-clock shows Sefer already ~4x faster than mimalloc on
  that cycle). New angle here: the checkpoint sketches a "pool of
  decommitted segments" (decommit + full reset + recommit-on-reuse); the
  keep-registered-as-is variant needs none of that machinery and reuses
  the already-existing recommit-on-reuse branches never.
- **This is the exact same recommendation as the companion churn-reuse
  review's finding 1** (`2026-07-10-perf-churn-reuse-review.md`) — that
  review measured the wall-clock cost of this mechanism directly (224ns/op
  vs 45ns/op achievable), providing the empirical justification this
  review's structural analysis independently arrived at.
- Must go through the judge (`npm run iai`, `seg_cycle_decommit_256k`)
  per the project's measure-before-fix rule.

### 2. Windows reserve: stop committing 2x and decommitting the trim — HIGH confidence, LOW risk

- **File:** `crates/vmem/src/lib.rs:341-378` (`reserve_aligned_raw`,
  Windows).
- Current: `VirtualAlloc(NULL, size+align, MEM_RESERVE|MEM_COMMIT)` then
  up to two `VirtualFree(MEM_DECOMMIT)` calls to trim head+tail. For every
  4MiB segment: **8MiB committed** (transient 2x commit-charge spike +
  page-table population for pages immediately discarded) plus **3
  syscalls**.
- Windows natively supports committing a sub-range of a reserved region:
  `VirtualAlloc(NULL, over, MEM_RESERVE)` (no commit) + `VirtualAlloc
  (base_aligned, size, MEM_COMMIT, PAGE_READWRITE)` = **2 syscalls, zero
  over-commit, zero trim work**. The release path is untouched —
  `VirtualFree(reservation, 0, MEM_RELEASE)` frees the whole reservation
  regardless of commit state.
- **Fix direction:** two-step reserve/commit in `reserve_aligned_raw`
  (windows). Every segment reservation in the process (small fresh, large
  slow path, seg-cycle re-reserve) benefits on the primary dev platform.
- **Caveat:** invisible to the iai judge (runs the Linux path under WSL);
  validate with `npm run bench:table` / the criterion
  `segment_decommit_cycle` bench.
- **This is the single highest-confidence recommendation of this
  review** — see summary below.

### 3. Fresh-segment metadata init elision — skip the 32KiB AllocBitmap memset on a virgin OS reservation — HIGH confidence on soundness, MEDIUM on magnitude, MEDIUM risk

- **Files:** `src/alloc_core/alloc_core.rs:4109-4116` +
  `src/alloc_core/bootstrap.rs:74-80`; loop at
  `src/alloc_core/alloc_bitmap.rs:80-86` (`FOOTPRINT = SEGMENT/MIN_BLOCK/8
  = 32KiB`, byte-wise loop).
- Every fresh small segment (and the primordial bootstrap) explicitly
  zeroes 32KiB of AllocBitmap and writes 1024 PageMap bytes — on memory
  the OS **guarantees is zero** (Windows MEM_COMMIT demand-zero; anonymous
  mmap zero-fill on Linux/macOS). The bitmap's init state is all-zeros, so
  the memset is a tautology on this path; worse, it eagerly dirties 8
  metadata pages that would otherwise fault lazily.
- **Fix direction:** in `reserve_small_segment`/bootstrap (the only
  fresh-reservation call sites), skip `AllocBitmap::init_in_place` under
  `cfg(not(miri))` (miri fallback is `std::alloc::alloc` = uninitialized,
  keep explicit zeroing there). The decommit-full-reset path
  (`decommit_empty_segment_impl`, release_follows=false — currently dead
  code) keeps its explicit re-init, so non-virgin reuse is unaffected.
  Optionally flip `PageClass::Free` from `0xFF` to `0`
  (`segment_header.rs:178-183`, classes become `c+1`) so PageMap init
  shrinks from 1024 writes to ~9 Meta-page writes.
- **NOT the rejected P4(b):** that NO-GO was about *per-block* virgin
  state for user-visible `alloc_zeroed` inside already-committed segments,
  where macOS `MADV_DONTNEED` laziness makes recycled payload legitimately
  non-zero. Here the "virgin" signal is exact (literally inside the
  fresh-reserve function), it's metadata not user payload, and the
  decommit-reuse path is excluded by construction. Different question.
- **Impact:** directly visible to the judge — cuts a few-thousand Ir per
  fresh segment from `cold_*`, `multiseg_cold_256k`,
  `seg_cycle_decommit_256k`, and 8 fewer dirty pages/segment in
  wall-clock. `IAI_BASELINE.md` itself names "32KiB bitmap-init" as a
  dominant bootstrap component.
- **Risk: medium** — needs a counterfactual test (poison-then-assert) and
  an audit that no third path calls `init_in_place` on dirty memory.

### 4. Unix reserve: try exact-size mmap first, over-reserve only on misalignment — MEDIUM confidence, LOW risk

- **File:** `crates/vmem/src/lib.rs:473-509` (`reserve_aligned_raw`,
  unix).
- Current: unconditionally mmaps `size+align` (8MiB for a 4MiB segment)
  then up to two munmap trim calls — 3 syscalls per reservation, every
  time. After the first aligned mapping, Linux's top-down mmap placement
  often returns already-4MiB-aligned addresses for subsequent whole-
  segment requests — and in the decommit→recycle→re-reserve cycle the
  kernel tends to hand back the same aligned hole just released.
- **Fix direction:** exact-size `mmap(size)` + alignment check succeeds in
  1 syscall in the common case; misaligned result → `munmap` + fall back
  to the existing over-reserve path (worst case 5 syscalls, rare).
  mimalloc uses the same opportunistic-alignment trick.
- **Risk: low** (fallback preserves current behavior exactly).
  Complementary to finding 2 (each platform gets its own reserve fix).

### 5. Large-cache hit: best-fit instead of first-fit — HIGH confidence on correctness, MEDIUM-LOW on measured impact, LOW risk

- **File:** `src/alloc_core/alloc_core.rs:3514-3524` (slot scan in
  `alloc_large`), `LARGE_CACHE_SLOTS = 8`, `LARGE_CACHE_SIZE_FACTOR = 2`
  (`:73-82`).
- The hit scan takes the FIRST slot with `usable <= slot.usable_size <=
  2*usable`. A 4MiB request can consume a cached 8MiB span while an exact
  4MiB span sits in a later slot — then the next 8MiB request MISSES and
  pays a full OS reserve+release round-trip that best-fit would have
  avoided, plus the 4MiB request holds 2x RSS.
- **Fix direction:** scan all 8 slots, pick the smallest compatible
  `usable_size` (O(8) on the cold large path — free).
- Needs a mixed-size large workload to demonstrate measured impact.

### 6. (Conditional on finding 1's design) `MADV_FREE` instead of `MADV_DONTNEED` for decommit-without-release — MEDIUM confidence, MEDIUM risk, currently near-moot

- **File:** `crates/vmem/src/lib.rs:519-534`.
- Currently near-moot: post-PERF-4 there is no production path that
  decommits without immediately releasing (full-reset
  `decommit_empty_segment` is `#[allow(dead_code)]`; the large cache
  deliberately keeps pages committed). If Mechanism 2 (finding 1) is ever
  built in the "decommit-on-retention" variant instead of keep-committed,
  Linux `MADV_FREE` makes the decommit→quick-reuse cycle nearly free
  (pages only actually reclaimed under memory pressure). Correctness
  already tolerates non-zero-fill semantics. Flagged only as a companion
  to finding 1 — the keep-registered variant of finding 1 makes it
  unnecessary.

## Summary recommendation

Fix the Windows reservation path in `crates/vmem/src/lib.rs:341-378`
(finding 2) — split the single `MEM_RESERVE|MEM_COMMIT` over-allocation
into reserve-only + a commit of just the aligned span. A small,
mechanically-verifiable change to one function with an untouched release
path; removes a guaranteed 2x transient commit charge, the page-table
population of ~4MiB of pages discarded microseconds later, and one
syscall per segment reservation on the platform this project develops and
wall-clock-benches on. It compounds with everything else because every
segment lifecycle idea above sits on top of this reserve primitive. The
keep-registered deferred-recycle (finding 1) has the larger theoretical
ceiling — corroborated independently by the companion churn-reuse review
— but it is documented Mechanism-2 territory that must go through a
judge-measured design cycle first, whereas finding 2 is a strict
improvement with no policy dimension.
