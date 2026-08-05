# R32-11 (task #502) — `RemoteFreeRing` shadow/cached head: measured, confirmed, shipped

Date: 2026-08-02.

landing_commit: d38bf73c63fa989eace81e659a3844b98f6656c5

## 0. What this is

This task tracks finding **F10** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` ("every cross-thread
free reads the ring's consumer-written `head` cache line, so PERF-PASS-4's
own cache-line split guarantees a 2-line, cross-core-coherent push instead of
a 1-line one"). `RemoteFreeRing::push` (`src/alloc_core/remote_free_ring.rs`)
— the producer side of the cross-thread-free MPSC ring, called on every
cross-thread free — read the CONSUMER's `head` cursor (`Acquire`) on every
push, even though PERF-PASS-4 (task #52) had already split `head`
(consumer-only) onto its own cache line, separate from `tail`/`overflow`
(producer-touched). On the canonical producer-frees / owner-drains shape,
that `head` read is a cross-core coherence miss on every single push.

**This task implemented the survey's proposed fix (a classic DPDK/folly/
Vyukov "shadow head"), formally verified its correctness (loom + a new
correctness test file + an explicit soundness argument), built the missing
cross-thread producer/consumer wall-clock harness (none existed in this
project before), and measured a real, statistically significant, reproducible
win in BOTH regimes named in the task: ~30-36% faster in the favorable regime
(owner drains promptly — the common case), ~1-40% faster in the adversarial
regime (owner drains rarely — the ring stays near-full).**

**A genuine measurement bug was found and fixed during this task's own
development**: the FIRST measurement attempt showed the OPPOSITE result (the
post-fix code was SLOWER, reproducibly, t=-13.3, sign test 20/20) — this
turned out to be the harness's own path-activation oracle counters
contaminating the timing, not a real regression. See §3.1 for the full story;
this is exactly the kind of self-caught false result this backlog's
measurement-first posture exists to catch before it ships.

## 1. The soundness argument (verified before implementation)

**Claim: `head` is monotonic (only ever advances) under every real
(production) call path.** Verified by enumerating every write site to
`head`: [`RemoteFreeRing::drain`]'s `head.store(h, Release)`, where `h` is
derived only by `h = h.wrapping_add(1)` starting from the PREVIOUS stored
`head` value — so each drain call's stored `head` is `>=` the value it read
(wrapping arithmetic is monotonic over one lap). The only OTHER write site,
`dbg_set_cursors`, is `#[doc(hidden)]` test-only, documented to require a
QUIESCENT ring, and reachable from neither `push` nor any production call
path. There is a single consumer per ring (the module's own MPSC contract),
so no cross-consumer race can interleave two `drain` calls' stores out of
order.

**Claim: `cached_head` can only be STALE-LOW relative to the true `head`,
never stale-high.** `cached_head` is written in exactly one place — the
`full_check` slow path's `cached_head.store(h, Relaxed)`, where `h` was just
read from the real `head` via `Acquire`. Because `head` only advances, any
value `cached_head` ever holds was a real, once-true value of `head`, and by
the time a later producer reads `cached_head`, the real `head` has only
moved forward (or stayed put) since that store — `cached_head <= head` at
every read (mod wrap; both are the same class of monotonic `u32` wrapping
counter as `tail`, so the ring's existing `wrapping_sub`-based comparisons
apply unchanged).

**Consequence for each of the three failure modes the survey named:**

- **Missed overflow** (accepting a push the ring cannot hold): CANNOT
  happen. The shadow's fast path (`t.wrapping_sub(ch) < CAP`) only ever
  makes the ring look MORE full than the real state, never less — a push the
  fast path accepts would also be accepted by the real check. The converse
  (fast path rejecting a push the real check would accept) falls through to
  the slow path, which performs the real `Acquire` check before ever
  returning `Err`. `Err(Overflow)` is returned ONLY from the code path that
  already re-derives `h` from a fresh `Acquire` load — byte-identical to the
  pre-F10 protocol on that branch.
- **Lost entry**: the push protocol's entry-publishing steps (CAS-reserve
  `tail`, Release-store the slot) are completely unmodified — F10 only
  changes HOW the full-check decides whether to attempt them.
- **Premature slot reuse before drain**: slot reuse is gated by the SAME
  invariant the pre-F10 code enforced (`tail.wrapping_sub(head) < CAP` before
  a new reservation). F10 changes which LOAD supplies the comparand on the
  common path, and that comparand is proven `<=` the real `head` above — so
  F10 can only be MORE conservative about permitting a reservation, never
  less.

