# 08 — Test quality & verification coverage (read-only audit)

Scope: `tests/` (169 integration test files), `crates/*/tests/`, verification
runners (`scripts/{loom,miri,tsan}.mjs`), `src/kani_proofs.rs`, `docs/INVARIANTS.md`.
Read-only — no source or test files modified.

**Note on repo state at audit time:** `src/alloc_core/alloc_core.rs` and
`src/alloc_core/mod.rs` show as modified (`git status`) with an EMPTY `git diff`,
and four new untracked files exist
(`alloc_core_{large,large_cache,small,small_pool}.rs`) — another agent is
mid-flight splitting `alloc_core.rs` into smaller files. This audit read the
CURRENT on-disk state (post-split) since that is what the tests in `tests/`
compile against; it is out of this audit's thematic scope (AUDIT-3/AUDIT-9
territory) and not otherwise commented on here.

## Summary table

| # | Severity | Location | Type | One-line |
|---|----------|----------|------|----------|
| 1 | Medium | `tests/lazy_commit_frontier.rs:108-115` | устаревшая премисса | Asserts `frontier == SEGMENT` on the whole `not(windows)∨miri∨numa-aware` leg; wrong for `not(windows)∧not(numa-aware)∧alloc-lazy-commit` since R7-B6 |
| 2 | Medium | `tests/lazy_commit_b2_grow.rs:86-90, 311-317` | устаревшая премисса | Same bug, 2 of its 5 `any(...)` sites actually assert the value (others are harmless skips) |
| 3 | Medium | `tests/lazy_commit_b4_matrix.rs:126-131, 1152-1158, 1194-1201` | устаревшая премисса | Same bug, 3 of its 20 `any(...)` sites actually assert the value |
| 4 | Low | `tests/lazy_commit_b2_grow.rs:157-161, 244-248, 439-443`; `tests/lazy_commit_b4_matrix.rs` (15 of its 20 `any(...)` sites) | пропуск | These sites `return` early with no assertion at all on the `not(windows)∧not(numa-aware)∧alloc-lazy-commit` sub-leg — a real coverage gap (not a wrong assertion), silently skipping the scenario the R7-B6 comment says is now reachable |
| 5 | Low | `tests/loom_dirty_publish.rs`, `tests/loom_epoch.rs`, `tests/loom_sharded.rs`, `tests/loom_dirty_multi_segment.rs` | вакуумный (weak) | Zero `#[should_panic]` counterfactuals; the "verified: breaking X makes this fail" claim rests on a doc comment, not a runnable regression |
| 6 | Low | `tests/decommit_soak.rs`, `tests/decommit_stale_ring.rs`, `tests/decommit_miri_cycle.rs` | пропуск | Decommit (M6) is exercised only single-threaded; no real-thread test races decommit against a concurrent remote-free (the two protocols are independently well-tested but never jointly) |
| 7 | Info | `tests/medium_classes_correctness.rs:422` | (cleared) | Sub-agent flagged as weak; verified `dbg_layout_class_for` returns `None` for Large, so `is_some()` IS the correct Small-vs-Large check — not a real finding |

