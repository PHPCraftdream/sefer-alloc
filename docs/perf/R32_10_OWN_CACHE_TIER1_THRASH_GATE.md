# R32-10 (task #501) — `OWN_CACHE_SIZE` Tier-1 cache: the missing hit/miss counter, the missing Large-heavy workload, and the resulting 4→16 bump

Date: 2026-08-02.

landing_commit: 5289c661877462f3caf6c4e136ad3c163f6fe15b

## 0. What this is

This task closes finding **F2** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` ("`OWN_CACHE_SIZE = 4`:
the free path's Tier-1 ownership cache is a 4-entry direct-mapped array that
a Large-heavy workload must thrash by construction") and answers OPEN_ITEMS
`[A]` item 1's own last-open clause, verbatim: *"Separately, Tier-2-hash-
probe-heavy workloads might show `contains_base` > 8.8% (open, not a proven
floor)."*

**What shipped:**

1. A process-wide, `bench-internals`-gated Tier-1 hit/miss path-activation
   oracle — `CONTAINS_BASE_TIER1_HITS`/`CONTAINS_BASE_TIER1_MISSES`
   (`src/alloc_core/alloc_core.rs`), incremented inside
   `SegmentTable::contains_base` (`src/alloc_core/segment_table.rs`), with
   accessors at both the `AllocCore` level
   (`src/alloc_core/alloc_core_core_diag.rs`) and the `HeapCore` level
   (`src/registry/heap_core_diag.rs`), plus a reset hook. This is the
   instrument neither R22-17 nor R23-3 ever built — R23-3 §1.3/§6.2
   explicitly says a benchmark cannot *predict* which OS-assigned addresses
   collide; this counter *observes* the resulting hit rate after the fact
   instead, sidestepping the prediction problem entirely.
2. `examples/r32_10_own_cache_tier1_thrash_gate.rs` — the Large-heavy
   workload the survey asked for: repeated in-place `realloc` (same size, no
   move, no free) rotating across `K` concurrently-live Large objects,
   sweeping `K` over `{4, 8, 16, 24, 32, 48, 64}` under subprocess-per-arm
   isolation. See §2 below for why this specific shape (not free+realloc,
   which two earlier design attempts in this same task proved structurally
   incapable of showing any effect — kept in the file's own module doc as a
   documented pitfall, not scrubbed).
3. `tests/segment_table_contains_base_tier1_counters.rs` — dedicated
   coverage for the new counter pair (same-segment-repeated-free hit
   confirmation; a negative test proving the pre-existing
   `dbg_hash_contains_only` bypass hook does NOT move the new counters).
4. `src/alloc_core/segment_table.rs`: `OWN_CACHE_SIZE` raised **4 → 16**,
   plus a `const _: () = assert!(...)` compile-time power-of-two pin (the
   masking arithmetic in `cache_index` requires it; there was no such pin
   before this task).
5. `scripts/r32_10_own_cache_tier1_summary.mjs` — the ONE checked script
   deriving `R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv` from the raw
   logs, asserting every headline claim in this report before writing the
   CSV (CLAUDE.md's "assert the arithmetic" rule).

**Feature-composition note.** `production = [alloc-global, alloc-xthread,
alloc-decommit, fastbin, alloc-segment-directory, primordial-lazy-commit,
class-aware-dirty]` (`Cargo.toml`) — `alloc-xthread` is what makes
`dealloc_routing`'s `contains_base` call the always-on ownership check on
every free, and `alloc-decommit` is what makes every Large free go through
`unregister` (see §2). `OWN_CACHE_SIZE`'s cost/benefit therefore applies to
the plain `production` default, not an opt-in feature — this IS a
`perf(runtime)`-shaped change (see §6).

## 1. The counter design

`SegmentTable::contains_base` (`src/alloc_core/segment_table.rs`) is the
production ownership-check entry point every own-thread free and every
`realloc` in-place-check call goes through:

```rust
pub(crate) fn contains_base(&mut self, base: *mut u8) -> bool {
    let idx = Self::cache_index(base);
    if self.own_cache[idx] == base && !base.is_null() {
        // R32-10: Tier-1 HIT.
        #[cfg(feature = "bench-internals")]
        CONTAINS_BASE_TIER1_HITS.fetch_add(1, Ordering::Relaxed);
        return true;
    }
    // R32-10: fell through to Tier-2 -- count the ROUTING decision before
    // running the probe (regardless of whether the probe itself then finds
    // `base`).
    #[cfg(feature = "bench-internals")]
    CONTAINS_BASE_TIER1_MISSES.fetch_add(1, Ordering::Relaxed);
    if self.hash_contains(base) { self.own_cache[idx] = base; true } else { false }
}
```

Design choices, matching this project's established `dbg_*` counter
convention (`DECOMMIT_CALLS`, `MAYBE_DECAY_GUARD_PASSED` from task #499 —
same shape):

- **`bench-internals`-gated, Relaxed, process-wide `AtomicU64` pair.** A
  plain `production` build never touches these statics — zero cost,
  zero behavior change outside a measurement build.
- **Hits + misses, not hits + total.** Mirrors `MAYBE_DECAY_GUARD_PASSED`'s
  own precedent of a single clean counter per outcome rather than a
  derived-at-read-time total.
- **A reset hook** (`dbg_reset_contains_base_tier1_counters`), mirroring
  `dbg_reset_hash_remove_max_scan_steps`'s established pattern, so a
  measurement window can start clean instead of accumulating across a
  process's whole lifetime.
- **Counts ROUTING, not membership.** The miss counter increments whenever
  the call fell through to Tier-2, regardless of whether that Tier-2 probe
  then finds `base` or not — this is deliberately the "which tier did the
  work" question, not "is `base` registered".
- **Does NOT touch `contains_base_ro`** (the `&self` read-only sibling used
  only by test-only `dbg_*` accessors and census code) — only the
  production `&mut self` `contains_base` that `dealloc_routing`/`realloc`
  actually call.

Coverage (`tests/segment_table_contains_base_tier1_counters.rs`, 2 tests,
both green):

1. `repeated_same_segment_frees_are_observed_as_tier1_hits` — N deallocs of
   blocks in the SAME (primordial) segment produce at most 1 cold-start miss
   then all hits; asserts `hits_delta + misses_delta == N` (sanity) and
   `hits_delta >= N-1`.
2. `hash_contains_only_bypass_hook_does_not_move_the_new_counters` — the
   pre-existing R23-3 `dbg_hash_contains_only` bypass hook (calls
   `hash_contains` directly, skipping Tier-1) must NOT move the new
   counters — proves they are wired specifically inside `contains_base`'s
   own body, not inside `hash_contains` itself (a wiring bug that swapped
   which function increments would otherwise be silently invisible, since
   both functions touch the same hash table).

## 2. The workload: two false starts, then the correct shape

**The task's own framing was right that this is the hard part.** Two
increasingly subtle false starts, both caught by this project's own
path-activation-oracle discipline (`R30_8`/`oracle2` in the harness code)
BEFORE any wrong number was published — the harness itself refused to trust
its own first two designs.

### 2.1 False start 1 — free+realloc rotation, K=64 fixed

First design: hold K=64 Large objects live, each round free-then-reallocate
every one of them, round-robin. Measured **exactly 50.00% Tier-1 hit rate**,
bit-identical (`tier1_hits == tier1_misses == 32768`) at BOTH
`OWN_CACHE_SIZE=4` AND `=16` — a suspiciously exact number that turned out to
be a structural artifact, not "no effect": under `alloc-xthread + fastbin`
(both in `production`), one `HeapCore::dealloc` of a Large object drives
**2** `contains_base` calls back-to-back on the same base with nothing else
running in between — `dealloc_routing`'s own-thread check, then (because a
Large object isn't `fastbin`-magazine-eligible)
`dealloc_own_thread_with_base`'s "Large / non-small / non-fastbin: delegate
to core" fallthrough calls `AllocCore::dealloc`, which RE-DERIVES `base` and
RE-RUNS `contains_base` from scratch (the same F6-shaped redundancy task
#494 fixed for `realloc`'s move leg, left un-fixed here — out of THIS task's
scope, noted below as a follow-up candidate). The second call always hits
(the first call just filled the cache slot), so every dealloc contributes
exactly one guaranteed hit regardless of cache behavior:
`hit_rate = 0.5 + call1_hit_count / (2*N)`.

### 2.2 False start 2 — K-sweep still shows exactly 50.00% at EVERY K, including K ≤ cache size

Sweeping `K` across `{4, 8, 16, 24, 32, 48, 64}` (straddling every candidate
`OWN_CACHE_SIZE`) with the SAME free+realloc shape still measured **exactly
50.00% at every single K, including K=4** — well within even the OLD 4-entry
cache. This ruled out "K just isn't small enough yet" and pointed to a
DEEPER structural reason: `AllocCore::dealloc`'s Large branch calls
`self.table.unregister(base)` UNCONDITIONALLY on every Large free (all three
branches in `alloc_core.rs` — cache-admitted, admission-rejected, and
`alloc-decommit`-off — call it), and `unregister` itself calls
`own_cache_clear(base)`, evicting the base from `own_cache` at the END of
the very dealloc call that just warmed it. A base's cache slot can therefore
NEVER survive from one free to the next visit of that base under a
free-then-realloc rotation — `own_cache` for a repeatedly-FREED Large object
is self-defeating BY CONSTRUCTION, independent of `OWN_CACHE_SIZE`. Raising
the cache size cannot help this workload shape at all — not because the
cache is too small, but because nothing ever stays IN the cache across two
touches of the same base.

### 2.3 The correct shape: repeated in-place `realloc`, never freed

`HeapCore::realloc`'s in-place success path (`try_realloc_inplace_known_base`
returning `Some`, OPT-G Large-grow-in-span with `new_size == old_size`) calls
`contains_base` **exactly once** per call and does **not** unregister the
segment — the object stays live and its base stays a candidate for a warm
cache hit on the object's next visit, `K-1` other objects later. This is the
workload F2's own trigger condition actually describes ("N concurrently-live
Large objects", not N objects destroyed and rebuilt each round).

**Layer.** `HeapCore::realloc` is the entry point `SeferAlloc`'s
`#[global_allocator]` face calls (via `HeapRegistry::claim`), matching the
R31-0 entry-point-honesty rule — not a bare-`AllocCore` shortcut.

