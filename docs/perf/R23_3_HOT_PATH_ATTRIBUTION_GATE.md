# R23-3 — orthogonal hot-path attribution: which component of the 16 B alloc/free path costs what

**Task #372 (R23-3), Round 23.** Executes
`docs/reviews/2026-07-26-r22-readonly-review.md` §4.1/§6's own P0
recommendation ("R23-3: split hot alloc / hot free / cold alloc / cold
free") — a follow-up to R22-17/R23-1 (which isolated `contains_base`, 8.8%,
and `segment_base_of_ptr`, 9.8%, as point components of a real free's `Ir`)
and R22-15/R23-2 (which corrected the SeferAlloc-vs-mimalloc `Ir` ratio and
found hot churn is break-even-to-favorable while cold-carve is ~2x costlier
for SeferAlloc). This task's job: build a fuller, orthogonal decomposition of
the WHOLE hot alloc/free path, not just the two point-isolations that already
existed, so a future remediation task has a ranked list of "which single
mechanism is the best next target" instead of a diffuse "cold-carve/recycle
is ~2x costlier" headline with no internal breakdown.

**Date:** 2026-07-27. **Base revision measured:** `main` @ `3cf2d669fd4102536a0c6851c0f6eef64de1780d`
(working tree otherwise carrying only this task's own additive edits at
measurement time; the usual untracked `docs/checkpoints/`/`docs/reviews/`
files from concurrent review sessions present, not touched by this task).
**Platform measured:** WSL2 (Ubuntu, kernel `6.18.33.2-microsoft-standard-WSL2`)
under Windows 10 Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`,
WSL rustc `1.98.0-nightly (bd08c9e71 2026-06-25)` — same toolchain/host as
every other `npm run iai` measurement in this doc tree.

---

## 0. Headline: the free path is dominated by ONE fused mechanism (80.8%), not by routing

| free-path component | isolated Ir/op | share of the real free loop (92.5 Ir/op) |
|---|---:|---:|
| **own-thread body: M2 oracle checks + magazine push (fused)** | **74.70** | **80.8%** |
| `segment_base_of_ptr` | 9.03 | 9.8% |
| `contains_base` (Tier-1 cache hit) | 8.17 | 8.8% |
| residual / subtraction rounding | 0.59 | 0.6% |
| **total (real_free_loop)** | **92.50** | **100.0%** |

`contains_base`+`segment_base_of_ptr` (the ROUTING prefix R22-17/R23-1
isolated) together account for only **18.6%** of a real free — the OWN-THREAD
BODY that runs once ownership is confirmed (the M2 double-free oracles plus
the magazine push, `dealloc_own_thread_with_base`) is **more than 4x
larger**, at **80.8%**. This is the report's single most important finding:
prior rounds' attention on `contains_base`/routing was examining a genuinely
material but numerically MINOR piece of the free path; the dominant cost was
un-isolated until this task.

On the alloc side, the magazine-HIT pop itself (22.4 Ir/op) is a real but
smaller fraction (32.4%) of `small_churn_16b`'s combined alloc+free marginal
cost (69.0 Ir/op, R23-2) — leaving roughly two-thirds of a churn op's cost on
the free half, consistent with the free-path table above.

See §5 for the full ranked table and §6 for the honest list of what could
NOT be cleanly isolated.

---

## 1. Investigation performed first, per the task's own instruction

### 1.1 Hot alloc — magazine hit (`src/registry/heap_core_alloc.rs:132-255`)

Read `HeapCore::alloc`'s fastbin block in full. The magazine-HIT arm
(`cnt > 0`) is a small, self-contained unit: decrement `count`, read
`slots[new_cnt]`, then (under plain `production`, no `hardened`) ONE
`clear_magazine` bitmap write via `SegmentMeta::new(base).magazine_bitmap()`.
The `hardened`-only generation bump and the `alloc-stats`-only hit counter
are both compiled OUT under plain `production` — confirmed by reading their
`#[cfg]` gates. This is the minimal isolable unit for "magazine hit" under
the measured feature set.

### 1.2 Hot free — magazine push (`src/registry/heap_core_free.rs:296-830`,
`dealloc_own_thread_with_base`)