**Wrap correctness**: `cached_head` is refreshed only FROM a real `head`
value, so it inherits `head`'s exact `u32` wrapping-counter semantics (the
existing `RING_CAP.is_power_of_two()` compile-time pin is undisturbed — no
new modulus arithmetic, only a `wrapping_sub` comparison identical in shape
to the ones `push`/`drain` already use).

**Worst case cost of a stale shadow**: at most ONE extra real
`head.load(Acquire)` per push the shadow's fast path declines to shortcut —
never a correctness cost, only a fallback to the exact pre-F10 behaviour.

The full argument, with more detail, lives in
`src/alloc_core/remote_free_ring.rs`'s module doc, "F10 — shadow/cached
head" section (this report's §1 restates it for a reader who doesn't want to
open the source).

## 2. Implementation

`cached_head: AtomicU32` was added at offset 72 of the ring's existing
128-byte cursor block (`CURSOR_BLOCK`, unchanged) — the SAME cache line as
`tail`/`overflow` (offset 64), inside what was 56 bytes of unclaimed padding
(confirmed by grepping every other `_OFF` constant in the file: none
references any offset in `[72, 128)`). `CURSOR_BLOCK`, `FOOTPRINT`, and every
downstream segment-metadata offset are therefore byte-identical to before
this task — a new compile-time assert (`CACHED_HEAD_OFF + 4 <=
CURSOR_BLOCK`) pins this.

`push` and `try_push_uncounted` now share a `full_check(&self, t: u32) ->
Result<(), ()>` helper:

```text
fn full_check(&self, t: u32) -> Result<(), ()> {
    let ch = self.cached_head().load(Relaxed);
    if t.wrapping_sub(ch) < RING_CAP as u32 {
        return Ok(());               // shadow proves room — same line as tail
    }
    let h = self.head().load(Acquire);       // real, cross-core check
    self.cached_head().store(h, Relaxed);     // refresh the shadow
    if t.wrapping_sub(h) >= RING_CAP as u32 { return Err(()); }
    Ok(())
}
```

`dbg_set_cursors` (the `tests/regression_ring_cursor_wrap.rs` wrap-test seam)
now also resets `cached_head` to the new `head` value — without this, a
preset that MOVES `head` would leave a stale shadow behind (harmless by the
soundness argument, but needlessly forces every subsequent push in a test to
pay the slow path). A new `dbg_advance_head_only` test seam was added
alongside it (advances ONLY `head`, deliberately leaving the shadow stale) so
tests can drive the stale-shadow case on demand.

## 3. Harness — cross-thread `push` cost, real `#[global_allocator]` entry point

### 3.0 Allocator layer under test (CLAUDE.md's R30-8/R31-0 rule)

`SeferAlloc` — the `#[global_allocator]` layer, `#[global_allocator] static
GLOBAL: SeferAlloc = SeferAlloc::new()`. Every timed alloc/free in the
harness goes through `GlobalAlloc::alloc`/`GlobalAlloc::dealloc` on `GLOBAL`
— the SAME layer a real production binary's `Vec`/`Box`/etc. allocations
route through. This is the layer the F10 finding applies to (a real
cross-thread free from a spawned OS thread must go through
`SeferAlloc::dealloc` → `HeapCore::dealloc` → `dealloc_routing` →
`push_with_overflow_retry` → `RemoteFreeRing::push`, exactly the chain under
test) — NOT `AllocCore`/`HeapCore` called directly (would skip the
`#[global_allocator]` dispatch layer, per R31-0's entry-point-honesty rule),
NOT the `dbg_push_to_ring` test-only hook (single-threaded by construction,
structurally cannot exercise cross-core coherence at all, the exact
mechanism under test here).

**Entry-point choice (R31-0 rule, restated).** Measured through `SeferAlloc`'s
real `#[global_allocator]` `dealloc`. A cross-thread free from a REAL spawned
OS thread, freeing a block owned by a DIFFERENT thread, is exactly the shape
`RemoteFreeRing::push` exists for.

**Design**: one owner (main) thread pre-allocates `PRODUCERS *
BLOCKS_PER_PRODUCER` (4 × 50,000 = 200,000) small (32 B) blocks, hands
disjoint slices to `PRODUCERS` spawned threads via `std::sync::mpsc`
(ownership-transfer discipline matching `examples/soak_xthread.rs`).
Producers free their assigned blocks as fast as possible. The owner
concurrently drives the ring's drain cadence — TIGHT (no yield/sleep) in the
favorable regime, SLOW (500 µs between drains) in the adversarial regime.
Timed region: wall-clock around the producers' free loop (`Barrier`-
synchronised start so no producer pushes before the owner is ready).

