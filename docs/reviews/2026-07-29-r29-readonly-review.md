# Read-only review: Round 29

Date: 2026-07-29

Reviewed range: `b7ff9fe..a8f6ff4` (23 commits, R29-1 … R29-17 + gate/doc commits)

Review mode: read-only, but **not** paper-only. Unlike the R27–28 review, this
one ran inspection commands: `cargo check`/`clippy`/`tree`/`metadata` on
Windows, a YAML parse of `ci.yml`, an independent Python re-implementation of
the R29-9 tripwire's own scan, an independent line-multiset diff of the
`OPEN_ITEMS.md` split, and one `cargo check` inside WSL2/Linux. No file was
modified except this report; no formatter, no fix, no commit.

---

## Executive verdict

Round 29 is, in substance, the strongest process round this project has had.
It closed two independent review backlogs to zero, found and fixed a real
flaky-test root cause instead of accepting a hypothesis, converted the
recurring R25-1 soundness class into a mechanically enforced invariant (which
then caught a fourth live instance the same day), and split a 1,581-line index
without losing a single line of history. Its four measurement reports are
backed by committed raw logs whose numbers I reproduced verbatim, one by one.

It also **shipped a red pre-push gate and a red CI job**, and its two headline
quantitative claims do not survive scrutiny.

- **P0 — `npm run check` and the `perf-gate` CI job are broken at `HEAD`.**
  R29-16 registered four `virgin-zero-skip`-gated iai arms in
  `library_benchmark_group!` without the `#[cfg(not(...))]` no-op stub the same
  file already uses for exactly this situation. Confirmed on Linux:
  `cargo check --bench perf_gate_iai --features "production bench-internals"`
  → 4 × `E0433`. That is the literal command in `perf-gate.yml` and the default
  in `scripts/iai.mjs`.
- **P1 — `cargo clippy --features production -- -D warnings` exits 101 at
  `HEAD`**, on three dead-code items R29-4 introduced. This is the *same*
  ungated-definition-vs-gated-reexport bug that commit `8e6f9d1` fixed for the
  promotion counters, in the same round, missed for R29-4's structs. No CI row
  and no `npm run check` row covers plain `production` clippy, so nothing
  catches it.
- **P1 — R29-16's "21.4× Ir win" is a Valgrind artifact.** 65,624 Ir for a
  65,536-byte memset is 1.001 instructions per byte. No native memset costs
  that; the number is Valgrind's emulation of `memset` (replacement byte/word
  loop, or `rep stos` counted per repetition). The report's own attempted
  sanity check is off by 64× against its own number.
- **P1 — R29-16's wall-clock arm cannot detect the feature it tests.** Its
  "virgin" scenario frees the whole batch each criterion iteration, so from
  iteration 2 of ~30,000 onward every `alloc_zeroed` is a free-list pop, i.e.
  not virgin. The skip fires ~16 times out of ~500,000 calls. The report's
  page-commit explanation for the null result is describing the wrong cause.
- **P2 — R29-3's verdict prose contradicts R29-3's own numbers** ("net loss"
  vs. a measured +1.7–5.1% saving), and the contradiction was copied into
  `OPEN_ITEMS.md` items 15 and 16 as the recorded verdict.
- **P2 — the round's own new immutable-provenance rule is violated by the
  first two reports subject to it**: R29-13 cites a 63-character "sha256" and a
  recipe that provably cannot reproduce it; R29-16 cites no source identity at
  all.
- **P2 — R29-13's "30×" and R29-5's "0.82%" are both denominator artifacts**,
  and both reached `CHANGELOG.md`.

Nothing I found is a soundness defect in shipping allocator code. The four
`dbg_*` hook tasks are correct work, correctly done. The damage is confined to
(a) two broken build/lint invariants and (b) four overstated numbers in
measurement reports — the exact failure mode this project's rules exist to
prevent, recurring one level up: the round audited its *hooks* rigorously and
its *own arithmetic* loosely.

**Recommended immediate action, before anything else in Round 30:** fix the two
build breaks (P0-1, P1-2) and append dated corrections to
`R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md`, `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`,
`R29_13_LARGE_CACHE_RETENTION_GATE.md`, `R29_5_PROMOTION_FREQUENCY_GATE.md`,
`OPEN_ITEMS.md` items 15/16/25/L27, and the Round 29 `CHANGELOG.md` entry.

---

## 1. Soundness / the `unsafe` discipline

### 1(a) — the re-gated hooks: **VERIFIED CORRECT**

I read each hook and its callers.

- `tls_heap::dbg_restore_local_for_test` (`src/global/tls_heap.rs:776`) is now
  `pub unsafe fn`, `#[cfg(feature = "bench-internals")]` (`:775`), with a
  `# Safety` contract that states the *actual* precondition — null-or-
  previously-returned, this-thread-only, never sent across threads — and names
  the exact UB (the resolver classifies non-nullish as `CurrentHeap::Own` and
  dereferences). Covered by the file's existing tier-1
  `#![allow(unsafe_code)]`; no new tier-2 site, as claimed.
- Its twin `dbg_mark_local_torn_for_test` (`:739`) stayed a safe `fn` and got
  the `bench-internals` gate. The justification in the comment is right and
  non-lazy: the body only reads/writes `LOCAL`'s bit pattern, never
  dereferences, so it has no contract to carry. CLAUDE.md's tier-2 rule
  ("a documented reason per site") is respected rather than gestured at.
- `AllocCore::dbg_force_decommit_retain_for`
  (`src/alloc_core/alloc_core_small_pool.rs:815`) is `pub unsafe fn` under
  `#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]`
  (`:813`) with an item-level `#[allow(unsafe_code)]` (`:814`) — correctly
  identified as a genuinely NEW tier-2 site, since that file has no
  module-level lift. The `# Safety` doc names `live_count == 0` and explicitly
  distinguishes what the body *does* check (`contains_base_ro`, `Small` kind)
  from what it does not.
- `HeapCore::dbg_directory_bit_for_ptr` (`src/registry/heap_core_diag.rs:257`)
  got a `segment_bases().any(|b| b == base)` guard *before* the
  `SegmentHeader::segment_id_at(base)` read (`:268-271`). Correct: a null `ptr`
  masks to base 0, fails containment, returns `None` — no crash, no garbage
  sid. The guard-and-return-`None` shape (rather than assert-panic) is the
  right choice here because the function's pre-existing doc already promised
  `None` for a foreign `ptr` — the fix *implements a documented contract that
  was never implemented*, which is a better outcome than a new panic.

Callers are all correctly gated (`tests/dealloc_only_no_bind_torn.rs:180/205`
per-function, `tests/alloc_zeroed_virgin_small_skip.rs:425` per-function,
`tests/r11_2_overflow_drain_directory_sync.rs:322` unaffected). No caller
exists in `examples/` or `benches/` that would break.

`cargo clippy --all-features --all-targets -- -D warnings` is clean (exit 0).