Read the WHOLE function body (not just the push, per the task's explicit
instruction to check whether the oracle-check-and-mark sequence is separable
from the push). **Finding: they are NOT separable without changing what a
real free actually does.** The M2 double-free oracles (in-magazine bitmap
probe, then flushed/alloc-bitmap probe) and the magazine push itself
(`slots[cnt] = ptr; count += 1; mark_magazine(off)`) share the SAME `base`/
`off`/`meta` locals computed once at the top of the function, in one
straight-line block with no branch boundary between "check" and "push" for
the common (non-double-free, non-overflow) case — see lines 681-743 of that
file. Splitting them further would need a hook that skips the oracle checks
while still performing the push (or vice versa) — i.e. a hook that does
something the production path never does, the exact Heisenberg risk (adding
a measurement mechanism that changes the very thing being measured) the task
brief warned against. **This report isolates them TOGETHER, as the smallest
honestly separable unit past the routing prefix — see §6.1 for the full
"not cleanly isolable" writeup.**

### 1.3 Free routing — Tier-1 vs Tier-2 (`src/alloc_core/segment_table.rs:443-489`)

Investigated whether a benched WORKLOAD (touching more than `OWN_CACHE_SIZE`
(4) distinct segments) can portably force a Tier-2 (8192-slot open-addressing
hash probe) hit, as the task asked. **Finding: no, not portably.**
`cache_index(base) = (base >> SEGMENT_SHIFT) & 3` depends ONLY on the
segment's OS-assigned virtual address (`mmap`/`VirtualAlloc` choose it, not
this allocator, confirmed by reading `os::Segment::reserve` →
`vmem::reserve_aligned`, which is a plain OS reservation call with no
address-selection logic of this crate's own). A workload allocating 5+
distinct segments does not guarantee their cache indices spread across more
than 4 buckets — the OS could lay them out so they all collide into ≤4
buckets, or so they never evict each other, on any given run. **Rather than
build a workload and hope, this task added a hook
(`SegmentTable::dbg_hash_contains_only`) that calls the SAME production
`hash_contains` routine DIRECTLY, unconditionally skipping the Tier-1 cache
check** — deterministically isolating Tier-2's cost regardless of address
layout, without inventing a new probe mechanism (it is the exact function
`contains_base` already falls through to on every real Tier-1 miss). See
§6.2 for the caveat this measures Tier-2's cost AS AN ALTERNATIVE to Tier-1,
not as a component that fires within THIS gate's own single-hot-segment
workload.

### 1.4 Cold carve vs recycle (`src/alloc_core/alloc_core_small.rs:1429`,
`carve_batch`/`carve_block`)

`AllocCore::carve_batch` (the batched sibling `carve_block_with_refill`'s
refill loop calls one-block-at-a-time via `carve_block`) is ALREADY exposed
test-only via the pre-existing `dbg_carve_batch` hook (task W4) — no new
hook needed. Driving it directly against a bare `AllocCore::new()` isolates
the pure bump-cursor-advance + (lazy-commit builds only) commit-frontier-grow
cost, WITHOUT `carve_block_with_refill`'s per-extra-block `dealloc_small`
push into the BinTable (`cold_alloc_free_256x16b` already exercises that
fuller path). This is a genuinely narrower mechanism than "cold carve through
the full production alloc path" — see §6.3 for why the two numbers are not
directly comparable as a percentage.

---

## 2. Two self-caught methodology bugs — disclosed, not silently fixed

Per this project's zero-trust convention, both bugs below were caught by
treating a SURPRISING derived number as a red flag and investigating before
publishing, not by outside review. Both are visible in the bench file's own
comments (`benches/perf_gate_iai.rs`) at the arms they affected.

### 2.1 `dealloc_own_thread_body_only_16b` — missing `#[inline(always)]`

