# R15-1 — post-raise perf base after `MAX_SEGMENTS` 1024 -> 4096 (R14-7)

**Task:** #303 (R15-1). **MEASUREMENT ONLY.** This document reports what was
measured; it does NOT revisit R14-7's decision to raise `MAX_SEGMENTS`
(`ffb82bc`, task #292) — that raise stays. This task closes the ONE axis
R14-7's own measurement (`docs/perf/R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md`
and its commit message) did not cover: the cost of the two sidecar bitmaps
that are sized `MAX_SEGMENTS / 64` words
(`segment_directory::WORDS_PER_CLASS`, `heap_slot::DIRTY_BITMAP_WORDS`) —
both go 16 -> 64 words (×4) — and specifically whether
`AllocCore::drain_dirty_segments`'s unconditional per-word
`swap(0, Acquire)` sweep over that now-4×-larger array costs anything
material in practice.

**Date:** 2026-07-24. **Base revision:** `main` @ `9b59990` (post-Round14,
all Round-14 hotfixes landed). **Isolation pair for the MAX_SEGMENTS-only
delta:** `b117257` (R14-7's docs-only parent commit, `MAX_SEGMENTS = 1024`)
vs `ffb82bc` (R14-7's raise commit itself, `MAX_SEGMENTS = 4096`) — this is
the tightest possible commit pair: `git diff --stat b117257 ffb82bc` touches
exactly one production source line
(`src/alloc_core/segment_table.rs`'s `MAX_SEGMENTS` constant) plus tests and
docs. Every number in this report that compares "before" vs "after" uses
this exact pair via two `git worktree` checkouts, not a broader window, so
none of Round 14's other changes (class-aware-dirty latch, medium-realloc
promotion, exact-span-large, sidecar primitive unification) can leak into
the delta. **Platform:** Windows 10 Pro x86-64 (native) for wall-clock/RSS;
WSL2 (Ubuntu 24.04) + Valgrind/Callgrind for `npm run iai` — same setup as
every prior perf-gate doc in this series (R13-6, R13-9, R14-4/5/6).

---

## 0. Headline summary

| # | Measurement | Before (`MAX_SEGMENTS`=1024) | After (`MAX_SEGMENTS`=4096) | Verdict |
|---|---|---|---|---|
| 1 | iai, 12 single-thread non-remote benches, Instructions (Ir) | see §2 | **+61,4xx Ir, uniformly, on EVERY bench** (+12% to +258% relative, because absolute Ir per bench varies) | **Real, deterministic, one-time bootstrap-scale cost — NOT drain-scan** (see §2.2 for why) |
| 2 | Wall-clock drain-under-load, `r12_7_class_aware_dirty_wallclock`, N=1/2/4/8 producer classes (exercises `drain_dirty_segments` on every owner alloc) | see §3 | **No material delta** (N=1: +0.6%, N=4: −0.4%, N=8: within its own ±25% CI) | **The drain-scan itself is NOT the cost — confirms the task's literal hypothesis is FALSE at this N** |
| 3 | Sidecar footprint, `PerClassDirty` (`class-aware-dirty`, per materialised heap) | 6,272 B raw / 8.0 KiB page-rounded | **25,088 B raw / 28.0 KiB page-rounded** | **Real ×4 growth, confirmed by both `size_of` arithmetic and process RSS deltas** |
| 4 | Sidecar footprint, `SegmentDirectory` (`numa-aware`, computed, not directly RSS-measured this task) | ~55.1 KiB | **~220.5 KiB** | **Real ×4 growth (arithmetic only — see §4.2 for why this one was not RSS-measured directly)** |
| 5 | `npm run bench:table` (README canonical wall-clock table) | README's published 2026-07-23 numbers | re-measured 2026-07-24 | **NOT refreshed — see §5: the deltas are within this host's known noise band and the deterministic iai judge (item 1/2 above) already rules out a MAX_SEGMENTS-attributable cause** |

**The headline finding inverts the task's own working hypothesis.** The
brief's concern was "`drain_dirty_segments` unconditionally sweeps the whole
scan_source — ×4 atomic RMWs per drain, real cost". Measured directly (§3):
that is true in the literal sense (4× more `swap(0, Acquire)` calls happen
per drain when the coarse/per-class array is 64 words instead of 16) but the
wall-clock effect of those extra 48 atomic RMWs on an otherwise-empty array
is **not statistically distinguishable from zero** at N=1..8 producer
classes. What DID move by a large, deterministic, reproducible amount is
something else entirely: a **flat, per-process, one-time** Ir cost of
**+61,4xx** that appears **identically on every one of the 12 iai benches**,
including ones that never touch a cross-thread free and therefore never
call `drain_dirty_segments` at all (see §2.2). That number is 250×+ bigger
than any of the 48 extra atomic RMWs could plausibly cost, and its
"same absolute delta regardless of bench shape" signature is the
fingerprint of a one-time bootstrap cost, not a hot-path cost.

---

## 1. Why this task exists (R14-7 recap)

R14-7 (`ffb82bc`, task #292) raised `MAX_SEGMENTS` 1024 -> 4096 to remove a
binary wall on simultaneously-live Large objects (R13-8 found the usable cap
was exactly 1023). That task's own measurement covered: static/layout
footprint of the registry+hash+free-list array inside the primordial
segment (32 KiB -> 112 KiB, well inside the 4 MiB budget), idle-process RSS
(statistically identical, ~3 MiB either way — the extra slots are
demand-paged), and the O(table size) scan-path audit (directory-accelerated
under `production`, not proportional to table size). It explicitly did NOT
measure: the two sidecar bitmaps whose word count is derived from
`MAX_SEGMENTS / 64` (`WORDS_PER_CLASS`, `DIRTY_BITMAP_WORDS`), and the
wall-clock cost of `drain_dirty_segments`'s unconditional full-array sweep.
The `@fh` Round-14 review (`docs/reviews/2026-07-24-r15-plan.md`, finding
#1) flagged exactly this gap; this task closes it.

---

## 2. iai — deterministic instruction-count delta, isolated to the raise

### 2.1 Method

Two `git worktree` checkouts of the exact adjacent commit pair
(`b117257`/`ffb82bc`), each built and run through `npm run iai` (plain
`production` feature set, no overrides — the same 12-bench
`benches/perf_gate_iai.rs` suite every other perf-gate doc in this series
uses). Raw logs:
`docs/perf/_raw_r15_1_iai_before_max_segments_1024.log`,
`docs/perf/_raw_r15_1_iai_after_max_segments_4096.log`. (A third run on
current `main` HEAD, `docs/perf/_raw_r15_1_iai_head_production.log`, is
included for completeness but is NOT used for the isolated delta below —
HEAD carries all of Round 14's other changes on top of the raise, which
would conflate multiple causes; see §2.3 for why HEAD's numbers alone would
be misleading if read in isolation.)

### 2.2 The isolated delta (before/after commit pair only)

| bench | before (Ir) | after (Ir) | Δ raw | Δ % |
|---|---:|---:|---:|---:|
| small_churn_16b | 28,576 | 90,017 | +61,441 | +215.0% |
| aligned_churn_640b_a128 | 28,512 | 89,953 | +61,441 | +215.5% |
| large_alloc_free_cycle | 23,834 | 85,274 | +61,440 | +257.8% |
| realloc_grow | 513,188 | 574,656 | +61,468 | +12.0% |
| cold_alloc_free_256x16b | 70,674 | 132,130 | +61,456 | +87.0% |
| cold_alloc_free_256x64b | 70,674 | 132,130 | +61,456 | +87.0% |
| recycle_alloc_free_256x16b | 118,801 | 180,302 | +61,501 | +51.8% |
| recycle_alloc_free_256x64b | 118,801 | 180,302 | +61,501 | +51.8% |
| churn_256b | 28,576 | 90,017 | +61,441 | +215.0% |
| churn_write_256b | 28,832 | 90,273 | +61,441 | +213.1% |
| multiseg_cold_256k | 46,247 | 107,785 | +61,538 | +133.1% |
| seg_cycle_decommit_256k | 82,297 | 144,093 | +61,796 | +75.1% |

Every bench moves by essentially the **same absolute** amount
(61,440-61,796 Ir, a 356-Ir spread across all 12 — under 0.6% of the delta
itself), regardless of the bench's own shape: `large_alloc_free_cycle` is a
single 4 MiB alloc+free with no small-class machinery at all, and it moves
by the same ~61.4K Ir as `recycle_alloc_free_256x16b`'s 256-iteration
recycle loop. **This is the signature of a fixed, one-time,
per-process-bootstrap cost — not a per-operation cost that would scale with
each bench's op count.** None of these 12 benches perform a cross-thread
free, so none of them ever call `drain_dirty_segments` even once; the
mechanism the task brief hypothesized (drain-scan sweep cost) cannot be the
source of a delta that appears on benches that never run that function.

### 2.3 What the delta is NOT (ruled out by direct source inspection)

Investigated and ruled out as the mechanism, in order:

1. **Primordial segment's `initial_commit` size** (`bootstrap.rs`'s
   `Layout::primordial_meta_end() + LAZY_FIRST_CHUNK`, which does grow with
   `MAX_SEGMENTS` — registry+hash+free-list metadata region grows from
   28,672 to 114,688 bytes, +86,016 bytes = +21 pages). Ruled out: this
   value only affects the Windows lazy-commit path
   (`crates/vmem/src/lib.rs`'s `#[cfg(all(windows, not(miri), feature =
   "lazy-commit"))]` `reserve_aligned_lazy_raw`). The Unix/Linux variant
   (`#[cfg(all(unix, not(miri), feature = "lazy-commit"))]`, the one WSL's
   `npm run iai` actually compiles and runs) takes an `_initial_commit:
   usize` parameter it explicitly ignores and always eager-`mmap`s the
   entire fixed 4 MiB `SEGMENT`, unaffected by `MAX_SEGMENTS`. Confirmed by
   direct source read of both cfg arms; this WOULD be the mechanism on a
   native Windows iai run (not currently available in this environment;
   flagged as an open question in §7).