**Path-activation oracle #1** (mechanism-reachability): `dbg_table_count()
>= K` at the floor, PLUS every timed-region `realloc` call asserted
non-null and `p_out == p_in` — a structural proof the call actually took the
in-place path (`try_realloc_inplace_known_base`'s own doc: "always returns
the SAME pointer on success, never moves the block"; a moved pointer would
mean the workload silently stopped exercising the intended mechanism).

**Path-activation oracle #2** (the actual claim): `tier1_hits + tier1_misses
== ROTATING_ROUNDS * K * EXPECTED_CALLS_PER_REALLOC` (with
`EXPECTED_CALLS_PER_REALLOC = 1`, verified empirically and now asserted, not
assumed).

Both oracles passed on 100% of cells across every measurement in this
report (35/35 cells for the initial 512-round/5-rep sweep at each cache
size; 49/49 cells for the final 8192-round/7-rep before/after pair — see
§4).

## 3. Design decisions this workload makes, stated explicitly

- **In-place `realloc`, same size** — guarantees the OPT-G Large-grow-in-span
  fast path every call (`new_eff >= old_eff` trivially holds for
  `new_eff == old_eff`, and `end <= span_usable` was already true by
  construction of the original allocation).
- **`K_VALUES = {4, 8, 16, 24, 32, 48, 64}`** — straddles every candidate
  `OWN_CACHE_SIZE` (old 4, shipped 16, considered-but-declined 32): values at
  and below 4, at and around 16, at and around 32, and one well past all of
  them (64) as an "always thrashes regardless of cache size" confirmation
  arm.
- **`ROTATING_ROUNDS = 8192`, `REPETITIONS = 7`** — raised from an initial
  `512`/`5` (§4.1 below) after that first pass produced a clean hit-rate
  signal but a noisy `ns_per_op` signal on this Windows single-shot
  `Instant::now` harness; the larger round count averages out more
  per-process timing noise for the latency axis specifically. The hit-rate
  axis was unaffected by this change (same result at both round counts,
  confirmed below).
- **No runtime CONFIG axis** — every arm uses `HeapRegistry::claim()`'s
  default config (CLAUDE.md's R26-4 rule, N/A branch, matching R32-9's own
  posture). `config_conflicts_delta` still emitted and asserted `== 0`.
- **Subprocess-per-arm isolation** — matters doubly here because
  `CONTAINS_BASE_TIER1_HITS`/`_MISSES` are PROCESS-WIDE statics, not
  per-heap fields; a fresh process's empty counters make cross-arm bleed
  structurally impossible, which is what makes reading them a valid
  per-(K, repetition)-cell measurement at all.

## 4. Measured results

### 4.1 Headline: before (`OWN_CACHE_SIZE=4`) vs after (`=16`), same workload, 8192 rounds × 7 reps

Derived by `scripts/r32_10_own_cache_tier1_summary.mjs` from
`docs/perf/_raw_r32_10_own_cache4_before.log` (before) and
`docs/perf/_raw_r32_10_own_cache16_after.log` (after) — every headline claim
below is an in-script `assert` the derive script itself enforces, not a
hand-transcribed number.

| K | before (cache=4) hit rate | before ns/op | after (cache=16) hit rate | after ns/op |
|---:|---:|---:|---:|---:|
| 4  | **0.00%** | 24.0 | **99.99%** | 26.1 |
| 8  | **0.00%** | 27.3 | **99.99%** | 26.4 |
| 16 | 0.00% | 27.7 | 0.00% | 28.9 |
| 24 | 0.00% | 28.7 | 0.00% | 28.4 |
| 32 | 0.00% | 27.8 | 0.00% | 28.3 |
| 48 | 0.00% | 25.7 | 0.00% | 28.2 |
| 64 | 0.00% | 26.6 | 0.00% | 26.1 |

**Path-activation oracles: 49/49 cells PASS both oracle #1 and #2**, in
BOTH the before and after runs (7 K values × 7 repetitions = 49 cells each).
`config_conflicts_delta == 0` in every cell.

**Headline 1 — before (`OWN_CACHE_SIZE=4`): EVERY tested K thrashes
completely, 0.00%, including K=4.** Confirms the survey's own point
empirically, and sharpens it: the survey's text says "even N == 4 only
works if the OS happened to hand out 4 bases whose bits 22-23 differ" — on
this measured run, it did NOT; even the SMALLEST tested rotation (4 live
objects into a 4-slot direct-mapped cache) collided completely.

**Headline 2 — after (`OWN_CACHE_SIZE=16`): K=4 and K=8 (both ≤ new cache
size) jump to 99.99% hit rate.** A dramatic, reproducible win — from
complete thrashing to near-total hits, for exactly the K range the new
cache size can hold.

**Headline 3 — after (`OWN_CACHE_SIZE=16`): K ≥ 16 STILL thrashes
completely (0.00%).** Even K == cache size (16 objects into 16 slots) does
not achieve a meaningfully nonzero hit rate at this measurement's address
layout — the direct-mapped (non-associative) design means K == cache size
does not guarantee K distinct buckets; a collision at K == cache size is
enough to erase most of the potential benefit for that K. This is NOT a bug
in the harness — the survey's own text already named this precisely: "even
N == 4 only works if the OS happened to hand out 4 bases whose bits 22-23
differ" — the same caveat applies to any cache size against K == that size.
The safe zone this measurement demonstrates is K comfortably BELOW
`OWN_CACHE_SIZE`, not K up to it.

**Headline 4 — the latency signal is an HONEST NULL.** Both before and
after arms sit in the same ~24-29 ns/op band at every K, including K=4/K=8
where the hit rate delta is enormous (0% → 99.99%). This is NOT a
contradiction: OPEN_ITEMS item 1's own component pricing puts a Tier-1 hit
at ~8.2 Ir/call and a Tier-2 miss at ~12.0 Ir/call (R22-17/R23-3) — a ~4 Ir
difference, on the order of 1-2 ns on typical hardware, small relative to
the whole `realloc` call's cost (header reads, size-class arithmetic,
`SegmentHeader::set_large_size_at`, etc.) and well within this harness's own
run-to-run noise band (visible even at K=16, where the hit rate did NOT
change between arms yet ns/op still varied 27.7→28.9). **No latency win is
claimed by this report.**

### 4.2 Supplementary: the 32-slot probe (5 reps, 512 rounds — exploratory, informed the ship decision)

An earlier, less-rigorous pass (`docs/perf/_raw_r32_10_own_cache32_exploratory.log`,
5 repetitions, 512 rounds — before the round count was raised for the final
before/after pair) measured `OWN_CACHE_SIZE=32`:

| K | hit rate (median of 5) |
|---:|---:|
| 4  | 99.80% |
| 8  | 99.80% |
| 16 | 99.80% |
| 24 | 33.27% |
| 32 | 0.00% |
| 48 | 0.00% |
| 64 | 0.00% |

At 32 slots, K=16 ALSO reaches ~99.8% (unlike at 16 slots, where K=16
thrashes) — a real additional benefit over 16 slots for K in the 9-16
range specifically. This did not change the shipped decision (see §6): the
marginal K range 32 buys over 16 (roughly 9-16 concurrently-hot Large
objects) is a narrower target than the K≤8 range both sizes already cover
well, and 32 slots costs twice the (already negligible) per-`SegmentTable`
memory of 16 for that narrower win. Recorded honestly as supplementary
evidence, not re-run to the same 8192-round/7-rep rigor as the shipped
before/after pair.

## 5. Correctness

- `OWN_CACHE_SIZE` stays a power of two (a NEW compile-time
  `const _: () = assert!(OWN_CACHE_SIZE.is_power_of_two(), ...)` pin —
  `cache_index`'s masking arithmetic requires it; there was no such pin
  before this task, so a future accidental non-power-of-two value would now
  be a compile error instead of silent wraparound-masking corruption).
- Full `cargo test --features production` run: **green, 0 failures**
  (confirmed on this task's tree; one pre-existing doc-hygiene test,
  `architecture_test_file_count_matches_reality`, needed
  `docs/ARCHITECTURE.md`'s test-file count updated 232→233 for this task's
  one new test file — done in this same commit).
- `cargo clippy` clean across all three CI feature-matrix entries (`""`,
  `--features experimental`, `--all-features`) plus
  `--features "production bench-internals"`.
- `cargo fmt --check` clean.
- The counter change itself (`segment_table.rs::contains_base`) is a
  `bench-internals`-gated addition with no control-flow change outside that
  feature — the `#[cfg]`-gated `fetch_add` calls sit on lines that already
  ran unconditionally (the existing `if`/`return` structure is unchanged).