**One accuracy note (P2-14, see §6):** the `CHANGELOG.md` headline says "four
more safe `dbg_*` raw-pointer hooks re-gated `unsafe`+`bench-internals`", and
its R29-17 bullet says R29-17 "re-gated `HeapCore::dbg_directory_bit_for_ptr`
`unsafe fn` + `bench-internals`". R29-17 did neither — its own commit message
(`bd1bd3e`) says "Stays a safe fn, no bench-internals gating needed". The real
count of hooks that became `unsafe fn` this round is **two**
(`dbg_restore_local_for_test`, `dbg_force_decommit_retain_for`), plus one that
was gated but stayed safe, plus one that got a containment guard.

### 1(b) — does the tripwire enumerate the CURRENT set? **YES — independently confirmed**

I did not trust the test. I wrote an independent scanner (different language,
different attribute-walk direction: backwards from each `pub fn dbg_*` rather
than forwards with an accumulator) over `src/` + `crates/` and diffed its
at-risk set against the union of the test's three lists.

Result: **29 safe, non-`bench-internals`-gated, pointer-shaped `pub fn dbg_*`
hooks found; 29 in the allowlist union; 0 extras, 0 missing.**
Per-file: 13 in `alloc_core_core_diag.rs`, 4 in `alloc_core_small_diag.rs`, 2
in `alloc_core_small_pool.rs`, 2 in `alloc_core_small_reclaim.rs` (1 of them
the declared heuristic false positive), 8 in `heap_core_diag.rs`. My scan also
independently found the same 5 pointer-shaped hooks that *are*
`bench-internals`-gated and therefore correctly excluded. `cargo test --test
dbg_hook_safety_tripwire` passes, and it is non-vacuous by construction (the
`found` set has 29 members, so a 30th unaccounted hook produces a non-empty
`extras` and fails).

**There is no 5th instance the allowlist misses today.** Three latent scope
holes are worth recording so a future round does not rediscover them
(P3-19, §6): the `has_bench_internals_cfg` string test would misclassify an
`any(...)`/`not(...)` gate as "gated"; integer-keyed metadata-mutating hooks
are out of scope (I checked `dbg_directory_force_clear_bit`,
`RemoteFreeRing::dbg_set_cursors`, `dbg_directory_set_miss_streak_for_class` —
each is bounds-checked or panics rather than UB, so no live instance); and
zero-input hooks that corrupt allocator state are out of scope, which matters
because this round created one (P2-8 below).

### 1(c) — R29-13's four new `HeapCore` delegations: **genuinely safe, correctly out of scope**

`dbg_large_cache_used`, `dbg_large_cache_slot_sizes`, `dbg_decay_config`
(`src/registry/heap_core_diag.rs:326-390`) take no arguments and return
`usize` / `[Option<usize>; 8]` / `(u32, u64, usize)` — no raw pointer in or
out, `&self`, pure reads forwarded to pre-existing `AllocCore` accessors.
`dbg_force_decay_tick` (`:388`) mutates via `&mut self` but takes **no caller
input at all**, so there is nothing to validate. All four are
`#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]`. They
are legitimately outside the tripwire's declared scope, and would be excluded
by its gate check anyway.

### 1(d) — P2-8: R29-3's `dbg_decomp_*` hooks leave the allocator in a UB-on-next-alloc state

**Severity: P2** (bench-internals-gated, no in-tree victim today, but it is the
R25-1 *spirit* in a shape the new tripwire structurally cannot see).

`AllocCore::dbg_decomp_full_cycle` (`src/alloc_core/alloc_core_small_pool.rs:1014`)
is a **safe `pub fn`**. It calls `reserve_small_segment()`, which sets
`self.small_cur = base` (`src/alloc_core/alloc_core_small.rs:2210`), and then
`release_or_pool_empty_segment(base)`, which on the release branch clears the
directory, calls `release_empty_segment_now`, and `table.recycle(base)`
(`alloc_core_small_pool.rs:361-371`) — **without clearing `small_cur`.**
`dbg_decomp_reserve_and_keep` + `dbg_decomp_release` have the same shape.

Production never reaches this state: `dec_live_and_maybe_decommit` refuses
when `base == small_cur` (`:172`), and `drain_small_pool` /
`finalize_orphaned_empty_segments` both skip `small_cur` (`:422`). There is
exactly one assignment to `small_cur` in the whole crate (`:2210`), so nothing
repairs it.

Failure scenario: `examples/r29_3_decomposition_gate.rs` pre-fills the pool to
`pool_cap` (`:83-85`), so all 200 timed iterations of arm A and all 200 of arm
A′ take the **release** branch. On return from `main`'s measurement loops, the
claimed `HeapCore`'s `small_cur` points at unmapped virtual address space. The
example survives only because it does not install `SeferAlloc` as
`#[global_allocator]` and never allocates through that heap again — a
property of the current caller, not of the hook. Any future caller that does
one small allocation on that heap after the loop carves from freed VA.

Recommended: either restore `small_cur` in the hooks, or make
`dbg_decomp_full_cycle`/`dbg_decomp_reserve_and_keep` `unsafe fn` with a
documented "no allocation may occur on this heap afterwards" contract. Either
way, this is the concrete argument for widening the R29-9 tripwire's charter
from "raw-pointer-shaped" to "leaves allocator state invalid".

---

## 2. Measurement integrity

Every number I could check against a committed artifact, I checked. **All four
reports' cited figures reproduce verbatim from their raw logs** — R29-3
(`_raw_r29_3_decomposition_run1.log:8-26`, `run2.log:8-26`), R29-5
(`_raw_r29_5_run1.log:8-30`), R29-10 (`_raw_r29_10_run1.log:657-658`,
`run2.log:561-562`), R29-13
(`_raw_r29_13_large_cache_retention_gate.log:951-953`, CSV `:986-994`), R29-16
(`_raw_r29_16_calloc_isolation_run2.log:7-10`). R29-4's per-state table
reproduces from `_raw_r29_4_reconciliation_run1.log:15-64`. No fabricated
number, no citation I could not verify. That part of the discipline held
completely.

The problems are in what the numbers *mean*.

### P1-3 — R29-16's 21.4× Ir ratio is a Valgrind emulation artifact

`docs/perf/R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md:102-103` reports the
recycled arm's isolated delta as `200,443 − 134,819 = 65,624 Ir` for one
`alloc_zeroed(64 KiB)` whose only extra work vs. the virgin arm is a
`Node::zero` over 65,536 bytes (`src/alloc_core/node.rs:143`,
`core::ptr::write_bytes` → `memset`).

`65,624 / 65,536 = 1.0013 instructions per byte.`

No real memset costs one instruction per byte. glibc's AVX/ERMS paths cost
~1 instruction per 32–64 bytes, or a single `rep stosb` for the whole span.
Both candidate mechanisms are Valgrind-side: Valgrind redirects `memset` to
its own scalar replacement, and Callgrind counts each `rep`-string repetition
as a separate instruction. Either way the figure is an emulation artifact and
does **not** transfer to hardware — nor does the `Est. Cycles` delta
(`475,492 − 339,854 = 135,638`), against a real ERMS 64 KiB store of roughly
5,000 cycles at ~30 GB/s on the cited 2.30 GHz part.