First measurement: 15,428 raw Ir → loop-only (after subtracting the shared
prefix and the base-only cost) **122.6 Ir/op** — LARGER than the entire real
free loop (92.5 Ir/op). Impossible for a strict sub-component. Root cause:
the new `dbg_dealloc_own_thread_with_base` hook
(`src/registry/heap_core_diag.rs`) lacked `#[inline(always)]`, unlike the
real `dealloc_routing` (which IS `#[inline(always)]` and inlines straight
into `dealloc_own_thread_with_base`, itself also `#[inline(always)]`), so
the large `dealloc_own_thread_with_base` body was not being inlined into the
bench's timed loop — costing extra register-spill/call overhead disproportionate
to the function's own size. Adding `#[inline(always)]` to the new hook
dropped the raw Ir to 12,362 → a coherent **74.70 Ir/op**, consistent with
(and smaller than) the real free loop. Fixed in the committed hook.

### 2.2 The alloc-magazine-hit and recycle-pop arms — invalid N/2N pairs

Both `alloc_magazine_hit_only_16b_2n` (first draft) and
`recycle_alloc_free_256x16b_2n` were built as ordinary N/2N siblings
(doubling a loop-count constant), following R23-2's technique blindly. Both
produced nonsensical numbers — magazine-hit at 136.6 Ir/op (nearly double
`small_churn_16b`'s ENTIRE alloc+free marginal cost, for a pop that should
be a small fraction of it); recycle-pop at 399.4 Ir/op (roughly DOUBLE
virgin-carve's own marginal cost, for a mechanism — freelist pop — that
should be cheaper or comparable, never a strict multiple more expensive than
carving). Root cause in both cases: the thing being doubled from N to 2N was
NOT the isolated component alone — it was a whole CYCLE/ROUND that bundled
the isolated component together with an equal-sized setup cost (carve+free
in the magazine case; a full virgin-carve round in the recycle case), so
`c = (Ir(2N)-Ir(N))/N` measured the marginal cost of one COMBINED unit, not
the isolated piece. **R23-2's N/2N technique is valid only when doubling the
loop count genuinely doubles ONLY the isolated quantity** — it is NOT a
universal substitute for shared-prefix subtraction when setup and signal are
tied 1:1 within a repeating unit. Both were replaced:

- **Magazine hit**: `alloc_magazine_prefill_only_16b` (fill+free only, no
  drain) + `alloc_magazine_hit_only_16b` (same prefill, PLUS one final
  16-block hit-drain) — shared-prefix subtraction isolates exactly 16 hits,
  giving a sane 22.4 Ir/op.
- **Recycle-pop**: no new arm needed at all. `cold_alloc_free_256x16b`
  (already in this file) IS byte-for-byte round 1 of
  `recycle_alloc_free_256x16b` (both are `SeferAlloc::new()` + 256 virgin
  alloc-then-free-all) — so `recycle_alloc_free_256x16b`'s raw Ir minus
  `cold_alloc_free_256x16b`'s raw Ir isolates round 2 (the freelist-pop
  round) alone, giving 188.2 Ir/op — comparable to (slightly below) virgin
  carve's 203.86 Ir/op N/2N marginal, not double it.

This is exactly the kind of "measured, not spun" honesty this project's
CLAUDE.md and prior gate reports (R22-17→R23-1, R22-15→R23-2) already
established as the norm — a wrong first number corrected in the same task,
not carried forward.

---

## 3. New measurement hooks (`#[doc(hidden)]`, following the exact existing pattern)

1. **`SegmentTable::dbg_hash_contains_only`** (`src/alloc_core/segment_table.rs`)
   — calls `hash_contains` (Tier-2) directly, no Tier-1 cache check, no cache
   fill. Unconditional (no feature gate), mirroring `contains_base`/
   `contains_base_ro` immediately above it in the same file.
2. **`AllocCore::dbg_hash_contains_only`** (`src/alloc_core/alloc_core_core_diag.rs`)
   — thin delegation, takes a pre-computed base (mirrors
   `dbg_segment_base_of_ptr`'s existing convention of NOT computing the base
   itself).
3. **`HeapCore::dbg_hash_contains_only`** (`src/registry/heap_core_diag.rs`)
   — thin delegation, gated `#[cfg(all(feature = "alloc-global", feature =
   "alloc-xthread"))]`, same gate as the sibling `dbg_contains_base`.