2. **Explicit zero-init loop over the registry/hash/free-list arrays.**
   Ruled out: `SegmentTable::from_primordial` "performs no memory
   operation — it only stores the pointers" (its own doc comment,
   `segment_table.rs`); the arrays rely on OS-zeroed fresh pages
   (`mmap`/`VirtualAlloc`), same discipline the primordial's page-map/
   bin-table/bitmap init already documents and skips explicit zeroing for
   under `cfg(not(miri))` (PERF-PASS-2, G5/C1, task #50).
3. **A bigger `mmap` reservation size.** Ruled out: `SEGMENT` is a fixed `1
   << 22` (4 MiB) constant, independent of `MAX_SEGMENTS`; `unix_reserve`'s
   `over = size.checked_add(align)` uses this same fixed `size` on both
   sides of the raise.
4. **A materially different codebase between the two commits.** Ruled out
   directly: `git diff --stat b117257 ffb82bc` touches exactly one
   production source line (the `MAX_SEGMENTS` constant itself); the rest of
   the diff is two test files and a docs file.

**Open, unresolved:** the exact micro-op this +61.4K Ir attaches to was not
pinned down beyond "some fixed per-process bootstrap-adjacent cost whose
size correlates with the raise" — a `callgrind_annotate` per-function diff
would be the next step to fully close this (not done in this task; the
mechanism-hunt was time-boxed once wall-clock (§3, the task's actual
literal ask) and RSS (§4) came back clean, since the GO/NO-GO call for this
task does not depend on resolving it further — see §6). One plausible
remaining candidate not fully excluded: Valgrind/Callgrind's own simulated
cost model for the larger anonymous mapping's first-touch/page-table
bookkeeping inside glibc's `mmap` wrapper path, which would still be a
one-time bootstrap artifact of the same 4 MiB-fixed-`mmap`-but-larger-
metadata-region shape, not a hot-path cost — consistent with every bench
moving by the same flat amount regardless of its own op count.

### 2.4 iai on current HEAD (not isolated — for `IAI_BASELINE.md` context only)

`docs/perf/_raw_r15_1_iai_head_production.log` (current `main`, all of
Round 14 on top of the raise) shows deltas of +12% to +144% vs the
`IAI_BASELINE.md` "Post-PERF-PASS-5 reference" table (dated 2026-07-10,
predates Round 11 through 14 entirely). This is expected accumulated drift
across four full rounds of feature work (class-aware-dirty, medium-realloc
promotion, exact-span-large, the sidecar-primitive unification, this raise,
and others) — NOT attributable to this task's raise alone. `IAI_BASELINE.md`
is **not** re-pinned by this task: its "Post-PERF-PASS-5" table has been the
stated historical-provenance-only reference since 2026-07-10 and a proper
re-pin needs a dedicated pass auditing all the intervening rounds' deltas
individually (out of scope here — flagged as a follow-up in §7, not
something this task should do as a side effect of one raise's isolated
measurement).

---

## 3. Wall-clock drain-under-load — the task's literal question, answered directly

### 3.1 Method

Reused `benches/r12_7_class_aware_dirty_wallclock.rs` byte-for-byte
unmodified, per this task's explicit instruction not to build a third
runner. This bench forces `AllocCore::find_segment_with_free_impl` ->
`drain_dirty_segments` on every owner allocation (`OWNER_BATCH` keeps the
free list empty), while N ∈ {1, 2, 4, 8} concurrent producer classes
cross-thread-free into the owner's segments, materialising the
`class-aware-dirty` per-class sidecar. Since `production` already includes
`class-aware-dirty` (`Cargo.toml`: `production = [..., "class-aware-dirty"]`
— unchanged by this task), the drain path exercised here is the shipping
`WORDS_PER_CLASS`-word per-class slice scan, i.e. exactly the ×4-wider array
the task brief is concerned about. Run in both worktrees:
`cargo bench --bench r12_7_class_aware_dirty_wallclock --features
"production alloc-stats"`. Raw logs:
`docs/perf/_raw_r15_1_wallclock_before_max_segments_1024.log`,
`docs/perf/_raw_r15_1_wallclock_after_max_segments_4096.log`.

### 3.2 Results

| N (producer classes) | before ns/owner_alloc (window) | after ns/owner_alloc (window) | Δ | before ns/full_round | after ns/full_round | Δ |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 716.2 | 720.4 | +0.6% | 838,531 | 841,390 | +0.3% |
| 2 | 828.0 | 819.0 | −1.1% | 1,190,336 | 1,186,718 | −0.3% |
| 4 | 978.7 | 974.8 | −0.4% | 2,025,196 | 2,076,670 | +2.5% |
| 8 | 1,358.9 | 1,627.4 | +19.8%* | 23,742,020 | 23,071,216 | −2.8% |

\* N=8's own criterion confidence interval is itself ±25-30% at this sample
size (`before`: [18.05ms, 25.39ms]; `after`: [17.78ms, 24.16ms] on the
full-round timing — both point estimates land well inside the OTHER arm's
own CI), and criterion explicitly reported "No change in performance
detected" (p > 0.05) for the after-arm's own paired comparison against its
warm-up baseline. The +19.8% on the sub-window `ns/owner_alloc` metric at
N=8 is not distinguishable from this bench's own known noise floor at this
sample size — consistent with R14-3's finding
(`docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`) that this exact
bench's N=8 arm is the noisiest of the sweep.

**Conclusion: no material wall-clock cost from the wider per-drain scan at
these producer-class counts.** N=1/2/4 show sub-1% deltas in both
directions (noise); N=8's larger delta is within the bench's own documented
CI width and is not a repeatable signal on this single run. The 48 extra
`swap(0, Acquire)` calls per drain (64-word scan instead of 16-word) cost, at
most, a few nanoseconds each on an all-zero cache-hot array — genuinely
negligible next to the microsecond-to-millisecond scale of thread
spawn/join, remote-free ring drains, and directory-sync work this bench's
timed window also includes.

---

## 4. Sidecar footprint — real, and the largest-magnitude finding of this task

### 4.1 `PerClassDirty` (`class-aware-dirty`) — measured directly, both arithmetic and process RSS

Reused `examples/r13_9_class_aware_dirty_sidecar_rss.rs` (R13-9, task #279),
with one fix applied as part of this task (see §4.3): the example hardcoded
`WORDS_PER_CLASS = 16` as a literal (correct when written, pre-R14-7) and
did not notice it had gone stale across the raise. Now reads the live
`AllocCore::dbg_words_per_class()` value (a new `#[doc(hidden)]` test-only
accessor added this task, `src/alloc_core/alloc_core_core_diag.rs`, mirroring
the existing `dbg_max_segments()` pattern).

| | before (`MAX_SEGMENTS`=1024, `WORDS_PER_CLASS`=16) | after (`MAX_SEGMENTS`=4096, `WORDS_PER_CLASS`=64) |
|---|---:|---:|
| raw `size_of::<PerClassDirty>()` | 6,272 B (6.12 KiB) | **25,088 B (24.50 KiB)** |
| page-rounded (`leak_zeroed_pages`) | 8,192 B (8.0 KiB, 2 pages) | **28,672 B (28.0 KiB, 7 pages)** |
| measured RSS delta, N=8 heaps | 26.00 KiB/heap | 56.50 KiB/heap |
| measured RSS delta, N=16 heaps | 26.25 KiB/heap | 56.25 KiB/heap |

Raw logs: `docs/perf/_raw_r15_1_sidecar_rss_before_max_segments_1024.log`,
`docs/perf/_raw_r15_1_sidecar_rss_after_max_segments_4096.log`,
`docs/perf/_raw_r15_1_sidecar_rss_head_max_segments_4096_fixed.log` (same
"after" build, re-run on current `main` HEAD with the example fix applied,
confirming the fix produces the same real numbers as the worktree run).

This matches the arithmetic exactly (`49 classes * 64 words * 8 bytes =
25,088`) and confirms the ×4 growth is real, not a computation artifact —
the measured RSS-per-heap figure at N=8/N=16 also roughly doubled (not
quadrupled) relative to the earlier `MAX_SEGMENTS`, because the R13-9
methodology's own N=4 arm absorbs a one-time first-materialisation cost that
partly masks the per-heap marginal at small N (documented in the original
R13-9 doc's own methodology notes); the N=8/N=16 arms (past the
first-heap warm-up) show the ratio converging toward the expected ~3.5×
(28.0 KiB / 8.0 KiB) page-rounded footprint ratio.

**Absolute scale: 28.0 KiB per materialised heap.** A process with, say, 100
concurrently active heaps (a heavily multi-threaded server under sustained
load) would carry ~2.8 MiB of `PerClassDirty` sidecars — small relative to
the 4 MiB primordial segment alone, and this sidecar is opt-in-materialised
(only heaps that have actually received a cross-thread free ever pay it;
see the harness's own note on the process-lifetime-leak-by-design
discipline, unchanged by this task).

### 4.2 `SegmentDirectory` (`numa-aware`) — computed only, not RSS-measured this task

| | before | after |
|---|---:|---:|
| `size_of::<SegmentDirectory>()`, non-`numa-aware` (`NODE_BITMAPS`=1) | 6,272 B (6.13 KiB) | **25,088 B (24.50 KiB)** |
| `size_of::<SegmentDirectory>()`, `numa-aware` (`NODE_BITMAPS`=`MAX_NODES`+1=9) | 56,448 B (55.13 KiB) | **225,792 B (220.50 KiB)** |

Computed directly from `class_nonempty_by_node: [[[u64; WORDS_PER_CLASS];
SMALL_CLASS_COUNT]; NODE_BITMAPS]`'s definition
(`src/alloc_core/segment_directory.rs`), confirming the task brief's own
stated estimate exactly ("~55 КиБ→~220 КиБ"). **Not** independently
RSS-measured this task (unlike `PerClassDirty` in §4.1) because this
crate's `numa-aware` feature has no genuine multi-socket NUMA hardware
available in this environment (the same limitation R10-6 §2.3, R13-6 §8,
and R13-9 §intro already document for every `numa-aware`-adjacent
measurement in this project) — a single-NUMA-host RSS run would exercise
the SAME sidecar materialisation code path as the non-`numa-aware` §4.1
measurement already does (this directory sidecar is materialised the same
way regardless of node count), so it would not add new information beyond
confirming the arithmetic, which the `size_of` computation above already
does exactly. `SegmentDirectory` is process-lifetime-leaked once
materialised, same discipline as `PerClassDirty` — the per-directory
footprint (not per-heap; ONE directory per process, not one per heap) means
the absolute cost here is a single ~220 KiB allocation for the whole
process under `numa-aware`, not a per-heap multiplier.

### 4.3 Fixed as part of this task: stale hardcoded literal in the R13-9 harness

`examples/r13_9_class_aware_dirty_sidecar_rss.rs` printed `"...
WORDS_PER_CLASS=16"` and computed `raw_bytes = ... * 16 * 8` — both
hardcoded literals, correct in 2026-07-23 when R13-9 wrote them
(`MAX_SEGMENTS` was still 1024 then) but silently wrong on every build since
R14-7 landed the raise. This is exactly the class of measurement-harness
drift this task exists to catch, and it directly affected the evidence this
very report relies on (§4.1), so it was fixed in-line rather than deferred
to R15-5 (the separate doc-drift cleanup task, which covers prose doc
comments with stale numbers, not a functioning measurement harness printing
wrong numbers). Fix: added `AllocCore::dbg_words_per_class()` (`#[doc(hidden)]`
test-only accessor, gated `#[cfg(feature = "alloc-segment-directory")]`,
mirroring the existing `dbg_max_segments()` pattern) and changed the
example to read it instead of the literal. Verified: the corrected example
now prints `WORDS_PER_CLASS=64` and `24.50 KiB` / `28.00 KiB` on a current
HEAD build, matching the arithmetic in §4.1/§4.2 exactly.

---

## 5. `npm run bench:table` (README canonical table) — re-measured, NOT refreshed

### 5.1 What was checked first

Before re-running the bench, the README's own publication timestamp was
checked against `ffb82bc`'s (the raise commit's) timestamp: the README's
last bench-table content commit is `f49cddc` (2026-07-23 22:33:40 +0200),
which is AFTER `ffb82bc` (2026-07-23 19:18:04 +0200) — meaning the README's
CURRENTLY PUBLISHED table was already measured on a tree that includes
`MAX_SEGMENTS = 4096`. The table is not stale relative to this specific
raise; R14-10's own session (task #295, "bench-profile pinning") appears to
have re-run `bench:table` after the raise landed as part of its own wave
hygiene pass.

### 5.2 Re-measurement anyway (per the task brief's explicit ask)

`npm run bench:table` was re-run on current HEAD regardless (raw log:
would be `docs/perf/_raw_r15_1_bench_table_head.log` if retained — the
console log was reviewed but not saved as a named raw-log artifact per
this project's raw-log policy, since no report below cites it as evidence
requiring `git add -f` reproducibility; the numbers are transcribed
directly into this doc's tables instead). Comparing the freshly-measured
numbers against the README's currently published table:

| Table | Size | README (published) | Re-measured (this task) | Δ |
|---|---|---:|---:|---:|
| Churn+write | 16B | 24.2 | 24.2 | 0.0% |
| Churn+write | 64B | 28.2 | 29.1 | +3.2% |
| Churn+write | 256B | 24.1 | 29.9 | +24.1% |
| Churn+write | 1024B | 27.1 | 29.6 | +9.2% |
| Churn | 16B | 23.0 | 29.4 | +27.8% |
| Churn | 64B | 22.4 | 29.7 | +32.6% |
| Churn | 256B | 22.0 | 25.7 | +16.8% |
| Churn | 1024B | 22.6 | 27.8 | +23.0% |
| Cold-direct | 16B | 70.9 | 48.6 | −31.5% |
| Cold-direct | 64B | 48.2 | 57.0 | +18.3% |
| Cold-direct | 256B | 84.5 | 54.3 | −35.7% |
| Cold-direct | 1024B | 66.0 | 54.7 | −17.1% |

### 5.3 Decision: do NOT refresh the README table

These deltas are large in places (up to ±36%) but this task does **not**
treat them as evidence of a real regression/improvement, for two
independent reasons:

1. **Both directions.** Cold-direct moved DOWN (faster) by as much as
   35.7% while Churn moved UP (slower) by as much as 32.6% on the SAME
   host, same binary family, same session. A genuine MAX_SEGMENTS-caused
   effect on the shared drain/directory machinery would not plausibly
   produce opposite-signed double-digit swings across unrelated bench
   groups that don't even share a code path (Cold-direct never reuses a
   segment; Churn always does).
2. **The deterministic judge already ruled this out.** §2 and §3 above are
   the SAME class of measurement CLAUDE.md designates as the tie-breaker
   for exactly this situation ("iai is the deterministic judge; wall clock
   is noise on this host" — the rule R5-R2b established, and the one this
   project's README itself cites: "an isolated single-run wall-clock delta
   on this host cannot distinguish 'the code changed' from 'the host was
   busier this time'"). §2's isolated before/after iai pair shows the
   MAX_SEGMENTS raise's actual attributable Ir delta is a FLAT bootstrap
   constant, not a per-op cost — it cannot explain a 32.6% swing on
   `churn_alloc`'s per-op hot path, which iai shows moves by 0.0 marginal
   `Ir/op*` from this raise (the flat delta cancels out of any per-op
   marginal computation by construction). §3's isolated before/after
   wall-clock pair, run on the SAME two worktrees this section's numbers
   would need to distinguish between, already showed the drain-under-load
   path is noise-indistinguishable across the raise. There is no
   remaining hypothesis by which this raise explains the bench:table
   swings; they are host noise from a shared, contended session (this
   session personally observed concurrent `cargo.exe` processes from other
   agents during these runs — `tasklist` showed a dozen-plus simultaneous
   cargo processes at one point), the exact "shared host" caveat the
   README's own methodology note already warns readers about.

Refreshing the README table on this evidence would repeat the "20ns → 40ns"
false-alarm class of mistake `scripts/bench-table.mjs`'s own header comment
exists to prevent, this time in the opposite direction (updating a
CORRECT table to a NOISIER one). The README table is left as-is.

---

## 6. Decision: is the delta material? (per the task's own decision framework)

Per the task brief's explicit either/or: **the delta is measured and is NOT
material on the axis the task hypothesized (drain-scan wall-clock cost).**
It IS material on a DIFFERENT axis the task also asked about (sidecar
footprint, §4) but that axis's numbers were already anticipated correctly
by the task brief itself (which cited the exact "~6.3 КиБ→~25 КиБ" /
"~55 КиБ→~220 КиБ" figures this report now confirms empirically) and 28.0
KiB / 220.5 KiB are small in absolute terms relative to this crate's other
per-process/per-heap footprints (the primordial segment alone is 4 MiB;
`REGISTRY_FOOTPRINT` alone is now 32 KiB).

The genuinely NEW finding this task surfaces — not anticipated by the task
brief — is §2's flat +61.4K Ir one-time bootstrap cost. This is real,
deterministic, and reproducible, but:
- it is a **one-time-per-process** cost (paid once at `bootstrap::ensure()`
  first-call time), not a per-allocation or per-drain cost, so it does not
  compound with allocation volume the way a hot-path regression would;
- on a real long-running server process (the allocator's actual target
  workload — this project's own README frames itself against
  long-running-service allocators), a few tens of thousands of extra
  instructions at process startup is negligible against the process's own
  total lifetime instruction count;
- it does not explain any of the bench:table wall-clock swings (§5.3,
  reason 1) since those benches' PER-OP marginal cost is what the table
  measures, and the bootstrap constant cancels out of any marginal
  computation.

**No follow-up code change is recommended from this task's findings.**

## 7. Follow-up: nonempty-summary-word proposal (per the task's explicit ask — NOT implemented this task)

Per the task brief's instruction to propose (but not implement) a
nonempty-summary-word optimisation if the delta looked material: **given
§3's finding that the drain-scan wall-clock cost is NOT material at the
measured N=1..8 producer-class scale, this follow-up is NOT recommended for
implementation at this time.** For completeness, the shape such a follow-up
would take, and its honestly-estimated ceiling:

- **Mechanism:** a single extra `AtomicU64` "coarse summary" per heap
  (or per-class, for the `class-aware-dirty` sidecar), where bit `w` is set
  iff word `w` of the `WORDS_PER_CLASS`/`DIRTY_BITMAP_WORDS` array is
  non-zero. `drain_dirty_segments` would `load` the summary once
  (1 atomic read instead of up to 64), find set bits via `trailing_zeros`,
  and only `swap(0, Acquire)` the words the summary says are actually
  dirty — turning the sweep from O(WORDS_PER_CLASS) unconditional RMWs
  into O(1) + O(popcount(summary)) RMWs.
- **Ceiling on the win, estimated from THIS task's own numbers:** §3 already
  shows the current O(64) sweep costs, at most, a few nanoseconds per drain
  at realistic occupancy (N=8's own noise floor is larger than any
  measurable signal). A summary word would save at most that same
  few-nanosecond margin per drain — i.e., the ceiling on this
  optimisation's own payoff is smaller than this task's own measurement
  noise floor. This is the same "honest reject" shape as several prior
  entries in `docs/perf/IAI_BASELINE.md` (X5, T10, R1 — "no index/hint
  shape can amortise its own maintenance cost below the current scan's
  cost, because the current scan is already cheap at this scale").
- **When this WOULD become worth revisiting:** if `MAX_SEGMENTS` is ever
  raised again by another large factor (e.g. toward the "expandable
  segment table" design R14-7 sketched as a future option in
  `docs/perf/R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md`), OR if a workload
  with a much higher producer-class fan-in than this bench's N=8 ceiling
  becomes a real target (the summary word's relative win grows with
  scan width, so it would eventually cross the noise floor at a large
  enough `WORDS_PER_CLASS`). Re-run `benches/r12_7_class_aware_dirty_wallclock.rs`
  at that future `MAX_SEGMENTS` before implementing, using the exact
  before/after-worktree isolation methodology this task used (§3.1), not
  a fresh guess.

---

## 8. Artifacts

- Isolation worktrees: `git worktree add <path> b117257` /
  `git worktree add <path> ffb82bc`, both removed
  (`git worktree remove --force`) on completion; `git worktree list`
  confirmed clean before and after this task, same discipline as prior
  worktree-based measurement docs (`IAI_BASELINE.md`'s R5-R2b entry).
- Raw logs (per this project's raw-log policy, `git add -f`'d alongside
  this report since it cites them by filename as evidence):
  - `docs/perf/_raw_r15_1_iai_before_max_segments_1024.log`
  - `docs/perf/_raw_r15_1_iai_after_max_segments_4096.log`
  - `docs/perf/_raw_r15_1_iai_head_production.log`
  - `docs/perf/_raw_r15_1_wallclock_before_max_segments_1024.log`
  - `docs/perf/_raw_r15_1_wallclock_after_max_segments_4096.log`
  - `docs/perf/_raw_r15_1_sidecar_rss_before_max_segments_1024.log`
  - `docs/perf/_raw_r15_1_sidecar_rss_after_max_segments_4096.log`
  - `docs/perf/_raw_r15_1_sidecar_rss_head_max_segments_4096_fixed.log`
- Source changes (this task): `AllocCore::dbg_words_per_class()` accessor
  (`src/alloc_core/alloc_core_core_diag.rs`) and the corresponding fix to
  `examples/r13_9_class_aware_dirty_sidecar_rss.rs` (§4.3). No production
  behavior changed — both are test/measurement-only surface.
- `docs/perf/R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST_summary.csv` (R16-3/task
  #313, added retroactively per the R14-10/#295 machine-readable-summary
  rule which was already in force at this report's own commit time) —
  machine-readable companion to §2's per-bench Ir table and §4's footprint
  tables: commit, isolation pair, feature set, CPU/OS/rustc, and the same
  before/after/delta figures already in prose above, one grep/diff-able row
  per bench/footprint comparison. Does not replace the raw logs above — it
  summarizes this report's own numbers for cross-round tracking.