- `OWN_CACHE_SIZE`'s bump only changes `SegmentTable`'s per-heap struct size
  (`own_cache: [*mut u8; OWN_CACHE_SIZE]`, 4→16 entries = 32→128 bytes on a
  64-bit target, +96 bytes/heap) and the masking constant `cache_index`
  uses — no other structural coupling, matching the survey's own "cost:
  96-224 extra bytes per SegmentTable/heap — negligible" estimate exactly
  (96 bytes at 16, would be 224 at 32).

### 5.1 The standing ±10 raw-Ir churn kill gate — NOT run this task (no Linux/Valgrind on this dev host)

Same platform constraint task #500 (R32-9) documented and this task
inherited: `iai-callgrind` requires Valgrind, Linux-only; this Windows dev
environment has neither. `cargo build --bench perf_gate_iai --features
"production bench-internals"` compiles clean (confirmed), so the gate is
NOT broken by this change, but the actual ±10 raw-Ir numbers for
`small_churn_16b`/`churn_256b`/`aligned_churn_640b_a128`/
`cold_alloc_free_256x16b`/`recycle_alloc_free_256x16b` were not obtained.
**Argued (not measured) that they should stay flat**: this task's only
non-test-only production-path change is `OWN_CACHE_SIZE`'s value (a data
size, not new instructions on the hot path — `cache_index`'s masking
arithmetic is `(base >> SEGMENT_SHIFT) & (OWN_CACHE_SIZE - 1)`, identical
instruction shape for 4 vs 16 since both masks are compile-time constants)
plus the `bench-internals`-gated counter increments (compiled out entirely
in a plain `production` build, so zero effect on the kill-gate's own
feature set, which does not include `bench-internals`). A future task
running on Linux CI (or a Linux dev box) should confirm this argument with
real numbers — flagged honestly as unconfirmed, not glossed over, per this
backlog's "measurement-first, honest-null-is-fine" posture.