4. **`HeapCore::dbg_dealloc_own_thread_with_base`** (`src/registry/heap_core_diag.rs`)
   — `unsafe fn` thin delegation to the real (already-`#[inline(always)]`)
   `dealloc_own_thread_with_base`, carrying the IDENTICAL `# Safety` contract
   as `HeapCore::dealloc` itself (no new unsafe reasoning invented — see the
   hook's own doc comment for why). Gated `#[cfg(all(feature = "alloc-global",
   feature = "fastbin"))]`, matching the delegated function's own gate.
   `#[inline(always)]` added after the §2.1 bug was found.

No existing production call site was touched by any of the four hooks —
each is read-only measurement tooling exposing an EXISTING private routine,
following the established `dbg_contains_base`/`dbg_segment_base_of_ptr`
pattern this file already uses (R22-17/R23-1).

---

## 4. New bench arms (`benches/perf_gate_iai.rs`)

| arm | isolates | technique |
|---|---|---|
| `alloc_magazine_prefill_only_16b` | shared prefix for the hit arm below | shared-prefix |
| `alloc_magazine_hit_only_16b` | magazine-hit pop (alloc side) | shared-prefix vs the arm above |
| `dealloc_hash_contains_only_probe_16b` | Tier-2 hash-probe, standalone | shared-prefix vs `dealloc_prealloc_only_16b`/`dealloc_segment_base_of_ptr_probe_only_16b` |
| `dealloc_own_thread_body_only_16b` | own-thread free body (oracle+push, fused) | shared-prefix vs the same two arms |
| `carve_batch_only_16b` / `_2n` | pure bump-carve, standalone `AllocCore` | N/2N (valid here: no setup/signal coupling) |
| (no new arm) recycle-pop | freelist-pop, round 2 alone | shared-prefix vs the EXISTING `cold_alloc_free_256x16b` row |