Independent confirmation inside the report's own table: the *prefix* arms
differ by `134,819 − 66,132 = 68,687 Ir`, and the only material difference
between those two prefixes is one `alloc` + `write_bytes(ptr, 0xAA, 65536)` +
`dealloc` (`benches/perf_gate_iai.rs:1214-1219`). Same ~1 Ir/byte for a second,
unrelated 64 KiB memset.

The report noticed the anomaly and papered over it. Its own sanity check reads:
"65,536 bytes ÷ ~64 Ir-equivalent bytes/instruction-group is the right order of
magnitude" (`:107-109`). That arithmetic yields **1,024**, not 65,624 — off by
64× — and the sentence is written as though it passed.

What survives: the *direction* (virgin skips a memset, recycled does not) and
the determinism (two byte-identical runs). What does not survive: the magnitude,
and therefore the report's §3 characterisation ("a real, large, deterministic
instruction-count win"), the §6 bottom line, the `R13_3_…` addendum's
"real, large, and deterministic … ~21.4× ratio", `OPEN_ITEMS.md` item 25's
"REAL, LARGE win", and the `CHANGELOG.md` headline's "real 21.4x Ir win".

This matters beyond one report: it is the first documented case in this tree
of Ir being *structurally* misleading in the direction of overstating a win.
The project's existing caveat inventory covers Callgrind's blindness to page
faults (R5-R2b, and R29-3 §1) but not its inflation of `mem*` primitives. That
belongs in `IAI_BASELINE.md` as a standing caveat, because any future arm whose
delta is dominated by `memset`/`memcpy` inherits it — including R29-5's
promotion memcpy if anyone ever measures it in Ir.

### P1-4 — R29-16's wall-clock "virgin" arm measures the recycled path

**The harness cannot detect the feature it exists to test.**

`benches/r29_16_virgin_zero_skip_calloc_wallclock.rs:101-107`:

```rust
// Free everything at the end of the batch (not interleaved) so
// the NEXT batch's carves stay virgin too ...
for p in ptrs {
    unsafe { (*heap).dealloc(p, layout) };
}
```

That comment is wrong about this allocator's own dispatch order.
`alloc_small_with_virgin` tries the current segment's free list **first** and
returns `is_virgin = false` unconditionally on that path
(`src/alloc_core/alloc_core_small.rs:277-280`); the bump carve that can be
virgin is step 3 (`:291-297`). The 16 freed blocks land on the class free list
(the segment is `small_cur`, so `dec_live_and_maybe_decommit` will not release
it, `alloc_core_small_pool.rs:172`), so criterion iteration 2 onward pops them.

With `sample_size(10)`, 200 ms warm-up and 800 ms measurement at ~31 µs/iter,
that is roughly 30,000 iterations × 16 calls ≈ 500,000 `alloc_zeroed` calls, of
which **16** are virgin. The measurable ON/OFF difference is ~0.003% of the
work — indistinguishable from zero by construction, not by host noise.

Consequently the report's §5 mechanism is diagnosing the wrong thing. It
states: "the virgin scenario's similarly-small separation is the more
informative result, because the Ir numbers prove the software-level skip IS
firing" (`:186-188`). In this harness it is not firing. The eager-page-commit
theory may well be *true* as a general statement, but it is not the reason
*this* bench shows no separation, and it is presented as "a specific, verified
reason" (§5 heading, and repeated verbatim in the `R13_3_…` addendum, in
`OPEN_ITEMS.md` item 25, and in the `CHANGELOG.md` headline).

A correct virgin arm must either (a) never free within the timed region and
accept a bounded batch per criterion sample, or (b) assert virginity per call
via `dbg_payload_virgin_for`, or (c) use `SMALL_ZERO_PASS_CALLS` (already used
as the skip-fired oracle in
`tests/r13_3_magazine_virgin_hit_skips_zero.rs`) to prove the skip count
matches the call count. The project already owns the instrument for (c) and
did not use it.

### P2-5 — R29-3's "net loss" conclusion contradicts R29-3's own numbers

`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md:184-201` (§5) says:

> the reservation-only design would ADD ~196-217K ns of decommit overhead
> while saving only ~21-27K ns … a **net loss**. … the reservation-only floor
> B … is consistently comparable to or MORE expensive than the current
> production cycle A′.

Both saved runs say the opposite. Run 1: `A′ = 2,215,980`, `B = 2,102,388`,
`A′ − B = +113,592 ns` (5.1% cheaper). Run 2: `A′ = 2,190,767`,
`B = 2,154,111`, `A′ − B = +36,656 ns` (1.7% cheaper). Both logs print
`A' − B = reservation-only SAVES` (`_raw_r29_3_decomposition_run1.log:22`,
`run2.log:22`) — the example only prints `COSTS EXTRA` when the sign flips
(`examples/r29_3_decomposition_gate.rs:229-241`), and it never did. B is
cheaper in 2/2 runs.

§5's next sentence concedes this ("The net 'saving' (A′−B) is 37-114K ns =
1.7-5.1% — and even this small figure is within measurement noise of zero"),
so the section contradicts itself inside four lines. The §5 reasoning is also
simply wrong as arithmetic: it compares the decommit cost against (1+2+3) while
ignoring that arm B never pays (1+2+3) *and* that its refault cost measured
~282K ns lower than A′'s fresh-mmap faults.

The verdict (do not open the design) is still defensible on the 1.0–1.3%
figure. The *reason given* is not. And the erroneous version is what got
indexed: `docs/perf/OPEN_ITEMS.md` item 15 ("so the reservation-only design
would be a NET LOSS on Linux") and item 16 ("would be a NET LOSS on Linux
(per-page PTE walk of 1,006 pages > bulk VMA teardown of `munmap`)") both
record it as the measured conclusion.

### P2-6 — R29-3 labels means as medians

Both raw logs and the report print `── Component breakdown (ns/cycle, median
of 200) ──`. The code computes `t0.elapsed().as_nanos() / N` — an arithmetic
**mean** over the whole loop — at
`examples/r29_3_decomposition_gate.rs:100` (A), `:113` (C), `:150-151`
(decommit/refault), `:179` (A′). No median is computed anywhere in the file.

For a kernel-time-dominated measurement on a shared Windows/WSL2 host, that is
the difference between "one scheduler stall inflates the figure" and "it does
not". It also explains run-to-run spread of 1.8× in component (1) (6,776 vs
3,676 ns) — consistent with mean-over-outliers, not with a median. Every
figure in the report, the summary CSV, and `OPEN_ITEMS.md` items 15/16 inherits
the mislabel.

### P2-7 — R29-3's verdict is highly sensitive to an unstated assumption

Arm B and arm A′ both touch **every** payload page of the segment — 1,006 of
them (`examples/r29_3_decomposition_gate.rs:126-128`, `:145-147`, `:174-176`).
That choice maximises (4+5) and therefore minimises the avoidable share, which
is the verdict statistic.

The avoidable term (1+2+3) is fixed per cycle; (4+5) scales roughly linearly
with the touched fraction *f*. At the measured ~1,895 ns/page, the report's own
20% threshold is crossed at roughly `f ≈ 1/50` — about 20 touched pages, i.e.
~80 KiB of a 4 MiB segment. R27-4's victim, whose churn this decomposition
exists to explain, allocates 1024 B in batches of 120 — plausibly far below a
full segment's worth of distinct pages per lifecycle.

