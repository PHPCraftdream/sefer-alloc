# R9-8 — Directory drift recovery: per-class miss-streak + OOM-rescue scan

**Task:** R9-8 (#230) — cheapen the directory self-heal's worst case under a
hypothetical directory-invariant drift, per an external review's finding on
the R8-2 (task #215) directory-authoritative-miss fast path.
**Outcome:** **GO (implemented).** Both requested fixes land as defense-in-depth
on the directory-trust fast path, with counterfactual tests proving each has
real teeth. No weakening of the directory's common-case correctness.
**Date:** 2026-07-20
**Base revision:** `main` @ `021c098` (R9-7 design-only just landed).
**Platform:** Windows 10 Pro x86-64 (native, single test process).

---

## 0. TL;DR

The R8-2 fast path trusts a directory MISS for up to
`DIRECTORY_MISS_FULL_SCAN_PERIOD - 1` consecutive misses before running a full
O(S) re-validation scan. Pre-R9-8 the streak was a SINGLE `u32` shared across
every size class, so a drift-affected class's rescan could be delayed by
cross-class traffic — worst case ~255 wasted 4 MiB segments (~1 GiB) before
detection. R9-8 makes two changes:

1. **Per-class miss-streak** — `directory_miss_streak` is now
   `[u8; SMALL_CLASS_COUNT]`, indexed by `class_idx`, and the period drops
   256 → **64** (per-class). A drifted class now trips its OWN rescan after 64
   of ITS misses, independent of other classes' traffic. Worst case caps at
   64 segments = **256 MiB** (4× tighter).
2. **Rescue scan before OOM** — right before the small path surfaces an OOM
   (`reserve_small_segment` returned `None`: segment table full or OS
   reservation failure), a forced O(S) linear scan runs as a last resort,
   bypassing the directory-trust. If it finds a real free block the directory
   hid, it self-heals the bit and serves that block instead of OOMing. A new
   counter `DIRECTORY_RESCUE_OOM_AVOIDED` distinguishes this from the periodic
   re-validation's `DIRECTORY_MISS_SELF_HEAL`.

A genuine drift remains **essentially impossible to construct** through the
invariant-preserving API (task #214's `assert_directory_equals_rebuild` oracle
proves the incremental directory tracks true state in every tested scenario);
both fixes are defense-in-depth against an undiscovered edge case or future
regression, exercised in tests via the established `dbg_directory_force_clear_bit`
test-only drift-manufacturing hook.

---

## 1. The review's finding — the worst case being cheapened

`find_segment_with_free_impl` (`src/alloc_core/alloc_core_small.rs`) trusts a
directory MISS (returns `None` immediately, no O(S) fallback scan) for up to
`DIRECTORY_MISS_FULL_SCAN_PERIOD - 1` consecutive misses, tracked by a SINGLE
field `self.directory_miss_streak: u32` shared across EVERY size class. Only on
the Nth consecutive miss does a full O(S) scan run as a periodic re-validation.

Because the streak is shared, if the directory's invariant were ever violated
for ONE class, every miss for that class trusts the (wrong) directory and
returns `None`, causing `reserve_small_segment` to carve a BRAND NEW 4 MiB
segment. This repeats up to `PERIOD - 1` times before the shared streak trips a
rescan — and in a busy multi-class workload the drifted class's own miss count
is a small fraction of the shared total, so its effective detection latency is
much worse than `PERIOD` of its own misses. Worst case: ~255 wasted 4 MiB
segments (~1 GiB address space; real commit charge on eager-commit Windows)
before detection.

## 2. Fix 1 — per-class miss-streak

### What changed

- **`src/alloc_core/segment_directory.rs`** — `DIRECTORY_MISS_FULL_SCAN_PERIOD`
  256 → **64** (per-class), with a const-assert that it fits the new `u8`
  per-class storage.
- **`src/alloc_core/alloc_core.rs`** — the field
  `directory_miss_streak: u32` → `directory_miss_streak: [u8; SMALL_CLASS_COUNT]`
  (init `[0; SMALL_CLASS_COUNT]`). `u8` keeps it at `SMALL_CLASS_COUNT` bytes
  (49 B default); the const-assert pins that a future period bump cannot wrap.
- **`src/alloc_core/alloc_core_small.rs`** — the directory-miss block indexes
  the streak by `class_idx`: each class's misses bump only its OWN slot, and the
  periodic re-validation resets only that class's slot.

### Why 64 (per-class), not 256

Pre-R9-8 the 256 was the TOTAL across all classes. Per-class, a shorter value is
defensible because a single class's own miss traffic is a much smaller fraction
of total allocator activity than all classes combined: 64 per-class achieves
comparable wall-clock detection latency to the old global 256 under realistic
multi-class load (a drifted class contributing ~1/4 of misses trips at its own
64th miss ≈ the same wall-clock point the global 256 would have), while
STRICTLY improving detection for a low-activity drifted class (which the shared
counter could starve indefinitely under busy healthy-class traffic). For a
SINGLE active class, 64 trips 4× sooner than the old 256 — strictly better
detection latency (the "equivalent-or-better when one class is active"
requirement), at the cost of ~1/64 ≈ 1.5% of that class's misses running a
re-validation scan that (for a healthy directory) finds nothing — negligible.

Worst case caps at 64 wasted 4 MiB segments = **256 MiB** (4× tighter than the
pre-R9-8 1 GiB), before the rescue scan backstops the OOM path on top.

### Single-class preservation

With one active class, the per-class array degenerates to a single non-zero
slot — the cadence is identical to a dedicated scalar with period 64. Test 1
(`authoritative_miss_skips_full_scan`, unchanged) and Test 2
(`periodic_revalidation_runs_every_period_misses`, unchanged — it reads the
period from the constant, so it simply runs 64 iters instead of 256) both
confirm the single-class behaviour is preserved and faster.

## 3. Fix 2 — rescue scan before OOM

### Trigger points

`reserve_small_segment` surfaces an OOM to its caller in two places: (a) OS
reservation failure (memory pressure) and (b) `table.register(base)` returning
`None` (segment table full, `MAX_SEGMENTS`). R9-8 intercepts at the **caller's
`None` branch** — the exact point where the OOM would surface to the user — in
both small-allocation entry points:

- **`alloc_small`** step-4 `None` branch (`alloc_core_small.rs`) — uses the
  unchecked rescue (`find_segment_with_free_forced`); this path is proven
  magazine-unreachable under `fastbin`, so no double-issue hazard.
- **`refill_class_bump_impl`** step-4 `None` branch (`alloc_core_small_magazine.rs`)
  — uses the CHECKED rescue (`find_segment_with_free_checked_forced`) under
  `fastbin`, so a cross-thread-freed magazine-resident block in a drained ring is
  NOT reclaimed (avoids a double-issue); the unchecked variant otherwise.

Both are gated on `alloc-segment-directory` + not-`numa-aware` + a materialised
sidecar — under `numa-aware` the directory is never trusted for lookups (the
linear scan runs every time), so there is no directory-trust hazard to rescue
from, and the rescue is correctly a no-op there.

### The forced scan

A new `rescue: bool` parameter on `find_segment_with_free_impl` (cfg-gated to
the directory feature, mirroring the existing `is_in_magazine` param) makes the
existing scan body reusable as a forced scan: when `rescue` is true, the
directory-trust `return None` is SKIPPED and the scan falls straight through to
the linear scan with the self-heal armed (`periodic_revalidation_active ||
rescue` at the heal sites). The streak is NOT touched in rescue mode (a one-shot
backstop, orthogonal to the periodic cadence). This reuses the EXISTING scan
body byte-for-byte — no duplication.

### Large allocations

`alloc_large_slow` does NOT consult the directory — Large segments have no
`BinTable` and are skipped in `find_segment_with_free_impl`. There is no
directory-trust hazard on the Large OOM path, so no rescue is wired there.

### Diagnostics

A new counter `DIRECTORY_RESCUE_OOM_AVOIDED` (read via
`dbg_directory_rescue_oom_avoided`) bumps once whenever the rescue scan finds a
block. The heal sites bump `DIRECTORY_MISS_SELF_HEAL` ONLY in periodic mode —
rescue-mode heals bump nothing at the site (the caller's counter covers it) — so
the two drift signals stay distinguishable: `DIRECTORY_MISS_SELF_HEAL` =
periodic re-validation found a stale-negative (the routine canary);
`DIRECTORY_RESCUE_OOM_AVOIDED` = a rescue scan avoided a spurious OOM. Both
indicate a directory bug and warrant investigation; neither is a normal event.

## 4. Tests

New file **`tests/r9_8_directory_drift_recovery.rs`** (cfg-gated identically to
`directory_authoritative_miss.rs`):

1. **`per_class_streak_decouples_rescans_across_classes`** — parks `class_x`'s
   streak at `period - 1`, drives ONE `class_y` miss, and asserts no rescan
   fires (per-class: `class_y`'s streak is 0 → 1) and `class_x`'s streak is
   unchanged. Then drives `class_y` to its OWN period (1 rescan) and confirms
   `class_x` is STILL untouched, then triggers `class_x`'s own rescan. **This
   is the counterfactual anchor** (see §5).
2. **`rescue_scan_finds_drifted_block_and_avoids_oom`** — manufactures drift
   (two live class_x blocks, `dbg_directory_force_clear_bit`), runs the rescue
   via the test-only `dbg_directory_rescue_scan` hook (the exact production OOM
   code path), and asserts: it finds the drifted segment, bumps
   `DIRECTORY_RESCUE_OOM_AVOIDED` by exactly 1, does NOT bump
   `DIRECTORY_MISS_SELF_HEAL` (distinguished), heals the bit, and is a safe
   no-op-when-healthy backstop (a second rescue on the now-healed directory
   still succeeds via the directory positive lookup).

Reaching a REAL OOM (`MAX_SEGMENTS` = 1024 live 4 MiB segments) is impractical
in a unit test; the test-only `dbg_directory_rescue_scan` hook invokes the
EXACT rescue code path the production OOM branches call
(`find_segment_with_free_forced` + `DIRECTORY_RESCUE_OOM_AVOIDED`), so the test
exercises the real mechanism without driving the table to capacity.

### Existing test adaptation (test 3)

`tests/directory_authoritative_miss.rs`'s
`self_heal_repairs_and_finds_segment` (Test 3) drove the streak via `class_y`
misses against the SINGLE shared counter — impossible under per-class streaks.
Its SETUP is adapted (not its assertions): the streak is now positioned directly
via the test-only `dbg_directory_set_miss_streak_for_class` hook (avoiding
polluting carves), and `small_cur` is moved off the drift segment via a new
`force_fresh_segment` helper (filling the current segment with `SMALL_MAX`
blocks — which touch `BinTable[small_max]`, never `BinTable[class_x]` — until a
fresh segment is registered). The three load-bearing assertions (heal returned
a target block, `DIRECTORY_MISS_SELF_HEAL` rose by 1, directory consistent with
rebuild) are byte-identical to the original. Tests 1 and 2 are UNCHANGED.

## 5. Counterfactual verification (non-vacuousness)

**Per-class decoupling** — temporarily routed all classes through streak slot 0
(simulating the pre-R9-8 shared counter). The decoupling test FAILED exactly at
its load-bearing assertion:

```text
test per_class_streak_decouples_rescans_across_classes ... FAILED
assertion `left == right` failed: a single class_y miss must NOT trip a rescan
  while class_x is at period - 1 ...
  left: 1
 right: 0
```

Under the shared simulation, the single `class_y` miss bumped the shared slot
from `period - 1` to `period`, immediately tripping a rescan (`left: 1`); under
per-class it must not (`right: 0`). Restored the per-class indexing; the test
passes. **The test has real teeth.**

**Rescue scan** — the rescue test's drift is manufactured via
`dbg_directory_force_clear_bit` (the same established test-only hook Test 3
uses); without the rescue scan, `dbg_directory_rescue_scan` would return `None`
(no forced scan, the directory miss is trusted) and the
`found.is_some()` / counter-rise assertions would fail. The rescue's
self-heal (`publish_nonempty` at the heal site under `rescue` mode) is what
makes the bit read SET afterwards — observable in assertion (d).

## 6. Verification results

```
cargo fmt --check                                         # clean
cargo clippy --all-targets -- -D warnings                 # clean (no-features)
cargo clippy --features experimental --all-targets -- -D warnings   # clean
cargo clippy --all-features --all-targets -- -D warnings  # clean
cargo clippy --features production --all-targets -- -D warnings      # clean
```

Directory test suite (all green):
```
tests/directory_authoritative_miss.rs .... 3 passed
tests/r9_8_directory_drift_recovery.rs .... 2 passed
tests/segment_directory_a5.rs ............. 5 passed
tests/segment_directory_a3.rs ............. 7 passed
tests/segment_directory_a2.rs ............. 6 passed
tests/segment_directory_a1.rs ............. 2 passed
tests/no_stale_doc_references.rs .......... 6 passed (test-file count bumped 175→178)
```

Full `cargo test --features "alloc-segment-directory alloc-stats"`: all green
(the only failure was the pre-existing-stale `tests/*.rs` file-count invariant
in `docs/ARCHITECTURE.md`, now updated to 178 — R9-6 had added a file without
bumping it).

## 7. Files changed

| File | Change |
|------|--------|
| `src/alloc_core/segment_directory.rs` | period 256→64 (per-class) + u8-fit const-assert + doc |
| `src/alloc_core/alloc_core.rs` | `directory_miss_streak: u32` → `[u8; SMALL_CLASS_COUNT]` |
| `src/alloc_core/alloc_core_small.rs` | per-class streak indexing; `rescue` param on `find_segment_with_free_impl`; forced wrappers; rescue at `alloc_small` OOM branch; heal-site `‖ rescue` |
| `src/alloc_core/alloc_core_small_magazine.rs` | rescue at `refill_class_bump_impl` OOM branch (checked under fastbin) |
| `src/alloc_core/directory_stats.rs` | new `DIRECTORY_RESCUE_OOM_AVOIDED` counter |
| `src/alloc_core/alloc_core_core_diag.rs` | streak read/set/reset-for-class hooks; rescue hook; rescue counter reader |
| `tests/directory_authoritative_miss.rs` | Test 3 setup adapted to per-class (streak setter + `force_fresh_segment`); Tests 1–2 unchanged |
| `tests/r9_8_directory_drift_recovery.rs` | NEW — decoupling + rescue tests |
| `docs/ARCHITECTURE.md` | test-file count 175→178 |

No R9-1..R9-7, `medium-classes-wide`, or pool/decommit files touched.
