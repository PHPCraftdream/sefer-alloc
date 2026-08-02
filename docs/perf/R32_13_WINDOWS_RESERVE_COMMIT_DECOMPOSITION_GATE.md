# R32-13 — Windows-native segment-lifecycle decomposition: the reservation path is small (4.3-4.8%), matching R29-3's Linux finding

**Task #504 (F11, from `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`).**
MEASUREMENT-ONLY, per this project's "measured, not spun" convention. This
is the Windows-native decomposition R29-3's own "Next trigger" named
verbatim as untested: *"the OS-backend changes to one where recommit is a
real separate syscall (Windows `MEM_DECOMMIT`+`MEM_COMMIT`, where the
VMA-teardown-vs-page-walk trade-off may differ)."* Windows is this
project's own development platform, so this task runs natively — no
WSL/Valgrind needed, unlike most of this backlog.

**Date:** 2026-08-03. **Base revision measured:** `main` @
`f126de1a77f01c6a33f605d0985a29cf71862ab5` (working tree carrying only this
task's own additive edits at measurement time — `git status --short`
showed `Cargo.toml`, `crates/vmem/Cargo.toml`, `crates/vmem/src/lib.rs`,
`src/alloc_core/alloc_core_core_diag.rs`, `src/alloc_core/alloc_core_small_pool.rs`,
`src/alloc_core/os.rs`, `src/registry/heap_core_diag.rs`,
`tests/dbg_hook_safety_tripwire.rs`, `README.md` modified, plus new files
under `examples/`/`scripts/`/`docs/perf/`, none of which this task touches
beyond its own additions). **Immutable source identity (R29-6 rule):** git
tree object `edc3656d3e57ed1fdc27f3e1b6bc3786411276ff` (`git write-tree`
after staging exactly this task's changed files — reproducible via
`git cat-file -p edc3656d3e57ed1fdc27f3e1b6bc3786411276ff`). **Platform
measured:** native Windows 10 Pro (build 19045), x86-64, CPU 11th Gen
Intel(R) Core(TM) i7-11800H @ 2.30GHz — the SAME CPU family R29-3's Linux
measurement ran on (via WSL2 on this same physical machine), rustc
`1.97.0 (2d8144b78 2026-07-07)`, cargo `1.97.0`.

---

## 0. Headline

