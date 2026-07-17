# Follow-up batch review — `1d39e43..8977e88` (#188, #189, #186, #187, #181)

**Date:** 2026-07-17. **Reviewer:** zero-trust follow-up review (fxx), READ-ONLY.
**Scope:** 5 commits, 18 files (`3d25263` tsan-runner, `583cd8f` proc-memstat,
`9d6c9f4` vmem fault-injection, `d062798` ring-mpsc NO-GO doc, `8977e88`
primordial lazy commit). Full diff read line-by-line; bootstrap.rs / os.rs /
segment_header_layout.rs / vmem lib.rs+fault_injection.rs traced against current
sources. No tests re-run (orchestrator already ran production 356/0,
production+alloc-lazy-commit 395/0, first-alloc judge exit 0, vmem
fault_injection 4/0, R7-B4 28/28); this review is the diff and the reasoning.

## Verdict: **SHIP-WITH-FIXES**

No blocker, no high. The #181 bootstrap-safety argument **holds** (see the
verified-correct section — every bootstrap write provably lands inside the
committed prefix). Findings are one medium (flaky-by-design new vmem test file)
and a small tail of lows/nits, none of which corrupts memory or weakens a
shipped invariant.

## Findings table (severity-ranked)

| # | Sev | Where | Finding | Status |
|---|-----|-------|---------|--------|
| F1 | medium | `crates/vmem/tests/fault_injection.rs` (whole file) | 4 `#[test]`s share the process-global `FAIL_NEXT`/`FAIL_AT_*` atomics and run on parallel test threads; each test's `arm_fail_next(0)` "disarm residue" call can disarm/consume ANOTHER test's just-armed hook mid-flight → intermittent false failures | CONFIRMED (by construction; not observed — 4/0 passed this run) |
| F2 | low | `tests/lazy_commit_b4_matrix.rs` `eager_path_is_pure_noop` (inner block), `tests/lazy_commit_b2_grow.rs` `commit_failure_leaves_state_unchanged` (first cfg arm) | Under `(not(windows) | miri) ∧ alloc-lazy-commit ∧ ¬numa-aware` these still assert a NON-primordial segment's frontier `== SEGMENT`, but `reserve_small_segment` stamps the understated `small_meta_end()+LAZY_FIRST_CHUNK` on ALL platforms — the same commit fixed exactly this in b3 (`primordial_and_pool_frontier_matches_reservation_gate` splits numa/¬numa) but left these two | CONFIRMED (latent; pre-existing, half-fixed inconsistently; not reachable in any current CI config) |
| F3 | low | `Cargo.toml:307-313` | `alloc-lazy-commit` force-enables `aligned-vmem/fault-injection` for every consumer build, compiling a `pub`, process-wide armable commit-kill-switch (`aligned_vmem::fault_injection::arm_fail_next`) into non-test binaries; could be scoped to tests via a dev-dependency feature (dev-dep feature unification covers the lib under `cargo test`) | CONFIRMED (design choice; runtime cost negligible — two relaxed loads on grow-commit only, matching the old in-tree hook cost) |
| F4 | nit | `src/alloc_core/os.rs:148-150` vs `bootstrap.rs:94` | `reserve_lazy`'s doc says the caller upholds "non-zero multiple of PAGE and <= SEGMENT via a debug_assert"; bootstrap's `debug_assert!` checks only `<= SEGMENT`. Page-multiplicity does hold by construction (`primordial_meta_end()` is `align_up(_, PAGE)`; `LAZY_FIRST_CHUNK` const-asserted a PAGE multiple) and vmem re-validates and returns `Err` — doc/enforcement mismatch only | CONFIRMED |
| F5 | nit | `tests/lazy_commit_*` (pre-existing, unchanged) | `dbg_arm_commit_fail`/`_at` remain process-global while tests run in parallel: a concurrent test's grow-commit can consume another test's armed fault (same class as F1, and the same class the b2 counter-oracle fix acknowledges). Pre-existing before this range and unchanged by it — noted for completeness | CONFIRMED (pre-existing) |

### F1 detail (the one fix worth making)

`fail_next_forces_exactly_one_real_commit_failure`, `fail_at_fails_exactly_the_kth_real_commit`,
`fail_next_has_priority_over_fail_at`, `zero_arming_is_a_pure_disarm` all begin
with `arm_fail_next(0)` / `arm_fail_at(0)` and then arm-and-fire against the
shared statics. libtest runs them concurrently; e.g. `zero_arming`'s disarm can
land between another test's `arm_fail_next(1)` and its first `commit_range`,
making `assert!(!first)` fail, or one test's commit can consume another's
one-shot. The sefer-side b2 fix in this very batch exists because of exactly
this hazard class. **Suggested fix:** a `static LOCK: Mutex<()>` guard taken at
the top of each test (or `--test-threads=1` for this one binary via harness
config). Non-blocking: the property tested is correct; only scheduling
robustness is at risk.

