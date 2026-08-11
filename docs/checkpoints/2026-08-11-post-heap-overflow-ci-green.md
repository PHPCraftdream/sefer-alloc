# Checkpoint — 2026-08-11 10:20 [post-heap-overflow-ci-green]

## Session summary

This session resumed from checkpoint `2026-08-10-post-bench-scale-tool-wave.md` and ran an `@oh` release-readiness review of `sefer-region` (`docs/reviews/2026-08-10-sefer-region-release-readiness-review.md`, 26 findings F1-F26 covering the post-F2 identity-redesign state). At user request, I launched a full fix campaign against that review using `@sh` sub-agents (Agent tool, `subagent_type: "sh"`), landing six sequential commits — F1 (no_std build fix via `AtomicUsize`→`AtomicUsize`→**`AtomicUsize`→`NonZeroUsize`... actually region_id widened to usize**), F3+F4 (semver-checks CI comment + publish-collision guard + SyncRegion poisoned-Debug fix), F5+F7 (I6 test/README/miri/fuzz coverage), F6 (perf re-measurement), F8+F9+F10 (stale docs + CHANGELOG), and F12-F19 (low-severity cleanup, including a real design decision — flipping `Handle`'s `Ord` to `region_id`-then-`key` — which I separately validated with an `@ox` consultation before applying its two follow-up refinements: a doc note on `Ord`'s unspecified-order-detail status, and a defensive field reorder).