### 5.2 CORRECTED 2026-08-02 — the kill gate WAS measurable (WSL) and does NOT stay flat in the raw ±10-Ir sense; decomposed into two components, one benign

This section is appended, not a rewrite — §5.1 above stays exactly as
originally published (per this project's append-only correction
convention). §5.1's environmental excuse ("no Linux/Valgrind on this dev
host") was **incorrect** — WSL (Ubuntu 24.04, with Valgrind installed) was
available on this same machine the whole time; it was not checked before
publishing §5.1's "argued, not measured" framing. Found during this task's
own zero-trust review, immediately re-measured, and this section reports
the actual numbers honestly, including that the raw kill-gate DOES exceed
±10 Ir — decomposed into why, and why the decomposition still supports
shipping.

**Method.** Three arms, same 5 standing churn benches, `--features
"production bench-internals"` (required to build `perf_gate_iai` at all —
`bench-internals` cannot be excluded from this specific gate, unlike a
plain user's `production`-only build):

1. **`base`** — commit `ce3f44da0a60d0f5c71b0c8bb26c1992726dccc4` (this
   task's own base commit, `OWN_CACHE_SIZE = 4`, no Tier-1 counter — the
   counter did not exist yet).
2. **`isolate`** — this task's landing commit
   (`5289c661877462f3caf6c4e136ad3c163f6fe15b`), with the two
   `CONTAINS_BASE_TIER1_HITS`/`_MISSES` `fetch_add` calls in
   `segment_table.rs::contains_base` TEMPORARILY commented out as a scratch,
   uncommitted edit — isolates `OWN_CACHE_SIZE`'s array-size change ALONE,
   with the counter held constant (absent) on both sides of this specific
   comparison.
3. **`head`** — the landing commit as actually shipped (`OWN_CACHE_SIZE =
   16` **and** the counter enabled) — the real committed state.

Each arm built in its own isolated `git worktree` (`base`) or a scratch
edit reverted before committing anything (`isolate` — no commit exists for
this arm; see the provenance note in §8 below), all run with `sccache`
disabled (`CARGO_BUILD_RUSTC_WRAPPER=""`) to avoid an unrelated
Windows/WSL toolchain-wrapper conflict on this dev box. Raw logs (truncated
to the 5 target benches + the summary line, full compile output cut per
this project's truncation-marker convention):
`docs/perf/_raw_r32_10_killgate_cache4_nocounter.log` (base),
`docs/perf/_raw_r32_10_killgate_cache16_nocounter.log` (isolate),
`docs/perf/_raw_r32_10_killgate_cache16_withcounter.log` (head). Derived +
self-asserting summary: `docs/perf/R32_10_KILLGATE_ADDENDUM_summary.csv`,
via `scripts/r32_10_killgate_addendum_summary.mjs` (every claim below is an
in-script `assert`, not hand-transcribed).

**Results (raw `Instructions`):**

| bench | base (cache=4) | isolate (cache=16, no counter) | head (cache=16, shipped) | Δ cache-size-alone | Δ counter-alone | Δ total (base→head) |
|---|---:|---:|---:|---:|---:|---:|
| small_churn_16b | 8,810 | 8,846 | 9,037 | +36 | +191 | **+227** |
| aligned_churn_640b_a128 | 8,746 | 8,782 | 8,973 | +36 | +191 | **+227** |
| churn_256b | 8,810 | 8,846 | 9,037 | +36 | +191 | **+227** |
| cold_alloc_free_256x16b | 50,968 | 51,004 | 51,547 | +36 | +543 | **+579** |
| recycle_alloc_free_256x16b | 99,185 | 99,228 | 100,763 | +43 | +1,535 | **+1,578** |

**Headline 1 — the raw base→head delta is well past ±10 Ir on every bench
(+227 to +1,578).** Taken at face value against the standing kill-gate
convention, this is a FAIL. It is reported here in full rather than
omitted.

**Headline 2 — decomposed, the delta separates cleanly into two components
with very different implications.** The `Δ cache-size-alone` column
(base→isolate) is **small (36-43 Ir) and near-constant across all five
benches regardless of their wildly different internal shapes** (three flat
single-unit benches and two ~256-iteration loop benches all land within 7
Ir of each other) — this is the same signature task #496's
(`docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md`) `PerClass`
`#[repr(C)]` fix found for a bigger zero-initialized struct: **a ONE-TIME
per-`HeapCore::new()` zero-init cost from the larger `own_cache: [*mut u8;
16]` array (96 extra bytes to zero vs the old 4-entry array), not a per-op
cost.** The `Δ counter-alone` column (isolate→head) is small on the three
flat benches (+191, roughly constant, consistent with ONE
`contains_base` call per bench's canonical unit) and MATERIALLY LARGER on
the two ~256-iteration benches (+543, +1,535) — consistent with a SMALL,
FIXED per-call cost (the `fetch_add`) applied once per `contains_base`
invocation, scaling with how many frees/in-place-reallocs each bench
performs.

**Headline 3 — the counter component (the larger share of the delta) never
ships.** `CONTAINS_BASE_TIER1_HITS`/`_MISSES` are `#[cfg(feature =
"bench-internals")]` — absent from every real `production`-only build a
user actually links against (confirmed: `cargo bench --bench
perf_gate_iai --features production` — without `bench-internals` — refuses
to even COMPILE, "requires the features: alloc-global, bench-internals",
because the harness itself needs the oracle; a real deployment never
enables `bench-internals` at all). So of the +227-to-+1,578 raw delta the
standing kill gate would report, roughly 84-97% of it (the counter
component) is a measurement-instrument cost invisible to every real user,
and the REMAINING real-production-path cost is the 36-43 Ir one-time
`OWN_CACHE_SIZE` bootstrap shift — negligible against a heap's whole
lifetime, and NOT a per-op regression (the property the kill gate exists
to catch).

**Conclusion: the shipped change does not, in fact, cause a per-op
regression to the small-object hot path.** §6's `perf(runtime)` decision
and §5.1's underlying argument are CONFIRMED, not overturned — but §5.1's
own claim that the numbers were unobtainable on this dev host was wrong,
and should have been checked (WSL) before publishing "argued, not
measured." This addendum replaces that gap with the real numbers and their
honest decomposition, rather than leaving the claim unconfirmed.

**One methodological note for a future round building on `contains_base`:**
any FUTURE `bench-internals`-gated counter added to a function this
project's standing churn kill-gate benches exercise will show up as a
non-zero delta in the raw ±10-Ir gate for the SAME reason found here — the
gate has no way to distinguish "a real per-op regression" from "a new
measurement instrument's own overhead" without this kind of before/after
decomposition. A future round adding such a counter should proactively run
this same base→isolate→head three-arm split rather than let the raw gate
number stand unexplained.

## 6. Decision: `perf(runtime)`, shipped as the new default — with an honest scope caveat

**`OWN_CACHE_SIZE` is reachable through plain `production`** (§0's
feature-composition note: `alloc-xthread` is in `production`, making
`contains_base` the always-on ownership check on every own-thread free and
in-place `realloc`). This is therefore a `perf(runtime)` change per
CLAUDE.md's commit-prefix taxonomy, not an opt-in.

**What is actually claimed:** a measured, reproducible mechanism win (Tier-1
hit rate 0%→99.99% for workloads with ≤8 concurrently "hot" Large objects
under repeated in-place growth/probe traffic), NOT a measured latency win
(§4.1 headline 4 — the ns/op signal is a genuine, reported null on this
harness). The shipped value (16, not 32) is chosen because:

1. **16 already fixes the common case cleanly** — K≤8 reaches ~99.99% at
   both 16 and 32; the only additional K range 32 buys over 16 is roughly
   9-16 (§4.2's supplementary data), a narrower and less certain win.
2. **Memory cost scales linearly and 16 is already the survey's own
   suggested floor** ("`OWN_CACHE_SIZE` 4 → 16 (or 32)" — the survey named
   both as candidates, not a recommendation for one over the other).
3. **Neither size helps K ≥ its own value** (§4.1 headline 3) — this is a
   structural property of the direct-mapped design, not a size-selection
   question this task's data resolves either way; a future round choosing
   to widen further (or move to an associative design) should treat this as
   open, not settled by "32 not chosen this round."

**Honest scope statement:** this is a real, reproducible, MECHANISM-level
win at the specific workload shape this report measured (repeated in-place
`realloc`/probe traffic on a modest number of concurrently-live Large
objects) — it is NOT a validated wall-clock speedup claim, and a future
report finding this task's latency-null was actually a harness-noise-floor
artifact (rather than a genuine "the Ir delta is too small to matter at
this workload's total cost") would not contradict anything stated here.

## 7. Follow-up candidates identified, NOT pursued this task (out of scope)

- **F6-family redundancy in `dealloc_own_thread_with_base`'s Large
  fallthrough** (§2.1): `AllocCore::dealloc`'s Large branch re-derives
  `base` and re-runs `contains_base` even though the caller
  (`dealloc_own_thread_with_base`) already has `base` in hand and already
  proved `contains_base(base) == true`. The same shape task #494 already
  fixed for `realloc`'s move leg. A future task could give
  `AllocCore::dealloc` (or a new `_with_base` sibling) the pre-derived
  `base`, saving the redundant `os::segment_base_of_ptr` (~9 Ir) +
  `contains_base` (~8-12 Ir) on every Large free — independent of this
  task's `OWN_CACHE_SIZE` change, and orthogonal to it.
- ~~**A real Linux `Estimated Cycles`/RAM-hit run of the ±10 raw-Ir churn
  kill gate** (§5.1) — this task argued but did not measure that it stays
  flat.~~ **RESOLVED by §5.2** (same-day correction): measured via WSL,
  decomposed into a benign one-time bootstrap cost (36-43 Ir) plus a
  bench-internals-only measurement-instrument cost that never ships
  (191-1,535 Ir). Estimated Cycles/RAM-hit breakdown specifically (as
  opposed to raw Instructions) remains unmeasured — a narrower residual
  follow-up than originally stated.
- **Extending `benches/macro_multiseg_steady_state.rs` (task #500's
  harness) with a Large-heavy in-place-realloc churn variant**, so a future
  round's `iai-callgrind` `Estimated Cycles` axis can see this same effect
  at real cache-simulation granularity, not just wall-clock.

## 8. Provenance

- Base commit: `ce3f44da0a60d0f5c71b0c8bb26c1992726dccc4`. Landing commit
  (this task's own changes): `5289c661877462f3caf6c4e136ad3c163f6fe15b` —
  see that commit's message for the exact file list.
- **Immutable-source-identity caveat (CLAUDE.md's R29-6 rule, honestly
  applied):** the "before" (`OWN_CACHE_SIZE=4`) measurement was taken by
  TEMPORARILY editing the constant in the working tree, running the
  harness, then restoring it to the shipped value (16) — a
  source-CONSTANT-level before/after within one uncommitted working session,
  not two separate commits/worktrees. The "before" state is therefore **NOT
  separately reproducible from the landing commit alone** (the landing
  commit only contains the AFTER state, `OWN_CACHE_SIZE = 16`); reproducing
  the "before" numbers requires editing `OWN_CACHE_SIZE` back to `4` in a
  checkout of this commit (or any ancestor) and re-running the same example.
  This is a real, disclosed gap relative to the R29-6 rule's four preferred
  forms (temp commit SHA / `git write-tree` / patch hash / binary hash) —
  none of those was captured for the "before" state specifically, because
  the constant was flipped as a quick in-place probe rather than staged as
  its own commit. Flagged explicitly per the rule's own "an exemption-note
  route" allowance, rather than silently omitted.
- Raw logs (full, not truncated — each under 900 lines, below this
  project's truncation threshold): `docs/perf/_raw_r32_10_own_cache4_before.log`
  (OWN_CACHE_SIZE=4, 8192 rounds × 7 reps),
  `docs/perf/_raw_r32_10_own_cache16_after.log` (OWN_CACHE_SIZE=16, 8192
  rounds × 7 reps), `docs/perf/_raw_r32_10_own_cache32_exploratory.log`
  (OWN_CACHE_SIZE=32, supplementary, 512 rounds × 5 reps, §4.2 only).
- Summary CSV: `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv`
  (derived + headline-asserted by `scripts/r32_10_own_cache_tier1_summary.mjs`).
- CPU/OS: Windows 10 Pro 10.0.19045, Intel Core i7-11800H @ 2.30GHz
  (same-host, same-run relative comparisons — not a cross-host claim).

## 11. CORRECTED 2026-08-03 — the latency-null is now DEMONSTRATED, not merely asserted: paired-A/B with t-test + same-vs-same controls at all 7 K values (R33-5, task #510)

This section is appended, not a rewrite — §4.1's Headline 4 "the latency
signal is an HONEST NULL ... No latency win is claimed" stays exactly as
originally published (per this project's append-only correction convention;
see §5.2 for the established pattern). The round-32 readonly review
(`docs/reviews/2026-08-03-round32-readonly-review.md` §7, finding F4 [P2])
found that §4.1's null was **asserted, not demonstrated**: the report
contained no stddev, no t-test, no confidence interval, and no same-vs-same
control on the latency axis — only the phrase "run-to-run noise band." Two
other tasks in the same round (R32-11 §4.1–4.3, R32-12 §4) set the bar higher
for exactly this question (paired A/B with `t` vs `crit` plus same-vs-same
controls). This addendum closes that gap.

**Method.** The SAME harness as §4.1
(`examples/r32_10_own_cache_tier1_thrash_gate.rs`, child mode via
`R32_10_CHILD_K=<k>` env var, `ROTATING_ROUNDS=8192`), driven through
`scripts/paired-ab-runner.mjs`'s A/B/B/A protocol (20 pairs = 80 process
launches per comparison), matching R32-11's N=20 exactly. Three comparisons
per K value:

1. **before vs after** — the main comparison (`OWN_CACHE_SIZE=4` vs `=16`).
2. **before vs before** — same-vs-same control (identical binary against
   itself, establishes the harness noise floor).
3. **after vs after** — same-vs-same control (same purpose, the other arm).

All 7 K values from §3's sweep were measured: `{4, 8, 16, 24, 32, 48, 64}`.
Total: 21 comparisons × 20 pairs × 4 launches = **1,680 process launches**.
The metric is `churn_elapsed_ns` (integer ns, the total wall-clock of the
timed rotation region); the derive script converts to `ns_per_op` by dividing
by `ROTATING_ROUNDS × K`. The `t`-statistic itself is scale-invariant (both
mean and SE scale by the same constant), so the significance verdict is
identical whether computed on `churn_elapsed_ns` or `ns_per_op`.

**Binaries.** "after" = built from HEAD
(`7d55209de6159bd42397fc28a746715c97fc91a5`, `OWN_CACHE_SIZE = 16`, the
shipped state). "before" = built from the SAME HEAD with `OWN_CACHE_SIZE`
temporarily edited to `4` in `src/alloc_core/segment_table.rs` (the identical
scratch-edit technique §8 documents — the constant is flipped in-place, the
binary built, then the constant restored; `git diff` verified clean after
restoration). Both built `--features "production bench-internals"` (the
`bench-internals`-gated Tier-1 counters are present in BOTH arms identically,
so their `fetch_add` overhead cancels in the paired differential — unlike
R32-11's contaminated-timing finding, where the counter was new in only the
AFTER arm). **Immutable source identity (CLAUDE.md R29-6 rule, option 3):**
the "before" state's patch hash is
`9da1a54e83cec28adae585eeb1d2e55a93f44581f9471f13b268ff9fe85892ae`
(`sha256sum` of `git diff src/alloc_core/segment_table.rs` when
`OWN_CACHE_SIZE` is changed `16 → 4` over HEAD `7d55209...`); the "after"
state IS HEAD `7d55209...` directly (no patch).

**Path-activation oracle.** Every one of the 1,680 launches passed oracle #2
(`oracle2_pass = 1`, asserted by the paired-ab-runner's `sanity` gate — a
launch that failed would have aborted before its `churn_elapsed_ns` was
trusted). This confirms every timed region genuinely drove
`ROTATING_ROUNDS × K` real `contains_base` calls through the production
ownership check, at both cache sizes.

### 11.1 Main results — before (cache=4) vs after (cache=16), 20 paired A/B/B/A blocks

Derived by `scripts/r33_5_latency_null_addendum_summary.mjs` from the 21
`docs/perf/paired_ab_runs/2026-08-03T15-*.json` provenance files — every
headline number below is an in-script `assert` the derive script enforces
(it independently recomputes the t-test and sign test from the raw per-sample
`churn_elapsed_ns` arrays and verifies they match the runner's own values, per
CLAUDE.md's "a script that computes a headline ratio must assert the
arithmetic it prints" rule).

| K | before ns/op (mean of 20) | after ns/op (mean of 20) | Δ ns/op | % change | t | crit (p<0.05) | significant | sign test (before/after faster) |
|---:|---:|---:|---:|---:|---:|---:|---|---|
| 4 | 31.47 | 28.61 | +2.86 | −9.1% | 1.593 | 2.101 | no | 7/13 |
| 8 | 31.80 | 31.51 | +0.30 | −0.9% | 0.084 | 2.101 | no | 7/13 |
| 16 | 33.77 | 32.87 | +0.90 | −2.7% | 0.289 | 2.101 | no | 9/11 |
| 24 | 32.49 | 33.49 | −1.00 | +3.1% | −0.323 | 2.101 | no | 12/8 |
| 32 | 31.75 | 36.88 | −5.13 | +16.2% | −1.637 | 2.101 | no | 11/9 |
| 48 | 30.63 | 30.83 | −0.20 | +0.7% | −0.183 | 2.101 | no | 11/9 |
| 64 | 28.59 | 29.14 | −0.55 | +1.9% | −1.729 | 2.101 | no | 13/7 |

**No K value reaches statistical significance.** The maximum `|t|` across all
7 K values is 1.729 (at K=64), well below the p<0.05 critical value of 2.101
for df=19. The sign tests are roughly even at every K (never more lopsided
than 13/7), nowhere near the 17+/20 lopsidedness R32-11's favorable-regime
win showed. The latency null is **CONFIRMED**, not merely asserted.

**One directional observation worth recording honestly.** At K=4 — the cell
where the mechanism win is maximal (Tier-1 hit rate 0% → 99.99%) — the
nominal direction **favors after** (cache=16 is nominally ~9% faster, 7/20
sign test favoring after). This is the opposite of §4.1's single-run numbers
(cache=16 was +8.8% slower at K=4 in the original 7-rep median), and it is
directionally consistent with the mechanism (cache hits are cheaper than
cache misses): a Tier-1 hit costs ~8.2 Ir vs a Tier-2 miss's ~12.0 Ir
(OPEN_ITEMS item 1), so after *should* be marginally faster at K=4 — but the
~4 Ir/call delta is too small relative to the whole `realloc` call's cost
(header reads, size-class arithmetic, etc.) and the process-level timing
noise to reach significance. §4.1's original +8.8% figure at K=4 was
**run-to-run noise, not a real signal** — this is now demonstrated, not
asserted.

### 11.2 Same-vs-same controls — harness noise floor established

All 14 control runs (7 K values × 2 controls: before-vs-before and
after-vs-after) are cleanly NOT significant, with roughly-even sign splits:

| K | control | t | crit | sign test |
|---:|---|---:|---:|---|
| 4 | before-vs-before | −0.667 | 2.101 | 10/10 |
| 4 | after-vs-after | 1.147 | 2.101 | 9/10 |
| 8 | before-vs-before | −0.403 | 2.101 | 13/7 |
| 8 | after-vs-after | −0.852 | 2.101 | 11/9 |
| 16 | before-vs-before | −0.156 | 2.101 | 9/11 |
| 16 | after-vs-after | −0.371 | 2.101 | 10/10 |
| 24 | before-vs-before | −0.185 | 2.101 | 8/12 |
| 24 | after-vs-after | 0.922 | 2.101 | 11/9 |
| 32 | before-vs-before | −0.117 | 2.101 | 7/13 |
| 32 | after-vs-after | 0.259 | 2.101 | 8/12 |
| 48 | before-vs-before | 0.720 | 2.101 | 10/10 |
| 48 | after-vs-after | −0.259 | 2.101 | 9/11 |
| 64 | before-vs-before | −1.248 | 2.101 | 11/9 |
| 64 | after-vs-after | 1.361 | 2.101 | 8/12 |

Every control's `|t|` is well under 2.101 and no sign test is more lopsided
than 13/7. This confirms the before-vs-after results above are not a harness
artifact (non-reproducible workload, host-launch-order bias, etc.) — matching
R32-11 §4.3's and R32-12 §4's same-vs-same control conventions exactly.

### 11.3 Conclusion

**The original report's "honest null" latency claim (§4.1, Headline 4) is
CONFIRMED by rigorous paired-A/B evidence, not corrected to something else.**
The latency axis now carries: (a) N=20 paired samples per cell, (b) a `t`
statistic vs the df=19 critical value, (c) a sign test, and (d) same-vs-same
controls for both arms — the same evidence standard R32-11 and R32-12 applied
to their latency axes. No statistically significant latency difference between
`OWN_CACHE_SIZE=4` and `=16` exists at any of the 7 K values, including K=4
(where the mechanism win is maximal) and K=48 (where §4.1's original numbers
showed the largest nominal +9.5% delta). The `OWN_CACHE_SIZE = 16` production
default is NOT reverted (explicitly out of scope per the review's own
disposition — this measurement task does not license a revert).

**Why the null is real, not a measurement failure.** The Tier-1 hit/miss
delta is ~4 Ir/call (8.2 vs 12.0 Ir); at K=4 this saves ~4 × 32768 ≈ 131 K Ir
total per timed region, a real instruction-count win — but the `realloc` call
itself costs far more than just `contains_base` (header reads, size-class
arithmetic, `SegmentHeader` writes), so the ~4 Ir/call delta is a small
fraction of the total per-call cost, and that fraction is below the
process-launch-level timing noise on this Windows `Instant::now` harness
(~0.4–2 ns/pair standard error on this hardware, as R32-12 §4 independently
established for the same timer class). A future Linux/Valgrind
`Estimated Cycles` run (as §5.2 already obtained for the kill-gate axis) at
this workload shape could potentially resolve the sub-noise delta at the Ir
level, matching how §5.2 resolved the kill-gate question the raw ±10-Ir gate
could not — but that is a follow-up candidate, not a deficiency in this
addendum's wall-clock evidence.

### 11.4 Provenance and reproduction

- **"after" binary source identity:** HEAD `7d55209de6159bd42397fc28a746715c97fc91a5`
  (`OWN_CACHE_SIZE = 16`, the shipped state).
- **"before" binary source identity (R29-6 rule, option 3):** patch hash
  `9da1a54e83cec28adae585eeb1d2e55a93f44581f9471f13b268ff9fe85892ae` (sha256
  of `git diff src/alloc_core/segment_table.rs` over HEAD `7d55209...`,
  changing `OWN_CACHE_SIZE` from 16 to 4). Reproducible: checkout
  `7d55209...`, apply the one-line edit, re-hash the diff.
- **Feature set:** `production bench-internals` for both arms (the counter
  is present in both, cancelling in the differential).
- **CPU/OS:** Windows 10 Pro 10.0.19045, Intel Core i7-11800H @ 2.30GHz
  (same host as §4.1/§5.2).
- **Raw console logs:** `docs/perf/_raw_r33_5_k{4,8,16,24,32,48,64}_{before_after,before_control,after_control}.log`
  (21 files, `git add -f` per CLAUDE.md's raw-log policy).
- **Full per-launch provenance (structured JSON, raw per-sample data):**
  `docs/perf/paired_ab_runs/2026-08-03T15-{27-55-060Z,...,31-36-883Z}.json`
  (21 files — one per comparison; the derive script reads these and
  independently recomputes every statistic). Full file list in the companion
  CSV's `provenance_file` column.
- **Checked derive script:** `scripts/r33_5_latency_null_addendum_summary.mjs`
  (reads the 21 JSON files, recomputes t-test/sign-test from raw samples,
  asserts they match the runner's values, asserts all same-vs-same controls
  are non-significant, asserts n=20 for every comparison, writes the summary
  CSV).
- **Summary CSV:** `docs/perf/R32_10_LATENCY_NULL_PAIRED_AB_summary.csv`
  (21 rows: 7 K values × 3 comparisons, one row per paired_ab_runs JSON file;
  companion to the existing `R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv`
  — different schema, not appended to the original).
- **`landing_commit` field decision (per the F6 [P3] follow-up's lesson):**
  omitted from the derived CSV entirely (no `landing_commit` column). This
  measurement task has no landing commit at derive time (the commit is created
  after the script runs), and hardcoding a placeholder that must be
  back-filled is exactly the pattern F6 flagged. The measurement's source
  identity is captured in the prose above (HEAD SHA + patch hash) and in each
  JSON file's own `git_commit` field — both more durable than a CSV column
  that would be stale by construction.
- **Config JSON:** `scripts/_r33_5_own_cache_ab.json` (the paired-ab-runner
  config defining before/after binary paths, metric, and the `oracle2_pass`
  sanity gate).

**Reproduce:**

```text
# Build both binaries (after = current tree, before = OWN_CACHE_SIZE edited to 4):
cargo build --release --example r32_10_own_cache_tier1_thrash_gate --features "production bench-internals"
cp $CARGO_TARGET_DIR/release/examples/r32_10_own_cache_tier1_thrash_gate.exe <before_path>
# Edit OWN_CACHE_SIZE to 4, rebuild, copy to <after_path>, restore OWN_CACHE_SIZE to 16.

# Run one comparison (e.g. K=4 before vs after, 20 pairs):
R32_10_CHILD_K=4 node scripts/paired-ab-runner.mjs --config scripts/_r33_5_own_cache_ab.json --pairs 20

# Same-vs-same control:
R32_10_CHILD_K=4 node scripts/paired-ab-runner.mjs --config scripts/_r33_5_own_cache_ab.json --arms before,before --pairs 20

# Derive the summary CSV + verify all assertions:
node scripts/r33_5_latency_null_addendum_summary.mjs
```