### F2 detail

`alloc_core_small.rs:1528-1536` stamps `meta_end + LAZY_FIRST_CHUNK` under
`alloc-lazy-commit ∧ ¬numa-aware` with **no platform gate** (comment in b3's
updated test states this explicitly: "the frontier bookkeeping is deliberately
understated" on Unix/miri). So `cargo test --features "production alloc-lazy-commit"`
on Linux (or under miri) would fail `eager_path_is_pure_noop`'s
`f2 == SEGMENT` + `dbg_grow_commit_count() == 0` inner block and
`commit_failure_leaves_state_unchanged`'s `assert_eq!(initial_frontier, SEGMENT)`.
Not in the CI matrix today (Linux CI runs `--all-features`, which turns on
`numa-aware` and makes both true; Windows compiles the blocks out). **Suggested
fix:** apply the same numa/¬numa split b3 got, next time these files are touched.

## Per-commit assessment

### #181 (`8977e88`) — primordial lazy commit: **the bootstrap-safety argument is CONFIRMED**

The absolute question — does every write `bootstrap::primordial()` performs land
inside the committed prefix `[0, primordial_meta_end() + LAZY_FIRST_CHUNK)` —
I traced end to end and answer **yes**:

1. **What is actually committed.** `Segment::reserve_lazy(initial_commit)` →
   `vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit)` →
   (Windows) `win_reserve_commit` which `MEM_COMMIT`s exactly
   `[aligned_base, aligned_base + initial_commit)` **at the aligned base**, not
   the raw over-reservation base (`crates/vmem/src/lib.rs:800-867`) — so the
   committed prefix is measured from the same `base` bootstrap writes against.
   vmem validates `initial_commit` (non-zero, PAGE-multiple, `<= size`) and
   errors otherwise. Unix/miri arms fall back to a full eager commit
   (`reserve_aligned_lazy_raw` → `reserve_aligned_raw`), so understatement
   there is safe by supersets.
2. **Every bootstrap write enumerated against the layout chain**
   (`segment_header_layout.rs` — offsets are a strictly monotone chain):
   header@0 (twice: initial + kind/bump fixup; `size_of::<SegmentHeader>` <
   `page_map_off()`), page map@`page_map_off`, bin table@`bin_table_off`
   (second BinTable footprint reserved-but-unwritten), alloc/magazine bitmaps
   (written under miri only)@their offsets, remote ring
   (`alloc-xthread`)@`remote_ring_off`, generation table
   (`hardened`)@`gen_table_off` — all `< small_meta_end()`; registry slot 0
   @`small_meta_end()`, full hash-table zero-fill
   @`[primordial_hash_off, +HASH_FOOTPRINT)`, free-list array
   @`[primordial_free_list_off, +FREE_LIST_FOOTPRINT)`, free-top u32
   @`primordial_free_top_off` — and `primordial_meta_end() =
   align_up(free_top_off + 4, PAGE)` closes over ALL of them, hardened and
   xthread variants included (`small_meta_end` composes the gen table under
   `hardened`, the X7-Ф3 fix). `committed_payload_end` itself is a
   `SegmentHeader` field (offset < PAGE). **No bootstrap write exceeds
   `primordial_meta_end()`; the whole region is committed before any write
   runs — no incremental commit-alongside-write dance.**
3. **Compile-time pin.** `segment_header.rs:1066-1069`:
   `primordial_meta_end() + LAZY_FIRST_CHUNK <= SEGMENT` under
   `cfg(alloc-lazy-commit)` — correct cfg (`LAZY_FIRST_CHUNK` exists under
   exactly that feature, numa-independent), so `--all-features` compiles and
   future metadata growth fails the build, not the runtime.
4. **First carve and beyond.** `LAZY_FIRST_CHUNK` (256 KiB, PAGE-multiple by
   const-assert) is perf headroom, not a safety dependency: `carve_block`/
   `carve_batch` check `committed_payload_end` **before** any payload write and
   grow via `os::commit_pages` (rollback on failure), and that path is
   kind-generic — it operates on whatever segment `meta` wraps, primordial
   included. The frontier stamp (`meta_end + LAZY_FIRST_CHUNK`) equals the
   reservation's `initial_commit` exactly — never overstates what is committed
   (the fault would be an overstated frontier; here stamp == commit).
5. **No path can un-commit the primordial's prefix.**
   `dec_live_and_maybe_decommit` (`alloc_core_small_pool.rs:89-101`) admits
   only `SegmentKind::Small` — the primordial is never decommit-reset, never
   pooled, never recycled, so its frontier grows monotonically.
6. **Feature gating.** Reservation gate and frontier stamp use the SAME
   `all(alloc-lazy-commit, not(numa-aware))` condition; the three arms
   (lazy∧¬numa → `reserve_lazy` + `meta_end+LAZY_FIRST_CHUNK`; lazy∧numa →
   `Segment::reserve(SEGMENT)` + `SEGMENT`; feature-off →
   `Segment::reserve(SEGMENT)`, no stamp — the field is feature-gated) exactly
   reproduce pre-B6 behaviour off the lazy leg. The union of the two stamp
   cfgs equals the old `cfg(alloc-lazy-commit)` — no configuration lost.
   Eager/feature-off path is byte-identical.
7. **Cross-thread writes** into the primordial land only in the remote ring /
   gen table / registry / hash / free-list (all metadata, always committed) or
   in freed blocks' next-pointers (blocks previously carved, hence below the
   frontier).