All arms follow this file's existing conventions: `#[cfg(target_os =
"linux")]` (plus `alloc-xthread`/`fastbin` where the delegated mechanism
requires it), `black_box` on every observable result, doc comments
explaining what each isolates and how, registered in the `perf_gate`
`library_benchmark_group!` list. Two arms from the abandoned first drafts
(`alloc_magazine_hit_only_16b_2n`, `recycle_alloc_free_256x16b_2n`) were
REMOVED after §2.2's finding — not left in as dead/misleading rows.

---

## 5. Results — real, deterministic `npm run iai` numbers (two independent runs, byte-identical `Ir`)

Raw evidence (both truncated per this project's truncation-marker
convention — cargo's dependency-compile noise cut, the bench output block
verbatim; full untruncated logs are reproducible via `npm run iai` at the
cited commit):

- `docs/perf/_raw_r23_3_hot_path_attribution_run1.log`
- `docs/perf/_raw_r23_3_hot_path_attribution_run2.log`

Both runs: **36 benches, byte-identical `Ir` for every row including all new
arms** (confirmed via a diff of the extracted `Ir` column across both runs —
zero differences). This is the same determinism property every prior gate
report in this tree (R22-15, R22-17, R23-1, R23-2) has established.

### 5.1 Raw Ir table (new arms + the pre-existing rows they are derived against)

| bench | raw Ir | ops |
|---|---:|---:|
| `dealloc_prealloc_only_16b` (shared prefix) | 7,003 | 64 |
| `dealloc_free_only_16b` (real free loop) | 12,923 | 64 |
| `dealloc_contains_base_probe_only_16b` (R23-1) | 8,104 | 64 |
| `dealloc_segment_base_of_ptr_probe_only_16b` (R23-1) | 7,581 | 64 |
| `dealloc_hash_contains_only_probe_16b` (R23-3, new) | 8,349 | 64 |
| `dealloc_own_thread_body_only_16b` (R23-3, new) | 12,362 | 64 |
| `alloc_magazine_prefill_only_16b` (R23-3, new) | 7,450 | — (setup only) |
| `alloc_magazine_hit_only_16b` (R23-3, new) | 7,808 | 16 (final drain) |
| `carve_batch_only_16b` (R23-3, new) | 68,284 | 256 |
| `carve_batch_only_16b_2n` (R23-3, new) | 74,185 | 512 |
| `cold_alloc_free_256x16b` (existing, R22-15/R23-2) | 50,164 | 256 |
| `recycle_alloc_free_256x16b` (existing) | 98,343 | 512 (2 rounds x 256) |
| `small_churn_16b` (existing, context) | 8,051 | 64 |
| `large_alloc_free_cycle` (existing, bootstrap context only) | 3,308 | 1 |

### 5.2 Ranked table — largest-to-smallest isolated contributor

**Free path** (of the real free loop's 92.50 Ir/op, = 100%):

| rank | component | Ir/op | share |
|---|---|---:|---:|
| 1 | own-thread body: M2 oracles + magazine push (fused, not further isolable) | 74.70 | 80.8% |
| 2 | `segment_base_of_ptr` | 9.03 | 9.8% |
| 3 | `contains_base` (Tier-1 cache hit — this workload's real path) | 8.17 | 8.8% |
| — | residual / subtraction rounding | 0.59 | 0.6% |
| (alt.) | `hash_contains` (Tier-2, IF it fired instead of Tier-1 — not part of this workload's real path) | 12.00 | 13.0% (of this workload's total, hypothetically) |

**Alloc path** (context: `small_churn_16b`'s combined alloc+free marginal
cost is 69.0 Ir/op per R23-2's N/2N derivation):

| component | Ir/op | share of one alloc+free pair |
|---|---:|---:|
| magazine-hit pop (alloc side) | 22.38 | 32.4% |
| (implied remainder: free side + any refill amortization) | ~46.6 | ~67.6% |

**Cold path** (standalone mechanisms, NOT directly comparable to the full
per-op figures above — see §6.3):

| component | Ir/op (standalone) |
|---|---:|
| pure bump-carve (`carve_batch`, no magazine/refill/BinTable-push) | 23.05 |
| freelist-pop (round 2 of `recycle_alloc_free_256x16b`, full production path) | 188.20 |
| virgin-carve through the FULL production path (R23-2 N/2N, for comparison) | 203.86 |

---

## 6. What could NOT be cleanly isolated, and why

### 6.1 Own-thread free body: M2 oracles vs magazine push

**Genuinely fused, not merely unmeasured.** Read in full (§1.2): the two
oracle checks and the push share the same `base`/`off`/`meta` locals in one
straight-line block with no branch boundary between them for the common
case. Any hook that ran the oracles without the push (or the push without
the oracles) would be measuring something the production path never does —
a different, invented mechanism, not an isolated piece of the real one. This
is reported as ONE 80.8%-share component, not artificially split.

### 6.2 Tier-1 vs Tier-2 `contains_base` — isolable in cost, NOT in workload-realism

`dbg_hash_contains_only` (§3) gives a real, deterministic Tier-2 cost (13.0%
of this workload's free-path total, IF it fired). But this gate's own
workload (a single hot segment, reused 64 times) NEVER exercises Tier-2 in
the real `dealloc_routing` path — every real `contains_base` call after the
first is a Tier-1 hit (R22-17's own finding, unchanged by this task). So
13.0% is "what Tier-2 would cost on this SAME 64-call shape, as a
counterfactual alternative to Tier-1", not "Tier-2's share of THIS report's
measured free-path total" (which is, and remains, Tier-1's 8.8%, since
Tier-2 never actually fires here). Whether a real multi-segment,
Tier-2-heavy workload would show Tier-2 firing MORE or LESS often than 13.0%
per call remains unmeasured — this task did not (and, per §1.3, could not
portably) construct a workload that reliably exercises Tier-2 through the
real routing path; it measured the mechanism's OWN cost directly instead.

### 6.3 Carve-standalone vs carve-through-full-path — different scopes, not a percentage

`carve_batch_only_16b`'s 23.05 Ir/op measures ONLY the bump-cursor-advance +
commit-frontier-grow logic, via a bare `AllocCore` with NO magazine, NO
`HeapRegistry`/`HeapCore`, NO per-extra-block `dealloc_small` BinTable push.
`cold_alloc_free_256x16b`'s 203.86 Ir/op (R23-2) measures a FULL alloc+free
pair through the production `SeferAlloc` face, including the magazine
miss/refill machinery, per-extra-block BinTable pushes, and the free half.
These are not the same denominator — reporting "carve is 11.3% of cold-carve
total" would be arithmetically valid but a category error (one number is a
sub-mechanism's OWN cost, the other is a full round-trip including a
different allocator surface's bookkeeping around it). This report cites both
numbers side by side (§5.2) without computing a misleading ratio between
them.

### 6.4 Recycle-pop's exact percentage of `recycle_alloc_free_256x16b`'s total

188.20 Ir/op is round 2's OWN marginal cost, isolated by subtracting round
1's cost (via `cold_alloc_free_256x16b`'s existing row) from the two-round
bench's total. This is directly comparable to virgin-carve's 203.86 Ir/op
(R23-2's N/2N figure, also a full alloc+free marginal cost) — both are
"cost of one alloc+free pair through the full production path", just
sourced from different-shaped benches (one round vs the second of two
rounds). The comparison itself (§5.2) IS valid; what is NOT available is a
direct N/2N-style linearity cross-check for the recycle-pop figure the way
R23-2 had a third `_4n` point for cold-carve — this task did not add a
third recycle round to cross-check linearity, scoped out for time.

---

## 7. Recommendation for the next remediation task

**Best single next target: the own-thread free body (M2 oracles + magazine
push), 80.8% of the free path.** This is the report's clearest, most
actionable finding — it dwarfs every other isolated component by 4x or more,
and unlike `contains_base` (already gated by real ownership-safety
constraints per R22-17 §4.2) or the routing prefix (a small, already-cheap
Tier-1 cache hit), this component has not had ANY design-level scrutiny in
prior rounds. Two concrete, narrower sub-questions a future measurement (not
remediation) task could still usefully split further, if warranted:

1. **Which of the two oracle checks (in-magazine bitmap probe vs
   flushed/alloc-bitmap probe) dominates the 74.70 Ir/op**, versus the raw
   magazine-slot write itself? This report did NOT attempt that finer split
   (it would need a NEW hook mid-function, not exposing an existing routine
   — a larger step than this task's "expose what already exists" scope) but
   flags it as the natural follow-up before any remediation attempt, so a
   remediation task knows whether it is optimizing an oracle or the push.
2. **Cold-carve (23.05 Ir/op standalone) vs recycle-pop (188.20 Ir/op)**:
   recycle-pop, run through the FULL production path, costs roughly 8x more
   than bare bump-carving, but is comparable to (marginally cheaper than)
   virgin-carve run through that SAME full path (203.86 Ir/op) — i.e. the
   "recycle is expensive" framing from a purely mechanism-level read
   (§5.2's cold-path table) is misleading once matched to the SAME
   full-path denominator (§6.3/§6.4); recycle is not a costlier path than
   virgin-carve on this crate's numbers, it is roughly on par. This
   REVISES, not confirms, R22-15/R23-2's suspicion that cold-carve/recycle
   is "the main other candidate" needing architectural attention — the free
   path's own-thread body is the larger, clearer, and previously-unexamined
   target.

**This report does not recommend implementing any remediation.** Per this
project's "measure first, remediate as a separate task" convention
(established by R22-15/R22-17/R23-1/R23-2), that is explicitly out of scope
here.

---

## 8. Verification performed

- **Read the mechanism FIRST for every component** (§1) before writing any
  bench code — `HeapCore::alloc`'s magazine-hit arm, `dealloc_own_thread_with_base`'s
  full body, `SegmentTable::contains_base`/`cache_index`/`hash_contains`,
  `AllocCore::carve_batch`/`carve_block`.
- **Investigated Tier-1/Tier-2 forceability BEFORE assuming a workload could
  do it** (§1.3) — traced `cache_index` to `os::Segment::reserve` →
  `vmem::reserve_aligned`, confirmed no address-selection logic of this
  crate's own exists, concluding a workload-only approach is not portable;
  built the direct-call hook instead.
- **Two self-caught methodology bugs found and fixed BEFORE publishing any
  number** (§2) — the missing `#[inline(always)]` (own-thread body) and the
  invalid N/2N pairs (magazine-hit, recycle-pop) — both disclosed in this
  report and in the bench file's own comments, not silently corrected.
