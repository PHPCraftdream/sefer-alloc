# Task #1995 — bind-less cold arm census (measurement-only)

**Verdict: NULL / NO-GO on building the negative-cache flag now.** The
mechanism the original review predicted from reading the code is REAL and
now measured (not merely inferred), but its own trigger condition — a
process with more than `MAX_HEAPS` (4096) *simultaneously live,
never-recycled* threads for case (a) — is itself extreme enough that the
measured per-call cost, tens of nanoseconds, does not justify the added
complexity of a negative-caching TLS flag under this crate's current
`docs/perf/OPEN_ITEMS.md` priority queue. Revisit only if a real deployment
is shown to spend meaningful wall-clock time actually inside case (a) or (b).

## 0. Scope and what this report is / is not

This is a measurement-only census per the task's own explicit framing
("Priority: low; measure first ... let it decide whether the flag is
worth it") — no allocator code changed. Per CLAUDE.md's raw-log/summary-CSV
rule, the verdict rests on measured numbers (elapsed time, an existing
counter's delta), so it owes — and includes — raw logs and a summary CSV
despite being a small, one-off probe rather than a permanent benchmark
harness.

**Not a rigorous statistical benchmark.** The wall-clock numbers below come
from `std::time::Instant` around a tight loop in a `--release` binary, not
criterion — acceptable for this low-priority go/no-go census (the run-to-run
noise is reported honestly, see §2), but not a claim of measurement rigor
beyond "same order of magnitude, repeatably".

## 1. Entry point and mechanism under test

**Entry point:** `SeferAlloc`'s real `#[global_allocator]` path (`Vec<u8>`
allocations through a `#[global_allocator] static GLOBAL: SeferAlloc`), not
a bare `AllocCore`/`HeapCore` call — the cold arm under study lives entirely
in `global::tls_heap`'s `current_for_alloc`/`bind_slow_tagged` resolution,
upstream of both, so this is the correct layer per this file's own
entry-point rule.

**Case (a) — registry exhaustion.** `HeapRegistry::claim()` is a raw
registry operation independent of the calling thread's own TLS binding (it
never touches `LOCAL`), so ONE thread can claim all `MAX_HEAPS` slots
itself, never recycling any, and thereby exhaust the registry without
spawning `MAX_HEAPS + 1` = 4097 real OS threads (which a real reproduction
of this scenario would otherwise require). This made the census cheap and
fast (~0.06–0.10 s to fill 4095 slots — see raw log) instead of requiring a
heavy/exhaustive many-thread harness.

**Case (b) — allocation during this thread's own post-`AbandonGuard`
TLS-teardown window** is NOT measured here: reliably forcing genuine
thread-destructor ordering (this thread's `GUARD` TLS slot already torn
down, then a LATER-registered thread-local's `Drop` allocates) is
platform/implementation-specific and not portably reproducible from a
probe. Structural argument instead (§4).

**Path-activation oracle:** [`sefer_alloc::global::dbg_fallback_lock_acquisitions`]
— an existing counter (`src/global/fallback.rs`), no new instrumentation
added by this probe — counts every successful fallback-spinlock
acquisition process-wide. Its delta over N calls proves, rather than
assumes, that each exhausted-arm call independently takes the fallback
lock (i.e., redoes the resolution) instead of hitting some already-cached
"known-hopeless" state (none exists today, which is exactly the reviewed
finding).

**Probe:** `examples/task1995_bind_less_cold_arm_census.rs`, committed
alongside this report (not a throwaway — see CLAUDE.md's raw-log-artifact
rule; a small permanent example is the established pattern here, mirroring
`examples/r21_2_opt_h_stage1_probe.rs`).

## 2. Measured results

Three consecutive runs (`docs/perf/_raw_task1995_bind_less_cold_arm_census.log`,
full raw output; `docs/perf/TASK1995_BIND_LESS_COLD_ARM_CENSUS_summary.csv`,
machine-readable):

| run | warm ns/round | exhausted ns/round | fallback_lock_delta (exhausted, N=2000) | ratio |
|-----|---------------:|--------------------:|:----------------------------------------:|------:|
| 1   | 9.0            | 55.4                 | 2000                                     | 6.16  |
| 2   | 14.2           | 85.4                 | 2000                                     | 6.04  |
| 3   | 13.6           | 77.5                 | 2000                                     | 5.72  |

**Mechanism confirmed, not assumed:** `fallback_lock_delta == 2000` (== N)
in all three runs — every one of the 2000 `alloc` calls from the
registry-exhausted thread independently re-resolved via `bind_slow_tagged`
and took the fallback lock; had a negative cache existed, only the FIRST
call would show a delta, with `1999` subsequent calls contributing `0`. The
probe's own oracle assertion (`exhausted_lock_delta > 0`, plus
`warm_lock_delta == 0` for the baseline) is hard-checked in the binary
itself, not just eyeballed from the printed numbers.