After the campaign landed, the user asked to push and monitor CI (per their standing "push only on explicit request" rule, now satisfied). The push (`e7c13b2..f044f86`, 22 commits) revealed CI had genuinely never run against most of this work — three real, previously-undetected bugs surfaced: (1) `cargo doc -- -D warnings` doesn't work (cargo doc doesn't forward trailing rustdoc flags the way clippy does — needed `RUSTDOCFLAGS` env var instead), (2) `benches/locality.rs` had an unbound `Region::insert()` call that only became a lint violation after an earlier round added `#[must_use]` to `insert`, and (3) `miri (core)`'s `remote_fanin_miri_minimal_retry_ub_check` failed under `-Zmiri-strict-provenance` — first on a test-file int-to-pointer round-trip (fixed directly by me using the established `SendPtr` newtype pattern), then — after that was fixed and miri progressed further — on a genuinely deeper production-code bug: `HeapOverflow` (the allocator's bounded MPSC second-chance overflow ring, `src/registry/heap_overflow.rs`) stored freed-block segment bases as `usize` in `AtomicUsize` slots, losing/fabricating pointer provenance on every push/drain round-trip. Confirmed this was pre-existing (main was already red on `e7c13b2`, before any of today's work) and unrelated to sefer-region.

For the `HeapOverflow` fix, I first asked the user via `AskUserQuestion` whether to proceed (given it touches hot-path, heavily-documented lock-free allocator internals) — they redirected to `/oh` to ask me directly whether `AtomicPtr<u8>` was the right design, which I confirmed after tracing the full provenance chain (`os.rs::segment_base_of_ptr`'s `map_addr` already preserves provenance; the `AtomicUsize` round-trip was the only lossy hop; `reclaim_offset`/`reclaim_offset_checked` genuinely dereference the reconstructed pointer). The user then said "делай, используй /crush" — I delegated the actual `AtomicPtr<u8>` conversion to a `/crush` session (not `@sh` this time, per explicit instruction), which also updated the two standalone loom shadow-model test files that independently re-implement the ring for model-checking. I personally re-verified every claim from that session (re-ran the reproducing miri test, both broad `cargo test` feature combos with full untruncated output — 292 and 323 tests respectively, 0 failed — the loom suite, clippy, fmt) before committing.

Pushed again (`f044f86..bce871e`). The new CI run came back **green everywhere except one job** — `sefer-region package gates`'s "Guard against publishing an already-taken version" step, which is the F3 fix's own publish-collision guard, correctly and honestly reporting that `crates/region/Cargo.toml` is still `0.1.0` and that version is already live on crates.io. This is not a bug — it is the gate working exactly as designed, blocking exactly the situation (F2, the version bump) that remains gated on explicit user authorization (task #801, Stage E). `miri (core)` is now confirmed green in real CI, not just locally — the `HeapOverflow` fix holds.

No `/babygoal`/`/goal` Stop hook is currently armed (cleared earlier in a prior session window). The babysit cron (`820b2912`, session-only) may still be armed and ticking every ~15 min, correctly reporting "blocked" since all remaining top-level tasks are gated on user decisions.

## Active goal

None.

## TaskList

### in_progress
- #656 sefer-region — verify/prepare for crates.io republish (blocked in practice on #801/Stage E, the version bump)
- #657 numa-shim — verify/prepare for crates.io republish (blocked in practice on #658)
- #658 aligned-vmem — publish 0.2.0 (most release-critical; investigation complete, awaiting user go-ahead)
- #659 racy-ptr-cell — first publish to crates.io (clean, awaiting go-ahead)
- #660 size-classes — first publish to crates.io (clean, awaiting go-ahead)
- #661 tagged-index-stack — first publish to crates.io (clean, awaiting go-ahead)
- #811 Verify CI green on landing SHA after push — effectively DONE (CI is green except the intentional version-bump gate); not yet marked completed, should be closed next session unless there's a reason to keep it open pending the actual version bump

### pending
- #662 Root sefer-alloc: bench-scale-tool design note (done, awaiting user sign-off on 8-workload subset/CI cadence before #763)
- #673 sefer-region: old "[UNVERIFIED — no defect found]" placeholder, candidate for deletion (matching #785's earlier deletion), no action taken yet
- #763 Root sefer-alloc: implement bench-scale-tool per the approved design (informally gated on #662's sign-off)
- #801 sefer-region: Stage E — final release matrix + isolated package verify + tag on clean SHA (blockedBy: all F-campaign tasks, now ALL completed; the only remaining blocker is explicit user authorization for the version bump itself — everything else is ready)

### recently completed (most recent 10)
- #812 Fix HeapOverflow's AtomicUsize→AtomicPtr<u8> provenance bug (pre-existing, blocked miri (core) CI job) — commit `bce871e`
- #810 sefer-region F12-F19 bundle — low-severity cleanup — commit `1fd342b` (+ follow-up Ord-doc-note commit `f044f86`)
- #809 sefer-region F8+F9+F10 bundle — stale docs, CHANGELOG — commit `67fe27b`
- #808 sefer-region F6 — perf re-measurement — commit `57013c8`
- #807 sefer-region F5+F7 bundle — I6 coverage — commit `0c83f14`
- #806 sefer-region F3+F4 bundle — CI comment + publish-collision guard + SyncRegion Debug — commit `2a6e050`
- #805 sefer-region F1 — no_std build fix — commit `3a77e1a`
- #804 sefer-region: @oh release-readiness review — produced `docs/reviews/2026-08-10-sefer-region-release-readiness-review.md`
- #803 sefer-region: F14 remainder (Debug/IntoIterator/Ord/into_inner)
- #802 sefer-region: F2 domain-aware Handle identity redesign

## Decisions

- **Handle<T>'s Ord flipped to region_id-then-key** (was key-then-region_id) — validated via independent `@ox` consultation before accepting; rationale: groups handles by owning Region under BTreeMap/sort(), the old key-first order would interleave handles from different Regions since first-insert-per-Region commonly produces colliding raw keys. Not a semver break (the region_id-bearing Ord shape itself is new/unpublished).
- **Added a doc note + defensive field reorder to Handle's Ord** per @ox's two follow-up recommendations: rustdoc now explicitly states Ord's relative ordering is an unspecified implementation detail (not a promised grouping guarantee), and struct field declaration order was flipped to match comparison order (defensive against a future `#[derive(Ord)]` substitution).
- **HeapOverflow's `bases` field: AtomicUsize → AtomicPtr<u8>**, not the exposed-provenance API route — the latter is explicitly also disallowed under `-Zmiri-strict-provenance` (confirmed from the miri error text itself), so AtomicPtr was the only sound option. Validated via `/oh` consultation on my own reasoning before authorizing the fix, then delegated the actual conversion to `/crush` (not `@sh`) per explicit user instruction.
- **The two loom shadow-model files (loom_heap_overflow.rs, loom_heap_overflow_drain_guard.rs) were updated alongside the production fix** — not strictly required for loom's own correctness (loom doesn't check provenance), but left un-updated they'd model a stale representation, itself a defect.
- **The publish-collision guard's current CI failure is correct behavior, not a bug to fix** — it is deliberately blocking the exact situation (F2 version bump) that remains gated on explicit user authorization.

## Open questions

- **Stage E's version bump** (#801): the only remaining blocker for sefer-region's actual 0.2.0 release — CI is now fully green apart from this gate. Ready to proceed the moment the user authorizes the version bump.
- **The five publish/republish decisions** (#656-661): all investigation-complete, awaiting explicit go-ahead. aligned-vmem (#658) should land before numa-shim (#657) per its dependency requirement.
- **bench-scale-tool root-integration sign-off** (#662/#763): proposed 8-workload subset + weekly CI cadence awaiting approval.
- **Task #673**: old placeholder with no defined action — delete (like #785) or keep as a standing gate?
- **Task #811**: should probably be marked `completed` next session now that CI is confirmed green (modulo the intentional version gate) — left open this session in case the user wants to keep tracking it through the eventual version bump.

## Repo state

```
?? docs/checkpoints/2026-08-09-sefer-region-f2-redesign-pending.md
?? docs/checkpoints/2026-08-10-post-bench-scale-tool-wave.md
?? docs/reviews/2026-08-10-sefer-region-release-readiness-review.md
```
```
bce871e fix(perf): HeapOverflow's bases field AtomicUsize -> AtomicPtr<u8>, closes a real strict-provenance UB hole (task #812)
fe994fe fix(ci): cargo doc doesn't accept trailing -D warnings; benches/locality.rs missing must_use binding
f044f86 docs(region): document Handle's Ord as unspecified-order-detail + reorder fields to match comparison order
1fd342b test, fix(region), docs: low-severity cleanup bundle F12-F19, closes the 2026-08-10 release-readiness review (task #810)
67fe27b docs(region): fix stale generation-saturation paragraph, root crate's stale Handle/audited-slotmap claims, CHANGELOG gap (F8+F9+F10, task #809)
```

origin/main is at `bce871e` (confirmed via `git fetch` + `git rev-parse origin/main`, matches local HEAD — no rebase/force). CI on this SHA: green on every job except the intentional `sefer-region package gates` version-collision guard.