- **Real measured numbers from my own `npm run iai` runs** (not estimated):
  two independent full-suite runs (36 benches each, `--features production`,
  the CI default) — byte-identical `Ir` for every bench including every new
  arm, confirmed via a diff of the extracted `Ir` column.
- **`cargo fmt --check`** (Windows-side, the four touched files) — clean
  after one `cargo fmt` pass (a pre-existing whitespace-alignment nit in a
  moved-then-edited comment block).
- **`cargo check --bench perf_gate_iai --features production`** (WSL2
  Linux target, the platform this bench actually compiles its real body
  under) — clean, twice (before and after the §2 fixes).
- **`production`'s feature composition confirmed unchanged**:
  `grep -n "^production = " Cargo.toml` still returns `["alloc-global",
  "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory",
  "primordial-lazy-commit", "class-aware-dirty"]`, byte-identical to
  pre-task. `git status --short` confirms `Cargo.toml` is not in this task's
  diff.
- **No production behavior changed**: every new item in `src/` is a
  `#[doc(hidden)]` thin delegation to an EXISTING private routine
  (`hash_contains`, `dealloc_own_thread_with_base`) — no production call
  site was touched, no existing function's body was edited (only
  `#[inline(always)]` was ADDED to the new hook itself, never to any
  pre-existing production function).
