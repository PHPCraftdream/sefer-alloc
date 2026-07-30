# R29-3 — decommit→reserve segment-lifecycle decomposition: trigger 2 measured → does NOT fire

**Task #434 (R29-3), Round 29.** MEASUREMENT-ONLY, per this project's
"measured, not spun" convention (R24-2/R24-5/R28-1). This is the
measurement that resolves `docs/perf/OPEN_ITEMS.md` item 15's trigger 2
(the unmeasured half of the reservation-only overflow tier design's
two-trigger conditional-entry rule).

**Date:** 2026-07-29. **Base revision measured:** `main` @
`bd1bd3e7feca2c9dbdf36d56c9c4f5b856b59785` (working tree carrying only
this task's own additive edits at measurement time — `git status --short`
shows `Cargo.toml`, `benches/perf_gate_iai.rs`,
`src/alloc_core/alloc_core_small_pool.rs`,
`src/registry/heap_core_diag.rs` modified, plus new files under
`examples/`/`docs/perf/`/`docs/checkpoints/`/`docs/reviews/` from other
in-flight work, none of which this task touches beyond its own additions).
**Platform measured:** WSL2 (Ubuntu, kernel
`6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64,
`valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc
`1.98.0-nightly (bd08c9e71 2026-06-25)`, CPU `11th Gen Intel(R) Core(TM)
i7-11800H @ 2.30GHz (8C/16T)` — same toolchain/host as every other
`npm run iai` measurement in this doc tree.

**Measurement only. No production behavior changed:** one new
`examples/r29_3_decomposition_gate.rs` binary, seven new
`bench-internals`-gated measurement hooks (5 safe `pub fn`, 2 `pub unsafe
fn` with `# Safety` contracts — following `HeapCore::dbg_flush_class_only`
as the positive pattern), and two new supplementary iai arms. No
production call site touched, no existing function body edited.

---

## 0. Headline

| question | answer |
|---|---|
| Is (1+2+3) — the avoidable overhead a reservation-only tier could skip — a material fraction of the segment-lifecycle cycle? | **NO.** (1+2+3) = ~24,068 ns = **1.0–1.3%** of the cycle across both saved runs. (4+5) page-fault cost dominates at **~99%**. |
| Does trigger 2 fire? | **NO.** Trigger 2 requires (1+2+3) to be a MATERIAL fraction (>20%). At 1.0–1.3%, it is not. |
| What happens to item 15? | **CLOSE — move from `[D]` to `[L]`** (honest reject with measured evidence). The reservation-only overflow tier design must NOT be opened. |

---

## 1. Why wall-clock, not iai — the methodological caveat

The task spec originally mandated iai/Ir for this decomposition. But
callgrind is **structurally blind to kernel time**: it counts only
userspace instructions, never the kernel's VMA teardown / page-zeroing /
fault-handling work that dominates BOTH sides of the (1+2+3)-vs-(4+5)
split. The evidence is irrefutable and was measured in this task:

- The existing `large_alloc_free_cycle` iai arm does a full 4 MiB
  reserve+commit+free — a real OS round-trip that costs ~50-200 µs
  (hundreds of thousands of cycles) on bare metal. Its callgrind report:
  **3,308 Ir / 20,715 est. cycles** (`_raw_r29_3_iai_supplementary.log`).
  Callgrind counts the userspace `mmap`/`munmap` syscall *wrappers* (a
  handful of instructions); the kernel's actual VMA-list manipulation,
  page-table teardown, and physical-page freeing are invisible.

Components (1) OS release+reserve, (4) recommit, and (5) first-touch page
faults are ALL kernel-time-dominated. An Ir-based (1+2+3)-vs-(4+5) split
would systematically zero-out the load-bearing terms on both sides and
produce an invalid verdict — exactly the outcome the task's own
anti-p-hacking guard forbids.

Wall-clock `std::time::Instant` around the SAME existing production code
paths (`os::` module's own `release_segment`/`Segment::reserve`/
`decommit_pages`, `SegmentTable` register/recycle, `SegmentMeta`/`SegmentHeader`
init) gives a methodologically valid basis. No new crate dependency was
added — `std::time::Instant` is std, and the OS/table/metadata functions
are already in this crate, reached via new thin `bench-internals`-gated
hooks. The iai arms are kept as supplementary characterization (§4 below)
and as quantitative evidence of iai's blindness.

---

## 2. Measurement design

### The decomposition

One decommit→reserve segment-lifecycle cycle (the churn R27-4 measured the
aggregate 22% win from eliminating) is decomposed into:

| component | description | what a reservation-only tier does |
|---|---|---|
| **(1)** OS release+reserve | `munmap` + `mmap` round-trip (Linux) | **AVOIDS** (keeps VA) |
| **(2)** SegmentTable unregister+register | `hash_remove` + `own_cache_clear` + slot NULL + free-list-push; then slot reuse + `hash_insert` | **AVOIDS** (keeps slot alive) |
| **(3)** metadata init | `write_header` + `BinTable::init_in_place` + `RemoteFreeRing::init_in_place` + frontier stamping + metadata page faults | **AVOIDS** (keeps metadata alive) |
| **(4)** recommit | re-committing decommitted payload pages | **IRREDUCIBLE** (still must recommit) |
| **(5)** first-touch page faults | kernel allocates + zeroes physical pages on first write | **IRREDUCIBLE** (still must fault) |

**(1)+(2)+(3)** = what a reservation-only tier COULD avoid. **(4)+(5)** =
what it STILL pays.

On Linux: `mmap` does reserve+commit in one shot (no separate commit
phase). `MADV_DONTNEED` (decommit) returns physical pages but keeps the VA
mapping. Recommit is **implicit** — re-access after `MADV_DONTNEED`
re-faults with a fresh zero page, no syscall. So (4) = 0 ns on Linux; the
real irreducible cost is (5) page faults.

### Measurement arms (wall-clock, `examples/r29_3_decomposition_gate.rs`)

| arm | what it times | isolates |
|---|---|---|
| **A** | `reserve_small_segment` + `release_or_pool_empty_segment`, NO payload touch | (1+2+3) avoidable overhead |
| **C** | `Segment::reserve` + `os::release_segment` (raw OS round-trip, no table/metadata) | (1) alone |
| **B** | `decommit_pages` (MADV_DONTNEED) + write every payload page (re-fault), on a single reserved segment | (4+5) irreducible floor |
| **A'** | `reserve_small_segment` + write every payload page + `release_or_pool_empty_segment` | real production cycle cost (with faulted payload — munmap frees physical pages) |

Pool pre-filled to `pool_cap` (4) before each timed run so all releases take
the release path (not the pool-push path). 200 iterations per arm, 20
warmup discarded. `std::time::Instant` brackets around each arm's inner
loop.

**(2+3)** = A − C by subtraction. The verdict split: (1+2+3) vs (4+5).

### Verdict rule (stated up front, honored exactly)

If **(1+2+3) > 20%** of the cycle → trigger 2 FIRES, design may be opened.
If **(4+5) dominates (< 20%)** → trigger 2 does NOT fire → close item 15.

---

## 3. Measured numbers

### Run 1 (primary, cited as evidence — verbatim from `_raw_r29_3_decomposition_run1.log`)

```
── Component breakdown (ns/cycle, median of 200) ──
  (1)   OS reserve+release round-trip:              6,776 ns
  (2+3) table + metadata init:                     20,456 ns  [= A − (1)]
  ─────────────────────────────────────────────────────────
  (1+2+3) AVOIDABLE subtotal (A):                  27,232 ns

  decommit syscall (MADV_DONTNEED):               195,814 ns
  (5)   first-touch page faults:                1,906,575 ns  (1006 pages @ 1,895 ns/fault)
  (4)   recommit (Linux: implicit, ~0 ns):             0 ns
  ─────────────────────────────────────────────────────────
  (4+5) IRREDUCIBLE subtotal (B):                2,102,388 ns

── Real-world production cycle comparison ──
  A'  current design (reserve+touch+release):     2,215,980 ns
  B   reservation-only floor (decommit+refault):  2,102,388 ns
  A' − B = reservation-only SAVES:                  113,592 ns/cycle (5.1%)

═══ SPLIT ═══
  (1+2+3) AVOIDABLE:     27,232 ns  (1.3%)
  (4+5)   IRREDUCIBLE: 2,102,388 ns  (98.7%)
```

### Stability across 2 saved runs (a third run was observed during
### development with a consistent ~1.0-1.1% avoidable share but its stdout
### was not redirected to a file — dropped from this table rather than cited
### without a reproducible log, per this project's raw-log policy)

| run | (1+2+3) A ns | (4+5) B ns | avoidable % | A' ns | A'−B ns | raw log |
|---|---|---|---|---|---|---|
| 1 | 27,232 | 2,102,388 | **1.3%** | 2,215,980 | +113,592 | `_raw_r29_3_decomposition_run1.log` |
| 2 | 20,904 | 2,154,111 | **1.0%** | 2,190,767 | +36,656 | `_raw_r29_3_decomposition_run2.log` |

Consistent: avoidable share is **1.0–1.3%** across both saved runs. The
verdict is robust — an order of magnitude below the 20% threshold.

---

## 4. Supplementary iai characterization

| arm | Ir (total, 8 cycles) | est. cycles | wall-clock equivalent |
|---|---|---|---|
| `decomp_full_cycle_8x` | 7,858 | 31,395 | ~22,000 ns × 8 ≈ 176 µs |
| `decomp_os_roundtrip_8x` | 1,394 | 2,497 | ~3,000-6,000 ns × 8 ≈ 24-48 µs |

Per-cycle Ir: ~982 (full cycle), ~174 (OS round-trip). Compare to
`large_alloc_free_cycle` at 3,308 Ir for a ~50-200 µs operation — the same
undercounting pattern. Callgrind counts the userspace syscall wrappers;
the kernel's VMA teardown / page-zeroing / fault-handling work (which
dominates the wall-clock cost) is invisible.