The report never states that its verdict assumes full-payload first touch, and
never bounds the sensitivity. That is a materially different omission from the
usual "we measured one shape" caveat, because here the *unmeasured* shapes are
the ones the design was proposed for. Recommendation for the eventual
correction: report the avoidable share as a function of touched pages (a single
extra sweep in the same probe), not as a single point.

### P2-11 / P2-12 — R29-13's "30×" and "32×" comparisons

R29-13's headline (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md:343-346`,
and the `CHANGELOG.md` Round 29 title) reads: "This is 30x the small pool's
proven ~8 MiB/heap retention (R27-3)".

R27-3's ~8 MiB is a **delta**, not a floor.
`docs/perf/R27_3_POOL_RETENTION_GATE.md:18-19` and `:61`: "a **post-teardown
RSS delta of ~+8 MiB per materialised heap** … The cap8−cap4 post-teardown
retention delta". R29-13's 238–241 MiB is an **absolute** retained figure.
Comparing them as a ratio is not a comparison. The small pool's absolute floor
at the shipped cap 4 is ≥16 MiB (its own byte cap: R29-4 measured
`small_pooled` = 4 segments = 16,384 KiB at cap 4,
`_raw_r29_4_reconciliation_run1.log:18-20`) plus the `small_active` segment,
i.e. ~20 MiB — an absolute-to-absolute ratio of ~12×, not 30×.

Separately, `:8-9` states the default headroom is "256 MiB/heap, 32x the small
pool's 16 MiB byte cap". `256 / 16 = 16`. And `:344-346` attributes a
"'32x the small pool's byte cap' framing" to "§1.2 of the source review" —
the source review says no such thing. Its actual text
(`docs/reviews/2026-07-29-oh-acceleration-code-project-review.md:191-196`) is
"retains up to `min(8 cached spans, 256 MiB)` … plus the small pool's
`pool_byte_cap` (16 MiB default) … on the order of **~272 MiB**, of which the
project has measured **16 MiB**" — an additive framing, not a multiple.

Everything else in R29-13 is sound and, in places, exemplary: subprocess-per-
arm isolation is real, `dbg_decay_config` read-back is a genuine hard assert
(`examples/r29_13_large_cache_retention_gate.rs:248-254`), threads are
genuinely long-lived and non-exiting (`:280-311`, `recycle` only after the
final measurement at `:311`), the idle Δ is exactly 0 in all 36 rows (I
verified `rss_post == rss_100ms == rss_1s == rss_2s` in every CSV row), and the
288-vs-272 MiB reconciliation in §2 is honest and checks out
(`301,989,888 / 8 = 36 MiB` span for a 34 MiB request; RSS confirms only the
touched 272 MiB is resident, `281,864 KiB − 3,164 KiB baseline = 272.2 MiB`).

Two smaller items: §4's mechanism says the 256 MiB arm "converges to roughly
SIX retained 36 MiB segments" — the measured reclaim is exactly one object
(275.26 − 241.25 = 34.0 MiB), so **seven** segments are retained (252 MiB of
`used_bytes`, 238 MiB of it touched); and §0's per-heap table lists 275.3
MiB/heap for the 1-thread arm without subtracting the ~3 MiB process baseline
it does effectively subtract at 32 threads, which is why the same fill reads
275.3 and 272.2 in adjacent rows.

### P2-13 — R29-5's ratios are dilution artifacts, and one is mapped onto a metric it never measured

The measured facts are solid: 33 promotions, 60,722 allocation events, every
event exactly 131,072 B, byte-identical across two runs, and the single-bucket
histogram is correctly explained as a structural consequence of pure doubling
(`docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md:152-164`). The report is also
commendably explicit that adding 20,000 never-grown background allocations
enlarges the denominator on purpose (`:76-79`).

The problem is `:45` / `:178-180`: `33 / 4,040 = 0.82%` is described as
"the closest analogue to §5.1's `promotions_triggered / medium_allocations_made`".
It is not. 4,000 of those 4,040 objects draw a ceiling uniform in
`[64 B, 64 KiB)` (`examples/r29_5_promotion_frequency_gate.rs:211-212`,
`SMALL_CEILING = 64 * 1024`) and therefore **cannot** reach the 256 KiB
promotion threshold by construction — they are never medium allocations at all.
On the population that can promote, the rate is `33 / 40 = 82.5%`. The probe
never counts medium-class allocations, so the named §5.1 ratio is simply not
measured.

The honest framing of the same data is: *promotion fires once per object that
crosses 256 KiB — about 83% of such objects — and such objects are ~1% of the
population under an assumed 100:1 small:large mix.* That mix
(`SMALL_POPULATION / LARGE_POPULATION == 100`, asserted at `:77`) is an
assumption, not a measurement, and it is the single parameter the whole
verdict rests on. "Well under 1% by every denominator tried" (`:190-191`) reads
as robustness across independent denominators; all three denominators share the
same assumption.

The NO-VICTIM verdict is probably still right — 4.1 MiB total moved across a
60K-op workload is genuinely small. But it should be stated as conditional on
the population mix, not as a property of the allocator.

One minor internal contradiction: `examples/r29_5_promotion_frequency_gate.rs:96-97`
claims `LARGE_CEILING` is set "so the large population actually exercises
multiple promotion events per object", while the report correctly explains
(`:152-164`) that promotion fires at most once per trajectory.

### R29-4 and R29-10 — clean, with one framing note

R29-4's per-state reconciliation is good work: the identity
`sum(states) + unknown == table.count()` is hard-asserted, `unknown_count = 0`
at every point, and `small_empty_orphan = 0` genuinely refutes the R28 review's
own hypothesis rather than confirming it. Every figure reproduces from
`_raw_r29_4_reconciliation_run1.log`.

**P3-18:** the `committed_bytes` columns are modelled, not observed.
`dbg_segment_state_reconciliation` assigns `seg_bytes = SEGMENT` (4 MiB) to
every non-decommitted Small/Primordial segment
(`src/alloc_core/alloc_core_small_pool.rs:1131`, `:1143`, `:1165`, `:1169`,
`:1175`), so "committed KiB" is exactly `count × 4 MiB` by construction. The
report's "Post-teardown total committed delta: **+8,192 KiB = +8 MiB** — matches
R27-3's ~+8 MiB/heap retention finding exactly" (`:70-71`) is therefore
`+2 segments × 4 MiB` agreeing with an independent RSS measurement — real but
weaker corroboration than the wording implies. Relatedly,
`SegmentStateAccount::committed_bytes`'s doc ("Bytes backed by physical memory
for segments in this state", `:38-39`) overstates what is computed; it is the
reservation size assumed fully committed, which would be wrong under
`small-segment-lazy-commit`.

