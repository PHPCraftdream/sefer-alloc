# Checkpoint — 2026-07-09 [perf4-decommit-mechanism1-fix]

## Session summary

PERF-4 (tasks #216/#217, session task #14) moved from "queued investigation"
to a landed fix for **Mechanism 1** of the decommit-churn regression the
shamir-db 47-target sweep flagged (0.3.0 ~15–18% slower than 0.2.1 on
"many short-lived small segments cycling quickly"). The 2026-07-09 performance
review (`docs/reviews/2026-07-09-performance-review.md`, finding F1) identified
the concrete mechanism; this session implemented the cheap, safe half of the
fix and added a dedicated regression bench. **Mechanism 2 (hysteresis) is NOT
implemented** — it remains the next step, deliberately deferred (it needs its
own judge-measured experiment cycle before a design is chosen).

## What was done (Part A — Mechanism 1, dead work before release)

**Mechanism 1** — on every emptied non-primordial small segment, the three
production sites that observe `live_count == 0`
(`dealloc_small`, the ring-drain in `find_segment_with_free_impl`, `flush_run`)
call `dec_live[_batch]_and_maybe_decommit`, which ran the FULL
`decommit_empty_segment` reset (an `os::decommit_pages` syscall on ~4 MiB of
payload, zeroing 49 `BinTable` heads, re-marking ~1 KiB of page-map entries, a
32 KiB `AllocBitmap` byte-wise re-init, and the `RunStack` clear) — and then
IMMEDIATELY called `self.table.recycle(base)`, which returns the ENTIRE
reservation to the OS (`os::release_segment` → `MEM_RELEASE`/`munmap`),
discarding every metadata page microseconds later. All of that reset except the
`bump` cursor is **dead work**.

**Fix:** added `AllocCore::decommit_empty_segment_for_release` (a
release-follows fast path) that does ONLY `meta.set_bump(payload_start)` (+ the
`set_decommitted(true)` flag for parity and the `DECOMMIT_CALLS` diagnostic
counter). `set_bump` is the sole load-bearing action: within a single ring
drain, subsequent stale ring entries for the same `base` are rejected by the
`off >= bump` guard in `reclaim_offset` / `dealloc_small` BEFORE they ever
consult the bitmap / bin table / page map, so the rest of the reset is
unobservable once the reservation is released.

The three empty-observing callers all recycle immediately → all use the fast
path. The former full-reset `decommit_empty_segment` is kept
(`#[allow(dead_code)]`) as the correct implementation for a hypothetical
decommit-WITHOUT-immediate-release path (e.g. the Mechanism-2 hysteresis pool,
or the currently-unreachable recommit-on-reuse branch in
`carve_block`/`carve_batch`). There is currently **no production path that
decommits without an immediate recycle**, verified by enumerating every caller
of `decommit_empty_segment` / `dec_live[_batch]_and_maybe_decommit`.

### Files changed
- `src/alloc_core/alloc_core.rs`
  - new `decommit_empty_segment_for_release` + shared
    `decommit_empty_segment_impl(release_follows: bool)`; old
    `decommit_empty_segment` now a thin `#[allow(dead_code)]` wrapper over the
    `false` (full-reset) branch.
  - `dec_live_and_maybe_decommit` and `dec_live_batch_and_maybe_decommit` now
    call the `_for_release` variant.
- `benches/perf_gate_iai.rs` — new iai bench `seg_cycle_decommit_256k` (the
  canonical judge; Linux/Valgrind-only).
- `benches/global_alloc.rs` — new criterion bench group
  `segment_decommit_cycle` (wall-clock, runnable on Windows) vs mimalloc/System.

### Behaviour preserved
- `dbg_decommit_count` still advances on every emptied segment (counter kept in
  the fast path) — the soak / regression tests are unchanged.
- End-user semantics of decommit+recycle are byte-identical; only redundant
  work before the OS release is removed. All decommit / regression tests green
  (`decommit_soak`, `decommit_miri_cycle`, `decommit_stale_ring`,
  `heap_core_tcache_decommit`, `regression_c3_unbounded_recycle`,
  `regression_own_segment_cache_invalidation`, `regression_batch_flush`,
  `regression_batch_freelist_drain`, `regression_bump_direct_refill`,
  `regression_run_stack_decommit` under `alloc-runfreelist`).

## NEXT STEP — Mechanism 2 (hysteresis) — NOT yet implemented

The dominant cost on this workload is NOT the dead metadata reset (Mechanism 1,
now removed) but the **OS reserve→commit→release syscall round-trip per
cycle** with a zero decommit/release threshold. A workload oscillating around a
segment-fill boundary pays, on every cycle: reserve (syscall) + metadata init +
first-touch page faults, then decommit + release. The Large path is shielded by
`large_cache` (8 slots + budget + decay); small segments have no analogue.
mimalloc holds a purge delay (tens of ms) here.

The `segment_decommit_cycle` criterion bench is the intended harness for this
residual (corrected numbers below; the earlier "milliseconds/round, ~500× gap"
figures were an artifact of a mis-sized bench block — see the correction note).

**Mechanism 2 = a hysteresis pool of the last N empty committed small
segments** (don't release immediately; keep a few for reuse; release on a decay
timer / budget). This is a separate, riskier architectural feature. Per the
review and the original #217 plan: **measure via the judge (`npm run iai`)
before choosing a design.** Not started.

## Correction — the original bench measured the WRONG path (invalidates the ~500× figure)

The numbers first recorded here (SeferAlloc ~2.28 ms/iter, mimalloc ~4.34 µs,
a "~500× gap = Mechanism-2 syscall-per-cycle signature") were **invalid**: the
`segment_decommit_cycle` / `seg_cycle_decommit_256k` benches used a block size
of **262,144 B (literal 256 KiB)**, but `SMALL_MAX` — the largest small size
class — is **258,752 B (≈253 KiB)**. Since `262144 > 258752`, every request
routed to the **dedicated-segment Large path**, where
`dec_live_and_maybe_decommit` bails on `kind != Small` and
`decommit_empty_segment_for_release` (the fix this bench exists to guard) is
**never reached**. The ~2.28 ms was Large reserve/release churn, not the small
decommit→recycle cycle; the "500× gap" was a mis-routing artifact. Verified via
`AllocCore::dbg_decommit_count`: at 262,144 B the counter never advanced.

**Fix (bench-only, adversarial-review follow-up):** block size → `258_752`
(SMALL_MAX exactly, Small path) in both benches, and batch → **34** (was 18).
The batch had to grow because one 4 MiB segment holds only **15 usable**
258,752 B blocks (16 fit, but the primordial reserves one block's worth for its
registry and each fresh small segment loses one to per-segment metadata), and a
batch that only spills into the SECOND segment does not decommit — that second
segment is still the current carve target (excluded from decommit). 34 blocks =
15 (primordial) + 15 (seg 2) + 4 (seg 3), leaving seg 2 non-current so freeing
the batch empties it → decommit fires. Verified: at 258,752/batch-34/6-rounds
`dbg_decommit_count` advances by exactly **6** (one decommit per round); at
262,144 it stays 0.

## Bench numbers (corrected, criterion, fast profile, Windows)

`cargo bench --features "production alloc-decommit" --bench global_alloc -- segment_decommit_cycle`
(34 × 258,752 B alloc then free-all → spans 3 small segments, non-current seg 2
empties → decommit → recycle, per iteration):

- SeferAlloc/253KiB: ~1.82 µs   (median; [1.78, 1.90] µs)
- mimalloc/253KiB:   ~7.78 µs
- System/253KiB:     ~364 µs

On the CORRECT Small-decommit path SeferAlloc is now ~4× FASTER than mimalloc
here (not 500× slower) — i.e. the earlier "residual Mechanism-2 gap" was never
real on this shape; it was the Large path. Absolute numbers are wall-clock and
un-profiled; the bench's value is as a directional regression/gain harness. The
deterministic judge number (iai `seg_cycle_decommit_256k`) requires
Linux/Valgrind and was not captured on this Windows host. NOTE: the Mechanism-2
hysteresis-pool motivation should be re-evaluated against these corrected
numbers before any design work — the syscall-per-cycle "500×" premise is gone.

## Open questions for the reviewer

- Confirm the counterfactual: does removing `set_bump` from the fast path (only)
  reintroduce a stale-ring UAF? (Expected yes — it is the one load-bearing
  action; `decommit_stale_ring` should catch it.) The other four elided steps
  were argued dead by the `off >= bump` guard ordering — worth a second read.
- Is keeping the full-reset `decommit_empty_segment` as `#[allow(dead_code)]`
  the right call, vs deleting it until Mechanism 2 needs it? Kept it because it
  is the correct decommit-without-release implementation and Mechanism 2 will
  want exactly it.
- Mechanism 2 design + judge-measured go/no-go — still entirely open.