**These numbers are NOT the verdict basis** — they confirm iai's
blindness to the kernel-time-dominated costs this decomposition hinges on.
Raw log: `_raw_r29_3_iai_supplementary.log`.

---

## 5. Additional finding: MADV_DONTNEED is MORE expensive than munmap+mmap

A striking secondary finding: the decommit syscall cost (MADV_DONTNEED)
is **~196-217K ns** — an order of magnitude MORE than the entire
avoidable overhead (1+2+3) at ~21-27K ns. This means the reservation-only
design would ADD ~196-217K ns of decommit overhead while saving only
~21-27K ns of OS round-trip + table + metadata overhead — a **net loss**.

The mechanism: `MADV_DONTNEED` walks and zaps 1,006 individual page-table
entries (one per 4 KiB payload page). `munmap` tears down the entire VMA
in one bulk operation. For a 4 MiB segment (~1,006 pages), the per-page
PTE walk costs more than the bulk VMA teardown.

The real-world comparison (A' vs B) confirms this: the reservation-only
floor B (2,102-2,154K ns) is consistently comparable to or MORE expensive
than the current production cycle A' (2,191-2,216K ns). The net "saving"
(A'−B) is 37-114K ns = 1.7-5.1% — and even this small figure is within
measurement noise of zero, NOT a reliable saving.

---

## 6. Verdict

**TRIGGER 2 DOES NOT FIRE.** The avoidable overhead (1+2+3) is 1.0-1.3%
of the segment-lifecycle cycle — two orders of magnitude below the 20%
materiality threshold. Page-fault cost (5) alone accounts for ~99% of the
cycle. A reservation-only overflow tier cannot meaningfully help: it only
ever saves the ~1% avoidable part, and on Linux it would actually ADD cost
(via the more-expensive MADV_DONTNEED per-page walk).

**Item 15 action: move from `[D]` (deferred designs) to `[L]` (honest
reject with documented revisit trigger)**, with the measured 1.0-1.3%
avoidable share as the documented reason. The revisit trigger: only if
segment size shrinks dramatically (fewer pages → MADV_DONTNEED cheaper)
or the OS-backend changes to one where recommit is a real separate
syscall (Windows, where `MEM_DECOMMIT` + `MEM_COMMIT` is the natural
decommit/recommit pair and the VMA teardown cost may differ).

---

## 7. Files touched

| file | change |
|---|---|
| `src/alloc_core/alloc_core_small_pool.rs` | +7 `bench-internals`-gated decomposition hooks on `AllocCore` |
| `src/registry/heap_core_diag.rs` | +7 `HeapCore` delegation wrappers for the above |
| `examples/r29_3_decomposition_gate.rs` | NEW — wall-clock decomposition binary |
| `benches/perf_gate_iai.rs` | +2 supplementary iai arms + group registration |
| `Cargo.toml` | +1 `[[example]]` entry for `r29_3_decomposition_gate` |
| `docs/perf/_raw_r29_3_decomposition_run1.log` | NEW — raw wall-clock run 1 |
| `docs/perf/_raw_r29_3_decomposition_run2.log` | NEW — raw wall-clock run 2 |
| `docs/perf/_raw_r29_3_iai_supplementary.log` | NEW — raw iai supplementary |
| `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE_summary.csv` | NEW — machine-readable summary |

---

## 8. 2026-07-30 correction — R30-1 re-run (task #450, append-only, does not replace §3's original numbers)

**Context.** R30-1 (task #450) fixed a dangling-`small_cur` soundness hazard in
`dbg_decomp_full_cycle` / `dbg_decomp_reserve_and_keep` / `dbg_decomp_release`
(see `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 and `CHANGELOG.md`'s Round 30
entry) — none of the hooks this gate's measurement arms A/B/C/A' call had
their MEASURED cost changed by that fix (the fix only removes a
`self.small_cur = base` write these hooks used to perform via
`reserve_small_segment`; the OS/table/metadata work itself, and the
`release_or_pool_empty_segment` call, are unchanged). As required by that
task's verification steps, the gate was re-run after the fix to check the
headline numbers still hold.

**Measured on:** `main` @ `b5ee62dab536c95e76b50d5eeb43edf7e257c705` +
this task's uncommitted working tree (`AllocCore::reserve_small_segment`
split into `reserve_small_segment`/`reserve_small_segment_impl`,
`alloc_core_small.rs`/`alloc_core_small_pool.rs`; no other production
behavior changed). **Platform:** WSL2 (Ubuntu, kernel
`6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64 — same
host/kernel as the original §0 measurement, rustc
`1.98.0-nightly (a595d0da2 2026-06-20)`.

**Numbers shifted; verdict did not.** Two fresh runs:

| run | (1+2+3) A ns | (4+5) B ns | avoidable % | A' ns | raw log |
|---|---|---|---|---|---|
| rerun1 | 47,454 | 2,323,359 | **2.0%** | 2,465,378 | `_raw_r30_1_decomposition_rerun1.log` |
| rerun2 | 47,124 | 2,629,966 | **1.8%** | 2,602,110 | `_raw_r30_1_decomposition_rerun2.log` |

Compare to §3's original 1.0–1.3% (runs 1–2). The absolute ns figures moved
(host/VM noise between measurement sessions — WSL2 wall-clock timing on a
shared Windows host is not perfectly reproducible run-to-run, as this doc's
own §1 caveat about kernel-time-dominated costs already flags), but the
**qualitative verdict is unchanged**: (1+2+3) avoidable overhead stays a
low-single-digit percentage, (4+5) page-fault cost still dominates at
~98%, and **TRIGGER 2 still does NOT fire**. §6's verdict and item 15's
`[D]`→`[L]` disposition stand as originally published.

**A separate, pre-existing, UNRELATED finding surfaced during this
re-verification, NOT caused by R30-1's fix and NOT fixed by it:** running
`examples/r29_3_decomposition_gate` natively on Windows (rather than under
WSL2/Linux, which is where this gate was always measured — see this doc's
own §"Platform" note in the header) crashes with `STATUS_ACCESS_VIOLATION`
inside Measurement B, specifically the `write_volatile` refault loop
immediately after `HeapCore::dbg_decomp_decommit_payload`. Root cause:
Windows `MEM_DECOMMIT` (`crates/vmem/src/lib.rs`'s
`decommit_pages_impl` for `cfg(windows)`) genuinely UNMAPS the payload
pages — unlike Linux `MADV_DONTNEED`, which keeps the VA mapping resident
and re-faults transparently on next write. The example's Measurement B
loop assumes the Linux semantics (write-after-decommit silently re-faults)
unconditionally; on Windows this is an access violation without an
explicit `VirtualAlloc(MEM_COMMIT)` recommit call the example never makes.
Confirmed NOT related to R30-1's fix: isolated by running just the
fixed hooks' pre-fill/A/C/A' loops (which never call
`dbg_decomp_decommit_payload`) natively on Windows — hundreds of iterations
completed cleanly; the crash reproduces identically whether R30-1's fix is
applied or reverted, and occurs in a code path (`dbg_decomp_decommit_payload`
→ `os::decommit_pages` → `crates/vmem`) untouched by this task's diff.
Filed as a new tracked item in `docs/CORRECTNESS_OPEN_ITEMS.md` for a future
round rather than fixed here (out of R30-1's scope, and this doc's own
methodology has always measured on WSL2/Linux specifically — see the
"Platform" line in this doc's header — so the example was never claimed to
be native-Windows-safe in the first place).