| question | answer |
|---|---|
| Is the reservation path (avoidable overhead a reservation-only tier could skip) a material fraction of the Windows segment-lifecycle cycle? | **NO.** Median avoidable share across 3 runs = **4.60%** (range 4.33-4.78%). Page-fault cost dominates at **~95.4%**, matching R29-3's own Linux finding (1.0-1.3% avoidable) in DIRECTION, though the Windows figure is ~4x larger in absolute percentage terms (see §5). |
| Does the reservation path justify a `VirtualAlloc2` prototype (step 3)? | **NO.** 4.60% is far below the 20% materiality threshold this project's decomposition gates use (same threshold R29-3 used). Step 3 is NOT pursued by this task. |
| What is the Unix exact-mmap fast-path hit rate (step 1's other question)? | **Not measured on this run** (this machine is Windows-only, no Linux/WSL access for this specific task per its own scoping) — but the counter now EXISTS (`aligned_vmem::UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS`) and is proven wired correctly by this task's own oracle (§3.1: on a Windows host, both stay 0 throughout, which is the expected complementary result). A future Linux-side task can read it directly; no new instrumentation work is needed. |
| Which costs more on Windows: `VirtualAlloc(MEM_RESERVE)` or `VirtualAlloc(MEM_COMMIT)`? | **Commit costs ~2x more than reserve**, consistently across all 3 runs (median 9,133 ns vs 4,580 ns — see §3.2). This is a NEW finding neither R29-3 nor the original F11 survey entry anticipated. |

---

## 1. Why this is a NEW artifact, not a re-run of R29-3

R29-3 (`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`) already
decomposed the Linux segment-lifecycle cycle and found the entire avoidable
(non-page-fault) share is 1.0-1.3% — small. Its own "Next trigger" named
the untested case verbatim (quoted above). This task is that trigger, and
adds two things R29-3 never measured:

1. **The reserve-vs-commit SPLIT.** R29-3's "OS reserve+release
   round-trip" (its component 1) lumps `VirtualAlloc(MEM_RESERVE)` and
   `VirtualAlloc(MEM_COMMIT)` into one timed region — correct for Linux
   (where `mmap` commits eagerly in one call, so there is nothing to
   split) but too coarse for Windows, where they are unconditionally TWO
   separate syscalls (`crates/vmem/src/lib.rs`'s `win_reserve_commit`,
   `:800-866`). This task's new `dbg_decomp_win_reserve_only`/
   `_commit_only`/`_release_only` hooks isolate them.
2. **A path-activation oracle for the F11-step-1 counters** (see §3.1) —
   proving each measured arm actually hit the Windows code path it claims
   to measure, not some cached/fast-fail path, per CLAUDE.md's R30-8 rule.

## 2. Step 1 — Unix hit-rate / Windows call-count counters (trivial, shipped first)

Per the survey's own scoping ("step 1 is trivial and independently useful
even if you stop here"), two `bench-internals`-gated counter pairs were
added, extending the SAME `SEGMENTS_RESERVED_TOTAL`/`SEGMENTS_RELEASED_TOTAL`
pattern already plumbed through `src/alloc_core/os.rs` (confirmed at
`:52-57` — not `:374-386` as the survey guessed; the survey's own citation
was an approximate line reference, corrected here):

- `aligned_vmem::UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS` (`crates/vmem/src/lib.rs`) —
  hit/total pair around `try_reserve_aligned_exact`, settling the Unix
  "coin flip" question the survey computed a theoretical bound for but
  never measured.
- `aligned_vmem::WINDOWS_RESERVE_COMMIT_CALLS` — count of `win_reserve_commit`
  calls (2 syscalls each), for parity/comparison.

Both are `#[cfg(feature = "bench-internals")]`, `AtomicU64` storage always
compiled, increments gated — zero cost in a plain `production` build.
Surfaced via `AllocCore::dbg_unix_exact_reserve_attempts`/`_hits`/
`dbg_windows_reserve_commit_calls`/`dbg_reset_vmem_bench_internals_counters`
(`src/alloc_core/alloc_core_core_diag.rs`) and the matching `HeapCore`
delegations (`src/registry/heap_core_diag.rs`), following this session's
established `dbg_*` counter pattern (tasks #499/#501/#502).

`sefer-alloc`'s own `bench-internals` feature now forwards
`aligned-vmem?/bench-internals` (the `?` prefix matters here — unlike the
existing `aligned-vmem/lazy-commit` forward under `large-reserved-capacity`,
which is always reached via `alloc-core`, `bench-internals` has no such
transitive guarantee, so the optional-dependency-activation form is
required to avoid force-enabling `aligned-vmem` when `alloc-core` is off).

**This machine cannot measure the Unix hit rate directly** (native Windows
dev environment, no WSL for this task per its own scoping note). The
counter exists and is proven correctly wired (§3.1's oracle shows it stays
at exactly 0 on a Windows host, the expected complementary state) — a
future Linux-side task reads it with zero additional instrumentation work.

## 3. Step 2 — Windows-native decomposition (the genuinely new artifact)

### 3.1 Methodology and path-activation oracle

`examples/r32_13_windows_reserve_commit_decomposition_gate.rs` ports R29-3's
methodology (`examples/r29_3_decomposition_gate.rs`) verbatim for its
R29-3-comparable measurements (§3.3 below reproduces R29-3's exact A/A'/B/C
arm shapes through the SAME hooks), and adds the reserve-vs-commit split
(§3.2) as new arms.

**New hooks, all `bench-internals`-gated, `alloc-decommit`-gated** (mirroring
the R29-3 `dbg_decomp_*` cluster's own gating exactly):

- `AllocCore::dbg_decomp_win_reserve_only()` — reserves a `SEGMENT`-sized
  span with only the first page committed. Deliberately raw
  `os::Segment`-based (like `dbg_decomp_os_roundtrip`), NOT
  `ReservedSmallSegment` — no table bookkeeping, no metadata init, no
  owner-binding, isolating pure OS-level cost.
- `AllocCore::dbg_decomp_win_commit_only(base)` (`unsafe fn`) — commits the
  remaining `[PAGE, SEGMENT)` range via `os::commit_pages_for_measurement`,
  a NEW `bench-internals`-gated wrapper over `aligned_vmem::commit_range`
  that does not require any sefer-level lazy-commit POLICY feature
  (`primordial-lazy-commit`/`small-segment-lazy-commit`) — see that
  function's doc comment in `src/alloc_core/os.rs` for why the split is
  kept independent of production reservation policy.
- `AllocCore::dbg_decomp_win_release_only(reservation_ptr, reservation_len)`
  (`unsafe fn`) — thin wrapper over `os::release_segment`.

Both `os::Segment::reserve_lazy_for_measurement` and
`os::commit_pages_for_measurement` reuse the SAME
`aligned_vmem::reserve_aligned_lazy`/`commit_range` primitive the opt-in
lazy-commit POLICY features already call in production — this task adds no
new OS-interface code, only a measurement-only entry point to an existing
one, matching R29-3's "no new mechanism" discipline.

**Path-activation oracle (CLAUDE.md R30-8 rule):** the binary reads
`dbg_windows_reserve_commit_calls`/`dbg_unix_exact_reserve_attempts`/`_hits`
before and after the reserve-only timed loop and hard-asserts:

- On Windows: `windows_reserve_commit_calls` delta == N exactly (one
  `win_reserve_commit` call per iteration), AND
  `unix_exact_reserve_attempts` delta == 0.
- On non-Windows: `windows_reserve_commit_calls` delta == 0.

**This oracle caught a real bug during this task's own development**, before
any wrong number was published — a first version of the harness snapshotted
the "before" counters BEFORE the warmup loop instead of after it, so the
oracle correctly failed with `left: 220, right: 200` (the extra 20 being
`WARMUP`'s own reserve calls silently folded into the timed delta). Fixed by
moving the snapshot to immediately before the timed loop; all 3 final runs
show the oracle PASSING with the exact expected delta. See the example
file's own comment at the fix site for the concrete before/after numbers.

### 3.2 The reserve-vs-commit split (NEW finding)

3 runs, `N=200` iterations each, `WARMUP=20` (discarded):

| run | reserve-only (ns) | commit-only (ns, remaining 1005 pages) | ns/page (commit) | combined (ns) | oracle |
|---|---:|---:|---:|---:|---|
| 1 | 4,904.0 | 10,806.5 | 10.8 | 15,710.5 | PASS |
| 2 | 4,247.0 | 8,780.0 | 8.7 | 13,027.0 | PASS |
| 3 | 4,580.5 | 9,133.0 | 9.1 | 13,713.5 | PASS |
| **median** | **4,580.5** | **9,133.0** | **9.1** | 13,713.5 | 3/3 PASS |

**Commit costs consistently MORE than reserve** — roughly 2x, in every one
of the 3 runs (asserted in-script by `scripts/r32_13_windows_decomposition_summary.mjs`,
not just observed in prose: the derive script throws if `commit_only_ns <=
reserve_only_ns` in any run). This is counter-intuitive relative to a naive
mental model where `MEM_RESERVE` (which must find/carve a VA range, an
over-reserve-of-`size+align` + alignment search) sounds like the more
expensive operation; the data says `MEM_COMMIT` (which must charge
commit-limit accounting across ~1,000 pages, even though nothing is
physically faulted yet at this point) is the larger cost here. Neither R29-3
nor the original F11 survey entry anticipated this — it was not previously
measured anywhere in this corpus.

Note this split measures the WHOLE remaining range (`[PAGE, SEGMENT)` =
1,005 pages) committed in ONE `VirtualAlloc(MEM_COMMIT)` call — not a
per-page cost; the "ns/page" column is a derived rate for scale, not a
claim that commit is charged per-page internally.

### 3.3 R29-3-comparable component breakdown

Same A/A'/B/C arm shapes as R29-3 (§2 of that report), run through the
SAME `dbg_decomp_full_cycle`/`dbg_decomp_os_roundtrip`/
`dbg_decomp_reserve_and_keep`/`dbg_decomp_release`/
`dbg_decomp_decommit_payload`/`dbg_decomp_recommit_payload` hooks R29-3
itself uses (cross-platform already, since they go through `os::`/
`aligned_vmem`) — this is the direct Windows/Linux comparison point:

| component | run 1 (ns) | run 2 (ns) | run 3 (ns) | median (ns) |
|---|---:|---:|---:|---:|
| (1) OS reserve+release round-trip (lumped) | 142,616.0 | 106,051.0 | 149,007.5 | 142,616.0 |
| (1+2+3) AVOIDABLE subtotal (A) | 131,935.5 | 124,365.0 | 117,871.0 | 124,365.0 |
| (4+5) IRREDUCIBLE subtotal (B) | 2,628,528.5 | 2,580,924.0 | 2,605,305.5 | 2,605,305.5 |
| A' (real production cycle: reserve+touch+release) | 2,834,328.0 | 2,765,485.5 | 2,679,369.0 | 2,765,485.5 |
| **avoidable %** (A / (A+B)) | 4.78% | 4.60% | 4.33% | **4.60%** |
| **irreducible %** (B / (A+B)) | 95.22% | 95.40% | 95.67% | **95.40%** |

Full per-run stdout is in `docs/perf/_raw_r32_13_run{1,2,3}.log`; the
machine-readable summary (derived by `scripts/r32_13_windows_decomposition_summary.mjs`
from the raw logs' own `# csv-start`/`# csv-end` blocks — not hand-typed) is
`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE_summary.csv`.

## 4. Windows vs Linux comparison — a real, explicable difference

| | Linux (R29-3) | Windows (this task) |
|---|---:|---:|
| avoidable (1+2+3) share | 1.0-1.3% | 4.3-4.8% |
| irreducible (4+5) share | 98.7-99.0% | 95.2-95.7% |
| materiality verdict (20% threshold) | NOT material | NOT material |

Both platforms land on the SAME verdict (reservation path is small, page
faults dominate), but Windows's avoidable share is consistently ~3-4x
Linux's. This is mechanistically explicable, not surprising once the
component costs are compared: Windows pays 2 real syscalls per
reserve+commit (`VirtualAlloc` MEM_RESERVE then MEM_COMMIT, §3.2) plus a
2x VA over-reservation (F11's original finding) that Linux's single eager
`mmap` avoids entirely; Windows's decommit is also a real unmap
(`MEM_DECOMMIT`), unlike Linux's `MADV_DONTNEED` hint. Both platforms'
avoidable share is still small in absolute terms — well under the 20%
threshold either way — so this difference does not change the verdict, but
it is worth recording as the first quantified cross-platform comparison in
this corpus.

**Not claiming this explains OPEN_ITEMS item 24.** Item 24's unexplained
~14-29% wall-clock churn slowdown (R5-R2b) was measured on a
SMALL-OBJECT-CHURN workload (`global_alloc_churn`), which reserves very
few segments — this task's decomposition measures a very different
regime (repeated fresh 4 MiB segment reserve/commit/decommit/release
cycles). The survey itself explicitly warned against drawing this
connection without evidence, and this task's data does not supply that
evidence either way. See §7 (OPEN_ITEMS updates) for the precise wording
recorded against item 24.

## 5. Verdict

**RESERVATION PATH IS SMALL on Windows: 4.60% median avoidable share**,
well under the 20% materiality threshold this project's decomposition
gates use. Step 3 (`VirtualAlloc2` prototype, collapsing the unconditional
2-syscall reserve+commit + 2x VA over-reservation into one aligned-reservation
call) is **NOT justified by this evidence** — the mechanism it would fix
(§3.2's ~13.7 μs combined reserve+commit cost) is dwarfed by the ~2.6 ms
page-fault cost (§3.3's component B) that dominates the real production
cycle regardless of how the reservation itself is issued. Per the survey's
own explicit scoping ("do NOT attempt step 3 unless step 2's measured
numbers genuinely justify it") and this backlog's established
measurement-first posture (tasks #497, #499, #503 all correctly stopped
short of a code change when the evidence didn't support one), this task
stops after step 2.

**This is a `bench` commit, not `perf(runtime)` or `perf(opt-in)`** — no
production reservation policy or default constant changed; every new hook
is `bench-internals`-gated with zero production callers, and the new
`os::commit_pages_for_measurement`/`Segment::reserve_lazy_for_measurement`
functions are measurement-only siblings of already-existing production
functions (`commit_pages`/`reserve_lazy`), reachable only from
`bench-internals` builds.

---

## 6. Files touched

| file | change |
|---|---|
| `crates/vmem/Cargo.toml` | +1 `bench-internals` feature (crate-local, no dependencies) |
| `crates/vmem/src/lib.rs` | +3 `bench-internals`-gated `AtomicU64` counters + accessors + reset hook; 2 increment sites (`try_reserve_aligned_exact`, `win_reserve_commit`) |
| `src/alloc_core/os.rs` | +1 `bench-internals`-gated `Segment::reserve_lazy_for_measurement` + 1 `bench-internals`-gated `commit_pages_for_measurement` |
| `src/alloc_core/alloc_core_core_diag.rs` | +6 `dbg_*` accessors delegating to the new `aligned_vmem` counters |
| `src/alloc_core/alloc_core_small_pool.rs` | +3 `bench-internals`-gated decomposition hooks (`dbg_decomp_win_reserve_only`/`_commit_only`/`_release_only`) |
| `src/registry/heap_core_diag.rs` | +4 `HeapCore` delegation wrappers for the step-1 counters + +3 for the step-2 split hooks |
| `Cargo.toml` | +1 `bench-internals` forward to `aligned-vmem?/bench-internals`+`aligned-vmem?/lazy-commit`; +1 `[[example]]` entry |
| `examples/r32_13_windows_reserve_commit_decomposition_gate.rs` | NEW — wall-clock decomposition binary |
| `scripts/r32_13_windows_decomposition_summary.mjs` | NEW — checked derive script (raw logs → summary CSV, headline arithmetic asserted) |
| `tests/dbg_hook_safety_tripwire.rs` | +4 entries in `UNSAFE_HOOKS` (the 4 new `unsafe fn dbg_*` hooks) |
| `README.md` | Corrected tier-2 `unsafe` inventory: `alloc_core_small_pool.rs` 4→6, `heap_core_diag.rs` 7→10 (2 of that delta pre-existing drift from R31-6, corrected honestly alongside this task's own +2/+2), aggregate 69→73 |
| `docs/perf/_raw_r32_13_run1.log` / `_run2.log` / `_run3.log` | NEW — raw wall-clock runs |
| `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE_summary.csv` | NEW — machine-readable summary (derived, asserted) |

## 7. `docs/perf/OPEN_ITEMS.md` updates

- **F11 (this task, task #504):** resolved. Step 1 (Unix/Windows counters)
  shipped. Step 2 (Windows decomposition) shipped and measured: 4.60%
  median avoidable share, NOT material. Step 3 (`VirtualAlloc2`) explicitly
  declined — evidence does not justify it.
- **Item 16 (R29-3 Linux decomposition):** its own "Next trigger" (b),
  quoted at the top of this report, is now fired and answered: the Windows
  OS-backend's avoidable share (4.3-4.8%) is larger than Linux's (1.0-1.3%)
  but still small in absolute terms and does NOT flip the verdict. No
  design is opened as a result.
- **Item 24 (R5-R2b unexplained Windows wall-clock signal):** this task
  supplies the first real Windows-native OS-interface measurement since
  that item was filed, but on a DIFFERENT workload regime (fresh-segment
  reserve/commit/decommit/release cycles, not small-object churn) — per
  the survey's own explicit caution, this task does NOT claim to explain
  item 24's signal. Recorded as a cross-reference only.

---

**2026-08-03 (task #504) — filed. `landing_commit` placeholder: this
report's raw logs and summary CSV cite `f126de1a77f01c6a33f605d0985a29cf71862ab5`
(the base SHA / immutable tree identity) as the measurement basis; the
actual landing commit SHA will be filled in a follow-up commit per this
project's established placeholder convention (see R31-10's note on using
the FULL 40-character SHA, never a 7-char short one).**