- **clippy was NOT run under WSL** (no network access to install the
  `clippy` rustup component in this sandbox at measurement time) — the
  Windows-side `cargo check --features production` (library) and
  `cargo fmt --check` both passed; a full `cargo clippy --all-targets
  --features production -- -D warnings` under WSL is deferred to the
  reviewing session's own `npm run check` pass, per this project's
  pre-push convention.

---

## Files touched

- `src/alloc_core/segment_table.rs` — added
  `SegmentTable::dbg_hash_contains_only` (measurement-only, unconditional,
  `#[doc(hidden)]`).
- `src/alloc_core/alloc_core_core_diag.rs` — added
  `AllocCore::dbg_hash_contains_only` (thin delegation).
- `src/registry/heap_core_diag.rs` — added `HeapCore::dbg_hash_contains_only`
  and `HeapCore::dbg_dealloc_own_thread_with_base` (thin delegations,
  `#[doc(hidden)]`, feature-gated to match their delegated targets).
- `benches/perf_gate_iai.rs` — added `alloc_magazine_prefill_only_16b`,
  `alloc_magazine_hit_only_16b`, `dealloc_hash_contains_only_probe_16b`,
  `dealloc_own_thread_body_only_16b`, `carve_batch_only_16b`,
  `carve_batch_only_16b_2n`; registered all six in the `perf_gate`
  `library_benchmark_group!` list (36 benches total, up from 30 before this
  task — two abandoned first-draft arms were added then removed in the same
  task, see §2.2). Zero changes to any pre-existing bench fn's body (other
  than the `#[cfg]`/comment context around the removed arms).
- `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` — this report.
- `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE_summary.csv` — companion
  machine-readable summary.
- `docs/perf/_raw_r23_3_hot_path_attribution_run1.log` /
  `_raw_r23_3_hot_path_attribution_run2.log` — full raw `npm run iai`
  stdout for the two independent, byte-identical-`Ir` runs cited in §5.
  `git add -f` needed (`.gitignore` excludes `docs/perf/_raw_*.log` by
  default, R13-10/task #280).
- `docs/perf/OPEN_ITEMS.md` — item 1 (the `contains_base`/free-path
  attribution item) gets a "DONE" follow-up note citing this task's
  findings.
- `Cargo.toml` — **untouched** (confirmed in §8).
- `.github/workflows/perf-gate.yml` — **untouched** (no new job/step
  needed; the existing `cargo bench --bench perf_gate_iai --features
  production` line already runs every fn in the group).

**Files needing `git add -f`** (gitignored by `.gitignore`,
`/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r23_3_hot_path_attribution_run1.log`
- `docs/perf/_raw_r23_3_hot_path_attribution_run2.log`
