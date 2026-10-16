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

**Card count:** 11 numbered records (8 open; 3 retained closed/resolved); item 188 is CLOSED and not a current record here (`RESOLVED.md`).

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

18. **[T, filed 2026-08-04, R34-2/task #521] Historical filing snapshot:
    Kani covered the Node seam and experimental hand protocol but had not yet
    proved two then-current pure ring/packing properties**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md` G4 [low]).
    At that filing, `src/kani_proofs.rs` covered `alloc_core::node` primitives
    and `concurrent::hand` (the research tier). The two properties identified
    then were: (a) the ring's wrap arithmetic — that `t.wrapping_sub(h) <
    RING_CAP` was an invariant of the push/drain pair across the
    `u32::MAX → 0` boundary; and (b) `pack_entry`/`unpack_entry` (both
    hardened and non-hardened packings) round-tripped and never produced
    `RING_SLOT_EMPTY` over the full real input ranges. Both were pure arithmetic
    with no pointers and were then protected only by unit tests plus
    `const _: () = assert!` on the *bounds*, not on the *round trip*.

    **Status: RESOLVED (2026-08-06, task #611/K16, commit `772b36d`).**
    Closure retained; the following proof counts and execution receipts describe
    the **historical 2026-08 tree**, not current coverage. Both
    (a) and (b) then had real, verified Kani proofs in `src/kani_proofs.rs`:
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

    **Historical wiring correction in the same task**: Kani had NEVER been
    wired into any CI job before this — the 13 pre-existing proof harnesses
    in `src/kani_proofs.rs` (`node_proofs`, `hand_proofs`, `pack_proofs`)
    were never continuously re-verified either, only run by hand at
    authoring time. Added a new `kani` CI job running all 19 harnesses
    (13 pre-existing + 6 ring harnesses) per-PR — **historical runtime
    measurement** of ~30s total, comparable to that workflow's miri jobs;
    neither this timing nor that 19-harness inventory is a current receipt.

    **Current snapshot (2026-10-11):** `src/kani_proofs.rs` declares
    **16 = 9 Node local-buffer + 2 vacant AtomicSlot + 3 Large + 2 saturation**
    harnesses. This is NOT a 16/16 verification receipt. All five new harnesses
    locally verified SUCCESS with cover 1/1 SATISFIED, including the Large
    transition relation after the owner's explicit word-change oracle decision
    (item 205). M-K2/3/4 each FAILED and restored SUCCESS; the CAS success-to-change
    direction is pinned by an existing native test with a RED return-true mutant.
    Existing Node/hand harnesses were retained, not rerun here.
    Node caller contracts and concurrent ordering/reclamation remain unproved.
    The main `.github/workflows/ci.yml` Kani job now selects
    `alloc-core experimental alloc-global internals`, retaining every harness;
    landing CI acceptance is pending. The duplicated
    standalone Kani workflow was removed. `pack_proofs` was removed (item 167's
    Kani portion only); its Loom mirror remains OPEN.
    Evidence: `src/kani_proofs.rs`, `.github/workflows/ci.yml`; item 205's
    local execution/model-diagnostic receipts and native exhaustive oracle.

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

167. **[T] loom/kani-модели `tagged-index-stack` не имеют production-имплементора: покрыт протокол крата, а не поставляемого кода (R12-08, вариант (а)).** (Filed 2026-10-06, fxx round 12, §5.6.)

    - **Status:** PARTIAL / OPEN — Kani `pack_proofs` removal DONE (2026-10-11); the verification-only Loom mirror remains. No shipping registry stack coverage is claimed.
    - **Current-number-or-verdict:** `tagged-index-stack` is outside the `production` graph (`[target.'cfg(any(loom, kani))'.dependencies]`, commit `ea47a10c`). `pack_proofs` is removed from `src/kani_proofs.rs`; `StackHead`/`StackStorage`/`StackOps` in `src/registry/bootstrap/loom_shim.rs` remain. Prior Loom run receipts are historical; no Loom rerun was performed here.
    - **Next trigger:** a Loom-job/mirror edit or return of the stack to the registry: complete variant (b) by removing the unused Loom stack mirror if it is still not shipping. New real-protocol proofs and their model blocker belong to item 205, not this card.
    - **Evidence:** `docs/reviews/2026-10-06-063308-src-review-fxx-round-12.md` R12-08 и §5.6; `src/registry/bootstrap/loom_shim.rs`, `src/kani_proofs.rs`; `Cargo.toml` (`[target.'cfg(any(loom, kani))'.dependencies]`).

171. **[T, filed 2026-10-06, R14] Miri rejects the existing experimental `EpochRegion` path while initializing `crossbeam-epoch 0.9.20`'s default collector.**

    - **Status:** OPEN — observed interpreter failure; whether this is a project/dependency aliasing defect or a model/toolchain incompatibility is not established. No failure suppression or false-positive classification.
    - **Current-number-or-verdict:** The pre-existing `tests/epoch.rs::single_threaded_sequence_matches_reference_model` fails under current `nightly-2026-10-06` Miri (rustc `1.101.0-nightly`, `ea137335b`) with Stacked Borrows UB in `crossbeam_epoch::internal::Local::element_of` (`internal.rs:562`), reached through `Local::register` → `Global::try_advance` during `epoch::pin()`. Exact command: `cargo +nightly-2026-10-06 miri test --locked -j 2 --features experimental --test epoch single_threaded_sequence_matches_reference_model -- --exact`. The same failure class occurred in the R14 late-TLS Miri attempt. Crossbeam also emits an integer-to-pointer provenance warning at `atomic.rs:204`. The project `EpochRegion` source and dependency versions were unchanged by R14. A separate Tree Borrows diagnostic on the older July Miri run also failed, with Crossbeam warning that its integer-to-pointer conversion is unsupported there; it is not evidence of a passing configuration.
    - **Next trigger:** isolate a minimal `crossbeam-epoch 0.9.20` reproducer and determine an upstream-fixed version or supported Miri configuration. Do not `cfg(miri)`-skip the failing behavior or suppress UB; any dependency-version change needs explicit authorization.
    - **Evidence:** `docs/reviews/2026-10-06-src-review-sol-round-14.md`, “Miri residual”; existing test command and crossbeam stack trace recorded there.

186. **[T] Route pin acquisition under shard-lock versus unlink/last-pin release lacks model coverage.**

    - **Status:** OPEN — confirmed static model gap; NOT verified by execution in this phase, not a demonstrated race.
    - **Current-number-or-verdict:** `loom_r11_small_sidecar` starts with registration and producer pins already acquired (`refs = 2`); channel-ordered native lifecycle tests do not model acquisition under the directory shard-lock racing unlink and last-pin drop. Item 17's fallback proof residual and item 162's terminal acceptance remain separate.
    - **Next trigger:** a route-lifetime/model change: model the acquisition/unlink/last-drop schedule, bind it to the real lock/refcount protocol, and review positive plus negative controls before claiming coverage.
    - **Evidence:** `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` R17-CON-04(a); `tests/loom_r11_small_sidecar.rs`, `tests/r6_route_directory.rs`; `src/registry/segment_route/directory.rs` pin increment/drop and locked lookup/remove.
    - **Round 6 (2026-10-15):** R6-CC-8 adds specifics to the gap: `loom_sidecar_bitmap_actual.rs` compiles the real code with `ClassMap::Dense` while production uses `ClassLeaves` with non-loom atomics; the selector models lack the "bump before scan" branch, the `candidate < count` branch, the constructor-OOM branch and the chunk-OOM-recovery branch; `loom_r11_registry_claim::recycle_one` uses `hint.store` where production does `swap`; `loom_sidecar_summary.rs` models the rejected C‴ design; and there are no models for `fallback::LOCK`/`INIT_STATE` or `MaintenanceService::start`. What is needed: an actual-source loom harness for `ClassLeaves`. Tracked as task R6-T53; Evidence: `docs/agent_reviews_round6/concurrency_review.md` R6-CC-8.
    - **Round 6 execution (2026-10-15, task R6-T53, fix round):** the pin-vs-unlink gap is covered by a shadow model: `tests/loom_route_pin_unlink.rs` models acquisition under the shard lock (loom `Mutex` standing in for `ShardLock`) racing unlink and last-pin release; positive and negative share the same `resurrected` oracle, with the lock-free-get mutant `negative_pin_without_lock_resurrects_freed_route` as the negative control. `tests/loom_r6_selector_model.rs` (an extension of the evidence-pinned `tests/loom_r11_registry_claim.rs`, which stays unchanged) mirrors the production pick order with `count`, production `bump_count` (index reservation only), versioned `SaturationHint` certification (snapshot / `try_mark_saturated` CAS), `recycle`'s swap-with-spill, and the `candidate < count` guard with its mutant (`negative_hint_without_count_guard_hands_out_an_unminted_slot`). Certification itself is NOT exercised by this model: with one claimant and one index-reserving thread, every schedule returns before certification (the claimant either wins the bump or scans the single unclaimed slot), and the model has no recycle or claimability publication, so the version never changes. Removing `before == max`, the versioned CAS or certification altogether would not be detected. A model that releases a slot concurrently with a certifying claimant is still owed. `tests/loom_r19_registry_free_set.rs` covers the chunk-OOM slice (failed-index fuel, recovery exclusion, materialisation gate); its mutant oracle is a post-join ownership check so the negative genuinely panics. The r20 `candidate < count` scenario was withdrawn: in `pick_with_hint_spill` the guard's absence is absorbed by `claim_impl`'s `slot_or_none` chunk-OOM path (claim.rs:317-325), which re-fuels the hint while spilling the displaced real hint into the FREE set — no observable false handout exists at that layer; the branch's negative coverage lives in the r11 pick-layer model. `ClassLeaves` is actual-source loom-covered (atomics from `super`), and `tests/loom_sidecar_bitmap_actual.rs` gained `loom_actual_leaves_prepare_race_publish_completed_issue_is_discoverable` and `loom_actual_leaves_two_producers_same_leaf_no_loss_no_dup`. `loom_sidecar_summary.rs` is marked in-header as the rejected C‴ protocol, not current-path coverage. Still open (deliberately): an actual-source model with the real `ShardLock`, models for `fallback::LOCK`/`INIT_STATE` and the `MaintenanceService::start` handshake (std `Mutex`/`Condvar` — needs a swappable lock import), and, if the owner wants it, a demonstrated-loader-cycle justification for priming the exit detector (see item 221's round-6 note).
Item 188 is CLOSED (2026-10-13); it is not a current record here — its closure record is in `RESOLVED.md`.

192. **[T] Hash backshift proptest uses generic insert, not production identity insert.**

    - **Status:** OPEN — confirmed static oracle mismatch; NOT verified by execution in this phase, not a demonstrated hash defect.
    - **Current-number-or-verdict:** `segment_table_backshift_proptest` calls `SegmentHashHarness::insert` → `hash_insert`; production uses `hash_insert_identity`. The harness already exposes the real identity-insert seam, so “no production test seam” is false. Corruption containment is item 191, not this oracle mismatch.
    - **Next trigger:** a hash/proptest edit: drive identity insert through randomized insertion/deletion/backshift and check canonical identity against a reference, with a production-path mutant control.
    - **Evidence:** `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` R17-VER-09; `tests/segment_table_backshift_proptest.rs`; `src/alloc_core/segment/segment_table/harness.rs` insert/identity-insert delegates.

193. **[T] Shipping HeapCore/SeferAlloc lacks a premerge randomized op-stream differential oracle.**

    - **Status:** OPEN — confirmed static coverage gap; NOT verified by execution in this phase, not absence of production tests generally.
    - **Current-number-or-verdict:** per-PR `alloc_core_differential`/`heap_differential` adapt AllocCore below the shipping magazine. `heap_core_ops` fuzzes direct SeferAlloc in one thread, but CI's premerge fuzz step builds only; execution is scheduled/manual. Deterministic/MT tests and item 162's end-to-end acceptance are separate evidence.
    - **Next trigger:** a shipping allocator verification/gate edit: add a bounded HeapCore/SeferAlloc randomized differential with data/lifetime/realloc checks and production-path negative controls; identify the actual premerge execution row.
    - **Evidence:** `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` R17-VER-10; `tests/alloc_core_differential.rs`, `tests/heap_differential.rs`, `fuzz/fuzz_targets/heap_core_ops.rs`; `.github/workflows/ci.yml` fuzz-build/fuzz-run.

197. **[T] Shipping hardened+batch strict-provenance Miri coverage remains absent.**

    - **Status:** OPEN — confirmed static feature-coverage gap; NOT verified by execution in this phase, not an established current UB.
    - **Current-number-or-verdict:** local and CI Miri selectors do not select `hardened` + `batch-api` together. Native hardened-batch rows are not interpreter evidence. R17-UNS-05's generation-table pointer reconstruction was deleted and closed as item 179; this card concerns surviving shipping refill/drain behavior, not that removed path. Item 190 owns runner classification, not this feature oracle.
    - **Next trigger:** a hardened/batch or Miri coverage edit: execute a bounded real shipping refill/drain under strict provenance with explicit feature/path activation and appropriate negative controls; preserve item 164's known-red distinction.
    - **Evidence:** `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` R17-UNS-05 and R18 closure boundary; `RESOLVED.md` item 179; `scripts/miri.mjs`, `.github/workflows/ci.yml`; `src/registry/heap_core/alloc/batch.rs`.

205. **[T] VER-04 real Large/saturation proofs and consolidated Kani CI: local proof/mutant acceptance.** (Filed 2026-10-11.)

    - **Status:** LOCAL proof/mutant COMPLETE / landing CI PENDING — all five new harnesses have local SUCCESS receipts and cover 1/1 SATISFIED. The owner explicitly approved `changed == expected_success` plus `changed => success`, with `ok => changed` pinned natively; the confirmed Kani strong-CAS result-model defect is NOT a demonstrated production defect. No production/model fix or harness exclusion. VER-04b added no native test file (one existing test minimally extended).
    - **Current-number-or-verdict:** 16 declared harnesses = 9 node + 2 hand + 3 large + 2 saturation. Kani 0.67.0 / CBMC 6.8.0, exact features `alloc-core experimental alloc-global internals`: codec SUCCESS 0/51 failed, generation SUCCESS 0/14, stale saturation SUCCESS 0/274 (unwind(5)), fresh saturation SUCCESS 0/149; each cover 1/1 SATISFIED. Current transition SUCCESS 0/277 failed (1 unreachable), cover 1/1 SATISFIED; baseline 30.467s, verification 1.964s. Historical blocker receipt (superseded by the owner-approved oracle): transition FAILED 1/277 (1 unreachable), cover 1/1 SATISFIED at `success == expected_success`. Counterexample: `before=9799832789158068250` (`0x87fffffffffe001a`, Live), `generation=1224979098644758531`, `operation=3` (`cache_consumed`). Production requires Consuming, so expected_success=false. Generated strong-CAS GOTO model conditionally writes only on equality but returns `{previous, 1, nondet}` even on mismatch; `.1` becomes `ok`, hence success=true without changing the word. Printed concrete playback supplied inputs; native playback was not run. Independent native `sidecar_granule_index_exhaustive`: one test, production ClassLeaves/from_leaves, 262144 granules in each uniform/mixed pass, 524288 records, 8192 cuts, 12 boundary rejections; GREEN, `offset / MIN_BLOCK + 1` mutant RED (pending word 0, offset 0: 2 != 1), cp restore + cmp OK, rerun GREEN. The duplicated standalone Kani workflow is removed; main Kani job selects all 16, including the now locally passing transition. Landing CI has not run.
    - **Next trigger:** green Kani on the landing CI SHA; the reviewing session re-executed all five harnesses, the three transition mutants and the native negative control (receipt below). Local proof/mutant completion is not landing acceptance. These five proofs do not verify existing Node/hand harnesses, concurrency, ownership or reclamation; exhaustive native indexing is not a memory-order proof. Exact publication boundary is not proved.
    - **Evidence:** `src/kani_proofs.rs`; `src/alloc_core/large/reservation_state.rs` cache_consumed/transition; `src/registry/bootstrap/saturation.rs` (internals gates only dbg_with_word_for_test, not snapshot/publish_claimable/try_mark_saturated); `tests/sidecar_granule_index_exhaustive.rs`; `.github/workflows/ci.yml` Kani job; `docs/reviews/2026-10-11-solution-research-verification-ver.md` §5.4 and owner decisions Question 8. Historical initial single-harness receipts (actual seconds): codec 4.15, generation 15.49, transition 17.10 FAILED; diagnostic `-Z concrete-playback --concrete-playback print` 33.12 FAILED; stale 12.49, fresh 10.08. Native mutant 2.688s RED, restored 3.969s GREEN. Cargo.lock cmp against external backup and Windows git diff --stat were clean; no repository logs. Initial codec 26.66s SUCCESS had a failed backup wrapper and is not the lock-protected receipt; protected rerun is 4.15s. CLI `--locked` is unsupported by Kani 0.67.0; user-authorized exception with external lock backup/comparison, native cargo remains --locked.
      Mutant receipts (one narrow harness per invocation; actual Kani exits, not wrapper exits): M-K1 `generation >= limit` -> `generation > limit`, `next_generation_never_exceeds_limit` RED exit 1, 1/14 failed at `generation < limit` (`src/kani_proofs.rs:211`), cover 1/1 SATISFIED, 12.511s (CBMC 0.23211366s); external cp restore, source cmp OK and Windows source diffstat empty; same harness GREEN exit 0, 0/14 failed, cover 1/1 SATISFIED, 10.624s (CBMC 0.27152106s).
      Saturation `LAST_VERSION = MAX-1` SURVIVED: stale harness exit 0, 0/274 failed (1 unreachable), cover 1/1 SATISFIED, 12.418s (CBMC 1.0058628s); overflow and unwind checks SUCCESS. External cp restore and source cmp OK, Windows source diffstat empty; restored stale rerun SUCCESS exit 0, 0/274 failed (1 unreachable), cover 1/1 SATISFIED, 9.762s (CBMC 0.59670997s). Failed research expectation: this mutant was expected to cause overflow, but the owner accepts it as safety-equivalent for stale/no-overflow properties, NOT exact-boundary equivalent; no harness expansion is required.
      Proof: `(MAX-2)+2 = MAX = 18446744073709551615` fits u64; `old >= MAX-1` disables publication, so the addition cannot overflow.
      The possible masked result `MAX-1` marks to `MAX` (DISABLED), never false saturated; the fresh harness assumes even `before <= MAX-3`, and exact publication boundary is NOT proved.
      Saturation `(old + 2) & !1` -> `old & !1`: one stale harness RED exit 1, 1/273 failed (1 unreachable), check 12 / assertion.1 at `src/kani_proofs.rs:310`, `after == u64::MAX || (after > before && after & 1 == 0)`; cover 0/1, UNSATISFIABLE; unwind SUCCESS; 8.723s (CBMC 0.73685616s). External cp restore, source cmp OK and Windows source/lock diffstat empty; same stale harness GREEN exit 0, 0/274 failed (1 unreachable), cover 1/1 SATISFIED, unwind SUCCESS, 9.921s (CBMC 0.5742959s). After every mutant/restored invocation, external Cargo.lock cmp OK and Windows `git diff --stat -- Cargo.lock` empty; no lock restoration needed. These historical saturation receipts are retained; current transition/mutant acceptance follows below.

    **VER-04b independent model check and owner-approved oracle:** external minimal crate, one `AtomicU64::new(0)`, `compare_exchange(1, 2, AcqRel, Acquire)`, `assert!(r.is_err())`: Kani 0.67.0 / CBMC 6.8.0 FAILED 1/101 (1 unreachable), actual exit 1, 1.508s total / 0.126s verification. The reviewing session had independently confirmed the same FAILED result on a separate minimal crate. This confirms the result-model defect independently of allocator code, not every detail of the historical GOTO diagnostic. The owner consciously chose `changed = after != before`, strong `changed == expected_success` per operation and `changed => success`; edge/generation/terminal assertions follow `changed`, returned-generation checks apply on change or a correctly rejected API result, not a false-positive CAS success flag. `cover!(changed)` is SATISFIED. The reverse `ok => changed` is pinned by `tests/r6_terminal_large_state.rs::consumed_cache_hit_advances_generation_only_after_reset_phase`, minimally extended for wrong-phase/wrong-generation/repeated rejection with immutable word and exact successful Cached word. Native baseline GREEN 1 passed (4 filtered), exit 0, 69.738s; temporary `transition { true }` without CAS RED 1 failed, exit 101, 34.791s at line 31; cp restore/cmp OK, production diffstat empty, restored GREEN 1 passed, exit 0, 9.487s.

    **VER-04b transition controls:** same narrow `cargo kani --features "alloc-core experimental alloc-global internals" --harness large_reservation_state_transition_relation` for every invocation; actual Kani exits, each cover 1/1 SATISFIED and 1 unreachable. M-K2 Pending->Cached in claim_pending: FAILED exit 1, 1/277, phase assertion (`src/kani_proofs.rs:283`), 8.461s / verification 2.219s; restored SUCCESS exit 0, 0/277, 8.404s / 1.867s. M-K3 omit next_large_generation (reuse same generation): FAILED exit 1, 2/274, expected-change and generation assertions (`:275`, `:284`), 8.019s / 1.913s; restored SUCCESS exit 0, 0/277, 6.966s / 1.886s. M-K4 release_cached source Cached->Live: FAILED exit 1, 1/277, expected-change assertion (`:275`), 9.225s / 1.894s; restored SUCCESS exit 0, 0/277, 10.595s / 3.262s. Every production mutant restored by external cp backup with source cmp OK and empty production diffstat; every Kani invocation had external Cargo.lock cp backup/cmp OK and empty Windows lock diffstat. Native Cargo used --locked; Kani used the authorized lock-comparison exception. No permanent production changes, no new tests, no saturation mutation in VER-04b.

    **Independent re-execution by the reviewing session (2026-10-11, after VER-04b; own receipts, not the worker's):** `cargo kani --features 'alloc-core experimental alloc-global internals'` with the five `--harness` selections and `--output-format terse` (WSL Ubuntu-24.04, Kani 0.67.0): exit 0, `Complete - 5 successfully verified harnesses, 0 failures, 5 total`; fresh 0/149 failed (1 unreachable) 0.159s, stale 0/274 (1 unreachable) 0.452s, transition 0/277 (1 unreachable) 3.44s, generation 0/14 0.072s, codec 0/51 0.190s; each cover 1/1 satisfied; `Cargo.lock` identical to its external backup. Transition mutants, one narrow harness each, production file restored by external `cp` and byte-compared: M-K2 FAILED exit 1, 1/277 (`large_phase(after) == Some(to)`); M-K3 FAILED exit 1, 2/274 (`changed == expected_success`, `large_generation(after) == next_generation`); M-K4 FAILED exit 1, 1/277 (`changed == expected_success`); production diff empty afterwards. Native: `cargo test --locked --features 'production internals' --test r6_terminal_large_state --test sidecar_granule_index_exhaustive`: 5 passed and 1 passed. Native negative control `transition` always `true` (no CAS): `r6_terminal_large_state` exit 101, 3 of 5 failed (panics at `tests/r6_terminal_large_state.rs:31`, `:64`, `:93`; `:31` is the added wrong-phase assertion), restored byte-identical. The worker's wrapper exit 2 (shell `test -eq`) was a script defect, not a verifier result; these reruns used exit 0 wrappers. Dependency future-incompatibility warning: proc-macro-error2 2.0.1, outside this change's scope. Local receipts only: not landing CI, not a 16/16 Kani receipt; exact publication boundary remains NOT proved.


221. **[T] Unconfirmed hypotheses: lazy `RtlDllShutdownInProgress` resolution inside lock waits; abort from `HeapLease::drop` while `panicking()`.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** PARTIAL — R6-T54 static disposition landed (dated execution notes in this card); no runtime witnesses were executed, so the hypotheses are documented, not proven.
    > - **Current:** item 221's dated R6-T54 notes analyze both claims by source reading; no code change was needed. Remaining: targeted witnesses if the owner wants runtime proof.
    > - **Next trigger:** reopen only on a concrete loader-lock wait scenario or evidence that `HeapLease::drop` can abort during unwinding.
    > - **Evidence:** `docs/agent_reviews_round6/concurrency_review.md` R6-CC-9; task R6-T54.
    > - **Round 6 execution (2026-10-15, task R6-T54):** static analysis; no code change landed this round.
    >   (1) Lazy exit-detector resolution (`RtlDllShutdownInProgress`, `src/global/process_exit.rs`): the original premise is narrower than assumed. `bind_slow_tagged`/`bind_slow_tagged_with_config` already call `process_exit_in_progress()` FIRST (`src/global/tls_heap.rs:509`/:526), and that call resolves the address on first use — so on the first-bind path the resolution already happens before any allocator lock is held, and a prime placed there would only duplicate the existing check. A first probe can still land inside a wait when the thread's first exit-check happens elsewhere: the directory's `lock_unless_exiting` closures (`src/registry/segment_route/directory.rs:864`/:896/:930/:961 — reachable for a thread that already holds `fallback::LOCK`, the only permitted nesting, while it spins on a shard lock), the fallback lock's own acquisition check (`src/global/fallback.rs:327`/:536 — the probe runs in the wait loop before the lock is won), and the exit paths (`tls_heap.rs:304`). Whether that residual justifies a prime is an owner decision: a real deadlock needs the loader APIs to block on a lock some thread holds while that thread waits on `fallback::LOCK`, which is not derivable from this code (`GetModuleHandleW` takes the loader's SRW module tables, not allocator locks), and the cached `QUERY` limits repeat lookups once a result is published (threads that race on the unresolved state can each run one lookup). Disposition: documented here instead of priming; revisit only with a concrete loader-blocking scenario.
    >   (2) Abort from `HeapLease::drop` while `panicking()` via `finish_bind` rollback (`src/global/tls_heap.rs` :595/:607/:633): NOT reachable on supported targets today. `claim_impl`'s constructor uses the OS aperture only and never the global allocator (`HeapCore::new` doc, `src/registry/heap_core/core.rs`: "Construction uses the OS aperture only ... never `std::alloc`, upholding M5"), so no nested bind can occur inside an unwinding claim; and `LOCAL`/`GUARD` `try_with` failures require already-destroyed TLS, which cannot happen mid-unwind on the native-TLS targets this crate supports. Residual: a `!target_thread_local` target (see (3)) or any future global allocation inside the claim path — keep the `panicking()` abort guard in `HeapLease::drop`.
    >   (3) Г-1 (`memory_safety_review.md`, TLS without native `target_thread_local`): every target in README §"Target support" (x86_64 linux-gnu/windows-msvc, aarch64 linux-gnu/apple-darwin) has native TLS, and `LOCAL` is a Drop-less `const` `Cell` — no destructor registration and no first-touch heap allocation — so `current_for_alloc` cannot recurse into `SeferAlloc::alloc` there. On non-native-TLS targets the recursion hazard is real, but the README explicitly claims nothing for them ("expected to work, not CI-tested, no claim"); no README correction needed. Residual documented here.

222. **[T] Miri aspect of R6-MS-1: a Miri test on `exact-span-large` where a foreign address in the key window must be rejected with no UB diagnostic (after R6-T22).** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** PARTIAL — source fix/test landed under R6-T22 (PR #35 `27b18893`); the CI Miri-execution proof this card owns is not evidenced here.
    > - **Current:** `tests/regression_r6_t22_foreign_window_key_miri.rs` exists and is selected by `scripts/miri-matrix.mjs` (`exact-span-large`, internals); no green miri-alloc-core CI receipt was inspected in this audit.
    > - **Next trigger:** confirm a green miri-alloc-core CI run executing this case under the specified features; only then close this proof-only item.
    > - **Evidence:** `docs/agent_reviews_round6/memory_safety_review.md` R6-MS-1 (Miri aspect); task R6-T22.