**Why the delta is exactly N, not 2N** (each round does one `alloc` +
one `dealloc`): under `alloc-xthread` (in `production`), `dealloc` resolves
via the separate `current_for_dealloc()` path, whose bind-less arm
(`CurrentHeapForDealloc::ForeignNoBind`) is explicitly documented and
designed to route WITHOUT ever taking the fallback lock (see
`global/tls_heap.rs`'s `current_for_dealloc` doc — R6-OPT-P0-1). Only
`alloc` (via `current_for_alloc`) touches the fallback lock on this arm.
This is itself independent confirmation that the existing `dealloc`-side
fast-exit design (a DIFFERENT, already-shipped optimization) works as
documented.

**Absolute cost:** the exhausted arm costs roughly 40–70 ns/call MORE than
the warm baseline (tens of nanoseconds, not microseconds) — a failed
`pick_slot`/CAS attempt plus (under `alloc-decommit`, active here via
`production`) a `LargeCacheConfig` copy, exactly the mechanism the original
review named, now bounded to a concrete order of magnitude.

**Immutable source identity:** the probe, its `Cargo.toml` registration,
this report, and the raw log/summary CSV all land in the SAME commit — the
SHA that introduces `docs/perf/TASK1995_BIND_LESS_COLD_ARM_CENSUS.md`
(`git log -1 --format=%H -- docs/perf/TASK1995_BIND_LESS_COLD_ARM_CENSUS.md`)
is therefore the exact source this report measures, satisfying the R29-6
immutable-identity rule without a separate pre-measurement commit.

**Environment:** 11th Gen Intel Core i7-11800H @ 2.30GHz, Windows
(MINGW64/MSYS2 `uname`: `MINGW64_NT-10.0-19045`), `rustc 1.97.0
(2d8144b78 2026-07-07)`, `--release`, `--features "production internals"`.

## 3. Reproduction

```
cargo build --release --example task1995_bind_less_cold_arm_census --features "production internals"
./target/release/examples/task1995_bind_less_cold_arm_census.exe
```

## 4. Case (b) — structural argument (not measured)

Case (b) requires: this thread's OWN `AbandonGuard` (`GUARD`) TLS slot has
ALREADY run its destructor (recycling this thread's registry slot and
stamping `LOCAL` to `TORN`), AND a thread-local declared BEFORE `GUARD`
(hence destroyed AFTER it, per Rust's reverse-declaration-order guarantee —
see `global::tls_heap`'s own module doc) has a `Drop` impl that allocates.
This requires:

1. The allocating type to be held in a thread-local declared textually
   before any thread-local this crate's own `AbandonGuard` sits after in
   declaration order — an ordering property of the CONSUMER'S code, not
   this crate's.
2. That later-destroyed thread-local's `Drop` to actually call into the
   global allocator (`Vec`/`Box`/`String`/etc. drop, or any transitive
   allocation) — most `Drop` impls that fire during thread teardown are
   simple field drops (numeric fields, already-empty collections) that
   don't reach the allocator at all.

Both conditions together describe a real but narrow slice of possible
Rust programs — plausible (some `thread_local!`-heavy frameworks with late
destructors that build/log a final report on thread exit could hit it), but
structurally rarer than case (a) already is, and with a materially smaller
number of `alloc` calls in play per occurrence (a handful of drops during
one thread's teardown, not steady-state churn). No number is claimed here
per CLAUDE.md's no-invented-numbers rule; this is the reasoning, not a
substitute for future measurement should case (b) ever look worth chasing
on its own.

## 5. Decision

The already-known mechanism (each call redoes the full claim attempt) is
now measured, not merely code-read: `fallback_lock_delta == N` exactly, and
the absolute added cost is tens of nanoseconds per call. Given:

- Case (a)'s trigger (`> MAX_HEAPS = 4096` simultaneously-live threads)
  is itself a pathological deployment shape most real services never
  approach.
- The measured per-call cost is small in absolute terms even in the
  worst case.
- Case (b) is structurally narrower still, and not independently measured.

Building the one-bit "bind is known-hopeless" negative-cache flag is **NOT
justified now**. Filed as NULL, not dropped silently — if a future
workload census (or a production incident) shows a real system actually
spends measurable time in either cold arm, this report's probe and
methodology (direct registry-fill via `HeapRegistry::claim()`, the
existing `dbg_fallback_lock_acquisitions` oracle) are ready to be reused
or extended without re-deriving them from scratch.