R29-10's 12.19 Ir/hit is exact and reproduces from both raw logs
(`4,477 − 4,282 = 195`, `/16 = 12.19`). The "54.5% of a magazine hit"
denominator comes from a different report (R23-3's 22.4 Ir/op) and the isolated
hook call carries call overhead the inlined production path does not — both
disclosed. No finding.

---

## 3. Feature-gating correctness under the full matrix

This is where the round's own stated lesson was not learned twice.

### P0-1 — `perf_gate_iai` does not compile under the feature set CI and `npm run check` actually use

**Severity: P0. Confirmed by execution, not inference.**

R29-16 (`7c2c62d`) added four arms gated on
`#[cfg(all(target_os = "linux", feature = "alloc-core", feature = "alloc-decommit",
feature = "virgin-zero-skip", feature = "bench-internals"))]`
(`benches/perf_gate_iai.rs:1152-1158`, `:1168-1174`, `:1195-1201`, `:1226-1232`)
and registered all four **unconditionally** in `library_benchmark_group!`
(`:2906-2909`).

`virgin-zero-skip` is **not** in `production`
(`Cargo.toml:399`). The same file already solves exactly this problem with
`#[cfg(all(target_os = "linux", not(feature = "batch-api")))]` no-op stubs,
under a comment that spells out why they exist: "no-op stubs so
`library_benchmark_group!` resolves when `batch-api` is absent"
(`:2749-2760`, and a second set at `:2764`). Every other arm in the group whose
cfg is not satisfied by `production bench-internals` has such a stub. The four
new arms have none — one definition each, no `not(...)` twin.

Verified in WSL2/Linux:

```
$ cargo check --bench perf_gate_iai --features "production bench-internals"
error[E0433]: cannot find `alloc_zeroed_calloc_virgin_64k_prefix` in `super`
error[E0433]: cannot find `alloc_zeroed_calloc_virgin_64k` in `super`
error[E0433]: cannot find `alloc_zeroed_calloc_recycled_64k_prefix` in `super`
error[E0433]: cannot find `alloc_zeroed_calloc_recycled_64k` in `super`
  note: found an item that was configured out
        the item is gated behind the `virgin-zero-skip` feature
error: could not compile `sefer-alloc` (bench "perf_gate_iai") due to 4 previous errors
```

Blast radius — all three are the *same* feature string:

1. `.github/workflows/perf-gate.yml:122` and `:124`:
   `cargo bench --bench perf_gate_iai --features "production bench-internals"`.
   Triggers: nightly cron `'30 3 * * *'`, `workflow_dispatch`, and
   `pull_request: types: [labeled]` with the `perf` label. The next nightly
   run fails.
2. `scripts/iai.mjs:78`: `const DEFAULT_FEATURES = 'production bench-internals'`.
   So bare `npm run iai` fails.
3. `scripts/check-all.mjs:117-127`: the final step of `npm run check` is
   `node scripts/iai.mjs` with no feature override. **The mandatory pre-push
   gate is red at `HEAD`.**

Why it was missed: R29-16 only ever ran the suite *with* `virgin-zero-skip`
(both its raw logs say `--features "production virgin-zero-skip
bench-internals"`), and on the Windows dev host the whole bench file is
`#![cfg(target_os = "linux")]`, so no local `cargo check`/`clippy` — including
`--all-features --all-targets`, which I confirmed passes — can see the problem.
This is the exact "narrow per-task verification missed the real CI row" pattern
the round's own commit messages (`8e6f9d1`, `894e9e3`) congratulate themselves
for catching twice.

Fix is four lines of the stub pattern already in the file.

### P1-2 — `cargo clippy --features production -- -D warnings` fails at `HEAD`

**Severity: P1. Confirmed: exit 101.**

```
error: struct `SegmentStateAccount` is never constructed
  --> src\alloc_core\alloc_core_small_pool.rs:34:12
error: struct `SegmentStateReconciliation` is never constructed
  --> src\alloc_core\alloc_core_small_pool.rs:53:12
error: method `recompute_total` is never used
  --> src\alloc_core\alloc_core_small_pool.rs:84:8
error: could not compile `sefer-alloc` (lib) due to 3 previous errors
```

Introduced by R29-4 (`e4576d9`; `git log -S SegmentStateAccount` confirms it,
and `git show b7ff9fe:…` has zero occurrences). Root cause is precisely the bug
`8e6f9d1` fixed for the promotion counters, one commit later in the same round:
the **definitions** at `:34`/`:53`/`:84` are ungated, while the only thing that
uses them — the `pub use` at `src/alloc_core/mod.rs:195` and the
`dbg_segment_state_reconciliation` method at `alloc_core_small_pool.rs:1127` —
is `#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]`.
Under plain `production` (has `alloc-decommit`, lacks `bench-internals`) they
compile and are unreachable.

Why nothing catches it: the clippy matrix is `""`, `--features experimental`,
`--all-features`, `--features "hardened medium-classes"`
(`.github/workflows/ci.yml:73-87`) and `""`/`experimental`/`--all-features`
in `scripts/check-all.mjs:54-68`. **Plain `production` — the crate's shipping
composition — has no `-D warnings` row anywhere.** `--all-features` masks it
(`bench-internals` on), and `cargo test --features production` reports the
warnings without failing.

Recommendation: gate the definitions with the same predicate as the re-export
(the fix `8e6f9d1` already models), and add `cargo clippy --all-targets
--features production -- -D warnings` to both `ci.yml`'s clippy job and
`check-all.mjs`. That single missing row is a standing hole, not a one-off.

### Everything else in the gating checks: **clean**

I built each new target against **exactly** its declared `required-features`,
nothing more:

| target | features | result |
|---|---|---|
| `--example r29_3_decomposition_gate` | `alloc-global alloc-xthread alloc-decommit bench-internals` | OK |
| `--example r29_4_segment_state_reconciliation_gate` | same | OK |
| `--example r29_5_promotion_frequency_gate` | `+ medium-classes` | OK |
| `--example r29_13_large_cache_retention_gate` | `alloc-global alloc-decommit bench-internals` | OK |
| `--bench r29_16_virgin_zero_skip_calloc_wallclock` | `alloc-global fastbin` | OK |

`cargo clippy --all-features --all-targets -- -D warnings` → exit 0. The
`8e6f9d1` reachability predicate at `src/alloc_core/alloc_core.rs:411-418` and
`src/alloc_core/mod.rs:178-185` matches `medium_promotion_reachable!`
(`src/registry/heap_core_free.rs:100-106`) verbatim, as claimed.

One residual note: `r29_5_promotion_frequency_gate` compiles and runs under
`--all-features`, but the counter increment site is compiled out there (that is
what `8e6f9d1` established), so the probe would silently print
`promotion_count=0`. Not a defect today — the report documents its feature
set — but a `dbg_promotion_count() > 0` assert in the example would make the
trap self-detecting.

---

## 4. Append-only-correction discipline

### The `OPEN_ITEMS.md` split: **VERIFIED, zero loss**

I did not trust the commit message. `34f3702^:docs/perf/OPEN_ITEMS.md` is 1,581
lines; `34f3702` leaves 426 and adds a 1,528-line archive. A true multiset diff
of `{pre}` vs `{post_open ∪ post_archive}`:

```
LOST (in old, missing from new+archive): 0 line-instances, 0 distinct
GAINED (new/archive only): 374 line-instances, 171 distinct
```

Every one of the 374 gained lines is navigational scaffolding I inspected:
176 blanks, 29 `---` rules, the two files' new preamble sections, 27
`Full history: … § <anchor>` pointers, and 12 new one-line "Recently resolved"
summaries whose full originals are preserved verbatim in the archive (which is
why LOST is 0). **No historical narrative was altered, reworded, or dropped.**

Anchor integrity, checked bidirectionally at `HEAD` (not just at the split
commit, so R29-13's later `L27` addition is included):

```
refs in OPEN_ITEMS.md:      28  (A1 A13 D2..D6 D14 D15 D25 D26 L7..L13 L16..L24 L27)
### anchors in ARCHIVE.md:  28  (identical set)
missing anchors: []   orphan anchors: []
```

Item numbers and tiers survived. Item 15's `[D]`→`[L]` migration (R29-3) is
done correctly: the `[D]` entry is struck through with a forward pointer
(`docs/perf/OPEN_ITEMS.md:172-186`) and re-stated as `[L]` item 16 (`:335-353`)
— relocation with a trail, not deletion. Caveat on my method: a line-multiset
diff cannot detect a line *moved to the wrong item's section*; I spot-read
several archive sections and found them coherent, but that is sampling, not
proof.

### The dated corrections to pre-existing reports: **clean, both of them**

`git diff b7ff9fe..HEAD -- docs/perf/R27_3_POOL_RETENTION_GATE.md
docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` contains **only `+` lines** —
zero deletions, zero modifications.

- `R27_3_POOL_RETENTION_GATE.md` gains a blockquoted `> **Correction note
  (R29-4/task #435, 2026-07-29):**` inserted after `:201`, adjacent to the text
  it corrects rather than orphaned at the end. It explicitly preserves the
  original's standing ("The decomposition table above remains valid as a
  phenomenological RSS-subtraction split; R29-4 upgrades the second tier from
  'inferred' to 'source-verified'"). This is the pattern to keep.
- `R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` gains
  `## 2026-07-29 addendum (R29-16, task #447)` appended at the end, with an
  explicit "**This does NOT overwrite the conclusion above**" clause. Correctly
  marked. Its *content* inherits P1-3 and P1-4 (it restates the 21.4× figure
  and the eager-commit explanation), so it will itself need a correction.
- `docs/CORRECTNESS_OPEN_ITEMS.md`: `+88 / -0`. R29-1's refutation of R28-2's
  build-contention hypothesis is appended, R28-2's original entry untouched.

No finding in this section. This is the one discipline the round executed
without a flaw.

---

## 5. CI / process changes (R29-14)

**Verified independently; the claim holds exactly.**

The gap claim is that `--all-features` enables `numa-aware-mock`
(`Cargo.toml:616`: `numa-aware-mock = ["numa-aware", "numa-shim/mock"]`), which
forces `numa-shim` to compile its `#[cfg(feature = "mock")]` arms and *not*
the real OS integration. Confirmed by resolved-feature inspection, not by
reading the manifest:

```
$ cargo tree --all-features -f "{p} FEATURES=[{f}]" | grep numa-shim
numa-shim v0.1.0 FEATURES=[default,mock,vmem-integration]

$ cargo tree --features "production numa-aware" -f "{p} FEATURES=[{f}]" | grep numa-shim
numa-shim v0.1.0 FEATURES=[default,vmem-integration]
```

So the new step genuinely compiles the non-mock arm that no other per-PR job
compiles, and the weekly `numa-real-kernel` job was indeed the only prior
coverage. `cargo check --features "production numa-aware"` succeeds locally
(3 pre-existing warnings, which are the P1-2 items — `cargo check` has no
`-D warnings`, so the step itself is green).

YAML structure, verified by parser rather than grep:

```
jobs: 30
test-feature-isolation steps: 10
  ... last step: check (--features "production numa-aware")
```

Correctly the final step of the intended job, correct indentation, `if:
${{ !cancelled() }}` matching its siblings, and the trailing blank line before
`test-windows:` preserved. The 34-line diff to `ci.yml` also correctly adds
`bench-internals` to the `production virgin-zero-skip alloc-stats` row (`:343-345`)
so R29-8's newly gated test keeps running there.

One honest limitation of my verification: CI runs this step on
`ubuntu-latest`, which compiles the `mbind(2)` arm; my local run compiled the
`VirtualAllocExNuma` arm. I confirmed the *feature resolution* is right on both,
not that the Linux arm typechecks — that is what the CI step exists to
discover, which is the point.

No finding.

---

## 6. Other findings

### P2-9 — R29-6's immutable-provenance rule is broken by the first report subject to it

`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md:54-60` cites, "per CLAUDE.md's
R29-6 immutable-source-identity rule", the identity

```
d40e8280b433892e17605b9b96c28baaebf852a8f3d70057ba64cd47ac0ec98
```

That string is **63 hex characters**. A SHA-256 is 64. It is not a valid digest
and cannot be matched against anything.

Worse, the stated recipe cannot reproduce it even in principle:
`sha256(git diff -- Cargo.toml src/registry/heap_core_diag.rs; cat
examples/r29_13_large_cache_retention_gate.rs)`, with the report's own
assurance that "this exact command is reproducible against the committed tree
once these files land". Once the files land, `git diff` on a clean tree emits
**nothing**, so the recipe hashes a strictly smaller input and cannot yield the
cited value. The rule offers three source-level options (temp commit SHA, `git
write-tree`, patch hash over a named base) — the second would have worked
here in one command and covers untracked files after `git add -N`.

`34f3702` (the rule) precedes `894e9e3` (the report) in this same round.

### P2-10 — R29-16 cites no source identity at all

`docs/perf/R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md` has no "Base revision
measured" line, no commit SHA, and no immutable identity — its provenance is
`Date: 2026-07-29` and nothing else. Its summary CSV's provenance column holds
the literals `run1` / `run2`
(`R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE_summary.csv:2-9`). Every sibling gate
doc in this round (R29-3 `:9-10`, R29-4 `:32-34`, R29-5 `:19-21`, R29-10 `:17`,
R29-13 `:50-51`) carries at least a base SHA. R29-16 landed after `34f3702`, so
both the new rule and the older established practice apply.

### P2-14 — the Round 29 `CHANGELOG.md` entry misstates what R29-17 did

Two places. The headline: "four more safe `dbg_*` raw-pointer hooks re-gated
`unsafe`+`bench-internals`". The R29-17 bullet: "re-gated
`HeapCore::dbg_directory_bit_for_ptr` `unsafe fn` + `bench-internals`, the
fourth instance of the R25-1 bug class".

`bd1bd3e`'s own message: "Stays a safe fn, no bench-internals gating needed
(consistent with every other guarded read in this hook family)." The diff
confirms it — the only change to the function is the containment guard. The
same bullet then *correctly* describes the guard, so the entry contradicts
itself. Accurate counts: two hooks became `unsafe fn`, one stayed safe and
gained the gate, one gained a guard.

This matters more than a typo: `CHANGELOG.md` is the durable record a future
round's "read the indexes" ritual consults, and the R25-1 rule's whole point is
that hooks get filed under the right remediation.

### P3-15 — the `CHANGELOG.md` R29-1 entry garbles the root cause it praises

It says the windowed delta was one "a long-running proptest session could push
negative across window boundaries even with zero real leaks". There is no
proptest in `tests/r14_4_promotion_free_correctness.rs`. The real mechanism —
correctly stated in `9ef2f85`'s message and in the in-test comment at
`tests/r14_4_promotion_free_correctness.rs:318-332` — is that the promotion
grow *releases* a segment whose matching *reserve* happened before the
snapshot window (heap/TLS init, primordial, or the sibling test sharing this
thread's persistent TLS heap). The captured evidence shows it exactly:
`released_delta=2 > reserved_delta=1`, `reserved before=3`, global
`released_total=2 <= reserved_total=4`
(`docs/_raw_r29_1_repro_captured.log`, all 6 failures identical).

### P3-16 — `CHANGELOG.md` says R29-11 migrated "eight" sections; it migrated seven

`2fd0b39`'s own subject line is "migrate IAI_BASELINE.md's **7** unindexed
honest-rejects", and its body explains R3 was already handled by R29-10's item
17. The CHANGELOG bullet says "eight previously-unindexed".

### P3-17 — R29-13's report documents the pre-fix `required-features`

`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md:371` still says
`required-features = ["alloc-decommit", "bench-internals"]`. The committed
value — corrected in the very same commit, and called out in that commit's
message as a caught E0432 — is `["alloc-global", "alloc-decommit",
"bench-internals"]` (`Cargo.toml:1592-1593`). The report documents the broken
set as if it shipped.

### P3-19 — the R29-9 tripwire's three scope holes

The test is correct today (§1(b)). For the record, so a future round does not
have to rediscover them:

1. `has_bench_internals_cfg` (`tests/dbg_hook_safety_tripwire.rs:264-294`)
   returns `true` for *any* `#[cfg…]` attribute whose text contains the
   substring `"bench-internals"`. A future
   `#[cfg(any(feature = "bench-internals", feature = "alloc-stats"))]` or
   `#[cfg(not(feature = "bench-internals"))]` would be silently classified as
   "gated" and dropped from the at-risk set — a false negative in the one
   direction that matters. Every current gate happens to be `all(...)`, so no
   live instance.
2. Scope is `dbg_*`-prefixed **and** raw-pointer-shaped. Safe `pub fn` hooks
   that mutate allocator metadata from unvalidated *integer* input are outside
   it. I checked the three candidates —
   `AllocCore::dbg_directory_force_clear_bit`
   (`src/alloc_core/alloc_core_core_diag.rs:914`, whose `slot_idx` reaches
   `table.base_at` and `publish_empty`),
   `RemoteFreeRing::dbg_set_cursors` (`src/alloc_core/remote_free_ring.rs:684`,
   which can violate the ring's documented `tail − head <= RING_CAP`
   invariant), and `dbg_directory_set_miss_streak_for_class`
   (`alloc_core_core_diag.rs:1066`) — and all three bottom out in
   bounds-checked array indexing or an early `null` return, so the worst case
   is a panic, not UB. No live instance, but no structural guard either.
3. It cannot see hooks that corrupt allocator state with **no** caller input.
   That is not hypothetical: see P2-8.

### P3-20 — R29-11's index tripwire is very weak

`honest_reject_sections_are_indexed`
(`tests/no_stale_doc_references.rs:512-...`) requires only that the bare
heading token (`X4`, `G1`, `T10`, `R1`, `R5-R2b`) appear *anywhere* in
`OPEN_ITEMS.md` as a non-alphanumeric-bounded substring. Any incidental prose
mention satisfies it; and since `collect_md_files` walks all of `docs/perf`
including `OPEN_ITEMS.md` itself, a heading added *to the index* trivially
satisfies its own requirement. It would not catch "the token is mentioned but
the item was never actually created". Worth tightening to "the token appears in
a `[A]`/`[D]`/`[L]` item line", but it is strictly better than the nothing that
preceded it.

### P3-21 — R29-1 narrowed the leak assertion more than the round admits

R29-1's diagnosis and fix are genuinely good — a reproduced 6/2000, a correct
root cause, a sound replacement, 0/2000 after, and both logs committed. But the
replacement global invariant
`stats_after_free.segments_released_total <= stats_after_free.segments_reserved_total`
(`tests/r14_4_promotion_free_correctness.rs:355-356`) is close to
unfalsifiable in normal operation — `released` runs far below `reserved`
always. All real leak-detection strength now lives in the per-base proof, which
is `#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]`. Under
`hardened medium-classes` (a real CI row, and the combination R28-2's own
cfg-gap involved), the test now asserts monotonicity and an inequality that
cannot fail — i.e. no leak property at all. R28-2's stated goal ("prove no
leak, not just no double-release") is met only under the gated combination.
That is a defensible trade, but `CHANGELOG.md`'s framing ("Replaced with a
global cumulative check … 0/2000 clean after the fix") does not surface it.

### Verified clean, no finding

- R29-2's README work: the recipe's API surface exists exactly as written
  (`SmallSegmentPoolConfig::new`/`pool_segments`/`pool_byte_cap` are `const fn`
  at `small_segment_pool_config.rs:129/150/163`,
  `LargeCacheConfig::pool` at `large_cache_config.rs:353`,
  `SeferAlloc::with_config` at `sefer_alloc.rs:260`, export at `src/lib.rs:338`),
  `src/lib.rs` does not `include_str!` the README so the fence is not a
  doctest, the benefit is stated narrowly with its scope, the cost is given
  equal prominence, and the paired-knob trap cites its own CI guard. The
  `README:74` "throughput-first defaults" overclaim is correctly narrowed to
  the large-cache policy.
- R29-15's three corrections are accurate. The `tcache.rs` module doc now
  correctly separates "the block's own payload is never touched" (still true)
  from "no dependent load on the hit path" (false since RAD-5), and the
  `numa-aware` × `small-segment-lazy-commit` no-op trap matches the source
  (the `numa-aware` arm at `alloc_core_small.rs:~1884-1907` calls
  `numa::reserve_aligned_on_node` with no lazy-commit participation).
- R29-12's `docs/FEATURE_PROMOTION_STATUS.md` is well-scoped, states plainly
  that it makes no decision and cites no new number, and its quoted
  `production` composition matches `Cargo.toml:399` exactly.
- The README unsafe inventory is self-consistent:
  `readme_unsafe_inventory_counts_match_reality` passes at `HEAD`.
- All raw-log citations resolve to committed files; no report cites a log that
  is absent.

---

## Summary table

| # | Sev | Area | Finding |
|---|---|---|---|
| P0-1 | **P0** | gating / CI | `perf_gate_iai` fails to compile under `production bench-internals` (4 × E0433) — breaks `perf-gate.yml`, `npm run iai`, and `npm run check` |
| P1-2 | **P1** | gating / lint | `clippy --features production -- -D warnings` exits 101 on 3 R29-4 dead-code items; no CI or `check-all` row covers `production` clippy |
| P1-3 | **P1** | measurement | R29-16's 21.4× Ir ratio is a Valgrind memset artifact (1.001 Ir/byte); the report's own sanity check is off by 64× |
| P1-4 | **P1** | measurement | R29-16's wall-clock "virgin" arm is recycled from iteration 2 of ~30,000; §5's stated cause of the null result is wrong |
| P2-5 | P2 | measurement | R29-3 §5 says "net loss"; both runs show a 1.7–5.1% saving — error propagated into `OPEN_ITEMS.md` items 15 & 16 |
| P2-6 | P2 | measurement | R29-3 labels arithmetic means as "median of 200" throughout |
| P2-7 | P2 | measurement | R29-3's verdict assumes full-payload first touch; at ~1/50 touched it crosses its own 20% threshold. Unstated, unbounded |
| P2-8 | P2 | soundness | `dbg_decomp_full_cycle` (safe `pub fn`) leaves `small_cur` dangling after release; next small alloc on that heap is UB |
| P2-9 | P2 | provenance | R29-13's cited "sha256" is 63 chars and its stated recipe cannot reproduce it — R29-6's own new rule, first application |
| P2-10 | P2 | provenance | R29-16 cites no base SHA and no immutable identity at all |
| P2-11 | P2 | measurement | R29-13's "30× the small pool" compares an absolute floor to R27-3's cap8−cap4 delta; absolute ratio is ~12× |
| P2-12 | P2 | measurement | R29-13's "32× the 16 MiB byte cap" is 16×, and the "32×" framing is misattributed to the source review |
| P2-13 | P2 | measurement | R29-5's `33/4,040 = 0.82%` is mapped onto §5.1's `promotions/medium_allocations`, which it does not measure; real rate on promotable objects is 82.5% |
| P2-14 | P2 | docs | `CHANGELOG.md` says R29-17 made the hook `unsafe`+`bench-internals`; it did neither. Headline "four hooks" is two |
| P3-15 | P3 | docs | `CHANGELOG.md`'s R29-1 root cause ("long-running proptest session") is not the mechanism the commit and logs establish |
| P3-16 | P3 | docs | `CHANGELOG.md` says R29-11 migrated eight sections; it migrated seven |
| P3-17 | P3 | docs | R29-13's report documents the pre-fix `required-features` (missing `alloc-global`) |
| P3-18 | P3 | measurement | R29-4's `committed_bytes` is `count × SEGMENT` by construction, not observed commit; the field doc overstates it |
| P3-19 | P3 | soundness | Three scope holes in the R29-9 tripwire (`any`/`not` cfg misclassification; integer-keyed hooks; zero-input hooks) — no live instance except P2-8 |
| P3-20 | P3 | process | R29-11's index tripwire only needs the bare token to appear anywhere, including in the index's own headings |
| P3-21 | P3 | tests | R29-1's replacement invariant is near-unfalsifiable; leak coverage now depends entirely on the `alloc-decommit + alloc-xthread` gated block |

Verified clean with no finding: the R29-6 archive split (0 lines lost, 28/28
anchors bidirectional), the two append-only report corrections and the
`CORRECTNESS_OPEN_ITEMS.md` append (`+88/-0`), R29-14's CI step (feature
resolution and YAML both confirmed), the four re-gated/guarded hooks
themselves, the tripwire's current accuracy (independently re-derived, 29/29
exact match), R29-13's four new delegations, R29-2's README recipe, R29-15's
three corrections, R29-12's survey, and every cited number's reproduction from
its committed raw log.

---

## Recommended Round 30 opening

**Before any new work** (both are hard breaks at `HEAD`):

1. Add the four `#[cfg(all(target_os = "linux", not(feature =
   "virgin-zero-skip")))]` no-op stubs to `benches/perf_gate_iai.rs`, following
   the `batch-api` pattern at `:2749`. Re-run
   `cargo check --bench perf_gate_iai --features "production bench-internals"`
   on Linux to confirm.
2. Gate `SegmentStateAccount` / `SegmentStateReconciliation` /
   `recompute_total` with the same predicate as their re-export, and add
   `cargo clippy --all-targets --features production -- -D warnings` to both
   `ci.yml`'s clippy job and `scripts/check-all.mjs`. That missing row is why
   both P1-2 and (indirectly) P0-1 survived review.

**Then, dated append-only corrections** — none of these needs a re-measurement:

3. `R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md`: the Ir magnitude is a Valgrind
   artifact (P1-3) and the wall-clock virgin arm is not virgin (P1-4). Add a
   standing "Callgrind inflates `mem*` primitives" caveat to
   `IAI_BASELINE.md`. Correct the `R13_3_…` addendum and `OPEN_ITEMS.md` item
   25 to match. Re-run the wall-clock arm only after fixing the harness,
   ideally with a `SMALL_ZERO_PASS_CALLS` oracle asserting the skip count.
4. `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` §5 and `OPEN_ITEMS.md`
   items 15/16: replace "net loss" with the measured +1.7–5.1% saving (P2-5),
   relabel means as means (P2-6), and state the full-payload-touch assumption
   with a sensitivity bound (P2-7).
5. `R29_13_LARGE_CACHE_RETENTION_GATE.md`: fix 30×→~12× absolute-to-absolute
   and 32×→16×, drop the misattributed quotation, correct "six segments"→seven,
   fix the `required-features` line, and either recompute a valid tree hash or
   add the exemption note the rule allows (P2-9, P2-11, P2-12, P3-17).
6. `R29_5_PROMOTION_FREQUENCY_GATE.md`: restate the ratio honestly (82.5% of
   promotable objects; ~1% of an *assumed* population) and stop mapping it onto
   §5.1's unmeasured metric (P2-13).
7. `CHANGELOG.md` Round 29 entry: fix the R29-17 description and hook count,
   the R29-1 root cause, and the R29-11 count (P2-14, P3-15, P3-16).

**Then the substantive follow-ups:**

8. Decide R29-3's `dbg_decomp_*` `small_cur` hazard (P2-8) and, in the same
   task, widen `tests/dbg_hook_safety_tripwire.rs` from "raw-pointer-shaped" to
   "leaves allocator state invalid", and harden `has_bench_internals_cfg`
   against `any(...)`/`not(...)` (P3-19). The tripwire earned its keep this
   round; its charter is the part that is too narrow.
9. Restore a non-vacuous leak assertion in
   `tests/r14_4_promotion_free_correctness.rs` for feature sets without
   `alloc-decommit + alloc-xthread` (P3-21).

**Process observation.** Round 29's zero-trust reviews caught three real
compile/lint breaks in flight and said so, prominently, in three commit
messages. They also shipped two more of the same kind. The difference between
the caught and the uncaught ones is not diligence — it is that the caught ones
were reachable from a command the reviewer already had in hand on Windows, and
the uncaught ones required either a Linux `cargo check` (P0-1) or a
`production` clippy row that does not exist (P1-2). The generalisable fix is
not "review harder": it is to make `npm run check` cover plain-`production`
clippy, and to accept that any change to `benches/perf_gate_iai.rs` is
unverifiable on this dev host without a WSL `cargo check` — which takes about
four minutes and would have caught P0-1 outright.