No vacuous/tautological tests, no orphaned loom models, no stale test names in
the `loom.mjs`/`miri.mjs`/`tsan.mjs` matrices, and no proptest under-provisioning
were found. Verification coverage of I1–I6 and M1–M8 is strong overall; the
lazy-commit frontier bug is the standout real finding (scope is 3 files, not the
2 already filed as task #191).

---

## Вакуумные тесты

No fully vacuous (assert-nothing, assert-tautology) tests were found in `tests/`.
Spot checks:

- `assert!(true)`, `todo!()`, `unimplemented!()`, self-referential
  `assert_ne!(a, a)` patterns: zero hits across `tests/*.rs`.
- `tests/registry_basic.rs:306-307` — `fn assert_sync<T: Sync>() {}` inside
  `heap_slot_is_sync()` — looks like an empty test body but is the standard
  Rust compile-time trait-bound idiom (a monomorphization call two lines below
  is the actual check: it fails to COMPILE, not merely fails to assert, if
  `HeapSlot` loses `Sync`). Not vacuous — verified correct pattern.
- `tests/medium_classes_correctness.rs:422` —
  `assert!(core.dbg_layout_class_for(new_layout).is_some())` initially looked
  weak (checks presence, not the specific class), but
  `src/alloc_core/alloc_core_core_diag.rs:392-398` shows
  `dbg_layout_class_for` returns `Some(class_idx)` for `Small` and `None` for
  `Large` — so `.is_some()` **is** exactly the Small-vs-Large assertion the
  test's own comment claims to verify ("300 KiB was Large; now it is a medium
  small class"). Cleared, not a finding.
- `loom_dirty_publish.rs`, `loom_epoch.rs`, `loom_sharded.rs`,
  `loom_dirty_multi_segment.rs` have **zero `#[should_panic]` counterfactuals**
  (see loom-coverage section) — these are positive-invariant-only models whose
  "the naive protocol fails here" claim is asserted in a doc comment
  ("verified: removing the seqlock re-check... makes loom fail here",
  `loom_epoch.rs:118`; "verified by temporarily breaking it", `loom_sharded.rs:29`)
  but never encoded as a runnable regression. This is weaker than the
  established `#[should_panic]`-counterfactual pattern used elsewhere
  (`loom_deferred_large.rs` ×3, `loom_heap_overflow.rs` ×3,
  `loom_magazine_ring_compose.rs` ×8, `loom_remote_ring.rs` ×5, etc. — 9 of the
  13 in-tree loom files DO have live counterfactuals). Low severity: the claim
  is plausible and documented, just not self-verifying on every run.
  **Fix:** add a `#[should_panic]`/`#[ignore]`-gated counterfactual variant to
  these 4 files mirroring the pattern in `loom_deferred_large.rs`, so a future
  edit that silently weakens the seqlock/CAS protocol is caught automatically
  instead of relying on a stale doc claim.

## Пробелы покрытия (I1–I6 + протоколы)

I1–I6 (Region/Handle face) are all solidly covered:

- **I1/I2** (resolution/tombstone) — `tests/region_invariants.rs:26`
  `insert_get_remove_keeps_others_valid`.
- **I3** (no-ABA) — `tests/region_invariants.rs:49`
  `stale_handle_after_reuse_is_none`.
- **I4** (accounting) — `tests/region_invariants.rs:91`
  `clear_invalidates_all_handles` + the proptest oracle in
  `tests/differential.rs`.
- **I5** (drop-once) — `tests/region_invariants.rs:71`
  `drops_each_value_exactly_once` + a drop-counter oracle in
  `tests/differential.rs`.
- **I6** (compaction) — `tests/compaction.rs:88,99,110` (three independent
  assertions: resolution across churn, density, free-list reuse).

M1–M8 (AllocCore) — all have at least one direct test; two are comparatively
thin:

- **M1/M3/M4** — strong (`tests/alloc_core_invariants.rs` `m1_*`/`m3_*`/`m4_*`
  functions cover small/large/zeroed validity, non-overlap, class-fidelity).
- **M2** — strong on the direct case
  (`tests/alloc_core_invariants.rs:80` `m2_double_free_is_noop`); the
  documented **residual limit** (ring↔magazine cross-thread double-free,
  `docs/INVARIANTS.md` M2 note) is pinned by
  `tests/regression_xthread_double_free_residual.rs` and modelled by
  `tests/loom_magazine_ring_compose.rs` — correctly scoped, not a gap.
- **M5** (reentrancy) — strong:
  `tests/alloc_core_reentrancy.rs:93` `m5_alloc_path_does_not_touch_global_allocator`
  runs a counting global allocator over an `AllocCore` workload and asserts
  zero delta; deliberately runs OUTSIDE miri (documented: miri's `os`
  aperture falls back to `std::alloc` as a test-only shim, which would give a
  false positive/negative on this specific claim).
- **M6** (OS return / decommit) — moderate. `tests/decommit_soak.rs`
  (`decommit_fires_and_recommit_roundtrips`, N≈60K churn),
  `tests/decommit_miri_cycle.rs` (bounded miri variant),
  `tests/decommit_stale_ring.rs` (stale-entry-post-decommit rejection) cover
  the bookkeeping and single-thread recommit round-trip well. Two real but
  low-severity gaps:
  - No test races a real decommit against a concurrent remote-free on the
    SAME segment (dirty-segment routing has its own strong real-thread +
    loom coverage in `dirty_segments_a4.rs`/`a5.rs` and
    `loom_dirty_{publish,multi_segment}.rs`, but the two protocols — decommit
    and dirty-routing — are never exercised together under real threads).
  - `os::decommit_pages` (`src/alloc_core/os.rs:261`) returns `()`
    (fire-and-forget `MEM_DECOMMIT`/`madvise`), so there is no fallible
    decommit contract to fault-inject in the first place — the fault-injection
    hook that exists (`aligned-vmem/fault-injection`, consumed by
    `lazy_commit_b2_grow.rs`/`lazy_commit_b4_matrix.rs`) is scoped to
    **commit** failures only, which is the operation that actually has an
    OOM-relevant fallible contract. Downgraded from "gap" to informational —
    verified there is nothing to test here.
- **M7** (owner routing) — thin but adequate:
  `tests/alloc_core_invariants.rs:186` `segment_of_finds_our_segment_base`
  checks the O(1) `segment_of` arithmetic; the actual cross-thread reclaim
  half of M7 is covered by the xthread regression/TSan/miri-PLAIN tests
  (`regression_xthread_large_free_no_leak`, etc.), so the invariant as a whole
  is not under-tested, just split across files.
- **M8** — carried by I3's tests on the Handle face per its own definition;
  no separate M8-only test exists, which matches the invariant's own wording
  ("I3 carried onto the segment substrate").

Cross-thread protocol loom coverage (`scripts/loom.mjs` `FEATURES` map
cross-referenced against `src/` grep for `RemoteFreeRing`, `HeapOverflow`,
`dirty_segments`, `thread_free`/`remote_free`): every shipped inline
cross-thread protocol has a loom model. `RemoteFreeRing` →
`loom_remote_ring.rs` + `loom_remote_ring_drain_guard.rs`; `HeapOverflow` →
`loom_heap_overflow.rs` + `loom_heap_overflow_drain_guard.rs` +
`loom_overflow_first_retry.rs`; `dirty_segments` →
`loom_dirty_publish.rs` + `loom_dirty_multi_segment.rs`; `thread_free` →
`loom_thread_free.rs` + `loom_xthread_protocol.rs`. No orphaned shipped
protocol found without a model.

All three sanitizer/interpreter runner matrices
(`scripts/{tsan,miri,loom}.mjs`) were cross-checked against the actual
`tests/*.rs` file list (and `crates/*/tests/*.rs` for the `crate:`-prefixed
loom entries) — every referenced test name resolves to a real file. No stale
names (the tsan.mjs `heap_cross_thread` staleness from task #188 is already
fixed and stays fixed).

## loom vs shipped (Урок #174)

No orphaned models and no un-modelled shipped protocol found. The
CRATE-P4-followup NO-GO decision (`d062798`, task #187) is correctly reflected:
the in-tree `RemoteFreeRing`/`HeapOverflow` did NOT get swapped onto the
extracted `ring-mpsc` crate, so `scripts/loom.mjs`'s comment
(lines ~46-53) correctly keeps BOTH the crate's own real-type loom suite
(`loom_ring_mpsc` → `crates/ring-mpsc/tests/loom_ring_mpsc.rs`, additive,
models the crate in isolation) AND all 7 in-tree shadow models
(`loom_remote_ring`, `loom_remote_ring_drain_guard`, `loom_heap_overflow`,
`loom_heap_overflow_drain_guard`, `loom_overflow_first_retry`,
`loom_dirty_publish`, `loom_dirty_multi_segment`) — exactly per lesson #174
("loom-модели нельзя удалять для кода, который всё ещё шипается inline").
Verified present: `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`,
`crates/ring-mpsc/tests/loom_ring_mpsc.rs`,
`crates/tagged-index-stack/tests/loom_aba.rs` — all three CRATE-P3/P4/P7
extractions correctly shipped a real-type loom suite (atomics aliased to
`loom` under `--cfg loom`), and `heap_registry.rs` now genuinely consumes
`tagged_index_stack::TaggedIndexStack` (confirmed via `kani_proofs.rs`'s
`pack_proofs` module, which imports the crate directly and documents the
in-tree `loom_free_slots_aba` shadow model as correctly DELETED — not
orphaned, replaced).

`src/kani_proofs.rs` is small (3 modules: `node_proofs`, `hand_proofs`,
`pack_proofs`) and current: all `pub(crate)`/`pub` internals it references
(`alloc_core::node::Node`, `concurrent::hand::AtomicSlot`,
`tagged_index_stack::TaggedIndex`) exist in the present tree. No stale proof
targets.

## Устаревшие премиссы

**Primary finding (scope larger than the already-filed task #191):**
`docs/checkpoints/2026-07-17-1859.md` and task #191 file this as
"lazy_commit b2/b4 tests assert frontier==SEGMENT on the unreachable
unix∧lazy∧¬numa leg" — but the premise "unreachable" is itself the bug, and
the actual scope is **3 files**, not 2:

Per `src/alloc_core/alloc_core_small.rs:1528-1537` (current on-disk state,
post the in-flight `alloc_core.rs` split), `reserve_small_segment`'s frontier
stamp is:

```text
#[cfg(feature = "alloc-lazy-commit")]
{
    #[cfg(feature = "numa-aware")]
    meta.set_committed_payload_end(SEGMENT);
    #[cfg(not(feature = "numa-aware"))]
    meta.set_committed_payload_end(meta_end + LAZY_FIRST_CHUNK);
}
```

This has **no `windows`/`miri` guard at all** — under `alloc-lazy-commit` +
`not(numa-aware)`, the frontier is understated to `meta_end +
LAZY_FIRST_CHUNK` on EVERY platform, including Unix and miri (the comment
block directly above, lines 1507-1519, and the parallel comment in
`tests/lazy_commit_b3_recycle.rs:465-472`, both explain this is deliberate:
Unix/miri's `reserve_aligned_lazy` physically commits the whole segment
eagerly, but the BOOKKEEPING field is still deliberately understated). `Cargo.toml:310-315`
gates `alloc-lazy-commit` as a plain additive feature (not restricted to
`cfg(windows)`), so `cargo test --features alloc-lazy-commit` on Linux/macOS
genuinely compiles and runs the `not(windows)` leg of these tests — it is not
dead code eliminated away, contrary to what "unreachable" in the task title
implies.

`tests/lazy_commit_b3_recycle.rs` gets this right (reference pattern, e.g.
lines 453-489: it splits the `numa-aware`-vs-not case INSIDE the
`any(not(windows), miri, feature = "numa-aware")` arm, asserting
`SEGMENT` only under `numa-aware` and the understated value otherwise). Three
other files repeat the OLD (pre-R7-B6) flat pattern and are factually wrong on
that leg:

- **`tests/lazy_commit_frontier.rs:108-115`** (function
  `fresh_small_segment_frontier_is_correct`) — asserts
  `frontier == SEGMENT` unconditionally for
  `any(not(windows), miri, feature = "numa-aware")`. **Not previously filed**
  under task #191 (only b2/b4 were named) — this is a new instance of the same
  bug in a THIRD file.
- **`tests/lazy_commit_b2_grow.rs:86-90`** (`carve_at_frontier_commits_next_chunk`)
  and **`:311-317`** (inside `carve_batch_one_commit_per_batch`, the
  fault-injection variant) — 2 of its 5 `any(...)` sites actually assert
  `frontier == SEGMENT`/`initial_frontier == SEGMENT`; the other 3
  (`:157,244,439`) are harmless early-`return`s with no assertion (see the
  separate "coverage gap" finding below, not a wrong-assertion finding).
- **`tests/lazy_commit_b4_matrix.rs:126-131`** (`primordial_and_pool_frontier...`-style
  scenario), **`:1152-1158`** (`eager_path_matches_numa_gate`-style, asserting
  "on non-Windows, the segment must be eagerly committed"), and
  **`:1194-1201`** (`eager_path_is_pure_noop`, asserting "non-primordial eager
  segment must have full frontier") — 3 of its 20 `any(...)` sites assert
  `frontier == SEGMENT`/`f2 == SEGMENT`; the other 17 are early-`return`s
  (coverage gap, not wrong-assertion).

**Practical severity:** Medium, not High — this only misfires when
`alloc-lazy-commit` is compiled AND run on `not(windows)` (or under `miri`)
with `numa-aware` off. `npm run check`'s CI feature matrix
(`""`, `--features experimental`, `--all-features`) does not appear to name
`alloc-lazy-commit` explicitly as a standalone matrix entry in `ci.yml` (not
independently confirmed here — out of this audit's file-reading budget to
fully trace `--all-features`'s effective set on a Linux runner), so whether
these 6 assertion sites actually fire red in CI today depends on whether
`--all-features` reaches a Linux/miri runner with this feature on. Either way
the test is asserting a factually wrong value for a real, reachable build
configuration and should be fixed together with task #191, not narrower than
it.

**Fix (all 3 files, 6 sites total):** mirror `lazy_commit_b3_recycle.rs`'s
pattern — split the `any(not(windows), miri, feature = "numa-aware")` arm
into an inner `#[cfg(feature = "numa-aware")]` (assert `SEGMENT`) vs
`#[cfg(not(feature = "numa-aware"))]` (assert `meta_end +
LAZY_FIRST_CHUNK`/`small_meta_end() + LAZY_FIRST_CHUNK`) branch, exactly as
`lazy_commit_b3_recycle.rs:477-489` and `lazy_commit_frontier.rs:58-72`
(the PRIMORDIAL-segment check earlier in the same file, which already got
this right) already do. Suggest widening task #191's scope to include
`lazy_commit_frontier.rs` and enumerate all 6 concrete assert sites (not just
"b2/b4") so the fix is not partial.

**Secondary, lower-severity companion finding — silent coverage gap (not a
wrong assertion):** the 20 remaining `any(not(windows), miri, feature =
"numa-aware")` sites across `b2_grow.rs`/`b4_matrix.rs` that do
`let _ = second_ptr; return;` with no assertion at all are not WRONG, but
they silently skip that scenario's entire test body on Unix/miri/numa-aware,
including the grow-on-carve / fault-injection / recycle scenarios those
functions are named for. Since (per the finding above) the frontier actually
DOES advance lazily on `not(windows)∧not(numa-aware)` now, these `return`s
are leaving real, reachable behavior (grow-on-carve commits firing on Linux
CI runners under `alloc-lazy-commit`) completely unverified on that platform
— every one of B2/B4's scenario-specific assertions (chunk-count, commit-count,
fault-injection-rollback) only ever runs on `cfg(windows)`. This is a
legitimate "M6-adjacent" coverage gap: the grow-on-carve logic changed by
R7-B6 (`alloc_core_small.rs:1528-1537` has no platform guard) is now
Windows-AND-Unix-reachable, but its test suite is still written as if it were
Windows-only.
