# R34-12 (task #531) — `RemoteFreeRing` shadow/cached head: CLEAN A/B re-gate

Date: 2026-08-04.

landing_commit: (filled post-commit)

## 0. What this is

The round-32/33 bench review
(`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`, "P1 —
эффект `RemoteFreeRing::cached_head` пока не изолирован") found that the
original R32-11 gate's claimed -30…-36% cross-thread win for the shadow-head
(`cached_head`) fast path was **not a clean A/B**:

1. **Different feature sets**: the "before" arm was built with `alloc-global
   alloc-xthread bench-internals`, the "after" arm with `alloc-global
   alloc-xthread` (no `bench-internals`).
2. **Different drain mechanisms**: "before" used the public diagnostic wrapper
   `SeferAlloc::dbg_drain_current_thread_rings` (`bench-internals`-gated),
   "after" used direct `tls_heap::current_for_trim()` +
   `HeapCore::dbg_drain_all_rings()`.

Since the owner drain runs **concurrently** with the timed producers, any
difference in wrapper codegen, feature compilation, or drain cadence can
change ring occupancy and producer timing independently of the shadow-head
mechanism itself.

This task builds a **clean A/B** with identical features, identical harness,
and identical drain mechanism for both arms, measures three regimes separately
(favorable / near-full / overflow), and reports an honest verdict.

### The meta-pattern (fourth instance)

This is the **fourth instance** of one meta-pattern that CLAUDE.md already
describes three times:

| instance | what differed between arms | CLAUDE.md rule |
|---|---|---|
| R26-4 (task #413) | wrong **CONFIG** — arms silently reused old pool cap | R26-4 config-evidence rule |
| R30-8 (task #452) | wrong **CODE PATH** — measured recycled not virgin | R30-8 path-activation oracle rule |
| R31-0 (task #471) | wrong **LAYER** — measured AllocCore not HeapCore | R31-0 entry-point rule |
| **R34-12 (this task)** | wrong **BUILD SHAPE** — arms differed in feature set + drain mechanism | *(this report)* |

The R32-11 gate's direction was correct (after IS faster), but the magnitude
was potentially confounded by the build-shape asymmetry. This re-gate isolates
the shadow-head mechanism cleanly.

## 1. What the clean A/B fixes

### 1.1 Identical feature sets

Both arms are built with `alloc-global alloc-xthread internals` — the SAME
feature set.

The `internals` feature (`internals = []` in Cargo.toml) is a **zero-runtime-
effect visibility gate**: it changes `pub(crate) mod` to `pub mod` for
`alloc_core`/`global`/`registry` in the AFTER tree (current HEAD), but has
NO cfg-gated code anywhere in `src/` (verified: `grep -rn 'feature =
"internals"' src/` matches only the six `mod` declarations in `lib.rs`). In
the BEFORE tree (`c9a3570`), these modules are already unconditionally `pub`,
so `internals` is a literal no-op — it was added to the BEFORE tree's
Cargo.toml solely to make the feature SET identical between arms.

No `bench-internals` in the timing build. The shadow fast/slow oracle counters
(`DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`) are NOT compiled in — they exist only in
the AFTER tree and only under `bench-internals`, which the timing build does
not enable. This eliminates the oracle-counter RMW overhead that caused the
original R32-11 gate's false-start regression (R32-11 §3.1).

### 1.2 Identical drain mechanism

Both arms use the SAME direct drain path:

```text
tls_heap::current_for_trim() → HeapCore::dbg_drain_all_rings()
```

No `bench-internals`-gated wrapper. The original gate's "before" arm used
`SeferAlloc::dbg_drain_current_thread_rings` (the public wrapper), while the
"after" arm used the direct path. Both paths call the same underlying
`dbg_drain_all_rings`, but the wrapper adds an indirection and, under
`bench-internals`, the `full_check` counter RMWs fire on every push. The
clean A/B uses the direct path for BOTH arms, so the drain SEMANTICS and
OVERHEAD are byte-identical.

### 1.3 Identical source

One source file (`examples/r34_12_remote_ring_clean_regate.rs`) compiles
byte-identically at both commits. The harness references only APIs that exist
in BOTH trees: `SeferAlloc`, `DBG_RING_OVERFLOW`, `tls_heap::current_for_trim`,
`HeapCore::dbg_drain_all_rings`.

## 2. Harness design

### 2.1 Allocator layer under test (R31-0 rule)

`SeferAlloc` — the `#[global_allocator]` layer. Every timed alloc/free goes
through `GlobalAlloc::dealloc` on `GLOBAL` — the SAME chain a real production
binary's cross-thread free uses (`SeferAlloc::dealloc` → `HeapCore::dealloc` →
`dealloc_routing` → `push_with_overflow_retry` → `RemoteFreeRing::push`).

### 2.2 Three regimes (measured separately, never merged)

| regime | owner drain cadence | ring occupancy | intended `full_check` path |
|---|---|---|---|
| `favorable` | continuous (tight poll, no sleep) | far from `RING_CAP` | AFTER: fast path (shadow proves room); BEFORE: always loads `head` |
| `near_full` | 500 µs between drains | near `RING_CAP` | AFTER: slow path (shadow says full, real check says room); BEFORE: always loads `head` |
| `overflow` | 5 ms between drains | at `RING_CAP` + heap-overflow ring saturated | AFTER: overflow path (Err returned); BEFORE: overflow path (Err returned) |

**Design parameters**: `PRODUCERS = 4`, `BLOCKS_PER_PRODUCER = 50,000`
(200,000 total pushes), `BLOCK_SIZE = 32` B (smallest size class, single
ring). `Barrier`-synchronised producer start. Timed region: wall-clock around
the producers' free loop (owner drain runs concurrently inside this window).

### 2.3 Oracle counters OUTSIDE the timed region

The only counter bumped INSIDE `push` is `DBG_RING_OVERFLOW` (on the overflow
path only, a `Relaxed fetch_add` — not on every push). The shadow fast/slow
counters are NOT compiled into the timing build. The overflow counter is read
BEFORE and AFTER the timed block, not inside it.

**Path-activation oracle (overflow-based, available in BOTH trees):**
- Favorable: `overflow_pct ≤ 2.0%` (ring stayed far from full)
- Overflow: `overflow_pct ≥ 1.0%` (Err path genuinely exercised)

The near-full regime uses the same `≤ 2.0%` threshold (it has ~1.9% overflow,
which is the documented sound leak, not a retry-storm).

**Shadow-counter oracle (AFTER tree only, separate bench-internals build):**
A separate build of the existing R32-11 harness with `bench-internals`
confirms regime fidelity via `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`:

| regime | fast_pct | slow_pct | overflow_pct | oracle |
|---|---:|---:|---:|---|
| favorable | 99.62% (374,002/375,430) | 0.38% (1,428/375,430) | 0.0065% (13/200,000) | PASS |
| adversarial (= near_full) | 7.33% (381,853/5,205,939) | 92.67% (4,824,086/5,205,939) | 1.87% (3,744/200,000) | PASS |

(The overflow regime's path activation is confirmed by the overflow counter
alone — the shadow counters show the slow path dominates, as expected.)

Note: the ns_per_push values from this oracle build are NEVER cited as timing
evidence — they include the oracle-counter RMW overhead (R32-11 §3.1). Only
the fast/slow/overflow percentages are cited, as regime-fidelity confirmation.

## 3. Immutable source identity

Per CLAUDE.md's R29-6 rule, both arms' source identity is recorded BEFORE
measurement:

| arm | identity type | value |
|---|---|---|
| BEFORE | commit SHA (git worktree) | `c9a3570bfa4393c4a8383be25c3654e1421c7671` |
| AFTER | git tree SHA (`git write-tree`) | `b7f0bf8060c33c5afddc4fab55b568a16e14d32e` |
| AFTER HEAD | commit SHA | `43115cf77290875933564040810f7f50707a9b5a` |
| harness source | sha256 | `67d57ea898f672323bc62eb786a675fa75663b08b53c2cb557e2b9ffca7e98bd` |
| BEFORE binary | sha256 | `c984757f97bc7e75b5c7b01bcd26dacc677be45d0a8ef6be900851d5188341d9` |
| AFTER binary | sha256 | `26fc4a00720ff98ddc819d063c21d7ce2344ebdcd6733e3bcce6a9ae773e27b0` |

The AFTER tree SHA (`git write-tree` after staging all changes) snapshots the
exact file contents at measurement time without requiring a commit. Both
binary hashes additionally prove which executables ran.

The BEFORE tree is a `git worktree` at `c9a3570` (the parent of the F10
landing commit `d38bf73`). The harness source was COPIED into the worktree's
`examples/` directory (the file did not exist at that commit — it was created
by this task). The `internals = []` feature was added to the worktree's
`Cargo.toml` as a no-op (modules are already `pub` in that tree).

**R34-6 note**: the AFTER tree includes commit `a9edc87` (R34-6, task #525),
which promoted `cached_head`'s accesses from `Relaxed` to `Acquire`/`Release`
in `full_check`. This promotion is byte-identical on x86-TSO (all x86 loads
are acquire, all non-SeqCst stores are release) and was independently measured
at noise-level cost (~1.9% delta, `benches/r34_6_remote_ring_cached_head_ordering_gate.rs`).
The "after" arm therefore measures the shadow-head mechanism AS IT CURRENTLY
SHIPS, including the ordering promotion.

## 4. Timing result (clean A/B, 20 pairs per comparison)

Harness: `examples/r34_12_remote_ring_clean_regate.rs`, timing-only build
(`--features "alloc-global alloc-xthread internals"`, no `bench-internals`).
Judge: `scripts/paired-ab-runner.mjs --config docs/perf/r34_12_run.json`,
A/B/B/A protocol, 20 pairs/comparison (80 process launches per comparison,
240 total). `CARGO_TARGET_DIR` isolated per tree
(`D:/dev/rust/.cargo-target-r34-12-before` for BEFORE,
`D:/dev/rust/.cargo-target` for AFTER) so neither clobbers the other's binary
— the R33-12 target-dir binary-reuse trap.

### 4.1 Favorable regime — CONFIRMS original claim

| metric | before | after | Δ |
|---|---:|---:|---:|
| ns/push (mean of 20 pair-blocks) | 182.60 | 124.04 | −32.07% |
| mean delta (elapsed_ns) | — | — | 11.712 ms |
| t-statistic | — | — | 11.125 |
| t-critical (df=19, p<0.05) | — | — | 2.101 |
| significant | — | — | **YES** |
| sign test (before/after faster) | — | — | 0/20 |

**Verdict: the original R32-11 claim of −30…-36% is CONFIRMED at −32.07%
under the clean A/B.** The sign test is maximum lopsidedness (0/20 — after
won every single one of the 20 paired blocks). This is consistent across all
20 independent pair-blocks with no outlier sensitivity.

### 4.2 Near-full regime — smaller but significant win

| metric | before | after | Δ |
|---|---:|---:|---:|
| ns/push (mean of 20 pair-blocks) | 2,259.74 | 2,185.94 | −3.27% |
| mean delta (elapsed_ns) | — | — | 14.759 ms |
| t-statistic | — | — | 3.279 |
| t-critical (df=19, p<0.05) | — | — | 2.101 |
| significant | — | — | **YES** |
| sign test (before/after faster) | — | — | 6/14 |

**Verdict: the shadow-head shows a REAL but smaller win (~3.3%) in the
near-full regime.** Consistent with the mechanism: when the ring sits near
`RING_CAP`, the shadow usually says "might be full" (forcing the real
`head.load(Acquire)` anyway), so the shadow only saves the cross-core
coherence miss on the ~7% of pushes where the shadow still proves room (per
§2.3's oracle). The absolute delta (~74 ns/push) is the same order as the
favorable regime's ~59 ns/push, but the total ns/push is ~18× higher (dominated
by retry-storm cost), so the percentage is smaller.

### 4.3 Overflow regime — NOT statistically distinguishable

| metric | before | after | Δ |
|---|---:|---:|---:|
| ns/push (mean of 20 pair-blocks) | 13,145.82 | 13,077.61 | −0.52% |
| mean delta (elapsed_ns) | — | — | 13.643 ms |
| t-statistic | — | — | 2.068 |
| t-critical (df=19, p<0.05) | — | — | 2.101 |
| significant | — | — | **NO** |
| sign test (before/after faster) | — | — | 9/11 |

**Verdict: the shadow-head shows NO statistically significant difference in
the overflow regime (t=2.068 < crit=2.101, sign test 9/11 roughly even).**
The overflow regime is dominated by `push_with_overflow_retry`'s spin-retry-
sleep cost (~15 ms effective per round on this Windows host): each push
attempts `push` → `push_to_heap_overflow` → retry loop, and the retry loop's
OS-level `thread::sleep(200µs)` (effective ~15 ms granularity) dwarfs the
shadow-head's nanosecond-scale `full_check` cost. The absolute delta (~68
ns/push) is the same order as the other regimes, but the total ns/push is
~60× higher, so the percentage vanishes into noise.

**Honest note on the overflow rate**: in this allocator's design,
`push_with_overflow_retry`'s retry loop (with `LAST_STALL_CONCESSIONS` fast-
concede cache) prevents dramatic overflow-rate increases when the owner is
live — it retries until the owner drains, rather than overflowing. The
overflow rate stays ~1.8% across both near-full and overflow regimes (3,581/
200,000 and 3,714/200,000 respectively). The overflow regime is distinguished
from near-full by its ~6× higher ns/push (more retry-sleep per push), not by a
higher overflow rate. A genuinely overflow-dominated regime would require the
owner to be completely paused, which triggers the 128-round (~2 second) first-
concession cost — impractical for repeated measurement.

### 4.4 Same-vs-same controls — harness reliability confirmed

| control | t | crit | significant | sign test |
|---|---:|---:|---|---|
| after_favorable vs after_favorable | 0.223 | 2.101 | no | 10/10 |
| after_near_full vs after_near_full | −0.995 | 2.101 | no | 11/9 |

Both controls are cleanly NOT significant with roughly-even sign splits —
confirming the before/after results are not a harness artifact.

## 5. Comparison with the original R32-11 gate

| regime | R32-11 (original) | R34-12 (this clean A/B) | direction |
|---|---|---|---|
| favorable | −30.00 to −35.75% (3 trials) | −32.07% | **CONFIRMED** — clean A/B reproduces the original claim |
| adversarial / near-full | −0.96 to −9.53% (5 trials, 3 significant) | −3.27% (significant) | **CONFIRMED** — within the original range |
| overflow | *(not measured)* | −0.52% (not significant) | NEW — no measurable shadow-head effect |

The original gate's favorable-regime numbers (−30 to −36%) were DIRECTIONALLY
correct despite the build-shape asymmetry, because the shadow-head's
cross-core-coherence-miss savings (~59 ns/push) dominate over the confounding
factors in the favorable regime (where the ring is nearly empty and push cost
is the primary contributor). The clean A/B confirms this.

## 6. Hardware counters — not available

`perf stat` (Linux hardware counters for cache-line transfers / LLC misses) is
not available in this Windows environment. The wall-clock measurement is the
available evidence; the cross-core coherence mechanism is confirmed by the
assembly-level analysis in R32-11's §1 and R34-6's byte-identical disassembly
proof.

## 7. Derive script and raw data

**Checked derive script**: `scripts/r34_12_clean_regate_summary.mjs` — reads
the raw paired-ab provenance JSON (per-process samples, NOT aggregates),
derives every headline number, and asserts the arithmetic (CLAUDE.md's
R22-14 / derive-from-raw-data rule). Running it reproduces
`docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE_summary.csv`.

**Raw provenance**: the paired-ab-runner writes per-process JSON provenance to
`docs/perf/paired_ab_runs/`. This report cites:
- Main run (3 comparisons, 20 pairs each):
  `docs/perf/paired_ab_runs/2026-08-04T16-40-55-214Z.json`
- Favorable control (same-vs-same, 20 pairs):
  `docs/perf/paired_ab_runs/2026-08-04T16-41-47-549Z.json`
- Near-full control (same-vs-same, 20 pairs):
  `docs/perf/paired_ab_runs/2026-08-04T16-42-31-023Z.json`

**Raw runner output** (truncated to the statistics summary + first/last sample
lines): `docs/perf/_raw_r34_12_paired_ab_full.log`.

## 8. Verdict

**The original R32-11 claim of −30…-36% for the shadow-head fast path IS
CONFIRMED under the clean A/B: the favorable regime measures −32.07%
(t=11.125, sign test 0/20), squarely within the original range.**

The near-full regime shows a smaller but significant win (−3.27%, t=3.279,
sign test 6/14). The overflow regime shows no significant difference (t=2.068
< crit=2.101, sign test 9/11), consistent with the retry-storm dominating the
total cost at that occupancy level.

**No production code changed.** This is a measurement-only re-gate of an
already-shipped mechanism (the F10 shadow-head, landing commit `d38bf73`, plus
the R34-6 ordering promotion, commit `a9edc87`). The harness, run config,
derive script, and report are all new files.

## 9. Files changed

- `examples/r34_12_remote_ring_clean_regate.rs` (new) — the clean A/B harness.
- `Cargo.toml` — registers the new example (`required-features =
  ["alloc-global", "alloc-xthread", "internals"]`).
- `scripts/r34_12_clean_regate_summary.mjs` (new) — the checked
  summary-derivation script.
- `docs/perf/r34_12_run.json` (new) — the `paired-ab-runner.mjs` config.
- `docs/perf/_raw_r34_12_paired_ab_full.log` (new, `git add -f`) — cited raw
  runner output (truncated).
- `docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE_summary.csv` (new) — derived.
- `docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE.md` (new) — this report.
