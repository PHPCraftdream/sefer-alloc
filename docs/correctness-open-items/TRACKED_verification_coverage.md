# Correctness / CI-debt open items -- [T] Tracked tier -- miri / loom / kani proof coverage

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if it is about whether an `unsafe` seam or algorithmic invariant has (or lacks) interpreter/model-checker PROOF coverage (miri, loom, kani) -- distinct from ordinary CI gate wiring (a test that exists but does not run under some job) and from platform empirical verification (real hardware, not a formal tool).

**Card count:** 5.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
17. **[T, filed 2026-08-04, R34-2/task #521] Five tier-1 `unsafe` seams have
    no miri, no loom, and no kani harness — covered by ordinary integration
    tests only (`docs/reviews/2026-08-04-release-stabilization-audit.md` G3
    [medium]).** The five: `global::sefer_alloc` (the `unsafe impl GlobalAlloc`
    itself), `global::fallback` (the `static mut MaybeUninit<HeapCore>` +
    spinlock), `registry::heap_slot` (the single load-bearing `unsafe impl
    Sync` in the crate), `alloc_core::sidecar` (the shared lazily-materialised
    sidecar deref boundary, on the `production` path via
    `alloc-segment-directory` + `class-aware-dirty`), and
    `alloc_core::large_cache_extended`. Additionally, `alloc_core::dirty_by_class`
    has `loom_class_aware_dirty` but per ci.yml's own note that model uses
    hand-rolled `loom::sync` atomics, not the real `PerClassDirty`/`RacyPtrCell`
    types — so the real sidecar deref is unmodelled there too. For a
    stabilization release, adding at least a miri or loom harness to each
    (especially `sidecar`, which is on the `production` path) closes the
    largest remaining verification-coverage gaps.

    **Status: PARTIALLY RESOLVED (2026-08-06, task #606/K11) — 2 of 5 claims
    corrected, 2 real CI-wiring gaps closed, 2 seams remain genuinely
    uncovered (accepted risk, see below).**

    - **`registry::heap_slot`'s claim was already stale when filed.**
      `tests/regression_xthread_thread_free_alias_miri.rs` (its own header
      comment: "`Sync` `HeapSlot`, mirroring W3) is required") already
      exercises the exact `unsafe impl Sync for HeapSlot` this item names,
      under real cross-thread miri, and was already wired into
      `ci.yml`'s `miri-plain` job (line ~973) before this item was even
      filed. No action needed — the original claim was simply wrong.
    - **`alloc_core::sidecar` / `alloc_core::large_cache_extended` — real
      gap, partially closed.** `tests/segment_directory_a5_miri.rs` (R7-A5's
      own miri target) already existed, already passed, and genuinely
      exercises the shared `alloc_core::sidecar::OwnedSidecar` primitive
      (via `os::reserve_directory_sidecar`/`deref_directory_sidecar`, which
      call `sidecar::reserve_zeroed_with`/`sidecar::deref` directly — same
      primitive `large_cache_extended.rs` calls) — but was never wired
      into any CI job, so it never actually ran. Wired into `miri-core`
      as a new step (commit `4dd0624`). Residual gap, explicitly NOT
      closed: this test is BELOW-threshold only (`table.count() < 32`,
      the sidecar never actually materialises) — the test's own header
      comment explains why the full materialised path (reserve, rebuild,
      lookup, set/clear bits, 32+ segments) is impractically slow under
      miri and is instead covered only by NATIVE tests
      (`segment_directory_a1.rs`/`_a2.rs`/`_a3.rs`/`_a5.rs`/`_a5_proptest.rs`).
      The materialised-path `sidecar::reserve`/`deref` calls themselves —
      the actual UB-sensitive boundary — remain unproven under miri.
      Writing a miri-tractable materialised-threshold test (a lower
      test-only threshold, or a direct unit-level `OwnedSidecar` miri test
      that bypasses the 32-segment precondition entirely) is real
      follow-up work, not attempted here.
    - **A second, unrelated CI-wiring gap found and closed in the same
      pass**: `tests/remote_fanin.rs`'s `remote_fanin_miri_minimal_retry_ub_check`
      (a purpose-built minimal miri UB-detection harness for
      `push_with_overflow_retry`'s retry path, per its own doc comment
      "Harness 3: minimal miri UB-detection target") also existed, already
      passed, and was also never wired into any CI job. Wired into
      `miri-core` as its own step (commit `4dd0624`) — kept separate from
      the pre-existing `reclaim_offset_unit` step rather than combined
      with a positional test-name filter, after that combination was tried
      first and found to silently zero `reclaim_offset_unit`'s own test
      out of its run ("0 passed ... 1 filtered out") — the exact
      false-PASS shape `miri-core`'s own header comment already documents
      from a prior incident (a bare positional filter matching nothing
      while still reporting green). Caught before landing, not shipped.
    - **`global::sefer_alloc` (the `unsafe impl GlobalAlloc` boundary
      itself) and `global::fallback` (the `static mut MaybeUninit<HeapCore>`
      plus spinlock) — genuinely zero miri/loom/kani coverage, confirmed by
      direct grep across `src/` and `tests/`, ACCEPTED AS RESIDUAL RISK for
      this release rather than closed.** Both are exercised extensively by
      ORDINARY (non-miri/loom) integration tests (`tests/global_alloc.rs`,
      `tests/global_alloc_mt.rs`, `tests/global_alloc_installed.rs`, and
      indirectly by the whole test suite, since `SeferAlloc` is the
      `#[global_allocator]` under `--features production`) — functional or
      logic bugs in these paths would be caught. What miri/loom
      specifically add beyond that — Stacked/Tree Borrows aliasing
      violations, data races invisible without a memory model, the exact
      class of bug `heap_slot`'s own dedicated test above was written to
      catch for a DIFFERENT boundary — remain unproven here. Rationale for
      accepting this rather than blocking release: (a) `global::sefer_alloc`'s
      own trait impl is a thin TLS-lookup-and-dispatch wrapper (the heavy
      unsafe logic it delegates to — `HeapCore::alloc`/`dealloc` — already
      has substantial miri coverage via `reclaim_offset_unit`,
      `decommit_miri_cycle`, and now
      `remote_fanin_miri_minimal_retry_ub_check` above); (b)
      `global::fallback`'s pre-TLS/post-teardown windows are, by the
      module's own doc comment, rare and effectively single-threaded in
      practice, narrowing the real-world UB surface relative to the hot
      per-thread path. Writing dedicated miri/loom harnesses for both
      remains real, valuable follow-up work, not attempted here — this
      status update is the explicit "record the accepted residual risk"
      resolution K11's own filing offered as an alternative to full
      harness-writing.

18. **[T, filed 2026-08-04, R34-2/task #521] kani proves only the smallest
    seam and a deprecated tier — two highest-value CBMC-reachable properties
    are unproven (`docs/reviews/2026-08-04-release-stabilization-audit.md` G4
    [low]).** `src/kani_proofs.rs` covers `alloc_core::node` primitives and
    `concurrent::hand` (the research tier). The two unproven high-value
    properties are: (a) the ring's wrap arithmetic — that
    `t.wrapping_sub(h) < RING_CAP` is an invariant of the push/drain pair
    across the `u32::MAX → 0` boundary; and (b) `pack_entry`/`unpack_entry`
    (both hardened and non-hardened packings) round-trip and never produce
    `RING_SLOT_EMPTY` over the full real input ranges. Both are pure
    arithmetic with no pointers — ideal kani targets — and both are currently
    protected only by unit tests plus `const _: () = assert!` on the *bounds*,
    not on the *round trip*.

    **Status: RESOLVED (2026-08-06, task #611/K16, commit `772b36d`).** Both
    (a) and (b) now have real, verified Kani proofs in `src/kani_proofs.rs`:
    `ring_wrap_proofs` (2 harnesses, generalising
    `tests/regression_ring_cursor_wrap.rs`'s hand-picked wrap-boundary values
    into an exhaustive proof over every `u32` head and every occupancy
    `0..=RING_CAP`) and `ring_entry_pack_proofs` (4 harnesses: round-trip +
    `RING_SLOT_EMPTY`-never-collides, for both the non-hardened and
    `hardened`-only packings). All 6 verified via a real `cargo kani` run
    (kani-verifier 0.67.0 under WSL2 — Kani does not support Windows at all,
    confirmed: `kani-verifier` fails to even compile under
    `x86_64-pc-windows-msvc`) and one counterfactually confirmed non-vacuous
    (a deliberately injected off-by-one bug was caught as `FAILURE`, then
    reverted and reverified `SUCCESS`).

    **Also discovered and fixed in the same task**: Kani had NEVER been
    wired into any CI job before this — the 13 pre-existing proof harnesses
    in `src/kani_proofs.rs` (`node_proofs`, `hand_proofs`, `pack_proofs`)
    were never continuously re-verified either, only run by hand at
    authoring time. Added a new `kani` CI job running all 19 harnesses
    (13 pre-existing + 6 new) per-PR — measured at ~30s total, comparable to
    this workflow's existing miri jobs.

41. **CLOSED** by task #1057 (dedicated per-PR `aligned-vmem-miri` CI job added). See "Recently resolved" in RESOLVED.md for the full closure narrative.

61. **CLOSED** by task #1057 (same fix as item 41 — the new `aligned-vmem-miri` CI job runs the interpreter, closing this runtime-semantics phrasing of the same gap). See "Recently resolved" in RESOLVED.md for the full closure narrative.

84. **[T, accepted coverage reduction — not a defect] `aligned-vmem`'s miri job loses BOTH of its source-text guards: `granted_huge_reader_enumeration_is_pinned` and `no_borrowed_reservation_escapes_lazy_reservation` are now `#[cfg_attr(miri, ignore)]`, because miri's default filesystem isolation forbids the `std::fs::read_dir` walk of `src/` that both guards are built on.** (Filed 2026-08-19, task #1147. Two waves collided, neither aware of the other: the `aligned-vmem-miri` job was added by task #1057 to close items 41/61, and the two guards were added independently by tasks #1103 and #1104/#1113.)

    - **A round-start reader needs to take NO ACTION on this card.** Both guards still run, and still fail when they should, under every non-miri `cargo test` invocation. What is lost is only their miri-INTERPRETED execution — and miri interpretation was never their purpose: they are pure text scans over `.rs` files, with no memory-model or UB content whatsoever.
    - **Status:** OPEN as an accepted, recorded coverage reduction. Recorded here rather than left in a commit body precisely because that is R22-3's class — a follow-up that reaches no index is lost.
    - **Current-number-or-verdict:** `cargo miri test -p aligned-vmem` and `... --all-features` both complete with both guards reported `... ignored`. **The second guard was MASKED, not absent:** miri aborts on the first failure, so landing SHA `1ed79e96`'s CI log named only `granted_huge_reader_enumeration`; `lazy_reservation_no_borrowed_reservation.rs` fails identically and was found only by isolating the later-ordered binaries. Independently confirmed at filing that these two are the complete set: `grep -rln "std::fs\|fs::read_dir\|std::process\|Command::new\|std::net" crates/aligned-vmem/tests/ crates/aligned-vmem/src/` returns exactly those two files and nothing else.
    - **Counterfactual, re-verified at filing (a guard that cannot fail is not a guard):** perturbing `granted_huge_reader_enumeration.rs`'s `"src/os/unix.rs" => (0, 0, 0, 7)` expectation to `999` produces a red `cargo test` with the real mismatch printed; injecting a bogus name into `lazy_reservation_no_borrowed_reservation.rs`'s pinned method list does the same. Both restored to green.
    - **Host-dependence of the error text (know this before reproducing):** on Linux CI miri reports ``unsupported operation: `opendir` not available when isolation is enabled``; on a Windows host the same root cause surfaces as ``can't call foreign function `FindFirstFileExW` ``. Same sandbox, OS-specific foreign-function name — a reproduction that greps for the Linux string on Windows will wrongly conclude the defect is gone.
    - **Next trigger — option (c), moving both guards to `scripts/`, is the correct permanent home and is NOT blocked by any technical loss.** Explicitly evaluated at filing and found lossless: both guards use exactly one Rust-specific construct, `env!("CARGO_MANIFEST_DIR")`, and are otherwise byte/string scanning; neither uses `cfg!`, macros, or type information, and `scripts/vmem-doc-drift-guard.mjs` already does the identical class of work. It was NOT attempted here because `lazy_reservation_no_borrowed_reservation.rs`'s scanner is a six-class hand-rolled analyser guarding the H1 borrow-leak property, and a subtly wrong port would silently weaken a safety guard — worse than an honestly-recorded `#[ignore]`. That migration needs its own zero-trust review plus a `scripts/check-all.mjs` step and a `ci.yml` row.
    - **Option (b) — `-Zmiri-disable-isolation` on the job — is rejected, and would silently reverse a documented decision.** Task #1057 chose `MIRIFLAGS: -Zmiri-ignore-leaks` over `-Zmiri-disable-isolation` deliberately and recorded why in the job's own comment (`.github/workflows/ci.yml`): the only intentional-leak case is `tests/smoke.rs`'s `leak_zeroed_pages_is_zeroed_and_static`, and that narrower flag sufficed. Widening to disable isolation would weaken the whole job's sandbox for the sake of two text tests.
    - **Evidence:** `crates/aligned-vmem/tests/granted_huge_reader_enumeration.rs` and `crates/aligned-vmem/tests/lazy_reservation_no_borrowed_reservation.rs`, both `#[cfg_attr(miri, ignore)]` with inline comments cross-referencing each other and this card; the `aligned-vmem-miri` job in `.github/workflows/ci.yml` (`dtolnay/rust-toolchain@nightly` + `components: miri`, `MIRIFLAGS: "-Zmiri-ignore-leaks"`, `ubuntu-latest`); CI landing SHA `1ed79e96`'s `aligned-vmem under miri` job log — the single-failure report that triggered this task and that, by aborting, concealed the second guard.