**Tests non-vacuous:** the new/updated frontier assertions
(`primordial_frontier_is_correct`, `primordial_metadata_committed_up_front`,
`primordial_frontier_matches_reservation_gate`,
`primordial_and_pool_frontier_matches_reservation_gate`) assert
`frontier == payload_start + dbg_lazy_first_chunk()` with `payload_start` from
the pre-existing, kind-aware `dbg_payload_start_for` (returns
`primordial_meta_end()` for Primordial). Were the primordial still eager the
frontier would read `SEGMENT` (4 MiB ≠ ~0.9 MiB) — the tests fail. The
numa-aware arms assert `SEGMENT`, matching the still-eager path. The
`dbg_grow_commit_count()` assertion removals (b2 `commit_failure_leaves_state_unchanged`,
b2/b4 eager tests) are a **legitimate race fix, not a weakening**: the counter
is a process-global static shared by parallel tests in the same binary
(`fill_entire_lazy_segment` et al. bump it concurrently), while the retained
segment-local frontier equality is the load-bearing oracle for "no successful
commit happened" — a successful commit cannot occur on this segment without
advancing its frontier (`carve_block` line ~1041-1042 orders commit → count →
frontier). The disarm-and-recover assertion is retained.

### #186 (`9d6c9f4`) — vmem fault-injection: correct

- **Production path byte-identical when off:** the hook consult in
  `try_commit_range` is `#[cfg(feature = "fault-injection")]` on both the
  module and the call site; feature-off compiles it out entirely.