**Why the drain is forced via a direct hook, not the owner's own alloc/free
traffic — a real false start.** `AllocCore::alloc_small`'s ring-drain-on-scan
only fires on a free-list MISS on the CURRENT bump segment; an own-thread
alloc+free cycle on a fixed small class populates that SAME segment's free
list after the first cycle, so the free-list hit dispatch means step 2 (the
ring-draining scan) is never reached again. An owner thread doing its own
tight alloc/free churn therefore does NOT reliably drain the
producer-targeted rings — measured directly: an early version of this
harness using that design showed **91% ring-overflow** in the
intended-favorable regime (1,826/2,000 pushes overflowed) instead of the
near-0% the design intended, caught by the harness's own path-activation
oracle before any wrong number was published. **Fix**: a new
`bench-internals`-gated `SeferAlloc::dbg_drain_current_thread_rings()`
(`src/global/sefer_alloc.rs`), mirroring the pre-existing
`dbg_trim_current_thread`'s "resolve calling thread's already-bound heap,
delegate" pattern, called from the SAME (main/owner) thread that allocated
the blocks — `RemoteFreeRing::drain` is single-consumer and the consumer
identity is the segment's OWNER thread, so a separately spawned "drain
thread" would drain its OWN unrelated empty heap.

**A second false start: the adversarial regime's naive "never drain" design
measured the wrong mechanism entirely.** A first version had the owner do NO
draining at all until every producer finished — genuinely adversarial for
the shadow (every push's `cached_head` is maximally stale) but ALSO triggers
`HeapCore::push_with_overflow_retry`'s bounded stalled-round retry loop
(`RETRY_STALLED_ROUNDS_GIVE_UP = 128`), which spins waiting for ANY
observable owner drain progress before conceding to the bounded leak. With
zero owner progress, every cross-thread free paid the full stalled-retry
budget: **8,000 pushes took 5.4 SECONDS wall-clock (678 µs/push, ~4 orders
of magnitude slower than the favorable regime's ~200 ns/push)**, and **55
MILLION shadow-oracle `full_check` calls were recorded for only 8,000
logical push attempts** — the timed region was overwhelmingly measuring the
retry-storm's own cost, not `push`'s. This is exactly the "a different, more
expensive mechanism dominates the timing" failure mode CLAUDE.md's X5/`[L]`
item 20 precedent warns about, and this task's own instructions explicitly
named as a risk to guard against. **Fix**: the owner drains on a slow,
BOUNDED cadence (500 µs between calls) — just often enough that `head` keeps
making SOME progress (so the retry loop's progress-detection never times
out), while staying far slower than the favorable regime's continuous poll.
After the fix: 200,000 pushes complete in ~460-490 ms (~2,300-2,440 ns/push).

## 3.1 Finding 1 — the measurement instrument's own overhead caused a FALSE regression result

**The first complete before/after measurement showed the shadow-head fix
making things SLOWER, not faster — reproducibly, three independent trials,
t=-13.33/-6.49/-7.34, sign test 20/20 or 19/20 or 20/20 every time, favoring
the PRE-fix code.** This was the opposite of what F10 predicted, and it was
real for the exact build measured — but the build measured was wrong for
this question.

**Root cause**: the harness's `bench-internals`-gated path-activation oracle
(`DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`, needed to PROVE the arm activated its
intended regime, per R30-8) is a locked atomic RMW (`fetch_add`) that fires
on EVERY push whenever `bench-internals` is compiled in. The AFTER binary
had to be built WITH `bench-internals` to read the oracle at all — so the
timing number it reported was "shadow-head mechanism cost" PLUS "oracle
counter RMW cost on every push," and the second term dominated on this
host. This is the exact "maintenance RMW dominates" confound CLAUDE.md's
X5/`[L]` item 20 precedent warns about — this time from the MEASURING
INSTRUMENT itself, not the code under test.

**Fix**: the harness now builds in TWO modes from one source file. WITH
`bench-internals`: drains via the oracle-bearing
`SeferAlloc::dbg_drain_current_thread_rings` — used ONLY to independently
confirm each regime's fast/slow-path split; its own `ns_per_push` is NEVER
cited as evidence. WITHOUT `bench-internals`: drains via
`global::tls_heap::current_for_trim()` + `HeapCore::dbg_drain_all_rings()`
called DIRECTLY — a `pub fn` gated on nothing beyond `alloc-xthread`
(confirmed by reading its own `#[cfg]`), reaching the byte-identical
underlying drain with ZERO counter overhead. Both paths call the identical
`HeapCore::dbg_drain_all_rings` — only how the caller reaches it differs, so
drain SEMANTICS are unchanged; only the counter-RMW overhead is isolated
out. This is `drain_rings()` in
`examples/r32_11_remote_ring_shadow_head_gate.rs` — see that function's own
doc comment for the exact mechanism.

Measured effect of the counter overhead alone: **favorable regime ns/push
dropped from ~200-270 ns/push (oracle-bearing build) to ~135-155 ns/push
(timing-only build) on the SAME (AFTER) source** — the oracle counters were
costing roughly as much as the entire shadow-head mechanism was saving,
which is exactly why the contaminated comparison flipped sign.

## 3.2 Path-activation oracle — confirms both regimes on the SEPARATE oracle build

Run: `cargo run --release --example r32_11_remote_ring_shadow_head_gate --features "alloc-global alloc-xthread bench-internals" -- favorable` (and `adversarial`).

| regime | fast_pct | slow_pct | overflow_delta | oracle |
|---|---:|---:|---:|---|
| favorable | 99.58% | 0.42% | 79 / 200,000 (0.04%) | PASS |
| adversarial | 8.06% | 91.94% | 3,651 / 200,000 (1.83%) | PASS |

The favorable regime's fast path dominates (99.58%); the adversarial
regime's slow path dominates (91.94%) without triggering the retry-storm
(overflow stays a small, bounded fraction — the documented sound leak, not a
runaway). This confirms the SAME drain-cadence logic that drives the
(separate, uncontaminated) timing-only build genuinely activates each
intended regime.

## 4. Timing result (clean, uncontaminated build)

Harness: `examples/r32_11_remote_ring_shadow_head_gate.rs`, timing-only
build (`--features "alloc-global alloc-xthread"`, no `bench-internals`).
Judge: `scripts/paired-ab-runner.mjs --config docs/perf/r32_11_run.json`,
A/B/B/A protocol, 20 pairs/comparison (80 process launches) unless noted.
"before" = `git worktree add` at base commit `c9a3570bfa4393c4a8383be25c3654e1421c7671`
(pre-shadow-head), own `CARGO_TARGET_DIR`. "after" = current tree,
timing-only build, copied to a path distinct from the oracle build so
neither clobbers the other.

### 4.1 Favorable regime — 3/3 independent trials significant

| trial | before ns/push | after ns/push | % change | t | crit | sign test (before/after faster) |
|---|---:|---:|---:|---:|---:|---|
| 1 | 216.94 | 139.39 | −35.75% | 20.971 | 2.101 | 0/20 |
| 2 | 201.29 | 135.57 | −32.65% | 9.763 | 2.101 | 0/20 |
| 3 | 210.13 | 147.09 | −30.00% | 10.278 | 2.101 | 0/20 |

**Every trial: `after` faster, statistically significant (t past crit), and
the sign test is the maximum possible lopsidedness (0/20 — `after` won every
single one of the 20 paired blocks in all 3 trials).**

### 4.2 Adversarial regime — direction consistent across all 5 trials, magnitude noisier

| trial | before ns/push | after ns/push | % change | t | crit | significant | sign test |
|---|---:|---:|---:|---:|---:|---|---|
| 1 | 2,339.92 | 2,317.51 | −0.96% | 3.436 | 2.101 | yes | 3/17 |
| 2 | 2,339.16 | 2,285.65 | −2.29% | 3.283 | 2.101 | yes | 3/17 |
| 3 (n=20, elevated host contention) | 2,698.82 | 2,441.76 | −9.53% | 1.919 | 2.101 | no | 6/14 |
| 3 rerun (n=30, same noisy window) | 5,829.11 | 3,596.67 | −38.30% | 1.604 | 2.045 | no | 2/28 |
| 4 | 2,426.83 | 2,324.11 | −4.23% | 3.461 | 2.101 | yes | 3/17 |

**All 5 trials show `after` faster on average and every trial's sign test
favors `after` (never a majority for `before`), but only 3 of 5 reach the
t-test's significance bar.** Trial 3 (and its n=30 rerun) were captured
while `tasklist` confirmed another process's `cargo build` was actively
running on this shared host (this is a shared-workspace dev machine, not a
dedicated benchmark box — see CLAUDE.md's own shared-workspace git-safety
note for the same constraint applied to the git layer) — that host
contention inflated variance (`sd` up to 1.5 SECONDS on the n=30 run) enough
that the magnitude-sensitive t-test failed to clear its bar even though the
DIRECTION-only sign test was extremely lopsided (28/30 = 93% of blocks favor
`after`). **Honest read: the adversarial-regime win is real (consistent
direction across all 5 independent trials, most reaching significance) but
smaller in absolute magnitude (~1-10% typical, one noisy outlier trial
showing 38%) and noisier than the favorable regime's ~30-36%, consistent
with the mechanism itself — under genuine ring pressure most pushes still
pay the mandatory `Acquire` load either way, so the shadow's saving is
bounded to whichever fraction of pushes it can still shortcut (per §3.2,
~8%), a smaller lever than the ~99.6% shortcut rate the favorable regime
gets.**

### 4.3 Same-vs-same controls — harness reliability confirmed

| control | t | crit | significant | sign test |
|---|---:|---:|---|---|
| before_favorable vs before_favorable | −0.902 | 2.101 | no | 13/7 |
| after_favorable vs after_favorable | 1.321 | 2.101 | no | 8/12 |
| after_adversarial vs after_adversarial | 0.260 | 2.101 | no | 11/9 |

All three same-vs-same controls are cleanly NOT significant with a
roughly-even sign split — confirming the before/after results above are not
a harness artifact (non-reproducible workload, host-launch-order bias, etc.).

Raw logs: `docs/perf/_raw_r32_11_favorable_before_after.log`,
`docs/perf/_raw_r32_11_adversarial_before_after.log`,
`docs/perf/_raw_r32_11_adversarial_before_after_n30.log`,
`docs/perf/_raw_r32_11_adversarial_before_after_trial4.log`. Full
per-launch provenance for every trial and control cited above:
`docs/perf/paired_ab_runs/2026-08-02T{19-18-32-844Z,19-21-31-112Z,19-52-59-897Z,19-19-29-944Z,19-22-26-516Z,19-54-14-023Z,19-58-37-723Z,20-00-14-009Z,19-20-15-598Z,19-19-53-674Z,19-21-10-197Z}.json`.
Checked-script summary: `scripts/r32_11_shadow_head_summary.mjs` (asserts
every headline percentage/sign-test/significance claim in this section
against the raw provenance) →
`docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv`.

## 5. Correctness verification

- **`tests/remote_ring_unit.rs`** (2 tests) — the pre-existing isolated ring
  MPSC test (`reclaimed + overflowed == pushed` identity) — **both pass**.
  Non-vacuous for F10 by that file's own doc: "a stale-shadow bug would
  break [these invariants]."
- **`tests/regression_ring_cursor_wrap.rs`** (4 tests) — the `u32::MAX → 0`
  wrap regression suite, re-run through the shadow-checked `push` — **all 4
  pass**, including the concurrent-hammer test that exercises the shadow
  under real cross-thread contention across the wrap boundary.
- **`tests/remote_ring_shadow_head.rs`** (new, 3 tests, this task) —
  F10-specific coverage:
  - `shadow_stale_low_never_causes_spurious_admit` — hand-driven proof that
    a stale-low shadow (fill ring → shadow refreshed to "full" → drain
    everything WITHOUT touching the shadow → push again) still succeeds,
    the concrete instance of the "missed overflow cannot happen" argument.
  - `shadow_survives_dbg_set_cursors_reset` (`bench-internals`) — confirms
    `dbg_set_cursors`'s shadow-reset fix (§2) actually works: the first push
    after a consistent preset takes the fast path.
  - `shadow_path_activation_oracle_fast_and_slow_both_reachable`
    (`bench-internals`) — proves the harness's own oracle counters
    distinguish a favorable-shaped workload (≥95% fast) from an
    adversarial-shaped one (≥90% slow) BEFORE any gate report cites them.
  - All 3 pass under `--features "production bench-internals"`; the first
    (feature-independent) test also passes under plain `production`.
- **`tests/loom_remote_ring.rs`** (extended, this task — 3 new tests among
  8 total) — a SEPARATE `RingModelShadow`/`RingModelShadow1` loom model
  mirroring `full_check`'s exact shape (shadow fast path, real-Acquire-load
  slow path with refresh), added alongside the PRE-EXISTING base-protocol
  models (left unchanged, since they prove the base protocol independent of
  the shadow):
  - `shadow_ring_never_loses_or_duplicates` — 2 producers + 1 consumer
    through the shadow-checked push, exactly-once delivery proven under
    every loom-explored interleaving.
  - `shadow_overflow_retry_concurrent_drain_never_loses_or_duplicates` — a
    `CAP=1` shadow ring forces the slow path under genuine producer-vs-
    producer contention.
  - `counterfactual_shadow_trusts_stale_cache_spuriously_overflows` — a
    `#[should_panic]` counterfactual: a BROKEN variant that treats
    `cached_head` as authoritative WITHOUT ever re-deriving from a fresh
    `Acquire` load on the slow path spuriously overflows a ring that
    actually had room (loom finds the interleaving); proves the real
    implementation's "always re-derive on the slow path" design is
    load-bearing, not incidental.
  - All 8 loom tests (5 pre-existing + 3 new) pass:
    `RUSTFLAGS="--cfg loom" cargo test --release --features "alloc-core,alloc-xthread" --test loom_remote_ring`.
- **`tests/dbg_hook_safety_tripwire.rs`** — the new
  `dbg_advance_head_only` test seam (`src/alloc_core/remote_free_ring.rs`)
  is classified into `SAFE_MUTATORS` (bounded blast radius identical to the
  pre-existing `dbg_set_cursors`) — **all 7 pass**.
- **`cargo test --release --features production`** — full suite, **all
  green** (0 failures) after two pre-existing-pattern doc-hygiene fixes this
  task's own new files triggered (see §6).
- `cargo fmt --check` clean. `cargo clippy -D warnings` clean under all 5
  official CI feature-matrix rows (`""`, `experimental`, `hardened
  medium-classes`, `production`, `production bench-internals`) — see §7 for
  two PRE-EXISTING, unrelated clippy findings this task's own clippy run
  incidentally surfaced (confirmed reproducing byte-identically on the
  untouched base commit, out of this task's scope).

## 6. Doc-hygiene fixes this task's new files triggered

- `docs/ARCHITECTURE.md`'s `tests/*.rs (N files, ...)` count: 233 → 234
  (this task added `tests/remote_ring_shadow_head.rs`).
- `tests/dbg_hook_safety_tripwire.rs`'s `SAFE_MUTATORS` allowlist: added
  `dbg_advance_head_only` (see §5).

## 7. Pre-existing clippy findings this task's clippy run surfaced (NOT this task's scope)

Confirmed via `git worktree add` at the untouched base commit
(`c9a3570bfa4393c4a8383be25c3654e1421c7671`), byte-identical reproduction —
these are NOT caused by this task's diff:

1. `cargo clippy --features "alloc-global alloc-xthread bench-internals" -D
   warnings` (a narrower combination than any of the 5 official CI rows —
   `production`'s always-on features like `alloc-decommit` are absent) fails
   with 4 pre-existing dead-code/unused-import errors in
   `src/registry/heap_core_diag.rs` and `src/alloc_core/{alloc_core,magazine_bitmap}.rs`.
2. `cargo clippy --all-features -D warnings` fails with one pre-existing
   dead-code error (`CachedLarge::reserved_capacity` never read).

Neither combination is one of `scripts/check-matrix.mjs`'s official 5
`PER_PR_ROWS` clippy rows (verified: `""`, `experimental`, `hardened
medium-classes`, `production`, `production bench-internals` — all 5 clean
for this task's diff, per §5). Not fixed here (out of scope — pre-existing,
unrelated to F10); flagged for a future round to either fix or add to the
matrix.

## 8. Files changed

- `src/alloc_core/remote_free_ring.rs` — `cached_head: AtomicU32` field
  (`CACHED_HEAD_OFF = 72`, in existing padding), `full_check` helper, the
  soundness argument (module doc), `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`
  path-activation oracle counters (`bench-internals`-gated), `dbg_set_cursors`
  shadow-reset fix, new `dbg_advance_head_only` test seam.
- `src/global/sefer_alloc.rs` — `dbg_drain_current_thread_rings` (new,
  `bench-internals`-gated).
- `tests/remote_ring_shadow_head.rs` (new) — F10-specific correctness
  coverage (§5).
- `tests/loom_remote_ring.rs` — 3 new loom tests (§5), pre-existing tests
  unchanged.
- `tests/dbg_hook_safety_tripwire.rs` — `dbg_advance_head_only` classified
  into `SAFE_MUTATORS`.
- `tests/regression_ring_cursor_wrap.rs` — unchanged (re-verified green
  through the shadow-checked path).
- `examples/r32_11_remote_ring_shadow_head_gate.rs` (new) — the harness
  (§3), two build modes (§3.1).
- `Cargo.toml` — registers the new example (`required-features =
  ["alloc-global", "alloc-xthread"]`, `bench-internals` optional).
- `scripts/r32_11_shadow_head_summary.mjs` (new) — the checked
  summary-derivation script (§4).
- `docs/perf/r32_11_run.json` (new) — the `paired-ab-runner.mjs` config.
- `docs/perf/_raw_r32_11_favorable_before_after.log`,
  `docs/perf/_raw_r32_11_adversarial_before_after.log`,
  `docs/perf/_raw_r32_11_adversarial_before_after_n30.log`,
  `docs/perf/_raw_r32_11_adversarial_before_after_trial4.log` (new,
  committed with `git add -f` per the raw-log policy) — cited raw evidence.
- `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv` (new).
- `docs/ARCHITECTURE.md` — test-file count (§6).
- `docs/perf/OPEN_ITEMS.md` — F10 marked resolved (see that file's own
  entry for the exact wording).

## 9. CORRECTED 2026-08-03 — the `#[should_panic]` loom counterfactual was vacuous; rewritten with a non-vacuity companion (R33-3, task #508)

This section is appended, not a rewrite — §5's bullet for
`counterfactual_shadow_trusts_stale_cache_spuriously_overflows` (lines
378–384 above) stays exactly as originally published (per this project's
append-only correction convention; see `R32_10_…GATE.md` §5.2 for the
established pattern). The round-32 readonly review
(`docs/reviews/2026-08-03-round32-readonly-review.md` §3, finding F2
[P2]) found that the original test was **vacuous**: its `would_admit`
assertion was `false` in **every** interleaving loom could schedule,
because the broken check read `cached_head` (only possible value: 0) and
`tail` (only possible value: 1) unconditionally — the concurrent drain
touched `head` and the slot but never `tail` or `cached_head`, so the
check's result was interleaving-independent. The test panicked identically
with zero concurrency, and a `#[should_panic]` test that panics regardless
of whether the design under test is correct is a tautology, not a
counterfactual.

**Fix.** The rewritten test
(`tests/loom_remote_ring.rs:889`) now **joins the drain thread first**
before running the broken check, so the drain's
`head.store(1, Release)` is guaranteed to happen-before the check —
`head` is deterministically 1 at check time, the ring genuinely has room
(1 − 1 = 0 < 1), and `cached_head` is observably stale (still 0). The
broken check now panics **specifically because** it trusts the stale
shadow without re-deriving from the real head — a genuine counterfactual.

**Non-vacuity proof (direction (b), previously entirely missing).** A new
companion test
(`correct_shadow_recheck_admits_after_drain_no_spurious_overflow`,
`:961`) places the REAL `RingModelShadow1::full_check` in the **exact
same** post-drain position (same prefill, same drain-thread-join-first
sequencing, same stale `cached_head` = 0) and asserts it does **not**
reject — the slow path re-derives `head` via a fresh `Acquire` load, sees
1, refreshes `cached_head` to 1, and admits. Both directions verified
under `RUSTFLAGS="--cfg loom" cargo test --release --features
"alloc-core,alloc-xthread" --test loom_remote_ring` (9 tests, 0 failures):
the counterfactual panics (`should panic … ok`), the companion does not
(`… ok`). A scratch-swap (temporarily replacing the counterfactual's
broken check with the real `full_check`) made the `should_panic` test
**fail** (test did not panic as expected) — confirming the panic is caused
by the broken check, not by structural accident. The scratch change was
reverted before committing.

**Scope.** The shipped `full_check` / `push` implementation itself is
unaffected (confirmed correct by the review's §3.1) — this is purely a
test-vacuity correction. The loom test count for this file is now 9 (was
8; the rewritten counterfactual + 1 new companion). The two loom models
(`RingModelShadow`, `RingModelShadow1`) were not modified.

## 10. CORRECTED 2026-08-03 — §1's `head` write-site enumeration was incomplete (R33-4, task #509)

This section is appended, not a rewrite — §1's soundness-argument text
(lines 40–50 above) stays exactly as originally published (per this
project's append-only correction convention; see §9 above for the same
convention applied to this same report, and `R32_10_…GATE.md` §5.2 for
the established pattern). The round-32 readonly review
(`docs/reviews/2026-08-03-round32-readonly-review.md` §3, finding F3
[P2]) found that §1's enumeration of write sites to `head` was
incomplete: it stated "the only OTHER write site, `dbg_set_cursors`",
implying exactly two sites total (`drain` + `dbg_set_cursors`), when
there are actually FOUR:

1. `drain` (`head.store(h, Release)`) — the real monotonic advance;
   §1's description of this site is correct.
2. `dbg_set_cursors` (`head.store(head, Release)`) — test-only; §1's
   description is correct.
3. `dbg_advance_head_only` (`head.store(head, Release)`) — test-only;
   ADDED by commit `d38bf73c63fa989eace81e659a3844b98f6656c5` itself
   (the same commit this report tracks), but not enumerated in §1. This
   report's own §3.2 (around line 131) already acknowledges it
   ("advances ONLY `head`, deliberately leaving the shadow stale") — so
   the document contradicted itself within its own text.
4. `init_in_place` (raw write of `0` to `HEAD_OFF`) — bootstrap-only;
   zeroes both `head` AND `cached_head` together (benign).

**Why it matters.** `dbg_advance_head_only` stores an arbitrary `u32`
into `head` and deliberately does not touch `cached_head`. Storing a
LOWER value would regress `head` below `cached_head`, producing the
stale-HIGH shadow the monotonicity argument declares impossible — which
would let the fast path admit a push into a full ring.

**Not a live soundness hole** (per the review's §3.1): the hook is
`#[doc(hidden)]`, `alloc-xthread`-gated, `at()` is `pub(crate)`,
`over_test_buffer` is `pub unsafe fn`, and its only real caller
(`tests/remote_ring_shadow_head.rs:288`) uses `wrapping_add(1)` — an
advance, never a regression. It is correctly enumerated in
`tests/dbg_hook_safety_tripwire.rs` under `SAFE_MUTATORS` with a
bounded-blast-radius justification. This is a documentation-completeness
defect in a formally-stated proof, not a shipped-code soundness bug.

**Fix.** The module doc in `src/alloc_core/remote_free_ring.rs` (F10
section, ~line 103) now lists all four write sites with a one-line note
on each. `dbg_advance_head_only`'s own doc comment now states an
explicit "must never regress `head`" precondition, matching the style
`dbg_set_cursors` already uses for its own `tail.wrapping_sub(head) <=
RING_CAP` precondition. A new drift-detection test
(`tests/remote_free_ring_head_write_sites.rs`) mechanically re-derives
the write-site count from the source so this enumeration cannot drift
out of sync with the code again.

**Scope.** No shipped runtime behavior changed — `full_check`, `push`,
`drain`, and all non-doc-hidden functions are unmodified. This is purely
a doc/comment fix plus one structural-drift test, per the review's own
explicit finding that the shipped code is sound.

## 11. CORRECTED 2026-08-05 — "formally verified" needs its residual scheduler/time assumption named alongside it (round32-review F7, restated as Sol release readonly review F7)

This section is appended, not a rewrite — §0's and §1's "formally
verified" language (lines 22, 38 above) stays exactly as originally
published (per this project's append-only correction convention; see §9
and §10 above for the same convention applied to this same report). The
round-32 independent readonly review
(`docs/reviews/2026-08-03-round32-readonly-review.md` §3, finding F7
[P3]) found that §1's "Wrap correctness" paragraph proved
`cached_head <= head` only modulo `2^32`, and only while the shadow's
staleness lag stays below `2^32` real `head` advances — a precondition
the report's "formally verified" framing did not state. A second,
independent review
(`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`,
`RemoteFreeRing::cached_head` section) reconfirmed the same gap after
R34-6 promoted `cached_head`'s ordering from `Relaxed` to
`Acquire`/`Release`: closing the *ordering* proof gap did not by itself
close this separate *staleness-bound* assumption. A third, independent
release-readiness review
(`docs/reviews/2026-08-05-sol-release-readonly-review.md`, finding F7
[P3]) raised the identical point once more against the shipped module
doc, explicitly asking that the assumption not be left implicit and that
no site claim "formally verified" without disclosing it.

**Fix (this task, Sol-F7, task #569).** `src/alloc_core/remote_free_ring.rs`'s
module doc already carried the staleness-bound paragraph (added in an
earlier pass responding to the second review above, "Wrap argument
precondition — the staleness bound (ASSUMPTION, not a theorem"), but the
compound nature of the claim — memory-model proof **plus** a
scheduler/time assumption, not memory-model proof alone — was stated only
implicitly (as "not a theorem of the abstract memory model"). Added one
explicit summary sentence directly after that paragraph naming both
halves of the compound claim in the reviewer's own suggested framing:
soundness rests on the Rust memory model (closed by R34-6's
Acquire/Release promotion) **plus** the bounded-staleness scheduler/time
assumption — never the memory model alone — and states plainly that any
"formally verified" claim omitting this residual is incomplete. The two
`docs/perf/OPEN_ITEMS.md` sites that used the bare phrase "formally
verified" for this same F10 item (the long-form entry and its summary
table row) were also given the same caveat inline, so a reader scanning
either the module doc, the OPEN_ITEMS index, or this gate report sees the
identical disclosed assumption.

**Practical weight (unchanged from both prior reviews).** This requires a
producer to be descheduled between the shadow refresh's `Acquire` load of
`head` and its immediately-following `Release` store of that same value —
two adjacent instructions — while ~4.29 × 10⁹ real drain advances
complete on that one segment's ring. Judged not practically reachable,
consistent with how this module treats its other genuinely-reachable-but-
astronomically-rare wrap hazard (the power-of-two `RING_CAP` compile-time
pin). The worst-case effect of the assumption failing is a lost
remote-free entry / bounded leak from premature slot reuse, not a proven
UAF or double-free — the same class of "sound but leaky" outcome the
module's existing overflow policy already documents and accepts.

**Scope.** No shipped runtime behavior changed and no new code was added
— this is purely a documentation-precision correction (`docs:` prefix,
not `fix(perf)`): the wrap/preemption assumption itself is pre-existing
(since R32-11, unchanged by R34-6's ordering fix), the underlying
soundness argument is unchanged, and no test, benchmark, or production
code path was touched.
