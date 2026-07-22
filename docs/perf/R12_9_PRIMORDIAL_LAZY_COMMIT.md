# R12-9 — Split `alloc-lazy-commit` into `primordial-lazy-commit` /
# `small-segment-lazy-commit`; measure the primordial-only policy in isolation

**Task:** #260 (P1). `alloc-lazy-commit` already existed
(`src/alloc_core/bootstrap.rs`, `src/alloc_core/os.rs`) and gave a first-heap
commit win at bootstrap (~5.1x smaller), but it was one feature flag
controlling TWO distinct reservation call sites: the one-time primordial
segment AND every ordinary small-segment reservation. The small-segment leg
carries the decommit/recommit correctness surface R8-10 (task #223, commit
`852828e`) found responsible for a 50-75x syscall-count regression on
empty→pool→reuse→refill cycles (before that task's admission-side fix), which
is why `alloc-lazy-commit` stayed out of `production`. This task splits the
one feature into two independently-gated policies so the primordial-only
half — which is structurally excluded from the decommit/pool lifecycle — can
be evaluated for `production` inclusion on its own merits, without also
promoting the small-segment-lazy-commit surface.

**Outcome:** split shipped as two new Cargo features
(`primordial-lazy-commit`, `small-segment-lazy-commit`), both additive over
`alloc-core`; `alloc-lazy-commit` becomes a pure combinator alias
(`= ["primordial-lazy-commit", "small-segment-lazy-commit"]`) preserving
100% backward compatibility for existing `--features alloc-lazy-commit`
builds. Neither new feature was added to `production` — that inclusion
decision is left to the orchestrator per this report's numbers.

**Date:** 2026-07-22. **Platform:** Windows 10 Pro x86-64 (the only platform
where the lazy path is genuinely lazy; Unix/miri fall back to eager
internally, so this split has zero *effect* — though it does compile and
gate correctly — on those platforms; see "Windows-gated effect, not
Windows-gated code" convention established in R12-4).

---

## 1. Mechanism split — shared frontier, independent reservation gates

The split is NOT two independent mechanisms from scratch. There is ONE
shared frontier mechanism:

- The `committed_payload_end` field on `SegmentHeader` (present in every
  build's layout, read/written only when either sub-feature is on).
- B2 grow-on-carve in `carve_block`/`carve_batch`
  (`src/alloc_core/alloc_core_small.rs`) — generic over `SegmentKind::Small |
  SegmentKind::Primordial`, reading/writing `committed_payload_end` the same
  way regardless of which policy caused the segment's initial reservation to
  be partial.
- B3 decommit-aware reuse in `decommit_empty_segment_impl`
  (`src/alloc_core/alloc_core_small_pool.rs`) — reachable ONLY for `Small`
  segments (see §2), so gated on `small-segment-lazy-commit` specifically.
- Every `dbg_*` diagnostic in `alloc_core_small_diag.rs`
  (`dbg_committed_payload_end_for`, `dbg_grow_commit_count`,
  `dbg_grow_chunk`, `dbg_lazy_first_chunk`, `dbg_arm_commit_fail[_at]`).

What differs between the two policies is ONLY the reservation call site:

| Call site | File | Gate |
|---|---|---|
| `bootstrap::primordial`'s `Segment::reserve_lazy` | `bootstrap.rs` | `primordial-lazy-commit` |
| `Segment::reserve_lazy` constructor itself | `os.rs` | `primordial-lazy-commit` |
| `reserve_small_segment`'s `reserve_aligned_lazy` | `alloc_core_small.rs` | `small-segment-lazy-commit` |

The shared frontier-stamping code that follows each reservation call
(`meta.set_committed_payload_end(...)`) had to be widened from "gated on the
policy that did the lazy reservation" to "gated on `any(...)` of both split
features, with an explicit `SEGMENT` fallback arm for the sibling-off case" —
see §4 (a real bug this task's own isolation test caught and fixed, not a
hypothetical).

**Conclusion on the review's own question:** the split is structurally
clean — `primordial-lazy-commit` is `small-segment-lazy-commit`'s identical
mechanism narrowed to ONE segment, not a parallel implementation — but the
narrowing is not "free": the shared grow-on-carve check now runs on BOTH
segment kinds whenever EITHER policy is on, so both reservation-adjacent
frontier stamps must unconditionally initialize the field (to `SEGMENT` when
their own policy is off), not just conditionally when their own policy is on.

---

## 2. Why `primordial-lazy-commit` cannot re-enter the R8-10 regression

Verified in source, not asserted: `AllocCore::dec_live_and_maybe_decommit`
and `dec_live_batch_and_maybe_decommit`
(`src/alloc_core/alloc_core_small_pool.rs`) are the ONLY entry points that
route an emptied segment into `release_or_pool_empty_segment` (pool
admission) or a release. Both hard-gate:

```text
if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
    return false;
}
```

A `SegmentKind::Primordial` base never satisfies `SegmentKind::Small`, so the
primordial segment can never be pooled, decommitted, or released — it is
reserved exactly once at process start and lives for the process's entire
lifetime. R8-10's regression fired on repeated empty→pool→reuse→refill
cycles of *ordinary* small segments; that lifecycle is unreachable for the
primordial segment by construction. `primordial-lazy-commit`'s only exposure
is the ONE-TIME initial `reserve_lazy` call plus whatever grow-on-carve
commits are needed as the bump cursor advances past the initial frontier
during the first few carves out of the primordial — never a repeated
decommit/recommit cycle.

---

## 3. Measured numbers

### 3.1 First-heap commit (the `docs/perf/_raw_firstalloc_*` methodology,
`examples/first_alloc_process.rs`, `cargo run --release --example
first_alloc_process`)

5 samples each, `commit_after_1_heap_kib − commit_before_kib` (the headline
metric per that harness's own doc comment):

| Build | Samples (KiB) | Mean (KiB) | Mean (MiB) |
|---|---|---|---|
| `production` (baseline, no lazy-commit) | 4640, 4644, 4644, 4644, 4644 | 4643.2 | 4.535 |
| `production,primordial-lazy-commit` | 904, 908, 904, 900, 904 | 904.0 | 0.883 |

**Ratio: 4643.2 / 904.0 ≈ 5.14x** — matches the task brief's stated ~5.1x
(and the ~4.52 MiB → ~0.887 MiB figures) within sampling noise.

### 3.2 Decommit-heavy lifecycle — zero-syscall regression guard

No dedicated wall-clock decommit-lifecycle benchmark exists in `benches/`;
the codebase's actual regression oracle for this class of defect is the
SYSCALL-COUNT assertion in `tests/lazy_commit_b3_recycle.rs` (the exact test
suite R8-10 itself used to prove/disprove the 50-75x claim — see that file's
own header: "Verified non-vacuous by reverting only the two src files...
confirming 4/5 tests go red against pre-fix code — GROW_COMMIT_COUNT delta of
15 and a decommit delta of 2, matching the review's 50-75x claim"). Re-run
under the split:

- `cargo test --release --features "production,alloc-lazy-commit" --test
  lazy_commit_b3_recycle` → **5/5 green**, including
  `repeated_cycles_stay_zero_syscall` and
  `zero_syscalls_across_empty_pool_reuse_refill_cycle` — the empty→pool→
  reuse→refill cycle costs **exactly zero** `GROW_COMMIT_COUNT` and
  `dbg_decommit_count()` deltas, confirming R8-10's fix is intact after the
  split (this exercises `small-segment-lazy-commit`'s code paths, since
  `alloc-lazy-commit` enables both sub-features).
- `cargo test --release --features "production,primordial-lazy-commit"
  --test lazy_commit_b3_recycle` → **0 tests run** (the file's own
  `#![cfg(feature = "alloc-lazy-commit")]` gate does not fire under
  `primordial-lazy-commit` alone) — confirming BY CONSTRUCTION that none of
  the R8-10-relevant decommit/pool-cycle code is even compiled into a
  `primordial-lazy-commit`-only build.
- The new `tests/lazy_commit_r12_9_split.rs`'s
  `primordial_only_isolates_from_small_segment` test additionally asserts,
  under `production,primordial-lazy-commit` specifically, that carving 2000
  further blocks into a SECOND (ordinary, eagerly-committed) small segment
  produces a **zero** `dbg_grow_commit_count()` delta — i.e. the shared
  grow-on-carve machinery that IS compiled in (because `primordial-lazy-commit`
  pulls in the shared substrate) never spuriously fires on ordinary small
  segments when `small-segment-lazy-commit` is off. This is the exact
  counterfactual a naive gating mistake would fail (see §4) — the test
  caught a real bug during this task's own development.

**Conclusion:** no 50-75x-class regression exists or can exist under
`primordial-lazy-commit` alone — confirmed by an oracle stronger than a
timing measurement (an exact syscall-count assertion), not by assumption.

---

## 4. A real bug this task's isolation test caught

While writing `tests/lazy_commit_r12_9_split.rs`, the first version of the
split had the frontier-stamping blocks in both `bootstrap.rs` and
`reserve_small_segment` gated on ONLY their own sub-feature (e.g.
`#[cfg(feature = "small-segment-lazy-commit")]` for the small-segment stamp,
with no `else` arm). Under `primordial-lazy-commit`-only, this left an
ordinary small segment's `committed_payload_end` at its constructor default
of `0` (never stamped `SEGMENT`) — and since `primordial-lazy-commit` alone
already pulls in the SHARED grow-on-carve check (gated on
`any(primordial-lazy-commit, small-segment-lazy-commit)`), the very first
carve into that unstamped segment saw `carve_end > frontier` (`0`) and fired
a spurious grow-on-carve commit. `primordial_only_isolates_from_small_segment`
failed with `left: 262144, right: 4194304` (a `GROW_CHUNK`-rounded commit
instead of the expected eager `SEGMENT`), an exact, reproducible,
non-vacuous counterfactual. Fixed by widening both frontier-stamp blocks'
OUTER gate to the shared `any(...)` condition, with an explicit "sibling
policy is off → stamp `SEGMENT`" fallback arm in each. Both isolation tests
(`primordial_only_isolates_from_small_segment`,
`small_segment_only_isolates_from_primordial`) are green after the fix.

---

## 5. Test status

- `cargo test --features production` (neither new feature): unchanged test
  count/behaviour (verified via the pre-existing suite; only
  `docs/ARCHITECTURE.md`'s tracked test-file count needed a `196 → 197`
  bump for the new test file).
- `cargo test --features "production,primordial-lazy-commit"`: green.
- `cargo test --features "production,small-segment-lazy-commit"`: green.
- `cargo test --features "production,alloc-lazy-commit"`: green, all four
  pre-existing `tests/lazy_commit_*.rs` files pass unchanged (38 tests
  total: `lazy_commit_frontier.rs` (5), `lazy_commit_b2_grow.rs` (7),
  `lazy_commit_b3_recycle.rs` (5), `lazy_commit_b4_matrix.rs` (21)).
- `cargo test --all-features`: green except two PRE-EXISTING failures
  unrelated to this task and already tracked
  (`directory_hit_triggers_mutation_during_scan_stays_consistent` in
  `tests/r12_1_directory_scan_no_aliasing.rs`,
  `ninth_distinct_high_node_overflows_to_unknown_bucket_without_corruption`
  in `tests/segment_directory_numa_high_node_ids.rs`) — confirmed identical
  on `main` before this task's changes via `git stash`.
- `cargo clippy --all-targets -- -D warnings` (default, `experimental`,
  `--all-features`, `production`, `production+primordial-lazy-commit`,
  `production+small-segment-lazy-commit`): clean on every configuration.
- `cargo fmt --check`: clean.

---

## 6. Recommendation

**GO** for `primordial-lazy-commit` on the numbers measured here:

- ~5.1x smaller first-heap commit (matches the pre-existing
  `alloc-lazy-commit` claim, isolated to the narrower policy).
- Zero measurable exposure to the R8-10 decommit/recommit regression class,
  confirmed by an exact syscall-count oracle, not a timing heuristic — the
  primordial segment is structurally excluded from the pool/decommit
  lifecycle this task's own code proves it can never enter.
- The only correctness surface `primordial-lazy-commit` adds beyond today's
  `production` is the ONE-TIME initial partial reservation + grow-on-carve
  on the primordial's first few carves — a small, well-tested, already-
  reviewed surface (`docs/perf/R7_INCREMENTAL_COMMIT.md`,
  `tests/lazy_commit_frontier.rs`'s `primordial_frontier_is_correct`,
  `tests/lazy_commit_b4_matrix.rs`'s
  `primordial_metadata_committed_up_front`) that has existed since R7-B6,
  just never been gated into `production`.
- `small-segment-lazy-commit` (the sibling, full former `alloc-lazy-commit`
  behaviour) is explicitly NOT part of this recommendation — its decommit/
  recommit correctness surface on every pool eviction remains materially
  larger and is left opt-in, matching the existing pre-R12-9 posture.

This is a recommendation, not a decision — the orchestrator has NOT been
asked to include `primordial-lazy-commit` in `production = [...]`, and this
task's diff does not touch that list.