- **Faithful port:** `should_fail_commit()` reproduces the deleted
  `COMMIT_FAIL_*` logic exactly — fail-next checked first with early return
  (so a fail-next hit does NOT advance fail-at's counter, same as before),
  fail-at 1-based one-shot with disarm+counter-reset, `arm_fail_at` resets the
  counter, all Relaxed.
- **Same interception scope as before:** the hook fires only in
  `try_commit_range` (sefer's `os::commit_pages` → `vmem::commit_range` is its
  only sefer caller). `reserve_aligned_lazy`'s initial commit goes through
  `win_reserve_commit`'s direct `VirtualAlloc` and `try_recommit` calls
  `recommit_pages_impl` directly — neither consults the hook, exactly matching
  the old sefer-local placement (hook lived in `commit_pages` only). So the
  R7-B4 "k-th commit" numbering is unchanged.
- **R7-B4 tests still inject a REAL fault:** sefer's `dbg_arm_commit_fail(_at)`
  now delegate to `arm_fail_next`/`arm_fail_at`; the grow-on-carve path flows
  os::commit_pages → commit_range → try_commit_range → hook → real
  `commit_range_impl`. `mock` is not enabled anywhere in sefer's dependency on
  aligned-vmem (checked), and the vmem test file itself `#![cfg(...not(mock))]`s
  and proves real committed memory is writable post-fault (non-vacuous).
- **Zero unsafe** in `fault_injection.rs`; no new `allow(unsafe_code)` anywhere
  in the range. (Flaky-risk of the new test file: F1.)

### #189 (`583cd8f`) — proc-memstat: correct

`VmRSS`/`VmSize`/`VmHWM` parsed from one `/proc/self/status` read via
`strip_prefix` + first whitespace-split field; kernel reports these in kB
(KiB) → `* 1024` = bytes, matching the old `VmHWM` conversion. Fallbacks
preserved: unreadable file → `unwrap_or_default()` → empty → `rss`/`commit` 0,
`peak_rss` `None` — same honest-zeros contract. Windows/macOS/stub modules
untouched (diff confined to the linux module + docs). Page-size independence
claim is correct — `/proc/self/status` values are kB regardless of base page
size, unlike `statm` pages × hardcoded 4096.

### #188 (`3d25263`) — tsan runner: correct

Every target in `DEFAULT_TESTS` (`race_repro`, `race_norecycle`,
`global_alloc_mt`, `regression_xthread_large_free_no_leak`) and `PROD_TESTS`
(`global_alloc_mt`, `tls_heap_teardown_ordering_stress`,
`regression_percounter_perheap_aggregation`, `regression_realloc_xthread_stamp`,
`regression_xthread_large_free_no_leak`, `stress_concurrent_boundaries`) has a
matching `tests/<name>.rs` — verified against the tree; no phantom target
remains, and the usage-example comment was also fixed.

### #187 (`d062798`) — ring-mpsc NO-GO doc: reasoning sound, facts check out

Spot-checked every cited source fact: `RemoteFreeRing` uses `AtomicU32`
head/tail + an `overflow: AtomicU32` word + `CURSOR_BLOCK` 128-byte cursor
block with `FOOTPRINT = 128 + RING_CAP*4` (module doc lines 45-64), wired into
`small_meta_end()` which everything downstream chains from; `ring-mpsc`'s
cursors are fixed `AtomicUsize` (`Cursors { head: AtomicUsize, tail: AtomicUsize }`,
`RawStore`) with no overflow word and no padding — the swap really would break
the PERF-PASS-4 cache-line fix and the offset asserts. `heap_overflow.rs`'s
module doc documents the wedge hazard and the sidecar-materialisation-
before-tail-CAS ordering inside push (`ensure_overflow_sidecar`,
`t >= INLINE_CAP` check) — a protocol `MpscRing::push`'s opaque loop cannot
express. All 7 in-tree loom model files exist in `tests/` (`loom_remote_ring`,
`loom_remote_ring_drain_guard`, `loom_heap_overflow`,
`loom_heap_overflow_drain_guard`, `loom_overflow_first_retry`,
`loom_dirty_publish`, `loom_dirty_multi_segment`) — no coverage deleted.
Docs-only commit confirmed (zero code files in its diff).

## Cross-cutting checks — all clean

- **Confined-unsafe two-tier seam:** `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
  → 50 matches, none added by this range (`git diff | grep '^\+.*allow(unsafe_code'`
  → empty). `fault_injection.rs` is zero-unsafe; `bootstrap.rs` stays
  zero-unsafe (its lazy branch calls the safe `Segment::reserve_lazy`);
  `os.rs::reserve_lazy` is safe code inside the existing seam; the new
  `unsafe {}` blocks in the diff are all in `tests/` calling pre-existing
  `unsafe fn` APIs (`dbg_payload_start_for`, vmem `commit_range`) with SAFETY
  comments.
- **No version bumps:** vmem stays 0.2.0, sefer stays 0.3.0; no `cargo publish`.
- **No stray deletions:** `git diff --diff-filter=D --name-only` over the range
  is empty; fuzz corpus untouched.
- **No TODO/placeholder/out-of-scope edits** found in the diff; ARCHITECTURE.md
  not touched; `R7_INCREMENTAL_COMMIT.md` update is consistent with the code
  (headline update accurately describes Option A and the gating).
- **Cargo.toml:** only the documented `alloc-lazy-commit` feature-list change
  (see F3).

## What I verified correct (summary)

- **#181 bootstrap safety: CONFIRMED** — every write `bootstrap::primordial()`
  performs is inside `[0, primordial_meta_end())`, the entire region is
  committed at the aligned base before the first write, the frontier stamp
  equals the actual initial commit, the first/any payload carve is protected by
  the pre-existing kind-generic grow-on-carve check, the primordial is
  structurally excluded from every decommit path, and the compile-time assert
  pins the footprint. Feature-off and numa-aware paths are byte-identical to
  pre-B6.
- **#181 tests: non-vacuous** (frontier equality fails if the primordial stayed
  eager); the counter-assertion removals are a legitimate race fix with the
  load-bearing oracle retained.
- **#186:** faithful port, identical interception scope, cfg-gated to
  byte-identical-off, real-path non-vacuous tests, zero new unsafe.
- **#189:** parsing and unit conversion correct, fallbacks preserved, other
  platforms untouched.
- **#188:** all runner targets exist.
- **#187:** NO-GO reasoning verified against both sources; all 7 loom models kept.
