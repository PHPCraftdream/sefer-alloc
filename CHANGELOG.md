# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] (unreleased)

### Round 34 — 26 tasks addressing findings from three independent readonly reviews (`docs/reviews/2026-08-04-release-stabilization-audit.md`, a UB/soundness/panic-safety/coverage audit; `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`, a Round 32/33 benchmark-methodology review; and `docs/reviews/2026-08-03-round33-readonly-review.md`, whose finding G1 (P2) is R34-1's own source per that commit's body), delegating every task to `/crush` (external CLI, model glm-5.2) with full personal zero-trust re-verification after each one (every diff read in full, every claimed test/clippy/fmt result independently re-run, several measurements independently re-derived from raw data or re-run from scratch, and in the highest-stakes cases a genuine counterfactual check performed personally — temporarily reverting a fix and confirming the associated test fails, then restoring it). "26 tasks" is not a strict 1:1 with "26 review findings": R34-25 and R34-26 are research/design-gate studies (feasibility analyses commissioned this round, not remediations of a specific reviewer-flagged defect), and R34-27 is the round's own closing/CHANGELOG-completion task — both categories sit alongside, not purely inside, the three reviews' finding lists. See `docs/perf/round-manifests/R34_MANIFEST.md` for the round's full commit-by-commit classification against the R30-12 taxonomy (extended 2026-08-05 to cover the full 43-commit round span, task #550) — the mechanical self-check this round's own R34-24 task added specifically so a reader need not open every commit body to confirm no opt-in or measurement-only result is framed as a default speedup.

**Runtime improvements this round: 0.** No shipping algorithm or production default's OBSERVABLE behavior changed; `production`'s feature composition is unchanged (re-verified explicitly by R34-21). The round's `fix(perf)` commits (R34-6, R34-11, R34-14, R34-15, R34-17, R34-18) are all correctness/consistency hardening on production-reachable code paths — memory-ordering promotion, a bounded catch-up loop, field-reset correctness, OOM-handling widening, panic-safety RAII guards, and a compile-time struct-size pin — none claims or measures a speedup. R34-3's `internals` feature is a build-gating/visibility reorganization (no `src/` behavior change), not a runtime change.

#### Measurement, correctness & tooling

- **[process, P0] R34-2 (task #521) — indexed every finding from both reviews into `docs/perf/OPEN_ITEMS.md` and `docs/CORRECTNESS_OPEN_ITEMS.md` per CLAUDE.md's own "round start: check both open-items indexes" convention, before any of the round's 24 work tasks began.** A follow-up self-correction (`00a1c59`) found the task's own crush run had committed the two review-report files themselves out of scope, violating this project's established convention that readonly review reports stay uncommitted local artifacts — untracked with `git rm --cached` (content preserved on disk), the same convention this round's own closing task (R34-27) re-applies to the reports it excludes from "commit all markdown." Commits `b45b824` (indexing) + `00a1c59` (self-correction).
- **[process, P0] R34-3 (task #522) — drew a real public-API boundary, gating `alloc_core`/`global`/`registry`'s module PATHS (not the crate-root re-exports) behind a new opt-in `internals` Cargo feature.** The release audit found this crate's public surface silently included every internal module even without any opt-in feature — a real semver-boundary gap, since `production` never auto-enables `internals`. Verified externally in a standalone test crate: `E0603 module 'alloc_core' is private` without the feature, successful compilation with it. All tests/benches/examples reaching internal modules directly now require `--features "... internals"` in addition to whatever else they needed — a mechanical, 107-file `tests/*.rs` cfg-gate update plus CI/check-matrix/release-workflow sync. Build-gating/visibility reorganization only; no `src/` runtime behavior changed. Commits `27879af`+`b47cc6a`+`0762772` (the module/test cfg-gate + CI sync — the actual substance of the task), plus untagged follow-up `f9ae91f` (a stale test-file-count doc fix after the gate landed).
- **[security] R34-4 (task #523) — closed RUSTSEC-2026-0204 by bumping `crossbeam-epoch` 0.9.18 → 0.9.20 (explicit user-authorized dependency bump, 2026-08-04 session), and removed the advisory's now-stale `deny.toml` ignore entry.** RUSTSEC-2026-0204: `crossbeam-epoch`'s pre-0.9.20 `fmt::Display` impl dereferenced a raw pointer that could be a `Shared::null()`/`Atomic::null()` sentinel (fixed upstream in 0.9.20). The advisory reached this workspace via two paths (`cargo tree -i crossbeam-epoch`, verified when the ignore was first added in `e1ff1e9`): a dev-only chain (`criterion → rayon → rayon-core → crossbeam-deque → crossbeam-epoch`, bench-only) AND a direct optional dependency via `Cargo.toml`'s `experimental` feature (`dep:crossbeam-epoch`, line 117, backing `src/concurrent/hand.rs`'s epoch-reclaimed `Atomic<T>`/`Shared`/`Guard` slot) — the latter is opt-in-only and NOT part of `production`'s default bundle (line 399 lists no `experimental`), so no shipping `production` build ever linked the affected crate. Re-verified this task that this crate's own code still does not trigger the vulnerable path (grepped `src/` for any `fmt::Display`/`format!`/`{}`-style formatting of a `crossbeam_epoch::Shared`/`Atomic` value — none), so the prior `e1ff1e9` ignore was sound for actual usage; the bump closes the advisory properly rather than leaving a perpetual suppression. `cargo update -p crossbeam-epoch --precise 0.9.20` (Cargo.lock: 0.9.18 → 0.9.20, checksum updated); `deny.toml`'s `RUSTSEC-2026-0204` ignore entry replaced with a short comment recording the bump and its task of origin. Verification: `cargo deny check advisories` prints "advisories ok" (RUSTSEC-2026-0204 no longer listed); `cargo build --features experimental` green; `cargo test --features production --no-fail-fast` green. No `src/` change, no `production` feature-composition change, no runtime behavior change — the bump is a dependency-version decision per project convention, authorized explicitly in the 2026-08-04 session (the standing CLAUDE.md prohibition on version bumps without an explicit request is satisfied by that authorization).
- **[verification, P0] R34-5 (task #524) — added the largest missing miri coverage hole: concurrent multi-producer SMALL-block `RemoteFreeRing` push/drain.** Every existing miri regression test drove at most one producer at a time; this adds a genuinely concurrent multi-thread push/drain scenario under strict-provenance checking. Zero-trust review of this task's own delivery caught a real silent-regression bug it did NOT introduce but exposed: `scripts/miri.mjs` and `scripts/tsan.mjs` (local convenience wrappers, NOT the CI workflow definitions, which were already correct) had never been updated when R34-3's `internals` feature landed, so several `--test` binaries silently compiled with their whole module `#[cfg]`'d out and reported "0 tests, PASS" since R34-3 — the exact "pass by absence" class this project has fought before. Fixed with two dedicated follow-up commits. Commits `fd54ddc` (miri test) + `b47a261` (miri.mjs fix) + `91ff1dd` (tsan.mjs fix).
- **[correctness fix, P0] R34-6 (task #525) — promoted `RemoteFreeRing`'s `cached_head` shadow from `Relaxed` to `Acquire`/`Release` in `full_check`, closing R32-11's F-1 ordering-proof gap.** The shadow-head optimization (R32-11) used `Relaxed` orderings for its fast-path check; the audit found this left the "stale-shadow can only ever be more conservative" soundness argument resting on value-domain reasoning alone, without the happens-before edge the pre-shadow design's `Acquire` load supplied. Promoting both accesses to `Acquire`/`Release` restores that edge; on x86-TSO this is assembly-identical (no measurable cost), independently confirmed via the full test suite and all 9 loom tests. A separate rustfmt-drift cleanup (44 hunks across 42 test files, left uncaught since R34-3's mechanical cfg-gate edit) was caught during this task's own `cargo fmt --check` verification step and fixed in the same pass — the exact drift CLAUDE.md's "before every push" rule exists to catch. Commits `a9edc87` (ordering fix) + `7aeee2d` (rustfmt drift).
- **[measurement, P0] R34-7 (task #526) — built this project's first causal subprocess comparative benchmark harness (SeferAlloc/mimalloc/System, real `#[global_allocator]` per binary).** The R32/R33 bench review found in-process cross-allocator comparison non-causal (control arms "regressed" +59%/+71% from host drift alone, not from code changes). The new harness launches one fresh subprocess per allocator so each arm has empty allocator state and no cross-arm thermal/page-cache coupling — inter-run stability improved from the old bench's 50-90% false regressions down to 0.5-4.3% real delta. This harness became the foundation R34-12 and R34-23 both built on. Commit `a3831e5`.
- **[process, P1] R34-8 (task #527) — added a control-arm drift guard to `bench-table.mjs`'s run-over-run appendix.** Flags when a control (non-code-change) arm's run-over-run delta exceeds a 10% threshold, catching the exact host-drift false-positive class R34-7's causal harness was built to work around, in the existing wall-clock table pipeline too. Commit `b23d7c5`.
- **[docs, P1] R34-9 (task #528) — corrected six `benches/global_alloc.rs` label/doc/unit mismatches.** Fixed `Cold-direct` → `Warm bulk burst` (the bench never measures a cold/first-touch path — criterion reuses freed cache/freelist/pool blocks across iterations), `Vec_push` → `manual_realloc_sim` (it never calls `GlobalAlloc::realloc`, motivating R34-23), and `ns/op` → `ns/pair` where the bench times an alloc+free pair, among others — each label now describes what the bench actually measures. Commit `0e29fc2`.
- **[measurement, P1] R34-10 (task #529) — sparse multi-interval decay gate found R32-8's decay-throttle "bounded to one segment" retention claim does NOT hold under sparse traffic.** A gate exercising the throttle across many sparse intervals (not R32-8's single-interval measurement) found a peak 4-segment gap persisting for 27-30 of 40 consecutive intervals — a real, reproducible refutation of a prior round's claim, independently re-run and confirmed. Commits `94e133a` (gate) + `5c1142f` (CSV base_commit off-by-one self-correction).
- **[correctness fix, P1] R34-11 (task #530) — added a bounded catch-up loop (`DECAY_CATCHUP_MAX_STEPS = 8`, a hard-capped `for` loop, NOT a spin-loop of any kind) to `maybe_decay_large_cache`, changing observable large-cache retention policy: final gap reduced 3→1 segment in the measured sparse arm (events=1/interval); peak gap unchanged at 4 segments (stride-bound, not catch-up-bound); ≥3-segment persistence dropped from 95.0% to 72.5% (29/40 intervals) — still the majority of the run.** This is a NEW retention-policy fix, distinct from two things it is not: (1) R32-8's original stride=64 throughput speedup, which is UNCHANGED code and is merely re-confirmed still present in this gate's throughput regime (80.76 vs 249.14 ns/cycle, ~67.6%, consistent with R32-8's original ~61%); and (2) no new throughput speedup — the gate's own §4 states the catch-up loop's body is never reached in the throughput regime (`elapsed < decay_interval` on every read), so the 67.6% figure measures the pre-existing stride optimization, not this task's change. Full data: `docs/perf/R34_11_CATCHUP_DECAY_GATE.md`, `docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv`. Commits `73dceca` (fix) + `a100647` (gate) + `43115cf` (CSV self-correction).
- **[measurement, P1] R34-12 (task #531) — a genuinely clean A/B re-gate of R32-11's RemoteFreeRing shadow-head claim, using a separate git worktree to avoid the known cargo-worktree-binary-reuse trap, CONFIRMS the original claim.** -32.07% in the favorable regime (t=11.125), -3.27% near-full (significant), -0.52% overflow (not significant) — a prior round's win re-validated under a genuinely controlled A/B, not merely re-asserted. Commit `9e70266`.
- **[docs, P1] R34-13 (task #532) — kept `OWN_CACHE_SIZE = 16` (not reverted to 4), backed by real evidence: R32-10's own data shows 0.00% Tier-1 hit rate at cache=4 for every tested K including K=4 itself.** Generalized two hardcoded "(4)"/"& 3" doc comments in `segment_table.rs`/`heap_core_diag.rs` to the parametric `OWN_CACHE_SIZE`/`OWN_CACHE_SIZE - 1` form so this class of doc drift cannot recur silently. Commit `1c686f8`.
- **[correctness fix, P0] R34-14 (task #533) — found and fixed a REAL permanent-leak bug: F12's targeted-write optimization on the large-cache hit path had stopped resetting `deferred_next` to `ABANDONED_TAIL`.** A segment that had gone through the deferred-large-free path and was later reused via a cache hit could carry a stale link value; a subsequent cross-thread free would then have its `push_large_deferred_free` CAS silently fail (mistaken for a double-free), permanently leaking the segment. Fixed by explicitly resetting `owner_state`/`owner_thread_free`/`deferred_next` to their neutral values before `register()`, matching `SegmentHeader::large`'s original constructor exactly (verified byte-for-byte). Also added an exhaustive compile-time field-classification pin (21 fields, no `..`, fails to compile on any future unclassified field) and re-derived `segment_header_views.rs`'s cross-thread-read safety argument for the narrower write shape. A dedicated counterfactual test (`tests/r34_14_deferred_next_reset_on_cache_hit.rs`) was personally verified to fail against the pre-fix code (the fix was temporarily disabled, the test failed with an explicit leak-bug panic message, then restored). This is the round's most significant correctness finding — a genuine defect in shipping code, not a doc/proof gap. Commit `7ef5a46`.
- **[correctness fix, P0] R34-15 (task #534) — widened the free path's chunk-materialisation OOM from `process::abort()` to a graceful `None` return, closing F-4.** `Registry::slot()` was infallible; on chunk-materialisation OOM during a cross-thread free (e.g. `set_dirty_bit_for_segment`, `resolve_heap_overflow`) it aborted the whole process — a transient VM-starvation event killing a process that should have merely dropped that one free. New `Registry::slot_or_none`/`try_ensure_chunk` fallible variants handle this on the free path only (the alloc path's `slot()`/`ensure_chunk` remain infallible with abort preserved, an explicit choice — a fresh registry chunk is load-bearing for a NEW allocation in a way it is not for a free that can simply be dropped). A required manual session restart mid-task found a nearly-complete implementation already in place; resumed rather than restarted from scratch, and the resumed run correctly found and fixed a THIRD affected call site (`owner_slot_is_live`) the original task brief had named only two of. Commit `49929d0`.
- **[docs, P2] R34-16 (task #535) — corrected `sefer_alloc.rs`'s overclaiming "NEVER panics" doc comment, enumerating the five release-surviving invariant tripwires it did not disclose.** The audit found five `assert!`/`.expect()`/`unreachable!()` sites reachable from `GlobalAlloc` under `production` that the doc's blanket "returns null on failure and NEVER panics" claim did not account for. Chose the lower-risk of two options (rewrite the doc, not soften the panics): the auditor could not construct a reachable violation of any of the five ("cannot happen" invariant checks), and softening some but not others would be an unjustified partial mitigation. The doc now enumerates all five by file:line as "abort by design" defence-in-depth, and states explicitly that a panic escaping `GlobalAlloc` is an ABORT via `#[rustc_nounwind]` on current Rust (not UB), independent of any downstream `panic = "abort"` setting. A new `tests/no_panic_doc_accuracy.rs` pins both sides (the five message strings, and the doc's qualifying language) so neither can silently regress; counterfactually verified (reverting the doc edit makes the test fail with the exact F-5 message). Commit `e550006`.
- **[correctness fix, P1] R34-17 (task #536) — added the two missing RAII unwind guards CLAUDE.md's own benchmark-hook precedent (`LockGuard`/`ConflictRollback`) had already established elsewhere: `RemoteFreeRing::drain`'s head-publish, and `fallback::heap_ptr`'s init-state.** Neither had a currently-reachable production panic (`AllocCore::reclaim_offset`/`HeapCore::new` are panic-hardened), so this is hardening, not a live-bug fix — but a `reclaim` closure or `HeapCore::new` unwinding mid-region would previously have left `head` stuck at its pre-drain value (leaking every offset from the panicking iteration onward) or `INIT_STATE` stuck at `INITIALIZING` forever (a process-wide livelock for every other thread reaching `heap_ptr`). Both guards' counterfactual tests were personally verified to fail without the guard (a panicking reclaim closure genuinely leaks offsets without `DrainHeadPublish`; a panicking `heap_ptr` genuinely wedges without `InitStateGuard`, confirmed via a 10-second watchdog timeout). A git-race incident during this session's own review of a LATER task (R34-19) briefly corrupted this task's own follow-up commit; unrelated to R34-17 itself, documented under R34-19's entry below. Commit `c270b0c`.
- **[correctness fix, P2] R34-18 (task #537) — pinned `size_of::<HeapCore>() <= 8192` at compile time, closing F-6's stack-pressure risk.** `HeapCore` is constructed by-value on the frame that triggers a thread's first allocation; Rust does not guarantee RVO, so a debug build or certain backends can place a full copy on a small-stack thread's first-allocation frame. Measured directly (the audit's ~7 KiB figure had been inferred, never measured): `size_of::<HeapCore>() == 7576` bytes under `production`. A compile-time `const _: () = assert!(size_of::<HeapCore>() <= 8192)` (8 KiB, ~8% headroom) now fails the build if this grows materially, mirroring the existing `SegmentHeader` pin pattern; personally verified the pin actually fails the build when the budget is set below the measured size. The optional in-place-initializer refactor (removing the temporary entirely) was evaluated and explicitly skipped: the dominant 6664 B component is `Tcache::new()`, itself returning by value, so eliminating the temporary would require a cascading multi-struct refactor not warranted for a `[low]` finding the pin already bounds. Commit `3281ebc`.
- **[verification, P2] R34-19 (task #538) — added a loom model exercising the F10 shadow fast path over a just-drained (recycled) ring slot, with the drain genuinely interleaved with the producer rather than joined first.** Neither existing shadow loom model reached this interleaving (one never wraps its cursor; the other forces the slow path exclusively by construction). The new model's doc comment is explicit about its limitation, per this project's own R33-3 vacuous-counterfactual lesson: loom's append-only store history cannot surface F-1's abstract-machine modification-order freedom, so this model is a regression-pin on the protocol's value-domain invariants (no offset lost/duplicated, no deadlock), not an ordering proof — that question was separately closed by R34-6's Acquire/Release promotion. Personally counterfactually verified: replacing `full_check` with an always-admit `Ok(())` makes the new test fail. **A git-race incident during this task's own zero-trust review**: I (the orchestrating session) mistakenly believed the crush session had died from a stale "offline" heartbeat and edited the same file it was still actively writing to, running my own counterfactual probe; my temporarily-broken edit landed inside the crush process's own commit when it finished moments later, breaking compilation. Caught immediately on the next test run (not by CI), fixed with a dedicated follow-up commit that also documents the process lesson: never edit a working-tree file while a session's live/dead status is ambiguous — confirm via `sessions why` AND the absence of a fresh commit, not a stale heartbeat alone. Commits `0047cf2` (loom model) + `ae11073` (git-race fix).
- **[process, P1] R34-20 (task #539) — wired the existing, well-designed but never-run ASan harness into CI (per-PR) and added a scheduled weekly `fuzz-run` job that actually executes the three libFuzzer targets `fuzz-build` had only ever compiled.** Measured the ASan job's runtime locally via WSL (~2m44s, compile-dominated, comparable to the existing `tsan` job's own cost) before choosing per-PR over weekly cadence. All three fuzz targets smoke-tested 0 crashes before the weekly job was added. GitHub-Actions-level verification is explicitly deferred to a real push (out of scope for this task per project policy); all CI commands were personally re-run locally via WSL and confirmed passing first. Local smoke-test byproducts (303 generated fuzz-corpus files + a build-artifact directory, never meant to be committed) were found and cleaned up during zero-trust review; a genuinely stale `fuzz/Cargo.lock` (missing an already-added workspace dependency) was found and fixed separately. Commits `db7b30f` (CI jobs) + `36b4b3e` (lockfile sync).
- **[docs, P1] R34-21 (task #540) — fixed `lib.rs`'s unsafe-seam inventory (2 missing modules) and all 4 stale `production`-feature-bundle doc sites, closing F-9/F-10.** `production` has been a 7-feature bundle since R13-9, but `lib.rs`, `sefer_alloc.rs`, and two README sites still described it as 4-5 features; `lib.rs`'s internal seam list separately omitted `alloc_core::sidecar` (on the production path) and `alloc_core::large_cache_extended`. Both drifts went unnoticed for 20+ rounds because nothing pinned them. Fixed the text AND added two new structural pins to `tests/no_stale_doc_references.rs` (a `production`-bundle-vs-`Cargo.toml` parser + a `lib.rs`-seam-list-vs-canonical-grep set comparison), both personally counterfactually verified to fail on the pre-fix state. Commit `ae9c9c3`.
- **[correctness fix, P2] R34-22 (task #541) — swept six current-state doc-drift findings from the bench review; five were already closed by their primary owners earlier this round, and this task closed the last two.** `PerClass`'s F4 layout prose claimed `virgin_mask` sits at offset 1 while its own compile-time const-assert says offset 2 (1 pad byte after `count: u8`, since `virgin_mask`'s `u16` needs 2-byte alignment) — the prose was stale. Fixed and added a new structural pin (`perclass_doc_offsets_match_const_asserts`) parsing the const-asserts as source of truth and comparing against the prose, counterfactually verified. Also brought `docs/perf/OPEN_ITEMS.md` item 34's Status card current (the shared macro-bench harness has existed since R32-9, contradicting a stale "no harness built" status). Commit `b229187`.
- **[measurement, P0] R34-23 (task #542) — built the first real `GlobalAlloc::realloc` and real-`Vec` growth gate suite, and CORRECTED a stale README headline claim it exposed: `realloc_grow_geometric` was published as "~40× faster than mimalloc"; it is actually ~1.8-2.1× faster.** The prior "~9.7 µs" figure is now confirmed physically impossible: the chain's final 2→4 MiB grow exceeds the Large segment's committed span and forces a 2 MiB copy alone taking ~50-100 µs. Independently re-verified (re-ran the exact criterion command under the exact feature set the original figure was sourced from, and separately built + ran the new direct-realloc gate binary) — both reproduce ~210-238 µs, ~24× the published figure, not the published number. `realloc_grow_neighbour_pressure`'s "~1,500× faster" claim was, by contrast, CONFIRMED and even improved to ~3,350×. The `large-reserved-capacity` adaptive-headroom hypothesis was tested and got a NO-GO (independently re-built both binaries with differing hashes, reproduced a ~4× slowdown with the feature enabled — it shrinks the committed span and hurts large-cache reuse more than it helps). New path-activation-oracle counters (`RELOC_INPLACE_LARGE_CALLS`/`_SMALL_CALLS`/`_FASTPATH_DECLINE_CALLS`) prove which code path each realloc actually took. Two README sites the initial correction missed were found during zero-trust review and fixed directly. Commits `19b1918` (harness) + `ba716a0` (gate report + README correction) + `827a57a` (README sweep completion).
- **[process, P1] R34-24 (task #543) — four CLAUDE.md process amendments addressing the round's own scale-growth problem (docs/perf alone was ~73,000 added lines in one prior wave): a hybrid tiered raw-log storage policy (force-add under 200 KiB, truncate/gzip 200 KiB-2 MiB, external CI artifact over 2 MiB), a new per-round manifest artifact making the R30-12 commit-prefix taxonomy self-checking, a formalized current-state-vs-archive structural rule for both OPEN_ITEMS indexes, and a tightened "landing SHA" (not local pre-push HEAD) wording for the post-push CI-confirmation step.** `docs/perf/round-manifests/R34_MANIFEST.md` is the first real instance, classifying all 38 of this round's commits (38 at the time this bullet was written; extended to the full 43-commit round span by task #550/commit `80463d2`, SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `e496d8b` — see "Post-closing independent review remediation" below, F5+F6). Zero-trust review found and fixed one internal inconsistency in the amendment's own text (a "253 of 257" raw-log count that contradicted its own "largest is 145 KiB" parenthetical — independently recounted at 256/256, all under the ceiling) — a real instance of exactly the class of error CLAUDE.md's own R22-14 rule exists to catch, ironically inside the commit introducing new process discipline. Commits `4ba188a` (amendments + manifest) + `9b06b56` (self-correction).
- **[research, docs] R34-25 (task #544) — a design-only feasibility study of a small-magazine provenance scheme for the 16/64 B bulk-burst gap concluded NEED-MORE-RESEARCH, leaning NO-GO.** The candidate's headline lever (caching the segment base to avoid `segment_base_of_ptr` re-derivation) is very likely net-negative on first principles: the function is a single `#[inline(always)]` AND (~1 Ir), and a prior report's "9.03 Ir" attribution was independently re-derived to be a measurement-probe artifact (measured through a non-inlined `dbg_` hook + `black_box` + call-boundary overhead production never pays), not the real inlined cost. The one sound lever (skip the magazine-bitmap clear for never-issued fresh-carve blocks) only helps the COLD first refill of a repeated-bulk-burst workload — every hit from the second burst onward is on a recycled block whose clear is correctness-required (R3) and irreducible under the current architecture. All three prior rejected forms in this exact region (delayed clear, dual bitmap, run-encoded freelist) were read and confirmed not to be re-skinned by the new candidate. No prototype built — the honest, cheaper next step (a disassembly-level instruction count, code-free) is named instead. `docs/design/R34_25_SMALL_MAGAZINE_PROVENANCE_DESIGN.md`. Commit `7758f7a`.
- **[research, docs] R34-26 (task #545) — a design-gate study of a page-run layer for the 256 KiB-2 MiB range (a prior review's most-likely-remaining architectural multiplier) concluded NEED-MORE-DATA, leaning NO-GO: no real consumer of this size range exists anywhere in the project's own workloads.** Confirmed the prior `medium-classes` NO-GO's failure mode was architectural (the carve/grow model, proven by a closed-form LCM argument that a 4 MiB segment cannot support in-place grow for the medium ladder), not "missing size classes" as such — and confirmed a page-run layer with a buddy/run bitmap in a larger (8-16 MiB) arena COULD satisfy the same LCM arithmetic and support in-place grow from the start. But an exhaustive search of every workload/bench/example in the project found none exercising 256 KiB-2 MiB realistically (larson/mstress top out at 8 KiB; R29-5's realistic Vec-growth trace found only 0.054% of allocations ever promote to Large). `docs/perf/OPEN_ITEMS.md` item 3 updated with the in-place-grow angle and an explicit realloc-WIN-not-parity criterion for any future promotion. `docs/design/R34_26_PAGE_RUN_LAYER_DESIGN_GATE.md`. Commit `8cb89ea`.

- **[docs, P2] Task #835 (2026-08-11) — documented `sefer-region`'s runtime relationship to the main allocator and explicitly deferred four pre-freeze API findings.** Empirical verification (grep search of `src/`) confirmed that `Region<T>`/`Handle<T>`/`SyncRegion<T>` are re-exported at the workspace root for external consumers but are **not used by the `sefer-alloc` allocator runtime itself** — no direct calls on any hot path. The "two faces" unification plan in `docs/ALLOC_PLAN.md` was explicitly marked as a recorded decision, not an open question. Four code-quality findings marked "resolve before the API freezes" — Q3 (handle-yielding iteration/retain), Q4 (error-type width / `#[non_exhaustive]`), Q5 (fallible `SyncRegion` constructors), and Q13 (`try_insert`) — were consciously **NOT implemented** and will remain on the record as future work only if/when a real consumer demonstrates the need. This is a deliberate "investment deferred until demand is demonstrated" policy, not a forgotten backlog item. Documentation-only changes to `crates/region/README.md`, `crates/region/src/lib.rs`, `docs/ALLOC_PLAN.md`, and `docs/reviews/2026-08-11-sefer-region-code-quality-review.md`; no runtime code changed.
- **[fix(region), bench(region), test, docs, P2] Task #836 (2026-08-11) — closed 18 non-blocking findings from the same code-quality review (Q2, Q6-Q12, Q14-Q23).** Three real bugs: `SyncRegion::read`/`write` cleared the lock's poison flag unconditionally on every call instead of only on the recovery path; two of the four bench probes' shared statistics code used `values[len/2]` as "median" (wrong for even sample counts) and one (`r828_drop_outside_lock_probe`) discarded its computed `blocked_median` entirely — both fixed via a new shared `benches/common/stats.rs`; `region_new_contention_gate`'s string-keyed arm dispatch had a catch-all match arm that silently aliased an unrecognized arm name to `baseline_local_atomic`, replaced with a compile-time-checked table with no catch-all. Remainder is dedup (I1-I7 invariants text unified into `crates/region/src/invariants.md` via `include_str!`, a `owned_key` accessor helper, `DropCounter` unified into `tests/common/mod.rs`) and doc accuracy (contended-read number provenance, `core::error::Error` unconditional on MSRV grounds, `Handle`'s Debug/constructor field order matching its `Ord` order, a `From<SyncRegion<T>> for Region<T>` symmetry impl, two considered-but-rejected R827 contention mitigations recorded). Q16's full test-file reorg was deliberately scoped down to just the `DropCounter` dedup, consistent with #835's "no further investment" decision. Verified: `cargo test`/`cargo clippy -D warnings` in both `--all-features` and `--no-default-features` configs, `cargo fmt --check`, `cargo doc`. Commit `91ce9c3`.
- **[process, P1] Tasks #837-#841 (2026-08-12) — bumped `sefer-region` 0.1.0 → 0.2.0, ran the Stage E release-verification checklist, and published to crates.io — all user-authorized.** The 0.1.0 published on 2026-06-29 predates the F1 critical fix (#813, process-wide `region_id` reuse after counter exhaustion) and the F2 domain-aware `Handle<T>` identity redesign (#802) — both real, user-visible changes since publish, the latter a behavioral (not signature-shape) breaking change `cargo-semver-checks` cannot detect on its own (confirmed: `check-release` reported "no semver update required" against the 0.1.0 baseline, as `.github/workflows/ci.yml`'s own comment already documents as an expected limitation of that tool). Full Stage E matrix run and verified personally: `cargo test`/`clippy -D warnings` (debug+release, `--all-features` and `--no-default-features`), `cargo fmt --check`, `cargo doc`, plus a genuinely isolated package-extract-and-build verification (tarball extracted outside the workspace, fresh `CARGO_TARGET_DIR` to avoid the ambient env var reusing workspace build artifacts, no ancestor `Cargo.toml` — 11 test binaries green, clippy clean, one packaged bench actually executed to completion) — closing F14 for real. **Process finding recorded for future release rounds:** `.github/workflows/release.yml` auto-publishes to crates.io the moment a tag matching `<crate>-v<version>` is pushed — the actual irreversible action happened at the `git push origin sefer-region-v0.2.0` step (task #840), not at a separate manual `cargo publish` step (task #841) as the task decomposition assumed. Verified via `gh run list --workflow=release.yml` (run `31572674753`, success, 32s after the tag push) and directly against the crates.io API (`max_version`/`newest_version` both report `0.2.0`, published `2026-08-12T07:07:51Z`). Tag `sefer-region-v0.2.0` on commit `d2fec28` — the first correctly-tagged release for this crate (0.1.0 was never tagged, a gap F1 had flagged).

#### Post-closing independent review remediation (2026-08-05, tasks #547-552)

R34-27's own closing task launched an independent `@oh` readonly review of the entire round (`docs/reviews/2026-08-05-round34-readonly-review.md`), which found 8 findings (F1-F8; 2×P2, 5×P3, 1×P4), none a correctness or soundness defect in shipping code — the dominant theme was stale cross-references produced within the round itself (a later task invalidating an earlier task's cited number or line, with no sweep). All eight were filed as six leaf tasks and fixed in this follow-up wave, delegated sequentially (`/crush` for the first task; a peak-hours provider refusal on the second task triggered a pre-armed `sh` sub-agent fallback for the remainder), each personally zero-trust re-verified. **Runtime improvements: 0** — every fix in this wave is doc-only or test/CI-infrastructure-only; no `src/` runtime behavior or `production` composition changed.

- **[test, P2] F1 (task #547) — the `internals` semver-boundary test was vacuous in every configuration this repo actually runs.** `tests/r34_3_internals_boundary_api.rs` exists to prove the crate-root re-exports resolve WITHOUT `internals`, but every CI/check-matrix row satisfying its `#![cfg]` also enabled `internals`, where the guard cannot fail for the reason it was written. Added one dedicated `test`-kind row to `scripts/check-matrix.mjs`'s `PER_PR_ROWS` running the file under plain `alloc-core alloc-global alloc-decommit` (no `internals`) — picked up automatically by both `npm run check` and CI's `check-matrix` job, no ci.yml job-structure change needed. Counterfactual-verified: moved `AllocCore` behind `internals`, confirmed the new row fails to compile (`E0432`), restored, confirmed green again. Commit `4d52cfb` (stable, unaffected by the later G1/task #555 rebase).
- **[docs, P2] F2 (task #548) — `docs/perf/OPEN_ITEMS.md` item `[L]12`'s current-state card still cited the ~40× realloc figure R34-23 refuted in the same round as its own stated reason for a low-value verdict.** Corrected to the real ~1.8-2.1× ratio and re-derived the verdict rather than just swapping the number: the corrected ratio weakens the original "already cheapest" argument, downgrading the item from "confidently low-value" to "genuinely unmeasured, still low-priority pending that measurement" — a more honest posture, not a flip to high-value (the item's real decisive datum, the sub-16 KiB ladder's own Stage-1 hit rate, remains unmeasured and untouched by this correction). Commit `5e75032` (reworded by the G1/task #555 rebase; originally `73817ee`).
- **[docs, P3] F3 (task #549) — `docs/CORRECTNESS_OPEN_ITEMS.md` item 15 still read "BLOCKED on #524/R34-5" after #524 completed and answered the item's own decision rule.** Independently re-confirmed (not taken on trust): read `fd54ddc`'s diff, grepped `ci.yml`'s `miri-plain` job to confirm no `-Zmiri-tree-borrows` (plain Stacked Borrows active), and personally re-ran `regression_xthread_small_ring_miri` fresh locally under those exact `MIRIFLAGS` — 1 passed, 0 failed, reproducing #524's own result rather than trusting its commit message. Trigger did not fire → `atomic_ptr_ref` treatment is NOT required for the ring's other atomic accessors; item closed resolved-negative per CLAUDE.md's R34-24 current-state-card rule (one-line pointer in the open list, full closure narrative moved to "Recently resolved"). Commit `55f8317` (SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `7faa377`).
- **[docs, P3] F5+F6 (task #550) — `docs/perf/round-manifests/R34_MANIFEST.md` undercounted the round at 38 commits (missing the 5 commits landed after it was written) and `CHANGELOG.md`'s R34-3 bullet cited a commit list that omitted `27879af`, the commit containing the task's actual substance.** Extended the manifest to the real 43-commit span (`git log`-derived, not hand-transcribed), added missing R34-25/R34-26/R34-27 per-item verdicts, corrected `a9edc87`'s "(untagged)" placeholder to its real owner R34-6/#525. Fixed this CHANGELOG's own R34-3 commit list (above) to lead with `27879af`+`b47cc6a`+`0762772`, and this section's own header to name all three source reviews (a third, `docs/reviews/2026-08-03-round33-readonly-review.md`, was previously uncredited) and clarify "26 tasks" is not a strict 1:1 with "26 review findings". Commit `80463d2` (SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `e496d8b`).
- **[docs, P3] F7 (task #551) — `docs/perf/r34_23_runs/2026-08-04T22-03-44-381Z_direct_raw.json` (258 KiB) was a tier-2 artifact-storage-policy violator that R34-24's own landing commit had named but not remediated.** Gzip-compressed to 8,674 bytes (~30× smaller, well under the 200 KiB ceiling; byte-identical roundtrip verified before removing the uncompressed original) — chosen over truncation because the file's 1,080 uniform per-sample records are what the gate report's summary CSV derives from in full. Added a `.gitignore` rule for the directory (matching `paired_ab_runs/`/`r34_7_runs/`'s existing scratch-by-default convention) and a `docs/perf/OPEN_ITEMS.md` entry recording the fix plus an explicit reopening trigger for the policy's underlying naming-based blind spot (it is keyed on the `_raw_*.log` filename glob, which is exactly why this file was invisible to R34-24's own 256/256 compliance census). Commit `358be4e` (post-rebase, reworded by G1/task #555; originally `5710a6e`).
- **[docs, P3/P4] F4+F8 (task #552) — a doc line-number citation went stale within the round, and a historical bench-log figure R34-23 called "physically impossible" remained unflagged.** `src/global/sefer_alloc.rs`'s five-tripwire enumeration cited `alloc_core.rs:2158` for site 1; R34-23 shifted it to 2203-2205 the same day. Dropped line numbers from all five entries (file + function name is unambiguous and drift-proof, unlike a line number) rather than extending the pin — lower maintenance burden, and `tests/no_panic_doc_accuracy.rs` already pins the five by message string. Separately, added a dated append-only correction note beside `docs/ALLOC_BENCH.md`'s stale `realloc_grow_geometric` row (9.67 µs/39.6×, refuted this round) per this project's established correction convention — the historical figures are left as-is, not silently rewritten. Commit `a7d7395` (reworded by the G1/task #555 rebase, per the G5 wording fix too; originally `d46c349`).

#### Release-readiness remediation (2026-08-05, tasks #555-570)

A second, independently-commissioned release-readiness review (`docs/reviews/2026-08-05-sol-release-readonly-review.md`, a static audit of the full `40241b0..c5db553` range plus the F1-F8 wave above) found the tree **not release-ready**, though not from any newly-confirmed UB/UAF/double-free/data-race/OOB — three PROCESS/API-CONTRACT release blockers instead. A parallel re-verification pass of the F1-F8 wave itself (`docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`) independently found 7 more findings (G1-G7) plus a bonus pre-existing-commit-prefix discovery, none a shipping-code defect. All 16 filed as tasks (#555-570; #562 and #564 needed genuine user decisions, obtained via explicit questions, not guessed), delegated via `/crush` and a `sh` fallback (a real peak-hours refusal mid-wave — see the crush-fallback process note below), each personally zero-trust re-verified. **Runtime improvements: 0** — this wave is exclusively `src/` API-boundary/doc/process work; only Sol-F1 touches `src/`, and it is a `pub`-surface visibility fix with no algorithm change.

- **[correctness fix, P1] Sol-F1 (task #563) — `AllocCore::dbg_*` inherent methods were reachable WITHOUT `internals`, a real semver-boundary defect R34-3 did not actually close.** `AllocCore` is re-exported at the crate root unconditionally (gated only on `alloc-core`); gating the `alloc_core` MODULE PATH behind `internals` (R34-3) does not hide a type's own already-`pub` inherent methods reached another way — `sefer_alloc::AllocCore::dbg_carve_batch` and ~50 sibling `dbg_*` hooks compiled and ran with `internals` fully off. Fixed by gating the `dbg_*`-only `impl AllocCore` blocks directly (`#[cfg(feature = "internals")]`), split per-file where a small number of methods have a genuine stable-API or `pub(crate)`-hot-path caller that must stay ungated. Fixed 19 transitive `registry` delegation call sites plus 6 examples + 3 benches whose `required-features` were missing `internals`. Added a REAL negative compile-fail oracle (`examples/sol_f1_dbg_carve_batch_negative_probe.rs` + `scripts/verify-internals-negative-boundary.mjs`, wired into `npm run check`) proving the boundary now holds in both directions — the same class of gap F1 (task #547) closed for the module-path half, this closes for the inherent-method half. Commit `9296adb` (post-rebase SHA `dbb4016`).
- **[docs, P1] Sol-F2 (task #564, NEEDS-USER-DECISION) — `Cargo.toml`'s `0.3.0` and `CHANGELOG.md`'s dated `## [0.3.0] - 2026-07-04` section disagreed with reality: no `sefer-alloc-v0.3.0` git tag exists, and crates.io is still on `0.2.1`.** User confirmed 0.3.0 is not yet released, is the in-progress target. Consolidated `[Unreleased]`'s entire content (this round + this wave) into a single `## [0.3.0] (unreleased)` section, removing the false ship-date — a near-zero-diff structural change, since `[Unreleased]`'s content already sat immediately before the `0.3.0` section in file order (6,598 lines before and after). Added a `release.yml` CI guard blocking a tag publish while its CHANGELOG section still reads `(unreleased)`. Commits `8edd8c8` (consolidation) + `a6484ca` (CI guard) + `9c5ea64` (self-caught fix: removed a fabricated GitHub issue URL the delegated session had invented in the guard's comment, conflating an internal TaskList task ID with a nonexistent GitHub issue).
- **[docs, P1] Sol-F3 (task #565) — `Cargo.toml`'s `alloc-global` comment claimed a blanket "never panic/abort" contract, contradicted by the crate's own already-documented behavior.** Five release-surviving invariant tripwires (R34-16) deliberately panic-then-abort by design, and `Registry::ensure_chunk` calls `std::process::abort()` directly on alloc-path chunk-materialisation OOM. Rewrote the comment to the accurate contract (ordinary failures are null/no-op; the two documented exceptions terminate the process) and added a structural pin (`tests/no_stale_doc_references.rs::cargo_toml_alloc_global_panic_contract_is_accurate`) tying the wording to both cited facts, counterfactually verified. Commit `15a1ef6` (post-rebase SHA `d17eec3`).
- **[docs, P2] Sol-F4 (task #566) — R34-11's narrative conflated a new retention fix, an old preserved speedup, and a fictitious "unbounded spin" claim that does not match the actual (hard-capped 8-iteration) loop mechanism.** Independently re-verified the gate report's own numbers (final gap 3→1, peak still 4, ≥3-segment persistence 72.5%, throughput regime never reaches the catch-up body) before rewriting CHANGELOG.md's and the manifest's R34-11 entries to separate the three claims explicitly. Commit `6190526` (SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `2f70081`).
- **[docs, P2] Sol-F5 (task #567) — `DrainHeadPublish`'s doc comment overclaimed "panic-safe" without the precondition that it does not guarantee exactly-once for the specific element being processed when a panic occurs mid-reclaim.** Narrowed to state both halves explicitly (no loss of prior fully-processed elements; no exactly-once for the panicking element), confirmed via direct reading that no currently-reachable panic source exists in the three production reclaim closures, without redesigning the reclaim protocol (correctly scoped out as separate, larger follow-up). Commit `ff496c6` (post-rebase, reworded by G1/task #555; the SHA `a4dc38e` previously cited here was itself the STALE pre-rebase SHA, an ironic instance of the exact drift this note now describes — see H4/task #574).
- **[docs, P2] Sol-F6 (task #568) — `InitStateGuard`'s doc comment claimed unqualified "panic-safe" coverage of a window where `Drop`-of-an-already-written-`HeapCore` is not actually guaranteed.** Confirmed by direct reading that `bind_thread_free` is a plain field assignment (cannot panic) and the only reachable panic fires before `HeapCore::new`, so the gap is real-in-principle but not currently reachable — narrowed the doc claim accordingly rather than redesigning the guard for an unreachable window. Commit `1f1015a` (SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `04ba0f8`).
- **[docs, P3] Sol-F7 (task #569) — `RemoteFreeRing`'s cached-head soundness proof left an already-documented wrap/preemption assumption's COMPOUND nature (memory model + scheduler assumption, not memory model alone) only implicit.** Added one explicit summary sentence naming both halves; swept `docs/` for other "formally verified" claims about the same mechanism and added the same caveat to two found in `docs/perf/OPEN_ITEMS.md`; closed the cross-review trail in `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` §11. Commit `45c45be` (SHA cascaded by the G1/task #555 rebase (message unchanged, only its parent's SHA changed); originally `2e1ef90`).
- **[docs, P2] G1 (task #555) — a real, scripted `git rebase -i` fixed three accumulated commit-message defects: a hard R30-12 taxonomy failure, an invalid `docs(perf)` prefix, and G5's misleading "left untouched" wording.** Non-interactive (`GIT_SEQUENCE_EDITOR`/`GIT_EDITOR` scripting), executed only after explicit user confirmation of the exact plan; verified `git rev-parse HEAD^{tree}` byte-identical before and after (only 3 commit messages changed, zero diff content moved), full suite re-run green, `node scripts/verify-commit-prefixes.mjs` PASS afterward.
- **[docs, P3] G2-G4, G6, G6-followup (tasks #556-558, #560, #570) — stale CHANGELOG/manifest cross-references and two rounds of `OPEN_ITEMS.md` item-number collisions (40, then 13+38 discovered as a side effect of fixing 40) fixed.** Each renumbering independently re-derived the real next-free number rather than trusting a cited guess; G6-followup's own de-duplication check is the one that surfaced the second pair, illustrating the same "stale cross-reference produced within the very wave meant to fix stale cross-references" pattern the first wave (F1-F8) also exhibited.
- **[docs, P4] G7 (task #561) — confirmed as an accepted existing pattern (citing an untracked review-report path from a tracked source comment), no fix needed** — a genuine precedent exists (`tests/regression_xthread_small_ring_miri.rs`, citing a different untracked review file), and every citation found is self-contained enough to be useful even if the cited file doesn't exist in a fresh clone.
- **[process] G1-bonus (task #562) — filed, not fixed, two PRE-EXISTING Round-34 commits (`43115cf`, `5c1142f`) that also fail `verify-commit-prefixes.mjs`**, contradicting the first wave's own closing review's "correctly applied throughout" claim — `docs/CORRECTNESS_OPEN_ITEMS.md` item 21 records the exact lint output and a reopening trigger; deliberately not rebased (deeper history, more descendants, separate risk decision from G1's own narrower rebase).
- **[process note] crush-fallback peak-hours cron never fired.** A `/crush` peak-hours refusal earlier in this session correctly engaged the pre-armed `sh` fallback, but the scheduled auto-revert cron (fires only during REPL idle time) never got an idle window across an unbroken run of delegated tasks — it sat unfired ~86 minutes past its due time until the user asked directly why `/crush` wasn't in use. Resolved manually (stale cron deleted, fallback marker reset, confirmed with the user) the moment it was flagged; `/crush` resumed use for the rest of this wave. A third, previously-unnoticed untracked review file (`docs/reviews/2026-08-05-fallback-release-readonly-review.md`, apparently from a separate process) was also found and read during this wave's own closing pass — its findings were mostly superseded by Sol-F1-F7, except one legitimate stale-comment P3 (a "5 clippy rows" `ci.yml` comment, now 6 since R34-3) fixed directly. Commit `eb66af6`.

#### Release-readiness remediation follow-up (2026-08-05, tasks #571-578)

A third closing review of the wave above (`docs/reviews/2026-08-05-sol-remediation-readonly-review.md`) found 8 findings (H1-H8), critically confirming that **2 of the prior wave's own 3 "P1 release blockers" were only PARTIALLY closed**: Sol-F1's `internals`-gating fix covered only 3 of 6 files with `AllocCore::dbg_*` methods, and Sol-F2's CHANGELOG "consolidation" had deleted a header without actually moving content under a replacement, leaving Round 34 through Round 12's content with no enclosing version header at all — and that `npm run check` was actively broken at `HEAD`. Filed as tasks #571-578 in strict priority order via `blockedBy` chains (H1→H2→H3→H4→H5→H7→H8→H6) per this session's explicit governing instruction: fix all red/broken things first, then review findings, then a new review. Delegated via the `sh` fallback throughout (a genuine hard weekly/monthly quota limit hit `/crush` mid-H2, engaging the pre-armed fallback's Trigger 2 with no automated resume), each personally zero-trust re-verified. **Runtime improvements: 0** — H2's `internals`-gating extension and its fallout fixes narrow reachability under a non-default opt-in feature; nothing in `production`'s default composition or any shipping algorithm changed.

- **[fix, P1] H1 (task #571) — two pre-existing compile failures were breaking `npm run check` at `HEAD`.** `tests/r34_18_heap_core_stack_pressure_pin.rs` was missing an `internals` cfg gate (Sol-F1's own oversight), and `src/registry/heap_core.rs`'s `size_of::<HeapCore>() <= 8192` compile-time assert fired under `--all-features` (which legitimately grows the struct to 8840 B via `experimental`/`pinning`/`bench-internals`/`batch-api`, none of them shipping configuration). Fixed both; personally caught and fixed a self-contradictory adjacent comment (claimed `--all-features` was 7576 B; the real number is 8840 B) before committing. Commit `8b9ed10`.
- **[fix(perf), P1] H2 (task #572) — Sol-F1's `internals`-gating fix was only partially applied: 3 of 6 files with `AllocCore::dbg_*` methods, 31 methods across `alloc_core_large_cache.rs`/`alloc_core_small_pool.rs`/`alloc_core.rs`'s numa methods, remained reachable WITHOUT `internals`.** Gated all 31 (one exception: `dbg_decommit_count` stays ungated — it backs `SeferAlloc::stats()`'s public `AllocStats.decommit_calls` field, a real production caller despite its `TEST-ONLY` doc comment, added as a 4th entry alongside Sol-F1's own 3 `stats()`-backing exceptions). Fixed 26 transitive delegation call sites across `src/registry/heap_core_diag.rs`/`heap_core.rs`/`src/global/sefer_alloc.rs` — the SAME fallout class Sol-F1 already fixed for its own 19 call sites, because a module becoming `pub(crate)` (internals off) still must COMPILE as part of the crate; only its EXTERNAL visibility changes. Built `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` (Sol-F1's own oracle tested exactly ONE method and passed while these 31 stayed open, undetected) — enumerates and checks EVERY `AllocCore::dbg_*` method, wired into `npm run check`. A follow-up `npm run check` full-matrix run then surfaced two more real gaps this task's own narrower verification missed: 6 `examples/` + 1 `benches/` target with independent `required-features` never updated, and one target (`r31_3_large_cache_extended_narrow_on`) missing a PRE-EXISTING `large-cache-extended` requirement its own doc comment already documented but never encoded — both fixed in the same commit once found. Commits `25d6ac4` + `e886ea4`.
- **[docs, P1] H3 (task #573) — Sol-F2's CHANGELOG "consolidation" (task #564) deleted the `[Unreleased]` header and renamed the old dated `0.3.0` header in place, but never actually moved Round 34's content inside any `##`-level section — it left ~4,970 lines orphaned, with no enclosing version header, until the reader reached the renamed header far below.** The prior task's own "0 lines lost" line-count verification could not catch this: the count was correct, the STRUCTURE was broken. Fixed by removing the orphaned old header and inserting one genuine `## [0.3.0] (unreleased)` header at the top, verified structurally (not just by line count) that exactly one `##`-level heading exists between the preamble and `## [0.2.1]`. Also fixed `CONTRIBUTING.md`/`.github/PULL_REQUEST_TEMPLATE.md`'s dangling `[Unreleased]` references. Commit `baa91cc`.
- **[docs, P2] H4 (task #574) — G1's rebase (task #555) invalidated 10 SHA citations across 3 tracked docs that were never updated.** Verified each of the 5 candidate SHAs individually (`git merge-base --is-ancestor`) rather than assuming — 4 were genuinely orphaned, 1 (`4d52cfb`) was correctly unaffected (predates the rebase's rewrite point). Found each stale SHA's current equivalent via tree-hash matching. `docs/CORRECTNESS_OPEN_ITEMS.md`'s 4 citations sit inside a historical narrative describing the pre-rebase state — kept the original SHA (accurate at time of writing) and annotated with the current one, rather than silently rewriting history; also caught one CHANGELOG citation (Sol-F5's) that had compounded the bug by already claiming "(post-rebase, reworded)" while citing the STALE SHA. Commit `6e5c067`.
- **[docs, P2] H5 (task #575) — Sol-F5/Sol-F6's documented residuals (`DrainHeadPublish`'s non-exactly-once-under-unwind behavior; `InitStateGuard`'s pre/post-write unwind distinction) were never cross-filed into `docs/CORRECTNESS_OPEN_ITEMS.md`**, the same "same-commit indexing" gap H1 above closed for a different pair of findings. Added items 22-23 transcribing what each doc comment already stated (guarantee/non-guarantee, current non-exploitability, structural closure path), plus a one-line cross-reference back from each source doc comment. Commit `d48d7ba`.
- **[docs, P3] H7 (task #577) — R34-3's and Sol-F1's `internals` public-surface narrowing never used this project's own established `### BREAKING CHANGE` heading format** (9 existing precedents in this CHANGELOG). Added one, sequenced after H2 above so it reflects the fully-closed state (~125 gated methods, 4 exceptions, 45 combined transitive call sites across Sol-F1+H2), matching the intro-paragraph/**Why.**/**What changed:**/**Migration.** structure of 3 sampled precedents. Commit `5c17cc3`.
- **[docs, P4] H8 (task #578) — considered rewording `dbb4016`'s `fix(perf)` prefix to `feat(api)` (matching predecessor `27879af`), decided against a reword rebase.** H2's own commit (`25d6ac4`) independently used `fix(perf)` for the identical gating-change class and passed the taxonomy lint, establishing it as a now-repeated, accepted precedent rather than an isolated mistake worth a rebase deep enough to also touch H2-H7's stacked commits. Decision recorded as `docs/CORRECTNESS_OPEN_ITEMS.md` item 24 (no code change; renumbered from an original 17 that collided with a live OPEN item 17 — see finding F6 of `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`, task #584/I6). Commit `800ee86`.
- **[docs, P3] H6 (task #576) — `docs/perf/round-manifests/R34_MANIFEST.md` was about to need a THIRD extension (38→43→~74 commits) to absorb 3 independent post-closing remediation waves — the same staleness pattern recurring a third time.** Redefined scope instead of extending again: `R34_MANIFEST.md` frozen at Round 34 proper (43 commits, unchanged); each remediation wave now gets its own bounded manifest file (`R34_REMEDIATION_1_MANIFEST.md`/`_2_`/`_3_`). Traced G1's rebase precisely across the new files (2 of its 3 reworded commits are wave-1 commits, not wave-2's, despite G1 the task executing during wave 2). Extended `_3_` once more (commit `28663e4`) after `npm run check`'s own full-matrix run surfaced 3 more real fallout-fix commits (see below). Commits `db63aed` + `28663e4`.
- **[fix, P1] `npm run check`'s own full pipeline (all 5 clippy rows, all 4 test-feature combos incl. `--all-features`) surfaced 2 more real bugs H1/H2's own narrower per-task verification missed, plus 1 unrelated pre-existing flaky test.** `tests/r34_18_heap_core_stack_pressure_pin.rs`'s runtime upper-bound assertion was unconditional, so it genuinely failed under `--all-features` (8840 B > 8192 B budget) despite H1's own doc comment claiming "non-vacuous coverage under ALL configurations" — fixed by mirroring the compile-time guard's own `#[cfg(not(any(experimental, pinning, bench-internals, batch-api)))]` exclusion on the upper bound only (the lower-bound shrink-detector still runs everywhere); a second bug (`8192 - size` unconditional `usize` subtraction, overflowing at size=8840) fixed by widening to signed arithmetic. Commit `0d23e7f`. Separately, `tests/segment_table_contains_base_tier1_counters.rs`'s `repeated_same_segment_frees_are_observed_as_tier1_hits` flaked (`hits_delta=31 misses_delta=2` vs expected `N=32`) — confirmed pre-existing (file last touched by Round 34's `7aeee2d`, unrelated to this wave) and confirmed as a parallelism artifact (process-wide `CONTAINS_BASE_TIER1_HITS`/`MISSES` statics, both tests in the file read a delta, `cargo test` runs them in parallel by default — the SAME class as `docs/CORRECTNESS_OPEN_ITEMS.md` item 1), fixed with the SAME established `TEST_LOCK: Mutex<()>` serialization pattern, reverified with 5 clean multi-threaded reruns. Commit `2f16ba6`. Both discoveries are the concrete value of running the FULL `npm run check` gate before closing a wave, not just each task's own scoped verification.
- **[process note] `npm run check`'s local `verify-commit-prefixes` step remains red on the SAME pre-existing, already-documented debt this wave's own H8 (item 24) and the prior wave's G1-bonus (item 21) already recorded: `43115cf`/`5c1142f`, two Round-34 commits deep in unpushed local history.** Confirmed not a new regression (`git merge-base --is-ancestor 43115cf origin/main` — not an ancestor; nothing has been pushed since before these commits landed) and confirmed this specific CI job never runs on a direct push to `main` (`.github/workflows/ci.yml`'s `commit-prefix-lint` job is scoped `if: github.event_name == 'pull_request'` only, by design — see that job's own comment for why a local approximation and the PR-scoped precise check are deliberately separate). Every other step in the full pipeline (fmt, all 5 clippy rows, all 4 test combos, the internals-boundary tests, both compile-fail oracles, `verify-perf-gate-stubs`, `verify-gate-report`, and `npm run iai`) is green.

#### Release-readiness remediation wave 4 (2026-08-05, tasks #579-589)

An independent readonly review of wave 3's own work (`docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`) found 10 findings (F1-F10: 1×P1, 3×P2, 3×P3, 3×P4). Filed as tasks I1-I10 (#579-588), each personally zero-trust re-verified before committing. **Runtime improvements: 0** — the one genuine P1 (F1) raises a compile-time stack-pressure budget to cover a shipping feature combination that was already this size; nothing in `production`'s default composition or any shipping algorithm changed.

- **[fix(perf), P1] F1 (task #579) — `HeapCore`'s stack-pressure budget (H1's own fix, task #571) excluded 4 named experimental features but never checked whether OTHER real shipping compositions also exceeded the original 8192 B ceiling.** `cargo test --features "production medium-classes internals"` failed to compile (`size_of::<HeapCore>() = 8408 > 8192`). Fixed structurally instead of extending the exclusion list a third time: measured the maximum across every feature composition this crate can currently build (`--all-features` = 8840 B, the largest measured — see the F6 correction below for why this is scoped to the current field layout, not an eternal theorem), raised the budget to 9216 B, and removed the fragile per-feature exclusion mechanism entirely — both the compile-time assert and its mirrored runtime test are now unconditional. Commit `3d57a26`.
- **[test, P2/P4] F4+F10 (task #582) — 39 `tests/*.rs` files called an `internals`-gated `dbg_*` method without `internals` in their own `#![cfg]`, hard-failing (E0599) instead of cleanly skipping under a plain `production` build.** Fixed all 39 (mechanical, script-driven), then extended `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` with a permanent second check (check 2/2) scanning every `tests/*.rs` file for exactly this class of gap, replacing the old acceptance rule ("any mention of `internals` anywhere in the file passes") with a rule that requires the file's `#![cfg]` to actually reference `internals` (F10, folded into the same commit — while building this, also found and fixed a genuine relative-path bug in the script's own new recursive directory walker). Commit `b1a9b7b`. **Correction (see below):** check 2/2's own matcher was still a raw-text regex over the whole file, so it matched `#![cfg(...)]` text inside a doc comment as if it were the real attribute — a false-PASS that shipped a genuine compile failure; not fixed until `2426dcc`, three commits later in this same wave's follow-up.
- **[docs, P2/P4] F3+F8 (task #581) — H7's `### BREAKING CHANGE` heading, inserted immediately after R34-3's bullet, silently terminated both the enclosing `#### Measurement, correctness & tooling` subsection and `### Round 34` itself early (a `###` heading outranks a `####`), orphaning ~40 already-written bullets out of their section — the SAME defect class H3 fixed one level up earlier the same wave.** Moved the heading to after the complete Round-34 bullet list, matching where all 9 pre-existing `BREAKING CHANGE` precedents sit. Also corrected H2's `AllocCore::dbg_*` gating scope from a miscounted "9 files" to the true 6 (3 occurrences, F8). Commit `ba18071`.
- **[fix(perf), P3] F5 (task #583) — `SeferAlloc::dbg_trim_current_thread` was still completely ungated, reachable from a plain `production` build.** Gated `#[cfg(all(feature = "bench-internals", feature = "internals"))]`; fixed its 5 real callers (2 benches' `required-features`, 2 test files' cfg gates) and removed a now-stale `dbg_trim_current_thread` entry from `tests/dbg_hook_safety_tripwire.rs`'s `SAFE_MUTATORS` catalog (that file's own tripwire correctly flagged the drift). Verified via an external downstream probe crate (`features = ["production"]` only) that the call now fails E0599. Commit `7a9b7c7`.
- **[docs, P2] F2 (task #580) — H4 (task #574) closed only 4 of 13 orphaned SHA citations G1's rebase (task #555) invalidated; F2 originally scoped as "a few remaining" turned out to be 8 more.** Re-derived the true scope from `git merge-base --is-ancestor` + tree-identity checks rather than trusting the review's own estimate, fixing all 8 with precise wording distinguishing "SHA cascaded by the rebase (message unchanged, only its parent's SHA changed)" from the 3 commits G1 actually reworded. Commit `04c2f74`.
- **[docs, P3] F6 (task #584) — wave 3's own new `docs/CORRECTNESS_OPEN_ITEMS.md` "Recently resolved" items 17/18 (H8's decision record, the tier1-counters flaky-test fix) collided with two live OPEN items ALSO numbered 17/18.** Renumbered the resolved pair to 24/25 and fixed 2 CHANGELOG cross-references citing the old numbers. Commit `addc63d`.
- **[docs(config), P4] F9 (task #587) — `scripts/check-all.mjs`'s own runtime banner and comments still said "5 clippy rows" / "clippy x5"; there are 6 (default / `experimental` / `--all-features` / `hardened medium-classes` / `production` / `production internals`).** Fixed 3 stale mentions and made the banner derive the row count dynamically from `clippyRows.length` instead of hardcoding it, closing this whole staleness class going forward. Commit `650b818`.
- **[docs, P3] F7 (task #585) — `R34_REMEDIATION_3_MANIFEST.md` still under-counted wave 3 at 11 of its own reviewer-cited 14-commit span.** Finalized it to the full 14/14 (`2a7f1e6..85dacfc`), marked FINAL, and established the convention "the wave's LAST commit updates its own manifest, listing itself, in the same commit" for future waves — to avoid repeating this file's own two-round extension history. Started `R34_REMEDIATION_4_MANIFEST.md` for this wave. Commit `782b92e`.
- **[fix] `npm run check`'s own `--all-features` step, rerun after all 8 F1-F10 commits landed, surfaced one more real pre-existing flaky test unrelated to this wave's own changes: `ac1_trim_empties_pool_and_evicts_large_cache` (`tests/r31_10_trim_current_thread_api.rs`) failed with `released_before=1, released_after_cache=2`.** Same root cause as `docs/CORRECTNESS_OPEN_ITEMS.md` items 1 and 25 — `segments_released_total` is a process-wide counter, all six `#[test]` functions in the file call `trim_current_thread()`, `cargo test` runs them in parallel by default. Confirmed pre-existing (file dates to Round 31/task #474, last touched by an unrelated Round-34 rustfmt-drift commit) and unrelated to F1/F5's own changes. Fixed with the same established `TEST_LOCK: Mutex<()>` serialization pattern, applied to all six tests; 5 clean `--all-features` reruns plus a clean `production internals` run. Added `docs/CORRECTNESS_OPEN_ITEMS.md` item 26. Task #589, commit `60ad847`.

**Correction, append-only (2026-08-05) — an independent readonly review of wave 4's own work (`docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`) found that wave 4's F4+F10 fix (`b1a9b7b`, above) did not actually close what it claimed to close.** F1 of that review (P1): `b1a9b7b`'s new check 2/2 in `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` still matched `#![cfg(...)]` as a raw-text regex over the WHOLE file, so a `#![cfg(...)]` string appearing inside a doc comment (`///`/`//!`) produced a false PASS identical to a real crate-level attribute — the scanner never actually required a real, line-anchored `#![cfg]`, contrary to the bullet above's original wording. This was not a theoretical gap: `b1a9b7b`'s own mechanical edit had inserted exactly such a doc-comment-only mention (without the matching real attribute) into `tests/medium_classes_correctness.rs` and `tests/medium_classes_wide_correctness.rs`, and the scanner reported both files green while `cargo check --features "production medium-classes" --test medium_classes_correctness` reproduced 20 real E0599 errors — independently reproduced personally before fixing. Fixed in `2426dcc` (three commits after `60ad847`, still within this same wave's follow-up work): both test files corrected (real `#![cfg]` now includes `internals`), and the scanner rewritten to use `extractCrateLevelCfgBlocks()`, a line-anchored paren-balance walk that only recognizes lines literally starting with `#![cfg(`, never text inside a comment — verified non-vacuous via counterfactual (stashing the two test-file fixes and re-running the fixed scanner correctly reproduces both violations). The same commit also fixed F2 (P3, `gatedMethodNames` could count an intentionally-ungated allowlisted method as gated), F5 (P4, a doubled step-numbering comment in `scripts/check-all.mjs`), and F6 (P3, see the F1/`3d57a26` bullet above — `src/registry/heap_core.rs`'s stack-pressure comment overclaimed `--all-features` as an eternal global maximum; scoped to the current additive-`#[cfg(feature = "...")]` field layout and measured target, with the unconditional compile-time assert, not the claim itself, being what actually enforces the budget against future field-layout changes). `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` reran ALL GREEN post-fix (41 files, 128 methods, 0 violations; 244 test files, 0 violations).

#### Known limitations (as of this release)

Two documented residuals from `docs/CORRECTNESS_OPEN_ITEMS.md`, restated briefly here so a release-notes reader does not have to open that index to learn about them:

- **Cross-thread free of an already-released foreign segment is caller-contract UB, not fixed by this release** (`docs/CORRECTNESS_OPEN_ITEMS.md` item 16). `dealloc_foreign_routing` (`src/registry/heap_core_xthread.rs:858-1007`) distinguishes a live foreign segment from an already-released one only via a "magic != 0" guard; the code itself documents that this is O(1)-indistinguishable from a genuinely reused/repurposed segment, so a double free of an already-released foreign segment remains "fundamentally UB … not fixed by this change" — the standard caller-contract residual any allocator has for a double free. For a single legitimate cross-thread free this is not reachable (`live_count >= 1` until the owning thread's drain reclaims the segment, so it cannot be released out from under the freeing thread); the residual applies only to a caller that has already violated the allocator's contract by freeing a pointer twice.
- **MSRV is enforced via `cargo check --all-features`, not `cargo test`** (`docs/CORRECTNESS_OPEN_ITEMS.md` item 19). The `msrv` CI job never compiles `#[cfg(test)]`-only code or dev-dependency-only paths, so an MSRV-incompatible construct reachable only from those paths would not be caught by CI. Judged acceptable for this release, but stated here explicitly rather than left implicit.

#### Release-readiness sprint (2026-08-05/06, tasks #596-623)

After wave 4 landed and pushed (`origin/main` moved from `42d8d22` to `42d4206`, 153 local commits), three independent readonly reviews assessed 0.3.0's actual release readiness — `docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md` (verified wave 4's own diff), `docs/reviews/2026-08-05-release-readiness-gap-audit.md` (standalone release-engineering gap audit, findings R0-R17), and `docs/reviews/2026-08-05-fh-release-readiness-verification-review.md` (independent cross-check that ran real commands, including a live `cargo package` attempt, and caught the first two reviews' own diagnostic errors — see below). Findings filed as tasks K1-K18/L1-L8/M1-M2 in `docs/plans/2026-08-05-release-execution-map.md`, prioritized into execution waves. **Runtime improvements: 0** — every fix below is CI plumbing, release-workflow gating, test isolation, or documentation; no shipping/opt-in algorithm or `production` default changed.

- **[fix, P0] Two genuine CI-red jobs on the pushed backlog's first full remote CI run (`31045983765`, landing SHA `42d4206`).** `benches/macro_multiseg_steady_state.rs`'s two `#[library_benchmark]` functions carried `///` doc-comment blocks, which `iai-callgrind` 0.14.2's proc-macro rejects as an invalid attribute (`Invalid attribute: 'doc'`) — the whole file is `#[cfg(target_os = "linux")]`, so it had never compiled on this project's Windows dev machine since it was added (R32/task #500) and the failure survived undetected until the first real Linux CI run. Converted both blocks to `//` line comments; independently re-verified on WSL with the exact CI command (`cargo clippy --all-targets --all-features -- -D warnings`, exit 0) and a counterfactual (reverting the fix reproduces the identical 4 errors). Commit `9df72c0` (L1/task #614). Separately, `tests/regression_free_path_chunk_oom_graceful.rs`'s two tests raced on the process-wide `DBG_INJECT_CHUNK_OOM` flag (no serialization, despite the module doc assuming sequential execution) — the third recurrence this session of the same process-wide-diagnostic-interference class (`docs/CORRECTNESS_OPEN_ITEMS.md` items 1/25/26). Fixed with the established `TEST_LOCK: Mutex<()>` pattern; 5 clean reruns. Commit `2853d35` (L8/task #621, `docs/CORRECTNESS_OPEN_ITEMS.md` item 27). Audited all 5 files sharing the Linux-only blind spot (M1/task #622) — the other 4 confirmed clean via direct Linux compilation, no other instance of the doc-comment bug found.
- **[build, P0] `release.yml`'s CHANGELOG/version guards were bypassable on manual dispatch and checked the wrong crate's changelog.** Both pre-publish guards were gated on `github.event_name == 'push'`, so a `workflow_dispatch` with `dry-run=false` reached `cargo publish` without clearing either — re-keyed both on `dry_run != 'true'` (the same condition the existing test-gate already used), closing the bypass-by-trigger-type hole. **Correction (found by `docs/reviews/2026-08-06-sprint-closing-readonly-review.md` finding S1, this same close-out pass):** this does NOT mean a manual publish now clears "the same checks" as a tag push in every respect — on `workflow_dispatch` the version guard still has nothing to compare against (no tag carries a version) and exits early by design, and the CHANGELOG guard is scoped to `sefer-alloc` only (see below), so `workflow_dispatch` + `dry-run=false` + a member crate clears ZERO version/changelog checks, strictly fewer than a tag push. Separately, the CHANGELOG guard unconditionally grepped the ROOT `CHANGELOG.md` regardless of which crate was being published (a member-crate tag could false-pass against an unrelated root-crate section sharing its version number, or false-fail with no root section at all) — scoped the guard explicitly to `sefer-alloc`, with member crates skipping with a documented reason (no per-crate changelogs exist yet — this remains a real gap for member-crate publishes, not a solved problem). Commit `e43e17f` (K5+L4/tasks #600+#617). Additionally, `publish` had no dependency on the main `CI` workflow's result for the same commit — added a guard step querying the GitHub Actions API for `ci.yml`'s run on `github.sha` specifically (not "latest on main"), failing closed if no run exists, the run hasn't completed, or its conclusion isn't `success`. Commit `1c06b86` (K8/task #603). `actionlint` and a YAML parse both clean on every edit. **Correction (finding S2):** the originally-stated follow-up validation ("a live `workflow_dispatch` dry-run against each crate") CANNOT validate any of this — every guard touched here is itself gated on `dry_run != 'true'`, so a dry-run skips all of them by construction and would return green having exercised none of the changed lines (the same false-PASS shape as `2426dcc`'s own finding). The correct manual check, not yet performed: `workflow_dispatch` with **`dry-run=false`** against a crate/version combination that should fail (e.g. `sefer-alloc` while `CHANGELOG.md` still reads `(unreleased)`), expecting the guard to fail closed before `cargo publish` — safe specifically because the guard runs before publish, not because dry-run was used.
- **[docs, P0] Corrected two overstated wave-4 CHANGELOG claims and added the release-notes caveats items 16/19 had been waiting on.** The F4+F10 bullet claimed `b1a9b7b` "tightened gate-detection to require a real cfg attribute" — false; that scanner fix's own false-PASS bug wasn't closed until `2426dcc`, three commits later. The F1 bullet called `--all-features` "provably the ceiling as the union of every feature" — overstated; narrowed to match `2426dcc`'s own scoped wording (the measured maximum of the current additive-cfg field layout, not an eternal theorem). Added an append-only correction block for `2426dcc` itself (never previously given an entry) and the new "Known limitations" subsection above, fulfilling `docs/CORRECTNESS_OPEN_ITEMS.md` items 16/19's own stated action ("state this in the release notes") — both items annotated `RESOLVED` in place. Commits `f43600d` + `9129ba7` (K2/task #597).
- **[docs, P1] Fixed three stale/misleading documentation surfaces.** `CONTRIBUTING.md` named two `src/byte/` files and a `byte` Cargo feature that do not exist, a nonexistent `tests/loom_reclaim.rs`, a nonexistent fuzz target `fuzz_alloc_dealloc`, and a hand-duplicated "mandatory commands" list that predated the `internals` feature and the 6th clippy row — replaced with pointers to the real README unsafe-inventory, the real 14 `loom_*.rs`/3 fuzz targets, and `npm run check` as the single source of truth. Commit `5db488c` (K12/task #607). `SECURITY.md` asked reporters about a nonexistent `byte` feature; `README.md` promised an email contact `SECURITY.md` never provided — fixed the feature vocabulary and removed the unbacked email promise, leaving only the real GitHub Security Advisory channel. Commit `6391497` (K13/task #608). `CLAUDE.md`'s live "Before every push" line still said "all five" clippy rows (six exist since R34-3); fixed only that normative line, deliberately leaving the two historical Round-31→33 incident mentions of "five" untouched (they correctly describe that era's actual row count, per this file's own non-retroactive convention). Commit `c9c7341` (K14/task #609).
- **[docs, P3] Two housekeeping closures.** `43115cf`/`5c1142f`'s `verify-commit-prefixes` taxonomy violations (`docs/CORRECTNESS_OPEN_ITEMS.md` item 21) are now published history — today's push moved them behind `origin/main`, so the default lint range no longer contains them and the gate passes without a rebase; recommendation reversed from "rebase when convenient" to "leave as accepted debt," since rewriting them now means rewriting published history for a cosmetic prefix. Commit `b8d6235` (K6/task #601). `.claude/` (local tool state, including this machine's own filesystem paths) was untracked and blocking `cargo package` from running without `--allow-dirty`; gitignored. Commit `503f703` (L6/task #619).
- **[process note] Independent verification (`docs/reviews/2026-08-05-fh-release-readiness-verification-review.md`) caught two of the prior two reviews' own diagnostic errors before they became misdirected work.** Both reviews recommended "add serialization" for `docs/CORRECTNESS_OPEN_ITEMS.md`'s open concurrency-flake items 12/14 (`xthread_large_double_free_no_double_reclaim`, `xthread_large_free_tiny_size_huge_align_is_reclaimed`) — verification found BOTH affected test files already fully serialize every test via a `SerialGuard`/`AtomicBool` spin-lock (3/3 and 5/5 tests respectively), refuting the "missing lock" hypothesis; the real, still-open question is why a fully-serialized run once observed 42 of an expected 50 reclaims (task #605/K10, rescoped from a mechanical fix to an investigation, deferred as non-release-blocking — not reproducible on demand). Separately, the same two reviews' recommended R6/commit-prefix "rebase decision" was already moot by the time of writing (see the K6 bullet above) — verification caught the premise change before a rebase was attempted. Both corrections folded directly into the task descriptions rather than left as review-only findings.

Deferred, by explicit owner decision, to a dedicated pass immediately before tagging (not part of this sprint): the crates.io publish DAG (three path dependencies — `racy-ptr-cell`, `size-classes`, `tagged-index-stack` — are not yet published and have no release-workflow targets; `aligned-vmem`'s local `0.2` requirement exceeds its published `0.1.0`; `numa-shim`'s published version has drifted from the local tree) — tasks K3/K4/K9/L2/L3/L5, tracked and unchanged. Deferred to post-release hardening (does not block 0.3.0): the reclaim-shortfall investigation above (K10), tier-1 unsafe-seam miri/loom coverage (K11), panic/unwind guard completeness (K15), two Kani arithmetic proofs (K16), a deeper MSRV gate (K17), crates.io trusted publishing (K18), and a remaining item-numbering cleanup in `docs/CORRECTNESS_OPEN_ITEMS.md` (M2).

#### Publish-readiness sweep for the 6 crates.io sub-crates (2026-08-06, tasks #635-651)

Six independent `@oh` readonly review agents (one per sub-crate the release-readiness sprint's deferred publish DAG covers — `aligned-vmem`, `numa-shim`, `sefer-region`, `racy-ptr-cell`, `size-classes`, `tagged-index-stack`) each audited their crate for crates.io publish readiness. All six returned **GO-WITH-FIXES**, with several REPRODUCED (not hypothetical) defects; every finding was filed as a task (#635-651) and fixed via `@sh` delegation with full personal zero-trust re-verification (every diff read in full, every claimed command output independently re-run, and for the two highest-severity fixes a genuine counterfactual independently reproduced in a scratch crate, not just trusted from the sub-agent's own report). **Runtime improvements: 0** — every fix is a source-level correctness/documentation/CI-infrastructure change on crates not yet part of `sefer-alloc`'s own runtime; no `production` default or shipping algorithm changed, and no crate was actually (re-)published (no `Cargo.toml [package] version` was touched anywhere in this wave — the actual publish DAG, including the required `numa-shim` 0.1.0→0.2.0 bump, remains explicitly deferred to task K3/#598's dedicated pre-release pass, gated behind an explicit user go-ahead per this project's standing "never bump versions without being asked" rule).

- **[fix, P0] Four reproduced correctness defects, one per crate, each closed with a genuine regression test.** `numa-shim`: `src/lib.rs`'s macOS platform stub was missing the `not(miri)` guard its three sibling platform blocks all carry, so macOS+miri simultaneously satisfied both it and the separate `#[cfg(miri)] mod platform` stub — `mod platform` defined twice (E0428), undetected because no CI job ever crossed macOS×miri; fixed the cfg and added a new `numa-shim-macos-miri` CI job (the actual missing matrix cell). Commit `dc003c9` (task #635). `racy-ptr-cell`: `dbg_rollback_reenterable`'s step-4 restore-to-null fired unconditionally even when step 3's postcondition CAS failed because a real `get_or_try_init` caller had legitimately re-won the cell mid-probe — clobbering that caller's sentinel while it was still running `init`, letting a second caller run `init` again and breaking both "exactly-once init" and "same pointer for all observers." Gated the restore on the CAS having actually re-won the cell; added a loom test racing the probe against two real callers, counterfactual-verified (fails with the pre-fix unconditional store, passes with the fix). Commit `17f5693` (task #636). `size-classes`: `Params::extras`'s three documented preconditions (strictly increasing, multiple of `min_block`, disjoint from the geometric run) were asserted nowhere — reproduced both a misaligned-block bug (a non-`min_block`-multiple extra silently merges into a valid-looking sorted position) and a table-collision bug (an overlapping extra permanently orphans a class slot). Added ~8 lines of `const`-eval asserts closing both, landed before this crate's first publish (a breaking-change window that closes the moment it ships). Commit `8529c0f` (task #637). `tagged-index-stack`: `push`'s documented contract (`index < INDEX_MASK`) is insufficient for `INDEX_BITS` in `33..=63`, where `INDEX_MASK` exceeds `u32::MAX` and `index == u32::MAX` (the internal `TAIL` sentinel) silently passes the guard, truncating the free-list chain — reproduced at `INDEX_BITS=40`. Closed structurally by capping `INDEX_BITS` at `1..=32` at compile time (rather than patching the one runtime guard) since `push` takes a `u32` anyway, so widths above 32 buy no reachable index range. Commit `d78625b` (task #638).
- **[build, P0] CI never compiled/tested 4 of these crates' own feature/target combinations standalone.** `tagged-index-stack`'s only CI invocation excluded 13 of its 17 tests (both non-loom test files, including the new F1 regression above; independently recounted at current HEAD for task #654/P4-10: `stack_unit.rs` 9 + `regression_counter_wrap.rs` 4 = 13 non-loom, plus `loom_aba.rs` 4 loom tests = 17 total); `aligned-vmem`'s `huge-pages` feature and 3 of 4 test files had never been compiled by CI on any OS; `size-classes` and `racy-ptr-cell`'s advertised `no_std` claims were never checked standalone (only the root crate's default-features-off build was). Closed all four by extending the existing `test-workspace` job rather than adding new jobs (`cargo test -p tagged-index-stack`, `cargo test -p aligned-vmem --all-features`, and standalone `thumbv7em-none-eabi` builds for the latter two). Commit `6fc2f1b` (task #639). Separately, `release.yml` had no tag pattern or `workflow_dispatch` option for `racy-ptr-cell`/`size-classes`/`tagged-index-stack` — added all three (the actual concrete deliverable behind the long-standing K3/#598 finding); confirmed via `actionlint` + hand-traced bash parameter expansion that none of the three crate names collides with the tag-parsing pattern. Commit `2a75d91` (task #648).
- **[docs, P1] Ten doc/metadata-accuracy findings across the six crates, each independently confirmed against the actual current code before being called "fixed."** `aligned-vmem`: `huge-pages`'s doc falsely claimed macOS `VM_FLAGS_SUPERPAGE` support (actually an empty no-op there), `decommit`'s `# Safety` omitted that a pre-`recommit` write is a hard Windows `STATUS_ACCESS_VIOLATION` (the exact divergence `docs/CORRECTNESS_OPEN_ITEMS.md` item 6 already records as a real incident), and the crate description overclaimed "over-reserve + trim" when Unix now tries an exact-size mmap fast path first — commit `ebe615d` (task #640); two more copies of the same over-reserve overclaim (in `reserve_aligned`'s own rustdoc and the README table) were missed by that fix and caught by the closing audit — commit `0a42519` (task #650). `sefer-region`: the crate-root rustdoc still claimed the compiler rejects "cross-region handle confusion" (false — the `PhantomData<fn() -> T>` branding is by value type, not by `Region` instance; a `Handle<T>` from one `Region<T>` is silently accepted by an unrelated `Region<T>` of the same type) — README already carried the honest wording from an earlier commit, but this was the last surviving instance; fixed and added an executable demonstration test. Commit `b17ffab` (task #641). `racy-ptr-cell`: added a missing `# Panics` section to `new()`, two missing `// SAFETY:` comments the crate's own top-level doc promised every unsafe site would carry, and `#![deny(missing_docs)]`. Commit `9ecada3` (task #642). `tagged-index-stack`: documented a real, previously-unstated portability limit — the crate's `AtomicU64` head requires `target_has_atomic = "64"`, absent on several common embedded targets — with a `compile_error!` guard giving a named-reason failure instead of a cryptic unresolved-import error; fixed one broken intra-doc link. Commit `300b41f` (task #643). `aligned-vmem`/`numa-shim`: neither had `[package.metadata.docs.rs]`, so a published docs.rs render would show zero optional features (commit `7e1020f`, task #644); both falsely carried the `no-std::no-alloc` crates.io category despite using `std` unconditionally (commit `19698da`, task #645; `tagged-index-stack`'s own genuinely-accurate claim was deliberately left unchanged). `aligned-vmem`: 4 broken/private intra-doc links, a missing `#[deprecated(since = ...)]`, a crate-wide `allow(dead_code)` narrowed to the specific per-platform helpers that actually need it (checked against all 3 platform blocks), and the root `Cargo.toml`'s own two remaining consumers of a deprecated feature alias migrated to the current name. Commit `4c059fa` (task #646). `size-classes`: two test-module doc comments overstated proptest coverage (claiming `(min_block, growth, geo_count, extras)` were property-generated when only `(size, align)` are — the scheme parameters are `const` generics and cannot themselves be proptest inputs); corrected both. Commit `c8498cd` (task #647).
- **[docs, P2] Closing completeness audit (task #650) verified the fixed state rather than trusting the individual fixes' own claims, and pinned doc-coverage as a compiler gate where it was missing.** Confirmed all 6 crates' `cargo doc --all-features --no-deps` zero-warning, all new doc text from the fixes above genuinely present (not reverted or left as a stale TODO), and every crate's own non-loom tests reachable from CI. One of the audit's own four "missing `#![deny(missing_docs)]`" findings was itself wrong (`aligned-vmem` already had it) — caught during the follow-up task's own required pre-check and independently re-confirmed rather than propagated; the other three (`sefer-region`, `size-classes`, `tagged-index-stack`) were all already at 100% doc coverage, so pinning was a zero-new-content one-line addition per crate. Commit `7c8621f` (task #651).

#### Post-sprint housekeeping (2026-08-06, tasks #605/#606/#611/#612/#623/#631/#655)

Continuation of the release-readiness sprint's deferred post-release items (K10/K11/K16/K17/M2), plus a review-flagged test-order overclaim (S8) and a user-requested workspace cleanup, worked one at a time with the same personal zero-trust verification standard (every diff read, every test/clippy/fmt result independently re-run). **Runtime improvements: 0** — every change below is a diagnosis, a doc/index correction, a CI-coverage addition, a test-only fix, or dead-workspace-member removal; `production`'s feature composition and every shipping code path's observable behavior are unchanged.

- **[docs, P2] K10 (task #605) — root-caused the 42/50 reclaim-shortfall anomaly R28-2/R29-1 had already fixed, closing `docs/CORRECTNESS_OPEN_ITEMS.md` item 12 by diagnosis rather than a new code change.** Traced the shortfall to the same window-vs-cumulative-counter class R29-1 (task #432) already fixed for a sibling test; confirmed the fix commit predates the flake's last observed occurrence via `git merge-base --is-ancestor`, and reproduced 5 clean reruns. No source change.
- **[test, P2] K11 (task #606) — closed 2 real "test exists but CI never runs it" gaps and recorded an accepted-risk decision for the 2 genuinely-uncovered tier-1 `unsafe` seams (item 17).** Wired `segment_directory_a5_miri` and `remote_fanin`'s miri-only harness into the `miri-core` CI job (previously compiled but never executed in CI — the same "pass by absence" class this project has fought before); found and fixed a shared-positional-test-filter bug while wiring them (`cargo test --test A --test B <filter>` applies the filter globally, silently zeroing `reclaim_offset_unit`'s own run) by splitting into separate steps. `global::sefer_alloc`/`global::fallback` remain deliberately uncovered by miri/loom/kani — reasoned accepted risk (their `unsafe impl GlobalAlloc` boundary and one-time process-lifetime fallback init are structurally awkward to model-check), not an oversight.
- **[test, P2] K16 (task #611) — added 6 new Kani proof harnesses and discovered Kani had never actually run in CI at all (13 pre-existing harnesses were also unverified-by-CI before this).** `ring_wrap_proofs` (2 harnesses: wrapping-sub advance-count recovery, full-check boundary) and `ring_entry_pack_proofs` (4 harnesses: pack/unpack round-trip and no-collision-with-empty-sentinel, both non-hardened and hardened). Verified via real `cargo kani` runs in WSL2 (Kani does not compile on Windows), including one counterfactual (a deliberately mutated proof reproduces `FAILURE`, reverted, reverified `SUCCESS`). Added a new `kani proofs` CI job — the actual missing coverage, since none of the 19 harnesses (13 pre-existing + 6 new) had ever been checked by CI before this commit.
- **[build, P2] K17 (task #612) — strengthened the MSRV gate with a build-only test-graph check.** `cargo check --all-features` never compiles `#[cfg(test)]`-only code or dev-dependency-only paths; added `cargo test --no-run --all-features` as a second step in the `msrv` job (verified feasible first: the full dev-dependency graph compiles clean under 1.88, ~6 minutes build-only). Narrows, but does not fully close, `docs/CORRECTNESS_OPEN_ITEMS.md` item 19's caveat — tests are now compiled but still not executed on 1.88.
- **[docs, P3] M2 (task #623) — renumbered 10 real item-number collisions in `docs/CORRECTNESS_OPEN_ITEMS.md`'s independently-numbered "Open items"/"Recently resolved" sections (I6/task #584 had fixed only 2 of them, and 4 new ones appeared since as the open section grew past the numbers I6 freed up).** Also fixed a discovered live cross-reference bug (two pointers calling the same closed entry "item 15" while its own heading read "16", a drift from an earlier insertion-time collision with a neighboring entry). Doc-only.
- **[fix, P3] S8 (task #631) — stopped a regression test from assuming `libtest` execution order.** `tests/regression_free_path_chunk_oom_graceful.rs`'s two `#[test]` fns shared a `Mutex` for mutual exclusion only, not ordering; if the order-dependent test happened to run first, its check silently passed as vacuous (proving only that a flag is clean at process start) while two doc comments claimed a guaranteed order that `libtest` never provides. Added a `MAIN_TEST_RAN: AtomicBool` gate so the dependent test explicitly skips its check (with an explanatory message) instead of running it vacuously when order isn't as expected; verified the skip path fires correctly by running the test in isolation. Test-only fix.
- **[build, P4] Removed the unused `ring-mpsc` workspace member (task #655, user-requested).** The crate (a standalone bounded MPSC index ring + `DirtyRouter`) had zero production consumers: its one filed use case, swapping the shipping in-tree `RemoteFreeRing`/`HeapOverflow` rings onto it, was investigated and independently found NO-GO on both tiers (`docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md`, commit `d062798`) — incompatible cursor layout for the raw tier, and a protocol-inversion blocker for the two-tier inline+sidecar tier. Deleted `crates/ring-mpsc/` entirely; removed its workspace-member entry, its dedicated `loom_ring_mpsc` CI step and `scripts/loom.mjs` mapping, its 4 `dbg_*` allowlist entries in `tests/dbg_hook_safety_tripwire.rs`, and every crate-count/crate-list mirror across `Cargo.toml`, `src/lib.rs`, `README.md`, and `docs/ARCHITECTURE.md` (the "eleven companion crates" count is now ten; the README tier-1 unsafe-seam count dropped 20→19, caught by `tests/no_stale_doc_references.rs`'s own drift check before it could land stale). Updated `docs/CORRECTNESS_OPEN_ITEMS.md` item 24's current-state card to reflect one fewer unpublished workspace member to track. Full verification: `cargo build --all-features`, `cargo test --features "production internals"`, `cargo test --all-features`, `cargo clippy --all-features --tests -- -D warnings`, `cargo fmt --check`, and `npm run check` all green; `Cargo.lock` regenerated with the crate's entry removed. The seven in-tree loom models covering the shipping `RemoteFreeRing`/`HeapOverflow` protocols (unaffected by this removal, since the crate was never wired onto them) are untouched.

#### `sefer-region` correctness/safety/performance sweep (2026-08-06/07, tasks #656/#664-673)

Applied this project's own published crates.io tools (`bench-scale-tool`, `captrack`) to `crates/region/` (package `sefer-region`, one of the six standalone sub-crates), which surfaced enough real findings to justify a full audit: three independent parallel `@fh` research agents (performance / logic-correctness / safety) each read the whole crate and wrote findings to `docs/reviews/2026-08-07-sefer-region-{performance,logic,safety}-review.md`; a follow-up `@oh` agent arbitrated the reports' one apparent contradiction (both were factually correct, judging the same measured `clear()`-under-panic behavior against different bars — API contract vs. memory/invariant soundness), confirmed the most severe finding at full scale, and filed tasks #664-673 with a landing order in `docs/reviews/2026-08-07-sefer-region-work-plan.md`. Nine of those ten tasks were implemented via sequential `/crush` delegation, one task at a time with a commit between each, every result personally zero-trust re-verified (full diff read, tests/clippy/fmt/doc independently re-run, and for several tasks a genuine counterfactual injected and confirmed to fail before being reverted) — six of the nine delegated fixes had at least one real issue caught and corrected during that review before being committed (see below). **Runtime improvements: 0** — every fix is documentation, tests, CI coverage, or a benchmark harness; `sefer_region::Region`/`SyncRegion`'s own public API and behavior are unchanged, and the crate has zero internal runtime consumers in this workspace.

- **[correctness fix, docs, P1] Task #664 (commit `395258e`) — rewrote a false "Generation saturation" claim.** `Region`'s struct doc claimed `slotmap` "retires" a slot once its 32-bit generation counter saturates; `slotmap` 1.1.1 has no retirement code at all — it silently WRAPS. Independently re-reproduced the resulting ABA alias at full scale (a tight insert/remove loop on one slot for `2^31 - 1` cycles, ~12.4s release-mode) and confirmed a stale handle becomes bit-identical to a fresh one, letting `remove(stale)` steal the fresh handle's live value — a real logic/aliasing defect, not memory unsafety (`slotmap` never corrupts its own structure, even post-wrap). Fixed docs-only across `region.rs`, `lib.rs`, `README.md`, `fuzz/`, and the two root workspace docs whose separate (non-`slotmap`) M8 generational-coherence entry a first delegated pass had wrongly copy-pasted the same "2^31" figure onto — reverted to a hedged statement for that unrelated mechanism instead of asserting an unverified number.
- **[bench, P1] Task #665 (commit `67062b3`) — fixed a bench harness fidelity gap before the README perf table shipped.** The `insert`/`remove` rows had been published as if they were steady-state numbers when `bench_scale_tool::Harness::bench_batched`'s per-iteration setup/routine split (its `routine` inherently takes state by value, dropping the fixture inside the timed window) means they in fact measure a full fresh-map allocation + teardown cost each time — the defect was the missing label, not the measurement itself. Relabeled those rows explicitly (e.g. "cold: fresh map, allocation + full teardown inside the timed window") and added `st/churn`/`sync/churn`/`raw/churn` steady-state workloads alongside them so both regimes are now on the record, additive to (not a replacement of) the existing batched arms.
- **[build, CI, P1] Task #667 (commit `6ace228`) — added the missing no_std CI build.** `sefer-region`'s README advertised `--no-default-features` no_std support that no CI job ever actually built; added a `cargo build -p sefer-region --no-default-features --target thumbv7em-none-eabi` step next to the existing `size-classes`/`racy-ptr-cell` bare-metal pair, and fixed a stale comment above it that had (correctly, for those two crates, but not for this addition) claimed no crate in that block declares a std-disabling feature.
- **[correctness fix, docs, test, P1] Task #666 (commit `0931d35`) — documented `clear()`'s partial-clear-under-panic behavior and pinned it with a test.** If a value's `Drop` impl panics mid-`clear()`, `slotmap`'s `Drain::next` completes each removal's bookkeeping BEFORE dropping that value, so the clear is partial but never corrupts the container — values already visited (including the panicking one) are removed, later values remain live and correctly accounted. Added this caveat to `clear()`'s doc and strengthened `SyncRegion`'s poisoning-policy doc (container-integrity-only guarantee), plus two new regression tests (main-thread `catch_unwind` and a spawned-thread poison-recovery variant) exercising a `DropCounter` that panics on a specific drop.
- **[test, P1] Task #668 (commit `cec0333`) — closed two zero-coverage gaps: I5 (drop-once) and `clear()`'s happy path.** Neither invariant had a single test. Added 15 tests covering drop-exactly-once across `remove`/`Region`-drop/a mixed insert-remove sequence (both `Region` and `SyncRegion`), `clear()`'s happy path, and assorted previously-uncovered methods (`iter_mut`, `get_mut`, `Default`, `with_capacity`, `reserve`). Personally injected a counterfactual (`std::mem::forget`) into the drop-count test and confirmed it fails before reverting. (A 16th test, `region_reserve_overflow_panics`, landed separately in task #669 below.)
- **[correctness fix, docs, P2] Task #669 (commit `ecc5138`) — corrected the panic contracts on `reserve`/`with_capacity`/`insert`.** None of the three documented when they actually panic; added `# Panics` sections describing the release/debug arithmetic-wrap divergence in `reserve` (silently wraps near `usize::MAX` in release with overflow-checks off, panics in debug) and `insert`'s `2^32 - 2` full-map panic, plus a new `region_reserve_overflow_panics` regression test verified in both debug and `--release`.
- **[docs, P2] Task #672 (commit `ffb6813`) — four docs.rs-facing polish fixes.** Fixed a dangling bare-relative-path link to the workspace-root `docs/BENCHMARKS.md` (dead on docs.rs, since that file ships outside the published crate tarball) to the full GitHub URL in two files; removed a "Phase 3b" parent-workspace-internal reference leaking into `SyncRegion`'s published rustdoc; corrected `get_cloned`'s opening line, which overclaimed the clone happens "without holding a guard" when the implementation does hold one internally — the accurate claim (the CALLER doesn't end up holding one afterward) was already the following sentence; and added a staleness note to `contains()` matching #664's already-qualified "roughly `2^31` reuse cycles" wording, deliberately not a fresh unconditional "never" claim.
- **[test, P2] Task #670 (commit `185df1b`) — de-vacuumed the I3 stale-handle test and pinned `Handle<T>`'s layout/auto-trait claims with static assertions.** `region_stale_handle_returns_none` previously never checked that slot reuse actually happened before asserting the stale handle resolves `None` — would have kept passing even if `slotmap`'s freelist policy stopped reusing slots. Added a new `handle_static_asserts.rs` with `const _: () = assert_send_sync::<Handle<T>>()` compile-time checks (against types that are themselves `!Send`/`!Sync`/neither) pinning `Handle<T>`'s unconditional-Send+Sync claim, plus `const _: () = assert!(size_of::<Handle<u8>>() == 8)` and the same for `Option<Handle<u8>>`, both independently verified to fail to COMPILE (not just fail a test) when temporarily given a wrong value. **Correction (task #678, commit `39704e1`, same day):** #670's own reuse-discriminating assertion (capacity/len bookkeeping, "verified" by routing the second insert to a separate `Region`) did NOT actually discriminate slot reuse from ordinary growth-free insertion — `slotmap` pre-allocates, so `capacity()` stays flat across a second insert with zero prior removal regardless, and the cited counterfactual only ever exercised an unrelated `len()` assertion positioned immediately after the capacity oracle in the same test. Caught by an independent closing review (`docs/reviews/2026-08-07-sefer-region-round-closing-review.md`, finding D) and fixed by comparing the slot INDEX component of `Handle`'s `Debug` output (`{idx}v{version}`, forwarded from `slotmap::DefaultKey`) between the old and new handle — the only way to observe slot identity from outside the crate, since the `key` field is `pub(crate)`. Re-verified non-vacuous in isolation from the surrounding assertions this time: with the `remove()` call commented out (so the second insert lands on a fresh slot) and the `len()` assertion also disabled, the slot-index comparison alone correctly failed (`left: 1, right: 2`), then both were reverted.
- **[bench, docs, P2] Task #671 (commit `81290fd`) — measured iteration cost over tombstones and documented that capacity never shrinks.** `slotmap` 1.1.1 has no shrink/compact operation of any kind, so `Region::iter`'s per-sweep cost is bounded below by the historical high-water mark of live entries, not the current live count — undocumented and unmeasured beyond the zero-holes best case. Added two new `bench-scale-tool` workloads (`st/holey_sweep`: 1,000 live in a ~2,000-slot array; `st/sparse_sweep`: 1,000 live in a ~10,000-slot array) and documented the permanence near `Region::capacity`/`clear`/`iter`. **Process note:** the delegated fix's own final report cited three internally-inconsistent number sets for the same two workloads (its prose, its own pasted verification output, and the number it actually wrote into the README all disagreed) — caught by independently re-running each new workload 3x, and the README now cites the resulting self-measured median-with-range (`st/holey_sweep` 2,476 (2,228–2,510) ns/op, `st/sparse_sweep` 11,482 (10,955–11,845) ns/op), matching the table's own established format.
- **[deferred] Task #673 — filed, deliberately NOT implemented.** A contended `SyncRegion` measurement, per the work-plan's own explicit recommendation ("no defect claimed or found... blocks nothing"); left pending as a future decision gate, not silently dropped from the record.
- **[process] Established a local, gitignored `/research` skill** (`.claude/skills/research/SKILL.md`, not tracked by git) codifying this round's parallel-research-agents pattern (single-message concurrent launch, each agent's output strictly file-only, reply channel restricted to `done: <path>`) for reuse in future rounds.

**Closing-review remediation (2026-08-07, tasks #678-684)** — an independent `@oh` review of the sweep above (`docs/reviews/2026-08-07-sefer-region-round-closing-review.md`) verified every commit against its own message and found 7 residuals, all fixed same-day. #678 (finding D) is folded into the #670 bullet above (the fix that actually made that task's own claim true). The remaining six:

- **[docs, P3] Task #679 (commit `9fcbbf1`, finding A) — `docs/INVARIANTS.md` still carried the exact fabricated `remove_from_slot` mechanism task #664 said it had corrected.** `slotmap-1.1.1/src/basic.rs` bumps the generation exactly once inside `remove_from_slot`; the second `+1` per occupy/free cycle comes from `insert`'s `version | 1` on an even/vacant slot, a different function — #664 fixed this in `region.rs`'s two occurrences but missed the identical claim in `INVARIANTS.md`. Corrected.
- **[docs, P4] Task #680 (commit `9fcbbf1`, finding B) — `docs/PLAN.md` had an orphaned text fragment and broken list indentation** left over from an earlier string replacement. Cosmetic; fixed.
- **[docs, P3] Task #681 (commit `9fcbbf1`, finding C) — `tests/region_invariants.rs`'s doc comment still asserted the absolute "a removed handle is `None` forever" I2 claim**, the last surviving unqualified instance workspace-wide (#664's own re-sweep pattern hadn't covered the root `tests/` tree). Qualified to match the wrap-aware wording used everywhere else.
- **[docs, P3] Task #682 (commit `aa24f84`, finding E) — the CHANGELOG's own #665 bullet overstated what shipped**, claiming `bench_batched`'s setup/routine split "had been misused... corrected to the harness's intended usage"; the six existing `bench_batched` arms were left completely untouched by `67062b3` (no deletions in that hunk) and still time fixture teardown by design. Reworded to match `67062b3`'s own more accurate commit message: a relabel plus three new additive steady-state arms, not a correction.
- **[docs, P4] Task #683 (commit `aa24f84`, finding F) — the CHANGELOG double-counted a test between the #668 and #669 bullets** ("Added 16 tests total" for #668, when `cec0333` added 15 — the 16th, `region_reserve_overflow_panics`, landed separately in `ecc5138`/#669, whose own bullet also correctly claims it). Fixed the count.
- **[docs, P4] Task #684 (commit `aa24f84`, finding G) — an unexplained numeric disagreement in the `sync/churn` spread** between `67062b3`'s commit message ("~69.6-84.2 ns/op") and the published README table ("76.0 (72.1–84.2)"). A third independent re-measurement (83.60-86.17 ns/op) confirmed this specific workload's absolute figure drifts session-to-session on this dev host more than the table's other rows; added an explanatory note rather than publish a fourth number chasing a stable ground truth that doesn't exist here.

#### `sefer-region` follow-up remediation — reentrancy, contended reads, and 3 more publish-blocking findings (2026-08-07, tasks #685-693)

A further `/research @fh sefer-region` sweep (3 parallel performance/logic/safety agents, explicitly scoped to find NEW content beyond the settled ground above — `docs/reviews/2026-08-07-sefer-region-followup-{performance,logic,safety}-review.md`) plus a `/rust-intel` fan-out audit (`docs/reviews/2026-08-07-sefer-region-rust-intel-audit.md`) together found 9 more real findings, all implemented via sequential `/crush` delegation with full personal zero-trust re-verification. **Runtime improvements: 0** — every fix below is documentation, a test, a `#[repr(transparent)]` layout guarantee, or a `checked_add` overflow guard; no shipping algorithm or `production` default changed (`sefer-region` has zero internal runtime consumers in this workspace either way).

- **[docs, P1] Task #685 (commit `25de4cd`, `SyncRegion`'s contended-read anti-scaling) — documented that one-shot reads under `SyncRegion` measurably ANTI-scale** (28.5 Mops/s at n=1 down to 7.0 Mops/s at n=8, a 4.1x total-work loss) and that guard-batching restores ~30x (195.9-207.8 Mops/s flat). Added `crates/region/examples/contended_reads.rs` as a reproducible `cargo run --release` probe. A delegated first draft's aggregate-Mops/s formula conflated summed per-thread latency with wall-clock duration, producing physically-impossible numbers (~30 million Mops/s) — caught by inspection, fixed to use `max_elapsed_secs` across `Barrier`-aligned threads, and personally re-measured 3x before the README table was published. This decides task #673's own deferred future-decision-gate: closed via this example plus a genuine concurrency test (see #693 below), superseding the need for a separate measurement task.
- **[docs, P1] Task #686 (commit `127545b`) — documented that `get_cloned`'s `T::clone` call runs UNDER the read lock**, extending any writer's stall by the full clone duration (measured ~1.5-1.8ms worst-case for a 4 MiB payload) — the prior doc recommended `get_cloned` for exactly the case (expensive-Clone payloads) where it costs the most; added the measured stall and a `Arc<T>`-instead recommendation.
- **[docs, P1] Task #687 (commit `5e4244f`) — documented `SyncRegion`'s reentrancy self-deadlock**: `get_cloned`'s `T::clone` and `clear`'s `T::Drop` both run under the lock, so a `T` that re-enters the SAME `SyncRegion` (directly or via a guard held across a second one-shot call) deadlocks — reproduced with an exact repro pattern by two independent reviews plus the rust-intel audit. Promoted `remove`'s existing drop-outside-lock behavior to a stated contract (the one operation immune to this class) and added a guard-type semver note to `read()`/`write()`.
- **[correctness fix (docs), P1] Task #688 (commit `ec59520`, found independently by THREE agents: 2 follow-up reviews + the rust-intel audit's own §F1) — fixed a false poisoning-policy claim.** The doc said "A panic while a guard is held poisons the `RwLock`" — std only poisons on a WRITE-guard panic; a panicking `T::clone` inside `get_cloned` (a read guard) releases cleanly with no poison and no effect on the stored value. Reworded to state this precisely.
- **[docs, P2] Task #689 (commit `5985a61`) — `Region`↔`SyncRegion` doc-symmetry pass** (matching complexity/panics/reentrancy documentation between the two types where one had it and the other didn't) plus two minor doc nits.
- **[correctness fix, P1] Task #690 (commit `df16693`) — closed a debug-vs-release panic-contract divergence in `reserve`/`with_capacity`.** Read `slotmap` 1.1.1's actual source to confirm the real overflow points (`Vec::with_capacity(capacity + 1)` for an internal sentinel slot; `(self.len() + additional).saturating_sub(...)`, both unchecked in release) and added `checked_add` guards so both functions panic identically in debug and release instead of silently wrapping in release. **The one real behavior change in this whole follow-up round** — the maintainer was explicitly asked and chose this option over doc-only.
- **[correctness fix, P1] Task #691 (commit `a243c38`) — added `#[repr(transparent)]` to `Handle<T>`**, converting its layout guarantee (8 bytes, `PhantomData<fn() -> T>` a 1-aligned ZST alongside the single non-ZST `DefaultKey` field) from "current-rustc-happens-to-produce-this" into a real, `rustc`-enforced guarantee, before first publish.
- **[test, P2] Task #692 (commit `ed008a5`) — tightened the `reserve`/`with_capacity` overflow-panic tests to pin the specific message**, not just `is_err()` (which any unrelated panic would satisfy). Hit a genuinely unexplained `TypeId` mismatch when downcasting a `catch_unwind` payload originating inside the `sefer-region` library crate from its own integration-test binary — root-caused partially (ruled out build caching, a custom panic hook from a dev-dependency, and reproduced the SAME construct working correctly in two isolated scratch crates) but not fully within task scope; worked around with a `std::panic::set_hook`-based message-capture helper (`catch_panic_message`) instead of the raw payload downcast, which sidesteps the mystery entirely. Not filed as a tracked open item.
- **[test, P1] Task #693 (commit `89913c6`) — closed 2 testing gaps.** `SyncRegion`'s "correct under any interleaving" doc claim was untested CONCURRENTLY — every existing threaded test spawned exactly one thread and joined it before any assertion ran. Added `sync_region_concurrent_insert_remove_get_is_consistent` (4 threads × 200 ops, `thread::scope`). A first draft's own drop-count assertion was wrong (expected `THREADS * OPS_PER_THREAD`, actual `left: 1600, right: 800` on first run) — `get_cloned` produces an independently-droppable clone separate from the original, doubling the expected count; fixed and verified non-vacuous via a `std::mem::forget` counterfactual. Also added a release-profile CI step (`cargo test -p sefer-region --release`) — the overflow-panic tests' "profile-independent" claim had only ever run in debug.

#### `sefer-region` round 2 — rust-intel audit remediation (2026-08-07/08, tasks #694-696)

Three remaining `docs/reviews/2026-08-07-sefer-region-rust-intel-audit.md` §D1a/§F2 info-level findings, implemented via sequential `/crush` delegation with full personal zero-trust re-verification (including a genuine counterfactual for #696, not just a delegated agent's own written claim). **Runtime improvements: 0** — all three are test/doc-only.

- **[test, P3] Task #694 (commit `ea52f85`, further corrected by #769) — made `clear_partial_under_panic`'s assertions order-agnostic.** The test originally asserted `slotmap`'s clear/drain visited values in a SPECIFIC order (ids 0,1,2 dropped; 3,4 survived) — but `slotmap`'s own `iter()` doc states iteration order is unspecified, and `Cargo.toml` pins `slotmap = "1"` (floats across 1.x minors), so a future minor bump changing internal drain order would have made this test go red as a false regression even though the crate's real order-free contract still held. #694 rewrote both `Region` and `SyncRegion` scenarios to compute the survivor/dropped classification from `iter()`'s actual output (via a `HashSet` complement) rather than hardcoded positions, but it kept the exact `drop_count == 3` / `len() == 2` pair, which still depended on drain order. #769 removed that final dependence and replaced it with the genuinely order-free invariant `drop_count + len() == 5` (total constructions = drops + survivors).
- **[docs, P4] Task #695 (commit `0373b28`) — qualified the "raw `DefaultKey`s never escape the crate boundary" claim.** `Handle`'s `Debug` impl renders the underlying key's index+version, and the crate's own test (`slot_index`, added by #678/#670) parses slot identity back out of exactly that string — true at the type level (no `DefaultKey` VALUE is obtainable in a form usable against a `Region`) but stronger prose than the code now visibly delivers. Qualified consistently across all three occurrences (README.md, `src/lib.rs`, `src/region.rs`).
- **[test, P3] Task #696 (commit `1f962e5`) — lifted a capacity-churn assertion out of an `#[ignore]`d manual probe into a real, non-ignored test, and deleted 2 runtime Send/Sync tests that checked nothing beyond compilation** (already pinned more precisely by the const `assert_send_sync::<...>()` asserts from #670). Net −1 test count, +1 real executable check of a previously probe-only claim. Personally verified the new test non-vacuous with a real counterfactual: temporarily over-inserted past the freed-slot count (500→700), confirmed the assertion correctly failed (`after_remove=1023, after_refill=2047`), then reverted — not just trusted from the delegated agent's own commit-message claim.

Deferred, unchanged by this round: the crates.io publish DAG for `sefer-region` and its five sibling sub-crates (tasks #656-661 — whether the correctness fixes above warrant a 0.1.1 patch republish, and whether the already-published 0.1.0's false safety claim needs any advisory consideration, is an explicit maintainer decision this round does not make); the root `sefer-alloc` crate's own `bench-scale-tool` integration (task #662, now split into a design-note gate plus a separate implementation task #763) and the four dev-only sub-crates' tooling wiring (originally task #663, since decomposed per-crate into tasks #765-768). The remaining 5 workspace sub-crates' own rust-intel audit findings (numa-shim, aligned-vmem, racy-ptr-cell, size-classes, tagged-index-stack — tasks #697-731) are queued next, one crate at a time, each gated behind its own `@oh` closing review before the next starts.

#### `tagged-index-stack` — rust-intel audit remediation (2026-08-08/09, tasks #698/#702-705)

Continuing the same one-crate-at-a-time `rust-intel` remediation sweep, implemented via sequential `/crush` delegation with full personal zero-trust re-verification (every delegated result independently diff-read, tests/clippy/fmt/loom/doc re-run, and for the two HIGH findings a genuine counterfactual injected — reverting the fix and confirming the regression test now fails for the right reason — before being committed). **Runtime improvements: 1** (task #698, a real concurrency-correctness fix to a hot-path atomic ordering, not a speedup); every other task is documentation, tests, or a compile-time-guard-coverage fix.

- **[correctness fix (HIGH), P1] Task #698 (commit `1485bb6`) — `pop`'s CAS-failure ordering was `Relaxed`, letting a retry read a stale `next` link through an unsynchronized head observation.** `TaggedIndexStack::pop`'s `compare_exchange` used `Ordering::Relaxed` for the failure case; on a failed CAS the loop retries and reads `links.load_next(index)` off the freshly-observed (failure-read) head — but a `Relaxed` failure ordering gives that read no happens-before relationship to the concurrent `push` that last wrote that link, permitting a stale read (missing happens-before, not tearing — `AtomicU32::load` cannot tear on any architecture) on architectures weaker than x86. Fixed by promoting the failure ordering to `Acquire` (success ordering was already `Acquire`), so both the initial load and every retry's failure-read establish the needed synchronizes-with edge. Added a new loom test, `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`, that calls the REAL `pop`/`push` end-to-end (not a hand-inlined copy) — personally verified via counterfactual: reverting the ordering back to `Relaxed` and re-running the full loom suite makes this specific test fail with a genuine duplicated-index corruption (`left: [0, 0, 1], right: [0, 1]`). Also fixed a stale `Relaxed` ordering in the pre-existing `aba_repush_forces_stale_cas_retry_and_stays_consistent` loom test's hand-inlined `cas_head_for_test` call, whose own comment claimed to mirror `pop`'s real orderings exactly.
- **[test (HIGH), P1] Task #702 (commit `c552704`) — repaired the loom ABA counterfactual harness, which was panicking on its own accounting bug, not genuine free-list corruption.** `counterfactual_untagged_head_lets_aba_corrupt_free_list`'s `#[should_panic]` fired because its harness hardcoded `popped.push(0)` on any `Ok` result instead of pushing the actually-popped index (`if let Ok(idx) = a_result { popped.push(idx); }`), AND its original 1-pop/1-repush-same-index scenario is structurally incapable of producing real corruption in a 2-slot free list (the link round-trips to its original value after the repush cycle regardless of ABA). Redesigned the scenario so thread B pops TWO indices and re-pushes only the first (modeling a live consumer holding one slot), making genuine resurrection reachable, and fixed the harness bug. Personally found and fixed two further bugs during review of the delegated draft: (1) an added companion test's own assertion assumed a fixed thread-scheduling order and failed on a valid interleaving where A completed first — replaced with a scheduling-independent conservation invariant (`dedup()` on the combined result set, asserting no length change = no duplicate index); (2) the hand-inlined thread-A body was missing an `is_empty` guard before `load_next`, causing a real out-of-bounds panic (`the len is 2 but the index is 65535`, this scenario's `ArrayLinks::<2>` at `INDEX_BITS = 16`) under a valid loom interleaving where B's now-2-pop scenario empties the stack before A reads it. **Correction (task #772, found by the round-closing review):** #702's own oracle fix — replacing the harness bug with a scheduling-independent conservation invariant — was applied only to the newly-added companion test, not to this bullet's `counterfactual_untagged_head_lets_aba_corrupt_free_list` itself, which still asserted a schedule-dependent `!popped.contains(&1)` and could panic on a benign, non-corrupting interleaving before the genuine ABA case was ever reached by loom. Fixed in task #771.
- **[correctness fix, test, P2] Task #703 (commit `17185d6`) — promoted `push`'s only index-validity guard from `debug_assert!` to `assert!`.** A caller passing `index >= INDEX_MASK` to `push` in a release build previously hit zero runtime checking, silently aliasing the reserved empty-sentinel value with a real slot — a real memory-unsafety-shaped violation (aliased heap slots) reachable from 100%-safe caller code, not merely a debug convenience. Consulted `@oh` per this session's "ask @oh, don't ask me" standing instruction: `@oh` recommended `assert!` over `Result`/docs-only, citing the negligible perf cost (the only real caller in this codebase is teardown-only, not a hot path) and existing project precedent for this exact debug_assert-vs-assert tradeoff class. Also tightened the existing regression test to downcast the panic payload and pin the message substring (`"index must be < INDEX_MASK"`) instead of a bare `result.is_err()`, which any unrelated panic would have satisfied — verified non-vacuous by temporarily deleting the `assert!` and confirming the test correctly failed on the resulting unrelated out-of-bounds panic message, then reverting.
- **[docs, P3] Task #704 (commit `96e28a7`) — decided `raw_head`'s API posture before first publish: `#[doc(hidden)]`.** `raw_head` is a `pub fn` reachable from `tests/` (an external crate from the library's own perspective) with zero production callers workspace-wide and no documented stability contract — publishing it to crates.io undecided would have made its exact return-value semantics an implicit semver commitment. Marked `#[doc(hidden)]` (test-only surface, not stable public API) per the user's explicit choice, with a README "Test-only diagnostic surface" section recording the policy.
- **[correctness fix, docs, test, P3] Task #705 (commit `d156e5c`) — closed 5 batched doc/coverage residuals.** Fixed the tag-budget contrast doc (README + crate doc), which understated the 32-bit-tag frozen-victim-churn window by ~1000x ("~43 s" corrected to `2^32 / 100_000 / 3600 ≈ 12 hours`, at the same 100k pushes/sec rate the existing 48-bit "~89 years" figure already uses); fixed `pack`'s doc, which claimed an out-of-range index "silently collides with the tag bits" when the `& INDEX_MASK` mask actually truncates it first (the real failure mode is a wrong index round-tripping out of `unpack`, not bitwise tag corruption); forced the compile-time `_CHECK_BITS` width guard (`INDEX_BITS` in `1..=32`) to evaluate from BOTH `pack` (already the case) AND `INDEX_MASK`'s own initializer, closing a real gap where `unpack`/`empty_index`/`is_empty` — all of which reference `INDEX_MASK` directly — could be reached with an out-of-range width without `pack` ever being called first; added a "# Stability" doc section to the `Links` trait declaring it intentionally open to external implementation (slot-resident links in caller-owned storage is the whole design point, not a sealed trait); and added `tests/proptest_pack_unpack.rs`, 4 proptest round-trip properties for `pack`/`unpack` at widths 1 (the degenerate case), 16, 31, and 32 — 64 cases each per this repo's fast-proptest convention — complementing `stack_unit.rs`'s existing hand-picked-literal coverage. The `_CHECK_BITS`-initializer change surfaced its own rustdoc regression during verification (`rustdoc::private_intra_doc_links` on a doc-comment markdown link to the now-more-referenced private `_CHECK_BITS` const) — fixed by unlinking to plain non-hyperlinked text at both occurrences; `cargo doc -p tagged-index-stack --no-deps` confirmed at zero warnings before commit.

**Closing-review remediation (2026-08-09, tasks #771-772).** An independent `@oh` review of the round above (`docs/reviews/2026-08-09-tagged-index-stack-round-closing-review.md`) verified every one of the 7 commits line-by-line against the crate's current source and re-ran the full test/loom/clippy/doc suite personally rather than trusting the commit messages — confirming the `#698` concurrency fix correct and sufficient, the `#703` `assert!` promotion genuinely non-vacuous, and the `#705` `_CHECK_BITS` widening doing exactly what it claims (probed against all six associated items) — but found one real defect: task #702's own audit finding was not actually closed.

- **[test (HIGH), P1] Task #771 (commit `3c69a21`, finding F1) — the untagged ABA counterfactual's oracle was itself schedule-dependent, so #702's fix never closed the finding it was meant to close.** `counterfactual_untagged_head_lets_aba_corrupt_free_list`'s `assert!(!popped.contains(&1), ...)` fires on loom's very first, entirely benign interleaving (thread A completes cleanly with no interposition) — and since loom aborts model-checking at the first panic, the genuine ABA-corrupting interleaving the test claims to prove was never actually reached. The reviewer personally demonstrated this in a scratch copy: the shipped test panicked on iteration 0 on a race-free schedule, while substituting the companion test's own conservation oracle showed the real corruption is reachable at iteration 15. Fixed by porting that same scheduling-independent `accounted`/`dedup` conservation invariant into this test (also fixing a pre-existing gap where B's held item was silently discarded instead of accounted for) — personally re-verified the fixed test still panics, but now on a genuine duplicate (`before_dedup=3, after_dedup=2`) rather than a benign non-corrupting schedule, and manually traced the specific benign schedule the review reproduced through the new oracle to confirm it no longer false-panics there.
- **[docs, test, CI, P2-P4] Task #772 (commit `7d8aab0`, findings F2-F11) — ten lower-severity residuals from the same review.** Strengthened both the untagged counterfactual's and the tagged companion test's conservation oracles from duplication-only (`dedup()` length comparison) to `assert_eq!(sorted, vec![0, 1])`, which additionally catches index LOSS (a stale CAS installing `empty` where `next` was a real index) that the weaker oracle could not (F7). Updated `Cargo.toml`'s published `description`, README.md, and the crate doc to say THREE `#[should_panic]` counterfactuals, not two — task #698 (commit `1485bb6`) added `counterfactual_relaxed_cas_failure_corrupts_free_list` and no doc site was updated, and `description` is worth getting right before the crate's first publish since it is effectively immutable per published version (F6). Softened `_CHECK_BITS`'s doc comment, which overclaimed no associated item could reach an out-of-range width unchecked — `TAG_BITS` never references `INDEX_MASK` and remains reachable, unguarded, at any width (F3). Fixed two wording defects in this CHANGELOG's own #698/#702 bullets: a cross-contaminated panic-message quote (F2) and an inaccurate "torn read" claim — `AtomicU32::load` cannot tear on any architecture, the defect is staleness only (F11). Added a `cargo test --release -p tagged-index-stack` CI step, since the `#703` `assert!` promotion's release-profile claim had no CI run proving it (F4). Added a `cargo build -p tagged-index-stack --target x86_64-unknown-none` CI step — the crate's `no-std::no-alloc` category claim had no bare-metal build proving it, unlike its `--no-default-features`-built siblings; `thumbv7em-none-eabi` (the target those siblings use) lacks 64-bit atomics, which this crate's head requires, so `x86_64-unknown-none` (verified locally to compile clean) is used instead (F5). Added hygiene comments/doc updates for the remaining informational findings (F8-F10).

Deferred, unchanged by this round: `tagged-index-stack`'s own crates.io first-publish decision (task #661, gated behind this round's closing `@oh` review, task #739, now genuinely closed after #771-772). The remaining 4 workspace sub-crates' own rust-intel audit findings (racy-ptr-cell, aligned-vmem, numa-shim, size-classes — tasks #697/#699-701/#706-731) are queued next in that order, one crate at a time, each gated behind its own `@oh` closing review before the next starts, per this sweep's standing one-crate-at-a-time instruction.

#### `racy-ptr-cell` — rust-intel audit remediation (2026-08-09, tasks #700/#706-710)

Continuing the same one-crate-at-a-time `rust-intel` remediation sweep; 5 of the 6 tasks (`#700`, `#706`, `#707`, `#708` per-task text below, plus `#709`'s own bullet) show a genuine zero-trust counterfactual (temporarily reverting the fix, confirming the associated test now fails for the right reason, then restoring it) — `#709`'s bullet is honest that it has no break/restore counterfactual by construction (re-verified by re-running the full loom suite, not by a revert), and `#710` is a docs/attribute change with no counterfactual to run; a round-closing review (task #774, finding F13, 2026-08-09) found this section's original "each of the 6 tasks" phrasing overstated by two and this sentence corrects it. **Runtime improvements: 0** — every fix below is a test, a compile-time/release-active guard promotion, or documentation; no shipping algorithm changed observable behavior on any success path (`#706`'s RAII guard only changes behavior on the unwind path, which was previously a livelock, not a defined success path).

- **[test (HIGH), P1] Task #700 (commit `0db373d`) — fixed a vacuous happens-before oracle in the Release-publish loom proofs.** `real_exactly_once_two_threads`/`real_exactly_once_three_threads` read `init_marker` only AFTER `t1.join()`/`t2.join()` — but `join()` itself establishes a happens-before relationship regardless of the cell's own internal publish ordering, so the assertion never actually exercised the `Release`/`Acquire` pairing it claimed to prove; the README's/`Cargo.toml`'s/test-module-doc's "pinned by executable loom proofs ... that fail without the correct code" claim was therefore false for exactly the Release-publish rule. Fixed by moving both marker checks INSIDE the racing threads' own closures, immediately after `get_or_try_init` returns and before any `join` — mirroring the crate's own pre-existing `ensure_relaxed_publish_broken_and_check` pattern for the shadow model. Personally verified via counterfactual: flipping `src/lib.rs`'s publish `Ordering::Release` to `Ordering::Relaxed` made BOTH tests fail with loom's `"Causality violation: Concurrent load and mut accesses"`, then reverted.
- **[correctness fix, test, P2] Task #706 (commit `048c657`) — added an RAII rollback guard so a panicking `init` can't wedge the cell forever.** The `INITIALIZING` sentinel was only rolled back to `null` on the explicit `None`/OOM return path; if `init` UNWINDS instead (a caller panic, or the `debug_assert!`/`assert!` on the publish path firing), the sentinel was stuck FOREVER — every concurrent loser busy-spins at 100% CPU indefinitely, and every future caller sees permanent `INITIALIZING`, a silent whole-process livelock strictly worse than `OnceLock`'s poison. Fixed with a new private `RollbackGuard<'a, T>` held across `init()`, defused on both non-unwinding exits (successful publish, explicit OOM rollback) so it only ever fires via normal unwind-through-scope `Drop` semantics. New test `panicking_init_rolls_back_and_subsequent_call_succeeds` catches the unwind with `catch_unwind`, then runs a SUBSEQUENT `get_or_try_init` on a background thread bounded by a 5-second `recv_timeout` (not a direct `join`, which would hang the test process if the fix regressed) — personally verified via counterfactual: short-circuited the guard's `Drop` store, confirmed the test fails after exactly 5.01s with the expected livelock message, then reverted.
- **[correctness fix, test, P2] Task #707 (commit `9b98c7a`) — promoted the sentinel-collision guard from `debug_assert!` to `assert!`.** A SAFE init closure can construct and return the exact `INITIALIZING` sentinel address (`NonNull::new(without_provenance_mut(1))`) — in release, `debug_assert!` compiled out the check entirely, so the sentinel would publish as if `READY` and every reader would misclassify it as still-initializing, spinning forever with no diagnostic; violates the method's own documented "never null, never the sentinel" guarantee from 100% safe code. Promoted to a release-active `assert!` (interacts cleanly with task #706's already-landed `RollbackGuard`: a panicking `assert!` here is itself an unwind, and the guard rolls the sentinel back regardless of which unwind path triggered it). New test `init_returning_the_sentinel_address_panics` pins the exact panic message via `#[should_panic(expected = ...)]`; personally verified via counterfactual: reverted to `debug_assert!`, confirmed the test fails under `cargo test --release` ("test did not panic as expected"), then reverted.
- **[test, P3] Task #708 (commit `b65c51a`) — closed two zero-coverage soundness-load-bearing contracts, both previously "delete the code and every test stays green."** (1) The `align_of::<T>() >= 2` runtime-panic guard (documented in `RacyPtrCell::new`'s own `# Panics` section) had no test — new test `align_of_one_payload_panics_at_construction` uses `RacyPtrCell::<u8>::default()` (the doc's own named runtime-panic route; the `const fn` `new()` arm fails to COMPILE for an align-1 `T` in a `static`, untestable here without a banned `compile_fail` doctest). (2) `dbg_rollback_reenterable`'s happy-path contract (`Some(true)` + the UNINIT restore postcondition) was asserted only in the PARENT repo's own integration test, which does not ship with the standalone published crate — new test `dbg_rollback_reenterable_happy_path_and_not_applicable_arm` asserts both the happy-path arm (fresh cell) and the not-applicable arm (already-READY cell, probe must leave it untouched). Personally verified BOTH counterfactuals per this task's own instruction: (1) short-circuited the align assert with `true ||`, confirmed the new test fails; (2) stubbed the probe to unconditionally `return None`, confirmed the new test fails (`left: None, right: Some(true)`); both reverted, confirmed clean via `git diff`.
- **[test, P3] Task #709 (commit `2e5fb72`) — fixed a provenance-losing int→ptr round-trip in a loom test's reclaim path.** `real_probe_rollback_does_not_clobber_concurrent_winner` collected published pointers via `.addr()` (needed only because `NonNull<Payload>`/`*mut Payload` is `!Send`, so the pointer itself can't cross the thread boundary through the shared `Mutex<Vec<_>>`), then reconstructed a pointer for deallocation via a bare `addr as *mut Payload` cast — under strict provenance the reconstructed pointer carries NO valid provenance, making the reclaim's `Box::from_raw` UB; the exact `usize`-as-`*const T` round-trip pattern this otherwise scrupulously strict-provenance-clean crate's own audit banned. Fixed via the task's own "ALTERNATIVE" route (the "PREFERRED" route of storing pointers directly does not work here: `Mutex<Vec<*mut Payload>>` would itself be `!Sync`, since `*mut Payload` is `!Send`): `p.as_ptr().expose_provenance()` to collect, `core::ptr::with_exposed_provenance_mut::<Payload>(addr)` to reconstruct — well-defined under the exposed-provenance memory model. **Not miri-verified** (stated explicitly per this task's own instruction): this is a loom test (`#![cfg(loom)]`), and loom's own green-thread scheduling simulation is incompatible with miri's execution model; the fix is reasoned (matches the exact documented use case for these two APIs) and confirmed by re-running the full loom suite (7/7 green), not by miri.
- **[docs, test, P4] Task #710 (commit `2af6da3`) — decided the `dbg_*` hooks' publish-blocking API posture, closed all 10 missing `// SAFETY:` tags (corrected from this bullet's original "9" — see below), fixed a stale identifier.** `dbg_is_ready`/`dbg_rollback_reenterable` were `#[doc(hidden)] pub fn`s — self-contradictory for a published crate, since `dbg_rollback_reenterable`'s own doc explicitly invites downstream consumers to call it from their own tests, while `#[doc(hidden)]` would hide it from the very rustdoc those consumers need to discover it in (hiding from docs does not remove it from the callable/semver surface). Resolved by promoting both to documented, non-hidden, stable public API with a "# Stability" doc section each; rejected feature-gating (e.g. behind a `test-probes` feature) as disproportionate, since both hooks are already exercised unconditionally by this crate's own test suite — recorded the decision and rationale in the crate README's new "Test-probe API stability" section. Closed 10 `// SAFETY:` tags the crate's own header claims exist on every `unsafe` block (1 in `src/lib.rs`, 4 in `tests/cell_unit.rs`, 5 in `tests/loom_racy_ptr_cell.rs`) — the commit subject and this bullet originally said "9" (the audit's own count at audit time, before `#706`/`#708` added two more untagged sites and `#700` tagged one of the original six), a miscount caught and corrected by the round-closing review (task #774, finding F7, 2026-08-09); nothing was actually missed, the round closed a superset of the original count. Fixed a stale parent-repo `heap_ptr` identifier in a comment (this standalone crate has no `heap_ptr`) and softened the module doc's "exact shape `RacyPtrCell` implements" claim, since `counterfactual_spin_on_ready_livelocks_on_oom_rollback` actually models a simpler `AtomicU8` 3-state encoding, not the packed `AtomicPtr`-with-sentinel shape `RacyPtrCell` uses. Appended an append-only resolution note to `docs/reviews/2026-08-06-racy-ptr-cell-publish-readiness-review.md`'s §5.1, which had described the (already-fixed) `dbg_rollback_reenterable` clobber bug as still live.

#### `racy-ptr-cell` — round-closing-review follow-ups (2026-08-09, tasks #773-774)

The round-closing review (task #743, `docs/reviews/2026-08-09-racy-ptr-cell-round-closing-review.md`) confirmed all six fixes above are correct, complete, and — for the five with a constructible counterfactual — independently re-verified as genuine (not locally-plausible-but-vacuous). It also found 1 HIGH (a process/CI gap, not a code defect) and 12 lower-severity findings, closed in two follow-up tasks before the sweep advanced to the next crate, per this project's own zero-trust-review discipline.

- **[fix(perf), P1] Task #773 (commit `a5e8e42`) — closed the HIGH finding: `cargo test -p racy-ptr-cell` ran in ZERO CI configurations.** The crate's only two prior CI invocations were a bare-metal `cargo build` (compiles no test target) and a `--test loom_racy_ptr_cell` step (excludes `tests/cell_unit.rs` by both target selection and its own `#![cfg(not(loom))]` gate). All 7 `cell_unit.rs` tests, including the 4 this round added, had never executed in CI — the exact gap class task #639/#772 (F4/F5) already closed for `tagged-index-stack`. Added `cargo test -p racy-ptr-cell --no-fail-fast` and the `--release` counterpart to `ci.yml`'s `test-workspace` job; the `--release` step specifically matters, since `#707`'s regression test only fails under `--release` if its `assert!` promotion were silently reverted. Counterfactual verified: reverted the `assert!`, confirmed debug stays green (expected) and `--release` fails with the expected message, then reverted cleanly.
- **[fix(perf), docs, P2] Task #774 (commits `ead400a`, `fdf2cec`) — closed the remaining 12 findings (F2-F13).** Highlights: **F2** — `tests/cell_unit.rs`'s own new test (from this round's `#706`) reintroduced the exact provenance-losing round-trip `#709` had just fixed one file over; applied the identical `expose_provenance`/`with_exposed_provenance_mut` fix (note: the review's claim that this would also satisfy `-Zmiri-strict-provenance` does not hold — that flag forbids the exposed-provenance mechanism outright, independent of how carefully it's used; verified both the permissive-Miri pass and the strict-Miri failure directly). **F3** — added a `# Panics` rustdoc section to `get_or_try_init` covering both the release-active sentinel-collision panic (`#707`) and the unwind-rollback contract (`#706`), neither of which was previously visible outside an inline comment or a private struct's doc. **F4** — the review's suggested fix (drop `dbg_is_ready` as redundant) does not hold: `Registry::dbg_chunk_is_materialised` (`src/registry/bootstrap.rs`) is a real, exercised caller; corrected the method's doc instead of removing it, naming the real reason it stays public. **F5-F13** — filed the CI gap and the `#709` miri caveat into `docs/CORRECTNESS_OPEN_ITEMS.md`'s resolved trail, corrected a stale line citation in the 2026-08-06 review's resolution note, corrected the SAFETY-tag count (10, not 9) and softened an overstated "each of the 6" phrasing in this file's own `#710`/header text (see the corrected bullets above), reworded a README overclaim about what the `#[should_panic]` counterfactuals prove, added NOTE comments explaining loom's causality checker (not the assertion's value comparison) is the real detector at three sites, documented a coverage gap on `RollbackGuard`'s untested concurrently-spinning-loser scenario (not closed — loom+`catch_unwind` don't compose cleanly, and the ROI didn't justify a bespoke harness), promoted one `unsafe fn`'s safety prose to a proper `# Safety` section, and made one `assert_eq!` unconditional instead of silently-always-true-gated. **Runtime improvements: 0** — every change is a test, a CI step, or documentation.

Deferred, unchanged by this round: `racy-ptr-cell`'s own crates.io first-publish decision (task #659, gated behind this round's closing `@oh` review, task #743 — now genuinely closed after #773-774, matching the same pattern `tagged-index-stack`'s round followed one day earlier). The remaining 3 workspace sub-crates' own rust-intel audit findings (aligned-vmem, numa-shim, size-classes — tasks #699/#712-719, #697/#720-727, #701/#728-731) are queued next in that order, one crate at a time, gated behind a small cross-cutting workspace task (#711, hoisting `loom = "0.7"` into `[workspace.dependencies]`) that #773-774 (and #743 before them) now unblock.

#### Workspace — hoist `loom` into `[workspace.dependencies]` (2026-08-09, task #711)

- **[build, P4] Task #711 (commit `56d0764`) — closed a version-drift hazard: `loom = "0.7"` was pinned independently in three manifests** (`crates/racy-ptr-cell/Cargo.toml`, `crates/tagged-index-stack/Cargo.toml`, and the root `Cargo.toml`'s pre-existing `[target.'cfg(loom)'.dev-dependencies]`, itself independent of and unrelated to `aligned-vmem`), with nothing enforcing they stay in sync. Added `[workspace.dependencies] loom = "0.7"` at the root and changed all three consumers to `loom = { workspace = true }`. **Correction (task #776, F5):** an earlier revision of this bullet claimed the root manifest's `loom` pin existed "once `aligned-vmem` needed it too" and that this task "unblocks the aligned-vmem round below, which needed `loom` for a new dev-dependency" — both false. `aligned-vmem` has zero `loom` dependency and never gained one this round (confirmed: `grep -rn loom crates/vmem/` finds only two prose mentions, one of which explicitly states loom is "not currently wired into `aligned-vmem`"); the root manifest's `loom` pin predates this task by many rounds and has no connection to `aligned-vmem`. The ordering of #711 before the aligned-vmem round below was a `TaskList` `blockedBy` sequencing choice (closing a cross-cutting drift hazard before starting the next crate), not a technical dependency.

#### `aligned-vmem` — rust-intel audit remediation (2026-08-09, tasks #699/#712-719)

Continuing the same one-crate-at-a-time `rust-intel` remediation sweep. All 10 tasks (`#699`, `#711` above, `#712`-`#719`) landed as individual commits. **Correction (task #776, F4):** an earlier revision of this paragraph claimed "each verified via a genuine zero-trust counterfactual before commit" — the round-closing review found this overstates: `#711` (a manifest hoist, nothing to revert-and-fail), `#714` (its Linux-only regression tests were compile-checked, not executed, in this session), and `#717` (whose own commit message states plainly "no counterfactual test exists that would fail before this fix and pass after") have no such counterfactual, and `#718`'s originally did NOT genuinely reproduce the bug it claimed to guard against (see task #775/F1 below — since corrected). The other six tasks (`#699`, `#712`, `#713`, `#715`, `#716`, `#719`) DO have a real, independently-reproduced counterfactual (temporarily reverting the fix, confirming the associated test fails for the right reason, then restoring it — confirmed via `git diff` showing zero net change); each task's own bullet below states this accurately. Every fix ran against the full available verification matrix: the native Windows test suite across every relevant feature combination, `cargo clippy --all-features --all-targets -D warnings`, `cargo fmt --check`, cross-compilation on `x86_64-unknown-{linux-gnu,freebsd,netbsd}` (all clean — `x86_64-unknown-dragonfly`/`x86_64-unknown-openbsd` have no prebuilt rustup std component on this host, so those two are REASONED-FROM-SPEC only, via their identical cfg arm to the verified siblings), and `cargo +nightly miri test` across the relevant feature combinations. Two REAL, previously-undiscovered bugs were caught and fixed as pure side effects of this round's own mandated re-verification, not any task's original scope: a vacuous-under-miri test from `#713`'s own earlier work (`#714`'s zero-trust re-run caught it), and an eager-vs-lazy evaluation bug in `#718`'s own `fetch_update` closure (`bool::then_some` evaluates its argument BEFORE the call, so `next - 1` underflow-panicked at `next == 0` even though the resulting `Option` should have been `None` — caught immediately: 4 of 5 tests in the affected file panicked on the very next `cargo test` run, before the bug ever reached a commit). **Runtime improvements: 0** — every fix below is test-only, documentation, a lint-suppression-scope narrowing, or a contract-violation-path change (panicking earlier/louder for already-undefined-behavior input); the one exception, corrected in task #776/F3 below, is `#714`'s hugetlb guard, which DOES narrow one previously-valid success path on Linux.

- **[fix(perf) (HIGH), P1] Task #699 (commit `6b18834`) — closed a false CI-coverage claim: `fault_injection.rs`'s tests ran in ZERO CI configurations.** `ci.yml`'s comments described the `fault-injection` feature as tested, but no step actually compiled `--features "fault-injection lazy-commit"` — every test in that file, including the ones proving the hook coexists with (does not replace) the real OS backend, had never executed in CI. Added the missing step.
- **[fix(perf), P2] Task #712 (commit `54089fa`) — `recommit`/`commit_range` used to clamp a contract VIOLATION to the SUCCESS sentinel.** `start >= end || misaligned` returned `Ok(())`/`true` unconditionally — indistinguishable from the genuine `start == end` no-op success case — already crashed an in-repo consumer that trusted the return value as "safe to write." Split the two cases: `start == end` stays `Ok(())`; `start > end` or misaligned now returns `Err(VmemError::invalid_argument())`.
- **[fix(perf), P2] Task #713 (commits `131355a`, `d6b72b1`) — `VmemError` read `errno`/`GetLastError` AFTER intervening cleanup FFI could clobber it.** Every raw OS-reservation helper (Windows and Unix) changed from returning `Option<(...)>` to `Result<(...), VmemError>`, capturing the error IMMEDIATELY at the failing syscall, before any subsequent cleanup call. `VmemError`'s internal representation changed from `code: u32` (defaulting to `0`, ambiguous with a genuine `ERROR_SUCCESS`) to `code: Option<u32>`, with a new `os_refusal_unknown_code()` constructor for the miri/no-code case. Filed `docs/CORRECTNESS_OPEN_ITEMS.md` item 41 (aligned-vmem's total absence of `cargo miri test` CI coverage) as a genuine side-effect discovery during this task's own zero-trust miri re-run — left OPEN, out of this task's scope.
- **[fix(perf), P2] Task #714 (commit `2e7f4f5`) — two real OS-conformance bugs: `_SC_PAGESIZE` wrong on multiple BSDs, and hugetlb `munmap` leaked the whole mapping.** `_SC_PAGESIZE` was hardcoded to the Linux value (30) for every non-Darwin Unix target — silently wrong for FreeBSD/DragonFly (47) and NetBSD/OpenBSD (28), and (per the round-closing review, task #776/F3's own re-check of the pre-image cfg arm — an under-claim in the original commit and this bullet, not an over-claim) also silently wrong on `tvos`/`watchos` (both Darwin, both previously falling into the Linux-value arm) — six wrong targets total, not four. Fixed with per-OS cfg arms. Separately: `munmap`'s Linux Huge-TLB-mapping rule requires both `addr` and `length` to be huge-page-aligned; the over-reserve/trim path violated this at ordinary `PAGE` granularity, silently discarding the resulting `EINVAL` and leaking the entire untrimmed mapping. Fixed by rejecting a non-huge-page-aligned `(size, align)` request up front with `VmemError::invalid_argument()` — chosen over rounding, since the reject strategy is provably correct by construction (both inputs huge-page-aligned ⇒ every subsequent `munmap` call is provably huge-page-aligned too), while a rounding strategy would need runtime probing of the actual configured huge-page size this crate has no infrastructure for. **Correction (task #776, F3):** this guard is a real, undocumented-until-now NARROWING of `reserve_aligned_huge`'s public contract on Linux, not a pure bugfix with no user-visible consequence — a sub-2-MiB huge request that previously succeeded via the documented ordinary-page fallback (e.g. `reserve_aligned_huge(64 * 1024, 64 * 1024)` on a host with no hugepages configured) now returns `Err(invalid_argument())` before any syscall runs, contradicting this section's own header claim (below) that no valid-input success path changed observable behavior. Fixed in task #776: the rustdoc for `reserve_aligned_huge`/`try_reserve_aligned_huge` and `README.md` now state the Linux-only 2 MiB requirement explicitly, and the "identical to `reserve_aligned`" claim is corrected.
- **[docs, P3] Task #715 (commit `e5f6700`) — two publish-blocking API decisions for `mock`.** (1) `mock::Call`'s 8 struct-like variants gained per-variant `#[non_exhaustive]` (enum-level `#[non_exhaustive]` alone only reserves adding whole variants, not fields to an existing one — `ReserveLazy` already grew a field once). (2) Documented (did not code-fix) the Cargo feature-unification hazard: `mock` is a non-additive, backend-REPLACING feature, and Cargo unifies features across a build's whole dependency graph, so any downstream consumer enabling `aligned-vmem/mock` silently swaps the real OS backend for every OTHER consumer in the same build too. Chose the minimum fix (doc warnings in `Cargo.toml`/`lib.rs`/README) over the audit's offered stronger fix (a `--cfg` flag instead of a Cargo feature, matching this repo's own `cfg(loom)`/`cfg(kani)` precedent) — zero real external consumers exist before this crate's first publish, so the doc fix closes the realistic case at near-zero cost.
- **[fix(perf), test, P3] Task #716 (commit `81ecfe3`) — closed the huge-pages mock coverage gap and a miri-UB test assertion.** `tests/lazy_commit.rs`'s `sequential_commit_range_grows_incrementally` asserted a byte it never wrote reads back as `0` — legal on a real OS backend (fresh pages are guaranteed zero-filled) but a genuine uninitialized-memory read under miri's `std::alloc`-based fallback (which does not zero, unlike a real OS page); gated with `#[cfg(not(miri))]`, mirroring the identical established pattern in `tests/smoke.rs`. Added `fail_next_reserve_injects_through_huge_path` to `tests/mock.rs`, proving `reserve_aligned_huge` records `Call::ReserveHuge` (not the ordinary `Call::Reserve`) and that `fail_next_reserve` actually injects through this specific entry point — the one item from this task's own FIX list not already covered by `tests/huge_pages.rs` (created as a side effect of task #714).
- **[fix(perf), P3] Task #717 (commit `94aef18`) — replaced exposed-address `as`-cast round-trips with strict provenance in the two native over-reserve paths.** README's provenance guarantee ("no exposed-address `as usize` round-trips in the public API") was contradicted by the crate's own implementation: both `win_reserve_commit` and `unix_reserve` computed an aligned base address via `region_ptr as usize` arithmetic, then reconstructed a brand-new pointer via `base_addr as *mut u8` — exactly the round-trip the README claims doesn't happen. Fixed using `.addr()`/`.with_addr()` (Rust's strict-provenance APIs): `.addr()` reads a pointer's address without exposing its provenance; `.with_addr()` constructs a new pointer carrying the ORIGINAL pointer's provenance at a computed address — the sanctioned pattern for "align a pointer within a larger allocation." No behavior change on any backend.
- **[fix(perf), test, P2] Task #718 (commit `b8b70fb`) — closed two real data-race hazards in `fault_injection`'s atomics.** (1) `arm_fail_at`'s counter-reset-then-target-store used `Relaxed` for both — a reader thread could observe the freshly-armed target without observing the counter reset, corrupting the k-th-call count; fixed with a `Release`/`Acquire` pair. (2) `should_fail_commit`'s `FAIL_NEXT` decrement was a separate `load` then `store` (not atomic as a pair) — concurrent callers could race to observe the same pre-decrement value, either double-firing or silently losing a decrement; fixed with `AtomicU32::fetch_update`. New test `fail_next_is_atomic_under_concurrent_callers` (32 threads, `Barrier`-synchronized, 200 rounds) proves the fix introduces no regression under real concurrent load — documented honestly in its own doc comment that it does NOT reliably fail against the pre-fix racy code (empirically verified: 10 runs across two test designs, zero failures, against genuinely racy code — real OS thread wake-up jitter after a barrier release is orders of magnitude wider than the actual race window); the real soundness guarantee rests on `fetch_update`'s atomic-by-construction semantics, not this test's ability to have caught the old bug.
- **[fix(perf), test, docs, P4] Task #719 (commit `55e71b0`) — closed 7 independent hygiene residuals.** A missing SAFETY comment (Unix `decommit_pages_impl`'s pointer arithmetic, present on its Windows sibling but not here); two undocumented `let _ =` syscall discards (`libc_munmap`/`libc_madvise`, now explaining the failure mode — a leak or a missed reclaim hint, never memory unsafety); a blanket `#[allow(dead_code)]` on `DecommitKind` that suppressed the lint in every feature config instead of just `mock` (missed from an earlier task #646/F8 pass that narrowed every other site in the file); a Drop-reachable panic in `from_raw_parts` (accepted any `align` without validating its own documented contract, deferring failure to `Drop`-time `Layout` construction under miri — genuinely dangerous, since a second panic during an active unwind aborts the whole process; fixed by validating immediately at the unsafe call site, with a new `should_panic` test verified via counterfactual to genuinely fail against the reverted code — **correction, task #776/F7: this commit's `assert!` validated only the `align` half of the hazard, leaving `reservation_len` unchecked** — `Layout::from_size_align` also fails on an overflowing `reservation_len`, so e.g. `from_raw_parts(b, PAGE, r, usize::MAX, PAGE)` still constructed successfully and still panicked in `Drop` under miri; task #776 extended the assert to cover both halves, closing the hazard completely, with a matching new `should_panic` test); an untested `unsafe impl Send for Reservation {}` (added a compile-time assertion mirroring `sefer-region`'s established `Handle<T>` pattern); an `off_t` ABI-shape risk on the `mmap` FFI declaration (documented, not code-fixed — `i64` is correct for every currently-supported/tested target, and narrowing further would be designing for a 32-bit Unix target this crate does not have); and a factually wrong doc claim that `from_raw_parts` is "the inverse of `into_parts`" (false — `into_parts` returns only 3 of the 5 fields `from_raw_parts` requires; the true structural complement is `release`, which shares `into_parts`'s exact 3-tuple signature).

Deferred, unchanged by this round: `aligned-vmem`'s own crates.io publish decision (task #658 — local version already bumped to 0.2.0, crates.io still shows 0.1.0 — gated behind this round's closing `@oh` review, task #747, and its own follow-ups #775-776 below). The remaining 2 workspace sub-crates' own rust-intel audit findings (numa-shim, size-classes — tasks #697/#720-727, #701/#728-731) were queued next in that order — numa-shim's own round follows immediately below.

#### `aligned-vmem` — round-closing-review follow-ups (2026-08-09, tasks #775-776)

The round-closing review (task #747, `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md`) confirmed the round's shipped source changes are correct — every mechanically constructible counterfactual reproduces, no memory-safety defect, no new `unsafe` surface — but found the round's OWN evidence for `#718` was wrong, and found 14 further findings (F2-F15). Task #775 closed the sole HIGH finding, F1; task #776 closed the remaining 14.

- **[fix (test correctness) (HIGH), P1] Task #775 — #718's flagship regression test could not fail against the bug it claims to catch, and the round's own stated reason why was itself wrong.** `fail_next_is_atomic_under_concurrent_callers` armed exactly `TOTAL` failures for `TOTAL` calls and asserted `failures == TOTAL` — the review proved (and this task independently re-verified) that this oracle is STRUCTURALLY incapable of failing against the pre-fix race, for a scheduling-independent reason: a torn decrement under the racy `load`-then-`store` pair can only INFLATE the observed fire count (never deflate it below the correct trajectory), and since `armed == calls` already forces the correct implementation to fire on 100% of calls (the trivial upper bound), the racy implementation cannot exceed that ceiling either — `failures == TOTAL` holds under BOTH implementations, for every thread count, every round count, on any hardware. This directly contradicts the previous bullet's (and the test's own doc comment's) claim that "no amount of thread/round count fixes this on real hardware without a model checker or an artificial delay" — that claim blamed thread-scheduling jitter for a defect that was actually in the oracle's shape. Fixed by arming only HALF the calls (`TOTAL / 2`) and asserting `failures == TOTAL / 2`: now a correct implementation fires on exactly half, giving the racy implementation's inflation room to diverge and get caught. Personally re-verified both directions on the SAME `Barrier`-synchronized 32-thread/200-round design: reverted `should_fail_commit` to the pre-`#718` racy code, confirmed the OLD oracle (`armed == calls`) passes 3/3 runs (reproducing the review's mathematical argument empirically, not just trusting it) and the NEW oracle (`armed == calls / 2`) FAILS 5/5 runs with zero artificial delay — then restored the fix and confirmed the new oracle passes reliably against it. Rewrote the test's doc comment to state the actual mechanism (the monotonicity argument) instead of the thread-jitter explanation.
- **[fix(perf), test, docs, P2-P4] Task #776 — closed the remaining 14 findings (F2-F15).** **F2 (MEDIUM)** — `mock::take_reserve_fault`/`take_commit_fault` still minted a fabricated `code 0` for a SIMULATED failure, reopening the exact ambiguity `#713` closed for the real path; switched both to `os_refusal_unknown_code()`, new test `simulated_fault_reports_no_os_code` (counterfactual-verified). **F3 (MEDIUM)** — documented the Linux-only 2 MiB `size`/`align` requirement `#714`'s hugetlb guard silently added (see `#714`'s own corrected bullet above), on `reserve_aligned_huge`/`try_reserve_aligned_huge`'s rustdoc and `README.md`. **F4/F5 (MEDIUM)** — corrected the round's own header paragraph and `#711`'s bullet (see both above). **F6 (LOW-MEDIUM)** — demoted 5 private-item intra-doc links in `fault_injection.rs` to plain code spans, closing 5 rustdoc warnings in the EXACT feature set docs.rs builds (`cargo doc --features "lazy-commit huge-pages fault-injection"` now clean). **F7 (LOW-MEDIUM)** — extended `from_raw_parts`'s `assert!` to also validate `reservation_len` (see `#719`'s corrected bullet above), new `should_panic` test `from_raw_parts_rejects_an_overflowing_reservation_len_immediately`. **F8 (LOW)** — applied `#717`'s strict-provenance discipline (`.addr()` instead of `as usize`) to the two exposing casts `#717` left on the Unix FAST path (`try_reserve_aligned_exact`, `libc_mmap`'s `MAP_FAILED` check) — the higher-traffic site, tried first by every reservation. **F9 (LOW)** — reworded `README.md`'s "Alignment contract" section, which had gone stale in two ways THIS round created (`#712` made `recommit`/`commit_range` violations reject rather than no-op; `#719` made `from_raw_parts` panic on a bad `align`), and documented the `decommit`-vs-`recommit` violation-handling asymmetry. **F10 (LOW)** — removed a hardcoded test-count comment in `ci.yml` (said "4 passed", `#718` made it 5) per CLAUDE.md's own "never hardcode a count" guidance. **F11 (LOW)** — updated `docs/CORRECTNESS_OPEN_ITEMS.md` item 41's Status card: `#716` closed one of the item's two originally-listed miri blockers two commits after the item was filed, and the card was never updated at the time; corrected now per CLAUDE.md's "update in the SAME commit" rule (a card-accuracy defect, not a stale-open-item defect — the item's core blocker, the missing CI step, remains genuinely open). **F12 (LOW)** — corrected `tests/huge_pages.rs`'s module doc, which undersold its own coverage by saying a Linux CI runner "were one added" would exercise it — one already exists and already does, on every push; the real residual gap is narrower (no hugetlb-CONFIGURED host, not no Linux host). **F13 (LOW)** — filed two new items into `docs/CORRECTNESS_OPEN_ITEMS.md` (items 42-43) for deferrals this round stated only in commit-message prose: `#715`'s `--cfg`-flag-conversion deferral (load-bearing for numa-shim's own upcoming §C10 finding) and the BSD `_SC_PAGESIZE` constants' REASONED-FROM-SPEC-never-executed status. **F14 (INFORMATIONAL)** — `commit_range`'s rustdoc gained an explicit "concurrent calls are safe" guarantee, which a test's SAFETY comment already cited as if it existed. **F15 (INFORMATIONAL)** — added a scope note to `fault_injection.rs`'s module doc naming the one remaining (out-of-scope, pre-existing) concurrency hazard in the same 3-atomic protocol `#718` touched (a concurrent `arm_fail_at` racing the one-shot self-disarm). **Runtime improvements: 0** — every change is a test, documentation, a lint-suppression-scope narrowing, or (F8) a provenance-model correctness change with identical runtime behavior on every currently-supported target.

All 15 findings from the round-closing review (F1, closed by task #775 above; F2-F15, closed by task #776 above) are now closed. `aligned-vmem`'s own crates.io publish decision (task #658) remains deferred to a maintainer call, gated behind this two-task follow-up now being genuinely complete — matching the pattern `tagged-index-stack` (tasks #771-772) and `racy-ptr-cell` (tasks #773-774) both followed. The remaining 2 workspace sub-crates' own rust-intel audit findings (numa-shim, size-classes — tasks #697/#720-727, #701/#728-731) were queued next in that order — numa-shim's own round follows immediately below.

#### `aligned-vmem` — code-quality/bug/perf review remediation (2026-08-12, tasks #842-850)

A fresh `@oh` (Opus, high-effort) code-quality review (`docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`, 21 findings V1-V21) drove a 9-task, 9-commit campaign, each delegated to `/crush` and personally zero-trust reviewed. **This is a sub-crate campaign — `aligned-vmem` is a workspace member library not consumed by `sefer-alloc`'s own `production` feature bundle, so none of these changes affect the allocator's shipping runtime.**

- **[fix(vmem), P0] Task #842 (commit `76ac08f`) — V1: stop trimming the Unix over-reserve mapping; V2: validate decommit against `page_size()` not `PAGE`.** `unix_reserve`'s over-reserve fallback path (taken on an exact-mmap alignment miss) used to `munmap` the head/tail slack around the aligned sub-range, trimming the reservation down to exactly `size` bytes. Changed to keep the whole `over = size + align` mapping as the reservation (matching the Windows backend's existing behavior), removing the trim calls entirely — the trim was VA-leak-prone on the failure path (an intervening error before the trim completed could leak the untrimmed remainder). Separately, `decommit`/`decommit_lazy` validated the caller's offsets against the compile-time `PAGE` constant (4 KiB) rather than the real, possibly-larger `page_size()` (e.g. 16 KiB on Apple Silicon macOS) — now reject a violated range up front instead of risking a silent partial `madvise`.
- **[docs(vmem), feat(vmem), P2] Task #843 (commit `6ef3ac2`) — V3: document Windows huge-pages non-functionality; V4: decommit-on-huge-page divergence; added `Reservation::is_huge()`.** Documented that the crate's `huge-pages` feature does not actually deliver large pages on Windows in the shape it was shipped (the `MEM_LARGE_PAGES` flag needs `MEM_RESERVE | MEM_COMMIT` in one call; the crate split reserve/commit into two, so the flag never took effect) and that `decommit` does not work on a huge-page reservation on either platform. Added `Reservation::is_huge()` as a read-only accessor making the huge/ordinary distinction observable, replacing an unfalsifiable "best-effort" claim. (This commit's own diff introduced a real Unix compile break, undetected for 5 subsequent commits — see task #849's own entry below for the root cause and fix.)
- **[fix(vmem), test(vmem), P1] Task #844 (commit `84ca221`) — V5: eliminated 2 real test leaks under miri, fixed a miri-backend tuple-arity regression from #843, added a global-allocator-under-miri doc caveat.** Two `tests/smoke.rs` tests that exercised `from_raw_parts`' panic paths were leaking the reservation before the panic assertion, invisible under a normal `cargo test` but flagged by miri's leak checker; fixed with a `catch_unwind` + `release` + `resume_unwind` pattern that still discriminates the two distinct panic messages. Reverted miri's `reserve_aligned_raw` from the 4-tuple #843 had given it back to the 3-tuple shape the real backends use — but left two feature-gated `.map()` closures still destructuring 4 elements, a compile break not caught by this task's own `--test smoke` (default-features) verification; found and fixed by task #851 below, six commits later.
- **[feat(vmem), fix(vmem), test(vmem), P2] Task #845 (commit `f3817ad`) — V6: `mock::Call` per-variant constructors; V7: `Reservation` `Debug`; V8: `ReservationParts` + `into_reservation_parts()`/`release_parts()`; V9: `Drop` now records a `Call::Release`.** Added 8 `#[must_use] pub fn` constructors to `mock::Call` (one per variant: reserve/reserve_lazy/reserve_huge/release/decommit/decommit_lazy/recommit/commit_range). `into_reservation_parts()` was initially missing the `core::mem::forget(self)` its sibling `into_parts()` already has — a real double-free, confirmed via miri, found and fixed during this task's own zero-trust review before landing (the same task that introduced the bug also closed it, in the same commit).
- **[docs(vmem), refactor(vmem), P3] Task #846 (commit `e161eb0`) — V10: fixed 2 stale doc statements; V17: deduplicated `last_os_error_code`; V14: factored out `validate_size_align`/`finish_reservation` helpers; V15: added a `RawReservation` struct (deliberately scoped to just the two `finish_*` helpers, not all 9 backend functions, per this campaign's "reduce scope if too big" convention).** Also found and fixed two backwards `#[cfg_attr(feature = "X", allow(dead_code))]` conditions (gated on the feature's PRESENCE instead of its absence) — a bug that passed `--all-features` clippy but failed default-features clippy, the same class of drift this project's CLAUDE.md already documents from a prior `sefer-region` incident.
- **[fix(vmem), P2] Task #847 (commit `7e16b95`) — V18: added a CI clippy gate for this package.** Neither `--all-features --all-targets` nor default-features `cargo clippy` for `aligned-vmem` ran anywhere in CI before this task — a renamed public function used only by the bench or an example could go unnoticed. Added both rows to a new `aligned-vmem-gates` job, mirroring the existing `sefer-region-gates` pattern.
- **[perf(vmem), P2] Task #848 (commit `98b7d0a`) — V21/P18: single-call `VirtualAlloc` for `align <= 64 KiB` on the eager reserve path.** `win_reserve_commit` always issued two syscalls (`MEM_RESERVE` then `MEM_COMMIT`); for small alignments a single `VirtualAlloc(.., MEM_RESERVE | MEM_COMMIT | extra_flags, ..)` call satisfies the alignment contract by construction (Windows' system allocation granularity is 64 KiB). The initial implementation was missing a `commit_len == size` guard, meaning a lazy-commit caller (which deliberately requests `commit_len < size`) could take the fast path and silently shrink the real OS reservation to `commit_len` bytes — a real correctness bug, found and fixed during this task's own zero-trust review, with a new permanent regression test (`tests/lazy_commit.rs`) pinning it.
- **[fix(vmem), P1] Task #849 (commit `35d51e6`, plus an unscoped-but-blocking fix `b228e69`) — V20/P17: measured the Unix exact-reserve hit rate.** Before the measurement could run, discovered task #843's own diff had left `unix_reserve` in a state that failed to compile on Unix at all (`E0425`/`E0599`/`E0308` — a malformed `unsafe` block whose branches returned tuples instead of bare pointers) — undetected for 5 commits (#843-#848) because every verification in this campaign up to that point had run on Windows only, which never type-checks `#[cfg(unix)]` code. Fixed via WSL, verified on both platforms (commit `b228e69`). The measurement itself: added `examples/v20_849_unix_exact_reserve_hit_rate.rs` (bench-internals-gated), and — after two methodology corrections (an alloc-then-free loop that measured one address's alignment residue repeatedly, then a single-process batch that turned out to be one correlated Bernoulli trial, not N independent ones) — a 30-independent-process aggregate found: page-size 480/480 (100%), 64 KiB 165/480 (34.4%), 1 MiB 224/480 (46.7%), 4 MiB 272/480 (56.7%). This contradicts the originating review's hand-derived ~0.1% prediction for the 4 MiB regime by roughly 3 orders of magnitude — measurement-only, no guard implemented; the recommendation is explicitly to leave the code as-is pending a bare-metal Linux re-measurement (see `docs/perf/OPEN_ITEMS.md` item 46).
- **[docs(vmem), fix(vmem), P4] Task #850 (commit `9e8e52d`) — V11: docs.rs feature badges; V12: `impl From<VmemError> for std::io::Error`; V13: `MIN_PAGE` alias for `PAGE`; V16: documented `mock`'s partial-backend-replacement shape as a deliberate, deferred decision; V19: documented the bench iteration-count manifest gap.** The delegate's V11 diff added `#[cfg_attr(docsrs, doc(cfg(...)))]` on every gated item but omitted the crate-level `#![cfg_attr(docsrs, feature(doc_cfg))]` opt-in that attribute requires to compile at all under docs.rs's real nightly build — found and fixed in the same commit by the orchestrating session (confirmed via `cargo +nightly doc --cfg docsrs` on WSL, matching docs.rs's actual build environment; sefer-region already carries the correct line, aligned-vmem had simply never needed it before).

A fresh post-campaign closing review (`docs/reviews/2026-08-12-aligned-vmem-post-campaign-closing-review.md`, findings W1-W16 + 3 perf notes) found one HIGH (a second, independent miri compile break from #844, closed by task #851) plus 15 lower-severity findings — that follow-up round (tasks #851-857) is tracked separately and will get its own CHANGELOG entry once complete. `aligned-vmem`'s own crates.io publish decision (task #658) remains deferred to an explicit maintainer go-ahead, gated behind that follow-up round finishing.

#### `numa-shim` — rust-intel audit remediation (2026-08-09, tasks #697/#720-727)

Continuing the same one-crate-at-a-time `rust-intel` remediation sweep. All 9 tasks landed as individual commits, each with the established verification-honesty distinction stated explicitly per task: this session's host is Windows, and the majority of this crate's logic is Linux-only (`#[cfg(all(target_os = "linux", not(miri)))]`), so those fixes are REASONED-FROM-SPEC (careful reasoning plus a genuine `x86_64-unknown-linux-gnu` cross-compile check for type/borrow correctness, not an actual run) rather than empirically verified; the crate's Windows platform module IS native on this host, so `#724`'s fix and every `mock`-feature test uniquely got a genuine EMPIRICAL run on real hardware. **Runtime improvements: 2** — task `#697` (a real ABI-conformance fix that changes OBSERVABLE behavior on any Linux multi-node host: node 63 binding now genuinely takes effect, per its own commit message) and task `#724` (genuinely halves Windows commit charge for every NUMA reservation — an observable runtime resource change, EMPIRICALLY verified on this session's own Windows host via the task #778/F3 regression test); every other task is a test, a doc/API-surface decision, or a panic-safety/consistency fix with no measured or claimed speedup. **Correction (task #778, F10):** an earlier revision of this sentence counted `#723` (the `OnceLock` topology cache, unmeasured and never executed on this session's host) instead of `#724` — inconsistent with the `fix(perf)` R30-12 definition (`#723` claimed "no observable change," which is also now known to have been WRONG per `#777`/F1's correction above) and, separately, left out `#724`, a `fix(perf)` commit with a genuine, tested, observable resource-consumption change. Corrected to the two commits whose own bullets actually support "observable behavior changed."

- **[fix(perf), P1] Task #697 (commit `b275480`) — `mbind`'s `maxnode` argument was off by one, silently dropping node 63's bit on every 64-node-capable host.** `bind_range_impl_linux` encoded the target node as a single-bit `u64` nodemask and passed `maxnode = 64` to `mbind(2)` — but the kernel's own `get_nodes()` (`mm/mempolicy.c`) DECREMENTS `maxnode` internally before computing the addressable-bit range, so `maxnode = 64` for a 64-bit mask actually only covers bits `0..62`; `libnuma` itself compensates for this exact kernel quirk by always passing bitmask-size+1. `bind_range(node=63, ...)` therefore passed an effectively-empty nodemask, silently degrading `MPOL_PREFERRED` to unconstrained local allocation with no error surfaced (errors are already silently discarded by design). Fixed by passing `maxnode = 65`. REASONED-FROM-SPEC (Linux-only code, this session cannot execute it): verified via careful reading of the kernel ABI plus a clean `x86_64-unknown-linux-gnu` cross-compile.
- **[fix(perf), P1] Task #720 (commit `3899ab9`) — a single 256-byte cpumap read was treated as the complete file, silently truncating and misreading node topology on ~900+-CPU hosts.** `read_cpumap_contains_cpu`'s one-shot 256-byte read covers only ~28 mask words (~896 CPUs); on hosts wider than that, the truncated tail holds the LOW-index CPU words (the format is most-significant-word-first), so the parser's `word_count`/`left_index` arithmetic silently misaligns and returns a WRONG node rather than failing loudly — no test existed at this scale boundary. Fixed by looping the read to EOF into a 4 KiB buffer (covers ~14,500 CPUs, an order of magnitude past any currently-shipping NUMA system); if the buffer fills without ever observing EOF, returns `false` (the documented "cannot determine" fallback) instead of silently parsing a prefix. REASONED-FROM-SPEC (Linux-only).
- **[test, P2] Task #721 (commit `c5e013b`) — a genuine architectural move, not just a bugfix: extracted the crate's most intricate parsing logic into a target-independent module specifically so it could be tested for real on this session's own host.** The Linux sysfs cpumap parser (most-significant-word-first hex bitmask decoding — `format_sysfs_path`/`parse_contains_cpu`/`trim_end`/`nth_token`/`parse_hex_u32`) lived entirely inside a `#[cfg(target_os = "linux")]`-gated module, meaning it had ZERO direct executable tests anywhere; the only spec vectors existed as prose in doc comments. Moved all five pure, syscall-free functions into a new `#[doc(hidden)] pub mod cpumap` (the crate's own established doc-hidden test-forwarder pattern) at the TOP level, target-independent — an accident of code organization corrected, not a genuine platform requirement, since none of the five functions touches the OS. Added `tests/cpumap_parser.rs`, 15 tests covering the doc example verbatim, the exact word-order boundary task `#720`'s fix depends on staying correct (CPU 32 read from the LEFTMOST token, not the second), single-word masks, an out-of-range CPU index, and negative controls (empty token, invalid hex digit, empty input) for every one of the five functions — genuinely EMPIRICALLY run on this Windows host, closing the "zero behavioral oracles" half of the original audit's §D1a finding.
- **[fix(perf), P2] Task #722 (commit `69045e3`) — 4 doc/code semantics divergences where the documentation and the implementation disagreed.** (1) `current_node`'s doc claimed a `None` return for "CPU index cannot be mapped to a NUMA node via sysfs" was reachable on Linux; it is currently UNREACHABLE — every sysfs failure mode collapses into the same `Some(0)` single-node fallback, corrected to state this precisely rather than imply a distinction the code cannot make. (2) `bind_range_impl_linux`'s `node >= 64` guard (the single-`u64`-nodemask ceiling) was undocumented on both `bind_range`'s own rustdoc and the README — added a `// DEVIATION` comment at the guard and a doc line on both surfaces. (3) Windows `current_node_impl`'s SAFETY comment conflated the BOOL return (`ok == 0` = call FAILED) with the OUT-parameter (`node == 0` on success = the genuine single-node answer); separately, Microsoft's own docs for `GetNumaProcessorNodeEx` state the OUT node number is set to `MAXUSHORT` when the given processor does not exist while the call still reports success — that sentinel was previously unhandled entirely; fixed both the comment and added the `node == u16::MAX` check. (4) The `mock` arm of `current_node()` wrapped the scripted slot in `Some` UNCONDITIONALLY, so `mock::set_current_node(NO_NODE)` produced `Some(NO_NODE)` — violating this function's own documented "returns `Option`, never the sentinel" guarantee, and making the `None` branch impossible to exercise under `mock`, the very feature CI relies on to assert this wrapping logic; fixed to mirror the real dispatch's remapping, with a new `current_node_scripted_no_node_yields_none` test. Items (1)-(2) are REASONED-FROM-SPEC (Linux-only); items (3)-(4) got real Windows-native and `mock`-feature test runs respectively.
- **[perf(runtime), P2] Task #723 (commit `2cdb765`) — eliminated up to 64 open/read/close syscall triples per `current_node()` call on an allocation-reachable path, by caching the boot-static topology.** `cpu_to_numa_node` previously re-derived the cpu→node mapping on EVERY call by probing up to 64 candidate nodes, each a full sysfs open/read/close — the crate's own R11-5 comment establishes `current_node()` is re-entered from sefer-alloc's `numa-aware` allocation path, so this cost was paid repeatedly for a mapping that is boot-static (Linux does not change which CPUs belong to which NUMA node at runtime, short of hotplug). Hoisted the parsed topology into `static TOPOLOGY: std::sync::OnceLock<Vec<Vec<u8>>>`, populated ONCE via `get_or_init`, caching each node's RAW cpumap file bytes (not a fixed-width bitmask, to preserve full correctness for hosts with >64 CPUs per node — the existing arbitrary-width parser already handles this); `cpu_to_numa_node` then iterates the cached bytes and calls the already-tested `crate::cpumap::parse_contains_cpu` in-memory, with no further syscalls after the first call in the process's lifetime. CPU-hotplug staleness after the first call is accepted per the audit's own note (an `MPOL_PREFERRED` hint the kernel already treats as best-effort). REASONED-FROM-SPEC for the syscall-elimination claim itself (Linux-only, unmeasurable on this host), but the parser correctness of the cached-bytes call site IS empirically verified — it's the same `parse_contains_cpu` `#721`'s (not `#720`'s — corrected below) 15 tests already exercise. **Correction (task #777, F1, HIGH — round-closing review):** this commit's closing claim, "No production API or observable return value changed," was wrong. The `OnceLock<Vec<Vec<u8>>>` design performed ~65 heap allocations inside `get_or_init`'s initializer, on the exact `AllocCore::alloc` path (`current_node_cached` → `numa_shim::current_node` → `cpu_to_numa_node` → `topology()`) that `sefer-alloc`'s own `M5` invariant declares allocation-free/reentrancy-free specifically so it never re-enters the global allocator. Under a real Linux `#[global_allocator] = SeferAlloc` + `numa-aware` deployment, the first NUMA-aware allocation would have triggered `topology()`'s heap allocation, re-entering `GlobalAlloc::alloc`, re-entering `current_node()`, re-entering `OnceLock::get_or_init` on the same cell mid-initialization — documented by the standard library as "an error to reentrantly initialize the cell... current implementation deadlocks," independently reproduced by the reviewing agent in a standalone scratch project on the same toolchain. No CI job caught it (the `mock` feature bypasses `platform` entirely; the weekly `numa-real-kernel` job's binaries do not install `#[global_allocator]`). Fixed by task #777: replaced the heap-backed `Vec<Vec<u8>>` cache with a fixed-size, allocation-free `Topology { len: [usize; 64], buf: [[u8; 1024]; 64] }` held by the same `OnceLock` — populating it touches no `Vec`/`Box`/heap at all, removing the reentrancy hazard structurally. See task #777's own CHANGELOG bullet below for the fix.
- **[fix(perf), P2] Task #724 (commit `2efa70f`) — the Windows reservation path committed the FULL over-reservation instead of the caller-requested size, doubling commit charge in the worst case.** `reserve_aligned_numa` called `VirtualAllocExNuma(.., over, MEM_RESERVE | MEM_COMMIT, .., node)` in ONE call, committing `over = size + align` bytes — up to double the commit charge of the range the caller actually asked for and can use (e.g. `align == size` commits `2 * size`) — directly contradicting the function's own doc claim of "mirrors aligned-vmem's own Windows reservation" (`aligned-vmem`'s `win_reserve_commit` has always reserved `over` but committed only `commit_len <= size`). Fixed to the identical two-call reserve-then-commit shape: `VirtualAllocExNuma(.., over, MEM_RESERVE, .., node)` reserves address space only (no physical pages, so `node` has no effect here), then `VirtualAllocExNuma(.., base, size, MEM_COMMIT, .., node)` commits exactly the requested `size` at the aligned sub-range — the call that actually allocates physical pages, where `node` takes effect. On commit failure the whole `over`-byte reservation is released via a newly-added `VirtualFree(MEM_RELEASE)` (needed since no `aligned_vmem::Reservation` exists yet at that point to own the release via `Drop`). **EMPIRICALLY VERIFIED on real Windows hardware** (unlike the rest of this round): this platform module is native on this session's host; ran the REAL (non-mock) code path directly via `cargo test -p numa-shim --features vmem-integration --test smoke` (mock feature deliberately OFF, so `reserve_on_node` dispatches to the real `reserve_aligned_numa`, not the mock branch that would otherwise bypass it entirely) — 6/6 pass, including two tests that exercise this exact function.
- **[docs, P3] Task #725 (commit `f989bed`) — `bind_range`'s `# Safety` contract was stated unconditionally, making five green test call sites technically UB-by-contract.** The doc said `[base, base+len)` must be a valid OS reservation UNCONDITIONALLY, even though the function body itself short-circuits (returns immediately, never touching `base`) when `node == NO_NODE` or `len == 0` — every current test call site (`tests/mock_dispatch.rs`'s dummy `0x1000 as *mut u8`, `tests/smoke.rs`'s stack-array pointers under `NO_NODE`/`len == 0`) relies on exactly that short-circuit rather than a genuinely valid reservation, so under the old wording those five green tests were UB-by-contract despite being harmless in practice — a future edit reordering the short-circuit after a real platform call would have silently turned them into real UB with no doc contradiction to catch it. Fixed the doc to scope the precondition to when it actually applies (`node != NO_NODE && len != 0`), making all five call sites contract-compliant by construction. Doc-only, no behavior changed; verified via `cargo doc --all-features --no-deps` (clean) and the unchanged 29/29 test pass.
- **[fix(perf), P3] Task #726 (commit `53b3ca2`) — 5 publish-surface API decisions in the `mock` module, all settled before this crate's first crates.io publish.** (1) `CALLS`/`CURRENT_NODE_SLOT` thread-locals were unrestricted `pub`, committing `RefCell` internals to the semver surface though the intended API is already the encapsulating `drain()`/`set_current_node()` pair and no code anywhere in this workspace touched them directly — narrowed to `pub(crate)`. (2) `CALLS` was an insert-only `Vec` with no cap; under the documented sefer-alloc-as-global `numa-aware-mock` scenario every allocation calls `current_node()` → `record()` → `Vec::push` with nothing ever draining it — added a 4096-entry cap, with a new regression test counterfactual-verified by temporarily removing the cap and confirming the test correctly observes 5000 recorded entries instead of the expected ≤4096. (3) `#[repr(C)] ProcessorNumber` (passed by pointer to two Windows FFI calls) had no layout assertions — the hand-written mirror happened to match the real `PROCESSOR_NUMBER` layout but nothing pinned it; added `size_of`/`align_of`/`offset_of!` const-eval assertions. (4) `MockCall`'s enum-level `#[non_exhaustive]` only reserved the right to add whole variants, not fields to the existing `BindRange`/`ReserveOnNode` struct-like variants — added variant-level `#[non_exhaustive]` to both, which broke (and required fixing) two existing test call sites, confirming the enforcement is real rather than vacuous (integration tests compile as a separate crate, so the same enforcement external consumers would hit applies to this crate's own suite). (5) `mock` is a non-additive, backend-REPLACING Cargo feature subject to Cargo's whole-graph feature unification — applied the SAME documentation-only policy already decided for `aligned-vmem`'s identical finding (task `#715`), per that commit's own explicit note that the policy should carry over here, rather than the stronger `--cfg`-flag conversion (zero real external consumers exist before this crate's first publish, so the doc fix closes the realistic case at near-zero cost).
- **[fix(perf), P4] Task #727 (commit `94c4a74`) — 3 parser/test hygiene residuals, the round's last three findings.** `format_sysfs_path`'s internal digit buffer was `[u8; 4]`, which panics for any `node >= 10000` — latent (the crate's only caller iterates `0..64`), but reachable directly through this `#[doc(hidden)] pub` function; resized to `[u8; 10]` (the full decimal-digit range of any `u32`), with a new regression test counterfactual-verified (reverted to `[u8; 4]`, confirmed the exact "index out of bounds: the len is 4 but the index is 4" panic the audit describes, then restored). `parse_hex_u32` silently WRAPPED a hex token longer than 8 digits instead of returning `None` like every other malformed input this parser rejects — added a length guard, counterfactual-verified (disabled the guard, confirmed `Some(4294967295)` instead of the expected `None` for a 9-digit token, then restored). Two smoke tests (`bind_range_no_node_is_noop`/`bind_range_zero_len_is_noop`) are the BANNED "do_thing(); /* no assert */" shape on non-mock builds — kept both (they retain real, narrower value as cross-platform "doesn't crash the real dispatch path" probes) rather than deleting them, with doc comments now stating precisely what they do and don't cover (the actual short-circuit postcondition IS covered for real, in `tests/mock_dispatch.rs`).

Every fix that could be counterfactually tested on this host was, per this project's zero-trust review discipline: reverted, confirmed the associated regression test fails for the right reason with the expected message, then cleanly restored (verified via `git diff` showing zero net change before the final commit). `cargo test -p numa-shim --all-features` (32/32), native + `x86_64-unknown-linux-gnu` cross-compile `clippy --all-features --all-targets -D warnings` (both clean), `cargo fmt --check`, and `cargo doc --all-features --no-deps` all ran clean after every task. `numa-shim`'s own crates.io publish decision (task #657) remained deferred to a maintainer call, gated behind this round's closing `@oh` review (task #751) and its own follow-ups #777-778 below.

#### `numa-shim` — round-closing-review follow-ups (2026-08-09, tasks #777-778)

The round-closing review (task #751, `docs/reviews/2026-08-09-numa-shim-round-closing-review.md`) confirmed the round's shipped parsing/API-surface changes are correct — every mechanically constructible counterfactual reproduces, `#697`'s Linux `mbind` `maxnode` ABI reasoning was independently re-derived from the kernel source and confirmed right — but found one HIGH: task `#723`'s own closing claim was wrong, and its cache design put a heap allocation on this repository's own reentrancy-free allocation path. It also found 12 further findings (F2-F13). Task #777 closed the sole HIGH finding, F1; task #778 closed the remaining 12.

- **[fix(perf) (HIGH), P1] Task #777 (commit `f97bf1d`) — task #723's `OnceLock` topology cache allocated on, and could deadlock, the exact allocation path `AllocCore`'s own M5 invariant declares reentrancy-free.** `topology()`'s `OnceLock<Vec<Vec<u8>>>` initializer performed ~65 heap allocations (one `.to_vec()` per readable node plus the outer `collect()`) — but `current_node()` is reachable from `AllocCore::alloc` (via `current_node_cached` on a cache miss, inside `reserve_small_segment`/`alloc_large_slow`), and `src/alloc_core/alloc_core.rs:9-11`'s own documented invariant states the alloc path "contains NO `unsafe` and NO `Vec`/`Box`/`HashSet`/`std::alloc`... reentrancy-free (M5)" specifically so it cannot recurse into the global allocator; `src/global/tls_heap.rs:20-22` deliberately dropped its own `RefCell` reentrancy guard on the strength of that same invariant, handing out `&mut HeapCore` with no borrow check. Under a real Linux `#[global_allocator] = SeferAlloc` + `numa-aware` deployment (the exact configuration this crate's own README/`src/lib.rs:47` advertise and `ci.yml:629` compiles), the first NUMA-aware allocation would have triggered `topology()`'s heap allocation, re-entering `GlobalAlloc::alloc`, aliasing a second `&mut HeapCore` (UB) and then re-entering `OnceLock::get_or_init` on the same cell mid-initialization — the standard library documents this as "an error to reentrantly initialize the cell... current implementation deadlocks," independently reproduced by the reviewing agent in a standalone scratch project on the same toolchain. No CI job caught it: `numa-shim-mock` bypasses the real `platform` module entirely; the weekly `numa-real-kernel` job exercises real Linux but its test binaries do not install `#[global_allocator]` (grep-verified). Fixed by replacing the heap-backed `Vec<Vec<u8>>` cache with a fixed-size, allocation-free `Topology { len: [usize; 64], buf: [[u8; 1024]; 64] }` held by the same `OnceLock` — populating it touches no `Vec`/`Box`/heap at all, removing the reentrancy hazard structurally rather than guarding against it. `NODE_CPUMAP_BUF_LEN = 1024` (down from `#720`'s original 4096) is a documented DEVIATION, chosen to keep the cache's total static footprint at 64 KiB instead of 256 KiB, while still covering ~3640 CPUs per single node — comfortably past any currently-shipping NUMA system. `cpu_to_numa_node`'s iteration order and the node-0 single-node fallback are preserved exactly (traced by hand against the unchanged `crate::cpumap::parse_contains_cpu`). Appended an append-only correction to the `#723` bullet above per this project's non-retroactive correction convention. REASONED-FROM-SPEC for the reentrancy-removal claim itself (this session is Windows-only; the real deadlock scenario cannot be executed here) — the confidence rests on the new populate path containing no `Vec`/`Box`/heap call anywhere, verified by reading the diff, not on a live re-run of the reviewer's scratch repro.
- **[fix(perf), docs, test, CI, P2-P4] Task #778 (commit `fd2a3bb`) — closed the remaining 12 findings (F2-F13).** **F2 (MEDIUM)** — `#724`'s SAFETY comments/rustdoc stated the `node` argument "has no effect" on the `MEM_RESERVE` call and "takes effect" on `MEM_COMMIT` — the exact INVERSE of Microsoft's documented `VirtualAllocExNuma` `nndPreferred` contract (used only when allocating a NEW VA region, i.e. the reserve call; ignored when committing into an existing region); net shipped behavior was correct by accident, but a future editor trusting the old comments would have every reason to disable Windows NUMA binding entirely with no error — corrected all sites. **F3 (MEDIUM)** — `#724`'s own "EMPIRICALLY VERIFIED" evidence (`reserve_on_node_returns_valid_span`/`reserve_on_node_large_align_round_trip`) was shown to pass IDENTICALLY against the reverted pre-fix double-commit bug (personally reproduced: reverted, both cited tests still passed 6/6); added a genuine `VirtualQuery`-based regression test asserting the region beyond `[base, base+size)` reports `MEM_RESERVE` not `MEM_COMMIT`, counterfactual-verified (reverted `#724`'s fix, confirmed the tail region reports `MEM_COMMIT`, which also validated the test's own locally-declared `MemoryBasicInformation` struct layout). **F4/F5 (MEDIUM)** — filed 4 audit findings that had no durable index record into `docs/CORRECTNESS_OPEN_ITEMS.md` (items 44-47: the mbind path's own missing behavioral oracle, `CURRENT_NODE_SLOT` should be `Cell` not `RefCell`, `reserve_on_node`'s public signature coupling numa-shim's semver to `aligned-vmem` 0.2, and the round's own Linux-only never-executed-here status); updated item 42's Status card to CLOSED, recording `#726` as the matching numa-shim-side resolution of the `mock`-feature-unification deferral that item explicitly handed to this round. **F6 (LOW-MEDIUM)** — `#725`'s rewording missed a SIXTH audit-flagged `bind_range` test call site (a real `Vec<u8>` heap buffer against a real platform backend); reworded the `# Safety` contract to "valid mapped range" plus a page-granularity caveat and moved the task-history narrative out of the `# Safety` heading. **F7 (LOW-MEDIUM)** — `#726`'s new `CALLS_CAP` silently truncated past 4096 entries with `drain()`'s own rustdoc still claiming to drain "every recorded call"; made `CALLS_CAP` `pub`, documented the cap on `drain()` and the `mock` module doc, updated the test to assert against the real constant instead of a hardcoded mirror. **F8 (LOW)** — already corrected in `#777`'s own commit (the 15 cpumap tests are `#721`'s, not `#720`'s). **F9 (LOW)** — `cargo clippy -p numa-shim --all-targets -- -D warnings` FAILED in the crate's DEFAULT feature configuration (what `cargo add numa-shim` produces) with an unused `GetCurrentProcess`; moved it into the `vmem-integration`-gated extern block and added clippy steps to the existing `numa-shim-mock`/`numa-shim-windows` CI jobs (actionlint-clean). **F10 (LOW)** — reconciled the round header's "Runtime improvements: 2" count (previously `#697`+`#723`, the latter unmeasured and per F1 factually wrong about its own behavior) to `#697`+`#724` (the two commits whose own bullets actually support "observable behavior changed"). **F11 (INFO)** — replaced stale hardcoded line-number citations in the macOS-stub comment with a grep-able role description. **F12 (INFO)** — documented the torn-snapshot-during-populate hotplug caveat (distinct from the already-documented after-first-call caveat) and `current_node()`'s first-call syscall/allocation cost on its public rustdoc. **F13 (INFO)** — documented the deliberate decision to leave `MockCall::CurrentNode` without variant-level `#[non_exhaustive]` (a single scalar field has no plausible growth path the way `bind_range`/`reserve_on_node`'s multi-argument calls do). Verified: `cargo test -p numa-shim --all-features` (33/33, incl. the new F3 regression test); native + Linux-cross-compile clippy `-D warnings` AND default-features-only clippy (all clean); `fmt --check`; `cargo doc --all-features --no-deps` AND `--features vmem-integration --no-deps` (both clean); `actionlint .github/workflows/ci.yml` (clean).

All 13 findings from the round-closing review (F1, closed by task #777 above; F2-F13, closed by task #778 above) are now closed. `numa-shim`'s own crates.io publish decision (task #657) remains deferred to a maintainer call, gated behind items 46 (semver coupling) and 47 (never-executed-on-Linux status) in `docs/CORRECTNESS_OPEN_ITEMS.md`, both newly filed by task #778 — **correction (task #755's size-classes round-closing review, F7):** this sentence previously read "gated behind this two-task follow-up now being genuinely complete," which a reader could take to mean the gate had cleared with only a maintainer's signature left; it had not — `#778` created the two blocking items above in the same commit. The last workspace sub-crate's own rust-intel audit findings (size-classes — tasks #701/#728-731) followed next, one crate at a time — see the section immediately below.

#### `aligned-vmem` — post-campaign closing review remediation (2026-08-12, tasks #851-857)

A fresh post-campaign closing review (`docs/reviews/2026-08-12-aligned-vmem-post-campaign-closing-review.md`, findings W1-W16 + 3 perf notes) found one HIGH (a second, independent miri compile break from #844, closed by task #851) plus 15 lower-severity findings. All 7 tasks delegated to `/crush` with full personal zero-trust re-verification. **This is a sub-crate campaign — `aligned-vmem` is a workspace member library not consumed by `sefer-alloc`'s own `production` feature bundle, so none of these changes affect the allocator's shipping runtime.**

- **[fix(vmem), P0] Task #851 (commit `78ecc81`) — W1: fixed a second, independent miri compile break.** Task #844 correctly reverted miri's `reserve_aligned_raw` from the 4-tuple #843 had given it back to the 3-tuple shape the Windows/Unix backends use, but left two feature-gated `.map()` closures still destructuring 4 elements (`crates/vmem/src/lib.rs:2239`, `:2250`). Fixed by removing `_granted_huge` from both patterns. Added a miri compile-only CI guard to `aligned-vmem-gates` in `.github/workflows/ci.yml`: `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features`.
- **[fix(vmem), docs(vmem), P2] Task #852 (commit `8ea67ed`) — W2: fixed Unix `granted_huge` semantics; W3: corrected Windows huge-pages documentation.** On non-Linux Unix, `libc_mmap` silently discards the `huge` request parameter (MAP_HUGETLB is Linux-only), but both Unix return paths used to report the caller's REQUEST unconditionally, so `Reservation::is_huge()` falsely returned true for an ordinary-page reservation on every non-Linux Unix. Fixed with a new `HUGE_SUPPORTED` const (true only on Linux with the huge-pages feature enabled). Separately: corrected the Windows huge-pages documentation, which had drifted false in both directions across the campaign. Task #848 added a single-call reservation fast path for align <= 64 KiB that happens to issue exactly the `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` combination Windows requires — meaning huge pages CAN now succeed on Windows for small aligns, but 4 doc sites plus the README still said "always false" / "not functional." Corrected all sites to state the real, narrow condition (align <= 64 KiB, size a multiple of the OS large-page minimum, SeLockMemoryPrivilege held). **P-A (INFO, zero-risk):** skipped a provably-true alignment check on the Unix fast path (for align <= page_size() the alignment check is always false by construction, since any mmap result is already page-aligned).
- **[fix(vmem), docs(vmem), P2] Task #853 (commit `1cdae19`) — W4: split the Windows syscall counter into single-call vs two-call; W11: bench-internals raw statics are now `#[doc(hidden)]`.** `WINDOWS_RESERVE_COMMIT_CALLS`'s rustdoc claimed "each call issues exactly 2 syscalls" — false since task #848 added a single-syscall fast path that incremented the SAME counter. Split into `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` / `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`, each incremented at its own real call site. Separately: the four bench-internals `AtomicU64` statics (`UNIX_EXACT_RESERVE_ATTEMPTS`/`HITS`, `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`/`TWO_CALL_PAIRS`) were plain `pub` with docsrs badges — documented, semver-committed API surface a downstream consumer could corrupt directly. Marked `#[doc(hidden)]` on the statics only; the accessor functions remain `pub` with their docsrs badges intact.
- **[docs(vmem), P2] Task #854 (commit `0370b17`) — W5: fixed trim/over-reserve doc drift + documented Unix VA cost.** `unix_reserve`'s over-reserve fallback path (taken on an exact-mmap alignment miss) used to `munmap` the head/tail slack around the aligned sub-range, trimming the reservation down to exactly `size` bytes — the trim was VA-leak-prone on the failure path (an intervening error before the trim completed could leak the untrimmed remainder). Task #842 fixed this by keeping the whole over-reservation mapping, but several doc sites still described the old behavior. Updated all affected doc sites to match the new "no trim" behavior. Separately: documented the measured Unix VA cost of the over-reserve path (the real cost of the exact-mmap miss case, which was not measured or documented before this change).
- **[docs(vmem), fix(vmem), P2] Task #855 (commit `e8e204a`) — W6-W13 + P-B: hygiene bundle.** Moved 2 orphaned doc comments off `validate_size_align` and `RawReservation` onto their real owners; added a doc note to `RawReservation` stating plainly it is a call-site convenience, not the tuple-transposition hazard's full elimination; corrected `page_size()`'s rustdoc to the crate's own honest all-or-nothing wording; `tests/mock.rs`'s `records_reserve_and_decommit` now uses `page_size()` instead of the hardcoded `PAGE` constant for decommit offsets; README's alignment-contract section now states the real decommit-vs-recommit granularity asymmetry; added rows to the API table for `MIN_PAGE`, `ReservationParts`/`release_parts`, `is_huge`, and the `VmemError`->`io::Error` bridge; added a doc note to `ReservationParts` explaining it currently has no public constructor; added doc notes to `decommit`/`decommit_lazy` explaining the missing `try_` form is a deliberate design choice. **P-B (INFO):** hoisted the duplicate `page_size()` call in both `decommit` and `decommit_lazy` into one local binding.
- **[ci, P1] Task #856 (commit `c3d38c1`) — W14: added macOS test row + feature-powerset sweep for aligned-vmem.** Added `cargo test -p aligned-vmem --all-features --no-fail-fast` to the test-macos job in `.github/workflows/ci.yml`, so this crate's Darwin-specific code paths get real macOS hardware execution in CI. Separately: added a weekly-cadence feature-powerset job for aligned-vmem: `cargo hack check --feature-powerset --depth 2 -p aligned-vmem --no-dev-deps` (resolves to exactly 21 invocations, all passing).
- **[docs, P1] Task #857 (commit `7663811`) — W15/W16: paper trail for task #849 measurement.** Zero-trust review found the delegate's delivery was substantially fabricated — discarded entirely and rewritten from verified facts. The commit replaces a fabricated draft CHANGELOG entry (with invented test file names, source paths, mock::Call variant names, and perf numbers) with a genuine entry using only facts verified while authoring or zero-trust-reviewing every one of the 9 real commits (#842-850) earlier in the same session. Separately: replaced a fabricated raw log file (containing 30 lines of per-run data with an obvious 7-run repeating cycle) with the REAL 30-run per-process output from task #849's own work; corrected an item-number collision in `docs/perf/OPEN_ITEMS.md` (renumbered from item 31 to item 46).

All 16 findings from the post-campaign closing review are now closed. `aligned-vmem`'s own crates.io publish decision (task #658) remains deferred to an explicit maintainer go-ahead, gated behind this follow-up round finishing.

#### `size-classes` — rust-intel audit remediation (2026-08-09, tasks #701/#728-731)

Continuing (and concluding) the same one-crate-at-a-time `rust-intel` remediation sweep — this is the SIXTH and LAST crate in the sequence. All 5 tasks landed as individual commits. Unlike `numa-shim`'s round (mostly Linux-only, gated behind cross-compile-only verification on this Windows session), `size-classes` is a plain `no_std` crate with no platform-specific `#[cfg]` gating at all — every fix here got a genuine, native, EMPIRICAL zero-trust counterfactual (revert the fix, confirm the associated test fails for the right documented reason, restore, confirm zero net `git diff`), not REASONED-FROM-SPEC. **Runtime improvements: 0** — every fix below is a `const`-eval-time, release-active-precondition, or debug-only guard addition, an API-surface decision, or documentation **(correction, task #755's closing review, F9: the previous wording "const-eval-time or debug-only guard promotion" undersold that `#701`'s `checked_mul`/`checked_add` and `#731`'s two `assert!`s are release-active RUNTIME guards, not const-eval-time or debug-only — see `#701`'s own bullet below, which already states this correctly: "the fix's real effect is entirely on the previously-unguarded runtime call sites")**; no shipping algorithm's OBSERVABLE runtime behavior changed on any in-contract input.

- **[fix(perf), P1] Task #701 (commit `7ffeba5`) — the geometric-advance multiply could silently wrap in a release profile, with the min-step fallback masking the wrap into a valid-looking-but-wrong table.** This crate's own audit's HIGHEST-severity finding (MEDIUM §B26; 0 critical, 0 high in the whole crate). `build_table`'s geometric-advance step computed `let mut next = (cur * num).div_ceil(den)` with a BARE multiply on `cur`, a value that grows on every step — since `build_table`/`Params` are `pub` with some call sites reachable at runtime (not just `const` table construction), this crate cannot assume its consumer builds with `overflow-checks = true`. A release-profile wrap would then hit the very next line's `if next <= cur { next = cur + min_block }` min-step fallback, which MASKED the wrap into a valid-looking, strictly-increasing table (min_block-sized steps instead of the requested geometry) rather than surfacing any error — `build_size2class`'s own downstream monotonicity check cannot catch this, since the masked table genuinely IS still strictly increasing. Fixed with `checked_mul`/`checked_add` and a named panic message; in `const` context this was already a hard compile error either way, so the fix's real effect is entirely on the previously-unguarded runtime call sites. Added `geometric_advance_overflow_panics_instead_of_silently_wrapping` (`min_block = 2^63`, doubling growth, `geo_count = 2` — the smallest reproduction reaching the first checked multiply). Zero-trust counterfactual with a genuine methodological subtlety: temporarily reverted both checked ops to `wrapping_mul`/`wrapping_add` and re-ran — DEBUG mode still panicked, but for a DIFFERENT reason (a separate, untouched bare `+` in the min-step fallback still trips debug's own overflow-checks after the wrapped multiply lands `next` back at 0), so debug alone was not a valid counterfactual; re-ran under `--release` (where ALL arithmetic wraps silently, including that fallback add) and the test correctly FAILED ("test did not panic as expected"), confirming the pre-fix code genuinely produces a silently-wrong table with zero diagnostic exactly as the audit describes. Restored the fix, confirmed the suite passes again with zero net diff.
- **[fix(perf), P2] Task #728 (commit `a80ba49`) — decided `Params`' publish-blocking API posture: `#[non_exhaustive]` + a `const fn` constructor.** §C1a: this crate is queued for its first crates.io publish (task #660), and retrofitting `#[non_exhaustive]` on an already-published all-pub-field config struct is itself a breaking change, so this had to be settled now. `Params<'a>` was constructed by consumers (including this workspace's own root `sefer-alloc` crate) via struct literal with no `#[non_exhaustive]` and no constructor — field growth is plausible (the audit itself named `small_align_max`, currently hardwired to `min_block` inside `SizeClasses::build`, as an obvious future knob). Added `#[non_exhaustive]` AND `Params::new(min_block, growth, geo_count, extras, huge_threshold)`, a `const fn` so it works in the same `const PARAMS: Params = ..` binding shape the old struct-literal syntax did — plain `#[non_exhaustive]` alone would have made the type unconstructable downstream, since `const` context has no `Default`/functional-record-update escape hatch. Updated all 9 construction sites across the workspace (8 in `crates/size-classes/tests/{builder,proptest_builder}.rs`, plus the one real external consumer, this workspace's own `src/alloc_core/size_classes.rs`) — **correction (task #755's closing review, F8):** this bullet and this task's own commit message originally said "10"/"9 in tests/"; `git show a80ba49 -- crates/size-classes/tests/ | grep "^+.*Params::new"` returns 8 lines at the time of that commit, not 9 (the current higher count in those files includes 3 sites added LATER, by tasks `#729`/`#730`). Verified real enforcement, not vacuous: reverting any one site to the old struct-literal form reproduces `E0639` ("cannot create non-exhaustive struct using struct expression") — the identical pattern task `#715`/`#726` already established for `aligned-vmem`'s/`numa-shim`'s `mock` enums this same sweep. Folded in the audit's separate INFO §B1b note (recorded, not a defect): `Params`'s borrowed lifetime `'a` is justified for a `no_std`, zero-alloc, `const`-fn crate — added one doc sentence noting `'a` typically resolves to `'static` in real `const` usage.
- **[fix(perf), P2] Task #729 (commit `5741243`) — `class_for`'s non-power-of-two-`align` precondition was undocumented and unchecked in BOTH internal paths.** §F2/§B26: the documented fit predicate ("`block_size % align == 0`") could be silently violated for a non-pow2 `align` — the fast path returned `seed` unconditionally with no divisibility check at all, and the slow path's bitmask round-up (`(block | (align - 1)) + 1`) is only correct for a power-of-two `align`, silently overshooting for a non-pow2 one. The pow2 precondition existed only in an INTERNAL slow-path comment, never in `class_for`'s own public contract, and no test (hand-written or proptest) ever generated a non-pow2 `align`. Fixed: documented the precondition explicitly and added `debug_assert!(align.is_power_of_two())` — deliberately a `debug_assert`, not a release-active `assert!`, unlike task `#701`'s promotion, since the failure mode here is a suboptimal/wrong CLASS CHOICE for a contract violation, not memory unsafety or a corrupted table (every real caller in this repo derives `align` from `core::alloc::Layout`, which already guarantees power-of-two, so practical exposure was already low). Added `class_for_non_pow2_align_violates_debug_assert`, zero-trust counterfactual verified (removed the guard, confirmed the test correctly failed with "test did not panic as expected," restored).
- **[test, P3] Task #730 (commit `d07102a`) — 3 test-hygiene defects, all in `tests/builder.rs`.** §D1 MEDIUM: `extras_overlapping_geometric_run_panics`'s `#[should_panic(expected = "strictly increasing")]` matched BOTH `build_table`'s OWN "Params::extras: must be strictly increasing" message (reachable in the test's SETUP) AND `build_size2class`'s "table must be strictly increasing" message (the actual chokepoint under test) — a spurious setup-path panic could have coincidentally satisfied the expectation, silently defeating the test. Narrowed to the `build_size2class`-specific prefix; zero-trust counterfactual verified by temporarily corrupting the setup (`extras=[32,16]`, not strictly increasing) and confirming the tightened test correctly FAILS on the wrong-site panic message rather than passing. §D1a/§F1 INFO: `reference_table`'s rounding/spacing core is byte-identical to `build_table`'s own formula — a circular-oracle shape that can only prove const-eval and runtime-eval agree on ONE expression tree; added `geometric_run_matches_hand_derived_golden_values`, 8 classes of the `(5,4)`/`16` scheme computed BY HAND (arithmetic shown in the test's own comment), independent of both `build_table` and `reference_table`. §D1 INFO: `is_huge_uses_the_policy_threshold_not_an_os_constant`'s own comment promised "two different thresholds → two different verdicts," but the test built only ONE scheme — an `is_huge` hardcoded to compare against the literal 1024 would have passed; added a second scheme (`huge_threshold: 4096`) and asserted the SAME size gets OPPOSITE verdicts across both, zero-trust counterfactual verified (temporarily hardcoded `is_huge` to `size >= 1024`, confirmed the new cross-scheme assertion correctly failed, restored).
- **[fix(perf), P4] Task #731 (commit `9d2d2fa`) — 4 small doc/validation residuals, batched.** §B26: the growth denominator (`params.growth.1`) was never asserted non-zero, hitting a bare "attempt to divide by zero" instead of a named diagnostic like every sibling precondition — added `assert!(params.growth.1 > 0, ..)` (deliberately left `growth.0 == 0` unguarded — it already degrades to a valid, if unusual, linear table via the existing min-step fallback, not a contract violation). §B26: `size2class_len` was this crate's ONE `pub fn` with zero parameter validation — added the matching `min_block.is_power_of_two()` assert every sibling entry point already has. §F2: `SizeClasses`'s struct-level "no panics on the lookup path" claim was directly contradicted by `block_size`'s own documented `# Panics` on the same type — qualified the struct-level claim to state it holds for in-contract inputs only. §F2: `README.md`'s "an arbitrary sorted list of explicit extra classes" understated the machine-checked `Params::extras` preconditions (strictly increasing, `min_block`-multiple, both const-eval panics on violation) — corrected to match `Params::extras`'s own already-accurate rustdoc.

Every fix in this round got a genuine EMPIRICAL zero-trust counterfactual (this crate has no platform-`#[cfg]` gating, so every verification ran natively on this session's own host, unlike most of `numa-shim`'s round). `cargo test -p size-classes --all-features` (14/14), `cargo clippy -p size-classes --all-features --all-targets -D warnings` (clean), `cargo build -p size-classes --target thumbv7em-none-eabi` (this crate's own advertised `no_std` bare-metal target, clean), `cargo fmt --check`, and `cargo doc --all-features --no-deps` all ran clean after every task; each task additionally re-verified the one real in-repo consumer (this workspace's own root `sefer-alloc` crate, `src/alloc_core/size_classes.rs`) still compiles and lints clean under `--features "production internals"`. `size-classes`' own crates.io publish decision (task #660) remains deferred to a maintainer call, gated behind this round's closing `@oh` review (task #755) — the FINAL task-generating step of this session's entire six-crate `/rust-intel` sweep (sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem → numa-shim → size-classes).

#### `size-classes` — round-closing-review follow-ups (2026-08-09, tasks #779-780)

The round-closing review (task #755, `docs/reviews/2026-08-09-size-classes-round-closing-review.md` — the report itself required a second review-agent launch: the first background agent died mid-stream with a connection error and produced no report, the identical failure class that hit `numa-shim`'s own review three times earlier this session; a fresh agent with the same prompt succeeded on its first attempt) confirmed all 5 fix commits are, on their own terms, correct — hand-re-derived #730's golden values, re-ran #701's debug-vs-release counterfactual independently, reproduced `E0639` against `#[non_exhaustive]` from outside the crate, confirmed `Params::new`'s argument order, confirmed #731's two new `assert!`s trip no existing call site — but found 9 findings AT THE BOUNDARY the round itself never looked at: what the round's fixes touch OUTSIDE `crates/size-classes/`. Three were HIGH, and **`main` was genuinely red** at the review's own baseline commit (`9018c07`) — a first for this six-crate sweep; every prior crate's closing review found a real defect, but none had actually broken CI at review time. Task #779 closed the 3 HIGH findings (F1-F3); task #780 closed the remaining 6 (F4-F9).

- **[fix(perf), test, CI (HIGH), P1] Task #779 (commit `2ca3537`) — task #729's new precondition guard broke an existing, CI-covered root test; the natural CI fix then exposed a third, release-profile-only bug.** **F1** — `tests/medium_classes_correctness.rs`'s `item1_mib_alignment_resolves_to_small_not_large` looped `MEDIUM_SIZES` (256/320/384/512/768 KiB, 1 MiB — three NOT powers of two) as the `align` argument to `SegmentLayout::class_for`; task `#729`'s new `debug_assert!(align.is_power_of_two())` fired, failing this test in 3 whole-suite CI feature rows (`ci.yml:387`, `:426`, `:599`). `#729`'s own justifying rustdoc sentence — "every real caller in this repo derives `align` from `core::alloc::Layout`, which already guarantees power-of-two by construction" — was simply FALSE, and a single `grep -rn "class_for(" tests/` would have shown the offending call site; `#729`'s own verification block ran `cargo check`/`cargo clippy` on the root crate but never `cargo test`, and `production` (the feature set those DID run under) does not enable `medium-classes`, so even a root `cargo test` under that exact combination would have stayed green — two independent gaps had to line up. Fixed by restricting the align-axis loop to the pow2 members of `MEDIUM_SIZES` (the size-axis loop two blocks below already covers all six values correctly on the axis that carries no pow2 precondition) and correcting the false rustdoc claim on `class_for`. **F2** — `size-classes` had ZERO `cargo test` step anywhere in `ci.yml` — only a `cargo build --target thumbv7em-none-eabi` cross-build compiling no test target; all 14 of the crate's tests, including this round's 3 new regression tests, had never executed in CI — the IDENTICAL gap class this same sweep already closed for `tagged-index-stack` (`#772` F4/F5) and `racy-ptr-cell` (`#773` F1), but nobody had checked whether the sixth crate had it too. Added both a debug and `--release` `cargo test -p size-classes --all-features --no-fail-fast` row, mirroring the `racy-ptr-cell` pattern. **F3** — the F2 fix's new `--release` row immediately failed: `class_for_non_pow2_align_violates_debug_assert` (`#729`'s own new test) is `#[should_panic]` against a `debug_assert!`, which compiles away entirely in `--release`. Gated the test to `#[cfg(debug_assertions)]` — the only profile that can satisfy it — rather than promoting the guard to a release-active `assert!` (that would be a hot-path behavior change, and would turn F1's guard into a shipped release panic instead of a debug-only test failure). Zero-trust counterfactual on every piece: reverted the F1 test-loop restriction and confirmed the new debug CI row reproduces the exact panic message reported; reverted the F3 `#[cfg(debug_assertions)]` gate and confirmed `--release` fails exactly as predicted ("test did not panic as expected"); restored both, confirmed green; re-ran all three previously-failing root CI feature combinations (`hardened medium-classes internals`; `--all-features`; `production medium-classes exact-span-large internals`) against `tests/medium_classes_correctness.rs` and confirmed all green (13/13, 1/1 targeted, 14/14). `cargo fmt --check` and `cargo clippy --features "hardened medium-classes internals" --tests -- -D warnings` both clean on the root crate.
- **[fix(perf), docs, P2-P4] Task #780 (commit `ab269a5`) — closed the remaining 6 findings (F4-F9).** **F4 (MEDIUM)** — the geometric-advance min-step fallback's bare `+` (`next = cur + min_block`) shared the exact overflow hazard `#701` fixed on its two neighbouring `checked_mul`/`checked_add` calls — `#701`'s own commit message had NAMED this exact line as a known-but-unfixed sibling. Reachable with a `min_block` in the 2^62+ range, and on EVERY step of the `growth.0 == 0` linear-degradation scheme `#731` itself documents as valid (that scheme's fallback IS the only advance path); reproduced pre-fix: `min_block = 1 << 62` produced a table with a duplicate AND a zero-sized class — not even monotone, worse than the bug `#701` fixed, whose masked table was at least strictly increasing. Fixed with the same `checked_add` pattern as its neighbours; added `min_step_fallback_overflow_panics_instead_of_silently_wrapping`, zero-trust counterfactual verified (reverted to the bare `+`, confirmed the test fails under `--release` with the exact silent-wrap signature F4 described, restored). **F5 (MEDIUM)** — `README.md`'s only code example — the crates.io/docs.rs front page — still used the `Params { .. }` struct-literal syntax `#728` made a hard `E0639` compile error (`#728`'s own commit message claimed "every construction site" was updated, but missed the README; `#731` edited this same file three lines above without noticing); converted to `Params::new(..)` and added a line documenting `Params` is `#[non_exhaustive]`. **F6 (LOW)** — this workspace's own root shim over the crate (`src/alloc_core/size_classes.rs`) still carried the unqualified "no panics on the lookup path" claim `#731` had already qualified to "for in-contract inputs" on the crate side — now doubly wrong since `#729` added a real panic to `class_for` that this shim forwards straight into; also corrected an adjacent pre-existing "(debug)" claim on `block_size`'s panic doc to "(all profiles)" — it panics via a bounds-checked array index in every profile. **F7 (LOW)** — corrected the numa-shim closing-review-follow-ups section above (append-only, per this project's non-retroactive correction convention): it said numa-shim's publish decision was "gated behind this two-task follow-up now being genuinely complete," but `#778`'s own commit message says it's gated behind two NEW items (46, 47) that commit filed in `docs/CORRECTNESS_OPEN_ITEMS.md`, one of which records the round's Linux-only code was never empirically executed. **F8 (INFO)** — corrected `#728`'s construction-site count above (append-only): "10"/"9 in tests/" was off by one — `git show a80ba49 -- crates/size-classes/tests/` shows 8 sites at that commit, 9 total, not 10 (the current higher count includes 3 sites added later by `#729`/`#730`). **F9 (INFO)** — corrected this round's own header above (append-only): "every fix below is a const-eval-time or debug-only guard promotion" undersold that `#701`'s and `#731`'s guards are release-active RUNTIME guards, not const-eval-time/debug-only. Verified: `cargo test -p size-classes --all-features` (11/11 debug, 10/10 release — the extra debug test is F3's `#[cfg(debug_assertions)]`-gated test), `cargo clippy -p size-classes --all-features --all-targets -- -D warnings` clean, `cargo fmt -p size-classes -- --check` clean, `cargo build -p sefer-alloc --features "production internals"` confirms the root crate still compiles against the shim doc changes.

All 9 findings from the round-closing review (F1-F3, closed by task #779 above; F4-F9, closed by task #780 above) are now closed, and `main` — genuinely red at this round's baseline for the first time in this sweep — is green again. `size-classes`' own crates.io publish decision (task #660) remains deferred to a maintainer call. **This closes the ENTIRE six-crate `/rust-intel` audit sweep** (sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem → numa-shim → size-classes): every crate's fix round AND every crate's closing-review follow-up round has now landed, verified, and been recorded here. Matching this sweep's established practice (no prior crate's closing-review-followup round was itself put through a second `@oh` review), no further review is planned for this followup round.

#### `sefer-region` — round-closing-review follow-ups (2026-08-08/09, tasks #769-770)

The round-2 closing review (task #743, `docs/reviews/2026-08-08-sefer-region-round2-closing-review.md`) verified the `sefer-region` rust-intel audit remediation round (tasks #694-696) and found 7 findings (A-G). All are now closed.

- **[fix(perf), P2] Task #769 (commit `f9e2618`) — closed the remaining drain-order dependence in `clear_partial_under_panic` (finding A).** Task #694 (commit `ea52f85`) made the "which IDs were dropped" half of the test's assertions order-agnostic (via `HashSet` complement), but kept the exact `drop_count == 3`/`len() == 2` pair — which IS a direct function of the bomb's ordinal position in slotmap's drain visitation order (verified at slotmap 1.1.1 source: `SlotMap::clear → drain() → Drain::next` walks `cur` ascending from 1). That pair IS the drain-order oracle filed as a false-red hazard in the round-2 review's independent empirical probe: moving the bomb to the last slot yields `drop_count=5/len=0`, to the first slot `drop_count=1/len=4` — both would fail the OLD committed assertions while the crate's order-free partial-clear contract holds perfectly in every case. Replaced the exact pair with the genuinely order-free invariant `drop_count + len() == 5` (total constructions = drops + survivors) in both the `Region` and `SyncRegion` scenarios, adjusted the two downstream assertions that also depended on the exact split (post-refill `len()` check, total-drops-after-second-clear commentary) to use order-agnostic checks instead, and reworded the misleading in-code comment that previously denied the dependence existed. Verified non-vacuous with a real counterfactual: temporarily moved `bomb_id` to slot 4, then separately to slot 0 — both variants pass cleanly under the new assertions, confirming `drop_count + len() == 5` holds regardless of drain position.
- **[docs, P3] Task #770 (commit `6cb3f6b`) — closed 3 small accuracy residuals (findings B, D, E).** **Finding B:** `coverage_gaps.rs`'s `region_reserve_reuses_freed_slots_on_churn` test comment described a counterfactual that was never run ("bypassing the slotmap free list, inserting fresh handles instead of reusing freed slots") — the counterfactual actually performed for task #696 was raising the refill loop from 500 to 700, over-running the 500 freed slots. Reworded to describe what was actually done. **Finding D:** `CHANGELOG.md`'s corrected #670 bullet (added by task #733) said the cited counterfactual "only ever exercised an unrelated `len()` assertion positioned earlier in the same test" — reconstructing `185df1b`'s exact assertion block shows the failing `len()` assertion sits ONE LINE AFTER the capacity oracle, not before it (inherited from task #678's own commit message, which had the same inversion). Fixed to "immediately after". **Finding E:** five of nine bullets in the #685-693 CHANGELOG subsection (tasks #685-689) cited no commit SHA, while every other bullet in this section across both prior rounds does. Added the five SHAs, each verified against `git log -1 <sha>` before writing: `25de4cd` (#685), `127545b` (#686), `5e4244f` (#687), `ec59520` (#688), `5985a61` (#689). All three are comment/CHANGELOG wording only — no test behavior changed.

#### `sefer-region` — domain-aware `Handle<T>` identity redesign + release-readiness follow-ups (2026-08-09/10, tasks #802-803/#805-808)

The static release audit's central finding, F2, is closed with a real breaking change: before this, a `Handle<T>` minted by one `Region<T>` was silently accepted by any OTHER `Region<T>` of the same type, because branding was by value type `T` only — never by `Region` instance. Since a fresh `Region`'s first insert tends to produce the same `slotmap::DefaultKey`, this was not a rare edge case: two freshly-created `Region<T>`s routinely aliased each other's first value. User-visible symptom (pre-fix): `region_a.get(handle_from_region_b)` could return `Some(&wrong_value)` instead of `None`. **User-authorized breaking change** ahead of any real downstream consumers (no crates.io publish had happened yet) — **targets 0.2.0** (the `Cargo.toml` version bump itself is deferred to the Stage E release gate, task #801, per this repo's never-bump-without-explicit-request rule; `Cargo.toml` is still `0.1.0` as of this entry).

- **[feat(region)!, P0] Task #802 (commit `9741388`) — domain-aware `Handle<T>` identity: reject cross-`Region` aliasing (F2).** Runtime instance-id, not compile-time generative branding (a lifetime-brand/generativity approach would force every `Region::new()` through a scoping macro — too heavy an ergonomics tax for a plain typed handle store). `Region<T>` gains a private `region_id` field stamped from a process-wide atomic counter in both `new()`/`with_capacity()`; `Handle<T>` gains a matching `region_id` field next to `key`. Every accessor (`get`, `get_mut`, `contains`, `remove`) checks `handle.region_id == self.region_id` before touching the backing slotmap — a mismatch is treated exactly like a stale handle (`None`/`false`), never a new panic or `Result`/`Error` variant, preserving the existing all-`Option` API shape. `SyncRegion<T>` needed no code changes (delegates through `Region<T>` under its `RwLock`). **`Handle<T>` grows from 8 to 16 bytes on a 64-bit host** (`#[repr(transparent)]` is no longer valid with two fields and is replaced with **`#[repr(C)]`**); `Option<Handle<T>>`'s niche optimization is preserved. New invariant **I6 — instance isolation**, documented in `lib.rs`/`region.rs`/README. `region_id` was initially `AtomicU64`/`NonZeroU64`.
- **[feat(region), P1] Task #803 (commit `c077fd2`) — F14 API-ergonomics remainder, sequenced after #802 so it operates on the final post-redesign layout.** New public items: `SyncRegion::into_inner(self) -> Region<T>` (recovers from RwLock poisoning, mirroring `read()`/`write()`); `Debug` for `Region<T>` and `SyncRegion<T>` (`#![warn(missing_debug_implementations)]` added now that both carry an impl); `IntoIterator for &Region<T>` / `&mut Region<T>` (owned `IntoIterator for Region<T>` deliberately NOT added — would expose raw `DefaultKey`s, breaking the "raw keys never escape" guarantee); `PartialOrd`/`Ord` for `Handle<T>` (hand-written, comparing `key` then `region_id`, consistent with `Eq`); widened `iter`/`iter_mut` bounds to `ExactSizeIterator + FusedIterator` (`iter()` additionally `Clone`). `Extend`/`FromIterator` deliberately NOT added (F26 caution: collecting into a `Region` mints new handles, silently invalidating callers' old ones). New public newtypes **`Iter<'a, T>`/`IterMut<'a, T>`** wrap slotmap's own iterator types so no third-party type path leaks into this crate's public API (a real semver liability the crush-delegated diff had missed — caught in personal zero-trust review and fixed before landing).
- **[fix(perf), P1] Task #805 (commit `3a77e1a`) — F1 (HIGH): fix a real no_std build break `#802` introduced.** `region_id`'s `AtomicU64`/`NonZeroU64` broke the build on bare-metal `no_std` targets without 64-bit atomics (`thumbv7em-none-eabi`, `riscv32imc`/`imac`, `msp430`, `avr`) that this crate advertises supporting. Widened to `AtomicUsize`/`NonZeroUsize`: `Handle<T>` stays 16 bytes on a 64-bit host, **shrinks to 12 bytes on a 32-bit host** (empirically verified on both `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`). Also documented the previously-unstated `# Panics` contract on `new`/`with_capacity`/`Default` (region-ID counter exhaustion after `2^{pointer_width} - 1` `Region` constructions).
- **[fix(region), test, docs(ci), P2] Task #806 (commit `2a6e050`) — F3+F4 (MEDIUM).** F3: the `cargo-semver-checks` CI step's comment overclaimed what the tool catches — verified it reports `0.1.0->0.1.0 no semver update required` even for #802's breaking identity change, because it only compares rustdoc-visible API shape, not runtime semantics, and doesn't check whether the manifest version is already published; reworded the comment and added a separate CI step that queries crates.io's sparse index and fails if the current `Cargo.toml` version for `crates/region` is already published. F4: `SyncRegion`'s `Debug` impl collapsed "poisoned but free" and "held by another thread" into the same `"<locked>"` placeholder; now matches `TryLockError::Poisoned` explicitly and reports `poisoned: true`, matching `std::sync::RwLock`'s own `Debug` shape.
- **[test, docs(region), P2] Task #807 (commit `0c83f14`) — F5+F7 (MEDIUM): close I6 coverage gaps.** I6 — the release's headline new invariant — had zero test coverage on `get_mut`, on `SyncRegion` entirely, in the README's own invariants list, under miri, and in the fuzz oracle. Added cross-instance-rejection tests for all of the above, including a new miri-covered case in the root `tests/region_invariants.rs` and a new `Op::CrossRegion` branch in `fuzz/fuzz_targets/region_ops.rs` (compiles/lints clean; an actual `cargo fuzz run` was not executed — `cargo-fuzz` was not available locally).
- **[bench(region), P3] Task #808 (commit `57013c8`) — F6 (MEDIUM): re-measure perf tables post-F1/F2.** README's Performance table and "Wrapper overhead" A/B section were measured before #802's per-accessor `region_id` check and #805's `usize` widening landed. Re-measured on commit `0c83f14`: deltas small across every row, within/near prior noise band on this single noisy Windows dev host — no regression from the added check on an already-warm cache line (`get_hit`: 5.0 → 5.4 ns/op). Added two workloads: `st/get_wrong_region` (a cross-`Region` handle rejected before any slotmap generation check — the *cheapest* `get` variant, 4.8 ns/op, not a regression) and a manual `std::thread::scope` contention workload for `Region::new()` against the shared region-ID counter (8 threads, ~13.9M `Region::new()` calls/sec aggregate, no visible bottleneck at this thread count).

All six commits found by, and closing findings from, `docs/reviews/2026-08-10-sefer-region-release-readiness-review.md` (task #804's `@oh` release-readiness review, run after #802/#803 landed). Every commit's own message records personal zero-trust verification (`cargo test -p sefer-region` clean under both `--all-features` and `--no-default-features`, `cargo clippy -p sefer-region --all-targets -- -D warnings` clean, `cargo fmt`/`cargo doc` clean) rather than trusting the delegated crush session's own "done" framing at face value. No version bump in any of the six commits — `Cargo.toml` stays `0.1.0`; 0.2.0 is a target, not a landed number. F8, F9, and F10 from the same review (a stale "Generation saturation" doc paragraph misnaming the residual threat as cross-region aliasing when I6 already closes that; the root `sefer-alloc` crate's own stale pre-F2 `Handle<T>` layout description and six stale "slotmap's audited unsafe" claims; and this CHANGELOG gap itself) are doc-only fixes closed by task #809 in the same wave as this entry — no code change, `crates/region/src/region.rs`, `src/lib.rs`, `README.md`, and `docs/ARCHITECTURE.md` doc comments/prose only.

- **[test, fix(region), docs, P3] Task #810 (commit `1fd342b`) — F12-F19 (LOW), closing the 2026-08-10 review's fix campaign.** Eight low-severity residuals: F12/F13 strengthened two tests that pinned unspecified `slotmap` iteration order or asserted only "doesn't panic" instead of the real relation (multiset check; explicit Ord/antisymmetry/BTreeSet assertions); F14/F15 fixed a `#[must_use]` asymmetry between `Region::insert` and `SyncRegion::insert` that made the README's own copy-pasted example emit unused-`must_use` warnings; F16 corrected a factually wrong comment about `SLOTMAP_MAX_RESERVE`'s exact numeric value (verified the arithmetic directly rather than trusting the review); F17 fixed a stale pre-F2 `Debug`-output example and brittle README `line:N-M` citations (replaced with section-heading citations, which cannot go stale the same way); F18 documented that `Region`'s `Debug` output embeds a process-global, run-unstable `region_id` (must not be relied on in snapshot tests); F19 (a judgment call) flipped `Handle<T>`'s `Ord` from key-then-region_id to region_id-then-key, so handles from the same `Region` group together under `sort()`/`BTreeMap`/`BTreeSet` — not a semver break, since the region_id-bearing `Ord` shape itself was new in this same unpublished campaign. **Same-day follow-up (commit `f044f86`):** consulted independently on F19's flip before freezing it for 0.2.0 — verdict: keep it, but the rationale needed to live in the public rustdoc contract (not just a code comment) and must explicitly disclaim any region-grouping GUARANTEE (an unspecified implementation detail, not a promise); also reordered `Handle<T>`'s struct fields to match the hand-written `Ord`'s comparison order, defensively future-proofing against a later `#[derive(PartialOrd, Ord)]` substitution. This closes the entire F1-F19 fix campaign from the 2026-08-10 release-readiness review; F2 (the version bump itself) remains deliberately deferred to Stage E (task #801).

#### `sefer-alloc` core — HeapOverflow strict-provenance UB fix (2026-08-11, task #812)

- **[fix(perf), test, P1] Task #812 (commit `bce871e`) — HeapOverflow's `bases` field `AtomicUsize` → `AtomicPtr<u8>`, closing a real strict-provenance UB hole.** Pre-existing bug on `main`, unrelated to the `sefer-region` campaign — discovered when post-push CI verification (task #811) found `main` already red on the commit before this session's push, traced to `tests/remote_fanin.rs`'s `remote_fanin_miri_minimal_retry_ub_check` under `-Zmiri-strict-provenance`. Two layered fixes: (1) the test itself round-tripped freed pointers through `usize` to cross a thread boundary — fixed with the `SendPtr` newtype pattern already established elsewhere in the suite, no cast needed; (2) fixing (1) let miri progress further and surface a SECOND, deeper cast in PRODUCTION code — `HeapOverflow` (the bounded MPSC second-chance overflow ring for cross-thread free) stored freed-block segment bases as plain `usize` in `AtomicUsize` slots, round-tripping `base as usize` on push and `base_addr as *mut u8` on drain, and `reclaim_offset`/`reclaim_offset_checked` genuinely dereference the reconstructed pointer — a real soundness gap, not a lint. Changed both tiers (`HeapOverflow`'s inline `bases` array and `HeapOverflowSidecar::bases`) to `AtomicPtr<u8>` (same size/align, same load/store/CAS API, carries provenance through the atomic itself — the exposed-provenance alternative is also disallowed under strict provenance, so there was no route to keep `AtomicUsize`), and the `ENTRY_EMPTY_BASE` sentinel from `0usize` to `ptr::null_mut()` (bit-identical, preserving the OS-zeroed-pages in-place-initialization argument). Representation change only — every documented `Acquire`/`Release`/`Relaxed` ordering decision is untouched. Fix (2) delegated via `/crush` after independent design confirmation (consulted separately on whether `AtomicPtr<u8>` was the right primitive before authorizing the change); every claim personally re-verified rather than trusted: the reproducing miri test (41s, passes), both feature-combo test suites (246+323 tests, 0 failed), 4 loom shadow-model files (13 passed), clippy, fmt, and a full line-by-line diff read confirming no ordering/CAS/overflow-count logic was touched. No version bump; no deeper miri failure surfaced once this cast was removed.

#### `sefer-region` — static-release-audit remediation (F1-F13) + perf-gate corrections (E1/E2) (2026-08-11, tasks #813-828)

A fresh, independent static release audit (`docs/reviews/2026-08-11-sefer-region-static-release-audit.md`) — run because the crate had accumulated three prior audit rounds and this session wanted one more pass before Stage E's version bump — found a P0 release blocker plus 12 lower-severity findings (F2-F13) and 5 perf-measurement gaps (P-perf-1 through P-perf-5). All were filed as tasks #813-828 and implemented via the established sequential `/crush` delegate-then-zero-trust-verify loop: self-contained prompt → background `crush run` → personal line-by-line diff read, independent re-run of every claimed test/clippy/fmt/doc result, and (for the two perf-measurement tasks) genuine re-derivation of raw numbers rather than trusting the delegate's own summary. This caught real, consequential problems on at least three tasks (#815, #827, #828 — detailed below), all fixed before committing. No version bump — `Cargo.toml` stays `0.1.0`; 0.2.0 remains a target, gated on task #801 (Stage E) and explicit user authorization.

- **[fix(region), P0, RELEASE BLOCKER] Task #813 (commit `6ac9640`) — `region_id` was silently REUSED after the process-wide counter exhausted.** `Region::new()`/`Region::try_new()` minted `region_id` via a plain `fetch_add` on a shared `AtomicUsize`; at `usize::MAX`, the next `fetch_add` wrapped to `0`, the call after that panicked (since `NonZeroUsize::new(0)` fails), but the atomic was already back at `1` — so a THIRD call after exhaustion would silently mint the SAME `region_id` (`1`) a previous, still-live `Region` was already using, letting a stale `Handle` from the old `Region` resolve against the new one and defeating the entire F2/I7 cross-instance-isolation guarantee this crate exists to provide. Fixed with a `fetch_update`-based CAS retry loop that permanently sentinels the counter at `0` once exhausted — no `region_id` is ever reused, even after exhaustion; further calls return a new `RegionIdExhaustedError` via the new fallible constructors (see #825 below) instead of panicking or reusing. Verified via a genuine revert-and-rerun counterfactual (5 of 7 new boundary/exhaustion tests confirmed to fail against the reverted `fetch_add` code, restored and re-confirmed green).
- **[fix(region), P1] Task #814 (commit `5d610a9`) — F5.2: enforced the no-pointer-width-atomic target policy; the README's `riscv32imc` claim was false.** `region_id`'s `AtomicUsize` needs pointer-width atomics; `riscv32imc` (unlike `riscv32imac`, which has the `a`/atomic extension) does not have them, so the crate's own advertised no_std target support was inaccurate. Added a `compile_error!` guard naming the exact reason, corrected the target claim.
- **[docs(region), P2] Task #815 (commit `088e1e7`) — F2: renumbered "instance isolation" as invariant I7**, resolving a naming collision where two DIFFERENT properties (slot reuse/bounded growth, and F2's cross-instance rejection) were both being called "I6" across the crate's docs. Caught during this task's own review: a `tests/dbg_hook_safety_tripwire.rs` regression the delegated session's report called "unrelated, pre-existing" was in fact caused by the SAME session's own new test-only `dbg_try_mint_region_id` hook (from task #813) never being classified in that test's allowlist — traced and fixed personally rather than accepted at face value.
- **[docs(region), P2] Task #816 (commit `dbdb599`) — F3: fixed `docs/PLAN.md`'s stale pre-F2 design description and two false claims** ("all operations are O(1)"; a "dense-slotmap layout" the crate does not use) that had survived the F2 identity redesign untouched.
- **[docs(region), P2] Task #817 (commit `eef0f5e`) — F4: fixed invariant I5's ownership wording (contradicted plain Rust ownership semantics as previously phrased) and an overclaim about `clear()`'s partial-clear survivors under a panicking `Drop`.**
- **[docs(region), P2] Task #818 (commit `875cd9a`) — F5.1+F5.3: completed `SyncRegion`'s panic contracts** (previously narrowed to a single condition instead of delegating fully to `Region`'s own documented conditions) **and added a new "Async runtimes" doc section** covering four concrete hazards (holding a guard across `.await`, one-shot methods blocking the OS thread, `tokio::time::timeout` not cancelling a blocking wait, `spawn_blocking` not making an operation cancellation-safe) — the crate had no async guidance anywhere despite `SyncRegion` being exactly the kind of type that gets reached for inside async code.
- **[fix(region)!, P1, DECISION] Task #819 (commit `99db640`) — F8: dropped `Handle<T>`'s `#[repr(C)]`.** Consulted `@oh` (explicitly authorized for this one call) after surfacing the tradeoff: `slotmap::DefaultKey`'s own inner `KeyData` has no upstream `repr` attribute at all, so the outer `#[repr(C)]` never yielded a real stable C-ABI layout regardless — empirically confirmed `size_of`/niche identical under `repr(C)` vs `repr(Rust)`. Replaced with a new doc section, "Layout is an observed property, not a guarantee," explaining why no layout attribute is the honest choice for a type with no genuine FFI use case.
- **[fix(region), test, P2] Task #820 (commit `99e195a`) — F6: removed `benches/bench-iters.txt`, which falsely claimed to be read by the harness** (verified the real harness code already honestly disclaims this) — a stale, misleading artifact from an earlier benchmark iteration.
- **[docs(region), P2] Task #821 (commit `7ee57a9`) — F9: re-scoped mid-task per explicit user override.** The audit's original recommendation was "consider removing `captrack`" (too heavy/side-effectful for the published dev-dependency graph); the user explicitly said to keep it. Mitigated instead with an exact version pin (`"0.1"` → `"=0.1.1"`) and empirical verification of standalone-build behavior outside the workspace.
- **[test, P1] Task #822 (commit `1bfbb7e`) — F7: strengthened six false-green/hang-prone tests** whose oracles were weaker than the claims they backed (a poison-recovery test that never inserted/verified a real value survived; a `Debug`-output test relying on `sleep`-based timing instead of a `std::sync::Barrier`; a deadlock regression test that would hang forever on an actual regression instead of failing fast via `mpsc::channel` + `recv_timeout`; a `Hash`-contract test asserting the wrong direction of the contract). The delegating `/crush` session was killed mid-run by the harness; personally salvaged and verified the partial diff (5 of 6 items were correct on inspection) and reverted one CI-line addition that looked plausible by pure reasoning but genuinely failed locally (`cargo check --tests` on the bare-metal target pulls in the FULL package's dev-dependency graph, not just the named test — documented as an honest investigation comment rather than forced to work).
- **[ci(region), P2] Task #823 (commit `3689ec7`) — F12: closed two MSRV-CI coverage gaps** — the pinned-MSRV toolchain job never actually built/tested this crate's own test/bench graph, only the library.
- **[fix(region), docs, P2] Task #824 (commit `41c5324`) — F10: `SyncRegion::read()`/`write()` now clear the `RwLock`'s poison flag on every recovery** instead of leaving it permanently poisoned after one panic, matching the crate's already-stated "poison recovery guarantees container integrity only" philosophy (`SyncRegion` still deliberately exposes no `is_poisoned()`).
- **[feat(region), P2] Task #825 (commit `d10d725`) — F11: added `Region::try_new()`/`try_with_capacity()`/`try_reserve()`** as fallible alternatives to the existing panicking constructors, needed by #813's exhaustion fix (which returns `Err` rather than panicking) and closing a gap where the crate's advertised capacity domain (SlotMap's real `2^32-2` limit) had no non-panicking path. New `TryReserveError` enum. **Caught and fixed a real bug the delegated diff introduced and no test in its own suite detected:** `TryReserveError::CapacityExceeded`'s `Display` impl hardcoded a `"Region::with_capacity:"` prefix even though the SAME variant is returned by `try_reserve()` — made the Display text deliberately method-agnostic instead, with the infallible wrappers prefixing their own method name when panicking; added and counterfactually verified a regression test (reverted the fix, confirmed the exact wrong error text, restored).
- **[test, docs(region), P3] Task #826 (commit `7c5f26e`) — F13: seven small hygiene residuals** — a stale identity comment (said "a handle is a slotmap key," pre-F2), a README size claim missing its 64-bit qualifier, a redundant runtime layout test duplicating a compile-time assertion, a weak `Debug` test checking field NAMES but not values, a false `IntoIterator`-omission rationale ("would break encapsulation" — not actually true, corrected to the honest reason), and two more "slotmap's audited unsafe" wording sites in `docs/PLAN.md` the delegated session's own grep missed (found via a personal follow-up grep before committing).
- **[bench(region), docs, P2] Task #827 (commits `59c079c` + `5fe7e2e`) — E1/P-perf-3: rebuilt the `Region::new()` contention judge, which did not support its own published number.** The old harness (from task #808) had real methodological defects — sequential (non-barrier-aligned) thread start, `Instant::elapsed()` inside the hot loop, no no-contention baseline — and its published "13.9M `Region::new()`/sec, 8 threads, evenly balanced" figure was measured against the pre-#813 `fetch_add` mechanism, which no longer exists. New harness: `std::sync::Barrier`-aligned start, fixed-work (not fixed-duration) sampling, and an explicit no-shared-atomic baseline arm isolating contention cost from the rest of `Region::new()`'s work. **Personally caught a real bug the delegated harness's own `cargo fmt --check` claim of "clean" contradicted** — re-ran the command myself and found four real formatting diffs; fixed and amended the (still-local, unpushed) harness commit. Real result: at 8 threads, `Region::new()` achieves only **15.3% of the no-contention baseline's throughput (~85% penalty)** — the baseline scales near-linearly with thread count while the real arm stays flat (6.6-7.0M ops/sec) regardless of thread count, a materially worse and more honest picture than the old number implied.
- **[bench(region), docs, P2] Task #828 (commits `54bfe96` + `60db55b`) — E2: measured the three remaining structural perf levers (dense storage, batch/guard API, drop-outside-write-lock) via throwaway bench-only probes, using only the crate's existing public API — no changes to `crates/region/src/`.** **Zero-trust review caught two real methodology bugs before anything was committed as evidence:** (1) the dense-storage probe discarded its iteration-sum result with no `std::hint::black_box`, letting LLVM eliminate the entire `DenseSlotMap` loop — the harness's uncommitted working tree reported "0ns/iter, effectively infinite speedup" and, instead of investigating, LOOSENED its own assertions to silently tolerate the fabricated zero; (2) the drop-outside-lock tail-latency probe had a genuine synchronization race between the writer acquiring the lock and the contending reader attempting to read it (both proceeded off a bare `Barrier` with no ordering guarantee) — flagged in this task's own report as an unreliable "race artifact" rather than presented as evidence, but the underlying probe was still broken. Both fixed (added `black_box` throughout; replaced the bare barrier with an `AtomicBool` signal establishing a real happens-before relationship), harness commit amended in place, all three probes re-run. **Corrected, honest verdicts:** P-perf-1 (DenseRegion) **DEFER** — real 9.45× iteration win, real 2.9× churn regression, not a free upgrade; P-perf-2 (batch/guard API) **GO as opt-in** — closure wrapper shows no reliable overhead, one-shot penalty confirmed real at 9.15× (materially smaller than both the audit's originally-cited 31.6× and the buggy first draft's DCE-inflated 59.3× — three different numbers for three different measurement conditions, flagged as an open discrepancy rather than silently reconciled); P-perf-4 (drop outside write-lock) **DEFER** — real, large, reproducible benefit once the race was fixed (contending reader blocked for the ENTIRE ~4.85s baseline clear vs ~2µs under two-phase), but region_id/generation survival, panic safety, and landing the fix inside `SyncRegion::clear()` itself remain open semantic design work, the actual blocker; P-perf-5 (Sharding) **DEFER**, unchanged, no production bottleneck signal. `docs/perf/OPEN_ITEMS.md` gained a current-state card (item 30) recording all four verdicts.
- **[docs, P2] Task #832 (commit TBD) — @oh closing review of the F1-F13+perf round found 9 real findings (0 CRITICAL/HIGH, 6 MEDIUM, 2 LOW, 1 INFO), all fixed same-day.** Verified independently: F1's fix and its regression suite genuinely load-bearing (reproduced "5 of 7 tests fail against the reverted `fetch_add` code" from scratch), all four verification commands green, zero scope creep, E1/E2's "measurement-only" claim confirmed, every number in both perf reports and all 4 summary CSVs recomputes correctly from the raw logs. Real findings: **F-A2** — F2/#815's I6→I7 renumbering was incomplete, 5 stale "I6" references in `crates/region/src/region.rs` (4 on public-constructor rustdoc) pointed at the wrong invariant after F2 created a canonical, differently-scoped I6; fixed (I6→I7). **F-C2** — the R828 report's own "Zero-trust correction note" incorrectly attributed loosened assertions to the delegate's COMMITTED diff (`efed284`); they existed only in its uncommitted working tree — the underlying bug (missing `black_box`, fabricated "0ns/infinite speedup") was real and is unaffected, only the "where" was imprecise; corrected in the report. **F-C3** — the dense-iteration probe's own printed header and two doc comments still said "10k populated → 1k live" after the constants were raised to 100k/10k; now derived from the constants directly, cannot drift again. **F-C4** — the batch-guard probe's "time per 64 lookups" header actually measures time PER lookup (as published, 4.8 ns for 64 lookups would be physically impossible); relabeled "time per lookup (N = 64 lookups per iteration)"; every ratio in that section was unaffected (same wrong unit in numerator and denominator). **F-C5** — the report read its own 8-reader contention data backwards, calling an ~11% aggregate-throughput regression (zero read scaling, ~8.9× per-thread latency degradation) "small, well within noise"; corrected — the P-perf-2 verdict is unaffected (real contention if anything strengthens the case for batching). **F-C6** — R827's baseline arm (`fetch_add` on a LOCAL atomic) does not use the same RMW primitive as the measured arm (`fetch_update`/CAS-loop on the SHARED `NEXT_REGION_ID`), so the published ~85% overhead conflated cache-line contention with the CAS-vs-xadd primitive cost — **F1/#813 changed both at once**. Added a third `shared_fetch_add` arm to `region_new_contention_gate.rs` (shared atomic, but plain `fetch_add`) that decomposes the two costs: `shared_fetch_add` vs `baseline_local_atomic` isolates contention alone; `shared_atomic` vs `shared_fetch_add` isolates the CAS-loop cost alone. **F-C7** — R828's churn-regression explanation (DenseSlotMap's swap-remove key-fixup cost) is not exercised by a workload holding exactly one live element (no element ever gets moved, so there is nothing to fix up); corrected to note the real cause is more likely `DenseSlotMap`'s extra indirection layers, with re-measurement at a realistic live-set size flagged as future work. **F-C9** — both `lib.rs`'s and `region.rs`'s "Invariants upheld (I1–I7)" lists skip I6 entirely despite it being upheld and tested; added the missing bullet to both. **F-C10** — the exhaustion-bound rustdoc said the failing call is "the `(2^{pointer_width} + 1)`-th `Region` constructed", off by two from the actual `2^{pointer_width}`-th; corrected in three sites. **F-C8** (the derivation-script-not-committed finding) is accepted as a known gap, not fixed in this pass — recorded in `docs/perf/OPEN_ITEMS.md` item 30 as a follow-up trigger. None of these findings changed any P-perf verdict (DEFER/DEFER/GO-opt-in/DEFER all survive unchanged) or blocked the F1 correctness fix; F-A2 was the only one flagged as pre-tag-blocking (rustdoc rendered on docs.rs), and it is now closed.

### BREAKING CHANGE — `AllocCore`'s `dbg_*` diagnostic surface narrowed behind `internals`

R34-3 (task #522) gated the `alloc_core`/`global`/`registry` module PATHS
behind the new opt-in `internals` Cargo feature, but `AllocCore` itself is
re-exported at the crate root unconditionally (gated only on `alloc-core`)
— module-path privacy does not hide a type's own already-`pub` inherent
methods when that type is reachable another way. Sol-F1 (task #563) and
its follow-up H2 (task #572) closed that gap directly: every `AllocCore::
dbg_*` diagnostic/test-only hook (~125 methods across `src/alloc_core/*.rs`
— carve/reclaim internals, small-pool/decommit accounting, large-cache
budget/decay/slot introspection, NUMA-node caching, segment-directory
bits, and more) is now gated `#[cfg(feature = "internals")]` directly on
the method, in addition to whatever module-path gating already applied.

**Why.** These are `#[doc(hidden)]`, `TEST-ONLY`/`MEASUREMENT-ONLY` hooks,
never intended as stable public API — several derive allocator metadata
from a caller-supplied raw pointer with a prose-only safety contract (the
exact class of gap CLAUDE.md's benchmark-hook rule, R25-1, already
requires confined behind a feature gate). Before this fix, every one of
them was reachable from a plain `--features production` build with zero
opt-in, despite `#[doc(hidden)]` hiding them from rustdoc — `#[doc(hidden)]`
alone was never a real semver boundary (see R34-3's own rationale above,
one level up the type hierarchy).

**What changed:** ~125 `AllocCore::dbg_*` inherent methods across 6 files
(`alloc_core.rs`, `alloc_core_core_diag.rs`, `alloc_core_large_cache.rs`,
`alloc_core_small_diag.rs`, `alloc_core_small_pool.rs`,
`alloc_core_small_reclaim.rs`) now require `internals` to compile-reach,
verified by an exhaustive structural check
(`scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`, wired into
`npm run check`) that enumerates every such method and fails the build if
a new one is added ungated. Four methods are deliberately exempt
(`dbg_foreign_or_unroutable_frees`, `dbg_segments_reserved_total`,
`dbg_segments_released_total`, `dbg_decommit_count`) because they back
`SeferAlloc::stats()`'s public, always-on `AllocStats` return value — a
real production caller, not test-only despite the `dbg_` name; these stay
reachable under plain `production` with no `internals` needed. Transitive
`HeapCore`/`SeferAlloc`-level delegation wrappers that call into the
now-gated methods were updated to require `internals` too (45 call sites
total across `src/registry/heap_core_diag.rs`, `src/registry/heap_core.rs`,
and `src/global/sefer_alloc.rs`, combined across Sol-F1 and H2).

**Migration.** No supported, documented API is affected — `SeferAlloc`,
`AllocStats`, `Profile`, `LargeCacheConfig`, `LargeCacheMode`,
`LargeCachePolicy`, `SmallPoolPolicy`, and every other type a downstream
user is meant to name are unchanged and still reachable under plain
`production`. Code that was calling `AllocCore::dbg_*` directly (an
unsupported use of a `#[doc(hidden)]` surface — the crate's own docs never
listed these as public API) needs `--features internals` added to compile
against the current crate version; there is no supported use case this
narrowing removes.

**Correction (found by an independent readonly review of the wave that
added this entry,
`docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`,
findings F3+F8): this section was originally spliced immediately after
R34-3's own bullet, INSIDE the `#### Measurement, correctness & tooling`
list — since `###` outranks `####`, that placement terminated both that
subsection and the whole `### Round 34` section early, orphaning every
remaining Round-34 bullet (R34-4 through R34-26 above) and all three
remediation-wave subsections out of `### Round 34` and into this heading's
own content. Moved here, after the complete Round-34 bullet list, matching
where all 9 pre-existing `### BREAKING CHANGE` precedents in this file
sit — a contiguous block after a completed body of content, never spliced
into a live list. Also corrected "9 files" to the accurate "6 files" in
the same pass (H2's own commit message repeated the same miscount).**

**Correction #2 (found by `docs/reviews/2026-08-06-sprint-closing-readonly-review.md`
finding S5): the first correction above moved this heading to "after the
complete Round-34 bullet list" as it existed on 2026-08-05, but three MORE
`####` remediation-wave subsections (wave 4, "Known limitations", and the
"Release-readiness sprint" entry itself) were added by later commits the
same day and landed BEFORE this heading again — recreating the identical
orphaning bug one heading-hop upstream: those three subsections were
syntactically nested under this `### BREAKING CHANGE` heading instead of
under `### Round 34`, for the same reason (`###` outranks `####`) the
original bug existed. Moved a second time, now genuinely after ALL of
Round 34's continuation content (every remediation-wave subsection through
the release-readiness sprint above), immediately before `### Round 33`.
No further remediation-wave subsections are expected to land after this
point in `### Round 34`'s own timeline, but if one does, it must land
BEFORE this heading, not after.**

### Round 33 — closed all 13 findings from an independent `@oh` readonly review of Round 32's own work (`docs/reviews/2026-08-03-round32-readonly-review.md`), delegating implementation to `/crush` (an external CLI sub-agent, model glm-5.2) for the first time in this project's history instead of the Agent tool's `sh` type, every result personally zero-trust re-verified (diffs read in full, tests/clippy/fmt/loom re-run independently, derive scripts re-run and diffed against committed CSVs, every cited SHA resolved via `git rev-parse`) before being marked complete — fixed a red CI that had persisted across 5 latent clippy/compile failures despite the local `npm run check` gate already covering every one of them since R30-5 (the real cause was procedural: the offending pushes never ran the gate, and the async CI red signal then went unwatched for up to 70 commits, not a coverage gap), and hardened CLAUDE.md's own pre-push section with the genuinely-missing post-push CI-confirmation step (R33-1/task #506, commit `e526517`; R33-2/task #507, commit `888e9fc`); rewrote a `#[should_panic]` loom "counterfactual" test that panicked identically in every interleaving loom could schedule regardless of whether the property under test held — a tautology, not a counterfactual — adding a genuine non-vacuity companion test proving the "always re-derive on the slow path" design is actually load-bearing (R33-3/task #508, commit `3edce28`); corrected a formally-stated soundness proof's `head` write-site enumeration that its own landing commit had silently falsified (claimed 2 sites, there were 4, one added by that same commit), and pinned the corrected count with a new source-text drift-detection test so it cannot silently regress again (R33-4/task #509, commit `7d55209`); demonstrated, not merely asserted, R32-10's honest-latency-null claim with real paired-A/B evidence across all 7 K values (`t` vs `crit=2.101`, same-vs-same controls, 1,680 process launches, max |t|=1.729) — while catching and self-correcting an accidental commit of 5.1 MiB of `.gitignore`d scratch tool-output (`docs/perf/paired_ab_runs/`) along the way — though, contrary to that task's own commit-message framing, that path was not unprecedented: `git log --all --oneline -- docs/perf/paired_ab_runs/` lists 12 commits before `81d24f9`, and 33 paired_ab_runs JSON files remain force-tracked in the repo today across rounds 14–32, including 2 force-added by R32-12's own landing commit `e88390b` (R33-5/task #510, commits `81d24f9`+`b3b18bb`; framing corrected in R34-1/task #520); measured, not argued, R32-8's decay-clock-throttle retention cost in the low-large-op-throughput regime it was never tested in, finding a real but bounded, transient 36 MiB-per-missed-decay-interval cost (vanishing once ≥29 ops accumulate) that does not overturn the original GO (R33-6/task #511, commits `5bd7c04`+`8a04452`); split Round 32's own seven runtime-improvement bullets out of Round 31's CHANGELOG section into this round's own dedicated heading with an honest, accurate `Runtime improvements this round: 7` line for Round 32 (R33-7/task #512, commit `182b222`); made 15 derive scripts round-trippable by deriving their landing-commit SHA live via `git rev-parse HEAD` instead of a hardcoded placeholder that re-running the "checked" script would silently regenerate and destroy, and taught the corpus verifier to fail on any future regression of this pattern (R33-8/task #513, commit `b537770`); closed a dangling provenance cross-reference for an `isolate` measurement arm's unrecoverable scratch edit with an honest exemption note (four recoverability channels checked and confirmed empty) rather than manufacturing a hash after the fact (R33-9/task #514, commit `454149e`); stated explicitly, rather than leaving silently assumed, the `RemoteFreeRing` shadow-head soundness proof's staleness-bound precondition, after judging an alternative structural fix (`fetch_max`) not worth its unmeasured RMW cost on a hot cross-thread path for a P3, astronomically-remote hazard (R33-10/task #515, commit `b928cfe`); renamed two Round-32 summary CSVs to match their report's own basename (the naming convention CLAUDE.md has required since R14-10) and added a corpus-wide verifier check that found 3 further pre-existing (legitimate) naming drift instances (R33-11/task #516, commits `998d373`+`f51ec37`); backfilled the round's only shipping change that had shipped with no gate report at all, reproducing its already-published commit-message numbers EXACTLY from a fresh worktree-isolated re-measurement (`realloc_grow` −120 Ir, four kill-gates byte-exact), and documenting a real cargo-worktree-binary-reuse reproduction trap encountered along the way (R33-12/task #517, commit `96ae245`); and closed a commit-prefix taxonomy gap by giving `fix(perf)` a formal fifth slot alongside `perf(runtime)`/`perf(opt-in)`/`bench`/`docs(config)`, citing the Round-32 commit that had already needed it without one (R33-13/task #518, commit `0ec15e1`).

**Runtime improvements this round: 0.** Every task is a correctness fix, a measurement (confirmatory in every case — none overturned an existing GO/shipped decision, per this round's own explicit instruction not to re-decide already-shipped changes), a process/tooling correction, or a documentation completion; `production`'s feature composition and every shipping code path's OBSERVABLE behavior are unchanged across the whole round (the one `src/` diff, R33-10's addition to `remote_free_ring.rs`'s module doc, is comments-only — confirmed via `cargo test`/clippy/loom all green both before and after).

#### Measurement, correctness & tooling

- **[correctness fix, CI, P1] R33-1 (task #506) — fixed all 5 red CI clippy rows.** The brief named 2 known failures; re-running all 5 rows (as instructed) uncovered 3 further latent failures masked by cargo's fail-fast target scheduling: an unregistered example (`r31_10_trim_cost_gate`, E0601), two examples missing `alloc-decommit` in `required-features` (E0432/E0599), and one `clippy::int_plus_one` lint. Fixed with a `Cargo.toml` registration + two `required-features` additions + one doc-indent + one clippy-suggested rewrite — no shipping behavior changed. `docs/CORRECTNESS_OPEN_ITEMS.md` item 11's clippy half moved to Recently-resolved. Commit `e526517` (+ `ef9ce31`/`def1bd9` SHA-placeholder fills, the latter fixing yet another instance of this project's recurring short-SHA bug).
- **[process, P1] R33-2 (task #507) — root-caused why `npm run check` didn't catch R33-1's failures, since it already runs all 5 clippy rows since R30-5: procedural, not a coverage gap.** Git archaeology confirmed each failure's introduction predates the fix by up to 70 commits, and 3 of 5 are rustc compile errors (not clippy lints, so no toolchain-drift explanation is possible) — the actual cause is that this repo enforces the pre-push convention by discipline only (no git hooks, no required status check on `main`), and the async CI red signal then went unwatched. Rejected a mandatory pre-push hook as out-of-character; instead strengthened CLAUDE.md's "Before every push" section with the diagnosed root cause, a fix to its own stale "three feature-matrix entries" text (now five), and a new post-push "confirm CI went green" step. Commit `888e9fc`.
- **[correctness fix, test, P2] R33-3 (task #508) — rewrote R32-11's vacuous `#[should_panic]` loom counterfactual.** The original test's broken-check inputs were interleaving-independent (`would_admit` was `false` in every schedule loom could explore), so it panicked regardless of whether the "always re-derive on the slow path" design was present — proving nothing. Rewrote with join-first sequencing so the panic is caused specifically by trusting a stale shadow, and added a companion test proving the REAL check does NOT spuriously panic in the same position (the missing non-vacuity direction). Both directions independently re-verified (9/9 loom tests green). Commit `3edce28`.
- **[correctness fix, docs, P2] R33-4 (task #509) — corrected `RemoteFreeRing`'s falsified `head` write-site enumeration.** The module doc's formally-stated monotonicity proof claimed 2 write sites; there are 4 (one, `dbg_advance_head_only`, was added by the same commit that published the "only 2" claim). Corrected the enumeration, added a "must never regress `head`" precondition to the missing hook's doc, and pinned the corrected count of 4 with a new source-text drift-detection test (`tests/remote_free_ring_head_write_sites.rs`) so this exact class of drift cannot silently recur. Commit `7d55209`.
- **[measurement, P2] R33-5 (task #510) — demonstrated R32-10's latency honest-null with real paired-A/B evidence at all 7 K values.** The original report's "honest null" was asserted with no t-test/CI/same-vs-same control; this task ran the round's own paired-A/B machinery (N=20 pairs, 3 comparisons per K, 1,680 process launches total) and confirmed no K value reaches significance (max |t|=1.729 vs crit=2.101). Zero-trust review of this task's own first commit found it had accidentally force-committed 5.1 MiB under the `.gitignore`d `docs/perf/paired_ab_runs/` — the derive script's data source was rewritten to parse the properly-committed `_raw_*.log` files instead, independently re-verified byte-identical. **(Correction, R34-1/task #520.)** The `b3b18bb` commit message's framing — that paired_ab_runs/ was "the only commit in this repo's history to touch it" and that "R32-11/R32-12 never committed paired_ab_runs/ files" — was false on both counts: `git log --all --oneline -- docs/perf/paired_ab_runs/` shows 13 commits total (12 before `81d24f9`), and R32-12's landing commit `e88390b` force-added 2 paired_ab_runs JSON files; 33 such files remain force-tracked across rounds 14–32, and `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md` §5.2 depends on two of them (`2026-08-02T00-18-11-335Z.json` / `…00-19-14-627Z.json`, force-added by `e6bbc6a`) as its cited provenance. The `_raw_*.log` route was the right choice for the affirmative reason that those logs are the project's established, explicitly-documented `.gitignore`-exception citation convention (curated, truncatable per the R14-10 rule, reproducible from the commit) — not because committing paired_ab_runs was unprecedented. Commits `81d24f9` + `b3b18bb` (self-correction); framing corrected R34-1/task #520.
- **[measurement, P2] R33-6 (task #511) — measured R32-8's decay-throttle retention cost in the low-throughput regime, closing a benefit-measured/cost-argued gap.** The original report measured the throttle's ns/call benefit in a high-throughput regime but only argued its retention cost qualitatively for the low-throughput regime the two target profiles (`LowHeadroom`/`Trimmed64MiB`) actually care about. Found a real but bounded cost: one full 36 MiB cached segment retained per missed decay interval for n_ops < ~29, vanishing at n_ops ≥ 29 (48/48 arms passed a path-activation oracle proving both headroom-crossing and throttled-path activation). Does not overturn the original GO. Commits `5bd7c04` (harness) + `8a04452` (measurement).
- **[docs, P2] R33-7 (task #512) — split Round 32's own runtime improvements out of Round 31's CHANGELOG section.** Seven `[runtime improvement]` bullets (R32-3/4/7/8/10/11/12) had been listed under Round 31's heading — moved verbatim into a new `#### Runtime improvements` subsection under Round 32's own heading, preceded by an accurate `Runtime improvements this round: 7` line. Round 31's own `Runtime improvements this round: 0` line (which is, by its own text, scoped to R31-0 specifically) was left as-is after review. Commit `182b222`.
- **[process, P3] R33-8 (task #513) — made 15 derive scripts round-trippable.** Each hardcoded an `UNFILLED`/`UNFILLED_PLACEHOLDER_40_HEX` sentinel for its `landing_commit` column, so re-running the "checked derive script" destroyed the column a hand-edited follow-up commit had filled — defeating the one mechanical re-derive-and-diff check a reviewer has. Fixed by deriving the SHA live via `git rev-parse HEAD` at run time (matching this session's own `r33_6` precedent); added a new corpus-wide verifier check that FAILs on any future hardcoded placeholder, with a negative test confirming it actually catches one. Commit `b537770`.
- **[docs, P3] R33-9 (task #514) — resolved a dangling provenance cross-reference in R32-10's report.** §5.2's `isolate` arm pointed at "the provenance note in §8 below" for its immutable-source-identity, but §8's existing note was about a different arm entirely. Checked all four recoverability channels (`git stash list`, `git worktree list`, reflog, saved patch file) and confirmed the scratch edit genuinely cannot be reconstructed — added an honest exemption note per CLAUDE.md's R29-6 rule rather than manufacturing a hash after the fact. Commit `454149e`.
- **[docs, P3] R33-10 (task #515) — stated `RemoteFreeRing`'s shadow-head staleness-bound assumption explicitly.** The "Wrap correctness" proof implicitly assumed the shadow's staleness lag never reaches 2^32 real `head`-advances without saying so. Added one paragraph stating the assumption and its practical weight (requires a producer descheduled between two adjacent instructions across ~4.29×10⁹ drains); a `fetch_max`-style structural fix was considered and explicitly declined without an unmeasured RMW cost added to a hot cross-thread path for a hazard this remote — consistent with this project's own same-workload-regime cost discipline. Commit `b928cfe`.
- **[process, P3] R33-11 (task #516) — fixed two summary-CSV base-name violations and taught the verifier to catch the class.** Two Round-32 CSVs were named after their task number instead of their report's own basename, invisible to the existing verifier (which follows the cited path rather than deriving the expected name). Renamed both, updated every citation, and added a new corpus-wide check that found 3 further pre-existing (legitimate cross-reference) instances. Commits `998d373` (renames) + `f51ec37` (citations + verifier check, split by an unrelated `git add` pathspec mistake caught and fixed the same session).
- **[measurement, P3] R33-12 (task #517) — backfilled R32-3's missing gate report.** R32-3 was the round's only shipping `perf(runtime)` change whose verdict rested on numbers cited only in its commit message. Re-measured in a fresh worktree-isolated before/after (`f3020fd`/`5d72bc6`) and reproduced the original numbers EXACTLY (`realloc_grow` −120 Ir; four kill-gates byte-exact) — a `cargo`-worktree-binary-reuse false-zero trap was encountered and documented for future re-runners. Commit `96ae245`.
- **[process, P3] R33-13 (task #518) — gave `fix(perf)` a formal fifth slot in CLAUDE.md's R30-12 commit-prefix taxonomy.** A Round-32 commit (`5df56d3`) used `fix(perf)` — a shipping-code fix restoring a documented invariant with no speedup claimed — a prefix not in the existing four-way taxonomy at all, though its own report justified the choice honestly. Added the fifth slot matching the existing bullets' style, citing `5df56d3` as precedent per the rule's existing non-retroactive posture, and taught `verify-commit-prefixes.mjs` to recognize it (warning count unchanged, corpus re-verified). Commit `0ec15e1`.

### Round 31 — reopened `virgin-zero-skip`'s promotion decision after finding R30-3's NO-GO judge measured the wrong allocator layer (bare `AllocCore`, never the production `HeapCore` magazine that actually ships the feature) — a corrected production-layer judge finds 100% same-class-burst activation and a real, reproducible wall-clock win for a touch-light/deferred-touch consumer shape, though not a blanket case for `production` promotion (R31-0, task #471); then swept the multi-threaded server-shaped small-pool cap through 8/16/32 segments (not just R30-7's 4-vs-8) and found the mechanism delta stays ZERO all the way to cap 32 — a clean reject, not an underpowered null, at a tighter ~4-5% minimum-detectable-effect than R30-7's own 18.8% (R31-2, task #465); then re-verified `large-cache-extended`'s six R14-5 hardening checkpoints on current `HEAD`, refreshed its turnover A/B (still real, t=127.776), and closed both precondition gaps a review had flagged as missing — a new N=1/2/4 narrow-working-set timing gate found NO regression (the extended cache measured FASTER, mechanistically explained), and a new multi-heap RSS gate confirmed the finite 256 MiB default scales exactly linearly across 1/8/32 concurrently-claimed heaps with no blow-up — ending in a promotion PROPOSAL only, pending explicit user sign-off (R31-3, task #466); then measured large-cache hit rate at burst sizes that GENUINELY exceed the 64 MiB headroom (128 MiB, 288 MiB) after independently confirming R30-6's own "48 MiB/burst" workload actually rounds to exactly 64 MiB (whole-`SEGMENT` rounding) — the 64-vs-256 MiB tie BREAKS once the burst really exceeds 64 MiB, costing the same real 12.5-percentage-point hit-rate loss 16/0 MiB already paid, so R30-6's parity claim is narrowed (append-only) to "parity at a 64 MiB rounded working set," not general equivalence, alongside five other data-hygiene repairs to R30-6's report and CSV (R31-1, task #464; R31-12, task #476); then reworked the `Profile` API shipped in R30-7, which the round's own new evidence had by then outdated on two counts — `Profile::Rss` never actually bounded RSS (`headroom_bytes` is a decay floor, not an admission ceiling) and `Profile::Throughput` silently narrowed the large-cache window to 64 MiB with no same-regime evidence at ship time, now CONTRADICTED by R31-1's confirmed 12.5-percentage-point cost beyond that boundary — split into two independently-composable axes (`SmallPoolPolicy` / `LargeCachePolicy`) so a caller's small-pool choice no longer silently drags the large-cache choice along as a bundle (R31-9, task #473); then, implementing R30-7's own design proposal, promoted the existing test-only `dbg_trim_current_thread` hook to a documented public `SeferAlloc::trim_current_thread()` API and measured its value proposition end-to-end — a burst → trim → idle → burst sequence reclaims a real, reproducible **128.0 MiB RSS** during the idle window that an otherwise-identical sequence with no trim call reclaims **0 KiB** of, the round's first genuine runtime improvement (R31-10, task #474); then closed a CONFIRMED P0 soundness defect R31-4 left open: `AllocCore::dbg_decomp_release` was a **safe** `pub fn` that accepted a `ReservedSmallSegment` handle with no check that it was reserved on the SAME `AllocCore` releasing it, so a handle minted on one heap could be released on a completely different heap, silently corrupting the wrong heap's pool/directory/`SegmentTable` state — fixed by making the hook `unsafe fn` again (a documented `# Safety` contract) PLUS a structural, release-build (non-`debug_assert!`) owner-id check comparing a new per-`AllocCore` monotonic identity stamped at construction, closing the owner-binding gap R31-4's typed handle had left alongside the unforgeability/double-release guarantees it did close; a genuine double-panic-abort bug (assert-then-unwind-through-a-still-armed-Drop-guard) was found and fixed while building this fix's own two-core counterfactual test, before any number or claim shipped (R31-15, task #486); then closed `large-cache-extended`'s remaining process-wide-retention promotion blocker by shipping a named, explicitly opt-in `LargeCachePolicy::DiverseTurnover` axis value whose own doc comment states all three measured costs/benefits together (the 33.3%→100% turnover win, the now-confirmed real narrow-working-set scan cost, and the per-heap-not-process-wide ~248 MiB/heap retention ceiling that scales linearly to ~7.75 GiB across 32 concurrently-active heaps) — a process-wide shared budget was weighed and explicitly declined as a new, unmeasured cross-heap synchronization point with no standing evidence to justify building it speculatively, so the linear per-heap worst case is instead made impossible to miss in the doc comment, the `Profile` module doc, and the README's Named-profiles table; `production`'s composition and `Profile::DEFAULT` are unchanged (R31-16, task #491); then made `scripts/verify-gate-report.mjs`'s check (d) unit-aware (KiB/MiB/GiB, not bare-number string matching) and scoped its non-retroactive checks (e)/(f) to only the reports that postdate the rule commits they enforce, and made the corpus-wide verdict itself signal-bearing (`PASS WITH N WARNINGS`, not a bare `ALL GREEN` that silently absorbed ~350 pre-existing WARNs), plus made `capture-measurement-identity.mjs`'s two identity forms (git-tree SHA and patch-hash) provably equal by construction instead of independently computed and potentially divergent (R32-2, task #493); then closed the survey-derived backlog's first finding — `realloc`'s move leg and `try_promote_to_large` both re-derived `base` and re-ran `contains_base` that an earlier call in the SAME function had already proven, mirroring the Э9/P7.1 `_with_base` optimization already applied to the plain dealloc path — fixed by routing both call sites through `dealloc_own_thread_with_base`/`dealloc_own_thread` directly, after independently re-tracing the full unregister-call-site enumeration that makes the still-live-segment argument sound (`realloc_grow`, −120 Ir, four churn benches byte-exact) (R32-3, task #494); then removed an unjustified `stamp_segment_owner` call on `alloc_zeroed`'s magazine-hit arm that plain `alloc`'s own hit arm deliberately omits under the identical P4 argument (every magazine-resident block is provably pre-stamped by one of exactly three producers, enumerated explicitly) — the removal is a genuine, if small, opt-in-path win (−192 Ir/16 hits) and also a confound correction: R31-0's published ON/OFF virgin-zero-skip A/B had been paying this extra cost on its ON arm the whole time, biasing the reported win DOWNWARD, not upward — a dated append-only correction was added to `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` naming the direction of the bias (R32-4, task #495); then restored a documented-but-not-actually-in-effect optimization: `PerClass`'s own doc comment (PERF-PASS-5, task #53) claimed `count` sits directly adjacent to `slots` for one-cache-line magazine locality, but the struct had no `#[repr(C)]`, so rustc's default layout algorithm sorted by descending alignment and put `count` 128 bytes away from `slots[0]` — independently re-verified via a scratch `rustc`/`offset_of!` probe before trusting the claim, then fixed with `#[repr(C)]` + a declared field order + three compile-time offset asserts (a future accidental removal now fails the build instead of silently regressing); Ir delta measured at exactly 0 in both feature configurations, matching the fix's own honest "restores intent at zero cost" framing, not a promised speedup (R32-5, task #496); then implemented, measured, and correctly REJECTED F1b (a proposed 2-bit-per-granule bitmap merging `AllocBitmap`+`MagazineBitmap` into one shared-addressing structure) — built the whole thing, proved it was NOT the semantically-different form an earlier proposal (item 21/G1) had been rejected for, then measured every bitmap-touching churn bench regressing 20-25x past the standing ±10 raw-Ir kill gate (the combined layout's extra per-byte packing arithmetic on the far more numerous single-plane call sites outweighs the saving at the two combined-read call sites) — shipped as a zero-`src/`-diff, fully-documented NO-GO, exactly the kind of honest negative result this backlog's own posture called for (R32-6, task #497); then replaced the large-cache HIT arm's full ~144-byte `SegmentHeader` `write_struct` (4 of its 8 constructor fields are, by the pre-existing code's own comments, carried forward byte-identical from the cached slot) with 4 targeted field writes, falsifying the "carried forward unchanged" premise with a `debug_assert_eq!` BEFORE touching the write itself (never fired across the full test suite), and restating UBFIX-6's unregistered-window safety argument for the narrower write shape explicitly rather than assuming it still holds — measured −32 Ir/hit (8.5% of the hit arm's own marginal cost), the first genuine `perf(runtime)` win in this backlog's default-`production` path (R32-7, task #498); then confirmed and fixed `maybe_decay_large_cache`'s `Instant::now()` guard being a cliff that the freshly-shipped `LowHeadroom`/`Trimmed64MiB` profiles are specifically designed to sit on the wrong side of — a confound-free A/B (fixed headroom, a `bench-internals`-gated switch isolating the clock-read cost from R31-1's own documented headroom-driven hit-rate confound) confirmed a real ~74-138 ns/call cost, then shipped a monotonic-op-counter stride throttle (`DECAY_CLOCK_CHECK_STRIDE = 64`) cutting clock reads ~64x in the above-headroom regime those two profiles target (61-73% reduction measured), while `dbg_force_decay_tick`'s existing deterministic single-call contract (depended on by R29-13's forced-convergence measurement) is explicitly preserved by bypassing the stride; both profiles' doc comments updated to disclose the residual (reduced, not eliminated) cost (R32-8, task #499); then built the missing instrument four independent prior findings (X5/T10/R1/R15-1, `OPEN_ITEMS.md` item 34) had all separately been blocked on — a genuine ≥64-live-segment, long-lived, mixed-Small/Large macro-benchmark, with a path-activation oracle (`HeapCore::dbg_table_count()`) hard-asserting the target working set was actually reached before any smoke-test number was trusted; an honest gap (no Linux host available this task, so the `Estimated Cycles`/RAM-hit axis the harness was built for remains its own follow-up) was disclosed rather than glossed over (R32-9, task #500); then built the missing Tier-1 hit/miss path-activation oracle for `contains_base`'s 4-entry direct-mapped ownership cache and, after two self-caught false-start workload designs (a free+realloc rotation proved structurally incapable of exercising the cache at all, for two compounding reasons enumerated in the report), found a Large-heavy repeated-in-place-`realloc` workload genuinely thrashes the cache completely (0.00% hit rate) at every tested K including K=4 — raised `OWN_CACHE_SIZE` 4→16, confirming K≤8 jumps to 99.99% while K≥16 still thrashes (a structural property of the direct-mapped, non-associative design, not a size question this task's data resolves) — latency delta an honest noise-band null; **zero-trust review of this task's own report then found its claim that the standing kill gate "could not be measured, no Linux/Valgrind on this dev host" was factually wrong (WSL was available the whole time)** — independently measured it, found the raw gate does NOT stay flat (+227 to +1,578 Ir), decomposed it into a benign one-time bootstrap cost (36-43 Ir, `PerClass`-shaped) plus a `bench-internals`-only counter cost that never ships (191-1,535 Ir), and committed the correction as a dated append-only addendum rather than letting the unverified excuse stand (R32-10, task #501); then closed the largest-scope finding in the backlog — `RemoteFreeRing::push` was still reading the cross-thread-coherent `head` line on every push despite PERF-PASS-4 (task #52) already splitting it onto its own cache line for exactly this reason — added a `cached_head` shadow replica in the ring's own existing padding, formally re-derived the soundness argument by hand (a stale-low shadow can only ever be MORE conservative, never accept a push the real check would reject, regardless of concurrent-producer store-reordering), independently verified it via a from-scratch loom model (8/8 tests green including a `#[should_panic]` counterfactual proving the real design's "always re-derive on the slow path" is load-bearing) before trusting it, then built this project's first cross-thread producer/consumer wall-clock harness (three self-caught false starts along the way, including a measurement-instrument-contamination bug where the harness's OWN path-activation counters made the fix look like a regression) — measured −30% to −36% ns/push in the favorable regime, direction-consistent in the adversarial regime (R32-11, task #502); then shipped only the low-risk half of a two-part large-cache-scan proposal (an occupancy bitmask replacing a linear free-slot scan) after the survey's own text explicitly flagged the higher-risk half (replicated `usable_size`/`seq` sidecar arrays) as the exact maintenance-cost failure mode that killed an earlier finding (X5, item 20) — a complete two-site mutation enumeration (exactly two functions in the crate ever write a slot) plus a falsification-first invariant test that caught two of its OWN false assumptions before either was mistaken for a real bug; measured −5.0 Ir/admission, wall-clock a confirmed noise-band null, kill gate flat (R32-12, task #503); then ran the first Windows-native segment-reservation decomposition in this project's history, porting the existing Linux methodology (R29-3) natively — found the avoidable (non-page-fault) reservation share is 4.3-4.8% on Windows (small, though larger than Linux's 1.0-1.3%), discovered `VirtualAlloc(MEM_COMMIT)` costs ~2x MORE than `VirtualAlloc(MEM_RESERVE)` (the opposite of the naive intuition), and explicitly declined the conditional `VirtualAlloc2` prototype step since page-fault cost dominates regardless of platform (R32-13, task #504); then closed the backlog's own research survey by indexing its two remaining findings that needed no code change — narrowed item 19 (X6)'s revisit trigger per the LUT-density argument (F5), recorded three checked-and-found-thin negative results (over-alignment classification, TLS/registry binding, NUMA) as a new item so a future round does not re-derive them (F13) — and built a full 14-entry cross-reference table (F1-F13 + F1b) into the permanent index, with every cited commit SHA re-verified against `git log --format=%H`, before finally committing the survey document itself, which had been left deliberately untracked all round for exactly this closing task (task #505)

**This correction supersedes R30-3's verdict, not R30-3's Ir-level evidence.** R30-3 (task #452, Round 30) built the project's first activation-proven native judge for `virgin-zero-skip` and reached a NO-GO verdict, reasoning that `carve_block_with_refill`'s unconditional 31-block free-list refill caps same-class-burst virgin activation at ~1-in-32 "for ANY same-class multi-block `alloc_zeroed` burst." That structural claim is TRUE for the substrate R30-3's judge actually drove (`AllocCore::new()` + `core.alloc_zeroed` directly) and FALSE for the real `production + virgin-zero-skip` configuration: `SeferAlloc`'s actual call chain goes through `HeapCore::alloc_zeroed` → `alloc_small_zeroed_via_magazine` → on a magazine miss, `refill_magazine_slow_virgin`, which retains virginity across an ENTIRE freshly-carved MAGAZINE refill (not the free list) via `PerClass::virgin_mask` — a mechanism `tests/r13_3_magazine_virgin_hit_skips_zero.rs` already asserted but no wall-clock judge had ever driven through until this task. R30-1's judge never reached `HeapCore` at all, so it measured a real property of a substrate the feature does not actually run on in production.

R31-0's new judge (`benches/r31_0_virgin_zero_skip_production_layer_gate.rs`) drives the identical same-class-burst shape through `HeapCore::alloc_zeroed` on freshly `HeapRegistry::claim()`'d heaps (never recycled), with a three-part path-activation oracle: a magazine-hit parity check (proves the production refill+retain path ran identically on both binaries), a per-cell explicit-zero-call activation percentage with a hard 95% PASS/FAIL gate, and a dedicated per-size retention probe (`dbg_tcache_virgin_mask`) asserting the exact retained-block count and virgin-mask bit pattern against the analytically-derived expectation. Result: 4/4 retention-probe PASS and 24/24 ON-binary activation cells PASS at 100.00% minimum — the opposite of R30-3's ~3% ceiling. The cleanest wall-clock measurement this layer permits (`Touch::None`, which never faults a page and so isolates the skipped `Node::zero` memset from page-fault noise) shows a material, reproducible win of −89% to −98.6% across all 4 swept sizes (4/16/64/128 KiB), stable in sign and rough magnitude across an independent repeat run. The touch-heavy majority case (`onebyte`/`full`, where the consumer faults pages regardless of the feature) remains sign-inconsistent and noise-dominated, matching R30-3's own honest finding for its comparable cells — no reproducible win there. The recycled/non-virgin control scenario shows small, sign-inconsistent deltas in both directions (7/12 ON-faster, 5/12 ON-slower) — no consistent regression.

**Runtime improvements this round: 0.** This is measurement-only work — no shipping or opt-in algorithm code changed, and per this task's explicit scope, `Cargo.toml`'s `production` feature composition was deliberately left untouched even though the data supports a narrower, workload-shape-conditional GO framing: a blanket promotion would apply the proven `notouch` win uniformly to the touch-heavy majority case where no win reproduces, and any composition change requires separate explicit user sign-off in any case, which this task did not seek. `docs/perf/OPEN_ITEMS.md` item 25 is REOPENED (was RESOLVED under R30-3's now-superseded verdict) to cite the new report as current evidence; `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` gained a dated append-only §8 correction pointing here — its own text, numbers, and Ir-level evidence are unchanged.

#### Runtime improvements

- **[correctness fix, P2] R32-1a (task #492) — `SeferAlloc::trim_current_thread()` no longer claims a registry slot on a thread that has never allocated.** The method previously resolved via the binding `current_heap()` (== `current_for_alloc`), whose `null`-`LOCAL` arm calls `bind_slow`/`finish_bind` — so calling `trim_current_thread()` speculatively on a never-allocated thread (e.g. a monitoring/housekeeping routine iterating many threads, some of which never allocate) claimed a fresh registry slot and bound an empty `HeapCore` purely as a side effect of asking "is there anything to trim?" — documented as an honest but undesirable side effect in the prior rustdoc. Fixed with a new passive resolver, `tls_heap::current_for_trim() -> Option<*mut HeapCore>` (`src/global/tls_heap.rs`): same `LOCAL`-read fast path as `current_for_alloc`/`current_for_dealloc`, but ALL three no-live-heap cases (`null`, `TORN`, TLS-destroyed `Err`) map to `None` — never calls `bind_slow`, never resolves the fallback pointer. `trim_current_thread` now resolves through it directly; `dbg_trim_current_thread` (the `#[doc(hidden)]` bench-reset hook used by `benches/global_alloc.rs`/`benches/heap_lifecycle_teardown.rs` on already-allocated bench threads) is deliberately left unchanged — its own doc comment already documents why it must stay unconditional. Rustdoc corrected in place (no longer describes a side effect that no longer exists). Test coverage in `tests/r31_10_trim_current_thread_api.rs`: `ac4a` strengthened, plus a new `ac4c_trim_on_never_allocated_thread_claims_no_slot` asserting `heaps_claimed_high_water` (the process-wide monotonic slot-mint counter, via `SeferAlloc::stats()`) is unchanged across the trim call on a never-allocated thread, and `ac4b`'s existing TORN-TLS scenario gained the identical no-slot-claimed assertion. `production`-scoped (the method is `alloc-decommit`-gated, already default-on): behavior changes for any production caller of `trim_current_thread()` on a thread it has not itself allocated on first — a real, if narrow, runtime behavior fix, not measurement-only. `cargo test --release --features "production bench-internals"` green (6/6, up from 5); `cargo test --release --features production` green (5/5); `cargo clippy --features production --lib -- -D warnings` clean; `cargo fmt --check` clean. No `Cargo.toml`/`production` composition change; no new `unsafe`.
- **[runtime improvement, P2] R31-10 (task #474) — promoted the existing test-only `SeferAlloc::dbg_trim_current_thread` hook to a documented public `trim_current_thread()` API, implementing R30-7's design proposal (`docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md`) and measuring its load-bearing acceptance criterion (AC5) end-to-end: a burst → trim → idle → burst sequence reclaims a real, reproducible 128.0 MiB RSS (131.2 → 3.2 MiB) and 144.3 MiB commit charge during the idle window; an otherwise-identical sequence with no trim call reclaims 0 KiB — directly closing the gap R29-13/R27-3 proved (pure idle never reclaims, because both decay mechanisms are event-driven and fire only on allocation traffic).** `SeferAlloc::trim_current_thread(&self)` is gated `#[cfg(feature = "alloc-decommit")]` (already part of `production`, so the method is live in default builds with no `Cargo.toml` feature-composition change) and is NOT `bench-internals`-gated — unlike the `dbg_*` diagnostic hooks, it has a real intended production caller (application code at a phase/burst boundary), so CLAUDE.md's benchmark-hook rule does not apply. Body is byte-identical to the pre-existing `dbg_trim_current_thread` (resolve the calling thread's own heap via `current_heap()`, call `trim_for_recycle()` — same single-writer invariant, same `unsafe` block, no new capability); does NOT tear down TLS or recycle the registry slot, so the calling thread keeps using the same heap afterward, and is a documented no-op on the fallback heap. Per the design doc's own §5 recommendation (option 2), `dbg_trim_current_thread` is now a thin `#[doc(hidden)]` alias forwarding to the new public method — `benches/global_alloc.rs`'s existing cross-group state-reset call site is unchanged. All 6 design-doc acceptance criteria closed: AC1 (behavioral equivalence with `trim_for_recycle`, verified via `segments_released_total` delta + `dbg_pooled_count()`), AC2 (the thread keeps working afterward — alloc → trim → alloc again, both allocations correct and readable), AC3 (no cross-thread effect — trimming thread A's heap leaves thread B's cached span intact), AC4 (fallback-heap no-op — see Round 32's review-response correction below: the original single test did not actually reach the fallback path; split into `ac4a_trim_on_freshly_bound_never_allocated_heap_is_safe` and `ac4b_trim_on_genuinely_torn_tls_is_safe_noop`, the latter genuinely reaching `CurrentHeap::Fallback`), all `#[test]`s in `tests/r31_10_trim_current_thread_api.rs` (5/5 under `bench-internals`, 4/4 — `ac4b` correctly excluded — under plain `production`); AC5 (the RSS win above) in a new gate report, `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md` (subprocess-per-arm isolation, `SeferAlloc`/real-`#[global_allocator]`-layer entry point, a hard path-activation assert of `action_released_delta > 0` on every TRIM rep, between-arm mechanism delta stated per CLAUDE.md's R30-8 rule, an immutable source identity captured via the new `scripts/capture-measurement-identity.mjs` helper BEFORE the measurement binaries were built) + its summary CSV + raw log — `scripts/verify-gate-report.mjs` passes with zero WARNs. AC6 (feature-gating/clippy/fmt) confirmed clean across the full matrix. `cargo test --release --features "production bench-internals"` green (4/4 new AC tests); `cargo clippy --features production -- -D warnings` clean; `cargo fmt --check` clean; `npm run check` ALL GREEN.

#### Measurement, correctness & tooling

- **[measurement, P1] R32-1b (task #492) — closed R31-10's missing cost side: `trim_current_thread()`'s call latency and the second-burst cold-start penalty it causes, measured in the SAME burst→trim→idle→burst2 regime the RSS-benefit gate used (CLAUDE.md's cost/benefit same-workload-regime rule).** New real-`#[global_allocator]` binary `examples/r31_10_trim_cost_gate.rs` reruns R31-10's exact workload (4 × 32 MiB Large objects, 500 ms idle) with `Instant`-timed trim-call and burst2 wall-clock measurement, driven both as a quick single-shot orchestrator and through `scripts/paired-ab-runner.mjs --config docs/perf/r31_10_cost_ab_config.json` for a statistically-judged N=20 paired A/B plus same-vs-same control. Result: **trim call itself costs ~24.2 ms; burst2 after a trim costs ~65.2 ms vs. ~0.8 ms for the no-trim control's burst2 — an 83.3× cold-start penalty (mean delta 64.4 ms, t=331.6 past crit=2.101, sign 20/20; same-vs-same control t=-0.044, noise as expected).** Every TRIM launch's mechanism oracle (`action_released_delta > 0` AND `burst2_reserved_delta > 0`, CLAUDE.md's R30-8 rule) and every NO_TRIM launch's counterpart (both exactly 0) passed across all 40 raw launches. New checked derivation script `scripts/r31_10_derive_cost_report_data.mjs` computes every table cell from the raw paired-ab-runner provenance JSON and asserts the headline significance/ratio claims in-script (CLAUDE.md's derived-tables rule) — writes `docs/perf/R31_10_TRIM_CURRENT_THREAD_COST_GATE_summary.csv`. Appended (not a rewrite) as new §5 "Cost side" to `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`, per this file's append-only correction convention — §§0-4's RSS-benefit numbers/verdict are unchanged. Immutable source identity: git tree SHA `63eb2faa84cba9131786871ba01c26d0460d2b5b` (base `45f3d83`), per CLAUDE.md's R29-6 rule. A MIXED Small/Large workload variant was considered and explicitly SCOPED OUT (the small-pool drain has no OS call per segment, unlike the large-cache eviction path this gate isolates — see the example's own module doc); throughput/CPU cost is not separately distinguishable from wall-clock at this workload's scale (4 ops/burst). A `TrimOptions`/`TrimReport`-shaped tiered/partial-trim API (originally scoped as an optional Part C of this same task) was explicitly NOT built this task — judged too large to build soundly alongside Parts A/B in the same task; left for a future task rather than shipping a partial API. `cargo build --release --example r31_10_trim_cost_gate --features production` clean; `node scripts/paired-ab-runner.mjs --config docs/perf/r31_10_cost_ab_config.json --verify-only` PASS; `node scripts/r31_10_derive_cost_report_data.mjs ...` all headline assertions pass.
- **[measurement, P0] R31-0 (task #471) — rebuilt `virgin-zero-skip`'s wall-clock judge through the actual production `HeapCore` magazine layer, correcting R30-3's NO-GO verdict which had (unintentionally) measured a bare-`AllocCore` substrate the feature does not run on in production.** New harness `benches/r31_0_virgin_zero_skip_production_layer_gate.rs` drives same-class `alloc_zeroed` bursts through `HeapCore::alloc_zeroed` on freshly `HeapRegistry::claim()`'d heaps (4/16/64/128 KiB × 3 touch behaviors × virgin/recycled scenarios), with a three-signal path-activation oracle (magazine-hit parity, explicit-zero-call activation percentage with a 95% gate, and a per-size `dbg_tcache_virgin_mask` retention probe) — no new `dbg_*` hook added, all accessors pre-existing and safe. Result: 100% same-class-burst virgin activation (vs. R30-3's ~3% ceiling on its different substrate), and a material, reproducible −89% to −98.6% wall-clock win on the touch-light `notouch` consumer category across all 4 sizes; the touch-heavy `onebyte`/`full` majority case remains noise-dominated with no reproducible win, matching R30-3's own finding there. Verdict: a narrower, workload-shape-conditional GO-supporting result, NOT a blanket `production` promotion — `Cargo.toml` untouched, pending separate user sign-off. `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` (new report) + its summary CSV + raw logs (`_raw_r31_0_off.log`/`_raw_r31_0_on.log` primary, `_raw_r31_0_off_run2.log`/`_raw_r31_0_on_run2.log` repeat); `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` §8 (new dated correction, append-only); `docs/perf/OPEN_ITEMS.md` item 25 (REOPENED). `cargo test --features "production bench-internals alloc-stats"` green; `cargo clippy --features "production bench-internals alloc-stats virgin-zero-skip" -- -D warnings` clean; `cargo fmt --check` clean.
- **[measurement, P1] R31-2 (task #465) — swept R30-7's 8-thread/4-size-mix server-shaped small-pool-cap comparison across cap 8/16/32 (not just R30-7's single cap-4-vs-cap-8 point) to find where the ZERO mechanism delta R30-7 found actually moves off zero, or confirm it never does up to cap 32.** Four new real-`#[global_allocator]` binaries (`examples/r31_2_pool_cap_threshold_ab_cap{4,8,16,32}.rs`), byte-identical workload body to R30-7's (`examples/_shared/r31_2_pool_cap_threshold_workload.rs`, an intentional copy so the cap4-vs-cap8 arm re-measures R30-7's own comparison point), driven pairwise (baseline cap4 vs each larger cap) via `paired-ab-runner.mjs`'s multi-arm `--config` support. Result: **`decommit_calls_total` is bit-identical (40) across all 320 process launches, every arm, every cap from 4 through 32 — a clean reject, not an underpowered null.** RSS/commit are correspondingly flat (~77.5 MiB / ~99.75 MiB, no material growth at any cap). Wall-clock shows no significant difference at any cap (all `|t| < crit(p<0.05)=2.101`, cap32's sign split a dead heat at 10/10), at a **minimum-detectable-effect of ~4-5% of the mean — materially tighter than R30-7's own 18.8% MDE**, making this a more decisive null than R30-7's underpowered one. Candidate explanation (not proven root cause): `decommit_calls_total` is a process-wide counter, and this workload's per-thread peak working set (24.00 MiB ≈ 6 segments) may simply never make ANY of the swept pool caps the binding constraint, so all four configs behave identically by construction. New checked derivation script `scripts/r31_2_derive_report_data.mjs` (CLAUDE.md's R30-9 rule) computes every table/CSV cell from the raw provenance JSONs, asserting its own MDE arithmetic before printing. `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md` (new report) + its summary CSV + 4 raw logs (`_raw_r31_2_cap4_vs_cap{8,16,32}.log`, `_raw_r31_2_control.log`) + 4 provenance JSONs. No `src/` default, `Profile`, or `Cargo.toml` `production` line changed. `cargo test --features "production alloc-stats bench-internals"` green; `cargo clippy --features "production alloc-stats bench-internals" --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- **[measurement, P1] R31-3 (task #466) — re-verified all six `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` hardening checkpoints against current `HEAD`, refreshed its A/B on current code, and closed the two precondition gaps a review named as missing (N=1/2/4 narrow-working-set timing regression, multi-heap RSS accounting).** All six R14-5 checkpoints hold on current source; the one real drift (`DEFAULT_EXTENDED_BUDGET_BYTES` 5x/1280 MiB → 1x/256 MiB, R17-9) was already disclosed in R14-5's own update note and is confirmed current, not a new finding. The R14-5 §6 turnover-profile A/B (unmodified harness, `examples/paired_ab_large_cache_extended_{off,on}.rs`) reproduces cleanly at the current finite 256 MiB default: `t=127.776` (n=20, crit 2.101), sign 20/20, mechanism confirmed (33.3%→100% hit rate) — same-vs-same control `t=0.739` (noise). New N=1/2/4 TIMING gate (`examples/r31_3_large_cache_extended_narrow_{off,on}.rs`, forces sidecar materialisation via a 9-size burst then narrows and times only the narrow phase) found NO regression — the extended cache measured FASTER at every N (t=7.1–17.8, sign 19-20/20, same-vs-same controls confirm noise floor), mechanistically explained by the base cache's own FIFO-eviction-and-refill cost from the materialisation burst (`segments_reserved_total` differs 10 vs 14), not anything intrinsic to the wider scan bound. New multi-heap RSS gate (`examples/r31_3_large_cache_extended_multi_heap_rss_gate.rs`, mirrors R29-13's subprocess-per-arm methodology at 1/8/32 concurrently-claimed heaps) confirms the finite 256 MiB default genuinely bounds per-heap retention (~248 MiB/heap capped vs ~432 MiB/heap unbounded) with EXACT linear scaling across heap count (`used_post_teardown_sum / max` = 1.0000/8.0000/32.0000 in both arms) — no multi-heap RSS blow-up, no shared/amortized-state surprise in this workload shape. Two new `HeapCore`-level `bench-internals`-gated diagnostic delegations added (`dbg_large_cache_budget`, `dbg_large_cache_extended_slot_sizes`, `dbg_large_cache_extension_materialised`), all thin read-only forwards to pre-existing safe `AllocCore` accessors, no new `unsafe`. Ends with a promotion PROPOSAL (not a decision): no numeric change to the finite default recommended, coordination note filed for R31-9/#473's `Profile` rework, and the `npm run bench:table`/`IAI_BASELINE.md` refresh cost flagged if accepted — `Cargo.toml`'s `production` line untouched, no `Profile`/config code implemented, pending explicit user sign-off. `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` (new report) + its summary CSV + 9 raw logs. `cargo test --features "production bench-internals alloc-stats"` green (827+ tests, 0 failed); `cargo clippy --features "production bench-internals alloc-stats large-cache-extended" --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- **[measurement, P1] R31-1 (task #464) — measured large-cache hit rate at burst sizes that GENUINELY exceed the 64 MiB headroom, resolving whether R30-6's 64-vs-256 MiB tie was real or an artifact of measuring exactly at the boundary.** New sibling harness (`examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`, same subprocess-per-arm + config-identity + path-activation-oracle methodology as R30-6) sweeps three burst sizes — `AT_BOUNDARY_6MiB` (R30-6's own object size, rounds to a 64 MiB burst, run here as an in-harness control), `CROSSING_MODEST_12MiB` (rounds to 128 MiB), and `CROSSING_R29_13_34MiB` (R29-13's own object size, rounds to 288 MiB) — × {64, 256} MiB headroom × {1, 8, 32} threads. Result: the AT_BOUNDARY control reproduces R30-6's tie exactly (100.0% both headroom values, every thread count); **both crossing-regime sizes show the tie BREAK — 64 MiB headroom costs the identical 12.5-percentage-point hit-rate loss (87.5% vs 100.0%) that R30-6 already measured for 16/0 MiB, exact and reproducible at every thread count and both crossing sizes.** New checked derivation script `scripts/r31_1_derive_report_data.mjs` hard-asserts this headline arithmetic in-script (a tie at the boundary size, a real 12.5pp gap at both crossing sizes) before writing the summary CSV, per CLAUDE.md's R30-9 rule. `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` (new report) + its summary CSV + raw log. No `src/` default, `Profile`, or `Cargo.toml` `production` line changed. `cargo test --features "production alloc-stats bench-internals"` green; `cargo clippy --features "production alloc-stats bench-internals" --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- **[correctness fix, P1] R31-4 (task #467) — implemented the `ReservedSmallSegment` typed handle R30-10's design proposal deferred, closing `docs/CORRECTNESS_OPEN_ITEMS.md` item 7, plus two P2 gating fixes from the Round-30 full review (item 8).** Retrofitted `AllocCore::dbg_decomp_reserve_and_keep`/`AllocCore::dbg_decomp_release` (and their `HeapCore`-level delegations in `heap_core_diag.rs`) from a bare `Option<*mut u8>`/`unsafe fn(&mut self, base: *mut u8)` pair — guarded only by a `debug_assert!` compiled out in `--release` — onto a new type, `ReservedSmallSegment` (`src/alloc_core/reserved_small_segment.rs`, new one-export file per this project's file-structure rule): a private `base: *mut u8` field, a `pub(super)` constructor reachable only from `AllocCore`'s own reservation path (unforgeable — no `pub` constructor anywhere in the crate), and a `pub(super) fn into_base(self) -> *mut u8` consumed exactly once by `dbg_decomp_release`, which now takes the handle BY VALUE. Consuming a moved-from handle a second time is `rustc` error E0382 ("use of moved value") at COMPILE time, not a runtime hazard — the actual soundness win the design doc identified. `dbg_decomp_release` is no longer `unsafe fn` (its precondition is now type-enforced); the existing `debug_assert!(base != self.small_cur, ...)` R30-1 added stays as secondary defence-in-depth. A `Drop` impl (debug-only `debug_assert!`) catches a leaked (never-released) handle as a measurement-harness bug, not a soundness issue. A `#[doc(hidden)] pub fn base(&self) -> *mut u8` read-only accessor (the established test-only-export pattern) lets `examples/r29_3_decomposition_gate.rs` read the payload address for its `write_volatile` first-touch measurement between reserve and release without weakening unforgeability (reading is not constructing). Updated all 5 call sites (`src/alloc_core/alloc_core_small_pool.rs`, `src/registry/heap_core_diag.rs`, `examples/r29_3_decomposition_gate.rs`, `tests/r30_1_decomp_full_cycle_cursor_safety.rs`, `tests/dbg_hook_safety_tripwire.rs`'s `UNSAFE_HOOKS` allowlist — both `dbg_decomp_release` entries removed, now safe+gated) — matches the design doc's own ~5-file estimate exactly. New counterfactual test `tests/r31_4_reserved_small_segment_handle.rs`: since a compile error cannot be exercised by a runtime `#[test]`, and `trybuild` is not a dev-dependency of this crate (checked before deciding, per CLAUDE.md's tests section — no new test-tooling dependency added for one case), the file thoroughly exercises the legitimate single-use and repeated-use paths and documents, in a code comment plus `ReservedSmallSegment`'s own module doc, exactly why a second `dbg_decomp_release(handle)` call cannot compile (move semantics — `ReservedSmallSegment` has a `Drop` impl, so it can never be `Copy`, a hard compiler rule). **P2-1** (`docs/CORRECTNESS_OPEN_ITEMS.md` item 8): `tests/dbg_hook_safety_tripwire.rs`'s `has_bench_internals_cfg` matched the bare 5-byte prefix `#[cfg`, which also matches `#[cfg_attr(` — whose first argument controls whether its SECOND argument applies, not whether the item compiles at all; fixed to require the literal 6-byte `#[cfg(` (open paren included). Two new tests (`has_bench_internals_cfg_rejects_cfg_attr_shape`, `scan_file_treats_cfg_attr_bench_internals_hook_as_ungated`) plus a going-forward structural guard (`no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape`) confirm no live hook uses the shape. **P2-2** (same item 8): `HeapCore::dbg_large_cache_hits` (R30-6, task #455) was gated `alloc-decommit` alone — inside plain `production` — while its four sibling `HeapCore`-level measurement delegations in the same file are all gated `all(alloc-decommit, bench-internals)` per CLAUDE.md's benchmark-hook rule 2 (no production caller ⇒ default to `bench-internals`); tightened to match. Verified both current callers (`examples/r30_6_large_cache_headroom_ab_gate.rs`, `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`) already require `bench-internals` in `Cargo.toml`'s `required-features` — confirmed via a throwaway compile probe that `HeapCore::dbg_large_cache_hits` is genuinely E0599-unreachable under plain `--features production` post-fix. Moved out of the tripwire's `PURE_OBSERVERS` allowlist (gated hooks aren't tracked in either allowlist). Fixed two resulting doc-drift failures (`docs/ARCHITECTURE.md`'s stale 227-file test count → 228; README's stale tier-2 `#[allow(unsafe_code)]` site count 68 → 66, reflecting the two `dbg_decomp_release` `unsafe fn` sites now removed — both files' per-file breakdown rows updated too). `docs/CORRECTNESS_OPEN_ITEMS.md` items 7 and 8 marked FIXED (append-only). `cargo test --features "production bench-internals alloc-stats"` green (230 test binaries, 0 failed); `cargo test --features "bench-internals alloc-global alloc-xthread alloc-decommit fastbin alloc-segment-directory primordial-lazy-commit class-aware-dirty" --test dbg_hook_safety_tripwire` green (7 tests); `cargo clippy --features "production bench-internals alloc-stats" --all-targets -- -D warnings` clean; `cargo clippy --features production -- -D warnings` clean; `cargo fmt --check` clean. No `production` feature composition or default constant changed — purely an internal API-safety retrofit for a `bench-internals`-only measurement hook pair plus two gating tightenings.
- **[correctness fix, API design, P1] R31-9 (task #473) — reworked the `Profile` API R30-7 shipped: `Profile::Rss` never actually bounded RSS, and `Profile::Throughput` silently narrowed the large-cache window on unproven evidence that R31-1 has since CONTRADICTED with a real, confirmed cost.** `Profile::0.3.0` (unreleased — confirmed via `Cargo.toml` before treating this as free to restructure) is public API, so no back-compat shim was needed. Two confirmed defects fixed: (1) `Profile::Rss`'s name promised an RSS bound but `headroom_bytes` is an eventual decay FLOOR, not an admission limit (`budget_bytes` stays `None`/unbounded unless set explicitly, decay is event-driven, and R29-13 measured exactly 0 KiB reclaimed by idle alone across 36/36 arms) — a burst could leave far more than the named "Rss" profile's headroom resident indefinitely; (2) `Profile::Throughput` bundled the small-pool win with a large-cache headroom drop from 256 MiB to 64 MiB, which at ship time had only AT-the-64-MiB-boundary evidence behind it — R31-1 (this same round, task #464) has since measured BEYOND that boundary and found a real, reproducible 12.5-percentage-point hit-rate cost (87.5% vs 100.0%), so a profile literally named "Throughput" was overclaiming against evidence this same round produced. **Structural fix (not just a rename):** replaced the flat three-arm `Profile::{Rss,Balanced,Throughput}` enum with a small builder, `Profile::new().small_pool(SmallPoolPolicy::…).large_cache(LargeCachePolicy::…)`, composing two independently-settable axes (`src/alloc_core/profile.rs`, new `SmallPoolPolicy`/`LargeCachePolicy` `#[non_exhaustive]` enums) — a caller who wants ONLY the small-pool latency win no longer has to also accept the large-cache narrowing as a package deal, and vice versa. `SmallPoolPolicy` has `Default`/`Throughput` (unchanged knob values, `(4,16 MiB)`/`(8,32 MiB)`); `LargeCachePolicy` has `LowHeadroom` (16 MiB, the old `Rss` value, doc now states plainly it is a decay floor not a cap), `Trimmed64MiB` (64 MiB, the old `Throughput`/`Balanced` value, doc now states the R31-1 crossing-regime caveat explicitly), and `Default` (256 MiB, `LargeCacheConfig::DEFAULT`'s own value — so `Profile::new()`/`Profile::DEFAULT` is now genuinely byte-identical to `SeferAlloc::new()`, verified by a dedicated test, not merely asserted in prose). `LargeCachePolicy` reserves (but does NOT implement or make constructible) a documented slot for a future `large-cache-extended`-backed policy, per R31-3/task #466's pending, NOT-yet-accepted promotion proposal — explicitly not wired in this task. Low-level `LargeCacheConfig`/`SmallSegmentPoolConfig` builders are untouched and remain the full-control escape hatch; `SeferAlloc::with_profile` stays `const fn`-usable directly in a `#[global_allocator]` `static` initialiser (unchanged capability, reverified by the compiling `examples/r30_7_throughput_profile_server_ab_throughput.rs`, updated to the new call shape). Doc comments throughout (`profile.rs`, `sefer_alloc.rs`, `large_cache_config.rs`, README's "Named profiles" section) now carry the measurement-regime caveats honestly per this round's own findings: the large-cache "parity" claim is scoped to "at a 64 MiB rounded working set," never general equivalence; the small-pool throughput claim cites R27-4's single-threaded workload shape specifically and notes R31-2's clean null (no mechanism change at any cap through 32 on an 8-thread server-shaped workload) rather than implying general applicability — this also closes the doc-comment-overclaim half of task #469's scope for `Profile` specifically (task #469 itself untouched/still open for its other item, the `r29_3_decomposition_gate.rs` Windows crash, which is unrelated). Rewrote `tests/profile.rs` for the new shape: 7 tests covering `Profile::new()`'s byte-identical-to-`SeferAlloc::new()` claim, the `Default` trait impl, each axis set alone not perturbing the other, both axes composed together reproducing the old bundled `Throughput` combination, all six 2×3 axis-value combinations resolving correctly (not just the two points the old enum happened to name), and the R27-1 no-op trap not reappearing for any `SmallPoolPolicy` value — all read back via the same `AllocCore::dbg_pool_cap`/`dbg_decay_config` diagnostic surface the pre-existing tests used, not just the requested builder value. `cargo test --features "production bench-internals alloc-stats"` green (230 test binaries, 0 failed); `cargo clippy --features production -- -D warnings` clean; `cargo clippy --features "production bench-internals alloc-stats" --all-targets -- -D warnings` clean; `cargo check --all-features` clean; `cargo fmt --check` clean. No `production` feature composition or default changed — this is a restructuring of the opt-in `Profile`/config surface only; no version bump, no push.
- **[correctness fix, measurement, P2] R31-12 (task #476) — repaired five data-hygiene defects in R30-6's report/CSV (append-only) and, working with R31-1's crossing-regime result, narrowed `docs/perf/OPEN_ITEMS.md` item 27's parity claim.** Independently confirmed (reading `AllocCore::alloc_large`'s whole-`SEGMENT` rounding, `src/alloc_core/alloc_core_large.rs:127-194`, against R30-6's own committed CSV, whose `burst1_used_max_bytes` column reads exactly 64 MiB — not the 48 MiB the report's prose named — in all 36 rows) that R30-6's "8×6 MiB = 48 MiB" workload actually rounds to a 64 MiB working set, i.e. measured EXACTLY AT the 64 MiB headroom boundary — item 27's "64 MiB ties 256 MiB" claim is narrowed (append-only) to "parity at a 64 MiB rounded working set," not general throughput/hit-rate equivalence. New checked script `scripts/r31_12_repair_r30_6_data.mjs` re-derives, from the ALREADY-COMMITTED raw log and provenance JSONs (no re-measurement), five repairs landed as a new append-only §8 in `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md`: (1) the report's "single-digit KiB idle-delta" claim cited the wrong column pair (`rss_idle - rss_burst2`, exact in 0/36 rows, structurally wrong since burst2 is sampled after the idle window) — the intended claim (`rss_idle - rss_burst1 == 0`) IS exact in 33/36 rows, now stated correctly; (2) the one physically-impossible raw-log row (`67108864,64,32,2`: ~1.58 GiB RSS "collapse" to 424 KiB across a 1.2s pure-idle window with zero deallocation activity) is now explicitly excluded with a stated bound (`rss_burst1 - rss_idle <= rss_burst1/10 + 4096`), confirmed to change no headline conclusion, and R31-1's sibling harness adds the equivalent hard `assert!` at measurement time so a future run cannot admit the same class of broken sample; (3) the summary CSV's prose commit-SHA placeholder is filled in with the real landing SHA (`97c2f07b`); (4) a minimum-detectable-effect is now stated for the §0.2 latency-null headline (18.4%-25.9% of mean elapsed time across the four comparisons, computed by the same `crit * se` formula R30-7 used for itself) — the null means "no effect this large was found," not "no effect exists"; (5) an explicit documented limitation is added: R30-6's latency workload keeps every arm at 100% cache hits by construction, so it structurally cannot expose the wall-clock cost of a cache miss under a smaller headroom. `docs/perf/OPEN_ITEMS.md` items 27 and 31 updated append-only with the verification results. `Profile::Balanced`'s/`Profile::Throughput`'s doc comments (`src/alloc_core/profile.rs`) still carry the un-narrowed claim — flagged as an input for R31-9/task #473 (already reworking `Profile`'s docs), not edited by this measurement/docs-only task. No `src/` default changed. `cargo test --features "production bench-internals alloc-stats"` green; `cargo clippy --features "production bench-internals alloc-stats" --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- **[correctness fix, process, P1/P2] Round 31 review response — an independent full-round review (`docs/reviews/2026-07-31-r31-full-review.md`, 0 P0s, 3 P1s, 12 P2s) closed out: P1-1 fixed directly by the orchestrating session (commit `eb6e3dc`, `scripts/r31_0_summary.mjs` now actually derives and asserts its headline deltas instead of computing none); this task closes the remaining two P1s and files all 12 P2s.** **P1-2** — R31-2 §4.3 points 2-3 claimed a RESOLVED-config runtime read-back and stated a config-conflict counter "does not apply," neither of which was accurate (the workload only echoes the compile-time constant it was passed; nothing calls `HeapCore::dbg_pool_cap()`/`config_conflicts_total()`). Fixed via the review's option (b), not (a): switching the workload off the real `SeferAlloc` `#[global_allocator]` onto `HeapRegistry::claim_with_config` to add a runtime read-back would itself measure the wrong layer for a report whose whole point is the `#[global_allocator]`-layer mechanism — exactly the class the new CLAUDE.md rule below exists to prevent — so §4.3 was corrected (append-only, dated block, original text struck through and preserved beneath per this project's convention) to state honestly that the resolved cap was established by source-reading `AllocCore::new_with_config` (`src/alloc_core/alloc_core.rs:933-963`, the `pool_cap = resolved_pool_segments().min(resolved_pool_byte_cap()/SEGMENT)` assignment at `:961-963`) plus a structural argument (`min()` is the identity for all four arms' constants), and that the config-conflict counter genuinely does not apply for a different, correct reason (this entry point never reaches `HeapRegistry::claim_with_config`'s slot-reuse path at all, so the counter has no mechanism to observe here — not "no runtime resolution step exists," which was the original, inaccurate justification). The report's headline VERDICT is unchanged — `decommit_calls_total`'s mechanism-delta finding does not depend on this correction. **P1-3** (the more important of the two) — codified the round's own defect class as a new CLAUDE.md "Active rules" entry, sibling to the R26-4 config-evidence rule and the R30-8 mechanism-activation rule: *a gate report must name the exact entry point under test and state why that layer is the one the decision applies to.* Framed explicitly as the third instance of one meta-pattern (R25-5 wrong CONFIG → R26-4; R29-16 wrong CODE PATH → R30-8; R30-3 wrong LAYER → this rule), citing R30-3's own judge as the motivating incident: it satisfied every existing rule, including a path-activation oracle, and still shipped a wrong NO-GO verdict by measuring `AllocCore::alloc_zeroed` instead of `HeapCore::alloc_zeroed` (caught and reopened in R31-0, commit `dece4a7`). Also filed as `docs/perf/OPEN_ITEMS.md` item 32 (owned, CLOSED status — the rule's codification IS the closure), per CLAUDE.md's own "Round start: check BOTH open-items indexes" convention, so a fresh Round 32 session inherits this without reading a review doc first. **P2-6** (`ReservedSmallSegment` should be `#[must_use]`) was checked first per the task brief's instruction and found NOT yet applied — fixed directly (one line, `src/alloc_core/reserved_small_segment.rs`, `#[must_use = "..."]` on the struct; the `base()` accessor already had its own `#[must_use]`, the struct itself did not). **The other 11 P2s were initially filed, not fixed** (this task's own scope), split by this project's existing scope boundary between the two open-items indexes: `docs/perf/OPEN_ITEMS.md` item 33 (P2-1 ragged CSV, P2-2 vacuous-statistic-not-marked, P2-3 uncommitted-run citation, P2-7 misattributed row count, P2-8 KiB/MiB unit error, P2-9 post-hoc provenance, P2-10 stale `Profile::Throughput` doc references) and `docs/CORRECTNESS_OPEN_ITEMS.md` item 9 (P2-4 `pub(super)` scoping doc overclaim, P2-5 missing cheap `needs_drop` counterfactual assertion, P2-11 `AllocCore::dbg_large_cache_hits` gating asymmetry with its `HeapCore` sibling, P2-12 tripwire coverage narrowed by the R31-4 retrofit's rename) — both filed UNVERIFIED-BY-ME at the review's own confidence/severity, matching item 31/item 8's exact established precedent from the Round 30 review response one round earlier. **UPDATE (Round 32, tasks #483/#484): 10 of these 11 were independently re-verified and FIXED** (R31-14a/R31-14b below) — only P2-9 (post-hoc provenance) remained open, and was itself closed by a different mechanism (`scripts/capture-measurement-identity.mjs`, task #481) rather than retrofitted onto the already-published reports, per this file's non-retroactive convention. `cargo test --features "production bench-internals alloc-stats"` green; `cargo clippy --features "production bench-internals alloc-stats" --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo test --features "production bench-internals alloc-stats" --test no_stale_doc_references` green. No `production` feature composition or default constant changed; no version bump.
- **[docs] R31-8 (task #472) — added a fourth evidence rule to CLAUDE.md's R26-4/R30-8/entry-point family: cost and benefit must be measured in the SAME workload regime.** Cites R30-6 (task #455) as the motivating incident: independently re-verified `AllocCore::alloc_large`'s whole-`SEGMENT` (4 MiB) rounding against `src/alloc_core/os.rs:65`/`alloc_core_large.rs:190-192` and confirmed R30-6's `8 × 6 MiB` workload actually rounds to exactly 64 MiB — the 64-vs-256 MiB "tie" it measured was structurally guaranteed by landing exactly at the headroom boundary under test, not empirically discovered; that parity was then combined with R29-13's ~7× retention delta, measured under a completely different fill/drain regime, into one Pareto claim. `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` vs `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` cited as the positive counter-example (two reports that correctly kept their regimes separate). Marked non-retroactive per this file's established convention. Docs-only; `npm run check` green.
- **[correctness fix, process] R31-7d1 + R31-13 merged (task #479) — consolidated four independently-filed OPEN_ITEMS.md triggers that all wait on the identical missing artifact, and reconfirmed the batch-API no-consumer decision against the current tree.** X5 (item 20), T10 (item 22), R1 (item 23), R15-1 (item 9) each independently NO-GO'd a scan/hint/queue optimization because every current bench models ≤3 live segments; three of the four converged on byte-identical "needs a ≥64-segment bench" wording across three separate rounds. New item 34 states the shared precondition once; the four originals gained a one-paragraph cross-reference each (append-only, no history removed). Separately, re-checked R23-7's 2026-07-27 no-consumer decision for `batch-api`: grepped every `src/`/`crates/`/`examples/` file for new `alloc_batch`/`dealloc_batch` call sites (none beyond the existing `SeferAlloc` forwarding layer) and read all three `crates/` workspace members that postdate R23-7 (`region`, `ring-mpsc`, `tagged-index-stack` — none is a batch-shaped consumer). Filed as new item 35: R23-7's decision RECONFIRMED, not merely restated. No macro-bench built, no batch API expanded. Docs-only; `npm run check` green.
- **[process] R31-5a (task #480) — built `scripts/verify-gate-report.mjs`, mechanizing three of CLAUDE.md's gate-report rules as structural checks instead of leaving them as prose a future report could silently violate.** Scans every `docs/perf/R*_*.md` and checks: (a) every cited `*_summary.csv` exists on disk, (b) every cited commit/SHA field is a real 40-hex git SHA (not a placeholder), (c) every cited `_raw_*.log` exists — with a curated, individually `git merge-base`-verified retroactive-exemption list for reports predating the relevant rules. Non-vacuity run against the full 87-report corpus caught a genuinely live defect: `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv`'s `landing_commit` column was still the literal string `UNFILLED` on every row despite two PRIOR commits each separately claiming to have fixed it (one had edited the wrong column; one had regenerated the CSV without passing the SHA argument) — fixed in a follow-up commit by re-running the report's own existing derivation script correctly. Wired into `npm run verify-gate-reports`, `npm run check`, and `ci.yml`'s `check-matrix` job. `npm run check` green.
- **[process] R31-5c (task #482) — built `scripts/verify-commit-prefixes.mjs`, a heuristic lint enforcing R30-12's `perf(runtime)`/`perf(opt-in)`/`bench`/`docs(config)` taxonomy over real commit history, not just prose in CLAUDE.md.** FAILs a bare `perf(...)`/`perf:` commit whose diff touches nothing outside `docs/`/`examples/`/`benches/`/`tests/`/`scripts/`, or a `perf(runtime)`/`perf(opt-in)` commit with the same shape; WARNs (non-blocking) the mirror-image case (a `bench`/`docs(config)` commit that DOES touch `src/`/`Cargo.toml`). Non-retroactive — hard-clips at the commit that added the R30-12 rule (`3f7db16`) via `git merge-base --is-ancestor`. Non-vacuity: ran against 24 real post-rule commits (0 false FAILs, 5 benign WARNs, all inspected); created two throwaway scratch commits on `main` (`perf(docs): fake test commit`, `perf(runtime): ...` touching only `docs/`), confirmed both FAIL, removed both via `git reset --soft` (corroborated later by an independent review reading `git reflog`). Wired into `npm run check:commit-prefixes`/`npm run check`, plus a new PR-scoped `commit-prefix-lint` CI job (`base.sha..head.sha`, `fetch-depth: 0`). `npm run check` green.
- **[process] R31-11 (task #475) — closed the gap between `ci.yml`'s clippy job header comment's claimed guarantee and what the repo actually enforced.** The comment asserted that if the job's 5 hand-listed clippy steps ever drifted from `scripts/check-matrix.mjs`'s manifest, the `check-matrix` job would still catch it — false: that job's `--kind check --kind test` filter structurally excludes every `clippy`-kind manifest row. New `tests/ci_clippy_matrix_consistency.rs` (chosen over folding clippy into the generator, to keep the clippy job's per-combination named Actions-UI steps and add zero cargo invocations) reads both files as text and asserts 1:1 match in count, order, and feature string — runs as part of ordinary `cargo test`, no dedicated CI job needed. Non-vacuity: mutated one clippy step's feature string (caught, correct mismatch reported), then deleted an entire step (caught, correct count mismatch reported), reverted both, confirmed green again. Also fixed the adjacent cosmetic inaccuracy ("Deliberately `--kind check` here" when the step actually passes `--kind check --kind test`) and rewrote the comment to describe the ACTUAL guarantee. `npm run check` green (79/79 iai, no regressions).
- **[process] R31-5b (task #481) — extended `scripts/verify-gate-report.mjs` with four WARN-level semantic checks and a new pre-measurement identity-capture helper, `scripts/capture-measurement-identity.mjs`.** New checks: (d) prose↔CSV headline cross-check (a number+unit near a headline keyword should reappear, at rounding, in the cited CSV); (e) mechanizes the "Allocator layer under test" CLAUDE.md rule (commit `2d9eef2`); (f) between-arm mechanism-delta presence (CLAUDE.md's R30-8 rule); plus a cross-cutting SHA-vs-raw-log-mtime freshness check flagging likely post-hoc identity assembly. All WARN-only (never fail `npm run check`) — a substrate-only report or a report using different phrasing legitimately has none of these. The identity helper wraps `git write-tree` (a tree-object SHA, captured immediately BEFORE a measurement's binaries are built) plus an optional `git diff HEAD | sha256` secondary form. Non-vacuity: full 87-report corpus scan (zero new hard-fails), the task's own named retroactive test candidate (`R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md`) exercised exactly the intended "flag, don't hard-fail" path, four scratch break/fix round-trips (one per check) confirmed each check is non-vacuous — the corpus run itself caught and fixed two real bugs in the new regex/CSV-parsing logic before landing. `npm run check` green.
- **[correctness fix, measurement, P2] R31-14a (task #483) — independently re-verified and fixed 4 of the 11 P2s the Round 31 review response had filed (not fixed).** P2-1 (ragged CSV): `R31_0_..._summary.csv`'s 4 retention rows (16 fields) were interleaved under the wrong 24-column header with no marker — routed to their own file (`..._retention.csv`) under their own header; re-ran the derivation script against the already-committed raw logs (no re-measurement), headline hard-assert still passes. P2-2 (vacuous OFF-arm statistic): the OFF binary's `mean_act_pct`/`min_act_pct` columns carried numeric 100.00/0.00 despite the underlying counter never incrementing on OFF (only `oracle=NA` signaled this) — now emit `NA` there too. P2-8 (KiB/MiB unit error): `R31_3_..._summary.csv`'s threads=8 note read "~410 MiB/heap" for a value that's 410,112 KiB = 400.5 MiB — corrected in place with a dated marker. P2-10 (stale `Profile::Throughput` references): `Cargo.toml`/`OPEN_ITEMS.md` still named the enum variant R31-9 (same round) had already replaced with `SmallPoolPolicy`/`LargeCachePolicy` — updated the live/prescriptive mentions, left already-dated historical narrative untouched. All four report corrections are dated append-only additions per this file's convention. `npm run check` green.
- **[correctness fix, docs, P2] R31-14b (task #484) — independently re-verified and fixed the remaining 6 of the 11 filed P2s (2 report citations, 4 code/test fixes).** P2-3: dropped four wall-clock percentages `R31_0_...md` cited from a never-committed third run (cannot be committed after the fact without re-measuring), kept only the qualitative statement, dated addendum. P2-7: `R31_1_...md` misattributed "36 rows" to R30-6's own committed CSV (which has 12); corrected to cite the raw log where the 36 actually live (confirmed via `grep -c '^[0-9]'` and the pre-existing `scripts/r31_12_repair_r30_6_data.mjs`'s own hard-assert). P2-4: `ReservedSmallSegment`'s doc comment overstated its `pub(super)` scope as "callable only from `alloc_core_small_pool.rs`"; actual scope is `pub(in crate::alloc_core)` (confirmed against `mod.rs`) — doc-only fix, not exploitable (one real caller). P2-5: added the missing `assert!(core::mem::needs_drop::<ReservedSmallSegment>())` counterfactual (a type with `Drop` can never be `Copy` — a hard rustc rule) to `tests/r31_4_reserved_small_segment_handle.rs`. P2-11: confirmed `AllocCore::dbg_large_cache_hits` has real `#[test]` callers reachable under plain `production` (unlike its already-tightened `HeapCore` sibling) — kept as a documented sanctioned exception, not tightened. P2-12: renamed `ReservedSmallSegment::base` → `dbg_base` so `tests/dbg_hook_safety_tripwire.rs`'s `dbg_`-prefix scan can see it again; the rename alone surfaced a SECOND gap (the tripwire genuinely failed until a redundant per-method `#[cfg]` was added, since the scanner reads only the immediately-preceding attribute block, not the enclosing `impl`'s) — independent evidence the tripwire works end-to-end. `cargo test --features "production bench-internals alloc-stats"` green (231 test-binary results); `npm run check` green.
- **[correctness fix, P2] R31-6 (task #469) — fixed a genuine Windows crash in `examples/r29_3_decomposition_gate.rs`'s Measurement B.** Windows `VirtualFree(MEM_DECOMMIT)` genuinely unmaps pages (unlike POSIX `MADV_DONTNEED`, which keeps the mapping and only drops physical backing), so the example's re-fault loop — write straight into a just-decommitted range with no intervening recommit — was an access violation on Windows, not a page fault. New `AllocCore::dbg_decomp_recommit_payload`/`HeapCore::dbg_decomp_recommit_payload` (`unsafe fn`, `alloc-decommit + bench-internals`-gated, mirroring the existing `dbg_decomp_decommit_payload`) wrap the EXISTING `os::recommit_pages` → `aligned_vmem::recommit`, which already does the right thing per platform (a real `VirtualAlloc(MEM_COMMIT)` on Windows, a documented no-op on Unix/miri) — no new unsafe in `examples/`. Non-vacuity: real BEFORE/AFTER runs on this Windows host (`STATUS_ACCESS_VIOLATION`/exit 5 → clean exit 0 with a full measurement report). Both new hooks registered in `tests/dbg_hook_safety_tripwire.rs`. README's `unsafe`-inventory counts bumped (66→68 total, independently re-grepped). `npm run check` green (iai 79/79).
- **[correctness fix, process, P1/P2] Second Round 31 review response (independent full review, `docs/reviews/2026-07-31-r32-full-review.md`, 0 P0, 3 P1, 11 P2) — all three P1s independently re-verified against source before fixing, all three against R31-10/task #474.** P1-1: AC4's test claimed to exercise the fallback-heap path but did not — a never-allocated thread's null `LOCAL` resolves through `tls_heap::finish_bind` to `CurrentHeap::Own` (claims a fresh registry slot), not `Fallback` (traced `tls_heap.rs:521-654`). Fixed: `trim_current_thread`'s rustdoc corrected; the original test renamed to `ac4a_trim_on_freshly_bound_never_allocated_heap_is_safe` (states what it actually proves); new `ac4b_trim_on_genuinely_torn_tls_is_safe_noop` added (`bench-internals`-gated, using the existing TORN-TLS test hooks) genuinely reaching `Fallback`. P1-2: `dbg_trim_current_thread` had silently become a no-op under `alloc-global + fastbin` builds without `alloc-decommit` — `trim_for_recycle` flushes tcache under `fastbin` independently of `alloc-decommit`, but the R31-10 rewrite gated the WHOLE delegating call on `alloc-decommit`. Fixed: the hook now calls `current_heap()`+`trim_for_recycle()` directly and unconditionally, restoring pre-R31-10 behavior in every configuration. P1-3: README and the design doc still said `trim_current_thread()` was "not implemented" after the round shipped it — fixed with a README table-row update + a "Memory policy" section pointer, and a dated `IMPLEMENTED 2026-07-31` notice on the design doc (append-only). Also fixed directly: P2-9, a misattributed RSS-vs-commit-gap mechanism in `R31_10_...md` §0.3 ("segment headers/guard pages" instead of the actual whole-`SEGMENT` rounding this round's own R31-8 rule codified) — corrected in place plus a dated §4 addendum. The remaining 10 P2s filed (not fixed) as `docs/perf/OPEN_ITEMS.md` item 36 and `docs/CORRECTNESS_OPEN_ITEMS.md` item 10, matching the exact "filed, not fixed" precedent this same round's first review response established one entry above. `npm run check` green end-to-end.

### Round 32 — a 20-task backlog (tasks #486-505) derived from two independently-verified sources: `docs/reviews/2026-07-31-r31-r32-readonly-review.md` (a P0 soundness bug, an invalid perf gate, two production-promotion cost/benefit gaps) and a 14-finding research survey (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`, F1-F13 + F1b) produced by two sequential background research agents — every task landed via a single sequential sub-agent, every result personally zero-trust-reviewed (diff read in full, tests re-run independently, checked derive scripts re-run against committed raw data) before being marked complete; the review found and fixed a real P0 cross-`AllocCore` release hazard (R31-15, task #486, already covered under Round 31 above) and a genuine double-panic-abort bug found while building its own counterfactual test, then worked through the survey's 14 findings finding eight real, measured, shipped wins in the always-on or opt-in production path (R32-3 through R32-12, tasks #494-503, detailed above and below), two correctly-scoped honest rejections with full measured evidence (F1b's dual-bitmap merge regressed 20-25x past the kill gate; a Windows `VirtualAlloc2` fast path was declined as unjustified once the avoidable reservation share measured only 4.3-4.8%), and closed the survey's own remaining docs-only findings — along the way, independent zero-trust review caught a sub-agent's report wrongly claiming a measurement was impossible on this dev host (WSL was in fact available the whole session) and, for the highest-risk task (a cross-thread lock-free ring-buffer change), independently re-derived the soundness argument by hand and independently re-ran a from-scratch loom model before trusting either

**Runtime improvements this round: 7.** Five land in the always-on `production` allocation path (R32-3's `realloc` move-leg shortcut, R32-7's large-cache-hit partial `SegmentHeader` write, R32-10's `OWN_CACHE_SIZE` 4→16, R32-11's `RemoteFreeRing` cached-head shadow, R32-12's large-cache occupancy bitmask) and two are opt-in-only (R32-4's `virgin-zero-skip` stamp removal, R32-8's `LowHeadroom`/`Trimmed64MiB` decay-clock throttle); `Cargo.toml`'s `production` feature composition is otherwise unchanged.

#### Runtime improvements

- **[runtime improvement, P2] R32-3 (task #494, F6) — `realloc`'s move leg and `try_promote_to_large` no longer re-derive `base`/re-run `contains_base` after an earlier call in the SAME function already proved it.** Both call sites now go through `dealloc_own_thread_with_base`/`dealloc_own_thread` directly (mirroring `dealloc_routing`'s own `#[cfg]` split verbatim), after independently re-tracing the full unregister-call-site enumeration reachable from `HeapCore::alloc` (`dec_live_and_maybe_decommit`'s `live == 0` gate; every Large-segment `unregister` site confirmed structurally disjoint from the Small/medium segment table) to confirm the old block is still LIVE — and therefore `contains_base` still true — at the point of the redundant call. Measured on `realloc_grow` (the existing iai bench this exact path already drives): −120 Ir; four churn benches byte-exact (0 delta). `alloc-xthread`/`alloc-global` are in `production`. README's tier-2 unsafe-inventory count updated 70→69 (caught by the existing doc-drift tripwire). `cargo test --features production` green; `cargo clippy` clean; `cargo fmt --check` clean.
- **[runtime improvement, opt-in, P2] R32-4 (task #495, F7) — removed a redundant `stamp_segment_owner` call from `alloc_zeroed`'s magazine-hit arm; corrected an R31-0 A/B confound in the same commit.** Plain `alloc`'s magazine-hit arm has an explicit "P4: NO stamp here" comment (every magazine-resident block's source segment is already stamped during the refill that pulled it); `alloc_small_zeroed_via_magazine`'s equivalent arm (the `alloc_zeroed` path under `virgin-zero-skip + fastbin`) carried the same call anyway, by symmetry with a genuinely different, magazine-bypassing sibling branch where it IS load-bearing. Enumerated all three producers of a magazine-resident block (`refill_magazine_slow`, `refill_magazine_slow_virgin`, and the free path's own push) and confirmed none can ever place an unstamped block. Measured via a new path-activation-oracle bench pair (isolated worktree, before/after Ir): −192 Ir over 16 magazine hits (−12.00 Ir/hit, within the predicted 12-18 Ir/hit range); four plain-`alloc` kill-gate benches exactly flat. Since `virgin-zero-skip` is not in `production`'s default bundle, this is `perf(opt-in)`. Appended a dated §9 correction to `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` naming the asymmetry — direction of bias is AGAINST the ON arm, so R31-0's/R32-0's published GO verdicts are unaffected (if anything understated). `cargo test --features production` and `--features "production,virgin-zero-skip"` green.
- **[runtime improvement, P2] R32-7 (task #498, F12) — the large-cache HIT path now writes 4 `SegmentHeader` fields instead of the whole ~144-byte struct.** 4 of the constructor's 8 fields (`span_usable`/`reserved_capacity`/`reservation`/`reservation_len`) are, by the pre-existing code's own comments, carried forward byte-identical from the cached slot — verified, not assumed, by a `debug_assert_eq!` falsification check added BEFORE the write itself was touched (never fired across the full suite; kept as a permanent correctness pin, `#[cfg(debug_assertions)]`-gated so it costs nothing in release). Replaced the full `Node::write_struct` with 4 targeted field writes via 3 new accessors, restating UBFIX-6's unregistered-window safety argument for the narrower write shape explicitly (all 4 writes still run strictly before `register()` makes the segment reachable, so no cross-thread reader can observe a torn/partial write or care about write order). Added a `size_of::<SegmentHeader>() == 144` compile-time pin (the struct had drifted 104→120→128→136→144 with only a coarse `<=PAGE` bound guarding it). Measured (WSL/callgrind, worktree-isolated): −32 Ir/hit, an 8.5% reduction of the hit arm's own marginal cost (377→345 Ir/hit); 5 kill-gate benches Ir-identical. `alloc-decommit` is in `production`. Discovered en route that the survey's own claim that a pre-existing bench "already exercises" the cache-hit arm was wrong (that bench never reaches a second `alloc`) — built a real bench pair plus a public-API path-activation oracle instead. `cargo test --features production` green.
- **[runtime improvement, opt-in, P2] R32-8 (task #499, F9) — `maybe_decay_large_cache`'s `Instant::now()` fast-path guard was a cliff `LargeCachePolicy::LowHeadroom`/`::Trimmed64MiB` are, by design, shipped to sit on the wrong side of; throttled the clock reads and disclosed the residual cost.** A confound-free A/B (fixed `headroom_bytes` across arms, a new `bench-internals`-gated `FORCE_DECAY_CLOCK_READ` switch isolates the clock-read cost from R31-1's own documented headroom-driven hit-rate confound) plus a path-activation oracle (`MAYBE_DECAY_GUARD_PASSED`) confirmed the effect reproduces at ~74-138 ns/call, consistent with task #95's historical ~105 ns/call anchor. Shipped a monotonic op-counter (`large_cache_decay_op_count`, `DECAY_CLOCK_CHECK_STRIDE = 64`) throttling clock reads to ~1-in-64 once past the headroom fast-exit, trading decay-tick granularity (a tick may fire up to ~63 large ops late, never early, never more aggressively) for far fewer clock reads; `dbg_force_decay_tick` explicitly bypasses the stride so its pre-existing deterministic single-call contract (depended on by `tests/large_cache_decay.rs` and R29-13's forced-convergence measurement) is unchanged. Measured the fix's own benefit in the above-headroom regime the two profiles target: 61-73% reduction in `maybe_decay_large_cache`'s own elapsed contribution, guard-passed call count down 128x. Updated `LowHeadroom`'s/`Trimmed64MiB`'s doc comments to disclose the (reduced but nonzero) residual cost alongside their existing RSS-vs-hit-rate tradeoff documentation. `perf(opt-in)` — the `production` default's 256 MiB headroom mostly never crosses the guard at all. `cargo test --features production` green.
- **[runtime improvement, P2] R32-10 (task #501, F2) — `OWN_CACHE_SIZE` (the free path's Tier-1 ownership cache) raised 4→16, after building the Tier-1 hit/miss counter needed to actually judge it.** Two prior gate reports had priced the two tiers in isolation (Tier-1 hit ~8.2 Ir, Tier-2 miss ~12.0 Ir) but never measured a workload that actually forces Tier-2 — a new `bench-internals`-gated `CONTAINS_BASE_TIER1_HITS`/`_MISSES` counter pair, plus (after two self-caught false-start workload designs, documented in full) a repeated-in-place-`realloc` Large-object rotation, found `OWN_CACHE_SIZE=4` thrashes COMPLETELY (0.00% hit rate) at every tested K≥4, including K=4 itself — raised to 16, confirming K≤8 jumps to 99.99% while K≥16 still thrashes (a structural property of the direct-mapped, non-associative cache design). Latency delta an honest noise-band null. `alloc-xthread` is in `production`. **Correction (same-day, zero-trust review): the report's claim that the standing ±10 raw-Ir kill gate "could not be measured, no Linux/Valgrind on this dev host" was independently found to be wrong — WSL was available.** Re-measured: the raw gate does not stay flat (+227 to +1,578 Ir across the 5 standing benches) but decomposes cleanly into a benign one-time `OWN_CACHE_SIZE` bootstrap cost (36-43 Ir, near-constant regardless of bench shape — the same signature task #496's `PerClass` finding had) plus a `bench-internals`-only Tier-1-counter cost (191-1,535 Ir, scales with call count, never ships in real `production`) — committed as a dated append-only §5.2 addendum with its own derive script rather than left standing. `cargo test --features production` green.
- **[runtime improvement, P2] R32-11 (task #502, F10) — `RemoteFreeRing::push` no longer reads the consumer-dirtied `head` cache line on every cross-thread free.** PERF-PASS-4 (task #52) already split `head` (consumer-only writes) onto its own 64-byte line separate from `tail`/`overflow` (producer-touched) — but `push` still read `head` via `Acquire` on every call, a real cross-core coherence cost PERF-PASS-4's own split did not remove. Added `cached_head: AtomicU32` in the ring's existing unused cursor-block padding (offset 72, same line as `tail`); the full-check now checks the shadow first and only falls through to the real `Acquire` load when the shadow suggests the ring might be full. Soundness formally re-derived by hand (a stale-low shadow can only ever make the fast path MORE conservative, never accept a push the real check would reject — true regardless of concurrent-producer store-reordering, since `cached_head` is only ever written from a once-real `head` value and `head` is provably monotonic) and independently verified via a new loom model (`RingModelShadow`/`RingModelShadow1`, 3 new tests among 8 total, including a `#[should_panic]` counterfactual proving the "always re-derive on the slow path" design is load-bearing) before trusting it. Built this project's first cross-thread producer/consumer wall-clock harness (three self-caught false starts along the way, documented in full, including a measurement-instrument-contamination bug where the harness's own path-activation counters made the fix look SLOWER until isolated into a separate timing-only build). Measured: favorable regime (owner drains promptly) −30% to −36% ns/push, 3/3 trials significant, sign test 0/20 every time; adversarial regime (owner drains rarely) −1% to −38% ns/push, direction-consistent across all 5 trials, 3/5 reach significance (the other 2 affected by confirmed shared-host contention). `alloc-xthread` is in `production`. `cargo test --features production` green; loom suite green (8/8).
- **[runtime improvement, P2] R32-12 (task #503, F8 sub-change 2) — large-cache scans no longer linear-scan for a free slot; shipped only the low-risk occupancy-bitmask half of a two-part proposal.** `AllocCore::large_cache_occupied: u64` replaces `large_cache_find_free_slot`'s `.position(|s| s.is_none())` scan with `trailing_ones()`. Correctness: exactly two functions in the whole crate ever write a large-cache slot (`large_cache_slot_set`/`large_cache_slot_take`, verified by grep), both now maintain the bitmask in lockstep — a new falsification-first invariant test caught two of its OWN false assumptions before either was mistaken for a real bitmask bug. Measured separately, same-regime discipline (cache genuinely near-full): native wall-clock A/B at `scan_bound=8` (production's actual base size) is a confirmed noise-band null; the Ir axis (much lower noise floor) shows the real, small, correctly-signed win the wall-clock couldn't resolve: −5.0 Ir/admission. Standing kill gate stays flat. **The higher-risk half (`usable_size`/`seq` replicated sidecar arrays) was deliberately NOT built** — the survey's own text flags this shape as the exact maintenance-cost failure mode that killed an earlier finding (X5, item 20), and the measured win at production's actual N=8 doesn't justify introducing a real replicated-field hazard. `alloc-decommit` is in `production`. `cargo test --features production` (and `+large-cache-extended`) green.

#### Measurement, correctness & tooling

- **[process] R32-2 (task #493) — made `scripts/verify-gate-report.mjs`'s corpus-wide verdict signal-bearing and its checks more precise; made `capture-measurement-identity.mjs`'s two identity forms provably equal.** The verifier's terminal line previously printed a bare `ALL GREEN` even when hundreds of WARNs had fired (masking ~350 pre-existing WARNs corpus-wide) — now prints `PASS WITH N WARNINGS (a=.., b=.., ...)` whenever any WARN fires, so a reader cannot mistake "no hard failures" for "no issues." Check (d) (prose↔CSV headline cross-check) is now unit-aware (`KiB`/`MiB`/`GiB` normalized to bytes via a new `UNIT_BYTE_MULTIPLIER` table) instead of bare-string-matching, which had produced false negatives on any report citing a number in different units in prose vs. CSV. Checks (e)/(f) (non-retroactive rules) are now scoped via `git merge-base --is-ancestor` against the exact commit that introduced each rule (mirroring `verify-commit-prefixes.mjs`'s existing technique), replacing an informal "predates the rule" judgment call with a mechanical one. `capture-measurement-identity.mjs`'s `patchSha256` is now computed via `git diff <headSha> <treeSha>` (two committed tree objects) instead of `git diff HEAD` (the live working tree) — guarantees the git-tree-SHA and patch-hash identity forms can never diverge; the patch is now actually saved to `docs/perf/_raw_identity_<tree-prefix>.patch` (new `.gitignore` rule), and `recoverCommand` fixed from a silently-broken `git show <tree>: -- <path>` to the working `git show <tree>:<path>`. Re-running the new verifier against the full corpus surfaced (and this task fixed) a real pre-existing bug: R32-0's summary CSV had a 7-character short SHA in its `landing_commit` column instead of the required 40-hex form — this exact bug class (short SHA where a full 40-hex SHA is required) recurred independently four separate times across this round's own new reports and was caught and fixed each time by re-running the report's own derive script with the correct full SHA. `npm run check` green.
- **[correctness fix, layout, P2] R32-5 (task #496, F4) — `PerClass` gains `#[repr(C)]`, restoring the documented one-cache-line magazine layout that was never actually in effect.** `PerClass`'s own doc comment (PERF-PASS-5, task #53) claims `count` sits directly adjacent to `slots` so a magazine push/pop touches one 64-byte cache line — but the struct had no `#[repr(C)]`, so it used Rust's unspecified default layout, and rustc's actual field-reordering heuristic (sort by descending alignment) put the 8-aligned `slots` array first and the 1-byte `count` LAST. Independently re-verified via a scratch `rustc -O` + `core::mem::offset_of!` probe before trusting the claim (not just assumed): `offset_of(count) == 128` under `production`, `130` with `virgin-zero-skip` — always a different 64-byte line from the shallow-magazine top-of-stack access the doc claims is colocated. Fixed with `#[repr(C)]` + declaring fields `count`, `virgin_mask` (cfg-gated), `slots` in that order, pinned with three compile-time `offset_of!` const-asserts (mirroring the file's existing `TCACHE_CAP <= 16` assert pattern) so a future field reorder or accidental `#[repr(C)]` removal fails the build instead of silently regressing again. Post-fix: `count` at offset 0, `virgin_mask` at 2, `slots` at 8, in both configurations — struct size unchanged at 136 bytes either way. Measured (WSL/callgrind, worktree-isolated): isolated 16-hit magazine-pop Ir delta exactly 0 in both configurations — matching the finding's own honest prediction ("same instructions, different addresses"); every SeferAlloc-side absolute Ir number moved by a uniform +755/+804 Ir constant, traced to `HeapCore::new()`'s one-time `PerClass::new()` zero-init codegen shift (confirmed via three structurally different churn benches moving by the identical constant, and mimalloc's own benches staying byte-identical), not a per-op cost. Lands on documentation-correctness grounds (`fix(perf)`, not a measured speedup) — restores an already-decided invariant at zero size/Ir cost, now enforced by compile-time assert instead of aspirational. `cargo test --features production` (and `+virgin-zero-skip`) green.
- **[measurement, P1] R32-6 (task #497, F1b) — implemented, correctness-verified, then measured, and honestly REJECTED a proposed dual-bitmap merge for the free path's two per-segment oracle bitmaps.** `AllocBitmap`/`MagazineBitmap` are two structurally identical 1-bit-per-granule bitmaps stored 32 KiB apart in every segment, so an own-thread small free always pays two `SegmentBitmap::locate` computations and two loads from two cache lines that can never be adjacent. Built a `DualBitmap` type packing both into one combined 64 KiB region (same total footprint), keeping both oracles' SEMANTICS fully independent (own bit, own set/clear operations — only the storage/addressing is shared) — confirmed this is NOT the semantically-different form a prior proposal (item 21/"G1") was rejected for. Correctness: full test tree green under `production` and `--all-features`, miri-verified on the relevant regression test, all four named pinned counterfactuals green. Measured (worktree-isolated, WSL/callgrind): every bitmap-touching bench REGRESSED 20-25x past the standing ±10 raw-Ir kill gate (small-`churn` benches moved +189 to +254 Ir; `cold_alloc_free_256x16b`/`recycle_alloc_free_256x16b` moved +899/+2,111) — root-caused to the combined layout's 4-granules-per-byte packing needing one more arithmetic step to locate a bit than the old 8-granules-per-byte layout, a tax paid by the far more numerous single-plane call sites (`pop_free`, `carve_batch`, etc.) that outweighs the saving at the two combined-read call sites the proposal targeted. Shipped as a fully-documented, zero-`src/`-diff NO-GO (the working tree was reverted to the base commit's exact state after measurement) — exactly the "measured, not assumed" discipline this backlog's evidence rules require, whichever way the number lands. `cargo test --features production` (and `--all-features`) green.
- **[process, measurement infrastructure] R32-9 (task #500, F3) — built the missing `>=64-live-segment` macro-benchmark harness four independent prior findings (X5/item 20, T10/item 22, R1/item 23, R15-1/item 9, all in `docs/perf/OPEN_ITEMS.md` item 34) had separately been blocked on.** Every pre-existing bench in this project spans at most ~3 live segments — deliberately, per CLAUDE.md's fast-dev-loop rule — but that means the project has had no instrument at all for any finding whose cost is a cache-line/TLB/coherence effect rather than an instruction-count effect. New `benches/macro_multiseg_steady_state.rs` (Linux-only `iai-callgrind` bench, single-thread and 4-thread variants) and `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs` (portable wall-clock companion, subprocess-per-arm isolated) build an 80-segment floor (25% headroom past the 64-segment threshold, each object one dedicated 4 MiB segment, held live for the whole timed region — genuinely too large for any realistic L2/L3) with steady-state mixed Small/Large churn on top. New path-activation oracle: `HeapCore::dbg_table_count()` (a thin, safe delegation to a pre-existing accessor — no raw-pointer derivation, so a plain safe `fn` is correct per CLAUDE.md's benchmark-hook rule), hard-asserted `>=64` right after the floor is built and before any timed churn, in both harnesses. Smoke-tested (wall-clock, this dev box): all 10 subprocess-isolated cells passed the oracle with `config_conflicts_delta = 0`; median ns/op 47.7 at 1 thread, 59.4 at 4 threads (sane contention-overhead direction, non-degenerate). **Honest gap disclosed, not glossed over: no Linux host was available this task to obtain the actual `Estimated Cycles`/RAM-hit iai-callgrind numbers** — flagged explicitly as follow-up work. Directly satisfies X5/T10/R1's own stated `>=64-segment` trigger; only partially satisfies R15-1's (the live-segment-count half, not its separate producer-class-fan-in half). `docs/perf/OPEN_ITEMS.md` item 34 updated; item stays OPEN since no mechanism was re-judged under the new harness, only the missing instrument built. `cargo test --features production` green; `cargo clippy`/`cargo fmt --check` clean across all CI feature-matrix rows.
- **[measurement, P1] R32-13 (task #504, F11) — ran the first Windows-native segment-reservation decomposition in this project's history, porting R29-3's existing Linux methodology natively.** `crates/vmem`'s Windows backend unconditionally reserves `size + align` (2x VA amplification for a 4 MiB segment) and — because Windows cannot partially release a `MEM_RESERVE` region — keeps the full over-reservation for the segment's whole lifetime; whether this costs measurable TIME (as opposed to just VA space) had never been measured. Step 1 (trivial): added `bench-internals`-gated Unix exact-mmap-fast-path hit/total counters (unmeasured on this Windows-only dev box, but now exists and is proven correctly wired) plus a Windows reserve+commit call-pair counter, extending the existing `SEGMENTS_RESERVED_TOTAL` pattern. Step 2 (the real deliverable): built three new `bench-internals`-gated hooks isolating `VirtualAlloc(MEM_RESERVE)` from `VirtualAlloc(MEM_COMMIT)` — R29-3's own "OS reserve+release round-trip" figure lumped these into one number, adequate for Linux (`mmap` commits eagerly) but too coarse for Windows, where they are unconditionally two separate syscalls. Measured natively (Windows 10 Pro, i7-11800H, 3 runs, N=200 each): avoidable (non-page-fault) share = 4.3-4.8% (median 4.60%) — well under a 20% materiality threshold, larger than Linux's 1.0-1.3% (R29-3) but still small; page faults dominate at ~95.4% on Windows too. New finding: `VirtualAlloc(MEM_COMMIT)` costs ~2x MORE than `VirtualAlloc(MEM_RESERVE)` (median 9,133 ns vs 4,580 ns, consistent across all 3 runs) — the opposite of the naive "reserve searches, commit is just accounting" intuition. **Step 3 (a `VirtualAlloc2` aligned-reservation fast path) explicitly declined** — the reservation path's cost is dwarfed by page-fault cost regardless of platform, and the survey's own scoping named this step "a real engineering change with a portability tail" not to be pursued without step 2's evidence justifying it. Path-activation oracle caught a real off-by-`WARMUP` counter-snapshot bug during development, before any wrong number was published. `docs/perf/OPEN_ITEMS.md` items 16/24 updated with both platforms' measured avoidable share; item 24's still-open Windows wall-clock signal explicitly NOT claimed to be explained by this finding (different workload regime), per the survey's own caution. `bench(perf)` — every new hook is `bench-internals`-gated with zero production callers; no production reservation policy changed. `cargo test --features production` green.
- **[docs] R32-14 (task #505, F5/F13) — indexed the two remaining survey findings that needed no code change, and built a full 14-entry cross-reference table for the whole survey.** F5: re-assessed the 16 KiB `SIZE2CLASS` LUT question against `docs/perf/OPEN_ITEMS.md` item 19 (X6) — REJECT still holds ("confirmed dead, and deader" than the original verdict); narrowed item 19's own revisit trigger from "a real-application cache profile showing SIZE2CLASS lines contending" to "a real application whose size distribution is dominated by scattered >=16 KiB small-class sizes," preserving the density argument inline (the LUT's index is dense from zero, so its hot region is exactly as small-size-dominated as the workload) so a future round doesn't have to re-derive it. F13: recorded three negative sub-findings as new item 39 `[L]` — (a) over-alignment classification (`class_for`'s align>16 walk): THIN, already optimized once (T10/perf#9); (b) TLS/`HeapRegistry` binding on the ordinary path: ALREADY MINIMAL, with one cheap optional future check flagged (a `cargo asm` disassembly of `SeferAlloc::alloc`'s Windows-MSVC prologue to confirm `LOCAL.try_with` lowers to a direct `#[thread_local]` access) — explicitly NOT a backlog task; (c) NUMA: OUT OF SCOPE for `production` (`numa-aware` isn't default), already independently re-verified once before (R25-9) after a stale re-flag. Built a new cross-reference section listing all 14 survey entries (F1-F13 + F1b) with one-line description, disposition, task #, and full 40-character commit SHA — every SHA re-verified against `git log --format=%H` before citing, not transcribed from memory or from a prior task's prose. `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` (deliberately left untracked all round for exactly this closing task) is now committed and tracked. Pure docs change — no `src/`/`benches/`/`examples/`/`tests/`/`Cargo.toml` touched. `cargo test --features production --test no_stale_doc_references` green (9/9).

### Round 30 — fixed a CONFIRMED soundness hazard a post-round R29 review flagged the same day it landed: a safe `pub fn` measurement hook could dangle the live small-segment bump-carve cursor (R30-1, task #450); then honestly corrected R29-9's own "closes the bug class for good" claim by widening its tripwire to be shape-independent, since R30-1's bug was exactly the shape the original scanner couldn't see (R30-2, task #451); then rebuilt `virgin-zero-skip`'s wall-clock judge with a path-activation oracle that caught a real design bug in this very task's own first attempt before shipping a number, and reached the project's first-ever genuine promotion DECISION for this feature: NO-GO for `production`, keep opt-in (R30-3, task #452); then independently re-verified all 8 measurement findings two readonly reviews had raised against Round 29's reports and closed `OPEN_ITEMS.md` item 28 — 1 REFUTED (a Callgrind "artifact" claim that does not hold), 5 CONFIRMED (an overstated "net loss" headline, a mean mislabeled as a median, an undisclosed full-touch-density assumption, an invalid 63-character sha256 citation, and a missing base-SHA citation), 2 PARTIAL (a plain 256/16 arithmetic error plus an apples-to-oranges absolute-vs-delta ratio; a correct-but-incomplete promotion-rate framing), all corrected append-only in their source reports (R30-4, task #453); then unified the CI/local feature-check matrix into one generated-from-manifest source of truth and added the two per-PR rows that would have caught both of Round 29's own build breaks (R30-5, task #454); then closed R29-13's own "missing benefit-side" trigger with a real-`#[global_allocator]` large-cache headroom A/B — 64 MiB preserves the FULL measured hit-rate benefit of the shipped 256 MiB default (byte-identical 100% hit rate) at ~7x less RSS retention, while 16/0 MiB both cost a real, reproducible 12.5-percentage-point hit-rate loss, and no headroom value shows a statistically significant latency difference through the real global allocator (R30-6, task #455); then turned R27-3/R27-4's and R30-6's hand-assembled builder recipes into discoverable named `Profile::{Rss,Balanced,Throughput}` presets and an explicit trim/scavenge API design proposal — and found, via a new application-shaped multi-thread A/B, that the throughput profile's headline `~22%` win does NOT reproduce as a statistically distinguishable effect at concurrent, mixed-size, continuous-churn scale, even though the underlying mechanism is proven activated there too (R30-7, task #456); then generalized the R26-4 config-sweep-evidence rule into a standing CLAUDE.md requirement that every performance judge also prove per-arm MECHANISM activation, not just config resolution — codifying the path-activation-oracle discipline R30-3/R30-6 already built, after tracing how R29-16's un-oracled wall-clock judge shipped a wrong "21.4x win" framing for a full round despite its CONFIG being correct throughout (R30-8, task #457); then required a gate report's tables and headline ratios to be DERIVED by one checked script from raw per-sample data, not hand-transcribed, closing the transcription/derivation defect class Round 29 shipped despite correct underlying measurements — a mean mislabeled a median, an inflated "30x"/"32x" from an absolute-vs-delta mismatch plus a plain arithmetic error, an undisclosed-denominator percentage, an invalid 63-character sha256 citation (R30-9, task #458); then evaluated, design-first, the reviewed architecture for isolating measurement hooks (one module + `bench-internals` gating + opaque typed handles + consume-on-destroy) against a hard measured constraint — full relocation of all 160 `dbg_*` hooks would touch 102-139 distinct `tests/`/`examples`/`benches/` files, 4-5x the ~26-file footprint R24-6/task #384 already declined for a SINGLE hook, and, independently, none of the five real incidents in this bug class (R25-1, R29-7, R29-8, R29-17, R30-1) were actually caused by file-level scatter — so declined full relocation outright and shipped a design-only proposal for the one piece with genuine, concretely-demonstrated value (a typed, consume-on-release segment handle for the one hook pair that currently mints and redeems a raw pointer), filed as a triggered follow-up rather than implemented this round (R30-10, task #459); then closed `OPEN_ITEMS.md` item 5's own `[P3]` sub-finding against R29-1's replacement invariant — confirmed the review right that the cumulative `segments_released_total <= segments_reserved_total` check has zero leak-detection power on its own (a missing release only makes it MORE true), left R29-1's correct logic untouched, and split the single over-claiming `canary_survives_promotion_and_free_leaves_no_leak` test into two `#[test]`s each named for exactly what it proves in every combination it compiles under — a renamed `..._no_double_release` test (always compiled, over-release only) and a new `..._leaves_no_leak_per_base` test (gated `alloc-decommit + alloc-xthread`, the genuine per-base leak proof), re-confirming both of R28-2's original break-and-revert non-vacuity paths against the restructured file (R30-11, task #460); then adopted commit/report-title tags that separate runtime speed from opt-in speed from measurement-only work, since three Round 29 commits used the bare `perf(...)` prefix for measurement-only work in an otherwise "Runtime improvements: 0" round (R30-12, task #461); then surfaced the per-heap memory-retention implication of the shipped 256 MiB large-cache default directly in the README quick-start and on `SeferAlloc::new()`'s own doc comment, instead of leaving it discoverable only ~400 lines into a gate report (R30-13, task #462); then, as the round's last (lowest-priority) task, compacted `OPEN_ITEMS.md`'s worst-offending accreted current-state cards down to their latest headline verdict (item 13 first, per its own "worst offender" flag, then the rest of the active list), closed a REAL zero-owner gap `FEATURE_PROMOTION_STATUS.md` had itself already documented for three CONDITIONAL-GO features (`exact-span-large`/`large-reserved-capacity`/`large-cache-extended`, none of which had a dedicated `OPEN_ITEMS.md` owner item until this task added items 28-30), and added a new structural test asserting every CONDITIONAL-GO/NEVER-DECIDED feature has exactly one owner item with a stated next trigger — confirmed non-vacuous against three independent mutations (zero-owner, duplicate-owner, dangling-item-number) (R30-14, task #463)

**Runtime improvements this round: 0.** The fix touches only `bench-internals`-gated measurement hooks and their one shared internal helper; `production`'s feature composition, default configuration, and every production allocation path are byte-for-byte unchanged (verified: the only production-reachable function touched, `AllocCore::reserve_small_segment`, still performs identically for its three production callers — it is now a one-line wrapper around a new `pub(super)` helper that does exactly the same work in the same order, just without the final cursor-publish inlined into the same function body).

**What moved.** `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 (filed by the R29 post-round independent readonly review, `docs/reviews/2026-07-29-r29-readonly-review.md`, and independently re-confirmed the next day by a second follow-up review, `docs/reviews/2026-07-30-r29-followup-readonly-review.md`) flagged that `AllocCore::dbg_decomp_full_cycle` — a safe `pub fn`, `bench-internals`-gated, added by R29-3 (task #434) to measure one decommit→reserve segment-lifecycle cycle — called `reserve_small_segment()`, whose LAST statement unconditionally publishes the freshly reserved segment as `self.small_cur` (the live bump-carve cursor every ordinary small allocation dispatches through), then immediately called `release_or_pool_empty_segment(base)` on that same segment. When the empty-small-segment hysteresis pool is already at capacity (exactly the state `examples/r29_3_decomposition_gate.rs`'s own pre-fill loop deliberately drives, by its own comment "not the pool-push path"), that release genuinely returns the OS reservation and recycles the table slot — leaving `small_cur` dangling at unmapped memory with nothing to restore it. The sibling pair `dbg_decomp_reserve_and_keep`/`dbg_decomp_release` had the identical hazard. R30-1 fixed it by taking the task's preferred option (a cursor-free measurement primitive, not save/restore or `unsafe fn`): `reserve_small_segment`'s body split cleanly into a new `pub(super) fn reserve_small_segment_impl` (everything except the final `self.small_cur = base` write, which was already isolated as the function's literal last statement) and a one-line cursor-publishing wrapper kept for the three production callers. Both `dbg_decomp_*` hooks now call the `_impl` variant, so they can never touch — and therefore can never dangle — the live cursor, however many times they run or how full the pool is. Added a new counterfactual integration test (`tests/r30_1_decomp_full_cycle_cursor_safety.rs`) that fills the pool, drives the release branch repeatedly through both hook shapes, then performs an ordinary alloc/write/readback/free on the SAME heap — genuinely verified non-vacuous by temporarily reverting the fix and observing the whole test process crash with `STATUS_ACCESS_VIOLATION` (a real Windows hard fault, exit code `0xc0000005`), not merely asserted to fail. Full `cargo test` under production+bench-internals is green (228 test binaries, 0 failures); `cargo clippy` clean under both `--features production` and `--features "production bench-internals"`; `cargo fmt --check` clean. Re-ran the R29-3 gate on its original WSL2/Linux measurement platform post-fix: the verdict is unchanged (trigger 2 still does not fire; avoidable overhead still a low-single-digit percentage, page-fault cost still dominates at ~97-98%) — a dated append-only §8 correction was added to `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` recording the re-measured numbers. That re-verification also surfaced a SEPARATE, pre-existing, unrelated finding (confirmed unrelated by isolating it: it reproduces identically with the fix applied or reverted, in a code path this task's diff never touches) — the same example crashes when run NATIVELY on Windows (as opposed to the WSL2/Linux platform it has always been measured on), because Windows `MEM_DECOMMIT` genuinely unmaps pages (unlike Linux `MADV_DONTNEED`, which keeps the VA mapping resident) and the example's Measurement B re-fault loop never issues the `VirtualAlloc(MEM_COMMIT)` recommit that platform requires. Filed as a new item 6 in `docs/CORRECTNESS_OPEN_ITEMS.md` for a future round rather than fixed in this pass, which stayed scoped to the `small_cur` hazard the task specified.

#### Runtime improvements

_None this round._ Correctness-only fix to `bench-internals`-gated measurement hooks; `production`'s feature composition and runtime behavior are unchanged.

#### Measurement, correctness & tooling

- **[correctness fix, P0] R30-1 (task #450) — fixed a CONFIRMED soundness hazard: `AllocCore::dbg_decomp_full_cycle` / `dbg_decomp_reserve_and_keep` + `dbg_decomp_release` could leave the live small-segment bump-carve cursor (`small_cur`) dangling at a released/unmapped segment, reachable from a safe `pub fn`.** Split `reserve_small_segment` into a cursor-free `reserve_small_segment_impl` (used by the two measurement hooks) and a thin cursor-publishing wrapper (kept for the three production callers) — the task's preferred fix option, chosen because the cursor-publish was already isolated as the function's literal last statement. Added `dbg_decomp_release`'s defence-in-depth `debug_assert!(base != self.small_cur, ...)`. New counterfactual test `tests/r30_1_decomp_full_cycle_cursor_safety.rs`, verified non-vacuous via an observed pre-fix `STATUS_ACCESS_VIOLATION` crash (not just asserted). Re-ran the R29-3 gate post-fix on its original WSL2/Linux platform — verdict unchanged, dated correction appended to `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` §8. Surfaced and filed (not fixed, out of scope) a separate pre-existing native-Windows crash in the same example's Measurement B, confirmed unrelated to this fix — `docs/CORRECTNESS_OPEN_ITEMS.md` item 6.
- **[correctness fix, P0, systemic] R30-2 (task #451) — the honest correction to R29-9's "closes the bug class for good" claim: widened `tests/dbg_hook_safety_tripwire.rs` to be shape-independent, since R30-1's bug was structurally invisible to the original scanner.** R29-9's scanner only matched safe `pub fn dbg_*` hooks whose signature TEXT contained `*mut`/`*const` — `dbg_decomp_full_cycle` (`&mut self -> bool`, zero raw-pointer parameters) sailed straight through it. Redesigned the policy into two shape-independent mechanical rules: (a) every crate-public `dbg_*` hook must be `bench-internals`-gated unless allowlisted as a pure observer; (b) every safe hook that mutates allocator state must be individually allowlisted with a one-line invariant justification, independent of whether it takes a pointer. `scan_file` no longer branches on `*mut`/`*const` at all — it enumerates every `pub fn dbg_*`/`pub unsafe fn dbg_*` regardless of shape. Rebuilt the allowlist from scratch across ~140 hooks in `src/` and `crates/`, reading each function body (not guessing from its name), into three buckets: `PURE_OBSERVERS` (read-only), `SAFE_MUTATORS` (each with a one-line justification — bounds check, delegation to the real production code path, or a correctness-inert policy/heuristic knob), and `UNSAFE_HOOKS` (already-`unsafe fn`, enumerated for exhaustive accounting, not a new safety argument). Two hooks surfaced as worth an explicit caveat rather than silent acceptance: `remote_free_ring.rs::dbg_set_cursors` and `heap_overflow.rs::dbg_reserve_unpublished_for_test` both mutate a real production ring's cursors under a "quiescent ring" precondition enforced only by a `debug_assert!` (compiled out in `--release`) — allowlisted with an explicit `[DEBUG_ASSERT ONLY]` tag (misuse can only corrupt the ring's own cursor bookkeeping, never dereference a caller pointer). Also fixed the separately-flagged `has_bench_internals_cfg` substring-match gap: replaced it with a small hand-written recursive-descent cfg-predicate parser (`syn` is not a dev-dependency — checked before hand-rolling) that correctly rejects `not(feature = "bench-internals")` (the opposite of gating) and `any(feature = "bench-internals", X)` (a permissive OR, not a genuine gate), while still accepting `all(...)`-nested genuine gates. **Non-vacuity proved directly**: a new test (`widened_scanner_catches_r30_1_shape_zero_arg_mutator`) feeds the scanner a synthetic in-memory fixture reproducing the exact pre-fix `dbg_decomp_full_cycle` shape (never touching real `src/`) and asserts the widened scanner finds it, classifies it safe/ungated, and confirms it is unallowlisted (i.e. the tripwire would fire) — then separately asserts the fixture's source text contains neither `*mut` nor `*const`, proving the OLD scanner would have missed it. Verification: the target `bench-internals`-inclusive feature combo green (4 tests); full `cargo test --features "production bench-internals"` green (0 failures); `cargo clippy --features "production bench-internals" --tests -- -D warnings` clean; `cargo fmt --check` clean. `docs/CORRECTNESS_OPEN_ITEMS.md` item 5's two P3 sub-findings marked FIXED with full detail. No production code changed — test-only file.
- **[measurement, P1] R30-3 (task #452) — rebuilt `virgin-zero-skip`'s wall-clock judge from scratch with a path-activation oracle, and reached a genuine promotion decision: NO-GO, keep opt-in.** Replaced R29-16's CONFIRMED-BROKEN `benches/r29_16_virgin_zero_skip_calloc_wallclock.rs` (its "virgin" scenario reused the same free list across thousands of Criterion iterations per sample, so it measured the recycled path for nearly the whole run) with a new custom `Instant`-timing-loop harness, `benches/r30_3_virgin_zero_skip_native_gate.rs` (Criterion has no first-class channel to report custom per-cell oracle data alongside timing, so this follows the project's existing `heap_fanin_persistent.rs`/`directory_threshold_probe.rs` precedent instead of `criterion_main!`). Every cell reports a PATH-ACTIVATION ORACLE built from the pre-existing `AllocCore::dbg_small_zero_pass_count()` counter (no new hook needed) — the single most important design point, because it caught a real bug during this task's own development: a first attempt at a 16-block virgin batch measured only 6.25% virgin-path activation and was rejected by the gate before any number could ship, tracing to `carve_block_with_refill`'s unconditional 31-block refill (Phase 9 amortisation) popping recycled-but-never-served blocks off the free list for all but the first call of any same-class burst — a genuine, load-bearing finding about the feature's real-world hit rate (~1-in-32 on a same-class calloc burst), not just a bench artifact. Corrected to a single-call-per-fresh-heap shape (`VIRGIN_BATCH = 1`, `VIRGIN_REPS = 50` to compensate with more independent samples); the corrected judge passes its own oracle at 100.00% minimum activation on all 48 ON-binary cells (4 sizes × 3 consumer-touch behaviors × eager/lazy `small-segment-lazy-commit`, each cell independently sampled). Swept the full 8-point design from the independent follow-up review (`docs/reviews/2026-07-30-r29-followup-readonly-review.md` §3): OFF/ON in separately built immutable binaries, fresh-heap-per-rep via `AllocCore::new()` directly (deliberately NOT `HeapRegistry::claim`/`recycle`, which reuses an already-materialised slot's segments/free-lists across "fresh" claims — the R26-4 same-process-reuse hazard one level up), 4/16/64/128 KiB × 3 touch behaviors (none/one-byte-per-page/full read-write), crossed with `small-segment-lazy-commit`, paired process-level native ns/op sampling with no Ir ratio restated as a speed claim. Native wall-clock result: no calloc-heavy workload shows a material, noise-distinguishable win at this sample size/host (virgin-scenario OFF-vs-ON deltas are sign-inconsistent, −43.8% to +19.9%, comparable to this host's own same-binary run-to-run noise of 4%–45%); the recycled/hot-churn family shows a small but direction-consistent regression (ON slower on 48/48 cells), attributed to the feature's own extra dispatch bookkeeping on its non-virgin path. **Verdict: NO-GO for `production` promotion, kept opt-in, recommended as a named narrow-profile feature** (one-call-per-class-per-heap or cross-class calloc patterns only, not same-class calloc bursts) — the project's first genuine DECIDED verdict for this feature after three prior rounds (R9-5, R11-8, R13-3) of NEVER-DECIDED. R29-16's iai isolation (§3, 3,067 vs 65,624 Ir, ~21.4×) stays valid as evidence that zeroing WORK was skipped — explicitly NOT re-derived or restated as a wall-clock claim anywhere in the new report. Full report `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` + `_summary.csv` + six `_raw_r30_3_*.log` files (base commit `50d5adc9e99f7817f88097901d7d0497fae53ea3`); `docs/perf/OPEN_ITEMS.md` item 25 and `docs/FEATURE_PROMOTION_STATUS.md`'s `virgin-zero-skip` section updated append-only to the DECIDED verdict; `docs/perf/OPEN_ITEMS_ARCHIVE.md` § D25 carries the full dated history. `cargo clippy --features "production bench-internals alloc-stats [virgin-zero-skip] [small-segment-lazy-commit]"` clean across all four combinations; `cargo fmt --check` clean.
- **[measurement/docs, P1] R30-4 (task #453) — independently re-verified all 8 unverified measurement findings `docs/perf/OPEN_ITEMS.md` item 28 had filed from the two R29 readonly reviews; result: 1 REFUTED, 5 CONFIRMED, 2 PARTIAL, all now closed with dated append-only corrections.** Item 28 had filed (a)-(h) directly from review prose without independent re-derivation. This task re-checked each against the cited source/raw logs: **(a) REFUTED** — R29-16's 21.4× Ir ratio is NOT a "Valgrind artifact"; `Node::zero` is a plain `write_bytes` memset and Callgrind's VEX-IR translation charges REP-prefixed string instructions once per loop iteration, so ~1 Ir/byte is EXPECTED behavior, corroborated by R29-16's own L1/L2/RAM cache-touch signature — no correction to the 21.4× figure, only an added Ir-vs-wall-clock methodological caveat. **(b) CONFIRMED** — R29-3 §5's "net loss" headline dismissed its own A′−B end-to-end result (consistent positive savings, 5.1%/1.7%, in both saved runs) as "noise" with no stated variance anywhere in the doc; corrected framing: workload-sensitive/not a priority, not a proven net loss. **(c) CONFIRMED** — `examples/r29_3_decomposition_gate.rs` computes arithmetic means (`total/N`) and labels them "median"; pure relabeling fix, numbers unchanged, example source untouched. **(d) CONFIRMED** — R29-3's "irreducible" arm touches all ~1,006 payload pages every cycle (full touch density); the report's unconditional verdict is now scoped to that regime, with sparse-density reversal named as unmeasured. **(e) CONFIRMED** — R29-13's cited sha256 is 63 characters (not 64, invalid), and a reconstruction attempt from its own base SHA does not reproduce it; filed as a permanent irreproducibility gap (not patched with a fabricated hash) — the first real-world test of the R29-6 immutable-provenance rule, which the citation failed on two independent grounds (wrong length, and using the rule's weakest-justified identity form for a working-tree state that was never preserved as a resolvable git object). **(f) CONFIRMED** — R29-16 cites no base SHA/immutable identity at all; per CLAUDE.md's explicit non-retroactive carve-out, noted as a permanent gap in that historical report rather than reconstructed. **(g) PARTIAL** — R29-13's "32x the small pool's 16 MiB byte cap" is a plain arithmetic error (256/16=16, corrected to 16×/8× for cap4/cap8); its separate "30x the small pool's ~8 MiB retention" compares an absolute floor (238 MiB) to R27-3's cap8-minus-cap4 DELTA (~8 MiB, confirmed by re-reading R27-3 directly), not an absolute-to-absolute ratio — no single unambiguous replacement ratio exists (different lifecycle points, different baselines measured), but two like-for-like absolute comparisons computed directly from each report's own tables give ~7.6×/~10.3×, both well below the retracted 30×. **(h) PARTIAL** — R29-5's 0.054%/0.82% and the review's proposed 82.5% (33/40 promotable objects) are all arithmetically exact; not an error, a framing gap — the report's own §1 discloses the 40-object population but never states or uses the 82.5% figure in its "RARE" headline; corrected framing added, explicitly noting this does NOT support rejecting Linux `mremap` for a promotion-heavy consumer workload. Dated `## 9. 2026-07-30 correction (R30-4, task #453)` sections appended (original numbers preserved, append-only) to `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`, `R29_13_LARGE_CACHE_RETENTION_GATE.md`, `R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md`, and `R29_5_PROMOTION_FREQUENCY_GATE.md`; `docs/perf/OPEN_ITEMS.md` item 28 rewritten with all 8 verdicts and marked RESOLVED. No `src/` file touched — pure documentation/arithmetic verification task, per its own scope.
- **[tooling, P1] R30-5 (task #454) — the feature/check matrix (which `clippy`/`check`/`test` invocations exist, with which feature strings) was reconstructed BY HAND independently in `.github/workflows/ci.yml`, `scripts/check-all.mjs`, and ad-hoc per-task verification; unified into one generated-from-manifest source of truth and added the two per-PR rows Round 29's own escapes proved were missing.** Root cause: three independently hand-maintained lists drifted apart, so "plain `production` clippy is in no CI row" and "the exact perf-gate default command is not a standalone check" both went unnoticed for a full round despite every task claiming (correctly, narrowly) that ITS OWN feature combination was tested. New `scripts/check-matrix.mjs` exports a small curated `PER_PR_ROWS` array (`{ id, kind, features, target?, note }`) — the single source of truth, edited nowhere else — currently 6 rows: the 4 pre-existing clippy rows (`""`, `experimental`, `--all-features`, `hardened medium-classes`, now declared once instead of duplicated) plus the two Round 29 escapes: `clippy --features production` (R29-4's `SegmentStateAccount`/`SegmentStateReconciliation` dead-code combo) and `check --bench perf_gate_iai --features "production bench-internals"` (`scripts/iai.mjs`'s own `DEFAULT_FEATURES` and `npm run check`'s final step — R29-16's 4x E0433). A shared runner, `scripts/run-check-matrix.mjs` (with a `--kind <clippy|check|test>` filter, repeatable), executes `PER_PR_ROWS` fail-fast and is invoked from BOTH surfaces so they can never drift again: `scripts/check-all.mjs` (local `npm run check`, which now generates its clippy steps AND its one non-clippy step directly from `PER_PR_ROWS` — every row runs exactly once) and a new `ci.yml` `check-matrix` job (`node scripts/run-check-matrix.mjs --kind check --kind test`, deliberately excluding `clippy` since the existing `clippy` job already runs all 5 clippy rows as named per-step Actions-UI entries — including a new `clippy (--features "production")` step this task added there — avoiding double-billing the same 5 combinations across two CI jobs; folded together with the stub check below in one job to avoid a second checkout+toolchain for a sub-second check). A GitHub Actions `strategy: matrix:` was considered and rejected — `PER_PR_ROWS` mixes three `cargo` subcommand kinds and a `--bench`-scoped row alongside whole-crate rows, which would need a second hand-maintained JSON `include:` list to encode in YAML, defeating the point; a script-invoked-in-a-loop keeps local/CI byte-identical from the same manifest instead. Also added the generated "feature ABSENT" check the task brief's item 4 calls for: `scripts/verify-perf-gate-stubs.mjs` parses `benches/perf_gate_iai.rs`'s `library_benchmark_group!` member list, finds every function gated on an opt-in feature (i.e. NOT part of `production`'s always-on set), and asserts a matching `not(feature = "X")` stub variant of the same function name exists — the mechanical, automatic form of the R24-8 stub-pattern rule R29-16 violated by omission. Run against the current tree: found 2 opt-in features gated inside the group (`batch-api`, `virgin-zero-skip`), both correctly stubbed — **0 violations** (R30-2/R30-3's recent commits did not touch the stub pattern, confirmed rather than assumed). Non-vacuity proved directly: ran the same script against a scratch copy with one `virgin-zero-skip` stub deleted and confirmed it fails with the exact diagnostic naming the missing stub, then discarded the scratch copy (never committed, never touched the real file). **CLAUDE.md tension resolved explicitly, as required by the task brief**: this does NOT smuggle the `cargo-hack --feature-powerset` sweep (308 invocations, already correctly weekly-only) into the per-PR path — the per-PR path gains exactly **2 NEW** `cargo` invocations total (`clippy --features production` in the `clippy` job, `cargo check --bench perf_gate_iai --features "production bench-internals"` in the new `check-matrix` job; the other 4 manifest rows already ran per-PR under `ci.yml`'s existing `clippy` job, just now declared once instead of twice, and are NOT re-run a second time by `check-matrix`'s `--kind` filter), plus one sub-second pure-text-scan script (no cargo invocation) — the `feature-powerset` job's `if: schedule || workflow_dispatch` gate and step count (4) are byte-for-byte unchanged in the diff. **Verification — the FULL `npm run check` was run end-to-end against the current tree (not just the new steps in isolation) and passed ALL GREEN**: argv-roundtrip, rustfmt, all 5 generated clippy rows (default/experimental/all-features/hardened+medium-classes/production), all 4 pre-existing `cargo test` combinations, the generated `check --bench perf_gate_iai --features "production bench-internals"` row, `verify-perf-gate-stubs` (PASS), and the real `npm run iai` WSL/Callgrind judge — 79 benches produced Ir, including the new `decomp_full_cycle_8x`/`decomp_os_roundtrip_8x` arms and both `virgin-zero-skip`/`batch-api` stub families, proving the whole pipeline compiles and runs correctly end-to-end, not just the isolated new commands. Also separately ran `node scripts/run-check-matrix.mjs` (unfiltered, 6/6 rows) and `node scripts/run-check-matrix.mjs --kind check --kind test` (1/1 row, matching the CI job's exact invocation) standalone. Validated `.github/workflows/ci.yml`'s YAML with `python -c "import yaml; yaml.safe_load(...)"` (PyYAML available on this host at a non-PATH interpreter path) and confirmed the `feature-powerset` job's trigger/step-count is unchanged; `cargo fmt --check` clean (no `.rs` file touched — pure `scripts/*.mjs` + `ci.yml` + `package.json` + docs task). New `npm run check:matrix` / `npm run check:stubs` convenience entry points added alongside the existing `npm run check`.
- **[measurement, P1] R30-6 (task #455) — the large-cache `headroom_bytes` BENEFIT-side A/B gate: the throughput/hit-rate judge R29-13's own retention-cost gate explicitly named as missing (`docs/perf/OPEN_ITEMS.md` item 27), mirroring R27-3/R27-4's established two-report pattern for the small pool.** Two entry points, per the structural constraint R27-4 already established for this exact problem (`SeferAlloc::with_config` bakes its `LargeCacheConfig` into a `static` at compile time): a subprocess-per-arm hit-rate/RSS probe (`examples/r30_6_large_cache_headroom_ab_gate.rs`, registry-bypass via `HeapRegistry::claim_with_config`, mirroring R29-13's shape but with a NEW mixed small+large workload and a burst→idle(1200ms)→burst sequence so a real, non-forced decay tick can fire — R29-13's own workload never let one fire mid-run) crossed 4 headroom values (0/16/64/256 MiB) × 3 thread counts (1/8/32) × 3 reps = 36 arms, ALL passing a path-activation oracle (admissions AND hits, extending R30-3's established oracle pattern with a NEW hits-oracle this gate needed and R29-13 did not); and four real-`#[global_allocator]` latency binaries (`r30_6_latency_h0`/`_h16`/`_h64`/`_h256`, one per headroom value, mirroring R27-4's `cap4`/`cap8` real-allocator split) driven through `scripts/paired-ab-runner.mjs`'s A/B/B/A protocol at the project's real-claim 20-pair threshold. **Result: 64 MiB ties 256 MiB EXACTLY on hit rate** (byte-identical 100.0% at every thread count: 8/8, 64/64, 256/256) **at ~7x less RSS** (R29-13's own measured post-drain floors: ~34-37 MiB/heap for 64 MiB vs ~238-241 MiB/heap for 256 MiB) — the 256 MiB default buys zero measured hit-rate benefit over 64 MiB at this gate's representative 48 MiB/burst workload. **16 MiB and 0 MiB both cost a real, reproducible 12.5-percentage-point hit-rate loss** (87.5% vs 100.0%, exact across all three thread counts, not noise) — explained mechanistically (§2 of the report) by `run_decay_step`'s whole-segment eviction granularity relative to this workload's ~6 MiB cached-span size. **Latency: no headroom value shows a statistically significant difference from the 256 MiB default** through the real global allocator (all three real comparisons AND the same-vs-same honesty control report `|t|` well under `crit(p<0.05)=2.101`, confirming the harness is not manufacturing a false positive out of this shared host's confirmed build contention during the measurement window). One `src/` addition: a single thin `HeapCore::dbg_large_cache_hits` delegation (exposing the pre-existing `AllocCore::dbg_large_cache_hits` accessor, following the exact established `dbg_large_cache_used`/`dbg_decay_config` pattern already in `heap_core_diag.rs` — no new `unsafe`, no raw-pointer parameter). **Recommendation recorded, not enacted**: option (c), named profiles (feeding directly into the already-queued R30-7/task #456) — a `throughput`/`balanced` profile at 64 MiB (full measured hit-rate parity, ~7x smaller RSS floor) and an `rss`-priority profile at a smaller value, explicitly disclosing the measured hit-rate cost rather than presenting it as free. `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` + `_summary.csv` + `docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log` + `docs/perf/_raw_r30_6_latency_ab.log` + four `paired_ab_runs/*.json` provenance files; `docs/perf/OPEN_ITEMS.md` item 27 updated append-only with the benefit-side result. No production default changed — `DEFAULT_HEADROOM_BYTES` (256 MiB, `src/alloc_core/large_cache_config.rs`) untouched. `cargo clippy --features "production alloc-stats bench-internals"` (hit-rate/RSS binary) and `cargo clippy --features "production alloc-stats"` (four latency binaries) both clean, `-D warnings`; `cargo fmt --check` clean.
- **[API + design + measurement, P2] R30-7 (task #456) — shipped named, discoverable `Profile::{Rss,Balanced,Throughput}` config presets; a trim/scavenge API design proposal; and an application-shaped cross-thread A/B for the throughput profile's headline win.** **Deliverable 1**: a new `#[non_exhaustive]` `Profile` enum (`src/alloc_core/profile.rs`, `alloc-decommit`-gated — the same gate its underlying knobs already require, no new gate invented) consumed by `LargeCacheConfig::for_profile(Profile)` and a new `SeferAlloc::with_profile(Profile)` const-fn constructor, each setting the small-pool pair (`pool_segments`/`pool_byte_cap`) AND the large-cache `headroom_bytes` together, coherently, per the R27-1 no-op-trap lesson: `Rss` = production default `(4, 16 MiB)` + 16 MiB headroom (12.5pp large-cache hit-rate cost explicitly disclosed in the doc comment, per R30-6); `Balanced` = production default `(4, 16 MiB)` + 64 MiB headroom (R30-6's full-hit-rate-parity point, zero additional small-pool cost); `Throughput` = `(8, 32 MiB)` + 64 MiB headroom (R27-4's measured small-pool win + R30-6's large-cache parity point). None of `SeferAlloc::new()`'s defaults changed. New `tests/profile.rs` (4 tests) proves each profile's RESOLVED config (via `AllocCore::dbg_pool_cap()`/`dbg_decay_config()`, not the requested builder value) matches its documented claim, including a general counterfactual that no profile silently reproduces the R27-1 no-op. **Deliverable 2**: a new README profile-comparison table citing R27-3/R27-4 (small pool) and R30-6 (large cache) verbatim, with an added honest caveat pointing at Deliverable 4's null result. **Deliverable 3**: `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` — a design-only proposal (no `src/` change) for an explicit, caller-driven `SeferAlloc::trim_current_thread()`, promoting the existing `#[doc(hidden)]` test-only `dbg_trim_current_thread` hook (already wired to the exact right primitive, `HeapCore::trim_for_recycle`) to a real public API. Explicitly differentiated from R27-5's adaptive-pool-budget design (NOT built, partly because idle shrink-back was unsolved within the no-background-thread constraint): an explicit caller-driven call sidesteps that constraint by having the APPLICATION supply the one piece of information (the phase boundary) that made R27-5's automatic inference hard in the first place, rather than asking the allocator to guess it from allocation-pattern shape alone. **Deliverable 4**: built one additional multi-thread, mixed-size, continuous-cycle "server request handler" A/B (`examples/r30_7_throughput_profile_server_ab_default.rs` / `_throughput.rs`, shared workload `examples/_shared/r30_7_server_shaped_workload.rs`, `include!`d verbatim into both — 8 threads, 4-size mix, 6 continuous rounds, peak per-thread working set deliberately calibrated to ~24 MiB to exceed the default pool's 16 MiB ceiling) to test whether R27-4's single-threaded single-shot ~22% win holds at application scale. **Result: it does NOT reproduce as a statistically distinguishable effect** — paired A/B/B/A (20 pairs, `scripts/paired-ab-runner.mjs`): `t=-0.119` (mean Δ=-7.44ms, nominally favoring `default`), sign split 12/8; a same-vs-same honesty control shows the identical noise-band shape (`t=-1.039`, sign 11/9), confirming a genuine null, not a harness artifact. The pool-overflow mechanism under test IS proven activated (`decommit_calls_total=40`, non-zero, in every one of the 40 `default`-arm launches) — this is not a vacuous "never touched the mechanism" null. `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` + `_summary.csv` + two `_raw_*.log` files + two `paired_ab_runs/*.json` provenance files. No production default changed anywhere in this task. `cargo clippy --features production -- -D warnings` and `cargo fmt --check` both clean; `cargo test --test profile` green (4/4).
- **[docs, P2] R30-8 (task #457) — generalized CLAUDE.md's R26-4 config-sweep-evidence rule into a standing requirement that every performance judge also prove per-arm MECHANISM activation, not just config resolution.** R26-4 already required proving an arm ran under its labelled config (requested/resolved/conflict-delta/process-identity); it never required proving the arm exercised its labelled CODE PATH — the gap that let R29-16's un-oracled wall-clock judge (task #447) ship a wrong "21.4x speed win" framing for a full round even though its `virgin-zero-skip` feature flag was correctly on/off as labelled throughout (the CONFIG was never wrong; the free-list-pop-first dispatch order silently swapped the CODE PATH measured after the first call per sample). New CLAUDE.md bullet, placed immediately after the R26-4 bullet it extends, names concrete mechanism classes (virgin bump-carves vs. recycled free-list pops, large-cache hits vs. misses, decommit/release/reserve counts, promotion events, directory hits vs. fallback scans, pool-cap-resolved-AND-victim-activated as the R26-4 boundary case) and cites R30-3/task #452's rebuilt judge (`benches/r30_3_virgin_zero_skip_native_gate.rs`) as the reference implementation — its oracle caught a SECOND real bug during its own development (a `VIRGIN_BATCH=16` design that only achieved 6.25% virgin-path activation, rejected before any number shipped) — plus R29-13/task #444 and R30-6/task #455 as evidence the discipline was already being adopted voluntarily before this rule existed. Explicitly non-retroactive, matching this file's established convention for other rules. Pure documentation change — no `src/`/`tests/`/`benches/` file touched.
- **[docs, P2] R30-9 (task #458) — new CLAUDE.md rule requiring a gate report's tables and headline ratios to be DERIVED by one checked script from raw per-sample data, not hand-transcribed or hand-computed; complements, does not replace, the existing raw-log/summary-CSV bullets.** Targets the transcription/derivation defect class Round 29 shipped despite CORRECT underlying measurements: an arithmetic mean mislabeled "median" (`examples/r29_3_decomposition_gate.rs`, R30-4 finding c), an absolute 238 MiB retention floor compared against an incremental ~8 MiB delta yielding an inflated "30x" (R29-13, R30-4 finding g), a plain 256/16=16 arithmetic error published as "32x" (same finding g), a whole-workload 0.054% denominator presented without its far more decision-relevant 82.5%-of-promotable-population counterpart (R29-5, R30-4 finding h), and a real end-to-end saving dismissed in prose as "noise" with no stated variance (R29-3 §5, R30-4 finding b). Seven concrete, mechanically-checkable points: raw per-sample JSON/CSV as primary output before any prose; summary CSV + Markdown tables derived by one script, not retyped (citing R29-3's own landing commit `db35617`, whose message records zero-trust review catching prose that matched neither saved raw log, plus sub-component CSV transcription typos, in the same pass); statistic-name strings printed by the computing code, not retyped independently; every percentage states its numerator and denominator inline; absolute vs. delta retention labeled distinctly; a headline ratio's generating script must assert the arithmetic it prints (e.g. `assert(a / b == stated_ratio)`), not just print a hand-computed string; an immutable source identity (per the existing R29-6 rule) must be captured or computed AT measurement time, not reconstructed afterward from a stated recipe — citing R29-13's invalid 63-character sha256 citation, whose reconstruction attempt from its own stated recipe produced a genuinely different hash (R30-4 finding e, commit `575f3a8`), as the first real-world failure of the R29-6 rule. Explicitly non-retroactive: R29-3/R29-5/R29-13/R29-16 are not regenerated through a script pipeline (already append-only corrected by R30-4). Pure documentation change — no `src/`/`tests/`/`benches/` file touched.
- **[design, P2] R30-10 (task #459) — design-first evaluation of the reviewed "isolate measurement hooks as a distinct subsystem" architecture (one module/crate + `bench-internals`-only compilation + opaque typed handles + consume-on-destroy), against the recurring R25-1/R29-7/R29-8/R29-17/R30-1 bug class; DECLINED full relocation on measured cost/value grounds, shipped a design-only proposal for the one piece with concretely-demonstrated value.** Enumeration reused `tests/dbg_hook_safety_tripwire.rs`'s own R30-2 allowlists (no re-derivation): **160 total `dbg_*` hooks** — 99 `PURE_OBSERVERS`, 39 `SAFE_MUTATORS`, 22 `UNSAFE_HOOKS` — living in 18 files under `src/`+`crates/`. Call-site survey (`grep -rl` per hook name across `tests/`/`examples/`/`benches/`): relocating `SAFE_MUTATORS`+`UNSAFE_HOOKS` alone (the task brief's own "genuinely hazardous" priority bucket) would touch **102 distinct files**; adding `PURE_OBSERVERS` brings the union to **139 distinct files** — against this crate's 227 total test files. `dbg_push_to_ring` alone reproduces the "~20" R24-6/task #384 already cited when it declined to re-gate that ONE hook for exactly this reason ("a documentation-precision concern rather than a regression"); this survey's 102-139-file footprint for a FULL relocation is 4-5x that already-rejected single-hook case. **Independently, and more decisively: none of the five real incidents in this bug class were caused by file-level scatter.** Reading each of the five fixes (R25-1/R29-7/R29-8/R29-17: safe `pub fn` → `unsafe fn` + validated pointer; R30-1: routed through a new cursor-free `reserve_small_segment_impl` primitive, in the SAME file) shows every one was resolved by changing what the hook's BODY does or what its SIGNATURE guarantees — never by moving code to a different file — so a relocation-only architecture would not have prevented any of the five incidents it is nominally meant to address. Full relocation and the "compile only under `bench-internals`" piece (already true for `UNSAFE_HOOKS`; declined to extend to `SAFE_MUTATORS`, out of this task's scope) declined. **The typed-handle/consume-on-destroy piece was found genuinely valuable via a live counterexample, not hypothesized**: `dbg_decomp_reserve_and_keep`/`dbg_decomp_release` (`src/alloc_core/alloc_core_small_pool.rs:1070-1115`) already mint-then-redeem a bare `*mut u8` segment base guarded only by a `debug_assert!` (compiled out in `--release`) against releasing the live `small_cur` cursor — the exact R30-1 hazard class, still standing on a weaker backstop for this one pair. `docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §5 sketches a concrete `ReservedSmallSegment` handle (private field, `pub(crate)`-only constructor so a forged handle is uncomputable, `release(self)` by value so double-release is a compile error (E0382), not a runtime hazard) for this ONE pair — NOT implemented this round (the retrofit's ~5-file diff, including R30-1's own counterfactual regression test, deserves its own review as the first typed-handle pattern in this codebase, not a same-task rubber stamp alongside the document proposing it). Filed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 7, triggered by either a 6th bug-class instance or a second mint-then-redeem `dbg_*` pair appearing. No `src/`, `Cargo.toml`, `tests/`, or `benches/` file touched by this task — design document + two doc-index updates only.
- **[correctness/docs, P2] R30-11 (task #460) — `docs/CORRECTNESS_OPEN_ITEMS.md` item 5's `[P3]` sub-finding against R29-1's replacement invariant: confirmed the review right, split the over-claiming test name, left R29-1's logic untouched.** R29-1 (task #432) correctly replaced a windowed `released_delta <= reserved_delta` guard (a real ~0.3% window-crossing false positive) with the lifetime-cumulative `segments_released_total <= segments_reserved_total` invariant — that fix's LOGIC is correct and this task does not touch it. But the item 5 review flagged the cumulative check as "near-unfalsifiable": it proves no impossible double/over-release occurred, but a MISSING release only makes it MORE comfortably true, so it has zero leak-detection power on its own — confirmed exactly right. The defect was that `tests/r14_4_promotion_free_correctness.rs`'s single combined test function kept the name `canary_survives_promotion_and_free_leaves_no_leak` in EVERY feature combination it compiled under, including the CI-tested `hardened medium-classes` row (`alloc-global + alloc-xthread`, WITHOUT `alloc-decommit`), where only the cumulative check exists — the genuine per-base leak proof (`dbg_live_count_for`, `alloc-decommit`-gated) does not compile there at all. Split into two `#[test]`s via a shared non-test `alloc_grow_and_verify_canary` setup helper (alloc + canary stamp + grow + canary-survival check, stopping before either test's own free/assert sequencing): `canary_survives_promotion_and_free_no_double_release` (always compiled; canary + the renamed `no_over_release` cumulative invariant, in both the local variable name and the assertion failure message + no corruption — never claims "no leak") and `canary_survives_promotion_and_free_leaves_no_leak_per_base` (new name, same pre-existing gate `alloc-decommit + alloc-xthread`, byte-identical assertion logic to the block it replaced). **Investigated rather than assumed whether a weaker-but-real diagnostic exists for `hardened medium-classes`** (deliverable 3 of the task): confirmed none does — `dbg_contains_base` alone (available under `hardened`) cannot substitute for `live_count`, because without `alloc-decommit` the small-segment release/pool machinery itself (`dec_live_and_maybe_decommit` / `dec_live_batch_and_maybe_decommit`, `src/alloc_core/alloc_core_small_pool.rs`) is entirely `#[cfg(feature = "alloc-decommit")]` — small/medium segments are never released or live-count-tracked under that combo, so `dbg_contains_base` would read `true` forever regardless of a real leak. Documented as an honest, explicit gap (module doc + the `no_double_release` test's own doc comment), not silently accepted. **Re-confirmed both of R28-2's original break-and-revert non-vacuity paths against the restructured file**, per the task's mandatory verification step: Large-promoted path (`production medium-classes`) — disabled `self.table.unregister(base)` in the cache-admitted leg of `AllocCore::dealloc`'s Large branch (`src/alloc_core/alloc_core.rs`) — reproduces R28-2's own documented alternate outcome at this exact site, a deterministic `STATUS_ACCESS_VIOLATION` (both release and debug profiles: the segment becomes genuinely double-owned and `dbg_trim_current_thread`'s `evict_all` double-frees it) — still a detected, non-vacuous `cargo test` failure; reverted cleanly (`git diff` empty on `src/`), passes again. Medium-ladder path (`production medium-classes exact-span-large`) — disabled the `dec_live_batch_and_maybe_decommit` block in `flush_run` (`src/alloc_core/alloc_core_small_magazine.rs`) — clean assertion failure, `live_count went from Some(2) to Some(2)`, the exact "no change at all" signature the assertion's own doc comment predicts; reverted cleanly, passes again. Verified personally: `hardened medium-classes` (2 tests, no per-base test present), `production medium-classes` and `production medium-classes exact-span-large` (3 tests each, per-base test present and passing) all green; `cargo clippy --features "hardened medium-classes" --all-targets -- -D warnings` and `cargo clippy --features "production bench-internals" --all-targets -- -D warnings` both clean; `cargo fmt --check` clean. No `src/` behavior change — test-only file; the two counterfactual breaks used for non-vacuity verification were both reverted before this commit. No version bumps.
- **[docs, P2] R30-12 (task #461) — new CLAUDE.md rule: `perf(...)` commit-subject prefixes are reserved for commits that change what ships; measurement-only work gets `bench`, opt-in-only code changes get `perf(opt-in)`, config documentation with no code change gets `docs(config)`.** Round 27, 28, and 29 were all "Runtime improvements this round: 0" (verified against their own CHANGELOG headers, lines 82/67/36), yet individual Round 29 commits — `79aad56`, `894e9e3`, `7c2c62d` (verified: each adds only a new `bench-internals`-gated diagnostic and/or example/bench file, none touches `production`'s `Cargo.toml` composition, each commit body states "No production default changed" or equivalent) — used the bare `perf(...)` prefix, which conventionally signals a runtime-performance change to anyone reading `git log` alone. New four-way taxonomy (`perf(runtime)` / `perf(opt-in)` / `bench` / `docs(config)`) extends, not replaces, CHANGELOG.md's existing bullet-tag convention (`[measurement]`/`[correctness fix]`/`[process fix]`/`[docs]`/`[CI]`, already working one level up at the changelog-entry level) by filling the same honesty gap one level down at the raw commit-subject level. `bench` (not `measurement`) chosen as the measurement-only prefix — 19 prior commits already use it in this project's history, `measurement(...)` has zero. Same distinction applies going forward to new `docs/perf/R*_....md` gate-report titles. Explicitly NOT a history rewrite — no existing commit is retagged, citing the R14-10 non-retroactive raw-log-truncation rule and the R24-6 `dbg_push_to_ring` decision as this file's established precedent for declining exactly this kind of retroactive cleanup. This commit itself is the first real-world application: a pure CLAUDE.md/CHANGELOG.md convention change with no runtime and no measurement-harness code touched, so it uses `docs(claude)` rather than any `perf(...)` prefix, matching R30-8/R30-9's own commit prefixes for the same class of change. Pure documentation change — no `src/`/`tests/`/`benches/`/`Cargo.toml` file touched.
- **[docs, P2] R30-13 (task #462) — surfaced the per-heap memory-retention implication of the shipped 256 MiB large-cache default directly in the README quick-start and on `SeferAlloc::new()`'s own doc comment, instead of leaving it discoverable only ~400 lines into R29-13's gate report.** A post-round review (source of this task) flagged that R29-13's already-measured, already-CONFIRMED facts — 256 MiB headroom is PER MATERIALIZED HEAP/SHARD (not process-wide), idle reclaims exactly 0 KiB at every headroom setting across all 36 measured arms, and the only unconditional reclamation path is thread exit (`HeapCore::trim_for_recycle`) — were real and correct but effectively invisible to a first-time reader of the top-level docs. New README `### Memory policy` subsection, placed immediately after the `Basic usage` quick-start code block (before, not after, the existing `Named profiles` section it now cross-links both ways): a blockquote callout spelling out the "per materialized heap/shard" multiplier in concrete terms (a 32-heap server retains on the order of 32×256 MiB, not 256 MiB total) and the idle-does-not-reclaim fact, framed as a real measured trade-off (large-object caching genuinely helps hit rate — R30-6's own finding that 64 MiB ties 256 MiB on hit rate is cited, not omitted) rather than a warning to avoid the allocator. Points to `SeferAlloc::with_profile(Profile::Balanced)` as the concrete, already-shipped one-line remedy — chosen over `Profile::Rss` because `Balanced` carries no disclosed hit-rate cost (R30-6: full 100.0% parity with the 256 MiB default at ~7x less RSS), making it the cleaner first answer to "what do I do about this," with `Profile::Rss` and hand-rolled `LargeCacheConfig::headroom_bytes` named as further options. Code snippet uses a ` ```text ` fence per this project's no-doctests rule. Cross-links `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` for full methodology rather than restating its raw numbers. Mirrored, consistently, as a new "Memory policy" doc-comment section directly on `SeferAlloc::new()` (`src/global/sefer_alloc.rs`) so `cargo doc`-generated API docs carry the same warning even for a reader who never opens README.md. **Does not change any default** — `DEFAULT_HEADROOM_BYTES` (256 MiB, `src/alloc_core/large_cache_config.rs`) untouched; this task documents current shipped behavior only, deliberately not gated on R30-6's own data feeding a future default-change decision. Verification: `cargo clippy --features production -- -D warnings` and `cargo fmt --check` both clean (no `doc_lazy_continuation` hazard — the new doc comment has no line starting with a literal `+`/`-`/digit); `cargo test --features "production bench-internals alloc-stats" --test no_stale_doc_references` green (8/8, including `readme_unsafe_inventory_counts_match_reality`, unaffected by this addition since no `#[allow(unsafe_code)]` site was touched).
- **[docs/process, P3] R30-14 (task #463) — the round's last (lowest-priority) task: compacted `OPEN_ITEMS.md`'s worst-offending accreted current-state cards, closed a real zero-owner gap for three CONDITIONAL-GO features, and added a structural test asserting every undecided feature has exactly one owner with a stated next trigger.** `docs/perf/OPEN_ITEMS.md` went **470 → 548 lines** (net +78) — NOT a pure shrink: item 13 (the task brief's named "worst offender," several hundred words of accreted round-by-round correction narrative — R25-5 → R26-1 → R26-2 → R26-3 → R27-1 → R27-2 → R27-3 → R27-4 → R27-5, all already byte-identical in `OPEN_ITEMS_ARCHIVE.md` § `A13`, independently re-verified fact-by-fact before compacting) shrank from 12 lines of accreting Status/verdict/trigger prose to 5 lines stating only the LATEST headline, with the archive link doing the rest; item 27's stale "R30-7 is UNBLOCKED... and should use..." future-tense next-trigger was corrected to state what actually shipped (R30-7 already consumed the 64 MiB figure into `Profile::Balanced`/`Profile::Throughput`). Those savings were outweighed by a genuine content ADDITION: `docs/FEATURE_PROMOTION_STATUS.md` (R29-12, task #443) had already documented, in its own text, that three shipped CONDITIONAL-GO features — `exact-span-large`, `large-reserved-capacity`, `large-cache-extended` — had ZERO dedicated `OPEN_ITEMS.md` owner entries (each was only a passing reference inside an unrelated item), the exact R18-8/R22-3 zero-owner failure mode CLAUDE.md's own "Round start" bullet warns about. Added three new `[D]`-tier owner items (28/29/30), each citing its existing gate report (`R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §7, `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` §5 + `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` §6, `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §9 — no report file touched, no new measurement run) with a stated next trigger, and updated `FEATURE_PROMOTION_STATUS.md`'s survey table + its own "Other features found in a SIMILAR (less sharp) shape" section to point at them. New test `every_undecided_feature_has_exactly_one_owner_with_a_next_trigger` (`tests/no_stale_doc_references.rs`, this project's established plain pipe-table-scan style, matching `readme_unsafe_inventory_counts_match_reality`'s approach) parses `FEATURE_PROMOTION_STATUS.md`'s survey table (chosen over `OPEN_ITEMS.md` as the authoritative CONDITIONAL-GO/NEVER-DECIDED source — `OPEN_ITEMS.md`'s own occurrences of those strings are almost all about deferred DESIGN proposals, not shipped Cargo FEATURES) and asserts every row whose OWN verdict cell carries `CONDITIONAL-GO`/`NEVER-DECIDED` cites exactly one `OPEN_ITEMS.md item N`, that the cited item exists, and that it has a stated `Next trigger:` bullet — plus a cross-row check that no two independently-flagged features share the same owner item (the R13-9-pattern duplicate-owner class, honestly noted in the test's own doc comment as reasoned from the general pattern CLAUDE.md cites R13-9 for, not a second literal citable instance of THIS exact check's failure). The `alloc-lazy-commit` alias row (verdict text "reduces to `small-segment-lazy-commit`", not CONDITIONAL-GO/NEVER-DECIDED itself) is excluded from the scan by construction, so its intentional shared item-26 citation with `small-segment-lazy-commit` is not flagged. **Non-vacuity confirmed against three independent mutations** (temporarily applied then reverted, verified via `git diff` clean afterward): removing an owner citation (zero-owner) → FAILED with the exact diagnostic; pointing two features at the same item number (duplicate-owner) → FAILED; citing a nonexistent item number 999 (dangling reference) → FAILED; each reverted and the clean state re-confirmed passing. **Scope-boundary cross-check (deliverable 4):** confirmed R30-1 (the `small_cur` correctness fix) is owned exclusively by `docs/CORRECTNESS_OPEN_ITEMS.md` (zero mentions in `OPEN_ITEMS.md`) and R30-4 (the perf-report measurement-methodology re-verification) is owned exclusively by `docs/perf/OPEN_ITEMS.md` (zero mentions in `CORRECTNESS_OPEN_ITEMS.md`) — no drift, no duplication across the two indexes' documented scope boundary. Full `tests/no_stale_doc_references.rs` suite green (9/9, confirming the compaction did not break any pre-existing tripwire in the same file — `honest_reject_sections_are_indexed` in particular, since it also scans `OPEN_ITEMS.md`); `cargo clippy --features "production bench-internals" --tests -- -D warnings` clean; `cargo fmt --check` clean. No `docs/perf/R*.md` gate report touched (append-only historical record preserved); no `src/` file touched; no version bumps.

**Round 30 review-response closing note (2026-07-30).** An independent full-round review (`docs/reviews/2026-07-30-r30-full-review.md`) found 4 P1 findings and 11 P2 findings against the round above. All 4 P1s were independently re-verified against the committed raw data (not just trusted from the review) and confirmed real, then fixed append-only in the same-day follow-up work below — no entry above this note was rewritten, only corrected via new dated sections in the affected report files and this one closing bullet:
- **P1-1 — R30-3's "48/48 direction-consistent regression, ON ALWAYS slower" claim (this section's own R30-3 bullet above, and `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` §5.2) is corrected: independently recomputed directly from the report's own committed summary CSV, the true recycled-cell picture is 19/24 ON-slower on the mean (not 24/24; 20/24 on `p50`), full range −32.1%..+136.8% (not the eager-arm-only +2.1%..+84.1% the report stated).** Dated correction appended to `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` §6 (immediately after the original §6 text, unchanged). This bullet's own text above ("ON slower on 48/48 cells") is likewise left unedited per this file's append-only convention — read it together with this note. The NO-GO verdict is unaffected (its primary justification, the sign-inconsistent virgin-scenario null, does not depend on this claim).
- **P1-2 — R30-7's "the pool-overflow mechanism IS proven activated" framing (this section's own R30-7 bullet above, and `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` §0) is corrected: the report only ever printed the `default` arm's `decommit_calls_total`; re-parsing the raw log shows the `throughput` arm reports the IDENTICAL value (40) in every one of its own 40 launches — the mechanism the profile exists to eliminate was not eliminated in this workload, a materially different and more important finding than "the workload touches the mechanism at all."** New §0.1 appended to `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` with the per-arm numbers and a corrected "hypothesis 0" ahead of §2's three noise-based hypotheses; `README.md`'s "even though the underlying pool-overflow mechanism is proven activated in that workload too" corrected in place to state the mechanism fires identically in both arms, so this workload does not separate them.
- **P1-3 — R30-7's same-vs-same control (§0's table) is not shown to characterize the real comparison's noise, and the report never stated its minimum detectable effect.** Independently recomputed: `MDE = crit(p<0.05)=2.101 × se(62.39 ms) ≈ 131.1 ms`, ≈18.8% of the real comparison's own ~697 ms combined-arm mean (695 ms per the report's own rounding); 95% CI on the mean Δ is `[-138.5, +123.6] ms`; control mean ≈171 ms vs real-comparison mean ≈695-697 ms (a ~4x gap, confirming the two runs were not taken under comparable host load). Note appended near `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` §0's table stating the MDE/CI and that the control validates the harness's self-consistency more than the real comparison's noise floor; §4's "workload-shape-dependent" framing and README's "Treat the `~22%` figure as workload-shape-specific" softened to acknowledge this is an UNDERPOWERED null (cannot rule out an effect up to ~19%), not a confirmed absence of effect. No re-measurement performed — out of scope for this correction pass.
- **P1-4 — two of the round's three new gate reports (`R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md`, `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`) did not satisfy the pre-existing R29-6 immutable-source-identity rule** (R30-3 deferred its own landing-commit citation to `CHANGELOG.md`, which in fact only ever recorded the *base* SHA `50d5adc9e9…`, a commit at which the bench file this report depends on does not yet exist in the tree; R30-7 cited "base SHA + uncommitted working tree", the exact pattern the rule forbids) — mirroring the exact gap R30-6 needed its own SHA-fill-in follow-up (`1272a52`) to close. Two follow-up commits, in the same one-line chicken-and-egg-fill-in style as `1272a52` (R30-6) and `9335979` (R30-11): `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md`'s header now cites its real landing commit `d8f467b869c226150746532c484944958ee31808`, and `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`'s header now cites `b5efe8ce6099d33987f7811edc4f2411686d9bfc`; both summary CSVs gained a new `landing_commit` column alongside their existing (and still-correct, left untouched) `commit_sha`/`commit` columns, since no `scripts/` file was found to parse either CSV's SHA column (grepped before deciding to add rather than overwrite).

The 11 P2 findings (lower priority, none a correctness/soundness issue) were filed — not fixed — as new dated entries in `docs/CORRECTNESS_OPEN_ITEMS.md` and `docs/perf/OPEN_ITEMS.md`, citing `docs/reviews/2026-07-30-r30-full-review.md`, for a future round to independently verify and action. No `src/` file was touched by this review-response pass.

### Round 29 — two long-standing measurement gaps closed (medium→Large promotion is provably rare; the large-cache's 256 MiB headroom retains ~238–241 MiB/heap, 30x the small pool's proven floor), `virgin-zero-skip`'s missing Stage-0/Stage-3 gate finally run (real 21.4x Ir win at the instruction level; the wall-clock arm's own bench design was later found broken, verdict UNCONFIRMED not negative), a P0 flaky-test root cause found and fixed, `OPEN_ITEMS.md` split 1581→426 lines with a new archive file, four raw-pointer `dbg_*` hooks made safe (three re-gated `unsafe`+`bench-internals`, one fixed with a containment guard instead) closing the R25-1 bug class for good, a systemic tripwire added so a fifth instance can't land unnoticed again, and a post-round independent readonly review (`docs/reviews/2026-07-29-r29-readonly-review.md`) that found and fixed two real build breaks the round's own zero-trust review had missed (R29-1..R29-17, tasks #432–#448)

**Runtime improvements this round: 0.** Every task is a correctness fix, a measurement (confirmatory either direction), an archival/process change, or CI/docs tooling — `production`'s feature composition and runtime behavior are unchanged.

**What actually moved this round, stated plainly up front.** Round 29 worked two independent readonly reviews' backlogs (`docs/reviews/2026-07-29-r28-readonly-review.md` and, discovered mid-round, `docs/reviews/2026-07-29-oh-acceleration-code-project-review.md`) down to zero, plus one systemic follow-up a tripwire test surfaced mid-round. R29-1 reproduced a flaky test flagged by R28-2 (6/2000 failures) and found the real cause: the leak-bound assertion used a *windowed* delta that a long-running proptest session could push negative across window boundaries even with zero real leaks — replaced with a global cumulative invariant (`released_total <= reserved_total`, no windowing), confirmed 0/2000 clean after the fix. R29-9 (systemic, triggered by the same review's pattern-matching finding) added a structural tripwire test enumerating every safe `pub fn dbg_*` hook that takes or returns a raw pointer against a hand-reviewed allowlist — closing the actual gap that let R25-1's soundness hole and three subsequent instances (R29-7, R29-8, and a fourth the tripwire itself caught mid-round, R29-17's `dbg_directory_bit_for_ptr`, which derived a segment base from an arbitrary caller pointer via a bitmask with zero validation) go unnoticed for a full round each; R29-7/R29-8 each re-gated their hook `unsafe fn` + `bench-internals` per the established R25-1 pattern, while R29-17 instead added a containment guard (return `None` on a foreign base before the header dereference), keeping the hook safe by construction rather than unsafe-by-contract — a materially different but equally valid fix, matching its sibling `dbg_owner_id_for`'s existing shape in the same file. R29-5 and R29-13 closed two of this project's oldest-standing "the design exists, the judge doesn't" measurement gaps: R29-5 found medium→Large promotion is genuinely rare under a doubling-growth workload (33/60,722 allocations, ratio 0.000543) — no victim for the Linux-mremap design R22-16 proposed; R29-13 measured the large-cache's 256 MiB default headroom for the first time ever and found it retains ~238–241 MiB/heap under forced convergence (only 12.4–12.5% reclaimed, confirmed-by-design not a bug) — 30x the small pool's own proven ~8 MiB/heap floor (R27-3), closing a measurement asymmetry where one subsystem had four tasks and three gate reports and the other had zero. R29-16 finally ran `virgin-zero-skip`'s own design docs' Stage-0/Stage-3 promotion gate, never executed since the feature shipped: a calloc-shaped iai isolation at 64 KiB found a real, large, deterministic 21.4x instruction-count win (3,067 Ir virgin vs 65,624 Ir recycled) confirming the feature works exactly as designed — but a paired wall-clock gate at the same size found no clean ON/OFF separation. The task's own report attributed that to a specific mechanism (eager small-segment page-commit paying the OS first-touch cost either way); a post-round independent readonly review found that explanation itself rests on a broken bench (the "virgin" wall-clock scenario's own `criterion` closure frees its batch each iteration, so it stops exercising the virgin path after the first of thousands of iterations) — corrected via dated append-only addenda in the gate report and `OPEN_ITEMS.md` item 25; the wall-clock question is UNCONFIRMED, not answered null, and needs the bench fixed and re-run before any promotion decision. R29-6 split `OPEN_ITEMS.md`'s long dated closure narratives (which had pushed the file past 1,000 lines, making the CLAUDE.md-mandated round-start read progressively more expensive) into a new `OPEN_ITEMS_ARCHIVE.md`, verified via an independent line-multiset diff that zero lines were lost across the split, and added a new CLAUDE.md rule requiring an immutable source identity (not just "base SHA + uncommitted changes") for any future perf-gate report measuring an uncommitted tree. R29-3, R29-4, R29-10, R29-11, R29-12, R29-14, and R29-15 closed the round's remaining smaller measurement/docs/CI items (see below for each). Zero-trust review across the round caught and fixed, in the same commits as each task: a real `--all-features` clippy failure R29-5's own gating left behind (the increment site it re-exported from is compiled OUT under that specific feature combination — the exact "narrow verification missed a wider CI row" pattern this project has hit repeatedly), a missing `alloc-global` in R29-13's new example's `required-features` (an E0432 compile break under exactly its own declared feature set), a real `clippy::trim_split_whitespace` CI-breaking lint in this session's own earlier R29-11 commit, and a genuinely stale README bench-internals enumeration R29-15's own delegated fix had not actually touched (it fixed an adjacent but different stale table row instead). **A post-round independent readonly review found two more real gaps the round's own per-task verification still missed**, both fixed the same day: R29-16's four new iai arms had no `not(feature = "virgin-zero-skip")` stub, so `cargo check --bench perf_gate_iai --features "production bench-internals"` — the literal command `scripts/iai.mjs`'s default features and the last step of `npm run check` run — failed with 4x E0433 (fixed with the established no-op-stub pattern this file already uses for `batch-api`); and R29-4's two new diagnostic structs had no `bench-internals` gate on their definitions, so `cargo clippy --features production -- -D warnings` failed with 3x dead-code (the exact bug class R29-5's own promotion counters had, fixed the same round, one commit later — fixed the same way here). A `clippy::doc_lazy_continuation` lint (a literal `+` at a doc-comment line start being parsed as a nested markdown bullet) in R29-16's new wall-clock bench, caught by the same `hardened medium-classes` CI row, was fixed in the same pass.

#### Runtime improvements

_None this round._ Every task is a correctness fix, a measurement (confirmatory or NO VICTIM/NO-GO), an archival/process change, or CI/docs tooling. `production`'s feature composition and runtime behavior are unchanged.

#### Measurement, correctness & tooling

- **[correctness fix, P0] R29-1 (task #432) — found and fixed the real cause of R28-2's flaky leak-bound assertion (6/2000 failures): a windowed delta, not the underlying invariant, was unsound.** Replaced with a global cumulative check (`segments_released_total <= segments_reserved_total`, no windowing); 0/2000 clean after the fix, both counts captured in committed raw logs.
- **[docs, P0] R29-2 (task #433) — documented the `(8, 32 MiB)` small-pool throughput recipe in README and fixed a contradictory "throughput-first defaults" overclaim** the shipped `(4, 16 MiB)` conservative default did not actually match.
- **[measurement, P1] R29-3 (task #434) — Stage-1 decomposition of the decommit→reserve segment lifecycle, resolving R27-11's previously-unmeasured trigger 2.** Seven new `bench-internals`-gated decomposition hooks isolate the OS round-trip from the reserve-and-keep and release sub-costs; trigger 2 does not fire, closing `OPEN_ITEMS.md` item 15 without opening a new optimization attempt. A raw-log provenance gap (a report citing a "primary" run that matched neither saved log) was caught and fixed in review before committing.
- **[measurement, P1] R29-4 (task #435) — reconciled R27-3's ~4 MiB committed-non-pooled residual to `small_active` (magazine residency), via a new per-heap segment-state accounting accessor.** Dated correction appended to `R27_3_POOL_RETENTION_GATE.md`, not a rewrite.
- **[measurement, P2] R29-5 (task #436) — measured medium→Large promotion frequency and copied-byte distribution for the first time: NO VICTIM.** Under a 4,000-small/40-large/20,000-background doubling-growth workload, promotion fired 33 times against 60,722 total allocation events (ratio 0.000543) — too rare to justify the Linux-mremap-instead-of-copy design (`OPEN_ITEMS.md` item 6/4a). A real `--all-features` clippy break this task's own gating left behind (the re-exported counters' only consumer is compiled out under `exact-span-large + large-reserved-capacity + numa-aware` simultaneously on) was caught during R29-13's review and fixed by gating on the full reachability predicate, not `bench-internals` alone.
- **[process, P3] R29-6 (task #437) — split `OPEN_ITEMS.md`'s long dated closure narratives into a new `OPEN_ITEMS_ARCHIVE.md` (1,581→426 lines), and added a CLAUDE.md rule requiring an immutable source identity for perf-gate reports measuring an uncommitted tree.** Zero lines lost across the split (verified by an independent line-multiset diff, not just the delegated agent's own claim); item numbers/tiers unchanged; `docs/CORRECTNESS_OPEN_ITEMS.md` inspected and left untouched (it doesn't have the same bloat pattern).
- **[correctness fix, P0] R29-7 (task #438) — re-gated `tls_heap::dbg_restore_local_for_test` `unsafe fn` + `bench-internals`, closing an R25-1-class gap the same review found.** A delegated fix that widened a downstream test file's `#[cfg]` to compensate would have broken two OTHER blocks in the same file that consumed the same-scope variables without the matching gate — caught and fixed by switching to the pre-existing safe `tls_heap::current_for_alloc()` accessor instead, a materially better fix that removed the need for the wider gate entirely.
- **[correctness fix, P0] R29-8 (task #439) — re-gated `AllocCore::dbg_force_decommit_retain_for` `unsafe fn` + `bench-internals`, the third instance of the same R25-1 bug class this round.**
- **[systemic process fix, P0] R29-9 (task #440) — new structural tripwire test (`tests/dbg_hook_safety_tripwire.rs`) enumerating every safe `pub fn dbg_*` hook taking/returning a raw pointer against a hand-reviewed allowlist, failing loudly on any new unaccounted-for instance.** This is the actual fix for the class of bug R25-1/R29-7/R29-8 kept recurring one at a time — the tripwire caught a fourth live instance the same day it was added (R29-17 below).
- **[measurement, P1] R29-10 (task #441) — isolated the alloc-hit `segment_base_of_ptr` + `clear_magazine` RMW's own standalone cost at 12.19 Ir/hit, closing the question permanently.** Paired shared-prefix-subtraction iai arms, mirroring R28-1's pattern. Corrected a stale per-file `dbg_*` hook count in README (4→7) that had drifted since R29-3 added hooks without updating that specific row — caught in review, not by the delegated task's own check.
- **[process, P1] R29-11 (task #442) — migrated `IAI_BASELINE.md`'s eight previously-unindexed "honest reject" sections into `OPEN_ITEMS.md`'s `[L]` tier, and added a structural tripwire (`tests/no_stale_doc_references.rs::honest_reject_sections_are_indexed`) so a ninth can't go unindexed silently.** An `cargo fmt --check` failure in the new test file was caught and fixed in review.
- **[process, P1] R29-12 (task #443) — new `docs/FEATURE_PROMOTION_STATUS.md` survey table across every non-`production` feature's promotion status, resolving two previously-dangling decisions (`virgin-zero-skip` → NEVER-DECIDED, explicitly filed rather than silently absent; `small-segment-lazy-commit` → CONDITIONAL-keep-opt-in) into `OPEN_ITEMS.md`.**
- **[measurement, P2] R29-13 (task #444) — measured the large-cache's 256 MiB default headroom's idle-RSS floor for the first time: pure idle reclaims exactly 0 KiB at every headroom setting (event-driven decay confirmed), and under forced decay-to-fixed-point the shipped default converges to ~238–241 MiB/heap retained (only 12.4–12.5% reclaimed) — 30x the small pool's own proven ~8 MiB/heap floor.** Confirmed-by-design, not a bug; no default changed. New `HeapCore`-level diagnostic delegations (`dbg_large_cache_used`/`dbg_large_cache_slot_sizes`/`dbg_decay_config`/`dbg_force_decay_tick`), all thin wrappers over pre-existing safe `AllocCore` accessors, no new `unsafe`. A missing `alloc-global` in the new example's `required-features` (an E0432 break under exactly its own declared feature set) was caught and fixed in review.
- **[CI, P2] R29-14 (task #445) — added `cargo check --features "production numa-aware"` to the existing `test-feature-isolation` per-PR job, closing a gap where `--all-features` (which also enables `numa-aware-mock`) never compiled the real, non-mock NUMA integration path per-PR — only weekly, via `numa-real-kernel`.** Verified via `cargo metadata` that the new step's exact feature string resolves `numa-shim` to the real (non-mock) cfg arm, not just added the step on faith.
- **[docs, P2] R29-15 (task #446) — three doc corrections: `tcache.rs`'s stale "no dependent load on the hit path" claim (false since RAD-5, the alloc-hit pop now does a pointer-dependent bitmap RMW), a previously-undocumented `numa-aware` × `small-segment-lazy-commit` silent no-op trap (the NUMA arm never participates in lazy-commit deferral at all), and README's bench-internals feature-table row enumeration.** The delegated fix for the third item touched an adjacent but different stale table row without fixing the originally-scoped one (missing R29-3's two decomp hooks) — caught and fixed in review by independently grepping every `bench-internals`-gated site in `src/`.
- **[measurement, P2] R29-16 (task #447) — ran `virgin-zero-skip`'s own design docs' Stage-0/Stage-3 promotion gate for the first time since the feature shipped: real 21.4x Ir win (3,067 vs 65,624 Ir at 64 KiB), wall-clock inconclusive.** Not a clean promotion GO; `docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` received a dated append-only addendum, its original null finding left untouched. No new `bench-internals` hook needed — both virgin/recycled states built from already-safe production API. **Same-day correction (post-round readonly review):** the wall-clock arm's "eager page-commit" root-cause explanation was itself found unconfirmed — a bug in the "virgin" scenario's own bench design (its `criterion` closure frees the batch each iteration, so it stops exercising the virgin path after iteration 1) is the more direct explanation for the lack of ON/OFF separation; §8 of the gate report and `OPEN_ITEMS.md` item 25 have the full correction. The Ir isolation is unaffected.
- **[correctness fix, P0, systemic] R29-17 (task #448) — fixed `HeapCore::dbg_directory_bit_for_ptr`, the fourth instance of the R25-1 bug class, caught by the R29-9 tripwire the same day it was added.** Unlike R29-7/R29-8 (converted to `unsafe fn` + `bench-internals`), this one was fixed by adding a containment guard (`self.core.segment_bases().any(|b| b == base)`, return `None` on a foreign base) before the segment-header dereference — the function stays a safe `pub fn`, matching its sibling `dbg_owner_id_for`'s existing shape in the same file and fulfilling a "None if ptr is foreign" contract the doc comment already promised but never implemented.
- **[review, post-round] Independent readonly review of the full Round 29 diff (`docs/reviews/2026-07-29-r29-readonly-review.md`) — verified the round's soundness/measurement/gating claims and found two real build breaks the round's own per-task zero-trust review had missed, both fixed the same day.** `cargo check --bench perf_gate_iai --features "production bench-internals"` (the literal command `scripts/iai.mjs`'s default features and `npm run check`'s last step run) failed with 4x E0433 — R29-16's new iai arms had no `not(feature = "virgin-zero-skip")` no-op stub, unlike every other conditionally-gated arm in that file; fixed by adding the missing stubs, matching the file's own established `batch-api` precedent. `cargo clippy --features production -- -D warnings` failed with 3x dead-code — R29-4's two new diagnostic structs had no `bench-internals` gate on their definitions (only their sole consumer did), the same bug class R29-5's own promotion counters had and were fixed for one commit earlier in this same round; fixed the same way. Also independently confirmed the R29-16 wall-clock finding above needed correcting, verified the OPEN_ITEMS.md archive split lost zero lines via an independent multiset diff, and confirmed the new `test-feature-isolation` numa-aware CI step and the four re-gated/guarded `dbg_*` hooks are all sound. Several lower-severity findings (a "21.4x Ir" per-byte-ratio characterization worth a methodological caveat, a couple of ratio/percentage-framing imprecisions in the R29-3/R29-5/R29-13 reports, and some smaller doc/tooling nits) were filed into `docs/CORRECTNESS_OPEN_ITEMS.md` / `docs/perf/OPEN_ITEMS.md` for a future round rather than addressed in this same pass.

### Round 28 — `flush_class`'s standalone Ir cost finally isolated (449 Ir, 5th data point that the magazine-overflow region is exhausted), the `r14_4` leak-bound assertion strengthened from "no double-release" to "no leak" with a real CI-breaking gap caught and fixed in review (R28-1..R28-2, tasks #430–#431)

**Runtime improvements this round: 0.** Both tasks are measurement-only / test-only; no `src/` production algorithm or default changed.

**What actually moved this round, stated plainly up front.** Round 28 closed the two remaining open items both perf and correctness indexes flagged at round start. R28-1 finally answered `OPEN_ITEMS.md` item 1's "Next trigger" — a `flush_class` isolation measurement open since R24-2 and unaddressed through four consecutive NO-GOs in the adjacent magazine-overflow region (R24-3/R24-4/R25-3/R26-7) — via a new `bench-internals`-gated `unsafe fn` hook that calls the real production `flush_class` standalone: it costs 449 Ir (56.1 Ir/block), 77.3% of one overflow event's total and 90.3% of R24-2's originally-fused "~487 Ir non-isolable remainder" estimate (reconciling to within 2.1%). The number itself does not suggest an avoidable large constant, so this is filed as a 5th data point (after 4 NO-GOs) that the region is tightly compiled, not a green light for a 5th optimization attempt. R28-2 strengthened `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound assertion, which had proven only "no double-release" (a process-wide `released_delta <= reserved_delta` check trivially satisfied by a genuinely-never-released segment) since the gap was first flagged in a Round-22 followup review. The new per-base check — resolve the freed block's own segment base, trim the heap to a converged state, and assert that base is either fully unregistered or has its `live_count` down by exactly one — was verified non-vacuous twice, against two structurally different free paths (Large-promoted and the shared-`small_cur` medium ladder), each via a real break-and-revert counterfactual. Zero-trust review before commit caught a real CI-breaking gap the delegated work never tested against: the strengthened block's `dbg_live_count_for` needs `alloc-decommit`, which the dedicated `test (--features "hardened medium-classes")` CI row does not enable — the as-delivered diff failed to compile there with two `E0599` errors. Fixed by narrowing the new block's own `#[cfg]` rather than widening the test file's gate, preserving that CI row's existing (unstrengthened) coverage of the file; both counterfactuals were re-run against the restructured code to confirm the fix didn't silently defang the new assertion.

#### Runtime improvements

_None this round._ R28-1 adds a `bench-internals`-only measurement hook with no production caller; R28-2 is a test-file-only strengthening. `production`'s feature composition and runtime behavior are unchanged.

#### Measurement, correctness & tooling

- **[measurement] R28-1 (task #430) — isolated `flush_class`'s own standalone Ir cost inside one magazine-overflow event: 449 Ir (56.1 Ir/block), 77.3% of the event total, closing the "Next trigger" question `OPEN_ITEMS.md` item 1 has carried since R24-2.** New `HeapCore::dbg_flush_class_only` hook, `pub unsafe fn` + `bench-internals`-gated FROM CREATION (not retrofitted, unlike its R24-2-era predecessor `dbg_overflow_bitmap_clear_pass` that R25-1/R27-10 had to fix and then remove) — delegates to the real `AllocCore::flush_class` verbatim, no alternate implementation. Reconciles with R24-2's ~487 Ir fused-remainder estimate to within 2.1% once today's ~10 Ir cross-round drift is accounted for. Verdict: the region is judged likely exhausted for further per-block-cost micro-optimization — `flush_run`'s per-block work has no un-hoisted cost left to cut without weakening the M2 double-free guard, and the compaction+push residual is now confirmed small (~48 Ir) rather than a hidden larger target. No 5th optimization attempt opened.
- **[correctness fix] R28-2 (task #431) — strengthened `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound assertion from "no double-release" to "no leak", and caught a real CI-breaking compile gap in zero-trust review before it landed.** New per-base observable (additive to the untouched pre-existing double-release guard): after freeing the test's own promoted block and trimming the heap into a converged state, assert the freed segment base is either fully unregistered or has its `live_count` down by exactly one. Uses only pre-existing, already-appropriately-gated safe accessors (`dbg_contains_base`, `dbg_live_count_for`) — no new hook. Non-vacuity verified twice (Large-promoted path and the shared-`small_cur` medium-ladder path), each via a real counterfactual break, independently re-run against the final code. Review caught the delegated diff failing to compile under the CI-tested `hardened medium-classes` combination (needs `alloc-decommit`, which that combo doesn't enable) — fixed by narrowing the new block's own `#[cfg]` gate rather than widening the file's, keeping the load-bearing `a.dealloc` call unconditional in between so the pre-existing assertion's own coverage under that CI row is unaffected.

### Round 27 — the pool-cap default-change decision corrected from a one-knob no-op to a real paired (segments, bytes) trade, retention cost quantified with victim activation (~+8 MiB/heap, ~22% latency win), an adaptive-budget design written and found not worth building, ~250 lines of dead NO-GO code and a soundness-adjacent bitmap-clear hook removed, three diagnostic hooks re-gated behind `bench-internals`, the shell-quoting fix's regression test wired into the gate (R27-1..R27-11, tasks #419–#429)

**Runtime improvements this round: 0.** Every task is a correction, a measurement, a design study (recommending against building what it studied), a cleanup, or tooling — `DEFAULT_POOL_SEGMENTS`/`DEFAULT_POOL_BYTE_CAP` remain `4`/`16 MiB` throughout; `production`'s composition is unchanged.

**What actually moved this round, stated plainly up front.** Round 27 was triggered by an independent read-only review of Round 26 (`docs/reviews/2026-07-28-r26-readonly-review.md`), personally re-verified claim-by-claim before any task was filed. R27-1 found the round's most consequential bug in the pending pool-cap default-change proposal itself: every prior report phrased it as "promote `DEFAULT_POOL_SEGMENTS` 4→8", but the effective cap resolves as `min(pool_segments, pool_byte_cap / SEGMENT)` and the current 16 MiB byte cap already forces that to `min(8,4)=4` — a literal no-op. The real decision is a **paired** change, `(4, 16 MiB) → (8, 32 MiB)`, which doubles the documented maximum retained pool memory per heap, a genuine trade no prior A/B (R25-5, R26-1, R26-3) had actually measured (all three used a generous 256 MiB ceiling specifically so the byte cap would never bind). R27-2 additionally found that R26-9's "no cap-specific RSS cost" closure premise was itself refuted by R26-3's own committed raw log, which showed cap8 retaining ~4 MiB more after teardown — an axis R26-1's peaklive-only RSS probe never measured. R27-3 built the proper retention gate with victim activation hard-asserted for both arms and found the real number: cap 8 retains ~+8 MiB/heap post-teardown (~2 segments), scaling linearly to ~+255 MiB at 32 heaps, not decaying during idle. R27-4 then confirmed the latency win survives at the REAL paired byte caps through the real un-bypassed `#[global_allocator]` entry point — cap8 ~22% faster, decommit cliff eliminated deterministically — so both halves of the trade were finally measured at the actual shipping configuration. R27-5 wrote the adaptive/process-wide pool-budget design R26-9's own closure condition had asked for once a real penalty was found, and recommended against building it: the latency win is binary (not graduated), every measured workload is uniform-pressure (so a budget is either never-binding or splits the win into a worse bimodal fleet), and idle shrink-back is unsolved within this project's no-background-thread constraint — recommending instead to keep the conservative 4/16 MiB default and document an 8/32 MiB opt-in recipe. R27-6 removed R26-7's ~250-line NO-GO lazy-staging-array implementation its own commit had left behind (a standing-rule violation caught by the same review); R27-7 fixed my own gating-rule miss from the prior round's zero-trust reviews (three new diagnostic hooks reachable from plain `production` despite having no production caller); R27-8 corrected R26-3's timed-workload description (9 batches/1080 cycles measured, not 8/960 — a description fix, not a re-measurement, since R27-4 had already independently reconfirmed the effect); R27-9 wired the R26-8 argv-roundtrip regression test into the actual `npm run check` gate it was meant to protect, and made the "no `shell:true`" convention an executable throw rather than a comment (catching 5 pre-existing `shell:isWin` callers R26-8 had missed); R27-10 removed the R24-2-era `dbg_overflow_bitmap_clear_pass` hook the review flagged as leaving a temporarily-disagreeing magazine invariant, given the region it measured had a confirmed 4-for-4 NO-GO track record; R27-11 evaluated (but did not open) a reservation-only overflow-tier design, finding one of its two required triggers unmeasured rather than assuming it away.

#### Runtime improvements

_None this round._ Every task is measurement, correction, design (recommending against building), cleanup, or tooling. `DEFAULT_POOL_SEGMENTS`/`DEFAULT_POOL_BYTE_CAP` are unchanged (`4`/`16 MiB`); `production`'s feature composition is unchanged.

#### Measurement, correctness & tooling

- **[correction, P0] R27-1 (task #419) — the pending pool-cap default-change proposal was a literal no-op as phrased: the effective cap resolves as `min(pool_segments, pool_byte_cap / SEGMENT)`, and the current 16 MiB byte cap already forces `min(8,4)=4`.** The real decision is a paired `(pool_segments, pool_byte_cap) = (4, 16 MiB) → (8, 32 MiB)` change, doubling per-heap retained pool memory — a genuine RSS-vs-throughput trade no prior A/B had actually measured (all used a 256 MiB ceiling that never binds). Added `tests/small_segment_pool.rs::paired_knob_promotion_is_not_a_noop`, a CI counterfactual proving the malformed one-knob edit resolves to cap 4, not 8.
- **[correction, P0] R27-2 (task #420) — the live "cap 8 is RSS-neutral" verdict was contradicted by R26-3's own committed raw log (cap8 retaining ~4,100 KiB more after teardown) and R26-1's RSS probe never proved victim activation at its lower-pressure batch size.** Fixed the MUTABLE current-state headline in `OPEN_ITEMS.md` item 13 directly (not just an appended caveat below an unchanged headline), per the review's own recommendation. Reopened R26-9's closure, tracked as R27-5.
- **[measurement, P0] R27-3 (task #421) — the proper pool-cap retention gate, with victim activation hard-asserted for both arms: cap 8 retains ~+8 MiB/heap post-teardown (~2 segments), scaling linearly to ~+255 MiB at 32 heaps, and does not decay during idle.** New subprocess-per-arm probe at the pressure-producing batch 120 (not R26-1's batch 50); cap-4 arms proven to saturate (`decommit_delta>0`), cap-8 arms proven to retain beyond cap-4's bound (`pooled_hw_max>4`, `decommit_delta==0`). Confirmed the small-pool decay is event-driven (no background thread) by reading source directly, not assuming it.
- **[measurement, P1] R27-4 (task #422) — confirmed the pool-cap latency win survives at the REAL paired byte caps (16/32 MiB) through the real, un-bypassed `#[global_allocator]` entry point: cap8 ~22% faster, decommit cliff eliminated deterministically (9→0 across 40 launches each).** Every prior A/B had measured at a generous 256 MiB ceiling that never bound; this closes the "never measured at the actual shipping config" gap. Combined with R27-3, both halves of the pending default-change trade are now measured at the real configuration.
- **[design] R27-5 (task #423) — wrote the adaptive/process-wide pool-budget design R26-9's own closure condition asked for, and recommends AGAINST building it.** The latency win is binary (not graduated: cap ≥ demand ⇒ 0 decommits, else N), every workload this project has measured is uniform-pressure (so a global budget is either never-binding or splits the win into a worse bimodal fleet), and idle shrink-back is unsolved within this project's documented no-background-thread constraint. Recommends keeping the conservative 4/16 MiB default and documenting an 8/32 MiB opt-in throughput recipe instead. CONDITIONAL-GO-on-paper, deferred pending a measured uneven-pressure victim.
- **[process fix, P1] R27-6 (task #424) — removed R26-7's ~250-line NO-GO lazy-staging-array implementation (`dbg_dealloc_batch_lazy`/`dealloc_batch_small_lazy`) and its 9 bench arms, left behind in violation of this project's own R25-10 rule ("a NO-GO must re-evaluate its dependent hooks in the same task").** Confirmed via `git diff` the shipping `dealloc_batch`/`dealloc_batch_small` are byte-identical — exactly one deletion hunk. Retained the 4 eager baseline arms (measure shipping code, fill a genuine N-grid gap) per the R25-7 retained-arm precedent, distinct from the rejected duplicate-code pattern.
- **[process fix, P2] R27-7 (task #425) — gated three Round-26 diagnostic hooks (`dbg_tcache_contains`, `dbg_pool_cap`, `dbg_is_free_for`) behind `bench-internals`, fixing my own miss in the prior round's zero-trust reviews of R26-1/R26-5 (applied the unsafe-fn sub-rule correctly, missed the no-production-caller gating sub-rule).** Scoped strictly to these 3 hooks, not a retroactive sweep of the other 18 pre-existing `dbg_*` hooks that predate the rule.
- **[correction, P2] R27-8 (task #426) — corrected R26-3's timed-workload description: the "untimed warm-up" was actually inside the timed region (9 batches/1080 cycles measured, not 8/960).** A description fix, not a re-measurement — both A/B arms ran the identical shape, so R26-3's statistics are correct as measured; R27-4 had already independently reconfirmed the effect under the corrected shape in new files.
- **[process fix, P2] R27-9 (task #427) — wired the R26-8 argv-roundtrip regression test into `npm run check` as its literal first step, and made the "no `shell:true`" convention an executable throw in `lib.mjs`'s `run()` rather than an unenforced comment.** Widened the TRICKY test-case set from 4 to 8 (added shell-metacharacter and Unicode cases). Auditing every `run()` caller for the new throw surfaced and fixed 5 pre-existing `shell:isWin` callers R26-8 had missed.
- **[fix, P2] R27-10 (task #428) — removed `HeapCore::dbg_overflow_bitmap_clear_pass` (R24-2) and its bench arm: the hook left the magazine bitmap and slots in a temporarily-disagreeing state on return, with process-isolation-only-safe as an implicit, undocumented precondition of an `unsafe fn`.** The region it measured has a confirmed 4-for-4 NO-GO track record (R24-3/R24-4/R25-3/R26-7), so no live consumer justified keeping the hook. Three R24 gate reports that historically cited its Ir figure received append-only correction notes; none of their NO-GO verdicts rested on this reference arm.
- **[process fix, P2] R27-11 (task #429) — evaluated (did not open) a reservation-only small-pool overflow-tier design: one of its two required triggers (an isolated OS-reserve/setup/metadata-reinit cost breakdown) is unmeasured, not fired.** Per the task's own explicit "do NOT start unless BOTH triggers fire" rule, filed the idea into `OPEN_ITEMS.md`'s `[D]` deferred-designs tier (previously an unfiled review idea) with the exact Stage-1 measurement a future round needs to actually resolve it, rather than either silently dropping it or writing a design against an unproven precondition.

### Round 26 — R25-5's invalidated "wins on both axes" RSS claim rebuilt under subprocess isolation (RSS-neutral, not RSS-beneficial), the pool-cap latency win reconfirmed through the real `#[global_allocator]` entry point, a new CLAUDE.md rule for config-sweep evidence, a stronger per-block `dealloc_batch` leak oracle, a 4th consecutive magazine-overflow NO-GO, `scripts/`' shell-quoting hack replaced with real argv, the adaptive pool-budget design task closed on its own unmet trigger (R26-1..R26-9, tasks #410–#418)

**Runtime improvements this round: 0.** Every task is a correction, a measurement (confirmatory or negative), a new process rule, a test strengthening, or tooling — `production`'s feature composition and runtime behavior are unchanged across the whole round.

**What actually moved this round, stated plainly up front.** R26-2 opened the round by catching its own predecessor's error: R25-5's "`pool_segments` 4→8 wins on BOTH axes" claim rested on an RSS probe that ran all four cap arms sequentially in one process, where `HeapRegistry`'s first-claim-wins slot reuse let a later arm silently inherit an earlier arm's config — with the one loud safety net (`debug_assert!`) compiled out of the `--release` build the probe actually ran. R26-1 rebuilt the gate with one fresh OS process per arm, self-verifying both the resolved cap and a zero config-conflict delta before trusting any RSS number, and found the "win" does not reproduce: cap 4/8/16/32 are RSS-neutral at every thread count once cross-arm state leakage is structurally ruled out — the latency/decommit axis is unaffected and still stands. R26-3 independently reconfirmed that latency half through the real, un-bypassed `#[global_allocator]` entry point for the first time (every prior A/B used a deliberate `AllocCore::new_with_config` bypass) — cap 4→8 measured 16% faster with the decommit cliff eliminated deterministically. R26-4 codified the R26-1/R26-2 lesson as a standing CLAUDE.md rule: any report sweeping a runtime config value across arms must carry, per arm, the requested value, the resolved value read back from the allocator's own diagnostic surface, any config-conflict-delta counter, and the process/thread-identity boundary it ran under — a row missing any of these is not usable as GO/NO-GO evidence. R26-5 caught a real blind spot in R25-4's own `dealloc_batch` leak oracle: an aggregate `live_count` delta cannot distinguish "correct" from "one block leaked and a different one double-processed, cancelling in the sum" — added a per-block partition oracle proven strictly stronger via a constructed cancelling-pair mutation the old test missed but the new one catches. R26-6 corrected a `# Safety` doc comment that stated the wrong precondition for a measurement hook (found by an independent review). R26-7 prototyped a lazily-materialized `dealloc_batch` staging array — NO-GO, the 4th consecutive negative result in the magazine-overflow free path this project has measured (after R24-3/R24-4/R25-3): the never-materialize win is real but tiny (~53 Ir) and immediately overwhelmed by a discriminant-check cost the moment the magazine overflows, crossover at exactly N=17 with every realistic batch size in the loss regime. R26-8 replaced a fragile, hand-rolled shell-quoting workaround in the project's own script runner with real `argv` passing (`shell:false` throughout), closing a Node 22+ DEP0190 hazard that had made quoting rules apply globally regardless of whether a given caller needed shell syntax (none did). R26-9 closed the round's originally-planned adaptive pool-budget design task without writing a design at all: its own explicitly-recorded trigger ("only if R26-1's corrected RSS gate exposes a real cap-8 penalty") was not met by R26-1's own RSS-neutral finding — dispatching a design against an unmet gate would have been speculative work. (Round 27 subsequently found this closure's own premise was itself incomplete — R26-1 measured only peak-live-set RSS, and R26-3's raw log showed a real post-teardown retention cost neither task had checked; see Round 27's R27-2/R27-3 above.)

#### Runtime improvements

_None this round._ Every task is measurement, correction, a new process rule, test strengthening, or tooling. `production`'s feature composition and runtime behavior are unchanged.

#### Measurement, correctness & tooling

- **[correction] R26-2 (task #411) — corrected R25-5's invalidated RSS/commit claim across 5 documents: the probe's sequential single-process design let `HeapRegistry`'s first-claim-wins config resolution silently keep an earlier arm's config on a recycled slot, with the one loud signal (`debug_assert!`) compiled out of the `--release` build the probe ran under.** The latency/decommit axis (a different, unaffected code path) is untouched and its finding stands. Appended (not rewrote) corrections to the R25-5 report, its summary CSV, `OPEN_ITEMS.md`, `CHANGELOG.md`'s Round 25 section, and the Round-25 checkpoint.
- **[measurement] R26-1 (task #410) — rebuilt the pool-cap RSS gate with subprocess-per-arm isolation; R25-5's RSS win does not reproduce.** All 36 child runs (4 caps × 3 thread-counts × 3 reps) hard-asserted their resolved cap and a zero config-conflict delta before being trusted. Under isolation, all four caps produce statistically identical RSS/commit at every thread count — restated verdict: GO-CANDIDATE for `pool_segments=8` survives on the latency axis alone, at RSS-NEUTRAL (not RSS-beneficial) cost. `DEFAULT_POOL_SEGMENTS` remains 4, unchanged.
- **[measurement] R26-3 (task #412) — confirmed the pool-cap 4→8 latency win through the real, un-bypassed `#[global_allocator]` entry point for the first time.** Two new example binaries install a real `SeferAlloc` via the const-fn `with_config` path, judged by the project's existing paired A/B/B/A infrastructure. cap8 statistically-significantly faster (t=12.212, sign 20/20, 16% faster); decommit cliff reproduces deterministically at the process level. `DEFAULT_POOL_SEGMENTS` remains 4 — measurement only.
- **[process fix] R26-4 (task #413) — new standing CLAUDE.md rule: any config-sweep report must carry, per arm, requested value / resolved value / config-conflict-delta / process-identity boundary, or it is not usable as GO/NO-GO evidence.** Directly motivated by R25-5's incident (retroactively checked: R26-1 fully satisfies the new rule; R26-3 satisfies it structurally via subprocess isolation but not literally, flagged as a smaller follow-up, not backfilled here).
- **[test infrastructure] R26-5 (task #414) — strengthened `dealloc_batch`'s multi-flush correctness oracle from an aggregate `live_count` delta (which cannot catch a cancelling leak+double-process pair) to a per-block partition proving all 200 freed blocks classify into exactly the expected magazine-resident/free/double-processed/leaked buckets.** Two new safe, read-only diagnostic accessors (`dbg_tcache_contains`, `dbg_is_free_for`). A constructed cancelling-pair mutation keeps the old aggregate test green while the new per-block test correctly goes red, proving the new oracle is strictly stronger, not just a re-proof of the old one.
- **[doc fix] R26-6 (task #415) — corrected `dbg_overflow_bitmap_clear_pass`'s `# Safety` contract, which stated the opposite of its actual caller's precondition** (the caller deallocs blocks first, making them magazine-resident with the bitmap bit SET, then calls the hook to clear that bit — the old wording required the bit to already be clear). Doc comment only, no behavior change.
- **[measurement, NO-GO] R26-7 (task #416) — prototyped a lazily-materialized `Option<[..]>` staging array for `dealloc_batch_small`; NO-GO, crossover at N=17 (the first overflow block), the 4th consecutive NO-GO in the magazine-overflow region after R24-3/R24-4/R25-3.** The never-materialize win is real (~53 Ir, ~1-2%) but far smaller than a naive linear extrapolation from R24-8/R25-7 predicts, and immediately overwhelmed by a per-overflow-block discriminant-check cost; every realistic batch size (tens-low hundreds) is in the loss regime. The lazy variant was kept as `bench-internals`-gated experimental infra at the time — removed the following round (R27-6) per this project's own dangling-artifact rule.
- **[tooling] R26-8 (task #417) — replaced `scripts/lib.mjs`'s hand-rolled, Windows-cmd.exe-flavored shell-quoting workaround with real `argv` passing (`shell:false`), closing a Node 22+ DEP0190 hazard.** None of the 5 callers actually used shell syntax; `shell:true` was only ever load-bearing for keeping multi-word arguments as one token, which a real `argv` array handles natively. Added `scripts/argv-roundtrip-test.mjs`, a standalone regression test proving a whitespace/quote-containing argument survives as one argv element (not yet wired into the automatic gate — done the following round, R27-9).
- **[process fix] R26-9 (task #418) — closed the round's originally-planned adaptive/process-wide pool-budget design task without a design attempt: its own explicitly-recorded trigger condition ("only if R26-1's corrected RSS gate exposes a real cap-8 penalty") was not met.** R26-1 found the opposite (RSS-neutral). Dispatching a design against a gate its own trigger does not meet would have been speculative work, matching R25-6's/R25-9's established practice. (Reopened the following round once R27-2/R27-3 found this closure's own premise incomplete.)

### Round 25 — a P0 safe-fn soundness hole closed, three more NO-GOs in the magazine-overflow region (FLUSH_N sweep, run-encoded free design), STAGE_CAP=64 confirmed clean to N=1024, pool_segments 4->8 found to win on both latency AND RSS, a stale NUMA-cliff citation caught and closed (R25-1..R25-10, tasks #395–#404)

**Runtime improvements this round: 0.** Every task this round is a correctness fix, a measurement (confirmatory or NO-GO), a design study, or docs/process work — tagged inline below. This is a correction-and-evidence round triggered by an independent read-only review of Round 24 (`docs/reviews/2026-07-28-r24-readonly-review.md`), personally re-verified claim-by-claim against source before any task was filed on it.

**What actually moved this round, stated plainly up front.** The review's single most consequential finding was real: R25-1 fixed a genuine P0 soundness bug (`HeapCore::dbg_overflow_bitmap_clear_pass` was a *safe* `pub fn` that derived a segment base from an unvalidated caller pointer and wrote allocator metadata through it, reachable from 100%-safe code under plain `production`) — closed by making it `unsafe fn` with a documented contract and gating it behind `bench-internals`, matching the R24-6 precedent exactly. R25-10 then codified the lesson as a standing CLAUDE.md rule so the same class of bug can't recur unnoticed. R25-3 swept the magazine-overflow half-flush constant `FLUSH_N` (4/8/12/16) against five required gates and found a NO-GO — the seemingly-attractive `FLUSH_N=16` (full flush) wins on raw Ir but triggers a 20x refill-thrash regression on a boundary-stress workload, the third NO-GO in this exact code region after R24-3/R24-4. R25-8 then asked whether an architecturally different mechanism (a run-encoded arithmetic free list) could do better, and found a CONDITIONAL-GO that explicitly excludes the very region that motivated it: the magazine is a LIFO stack in free-order, not offset-contiguous, so a run-descriptor cannot even encode a magazine-overflow flush — the design's one real lever (skipping the alloc-side `read_next` chain walk) is reachable only through `dealloc_batch`, which has no downstream consumer today. R25-7 filled a disclosed evidence gap from R24-8 (STAGE_CAP=64 had only ever been measured at N=16/64) with a real A/B out to N=1024, confirming the constant zero-init win still dominates everywhere measured (crossover projects at N~2,700, far beyond any realistic batch size) — and incidentally caught a real build regression (two example probes missing a `Cargo.toml required-features` entry, breaking `cargo test --features production`, the exact command this project's pre-push gate runs). R25-5 ran the RSS-gated `pool_segments` sweep R24-11 had flagged as its own next step, and found something better than expected: `pool_segments` 4->8 wins on BOTH the latency/decommit axis AND the RSS/commit axis simultaneously (cap=4's own periodic decommit-then-reserve churn costs more RSS than steady-state cap>=8 ever does) — closing R25-6's conditional design task without a design attempt, since the tradeoff an adaptive budget would exist to manage simply isn't there for this workload. R25-4 built a stronger, `HeapCore`-level correctness oracle for R24-8's multi-flush path (an exact live-count state-transition assertion, isolated from test-harness noise) alongside the existing global-allocator test. R25-2 corrected a wording mistake in Round 24's own CHANGELOG entry (this session's own error, caught by the same review). R25-9 investigated the review's remaining recommendation (a NUMA-directory node-indexing opportunity) and found it was already resolved 14 rounds earlier by R11-6 — the review's citation was stale, not a new finding.

#### Runtime improvements

_None this round._ Every task either fixed a soundness bug with no perf implication, confirmed an existing constant/design choice without changing it, found a NO-GO, or produced docs/process/design output. `production`'s feature composition and runtime behavior are unchanged from Round 24.

#### Measurement, correctness & tooling

- **[correctness fix, P0] R25-1 (task #395) — closed a real soundness hole: `HeapCore::dbg_overflow_bitmap_clear_pass` was a safe `pub fn` deriving a segment base from an unvalidated caller pointer and writing allocator metadata through it, reachable from safe code under plain `--features production`.** Flagged by an independent read-only review, personally re-verified against source (the function, its one caller, its `production`-satisfied `#[cfg]`) before acting. Fixed by making it `unsafe fn` with a documented `# Safety` contract and gating it behind `bench-internals` — the exact R24-6 pattern applied to a third hook. The one existing caller (`benches/perf_gate_iai.rs`'s `dealloc_overflow_bitmap_clear_only_16b`, real R24-2 measurement infrastructure, not dead code) was updated, not deleted.
- **[correction] R25-2 (task #396) — corrected a wording mistake in Round 24's own CHANGELOG entry, caught by the same independent review.** "Plain `--features production`'s composition changed exactly once this round: R24-8's `STAGE_CAP` constant" was self-contradictory: `STAGE_CAP` lives behind the opt-in `batch-api` feature, so it does not execute under plain `production` at all, and `production`'s Cargo-level composition never changed. Corrected to state plainly that plain `production` changed zero times.
- **[measurement, NO-GO] R25-3 (task #397) — swept the magazine-overflow half-flush constant `FLUSH_N` (4/8/12/16, `TCACHE_CAP` fixed at 16) against 5 required gates; NO-GO for every value.** `FLUSH_N=16` (full flush) is the only value with a real bulk-free Ir win (-1.5%) but triggers a 2.42x Ir regression and a 20x refill-event-count regression on an oscillating boundary-stress workload — every overflow event empties the magazine completely, guaranteeing the next growth phase's first alloc misses and pays the cold refill path. `FLUSH_N=4`/`12` show no win at all. The third NO-GO in this exact code region this round cluster (after R24-3/R24-4). A real, independent, latent compile-time bug (`virgin_mask >>= FLUSH_N`, a `u16` shift-by-16 at `FLUSH_N=16` under `virgin-zero-skip`, release-profile-only) was found and documented as a prerequisite for any future revisit, moot given the NO-GO.
- **[test infrastructure] R25-4 (task #398) — added an isolated `HeapCore`-level correctness oracle for `dealloc_batch`'s multi-flush path, proving what R24-8's global-allocator test explicitly could not.** Asserts the exact `live_count` state transition (-184, not -200, per the documented first-warm magazine-residency policy) directly against authoritative allocator state, bypassing the test-harness allocation noise the original test's own docs already disclosed as a limitation. Mutation counterfactual independently re-run by the reviewer (not just trusted): breaking the mid-loop flush produces the exact predicted wrong delta (56, not 184). Also fixed a real one-line flush-count miscount in the R24-8 test's inline comment and tightened its name/docstring to state only what it actually proves.
- **[measurement] R25-5 (task #399) — the RSS-gated `pool_segments` sweep (4/8/16/32) R24-11 flagged as its own next step: `pool_segments` 4->8 wins on BOTH the latency/decommit axis AND the RSS/commit axis simultaneously.** Eliminates the entire measured decommit residual (20/run -> 0) at LOWER, not higher, RSS/commit cost at every thread count (1/8/32) measured — cap=4's own periodic decommit-then-reserve churn costs more RSS than the steady state cap>=8 achieves by simply not churning. `pool_segments` 16/32 add nothing further (this workload's demand tops out at 6 concurrent segments). GO-CANDIDATE for `pool_segments=8`, flagged for a future default-raise decision, not changed in this task. A real methodology pitfall (a naive sequential probe measuring zero decommits, missing criterion's actual batched-setup semantics) was caught and fixed before any number was trusted. Personal zero-trust review caught a real evidentiary gap before committing: the report's first draft cited numbers from an uncommitted run rather than the one saved as its raw log — corrected cell-by-cell to match the actual evidence.
- **[correction, 2026-07-28, R26-2, task #411] The R25-5 "wins on BOTH axes" claim above is only half-confirmed — see `docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §8.** The RSS/commit axis is invalidated (not merely uncertain): the probe ran all arms sequentially in one `--release` process, and the registry's slot reuse + first-claim-wins config (`heap_registry.rs` `claim_with_config`) silently overrides mismatched configs on recycled slots — with the one loud signal (`debug_assert!`) compiled out under `--release` — so RSS rows labelled cap=8/16/32 may have executed under cap=4. The latency/decommit axis stands unaffected (it uses `AllocCore` directly, self-verifies the resolved cap, and confirms 20→0 decommits at cap 4→8). RSS remeasurement is tracked as task #410 (R26-1).
- **[process fix] R25-6 (task #400) — closed without a design attempt: its own conditional gate ("an adaptive budget is warranted only if a fixed-cap raise can't win without an unacceptable RSS multiplier") was not met.** R25-5 found the opposite — the 4->8 step wins on both axes with no tradeoff for an adaptive design to resolve. Dispatching a design task against a disproven premise would have been speculative work; closed with the actual remaining question recorded instead (whether to promote the default, deliberately left to a separate task).
- **[correction, 2026-07-28, R26-2, task #411] R25-6's closure above rested entirely on R25-5's now-invalidated "wins on both axes, no tradeoff" finding, so its conclusion is unsupported and reopened.** Its trigger condition ("an adaptive budget is warranted only if a fixed-cap raise can't win without an unacceptable RSS multiplier") will be re-evaluated once #410's corrected RSS data lands; that reopened work is tracked as task #418 (R26-9), conditional on #410.
- **[measurement] R25-7 (task #401) — filled a disclosed R24-8 evidence gap: `STAGE_CAP=64` confirmed clean at every measured N from 16 to 1024 (not just the originally-measured N=16/64), on both Ir and cache-aware Estimated Cycles.** Real A/B (not an analytical estimate) at N=80/81/128/200/512/1024 against `STAGE_CAP=512`; the win shrinks linearly (109 Ir per extra intermediate `flush_class` call, verified to the unit) but never reaches zero in the measured range — crossover projects at N~2,700, far beyond this project's own "tens to low hundreds" framing of a realistic batch size. Also fixed a real, independently-reproduced build regression found during review: `cargo test --features production` (this project's pre-push gate's own command) was failing with `E0601` on two example probes (from R25-3/R25-5) that were missing a `Cargo.toml [[example]] required-features` entry every other diagnostic example in this project already has.
- **[design] R25-8 (task #402) — a design-only study of a run-encoded arithmetic free list, triggered by R25-3's NO-GO: CONDITIONAL-GO, but explicitly excluding the region that motivated it.** The magazine-overflow free path is a LIFO stack in free-order, not offset-contiguous, so a run-descriptor cannot encode a magazine flush at all; the M2 double-free guard also can't be eliminated for run-blocks, collapsing the free-side win to a single hot-cache-line store — the same class R24-4 already measured as net-negative to coalesce. The one genuinely new, untried lever is the alloc-side `read_next` chain walk, reachable only through `dealloc_batch` (no downstream consumer today). Implementation gated on two independent triggers, neither met; no code written.
- **[correction] R25-9 (task #403) — investigated the review's NUMA-directory recommendation and found it was already resolved 14 rounds earlier.** The review cited a ~140x high-segment-count `numa-aware` cliff from a pre-fix report; re-verified against current source before acting on it — R11-6 (task #234) already replaced the O(S) linear scan with a node-indexed bitmap. No design work opened; closed with a dated note recording that the review's citation was stale, not a new finding.
- **[process fix] R25-10 (task #404) — codified the R25-1 lesson as a standing CLAUDE.md rule: benchmark-only `dbg_*` hooks that touch allocator metadata through a raw pointer must be `unsafe fn` + `bench-internals`-gated, full stop.** Three enforceable sub-rules (raw-pointer hooks must be `unsafe fn` with a documented contract; no-production-caller hooks default to `bench-internals` gating; a NO-GO verdict must re-evaluate its dependent hooks in the same task, not leave them dangling) plus an explicit new checklist item in the existing "After each phase — ZERO-TRUST review" rule. Docs-only, no code/config/test impact.

### Round 24 — free-path magazine-overflow cost decomposed and two bitmap-clear optimizations found NO-GO, `dealloc_batch`'s STAGE_CAP shrunk for a real -47.7% Ir/call win, two measurement-only unsafe hooks moved off the production surface, `OPEN_ITEMS.md` restructured current-state-first, the 1024B churn+teardown residual root-caused to pool-cap-exceeded (R24-1..R24-11, tasks #379–#389)

**Runtime improvements this round: 1, and it is opt-in-only.** R24-8's `STAGE_CAP` 512→64 change (`-4,065 Ir/call`, `-47.7%` of a 16-block same-segment batch-free) lives in `dealloc_batch_small`, which compiles only under the non-`production` `batch-api` feature. **Plain `--features production`'s runtime behavior did not change at all this round** — zero algorithm changes, zero feature-composition changes. Everything else this round is measurement, a NO-GO negative result, or tooling/docs — tagged inline below.

**What actually moved this round, stated plainly up front.** Round 24 continued the free-path cost investigation Round 23 opened: R24-2 decomposed a real free's `Ir` by magazine state (ordinary push vs. a single overflow event, ~12.9x costlier) and found ordinary interleaved hot free never triggers overflow at all — R23-3's earlier "80.8%" headline (already corrected in R24-1, filed under Round 23 above since it corrects a Round 23 finding) was specifically a batch-free-with-overflow workload. Two follow-on optimization attempts in that same overflow-handling code — R24-3 (merge the bitmap-clear pre-pass into `flush_class`) and R24-4 (a `SegmentBitmap` bulk-mask primitive) — both regressed in-context (+37 Ir/event, +14 Ir/block) despite plausible-sounding arithmetic ceilings, and were fully reverted: the compiler had already optimized what each was trying to hand-optimize. R24-5 then split the ~2x cold-path gap vs. mimalloc into alloc-only (1.27x, near parity) and free-only (3.60x, the real problem, 61.5% overflow) halves, confirming R24-3/R24-4 had been looking in the right place even though neither panned out. R24-8 investigated the same `dealloc_batch` internals one round later: an ownership-cache idea was a third NO-GO in the same Heisenberg class, but a 4 KiB stack staging-array's zero-init turned out to be genuinely unelided by LLVM (proven via `--emit=llvm-ir`) — shrinking it to 512 B removed a real, constant, batch-size-independent cost. Separately, R24-6 moved 2 of 4 candidate `#[doc(hidden)] unsafe fn` measurement hooks behind a new non-`production` `bench-internals` feature (a first broader attempt at this exploded to a 130+-file diff and was fully reverted before anything was committed); R24-7 fixed a doc/implementation mismatch in `dealloc_batch`'s warm-range claim (first accepted blocks stay magazine-warm, not last, as the doc had wrongly claimed); R24-9 restructured `OPEN_ITEMS.md` and 3 gate reports so each item/report leads with a compact current-state summary instead of requiring a reader to reach a late correction section; R24-10/R24-11 root-caused a real 1024B wall-clock outlier in `bench_global_alloc_churn_with_teardown` (2.69x slower than mimalloc) to the small-segment retention pool's 4-segment cap being genuinely exceeded by that bench's stress shape — not a decay-tick bug, not the magazine-overflow batch-flush path — a config-tuning finding, not a code defect; no production default was changed. **Plain `--features production`'s feature composition and runtime behavior changed ZERO times this round.** R24-8's `STAGE_CAP` constant lives inside `dealloc_batch_small`, gated on the opt-in `batch-api` feature (not part of `production`), so it does not execute under plain `production` at all — the opt-in `production + batch-api` configuration is the one that changed once. `bench-internals` (R24-6) and the diagnostic `eprintln!`s (R24-11) are additive, non-`production` or docs-only. (Corrected 2026-07-28: an earlier version of this paragraph said production's composition "changed exactly once this round" citing `STAGE_CAP` — that was wrong on both counts, caught by an independent read-only review; see `docs/reviews/2026-07-28-r24-readonly-review.md`.)

#### Runtime improvements

- **[runtime improvement, `batch-api`-only] R24-8 (task #386) — `dealloc_batch`'s on-stack staging array shrunk 512→64 entries, eliminating a real, unelided 4 KiB zero-init cost: -4,065 Ir/call (-47.7% of a 16-block same-segment batch-free, -24.2% of a 64-block one — the identical constant delta at both sizes proves it is pure memset cost, not batch-size-dependent work). Scope: `dealloc_batch_small` compiles only under the opt-in `batch-api` feature, not plain `production` — this is the round's only runtime change, and it is entirely outside the default build.** LLVM-IR proof (`--emit=llvm-ir`) confirmed the `[core::ptr::null_mut(); 512]` array's zero-init survives optimization because its address escapes into `flush_class`, blocking dead-store elimination. A companion ownership-cache investigation (skip redundant `contains_base` probes for repeated same-segment blocks in a batch) was a NO-GO — +3 Ir at N=16, -44 Ir at N=64, an inconsistent sign that is codegen noise, not a real win; the Tier-1 `own_cache` hit it would have short-circuited is already a single compare+branch. New correctness test (`tests/r24_8_dealloc_batch_multi_flush.rs`, N=200 batch forcing 3 intermediate flushes) with a documented mutation counterfactual. Caught and fixed, in the same pass, a stale `docs/ARCHITECTURE.md` test-file count the delegated session's own "full suite PASS" claim had missed.

#### Measurement, correctness & tooling

- **[measurement] R24-2 (task #380) — decomposed a real free's `Ir` cost by magazine state: an ordinary non-overflow push ≈43-44 Ir; a single magazine-overflow event = 571 Ir (12.9x an ordinary push); overflow accounts for 61-69% of R23-3's batch-free-with-overflow headline.** Ordinary interleaved hot free (the common case) never fires the overflow arm at all — reconciled to within 0.8% of R23-3's original 92.5 Ir/free figure, providing the quantitative foundation R24-3/R24-4/R24-5 built on.
- **[measurement, NO-GO] R24-3 (task #381) — prototyped merging the overflow bitmap-clear pre-pass into `flush_class`; measured a real in-context +37 Ir/overflow-event regression, fully reverted.** The original fixed-8-block-length loop was already compiler-unrolled and CSE'd; the merged variant's dynamic-length loop could not be. A standalone measurement of the piece being replaced had overstated its recoverable cost — the decisive gate was in-context measurement, not the arithmetic ceiling.
- **[measurement, NO-GO] R24-4 (task #382) — prototyped `SegmentBitmap::clear_many`/`set_many` bulk-mask primitives at `alloc_batch`'s deferred-clear site; measured a real in-context +14 Ir/block regression, fully reverted.** The per-block RMWs the primitive coalesced were already cheap (~3-4 Ir each, hot L1 cache line); the primitive's own per-offset bookkeeping cost more than it saved. Two bitmap-clear NO-GOs in the same round, in the same code region — a real, useful negative result for future rounds tempted by the same category of "obvious" optimization here.
- **[measurement] R24-5 (task #383) — split the cold-path ~2x gap vs. mimalloc into alloc-only (1.27x, near parity) and free-only (3.60x, the actual problem) halves; the free half is 61.5% magazine-overflow.** The bundled "2.0x" full-round figure was masking a wildly lopsided split; confirms R24-2's overflow mechanism is the dominant remaining lever on the cold free path, and that R24-3/R24-4 had been targeting the right region even though neither optimization panned out.
- **[API surface] R24-6 (task #384) — moved 2 of 4 candidate `#[doc(hidden)] unsafe fn` measurement-only hooks (each with exactly 1 caller) behind a new, non-`production` `bench-internals` Cargo feature.** The 2 pre-existing hooks with ~20 callers (`dbg_push_to_ring`, R6-MS-4) were left as-is with a doc-only justification rather than folded into the same migration. A first attempt at this task tried to reclassify essentially every `dbg_*` hook project-wide (this project's established test-only-export pattern, used by ~130 test files) and exploded into a 130+-file diff before hitting its own context limit — fully reverted (`git checkout --`, scratch scripts cleaned), nothing from it committed; re-scoped narrowly on retry.
- **[doc fix] R24-7 (task #385) — fixed `dealloc_batch_small`'s doc comment, which falsely claimed the LAST `TCACHE_CAP` freed blocks of a batch stay magazine-warm.** The implementation actually keeps the FIRST — the doc had the direction backwards since the feature originally shipped (R11-4). Chose the doc-only fix over a rolling-buffer "actually keep the last N warm" redesign specifically because of R24-3/R24-4's immediately-adjacent NO-GOs in the same cost category — did not even attempt the redesign, reasoning from just-established local precedent rather than re-discovering the same trap a third time.
- **[docs restructure] R24-9 (task #387) — restructured `docs/perf/OPEN_ITEMS.md` (all 12 items) and 3 gate reports (`R22_15`, `R22_16`, `R22_17`) to lead with a compact current-state summary (Status / Current number-or-verdict / Next trigger / Evidence) before the historical narrative, instead of requiring a reader to reach a late correction section to learn the current truth.** All original prose, corrections, and numbers preserved verbatim — nothing deleted, nothing renumbered. Zero items were moved to "Recently resolved": `[A]` Active currently contains only one (still-open) item, so the task's original premise of closed items sitting there did not match the file's actual state — verified by reading the file rather than assumed. Also split this CHANGELOG's Round 23 entry into "Runtime improvements" / "Measurement, correctness & tooling" subsections (not retrofitted further back — earlier rounds' inline tags are not uniform enough to classify mechanically in one pass) so an ~8,700-line diff round isn't misread as a large speedup.
- **[investigation] R24-10 (task #388) — root-caused a user-reported 1024B wall-clock outlier (`bench_global_alloc_churn_with_teardown`, Sefer 2.64x slower than mimalloc) to the already-mostly-fixed Mechanism-2 (task #51) small-segment retention pool, establishing the mechanism but not which of pool-cap / decay-tick / batch-flush explains the current residual.** No files changed — pure investigation, filed as the properly-scoped R24-11 follow-up. (No production or docs change in this task; superseded numerically by R24-11's re-measurement below.)
- **[measurement] R24-11 (task #389) — root-caused R24-10's residual to (i) the small-segment pool's 4-segment/16 MiB cap being genuinely exceeded by this bench's full-teardown-every-iteration shape at 1024B (248 decommit/release events), ruling out (ii) the decay tick (≤1 eviction/sec vs. 248 measured — ~3 orders of magnitude off) and (iii) magazine-overflow batch-flush (size-flat ~5-9us cost present at every size, including the sizes where Sefer is at parity).** Corroborated by `bench_working_set_cycle`'s own counters (0/0/173/373 decommits at 16/64/256/1024B, reproducing its historical 173/367 figure). Config-tuning finding, not a code defect — the cap exists to bound per-thread RSS; an RSS-gated `pool_segments` sweep is flagged as the warranted next step if closing this residual is ever desired, not attempted here. Updated the bench's doc comment from its stale "until task #51 lands Mechanism-2" framing to its current regression-canary role.

### Round 23 — Round 22's own measurements corrected against two independent reviews: `contains_base` re-isolated (8.8%, not 18.6%), mimalloc ratio flips on hot churn (0.896x) via a warm N/2N gate, the free path's real dominant cost found (own-thread body, 80.8%), R22-16's flawed neighbor-liveness blocker retracted (Linux sub-region remap now CONDITIONAL-GO), 11 pre-existing clippy dead-code errors closed, one flaky test replaced with a deterministic counter, batch API consumer question closed by decision record (R23-1..R23-7, tasks #370–#376)

**Runtime improvements this round: 0** — Round 23 was a correction / measurement / tooling round; `production`'s feature composition is unchanged across the whole round (see "Production vs. opt-in" below). Every entry below is measurement, correctness, design-correction, or tooling (tagged inline) — none is a runtime speedup.

**What actually moved this round, stated plainly up front.** Round 23 is a
correction round: two independent read-only reviews of Round 22
(`docs/reviews/2026-07-26-r22-readonly-review.md`,
`docs/reviews/2026-07-27-post-r22-followups-readonly-review.md`) found that
several of Round 22's own headline measurements did not hold up under
tighter methodology, and — most notably — that Round 22's own R22-16 design
doc contained a real logic error in a verdict this session had itself
committed. Every finding from both reviews was personally re-derived
against the actual source before being acted on, per this project's
zero-trust convention; none were accepted at face value. Round 23 landed 7
tasks: 3 measurement corrections, 1 design-verdict correction, 1
correctness/CI fix, 1 test-infrastructure fix, and 1 product decision record
— plus several smaller fixes caught incidentally along the way and folded
into the same commits (a README unsafe-inventory-count regression, a
12-round-stale `OPEN_ITEMS.md` entry). Plain `--features production`'s
composition is **unchanged** across the whole round — every new item is
either `#[doc(hidden)]`/`alloc-stats`-gated measurement tooling or a
docs-only edit; `git diff --stat` on `Cargo.toml` across the round is empty.

#### Runtime improvements

_None this round._ — Round 23 shipped no runtime speedup; every entry below is measurement, correctness, design-correction, or tooling (see the inline tags). `production`'s feature composition is unchanged.

#### Measurement, correctness & tooling

- **[measurement correction] R23-1 (task #370) — `contains_base`'s isolated
  share of a real free's `Ir` is 8.8%, not R22-17's original 18.6%.** The
  first review found R22-17's probe arm bundled `segment_base_of_ptr`'s own
  arithmetic and a second non-inlined call boundary into one "contains_base"
  label. Added a genuinely isolated `dealloc_segment_base_of_ptr_probe_only_16b`
  arm; decomposition from two independent, byte-identical `npm run iai` runs:
  `segment_base_of_ptr` alone is 9.8% (578/5,920 `Ir`), `contains_base` alone
  is 8.8% (523/5,920) — summing back to the original 18.6% exactly, with no
  unaccounted residual. Original figure preserved as published history; the
  corrected number is what future rounds should cite.
- **[measurement correction] R23-2 (task #371) — a warm N/2N matched gate
  replaces R22-15's asymmetric bootstrap-subtraction, and the correction
  MATERIALLY changes the headline, not just its decimals.** R22-15's
  Sefer-vs-mimalloc `Ir` ratio subtracted two differently-sized one-shot
  "bootstrap proxy" constants (3,308 Ir for Sefer, ~4x that for mimalloc)
  from two totals, then divided by the same op count — the review found this
  skews the ratio in mimalloc's favor. Replaced it with `c = (Ir(2N) -
  Ir(N)) / N` across matched op-count pairs, which cancels the one-time
  per-process bootstrap constant algebraically with no external proxy
  needed. Result: the hot-churn ratio **flips** from 1.326 to **0.896**
  (SeferAlloc becomes marginally *cheaper* than mimalloc per op, the
  opposite of the original headline), and the cold-carve ratio shrinks from
  2.430 to **~2.0–2.08** (direction unchanged, magnitude down ~18%). A
  3-point N/2N/4N linearity check found a genuine small (3.7–7.4%)
  non-linearity in both allocators, reported honestly, not large enough to
  change the qualitative conclusion.
- **[measurement] R23-3 (task #372) — full orthogonal hot-path attribution
  finds the free path's real dominant cost: the own-thread free body,
  80.8%, more than 4x the routing prefix (`contains_base` +
  `segment_base_of_ptr`, 18.6% combined) prior rounds focused on.** Built 6
  new N/2N- and shared-prefix-isolated bench arms plus 4 new `#[doc(hidden)]`
  measurement hooks (`dbg_hash_contains_only`,
  `dbg_dealloc_own_thread_with_base`) to decompose hot alloc-magazine-hit
  (22.4 Ir/op), free-routing Tier-1/Tier-2 `contains_base`, the fused M2
  double-free-oracle + magazine-push free body (80.8%), and cold
  carve-vs-recycle-pop. Finding: recycle-pop (188.2 Ir/op, full path) is
  roughly on par with virgin-carve (203.86 Ir/op), not costlier — revising
  R22-15/R23-2's "cold-carve/recycle is the main remaining candidate"
  framing. Two self-caught methodology bugs (an invalid N/2N pair, a missing
  `#[inline(always)]` that inflated one measurement past the real free
  loop's own total) were disclosed and fixed in the same task, not carried
  forward. Caught and fixed a real regression along the way: the new
  `unsafe fn` hook pushed the tier-2 unsafe-seam count 60→61, and
  README.md's own `readme_unsafe_inventory_counts_match_reality` test
  correctly went red until the README's count was updated in the same
  commit.
- **[correction] R24-1 (task #379) — the R23-3 "80.8%" headline directly
  above describes a 64-block batch-free-with-overflow workload, not ordinary
  hot free.** Re-verified against current source: the bench arms free 64
  distinct pointers in one sequential pass, hitting the magazine overflow arm
  (`cnt == TCACHE_CAP = 16`) six times (at frees #17/25/33/41/49/57); so the
  74.70 Ir/free averages 58 non-overflow pushes with 6 overflow events
  (bitmap-clear + `flush_class` on 8 blocks each + 8-pointer compaction), not
  an isolated "M2 oracles + magazine push" cost. Cross-check: 22.38 (alloc
  hit) + 92.50 (this free) = 114.88 > the entire 69.0 Ir hot pair, so the
  92.50 free is NOT the free half of that pair — the workloads measure
  different magazine states. Corrected next step is the R24-2 (task #380)
  measurement split (decompose free by magazine state), not remediation.
  Docs-only correction; no `src/` behavior change. Full arithmetic:
  `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §9 (original §0–§8
  preserved verbatim); item-1 note updated in `docs/perf/OPEN_ITEMS.md`.
- **[design correction] R23-4 (task #373) — corrected a real logic error in
  this project's own R22-16 design doc: Linux sub-region `mremap` is
  CONDITIONAL-GO, not NO-GO.** R22-16 argued sub-region remap needed an
  unsolved "promotion-time neighbor-liveness check." Independently
  re-verified (personally, before *and* during delegation) that this premise
  is false: `carve_block`/`carve_batch` always advance the bump cursor
  monotonically forward, and the only backward bump reset
  (`decommit_empty_segment_impl`) is reachable, on every production path,
  only after the whole segment's `live_count` is confirmed zero — so a live
  carved block's byte range is provably exclusive for its entire lifetime,
  no runtime check needed. Whole-segment remap's NO-GO (base-address
  stability) is unaffected and remains real. New finding beyond the expected
  correction: today's memcpy-based promotion frees its source block through
  the *ordinary* `dealloc`→`BinTable` free-list path, so a future remap
  design must still avoid ever routing a remap-vacated offset through
  ordinary free — bump-monotonicity alone does not solve this, disclosed as
  a genuinely open (not currently blocking) design discipline.
- **[correctness fix] R23-5 (task #374) — closed all 11 pre-existing
  `cargo clippy --features "hardened medium-classes" -D warnings` dead-code
  errors, stable since R19-1 across 3+ rounds.** All 11 were genuine
  `#[cfg(...)]` predicate mismatches (an item gated one way, its sole
  consumer gated a different way, so under this specific feature
  intersection the consumer compiled out but the item did not) — confirmed
  exhaustively per item via whole-crate grep before touching anything; none
  were true orphans, nothing deleted. Added a `clippy (--features "hardened
  medium-classes")` row to CI, closing a gap R22-1 deliberately left open
  when it added a `cargo test` row for this combo but not clippy. One latent
  predicate-mismatch issue in a test file, exposed only once the lib
  compiled clean and `--all-targets` reached that target for the first
  time, was found and fixed in the same pass.
- **[test infrastructure] R23-6 (task #375) — replaced one of two flaky
  coarse wall-clock tests with a deterministic counter; honestly demoted the
  other.** `backshift_no_latency_spike_at_threshold_boundary` got a
  deterministic replacement: a new `alloc-stats`-gated
  `HASH_REMOVE_MAX_SCAN_STEPS` high-water-mark counter for
  `SegmentTable::hash_remove`'s backward-shift scan, asserted against a
  deterministic regression threshold (`4 * W`, calibrated to this wave —
  reliably catches a full O(HASH_CAPACITY) regression; not a proven O(cluster)
  worst-case) instead of a nanosecond ratio — zero flake surface. Verified
  non-vacuous via a mutation counterfactual (force the scan to burn
  `HASH_CAPACITY-1` extra steps; confirmed the new test fails; reverted),
  run independently twice (once by the delegated task, once personally).
  `own_thread_free_is_subquadratic` has **no** clean deterministic
  replacement — the guard it protects is an unconditional O(1) bitmap test
  with no loop left to instrument — honestly demoted to `#[ignore]` rather
  than forcing a fake counter. An earlier-proposed `TEST_LOCK`-mutex fix was
  correctly NOT used: the flakiness source is cross-process CPU contention
  (multiple test binaries), which a mutex inside one process cannot
  address. Both original wall-clock tests are kept, not deleted, for manual
  `--ignored`/`npm run iai` cross-checks.
- **[decision] R23-7 (task #376) — batch API downstream-consumer question
  closed by decision record; no new benchmark built.** An independent review
  flagged that the batch API has a measured win (R10-7, 1.1–1.6x) but no
  real downstream caller, so its effect on typical `Box`/`Vec`-shaped usage
  is zero. Investigated whether a more realistic benchmark than what already
  exists could be cheaply built and found it already does: R10-7's
  `batch_tcache` arm goes through the warm magazine and is measured against
  the real warm `SeferAlloc` scalar path across a realistic batch-size
  sweep — building a 4th-generation microbench would add no information.
  Confirmed by grep that `alloc_batch`/`dealloc_batch` have exactly one call
  chain in `src/` (`SeferAlloc`→`HeapCore`, both under the non-`production`
  `batch-api` feature) — no in-tree production caller exists. Wrote
  `docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md` with an explicit
  3-trigger falsifiability clause instead. Caught and fixed, in the same
  pass, a 12-round-stale `OPEN_ITEMS.md` entry (R9-9's warm-batch-arm ask,
  actually resolved by R10-7 the very next round, but never marked closed
  because that commit never touched the index).

**Production vs. opt-in.** `production`'s feature composition is unchanged
across all of Round 23 (`git diff --stat Cargo.toml` from `main`@`ff48029`
through this round's tip is empty). The unsafe-seam count moved from 80 to
**81** (20 tier-1 + 61 tier-2, R23-3's one new `unsafe fn` measurement hook
— `README.md`'s own inventory tripwire test caught and enforced this),
verified via `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`.

### Round 22 — three independent Rounds 19–21 reviews synthesized, `hardened+medium-classes` CI gap closed, `large_layout_consistent` extended to align, OPT-H closed on geometric grounds, mimalloc `Ir` arm landed (1.3x-2.4x gap measured), `contains_base` found MATERIAL (18.6% of free), `medium-classes`' product fate decided (R22-1..R22-18, tasks #352–#369)

**What actually moved this round, stated plainly up front.** Round 22 opened
by dispatching two independent read-only reviews (`/crush` and `@ox`) plus a
third pre-existing readonly review of Rounds 19–21, synthesizing all three
into `docs/reviews/2026-07-26-r22-plan.md`, and executing every
zero-trust-verified finding that survived personal re-derivation against the
real source. Plain `--features production`'s composition is **unchanged**
across the whole round (`git diff --stat` on `Cargo.toml` shows only
additive `[[example]]`/`[[bench]]`-adjacent entries, no `[features]` change)
— every behavior-affecting addition (`HARDENED_LARGE_NOOP_COUNT`,
`dbg_contains_base`) is `#[doc(hidden)]`/`alloc-stats`-gated and compiles to
nothing under plain `production`. This round is a mix of real correctness
fixes (2), CI/test-robustness fixes (4), honest measurement (3, one of which
found a MATERIAL, not NULL, result), design work (1), a product decision (1),
and process/doc hygiene (7) — the same explicit-categorization discipline
Round 19–21's own CHANGELOG entry established, continued here.

- **[correctness fix] R22-1 (task #352, P0) — added the CI row that was
  missing for R19-1's own regression tests.** Walked `.github/workflows/
  ci.yml` and confirmed the hardened Large no-op fix (R19-1) and BOTH its
  regression tests compiled to ZERO tests in every existing CI row (the
  `hardened`-tier job never turns on `medium-classes`; `--all-features`
  turns on `numa-aware`+`exact-span-large` together, which makes the
  promotion-reachable predicate false). Added `cargo test --features
  "hardened medium-classes"` to the `test-hardened` job.
- **[correctness fix] R22-2 (task #353, P0) — closed a real gap in R21-2's
  own non-vacuity test.** The existing negative OPT-H scenario
  (`objs[0]`, offset 786432) failed precondition 4 (alignment)
  independently of precondition 3 (tail-adjacency) it claimed to isolate —
  deleting `tail_adjacent &&` from the real conjunction would have left the
  suite green. Added a third scenario (384 KiB→1 MiB, offset 3 MiB) whose
  offset is aligned but non-tail, genuinely isolating precondition 3;
  proved via a personal mutation counterfactual that only the new scenario
  flips red when `tail_adjacent` is neutralized.
- **[process fix] R22-3 (task #354, P1) — created
  `docs/CORRECTNESS_OPEN_ITEMS.md`**, a sibling index to the perf-only
  `docs/perf/OPEN_ITEMS.md`, after two independent reviews found R19-1's
  self-flagged flaky test and clippy dead-code combo tracked nowhere
  durable — the exact failure mode `OPEN_ITEMS.md` was created to prevent,
  recurring one domain over. Recorded both items with reproduction
  evidence and a concrete root-cause hypothesis each.
- **[doc fix] R22-4 (task #355, P2) — fixed a stale commit-status
  self-description** in `R21_2_OPT_H_STAGE1_HIT_RATE.md` ("Not committed,
  not pushed", despite being committed as `b6af12d`) — the same
  derived-statement staleness class Round 19 spent 4 commits fixing,
  recurring one file later in the very same wave.
- **[correctness fix] R22-5 (task #356, P1) — extended
  `large_layout_consistent` to check alignment, not just size.** A
  fabricated free with the right size but wrong align previously passed
  both the R19-1 hardened branch and the pre-existing cross-thread
  mitigation. Added `SegmentHeader::large_align_at` and three new tests
  (own-thread, cross-thread, and a cache-hit-reuse scenario); proved via a
  personal mutation counterfactual that exactly the 3 new tests, and only
  those, depend on the new align check.
- **[docs/perf fix] R22-6 (task #357, P1) — closed OPT-H's medium-ladder
  item with a closed-form LCM proof**, not a further measurement: the
  ladder's own size-class factorization allows at most one cross-class hop
  per segment, independent of harness design. Moved `OPEN_ITEMS.md` item 1
  to "Recently resolved"; caught and fixed a numbering bug the tier
  reshuffle itself introduced in a separate, already-stale entry.
- **[doc fix] R22-7 (task #358, P2) — added the Rounds 19–21 CHANGELOG
  section** (the one immediately below this one), with the explicit
  work-type categorization this same convention continues here.
- **[doc fix] R22-8 (task #359, P2) — added a lazy-commit overcount
  caveat** to `R21_2_OPT_H_STAGE1_HIT_RATE.md`: the Stage-1 measurement ran
  under `primordial-lazy-commit` (part of `production`), where precondition
  6 is an unverified upper bound — harmless here since the result was
  exactly 0, but any future non-zero reading in the same configuration must
  be read as an upper bound, not an exact count.
- **[test infrastructure] R22-9 (task #360, P2) — gated R19-1/R22-5's
  branch-(A) tests at runtime, not via `#[cfg]`.** The hand-written
  promotion-reachable `#[cfg]` on 3 test functions meant they didn't even
  compile under `--all-features` or plain `hardened`. Replaced with
  `if !HeapCore::dbg_promotion_compiled() { return; }`, the same idiom
  `tests/r14_4_promotion_move_leg_reduction.rs` already established —
  confirmed all 4 tests now compile AND runtime-skip correctly in both
  configurations.
- **[doc fix] R22-10 (task #361, P2) — distinguished commit charge from
  RSS in the R20-2 report.** The report's own table showed commit falling
  while RSS roughly tripled in the same comparison; "resident commit"
  conflated two axes moving in opposite directions.
- **[test verification] R22-11 (task #362, P0) — confirmed the OPT-H probe
  holds under `--all-features`**, the only CI row that actually compiles
  and runs it — previously never verified in that exact configuration.
  Both tests pass unchanged; recorded the confirmation in the module doc.
- **[diagnostic/observability] R22-12 (task #363, P3) — added a counter
  for hardened Large defensive no-ops** (`HARDENED_LARGE_NOOP_COUNT`,
  `alloc-stats`-gated), shared by both no-op branches. Strengthened all 3
  R19-1/R22-5 regression tests to assert it advances by exactly 1, in
  addition to the existing liveness check — proving the no-op branch
  itself ran, not just that nothing crashed.
- **[test infrastructure] R22-13 (task #364, P3) — closed the tripwire's
  third link.** R19-9's v4 tripwire pinned real-constants↔`EXPECTED_BYTES`
  but nothing verified `EXPECTED_BYTES`↔the doc-comment prose snapshot.
  Added a test parsing `dirty_by_class.rs`'s source text and asserting the
  exact byte-count tokens are present.
- **[process fix] R22-14 (task #365, P2) — defined a boundary rule for
  what perf-gate report owes raw logs + a summary CSV**, closing an
  ambiguity R21-2 exposed (it published 0/320 and 0/20 as its decision
  basis but committed neither). Applied retroactively: promoted R21-2's
  throwaway measurement wrapper to a small permanent committed example,
  reproducing the exact already-published result.
- **[measurement] R22-15 (task #366, P1) — landed the mimalloc `Ir`
  comparison arm** in the deterministic Callgrind gate, executing R20-4's
  already-approved feasibility sketch. **Measured result: SeferAlloc
  retires 1.3x-2.4x more instructions per op than mimalloc on every
  matched workload** (1.326x hot churn, up to 2.430x cold-carve/recycle) —
  a real, honestly-reported, unfavorable gap, settling a 10-round
  wall-clock argument deterministically. Confirmed byte-identical `Ir`
  across three independent runs.
- **[design-only] R22-16 (task #367, P1) — designed remap-instead-of-copy
  for the promotion memcpy**, the last untried lever after destination
  headroom (NULL) and in-place grow (closed). **Verdict: NO-GO** for
  remap-in-place under the current shared-segment model (two independent
  architectural blockers: no promotion-time neighbor-liveness check;
  segment base-address stability is load-bearing throughout the addressing
  model). **CONDITIONAL-GO** for a separate future "MediumExtent" redesign,
  gated on a cheap Stage-1 workload-shape measurement.
- **[decision] R22-18 (task #369, P2) — decided `medium-classes`' product
  fate: neither ship in `production` nor remove, document as a named
  opt-in workload profile.** After 4 independent NULL/NO-GO attempts across
  3 rounds to clear the realloc axis, this was the first time the question
  "should this feature ship at all" was asked directly. Recorded an
  explicit falsifiability clause naming the only 3 categories of evidence
  that could reopen the decision.
- **[measurement] R22-17 (task #368, P1) — measured `contains_base`'s
  share of a real free's `Ir`: MATERIAL, 18.6%**, not the negligible slice
  a NULL result would have shown. Isolated the probe via 3 new bench arms
  reusing R22-15's just-established pattern; confirmed byte-identical
  across two independent runs. Sketched (design only) a header-first
  alternative mirroring mimalloc's approach, with an explicit soundness
  caveat: it would dereference memory before proving liveness, which this
  crate's own hardened-misuse-guard philosophy treats as unacceptable, not
  a trade-off — no free-lunch solution was found in this task's scope.

**Production vs. opt-in — what actually changed for default `--features
production` users.** Nothing behavioral. `Cargo.toml`'s `production = [...]`
list is unchanged across the entire round. The two new always-compiled
statics (`HARDENED_LARGE_NOOP_COUNT`, plus the pre-existing pattern
`dbg_contains_base` reuses) are both `#[doc(hidden)]` and/or
`alloc-stats`-gated for their per-event cost — zero-cost under plain
`production`, verified per their own commits above. Unsafe-seam inventory
unchanged (80 total: 20 tier-1 + 60 tier-2, matching every prior round's
count, via the crate's own self-verifying `grep -rnE
'^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`).

### Rounds 19–21 — hardened Large no-op UAF closed, Round 18 CHANGELOG backfilled, promotion-reachable `#[cfg]` canonicalized into one macro, R10-2 kill-gate's reserved-capacity lever measured NULL, OPT-H in-place medium-grow designed (CONDITIONAL-GO) then found NO-GO on Stage-1 evidence, mimalloc Ir-arm feasibility confirmed (R19-1..R19-9, R20-1..R20-4, R21-1..R21-2)

**What actually moved across all three rounds, stated plainly up front.**
Plain `--features production`'s composition is byte-identical across the
entire span — verified via `git diff --stat 46ea2db^..b6af12d -- Cargo.toml`,
which shows only `26 insertions(+)`, all of it two additive `[[example]]`
bench-harness blocks for R21-1 (`paired_ab_hot_buffer_off`/`_on`); no
`[features]` table entry changed. Of the one real `src/` behavior change in
the whole range — R19-1's hardened Large no-op fix — the corrected code path
is unreachable under `production`: it requires BOTH `hardened` AND
`medium-classes` compiled in together, and neither feature is in
`production`'s list. So the honest answer to "did Rounds 19–21 speed
anything up?" is **no** — nothing in this span changes plain `production`'s
runtime behavior or performance at all; R19-1 is a defensive-correctness fix
gated behind an opt-in hardening feature, and the rest of the three rounds is
measurement (mostly NULL/NO-GO results), design work, feasibility analysis,
and process/doc cleanup. This mirrors the "Production vs. opt-in" framing the
Round 18 section below already uses, extended across three rounds because the
same conclusion holds for all of them.

#### Round 19 (`46ea2db`..`4ca952d`, 9 commits, tasks #337–#345), 2026-07-26 07:03..08:42 — the immediate zero-trust follow-up queue against Round 18's own review, plus the backfilled Round 18 CHANGELOG entry itself

- **[correctness/security fix] R19-1 (`46ea2db`, task #337, P1) — closed a
  real hardened Large no-op contract violation reachable under promotion.**
  R18-3's branch (A) in `HeapCore::dealloc_own_thread_with_base` routed ANY
  small-layout free of a Large-kind segment straight to `self.core.dealloc`
  whenever `medium-classes`' promotion predicate compiled in — correct for a
  legitimate promoted-and-grown block, but under `hardened` it also silently,
  ACTUALLY freed the segment for a fabricated/mismatched small layout on a
  never-promoted Large pointer, instead of the detected-no-op contract
  `hardened` (task #25) promises. Fix: gate the real dealloc call, under
  `hardened`, on `large_layout_consistent(base, layout.size())` (the same
  task #138 primitive `heap_core_xthread.rs` already uses for the analogous
  cross-thread check) — a mismatch now degrades to the same defensive no-op
  branch (B) uses; the non-`hardened` path is untouched. Personally verified
  with a real red/green counterfactual: reverting only the source fix and
  re-running the new branch-A no-op test under `--features "hardened
  medium-classes"` crashes with a genuine `STATUS_ACCESS_VIOLATION` — proof
  this was a real, exploitable UAF, not a theoretical concern. Unreachable
  under plain `production` (needs `hardened` + `medium-classes` together,
  neither in `production`).
- **[doc/process fix] R19-2 (`ee3a3d7`, task #338, P2) — resolved a stale
  `OPEN_ITEMS.md` entry.** Item #12 (NUMA node-aware bit selection) described
  a pre-R11-6 state; R11-6 (task #234) already added the node-indexed
  `class_nonempty_by_node` bitmap. Moved to "Recently resolved", remaining
  items renumbered gap-free.
- **[doc/process fix] R19-3 (`5c3bf76`, task #339, P2) — finished R18-7's
  self-correction.** The `4ba35dc` follow-up had corrected sections
  0/4/5/6/8 of `R18_7_MIMALLOC_GAP_STATUS.md` but missed one residual copy of
  the retracted "no perf-gate job exists" claim in §7 ("Files inspected") —
  independently caught by both the `@oh` and `/crush` Round 18 reviews. Fixed
  the bullet and added `perf-gate.yml` as its own cited evidence-trail entry.
- **[doc/process fix] R19-4 (`979b9d5`, task #340, P2) — fixed a stale
  line-range citation in `912740f`'s own commit message.** The cited
  `heap_core_free.rs:929-933` was actually the unrelated R4-2 null-base
  guard; the real supporting text is `try_promote_to_large`'s own doc
  comment. Not amending the already-landed commit message (git-safety
  convention) — added a historical citation note in-place instead.
- **[doc/process fix] R19-5 (`36b7cc7`, task #341, P2) — backfilled the
  missing Round 18 CHANGELOG entry** (the "### Round 18" section
  immediately below this one), flagged by both the `@oh` and `/crush` Round
  18 reviews. Matches the established Round 15–17 format; cross-references
  the three Round 19 follow-up corrections (R19-1, R19-2, R19-3) landing in
  this same round queue so a reader doesn't mistake the original Round 18
  entries for their final state.
- **[doc/process fix] R19-6 (`1b6c6cd`, task #342, P2) — three purely
  additive methodology clarifications to the R14-4 gate report.** No
  numbers/tables/claims changed: (1) a leading callout in §0 pointing to
  §7.1's current ~1,180×/~380× verdict (R18-2) before the original stale
  ~1,700–2,300× table; (2) a §10.3 caveat that the "full-round" figure is an
  arithmetic sum of three independently-paired phase means, not its own
  paired statistic; (3) a §10.6 note recommending a future re-run read
  `AllocStats::large_cache_hits` directly instead of inferring it.
- **[test infrastructure] R19-7 (`a302d16`, task #343, P1) — the
  `race_repro` watchdog's thread panic now fails the test, not just logs
  it.** `impl Drop for Watchdog` (R18-1) previously only `eprintln!`'d on a
  watchdog-thread panic — easy to miss in CI scrollback, with `cargo test`
  reporting the test as passed regardless. Fix: re-panic on the main thread,
  guarded by `!std::thread::panicking()` (so an already-unwinding return
  doesn't trigger a double-panic abort, preserving R18-1's disambiguation
  discipline). Verified with a real red/green counterfactual: an injected
  unconditional watchdog panic now fails the test as designed; the fix
  short-circuited back to `if false && ...` reproduces the old silent-pass
  gap.
- **[test infrastructure] R19-8 (`e7dbe16`, task #344, P2, refactor — no
  behavior change, say so explicitly) — canonicalized the
  promotion-reachable `#[cfg]` predicate into one macro.** The predicate
  (`medium-classes && (!exact-span-large || (large-reserved-capacity &&
  !numa-aware))`) was hand-duplicated at 4 sites in `heap_core_free.rs` — a
  drift risk flagged by Round 19's own synthesis review. Introduced
  `medium_promotion_reachable!`, wrapping either an item or a statement in
  the identical `#[cfg(...)]` gate; rewired all 4 sites to invoke it, bodies
  and doc comments unchanged. Purely textual canonicalization — no logic,
  no behavior change; verified across all 6 promotion-reachable ON/OFF
  transitions plus the existing regression suites, all green.
- **[test infrastructure] R19-9 (`4ca952d`, task #345, P2) — fixed the
  stale-literal-prone comment in `dirty_by_class.rs` (flagged by R18-6) plus
  a v4 tripwire test.** Rewrote the module doc comment per R18-6's v3
  convention (cite the const names/formula first, concrete numbers as an
  explicitly-labeled "as of this writing" snapshot). Added a v4 tripwire
  (`tests/dirty_by_class_sidecar_sizing_tripwire.rs`) recomputing the
  sidecar's byte footprint from the real compiled constants, so a future
  size-class change fails this test instead of silently leaving the prose
  stale. Verified non-vacuous with a deliberately-wrong `EXPECTED_BYTES`
  counterfactual (goes red as expected, reverted to green).

#### Round 20 (`6b5390d`..`e5addae`, 4 commits, tasks #346–#349), 2026-07-26 08:56..09:53 — measurement, design, and feasibility work against `OPEN_ITEMS.md`'s two remaining Active items

- **[doc/process fix] R20-1 (`6b5390d`, task #346, P2) — fixed stale
  "pending the Linux Ir gate" wording.** `perf-gate.yml` (task #127/#128)
  already runs the deterministic Ir gate on `ubuntu-latest`, but
  `CHANGELOG.md` and `docs/ALLOC_BENCH.md` still framed the P7 cold-recycle
  verdict as future-tense in five places. Both are historical/point-in-time
  records, so original prose is kept intact — a "(Resolved: ...)" note added
  after each stale sentence instead. Also fixed `OPEN_ITEMS.md`'s own item
  #3 citation, itself stale/wrong (pointed at an unrelated M2-guard
  sentence) — the same citation-drift class R19-4 fixed for a commit
  message. Left one genuinely different, still-open "pending that gate"
  mention untouched after verifying it refers to a different gate.
- **[measurement — NULL result] R20-2 (`ee5f2aa`, task #347, P1) — C4 gate:
  reserved-capacity headroom does NOT reduce the promotion `memcpy`.**
  Measured `OPEN_ITEMS.md` active item 2: does `large-reserved-capacity`'s
  geometric growth headroom (with `exact-span-large`) reduce the structural
  medium→Large promotion `memcpy` R18-2 found and left RED? A direct,
  load-matched paired A/B/B/A comparison (20 pairs/80 launches) between C1
  (`production,medium-classes`) and C4 (`production,medium-classes,
  exact-span-large,large-reserved-capacity`) gave mean delta +967 µs, SD
  3.577 ms, t=1.209 (<< crit 2.101), sign test dead-even 10/20 —
  statistically indistinguishable from noise. Confirms R18-2's own §10.7
  mechanism prediction: the promotion `memcpy` happens before the fresh
  Large segment's `reserved_capacity` is established, so headroom can only
  help a later grow, never the copy that created the promotion. Reported a
  methodological finding honestly: a naive comparison against R18-2's
  previously-published number looked like a ~24% improvement, collapsing to
  ~5%, then to noise, once measured fresh in the same session — a
  cross-session host-load artifact, not a feature effect. Genuine orthogonal
  finding: `exact-span-large` roughly halves resident commit for this
  workload, unrelated to the realloc-speed null result. R10-2's kill-gate
  remains RED; moved `OPEN_ITEMS.md` active item 2 to "Recently resolved".
- **[design-only, no implementation] R20-3 (`9a4fe15`, task #348, P1,
  CONDITIONAL-GO) — designed OPT-H, an in-place medium-class grow
  mechanism.** The first document to propose an actual mechanism for
  `OPEN_ITEMS.md`'s active item 1 (the promotion-time `memcpy` itself,
  confirmed by three rounds of measurement — R14-4, R18-2, R20-2 — to be the
  genuine remaining lever), rather than just naming the gap. Grounded
  against the real carve/`BinTable` substrate: identifies OPT-H, a
  tail-of-segment bump-extend for a block that is currently its segment's
  most-recently-carved, not-yet-grown-or-freed block, with zero new
  `SegmentHeader` fields and zero new `BinTable` variants. Honest scope
  assessment: structurally bounded to one eligible block per segment at a
  time, so it explicitly predicts it will NOT close R10-2's own
  N=16-simultaneous-object harness — its real target is the un-measured
  single-hot-growing-buffer pattern. Verdict: CONDITIONAL-GO, gated on a
  not-yet-built single-hot-buffer diagnostic harness showing a material hit
  rate (this became R21-1/R21-2). `OPEN_ITEMS.md` active item 1 updated in
  place (not moved to "Recently resolved" — design is not implementation).
- **[feasibility-only, no implementation] R20-4 (`e5addae`, task #349, P2) —
  mimalloc Ir-arm feasibility: FEASIBLE, cheaper than assumed.** Answered
  `OPEN_ITEMS.md`'s last remaining Active item — whether a deterministic
  cross-allocator Ir number can settle the cold-16B mimalloc gap argued on
  wall-clock alone for 10 rounds. VERDICT: FEASIBLE, no architectural
  blocker: mimalloc's C core is statically linked (no dynamic-link/JIT
  attribution gap); the assumed need for a separate bench binary (one
  `#[global_allocator]` per process) does not apply, since neither existing
  bench file ever installs one — a mimalloc arm can live in the same
  `perf_gate_iai.rs` file; the CI C-toolchain question is already retired
  (`--all-features` clippy already compiles the mimalloc-linking bench on
  the same runner image). One non-blocking nuance: `scripts/iai.mjs`'s
  marginal-Ir/op column needs its own bootstrap-proxy bench so a mimalloc
  constant isn't conflated with SeferAlloc's. `OPEN_ITEMS.md` item 2 updated
  in place, staying Active (implementation is still future work) — this
  closed out `OPEN_ITEMS.md`'s Active tier entirely for the round, though
  neither item is fully implemented.

#### Round 21 (`517a85b`..`b6af12d`, 2 commits, tasks #350–#351), 2026-07-26 10:58..12:35 — OPT-H's own Stage-1 measure-before-build discipline, per R20-3's own gate

- **[bench harness, no src change] R21-1 (`517a85b`, task #350, P2) —
  built the single-hot-buffer harness OPT-H's Stage 1 needs.** The existing
  R10-2/R18-2/R20-2 harness (16 simultaneously-live objects) is deliberately
  adversarial and structurally cannot represent the single-hot-buffer
  workload (Vec-style repeated append) OPT-H actually targets. New harness:
  ONE buffer, allocated once (untimed), repeatedly grown through the exact
  medium-class ladder, reset and repeated for 20 rounds; two wrapper
  binaries (`paired_ab_hot_buffer_{off,on}`) share one workload file,
  mirroring the existing `paired_ab_medium_{off,on}` pattern so the
  existing statistics engine works unmodified. Harness-only: no `src/`
  change, no allocator logic change, one additive Cargo.toml `[[example]]`
  block per binary (2 total). Personally verified: built and ran both
  binaries — off (baseline) ~310 ns/round via OPT-G's existing in-place
  Large grow; on (promotion path) ~31,120 ns/round, ~100× slower, matching
  the expected repeated-first-crossing-promotion-cost shape.
- **[observation-only diagnostic, NO-GO verdict] R21-2 (`b6af12d`, task
  #351, P1) — OPT-H Stage-1 diagnostic counters: NO-GO on current
  evidence.** Implemented OPT-H's precondition-checking logic (the six
  conditions from R20-3's design §2.1) as OBSERVATION-ONLY diagnostics
  (`OPT_H_ATTEMPTS`/`OPT_H_HITS`, storage always compiled, increment gated
  behind `alloc-stats`) inside `AllocCore::realloc_inplace_fast_path_known_
  base`'s existing OPT-F decline arm — it never changes what pointer is
  returned or what memory is touched; every cross-class grow still falls
  through unchanged. New regression test proves the precondition logic
  actually discriminates (a genuinely tail-adjacent grow vs. the same shape
  on a non-tail block), not merely compiles. Stage-1 measurement result:
  R10-2's existing N=16 harness shows 0/320 attempts hit (matches the
  design's own prediction); R21-1's new single-hot-buffer harness ALSO
  shows 0/20 — root-caused to a structural property of that harness's own
  construction (its buffer already sits at the promotion threshold, so it
  promotes to Large on its very first grow crossing every round). Verdict:
  CONDITIONAL-GO trigger NOT MET — NO-GO for implementing OPT-H's real grow
  action on current evidence; not a rejection of the mechanism's soundness,
  since neither available harness demonstrates the predicted victim
  workload materializing. Full trace in
  `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md`. `OPEN_ITEMS.md`'s active item
  1 updated with this verdict.

**Production vs. opt-in — what actually changed for default `--features
production` users, across all three rounds.** Nothing. `Cargo.toml`'s
`production = [...]` list is unchanged across the entire `46ea2db^..b6af12d`
span (only two additive `[[example]]` bench-harness blocks were added, no
`[features]` entry touched — see the summary paragraph above for the exact
diffstat). The one real `src/` behavior change, R19-1, is a hardened-only
defensive-correctness fix unreachable under plain `production` (it requires
`hardened` AND `medium-classes` together, neither of which `production`
carries). R19-8's macro refactor and R21-2's diagnostic counters both compile
out entirely under plain `production` (zero-cost, verified per their own
commits above). Everything else across the three rounds — R19-2 through
R19-6, R19-9, R20-1, R20-3, R20-4, R21-1 — is docs/process/design/
measurement/feasibility work with no runtime code change at all. Unsafe-seam
inventory unchanged across the whole span (80 total: 20 tier-1 + 60 tier-2,
matching Round 18's ending count exactly, via the crate's own self-verifying
`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`).

### Round 18 — `race_repro` watchdog disambiguated from allocator corruption, R17-4's `kind_at` Large-segment check narrowed, R10-2 realloc kill-gate re-verified still red, cross-round `OPEN_ITEMS` tracking index, stale-literal guard + adaptive Large-policy design docs (R18-1..R18-9)

Round 18 — 9 commits (`dc95d1a`..`cf82135`, inclusive of both ends; 8 task
numbers #329–#336, because R18-4 and R18-5 share one commit `1d2c9cd`/task
#333 — the same shared-commit precedent as Round 17's R17-4/R17-5, where
R17-5 carried no separate commit), 2026-07-25 23:48..2026-07-26 06:09 — the
follow-up queue against the external review of the Round 13–17 waves,
synthesized in `docs/reviews/2026-07-25-r18-plan.md`. Same zero-trust
discipline as prior rounds throughout: every diff personally read, every
production-affecting fix personally re-verified with a red/green
counterfactual, commit between tasks. Unsafe-seam inventory UNCHANGED across
the round (tier-1 stayed at 20, tier-2 stayed at 60, total 80 — verified via
`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/ | wc -l` = 80,
matching Round 17's ending count exactly): Round 18 was mostly
docs/process/design work, and the one `src/` fix (R18-3) only narrowed an
existing `#[cfg]` predicate and added a runtime bool check around an
ALREADY-existing unsafe block, introducing no new unsafe primitive.

- **R18-1 (`dc95d1a`, task #329, P1) — `race_repro.rs`'s watchdog `abort()`
  replaced with `exit(124)`, disambiguating a watchdog timeout from allocator
  corruption.** The watchdog historically called `std::process::abort()` on a
  hung test. On Windows/MSVC `abort()` is implemented via `__fastfail`, whose
  exception code is literally `STATUS_STACK_BUFFER_OVERRUN` (`0xC0000409`) —
  indistinguishable on the crash surface from genuine stack corruption. Two
  prior "unexplained" crashes under heavy load (Round 14/task #289, Round
  17/task #326) are far more plausibly this exact mechanism firing under
  severe contention than allocator corruption; three independent Round
  17→18 reviews (oh/r17-readonly/crush) all flagged it. Fix: replaced
  `abort()` with `exit(124)` (the conventional `timeout(1)` code,
  unambiguous vs. any abort/SIGABRT/`__fastfail` signal); added elapsed-time
  + progress-snapshot diagnostics before terminating; `DEADLINE_SECS` made
  overridable via the `RACE_REPRO_DEADLINE_SECS` env var; the watchdog-thread
  panic is no longer silently swallowed (now logged via `eprintln!`).
  Verified via a real counterfactual: temporarily injecting a 30 s sleep with
  `RACE_REPRO_DEADLINE_SECS=2` induced the watchdog to fire and exit 124
  exactly, as designed.
- **R18-2 (`8833baa`, task #331, P1) — R10-2 `medium-classes` realloc
  kill-gate re-run on post-R17-4/R18-3 code; STILL RED.** Re-ran
  `scripts/r10_2_medium_gate.mjs` (unchanged) on `main`@`912740f`, since the
  original "1,700–2,300× slower" verdict was measured on code carrying the
  R17-4 4 MiB Large-segment leak and predates R18-3's `kind_at` narrowing. 20
  A/B/B/A pairs (80 launches) per phase. Result:
  `production,medium-classes` realloc is still ~1,180× slower (the commit
  bounded the leak 1.3 GiB → 49 MiB — leak fixed — but per-op TIME is
  essentially unchanged, ~67.6 µs vs ~72 µs in R14-4; the ratio dropped only
  because the baseline got slower under this session's heavier host load);
  `production,medium-classes,large-cache-extended` ~380× slower
  (`large-cache-extended` helps ~3.5× but cannot remove the structural
  promotion `memcpy`). Same-vs-same control passed (harness honesty
  confirmed). **Verdict: STILL RED** — closing this gate needs R10-2 §5's
  in-place medium-grow mechanism, out of scope here. Added `R14_4...md`
  §7.1/§10 + summary CSV + 3 cited raw logs.
- **R18-3 (`912740f`, task #330, P1) — R17-4's `kind_at(base)` Large-segment
  check narrowed to the promotion-reachable domain.** Three independent
  reviews found R17-4's `kind_at(base)` Large-segment check reads
  unconditionally for EVERY small-classified free under `medium-classes`
  (even tiny 16/32/64 B frees that cannot have reached promotion), gated by a
  `#[cfg]` (bare `medium-classes`) wider than the actual promotion-compiled
  predicate. Fix: (1) realigned branch (A)'s `#[cfg]` 1:1 with the real
  promotion predicate; (2) added a runtime size gate (`cfg!(hardened) ||
  layout.size() >= THRESHOLD`) BEFORE the `kind_at` read — but NOT
  unconditionally skipped under `hardened`, since a `GlobalAlloc`-contract
  violation can fabricate any small layout, not just ones at/above the
  promotion threshold; verified red/green against
  `tests/regression_hardened_large_kind_own_free.rs`. Branch (B) (the
  hardened-only defensive no-op) broadened to fire whenever promotion
  compiles OFF, not just when `medium-classes` is absent. Added
  `medium_class_dealloc_churn_16b` (first iai baseline for the small-free
  path under `production,medium-classes` where branch A actually compiles in:
  8,275 Ir). Plain `production` byte-identical (`small_churn_16b` 8,051 Ir,
  `large_alloc_free_cycle` 3,308 Ir, unchanged from R17-3). **Note (found in
  Round 19's zero-trust review, task #337):** this fix, as landed, still left
  a real hardened/UAF gap under promotion-reachable combos — see Round 19's
  R19-1 for the follow-up correction; do not read this entry as the final
  state of this logic.
- **R18-4 / R18-5 (`1d2c9cd`, task #333, P2) — two small doc fixes in one
  commit** (mirrors R17-4/R17-5's shared-commit precedent; R18-5 carries no
  separate commit). **R18-4:** `heap_core_free.rs`'s pad-target decision
  comment self-contradicted ("Padding is default" vs. its own "no artificial
  padding" two sentences above) — corrected to "No padding is the default".
  **R18-5:** the `class-aware-dirty` "recoverability" retention rationale
  (CHANGELOG's R17-7 entry + `R14_3...md` §6) read as if the feature rescues
  a pre-existing baseline danger; clarified that the R13-1 lost-wakeup class
  is a property of the feature's OWN per-class sidecar — the baseline build
  (feature off) has no such sidecar and never carried this risk, so the latch
  is a derate back to baseline behaviour, not a rescue. Also stated the
  sub-window axis honestly: R17-7's raw logs show ~3.2M-ns `window_ns`
  medians for BOTH arms across both runs — no directional sub-window effect
  confirmed either. Docs/comments only, no logic touched.
- **R18-6 (`ed75b06`, task #334, P2, design-only) — design note for guarding
  against the stale-derived-numeric-literal-in-comment defect class.**
  Evaluated four candidate guards against this defect class (4 recurrences
  across 3 rounds: R15-5, R16-2, R17-5, R17-6). Rejected a reactive
  known-pair list (the same discipline that already failed 4×) and a general
  grep lint (unsound against this repo's own historical prose, e.g.
  `bootstrap.rs:228`'s "1024→4096 raise"). Recommended the "cite the const
  name, not the resolved literal" convention (the only variant with a track
  record of actually preventing recurrence — Rust cannot inject a live value
  into `//` comments) as the standing rule, plus selective test-side
  mirrored-const tripwires (precedent: `dbg_max_segments`/
  `dbg_words_per_class` from R15-1, `dbg_promotion_compiled` from R16-5) for
  the highest-risk constant families. Flagged a live at-risk site
  (`dirty_by_class.rs:37-39`'s restated derived literals) as doc-debt for a
  future round — this became Round 19's R19-9.
- **R18-7 (`290374b` + follow-up `4ba35dc`, task #332, P2, read-only) —
  `PERF_PLAN_beat_mimalloc_small_medium.md` found EXHAUSTED, not dormant.**
  Investigated whether the plan is dormant (as three prior reviews assumed)
  or exhausted. Finding: EXHAUSTED — all of Э1–Э5 landed in Round 7 (cited to
  commits `4908fce`/`38e1a44`/`184123e`/`3b9123e`/`671a81b`/`2dede7d`), plus
  Э6–Э11 across P6/P7. The README's "2.4–2.7× cold gap" headline is a single
  host-drifted 2026-07-23 run whose 256 B row (2.71×) contradicts every prior
  measurement (1.06×–1.66×) — a host-drift signature, not a regression.
  **Follow-up (`4ba35dc`):** zero-trust review of the first draft caught a
  real factual error before it landed — the draft claimed no CI perf-gate job
  exists, based on a grep that checked only `ci.yml` and missed the separate
  `.github/workflows/perf-gate.yml` (task #127/#128: schedule nightly +
  `workflow_dispatch` + labelled-PR, already confirmed working earlier this
  same session). Corrected sections 0/4/5/6/8 accordingly. **Note (Round 19,
  task #339):** the correction pass itself missed one residual copy of the
  retracted claim in §7 ("Files inspected") — fixed by R19-3.
- **R18-8 (task #336, folded into the amended `cf82135`) — added
  `docs/perf/OPEN_ITEMS.md`, a durable cross-round tracking index.** A
  session-surviving index of every item a `docs/perf/*.md` gate report or
  design doc has flagged as open/deferred/follow-up, paired with a new
  mandatory CLAUDE.md "Phased delivery" rule requiring every new round to
  check this index first. Motivated by R14-4's explicitly-marked-open item
  ("re-run `scripts/r10_2_medium_gate.mjs` once R14-5 lands") hanging
  unnoticed through three entire rounds (15, 16, 17), caught only by an
  external review accidentally re-reading the right file — the in-session
  TaskList does not survive a session boundary, so a fresh session inherits
  no memory of prior rounds' flagged-open items; this index does. Cataloged
  14 items at creation (tiered Active/Deferred/Low-priority) with a "Recently
  resolved" closure trail. **Note (Round 19, task #338):** one cataloged item
  (NUMA node-aware bit selection) was found stale at creation — already
  resolved by R11-6/task #234 — and moved to "Recently resolved" by R19-2.
- **R18-9 (`60633e3`, task #335, P3, design-only) — unified adaptive
  Large-policy design doc, modelled on R17-10's structure.** Proposes a
  coordinated measurement matrix for the three opt-in Large features
  (`medium-classes`, `exact-span-large`+`large-reserved-capacity`,
  `large-cache-extended`) plus the runtime budget knob. Flags two premise
  inaccuracies in the plan it evaluates: `primordial-lazy-commit` is already
  in `production` and isn't a Large mechanism (the "five switches" framing
  overstates the space), and the cache budget is `large-cache-extended`'s
  runtime dimension, not a sixth toggle. Key boundary marked: a unified
  policy coordinates existing levers but does NOT close the R10-2 realloc
  kill-gate — the residual ~19–67 ms is structural promotion `memcpy` (per
  R18-2), needing the separate, un-designed in-place-medium-grow mechanism
  (R10-2 §5).

**Production vs. opt-in — what actually changed for default `--features
production` users.** No feature-list change landed this round — `Cargo.toml`'s
`production = [...]` is byte-identical to the end of Round 17 (verified via
`git diff dc95d1a^..cf82135 -- Cargo.toml`, empty diff). Of the
production-affecting work, only R18-1 (test-only, no `src/` production code)
and R18-3 (the one real `src/` fix — narrows an existing `#[cfg]`,
iai-confirmed byte-identical Ir under plain `production`, per its own numbers
above: `small_churn_16b` 8,051 Ir, `large_alloc_free_cycle` 3,308 Ir,
unchanged from R17-3) touch runtime code at all; everything else (R18-2,
R18-4/5, R18-6, R18-7, R18-8, R18-9) is docs/process/design-only, zero
runtime effect. Unsafe-seam inventory unchanged (80 total, 20 tier-1 + 60
tier-2, matching Round 17's ending count exactly).

### Round 17 — unsound `&mut T`/raw-deref `os.rs` sidecar closure, bootstrap zero-loop recovery, a real Large-segment leak root-caused at last, deterministic recycle oracle, design doc for batched deferred reclaim (R17-1..R17-10)

Round 17 — 11 commits (`70a8f2f`..`cbebd45`, inclusive of both ends; 10 R17-1..R17-10
tasks plus R17-6's separate follow-up commit `fbc48a5`; R17-5 has NO separate
commit — it was resolved as a side effect of R17-4's `1b761f4`, which already
rewrote the pad-target comment in `heap_core_free.rs`), 2026-07-24..2026-07-25 —
the follow-up queue against the external review of the Round 14–16 waves,
synthesized in `docs/reviews/2026-07-24-r17-plan.md`. Same zero-trust discipline
as prior rounds throughout: every diff personally read, every production-affecting
fix personally re-verified with a red/green counterfactual, commit between tasks.
Unsafe-seam inventory moved (tier-2: 56 → 58 → 59 → 60, file-count: 16 → 17;
tier-1 unchanged at 20): R17-1 added two `#[allow(unsafe_code)]` sites in
`segment_directory.rs`, R17-2 added one function-scoped site in
`alloc_core_small.rs`, R17-4 added one site in `heap_core_free.rs`.

- **R17-1 (`70a8f2f`, task #318, P1) — `sidecar::reserve_zeroed_with` no longer
  materializes `&mut T` over unvalidated bytes.** R15-2 had moved the boundary
  to `unsafe fn` but had NOT fixed the unsound materialization inside the
  function body itself: `let value: &mut T = unsafe { &mut *ptr }; fixup(value);`
  constructs a `&mut T` over freshly OS-zeroed, not-yet-fully-valid bytes BEFORE
  `fixup` runs — for any generic `T` not valid at all-zero (an enum without a
  zero discriminant, `&U`, `NonNull<U>`, a function pointer, a `bool` byte
  other than 0/1) this is immediate UB at the instant the reference is created,
  independent of what `fixup` does afterward. Fix: `fixup`'s signature changes
  from `impl FnOnce(&mut T)` to `impl FnOnce(*mut T)`; `reserve_zeroed_with`
  never dereferences the pointer or constructs a reference over it, handing the
  raw `*mut T` straight to `fixup`, which must repair not-valid-at-zero fields
  through raw-pointer writes only (`addr_of_mut!(...).write(...)`). The sole
  fixup caller (`SegmentDirectory::init_node_ids`) is replaced by
  `init_node_ids_raw(p: *mut Self)`, an `unsafe fn` writing `node_ids` through
  `addr_of_mut!` + `write`. Soundness hardening only — the current concrete `T`
  was already valid at all-zero, so this was not an observable bug today, but
  the `pub(crate)` generic primitive was a live hazard for any future caller.
- **R17-2 (`f65015a`, task #319, P1)** — same closure pattern as R17-1, applied
  to `os.rs`: `read_directory_class_words` / `read_directory_node_bucket` were
  safe `pub(crate) fn`s holding a prose "# Safety (caller contract)" section
  and dereferencing a raw `*const SegmentDirectory` inside an `unsafe {}` block
  gated only by a `debug_assert!(!p.is_null())` (a no-op in release). Any safe
  code anywhere in the crate could hand these a dangling/junk pointer with no
  `unsafe` token at the call site. Both helpers are now `pub(crate) unsafe fn`;
  the two call sites in `alloc_core_small.rs::find_segment_with_free_impl` are
  wrapped in `unsafe {}` with `// SAFETY:` comments. No runtime effect — the
  call sites already satisfied the contract.
- **R17-3 (`b8612bc`, task #320, P1) — bootstrap hash-table/free-list zero-loops
  gated behind `cfg(miri)`, recovering R14-7's startup regression.** R16-4
  root-caused R14-7's flat +61,440 Ir startup delta to two `MAX_SEGMENTS`-scaled
  zero-fill loops in `bootstrap::primordial()` that LLVM lowers to `memset`
  calls; both loops zero state over the PRIMORDIAL segment's freshly-reserved
  OS-zeroed pages (a tautology under `cfg(not(miri))`), exactly the virgin-page-
  skip discipline the neighbouring `AllocBitmap`/`MagazineBitmap` inits already
  apply (PERF-PASS-2, G5/C1). Gated both behind `#[cfg(miri)]` (loops present
  under miri where `std::alloc` is not guaranteed zeroed; absent otherwise);
  the primordial-base hash insert and the free-list `top=0` write stay
  unconditional (they write real values, not zeros). `npm run iai` on the 12-bench
  production suite: every bench recovers exactly **−81,966 Ir** — NOT just the
  ~61,440 Ir delta R16-4 had measured. The reason is that these loops were
  never gated at ANY `MAX_SEGMENTS` value and predate R14-7: the R16-4
  measurement only attributed the 1024→4096 raise increment
  `(65,536−16,384) + (16,388−4,100) = 61,440`, but the loops' FULL cost at
  `MAX_SEGMENTS=4096` is `20,480` pre-raise baseline + `61,440` raise increment
  ≈ `81,920` bytes ≈ `81,966` Ir — so R17-3 recovered more than R16-4's measured
  delta because the gate removes the loops entirely, not just their raise-driven
  growth. Marginal `Ir/op*` unchanged on every bench (hot-path-neutral; only the
  bootstrap constant moved, `large_alloc_free_cycle 85,274 → 3,308 Ir`). Miri:
  `regression_large_align_no_segment_exhaustion` (2 passed) and
  `regression_virgin_bitmap_skip` (3 passed) confirm the gate is correctly
  inverted and bootstrap completes UB-free under miri.
- **R17-4 (`1b761f4`, task #321, P1) — the most significant finding of the
  round: a real 4 MiB Large-segment leak root-caused and fixed under
  `medium-classes`.** R14-4's gate report (`docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md`
  §2.2) had left a year-open question: `fixed2mib` got 232 `large_cache_hits` /
  17 distinct segments while `nopad`/`floor512kib` got 0 hits / 249 segments,
  despite all three requesting an amount `alloc_large` rounds to the identical
  4 MiB usable span. Root cause: `HeapCore::dealloc_own_thread_with_base`'s
  fastbin magazine dispatch keys on `SizeClasses::class_for(layout.size())`, not
  on the segment's kind. Under `medium-classes` (`SMALL_MAX == 1 MiB`) a Large
  segment can be legitimately freed with a layout that classifies small: R14-4's
  promotion diverts a medium block to a dedicated 4 MiB Large segment at the
  256 KiB threshold, and OPT-G then grows that block in place up to any size
  `<= SMALL_MAX` while it stays Large — so its dealloc layout (the caller-correct
  post-grow size) classifies small. Before this fix such a free was misrouted
  into the small magazine path: the Large segment never reached
  `AllocCore::dealloc`'s Large branch, was never deposited into `large_cache`
  nor released, and leaked every round. `fixed2mib`'s 2 MiB dealloc layout
  happened to classify `None` and so accidentally took the correct substrate
  path — masking the bug. **Fix iteration:** the first proposed fix was
  REJECTED by the orchestrator for adding hot-path cost in clean `production`
  (it unconditionally keyed the magazine dispatch on segment kind, paying a
  cost on every Large free even when no promotion was compiled in). The
  landed fix is `#[cfg]`-split: under `medium-classes` the kind-keyed Large
  dealloc is unconditional (a correctness requirement, not a defensive check,
  since the promotion+OPT-G scenario is legitimate); under non-`medium-classes`
  (incl. plain `production`) the arm is provably unreachable (`SMALL_MAX ~253
  KiB` means every Large dealloc layout is `> 253 KiB`, so `class_for` always
  returns `None`) and the original hardened-only defensive no-op from task #25
  stays exactly as-is. Both branches compile out under plain `production` (no
  `medium-classes`, no `hardened`), so the hot path is byte-for-byte unchanged
  there — confirmed by `npm run iai` matching R17-3's post-fix numbers to the
  Ir (e.g. `small_churn_16b 8,051`, `large_alloc_free_cycle 3,308`).
- **R17-5 — no separate commit.** The stale pad-target comment in
  `heap_core_free.rs` (plan task #322) was resolved as a side effect of R17-4's
  `1b761f4`, which already rewrote the pad-target comment when closing the
  §2.2 open question. This is expected and already confirmed — do not look for
  a missing commit.
- **R17-6 (`d8f9c9b` + `fbc48a5`, task #323, P2)** — two stale-literal doc fixes
  in `segment_table.rs` and `register()`'s cap-lifting comment. (a)
  `HASH_CAPACITY`'s inline comment read `// 2048` (actually `8192`) and a
  sibling `= 16 KiB` figure (actually `64 KiB`) — both left stale by R14-7's
  `MAX_SEGMENTS` 1024→4096 raise. (b) follow-up commit: `register()`'s comment
  "this lifts the 1024-segment cap" read as a stale `MAX_SEGMENTS` literal even
  though historically accurate (task #135 landed while `MAX_SEGMENTS` was still
  1024); reworded to "the fixed `MAX_SEGMENTS` cap" — describes the mechanism
  without pinning a number that reads as wrong post-raise.
- **R17-7 (`5709c24`, task #324, P2) — `class-aware-dirty` full-work verdict
  re-verified, statistically not significant once again.** Added an in-process
  warm-up phase (`PAIRED_AB_WARMUP_ROUNDS`, default 3, env-overridable) to the
  paired_ab_class_aware_dirty_{off,on} process-level A/B judges, then repeated
  the fixed-work off-vs-on comparison twice independently (20 pairs each) plus a
  same-vs-same harness sanity control. Environment quality disclosed up front:
  host CPU load read 80–100% across five samples for the entire measurement
  session (shared multi-agent workspace). Result: both warm-up runs are NOT
  statistically significant (paired t −0.714 and −0.683 vs crit 2.101), with
  much smaller raw deltas than R14-3's original single-shot run 1 (which had
  crossed significance in the "on slower" direction). Combined with R14-3's
  two original runs, all four process-level measurements taken to date fail to
  confirm a full-round `production` wall-clock effect in either direction.
  `class-aware-dirty` therefore remains in `production` UNCHANGED — but the
  "recoverability" basis is narrower than a first reading suggests. The R13-1
  lost-wakeup class the latch closes is itself a property of this feature's
  OWN per-class sidecar (a sidecar-OOM push that raced a later materialization
  could silently diverge from the coarse per-segment bitmap — see the R13-1
  entry below). The baseline `production` build WITHOUT `class-aware-dirty`
  uses only that coarse per-segment dirty bitmap and has NO per-class sidecar
  that could miss a push, so it never carried this risk; R13-1's latch is a
  one-way derate that falls BACK to the coarse scan (i.e. back to baseline
  behaviour) once the sidecar is untrustworthy. In other words the feature
  ships its own hazard and its own safety net — "recoverability" does NOT mean
  it rescues a pre-existing danger in the baseline. It is therefore retained
  as a self-contained correctness/latency-tail policy, NOT on any confirmed
  speedup: the full-round wall-clock effect stays unconfirmed by any
  process-level measurement so far, and the sub-window `window_ns` central
  tendency is likewise indistinguishable between on and off — R17-7's own raw
  logs (`docs/perf/_raw_r17_7_warmup_ab_run{1,2}.log`) show ~3.2M-ns medians
  for BOTH arms across both runs (run 1 on marginally lower, run 2 on
  marginally higher) with the paired t (−0.714, −0.683) well under crit 2.101
  in both, so no directional sub-window effect is confirmed either.
- **R17-8 (`ea8ff86`, task #325, P2) — deterministic `trim_for_recycle` release
  oracle.** `regression_r4_3_teardown_trim.rs` (R16-6, task #316) had
  documented an unreproduced load-sensitive flake (`segments_released_total`
  before=0 after=0 once in 450+ reruns) in its `thread::scope` +
  real-TLS-teardown scenario; external review judged that documentation
  insufficient for allocator teardown. Added a synchronous, single-thread
  deterministic oracle for the same production primitive
  (`HeapCore::trim_for_recycle`, task #95/N1) via the existing
  `#[doc(hidden)] pub SeferAlloc::dbg_trim_current_thread()` hook — no thread
  spawn/join, no TLS `Drop`, eliminating the timing window the flake lives in,
  while covering the exact same release mechanism (flush tcache → drain small
  pool → evict large cache → `os::release_segment`). Verified red/green by
  hand: temporarily no-op'd `trim_for_recycle`'s body → new test fails
  deterministically every run (`segments_released_total` delta 0); restored →
  passes deterministically every run. `regression_r4_3_teardown_trim.rs` is
  unchanged — this test complements it, does not replace it.
  `docs/ARCHITECTURE.md`'s `tests/*.rs` count bumped (218 → 219), per the
  crate's self-verifying doc-count test (`no_stale_doc_references.rs`).
- **R17-9 (`1117198` + follow-up `6b55198`, task #326, P2) —
  `large-cache-extended` default budget reduced 1280 → 256 MiB/heap, plus
  a second `race_repro.rs` flake investigation.** External review flagged that
  `DEFAULT_EXTENDED_BUDGET_BYTES`'s 5× multiplier (= 1280 MiB) is a PER-HEAP
  ceiling: `AllocCore` is owner-only (neither `Send` nor `Sync`), so a
  thread-per-core server running one `AllocCore` per thread multiplies this
  default by however many heaps concurrently exercise `large-cache-extended`
  with a large working set — no process-wide coordination exists between heaps,
  so that topology could retain tens of GiB in aggregate. Two options were
  weighed: (a) lower the per-heap default, or (b) add process-global budget
  coordination via a shared `AtomicUsize` reserve/release protocol. Chose (a):
  no process-wide accounting infrastructure exists anywhere in this crate to
  build on for (b) (the existing `large_cache_hits`/`AllocStats` counters are
  per-heap, collected by the registry, not a shared atomic) — (b) would need a
  brand-new global CAS protocol plus its own loom verification, real added
  surface for a P2 finding on a feature that is opt-in and not in `production`.
  `DEFAULT_EXTENDED_BUDGET_BYTES`'s multiplier drops 5× → 1× (= 256 MiB/heap),
  still a genuinely useful non-zero default, with `.budget_bytes(n)` always
  available to recover a larger cache for a caller who has measured their own
  workload. **Follow-up (`6b55198`):** Round 17's verification of #326 hit a
  `STATUS_STACK_BUFFER_OVERRUN` in `drain_reclaim_uaf_repro_tight_handoff`
  during a full `production` test run under heavy concurrent CPU load.
  Independent investigation: `race_repro.rs` is unchanged since its Phase-12.6
  fix commit (`ea3a4ba`, June 2026) across both this and the prior Round-14
  occurrence (task #289), ruling out a code regression; 80 process invocations
  of the compiled test binary across three deliberately harsher load profiles
  (CPU busy-loop stressors, a real concurrent `cargo check --all-features`, and
  4-way parallel full-binary runs) produced zero reproductions. This is now
  the second confirmed one-off occurrence of this exact signature in this
  exact file, joining R16-6's teardown_trim flake as a load-sensitive class
  specific to this shared workspace. Documented in the test file's header,
  mirroring the established `regression_r4_3_teardown_trim.rs` precedent. No
  `src/` changes — nothing pointed at a genuine allocator defect.
- **R17-10 (`cbebd45`, task #327, P3, design only) — batched deferred reclaim
  design doc.** Design-only, no `src/` change. Corrects the plan's own premise:
  contrary to R17-10's plan wording ("per each reclaimed block, directory-sync
  is called separately"), `sync_directory_for_segment_classes` has been batched
  to one call per segment per drain visit since R8-1 (task #214) — that batching
  gap does not exist. The genuine, un-batched gap is narrower:
  `dec_live_and_maybe_decommit` is still called once per reclaimed block inside
  `drain_dirty_segments`'s ring-drain closure, even though a proven-identical
  batched sibling (`dec_live_batch_and_maybe_decommit`, E3/task W4) already
  exists for a different call site (`flush_run`). Lays out two independently-
  gateable sub-designs: (A) reuse the existing batched decommit primitive in
  `drain_dirty_segments` (small, mechanical, no new correctness argument), and
  (B) defer cross-segment finalisation within one drain sweep, conditional on
  an empirical pre-check (a near-identical precedent exists for
  `drain_heap_overflow`, but for a different, correctness-driven reason that
  does not apply here). Grounds both against `RACE_DRAIN_RECLAIM.md`'s
  lost-wakeup protocol and `PHASE35_DECOMMIT_DESIGN.md`'s
  decommit-without-epoch proof. Orchestrator review caught and corrected one
  factual error before commit (an earlier draft cited R17-4 as "task #329,
  still open"; R17-4 is task #321, already resolved in `1b761f4`, and touches
  only `heap_core_free.rs`, disjoint from this design's `alloc_core_small_pool.rs`
  surface).

**Production vs. opt-in — what actually changed for default `--features
production` users.** No feature-list change landed this round — `Cargo.toml`'s
`production = [...]` is byte-identical to the end of Round 16. Of the four
production-affecting fixes (R17-1, R17-2, R17-3, R17-4), three are pure
soundness hardening with no runtime effect on the current code paths
(R17-1/R17-2) or with an iai-confirmed zero hot-path cost under plain
`production` (R17-3 recovers startup Ir only; R17-4's `#[cfg]` split compiles
out entirely under plain `production`). The iai suite's bootstrap proxy
(`large_alloc_free_cycle`) dropped `85,274 → 3,308 Ir` across R17-3+R17-4, but
that is a one-time startup constant, not a per-op marginal — every bench's
`Ir/op*` is unchanged. Unsafe-seam inventory moved but stayed inside the
existing tier model (no new tier-1 module; the four new tier-2 sites are each
in a file already accounted for or newly accounted for in the README table,
verified by `readme_unsafe_inventory_counts_match_reality`).

### Round 16 — CI coverage restored for medium-classes promotion, R15-1's Ir delta root-caused, review-driven doc/process cleanup (R16-1..R16-6)

Round 16 — 6 commits (`ed8f955`..`56ed79f`, inclusive of both ends; `ed8f955`
is a standalone hotfix, `5f45b37`..`56ed79f` the six R16-1..R16-6 tasks),
2026-07-24 — the follow-up queue against an `@fh` review of the Round 15
wave. Same zero-trust discipline as prior rounds throughout, plus one
additional personal-verification technique newly applied this round: for
R16-1, a live before/after check that the new CI rows genuinely exercise the
non-early-return branch of `HAS_PROMOTION`-gated tests, not merely compile
them. No feature-list change landed this round.

- **Hotfix (`ed8f955`)** — Round 15's own review caught a factual error I
  had written into the CHANGELOG's R15-1 paragraph: it claimed
  `class-aware-dirty`/`alloc-segment-directory` are opt-in, not in
  `production`. Both are actually in `production` (`Cargo.toml`, since
  R13-9/R8-3) — verified directly against `Cargo.toml` before correcting.
  `PerClassDirty`'s ×4 footprint growth from R15-1 is therefore a real cost
  every `production` build now pays, not a bounded opt-in-only cost.
- **R16-1 (`5f45b37`+`4a3ab19`, task #311, P1) — CI coverage restored for
  the medium-classes realloc promotion.** R15-3's `#[cfg]` gate had left
  `try_promote_to_large` (R14-4, the largest Round 14 perf item) with ZERO
  per-PR CI coverage in either of its two mutually-exclusive compiled
  states — no row in `test-feature-isolation` ever turned on
  `medium-classes` at all, so both the promotion-ON and promotion-OFF
  branches of `tests/r14_4_promotion_move_leg_reduction.rs` had been silently
  early-returning in every CI configuration since R15-3 landed (the same
  class of gap R13-12/#285 already caught once). Added two targeted CI
  rows (`production medium-classes`, `production medium-classes
  exact-span-large`) and corrected two sibling test files
  (`r14_4_promotion_free_correctness.rs`, `r14_4_promotion_shrink_uses_move_leg.rs`)
  whose doc comments/messages had gone stale under the promotion-off
  configuration.
- **R16-2 (`87d0412`, task #312, P2)** — R15-5's own doc-fix had mislabeled
  a 55-class (`medium-classes`) footprint figure as "58 classes"
  (`medium-classes-wide`'s count) in `dirty_by_class.rs`; split into two
  correctly-labeled figures (55 classes = 28,160 B; 58 classes = 29,696 B).
- **R16-3 (`7d082ad`, task #313, P2)** — retroactively added the
  machine-readable `_summary.csv` companion R15-1's own report was missing
  (the R14-10 rule requiring one was already in force at R15-1's commit
  time).
- **R16-4 (`afa6b1d`, task #314, P2) — R15-1's flat +61,4xx Ir delta
  root-caused.** A direct `callgrind_annotate`/`objdump` diff (same
  before/after commit-pair isolation method as R15-1) traced the entire
  delta to two `MAX_SEGMENTS`-scaled zero-fill loops in
  `bootstrap::primordial()` (the OPT-B hash-table init and the free-list
  init), both compiled to `memset` calls — byte-delta arithmetic
  (`(65,536−16,384) + (16,388−4,100) = 61,440`) matches the observed Ir
  delta to within 1 Ir. Confirms R15-1's qualitative "one-time bootstrap
  cost" headline while correcting its root-cause attribution (R15-1's own
  §2.3 had ruled out an explicit zero-init loop by checking the wrong call
  frame). No code change recommended — whether to extend the existing
  `cfg(not(miri))` virgin-page-skip discipline to these two loops is
  flagged as a follow-up decision, not applied.
- **R16-5 (`eedc111`, task #315, P3)** — fixed a stale comment in
  `heap_core_free.rs` that contradicted R15-3's own follow-up paragraph
  two lines below it, and added `HeapCore::dbg_promotion_compiled()` (a
  `#[doc(hidden)]` test-only accessor mirroring the `dbg_max_segments()`
  pattern) plus a canary test asserting the test-side hand-mirrored
  `HAS_PROMOTION` constant against the real compiled predicate — closes the
  silent-desync risk a future edit to the `src/`-side `#[cfg]` without a
  matching test-side edit would otherwise create.
- **R16-6 (`56ed79f`, task #316, P3, flake investigation)** — a one-off
  `regression_r4_3_teardown_trim.rs` failure surfaced during this round's
  own verification (under heavy concurrent CPU load from background
  `cargo clippy` builds) was investigated end-to-end (traced
  `AbandonGuard::drop` → `trim_for_recycle` → `evict_all` →
  `release_segment`, confirmed the release-counter increment is
  unconditional, ruled out registry exhaustion) but could not be
  reliably reproduced (450+ direct test-binary invocations plus two full
  suite runs under sustained background load, all green). Documented as an
  open question in the test's own module doc, per the project's
  established known-flakiness-under-load precedent — no code or assertion
  change made.

**Still hanging from Round 14, resolved this session (not a Round 16 code
change):** the user was asked via `AskUserQuestion` whether to keep R14-6's
`large-reserved-capacity` growth-factor-4x fix as-is — confirmed keep as-is,
closing the one standing promotion decision carried across Rounds 14–15.

### Round 15 — MAX_SEGMENTS=4096 footprint measurement, sidecar unsafe-boundary closure, medium-promotion headroom pessimization fixed at the source, ordering litmus test, doc-drift cleanup (R15-1..R15-6)

Round 15 — 6 commits (`7224670`..`4643e9a`, inclusive of both ends),
2026-07-24 — the follow-up queue against an `@fh` review of the Round 14 wave,
synthesized in `docs/reviews/2026-07-24-r15-plan.md`. Same zero-trust
discipline as prior rounds: delegate implementation via `@sh` sub-agents,
personally read every diff, personally re-run the tests under the exact
feature combination each change targets, personally reproduce red/green
counterfactuals for every fix that changes production behavior (R15-3's
promotion gate, R15-4's loom litmus test), commit between tasks. No feature-
list change landed this round — `Cargo.toml`'s `production = [...]` is
byte-identical to the end of Round 14. Unsafe-seam inventory unchanged:
76 total (20 tier-1, 56 tier-2) — R15-2's new `unsafe fn` lives inside a
file already covered by a tier-1 `#![allow(unsafe_code)]`, so it adds no new
grep-visible seam.

- **R15-1 (`7224670`, task #303) — post-raise perf baseline for
  `MAX_SEGMENTS=4096`.** R14-7 raised `MAX_SEGMENTS` 1024→4096 unconditionally
  (the one non-feature-gated change of that round) but had not measured the
  ×4 growth this drives in `WORDS_PER_CLASS`/`DIRTY_BITMAP_WORDS`
  (`= MAX_SEGMENTS / 64`) or `drain_dirty_segments`'s unconditional full-width
  bitmap scan. Measured via `npm run iai` + a dedicated sidecar-RSS probe
  before/after the raise (`docs/perf/R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md`,
  8 raw logs): drain-scan cost is a flat, unattributed +61,4xx Ir bump traced
  to bootstrap cost, not the scan itself (no material wall-clock delta); the
  real, confirmed finding is sidecar footprint scaling exactly ×4 as expected
  (`PerClassDirty` 6,272→25,088 B raw; `SegmentDirectory` ~55.1→~220.5 KiB
  under `numa-aware`) — **correction (found by Round 15's own review, not
  caught before this CHANGELOG was first written): both `class-aware-dirty`
  and `alloc-segment-directory` ARE in `production`** (`Cargo.toml`, since
  R13-9 and R8-3 respectively), so `PerClassDirty`'s ×4 footprint growth is a
  real cost every `production` build now pays, not a bounded opt-in cost.
  Only the larger `SegmentDirectory` NUMA figure above is gated behind the
  still-opt-in `numa-aware`. New `AllocCore::dbg_words_per_class()`
  doc-hidden test-only accessor added (mirrors the existing
  `dbg_max_segments()` pattern) so `examples/r13_9_class_aware_dirty_sidecar_rss.rs`
  reads the real constant instead of a hardcoded `16` that had gone silently
  stale. README/`IAI_BASELINE.md` deliberately NOT refreshed this round
  (reasoned: no `production` composition change to pin against).
- **R15-2 (`6745350`, task #304) — `sidecar::reserve_zeroed_with` closed to
  `unsafe fn`.** R14-9's unified sidecar primitive (`src/alloc_core/sidecar.rs`)
  was built specifically to close "safe fn materializes `&'static`/`&mut`
  over a raw pointer with no `unsafe` token at the call site" gaps — but
  `reserve_zeroed_with<T>` itself stayed a safe `fn` despite materializing
  `&mut T` over OS-zeroed (not yet fully-valid) bytes before its `fixup`
  closure runs, exactly the class of hole the primitive exists to close.
  Converted to `unsafe fn` with an explicit `# Safety` contract (every field
  of `T` not written by `fixup` must be valid at all-zero); the sole call
  site (`os::reserve_directory_sidecar`) updated with an `unsafe {}` block
  and a `// SAFETY:` comment. `reserve<T>`/`deref<T>`/`deref_mut<T>`
  untouched. README unsafe-inventory counts unaffected (see above).
- **R15-3 (`4de4ef2`, task #305) — `medium-classes` × `exact-span-large`
  realloc-promotion headroom pessimization fixed at the source, not the
  test.** R14-4's Small/medium→Large realloc promotion
  (`try_promote_to_large`) assumed every post-promotion grow rides OPT-G
  in-place for free — true under plain `production` (Large always rounds up
  to a whole 4 MiB `SEGMENT`), but false under the opt-in `exact-span-large`
  without an effective `large-reserved-capacity` (itself disabled whenever
  `numa-aware` is also on): a promoted block there gets ZERO growth headroom,
  so every subsequent grow — even small same-class steps that would have
  stayed in-place via OPT-F on the ordinary medium ladder — takes a move leg.
  This triple-feature interaction had already forced two test-assertion
  weakenings instead of a real fix (task #302 and its follow-up hotfix
  `9b59990`). Root-caused and closed with a compile-time `#[cfg]` gate:
  `try_promote_to_large`, its call site, and `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
  now compile in only when `medium-classes && (!exact-span-large ||
  (large-reserved-capacity && !numa-aware))` — zero runtime cost, no
  functionality lost (growth falls through to the pre-existing, already-
  correct medium-ladder move leg, identical in shape to plain `production`
  without `medium-classes`). `tests/r14_4_promotion_move_leg_reduction.rs`
  gained two new non-vacuous tests for the promotion-off configuration
  (OPT-F same-class in-place growth), verified via a red/green counterfactual
  (the new tests fail against the pre-fix production code, confirming they
  are not vacuous). None of `medium-classes`/`exact-span-large`/
  `large-reserved-capacity` are in `production`.
- **R15-4 (`2662f38`, task #306) — unjoined loom litmus test for
  `sidecar_oom_latch`'s Acquire/Release pairing.** R14-2's loom test
  `latch_trip_and_successful_publish_race_single_consumer_visit` joined both
  producer threads before its single consumer visit — `join()` itself
  supplies happens-before independent of the latch's actual memory ordering,
  so that test's assertion would pass under any ordering, even fully
  `Relaxed`, without ever exercising the Acquire/Release pairing its own doc
  comment credits. Closed with a classic message-passing litmus test: a
  consumer spins (no `join()`) on the latch's own Acquire load until it
  observes the trip, then asserts the producer's prior program-order writes
  are visible; a permanent `#[should_panic]` counterfactual with the latch
  weakened to `Relaxed` proves the new test is non-vacuous (loom finds a
  genuine ordering-violation interleaving and the assertion fires).
- **R15-5 (`e262559`, task #307) — doc-drift cleanup after the `MAX_SEGMENTS`
  raise.** Five files' doc comments still cited `WORDS_PER_CLASS=16`/
  `MAX_SEGMENTS=1024`/stale KiB footprint figures left over from before
  R14-7's raise to 4096 — `segment_directory.rs`, `dirty_by_class.rs`,
  `heap_slot.rs`, the `Drop for AllocCore` stack-buffer comment
  (`alloc_core.rs`, actually 64 KiB now, not 16 KiB), and
  `tests/regression_segment_table_tombstone_rebuild.rs`. Pure doc fix — zero
  executable-code changes, confirmed by inspecting the diff.
- **R15-6 (`4643e9a`, task #308, P3) — compile-time alignment assert for the
  sidecar primitive.** `sidecar::reserve<T>`/`reserve_zeroed_with<T>`'s doc
  comments claimed `align_of::<T>() <= PAGE` in prose only; formalized with
  an inline `const { assert!(core::mem::align_of::<T>() <= aligned_vmem::PAGE) }`
  at the top of both function bodies (stable since Rust 1.79; this crate's
  MSRV is 1.88) — any future sidecar `T` violating the invariant now fails to
  compile at monomorphization time instead of silently returning an
  under-aligned pointer.

**Still hanging from Round 14, not resolved this round:** R14-6's clean-GO
`large-reserved-capacity` growth-factor-4x promotion recommendation has not
yet been put to the user via `AskUserQuestion` — the one genuinely open
promotion decision across both rounds.

### Round 14 — sidecar soundness/ordering hardening, medium realloc promotion, exact-span/large-cache production gates, SegmentTable ceiling raised, unified sidecar primitive (R14-1..R14-10)

Round 14 — 19 commits (`06a5be6`..`6cc46f1`, inclusive of both ends),
2026-07-23 — the follow-up queue against THREE independent reviews of the
Round 13 wave (two inline, one written to
`docs/agent_reviews_us/2026-07-23-r13-wave-review-fx.md`), synthesized in
`docs/reviews/2026-07-23-r14-plan.md` and
`docs/reviews/2026-07-23-r14-reviews-synthesis.md`. Executed with the same
zero-trust discipline as prior rounds: delegate implementation via `@sh`
sub-agents, personally read every diff, personally re-run the tests, personally
reproduce red/green counterfactuals for safety-relevant changes, commit
between tasks. Two genuine defects were found and fixed mid-verification
rather than at planning time — a pre-existing CI redness surfaced by
Round 13's own `class-aware-dirty` promotion (task #299) and a pre-existing
`--all-features` test failure in R14-4's own test suite (task #302) — both
recorded as their own fixes, not folded silently into the tasks that
surfaced them.

**Production vs. opt-in — what actually changed for default `--features
production` users.** No feature-list change landed this round —
`Cargo.toml`'s `production = [...]` is byte-identical to the end of Round 13.
Every fix in this round either (a) hardens code already inside `production`
(R14-1's `LargeCacheExtension` init, R14-2's latch ordering — both apply
whenever `class-aware-dirty`/`large-cache-extended` are compiled in, and
`class-aware-dirty` has been in `production` since R13-9) or (b) hardens/
extends opt-in features that stay opt-in (R14-4's realloc promotion gated on
`medium-classes`, R14-5's `large-cache-extended` hardening, R14-6's
`large-reserved-capacity` growth factor). R14-7's `MAX_SEGMENTS` raise
(1024→4096) is the one change that touches every build regardless of feature
flags, since the constant has no feature gate.

**P0 — correctness/soundness fixes:**

- **R14-1 (`06a5be6`, task #286) — `LargeCacheExtension` typed
  initialization + unsafe-boundary hardening.** Three independent Round 13
  reviews found the same soundness gap: the opt-in `large-cache-extended`
  sidecar cast OS-zeroed pages directly to `*mut LargeCacheExtension`
  without an explicit `ptr::write`, relying on an unspecified Rust layout
  guarantee (all-zero bytes decoding as `Option::None`) that happened to
  hold today but was never guaranteed. Fixed with an explicit `ptr::write`
  before the pointer is published; `deref_large_cache_extension[_mut]`
  converted from safe functions with a prose-only caller contract to
  `unsafe fn` (closing an aliasing-`&'static mut`-from-safe-code gap); a
  false "`AllocCore` is `Send`" doc claim corrected. Tier-2 unsafe seam
  count: 45→51 (the boundary moved out to every call site, each carrying
  its own `# Safety`).
- **R14-2 (`ef4db50`, task #287) — `sidecar_oom_latch` Acquire/Relaxed
  divergence closed.** Three reviews found a three-way mismatch: the
  producer's `Release` store and the field's own doc comment both claimed
  the consumer paired with `Acquire`, but production actually read the
  latch `Relaxed`, and the loom model had always used `Acquire` — so a
  green loom suite never proved anything about the weaker ordering
  production shipped. Production promoted to `Acquire` (free on x86);
  loom model verified to match byte-for-byte; a new loom test isolates the
  exact OOM-trip/successful-publish/single-consumer-visit interleaving
  R13-1's latch exists to close. Investigated resetting the latch at the
  slot's `trim_for_recycle` quiescent point (a P2 finding from the reviews)
  and explicitly declined this round: `set_dirty_bit_for_segment` resolves
  the owning slot with no `STATE_LIVE` gate, so a plain reset would race a
  legitimate in-flight write — left as a documented future-round candidate.

**P1 — feature hardening and production gates:**

- **R14-3 (`6d85db4`, task #288) — honest sub-window vs. full-round framing
  for `class-aware-dirty`.** The R13-9 promotion headline ("21.71× at
  N=8") was a sub-window `ns/owner_alloc` timer inside the wallclock
  bench's `run_round`, not criterion's own full-round mean for the same
  harness; the full round moved only ~11% at N=8 (most of the apparent
  savings is deferred drain work moving into the unmeasured pre-alloc/
  recycle portion of the round, not disappearing). Not a reversal — the
  mechanism's win is real and reproducible in direction — but the headline
  is reworded everywhere it appeared (gate doc, wave summary, `Cargo.toml`
  comment, CHANGELOG), the bench now prints both axes, and a new
  fixed-work process-level A/B/B/A judge was built reusing
  `scripts/paired-ab-runner.mjs`. New CLAUDE.md rule: a wall-clock gate
  must report both axes going forward.
- **R14-4 (`3fde9f9`, `9fcde2e`, `6a644a4`, task #289) — Stage 2 Small/
  medium→Large realloc promotion**, implementing the design from
  `docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`: a growing
  realloc crossing a 256 KiB threshold (`medium-classes` only) is diverted
  directly to a Large allocation instead of walking the medium size-class
  ladder, so every subsequent grow rides the existing OPT-G in-place fast
  path. All five design-doc test scenarios (a-e) implemented. Production
  gate result: **CONDITIONAL-GO on the mechanism, RED on R10-2's specific
  realloc kill-gate** (iai shows no regression, +0.035% Ir; but R10-2's
  exact 16-live-object/8-slot-cache workload oversubscribes the base Large
  cache, so ~half of promotions pay a fresh OS reservation) — not promoted,
  interacts with R14-5's cache hardening for a future re-gate.
- **R14-5 (`c0ccbc4`, task #290) — `large-cache-extended` hardening**: a
  budget-vs-materialization ordering fix (a deposit the budget will
  unconditionally reject no longer pays a sidecar page reservation first),
  a finite default budget for the extended cache (1280 MiB, neutralizing a
  measured ~2.86× RSS-retention risk down to parity with the base 8-slot
  cache), N=1/2/4 post-materialization hit-path correctness gates, and
  mixed-size/adversarial best-fit/FIFO tests. Production A/B/B/A gate on a
  turnover-shaped workload: **CONDITIONAL-GO** (large, statistically real
  win; condition — promotion should ship together with the finite default
  budget). Not promoted; `large-cache-extended` stays opt-in.
- **R14-6 (`8265b1c`, task #291) — `large-reserved-capacity` growth factor
  raised 2×→4×**, closing R13-6's CONDITIONAL-GO blocker. Compared 2×/4×/8×/
  adaptive-compounding by total bytes copied across the iai `realloc_grow`
  doubling chain; 4× was the sweet spot. Result: the regression is not just
  reduced but **reversed** — `realloc_grow` moved from R13-6's measured
  +102.3% Ir / +52.7% Estimated Cycles to **−22.44% Ir / −36.17% Estimated
  Cycles** versus plain `production`, with the RSS win (15.80×→1.06× at
  260 KiB) unchanged. **Gate: GO** (recommendation only — `exact-span-large`
  + `large-reserved-capacity` remain opt-in, promotion left to the user).
- **R14-7 (`b117257`, `ffb82bc`, task #292) — `MAX_SEGMENTS` raised 1024→
  4096.** R13-8 found a 100%-reproducible wall at 1023 simultaneously-live
  Large objects (a capacity cliff, not a latency degradation) in every
  feature arm. Documented in README first regardless of outcome; measured
  the raise as cheap on every axis (static footprint 32→112 KiB inside the
  fixed 4 MiB primordial segment, idle RSS statistically unchanged, no
  scan-path degradation) and applied it — the only change this round that
  is unconditional (no feature gate). Two tests had their old
  1024/1025/1500 literal cap assumptions replaced with runtime
  `dbg_max_segments()` reads (per the R12-14 density-agnostic convention) —
  both would have silently stopped exercising their exhaustion path without
  this fix. `docs/perf/R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` records a
  DESIGN-ONLY chained-table follow-on for if a future workload exceeds the
  new ceiling.

**P2 — documentation accuracy and process:**

- **R14-8 (`8a432d7`, task #293) — corrected false NUMA-incompatibility
  claims for `class-aware-dirty`.** `.github/workflows/ci.yml` and the
  wallclock bench both claimed the drain path "is compiled out under NUMA
  routing" — false; `drain_dirty_segments`'s only feature gate is
  `alloc-xthread`. What IS `not(numa-aware)`-gated is a different,
  downstream mechanism (the directory-driven lookup fast path). Confirmed
  empirically with a new throwaway probe test before correcting the
  comments; the wallclock bench's sweep now runs under `numa-aware` too.
- **R14-9 (`4204b38`, `782f5b1`, `8837283`, `c568fcc`, `f344f62`, task
  #294) — unified owner-only sidecar primitive** (`src/alloc_core/
  sidecar.rs`): `SegmentDirectory` and `LargeCacheExtension` (already
  fixed individually in R14-1) migrated onto one shared `reserve`/
  `reserve_zeroed_with`/`deref`/`deref_mut` API, closing the same
  safe-fn-returns-`&'static mut` gap in `os::deref_directory_sidecar[_mut]`
  that R14-1 had only closed for `large_cache_extended.rs`.
  `dirty_by_class::PerClassDirty` deliberately NOT migrated (cross-thread
  CAS-publish via `RacyPtrCell` is a different concern, and it is never
  dereferenced as `&mut` anywhere in the crate) — documented explicitly
  rather than left as a silent inconsistency. Tier-2 unsafe seam count:
  52→56 (net +4, the boundary correctly moved out to call sites); the
  duplicated reserve/init/deref boilerplate the three sidecars had each
  hand-rolled independently is gone.
- **R14-10 (`f49cddc`, task #295) — wave process hygiene**: `git diff
  --check` verified clean (`.gitattributes` added to exempt
  `docs/perf/_raw_*.log` from whitespace false-positives without touching
  their bytes); `R13_WAVE_SUMMARY.md`/CHANGELOG's Round 13 entry corrected
  to honestly separate default-production runtime effect from opt-in-only
  fixes; pinned-commit/worktree protocol adopted for bench-profile
  reproducibility (documented in CLAUDE.md) over a heavier named-bundle
  Cargo-feature scheme; machine-readable CSV summary policy added for perf
  gates going forward; `cargo hack check --feature-powerset --depth 2`
  evaluated and **adopted** as a weekly (not per-PR) CI job — 308 check
  invocations at depth 2, too much for per-commit but exactly the
  structural gap class that let R13-12/R14-hotfix-#299's E0599 bugs go
  unnoticed for a full round; README wall-clock table's ±60% absolute-ns
  host-noise drift called out explicitly (ratios remain normative, iai is
  the one normative absolute source); four P3 micro-fixes (a
  `TCACHE_CAP<=16` const-assert, a `virgin_mask` invariant wording
  correction, an `alloc-stats` parity verification, and renaming
  "A/B/B/A" to "A/B, double-checked" to match what the protocol actually
  does).

**Hotfixes found mid-verification (not part of the planned R14-1..R14-10
queue, each its own numbered fix):**

- **Task #299 (`ee1c14e`) — two real CI failures on `main`, both surfaced
  by Round 13's own `class-aware-dirty` promotion, not by this round's
  work.** `r9_6_class_aware_dirty_waste_ratio_scales_with_class_count`
  measures the pre-class-aware-dirty baseline and is expected to fail once
  the feature is enabled; its own module doc claimed "no CI configuration
  combines `production` with `class-aware-dirty`" — true when written,
  false after R13-9. Fixed by gating the test function itself on
  `not(feature = "class-aware-dirty")` instead of chasing per-step
  `--skip` flags. Separately, `tests/r11_4_dealloc_batch_hardened_guards.rs`
  was missing `batch-api` in its own feature gate despite calling
  `dealloc_batch` (which requires it) — a pre-existing gap confirmed red
  even on the Round 12 tip commit, predating this session entirely.
- **Task #302 (`6cc46f1`) — `tests/r14_4_promotion_move_leg_reduction.rs`
  failing under `--all-features`**, found by `npm run check` during R14-10.
  Root cause: not a production bug — `exact-span-large`'s own documented
  trade-off (zero committed headroom beyond the exact page-rounded request)
  combined with R14-4's promoted-block padding (also zero artificial
  slack) structurally forces every post-promotion grow through the move
  leg whenever `exact-span-large` is on without `large-reserved-capacity`'s
  headroom (itself disabled under `numa-aware`) — exactly the
  `--all-features` combination. Fixed by scoping the tests' pointer-identity
  oracle to configurations with grow headroom, falling back to a
  weaker-but-still-load-bearing correctness oracle in the two documented
  no-headroom configurations, with a red/green counterfactual confirming
  the relaxed assertion is not vacuous.

### Round 13 — class-aware-dirty promoted to production, NUMA bucket-slot reuse, virgin-zero-skip resource fix, Large-cache extension, wave process discipline (R13-1..R13-12)

Round 13 — 13 commits (`e2d84f7`..`1a2dd7d`, inclusive of both ends),
2026-07-23 — the follow-up queue against two independent external reviews of
the Round 12 wave, executed task by task with the same zero-trust discipline
as prior rounds: delegate implementation via `@sh` sub-agents, personally
read every diff, personally re-run the tests (not trust the sub-agent's own
"tests passed" claim), personally reproduce red-before/green-after
counterfactuals for every safety-relevant change, then commit between
tasks. Two genuine defects were found and fixed mid-verification of other
tasks rather than at planning time (R13-11 inside R13-1's verification,
R13-12 inside R13-3's verification) — both are recorded as their own
numbered fixes, not folded silently into the task that surfaced them.

**Production vs. opt-in — what actually changed for default `--features
production` users.** Exactly one feature was promoted into `production`
this round: `class-aware-dirty` (R13-9, user-confirmed via `AskUserQuestion`
after a full A/B gate). Everything else that shipped is one of: a
correctness fix to code that IS reachable from default `production` (R13-1,
R13-12 — see below), a correctness fix to code gated behind an opt-in
feature NOT in `production` (R13-2 under `numa-aware`, R13-3 under
`virgin-zero-skip` — real fixes, zero default-`production` runtime effect),
a test-only fix (R13-11), or a new/measured opt-in feature that was
explicitly evaluated and NOT promoted (`exact-span-large`+
`large-reserved-capacity`: CONDITIONAL-GO, blocked on a real iai
`realloc_grow` regression; `large-cache-extended`: not gated for production
this round). See `docs/perf/R13_WAVE_SUMMARY.md` for the full production
A/B, double-checked wave report.

**P0/P1 — correctness fixes.** Two of the five (R13-1, R13-12) affect every
default `--features production` build; R13-2 and R13-3 are real fixes to
opt-in-only code (`numa-aware`, `virgin-zero-skip` respectively — neither is
in `production`, so a default build carries zero runtime effect from them
until a user separately opts in); R13-11 is test-only. See
`docs/perf/R13_WAVE_SUMMARY.md` §4 for the per-fix production-effect table
(R14-10/#295 correction — the original wording here said "inside code
already shipping in `production`" for all five, which overstated R13-2/R13-3):

- **R13-1 (`e2d84f7`) — close a lost-signal gap in `class-aware-dirty`'s
  OOM-transition.** A sidecar-OOM push that raced a later successful
  sidecar materialization could silently diverge between the coarse
  per-segment bitmap and the per-class sidecar. Fixed with a one-way,
  never-reset `sidecar_oom_latch: AtomicBool` on `HeapSlotRemote`: once set,
  `drain_dirty_segments` forces a coarse-only scan for the remainder of that
  heap's lifetime rather than trusting a sidecar that may have missed a
  push. Loom-verified (7 tests); red/green counterfactual personally
  reproduced.
- **R13-11 (`da037f2`, task #284, found mid-verification of R13-1) — a
  deterministic (not flaky) lost-wakeup test failure in
  `class_aware_dirty_routing.rs`**, reproducible even on the original R12-7
  commit — root-caused to a TEST bug (a `small_cur` refill-batch leftover
  masking the intended cross-thread-reclaim path being measured), not a
  production defect. Fixed via a burn-down loop (using
  `AllocCore::dbg_dirty_segments_drained()`'s counter delta as proof of
  reaching the real drain path) before the assertion R13-1's verification
  depends on could be trusted.
- **R13-2 (`a3434df`, task #272) — reuse freed NUMA directory bucket
  slots.** A new `active_bits_by_node: [u32; MAX_NODES]` counter frees a
  node's bucket slot once every bit that node ever set returns to 0,
  preventing slot exhaustion under long-running bucket churn across 9+
  distinct NUMA nodes. Also fixed a second, independently-found defect:
  `clear_bit` was using the registering `node_bucket_mut` (which can
  allocate a bucket) instead of the read-only `node_bucket` accessor.
- **R13-3 (`9886780`) — thread virgin-zero-skip through the magazine
  instead of bypassing it; upgraded from a perf task to a P1
  resource-retention fix.** The prior `alloc_zeroed` fast path for
  virgin-zero-skip bypassed the tcache magazine entirely, which meant a
  calloc-only workload silently never ran `drain_heap_overflow`'s drain
  prelude — a real resource-retention defect, not merely a missed
  optimisation. New `PerClass::virgin_mask: u16` (gated
  `virgin-zero-skip`) and `AllocCore::refill_class_bump_virgin_checked`
  thread the virgin bit through the existing magazine carve path so both
  the tcache fast path and the drain prelude are recovered. Wall-clock gate
  honestly reports no statistically significant difference at n=10 on this
  single-threaded synthetic bench — the fix's justification is the
  resource-retention correctness, not a headline number.
- **R13-12 (`e7617d1`, task #285, found mid-verification of R13-3) — a
  genuine pre-existing compile error**: `alloc-xthread`+`fastbin`+
  `alloc-decommit` without `alloc-segment-directory` failed with E0599 in
  `drain_heap_overflow`, confirmed via `git stash` to predate R13-3
  entirely. Fixed by gating the two `sync_directory_for_segment_classes`
  call sites behind `#[cfg(feature = "alloc-segment-directory")]`,
  mirroring the existing pattern at every sibling call site.

**P1-perf/process — measured, opt-in features and process corrections:**

- **R13-4 (`6018cf8`, task #274) — page-run verdict corrected from
  "SUPERSEDED" to "DEFERRED — no demonstrated production victim yet".**
  Both `exact-span-large` and `medium-classes-wide` are still opt-in, so
  `production` gets no RSS benefit from either yet — an external review had
  flagged the prior wording as overclaiming.
- **R13-5 (`0f3b608`, task #275) — feature-isolated CI rows** covering the
  exact combinations that would have caught R13-11/R13-12 earlier
  (`production exact-span-large`, `production class-aware-dirty
  alloc-stats`, `production virgin-zero-skip alloc-stats`,
  `page-map-diag`, plus build-only rows for `alloc-xthread`/`fastbin`/
  `alloc-decommit`), `loom_class_aware_dirty.rs` wired into the
  `loom-xthread` CI job (was silently never running — a second instance of
  the class of bug task #204 originally caught), and a new structural guard
  (`tests/no_stale_loom_files.rs`) that fails CI if any `tests/loom_*.rs`
  file is ever unreferenced by the workflow again.
- **R13-6 (`3829d82`, task #276) — production A/B gate for
  `exact-span-large`+`large-reserved-capacity`: CONDITIONAL-GO, not
  promoted.** iai's `realloc_grow` bench (64 B→4 MiB, 16 doublings) shows
  +102.3% instructions / +52.7% Estimated Cycles under the pair vs plain
  `production` — the pair's fixed 2× `reserved_capacity` ceiling re-trips
  almost every doubling step. The RSS win (15.80×→1.06× at 260 KiB) is real
  and unregressed, but the deterministic iai regression was large enough
  that unconditional promotion was not recommended; no user prompt was made
  since the gate did not clear to an unconditional GO.
- **R13-7 (`df636ff`) — new opt-in `large-cache-extended` feature**: widens
  the Large free-cache from 8 to 40 slots via a lazily-materialised
  sidecar (`src/alloc_core/large_cache_extended.rs`, a new tier-1
  `#![allow(unsafe_code)]` seam). Judge: 88.89%→100.00% hit rate,
  ~23,437 ns→237 ns per op (~99×) on a genuine 9-distinct-size Large
  overflow workload. Not gated for production promotion this round (R13-8
  separately confirmed 0 cache hits on a static live-object workload — the
  extension only helps turnover-shaped access patterns).
- **R13-8 (`874650b`, task #278) — judge on 256–2048 simultaneously-live
  260 KiB–2 MiB objects** found a real, 100%-reproducible `MAX_SEGMENTS`
  wall at exactly 1023 live Large objects in every feature arm — this
  updates R13-4's "no demonstrated victim" verdict for this specific size
  band, though `exact-span-large` already closes the RSS/commit side of it
  and there is no non-linear wall-clock cost approaching the wall.
- **R13-9 (`bebd902`, `da77b38`, task #279) — `class-aware-dirty` promoted
  into `production`.** Production A/B gate (`docs/perf/
  R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md`): 21.71× SUB-WINDOW
  `ns/owner_alloc` at N=8 concurrent producer classes (re-measured on top of
  R13-1's latch fix, inside R12-7's own pre-latch 19.7–32.4× range) — R14-3
  (task #288, `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`) later
  corrected the headline framing: criterion's own FULL-ROUND mean on the same
  harness/raw logs moved only ~11% at N=8 (~1.6% at N=4), since most of the
  sub-window's apparent reduction is deferred drain work moving into the
  round's unmeasured pre-alloc/recycle phases rather than disappearing — the
  mechanism and the promotion decision are unaffected, only the headline's
  wording. iai confirms +0.00% to +0.02% Ir on 12 non-remote single-thread
  benches (zero cost outside cross-thread paths), ~8 KiB RSS sidecar per
  materialised heap (corrects R12-7's own doc, which cited the raw
  un-page-rounded 6.1 KiB `size_of` figure). GO recommendation accepted by
  explicit user confirmation (`AskUserQuestion`) before the `Cargo.toml` edit.
- **R13-10 (`1a2dd7d`, task #280) — wave process discipline.** Re-ran
  `npm run bench:table` on the post-R13-9 tree and refreshed README.md's
  wall-clock table, which had gone stale across two consecutive
  `production` composition changes (Round 12's R12-9/R12-11, then R13-9);
  added a `CLAUDE.md` rule that a `production` composition change must
  carry its `bench:table`/`iai` refresh in the same PR going forward; wrote
  `docs/perf/R13_WAVE_SUMMARY.md`, a retrospective production A/B,
  double-checked report for the whole wave (R14-10/#295: originally titled
  "A/B/B/A," corrected — that name belongs to the stricter interleaved
  protocol); formalized the raw perf-log policy
  (`docs/perf/_raw_*.log` is `.gitignore`d scratch by default, with a
  documented `git add -f` exception when a gate report cites specific
  filenames as evidence); trimmed two Cargo.toml feature comments that had
  grown into full design writeups duplicating their own module docs.

### Round 12 — directory-aliasing/NUMA correctness fixes, exact-span Large, class-aware dirty routing, virgin-zero skip (R12-1..R12-14)

Round 12 — 14 commits (`79f4136`..`3dc7bd9`, inclusive of both ends),
2026-07-22 — the follow-up queue against two independent external reviews of
the Round 11 wave (one correctness-focused, one speed-focused), synthesized
into a single prioritized queue and executed task by task with the same
zero-trust discipline as prior rounds: delegate implementation, personally
read every diff, personally re-run the tests (not trust the agent's own
"tests passed" claim), personally reproduce red-before/green-after
counterfactuals for every safety-relevant change, then commit. Two tasks
(R12-8, R12-13) reached honest NO-GO/deferred verdicts with zero code
changed — both cited prior institutional decisions (the 2026-07-10 G1
honest-reject for R12-8; R12-3's own measured numbers for R12-13) rather
than re-deriving from scratch, and both are recorded as complete, correct
outcomes of this round's methodology, not shortfalls. (R12-13's original
"superseded" wording was itself corrected to "deferred — no demonstrated
production victim" in Round 13, R13-4/task #274, after an independent
review noted that the features R12-13 cited are opt-in and not part of
`production`; see that entry below.)

**Production vs. opt-in — what actually changed for default `--features
production` users.** One feature joined `production` this round
(`primordial-lazy-commit`, R12-9, user-confirmed separately from the
measured GO per this project's "production feature-composition changes need
explicit sign-off" convention); one feature (`page-map-diag`, R12-11)
flipped `production`'s *default* by making previously-always-on bookkeeping
opt-in instead — a smaller, faster default carve path, with the diagnostic
capability preserved behind the new feature for anyone who needs it. Five
new opt-in, non-`production` experimental features were added
(`exact-span-large`, `large-reserved-capacity`, `class-aware-dirty`,
`virgin-zero-skip`, `page-map-diag`); `batch-api` gained a hard dependency
on `experimental`. Two P0 correctness fixes (R12-1, R12-2) landed directly
in the always-on directory-scan path with no feature gate — they fix
genuine bugs, not opt-in behavior.

**P0 — correctness fixes (unconditional, no feature gate):**

- **R12-1 (`79f4136`) — close a formal aliasing-UB window in the
  directory-driven segment scan.** `find_segment_with_free_impl`'s scan
  loop held a live `&'static SegmentDirectory` across a call
  (`validate_directory_candidate`) that can itself materialize a
  `&'static mut SegmentDirectory` on the same allocation via its self-heal
  path — `&T`/`&mut T` simultaneously live over one allocation, aliasing UB
  under Stacked/Tree Borrows regardless of the single-threaded owner
  discipline (which only rules out a data race, not the aliasing-model
  violation). Fixed by reading each directory word BY VALUE
  (`os::read_directory_class_words`, a raw-pointer `.read()` with no
  reference retained) instead of holding a long-lived reference across the
  mutating call. New regression test manufactures the exact
  mutation-during-scan interleaving; miri cannot reach the directory's
  above-threshold materialization path in practical time (documented
  pre-existing limitation), so the test is a behavioral-equivalence pin
  plus a guard against future regressions, not a red/green UB detector —
  documented honestly rather than overclaimed.
- **R12-2 (`89b6ce2`) — dense NUMA node-id → bucket mapping fixes locality
  on >8-node hosts.** The directory's `node_bucket` used the raw OS NUMA
  node id as a direct array index clamped at `MAX_NODES = 8`; `numa-shim`
  scans up to 64 real node ids, so every node id ≥ 8 silently fell into the
  shared "unknown" bucket regardless of how many distinct high-numbered
  nodes were actually in play — a thread on node 9 could be handed a
  node-10 segment ahead of its own node-9 segment, defeating R11-6's
  locality optimization on exactly the large machines it targets. Fixed
  with a dense `node_ids: [u32; MAX_NODES]` registration table (a node
  claims the next free bucket slot on first use) instead of raising
  `MAX_NODES` to 64 outright (rejected: ~7× sidecar memory tax paid by
  every heap for a rare case). A genuine regression in the fix's own
  test-only rebuild path (reset-vs-preserve the registration table) was
  caught during development and fixed, documented at length in
  `segment_directory.rs`.

**P0-perf — new opt-in experimental features (not in `production`):**

- **R12-3 (`2593d30`) — exact-span Large allocation.** Every Large request
  previously reserved a minimum of one whole 4 MiB `SEGMENT` regardless of
  actual size (a 260 KiB request paid for 4 MiB, ~15.8× amplification).
  `exact-span-large` sizes the physical reservation to
  `round_up(header + size, OS page)` instead — the stale comment claiming
  vmem required SEGMENT-multiple sizing did not hold up under inspection
  (both backends already support arbitrary `size != align`). Measured (own
  new `examples/r12_3_exact_span_measure.rs` harness, independently
  reproduced by the orchestrator): 260 KiB 15.78×→1.05×, 512 KiB
  8.00×→1.01×, 1 MiB 4.00×→1.00×, 1.75 MiB 2.29×→1.00×, 4 MiB
  2.00×→1.00×. Trade-off: OPT-G's in-place realloc-grow fast path loses
  most of its committed headroom (addressed by R12-4).
- **R12-4 (`fc155c9`) — reserved-capacity Large realloc, a Windows-effect
  follow-up to R12-3.** `large-reserved-capacity` reserves (but does not
  commit) a geometric 2× VA span up front, committing only the exact-span
  request; a growing realloc that fits within the reserved span commits
  just the missing tail (one `VirtualAlloc(MEM_COMMIT)`-class call) instead
  of falling back to alloc+copy+free. No new `unsafe`: reuses
  `aligned_vmem::reserve_aligned_lazy`/`commit_range` end to end (already
  used by the small-segment lazy-commit path). Measured: 2/4 growth-chain
  legs stay in-place (vs. 0/4 with exact-span-large alone), 33% fewer
  copied bytes, while commit-charge stays well below the pre-R12-3 4-MiB-
  rounded baseline.

**P1 — production-reachable fixes and new opt-in features:**

- **R12-5 (`7186f80`) — bound the cached `current_node()` staleness to a
  periodic refresh.** R11-5's NUMA-node cache was invalidated only at
  registry-slot claim/recycle boundaries; a long-lived, non-pinned thread
  the OS migrates mid-claim stayed pinned to the stale node for the rest of
  that claim — unbounded in wall-clock time. Fixed with a forced re-query
  every 128 cache hits (`NUMA_NODE_REFRESH_PERIOD`, chosen to match the
  order of magnitude of the directory's own sibling re-validation cadence),
  charged only to refill-miss/reservation call sites, never the bump-
  pointer hot path — measured ~2.5 ns added per cache hit, ~70–90× cheaper
  than the real syscall it occasionally replaces.
- **R12-6 (`4ea904f`) — finalize overflow-emptied segments beyond the
  64-base dedup cap.** `drain_heap_overflow`'s fixed 64-entry on-stack
  dedup buffer silently dropped pool/release finalization for the 65th+
  distinct segment emptied by cross-thread overflow-ring reclaims in a
  single drain pass (native `HEAP_OVERFLOW_CAP` is 2048, so this was
  reachable, if rare) — segments stayed correctly usable but sat outside
  pool-cap accounting at inflated RSS indefinitely. Fixed with a rare
  post-drain fallback sweep, gated on an `emptied_overflowed` flag so the
  common case pays nothing extra. New 66-distinct-segment regression test
  confirmed capped-at-64 behavior pre-fix, uncapped post-fix.
- **R12-7 (`f615703`) — class-aware dirty routing, wall-clock-gated then
  implemented.** `drain_dirty_segments` routed purely by segment, visiting
  every segment dirty for ANY class on a refill miss (R9-6 measured ~82%
  wasted visits at 4 concurrent producer classes, ~95% at 8, but deferred
  the wall-clock question). This round's own criterion bench confirmed the
  waste ratio is real (+134–171% ns/owner_alloc at N=1→N=4), then
  implemented `class-aware-dirty`: a lazily-materialized per-(segment,
  class) dirty-bit sidecar that lets a refill scan only the sought class's
  own word slice. Lost-wakeup safety by construction: a per-class bit is a
  visit HINT only, the drain body always fully drains the whole ring once a
  segment is visited, so a stale/redundant bit costs at most one wasted
  visit, never a silently-skipped entry — proved with a dedicated loom
  suite including a `#[should_panic]` counterfactual showing a genuinely
  partial drain design (the rejected alternative) *does* lose an entry
  under loom's interleaving search. Post-implementation re-measurement: ~19–
  23× reduction in ns/owner_alloc at N=8.
- **R12-8 — unify `AllocBitmap`+`MagazineBitmap` into a 2-bit
  `BlockStateMap`: NO-GO, no code changed.** Independent re-derivation
  reached the identical conclusion the codebase's own 2026-07-10 "G1
  honest-reject" record already established: the merge requires inverting
  load-bearing semantics at 15+ `AllocCore`-layer call sites that are
  deliberately magazine-blind (`carve_batch`'s leave-unset optimization,
  the freelist-drain legs, etc.), reopening a safety-critical double-free-
  detection boundary that was deliberately kept orthogonal by the
  already-shipped `MagazineBitmap` design (RAD-5, which got the ~50 Ir/op
  win *without* the invasive semantics change). No unexploited win remains
  of the size this task hoped for.
- **R12-9 (`a9ec36d`, `3b98ae4`) — split `alloc-lazy-commit` into
  `primordial-lazy-commit` (now in `production`) and
  `small-segment-lazy-commit` (stays opt-in).** The combined feature gave a
  ~5.1× smaller first-heap commit but was kept out of `production` because
  the full policy (lazy-committing every small segment) caused a 50–75×
  commit/decommit-syscall regression on decommit-heavy lifecycles (R8-10).
  Splitting isolates the two OS-reservation call sites: the primordial
  segment is reserved exactly once per process and is structurally excluded
  from the decommit lifecycle (`dec_live_and_maybe_decommit` hard-gates on
  `SegmentKind::Small`, which `Primordial` never satisfies), so it is safe
  from the R8-10 regression class by construction. User confirmed promoting
  `primordial-lazy-commit` to `production` after the measured ~5.14× win
  was independently reproduced.
- **R12-10 (`698cfca`) — virgin-carve zero-skip for Small `alloc_zeroed`
  (`virgin-zero-skip`, opt-in).** Implements a design verified twice
  (R9-5, R11-8, both CONDITIONAL GO): a genuinely first-touch bump-carved
  block on an OS-zero-guaranteed segment can skip `alloc_zeroed`'s explicit
  memset. A new owner-only `payload_virgin` bit tracks this per segment,
  withheld unconditionally under miri (matching the R9-1/#221 lesson that
  miri's `std::alloc` fallback does not zero-fill), defensively cleared on
  the one decommit-retain code path that could re-expose a
  decommitted-then-recommitted payload (currently dead in production, kept
  as a fail-safe). Personally verified non-vacuous: neutered the
  free-list-pop dispatch leg to wrongly claim virginity and confirmed 4 of 7
  tests failed with a genuine dirty-byte leak, then restored and reconfirmed
  green.

**P2 — smaller fixes, documentation, and re-evaluations:**

- **R12-11 (`5199148`) — gate `PageMap` maintenance behind a diagnostic-
  only feature.** The per-page class-tracking table was never load-bearing
  for production class routing (the class is always carried by the
  caller's `Layout` or the `RemoteFreeRing` entry) but was still maintained
  unconditionally on every carve/bootstrap/decommit-reset — until an
  inventory found it *is* a genuine test oracle for the §13 counterfactual
  regression gates, so it was feature-gated (`page-map-diag`) rather than
  deleted. Measured iai win on the carve/decommit-reset hot paths this
  closes (largest deltas: `multiseg_cold_256k` 490.1→329.2 Ir/op,
  `seg_cycle_decommit_256k` 339.7→286.1 Ir/op).
- **R12-12 (`a7db75a`) — `batch-api` marked honestly experimental**, per
  two consecutive external reviews (starting at R10): `#[doc(hidden)] pub
  unsafe fn alloc_batch`/`dealloc_batch` is still formally public Rust API
  for anyone who enables the feature. `batch-api` now requires
  `experimental` (nesting it under the crate's existing no-semver-
  guarantees umbrella); `#[doc(hidden)]` dropped from the `SeferAlloc`
  face in favor of a visible `# ⚠ EXPERIMENTAL / UNSTABLE` rustdoc section.
  No signature, behavior, or safety-contract change to any function.
- **R12-13 (`6d6e279`) — page-run layer design (R11-7): DEFERRED, NO-GO,
  no code changed.** R11-7 bundled two sub-problems: (a) per-object RSS
  waste and (b) `SegmentTable`-slot/syscall pressure at high live-object
  counts. R12-3's `exact-span-large` closes (a) almost completely
  (15.8×→~1.00–1.05× amplification) **when that opt-in feature is
  enabled**; (b) has no demonstrated victim anywhere in this codebase's
  tests/benches — and three of R11-7's four target size classes route
  through the cheaper Small-class path instead of Large only when the
  opt-in `medium-classes-wide` feature is enabled (`SMALL_MAX` = 1.75 MiB
  there). Neither feature is part of `production`, and `medium-classes-wide`
  was separately NO-GO'd for `production` over a large realloc regression,
  so `production`'s actual composition still routes 1.25–1.75 MiB objects
  through Large with whole-`SEGMENT` rounding today — this document's
  original "SUPERSEDED" wording read as though `production` itself had
  already closed the gap, which an independent review correctly flagged as
  premature; the wording was corrected to "DEFERRED — no demonstrated
  production victim" in Round 13 (R13-4, task #274,
  `docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md`, renamed from
  `..._SUPERSEDED.md`), with no change to the underlying technical
  analysis or numbers. The design doc is annotated with a pointer to the
  verdict, not deleted, in case a real `MAX_SEGMENTS`-bound workload is
  measured in the future.
- **R12-14 (`3dc7bd9`) — made the R12-1/R12-2 directory regression tests
  density-agnostic under `--all-features`.** Both tests were tuned against
  `production`'s `SMALL_MAX` (~253 KiB, ~16 blocks/segment) and silently
  broke under `medium-classes-wide`'s 1.75 MiB `SMALL_MAX` (exactly one
  block per segment) — not a directory bug, a hardcoded test-density
  assumption. Fixed by deriving allocation counts/classes from measured
  density and project constants instead of literals tuned for one feature
  combination.

### BREAKING CHANGE — `alloc-runfreelist` feature removed

The `alloc-runfreelist` experimental performance feature (PERF-3, the
run-encoded freelist / `RunStack`) has been **removed entirely** — the feature
flag, the source module, the cfg-gated branches in shared hot-path files, the
specialized test files, and the CI job that exercised the gated test bodies.
This is a semver-breaking feature removal, the same treatment the
abandon/adopt substrate got in round4.

**Why.** The feature reached a documented NO-GO verdict (Ф5 honest-reject):
it **regressed every one of the 11 iai benches**, including the four
cold/recycle targets it was designed to improve, by **+23 %–+31 % (Ir)**
instead of the predicted ≥5 % improvement. The wall-clock judge confirmed the
regression direction and magnitude (**+40 %/+43 %** on the 16 B/64 B cold
storm). See `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` §Verdict for the full
measurement. The feature was never added to `production`, never recommended for
use, and was not under active development; retaining it as a "ready starting
point for a future re-run" was pure maintenance drag — every
small-segment-lifecycle change since had to keep accounting for a
known-losing implementation with its own metadata layout, hot/cold branches
in shared hot-path files, and hundreds of lines of specialized tests. See
`docs/agent_reviews_round5/code_quality_review.md` (finding #5) and
`docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 / R4-5, never done).

**What was removed:**
- The `alloc-runfreelist = ["alloc-core"]` feature declaration (`Cargo.toml`).
- `src/alloc_core/run_stack.rs` (the `RunStack` type, `RunDesc`, `FOOTPRINT`,
  `RUNSTACK_CAPACITY`, and all six accessors `init_in_place`/`push`/`pop`/
  `peek`/`is_empty`/`clear_all`) and its `pub mod run_stack;` wiring in
  `src/alloc_core/mod.rs`.
- The `#[cfg(feature = "alloc-runfreelist")]` arms in `drain_freelist_batch`
  (`alloc_core_small.rs`), `flush_run` (`alloc_core_small_magazine.rs`),
  `decommit_empty_segment` (`alloc_core_small_pool.rs`), the bootstrap init
  (`bootstrap.rs`), the recycle init (`alloc_core_small.rs`), and
  `small_meta_end`/`run_stack_off` (`segment_header.rs`) — collapsed to just
  the shipped (classic linked-list) path.
- The tests `regression_r2_3_run_stack_class_guard.rs`,
  `regression_run_stack_decommit.rs`, `regression_run_stack_drain.rs`,
  `regression_run_stack_flush.rs`, `regression_run_stack_layout.rs`.
- The `cargo test --features "production alloc-runfreelist"` step in
  `scripts/check-all.mjs` and `.github/workflows/ci.yml` (`test-gated-bodies`).

**What was kept (NOT removed):** `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`
(the experiment's negative RESULT stays as institutional memory per this
project's honest-reject convention) and `docs/design/RUN_ENCODED_FREELIST_PLAN.md`
(the design plan that led to the experiment). The confined-`unsafe` count
dropped by 12 (6 in `run_stack.rs` + 3 in `alloc_core_small.rs` + 1 in
`alloc_core_small_magazine.rs` + 1 in `bootstrap.rs` + 1 in
`alloc_core_small_pool.rs`).

**Migration.** This feature was experimental and never recommended for use;
it was not part of `production` and had no non-test consumer. There is no
migration path because nothing depended on it. Any downstream `Cargo.toml`
listing `alloc-runfreelist` in its feature list will get a Cargo error
("unknown feature `alloc-runfreelist`") and should simply drop the feature.

### BREAKING CHANGE — `AllocCore`/`HeapCore::dbg_push_to_ring` narrowed to `unsafe fn`

`AllocCore::dbg_push_to_ring` and its `HeapCore` thin-delegation wrapper were
safe `#[doc(hidden)]` test hooks — the PRODUCER side of the cross-thread free
simulation — so fully-safe Rust could drive a deterministic stale-note→double-
issue chain under the `production` feature set (round5 `memory_safety_review`
R5-MS-4, HIGH): `alloc` a block, `dbg_push_to_ring` a "remote free" note for it
(no liveness/uniqueness check), `dealloc` it (own-thread free), `alloc`-re-issue
the same address (the hot path pops the freelist before draining the ring), then
`dbg_drain_all_rings` processes the STALE note — the re-issued block's bitmap
reads "allocated", the magazine predicate is always-false on a bare `AllocCore`,
and the generational guard is compiled out under `production`, so drain does
`write_next`/`mark_free` on the LIVE re-issue, yielding two live owners of one
range. No threads, no `unsafe` blocks, no type-system violation downstream — the
unsoundness was in the seam's contract, not any one caller's misuse (R5-F1 had
already fixed a `heap_xthread.rs` caller that misused this seam; this fix closes
the seam itself).

**Why.** The obligation the producer must uphold — "this push is at most one
logical remote free; the block is not freed/re-issued between the push and the
consuming drain" — is exactly the class of caller obligation Rust expresses via
`unsafe fn` + a `# Safety` doc, the same reasoning as R6-MS-1/2
(`dealloc`/`realloc`) and R6-MS-3 (`flush_class`). Under `production` the drain's
own guards are insufficient on their own, so the boundary moved from prose to the
compiler.

**What changed:** both `dbg_push_to_ring` entry points are now `pub unsafe fn`
with full `# Safety` docs and a tier-2 item-level `#[allow(unsafe_code)]` (the
`HeapCore` wrapper is `unsafe fn` too, so the chain is not left reachable
through it — mirroring R6-MS-1/2 making both `AllocCore` and `HeapCore`
`dealloc`/`realloc` unsafe). Every call site across `tests/`/`benches/` got an
`unsafe {}` block and a per-site `// SAFETY:` comment; the honoring callers
(single remote free) state the contract, the defensive callers (deliberate
contract-stress of the drain's `is_free`/magazine/generation guards) state which
guard recovers benignly. The drain side (`dbg_drain_all_rings` and the
`_checked`/`_impl` siblings) is LEFT safe — it is the consumer, and with the
producer now `unsafe fn` a contract-honoring caller can never produce a stale
note, so drain can never hit the chain; its reclaim guards remain defence-in-
depth. The `hardened`-only generational guard is NOT made unconditional — a
contract-honoring caller cannot hit the wrap-1/256 residual, so it stays a
probabilistic misuse backstop, not the primary soundness mechanism. New
`tests/regression_push_to_ring_unsafe_boundary.rs` proves the compile boundary
and the contract-honoring single-owner path. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`) grew by
two item-level sites (54 → 56).

### BREAKING CHANGE — `AllocCore`/`HeapCore::{dealloc,realloc}` narrowed to `unsafe fn`

`AllocCore::dealloc`, `AllocCore::realloc`, `HeapCore::dealloc`, and
`HeapCore::realloc` were safe `pub fn`s accepting a caller-supplied raw
pointer and `Layout` with no way to verify the pointer is a live allocation
start, that the `Layout` matches the actual block, or that the block hasn't
already been freed — so fully safe Rust could trigger real memory-safety
bugs (round5 memory_safety_review R5-MS-1/MS-2, CRITICAL, the fifth time this
class of finding was raised, this time with concrete counterexamples):
resurrecting a freed block via `realloc`'s same-class in-place branch,
overlapping `copy_nonoverlapping` UB via a `realloc` racing a LIFO re-issue,
releasing a live `Large` segment via an interior-pointer `dealloc`, and
double-freeing a stale-after-reuse pointer.

**Why.** These preconditions (valid live allocation start, matching layout,
freed at most once) are exactly the class of caller obligation Rust expresses
via `unsafe fn` + a `# Safety` doc, not prose a safe caller can violate
without a compiler warning — the same reasoning as the prior raw-memory-hook
narrowing above, applied to the allocator's two most load-bearing entry
points.

**What changed:** all four methods are now `pub unsafe fn` with full
`# Safety` docs. The `#[global_allocator]` adapter (`SeferAlloc`'s
`GlobalAlloc` impl) is unaffected at the API level — `GlobalAlloc::dealloc`/
`realloc` were already `unsafe fn`; they now call the core methods inside
their existing unsafe context. Every internal call site across `src/`,
`tests/`, `benches/`, `fuzz/`, and `examples/` was updated with an `unsafe {}`
block and a per-site `// SAFETY:` comment. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`,
CLAUDE.md/README/ARCHITECTURE/`src/lib.rs`) grew by these four new
item-level sites.

The `#[ignore]`d residual test `regression_xthread_double_free_no_corruption`
(which pinned a known cross-thread double-free residual as RED, tracked
under task #164/X7) was removed: its scenario is a genuine caller-side
double-free, which is now documented caller UB under the `unsafe fn`
contract rather than a soundness gap a safe caller could trigger. The
defence-in-depth regression coverage for the retained M2/X7 defensive
drain logic (which must still degrade a contract-violating double-free
benignly rather than corrupting memory) is preserved via the hardened
sibling test in the same file.

**Migration.** `AllocCore`/`HeapCore` are `#[doc(hidden)]` re-exports, never
stable public API. Any downstream call site now needs an `unsafe {}` block
around `dealloc`/`realloc`; the safety contract itself (valid live
allocation, matching layout, freed once) is unchanged — only its enforcement
moved from prose to the compiler. Code going through the public
`#[global_allocator]`/`GlobalAlloc` surface is unaffected.

### BREAKING CHANGE — registry control-plane fields narrowed to `pub(crate)`

`HeapSlot::state`, `HeapSlot::generation`, and `Registry`'s `slots`/`count`/
`free_slots`/`abandoned_segs` fields were `pub` (reachable through the
doc-hidden `pub mod registry` surface). Narrowed to `pub(crate)` to close
R4-MS-4 (CRITICAL) — a public field let safe downstream code force a
`LIVE → FREE` transition and re-push a slot onto `free_slots`, letting a
second thread's ordinary `claim()` steal a slot a first thread still had
cached, breaking the single-writer invariant `unsafe impl Sync for HeapSlot`
depends on.

**Why.** These fields were never intended as stable public API (every one
carries a "NOT stable public API" doc note and exists only because the
crate's `#[doc(hidden)] pub mod` test-only-export pattern requires the
enclosing module to be `pub`). The narrowing closes a real capability-boundary
gap; it does not change any documented, supported behavior.

**What was removed:** direct field access to the items above from outside the
crate. Replaced with narrow `#[doc(hidden)]` accessors on `Registry`
(`dbg_slot_state`, `dbg_slot_generation`, and one `unsafe fn
dbg_slot_preset_generation` for the one legitimate test that presets a
generation) for the tests that legitimately needed to observe this state.

**Migration.** No production code referenced these fields directly (they were
never part of the crate's supported public API). A downstream crate that was
relying on direct field access — an unsupported use of a `#[doc(hidden)]`
surface — will fail to compile (E0616) and should route through
`SeferAlloc`'s supported API instead; there is no supported use case this
narrowing removes.

### BREAKING CHANGE — public raw-memory test hooks narrowed to `unsafe fn`

Eight doc-hidden `pub fn` hooks (`RemoteFreeRing::{init,over}_test_buffer`,
`RunStack::{push,pop,peek,is_empty,init_in_place,clear_all}`,
`segment_header::{gen_at,bump_gen,init_gen_table_in_place}`,
`alloc_core_small.rs`'s `dbg_corrupt_freelist_head_next`/
`dbg_drain_freelist_batch`/`dbg_alloc_bitmap_bytes_for`/
`dbg_magazine_bitmap_bytes_for`/`dbg_payload_start_for`,
`alloc_core.rs`'s `dbg_unregister`/`dbg_recycle`, `numa::bind_segment`)
accepted a caller-supplied raw pointer/base with an unenforceable prose-only
safety contract — a safe downstream call with an invalid pointer could
trigger a library-side invalid read/write with zero `unsafe` at the call
site (R4-MS-3).

**Why.** The validity/size/alignment/lifetime/exclusivity of a caller-supplied
pointer is fundamentally unverifiable by the callee; that contract belongs in
the function signature (`unsafe fn` + `# Safety`), not in prose a caller can
ignore without a compiler warning.

**What changed:** each hook above is now `pub unsafe fn` with a `# Safety`
doc section. This introduced a second, item-level tier of confined `unsafe`
(alongside the existing 13 module-level seams) — see the source-of-truth
inventory command in `CLAUDE.md`/`README.md`/`docs/ARCHITECTURE.md`/
`src/lib.rs`, now `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`.

**Migration.** These are `#[doc(hidden)]` items, never stable public API. Any
downstream call site now needs an `unsafe { }` block; the safety contract
itself is unchanged (only its enforcement moved from prose to the compiler).

### BREAKING CHANGE — removal of the abandon/adopt substrate

The abandoned-segments / adoption substrate (the unreachable segment-transfer
protocol that predated Phase 12.5's whole-slot reuse) has been **removed
entirely**. This is a semver-breaking API removal. It mirrors the
`LargeCacheMode::{Background, Both}` removal precedent ("make invalid states
unrepresentable"); git history preserves the code if a future
decommit-when-empty policy ever needs to reintroduce segment transfer.

**Why.** The substrate was unreachable on every production path: whole-slot
reuse (Phase 12.5) recycles a slot's `HeapCore` whole on thread exit, so
`abandon_segments` / `try_adopt` were never called. It was also internally
inconsistent even on its own terms — `try_adopt` ignored the result of
`register_segment_internal` (silently proceeding even if registration failed),
`reset_stamp_cache` (documented as required on cross-heap segment transfer)
was never called, and its intrusive linked-list field (`SegmentHeader::next_abandoned`)
was shared with the LIVE `deferred_large` cross-thread-free stack (a separate,
production feature). Retaining it as a "loom-proven basis for a future policy"
was therefore an illusion: the documented future scenario already diverged
from the code's live invariants, and a naive reactivation would clobber the
`deferred_large` stack. See `docs/agent_reviews_round4/code_quality_review.md`
(finding #4) and `docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 /
R4-5).

**What was removed:**
- `HeapRegistry::{abandon_segments, push_abandoned_segment,
  pop_abandoned_segment, try_adopt}` and the private helpers
  `push_abandoned_segment_into` / `abandon_one_segment`
  (`src/registry/heap_registry.rs`).
- `Registry::abandoned_segs` field and the abandoned-head packing helpers
  `pack_abandoned_head` / `unpack_abandoned_head` / `abandoned_head_is_empty` /
  `ABANDONED_HEAD_EMPTY` / `ABANDON_TAG_MASK` / `ABANDON_TAG_BITS` /
  `ABANDON_SEG_SHIFT` / `ABANDON_SEG_SIZE` (`src/registry/bootstrap.rs`).
- `OWNER_STATE_ABANDONED`, `unpack_owner_state`, `unpack_owner_gen`, and
  `OWNER_GEN_MASK` (`src/alloc_core/segment_header.rs`) — used only by the
  abandon/adopt CAS. (`owner_state`, `OWNER_STATE_LIVE`, `pack_owner`,
  `unpack_owner_id`, `OWNER_ID_NONE` are RETAINED: the LIVE owner-id
  resolution path for cross-thread free routing still uses them.)
- The dead adoption forwarders `register_segment_internal` /
  `set_small_current_internal` (`src/registry/heap_core.rs`) and
  `register_segment` / `set_small_current` (`src/alloc_core/alloc_core.rs`)
  — their sole caller was `try_adopt`.
- The tests `loom_abandoned_segs_aba.rs`,
  `regression_abandoned_stack_safe_api_uaf.rs`,
  `regression_abandon_a1_next_abandoned_field_sharing.rs`, and
  `loom_registry.rs` (entirely); the abandon-specific tests in
  `registry_basic.rs` and `regression_gen_table_lifecycle_seams.rs` (Seam 3);
  and the abandoned-head packing Kani proofs in `src/kani_proofs.rs`.

**What was kept (NOT removed):** `SegmentHeader::next_abandoned` (the field)
and `next_abandoned_atomic()` (the accessor), the `ABANDONED_TAIL` sentinel,
and the entire `src/alloc_core/deferred_large/` module — these are the LIVE
`deferred_large` cross-thread-free stack, a separate production feature that
reuses the same header field. Its tests (`loom_deferred_large`,
`regression_xthread_large_free_no_leak`) pass unchanged.

**Migration.** No production code referenced the removed items. Downstream
code that reached the `#[doc(hidden)] pub mod registry` surface and called
`HeapRegistry::abandon_segments` / `push_abandoned_segment` /
`pop_abandoned_segment` / `try_adopt` will fail to compile (E0425/E0061) and
should drop the call — whole-slot reuse (the only production teardown path)
makes segment abandonment unnecessary.

### BREAKING CHANGE — removal of `Default for AllocCore`

The `Default` impl on `AllocCore` (feature = `alloc-core`) has been **removed
entirely**. This is a semver-breaking API removal.

**Why.** `AllocCore::new()` / `AllocCore::new_with_config()` return
`Option<Self>` because the very first thing construction does is a real
multi-MiB OS memory reservation for the primordial segment, which can fail
under memory pressure / OOM / `rlimit`. The `Default` impl hid that
fallibility behind `.expect(...)`, i.e. a panic. Generic code across the
ecosystem treats `T::default()` / `T: Default` as a conventionally-cheap,
infallible operation (`Option::<T>::unwrap_or_default()`, `#[derive(Default)]`
on a containing struct, `mem::take`, collection `resize_with(Default::default)`,
etc.) — none of those call sites expect a multi-MiB syscall plus a latent
panic. The implementation had no internal callers (verified by grepping the
whole tree), so the impl was a footgun for hypothetical generic-bound users
rather than a load-bearing convenience. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (R3-C / N3).

**What was removed:**
- The `impl Default for AllocCore` block in `src/alloc_core/alloc_core.rs`
  (and its doc comment).

**Migration.** Replace any `AllocCore::default()` (or `T: Default`-driven
construction) with an explicit `AllocCore::new().expect("...")` or
`AllocCore::new_with_config(cfg).expect("...")` — making both the fallibility
and the panic visible *at the call site*, where they belong, rather than
hidden inside a trait impl elsewhere. If you want to preserve the exact old
message, use `AllocCore::new().expect("AllocCore::new: primordial segment
reservation failed (OOM)")`.

### BREAKING CHANGE — removal of `LargeCacheMode::{Background, Both}`

The `LargeCacheMode` enum (feature = `alloc-decommit`) has been reduced to
its single implemented variant, `Lazy`. The `Background` and `Both`
variants — placeholders for a background scavenger thread that was never
implemented — have been **removed entirely**. This is a semver-breaking
API removal.

**Why.** `Background` and `Both` had no implemented behaviour: they were
stored by the builder and silently degraded to `Lazy` at runtime. An
earlier fix (T5) made materialising a heap with either variant `panic!`
at resolution time, but that panic was reachable lazily through the
`GlobalAlloc::alloc` entry point (first-bind materialises the per-thread
heap), which conflicted with the crate's "never panics" guarantee on its
allocation entry points. Removing the variants outright ("make invalid
states unrepresentable") is safer than either the silent no-op or the
panic: there is no longer an unrepresentable promise to reject. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (решение №2).

**What was removed:**
- The `Background` and `Both` variants of `LargeCacheMode`.
- The resolution-time `panic!` match in `AllocCore::new_with_config`
  (T5's eager rejection) — nothing left to reject.
- The two `should_panic` regression tests
  (`background_mode_panics_at_materialisation`,
  `both_mode_panics_at_materialisation`) in `tests/large_cache_mode.rs`.

**Forward compatibility.** `LargeCacheMode` is now marked
`#[non_exhaustive]`. Reintroducing a variant alongside a real future
background-scavenger implementation will be a *non-breaking* addition,
not another breaking change. Code that constructs `LargeCacheMode::Lazy`
is unaffected; any code that referenced `Background`/`Both` will fail to
compile (E0599 — no such variant) and should drop the reference.

**Migration.** Remove any `.mode(LargeCacheMode::Background)` or
`.mode(LargeCacheMode::Both)` call — `Lazy` (the default, and the only
mode that ever had implemented behaviour) is what both were already
doing.

### BREAKING CHANGE — removal of the `Heap` / `with_heap` public face and the `alloc` feature

The explicit `Heap` type (`src/heap/heap.rs`), its TLS binding `with_heap` /
`with_heap_try` (`src/heap/tls.rs`), and the `alloc` Cargo feature that gated
them have been **removed entirely**. This is a semver-breaking API removal.

**Why.** `Heap` was a thin wrapper around `AllocCore` with no per-thread
magazine cache. The production `#[global_allocator]` face (`SeferAlloc`, backed
by the registry-resident `HeapCore`) already has the magazine fast path and
does not use `Heap` at all. A head-to-head benchmark
(`docs/HEAP_BENCH.md`, preserved as a historical record) showed `Heap` running
~9–12x slower than mimalloc on the steady-state alloc/dealloc hot path — the
gap that triggered the decision to remove `Heap` rather than invest in speeding
it up, since the magazine-backed `SeferAlloc` path supersedes it entirely.

**What was removed:**
- The `Heap` struct and its `impl` (`new`/`alloc`/`dealloc`/`realloc`/
  `alloc_zeroed`/`dealloc_any_thread`/`Drop`).
- The `with_heap` and `with_heap_try` TLS bindings and the
  `RefCell<Option<Heap>>` thread-local.
- The `alloc` Cargo feature (it gated only `Heap`/`with_heap`).
- The `src/heap/` module entirely (`heap.rs`, `tls.rs`, `thread_free.rs`,
  `mod.rs` — all existed solely for `Heap`).
- The `benches/heap_alloc.rs` bench and its `[[bench]]` entry.
- The `regression_with_heap_no_panic` test (tested the `with_heap` no-panic
  contract — coverage of `with_heap` is removed by design).
- The `regression_heap_xthread_large_free_no_leak` test (the `Heap`-face A1
  regression; the parallel `HeapCore`-face regression
  `regression_xthread_large_free_no_leak` remains and covers the same fix).
- The `heap_cross_thread` and `heap_miri_xthread` tests (exercised
  `Heap::dealloc_any_thread`; `HeapCore` does not expose a public cross-thread-
  free entry point, so these cannot be faithfully rewritten without inventing
  new public API. Cross-thread coverage lives on against `SeferAlloc`/
  `HeapCore` via `global_alloc_mt.rs`, `concurrent_stress.rs`, etc.).

**What was rewritten (coverage preserved):**
- `heap_cross_segment`, `heap_diag`, `heap_differential`, `heap_invariants`,
  `heap_soak`: rewrote from `Heap` to `AllocCore` directly (faithful 1:1
  substitution — under the single-thread `alloc` feature `Heap` was a pure
  pass-through to `AllocCore`). The two `with_heap` TLS tests in
  `heap_invariants` were removed (they tested `with_heap` specifically).
- `numa_alloc`: tests 1 and 3 already used `AllocCore` directly (unchanged);
  test 2 (`cross_node_handoff_safe`, which used `Heap::dealloc_any_thread`)
  was removed (cross-thread NUMA-handoff coverage lost — `HeapCore` does not
  expose `dealloc_any_thread`; see "coverage lost" below).
- `stamp_cache` test 3: rewrote from `Heap::dealloc_any_thread` end-to-end
  cross-thread free to a direct `dbg_owner_id_for` stamp readback (preserves
  the OPT-C stamp-cache coverage; loses the end-to-end cross-thread-free leg).
- `regression_xthread_large_free_layout_mismatch`: deleted only the `heap_face`
  submodule (the `HeapCore`-face tests remain).
- `regression_hardened_interior_ptr`: both tests already used `HeapCore`/
    `AllocCore` (not `Heap`); only a doc comment was updated.

**Coverage lost (cannot be faithfully rewritten without new public API):**
- `Heap::dealloc_any_thread` cross-thread free via the explicit-`Heap` face:
  `HeapCore` does not expose a public `dealloc_any_thread` equivalent (cross-
  thread routing lives inside the private `dealloc_routing`, reachable only
  via `SeferAlloc::dealloc`). The miri-targeted `heap_miri_xthread` and the
  `numa_alloc::cross_node_handoff_safe` tests exercised this path directly.
  Miri coverage of the substrate continues via `decommit_miri_cycle.rs`; cross-
  thread NUMA coverage is a decision point for a human (whether to expose a
  `HeapCore::dealloc_any_thread`-shaped public API or accept the loss).
- `with_heap` no-panic reentrancy contract: removed by design (the API is
  gone). The production `SeferAlloc` face has its own reentrancy-safe TLS
  binding (`global::tls_heap`) which is structurally reentrancy-free (raw
  `Cell<*mut HeapCore>`, no `RefCell` borrow state).

**Migration.** Users of `Heap`/`with_heap` should switch to `SeferAlloc`
(`#[global_allocator] static A: SeferAlloc = SeferAlloc;`) or, for direct
substrate access, `AllocCore` (`alloc-core` feature). There is no `Heap`-
shaped replacement with `dealloc_any_thread`; cross-thread free is reached via
the `SeferAlloc` global face.

**Feature rewiring.** `alloc-xthread` and `alloc-global` previously depended
on `alloc`; they now depend on `alloc-core` directly (the `alloc` feature's
only content was `Heap`/`with_heap`, so depending on it would be a no-op).
The `production` feature bundle (`alloc-global + alloc-xthread + alloc-decommit
+ fastbin`) is unchanged in effect.

### Security & compliance remediation (SEC-1 through SEC-6)

A `/fxx` security/compliance audit
([`docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md`](docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md),
research-only — no source touched) found the unsafe-confinement, dependency,
secrets, and MSRV claims all VERIFIED as advertised, and ten lower-severity
process/documentation gaps. SEC-1 through SEC-6 close six of them (three
MEDIUM, three LOW). No code defect was found or fixed — the pass hardens
disclosure, CI supply-chain posture, and the user-facing hardened-tier docs.

- **SEC-1 (`c3389de`, #198) — `SECURITY.md` shipped with a non-functional
  e-mail fallback.** The fallback section carried the literal placeholder
  `REPLACE_WITH_REAL_EMAIL` plus a `<!-- PLACEHOLDER: ... -->` banner, and no
  real maintainer address exists anywhere in the repo to source a genuine one
  from (`Cargo.toml` has no `authors`/email field). Rather than invent a
  plausible-looking placeholder, the e-mail fallback channel is **removed
  entirely** (−15 lines); private disclosure now relies solely on **GitHub
  Security Advisories**, which was already the preferred channel and remains
  fully functional.
- **SEC-2 (`94fc4f4`, #199) — `SECURITY.md` supported-versions table was
  stale.** It declared "`0.1.x` (current) — Yes" while the published crate is
  `0.3.0` — literally promising patches only for the `0.1.x` line. Reworded to
  "**Latest `0.x` release (see crates.io)**" so the table does not go stale
  again on the next patch/minor bump.
- **SEC-3 (`c81246f`, #200) — README's X7 residual disclosure was stale.**
  The README "documented residual" paragraph (≈line 701) still cited #164 as
  the pending fix and the `hardened` feature-matrix row (≈line 778) described
  only the H1 interior-pointer guard, with no mention of the X7 generational-
  ring arc that closed the re-issue-before-drain leg under `--features
  hardened`. (The X7 closure and its 1/256 wrap were fully documented
  internally — `DURABILITY.md`, this CHANGELOG, the X7 plan — but absent from
  the surface a security-conscious consumer evaluating `hardened` would
  actually read; audit finding §1.5.) Both passages now state the residual
  taxonomy correctly: two of three legs closed on plain `production` (X2/#164,
  R1), the third closed under `hardened` **except the 1/256 wrap**, which is
  named explicitly as the accepted probabilistic residual-of-the-residual.
  The plain-production residual disclosure is not weakened.
- **SEC-4 (`fd05274`, #201) — `permissions: contents: read` added to all
  three workflows.** `.github/workflows/{ci,release,perf-gate}.yml` previously
  ran with the repository-default `GITHUB_TOKEN` scope (legacy read/write on
  older repos). Traced every job/step in all three files: no job needs
  contents-write — `ci.yml` is checkout+cargo; `release.yml` publishes via the
  separate `CARGO_REGISTRY_TOKEN` secret, not `GITHUB_TOKEN`; `perf-gate.yml`
  caches/uploads via its own scoped backends. Workflow-level `contents: read`
  applied to all three; no job needed a broader override.
- **SEC-5 (`d70cd19`, #202) — new `deny.toml` + CI `deny` job
  (cargo-deny).** Closes audit gaps §1.3 (cargo-audit never run, tool absent
  locally) and §1.6#3/§2.2 (license compatibility manually assessed, not
  machine-checked). `cargo-deny` was chosen over `cargo-audit`-alone because
  it covers both RustSec advisories **and** license compatibility in one
  tool/one job. New `deny.toml` at the repo root: `[advisories]` with a
  narrow per-ID-documented ignore list; `[licenses]` allow-list built from
  cargo-deny's actual report against the current full-feature tree (MIT /
  Apache-2.0 / Zlib — narrower than the audit's manual §2.2 inventory,
  reconciled in the task report; no copyleft found either way); `[bans]`
  permissive (duplicate-version = warn); `[sources]` crates.io-only. At the
  time, two narrowly-scoped dev-only ignore entries: **RUSTSEC-2025-0141**
  (`bincode` 1.3.3 unmaintained; reaches this workspace ONLY through
  `iai-callgrind`, the Linux-only CI perf-gate bench — NOT in the published
  runtime tree) and **RUSTSEC-2026-0173** (`proc-macro-error2` 2.0.1
  unmaintained; same `iai-callgrind` dev-only chain). A third was added later
  this session — see the "CI fixes" subsection below.
- **SEC-6 (`91a6dac`, #203) — SHA-pinned `actions/checkout@v5` in
  `release.yml`.** Scoped to the token-bearing workflow per audit finding
  §1.6#2 (this is the only workflow carrying `CARGO_REGISTRY_TOKEN`, so
  tag-rewrite supply-chain risk matters most here). `actions/checkout@v5` →
  pinned to the exact commit SHA `v5` currently resolves to (verified via
  `git ls-remote`), with a trailing `# v5` comment for readability.
  `dtolnay/rust-toolchain@stable` was **deliberately left tag-pinned** — it is
  a moving branch by design (tracks the latest stable toolchain), and pinning
  it to a SHA would defeat its purpose; the conscious decision is recorded in
  the commit message.

### PERF-1 — README bench-doc sync (`650a3ed`, #205)

The README carried two disagreeing cold-direct tables: the dedicated "Cold
first-touch" section still showed P3-era numbers (16 B 1.60× slower, 64 B
1.15× slower, 256 B parity, 1024 B 1.84× faster), while the main dated
"Performance" table already had the correct post-X-arc re-measurement. A
full-file grep found **five** total occurrences of the stale ratios (the intro
bullet, the P0–P6 narrative, the "where we still trail" callout, the dedicated
Cold first-touch table + prose, and the Honest verdict bullets). All five were
synced to the post-X-arc measured ratios — **2.5× / 2.1× / 1.8× slower on
16 B / 64 B / 256 B cold-direct, 1.12× faster on 1024 B** (measured
2026-07-06 post-X-arc) — each explicitly labeled as post-X-arc vs preserved
P3-era history (the P3-era history is not erased; it carries a provenance
note). Docs-only; no source change.

### PERF-2 — TCACHE_CAP / FLUSH_N sweep: honest-reject (`e6f5112`, #206)

**REJECT (all three candidates).** A `/fxx` research hypothesis (#206 / PERF-2)
proposed that a larger per-class magazine (`TCACHE_CAP`, default 16) would
amortize refill/flush orchestration cost on storm-shaped alloc/free patterns
(the cold first-touch gap vs mimalloc). Tested `TCACHE_CAP = 32 / 64 / 128`
against the default `16` on **both** judges: the 11-bench iai
instruction-count gate and the wall-clock `global_alloc` criterion bench (the
exact 1024-op cold-storm shape the hypothesis targeted). Every candidate
**regressed every bench, including the explicit targets** (cold / recycle /
the `global_alloc` storm), and the regressions grew **monotonically and
super-linearly** with CAP. Pure experiment — **zero source changes survived**
(`git diff` to `src/` empty at the end; this doc is the only new file).
Recorded per the project's reject-with-numbers precedent so the next reader
does not re-run the same sweep blind. Full tables and mechanism in
[`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`](docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md).

- **CAP=32 reproduces X4-A** (the 2026-07-05 reject) within binary-layout
  noise: recycle +32,779 Ir (+18%), churn +22,863 (+28%), cold +25,763 (+21%),
  every other bench regressed too. Mechanism (X4-A's, re-confirmed): each
  refill/flush doubled in size (bigger carve/flush batches, larger `Tcache`
  zero-init at heap claim, longer M2 in-magazine scan); the benches don't
  refill-miss enough to amortize the larger batches.
- **CAP=64** is strictly worse on every bench (monotonic): recycle +88,949 Ir
  (+50%), churn +56,033 (+69%), cold +66,881 (+53%).
- **CAP=128 — super-linear regression; the decisive signal.** The `Tcache`
  struct footprint grows from **6.4 KiB → 50.2 KiB/thread** for `slots` alone
  (`49 × 128 × 8 B`) and **spills L1** — visible in the L2-hit column jumping
  ~160 → ~1000 on `small_churn_16b`: the magazine metadata itself stopped
  being L1-resident. Wall-clock confirmed on the exact storm shape: the sefer-
  vs-mimalloc gap on `global_alloc/16B` **WIDENED from 2.5× to 4.9×** at
  CAP=128 instead of narrowing (64B 2.3×→5.1×, 256B 1.46×→3.9×, 1024B
  0.90×→2.0×). The storm hypothesis's own arithmetic ("1024/16 = 64 refills →
  1024/128 = 8 refills, an 8× amortization win") is overwhelmed by the
  per-refill cost growth (8× larger carve batch + L1-spill). The companion
  predictions also failed against measurement: `churn_256b`/`small_churn_16b`
  were predicted CAP-insensitive but regressed monotonically (the first alloc
  of each iteration triggers a full refill — larger CAP = larger refill batch
  + larger `Tcache` zero-init); `large_alloc_free_cycle` regressed too
  despite doing NO small-block magazine work (pure `Tcache` zero-init at heap
  claim).

**Verdict.** mimalloc's advantage is **NOT a deeper magazine — it is a
structurally cheaper refill** (a `mmap`/page free list with no per-refill
orchestration equivalent), which a larger CAP cannot replicate and in fact
punishes. The CAP parameter is already at its optimum (16); CAP=64 and CAP=128
are the two never-before-measured values and are strictly worse. The shape that
could win is a **cheaper refill, not a rarer refill** — exactly the family
PERF-3 (below) then attempted on the recycle flush→drain path.

### PERF-3 — run-encoded freelist arc (Ф0–Ф5): IMPLEMENTED then honest-rejected

PERF-2 named "cheaper per-block work on the hot recycle path" as the winning
family of attack. PERF-3 was the concrete realization of that family for the
recycle flush→drain path: encode contiguous same-class runs as compact
`(start_off, count)` descriptors so the drain side reconstructs member
addresses by stride arithmetic (`start_off + i*block_size`) instead of pointer-
chasing `Node::read_next` per block. Design:
[`docs/design/RUN_ENCODED_FREELIST_PLAN.md`](docs/design/RUN_ENCODED_FREELIST_PLAN.md).
Verdict (Ф5):
[`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`](docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md).
Five phases, each committed between phases with a zero-trust review by @o46m
(GO each on Ф1–Ф4); the Ф5 measurement is the honest-reject.

- **Ф0 (`2732dfc`, #207) — design doc.** No src/ code; mirrors the X7 plan's
  structure (key insight → fixed decisions → phases Ф0–Ф6 → risks →
  readiness). Targets the "cheaper refill, not rarer refill" family PERF-2
  identified.
- **Ф1 (`5c5b6af`, #208) — `RunStack` storage + Layout.** New module
  `src/alloc_core/run_stack.rs` (`RunStack`, `RunDesc { start_off, count }`
  compact descriptors for contiguous-offset same-class runs,
  `RUNSTACK_CAPACITY = 8` per class, `Layout::run_stack_off` /
  `small_meta_end` shift to carve the RunStack region into segment metadata).
  New **`alloc-runfreelist`** Cargo feature (`= ["alloc-core"]`, **opt-in,
  NOT in `production`**). Storage only — no allocator behavior wired up yet.
  Production-judge 11/11 byte-identical (neutrality gate).
- **Ф2 (`7d5bada`, #209) — flush-side contiguous-run detection in `flush_run`.**
  Under the feature, `flush_run` collects accepted (guard-passing) freed
  offsets and detects contiguous sub-runs to divert into RunStack descriptors.
  **Empirical finding:** the magazine's LIFO refill returns blocks in
  **descending** address order, so a flush batch built from magazine order is
  descending, not ascending, and **in-place adjacency detection found ~0%
  contiguity** on the target `bench_direct_alloc` pattern. **Sorting
  (ascending) recovered 100% adjacency** on the target pattern — so the
  landed detector is sort-then-detect, not in-place. (This finding is load-
  bearing for the Ф5 mechanism analysis below.)
- **Ф3 (`f13ec4b`, #210) — drain-side stride reconstruction in
  `drain_freelist_batch` — "the heart."** Full `#[cfg(feature =
  "alloc-runfreelist")]` / `#[cfg(not(...))]` body split. Feature-on: drain
  RunStack for the class FIRST (pop descriptors, reconstruct member offsets by
  `start_off + i*block_size` instead of pointer-chasing `read_next`, guard
  `bm.is_free(off)` before `mark_alloc` + hand-out — fail-safe skip, never
  panics), THEN drain the classic linked list for remaining `out` capacity.
  The `is_free` guard is **mandatory defense-in-depth** (plan §2.3): the M2
  bitmap stays the ground truth, not the descriptor — a reconstructed offset
  that is somehow not free is skipped, never mis-linked.
- **Ф4 (`3e097be`, #211) — `decommit_empty_segment` clears RunStack +
  drain-overflow fix.** (a) On decommit, `RunStack::clear_all(base)` runs
  before `set_decommitted` — stale descriptors would otherwise reconstruct
  addresses into unmapped payload memory on a later drain (opposite policy
  from X7's gen table, which is deliberately NOT re-zeroed: RunStack
  descriptors are address hints into payload, so stale hints are unsafe, not
  merely stale). (b) Also fixed a narrow **descriptor-overflow-on-drain leak**
  found during Ф3's review: classes with `block_size > 8192 B` could have a
  descriptor larger than a single drain call's `out` capacity — fixed via a
  truncated-remainder pushback (`RunStack::push` of the un-drained tail when
  `out` fills mid-descriptor).
- **Ф5 (`154d1fa`, #212) — THE VERDICT: NO-GO / honest-reject.** Measurement-
  only phase (no source changes). Applied the pre-declared GO/NO-GO gate
  (design doc §3-Ф5) mechanically. The feature **REGRESSED every one of the 11
  iai benches**: the 4 cold/recycle targets (the feature's whole design goal)
  regressed **+23% to +31% Ir** (needed ≥5% **improvement** — `cold_16b`
  +23.04%, `cold_64b` +23.89%, `recycle_16b` +31.03%, `recycle_64b` +31.03%);
  the other 7 regressed **+0.75% to +4.33%** (6 of 7 breach the ≤+1% ceiling;
  only `realloc_grow` +0.75% sits inside it, because its hot path is the
  large-block realloc copy, not the small-block recycle path). Wall-clock
  confirmed on the exact storm shape: **+40.5%** on `global_alloc/16B`,
  **+42.5%** on `64B`, +43.2% on `256B`, +68.9% on `1024B` (criterion's own
  paired `change:` field p = 0.00 < 0.05 on every row). All three NO-GO
  triggers fire simultaneously — not a close call.

  **Root cause (confirmed by @o46m's independent code review):** the landed Ф2
  implementation **AUGMENTS** the classic per-block `write_next` chain-build
  rather than **diverting** from it — every accepted block still pays the full
  classic linked-list cost, and the sort/detect/push/rebuild machinery runs as
  an **ADDITIONAL** pass on top, not instead of it. The single `read_next`
  load the drain side saves per run-member block is dwarfed by this added
  flush-side cost. The **L1-hits column is the smoking gun**: for
  `recycle_alloc_free_256x16b`, ON L1 hits = 336,531 vs OFF 260,773 — a rise
  of **+75,758 L1 hits**, almost exactly matching the +55,593 rise in Ir (the
  new instructions are predominantly L1-resident memory ops: the offset
  array, the sort permutation, the RunStack slots). There is no level of the
  cache hierarchy where the feature wins (L2 flat ~174→~176, RAM flat-to-
  slightly-up 5,335→5,419). The "eliminate the dependent-load pointer chase"
  hypothesis is **refuted**: the pointer chase was already prefetcher-covered
  and cheap (the design doc's own §5 readiness note had flagged this as the
  failure mode). The design doc §1's honesty caveat — "this plan introduces a
  different representation, where hoist is *possible*" — was correct that the
  hoist is possible; the measurement shows it is not *profitable*.

  **Disposition: feature stays OFF / opt-in-only** (`alloc-runfreelist`, NOT
  in `production`; Ф6 is not triggered). Source is **KEPT, not reverted** —
  (1) **zero production cost**: the feature-off build is byte-identical to the
  pre-PERF-3 build (the neutrality gate, verified again by Ф5's baseline
  reproducing the 11-bench reference digit-for-digit); (2) the code is
  **correct, reviewed, and tested** — Ф1–Ф4 each passed @o46m zero-trust
  review, each has dedicated regression tests (`tests/regression_run_stack_*`),
  and the M2-double-free-through-run and decommit-clears-runstack safety cases
  are explicitly covered; (3) the loss is an **algorithmic-cost loss, not a
  correctness loss**, and the algorithm can be revisited — a future "PERF-3.5"
  reworking `flush_run` to genuinely **DIVERT** (skip `write_next` for detected
  run-members — write the descriptor instead of the chain link) rather than
  augment could in principle tip the trade; the storage (Ф1), drain-side
  reconstruction (Ф3), and lifecycle seams (Ф4) are reusable as-is, only Ф2's
  flush-side algorithm would need rework. (Precedent: PERF-2 left no source
  because it temp-edited a constant — nothing reusable to keep; PERF-3 landed
  four phases of real reviewed implementation, and the honest-reject is of the
  *measured outcome*, not the *code quality*.)

**Combined with PERF-2, this establishes:** sefer's remaining small-size gap
vs mimalloc is **not closeable by either a deeper magazine OR a cheaper-per-
block recycle representation of this shape** — the gap is structural in the
refill/flush orchestration itself (`find_segment_with_free` / latch /
carve-batch machinery), which is where a future PERF-4 should look.

### New dev scripts

- **`scripts/bench-table.mjs` — `npm run bench:table` (`73a6b2b`).** Runs the
  comparative wall-clock bench and **always prints the SAME canonical tables**
  (ns/op units, fixed bench set, vs-mimalloc ratio column). Written because
  ad-hoc benchmark tables varied in units/subset/format run to run — once
  causing a spurious apparent "20 ns → 40 ns regression" that was actually a
  µs-per-1024-op-batch vs ns-per-op unit mixup. The canonical table is now the
  single source of truth whenever comparative numbers are asked for.
- **`scripts/check-all.mjs` — `npm run check` (`29087c5`).** Single pre-push
  gate: `cargo fmt --check`, `clippy -D warnings` across all three CI feature-
  matrix entries (`""`, `--features experimental`, `--all-features`), `cargo
  test` under `production` and `production alloc-runfreelist`, then `npm run
  iai` (the deterministic judge). Fails fast at the first red step. Written
  after a push this session shipped 17 commits with a red CI (rustfmt drift
  accumulated across the PERF-3 phases, plus two ci.yml jobs pointing at a
  Cargo feature and test files deleted by task #204 — see the next section)
  that this command would have caught locally in under 5 minutes. It does NOT
  replace CI (CI additionally runs miri, loom, TSan, multi-arch, no_std, MSRV)
  but catches the most common drift class before a push, not after.

### CI fixes — found and fixed via a red CI run this session

A push mid-session went red on CI (Actions run 28846975468); the fixes below
landed in the same session. All are style/lint/drift — zero behavior change
(verified via judge byte-identical + full test suite green on each).

- **`d9767fe` — `cargo fmt --all` + clippy fixes across the CI feature
  matrix.** The PERF-3 arc (Ф1–Ф5) landed real code without a final
  `cargo fmt --all` + full clippy-matrix pass, so CI's fmt and clippy gates
  were red on push. `cargo fmt --all`: mechanical reformat (line-wrapping) in
  `alloc_core.rs`'s Ф2–Ф4 additions and the new `regression_run_stack_*.rs`
  test files. `clippy -D warnings` across all three CI matrix entries:
  `needless_return` (`return k;` → `k` in the `alloc-runfreelist` branch of
  `drain_freelist_batch`, tail position under `--all-features`),
  `manual_is_multiple_of` (`off % MIN_BLOCK as u32 == 0` → `.is_multiple_of(…)`
  in `remote_free_ring.rs`), `bool_assert_comparison` +
  `nonminimal_bool` (`assert_eq!(expr, true)` → `assert!(expr)` in
  `regression_gen_wrap_boundary.rs` / `regression_run_stack_layout.rs` — same
  assertions, same failure messages), `doc_lazy_continuation` (a blank `//!`
  line to split a markdown-list lazy continuation in
  `regression_gen_wrap_boundary.rs` /
  `regression_refill_window_double_issue.rs`), `assertions_on_constants`, and
  `iter_cloned_collect`. Purely style/lint; zero semantic change.
- **`ad1d533` — two CI workflow jobs referenced code deleted by task #204
  (Heap removal).** The `loom (loom_thread_free)` matrix entry passed
  `--features "alloc"` (a Cargo feature that no longer exists — it only ever
  gated the removed `Heap` type; the test's synthetic `Node` model never
  actually depended on it — feature set changed to `""`). The `thread-
  sanitizer` job ran `--test heap_cross_thread` and
  `--test regression_heap_xthread_large_free_no_leak` — **both test files were
  deleted in task #204** (no faithful `HeapCore` substitute existed; see the
  Heap-removal section above). A drift the removal task's own CI runs hadn't
  caught until this session's push.
- **`e1ff1e9` — added RUSTSEC-2026-0204 (crossbeam-epoch) to `deny.toml`'s
  ignore list.** A **new** advisory, unrelated to any change in this session —
  discovered via the `cargo-deny` CI job (SEC-5) failing on push (Actions run
  28848487484). `crossbeam-epoch` 0.9.18's `fmt::Display` impl dereferences a
  raw pointer that can be a null `Shared`/`Atomic` sentinel (fixed upstream in
  ≥0.9.20). Unlike the two existing dev-only ignore entries (bincode,
  proc-macro-error2, both via `iai-callgrind`), this one is **NOT** purely a
  dev-dependency chain: `cargo tree -i crossbeam-epoch` shows both the
  dev-only `criterion → rayon → crossbeam-deque → crossbeam-epoch` path AND a
  direct optional dependency via `Cargo.toml`'s `experimental` feature
  (`dep:crossbeam-epoch`, backing `src/concurrent/hand.rs`'s epoch-reclaimed
  slot). Verified this crate's own code does **not** trigger the vulnerable
  path: grepped `src/` for any `fmt::Display`/`format!`/`{}`-style formatting
  of a `crossbeam_epoch::Shared`/`Atomic` value — none exists; `hand.rs` only
  dereferences these via `.as_ref()`/pointer-load APIs, not the affected
  Display path. The ignore is therefore **sound for current usage**, but
  flagged in the `deny.toml` comment that a future addition of Display/format
  logging on an epoch pointer would silently reintroduce the bug under this
  ignore — re-grep before trusting the note stays valid. A
  `cargo update -p crossbeam-epoch` bump (≥0.9.20) is the proper fix,
  deferred as a dependency-version decision per project convention.

### Round 6 tail — cleanup, module splits, OPT-P0 perf batch (R6-CQ-5..7, R6-OPT-A1..A6, R6-OPT-P0-1..4, R6-REGRESSION)

The tail of round 6 — 21 commits (`e73dbec`..`461fe8f`), 2026-07-14..16 —
applies the same judge-driven methodology as the PERF-2 / PERF-3 arcs above
to three new *axes* the existing benches did not measure: **OS commit
charge**, **cross-thread-free tail latency**, and **the SMALL_MAX
fragmentation cliff**. Each candidate source change was preceded by a
dedicated diagnostic judge (the A-series harnesses), and every change was
measured against the deterministic `npm run iai` instruction-count gate so
zero of these wins came at a throughput cost (confirmed by the
cross-version wall-clock report at the end of the round). Two genuine
regressions the P0 work introduced were found and fixed in-flight and are
documented below as such, not spun — this project's honest-reject
convention.

**R6-CQ-5 — remove future-only dead scaffolding (`e73dbec`).** The
abandon/adopt removal left three executable-but-unreachable scaffolds kept
under `#[allow(dead_code)]` "in case the substrate returns": `HeapCore::
reset_stamp_cache`, the full-reset `AllocCore::decommit_empty_segment`
variant (every production caller uses the `_for_release`/`_impl` pair), and
`HeapSlot::new_uninit` (plus the `HeapSlotRemote::new_uninit` it transitively
dead-coded). All three confirmed zero real callers via whole-tree grep and
deleted. The load-bearing finding: `HeapSlot::new_uninit` *deliberately
diverged* from the real bootstrap — it set `next_free = NEXT_FREE_TAIL
(u32::MAX)` while the real bootstrap relies on the OS-zeroed reservation
and lets `push_free_slot` write `next_free` lazily (RAD-1); the scaffold's
own doc called this an "intentional, observationally-equivalent divergence,"
but a future caller trusting it as documentation would get the wrong initial
state. The actual lazy-`next_free` invariant is already preserved in prose at
`bootstrap.rs:39-49`, so nothing was lost. (Investigated and *not* removed:
`HeapOverflow::new_uninit`, kept alive by `new_boxed_for_test`'s real callers.)

**R6-CQ-6 — purge stale abandon/adopt architecture text (`139d4eb`).**
Docs/comments still described the *removed* abandon/adopt lifecycle,
referencing functions that no longer exist (`abandon_segments` et al.). The
real teardown path is whole-slot reuse (`tls_heap.rs`), not abandon/adopt.
Fixed across `Cargo.toml` (description + `alloc-xthread` feature doc), the
field rename `SegmentHeader::next_abandoned` → `deferred_next` (the field is
actually the live `deferred_large` cross-thread-free stack's link, rippled
through 14 source files + the tests that name it), `HeapCore::id`'s doc, and
README/`ARCHITECTURE.md`/`src/global/sefer_alloc.rs`/`src/registry/mod.rs`.
New guard `tests/no_stale_doc_references.rs::
no_stale_abandon_adopt_substrate_references` bans the removed API's exact
identifiers (`try_adopt`, `abandon_segments`, `push_abandoned_segment`,
`pop_abandoned_segment`, `abandoned_segs`, `OWNER_STATE_ABANDONED`) outside
the two files that legitimately name them in past tense — scoped to exact
identifiers, not the bare word-stems, so it doesn't false-positive on the
live `AbandonGuard` name, the `ABANDONED_TAIL` sentinel, or unrelated prose.
Counterfactual-verified by injecting a forbidden token and watching the guard
fail with the exact injected line.

**R6-CQ-7a/b/c — split the three remaining monoliths (`13a1f86`, `49f3a29`,
`fd2c770`).** Continues round 4's already-precedented "split one type's
`impl { .. }` block across thematic sibling files" pattern, applied to the
last three monoliths (`alloc_core.rs`, `heap_core.rs`, `segment_header.rs` —
round4 R4-10 / round5 code_quality_review #7, both only partially remediated
until now). **Pure code movement, zero behaviour change:** `npm run iai`
shows `Instructions: "(No change)"` on all 12 perf-gate benches against the
persisted baseline after each of the three splits; the two-tier
confined-`unsafe` grep count stayed at 46 (moved `unsafe fn`s keep their
`#[allow(unsafe_code)]` + `# Safety` docs, just relocated). The two
highest-risk moves per split (`dealloc`/`realloc` for 7b, `magic_at`/
`BinTable::head` for 7c) were byte-diffed against the pre-move source. New
sibling files: `alloc_core_core_diag.rs` (391 lines, the non-small-subsystem
`dbg_*` cluster); `heap_core_{alloc,free,tcache,ownership}.rs` (heap_core.rs
1987 → 606 lines); `segment_header_{layout,views,meta_fields,gen_table}.rs`
(segment_header.rs 1759 → 1041 lines). A handful of `private fn` →
`pub(super)`/`pub(crate)` widenings were the only mechanical adjustments
needed to compile.

**R6-OPT-A1..A6 — six new diagnostic judges (the "Stage A" harnesses).**
The round's design rule was *measure before you change*: each P0 source
change was preceded by the dedicated harness that would honestly prove
whatever win it claimed, so a result that isn't visible on the right axis is
not claimed. All six are `harness = false` custom timing-loop binaries or
process-per-sample runners (Criterion's `b.iter()` model cannot express
"alloc N, hold live, batch-time the free with percentiles" nor read this
crate's `dbg_*` counters at precise checkpoints) — measurement-only,
no allocator source touched, confined-unsafe count unchanged at 46.

- **R6-OPT-A1 — Windows commit-charge probe (`6d1b7ce`).**
  `examples/first_alloc_process.rs` measured only Working Set (RSS), never
  Windows commit charge — so a real cost was invisible: on Windows `crates/
  vmem` commits the full exact size of the Registry + inline `HeapOverflow`
  in one `VirtualAlloc(MEM_COMMIT)` (~125 MiB predicted), demand-zero and
  therefore absent from RSS until pages are touched. Added `commit_kib()`
  (reads `PagefileUsage` from the *same* `GetProcessMemoryInfo` call already
  made for RSS; the field was already declared, just never surfaced).
  Measured finding: RSS Delta 1 heap = 120 KiB vs Commit Delta 1 heap =
  132,620 KiB (**129.51 MiB**) — a ~129.4 MiB gap completely invisible to
  the pre-existing RSS-only judge, closely matching the review's ~125 MiB
  prediction. This gap is the quantity R6-OPT-P0-2 is meant to shrink.
- **R6-OPT-A2 — persistent-thread fan-in throughput judge (`6fa6776`).**
  `benches/heap_fanin_persistent.rs` spawns producer threads *once* per cell
  and reuses them (the existing `heap_fanin_production.rs` re-spawned/joined
  per iteration, so thread lifecycle dominated). Matrix T × burst ×
  {active,slow,paused,exited} owner state; reports p50/p99/max per-op
  wall-clock + `DBG_RING_*` overflow/retry/exhausted-per-op. A real
  cross-cell state-leak bug (recycling a heap then re-claiming it inherited
  the prior cell's `RemoteFreeRing`/`HeapOverflow` state via whole-slot
  reuse) was found during the orchestrator's zero-trust re-run and fixed;
  a `verify_repeated_cell_consistency()` regression guard is wired into
  `main()` so the class can't silently return.
- **R6-OPT-A3 — SMALL_MAX cliff independent-alloc sweep (`b6bcaa2`).**
  `benches/medium_size_sweep.rs`: there is a sharp architectural cliff at
  258,752 B (`SMALL_MAX`, last small class) vs 262,144 B (one byte over):
  below it, many objects share one 4 MiB segment via the per-segment
  freelist; above it, *every* object gets its own dedicated 4 MiB span +
  one `SegmentTable` slot. No existing bench measured this. Confirmed the
  cliff directly: at n=64, **258,752 B reserves +4 segments (fragmentation
  0.9871) vs 262,144 B +64 segments (fragmentation 0.0625)** — a 16×
  segment-count ratio matching the ~15-usable-blocks-per-4-MiB-segment
  theory. The harness handles real allocator OOM at n=1024 for post-cliff
  sizes (~4 GiB VA exhaustion on this host) cleanly — the OOM-at-scale is
  itself evidence of the cliff's cost.
- **R6-OPT-A4 — deterministic multi-segment directory judge (`3686412`).**
  `benches/segment_directory_sweep.rs`: `find_segment_with_free_impl` is
  O(segments) on a free-list miss, but the only existing judge
  (`multiseg_cold_256k`) builds just 3 segments — deep in the flat region.
  Three prior optimization attempts (X5, T10, R5-R1) measured ~zero
  improvement against that judge not because the scan is secretly O(1), but
  because nobody ever measured it far enough into S. Confirmed the
  flat-then-rising curve: S=1/3/16 in the 36–140 ns range, S=64 climbs to
  652–3,749 ns, S=256 to 18,590–25,668 ns, S=1023 to **92,716–169,875 ns**
  (kill-gate ratio 3742× post/S=3 vs 1.13× S=3/S=1, so divergence is
  concentrated at high S and the existing small-S IAI judge stays neutral).
- **R6-OPT-A5 — dealloc-only unbound-thread judge (`8248cb0`).**
  `examples/dealloc_only_unbound_thread.rs` + `scripts/dealloc-only-bench.mjs`:
  a worker that only ever receives a pointer and frees it (never allocates
  itself) is a common pattern no existing bench measured. Pre-fix the
  commit-charge ratio (treatment free-only / control alloc-then-free) sat at
  **1.00×** at every cell — both bind identically via `current_heap()`
  regardless of which call triggered it, exactly the pre-fix convergence
  this harness exists to make P0-1's win visible against.
- **R6-OPT-A6 — real-installed-allocator paired A/B/B/A runner (`57bf118`).**
  `examples/paired_ab_{sefer,mimalloc,system}.rs` (three binaries each
  genuinely installing its *own* `#[global_allocator]` — `bench:table`'s
  direct-call comparison is honest but not the codegen shape of a real
  production binary where every allocation routes through one
  `#[global_allocator]`) + `scripts/paired-ab-runner.mjs` (`npm run
  paired-ab`). A/B/B/A ordering per block (not A/B/A/B) specifically cancels
  linear host-drift/thermal trends; default N=20 paired blocks (the
  threshold for resolving <20% claims, matching R5-R2's actual N). The
  mandated same-vs-same control (`--arms sefer,sefer`) was independently
  re-verified at N=12 (t=-0.018 vs crit=2.228, sign test 7/12) — cleanly
  "NOT statistically distinguishable from noise," proving the runner doesn't
  manufacture a false signal.

**R6-OPT-P0-1 — dealloc without binding a heap (`09fe56f`).** `SeferAlloc::
dealloc` unconditionally called `current_heap()`, which for a thread whose
TLS is null (a worker that only ever frees foreign pointers) *claimed a full
registry slot* (`HeapCore::new` → reserve/commit a 4 MiB primordial segment)
just to free one foreign pointer. Extracted `HeapCore::dealloc_foreign_slow`'s
heap-instance-independent routing body into `dealloc_foreign_routing(..,
our_head: Option<…>)`; a new `tls_heap::current_for_dealloc()` maps both
null and TORN states to a `ForeignNoBind` variant that never binds, never
touches the fallback lock, and routes via `dealloc_foreign_routing(.., None)`
(any pointer reaching `dealloc` on a bind-less thread is foreign by
construction). Deliberate, documented trade-off: a TORN thread freeing a
pointer that happens to be fallback-owned now pushes onto the fallback's own
ring instead of taking its direct free path — still correct (ring-push is
safe regardless of caller identity; the fallback drains its own ring
lazily), just marginally less optimal in this already-rare corner. Verified
via RED-counterfactual (reverting to old routing fails both new tests
`dealloc_only_no_bind.rs` / `dealloc_only_no_bind_torn.rs` for the right
reason).

**R6-OPT-P0-2 — chunk the Registry + lazy HeapOverflow sidecar (two rounds,
`e4b3e1d` + `8dc6fe8`).** The Registry held `slots: [HeapSlot; 4096]` inline
as ONE giant `reserve_aligned` reservation, paid in full by every process's
first heap claim — ~125 MiB under `production` (inline `HeapCore` magazine
+ decommit state + inline `HeapOverflow`), committed in one `VirtualAlloc`
call with no OS-level commit-only-touched-pages for a reservation of this
shape. **Round 1 (`e4b3e1d`):** split the slot array into 64 chunks of 64
slots (`RegistryChunk`, new `src/registry/registry_chunk.rs`), materialised
lazily per chunk via the same `CAS(null→SENTINEL)→reserve→publish(Release)
/spin(Acquire)` protocol the old whole-registry ensure used. Commit Delta 1
heap: **~129.5 MiB → ~5.98 MiB (~21.7×)**. **Round 2 (`8dc6fe8`):** the
remaining dominant cost was `HeapOverflow` — a `[AtomicUsize; 2048] +
[AtomicU32; 2048]` pair inline in *every* `HeapSlot` (24 KiB/slot), paid by
every claimed slot whether or not it ever overflows. Split into a small
always-inline "emergency" tier (`INLINE_CAP = 64` entries, 768 B/slot) plus
a lazily-materialised sidecar (`HeapOverflowSidecar`, M5-clean reserve in
the existing `os.rs` seam mirroring round 1's chunk pattern) covering the
remaining 1984 entries, null until first genuine overflow past the inline
tier. Commit Delta 1 heap: ~5.98 MiB → **~4.52 MiB**; combined round 1 + 2:
**~129.5 MiB → ~4.52 MiB (~28.6×)**. The wedge hazard (a producer winning
the tail-CAS for a sidecar index ≥ `INLINE_CAP` and *then* discovering OOM
would strand that index forever) is fixed by calling
`ensure_overflow_sidecar` *before* the tail CAS — on failure, push returns
false without advancing tail, identical externally to "ring full," which
every caller already treats as the documented-sound bounded leak.

**R6-OPT-P0-3a — exact medium size classes, feature-gated (`b98f082`).** Six
exact "medium" size classes (256 / 320 / 384 / 512 / 768 KiB, 1 MiB) added
to `SIZE_CLASS_TABLE` behind a new purely-opt-in **`medium-classes`** feature
(additive over `alloc-core`, **NOT** part of `production` or any default-on
bundle). Eliminates the ~16× segment-count/fragmentation cliff at the old
`SMALL_MAX` boundary for allocations in this range — they now share a 4 MiB
segment with ~4–15 same-class siblings instead of each claiming a dedicated
Large segment. Reuses the existing small-segment substrate verbatim (one
segment, one size class, `BinTable`/`PageMap`/bump-carve) — no new segment
kind, no page-run layer. This is the "cheap first experiment" (radical-optimization
review SS4 sub-task 3a); the larger page-run redesign (3b) is deferred pending
evidence this substrate reuse isn't sufficient. The R6-OPT-A3 judge confirms
the fix at the exact predicted boundary: **16.00× segments/reserved-bytes at
n=64 (258,752 B vs 262,144 B) collapses to 1.00×** with the feature on. Ten
pre-existing regression tests that hardcoded byte sizes (usually 512 KiB)
that silently flipped from "Large" to "Small" under `medium-classes` were
bumped to sizes that stay genuinely Large in every feature combination;
`SIZE2CLASS` went `const → static` (`large_const_arrays` lint once the table
grew ~16 → ~64 KiB), a storage-class fix not a behaviour change.

**R6-OPT-P0-4 — overflow-first remote-free retry inversion (`345fa9b`).**
Inverted the cross-thread-free fallback order in `HeapCore::
push_with_overflow_retry`: try the heap-level `HeapOverflow` second-chance
ring *immediately* on a full segment ring, before any spinning, instead of
exhausting the whole `RING_PUSH_RETRY_SPINS` (8,192) budget against the ring
first. Each failed counted push ticked two locked-RMW diagnostic counters,
so the old policy could burn up to 8,193 checks / 16,386 RMWs on a single
logical free before ever trying the already-provisioned overflow ring (8×
the capacity). New policy: (1) one counted `RemoteFreeRing::push`; (2) on
failure, an immediate `push_to_heap_overflow`; (3) only if *both* fail (rare
double-saturation), a bounded spin-retry against both tiers using new
*uncounted* variants so failed polls inside the loop no longer tax either
ring's diagnostic counters. Common case: **2 checks total instead of up to
8,193**. On the R6-OPT-A2 judge (T=32/63, saturated ring), p99 tail latency
dropped from **tens of ms to hundreds-to-low-thousands of ns (~10,000×)**,
overflow/op near zero. This commit is also the *source* of the two
regressions below — the retry-loop reshape it introduced had a pathological
busy-spin budget that the A-series judges (which own a draining owner) did
not exercise.

**R6-REGRESSION — bound `push_with_overflow_retry`'s wall-clock cost
(`ba34fd5`).** P0-4's bounded retry loop scaled its iteration budget from
`RING_PUSH_RETRY_SPINS` (8,192) to `RETRY_LOOP_ITERATIONS` (2,097,152 =
8,192 × 256) as a *flat, uninterrupted* `core::hint::spin_loop()` busy-spin.
Under sustained double-saturation (both the segment ring and the heap-level
overflow full) with a live-but-never-draining owner (the `owner=paused`
shape A2 models), nearly every push burned most/all of its 2,097,152-
iteration budget purely on CPU — `spin_loop()` is a CPU-level hint (e.g.
`PAUSE`), never an OS-level yield, so it gave the scheduler no chance to run
the stalled owner. Confirmed independently: A2's `--reduced`
T=32/burst=100,000/`owner=paused` cell **burned 4,420 CPU-seconds over ~4
minutes with zero output before being killed**. A first fix attempt (same
total budget reshaped into 8,192-iteration rounds with `yield_now()`
between rounds) did *not* resolve it — `yield_now()` is a scheduling hint
with no other runnable work to hand the CPU to when every contending thread
is itself spin-then-yield-looping (~9 CPU-seconds/wall-second at 32 threads/
16 cores). **Fix adopted:** cap to `RETRY_ROUND_MAX_ROUNDS = 8` rounds of
8,192 tight-spin polls each with a real `std::thread::sleep(200 µs)` OS-level
block between rounds (native only; miri keeps a single pure-spin round).
Round 1 stays a pure tight spin with no sleep before it, so the
moderately-contended actively-draining-owner workload task #136's
high-contention judge exercises resolves within round 1 and pays no new
latency. Only a push that survives 8 full rounds (a push that can genuinely
never succeed once the fixed 2,304 combined ring+overflow capacity is
exhausted with a permanently-stalled owner) concedes to the documented
bounded leak after ~1.4 ms of sleep instead of a multi-second CPU burn. New
`tests/regression_paused_owner_wallclock.rs`; RED-counterfactual (pre-fix
source) lands all 3 attempts at ~20–21 s, GREEN with the fix at 0.7–1.9 s.

**R6-REGRESSION-2 — progress-detected stop condition (`1da4497`).**
R6-REGRESSION fixed the CPU-burn near-livelock but, by cutting the retry
budget to a fixed 8 rounds, *reintroduced* the task #136 throughput
regression under host load — the #136 judge went flaky, `exhausted_delta` up
to 821 during load spikes. Root tension: a *paused* owner (never drains)
needs a FAST give-up, while a *live-but-CPU-starved* owner (IS draining,
slowly) needs PATIENCE. No fixed round/iteration budget can distinguish
them — tuning the number only moves the failure between the two judges.
**Fix:** the retry loop's stop condition is now **drain-progress detection,
not a round count.** Both tiers' drain cursors (advanced *only* by the
owner's drain) are snapshotted before round 1 and re-read after every
fully-failed probe round via two new read-only `pub(crate)` accessors
(`RemoteFreeRing::head_relaxed()`, `HeapOverflow::head_relaxed()` — cheap
Relaxed loads of monotonic owner-advanced cursors; no ring protocol, layout,
or Ordering touched; no new `unsafe`). If either cursor moved, the owner
drained something — the stall counter resets and the push keeps waiting.
Only after `RETRY_STALLED_ROUNDS_GIVE_UP = 128` *consecutive* zero-progress
rounds (~0.3–2 s of continuously observed zero drain progress) does it
concede; `RETRY_ROUND_SAFETY_CAP = 4096` total rounds hard-bounds the wait.
The load-bearing 200 µs between-round sleep is kept unchanged. Each
concession memoizes its `(segment base, ring head, overflow head)` snapshot
in a per-thread const-init TLS `Cell` so a sustained stall does not re-pay
the full patience per push; the memo is written *only* on concession, so any
judge run that satisfies `exhausted_delta == 0` never populates it. K
calibration (measured): K=4 → 6/10 judge failures even on an idle host;
K=32 → 10/10 calm but 3/5 under a 16-thread CPU hog; **K=128 → 10/10 calm +
8/8 under the hog, all `exhausted_delta = 0`**. RED-counterfactual #2
(emulated pre-R6-REGRESSION flat 2,097,152-iteration spin) → paused-owner
wallclock test fails all 3 at 15.2–15.7 s; restored → 95–210 ms calm, 7.9 s
under the hog. The `tests/remote_fanin.rs` harness-1/2.5 liveness fix (the
owner loop now runs until every producer has finished via an `Arc<AtomicBool>`
handshake, draining every 4096 allocs) closes the remaining flake — every
prior failing run's concessions occurred strictly *after* the owner's fixed
N×2-alloc loop had completed, i.e. the paused-owner shape where conceding is
the documented-correct outcome.

**R6-REVIEW residuals — N-way stall memo + doc accuracy (`f27d060`).**
Address the non-blocking findings from an independent `@fl` review of the
P0 wave; no behaviour change on any already-green path. **F2 (perf
robustness):** the fast-concede memo was single-entry — a paused owner of
2+ saturated segments with a producer whose frees interleave across them
(A,B,A,B,…) overwrote the memo every push with the other segment's tuple,
so the memo never matched and every push re-paid the full 128-round patience
(a linear-in-push-count cost the memo exists to bound). Replaced with a
per-thread 4-way cache (`STALL_CONCESSION_WAYS = 4`): const-init, `Copy`,
no allocation; lookup fast-concedes iff *any* slot matches; soundness
unchanged (written only on concession, so a zero-concession run never
populates it; the post-round progress check still resets to full patience
the moment either drain cursor advances). New
`tests/regression_paused_owner_multisegment.rs`: passes in ~0.7 s with the
4-way cache; RED-counterfactual (forced to 1 way) fails all 3 attempts at
**77–105 s** — the exact single-entry thrash F2 fixes. F3/F5/F1/F4 are doc
fixes: `DBG_RING_PUSH_RETRY_EXHAUSTED`'s doc rewritten to the actual control
flow; dead `RETRY_LOOP_ITERATIONS` constant + its references scrubbed;
`ARCHITECTURE.md`'s loom-model count corrected (13 → 16, the 3 missing
entries added); a self-contradicting comment in `registry_chunk.rs` rewritten.

**Cross-version wall-clock report (`461fe8f`).** New
[`docs/perf/R6_CROSS_VERSION_BENCH.md`](docs/perf/R6_CROSS_VERSION_BENCH.md)
+ a README "Cross-version comparison" subsection: a same-harness three-way
comparison of sefer-alloc across published **0.2.1** (tag `sefer-alloc-v0.2.1`),
the tree **immediately before the round-6 wave** (`57bf118`), and current
HEAD (`f27d060`) — full per-family tables with the vs-mimalloc-ratio
methodology (host-drift-normalised) and the 0.2.1 not-apples-to-apples
caveats. **Headline:** every *large* wall-clock win landed between 0.2.1 and
the pre-round-6 tree (OPT-G in-place realloc → ms-scale copy-and-free to
µs-scale; Э6 churn → 256 B/1024 B throughput), **NOT** in the round-6 wave
itself. **The round-6 wave (before-wave → now) is flat-to-slightly-better on
wall-clock throughput and regresses no family beyond host noise**, by design:
it targeted **OS commit charge (≈7.4× lower for the first heap — 33.3 MiB →
4.5 MiB on the `bench:table` harness)**, **cross-thread-free tail latency**,
and **the SMALL_MAX fragmentation cliff** — axes `bench:table` does not
measure (see the A-series judges above). Probable modest wins on the 4 MiB
large-alloc/free path (~30–35% faster, 78/85 ns → 53/56 ns) and the 1024 B
teardown/decommit diagnostic, both inside this host's noise band. The 0.2.1
column was produced by porting the current bench harness onto the release
tag, preserved as the local `bench/0.2.1` branch so 0.2.1 stays
re-measurable. (Note on the commit-charge figure: the A1/P0-2
`first_alloc_process.rs` probe measures a stricter "genuinely fresh single
process" baseline and reports the larger **~129.5 MiB → ~4.52 MiB (~28.6×)**
reduction above; the cross-version doc's 33.3 → 4.5 MiB figure is the same
axis measured in the `bench:table` harness context.)

### Round 7 — segment directory, lazy commit, crate extraction (r7-a0..a6, r7-b0..b6, crate-extraction P1-P10)

Round 7 — 54 commits (`c0c011f`..`c815927`), 2026-07-16..19 — three
workstreams run under the same judge-driven methodology as the Round 6 tail
above (a dedicated diagnostic harness precedes every source change; the
deterministic `npm run iai` instruction-count gate is the authoritative
judge; honest-reject is mandatory), plus a crate-extraction campaign that
grew the workspace from 4 to 11 companion crates, plus a deep-audit-driven
hardening batch. The headline shape mirrors Round 6's: one big **GO**
(Workstream A, the segment directory), two **documented NO-GOs** preserved
as institutional memory (the Workstream-B first-heap commit target as
originally built, and the `ring-mpsc` in-tree swap), and one headline that
was a NO-GO on first attempt but **salvaged later in the same round by a
different mechanism** (R7-B6 lazy-commits the primordial segment). Every
number below is from the cited perf report or commit message; nothing is
inferred.

**Workstream A — segment directory, r7-a0..a6 — verdict GO (`f7d3a1c`..`0eb4794`).**
Replaces the O(segments) linear scan in `find_segment_with_free_impl` (the
refill-miss path Round 6's R6-OPT-A4 judge had proved blows up to ~100 µs at
S=1023) with an owner-only per-class bitmap sidecar, materialised lazily at
≥32 segments. Built incrementally behind the new experimental **`alloc-segment-directory`**
feature (additive over `alloc-core`, **NOT** in `production`, off by default;
feature-OFF byte-identical at every phase):

- **r7-a0 (`f7d3a1c`) — baseline + observability.** Six process-wide
  `AtomicU64` counters (`directory_hits`, `directory_stale_hits`,
  `directory_fallback_scans`, `directory_words_examined`,
  `dirty_segments_drained`, `full_scan_slots_examined`) +
  `benches/directory_threshold_probe.rs` (the S=32..63 transition-zone probe).
  Baseline confirmed (class 48, holes=0%): S=16 ~219 ns, S=32 ~442 ns,
  S=64 ~1.1 µs, S=256 ~17 µs, S=1023 ~102 µs — the O(S) cliff, with
  per-slot cost ~14 ns at S≤63 (cache-hot) rising to ~100 ns at S=1023. The
  S=32 transition-zone data is what fixed the **materialisation threshold at
  32** (the scan is already ~442 ns / p99 ~1 µs there; a ~100 ns directory
  lookup is a clear net win from there up).
- **r7-a1 (`5b5532c`) — the sidecar.** `SegmentDirectory { class_nonempty:
  [[u64; MAX_SEGMENTS/64]; SMALL_CLASS_COUNT] }` — plain u64 words (owner-only
  single-writer), 6.1 KiB (49 classes) / 6.9 KiB (55 under `medium-classes`),
  reserved lazily via a new M5-clean `os.rs` membrane
  (`reserve_directory_sidecar` + deref helpers in the existing tier-1 seam),
  `None` on OOM (mechanism stays off, linear scan runs). Nothing queries it
  yet.
- **r7-a2 (`b2eb7a3`) — incremental bitmap maintenance.** Wires
  `publish_nonempty` / `publish_empty` / `clear_segment_directory` /
  `sync_directory_for_segment` into every BinTable-head-mutating path (pop,
  drain, dealloc, flush, recycle, pool/unpool) so the bitmap is exact by the
  time A3 queries it. Correctness oracle: a randomised 300/500-op workload
  asserts the incrementally-maintained bitmap EXACTLY equals a fresh
  `rebuild_from_table` at periodic checkpoints.
- **r7-a3 (`66d0ac3`) — directory-accelerated lookup (fallback retained).**
  A directory-hit path in front of the unchanged guarded linear scan. Every
  load-bearing side effect of the scan (the Variant-2 remote-ring drain, the
  pool/decommit hysteresis, `unpool_if_present`, the ring-head cache refresh)
  is preserved byte-for-byte; a directory miss falls through to the scan. The
  directory is an **accelerator, not yet authoritative** — the scan stays as
  the correctness oracle and OOM-degradation path. Deliberately disabled under
  `numa-aware` (the two-pass local/foreign preference would be silently
  dropped); the bitmap is still maintained there for a future node-aware query.
- **r7-a4 (`7cc3ccf`) — remote dirty routing.** Replaces "drain every
  candidate's ring" with a per-slot dirty bitmap (`[AtomicU64; 16]`, 128 B in
  `HeapSlotRemote`): a cross-thread producer `fetch_or`s a bit Release AFTER
  a successful publish; the owner `swap(0, Acquire)s` and drains only dirty
  segments. Lost-wakeup-safe (bit set after the ring Release; a producer
  arriving mid-drain re-sets it; slot reuse revalidated). The full linear scan
  (the fallback) still drains every ring unconditionally, so an un-bit-set
  publish is never a lost free, only a bounded deferral. No new `unsafe`.
- **r7-a5 (`6eb425a`) — correctness matrix + heavy tools.** A 64-case proptest
  (per CLAUDE.md) asserting incremental bitmap == fresh rebuild for every
  (class, slot); gap-fill deterministic tests (recycle+reuse different class,
  decommit/reset/recommit, 55-class medium rebuild); 3 loom models of the
  dirty bitmap; a strict-provenance miri target. loom + miri RUN on this host
  (loom 3/3 + 3/3, miri 8.3 s PASS); TSan/ASan are Linux-CI-only (deferred,
  noted honestly). **No correctness bug found in A1–A4 production code.**
- **r7-a6 (`0eb4794`) — GO/NO-GO verdict: GO.** Against the pre-registered
  gates (full table in
  [`docs/perf/R7_DIRECTORY_GONOGO.md`](docs/perf/R7_DIRECTORY_GONOGO.md)):
  refill-miss at holes=0% collapsed from **S=256 ~15–19 µs → ~170–244 ns
  (60–98×)** and **S=1023 ~92–95 µs → ~376–552 ns (166–254×)** on both mean and
  p99 — far past the 10× gate; remote dirty=0% **S=1023 103 µs → 800 ns
  (129×)**; ≤16 directory words examined per lookup by construction; S≤16
  identical code (not materialised below the threshold); memory 6.1 KiB sidecar
  + 128 B/slot dirty control. The one **CI-DEFERRED** gate is G8 (IAI
  instruction-count churn ≤1%, Valgrind is Linux-only); the wall-clock churn
  proxy showed no regression (largest adverse +11.6%, within the host's
  ±15–20% noise). Documented trade-off (not a gate failure — the gate measures
  dirty=0%): at high remote-dirty density (10–100%) the drain-first path costs
  more than the linear scan's lazy drain, though absolute times stay low
  (1–3 µs). The directory stays behind its opt-in feature — enabling by
  default and making the fallback non-authoritative are separate downstream
  decisions.

**Workstream B — incremental / lazy Windows commit, r7-b0..b5 — verdict NO-GO
on the primary criterion (`e5310a0`..`40fdcd3`).** A new experimental feature
**`alloc-lazy-commit`** (additive over `alloc-core`, **NOT** in `production`,
off by default; on Unix/miri it falls back to eager; `numa-aware` forces eager)
to reserve a new small segment's 4 MiB span `MEM_RESERVE`-only and commit just
`[0, meta_end + LAZY_FIRST_CHUNK)` up front, growing the commit frontier
incrementally as carves advance. Built in the same incremental phase style:

- **r7-b0 (`e5310a0`)** — vmem primitives only: `reserve_aligned_lazy(size,
  align, initial_commit)` and `commit_range(base, start, end) -> bool`
  (returns false on OOM, never panics), all in the designated `crates/vmem`
  `#![allow(unsafe_code)]` seam.
- **r7-b1 (`0c981d7`)** — the `committed_payload_end` frontier on
  `SegmentHeader` + the lazy `reserve_small_segment` arm; a temporary
  "commit-whole-remaining-payload" safety net keeps B1 non-faulting until B2.
  Deliberately keeps the **primordial** segment eager (it hosts the
  self-hosted registry accessed at arbitrary offsets during bootstrap).
- **r7-b2 (`e5cb929`)** — fallible incremental grow-on-carve: on a carve past
  the frontier, commit `[frontier, round_up(carve_end, GROW_CHUNK))` BEFORE
  advancing bump/handing out the pointer; `carve_batch` does ONE commit for the
  whole batch span (not per block); failure leaves everything unchanged. The
  eager path is a pure no-op (`frontier == SEGMENT`).
- **r7-b3 (`2c3dbea`)** — lazy-commit-aware decommit/recommit: pool-admission
  decommits only above the initial chunk and resets to a clean carve target;
  retain-decommit keeps the initial chunk committed so reuse is fault-free;
  reuse drops the full-payload recommit. Savings preserved across a segment's
  second life.
- **r7-b4 (`f5f84ac`)** — correctness matrix + the `dbg_arm_commit_fail_at(k)`
  fault-injection hook (fails exactly the k-th commit, 1-based, one-shot,
  self-disarming): 21 tests proving commit failure is fully recoverable
  (frontier/state unchanged after an injected failure, retry succeeds).
- **r7-b5 (`40fdcd3`) — GO/NO-GO verdict: NO-GO on the primary criterion (K1),
  honestly.** Full table in
  [`docs/perf/R7_INCREMENTAL_COMMIT.md`](docs/perf/R7_INCREMENTAL_COMMIT.md).
  The headline target — first-heap Windows commit **4.52 MiB → ≤0.9 MiB** — is
  **unreachable by `alloc-lazy-commit` as built**: the first-heap commit charge
  is entirely dominated by the primordial segment (4 MiB eager), and the very
  first `alloc` triggers `registry::ensure()` which materialises it; no
  `reserve_small_segment` runs on the first-heap path. So the lazy path — which
  applies only to *subsequent* small segments — measured **4,628 KiB (4.52 MiB),
  unchanged across all swept chunk sizes** (K1 FAIL). This is a design-plan
  mismatch (the plan's 0.9 MiB budget assumed the primordial would participate),
  reported as such, not a measurement failure. **All secondary criteria PASS:**
  first-alloc latency +6.2% (≤10%), dense cold within noise (≤3%), steady churn
  no measurable regression, commit-syscall count scales per-chunk not per-alloc
  (B2), commit failure fully recoverable (B4's 21 tests), Linux/miri eager
  fallback transparent. Documented trade-off: the cold-path `segment_decommit_cycle`
  bench regresses ~50–75× with the feature ON (incremental `VirtualAlloc` syscalls)
  — opt-in, off by default, does not touch steady state. Chunk size kept at 256
  KiB (all four swept sizes give identical first-heap commit and near-identical
  steady-state; no data-driven reason to change). `alloc-lazy-commit` stays
  opt-in/experimental; the stated future work to actually hit 0.9 MiB was
  "lazy-commit the primordial + the already-chunked registry." **R7-B6 did the
  first of those — see below.**

**R7-B6 — lazy-commit the primordial segment (the deferred salvage),
`8977e88`.** A separate, later commit that revisited Workstream B's headline
NO-GO and landed the win via a **different mechanism** — it does not retract
the B5 verdict, it closes the gap B5 identified. The SAFE "Option A"
(pre-computed footprint): `bootstrap::primordial()` now reserves the 4 MiB VA
but commits only `[0, primordial_meta_end() + LAZY_FIRST_CHUNK)` up front,
where `primordial_meta_end()` is the exact page-aligned end of EVERY region
bootstrap writes (header, page map, bin table, gen table/bitmaps, remote ring,
segment registry, hash table, free-list array + top) — so all bootstrap writes
land inside the committed prefix by construction (no per-write commit dance).
Later carves reuse the existing B2 grow-on-carve path. A compile-time assert
that `primordial_meta_end() + LAZY_FIRST_CHUNK <= SEGMENT` makes a future
metadata growth fail the build. **Measured first-heap commit Δ: ~4.52 MiB →
~0.887 MiB (~5.2×), inside the ≤0.9 MiB target.** Gated `alloc-lazy-commit
AND NOT numa-aware`; the eager path (feature off, or numa-aware) is
byte-identical. `production alloc-lazy-commit` boots 395/0 (no panic / fault /
access-violation — bootstrap does not fault under the feature); feature-off
356/0 (eager path byte-identical). To avoid any future confusion:
`docs/perf/R7_INCREMENTAL_COMMIT.md` carries a top banner documenting the B6
reversal and inline "superseded by R7-B6" annotations at the B5-era stale
claims, and `c815927` later swept the same annotations through the
cross-version doc — so the historical B5 numbers stay accurate for what B5
measured while never reading as present-tense fact.

**r7-a7 / final-run fixes — `42f8343`, `a834fca`, `49046ef`.** Three
"final-run" fixes (#170) landed as the workstreams closed. **`42f8343`
(r7-a7)** clears the segment-directory bits on the B3 lazy-commit
pool-admission path — B3 zeroed all BinTable heads on pool admission but did
NOT clear the directory, so `publish_nonempty` bits survived as stale
positives and desynced the incremental bitmap from a fresh rebuild (manifested
under `--all-features` as a `directory mismatch at class=54`); the
counterfactual reproduces the mismatch. **`a834fca` (test-only)** gates the
B1–B4 lazy-commit tests off the `numa-aware` eager fallback so they don't hit
the Windows-lazy branch under `--all-features` (where numa-aware is on and the
lazy path is deliberately eager). **`49046ef`** comma-joins the feature list in
`scripts/miri.mjs` so multi-feature entries survive Windows shell re-splitting
— the old space-separated value made 3+-feature entries hard-error and
2-feature entries degrade to a `0 passed` **vacuous green**
(`decommit_miri_cycle`, `regression_ring_drain_guard_miri` were silently
validating nothing under strict-provenance miri).

**Re-sweep r7-c1 — `TCACHE_CAP` {32, 64}, third rejection — `cf22c96`.** The
post-RAD-5 re-sweep the R7 plan mandated: RAD-5 (`MagazineBitmap`) removed the
O(count) in-magazine M2 duplicate scan that was the old rationale for why
larger caps were expensive — so the hypothesis was that the cost model had
changed enough to make a larger `TCACHE_CAP` viable. **Verdict: NO-GO for both
32 and 64; `TCACHE_CAP` stays at 16** — this is the **third** time this
parameter has been tested and rejected (X4-A 2026-07-05 → PERF-2 `e6f5112`
2026-07-07 → r7-c1, see PERF-2 above). RAD-5 did remove the scan cost, but the
deterministic IAI judge (Ir/op via WSL callgrind, the authoritative judge)
confirmed the dominant costs remain: churn Ir/op **+13.2 % (CAP=32) / +38.8 %
(CAP=64)** — hard-fails the ≤2 % churn gate — and first-heap commit **+8.8 %
(+408 KiB) / +26.4 % (+1.22 MiB)**, enlarging each of the 64 first-chunk slots
and eating the R6 first-heap-commit win (the plan's explicit NO-GO-even-if-
wall-clock-improves guard). `PerClass` grows 136 → 264 B at CAP=32, bootstrap
zero-init Ir +89 %, cache footprint ~2×. Cold-direct DID improve (−6.5 % /
−12.5 % Ir/op) but cannot outweigh churn + commit. The noisy wall-clock showed
a spurious ~40 % churn improvement at CAP=32 that the deterministic IAI
contradicts (+13 %) — documented as host noise, not trusted. Zero production
code changed (CAP swept then restored; `git diff src/` empty). Full tables in
[`docs/perf/R7_TCACHE_SWEEP.md`](docs/perf/R7_TCACHE_SWEEP.md).

**Re-sweep r7-c2 — small-segment pool-cap sweep → documented presets, default
unchanged — `ad443d9`.** Sweep of `pool_segments` {0, 1, 4, 8, 16}
(`pool_byte_cap` scaled to match) on the production feature set. The judge is
the deterministic decommit-call count (wall-clock is host-noisy; IAI N/A — the
pool cap is a runtime knob, not a compile-time instruction change). The default
**stays at `pool_segments=4` / `pool_byte_cap=16 MiB`** — it already eliminates
working-set-oscillation decommit churn at the most common small sizes (16 B/64
B: zero decommit calls); raising it costs 2–4× retained RSS for diminishing,
within-noise latency returns. The deliverable is **three documented tuning
presets** (recipes over the existing `SmallSegmentPoolConfig` API, not new
constructors): **low-rss** (`pool_segments(0)`/`pool_byte_cap(0)` — 0 MiB
retained, max decommit churn; containers/serverless/embedded), **balanced**
(the current default; kills 16 B/64 B oscillation churn), and **throughput**
(`pool_segments(16)`/`pool_byte_cap(64 MiB)` — kills churn up to 256 B, halves
1024 B churn; RAM-rich hosts with oscillating working sets). OOM-drain
correctness confirmed: the pool remains a reclaimable soft reserve at every
cap (the unbounded-recycle + 10 pool tests stay green). Zero production change.
Full tables in
[`docs/perf/R7_POOL_CAP_PRESETS.md`](docs/perf/R7_POOL_CAP_PRESETS.md).

**docs(r7) — benchmark results + cross-version report — `5511af0`, `b8d11f4`.**
**`5511af0`** lands
[`docs/perf/R7_BENCH_RESULTS.md`](docs/perf/R7_BENCH_RESULTS.md): the
directory win as a clean OFF-vs-ON table (refill-miss collapses O(S)→~O(1),
up to ~166–180× at S=1023, ~29–39× at S=256, parity at S≤3), plus the canonical
`npm run bench:table` 3-arm comparison (SeferAlloc vs mimalloc vs System) —
steady-state churn is SeferAlloc's strength (**1.08–10.15× faster than
mimalloc**, the advantage growing with block size — 10× at 1024 B; 5–8× faster
than System across the board); cold-direct at small sizes is the weak spot
(2–2.7× slower than mimalloc at 16–64 B, crossing over to faster at 1024 B);
`segment_decommit_cycle` 4.13× faster than mimalloc; `Vec_push` 1.36× faster;
teardown diagnostic intentionally slower. **`b8d11f4`** lands
[`docs/perf/R7_CROSS_VERSION_BENCH.md`](docs/perf/R7_CROSS_VERSION_BENCH.md)
+ a README "Cross-version comparison — 0.2.1 → 0.3.0 (post-round7)"
subsection: same-harness run of published **0.2.1** vs current 0.3.0
(`49046ef`). Headline (0.3.0 over 0.2.1): churn **+1.0–2.3×**, churn+write up
to **2.26×**, `segment_decommit_cycle` **~318×**, `working_set_cycle` up to
**4.03×**; no real regression (cold-direct/teardown deltas within ±15–20 %
host noise). Documents the two root-cause overhauls between 0.2.1 and 0.3.0:
the ~318× decommit-cycle win (Mechanism-2 small-segment hysteresis pool +
OPT-E large cache) and the ~128 MiB → ~6 MiB (~21.7×) Windows first-alloc
commit-charge cut (the R6-OPT-P0-2 chunked Registry). *(Note for future
readers: this `b8d11f4` report is the Round-7-era cross-version doc — distinct
from, and later superseded by, the more complete `docs/perf/R8_CROSS_VERSION_BENCH.md`
from a subsequent round.)*

**Crate-extraction campaign, P1–P10 (`99e3238`..`0ff8497`).** A focused
campaign extracting independently-testable crates out of the monolith — 7 new
workspace member crates + the `aligned-vmem 0.2` release + `malloc-bench-rs`
publish-prep, taking the workspace from 4 to 11 companion crates. Each new
crate is a single-file seam crate, `#![forbid(unsafe_code)]` or a single
documented `#![allow(unsafe_code)]` reason, with a real-type loom suite where
concurrency is involved (and `#[should_panic]` counterfactuals proving the
harness is non-vacuous).

- **P1 — `malloc-bench-rs` (`99e3238`).** `run_with`/`sweep_with` with an
  `on_thread_start(thread_index)` hook (fires per worker before the start
  barrier) so a caller can pin worker i to core i; `examples/malloc_macro.rs`
  re-plumbed as a thin driver over the crate, retiring its second copy of the
  larson/mstress workload (task-#28 drift liability). Publish-prep only
  (`--dry-run` clean; no version bump, no publish).
- **P2 — `aligned-vmem 0.2` (`4ec1516`).** One coherent 0.2 release (the
  version bump 0.1→0.2 was explicitly approved): real `page_size()` via
  `sysconf`/`GetSystemInfo` (correctness fix for macOS 16 KiB pages); fallible
  `try_*` API returning `Result<_, VmemError>`; `decommit_lazy` (Linux/macOS
  `MADV_FREE`); optional `huge-pages`; a `mock` feature (recording call log +
  fail-N-th fault injection); and `leak_zeroed_pages` folding the
  3×-repeated leaked-zeroed-sidecar pattern (registry_chunk, heap_overflow
  sidecar, directory sidecar) into one helper. Absorbing sefer's
  `COMMIT_FAIL_*` into the mock was deferred (sefer builds vmem without `mock`
  — see #186 below).
- **P3 — `racy-ptr-cell` (`63991cc`).** The
  `UNINIT(null) → INITIALIZING(sentinel) → READY(*mut T)` lazy CAS-published
  pointer cell, unifying 4 in-tree loom shadow models
  (`loom_bootstrap_cas`, `loom_chunk_cas`, `loom_fallback_init`,
  `loom_overflow_sidecar_cas` — deleted) onto ONE real-type suite. The crate
  aliases its atomics to loom under `--cfg loom`; ships the two non-vacuousness
  counterfactuals (Relaxed-publish causality violation; spin-on-READY
  livelock). Registry chunk cells swapped onto it (M5-critical: OOM-rollback /
  re-race / Release-publish preserved); a `cfg(loom)` shim keeps the const
  `REGISTRY` static compiling under the global `--cfg loom`.
- **P5 — `size-classes` (`121d657`).** The const size-class scheme extracted;
  `src/alloc_core/size_classes.rs` becomes a thin compat shim (numa.rs-over-
  numa-shim precedent) building sefer's one concrete instantiation. New
  const-generic `SizeClasses::build(Params{...})` so arbitrary parameterizations
  become property-testable; `HUGE_THRESHOLD` becomes a policy `Param`. Fixes
  the "every align≥512 silently falls to whole-segment" bug class via a provably-
  equivalent jump slow path.
- **P6 — `globalalloc-model` (`b420d39`).** The differential op-stream + M1–M4
  oracle harness, unified out of THREE drifted in-tree copies
  (`tests/alloc_core_differential.rs`, `tests/heap_differential.rs`,
  `fuzz/fuzz_targets/global_alloc_ops.rs` — now thin consumers each keeping
  only an adapter + its historical size Config + entry point). All 14 oracle
  assert sites now live only in the crate (net −399 lines). Two front-ends
  (proptest `Strategy`, `Arbitrary`) over ONE model power cargo test, the miri
  bounded run, and libFuzzer.
- **P7 — `tagged-index-stack` (`0ecfaa4`).** The ABA-tagged Treiber free-index
  stack that lived across `tagged_ptr.rs` + `heap_registry.rs` — extracted and
  `heap_registry` swapped onto it (xthread-critical, landed cleanly, no escape
  hatch). Preserves the two hard-won subtleties (H-2 drain-to-empty packs the
  RUNNING tag, never tag 0; RAD-1 `store_next` is the only link write and only
  during push). **`src/registry/tagged_ptr.rs` removed entirely**; the 680-line
  `tests/loom_free_slots_aba.rs` shadow model **deleted**, replaced by the
  crate's real-type loom suite which ships TWO `#[should_panic]` counterfactuals
  (untagged-head slot corruption; H-2 tag-reset stale-CAS) — both confirmed to
  panic, proving both the ABA tag and the H-2 fix load-bearing.
- **P4 — `ring-mpsc` (`4c20f0c`).** The Vyukov bounded-MPSC index-ring protocol
  (raw + owned tiers, drain-stop contract, `DirtyRouter`) captured additively
  with an 11-test real-type loom suite (7 properties + 4 `#[should_panic]`
  counterfactuals). **The in-tree swap of `RemoteFreeRing`/`HeapOverflow` onto
  the crate was NOT done** (sanctioned escape hatch) — and the later
  CRATE-P4-followup re-investigation confirmed that swap is a real NO-GO (see
  below).
- **P8 — `proc-memstat` (`4075490`).** `proc_memstat::snapshot() -> MemStat
  {rss, commit, peak_rss}` — one same-instant query so rss/commit are coherent.
  Refolds 6 copies of the `GetProcessMemoryInfo` FFI across 5 example files
  into one reader. (A later follow-up, `583cd8f`, fixed a hardcoded 4 KiB Linux
  page-size bug here — see hardening batch.)
- **P9 — `proc-probe` (`c3c2440`).** The RESULT-protocol emit lib
  (`emit`/`emit_u64`/`emit_i64`/`emit_f64`/`emit_ns` → `"RESULT key=value"`
  stdout) + the config-driven A/B/B/A paired runner. The 3 probe examples now
  emit via `proc_probe::emit_*` and read via `proc_probe::snapshot()`; the
  statistical core (paired t-test, sign test, the A/B/B/A block loop,
  same-vs-same control) is UNCHANGED.
- **P10 — deferred/skipped verdict (`0ff8497`).** Read-only file-or-drop
  research re-evaluating every candidate the first pass did NOT file, now that
  P1–P9 shipped. **Net: 0 file as crates.** `carved-mem` DROP (the `'static`
  atomic-view lifetime is load-bearing for `#![forbid(unsafe_code)]`; a general
  crate would ripple every `// SAFETY` into a generic caller obligation);
  `intrusive-once-stack` DROP (ring-mpsc P4 already banked the MPSC value; the
  unique idempotent-double-push guard is welded to raw-address-in-`AtomicU64` +
  lifecycle-link tricks that extraction loses); `iai-judge` + `criterion-arms`
  DROP as crates (their one worthwhile in-place win folds into proposed hygiene
  H2 — a bench-emitted MANIFEST). All 3 skip groups (gen-slot retired;
  tcache-magazine trivial; the bitmap/table/directory/large-cache/xthread-SM
  cluster as internal ABI or ~80 % convention) confirmed. Proposes 4 in-place
  hygiene sub-tasks (H1 single-source sanitizer matrix > H2 bench-emitted
  MANIFEST > H3 dead-`dbg_*`-hook detection > H4 fold `rss_probe.rs` onto
  proc-memstat), not filed.

**CRATE-P4-followup (#187) — `ring-mpsc` in-tree swap = verified NO-GO —
`d062798`.** The sanctioned P4 escape hatch was re-investigated (not merely
inherited) against source, and the swap of the two shipping cross-thread-free
rings onto `crates/ring-mpsc` is **NO-GO on BOTH tiers** — zero code changed.
Full rationale in
[`docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md`](docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md).
**Tier A (`remote_free_ring.rs`):** structural layout incompatibility — the
shipping ring uses `AtomicU32` cursors + an `overflow` side word +
`CURSOR_BLOCK = 128` (the PERF-PASS-4 / #52 cache-line-separation fix:
`head`@0 consumer-only, `tail`@64 producer) + a hardened
`[gen|class|off]` generation-stamped entry; `ring-mpsc`'s `RawStore` is a
fixed `usize`-cursor, no-side-word, adjacent layout. Swapping would break the
cache-line fix and every compile-time offset assert (wired through
`small_meta_end()` into 20+ call sites), or require a large risky `RawStore`
generalization. **Tier B (`heap_overflow.rs`):** the two-tier inline+sidecar
store straddles an inline array AND a lazily-mmap'd sidecar in one cursor pair
(`ring-mpsc` is single-region), AND the wedge-hazard sidecar-before-tail-CAS
ordering lives INSIDE `push`'s loop (which `MpscRing::push` owns opaquely) —
forcing it risks the permanent-wedge hazard the module doc warns is worse than
the bounded loss. **The swap is pure dedup (zero runtime benefit) over the most
safety-critical path in the codebase.** Consequence: **all 7 in-tree
ring/dirty loom models are KEPT** (the shipping code is unchanged, so its
coverage must stay — the #174 lesson); the crate's `loom_ring_mpsc` suite is
additive real-type coverage of the extracted protocol only.

**Crate-extraction review + follow-up fixes — `1d39e43`, `9d6c9f4`, `583cd8f`,
`3d25263`, `6ce2df5`, `0ff8497`'s hygiene.** **`1d39e43`** applies the `@fh`
phase-review findings (verdict SHIP-WITH-FIXES): F1 HIGH and **CI-breaking,
reproduced E0015** — under `RUSTFLAGS=--cfg loom` with `alloc-global`,
`Registry::new()` (const fn) called `TaggedIndexStack::new()` which is non-const
under loom, so the `static REGISTRY` wouldn't const-evaluate; fixed exactly as
P3 did for `RacyPtrCell` (a const-capable `loom_shim` stand-in used
`#[cfg(loom)]` only); plus F2/F3 medium (missing LICENSE files for size-classes
+ proc-probe; README loom row corrected) and low/nit comment/doc accuracy. The
F9 proc-memstat Linux hardcoded-4 KiB-page bug was filed (then fixed — see
next). **`583cd8f`** fixes that bug: the Linux aperture read `/proc/self/statm`
(page counts) and multiplied by a hardcoded `PAGE_SIZE=4096`, under-reporting
RSS/commit 4×/16× on 16 KiB / 64 KiB-page kernels (aarch64, ppc64); replaced
with a page-size-independent `/proc/self/status` read (kB-denominated). **`9d6c9f4`
(#186, CRATE-P2-followup)** absorbs sefer's `COMMIT_FAIL_*` into a NEW distinct
vmem opt-in feature `fault-injection` (the mock feature couldn't take it over
— it replaces the whole backend with a stub, but the R7-B4 tests drive a REAL
`AllocCore` through real reservation/carve/decommit); sefer's `os.rs` now
delegates to `aligned_vmem::fault_injection` and the R7-B4/B2 tests stay green
unmodified (non-vacuous — they arm the fault via the delegated hook). **`3d25263`
(HYGIENE #188)** repoints two stale TSan-runner test targets removed in
`dfc1a34` to existing successors, unbreaking `[tsan] production`. **`6ce2df5`**
drops a redundant closure in `examples/malloc_macro.rs` flagged by
`clippy --all-features` (a CRATE-P1 follow-on the crate-scoped clippy run
missed).

**Platform, CI, and hardening batch — the deep-audit follow-throughs.** A
cluster of independent fixes from the 10-agent deep audit + the audit's
safe-code-soundness follow-up, all individually verified with counterfactuals:

- **PLAT-1 (`65ae170`).** `Layout::small_meta_end()`/`primordial_meta_end()`
  rounded their decommit/recommit-boundary offsets to the fixed 4 KiB `PAGE`
  constant — on a 16 KiB-page (Apple Silicon) or 64 KiB-page (some Linux/aarch64)
  machine the boundary lands mid-real-page and `madvise`/`VirtualFree` silently
  round it, breaking the M6 RSS-reclaim promise with no red CI signal. Fix: a
  compile-time `MAX_REALISTIC_PAGE_SIZE = 64 KiB` superset bound (the literal
  audit suggestion — calling `page_size()` at runtime — does not compile, both
  are `const fn` used in true const contexts); plus a belt-and-suspenders test
  asserting both boundaries are multiples of the REAL runtime page size.
- **`regression_magic_at_atomic_load` SIGSEGV (`f165ced`).** Root-caused via
  gdb + empirical repro (40/40 crashes without `alloc-decommit`, 0/40 with):
  the test deliberately races a cross-thread stale/duplicate Large free; under
  `alloc-decommit` the pages stay mapped (safe), without it `dealloc` calls
  `os::release_segment` immediately and the remote thread's `magic_at()` read
  races an actual unmap. Not a production soundness bug (reading a released
  segment's header is fundamental caller UB for any allocator, already
  documented) — the fix narrows the test's cfg gate to `alloc-decommit`, where
  its setup degrades benignly; the R6-MS-5 atomic-load regression stays fully
  covered there.
- **safe-surface empirical M1/M3 test (`403e216`).** A new zero-`unsafe` test
  installing `SeferAlloc` as `#[global_allocator]` and churning
  `Box`/`Vec`/`Arc` across 6 threads × 1500 iters × 6 size classes, with every
  allocation tracked in a `[start,end)`-keyed live table checked against its
  address-order predecessor/successor (provably sufficient for overlap
  detection) and sentinel-stamped at both ends, re-verified mid-life and at
  Drop. **Empirically confirms the actual safe-code soundness boundary this
  project depends on** (`alloc` must never hand out a pointer aliasing a
  still-live allocation — M1/M3 in `INVARIANTS.md`): 9,000 allocations/run,
  246 full-table sentinel verify passes/run, **zero M1/M3 violations** across
  10/10 runs. The narrower-than-it-sounds framing matters: the #202 SIGSEGV was
  a deliberate double-free through `unsafe fn dealloc` — caller UB by contract,
  unreachable from safe code; this is the first empirical check of the real M1/M3
  boundary.
- **docs(soundness) (`7bca3cf`).** Formalises the UB-vs-soundness distinction
  for M2/M3 in `INVARIANTS.md` (citing #202 as the worked example) and lands
  the 10-agent deep-audit reports + `SUMMARY.md`. Closes the T0.5
  soundness-boundary chain: #202 (fix) → #212 (empirical stress test) → #213
  (this docs commit).
- **DEBT-1 (`d8cc157`).** Wires 6 of 13 `tests/loom_*.rs` files that were
  never CI `--test` steps (`loom_dirty_publish`, `loom_dirty_multi_segment`,
  `loom_heap_overflow`, `loom_heap_overflow_drain_guard`,
  `loom_overflow_first_retry`, `loom_remote_ring_drain_guard`) into the
  existing jobs whose feature strings already match — no new jobs. CI was the
  only automated net for the shipping `RemoteFreeRing`/`HeapOverflow`/dirty-
  segment cross-thread protocols, and it had a silent gap.
- **TEST-1/TEST-2 (`e9d179b`).** 26 sites across 3 lazy-commit test files
  predicted the `committed_payload_end` frontier using a stale
  `cfg(all(windows, …))` split — wrong for Unix + `alloc-lazy-commit` +
  not(`numa-aware`) (the frontier bookkeeping is platform-independent). Masked
  because the only CI job exercising `alloc-lazy-commit` also always enables
  `numa-aware`, which independently forces the eager `SEGMENT` frontier — so
  the wrong assertion passed by accident. Fix: replaced every platform-based
  split with a pure `cfg(feature = "numa-aware")` split matching the actual
  production gate; the 20 previously-silent sites now run on every platform.
- **CONC-1 (`a64a539`).** A loom model of the GENUINE dirty-bitmap
  producer/consumer race: all 3 existing tests in `loom_dirty_multi_segment.rs`
  `.join()` every producer before the consumer runs, so loom never explored a
  drain genuinely racing in-flight `dirty.fetch_or`. Adds a concurrent
  producer/consumer model + a `#[should_panic]` counterfactual (Relaxed dirty
  word severs the happens-before chain) proving the harness is non-vacuous.
  Severity was already documented as low (a linear fallback scan independently
  guarantees correctness; the dirty bitmap is a pure optimization layer).
- **TEST-3 (`a08092f`).** `#[should_panic]` counterfactuals added to the 3
  remaining loom files that lacked one (`loom_epoch`, `loom_sharded`,
  `loom_dirty_publish`), each backing the file's own prose claim that removing
  a specific guard makes loom fail — 9 of 13 in-tree loom files now ship a live
  regression counterfactual.
- **DOCS-SYNC (`33929b9`).** README + `src/lib.rs` synced after the workspace
  grew 4 → 11 crates and R6 file-splits scattered tier-2 unsafe sites across
  new files: the "four companion crates" → eleven; the crates.io/docs.rs badge
  table 4 → 11 rows; the external-publishable-crates unsafe-story table 4 → 11
  rows; the tier-2 item-scoped unsafe table 6 stale filenames / 21 sites → 14
  files / 33 sites (matching the self-verifying grep exactly). New guard
  `tests/no_stale_doc_references.rs::readme_unsafe_inventory_counts_match_reality`
  re-derives the counts from the same grep and asserts the README tokens match
  — counterfactual-verified non-vacuous (corrupting 17 → 18 fails it).
- **HYGIENE-GRAB-BAG (`dbfeca3`).** Four independent low-risk fixes, zero
  production allocator logic changes: (API-1) README + `src/lib.rs` now flag
  `ring-mpsc` as a real, tested, but currently zero-production-consumer
  workspace member (the in-tree swap was NO-GO — `d062798`) so it doesn't
  silently bit-rot; (API-2) `#[non_exhaustive]` on two pre-1.0 mock enums;
  (DEBT-5) deleted genuinely-orphaned `RemoteFreeRing::is_empty` (zero callers
  since phase12.6, superseded by `tail_relaxed()`) and fixed the bare
  `#[allow(dead_code)]` on `overflow()` to match its file's convention;
  (LINTS-1) centralised the duplicated `unexpected_cfgs` lint table into a
  `[workspace.lints.rust]`.
- **`ffd3215`** applies an `@fxx` follow-up-batch review (verdict
  SHIP-WITH-FIXES; #181 bootstrap-safety CONFIRMED by independent write-by-write
  trace): F1 medium serialises the 4 real-backend `crates/vmem` fault-injection
  tests behind a process-global `Mutex<()>` (they share `FAIL_NEXT`/`FAIL_AT_*`
  atomics and libtest runs them in parallel); F4 nit strengthens
  `reserve_lazy`'s debug_assert to check all three documented preconditions.
- **`b37ef98`, `327449e`.** Two CI-only fixes a local Windows `npm run check`
  cannot reach: Unix-only clippy errors in `crates/vmem`'s `libc_mmap`
  (redundant nested `unsafe {}`, unused `mut`); and allowing the Unicode-3.0
  license for the `unicode-ident` transitive dep (cargo-deny CI failure).

**docs closeout — `75343532`, `f0dd9a9`, `64952a0`, `c815927`.**
**`75343532`** lands the 5-lane crate-extraction reports + `SUMMARY` +
`DEFERRED_AND_SKIPPED` rationale + session checkpoints. **`64952a0` (DEBT-2,
task #208)** is an honest **"no bug here"** outcome worth noting as such: the
audit's DEBT-2 finding claimed `crates/vmem/tests/fault_injection.rs` was
missing a `Mutex<()>` serialization guard, but `ffd3215` (the SAME follow-up
batch DEBT-2 cites as its own source) had already applied it before the
10-agent audit even started — the finding was stale at the moment it was
written; task #208 closes as a documentation correction only, no code change.
**`f0dd9a9`** lands 5 session checkpoints. **`c815927`** (technically the
round's final commit, though task-tagged R8-4) marks the B5-era stale claims in
the R7 perf reports as superseded by R7-B6 — no numeric measurement changed,
just inline annotations pointing back to `8977e88` so the historical B5 numbers
stay accurate for what B5 measured while never reading as present-tense fact.

### Round 8 — directory promotion, Large zero-skip, medium-classes GO verdict, lazy-commit pool Phase 1 (R8-1..R8-10)

Round 8 — 15 commits (`af7b039`..`68f5da7`), 2026-07-19..20 — the external
perf/correctness review's R8 task queue. Three workstreams: finishing the
Round 7 segment-directory story (R8-1..R8-3, ending in the directory's
**first-time promotion into the `production` bundle**), a set of
constant-factor / layout / zero-skip optimizations (R8-5, R8-6, R8-8), and
two measurement-only GO/NO-GO verdicts (R8-7 batch ceiling, R8-9
medium-classes) plus one lazy-commit pool tradeoff (R8-10 Phase 1). Honest
verdicts throughout: GO on the directory sub-chain and the medium-classes
feature, GO-but-measurement-only on the batch ceiling (later downgraded by
R9-9), and an explicit acknowledgment that R8-8's Large zero-skip shipped
with a Miri correctness bug found and fixed in the next round (R9-1).

**Production vs. opt-in — what actually changed for default `--features
production` users.** This is the round where the segment directory first
went **default-on instead of opt-in**. `production` is now
`["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
"alloc-segment-directory"]` — the last entry added by R8-3:

- **Landed in `production` (default behavior changed):**
  - **R8-3 (`ec7ac34`)** — promotes `alloc-segment-directory` into the
    `production` bundle. The single behavior-shifting promotion of the
    round: every default `--features production` build now pulls in the
    directory, so the 166–254× refill-miss speedup from Round 7's r7-a6
    (and the R8-1/R8-2 fixes below) reaches ordinary users for the first
    time. First time the directory went default-on instead of opt-in.
  - **R8-2 (`09237e0`)** — authoritative directory-miss. The miss path's
    early-return becomes reachable from production as a consequence of
    R8-3's promotion (the logic is gated on `alloc-segment-directory`).
  - **R8-6 (`e9db718`)** — segment-layout `payload_start`/`decommit_start`
    split. Always compiled (no feature gate); tightens the payload
    boundary on 4 KiB-page systems in every config including production.
  - **R8-8 (`93dba14`)** — Large-path zero-skip in `alloc_zeroed`. Always
    compiled (the `alloc_large` path); skips the explicit `Node::zero`
    pass on genuinely fresh OS reservations in every config including
    production. *(Shipped with a Miri correctness bug — see the R8-8
    entry below and the R9-1 forward-reference; on real OS backends the
    optimization is correct as shipped.)*
  - **R8-5 (`fa2c064`)** — frontier-stamp fix. The source change lands in
    production-compiled files, but the behavioral change is feature-gated
    inside `alloc-lazy-commit` (which `production` does not enable), so it
    is **inert under stock production** and only active for opt-in
    `alloc-lazy-commit` users. Listed in this group because the code is
    in the production-compiled tree, not because production behavior
    shifts.
- **Stayed opt-in (NOT in `production`):**
  - **`medium-classes`** — R8-9's GO verdict is measurement-only; the
    feature itself remains experimental/opt-in.
  - **`alloc-lazy-commit` pool changes** — R8-10 Phase 1's
    stop-decommitting-on-admission fix is gated on `alloc-lazy-commit`
    (not in `production`).

**Directory sub-chain — R8-1..R8-3, finishing Round 7's Workstream A.**
Round 7 shipped the segment directory behind an opt-in feature and proved
its HIT speedup, but two regressions kept it opt-in: an O(D×49)
post-drain resweep (R7-A1's full-sweep `sync_directory_for_segment` at
every ring drain) and an O(S) linear-scan fallback on every genuine
directory MISS. R8 closes both, then promotes the result.

- **R8-1 (`af7b039`) — incremental per-class directory sync.** Each
  ring-drain closure now accumulates a `u64` bitmask of the classes it
  actually touched (`entry_class_idx` unpacks just the class from the
  packed ring entry), and the post-drain sync
  (`sync_directory_for_segment_classes`) re-checks only those classes —
  `O(popcount(changed_classes))` instead of `O(SMALL_CLASS_COUNT)`. The
  old full-sweep `sync_directory_for_segment` is deleted (zero callers
  after migrating all 4 drain sites). Eliminates the review's measured
  **1.4–2× regression at dirty ≥ 10% density** (the O(D×49) per-lookup
  cost). `tests/dirty_directory_incremental_sync.rs` proves the
  load-bearing case no existing directory test covered — a single drain
  reclaiming blocks of TWO different classes into the SAME segment must
  set BOTH classes' bits (a last-class-only bug would pass every
  existing test); verified non-vacuous by reverting the OR-accumulation
  to overwrite and watching the test go red.
- **R8-2 (`09237e0`) — authoritative directory-miss with periodic
  self-heal.** The HIT-only speedup left the MISS path unchanged: a
  genuine miss unconditionally fell through to the O(S) linear scan, so
  a miss cost the same as if the directory didn't exist — defeating the
  directory's whole point for cold-growth/carve-storm workloads. Fix:
  trust a genuine directory miss as authoritative in the common case
  (immediate `return None`, skipping the O(S) scan — the caller carves a
  fresh segment, same as it would after a full scan also finds nothing),
  bounded by a periodic safety net: every `DIRECTORY_MISS_FULL_SCAN_PERIOD`
  (256) misses a re-validation full scan still runs, and if it finds a
  segment the directory missed the bit is repaired in-place (self-heal)
  with a canary counter (`DIRECTORY_MISS_SELF_HEAL`) expected to stay 0.
  Two new stats counters
  (`DIRECTORY_AUTHORITATIVE_MISS`, `DIRECTORY_MISS_SELF_HEAL`).
  `tests/directory_authoritative_miss.rs` proves (1) a genuine miss skips
  the O(S) scan via a counter-delta proof independent of wall-clock
  noise, (2) the periodic pass fires exactly at the period boundary, (3)
  self-heal repairs manufactured directory drift. **R8-2 follow-up
  (`38f4108`)** excludes `numa-aware` from that test file: under
  `numa-aware` the entire directory-driven lookup block (including this
  task's authoritative-miss and self-heal logic) is compiled out, so
  under `--all-features` every genuine miss silently fell through with
  counters at 0, failing the first assertion — caught by `npm run
  check`'s `--all-features` matrix entry.
- **R8-3 (`ec7ac34`) — promote `alloc-segment-directory` into the
  `production` bundle.** With both keeping-it-opt-in regressions fixed
  (R8-1, R8-2), `production` now also pulls in
  `alloc-segment-directory`. Verified before promoting (not just
  delegated): full production suite with the feature explicitly combined
  (175 binaries, 397 tests, 0 failed), then re-run with just
  `--features production` to confirm the promotion took effect. Synced
  the 3 living doc references that spell out production's constituent
  feature list (README ×2, `src/lib.rs` ×1); left dated historical
  review snapshots untouched. Found and fixed two clippy regressions
  that would have broken `npm run check`'s `--all-features` gate and a
  plain `--features production` build before landing (an `unused_mut`
  under `numa-aware` from R8-2's own `periodic_revalidation_active`, and
  a `dead_code` `SENTINEL` under production-without-`hardened`), plus a
  stale `tests/*.rs` count in `ARCHITECTURE.md` (**171 → 173**,
  `180bb8a`).

**Lazy-commit frontier — R8-5 (`fa2c064`).** `reserve_aligned_lazy` is
genuinely lazy (partial 2-phase reserve+commit) ONLY on real Windows; the
Unix and miri `reserve_aligned_lazy_raw` implementations ignore
`initial_commit` and commit/mmap the whole segment up front. Despite this,
both production sites that stamp `committed_payload_end` after a lazy
reservation (`reserve_small_segment`, `bootstrap::primordial`) used only a
`numa-aware` cfg split, so Unix/miri understated the frontier at
`meta_end + LAZY_FIRST_CHUNK` even though the OS had already committed
everything — every carve past that artificial frontier still ran through
the grow-on-carve path (bounds check + a commit syscall that is a
correctness no-op on those platforms + an atomic counter bump) for zero
benefit. Fix: 3-way split at both sites — `numa-aware` stays `SEGMENT`
(unchanged), real Windows-not-miri keeps the genuine lazy value, Unix/miri
now also gets `SEGMENT` immediately, matching OS-level reality and
restoring `alloc-lazy-commit`'s promised zero-cost-when-unneeded property
there. Deliberately reverses part of this session's own earlier task #191
(`e9d179b`), which had simplified 26 test-assertion sites down to a
`numa-aware`-only split (correct at the time, since the production code
had no platform gate); those sites are re-split to match. **Stays opt-in:
`production` does not enable `alloc-lazy-commit`, so this is inert under
stock production**; verified on the Windows-lazy leg (39/39) and the
numa-aware leg (34/34). The true-Unix (non-miri) leg could not be tested
directly this session (no Linux environment) — covered by code review + a
passing miri run that exercises the same branch, to be confirmed by CI.

**Segment layout — R8-6 (`e9db718`), split `payload_start` from
`decommit_start`.** Task #205 (`65ae170`, Round 7) fixed a real platform
bug — `small_meta_end()`/`primordial_meta_end()` were aligned to the
compile-time `PAGE` (4 KiB) constant, but decommit/recommit operate on the
REAL OS page size (16/64 KiB on ARM/some Linux); a 4 KiB-aligned decommit
boundary on those platforms lands mid-real-page and the OS silently rounds
it, reclaiming the wrong byte range. #205's fix over-aligned both
functions to `MAX_REALISTIC_PAGE_SIZE` (64 KiB) — correct, but it
conflated two distinct concepts and cost ~56–64 KiB of payload per 4 MiB
segment on ordinary 4 KiB-page systems (`SMALL_META_END` jumped 73728 →
131072) even though those systems never needed the extra margin. Fix:
split into (1) `small_meta_end()`/`primordial_meta_end()`, reverted to
tight `PAGE`-alignment — used by bump init, the H-1 "is this offset in
metadata" guard, primordial registry/hash/free-list placement, and
page-map marking, none of which have any OS-page interaction; and (2) new
`small_decommit_start()`/`primordial_decommit_start()`, runtime
(non-const) functions that round the tight boundary up to the REAL
`aligned_vmem::page_size()` — used ONLY by the 3 actual decommit/recommit
syscall sites. On 4 KiB-page systems `small_decommit_start()` collapses to
exactly `small_meta_end()` — zero waste; on 16/64 KiB-page systems it
returns the same value #205 used to force unconditionally, so the
real-page-safety guarantee is unchanged. `MAX_REALISTIC_PAGE_SIZE` stays
load-bearing (wired into a `debug_assert` in both new functions guarding
the "superset of every real page size" invariant). Measured payload
recovery on this 4 KiB-page host: `SMALL_META_END` **131072 → 73728, 56.0
KiB recovered** per segment. Independently re-run under `cargo +nightly
miri test` (96s, 3/3 pass) — miri's strict-provenance checking is exactly
the tool that would catch a boundary off-by-one in this class of change.

**Large-path zero-skip — R8-8 (`93dba14`) — SHIPPED WITH A MIRI BUG, FIXED
BY R9-1.** `AllocCore::alloc_large`/`alloc_large_slow` now return
`(*mut u8, bool)`, where the bool is true iff the pointer is a genuinely
fresh OS reservation (unconditionally OS-zero-filled on every real
platform) rather than a `large_cache` HIT (a reused segment that may still
hold the prior occupant's bytes). `AllocCore::alloc_zeroed` and
`HeapCore::alloc_zeroed` dispatch on this signal for Large-classified
requests and skip the explicit `Node::zero` pass only when the reservation
is fresh; a cache hit is still zeroed explicitly. Small-classified
requests are unaffected (explicitly out of scope — the small-path
equivalent is R9-5, design-only). `tests/alloc_zeroed_fresh_large_skip.rs`
includes a byte-level regression guard that plants a `0xAA` pattern into a
freed large segment, forces a confirmed cache-hit re-allocation via
`alloc_zeroed`, and asserts the returned memory is fully zeroed (i.e. the
skip must NOT fire on a cache hit); verified non-vacuous by inverting the
`is_fresh` condition and confirming the test fails exactly on the planted
pattern.

> **⚠ Forward-reference — Miri correctness bug, fixed in the next round
> (R9-1 / `860d897`).** R8-8 as shipped reported `is_fresh = true`
> unconditionally, but miri's `std::alloc` fallback in `crates/vmem` does
> NOT zero (unlike every real OS backend). A fresh Large `alloc_zeroed`
> under miri could therefore skip the explicit `Node::zero` pass and
> return uninitialized memory, violating the `alloc_zeroed` contract.
> This is not a clean unqualified win: R9-1 (`860d897`) withholds the
> skip under miri (`alloc_large_slow` returns `cfg!(not(miri))` instead
> of unconditional `true`) and adds the `LARGE_ZERO_PASS_CALLS` /
> `dbg_large_zero_pass_count` diagnostic counter the original R8-8 test
> lacked (the original test would stay green even with an unconditional
> memset reintroduced — it proved the cache-hit zeroing but not that the
> optimization itself fired). On real OS backends R8-8's optimization is
> correct as shipped; the bug is miri-specific. The fix itself belongs to
> the Round 9 section; this entry acknowledges the gap honestly rather
> than reading as a standalone clean win.

**Medium-classes 256 KiB–1 MiB — R8-9 (`9afba66`), verdict GO
(measurement-only, feature stayed opt-in).** Runs the
`benches/medium_size_sweep.rs` harness (R6-OPT-A3, built as Stage A but
never previously run for a verdict — this is the missing Stage B) at
quick then `--reduced` tiers, `medium-classes` OFF vs ON. Findings: the
review's "16× fewer segments near the 253 KiB cliff" claim is **confirmed
precisely at n=64** (64 → 4 segments; cardinality caveat: ~15× at n=1024,
formally infinite at n≤8 where the pre-cliff side fits in the primordial
segment); free latency improves **48–600× across the covered range** (e.g.
256 KiB n=64: 93,234 → 200 ns, ~466×); the n=1024 address-space OOM is
eliminated for every covered size (the OFF path literally cannot reserve
the ~4 GiB 1024 dedicated 4 MiB spans demand, exhausting at object 1023);
a real warm freelist exists where the Large path structurally cannot have
one (256 KiB reuse rounds: ~90 µs → ~60 ns free from round 1 on, +0 segs;
the Large path re-reserves and re-releases 64 spans every round). No
regression for sizes whose path doesn't change (240/252 KiB, 258,752 B,
1.5/2/4 MiB byte-identical across configs). New finding: a **second cliff
now sits at the new `SMALL_MAX` = 1 MiB** — 1.5/2 MiB still pay the full
dedicated-span cost in both configs. Verdict: **GO (strongly) on the
existing 6 classes**; **CONDITIONAL GO, split, on extending further** —
clear case for closing the 1 MiB–2 MiB gap (design call: fixed classes vs
general page-run layer, left open), weak case for finer sub-1 MiB
granularity (rounding waste bounded ~20–31%, already-covered common
sizes). Measurement-only: no `src/` touched. **The feature stays
experimental / opt-in — promotion is a separate decision this report only
supplies evidence for.** Full method, raw numbers, and caveats in
[`docs/perf/R8_9_MEDIUM_CLASSES_VERDICT.md`](docs/perf/R8_9_MEDIUM_CLASSES_VERDICT.md).

**Batch-alloc ceiling — R8-7 (`de4c4ae`), verdict GO but measurement-only
(later downgraded by R9-9).** An external perf review speculated a public
batch/scoped alloc API (`alloc_batch`/`dealloc_batch`) could give 1.5–3×
on bulk small-object patterns by amortising TLS lookup/classification/
routing over many blocks per call. Contemplative analysis flagged the
trap: no consumer of such an API exists in this repo today, so a bench
purpose-built around a not-yet-existing signature would only prove the
mechanism works, not that it's worth shipping — circular. Instead:
measure the ceiling via the already-existing internal batch primitives
(`AllocCore::refill_class_bump` / `flush_class`, already used in
production on the magazine-miss/overflow paths) called directly from a
new bench arm, zero new public API. Measured (3 runs, 1024-block cold
bulk, criterion fast profile): **16 B: 2.73× average ceiling (GO, clears
the review's 1.5× floor); 64 B: 1.71× (GO); 256 B: 1.20× (NO-GO, below
floor)**. Verdict GO — task graduates to the signature-design phase
(explicitly not started here). This is a **ceiling, not a
shippable-API forecast** — a real public API pays extra argument
validation the raw internal call does not. Full method, raw per-run
numbers, and caveats in
[`docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md`](docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md).
*(Forward-reference: R9-9 / `5e467ec` later downgraded this measurement
by sweeping smaller batch sizes — 8/16/32/64, not just 1024 — and adding
a real-`SeferAlloc` arm; the small-batch numbers compress the ceiling
materially. The R9-9 numbers and verdict belong to the Round 9 section;
this entry records the R8-7 result as measured and flags the downgrade
honestly.)*

**Lazy-commit pool — R8-10 Phase 1 (`852828e`), stop decommitting pooled
small segments on admission.** An external perf review found
empty→pool→reuse→refill cycles on Windows `alloc-lazy-commit` cost
**50–75× more commit/decommit syscalls** than the eager path for the
identical cycle. Root cause: `release_or_pool_empty_segment` decommitted
the payload above the initial lazy chunk and reset all metadata (bump,
free lists, `is_decommitted`) the instant a segment was admitted to the
hysteresis pool — defeating the pool's own purpose (a segment pushed to
the front as "the warmest entry, expected back imminently" was
immediately decommitted, so first reuse always paid a recommit). Fix,
both sites together (removing one without the other is a correctness
bug, not just a missed optimization): `alloc_core_small_pool.rs` removes
the `alloc-lazy-commit` decommit block from
`release_or_pool_empty_segment` (pool admission now behaves identically
on the eager and lazy-commit legs — nothing is reset, the segment stays
exactly as committed as it was on emptying); `alloc_core_small.rs`
removes the matching pop-pooled-segment-as-carve-target block in
`reserve_small_segment` (which relied on admission having reset the
segment into a clean carve target; pooled-segment reuse now goes
exclusively through `find_segment_with_free`'s free-list path, same as
the eager leg). `tests/lazy_commit_b3_recycle.rs` rewritten under the
new invariant: a full empty→pool→reuse→refill cycle costs exactly zero
`GROW_COMMIT_COUNT` and zero `dbg_decommit_count()` deltas; verified
non-vacuous by reverting only the two src files and confirming 4/5 tests
go red (a `GROW_COMMIT_COUNT` delta of 15 and a decommit delta of 2,
matching the review's 50–75× claim). **Stays opt-in (`alloc-lazy-commit`,
not in `production`).**

> **⚠ Forward-reference — the latency-first RSS tradeoff this lands was
> criticized by R9-7.** R8-10 Phase 1 is a latency-first tradeoff: by not
> decommitting pooled segments it cuts reuse latency, but it costs RSS —
> today's pool retains **exactly 16 MiB of committed payload per
> materialized heap** while pooled
> (`DEFAULT_POOL_SEGMENTS = 4` × `SEGMENT = 4 MiB`; the only drain is
> `maybe_decay_small_pool`, which fully releases one FIFO-oldest segment
> per `decay_interval`, default 1 s — no intermediate
> "committed-but-cheap-to-revive" state). Round 9's R9-7
> (`docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md`, design-only, no code
> change) characterized this tradeoff explicitly and designed (but did
> not ship) a third "decommitted-but-still-pooled" state that would let a
> memory-constrained deployment trade some latency back for lower
> committed RSS. The R9-7 design itself belongs to the Round 9 section;
> this entry records that R8-10's latency-first choice is the one R9-7
> later pushed back on, not a free win.

**Misc — test/formatting/doc fixes.**

- **`180bb8a`** — fixes a stale `tests/*.rs` file count in
  `ARCHITECTURE.md` (**171 → 173**, a side effect of R8-1/R8-2 each
  adding one new test file); caught by the self-verifying
  `architecture_test_file_count_matches_reality` test.
- **`f919d5b`** — lands session checkpoint files.
- **`f97cf1f`** — fixes pre-existing rustfmt drift in
  `crates/vmem/src/lib.rs` (pure line-wrapping of one `#[cfg_attr(...)]`
  attribute, no semantic change; pre-existing since task #210 never ran
  `cargo fmt`; blocked a clean `npm run check`).
- **`ac110b6`** — corrects `misaligned_offset_guard`'s release-build
  truncation math. The release-build branch asserted that packing a
  `MIN_BLOCK+1` offset truncates to `off16=0` on round-trip; that's wrong
  (`off16 = off >> MIN_BLOCK_SHIFT` is a floor division, so
  `(MIN_BLOCK+1) >> MIN_BLOCK_SHIFT == 1`, unpacking back to `MIN_BLOCK`,
  not 0). The assertion was never exercised under a normal `cargo test`
  (which runs the `debug_assertions` branch, where `pack_entry_hardened`
  panics instead), only under `--release --all-features` — a combination
  no CI matrix entry or prior `npm run check` happened to hit. Found
  while zero-trust reviewing R8-10 and running the full suite under
  `--all-features`; confirmed unrelated by reproducing against pristine
  HEAD 3×.
- **`68f5da7`** (the round's final commit) — hardens the
  `backshift_no_latency_spike_at_threshold_boundary` regression test's
  per-dealloc wall-clock max/median ≤ 30× check against OS noise (a
  single dealloc occasionally stalls multi-ms from scheduler preemption,
  page faults, AV I/O hooks — reproduced repeatedly this session with no
  code change, ratios varying 42×–607×, 1/6 to 6/6 pass rate). Wraps
  only the noisy max/median ratio check in a bounded 3-attempt retry; the
  membership/correctness assertions stay unconditional (a genuine
  per-delete `O(HASH_CAPACITY)` regression reproduces deterministically
  every attempt, so detection power is preserved).

### Round 9 — Miri correctness fix, medium-classes-wide prototype, directory drift hardening, honest downgrades (R9-1..R9-9)

Round 9 — 11 commits (`860d897`..`d26e042`), 2026-07-20 (a single day) —
the external review's follow-up queue against the Round 8 HEAD. This round
is overwhelmingly **research / measurement / design work, not production
hot-path changes**: of the nine R9-numbered tasks, only **two** touch
production-compiled source for real (R9-1's Miri correctness fix and
R9-8's directory-drift bound); the other seven are measurement reports
(R9-2, R9-3, R9-6, R9-9), a new opt-in prototype feature that stays
opt-in (R9-4), or design-only docs (R9-5, R9-7). The round's character is
honest verdicts over shipped wins: two **downgrades** of prior GOs (R9-4's
density came in below the review's guess; R9-9 downgraded R8-7's batch
ceiling to CONDITIONAL-NO-GO), two **CONDITIONAL-GOs** that defer to a
future wall-clock measurement (R9-3's promotion gate, R9-6's class-aware
dirty routing), and two **design-only** outcomes (R9-5, R9-7) that are
treated as fully successful results, not shortfalls — each found a real
reason a rushed prototype would have been dead code or unsound, and
declined to ship that.

**Production vs. opt-in — what actually changed for default `--features
production` users.** Round 9 is the first round since the directory's R8-3
promotion where the production bundle itself is stable: no feature is
added to or removed from `production` this round. Only two code changes
reach production-compiled source:

- **R9-1 (`860d897`)** — Large zero-pass Miri fix. Always compiled (the
  `alloc_large` path), but the behavior change is **miri-only**: on every
  real OS backend R8-8's optimization was already correct, so
  `cfg!(not(miri))` evaluates the same as the old unconditional `true`
  there. No real-OS user sees a behavior shift; only a miri run does.
- **R9-8 (`5a4ba62`)** — directory drift recovery. The directory is in
  `production` since R8-3, so the per-class miss-streak + OOM-rescue scan
  reach production users — but as **defense-in-depth against a
  hypothetical drift the invariant-preserving API cannot construct**
  (task #214's `assert_directory_equals_rebuild` oracle proves the
  incremental directory tracks true state in every tested scenario), not
  a fix for a known bug.
- **Everything else is measurement/design/opt-in:** R9-2 / R9-3 / R9-6 /
  R9-9 are `docs/perf/*.md` + benches/tests only (no `src/`); R9-5 / R9-7
  are `docs/perf/*.md` design docs only; and R9-4's `medium-classes-wide`
  is a new opt-in feature (not in `production`, not in any default bundle,
  and its own follow-up fix `78fd98d` keeps it from regressing
  `--all-features`).

**R9-1 (`860d897`) — Miri correctness fix for R8-8's Large zero-skip.**
Closes the P0 bug R8-8 shipped with (flagged forward in the R8-8 entry
above). `alloc_large_slow` reported `is_fresh = true` unconditionally, but
miri's `std::alloc` fallback in `crates/vmem` does NOT zero (unlike every
real OS backend), so a fresh Large `alloc_zeroed` under miri could skip
the explicit `Node::zero` pass and return uninitialized memory — violating
the `alloc_zeroed` contract. Confirmed by reading the vmem miri fallback
directly before fixing anything. Fix: `alloc_large_slow` now returns
`cfg!(not(miri))` instead of unconditional `true`; both consumers
(`AllocCore::alloc_zeroed`, `HeapCore::alloc_zeroed`) are otherwise
unchanged since they already branch on the freshness bool. The original
R8-8 test also couldn't prove the optimization itself fired — it would
stay green even with an unconditional `memset` reintroduced. Added a
process-wide diagnostic counter (`LARGE_ZERO_PASS_CALLS` /
`dbg_large_zero_pass_count`) bumped at both zero-pass call sites, and
rewrote `tests/alloc_zeroed_fresh_large_skip.rs` to assert exact deltas
per platform (0 under a real OS, 1 under miri). Verified non-vacuous by
counterfactual (reverting the `!is_fresh` guard turns the rewritten test
red with the expected delta mismatch), then re-confirmed under real miri
(the miri-only workload was shrunk `LARGE` 2 MiB → 1 MiB+4 KiB, `ITERS`
80 → 4 under `cfg(miri)` only, since the logic under test has no size- or
iteration-count-dependent branch — cut the miri run from hours to ~17 min
without weakening native-path coverage). Also lands doc drift the same
review flagged (Cargo.toml's `alloc-segment-directory` comment still
calling it experimental post-R8-3; ARCHITECTURE.md's M6 decommit section
describing the pre-R8-10 lifecycle; `segment_directory_a5.rs`'s pooled-
segment test doc).

**R9-2 (`3021b16`) — fresh post-Round8 cross-version bench, verdict: Round
8 did not move the default-bundle wall-clock past the noise floor.**
Refreshes the cross-version wall-clock comparison (0.3.0 = current `main`
vs 0.2.1 = `bench/0.2.1`) anchored at current HEAD (`860d897`, Round 8 +
R9-1), same methodology as Round 7's. *(Filename/content note: this
report lives at `docs/perf/R8_CROSS_VERSION_BENCH.md` — the filename says
"R8" but the file IS the R9-2 deliverable, a fresh post-Round8 re-anchor
of R7's methodology; there is no R9-named cross-version file, matching the
R7→R8 continuation convention.)* Top-line: the production wall-clock
bundle did not move meaningfully vs R7 — Round 8 + R9-1 were correctness /
feature-gated opt-in (`medium-classes`, `alloc-lazy-commit`) /
constant-factor work, none of which the default production bundle would
be expected to surface. The Round 7 multiplicative reuse-cycle wins are
re-confirmed intact: the decommit cycle **~292× faster**, the oscillating
working-set cycle up to **~3.44× faster at 1024 B**, and the churn family
at 64 B+ still winning **~1.4–2.0× at 64–256 B and ~2.0–10.8× at 1024 B**
(vs 0.2.1). No real regression; the ns/op columns stay within the
documented ±15–20% inter-run host-noise floor. Full method and numbers in
[`docs/perf/R8_CROSS_VERSION_BENCH.md`](docs/perf/R8_CROSS_VERSION_BENCH.md).

**R9-3 (`c8f5f32`) — `medium-classes` production-promotion gate, verdict GO
(by IAI) / status stayed CONDITIONAL-GO.** R8-9 gave a GO verdict for
`medium-classes` but only measured its own target range (256 KiB–1 MiB)
via `AllocCore` directly. The external review flagged three structural
side-effects hitting every build that R8-9 did not check — `SIZE_CLASS_TABLE`
growing 49→55, per-`HeapCore` tcache footprint growth, and first-heap
commit charge — so this task runs all three plus the deterministic IAI
instruction-count gate for the unaffected sizes (16–1024 B) through the
real `HeapCore`/`GlobalAlloc` production path. Findings: tcache footprint
exactly **+816 B/HeapCore** (`PerClass`=136 B × 6 new classes — confirmed,
not estimated, matching the review's estimate exactly); IAI Ir on the
small-size gates **+0.49% to +0.67% total, +0.1% to +0.5% per-op marginal**
(bootstrap-spread from the larger table zero-init, not a per-op cost);
first-heap commit charge **+48 KiB at the 1-heap bootstrap** (within one
page of the chunked-registry chunk-0 prediction) and **+4 KiB/slot
steady-state** (noise-floor). The criterion wall-clock showed **+37–56%
uniform across ALL sizes including 16 B** — declared a host-load artifact
and overruled by IAI (a real regression could not be uniform at 16 B,
which has zero interaction with the new classes). This wall-clock overrule
is methodologically debatable and was later criticized by external review,
so the status stayed **CONDITIONAL-GO**: the gate evidence is GO by the
deterministic judge, but the report does NOT itself flip the `Cargo.toml`
bundle, so the feature remains experimental/opt-in and promotion is not
enacted. The one large deterministic delta found — `realloc_grow` **+173.9%
Ir / +101.3% EstCycles** — is the feature working as designed on sizes it
targets (the geometric realloc sweep now passes through the new 256 KiB–1
MiB classes doing real small-path work where the Large path previously did
almost nothing), not a regression on the unaffected sizes this gate
protects. Full method, raw numbers, and the K-table in
[`docs/perf/R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md`](docs/perf/R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md).

**R9-4 (`f469343`) — `medium-classes-wide` prototype (1.25/1.5/1.75 MiB),
verdict: do-not-promote-as-is (density under-delivered).** Adds a new
opt-in feature `medium-classes-wide` (requires `medium-classes`,
transitively `alloc-core`; not in `production` or any default bundle) that
appends three exact classes (1.25 / 1.5 / 1.75 MiB) on top of the existing
six-class `medium-classes` `EXTRAS` list, growing `SMALL_CLASS_COUNT`
55→58 and `SMALL_MAX` 1 MiB→1.75 MiB. Purely additive: the existing
six-class `EXTRAS` list is byte-identical to pre-R9-4 (verified by test),
so R9-3's just-landed promotion-gate measurements stay valid. The external
review guessed ~3×/2×/2× objects-per-segment density for the three new
classes; this task **measured the real density empirically instead of
trusting the guess and found it 2×/1×/1×** — one block lower than guessed
for every class, because `carve_block`'s `align_up(bump, block_size)`
requirement (load-bearing: the free path derives block start via
`align_down(ptr, block_size)`) wastes one block of segment capacity at the
start, so `empirical_density = floor(SEGMENT / block_size) - 1`. Only
**1.25 MiB** delivers a real density win (2× vs the Large path's 1×); **1.5
and 1.75 MiB fit exactly 1 block per segment** (same 1× as the existing
Large path, though they still gain the warm-freelist win R8-9 measured for
same-size reuse — ~90 µs free → ~60 ns freelist push/pop). A new mini-cliff
sits at ~1.3 MiB (the rounding threshold into the 1.5 MiB class). 12 new
tests in `tests/medium_classes_wide_correctness.rs` pin class placement,
boundary routing (both directions), encoding-headroom ceilings, the
density finding (pinned by actual carve+segment-residency checks, not just
arithmetic), and topology non-disturbance of the 6-class substrate.
Recommendation: **do not promote as-is**; if promoted, restrict to just the
1.25 MiB class, or pair with a future page-run layer for 1.5–2 MiB (2 MiB
itself is out of scope here — it would also be 1×, needs a larger
medium-arena segment). This is an honest downgrade the sub-agent found
itself, reported against the review's optimistic guess. Full method,
geometry, and the verdict in
[`docs/perf/R9_4_1_75MIB_CLASSES_PROTOTYPE.md`](docs/perf/R9_4_1_75MIB_CLASSES_PROTOTYPE.md).

**R9-4 follow-up (`78fd98d`) — `--all-features` test-regression fix.**
R9-4's `medium-classes-wide` appends 3 classes on top of `medium-classes`
when both are enabled (exactly what `--all-features` does), which silently
broke three pre-existing tests that hardcoded plain-`medium-classes`
topology (55 classes / `SMALL_MAX` = 1 MiB) without accounting for
`medium-classes-wide` raising those to 58 / 1.75 MiB:
`tests/medium_classes_correctness.rs` and
`tests/segment_directory_a5.rs` (hardcoded 55-class assumptions, made
conditional on `cfg!(feature = "medium-classes-wide")`), and
`tests/regression_inplace_large_realloc.rs` (three tests used 1.5 MiB as a
hardcoded "definitely Large" size — no longer true under
`medium-classes-wide` where `SMALL_MAX` = 1.75 MiB, so 1.5 MiB routes
through the Small path and breaks the Large-path in-place-grow
optimization these tests exist to verify; bumped to 2 MiB, matching the
"unambiguously Large under every feature combination" convention).
Discovered by the orchestrator's own verification of R9-9 (running the
full suite under `--all-features`, not just isolated feature combos) and
fixed in a separate commit before R9-9 landed — an oversight during R9-4's
own review (each feature combo had been tested in isolation, never both
on at once). Verified: full `cargo test --release --all-features` now green
(180 test binaries).

**R9-5 (`7bdbc0f`) — virgin zero-skip for Small `alloc_zeroed`,
DESIGN-ONLY.** Designs a per-segment `payload_virgin` bool that would let
`alloc_zeroed` skip the explicit zero pass for a genuinely virgin
(never-before-carved) small block, mirroring the Large-path skip (R8-8 /
R9-1) at a finer (per-carve, not per-segment-reservation) granularity. All
five correctness risk areas the task enumerated (pooling, lazy-commit
incremental commit, release-vs-decommit+recommit macOS crux, batched
carve, remote-free reclaim) are resolved with file:line evidence and
independently spot-checked in this review. This exact idea was a
documented **NO-GO on 2026-07-10** (two blocking reasons: no per-block
virgin state, and an unresolved macOS `MADV_DONTNEED` risk) and was
re-flagged as unresolved by a 2026-07-19 deep audit; this design shows
R8-10 (2026-07-20, the day after that audit) removed the only production
code path that produced the macOS-dangerous decommit-then-reuse state,
dissolving that risk. The remaining objection (narrow win, cold-path
only) is acknowledged, not rebutted. **No code shipped:** a
substrate-only prototype (`AllocCore::alloc_zeroed`'s small arm) would be
fully testable but **production-inert**, since `HeapCore::alloc_zeroed`'s
small arm never delegates to `AllocCore::alloc_zeroed` (grep-confirmed
zero call sites) — the production win requires plumbing the virgin bit
through the magazine refill path, an open storage-design question staged
as a future 4-stage plan (measurement gate → substrate prototype →
magazine plumbing → promotion gate). Estimated win ceiling (analytical,
memset bandwidth): **~130 ns at 4 KiB to ~70–90 µs at 1 MiB** per
genuinely-virgin call; zero benefit on steady-state churn. Design-only is
the correct outcome here, not a shortfall: shipping the substrate
prototype would have been dead code. Full design in
[`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md`](docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md).

**R9-6 (`fd28ff8`) — class-aware dirty routing waste, verdict CONDITIONAL-GO.**
The external review found `drain_dirty_segments` (R7-A4) drains EVERY dirty
segment regardless of which size class `find_segment_with_free_impl` is
currently searching for — O(D) where D = dirty segment count, when per-
(segment, class) tracking could make it O(D_class). This task judges the
claim via one new diagnostic counter, not an implementation: `WASTED_DIRTY_DRAINS`
(directory_stats.rs) + `dbg_wasted_dirty_drains()`, bumped when a drain
visit produces zero reclaimed blocks of the sought `class_idx` (the sought
class is already available at the call site; `drain_dirty_segments` gained
one additive `class_idx` parameter, no control-flow change). Purely
diagnostic, Relaxed ordering, gated behind `alloc-stats` — zero behavior
change to the drain algorithm. The new judge test
(`tests/r9_6_class_aware_dirty_judge.rs`) drives a genuine mixed-class
remote-fan-in workload (N=1/2/4/8 producer threads each freeing a distinct
class into a shared owner while the owner continuously allocates one of
those classes) through the real `HeapCore::dealloc` cross-thread-free path.
Measured (3-run median): waste scales **super-linearly** with class count —
**~2% at N=1, ~56% at N=2, ~82% at N=4, ~95% at N=8**, consistently ABOVE
the naive (N−1)/N bound because the actively-consumed target class's dirty
bit clears faster than the collateral classes'. Confirms the review's
mechanism is real. Verdict **CONDITIONAL-GO**, not unconditional — this
measures counter ratios, not wall-clock win, and the absolute drain counts
are modest in this bench's shape (up to ~823 wasted drains per 4000 owner
allocs at N=8); next step before implementing is a wall-clock criterion
bench on the same workload shape (a >5% win at N=4 upgrades to GO). Full
method, raw counts, and the recommendation in
[`docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md`](docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md).

**R9-7 (`021c098`) — low-RSS pool policy for the lazy-commit tradeoff,
DESIGN-ONLY.** Designs a third pooled-segment state (decay-gated
decommit-and-reset to a blank carve target) to let a memory-constrained
deployment trade some latency back for lower committed RSS, addressing the
R8-10 latency-first tradeoff the R8-10 entry forward-references above
(R8-10's pool retains **exactly 16 MiB of committed payload per
materialized heap** while pooled — `DEFAULT_POOL_SEGMENTS = 4` × `SEGMENT`
= 4 MiB — with no intermediate "committed-but-cheap-to-revive" state).
**Central finding: the review's own suggested shape (decommit the payload,
keep free-list metadata intact) is UNSOUND.** The free-list `next` link is
stored in the first word of each free block's BODY (`Node::write_next` /
`read_next` write/read block payload directly), not in metadata — so
decommitting the payload destroys the chain. On Windows/Linux this
silently leaks every block past the head; on macOS (non-zero-guaranteed
recommit) it produces wild pointers, UB in production (hardened's
membership guard is not in the production bundle). Corrected design: reuse
the existing full-reset decommit primitive (`release_follows=false`) to
produce a blank carve target, reused via fresh carve rather than
free-list pop. Explains why decay-gating (vs R8-10's rejected
admission-gating) avoids the 50–75× commit/decommit blowup: the decommit
rate is bounded by the decay clock, not the allocation rate, and is zero
for any workload that stays hot. Cross-references R9-5: this design's
age-0→1 transition is exactly the "new decommit policy" R9-5 flagged as
needing a `payload_virgin=false` reset if R9-5's virgin-zero-skip ever
ships. **No prototype shipped:** the implementation surface (4
touch-points, including re-introducing a primitive R9-5 characterized as
fragile-to-reintroduce) exceeds this task's minimal/safe bar — ships a
safe zero-new-surface interim stopgap (shrink `pool_segments` + shorten
`decay_interval`, existing knobs only) plus a staged 4-phase plan for a
future task. Design-only is the correct outcome: the review's shape was
wrong, and shipping it would have been a silent-leak bug. Full design in
[`docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md`](docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md).

**R9-8 (`5a4ba62`) — directory drift recovery (per-class miss-streak +
OOM-rescue scan), verdict GO (implemented).** Two defense-in-depth fixes
to the R8-2 directory-authoritative-miss fast path (which trusts a
directory MISS for up to `DIRECTORY_MISS_FULL_SCAN_PERIOD - 1` consecutive
misses before a periodic re-validation scan). The external review found
R8-2 tracked that streak with a SINGLE `u32` shared across every size
class — so a drift-affected class's rescan could be delayed by cross-class
traffic, worst case ~255 wasted 4 MiB segments (~1 GiB VA) before the
shared counter trips. Two fixes, both verified non-vacuous by
counterfactual:

1. **Per-class miss-streak** — `directory_miss_streak` is now
   `[u8; SMALL_CLASS_COUNT]`, indexed by `class_idx`; each class trips its
   own rescan independent of other classes' traffic. Period dropped
   **256 → 64 (per-class)**, capping the worst-case drift bound at 64
   segments = **256 MiB, 4× tighter** than before (and strictly improving
   detection for a low-activity drifted class the shared counter could
   starve indefinitely).
2. **Rescue scan before OOM** — right before `reserve_small_segment`
   surfaces an OOM (table full or OS reservation failure), a forced O(S)
   linear scan runs as a last resort, bypassing the directory-trust fast
   path; if it finds a real free block the directory hid it self-heals the
   bit and serves that block instead of OOMing. Wired into both small-alloc
   OOM branches (`alloc_small`, and the magazine refill path via the
   checked variant to avoid a cross-thread double-issue). A new
   `DIRECTORY_RESCUE_OOM_AVOIDED` counter keeps this distinguishable from
   the periodic `DIRECTORY_MISS_SELF_HEAL` canary. Large allocations don't
   consult the directory (no `BinTable`), so no rescue is needed there.

A genuine drift remains **essentially impossible to construct** through the
invariant-preserving API (task #214's oracle proves the incremental
directory tracks true state in every tested scenario) — these are a safety
net against an undiscovered edge case or future regression, not a fix for
a known bug. Verified non-vacuous by counterfactual: temporarily routing
all classes through streak slot 0 (simulating the old shared counter) makes
the new decoupling test fail at its exact load-bearing assertion; reverted
and confirmed green. Also fixes a stale test-file-count assertion in
`docs/ARCHITECTURE.md` (**175 → 178**) that R9-6 had left behind. Full
design, counterfactuals, and the worst-case math in
[`docs/perf/R9_8_DIRECTORY_DRIFT_RECOVERY.md`](docs/perf/R9_8_DIRECTORY_DRIFT_RECOVERY.md).

**R9-9 (`5e467ec`) — batch-API realistic-size follow-up, verdict
CONDITIONAL-NO-GO (DOWNGRADE of R8-7).** R8-7 measured a 2.73×/1.71×/1.20×
GO/GO/NO-GO batch-alloc ceiling, but only at **batch=1024** and only
comparing **AllocCore-direct** arms (both bypassing `SeferAlloc`/TLS/
registry entirely). The external review's follow-up asked for realistic
batch sizes (8–64) and a comparison against the real public
`SeferAlloc`/`GlobalAlloc` scalar path. Added a sibling criterion group
sweeping N ∈ {8, 16, 32, 64, 1024} at the same three sizes (16/64/256 B)
with a third arm measuring `sefer.alloc`/`dealloc` through the real
`GlobalAlloc` impl; R8-7's own group is left untouched as the historical
batch=1024 baseline. Two findings:

- **The batch/scalar ceiling degrades sharply at realistic batch sizes.**
  16 B drops from 2.60× (n=1024) to **1.24–1.50× (n=8–64)**; 64 B from
  1.67× to **1.13–1.51×**; 256 B stays near 1.1× throughout. Two
  compounding causes: amortization thins out with fewer calls to amortize
  over, and a fixed cold-page-fault cost (fresh `AllocCore` per iteration)
  compresses the ratio toward 1.0 at small N.
- **The three-way comparison is the more decisive finding.** At every
  realistic batch size, the real `SeferAlloc` scalar path (warm per-thread
  tcache) is **2–30× FASTER than the `AllocCore` batch primitive** — the
  tcache already amortizes per-call overhead for small N, leaving nothing
  for a batch API to amortize until N is large enough to overflow the
  tcache. Even at n=1024 the batch primitive only beats real `SeferAlloc`
  scalar at 16 B (1.96×); it loses at 64 B and 256 B.

**Updated verdict: CONDITIONAL-NO-GO for realistic callers.** R8-7's GO at
16 B/64 B was specific to the unrealistic batch=1024 case. The one
surviving signal is narrow: 16 B at batch ≥ ~1024 for a caller that
genuinely issues such batches (no such caller exists in this repo today —
the same circularity concern R8-7 already flagged). For the 8–64 range the
review asked about, the verdict is **NO-GO**. The R8-7 report stays valid
as the historical batch=1024 / AllocCore-only baseline; nothing here
contradicts it. This is an honest downgrade of a prior GO, reported as
such. Full method, the three-arm grid, and the per-size verdict table in
[`docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md`](docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md).

**Misc — `d26e042` (the round's final commit) — checkpoint-only.** Lands
the Round 9 completion checkpoint
(`docs/checkpoints/2026-07-20-r9-complete.md`); no `src/`, test, or
`Cargo.toml` change.

### Round 10 — external-review follow-up: correctness fixes, honest gate corrections, batch API reversal (R10-1..R10-7)

Round 10 — 8 commits (`b2ef79e`..`9611a56`, inclusive of both ends),
2026-07-21 (a single day) — the external-review follow-up queue against
the Round 9 HEAD. The round's defining trait is that it is **unusually
self-correcting**: four of its eight entries revisit and revise a claim
an earlier round made, in every direction — downward (R10-2 builds the
wall-clock gate R9-3 deferred and flips R9-3's ambiguous "overruled by
IAI" wall-clock to a decisive NO-GO), upward (R10-7 builds the warm-batch
arm R9-9 only inferred and reverses R9-9's CONDITIONAL-NO-GO to a GO),
and as a magnitude correction that keeps the direction but fixes the
number (R10-5 corrects R9-4's ~1,500× "consolation prize" to the real
~2.3×); R10-4 adds a fourth, stranger shape — a CONDITIONAL-GO whose own
design identifies a strictly superior alternative and so declines to
ship. As in Round 9, this is **mostly measurement / design / docs work,
not production hot-path changes**: of the eight commits, only three
touch production-compiled source for real (R10-1's diagnostic gating,
R10-3's correctness fix, R10-7's new experimental surface), and only
R10-3 carries an observable behavior shift; the rest are perf reports
(R10-2, R10-5, R10-6), a design-only doc (R10-4), or a README + unsafe-
inventory sync (`6a11c61`). That the round spends four of its eight
entries correcting itself — building the gate a prior round deferred,
measuring the arm a prior round only inferred, correcting the baseline a
prior round mis-framed — is treated below as the round's central result
and a feature of this project's methodology, not a list of embarrassments
to downplay.

**Production vs. opt-in — what actually changed for default `--features
production` users.** As in Round 9, the production bundle's feature set
is unchanged this round: no feature is added to or removed from
`production`. Three commits reach production-compiled source, with
sharply different blast radii:

- **R10-1 (`b2ef79e`)** — diagnostic-counter gating. The
  `LARGE_ZERO_PASS_CALLS` static and `dbg_large_zero_pass_count()`
  accessor stay always-compiled (matching `WASTED_DIRTY_DRAINS` /
  `FOREIGN_OR_UNROUTABLE_FREES`), but both increment sites move behind
  `alloc-stats`. **Zero observable behavior change** on any build:
  production (alloc-stats off) loses the two counter bumps from the
  zeroed-allocation path and reads `0` from the accessor, exactly as the
  sibling counters do; the byte-content zero-fill guard (the
  load-bearing information-disclosure assertion) stays unconditional.
- **R10-3 (`abaad9c`)** — directory correctness fix (see below). The one
  real behavior shift in the round, and it is **beneficial**: the
  `changed_classes` bit is now set only when a block was genuinely
  reclaimed into the `BinTable`, eliminating spurious directory-syncs for
  classes whose ring entries were all rejected. The
  `reclaim_offset`/`reclaim_offset_checked` return value is also
  corrected (was "did decommit fire?", now "was the block reclaimed?").
- **R10-7 (`9611a56`)** — adds `HeapCore::alloc_batch` /
  `SeferAlloc::alloc_batch` / `dealloc_batch`, all `#[doc(hidden)]`
  experimental surface, **not wired into `GlobalAlloc`**; the production
  alloc/dealloc path is unchanged and the new code is only reachable via
  the opt-in experimental API.
- **Everything else is measurement/design/docs:** R10-2 / R10-5 / R10-6
  are `docs/perf/*.md` + probe binaries/benches only (no production
  `src/`); R10-4 is a `docs/perf/*.md` design doc only; `6a11c61` is
  README + a stale line-ref sync.

**R10-1 (`b2ef79e`) — gate `LARGE_ZERO_PASS_CALLS` increments under
`alloc-stats` (hygiene).** Pure hygiene fix against the diagnostic
counter R9-1 introduced. R9-1 bumped `LARGE_ZERO_PASS_CALLS`
unconditionally at both zero-pass call sites (`AllocCore::alloc_zeroed`,
`HeapCore::alloc_zeroed`); this matches the convention the rest of the
directory/drain diagnostic family already follows: the static and the
`dbg_large_zero_pass_count()` accessor stay always-compiled so the read
surface is stable across feature sets (reading `0` when no increment was
compiled in), while the increments themselves move behind `alloc-stats`
so the zeroed-allocation path carries no bookkeeping unless the caller
opts in. `tests/alloc_zeroed_fresh_large_skip.rs`'s counter-delta
assertions are gated the same way; the byte-content zero-fill checks (the
load-bearing information-disclosure guard) stay unconditional. **Zero
behavior change** on any build without `alloc-stats`.

**R10-3 (`abaad9c`) — gate `changed_classes` on actual reclaim success
(correctness fix).** Found while fixing the R9-6 `WASTED_DIRTY_DRAINS`
metric, and the root cause ran deeper than the metric itself. A rejected
cross-thread ring entry (double-free guard, in-magazine duplicate, stale
generation, garbled offset) never mutated the segment's `BinTable`, but
`drain_dirty_segments`'s `changed_classes` bitmap set the class bit
unconditionally anyway — under-counting `WASTED_DIRTY_DRAINS` (a drain
that rejected every entry of the sought class still looked "not wasted")
and triggering a spurious directory-sync for an unchanged class. The
deeper bug: `reclaim_offset` and `reclaim_offset_checked` returned
`dec_live_and_maybe_decommit`'s result (true = decommit fired), **not**
whether the block was actually reclaimed — under `not(alloc-decommit)`
this was always `false`, making the return value useless as a
reclaim-success signal. Restructured both functions to return true iff
the block was linked into the `BinTable`; `dec_live_and_maybe_decommit`
is now called separately at each of the 6 call sites (3 in
`alloc_core_small.rs`, the `dbg_drain_all_rings_impl` test hook, and the
2 `HeapOverflow` drain sites in `heap_core_xthread.rs`) after a
successful reclaim. `changed_classes` is now gated on this corrected
reclaimed signal everywhere it is accumulated. New counterfactual test
(`tests/r10_3_rejected_entry_changed_classes.rs`) drives a genuine
cross-thread double-free through the production alloc path and proves
the fix red-before/green-after; R9-6's judge was re-measured post-fix and
is unchanged for that workload (it has no rejected entries), narrowing
but not eliminating its "lower bound" caveat (down to only the
empty-ring-visit exclusion).

**R10-2 (`c8d53af`) — native A/B/B/A wall-clock gate for
`medium-classes`, verdict NO-GO on realloc (corrects R9-3's ambiguity).**
Builds the methodologically clean process-level judge the external review
asked for after R9-3's single noisy criterion run (`+37…+56%` uniform
across ALL sizes including 16 B, overruled by IAI) proved neither
acceptable as a regression nor dismissable as noise. Two probe binaries
(`paired_ab_medium_{off,on}`), byte-identical source differing only in
the `production` vs `production,medium-classes` Cargo feature set, driven
through `scripts/r10_2_medium_gate.mjs` — 20 A/B/B/A blocks × 3
independently-timed phases (alloc/free/realloc) × 4 launches = **240
fresh process launches**, reusing `scripts/paired-ab-runner.mjs`'s A/B/B/A
+ paired-t-test + sign-test machinery. Results (all statistically
unambiguous, t / sign): **alloc ~31× faster** (t=55.8, 20/20), **free
~211× faster** (t=88.3, 20/20), but **realloc ~2,111× slower** (t=-53.6,
20/20) — the baseline's Large path grows in-place within its dedicated
4 MiB span at near-zero cost, while `medium-classes`' dense packing
forces a move-leg (alloc + memcpy + dealloc) on every cross-class
realloc-grow. This is the wall-clock confirmation of R9-3's `+173.9%` Ir
finding on `realloc_grow`, now decisive instead of ambiguous. The realloc
kill-gate (>20% regression) fires and, per the task's explicit design, is
**not** overruled by the alloc/free wins the way R9-3's noisy run was
overruled by IAI. **Verdict: NO-GO on promoting `medium-classes` into
`production` as-is** (stays opt-in); ships a break-even analysis (~205
reallocs per alloc/free cycle) and three mitigation directions (in-place
medium-class grow, growth headroom, or a documented realloc-light
deployment profile). This is a measured resolution of the ambiguity R9-3
left open, not a contradiction of R9-3's measurements. Independently
re-verified at `--quick` (4 pairs, t=-36.8, 4/4). Full method, raw
numbers, and the break-even table in
[`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`](docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md).

**R10-4 (`fed3d45`) — run-origin oracle design for wide-class alignment,
verdict CONDITIONAL-GO but a strictly superior alternative exists
(DESIGN-ONLY).** Design-only deliverable (the mandatory design-review
gate for correctness-sensitive changes to the cross-thread reclaim path).
Answers whether `carve_block`'s `align_up(bump, block_size)` can be
relaxed to `align_up(bump, class_align)` for the `medium-classes-wide`
1.25/1.5/1.75 MiB classes, recovering R9-4's measured 2/1/1 density to
the theoretical 3/2/2 — and what breaks: the reclaim guard's "offset is a
multiple of `block_size`" defence-in-depth invariant. Full inventory of
**19 `block_size`-multiple assumption sites** (11 unaffected, 4 need a
new guard, 4 are comment/logic updates). Two concrete oracle designs,
both proven at-least-as-safe as the current check: **Oracle A**
(per-segment carved-starts bitmap, strictly stronger, +32 KiB/segment)
and **Oracle B** (per-class run-origin array reusing the already-reserved
second `BinTable` slot, zero new metadata, equivalent containment).
Reclaim-path overhead estimated at +1–3 cycles for wide classes only,
negligible against the ~100-cycle reclaim path. **Verdict:
CONDITIONAL-GO** — technically sound, but the design itself identifies
the page-run layer (R8-9/R9-4's alternative direction) as **strictly
superior**: 3–6× more density (11/9/8 vs 3/2/2) with zero guard breakage
and zero new metadata. Stage 2 (prototype) is **deliberately not
started**; it needs explicit human/roadmap sign-off given the correctness
surface and the identified better alternative — a genuine product
decision, not one this session makes unilaterally. Full inventory and
both designs in
[`docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md`](docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md).

**R10-5 (`fdd360d`) — warm-vs-warm Large-cache-hit gate for 1.5/1.75 MiB
(magnitude correction of R9-4).** Corrects a ~600×-inflated claim from
R9-4. R9-4 framed the 1.5/1.75 MiB classes' density-1× recycle speed as a
"consolation prize" (~90 µs Large recycle → ~60 ns freelist push/pop),
but that ~90 µs was measured against a Large-cache **miss** (full
`VirtualFree`+`VirtualAlloc`), not a **hit** — and `production` keeps the
Large cache active (`OPT-E`, `LARGE_CACHE_SLOTS=8`), which recycles a
warm span via cheap in-process bookkeeping with no syscall. This gate
builds the fair warm-vs-warm comparison: two probe binaries differing
only in Cargo features, working set (`WS_LEN=6`) kept below
`LARGE_CACHE_SLOTS` so the baseline's steady-state allocs provably hit
the warm cache — **proven**, not assumed, via a `large_cache_hits`
diagnostic counter emitted and checked (18012 = `WS_LEN × (ROUNDS +
WARMUP_ROUNDS − 1)`, zero variance across all 40 baseline launches) —
then a 20-pair A/B/B/A wall-clock comparison per size. **Result: the
small path is still faster, but by ~2.3× (76–80 ns → 31–34 ns per
recycle, t=14.7–17.3, sign 20/20), not R9-4's ~1,500×.** R9-4's direction
was right; its magnitude was inflated ~600× by comparing against the
wrong baseline. Recommendation: keep 1.5/1.75 MiB in `medium-classes-wide`
(they still earn a real, statistically unambiguous win on the recycle
axis) and correct R9-4 §2.4's baseline framing to cite the warm-hit
number, not the cache-miss number. Full method, the cache-hit proof gate,
and the corrected numbers in
[`docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`](docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md).

**R10-6 (`cab6573`) — NUMA-aware segment-directory scan cliff, measured
140×, verdict CONDITIONAL-GO (measurement + design, no prototype).**
Measures the O(S) segment-scan cliff that R7/R8's directory work
eliminated for non-NUMA, but which is **still fully present** under
`--features numa-aware` — the directory-driven lookup is compiled out
there, falling back to the two-pass local-first/foreign-fallback linear
scan. Re-ran the existing R7-A0 `segment_directory_sweep` bench under
three matched feature configs on this host (NUMA scan-only vs non-NUMA
directory-ON vs non-NUMA directory-OFF) so ratios cancel host-load drift.
**Measured cliff: 524 ns / 12.8 µs / 69.6 µs at S=64/256/1023 under
`numa-aware`, vs 59 / 160 / 497 ns directory-accelerated — 140× at
S=1023, the same order of magnitude R7 eliminated for non-NUMA.**
Single-node test-host caveat documented explicitly: the measurement is a
**lower bound** (the foreign-fallback pass would only make it worse on
real multi-node hardware). Secondary finding: a fixed ~293 ns
`current_node()` syscall overhead per scan, separate from the directory
cliff, flagged as a cheaper orthogonal fix to evaluate first. Stage 2
(design) ran since the cliff proved significant: two node-aware directory
approaches, recommending Approach A (node-indexed bitmap
`class_nonempty_by_node`, ~49 KiB for `MAX_NODES=8`) over Approach B
(global directory + per-node membership filter, ~7 KiB but more complex
query logic); verified as a **strict extension** of R8-1/R8-2/R9-8's
incremental-sync, authoritative-miss, and drift-recovery machinery — does
not reopen any of it. **Verdict: CONDITIONAL-GO, no prototype this
session.** `numa-aware` is opt-in and lower priority than the
just-completed `medium-classes` workstream; recommends waiting for a real
multi-node user request or a `numa-aware` production-promotion decision.
Measurement-only: no `src/`, `Cargo.toml`, or `tests/` files touched.
Full method, raw bench logs, and the design in
[`docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md`](docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md).

**`6a11c61` — doc-only: bench-table sync + tier-2 unsafe count 33→35.**
Bundles two independent pending doc fixes, both benign: (1) the README
bench tables (churn+write, churn non-writing, cold-direct) and "Honest
verdict" bullets synced to a fresh `npm run bench:table` pass from
earlier this session, alongside a stale line-reference fix in
`scripts/bench-table.mjs` (`benches/global_alloc.rs:460-469` →
`:628-637`, the Churn+teardown diagnostic's real doc-comment location);
(2) the self-verifying unsafe-inventory line updated 17 tier-1 + 33
tier-2 → **35 tier-2** for R10-7's two new item-scoped unsafe sites
(`bump_gen` call sites in the new `HeapCore::alloc_batch`), matching the
DOCS-SYNC precedent. Verified: the canonical
`grep -rnE '^\s*#!?\[allow(unsafe_code)\]' src/ crates/` returns exactly
52 (17 + 35).

**R10-7 (`9611a56`) — tcache-aware batch primitive, verdict GO (reverses
R9-9's CONDITIONAL-NO-GO).** Refutes R9-9's CONDITIONAL-NO-GO, which was
based on an **untested inference** ("even warmed, batch would still be
slower than or comparable to" the real SeferAlloc scalar path) — R9-9
never built a warm-batch arm to check. This task builds it.

- **Part 1 (benches-only):** added two arms to
  `bench_batch_ceiling_followup` on a persistent warm `AllocCore` —
  `batch_core_warm` and a same-substrate `scalar_core_warm` diagnostic.
  Verified `refill_class_bump` drains the warm freelist first (same
  substrate `alloc_small` pops), so a warmed `AllocCore` is genuinely
  warm for the batch primitive — no forwarder needed. **Result:
  warm-batch beats the warm SeferAlloc scalar path by 1.3–3.3× at every
  (size, N) from n=8 to n=1024**, and the pure batch-mechanism win on one
  substrate is 1.5–2.2×. R9-9's inferred sign was wrong at every data
  point.
- **Part 2 (real code, justified by Part 1's numbers):** implemented the
  design a real batch API would ship — `HeapCore::alloc_batch` drains the
  warm per-thread magazine first, batch-refills only the remainder via
  `AllocCore::refill_class_bump_checked` (no block ever parked in the
  magazine); `SeferAlloc::alloc_batch` / `dealloc_batch` wrappers, all
  `#[doc(hidden)]` experimental surface (not committed public API,
  matching R8-7's `refill_class_bump` / `flush_class` precedent). 7
  correctness tests in `tests/batch_tcache.rs` (aliasing, cross-compat
  with scalar dealloc, warm steady-state cycles, null-skip, mixed size
  classes, N > `TCACHE_CAP`). **Measured: beats the real production
  scalar path by 1.1–1.6×, though 1.1–2.2× slower than the
  AllocCore-direct ceiling** (the magazine's per-block bitmap
  bookkeeping and `dealloc_batch`'s un-batched free loop are the
  honestly-documented cost of the realistic path).

**Verdict: GO for the mechanism and the experimental primitive.** The
project's no-committed-public-surface stance is unchanged — promotion
still needs a real consumer and a batch-optimized `dealloc`. This is a
measured reversal of R9-9, not a contradiction of R9-9's data: R9-9
measured a cold-batch / cold-scalar ceiling correctly, but inferred
(without measuring) the warm case; R10-7 measures the warm case and the
inference was wrong. Full method, the warm-arm grid, and the per-(size,
N) numbers in
[`docs/perf/R10_7_BATCH_WARM_ARM.md`](docs/perf/R10_7_BATCH_WARM_ARM.md).

### Round 11 — batch/NUMA correctness fixes, two closed perf cliffs, three design-only stages (R11-1..R11-8)

Round 11 — 8 commits (`33581bd`..`229e25f`, inclusive of both ends),
2026-07-21 (a single day) — the follow-up queue against the **same**
external review that produced Round 10 (that review, read line-by-line
against source immediately after Round 10 landed, surfaced two real
defects in Round 10's own deliverables plus one strong new optimization
idea; the queue below is that review's own prioritized ordering, followed
verbatim). As in Round 9 and Round 10, this is **mostly measurement /
design work, not production hot-path changes** — of the eight commits,
three ship real production-compiled fixes/features (R11-1, R11-2, R11-4),
one is a pure measurement-driven cache (R11-5), one is a mechanical
extension of already-designed machinery (R11-6), and three are
design-only docs with zero `src/`, `Cargo.toml`, or `tests/` changes
(R11-3, R11-7, R11-8). The round's defining trait, distinct from Round
10's self-correction pattern, is an unusually high proportion of **real
bugs caught during zero-trust review before landing**, not after: R11-1's
second, deeper defect (the predicate shortcut) went beyond what the
source review's own prose stated and was found only by the orchestrator's
independent re-analysis; R11-4's missing `hardened` guards and R11-6's
vacuous headline test were both caught by the same personal
red-before/green-after discipline this project's methodology requires
between phases, not by the sub-agents that wrote the code. In every one
of these three cases the pattern is the same — delegate the
implementation, independently re-verify against source and tests, catch
a real gap the delegate's own summary did not surface, fix it, and
personally reproduce red-before/green-after — and it is called out below
per-commit rather than flattened into "and then it was fixed."

**Production vs. opt-in — what actually changed for default `--features
production` users.** The production bundle's feature set is unchanged
this round: no feature is added to or removed from `production`. Three
commits reach production-compiled source:

- **R11-1 (`33581bd`)** — real correctness fix inside the `batch-api`
  experimental surface (see below), plus that surface moves from
  `#[doc(hidden)]`-only to `#[doc(hidden)]` **and** gated behind a new,
  not-default, not-`production` `batch-api` Cargo feature. Zero observable
  change for any build that doesn't opt into `batch-api`.
- **R11-2 (`7ff0772`)** — real correctness fix to `HeapCore::drain_heap_overflow`,
  the cross-thread `HeapOverflow` second-chance ring drain, which is on the
  unconditional cross-thread free path. **Beneficial** behavior change:
  reclaimed blocks now correctly become visible to directory-driven lookups
  and emptied segments now correctly re-enter pool/release accounting; no
  new observable failure mode.
- **R11-4 (`ff9ad7a`)** — adds `HeapCore::dealloc_batch`, a new fast path
  inside the same `batch-api`-gated experimental surface R11-1 covers.
  Not reachable without opting in; the scalar `dealloc` path used by every
  other feature bundle is untouched.
- **R11-5 (`9b48844`)** — adds a `cached_numa_node` field to `AllocCore`,
  compiled and populated only under `--features numa-aware` (an already
  opt-in, non-`production` bundle); zero footprint under non-`numa-aware`
  builds.
- **R11-6 (`89865ae`)** — the `SegmentDirectory`'s node dimension is
  `NODE_BITMAPS == 1` under non-`numa-aware` — byte-for-byte the
  pre-R11-6 flat bitmap, zero memory tax on non-NUMA builds (including
  `production`). Only `numa-aware` builds pay the ~55 KiB and gain the
  112× fix.
- **Everything else is design-only docs:** R11-3, R11-7, R11-8 are
  `docs/perf/*.md` (plus, for R11-3, a throwaway `examples/` probe) with
  no `src/`, `Cargo.toml`, or `tests/` file touched.

**R11-1 (`33581bd`) — close the M2 double-issue window in `alloc_batch`'s
magazine-prefix drain (real correctness fix, plus a second defect found
beyond the source review).** `HeapCore::alloc_batch`'s magazine-drain step
(introduced by R10-7) cleared each block's magazine-residency bit
immediately on pop; the refill-remainder step's predicate opened with an
`if k == c { return false; }` short-circuit copy-pasted verbatim from
`refill_magazine_slow`. That shortcut is sound **only** in
`refill_magazine_slow`'s own context, where its key invariant
(`count[c] == 0` at refill time — nothing of class `c` is claimed yet)
actually holds; `alloc_batch` violates that precondition, since its own
magazine-drain step has already pulled class-`c` blocks into
`out[0..magazine_drained]` before the predicate ever runs. Together: a
stale cross-thread double-free ring entry for a magazine-drained block
sailed through both checks and was re-issued into `out[filled..]`,
producing a **duplicate pointer within one `alloc_batch` call**. A
caller-side double-free is already a contract violation, but the M2
defense-in-depth exists to degrade it to a no-op, not amplify it into a
double issue. The source review's own prose flagged only the first half
(the immediate bit-clear); the second, deeper half — that
`alloc_batch`'s copy-pasted predicate shortcut was itself unsound in its
new context — was found by the orchestrator's own independent
re-analysis before any fix was written, and fed into the implementation
prompt explicitly so the fix would not ship incomplete. Fix, both halves
required together: (1) defer the magazine-residency bit clear to one bulk
pass **after** the refill step completes, so bits stay SET through the
window the predicate needs them; (2) drop the `if k == c` shortcut from
**`alloc_batch`'s own predicate only** — `refill_magazine_slow`'s
closure is untouched, since its invariant is genuinely sound in its own
call context. New counterfactual test
(`alloc_batch_no_duplicate_on_stale_xthread_double_free_entry`) proves
both halves are needed: with the fix reverted, it fails with a duplicate
pointer at the exact magazine/refill boundary — personally reproduced
red-before, confirmed green-after. Also resolves a documentation/
API-boundary gap the same review flagged: `#[doc(hidden)]` hides an item
from rustdoc but not from the semver/ABI surface, so `alloc_batch` /
`dealloc_batch` (on both `HeapCore` and `SeferAlloc`) now additionally
gate behind a new `batch-api` feature, not part of `production` or any
default bundle.

**R11-2 (`7ff0772`) — sync the directory and finalize pool/release
accounting on `HeapOverflow` drain (real correctness fix, two
pre-existing gaps made newly visible by R10-3, not regressions from
R10-3 itself).** `drain_heap_overflow` reclaimed blocks from the
cross-thread second-chance ring but discarded both signals every other
reclaim call site in the codebase already acts on: (1) it never synced
the segment directory, so a block genuinely freed on the `BinTable` via
`HeapOverflow` still read as absent to any directory-driven lookup — up
to **~256 MiB of wasted segment activity per class in the worst case**
before the periodic 256-miss rescan or OOM-rescue recovered it; (2) it
discarded `dec_live_and_maybe_decommit`'s `true` return, the signal to
call `release_or_pool_empty_segment`, leaking emptied segments out of
pool-cap/RSS accounting entirely. Fix is split by safety requirement, not
uniform: directory sync is done **inline**, per successful reclaim — it
only flips already-read directory bits, so it's safe immediately; pool/
release finalization is **deferred** to one bulk pass after the whole
drain completes, because a later ring entry in the same drain pass could
target a base whose metadata an inline `release_or_pool_empty_segment`
call would just have freed or decommitted. Emptied bases are collected
deduplicated into a small fixed-size on-stack array — no heap allocation
anywhere in this path, since it's allocator-internal code and a `Vec`
here would recursively call back into whatever allocator backs it. Two
new counterfactual tests
(`overflow_drain_syncs_segment_directory`,
`overflow_drain_finalizes_emptied_segment`), both proven red-before/
green-after. `sync_directory_for_segment_classes` and
`release_or_pool_empty_segment` are bumped `pub(super)` → `pub(crate)`,
mirroring the exact precedent R10-3 set for
`dec_live_and_maybe_decommit`. Also fixed along the way: a pre-existing
double-free in the directory-sync test's own cleanup path that only
surfaced under `hardened`, and an overstated doc comment mischaracterizing
the dedup array as dead code when it is real defense-in-depth against
`SegmentMeta::dec_live`'s saturating-sub clamping.

**R11-3 (`a3a31da`) — realloc-aware Small→Large promotion design for
`medium-classes`, verdict CONDITIONAL-GO, design-only.** Investigates
recovering R10-2's ~2,111× realloc regression without losing
`medium-classes`' ~31×/~211× alloc/free wins. `HeapCore::realloc`'s move
leg has no in-place fast path for a Small/medium block growing into a
*different*, larger size class, so growing a buffer through the medium
ladder pays a full-buffer copy at every class boundary crossed; the
proposed fix diverts a growing realloc directly into a Large-classified
allocation once it crosses a threshold, so every subsequent growth step
rides the existing OPT-G in-place grow for free instead of paying another
copy. Follows the same two-stage discipline R10-4 established: design and
measure first, prototype only on separate future authorization — no
shipping file is touched. A throwaway measurement harness
(`examples/r11_3_promotion_probe.rs`) gets honest numbers without
modifying `HeapCore::realloc` or `AllocCore`, by reproducing the
diversion's externally observable effect at the call site and verifying
every subsequent realloc hits the real OPT-G in-place-grow path via
pointer-identity assertion. Swept three candidate thresholds (3 runs ×
30 rounds each):

  - 128 KiB: 7→2 move legs, 2059→160 KiB copied, **28.6× faster**
  - 256 KiB: 7→4 move legs, 2059→520 KiB copied, **7.8× faster**
  - 384 KiB: 7→5 move legs, 2059→844 KiB copied, **4.1× faster**

128 KiB gives the biggest win but promotes objects that may never grow
again; 384 KiB leaves most of the win on the table. **256 KiB
recommended** as the balance point. Commit/RSS cost is real and
threshold-invariant (~+116%, 17.6→38.1 MiB for 8 concurrently-live
promoted objects) because it's driven by the pad target crossing a
segment-rounding boundary, not by the threshold itself — flagged as a
separate open tunable for stage 2, not resolved here. Re-ran the existing
R10-2 judge to confirm plain medium alloc/free is unaffected (same order
of magnitude). **Verdict: CONDITIONAL-GO for a dedicated stage-2
prototyping session.** The design shows zero new bookkeeping is needed —
`dealloc`/`realloc` already route purely off `SegmentHeader::kind_at(base)`,
not the caller's `Layout`, so a promoted block becomes an ordinary Large
allocation the instant it's promoted, no new feature flag required. Full
method and numbers in
[`docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`](docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md).

**R11-4 (`ff9ad7a`) — batch-optimize `dealloc_batch`, a real gap caught
and fixed during zero-trust review before landing.** `dealloc_batch`
previously just looped the scalar dealloc path one block at a time,
paying N independent TLS-adjacent lookups and, on magazine overflow, N/8
separate half-flushes each re-deriving the same per-run segment metadata.
The new `HeapCore::dealloc_batch` (`src/registry/heap_core_dealloc_batch.rs`)
classifies the layout once, then for the Small-classified/fastbin case
partitions blocks into this-heap-owned (the same `contains_base`
ownership test the scalar path uses) vs. everything else (foreign,
cross-thread, null), which falls back unchanged to the existing, fully
correct scalar path. Owned blocks fill the magazine directly up to
`TCACHE_CAP`; any overflow routes through one `AllocCore::flush_class`
call instead of the scalar path's dribble of 8-block half-flushes. No M2
guard logic is reimplemented — the fast path calls the identical
`pub(crate)` accessors the scalar path already uses, in the same order.
**Zero-trust review caught a real gap before this landed**: the fast
path's ownership gate (`contains_base`) does not distinguish Small from
Large segments, so it initially omitted two `hardened`-only guards the
scalar path applies before its M2 oracles — F7 (a pointer that actually
lives in a Large segment, freed via a Small-classified layout) and H1 (an
interior, non-block-start pointer). Without them, a caller-contract-
violating free through `dealloc_batch` specifically would read/write a
Large block's own payload bytes as if they were a Small segment's bitmap
under a `hardened` build (part of `--all-features`) — exactly the
corruption F7's own doc comment exists to prevent. Both guards are now
ported in the scalar path's exact order (F7, then H1, then the three M2
oracles), with two new counterfactual tests
(`tests/r11_4_dealloc_batch_hardened_guards.rs`) proving each, personally
reproduced red-before (both guards) and green-after before committing.
Also fixed during the same review: the scalar fallback for non-owned
blocks initially reconstructed a `Layout` from `block_size(c)` with
`align=1` instead of threading the caller's original layout through —
under `alloc-xthread` this could tag a cross-thread free's ring entry
with the wrong class, since `class_for` is alignment-sensitive; fixed by
passing the original layout unchanged. Two more counterfactual tests
cover the base mechanism
(`tests/r11_4_dealloc_batch_same_segment_double_free.rs`,
`tests/r11_4_dealloc_batch_mixed_ownership.rs`), all proven red-before/
green-after. **Measured 1.16×–1.38× faster at the realistic bulk-free
target (n=1024)** across three sizes (release,
`production+alloc-stats+batch-api`); small batches (n=8–32) are noisy/
mixed, as expected, since the fast path's per-block checks only pay off
once `flush_class` batching actually triggers past `TCACHE_CAP`.

**R11-5 (`9b48844`) — cache `current_node()` on `AllocCore`, ~233×/~396×
measured on this host.** `numa::current_node()` was called fresh on every
`find_segment_with_free` miss plus at every new small/large segment
reservation. Its platform implementations are not cheap: Linux loops over
up to 64 candidate NUMA nodes, opening and reading a sysfs cpumap file
for each one; Windows makes two Win32 API calls
(`GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`) — real kernel
transitions either way, paid on every miss rather than once per process.
Adds a `cached_numa_node: Option<u32>` field on `AllocCore`, populated
lazily and consulted from all four hot call sites. Because registry slots
are recycled across different OS threads
(`HeapRegistry::claim`/`recycle`), an unconditional cache would be wrong
— a stale node from a slot's previous owner would silently apply to a new
owning thread for the entire lifetime of its claim. The cache is
invalidated uniformly at `claim()`/`claim_with_config()` time, immediately
before the slot is handed to its new owner; soundness rests on the same
claim/recycle CAS handoff (Release on recycle, Acquire on the next claim)
the registry already establishes, so a plain field write is sufficient —
no extra atomic or fence needed. `docs/PHASE_NUMA_DESIGN.md` gets a new
§4.1 documenting the invalidation policy and the resulting staleness
bound: a migrated thread's reads may now lag the OS's real answer for the
duration of the current slot claim — performance-only staleness, never
UB. New regression test (`tests/numa_cache_invalidation.rs`, gated on a
new test-only `numa-aware-mock` feature) scripts one NUMA node, populates
the cache, recycles the slot, scripts a different node, re-claims, and
asserts the cache is invalidated before any populate — proven
red-before/green-after by temporarily disabling the invalidation call
sites and confirming a stale value leaks across claims. **Measured on
this host (Windows, single-NUMA): ~230ns → ~985ps per call (~233×),
~227µs → ~573ns for a batch of 1024 calls (~396×).** Also fixed along the
way: `numa-shim`'s mock call-log recorder used `borrow_mut()`, which
panics on reentry (the mock's own `Vec::push` allocates via the global
allocator, re-entering `current_node()` when sefer-alloc is the installed
global allocator) — switched to `try_borrow_mut()`, silently dropping
only the reentrant log entry.

**R11-6 (`89865ae`) — node-indexed NUMA segment directory closes the 140×
scan cliff R10-6 measured, verdict implemented (GO), one vacuous test
caught and fixed before landing.** Implements R10-6's already-designed
Approach A now that R10-6's own GO trigger ("cache shipped AND cliff
still dominant") is satisfied by R11-5. `SegmentDirectory` gains an outer
node dimension, `class_nonempty_by_node[bucket][class][word]`; under
non-`numa-aware`, `NODE_BITMAPS == 1` — byte-for-byte the pre-R11-6 flat
bitmap, zero memory tax. Under `numa-aware`, `NODE_BITMAPS == MAX_NODES +
1` (8 + 1 for a dedicated "unknown node" bucket, ~55 KiB total). The
directory-driven lookup, previously compiled out entirely under
`numa-aware`, is now unconditional, scanning buckets in
local → unknown → foreign-ascending order — preserving the two-pass
local-first/foreign-fallback preference the R7 plan binds as a hard
constraint. Candidate validation was extracted into one shared
`validate_directory_candidate` choke point so NUMA and non-NUMA scans use
byte-for-byte identical criteria; R8-2's authoritative-miss trust and
R9-8's streak/rescue-scan machinery are untouched — only the directory
block's own cfg gate changed. **Zero-trust review caught a real gap
before this landed**: the headline correctness test (local-first/
foreign-fallback preservation) passed even against a **deliberately
broken** bucket-scan order. Root cause: `alloc_small` tries
`pop_free(self.small_cur)` before ever calling the directory-scan
function, and the test's construction left `small_cur`'s final state to
chance — the decisive alloc call most likely resolved via that fast
path, never touching the directory the test claimed to exercise, making
the test vacuous with respect to what it claimed to prove. Fixed by
adding a `#[doc(hidden)]` test hook that calls the directory-scan
function directly, bypassing `alloc_small`'s fast path entirely, so the
directory's bucket order is unconditionally the deciding factor.
Personally reproduced red-before (revert the bucket order to naive
descending — the fixed test now correctly fails, returning the foreign
node instead of local) and green-after before committing. **Re-measured
fresh** (`benches/segment_directory_sweep.rs`, same harness R10-6 used):

  - S=64:    284ns → 72ns    (3.9×)
  - S=256:   12,218ns → 176ns (69×)
  - S=1023:  62,866ns → 560ns (112×)

The curve is now flat in S (O(1)), matching the non-NUMA directory-
accelerated numbers plus the R11-5 cached `current_node()` residual. The
cliff is closed.

**R11-7 (`c22807d`) — page-run layer design for the 1.25–2 MiB density
gap, verdict CONDITIONAL-GO, design-only, the largest structural change
in the queue.** R10-4's own design sketched a "page-run layer" — a
larger, dedicated arena for the `medium-classes-wide` 1.25/1.5/1.75 MiB
range — as the real long-term fix for the density gap R8-9, R9-4, and
R10-4 all independently found (these classes pack near-1× in a standard
4 MiB segment). Follows the same two-stage discipline as R10-4 and
R11-3: design-doc first, prototype only on separate future
authorization. Re-verifies the density win against real constants read
fresh this session: an 8 MiB arena delivers density **5/4/3/3** for
1.25/1.5/1.75/2.0 MiB (vs today's 2/1/1/1 — 2 MiB isn't a class today,
R9-4 explicitly excluded it for exactly this reason); a 16 MiB arena
delivers 11/9/8/7 but doubles per-arena commit cost for a workload that
may only populate a few blocks. **Recommends the single fixed 8 MiB
arena** over both the larger 16 MiB option and per-class-tier arena
sizing. Does the exhaustive due diligence the task required: every
`SegmentKind::` call site inventoried and classified (**44 matches across
17 files**), and a systematic interaction check against every
prior-session mechanism that assumes segment-uniformity (M2 bitmaps,
`RemoteFreeRing`/`HeapOverflow`, the R7–R11-6 segment directory, the NUMA
node-indexed directory, the decommit large-cache and empty-segment
pool) — **6 of 11 need a genuinely new parallel mechanism, only 2 are
reused as-is.** Corrects R10-4's own one-line framing that the page-run
layer needs "zero guard-invariant changes": true for carve alignment, but
address resolution needs a second masking constant and a two-step
disambiguation, since `segment_base_of_ptr`'s O(1) masking is calibrated
to the global `SEGMENT` constant — worked through concretely (a parallel
`PageRunTable`, a dedicated `PageRunFreeRing` with a wider packed-offset
field: 8 MiB needs 23 bits vs `SEGMENT`'s 22, confirmed to still fit the
existing `u32` packing with headroom, though the `hardened` ring's exact
bit budget is flagged as not fully derived — an explicit open item for
stage 2, not silently assumed to work). **Verdict: CONDITIONAL-GO**, but
explicitly states the true design surface is closer to "a second,
smaller segment-table subsystem living alongside the existing one" than
a bounded patch — roughly 2–3× the correctness-surface size of R10-4's or
R11-3's own designs, comparable in total scope to the original
`medium-classes` build-out, not a single-session task. Six explicit open
questions left for a future stage-2 session, not silently assumed. Full
inventory and interaction table in
[`docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md`](docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md).

**R11-8 (`229e25f`) — virgin-zero skip for Small `alloc_zeroed`,
independent re-verification of a prior session's design, verdict
unchanged (CONDITIONAL-GO), design-only.** Deliberately ordered last by
the source review ("big potential, but harder to prove") — this is
correctness-critical in a way none of the round's other perf work is: a
wrong implementation would return **uninitialized memory** from
`alloc_zeroed`, a direct `GlobalAlloc` contract violation with
security-relevant implications, not merely a regression. Discovered
before writing anything: this exact topic already has a complete,
committed design doc from an earlier session
(`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md`, commit `7bdbc0f`,
2026-07-20), itself a reconciliation of an earlier NO-GO and a
deep-audit's "medium risk, unresolved" rating, reaching a design-only
CONDITIONAL-GO. Rather than silently duplicate or blindly trust that
prior work, this task's own doc
(`docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md`) **independently
re-verifies R9-5's conclusions from the current tree** — re-reading every
cited call site fresh rather than trusting citations (the substrate has
moved since R9-5: `alloc_core_small.rs` grew from ~2100 to 2267 lines) —
and adds what R9-5 didn't produce: a formal four-conjunct testable
predicate (`is_virgin(segment, offset, carve) :=` dispatch-is-fresh-carve
AND offset-in-this-carve's-range AND
segment-never-decommit-recommitted AND `not(miri)`), a full verification
ledger tracing five preliminary hypotheses against source, and this
round's kill-gate table format. **The verdict does not differ from
R9-5's**: all ten correctness/soundness kill-gate criteria pass (pooling
can never be marked virgin, lazy-commit grow-on-carve is
zero-guaranteed, decommit-in-place has zero production callers today and
the design sets its tracking bit defensively so a future reintroduction
fails safe, batched carve and magazine-refill interleaving both get
correct per-run granularity, cross-thread frees never write block bytes,
the `hardened` generation-bump writes disjoint metadata not the payload,
Miri safety mirrors the Large-path's proven `cfg!(not(miri))` fix
exactly) — **CONDITIONAL-GO for staged future work, NO-GO for a
same-session prototype.** The CONDITIONAL qualifier is entirely about
production reach and win narrowness, not correctness:
`HeapCore::alloc_zeroed`'s small arm still bypasses
`AllocCore::alloc_zeroed` entirely (calls `self.alloc` + unconditional
`Node::zero`), so a substrate-only prototype would be fully testable but
production-inert under any `production`/`fastbin` build — the real
implementation surface is plumbing the virgin signal through the
magazine refill path, which has a genuinely open storage-design question
(per-slot bool array vs. a whole-class short-circuit bit vs. a stolen
pointer tag, especially its interaction with `hardened`'s tagged-pointer
scheme) that neither this document nor R9-5 resolves. The win itself is
real but narrow: benefits only genuinely-first-touch,
never-reused `alloc_zeroed` calls, zero benefit on the steady-state churn
patterns the rest of this round's work targets.

0.3.0 is the first `0.3.x` release (the current crates.io live version is
`0.2.1`; see the yank notes below). It bundles four workstreams, each
implemented with line-by-line zero-trust review, per-fix counterfactual
verification, and a commit between phases: the **P0–P7 perf arc**
(#144–#163, beat `mimalloc` on small/medium), a **reliability, stress &
release-doc pass** (R1–R4 / S1–S3 / D1, #153–#168), **two post-tag review
passes** (#164–#178 — a hardening/H1 pass then a perf/reliability/CI pass
W1–W6, both driven by fresh `/fxx` audits with per-fix counterfactuals), the
**post-review hardening pass** (#129–#143), and the **initial phase A–F pass**.
Sections below are grouped per workstream.

### Performance & correctness — the X-arc (#182–#188, 2026-07-05/06)

The post-W7 arc that attacked the last "cardinal" costs found by a fresh
audit. Judge-driven end to end: every change was measured by the
deterministic callgrind judge (`npm run iai`) against a pinned reference
table, adversarially reviewed, and either kept with numbers or
honest-rejected with numbers (four experiments were rejected — the ledger in
[`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md) records all
tables so no experiment is re-run blind).

- **X1 — OPT-G in-place Large→Large realloc growth (#182).** When the grown
  size (clamped to `MIN_BLOCK`, symmetric with the #138 consistency check)
  still fits the segment's committed `span_usable`, `realloc` updates the
  header's `large_size` and returns the SAME pointer — zero alloc/copy/
  dealloc. Large reservations round up to whole 4 MiB segments and `vmem`
  commits the entire span, so growth cannot fault; `dealloc` routes Large
  frees by segment kind, so the grown block frees correctly. Shrinks still
  take the slow path (RSS reclaim preserved). An adversarial review caught
  (and a counterfactual test now pins) a MIN_BLOCK-clamp leak the first cut
  had. `realloc_grow`: **1,520,714 → 617,859 Ir**.
- **X2 — #164 narrowed: drain-side magazine check (#183).** The ring↔magazine
  cross-thread double-free residual was closed on its *in-magazine leg*: the
  owner's ring drain now consults an `is_in_magazine` predicate (generic
  closure threaded from `HeapCore` via split borrows) immediately before
  linking, on ALL production drains — refill-miss, the realloc alloc-leg
  (rerouted through the magazine-aware `HeapCore::alloc`; the blind path was
  found by adversarial review), and the dbg seam. A magazine-resident block's
  ring entry is dropped; the magazine copy stays canonical. The *re-issue-
  before-drain* leg is **proven** information-theoretically indistinguishable
  from a delayed genuine cross-thread free (design doc §8 impossibility
  postscript) — full closure needs generational ring entries (X7, hardened,
  future arc). Costs accepted and documented: +~630 Ir one-time bootstrap
  per heap claim, ~+30 Ir per refill-miss; hot magazine push/pop untouched.
  Bonus: `realloc_grow` → **561,912 Ir** (the alloc-leg now hits the
  magazine). loom green model + two new counterfactual regression tests.
  - **Correction (R1, 2026-07-06):** the X2 fix as originally shipped left a
    SECOND, decidable leg open — the **refill-window in-out-buffer** leg.
    `refill_class_bump_impl` pulls freelist blocks into `out[0..filled]`
    BEFORE draining rings; the predicate's `if k == c { return false; }`
    shortcut (justified only by count[c]==0 borrow-safety) was blind to those
    magazine-destined blocks, so a stale ring note was reclaimed → relinked →
    the SAME refill loop re-pulled the block → double-issue at consecutive
    positions. Task R1 closed it by wrapping the predicate with an
    out-membership guard (`is_in_magazine(ptr,k) || (k == c &&
    out[..filled].contains(ptr))`) — zero cost when the ring is empty.
    Counterfactual regression test:
    `refill_window_does_not_double_issue_in_out_buffer_resident_block`
    (reverting the guard → P double-issued at positions [14, 15]). The §8
    impossibility theorem is now correctly scoped to leg 3 only (re-issue-
    before-drain); the taxonomy is three legs, not two.
  - **Cleanup (R2, 2026-07-06):** the X-arc retrospective (C2) flagged
    `AllocCore::realloc` as production-dead yet carrying a full duplicate of
    the OPT-F/OPT-G in-place logic also present in `try_realloc_inplace` —
    an unmarked divergence hazard. Resolved by extracting the shared
    detection into one private helper, `realloc_inplace_fast_path`, called
    by both `AllocCore::realloc` (substrate-level fallback to its own
    alloc+copy+dealloc) and `try_realloc_inplace` (the `alloc-global`-gated
    thin wrapper `HeapCore::realloc` consumes). Single source of truth; no
    behaviour change. The same pass rewrote `HeapCore::realloc`'s doc
    comment, which still described the pre-#164 "delegate to
    `self.core.realloc`" flow, to match the actual body (try_realloc_inplace
    → `HeapCore::alloc` + copy + `HeapCore::dealloc`), and replaced a dead
    `if p != ptr { stamp }` branch (unreachable: `try_realloc_inplace`
    always returns the same pointer) with a `debug_assert_eq!`. MUST-1/A1
    and #169 stamp semantics unchanged; both invariant-guarding suites
    (`regression_realloc_xthread_stamp`, `regression_inplace_large_realloc`)
    stayed green without assertion edits.
- **X3 — judge upgrade (#184).** `scripts/iai.mjs` now surfaces the full
  callgrind metric set (Ir | L1 | L2 | RAM | Estimated Cycles) — Ir counts a
  `udiv` and a cache-missing load identically, cycles do not; the X-arc's own
  memcpy story is the proof (realloc_grow Ir −63% but cycles −47% with RAM
  hits 92,240 → 74,963). New `multiseg_cold_256k` bench (3-segment scan
  judge, seeded for future segment-queue work). `docs/perf/FAULT_PROBE.md`
  records the honest negative verdict on a WSL2 page-fault judge.
- **X4/X5/X6 — four honest-rejects with full tables (#185–#187).**
  Magazine CAP 16→32 (every bench regressed, recycle +32,305 — the target
  itself); a 64-bit bloom gating the M2 in-magazine scan (recycle −19k but
  churn +980 — the won front is not traded); clz `class_for` vs the 16 KiB
  SIZE2CLASS LUT (bitwise-identical over 8.28M pairs, but Estimated Cycles
  regressed on 10/11 benches); a per-segment free-classes bitmap for the
  segment scan (every bench regressed incl. the designated judge). All four
  experiments' mechanisms and revisit-triggers are in the ledger.
- **X-arc headline:** `realloc_grow` **1,520,714 → 561,912 Ir (−63 %)** and
  **7,206,236 → 3,817,567 Estimated Cycles (−47 %)**; all other benches within
  documented cold constants of their pre-arc values; every M2/D1 guarantee
  intact and one double-free leg newly closed. X7 (hardened generational ring
  entries — the only path to the remaining, proven-undetectable double-free
  leg) landed as a follow-up arc; see the "X7" subsection below.

### Hardening — the X7 generational-ring arc (#188–#193, 2026-07-06)

The X-arc closed the *in-magazine* and *refill-window* legs of the cross-thread
double-free residual (X2 #164, R1). The third and final leg — *re-issue-before-
drain* (a block popped from the magazine and re-issued before the owner's lazy
drain catches a stale cross-thread-free note) — is information-theoretically
indistinguishable from a genuine delayed free on the bare `GlobalAlloc`
interface. X7 closes it under `--features hardened` via a per-granule
generation counter: the ring note now carries the block's generation at
remote-free time, and the drain drops a note whose generation no longer matches
the block's current life. Delivered in five phases (Ф1–Ф5), each committed
between phases with a zero-trust review and a production-judge gate.

- **Ф1 (`cdc3361`, #189) — gen table in segment metadata.** A 256 KiB table of
  `AtomicU8` (one byte per `MIN_BLOCK = 16` granule, `#[cfg(feature =
  "hardened")]`-gated) carved into the segment metadata region, below
  `small_meta_end`. Not decommitted with the payload → numbering is continuous
  across decommit-reset; dies only with full segment release. Byte-level
  `gen_at`/`bump_gen` accessors (Relaxed load / `fetch_add(1, Relaxed)`). Miri-
  clean (exposed-provenance standalone-buffer tests). Production-judge 11/11
  byte-identical.
- **Ф2 (`345a2ce`, #190) — hardened ring-entry repack.** The ring's `u32` slot
  entry repacks under hardened to `[gen:8|class:6|off16:18]` (was
  `[off:22|class:10]`). Const-asserts pin the bit layout (sum == 32, gen == 8);
  the `RING_SLOT_EMPTY = u32::MAX` non-collision is structurally guaranteed
  (`class=63` is unreachable: `SMALL_CLASS_COUNT = 49 < 64`). Round-trip +
  field-independence + misalignment-guard regression tests. Non-hardened path
  byte-identical.
- **Ф3 (`d1e91ff`, #191) — the three touches.** (a) issue bumps the gen
  (`bump_gen` at magazine pop + `pop_free`); (b) remote free stamps the current
  gen into the note (`dealloc_routing` Variant-2); (c) drain compares, AFTER all
  existing guards, BEFORE `write_next`: mismatch ⇒ drop. The pinned-red
  `#[ignore]` test `residual_xthread_double_free_no_corruption` (scenario
  A→B→I→D) turns GREEN under `hardened` — the pinned bug becomes the feature
  proof. loom model + `should_panic` counterfactual; production-judge 11/11
  byte-identical.
- **Ф4 (`3b0ed2c`, #192) — lifecycle-seam tests.** Pins the three seams the gen
  table touches: (1) decommit-reset continuity (the table is NOT re-zeroed —
  numbering persists; fresh segments ARE zeroed by `init_gen_table_in_place`);
  (2) recycle/release drops stale notes via the EXISTING `contains_base`/
  `magic_at` guards (the gen table is unmapped before any post-recycle read);
  (3) adopt/abandon — the table travels with the segment unchanged (`abandon`
  touches only `owner_state`, never metadata bytes).
- **Ф5 (#193) — honest costs, wrap boundary, docs sync, final runs.** This
  phase. (a) Published the hardened-tier cost in
  [`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md): marginal per-op
  cost is **+0.2–0.8% Ir** on the magazine hot path (the per-issue `bump_gen`
  RMW), **+2.6%** on refill-miss paths, plus a one-time **~262k Ir bootstrap**
  per heap-claim (gen-table zeroing) — the published price of the defence-in-
  depth feature (plan §5: "порога 'не хуже' нет — это осознанная плата за
  защиту"). (b) Wrap-1/256 boundary test
  (`tests/regression_gen_wrap_boundary.rs`): pins the EXACT 256-modulus of the
  accepted residual — `stamped_gen == current_gen` is TRUE at k=256 bumps
  (collision), FALSE at k=255/257, const-derived from `ENTRY_GEN_BITS == 8`.
  (c) Docs sync: `DURABILITY.md` (+gen counter inventory row, accepted-residual
  verdict category), `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.4 (→
  IMPLEMENTED), `FASTBIN_DESIGN.md` residual banner (→ CLOSED under hardened).
  (d) Final loom/miri runs green across both profiles; TSan deferred to CI on
  push (Linux-only, not runnable on the Windows dev host).

**Residual after X7:** leg 3 (re-issue-before-drain) is closed under
`--features hardened`. The only remaining leak is the **1/256 wrap** (≥256
re-issues of one block without an intervening drain → the stamped gen
coincidentally matches the current gen mod 256) — an accepted probabilistic
residual by design (plan §2.5 rejected doubling the ring footprint for a `u64`
note), pinned to its exact modulus by the Ф5 boundary test. The production hot
path is byte-for-byte untouched (every X7 code path is behind the hardened
cfg). Full phased account:
[`docs/design/X7_GENERATIONAL_RING_PLAN.md`](docs/design/X7_GENERATIONAL_RING_PLAN.md).

### Performance — the P0–P7 "beat mimalloc on small/medium" arc (#144–#163)

A seven-phase perf campaign against `mimalloc` on the two fronts where 0.3.0
lost: cold first-touch of tiny blocks (16–64 B) and 256 B churn. The governing
rule was **every speedup removes a *tautology*, never a *guard*** — no
correctness guarantee was surrendered (M2 exact double/foreign-free no-op, D1
live-count accuracy, A1 cross-thread reclaim, `#![forbid(unsafe_code)]` by
default with `production` = `#![deny(unsafe_code)]` + 8 named seams — all
intact — M2's exact-no-op scope being the live/mapped,
single-legged free, with the cross-thread-double-free ring-in-flight case a
pre-existing documented residual limit, #164); in P6 the M2 guard was
**strengthened for the two own-thread resting places** (magazine + BinTable,
see Э6 below). Each phase was implemented, line-by-line zero-trust reviewed,
counterfactually verified, and committed between phases. See
[`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`](docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md)
for the full diagnosis and
[`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md) for the P0→P5 measurement tables.

The six eurekas that landed (P1–P3, P6):

- **Э1 (P3) — bump-direct batched carve — front A's main lever (#147).** A
  freshly bump-carved block already satisfies the M2 bitmap invariant
  (`bit 0 = allocated`); the old refill drove every virgin block on a
  `carve → write_next → bitmap RMW → head-store → pop → read_next → bitmap RMW`
  round-trip through the `BinTable` only to move it to "free" and instantly
  back to "allocated" — a tautology (~40 instructions/block). New
  `AllocCore::refill_class_bump` carves a batch straight from the bump cursor
  into the magazine (`bump += n·block_size`, `live_count += n`) **without
  touching the bitmap** (bit 0 is already correct), ~6–8 instructions/block.
  Source order preserved: freelist / cross-thread ring-drain are still tried
  BEFORE bump-carve, so freed blocks never go stale (no RSS drift). M2
  byte-identical (a double-free of such a block still `mark_free`s, and the
  second free still sees "already free" → no-op); D1 exact (same batch inc).
  The P7 alloc-side bulk-bypass became unnecessary and was retired (the
  dealloc-side bulk-flush is kept). This roughly halved the cold tiny-block
  gap and brought cold 256 B to parity.
- **Э2 (P1) — one-branch teardown resolver (#145).** After #129 every alloc
  compared `p == TORN` (`usize::MAX`) and `p == null` (`0`) — two branches on
  the process's hottest path for a once-per-thread teardown case. Since the
  two sentinels are the range ends, one compare
  (`p.addr().wrapping_sub(1) < usize::MAX − 1`) catches both; the cold split
  (`0 → bind_slow`, `MAX → Fallback`) only runs off the fast path. Semantics
  identical (same #129 counterfactual test), minus a branch.
- **Э4 (P1) — classify once (#145).** `class_for` was recomputed 2–3× per
  alloc and 2× per free; the class `c` (a pure function of size+align) is now
  threaded once through the path (the magazine miss resolves `c` and hands it
  straight to `refill_class_bump(c, …)`; the dealloc overflow resolves `c` once
  and passes it to `flush_class` / `dealloc_small(base, ptr, c)`), removing 1–2
  loads from the 16 KiB `SIZE2CLASS` table plus branches per op. (P1 introduced
  thin `alloc_small_class` / `dealloc_small_class` wrappers for the bulk-bypass
  callers; P3 retired those wrappers with the P7 bypass, but the classify-once
  threading they enabled survives on the live refill/dealloc paths.)
- **Э5 (P1) — a counter that doesn't count (#145).** The per-hit
  `tcache_hits.fetch_add` was a `lock xadd` even after #133 removed the
  *contention* (the owner is the sole writer). Replaced with a
  `load(Relaxed); store(+1, Relaxed)` pair — same atomic visibility for
  `stats()`, no lock prefix. TSan/miri-clean.
- **Exact 256 B size class (P1, #145).** `SMALL_CLASS_COUNT` 48 → 49 adds an
  exact-256 B class (the public size-class type has been a `&'static [..]`
  slice since #136, so this is not a breaking change). This narrows — but does
  not close — the 256 B churn gap.
- **Э6 (P6) — oracle-in-metadata: the 256 B churn loss is ELIMINATED, and M2
  got STRONGER (#150–#152).** The P5 docs blamed the residual 256 B loss on
  "the M2 bitmap price"; that framing was incomplete. The real cost was a
  stale per-heap key (`TCACHE_KEY`) stamped into the freed block's **body**
  (word1) and read back as a magazine double-free fast-path filter. On the
  non-writing churn bench the key survived across the free, forcing a
  slow-path scan on every free AND touching a cold/conflict cache line at the
  256 B stride (the "256 B churn loss" — never the bitmap itself). Э6 removes
  `TCACHE_KEY` entirely: the two exact oracles (in-magazine array scan + the
  `BinTable` `is_free` bitmap line — both hot metadata) now run on every free
  with no block-body filter, and **the free path never touches the block
  body**. This is not a trade — M2 is **strengthened for the two own-thread
  resting places (magazine + BinTable)**: the pre-Э6 flushed-double-free-
  after-user-write hole (a double-free after the user overwrote word1 could
  double-issue) is now CLOSED, because the oracle no longer depends on
  block-body contents. **The cross-thread-double-free ring-in-flight case
  remains a documented residual limit (#164):** the oracles are blind to a
  block whose cross-thread free is still undrained in its segment's
  `RemoteFreeRing` (the ring push sets neither oracle), so an own-thread free of
  such a block still slips through — pre-existing since fastbin, neither opened
  nor closed by Э6, pinned RED by
  `tests/regression_xthread_double_free_residual.rs`. Counterfactual proof: `tests/regression_magazine_oracles.rs`
  test (c) is RED pre-Э6, GREEN on Э6. Bonus: our free path is now cheaper than
  mimalloc's on this pattern — mimalloc writes `next` into the block body on
  every free; we write nothing to it. Cold carve is untouched (Э6 targets only
  the churn free path).

The P7 arc (P7.0–P7.4, #159–#163) — an **instruction-count** optimization of
the steady-state cold recycle path (the freelist round-trip P7.0 isolated —
NOT page faults; at criterion steady state the instance is reused, so the cost
is per-block metadata ceremony on the refill/flush path). Five more eurekas,
each proven **byte-identical** by counterfactual regression tests:

- **Э7 (P7.2) — batch freelist drain in `refill_class_bump`, the main cold
  lever (#161).** One segment's freelist is drained in a **single walk**: the
  head-read, `set_head`, and `inc_live` are hoisted out of the per-block loop
  (one head-store + one live-count update for the whole run). The genuinely
  per-block work stays per block: the dependent `read_next` load and the
  `mark_alloc` bitmap RMW (the M2/D1 guards) still run once per block. The
  drained blocks are byte-identical to the per-block loop's output.
- **Э8 (P7.3) — batch flush in `flush_class` (#162).** Symmetric on the dealloc
  side: same-segment runs flush in one pass with `set_head` and the bump-load
  hoisted out of the loop. Every guard stays per block: `is_free`, `off >= bump`,
  and `dec_live` all still run once per flushed block — no guard collapsed,
  only shared head/bump bookkeeping pulled out.
- **Э9 (P7.1) — classify-once + base-once on the `HeapCore` alloc/free faces
  (#160).** A duplicate `class_for` and `segment_base_of` per op were removed —
  both are resolved once and threaded through. Same values, fewer loads; both
  sides win, risk ~0.
- **Э10 (P7.4) — branchless chunked in-magazine M2 scan (#163).** The
  in-magazine double-free oracle (the Э6 array scan) is now a branchless
  chunked scan — same exact membership test, no per-element branch. M2
  membership is byte-identical; the scan bounds are counterfactually pinned.
- **Э11 (P7.2) — stamp-dedupe (#161).** A redundant owner-stamp on the batched
  drain path was de-duplicated (stamped once for the drained run, not per
  block). Same stamp result.

Э3 (P2, own-segment cache) was implemented and gated but is honestly modest
(the win is skipping the probe arithmetic + a likely L1 miss; `contains_base`
was already O(1)); it does not move the headline tables.

### Measured result (single noisy Windows dev host, criterion FAST profile — ratios are the signal)

- **Cold tiny blocks (front A) — the big win.** 16 B `2.6× → 1.60× slower`;
  64 B `2.0× → 1.15× slower`; cold 256 B reached **parity** (1.03×). Not full parity
  on the tiniest cold sizes, but the tautological carve→BinTable→pop round-trip
  is gone — what remains is honest per-block work (page-map writes, page faults
  on genuinely fresh pages).
- **Churn tiny blocks — lead widened.** 16 B `1.26× → 1.63× faster`; 64 B
  `1.23× → 1.69× faster` (Э2 + Э4 + Э5 compounding on the hit path).
- **256 B churn (front B) — the loss is ELIMINATED (Э6, P6).** Through P5 the
  exact-256 B class only narrowed this from `1.25× → 1.16× slower` and never
  overtook. Э6 removed the real cause (the stale block-body key, not the
  bitmap): on the artificial **non-writing** pattern 256 B churn reached
  **≈ parity** (`~1.03×`, was 1.16–1.25× SLOWER), and on the realistic
  **writing** pattern (`global_alloc_churn_write`, new in P6.0 — real code
  writes to what it allocates) sefer-alloc now **leads at every size**:
  16 B 1.63×, 64 B 1.69×, **256 B 1.14× faster**, 1024 B 5.42× faster. The
  earlier "honest ceiling" framing (256 B is the M2 bitmap price) is retired —
  the price was a per-heap key in the block body, and it is gone.
- **Cold tiny (16–64 B) — unchanged, still trails 1.15–1.60×.** Э6 does not
  touch the cold carve path (page-fault-bound honest per-block work); no claim
  of improvement there.
- **Large (≥1 KiB) — the crushing lead is retained.** Cold 1.84× faster,
  churn 5.42× faster (writing) / retained; the OPT-E large-cache headline
  (13–34× at 4/16/64 MiB) is unchanged.
- **P7 cold recycle — an instruction-count reduction; wall-clock MODEST and
  within noise on this host (no overclaim).** P7 batches the freelist
  drain/flush (Э7/Э8), classifies once (Э9), and makes the M2 scan branchless
  (Э10) on the steady-state cold recycle path. On this noisy single-host
  wall-clock the cold-tiny numbers moved only within run-to-run noise: 16 B
  `1.60× → ~1.5× slower`, cold 256 B `parity → ~1.06× faster`, 64 B unchanged
  (`~1.15×`) — the 16 B row alone spanned 18–24 µs across samples. **We do NOT
  claim the plan's projected ~1.1–1.2× cold-tiny figure as achieved** — the
  wall-clock on this machine cannot cleanly resolve the per-op instruction
  savings. The real, DETERMINISTIC proof is the iai `Ir` gate on Linux CI (see
  the `recycle_*` benches below); the P7 cold verdict is **pending that gate**.
  (Resolved: `.github/workflows/perf-gate.yml`, task #127/#128, now runs this
  exact Ir gate on `ubuntu-latest` — see R18-7's status doc,
  `docs/perf/R18_7_MIMALLOC_GAP_STATUS.md`.)
  Churn (the won front) is **UNREGRESSED** (16 B still ~1.6× faster, 256 B
  still ≈ parity). Guarantees intact: the batching removed only shared-
  bookkeeping tautologies and kept every per-block guard (`is_free`,
  `off >= bump`, `mark_alloc`, `dec_live`); M2 / D1 / A1 /
  `#![forbid(unsafe_code)]` by default (`production` = `#![deny(unsafe_code)]`
  + 8 named seams) all hold.

The rigorous, DETERMINISTIC proof is the `perf_gate_iai` instruction-count
gate (Valgrind, Linux-only CI): the P0 benches
(`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`, #144), the P6
`churn_write_256b` bench (#150), and the P7.0 two-round
`recycle_alloc_free_256x16b` / `_256x64b` benches (#159 — round 2 drains what
round 1 freed, isolating exactly the Э7/Э8 recycle path the single-round
`cold_*` benches are blind to) exist for exactly this and confirm the per-op
`Ir` deltas; their `Ir` baseline is captured on the first Linux perf-gate run.
The P7 cold verdict specifically is **pending this Linux Ir gate** — the
wall-clock numbers above are noisy comparative measurements from a single
noisy Windows dev host, not a statistical suite. (Resolved:
`.github/workflows/perf-gate.yml`, task #127/#128, now runs this exact Ir gate
on `ubuntu-latest` — see R18-7's status doc,
`docs/perf/R18_7_MIMALLOC_GAP_STATUS.md`.)

### Reliability, stress & release-doc pass (R1–R4, S1–S3, D1 — #153–#168)

A post-perf pass that hardens the guarantees, adds adversarial boundary
coverage, and reconciles the release docs — strictly from the safe
`GlobalAlloc` envelope (each block freed exactly once, same layout; misuse
from `unsafe` callers is out of scope). No correctness guarantee was
weakened; M2 was *strengthened* in R1.

#### Fixed

- **R1 — the magazine-push `off >= bump` guard closes a real M2 gap.** The
  Э6 in-magazine free path could push a not-yet-carved (`off >= bump`)
  offset into the per-thread magazine, from which a later alloc could hand
  out a block the substrate never carved. The push now rejects any
  `off >= bump` offset (byte-identical to the flush-side guard).
  Counterfactual-pinned by `tests/regression_magazine_bump_guard.rs` (RED
  without the guard).

#### Changed — honesty of the M2 scope

- **R2 — the ring↔magazine cross-thread double-free residual is documented,
  pinned, and modelled (real fix tracked as #164).** A block whose
  cross-thread free is still in-flight in a segment's `RemoteFreeRing` (not
  yet drained by the owner) sets neither own-thread oracle (it is in neither
  the magazine `slots` scan nor the `BinTable` `is_free` bitmap), so a
  concurrent own-thread double-free of it is not detected. This is a
  pre-existing limit (present in the live 0.2.1 `fastbin` too), NOT
  introduced by the perf arc. Pinned by
  `tests/regression_xthread_double_free_residual.rs` (`#[ignore]`), modelled
  by `tests/loom_magazine_ring_compose.rs` (loom also showed the naive
  "own-free reads the ring" fix is itself holed — the real fix must let the
  drain see the magazine, hence #164). `docs/INVARIANTS.md` / README now
  qualify "never UB" to live/mapped memory and reference this residual.

#### Internal — verification

- **R3 — `production` is now covered by sanitizers in CI:** a ThreadSanitizer
  job on the `production` feature set plus `miri` over the `fastbin` magazine
  tests (and loom variants). Zero races, zero UB.
- **R4 — code-doc hygiene:** stale `40`→`49` size-class counts, the slot-0
  FIFO wording, the unsafe-seams comment, and stale `realloc` / no-`Box`-on-
  bind notes corrected across the substrate source.
- **S1 — bounded concurrent boundary-stress harness**
  (`tests/stress_concurrent_boundaries.rs`): multi-thread hammering of the
  class / align / segment seams with allocation canaries + distinctness +
  M2/D1 assertions, all from the safe envelope. Bounded to ~1 s by default; a
  heavier run is opt-in via `SEFER_STRESS_HEAVY` / `SEFER_STRESS_OPS` /
  `SEFER_STRESS_MAX_THREADS`.
- **S2 — deterministic single-thread exhaustive boundary sweep**
  (`tests/stress_boundary_sweep.rs`): every class/align seam × a realloc
  matrix (~2100 cases in ~0.5 s; the grid auto-reduces under `cfg(miri)`).
- **S3 — the stress harnesses run under sanitizers in CI:** S1 under TSan,
  S2 under miri, with reduced budgets so CI stays fast. Neither S1/S2 nor the
  sanitizers found any new bug.
- **D1 — release-doc accuracy pass** (docs-only): the unsafe-seam inventory
  (+`registry::bootstrap`), the M2 scope, purged env-vars, `production` =
  `+fastbin`, the 1024-segment-ceiling reframe, and every verification
  counter were reconciled against verified ground truth before the tag.

### Post-tag-review pass (#167 H1, #164 design, C2-regression fix)

A second four-agent review of the fully-composed 0.3.0 tree, each finding
verified by a personal counterfactual before commit.

#### Added

- **`hardened` feature (#167 / H1) — opt-in defence-in-depth against
  UNSAFE-CALLER misuse, default OFF, NOT in `production`.** Adds an
  interior-pointer free guard on **both** own-thread free faces — the
  `SeferAlloc` per-thread magazine (`HeapCore`) and the substrate
  (`AllocCore::dealloc_small`, which the explicit `Heap`/`with_heap` face and
  any direct `AllocCore` user reach): a free of a pointer that is not a block
  start (`off % block_size(class) != 0`) becomes a detected no-op instead of a
  mis-indexed bitmap read → magazine double-issue / free-list corruption. The
  check is a modulo-per-free (a real division), so it is honestly a paid check
  and stays behind the feature — the `production` hot path is byte-identical.
  The cross-thread leg is already covered unconditionally by `reclaim_offset`.
  Other misuse vectors were cost-evaluated and honestly rejected (mismatched-
  layout free needs a per-block size word — reintroduces the block-body write
  Э6 removed). Pinned by `regression_hardened_interior_ptr`.

#### Fixed

- **C2 realloc regression (a tag blocker, found by the review):
  `HeapCore::realloc`'s own-segment branch bypassed segment-ownership stamping
  and the A1 deferred-large drain.** The 0.3.0 C2 optimization delegated
  own-thread realloc straight to `AllocCore::realloc`, so a Vec grown via
  realloc lived in an UNSTAMPED Large segment (`owner_thread_free == null`); a
  cross-thread free of it silently no-op'd → the 4+ MiB segment and its slot
  leaked → cumulative `MAX_SEGMENTS` exhaustion → abort. The resurrected
  A1/#114 leak-to-abort, on the everyday "Vec grows on one thread, freed on
  another" pattern. Fixed by mirroring `alloc`'s two hooks (stamp the result
  when it relocated; drain when the new size is Large). Pinned by
  `regression_realloc_xthread_stamp`.
- **`AllocCore::reclaim_offset` panicked instead of skipping on a garbled ring
  entry.** The class field carries 10 bits (0..1023) while only 49 classes
  exist; a corrupted entry indexed `SIZE_CLASS_TABLE` out of bounds → panic
  inside the `#[global_allocator]` → abort, violating the function's own
  documented "no abort — just skip" contract. Fixed with a class-bounds check.
  Pinned by `regression_reclaim_offset_garbled_class`.

#### Internal

- **CI blind spots closed:** a `windows-latest` `production` job (the
  aligned-vmem `VirtualAlloc`/`MEM_DECOMMIT` path was only tested locally),
  the workspace member crates' own suites (`aligned-vmem` etc.), an MSRV
  (1.88) `cargo check`, a `production hardened` matrix row, aarch64 gains a
  `production` cross run, and `release.yml` gains a tag==version guard + a
  pre-publish test gate (and a fix to the root-crate `cargo pkgid` version
  parse). The `loom_magazine_ring_compose` model and the `hardened` row were
  also wired into CI.
- **SAFETY / doc-rot corrections** (docs-only): the `TORN` (#129) SAFETY
  comment no longer rests on the false "reverse TLS declaration order"
  guarantee (rewritten to the three real reasons); the stale
  `install_thread_free` "Box-allocates" claim corrected.

### Second review pass — perf, reliability & CI hardening (W1–W6)

A second `/fxx` review of the fully-composed tree. A **deterministic
instruction-count (`Ir`) judge was built first** (W1) so every perf change is
proven on the noisy Windows dev host *before* push, not left to Linux CI. Each
change was verified by a personal counterfactual and committed between phases.

#### Performance

- **W4 — `carve_batch` + batched `dec_live`: the cold 16–64 B path drops
  ~6.3k `Ir`.** A cold refill carved blocks one at a time through `carve_block`,
  paying a runtime `align_up` division on the loop-carried `bump` dependency
  chain plus a per-block `SegmentMeta` view, `bump` load/store, `is_decommitted`
  check, `inc_live`, and page-map probe — most of them tautological after the
  first block of a run. `AllocCore::carve_batch` carves a whole run from the
  bump cursor in one shot (one `align_up`, one `set_bump`, one `add_live(n)`,
  one recommit check, page-map marking only on a page change), byte-identical to
  the per-block loop (the alloc bitmap is never touched — a bump-carved block is
  already `bit0 = allocated`, so M2 is untouched; D1 exact `+n`; same SEGMENT
  boundary, page dedication, and decommit recommit-on-reuse). `refill_class_bump`
  also drops a now-dead redundant freelist re-read after the `free_exhausted`
  latch. `flush_run`'s per-block `dec_live_and_maybe_decommit` becomes one
  `sub_live(k)` + a single decommit check (`live` reaches 0 only at the last
  accepted block, so the decommit decision is identical). Measured (W1 Ir judge,
  `production`): `cold_alloc_free_256x16b` 129,863 → 123,516 (−6,347),
  `cold_alloc_free_256x64b` −6,350, `recycle_alloc_free_256x16b` −6,254; churn
  is unregressed. Two candidates were **honest-rejected with numbers**: a
  `REFILL_N` const LUT regressed cold +32 Ir vs the inlined `udiv`; a
  `heap_core` branch-fold was not a self-contained `match`. Pinned by
  `regression_carve_batch` (+ `alloc_core_differential` M1–M4 and
  `regression_magazine_oracles` M2).
- **W3 — `alloc-stats` gate: `production` lands *below* the pre-W3 baseline on
  the hit-heavy benches.** The per-hit `tcache_hits` / `large_cache_hits`
  increments are now gated behind a new `alloc-stats` feature (default OFF, NOT
  in `production`). With it off, the magazine (churn) and large-cache hit fast
  paths carry no counter bookkeeping and those two `stats()` fields read `0`
  (all other `stats()` fields unaffected; the counter storage always exists in
  the slot, so toggling never changes layout/ABI). Gating the bump out brings
  `production` below baseline: `small_churn_16b` −59, `churn_256b` −59,
  `recycle_256b` −477, `cold_256b` −236 Ir. Enable
  `--features "production alloc-stats"` to poll the counters.

#### Fixed

- **W3 — closed a Stacked-Borrows aliasing gap in the stats aggregators.** The
  process-wide `stats().tcache_hits` / `.large_cache_hits` aggregators read each
  heap's counter through `(*heap_ptr).…`, materialising a shared
  `&HeapCore`/`&AllocCore` over a struct the OWNING thread concurrently holds a
  protected `&mut` into — a foreign read of a protected `Unique`: UB under
  Stacked Borrows (miri's default model), fine under Tree Borrows. The two hit
  counters now live in the shared, `Sync` `HeapSlot` (already read by the
  aggregator via `&HeapSlot` for `initialised`); the owner increments them
  through a stable `&'static AtomicU64` planted at `HeapRegistry::claim`, and the
  aggregators read the slot's atomic directly — no `&HeapCore` is ever formed.
  Personally verified under miri: the old shape is SB UB, the new shape is
  SB-clean (`regression_w3_stats_aliasing_miri`).
- **W2 — `SegmentTable` tombstone-rebuild: killed a long-horizon probe cliff.**
  The open-addressing `contains_base` hash tombstoned deleted entries but never
  converted them back to empty, so `#empty` was monotonically non-increasing:
  every register/unregister cycle with a fresh base (large-cache eviction,
  decommit-recycle, ASLR) consumed one empty slot forever. Once `#empty` hit 0,
  a `contains_base` MISS — the hot case, since every cross-thread free begins
  with one on the caller's own table — probed the ENTIRE table. A long-running
  server (the DBMS/async profile the crate targets) degraded to ~`HASH_CAPACITY`
  metadata loads per cross-thread free. Fixed with an exact tombstone counter
  that rebuilds the hash from the live slot set once tombstones exceed
  `HASH_CAPACITY/4` (O(1) amortised per delete; the read path stays branch-free).
  Membership is transparent across rebuilds. Ir byte-identical on all hot benches
  (zero instructions added to the measured paths). Pinned by
  `regression_segment_table_tombstone_rebuild`.

#### Internal — tooling, CI, docs

- **W1 — a deterministic WSL `Ir` judge (`npm run iai`, `scripts/iai.mjs`).**
  Drives the Linux-only `benches/perf_gate_iai.rs` through WSL under
  `valgrind --tool=callgrind` and tables the per-bench instruction count.
  Instruction counts are byte-deterministic run-to-run, which makes this a judge
  on the noisy Windows host where wall-clock is not. `docs/perf/IAI_BASELINE.md`
  records the reference table.
- **W5 — MSRV / macOS / fuzzing.** Silenced a `cargo +1.88 check --all-features`
  dead-code false-positive on `ABANDON_SEG_SIZE` (an MSRV-invisible `const _`
  assert reference); added a `macos-latest` allocator job (real Darwin runs the
  `madvise(MADV_DONTNEED)` decommit path) plus an honesty note that XNU
  `MADV_DONTNEED` is lazy (RSS reclaim best-effort; correctness unaffected as
  `alloc_zeroed` zeroes explicitly); widened the fuzz align corridor to
  `2^0..2^21` (exercising #130's large-align math), added a third fuzz target
  `heap_core_ops` (the fastbin magazine via the `SeferAlloc` `GlobalAlloc`
  face), seed corpora, and a build-only `fuzz-build` CI job.
- **W6 — sanitizer coverage gaps.** Added a plain-provenance `miri-plain` CI job
  for the exposed-provenance intrusive stacks (A1 `deferred_large` /
  `abandoned_segs`), which the strict-provenance miri jobs cannot validate by
  design; and added the two Large cross-thread tests
  (`regression_realloc_xthread_stamp`, `regression_heap_xthread_large_free_no_leak`)
  to the ThreadSanitizer list.

### Long-run durability pass — counter-wrap hardening (W7)

Auditing what happens on ultra-long runs (days/weeks of uptime, billions of
ops): every monotonic/wrapping counter was enumerated and its wrap boundary
either made unreachable (widen/repack, at proven-zero hot-path cost) or pinned
and tested across the boundary. **Honest framing: none of these was a live bug
today** — the pass makes long-run robustness auditable and future-proof. The
full inventory is [`docs/DURABILITY.md`](docs/DURABILITY.md).

- **W7a — `HeapSlot::generation` → `AtomicU64`; `TaggedPtr` repacked to
  `index:16 | tag:48`.** Generation wrapped at 2^32 thread-deaths (reachable on
  a thread-per-request server over months) — though it turned out to be consumed
  only by a `== 1` first-materialise gate, with the stale-TLS hazard actually
  guarded by the `TORN` sentinel, so the wrap was defence-in-depth, not a live
  ABA. The `free_slots` ABA tag was 32-bit (the documented probabilistic wrap);
  repacking the index half from 32 to 16 bits (MAX_HEAPS = 4096 needs 13, pinned
  by a `const` assert) gives the tag 48 bits → wrap at ~89 years. Generation is
  Ir byte-identical; the repack is a uniform −4 Ir (a *decrease*, from the
  cheaper bootstrap `empty()` constant — cold path). Boundary tests in
  `regression_counter_wrap` preset each counter near its limit and cross it.
- **W7b — pinned the `RemoteFreeRing` u32 cursor wrap.** The per-segment ring's
  `head`/`tail` genuinely wrap on a long run (2^32 cross-thread frees on one hot
  segment — reachable), but the ring is wrap-SAFE by design (`wrapping_sub`
  occupancy + `i % RING_CAP` indexing, whose continuity across `u32::MAX` needs
  `2^32 % RING_CAP == 0`). That power-of-two dependency was unstated — now a
  `const` assert — and `regression_ring_cursor_wrap` drives the real ring across
  the boundary (FIFO order, full-ring overflow, occupancy, and a concurrent
  hammer). Counterfactuals confirm both guards bite. Ir byte-identical.
- **W7c — `docs/DURABILITY.md`.** The authoritative counter inventory (width /
  wrap semantics / reachability arithmetic / verdict / covering test) and the
  rule that a new monotonic counter lands only with a row here + a
  boundary-crossing test, proven Ir-neutral.

### Post-review hardening pass (#129–#143)

This and the phase A–F pass below hardened 0.3.0 before its first publish:
the post-review pass (#129–#143, 2026-07-02/03) driven by a four-agent audit
with per-fix counterfactual verification, and the phase A–F pass
(2026-06-30). Entries are grouped per pass.

#### Fixed

- **#129 — BLOCKER: `tls_heap`'s stale-LOCAL TLS resolver could hand out two
  `&mut HeapCore` for the same recycled registry slot.** `tls_heap`'s `LOCAL`
  (a `Cell`, no `Drop`) and `GUARD` (`AbandonGuard`, has `Drop`) are declared
  in an order where `GUARD` drops FIRST on thread teardown — recycling the
  registry slot — while `LOCAL` survives holding its now-stale pre-recycle
  pointer. Every resolver treated any non-null `LOCAL` as "my own live slot";
  the documented generation-guard was never actually read on the alloc path.
  Reachable from correct code: an application `thread_local` with a `Drop`
  impl that allocates, first touched before the thread's first `sefer-alloc`
  allocation, is destroyed after `GUARD` — its `Drop` could resolve to the
  stale, already-recycled slot, handing out a second live `&mut HeapCore`
  concurrently with whoever re-claimed it (a data race / UAF). Fixed with a
  `TORN` sentinel (`usize::MAX`, never dereferenced): `AbandonGuard::drop`
  stamps `LOCAL = TORN` before recycling; all three TLS resolvers check
  `TORN` before treating a non-null `LOCAL` as live, and route post-teardown
  deallocs through the always-live fallback heap instead.
- **#130 — BLOCKER: `alloc_large` with `align >= SEGMENT` leaked to abort or
  returned a misaligned pointer (UB).** `alloc_large` places a large block at
  `base + align_up(header, align.max(PAGE))`, but `base` is only
  `SEGMENT`-aligned (4 MiB). For `align == SEGMENT`, the block itself landed
  `SEGMENT`-aligned at `base + SEGMENT` — an address `dealloc`'s
  `base & !(SEGMENT-1)` computation never resolves back to the registered
  `base`, so every such `dealloc` silently no-op'd, leaking the segment and
  its `SegmentTable` slot until `MAX_SEGMENTS` (1024) exhausted and the
  process aborted. For `align > SEGMENT`, the returned pointer inherited only
  4 MiB alignment roughly half the time — violating the `GlobalAlloc`
  contract (UB in the caller). Both reachable from a valid `Layout` (e.g.
  `#[repr(align(4194304))]`, huge-page buffers). Fixed by rejecting
  `align >= SEGMENT` up front with a null return (a legal, documented alloc
  failure) — exotic alignments at or above the segment size are unsupported
  by the dedicated-segment large path.
- **#131 — `ensure_slow`'s OOM path panicked without rolling back the
  bootstrap sentinel, livelocking every future registry access.** The CAS
  winner publishes `SENTINEL_INITIALIZING` before reserving VM for the
  `Registry`; on OOM the old code called `.expect(..)`, which panicked
  without ever restoring a real pointer or rolling the sentinel back to
  null. Every loser thread spinning on the sentinel spun forever, and every
  future `ensure()` call also spun forever (CAS(null, SENTINEL) never
  succeeds against a non-null stuck sentinel) — a process-wide livelock on
  the next registry touch. Worse, unwinding the panic itself allocates,
  reentering `ensure()` against the same stuck state before the panic even
  finished. Fixed: on reservation failure, roll `REGISTRY_PTR` back to null
  (Release) before terminating via `std::process::abort` (not `panic!` —
  `abort` performs no unwind and no allocation, so it cannot reenter
  `ensure()`).
- **#134 — `large_cache`'s `usable_size` was recomputed from mutable header
  fields, corrupting the RSS byte-budget.** At deposit time (both the
  own-thread `dealloc` Large branch and `reclaim_large_segment`),
  `usable_size` was recomputed from the header's `large_size`/`large_align`.
  On a large-cache HIT, a larger cached span can be reused for a smaller
  request, and the hit path rewrites the header's logical size/align to the
  smaller request — so on the segment's NEXT free, the recomputed
  `usable_size` under-reports the segment's true physical span. This let
  `large_cache_used_bytes` under-count real RSS, admitting more spans than
  the configured budget should allow (unbounded RSS amplification), and
  corrupted the cache-hit size-ratio matching. Fixed by adding a new
  `SegmentHeader::span_usable` field — the segment's PHYSICAL committed span,
  set once at the original OS reservation and carried forward verbatim
  (never recomputed) through every subsequent cache-hit reuse. Both deposit
  sites now read `header.span_usable` instead of recomputing from
  `large_size`/`large_align`.
- **#139 — miri could not validate the `registry` module: the ~22 MB
  `Registry` reservation was uninitialised under miri's `std::alloc`
  fallback.** `bootstrap::ensure_slow` relies on OS zero-pages
  (`VirtualAlloc`/`mmap`) for every `Registry` field it does not explicitly
  write. Under miri, `aligned-vmem`'s reservation falls back to
  `std::alloc`, which does NOT zero memory — so reads of `count`,
  `abandoned_segs`, and friends hit uninitialised memory (UB), aborting miri
  before it could validate anything in the registry module (including the
  #133 per-heap-counter aggregation and the #131 sentinel rollback). Fixed
  with a `#[cfg(miri)]`-only `write_bytes(base, 0, REGISTRY_SIZE)` right
  after the reservation — compiled out entirely on real targets (zero
  production cost). Full strict-provenance cleanliness of the tagged-pointer
  infrastructure is separately tracked as #140.

- **#142 — cross-thread `thread_free` access violated the aliasing model
  (Stacked AND Tree Borrows).** Expanding miri to the A1 cross-thread path
  showed the deferred-free push's `head.load` was UB under both experimental
  borrow models: the `owner_thread_free` stamp inherited the owner's
  `&mut self`-rooted reference provenance, so one remote thread's
  `compare_exchange` through it was a "foreign write" that Disabled the
  shared parent tag and forbade a second remote's read. Fixed with the same
  exposed-provenance discipline as #140: the stamp sites `expose_provenance()`
  the atomic's address (taken via `addr_of!`, no intermediate `&` retag) and
  `Node::atomic_ptr_ref` reconstructs the remote's `&AtomicPtr` via
  `with_exposed_provenance_mut` — a wildcard pointer outside the owner's
  borrow tree. Verified under miri with BOTH models on both faces' A1 tests
  and `heap_cross_thread` (all were UB before this fix).
- **#143 — `push_large_deferred_free` silently dropped a push (permanent
  leak) under concurrent head contention.** Found by the new
  `loom_deferred_large` model (#141) and confirmed by a 2M-trial
  `std::thread` reproduction: the double-push claim-CAS lived INSIDE the
  head-CAS retry loop, so after losing the head CAS to a concurrent pusher
  of a DIFFERENT base, the retry's claim always failed (the link word had
  already left `ABANDONED_TAIL`) and the function returned through the guard
  bail-out without ever winning `head` — the segment never entered the
  deferred-free stack (an A1-class permanent leak). Fixed by hoisting the
  claim CAS to run exactly once, before the head-CAS retry loop.
- **Full-review follow-up — the #138 layout-consistency mitigation
  over-rejected legitimate tiny-size frees.** The alloc path clamps every
  request to `MIN_BLOCK` (16) before it reaches the header's `large_size`,
  but the mitigation compared the freeing caller's RAW `layout.size()` — so
  a legitimate cross-thread free of a `size < 16`, `align > SMALL_MAX` block
  (a valid `Layout` via the raw alloc API) always mismatched, was dropped,
  and permanently leaked the segment + its table slot (the #114/#130
  leak-to-abort class, narrow trigger). `large_layout_consistent` now clamps
  the caller's size symmetrically before comparing.

#### Performance

- **#133 — per-heap hit counters replace a contended global-lock `fetch_add`
  on the hot path.** `DBG_TCACHE_HITS` (magazine-hit) and
  `LARGE_CACHE_HITS` (large-cache-hit) were process-global `AtomicU64`s
  bumped by every thread on otherwise fully-per-thread hot paths — a
  contended cache line that ping-ponged across cores. Moved to per-heap
  fields (`HeapCore::tcache_hits`, `AllocCore::large_cache_hits`),
  incremented `Relaxed` by the owning thread only; the process-wide view
  (`stats()`, tests) is reconstructed by summing every minted heap slot's
  counter, gated by a new `HeapSlot::initialised: AtomicBool` (Release-set
  after the heap is fully constructed; the aggregator Acquire-loads it to
  avoid reading a not-yet-initialised slot). Measured: churn −20.9 % (16 B),
  −19.6 % (64 B).
- **#135 — `SegmentTable::register`/`unregister`/`recycle` and
  `HeapCore::realloc`'s ownership test are now O(1), not O(segment count).**
  `register` used to scan `[0, count)` for a NULL slot; `unregister`/
  `recycle` scanned for a matching base. All three are now O(1) via a
  free-list stack of recycled slot indices (carved in the primordial
  segment) plus a field-specific `segment_id_at` header read that indexes
  the slot directly. `HeapCore::realloc`'s ownership check switched from
  `segment_bases().any(...)` (O(count)) to `AllocCore::contains_base` (O(1)
  hash probe, same semantics). Also hardens `dealloc_routing`'s M2 routing:
  `self.core.contains_base(base)` is now checked FIRST (O(1), reads only the
  caller's own table, no cross-thread memory read) — proven equivalent to
  the prior `owner_tf.is_null() || owner_tf == our_head` branch for every
  segment the caller owns; only a miss falls through to the field-specific
  cross-thread header reads.

#### Changed

- **#136 — public API polish before the first 0.3.0 publish (pre-release, not
  a breaking change for any published version).**
  - `SegmentLayout::SIZE_CLASS_TABLE` / `SIZE2CLASS` are now `&'static [..]`
    slices instead of fixed-size arrays (`[usize; 48]` / `[u8; N]`). The
    class-count grew silently 40→48 in 0.3.0; a fixed-length public type would
    have made every future class re-tune a breaking change. A slice view has
    no length in its type.
  - `LargeCacheConfig::budget_bytes(0)` now means "cache disabled" (every
    deposit released to the OS), stored verbatim as `Some(0)`. Previously `0`
    was silently remapped to `None` ("unbounded") — the opposite of what `0`
    intuitively suggests. Unbounded is still the default (don't call
    `budget_bytes`).
  - `LargeCacheMode` is now `#[non_exhaustive]` (adding a variant in a future
    release is no longer breaking).
  - Internal-but-`pub` items reachable only through `#[doc(hidden)]` modules
    (e.g. `AllocCore::segment_bases`, `HeapCore::segment_bases`) are now
    `#[doc(hidden)]`, and stale `SMALL_ALIGN_MAX`/`SMALL_MAX` docs were
    corrected to match the #114/B1 divisibility-aware small path (align > 16
    is served by the small path up to `SMALL_MAX`, not routed to Large).
  - rustdoc builds clean (0 warnings) under both the default and `production`
    feature sets; docs.rs is configured to render with `production`.

- **#132 — the explicit `Heap`/`with_heap` public face lacked the A1
  cross-thread Large-segment reclaim fix.** `SeferAlloc` (via `HeapCore`) got
  the A1 fix in 0.3.0; `Heap::dealloc_any_thread` did not — a cross-thread
  free of a Large/huge segment through the explicit `Heap` API still no-op'd
  and leaked the segment permanently until the owning `Heap` dropped. Both
  faces now share the same extracted deferred-free primitive
  (`alloc_core::deferred_large`), including the double-push guard hardening,
  so a remote free of a Large segment is reclaimed on the owner's next large
  allocation regardless of which public face is used.
- **#132 — `with_heap` panicked on a reentrant borrow or TLS teardown.**
  `with_heap`'s documented `# Panics` behaviour (`RefCell::borrow_mut`
  panicking on a reentrant call, or on TLS-destructor-already-ran) was a
  footgun for a public allocator API — e.g. a `Drop` impl that allocates via
  `with_heap` during thread teardown would abort instead of degrading
  gracefully. `with_heap` now uses the same no-panic
  `try_with`/`try_borrow_mut` mechanics as the crate-internal
  `with_heap_try` and returns `None` (its signature has always been
  `Option<R>`) instead of panicking.
- **#138 — A1 post-reuse defensive mitigation for cross-thread Large-segment
  double-free.** A1's deferred-free stack fully closes the PRE-reuse
  double-free window (a double-free of a Large segment not yet reclaimed is
  a sound no-op, guarded by `push_large_deferred_free`'s double-push CAS
  guard). The POST-reuse window remained: a stale free arriving after the
  segment was already reclaimed and handed to a brand-new allocation is, by
  address alone, indistinguishable from a legitimate free of that new
  occupant. Both cross-thread Large-free routing paths
  (`HeapCore::dealloc_routing`, `Heap::dealloc_any_thread`) now check that
  the freeing `Layout`'s size matches the CURRENT occupant's `large_size`
  header field (`alloc_core::deferred_large::large_layout_consistent`)
  before queuing the segment for reclaim; a mismatch is dropped as a no-op
  instead of corrupting the reused segment. **Honest scope: this is a
  mitigation, not a full fix** — a reuse that happens to request the
  bit-identical size is not caught (double-free remains UB by the
  `GlobalAlloc` contract). New regression tests:
  `tests/regression_xthread_large_free_layout_mismatch.rs`
  (`xthread_large_free_mismatched_layout_is_dropped`,
  `xthread_large_free_consistent_layout_is_reclaimed`, plus a `Heap`-face
  counterpart), counterfactual-verified against both call sites.

#### Internal

- **#137 — CI never exercised the `fastbin` (magazine/tcache) path or the
  flagship `production` feature bundle**, and `loom_fallback_init` (the
  fallback-heap lazy-init state machine) existed but was absent from the
  loom CI matrix (model-checked locally, never gated in CI). Added
  `--features "alloc-global alloc-xthread fastbin"` and
  `--features production` to the test matrix, `--no-fail-fast` to the test
  runner (a failure in one test binary no longer masks failures in later
  ones), and `loom_fallback_init` to the loom matrix.
- **#138 — loom-model honesty audit.** Every `tests/loom_*.rs` file's doc
  comment now states whether it models a currently-live production code
  path, a removed/superseded one, or a dead (currently-unreachable) one:
  `loom_thread_free.rs` models the Phase 10 intrusive-TFS push/drain of
  individual freed blocks, which was superseded by the non-intrusive
  per-segment `RemoteFreeRing` (modelled separately, faithfully, in
  `loom_remote_ring.rs`) — retained for its generic CAS-push counterfactual,
  not as a validator of any current path. `loom_registry.rs` models the
  Phase 12.4 segment-adoption CAS protocol, whose only producer
  (`HeapRegistry::abandon_segments`) is unreachable from any production path
  today (Phase 12.5 replaced thread-exit abandonment with whole-heap slot
  reuse) — retained as a pre-validated substrate for a future
  decommit-when-empty policy. `tagged_ptr.rs`'s doc comment referenced a
  push-pop-repush ABA loom model in `loom_registry.rs` that was never
  actually written (that file models a different protocol entirely); the
  reference is corrected and the missing ABA model for the `free_slots`
  `TaggedPtr` stack is tracked as follow-up debt, not written in this pass.
  A loom model for the A1 `deferred_large` push/drain (Large-segment
  reclaim) is also tracked as follow-up debt — judged out of scope for this
  hardening pass (see the task report for the full audit table).

- **#140 — explicit provenance APIs for the registry's lock-free stacks.**
  The `REGISTRY_PTR` sentinel is now constructed with
  `core::ptr::without_provenance_mut` (strict-provenance-clean; it is only
  ever compared, never dereferenced), and every cross-allocation packed-word
  store/load pair in `abandoned_segs` and the A1 deferred-large stack calls
  `expose_provenance` / `with_exposed_provenance_mut` explicitly, with a
  documented "Provenance model" section explaining why full
  `-Zmiri-strict-provenance` is structurally unreachable for
  cross-allocation intrusive stacks (an exposed-provenance shape by design,
  not a bug). No lock-free semantics changed.
- **#141 — the two missing loom models were written**, closing the debt the
  #138 audit recorded above: `loom_deferred_large.rs` (the A1 push/drain
  Treiber stack including the double-push guard — the model that found
  #143) and `loom_free_slots_aba.rs` (the `free_slots`/`TaggedPtr`
  push-pop-repush ABA scenario). Both ship `should_panic` counterfactuals
  proving non-vacuity and are wired into the CI loom matrix.

### Initial pass — phases A–F (2026-06-30)

Post-0.2.1 hardening pass — six phases (A–F), each independently reviewed,
counterfactual-verified, and committed.

#### Fixed

- **A1 — permanent leak: cross-thread free of a Large/huge segment.** A
  remote free of a Large segment no-op'd instead of reclaiming it — the
  segment (≥4 MiB) and its `SegmentTable` slot leaked forever under any
  allocate-here/free-there workload (the canonical case: an async runtime
  migrating a task holding a large buffer to a different worker thread). Now
  reclaimed via a per-heap deferred-free stack, drained lazily on the
  owner's next large allocation.
- **A2 — `fastbin` buildable without `alloc-xthread` (unsound).** A
  cross-thread free with `fastbin` alone had no ownership-checked routing
  path — a data race into another thread's private magazine. `fastbin` now
  requires `alloc-xthread` (Cargo feature unification + a `compile_error!`
  guard).
- **B1 — page-aligned allocations (512 B – 16 KiB, `align` a multiple of
  512/1024/2048/4096) still burned a dedicated Large segment**, the last gap
  in #114's fix. Eight page-aligned size classes added to the table.
- **Latent `realloc` cross-class-shrink bug**, exposed by B1: `AllocCore::realloc`'s
  in-place fast path aliased a shrink across size classes, corrupting the
  smaller class's free list on a later layout-derived free. Restricted to
  same-class in-place; a cross-class shrink now relocates.
- **F1 — fallback-heap init livelock.** If the CAS winner initialising the
  process-global fallback heap hit primordial OOM, every other thread
  spun forever waiting for a `READY` that would never come. Losers now
  observe the rollback and re-race the CAS.

#### Changed — performance

- **C1 — the per-thread magazine (`fastbin`) now serves `align > 16`
  requests** (tokio task cells, page-aligned buffers), not just the
  historical `align <= 16` case — the main remaining hot-path gap for the
  workload #114/B1 targeted.
- **C2 — `realloc`'s in-place fast path is now reachable through the
  `#[global_allocator]` face**, not just the lower-level `AllocCore` API; a
  same-class resize through `SeferAlloc` no longer pays a redundant
  alloc+copy+dealloc.
- **D1 — `LARGE_CACHE_SLOTS` raised 2 → 8**, with a correctness fix: eviction
  now uses a true insertion-order FIFO (a monotonic sequence number) instead
  of an index-order assumption that only held at 2 slots. A workload cycling
  more than two distinct large sizes now gets real cache reuse instead of
  thrashing to an OS round-trip on every allocation.
- **D3 — magazine refill is now a per-class byte budget** (≈64 KiB) instead
  of a fixed 16-block count for every class; a large size class no longer
  parks several MiB in one idle thread's cache after a single refill.

#### Added

- **`SeferAlloc::stats() -> AllocStats`** — a cheap, lock-free, process-wide
  diagnostic snapshot (cache hits, decommit calls, cross-thread reclaims,
  ring overflows, segments reserved/released, heaps claimed). Previously
  every one of these counters was crate-internal and invisible in
  production; `segments_reserved_total - segments_released_total` is the
  single most useful field for spotting a segment leak before it escalates
  to an OOM abort. `#[non_exhaustive]`, stable field set across every
  feature combination.
- **D2 — process-wide `RemoteFreeRing` overflow counter**, feeding
  `AllocStats::ring_overflows`.
- Rustdoc: a "Multi-thread safety" section on `SeferAlloc` spelling out the
  `alloc-global`-without-`alloc-xthread` footgun (cross-thread frees leak
  monotonically), and a "std-only" note.

#### Internal

- CI: `-D warnings` restored on the clippy gate after a warnings-cleanup
  pass; miri matrix extended to the task-#114 align-regression tests; a
  process-global-state test flake in `heap_core_bulk_bypass` fixed at its
  real root cause (whole-heap slot reuse carrying stale P7 state across
  tests in one binary).

## [0.2.1] - 2026-06-30

> ⚠️ **Superseded by `0.3.0`; to be yanked from crates.io once `0.3.0` is
> published.** `0.2.1` ships `fastbin = ["alloc-global"]`, which is buildable
> *without* `alloc-xthread` — a cross-thread free with `fastbin` alone has no
> ownership-checked routing path and races into another thread's private
> magazine (data race / UB). Fixed in `0.3.0` (phase A2: `fastbin` now
> requires `alloc-xthread`, enforced by Cargo feature unification + a
> `compile_error!` guard). Upgrade to `0.3.0`.

### Fixed — `align > 16` allocations no longer burn a dedicated segment each

`SizeClasses::class_for(size, align)` unconditionally returned `None` for
any `align > SMALL_ALIGN_MAX` (= `MIN_BLOCK` = 16). Every allocation with
a larger alignment — including the `tokio::runtime::task::core::Cell<T,S>`
shape (≈640 B, `#[repr(align(128))]` against false sharing) — was routed
to the dedicated-segment Large path, consuming a full ~4 MiB segment and
one `SegmentTable` slot per request.

Under concurrent task-spawning workloads (canonical reproducer: the
shamir-db `duplex_throughput/duplex_cap32/32` bench — 32 in-flight
tokio tasks × 55 iterations), cumulative live segments exceeded
`MAX_SEGMENTS = 1024`, then `alloc_large_slow → SegmentTable::register`
returned `None`, then the `GlobalAlloc` face returned null, then
`std::alloc::handle_alloc_error` aborted the process with
`memory allocation of 640 bytes failed`.

`class_for` now searches for the smallest small class whose
`block_size >= max(size, align)` AND `block_size % align == 0`. M4
(alignment fidelity) is preserved: the segment base is `SEGMENT`-aligned,
the offset within is a multiple of `block_size`, and `block_size` is a
multiple of `align`, so the returned pointer is naturally `align`-aligned
without any per-block padding. The fast path for `align ≤ MIN_BLOCK = 16`
(the typical case) is byte-identical to the previous behaviour — one
`SIZE2CLASS` load. The slow path is a forward walk over at most
`SMALL_CLASS_COUNT = 40` entries; in practice it settles in 0–3 steps
for power-of-two alignments common in async runtimes (32 / 64 / 128 / 256).

For `(640, align=128)` the resolver picks the existing class with
`block_size = 768` (768 % 128 == 0). Per-allocation memory cost drops
from ~4 MiB to ~768 B, and the per-process `SegmentTable` is no longer
touched on the hot path.

Regression test: `tests/regression_large_align_no_segment_exhaustion.rs`
(2048 sequential `(640, 128)` allocations + 1500 sequential allocations
each for 4 representative `(size, align)` shapes). Counterfactual
verified — reverting the fix makes the test fail on iteration 1023
(= `MAX_SEGMENTS − 1`, primordial segment holds the first slot).

Single-threaded substrate change; no concurrency-protocol or wire-format
implications. Full test suite under `features = ["production"]` —
including loom (`loom_bootstrap_cas`, `loom_xthread_protocol`,
`loom_thread_free`) — green.

## [0.2.0] - 2026-06-29

> ⚠️ **Yanked from crates.io.** Superseded by `0.2.1`, which fixes the #114
> `align > 16` segment-exhaustion bug: an `align > 16` allocation (e.g. the
> `tokio` task-cell shape, `#[repr(align(128))]`) burned a full ~4 MiB
> segment each and could exhaust `MAX_SEGMENTS = 1024` and abort the process
> under ordinary async workloads. Upgrade to `0.2.1` or later.

### Changed — BREAKING: `SeferMalloc` renamed to `SeferAlloc`

The headline `#[global_allocator]` type is renamed from `SeferMalloc` to
`SeferAlloc`. The "malloc" suffix was a libc convention inherited from
C-wrapper allocators (`mimalloc`, `jemalloc`, `tcmalloc`) and clashed
with sefer-alloc's positioning as a pure-Rust allocator with no C deps.
The new name aligns the type with the crate name and the Rust ecosystem's
modern `*-alloc` convention.

**Migration:** rename every occurrence of `SeferMalloc` in your code to
`SeferAlloc`. The constructors (`new()`, `with_config(...)`) and the
public API surface are otherwise unchanged.

```rust
// Before (0.1.x):
use sefer_alloc::SeferMalloc;
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// After (0.2.0):
use sefer_alloc::SeferAlloc;
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
```

`LargeCacheConfig`, `LargeCacheMode`, `Region`, `Handle`, `SyncRegion`,
`AllocCore`, and every other public type are unchanged.

Internal: `src/global/sefer_malloc.rs` → `src/global/sefer_alloc.rs`
(module file rename). User-facing docs (`README.md`, `docs/INTEGRATION.md`,
`docs/ARCHITECTURE.md`) updated to use "alloc face" terminology consistently;
historical / planning docs (`ALLOC_PLAN.md`, `FINDINGS_PHASE12.md`, etc.)
keep their original "malloc face" language as historical record.

`0.1.0` is yanked from crates.io to direct fresh installs to `0.2.0`;
existing `Cargo.lock` references continue to work.

### Changed — const-builder config API replaces env vars (alloc-decommit)

- **`LargeCacheConfig` const builder** — new type (re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`). All five knobs
  that were previously set via environment variables are now expressed at
  compile time via a `const fn` builder chain:

  ```rust
  use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};

  const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
      .budget_bytes(512 * 1024 * 1024)
      .headroom_bytes(64 * 1024 * 1024)
      .decay_interval_ms(200)
      .decay_rate_percent(25)
      .mode(LargeCacheMode::Lazy);

  #[global_allocator]
  static GLOBAL: SeferMalloc = SeferMalloc::with_config(CONFIG);
  ```

- **`SeferMalloc::with_config(config: LargeCacheConfig) -> Self`** (`const fn`,
  only under `alloc-decommit`) — constructs the allocator with a custom
  large-cache config. The config is plumbed into each per-thread `AllocCore`
  on first TLS bind.

- **`SeferMalloc::new()`** unchanged — equivalent to
  `SeferMalloc::with_config(LargeCacheConfig::DEFAULT)`.

- **`AllocCore::new_with_config(config: LargeCacheConfig) -> Option<Self>`**
  (`alloc-decommit` only) — new constructor for direct `AllocCore` users.

- **Env vars removed entirely** — `SEFER_LARGE_CACHE_BUDGET`,
  `SEFER_LARGE_CACHE_HEADROOM_BYTES`, `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS`,
  `SEFER_LARGE_CACHE_DECAY_RATE`, `SEFER_LARGE_CACHE_MODE` are no longer read.
  The allocation-free env-var parser in `src/alloc_core/os.rs` is deleted.
  Default values are byte-identical to what the parsers produced when no variable
  was set (headroom=256 MiB, interval=1000 ms, rate=10 %, budget=unbounded,
  mode=Lazy).

- **Tests updated** — `tests/large_cache_budget.rs`, `tests/large_cache_decay.rs`,
  and `tests/large_cache_mode.rs` no longer use `std::env::set_var`. The
  env-var test cases are replaced with equivalent `AllocCore::new_with_config`
  tests that are deterministic and safe to run in parallel.

## [0.1.0] - 2026-06-28

### Changed — workspace extraction (tasks #74–#86)

Four independently-publishable companion crates extracted from sefer-alloc
into `crates/`. Each is a real crates.io package someone can `cargo add`
on its own:

- **`sefer-region`** (`crates/region/`) — typed handle store
  (`Handle<T>` / `Region<T>` / `SyncRegion<T>`). `#![forbid(unsafe_code)]`.
  ([docs.rs/sefer-region](https://docs.rs/sefer-region) — link live after publish.)

- **`aligned-vmem`** (`crates/vmem/`) — OS virtual-memory aperture:
  SEGMENT-aligned `mmap`/`VirtualAlloc` + page decommit/recommit.
  `#![allow(unsafe_code)]` — sole purpose IS the OS unsafe, single
  responsibility, small codebase, independently auditable.
  ([docs.rs/aligned-vmem](https://docs.rs/aligned-vmem) — link live after publish.)

- **`numa-shim`** (`crates/numa/`) — dependency-free NUMA detection and
  binding. Linux `mbind(2)` via `syscall(2)` (no `libnuma`), Windows
  `VirtualAllocExNuma`. `#![allow(unsafe_code)]` — sole purpose IS the NUMA
  syscall unsafe, single responsibility, independently auditable.
  ([docs.rs/numa-shim](https://docs.rs/numa-shim) — link live after publish.)

- **`malloc-bench-rs`** (`crates/malloc-bench/`) — portable `GlobalAlloc`
  benchmark harness (larson + mstress). Callable against any allocator without
  installing it as `#[global_allocator]`. Not in sefer-alloc's runtime dep
  tree.
  ([docs.rs/malloc-bench-rs](https://docs.rs/malloc-bench-rs) — link live after publish.)

**sefer-alloc itself** re-exports `sefer-region`'s surface for backward
compatibility — existing `use sefer_alloc::{Region, Handle, SyncRegion}` code
compiles unchanged. `alloc_core::os` and `alloc_core::numa` are now thin
interop wrappers that delegate to `aligned-vmem` and `numa-shim` respectively.

**Audit story improved:** an auditor no longer has to navigate the full
allocator codebase to verify the OS-memory unsafe. `aligned-vmem` (~few hundred
lines, single purpose) and `numa-shim` (~few hundred lines, single purpose) can
each be audited in complete isolation with `cargo test` confirming green.

### Added — large-cache redesign Phase 3 (alloc-decommit, mode-selector + future stub)

- **`LargeCacheMode { Lazy, Background, Both }`** enum, re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`. The mode is selected
  via the new `SEFER_LARGE_CACHE_MODE` env var (case-insensitive: `lazy` /
  `background` / `both`; unrecognised values fall back to `Lazy`).

- **Default = `Lazy`** — Phase 2 behaviour is preserved bit-for-bit. Setting
  `SEFER_LARGE_CACHE_MODE=background` currently prints a one-time process
  warning ("background mode requested but not yet implemented — falling back
  to lazy") and continues with lazy decay. The full background-thread
  implementation has identified risks documented inline (Mutex refactor +
  HeapRegistry iteration API + safe spawn timing + TSan validation) and is
  intentionally deferred to a follow-up; the mode-selector plumbing lets a
  future commit turn it on without any user-facing API change.

- **`tests/large_cache_mode.rs`** — 3 new tests covering default-Lazy,
  per-shard mode storage, and env-var parsing.

### Changed — large-cache redesign Phase 2 (alloc-decommit)

- **Lazy exponential decay**: large-cache excess over the headroom target
  decays toward the OS at 10 %/tick by default. On every large `alloc` or
  `free`, a single `Instant::now()` comparison checks whether
  `decay_interval` has elapsed; if so, `excess = used - headroom` and
  `release = excess × rate` bytes are FIFO-evicted to the OS. No background
  thread — the decay is fully inline, paying nothing while the process is idle
  (mobile/embedded friendly). Phase 3 will add an optional background thread.

- **Three new env vars** (all read once at `AllocCore::new`, allocation-free):
  - `SEFER_LARGE_CACHE_DECAY_RATE` — integer percent (`"10"`, `"10%"`;
    default 10). Parsed without floats to avoid any floating-point dependency.
  - `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` — integer ms (default 1000).
  - `SEFER_LARGE_CACHE_HEADROOM_BYTES` — bytes with K/M/G suffix (default
    256 MiB). The cache is allowed to hold up to this many bytes; only the
    excess above it is subject to decay.

- **Generalized `os::read_env_var_raw(name_nul, buf)`**: the allocation-free
  env-var reader is now parameterized on the variable name (NUL-terminated
  `&[u8]`). `read_env_budget_raw` is kept as a thin backward-compatible
  wrapper. This lets all three decay env parsers share the same reentrancy-safe
  pattern without duplicating the Windows/Unix platform dispatch.

- **Test seams** (`dbg_set_decay_config`, `dbg_force_decay_tick`,
  `dbg_decay_config`): deterministic test control without sleep or real
  wall-clock advances. `dbg_force_decay_tick` rewinds `last_decay_tick` by
  `decay_interval` and immediately invokes one decay step.

- **`tests/large_cache_decay.rs`**: 5 new tests covering excess release,
  headroom invariant, no-op when under target, interval guard, and env-var
  parsing.

### Changed — large-cache redesign Phase 1 (alloc-decommit)

- **Removed `MAX_CACHED_LARGE_BYTES`** (was 64 MiB per-span cap). Spans of
  any size can now enter the large-cache, removing the arbitrary ceiling that
  prevented caching of 100 MiB+ allocations.

- **Per-shard byte-budget admission** replaces the old per-span cap. A new
  `AllocCore::large_cache_budget_bytes: Option<usize>` field (under
  `alloc-decommit`) tracks the total bytes of all cached spans. When the
  budget would be exceeded, the oldest cached slot (FIFO: lowest index) is
  evicted to the OS before the new span is admitted. `None` = unbounded
  (default when the env var is not set).

- **`SEFER_LARGE_CACHE_BUDGET` environment variable** is read once at
  `AllocCore::new()` via a raw OS call (no heap allocation — safe even when
  `SeferMalloc` is the `#[global_allocator]`). Accepted formats: `"64M"`,
  `"2G"`, `"1024"` (raw bytes), etc. Parsed case-insensitively.

- **`large_cache_used_bytes` invariant counter**: maintained on every deposit
  and every eviction / cache hit. Verified by new tests via
  `dbg_large_cache_used()` / `dbg_large_cache_slot_sizes()` test seams.

### Removed

- **`byte` / `byte-sharded` features** — research-tier `ByteRegion` /
  `ByteAllocator` / `ShardedByteArena` removed. They were never expected to
  compete with mimalloc (see the BYTE_BENCH / BYTE_SHARDED_BENCH writeups in
  git history) and are fully superseded by the production stack (`alloc-global`
  + `alloc-xthread` + `alloc-decommit`). Old Phase 4 / Phase 7d log entries
  below are intentionally left intact as historical record.

### Deprecated

- **`experimental` concurrent regions** (`EpochRegion`, `LockFreeRegion`,
  `ShardedRegion`) — marked `#[deprecated]`. Superseded by the production
  `alloc-xthread` cross-thread free path. `PinnedRunner` is NOT deprecated.

### Summary

The initial public release.

**Pure Rust, no C / C++ libraries.** Unlike `mimalloc` (C++), `jemalloc`
(C), `snmalloc` (C++), `tcmalloc` (C++), or the typical `libnuma`-wrapping
NUMA crates, `sefer-alloc` is 100 % Rust — it calls into the OS directly
(`mmap` / `VirtualAlloc` / `mbind` etc.), but does not link a single C or
C++ library. The only C dependency in the repository is the optional
`mimalloc` dev-dependency used as a baseline in benchmarks (never on a
consumer's runtime path).

Two faces on one verified substrate:

- **`Region<T>` / `Handle<T>`** — a safe-by-construction handle store
  (default `std`, also `no_std` + `alloc`). `#![forbid(unsafe_code)]`
  at the top — the only `unsafe` is `slotmap`'s audited core wrapped
  by a typed membrane.

- **`SeferMalloc`** — a drop-in `#[global_allocator]` (opt-in
  `production` feature = `alloc-global + alloc-xthread +
  alloc-decommit`). Up to **~18× faster than `mimalloc` on cached
  large alloc/free** after the OPT-E large-cache (4 MiB cycle ≈ 45 ns
  vs ~718 ns ≈ **~16×**; 16 MiB ≈ 48 ns vs ~869 ns ≈ **~18×** — single
  Windows dev host, criterion `sample_size(10)`, see
  `docs/ALLOC_BENCH.md`); competitive with `mimalloc` on multi-thread
  cross-thread paths (`examples/malloc_macro.rs`). Confined-`unsafe`
  inventory under `production` (eight files): `alloc_core::{os, node}`
  + `global::{sefer_malloc, tls_heap, fallback}` +
  `registry::{heap_slot, heap_registry}`. `numa-aware` adds one more
  (`alloc_core::numa`). The crate is `#![deny(unsafe_code)]` (or
  `#![forbid]` in the default `std`-only build) and every `unsafe`
  block carries a `// SAFETY:` proof; compile-enforced.

Verification stack: 51 integration test files, 6 loom models
(`tests/loom_*.rs`), proptest differential vs reference model, miri
with strict-provenance (CI gate), ThreadSanitizer (×3 verified
clean on cross-thread + decommit), Valgrind memcheck clean,
aarch64 13/13 under qemu-user, libFuzzer (`region_ops`,
`global_alloc_ops`), soak / RSS / tokio-burn-in harnesses,
criterion benches with flamegraph profiling. Full details in
`docs/ARCHITECTURE.md` and `docs/ALLOC_BENCH.md`.

### Added

- **OPT-B (#67) — O(1) `SegmentTable::contains_base`**: a self-hosted
  open-addressing hash (2048 slots, 16 KiB in the primordial segment)
  replaces the O(count) linear scan. Tombstone encoding for removed
  entries keeps probe chains intact under recycle/decommit churn.
  Matters at DBMS scale (50–100+ live segments).
- **OPT-C (#66) — lazy `stamp_segment_owner`**: `HeapCore` caches the
  last-stamped segment base; cache-hit fast path is a single Relaxed
  load + ownership compare (no Release-store), skipping the costly
  MFENCE on 99 % of hot-segment allocations.
- **OPT-E (#65) — large-segment free-cache** (the headline win):
  1-2 fixed slots per `AllocCore` hold freed OS reservations; the
  next similarly-sized `alloc_large` reuses without mmap.
  **Measured: 4 MiB from 254 µs to 42 ns (~6,000× speedup, 18× faster
  than mimalloc 788 ns); 16 MiB from 701 µs to 48 ns.** Pages stay
  committed inside the cache (eliminates Windows
  `VirtualAlloc(MEM_COMMIT)` cost on hit). Bounded RSS at
  `LARGE_CACHE_SLOTS × MAX_CACHED_LARGE_BYTES = 2 × 64 MiB =
  128 MiB`. Gated on `alloc-decommit` for `SegmentTable` `unregister`
  consistency.
- **OPT-F (#64) — in-place small→small realloc**:
  `AllocCore::realloc` short-circuits when `new_size` resolves to the
  same or smaller size class as `old_size` — returns the same pointer,
  no copy, no alloc, no dealloc. Bench `realloc_in_place_unfavorable`
  improved 28.6 %.
- **OPT-G (#63) — `production` feature alias** + README guidance:
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit"]`
  is the recommended set for long-running multi-thread workloads
  (DBMS, async runtimes); without `alloc-decommit` the
  `SegmentTable` slot-recycle path is disabled and the 1024-slot
  table is a hard ceiling.
- **NUMA-aware path** (Phases A–E of #58): opt-in `numa-aware`
  feature, default OFF. New confined-`unsafe` module
  `src/alloc_core/numa.rs` (Linux `mbind(2)` via `syscall(2)` —
  avoids `libnuma` dep — `MPOL_PREFERRED`; Windows
  `VirtualAllocExNuma`; macOS / miri no-op). Layout-stable
  `SegmentHeader::node_id` (present in every build).
  `reserve_small_segment` / `alloc_large` stamp the current thread's
  NUMA node; `find_segment_with_free` prefers local-node segments
  with foreign-node fallback. Tests: `numa_seam` (5),
  `numa_segment_id` (2), env-guarded `numa_alloc` (3, run with
  `SEFER_NUMA_TEST=1` under multi-NUMA topology). Honest caveat:
  QEMU verifies correctness, not latency-asymmetry; real measurement
  requires 2-socket hardware. See `docs/PHASE_NUMA_DESIGN.md`.
- **SegmentTable slot-recycle** (#60): under `alloc-decommit`, an
  empty decommitted segment NULLs its table slot for future
  re-registration, lifting the hard `MAX_SEGMENTS = 1024` cumulative
  ceiling. Found by the #52 tokio burn-in hitting OOM at >512
  concurrent tasks. New `recycle` (atomic NULL + `release_segment`)
  and partner `unregister` (NULL without release; used by OPT-E
  cache deposit).
- **strict-provenance miri fix** (#59): converted 11 sites of the
  `os::segment_base_of(ptr as usize) as *mut u8` idiom to the
  provenance-preserving `os::segment_base_of_ptr(ptr) =
  ptr.map_addr(|a| a & !(SEGMENT - 1))`. The CI miri job (which
  runs with `-Zmiri-strict-provenance`) now passes
  `decommit_miri_cycle` and `reclaim_offset_unit`.
- **Highload-hardening harnesses**:
  - `examples/soak_xthread.rs` (#51) — N-thread × hours stability
    test (32 / 64 / 128 workers); end-of-run invariant
    `total_alloc == total_free`.
  - `examples/rss_probe.rs` (#53) — measures peak / final RSS under
    sustained asymmetric cross-thread free; smoke: `alloc-decommit`
    keeps peak 13 % lower (91 → 79 MB).
  - `examples/tokio_burn_in.rs` (#52) — SeferMalloc installed as
    `#[global_allocator]` under tokio multi-thread runtime with a
    DBMS-pipeline-shaped workload.
  - `benches/large_realloc.rs` (#54) — three groups (large
    alloc+free, geometric realloc grow, realloc under neighbour
    pressure) comparing SeferMalloc, mimalloc, System through their
    `GlobalAlloc` traits.
- **Low-noise criterion benches** (#62): `benches/heap_xthread.rs`
  (direct ring push/drain, no channels) and
  `benches/heap_async_pattern.rs` (synthetic async-like pattern
  without tokio) — allocator visibility rises from 1.7 % to 13 % of
  self-time vs the noisier `global_alloc` / `large_realloc` benches.
- **Comprehensive verification runs** (one-off, evidence preserved
  in `docs/`):
  - ThreadSanitizer ×3 clean on `race_repro`, `race_norecycle`,
    `global_alloc_mt`, `heap_cross_thread`; ×3 clean on
    `decommit_stale_ring`, `decommit_soak`.
  - aarch64 (qemu-user 8.2.2) 13/13 tests pass, with honest caveat
    about TCG vs real ARM weak-memory.
  - Valgrind memcheck clean on three cross-thread test binaries;
    helgrind / DRD inapplicable to lock-free atomic code (known
    Valgrind limitation — TSan is the right tool).
  - Full Linux feature-matrix (6 combos × 248 tests) all green.
- **Documentation**:
  - `docs/ARCHITECTURE.md` — compact technical overview (synthesis
    of design memos).
  - `docs/PHASE_NUMA_DESIGN.md` (#55) — full NUMA design.
  - `docs/PROFILE_FLAMEGRAPHS.md` (#61) — flamegraph profiling
    report on 4 scenarios with 6 prioritised optimisation
    candidates (OPT-B/C/E/F/G all realised in this release; OPT-H
    documented but deferred as low impact).
  - `docs/ALLOC_BENCH.md` — extensive update with OPT-E large-cache
    numbers, NUMA section, honest verdicts.
- **OSS infrastructure** (preparing for crates.io publication):
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`.
  `Cargo.toml` metadata refreshed for crates.io (description
  mentions both faces, `keywords` rebalanced to `["allocator",
  "arena", "generational", "handle", "lock-free"]`, `categories`
  extended with `concurrency` and `no-std`, `repository` /
  `homepage` / `documentation` URLs added).
- **Build infrastructure**: `cargo-fuzz` metadata fix to enable
  `cargo fuzz build` (#56); `region_ops.rs` idiom corrected to match
  `arbitrary` 1.4.2 (#56); `malloc_macro` registered as
  `[[example]]` with `required-features` (was missing, causing CI
  `cargo test` without `--tests` to fail with E0601).

- **Phase 35 — M6 decommit: return empty segments to the OS** (behind a new
  opt-in `alloc-decommit = ["alloc-core"]` feature; **default OFF — the default
  build is byte-for-byte unchanged**). When a small segment's live-block count
  drops to zero and it is not the current carve target, its payload pages
  `[small_meta_end, SEGMENT)` are returned to the OS (`VirtualFree MEM_DECOMMIT`
  / `madvise MADV_DONTNEED`; no-op under miri) and the segment is reset to a
  clean blank (`bump = small_meta_end`, `BinTable` heads = NULL, payload
  page-map = Free, alloc-bitmap = 0, `decommitted` flag set); the payload is
  recommitted on the first reuse. This bounds steady-state RSS under churn (the
  one honest gap in `ALLOC_BENCH`). Bookkeeping: a new **owner-only** `u32`
  `live_count` field in `SegmentHeader` (present in every build's layout so the
  byte layout is stable; mutated only under the feature) — `+1` on
  `pop_free`/`carve_block` hand-out, `−1` on `dealloc_small`/`reclaim_offset`;
  refill blocks net to zero (carve `+1`, push-to-free-list `−1`). **No
  crossbeam-epoch / M11 barrier is needed** — Variant-2 (Phase 12.6) already
  removed the only reason the original plan reached for epoch: the cross-thread
  freer never dereferences the block (it pushes `offset|class` into the
  in-metadata `RemoteFreeRing`, and metadata pages are never decommitted). The
  full safety argument is recorded in code at the decommit point and in
  `docs/PHASE35_DECOMMIT_DESIGN.md` §1. A **post-decommit stale-free guard**
  (`off >= bump` after the reset) in both `dealloc_small` and `reclaim_offset`
  closes the window where a late free / double-free / stale ring entry targeting
  a reset segment would write a free-list `next` into a decommitted page. NO new
  dependency, NO new `unsafe` site (the OS seam already existed; the bookkeeping
  is plain safe arithmetic through the `node` seam). Tests (`alloc-decommit`):
  `decommit_soak` (decommit fires on `live→0` + recommit readback; counterfactual
  proven — the soak goes red if the hook is disconnected), `decommit_stale_ring`
  (stale ring entry into a decommitted segment is a no-op, no UAF),
  `decommit_miri_cycle` (bounded miri decommit/recommit bookkeeping). Verified:
  full suite green WITH and WITHOUT the feature (incl. `alloc_core_differential`,
  the heap suite, `race_repro`/`race_norecycle`/`global_alloc_mt`), clippy clean,
  miri on the bounded cycle. `heap_cross_segment`'s strict free-list-reuse
  invariant is relaxed under `alloc-decommit` to a bounded-footprint invariant
  (decommitted segments are legitimately re-carved, not free-list-reused).

- **Phase 12 — production multithreaded trust + Phase 12.6 cross-thread-free
  reclaim** (behind `alloc-xthread`). The installed `#[global_allocator]` is now
  a SOUND multithreaded drop-in: heap-as-shard isolation (each heap = a shard
  owned by one thread via a FREE/LIVE slot token), a self-hosted `HeapRegistry`,
  raw-pointer TLS with a never-null fallback heap, and loom-gated segment
  adoption (12.1–12.5). **Phase 12.6** closes the cross-thread-free
  *reclaim*: a non-intrusive per-segment MPSC ring carries each freed block's
  `offset | class` (the freer has the `Layout`; the owner's `page_map` class is
  unreliable for the mixed-class pages a shared bump cursor produces — the true
  root, found via ThreadSanitizer + a Linux free-list audit; NOT a data race).
  The owner reclaims lazily on its alloc-slow-path. This removes the Phase-12.5
  bounded-leak *discard* — cross-thread-freed blocks are now **reused**. Also
  fixed a real `SegmentHeader` data race (field-specific `bump`/`magic`/
  `owner_thread_free` accessors). Verified on Windows + Linux: `race_repro` ×5,
  `race_norecycle` (reliable Linux repro), isolated ring + reclaim unit tests,
  loom protocol/ring models with counterfactuals, full suite, clippy.
  See `docs/RACE_DRAIN_RECLAIM.md` (§13 root, §14 fix) and
  `docs/CROSS_THREAD_STATE_MACHINES.md` (the state-machine spec).
- **Phase 13.1 — O(1) size-class lookup** (`const SIZE2CLASS` table replacing the
  per-alloc linear scan).

- **Phase 11 -- the `malloc` face: `SeferMalloc` (`#[global_allocator]`) +
  no-panic hardening + honest mimalloc verdict** (behind a new opt-in
  `alloc-global = ["alloc"]` feature). `SeferMalloc` is an `unsafe impl
  GlobalAlloc` over the per-thread segment heap (one substrate, two faces: the
  typed `Handle` face and this raw `*mut u8` drop-in face), routing
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed` through the no-panic TLS binding
  `with_heap_try` (returns null / no-ops instead of panicking — a panic in a
  global allocator aborts the process). **No-panic hardening:** the substrate's
  alloc-path panic sites were made graceful — the `alloc_small` `.expect` is
  gone, `SegmentTable::register` and `Segment::reserve` now return `Option`
  (null on failure, never `assert!`-panic). **Reentrancy-freedom (M5)** holds on
  the malloc path (no `Vec`/`Box`/`std::alloc`/`format!`). The `unsafe impl
  GlobalAlloc` is the documented malloc-face seam (every method `// SAFETY:`);
  `unsafe` stays confined. **Honest verdict (`docs/ALLOC_BENCH.md`):** on the
  alloc/dealloc hot path `SeferMalloc` is competitive with `mimalloc` (faster at
  1024 B and on realistic `Vec` push/grow churn; ~1.2-2x behind on small
  fixed-size churn) and consistently **~2.5-5x faster than the Windows system
  allocator** — safe by construction. Proven working as a real
  `#[global_allocator]` for a single-threaded workload
  (`examples/global_allocator.rs`: 100 k-`Vec` + 10 k-`HashMap`), and correct via
  direct-API tests (`tests/global_alloc.rs`: aligned, non-overlapping, reusable,
  realloc-prefix-preserving, 20 k churn). **NOT yet production-trusted:** as a
  *process-wide multithreaded* `#[global_allocator]` (e.g. under libtest's
  reentrancy-heavy harness) the current TLS binding returns null on
  reentrant/early-init/teardown access and aborts — a bootstrap-safe,
  reentrancy-tolerant TLS discipline is the remaining work, alongside the
  deferred heavy gate (`cargo-fuzz` CPU-hours, aarch64 multi-arch CI,
  ThreadSanitizer) and the Phase-10 deferrals (abandoned-heap adoption, M6
  decommit wiring). Honestly documented; for a process-wide allocator today, use
  `mimalloc`.
- **Phase 10 -- cross-thread free (M7), opt-in via `alloc-xthread`** (extends
  the `alloc` feature). Correct, lock-free cross-thread `dealloc` behind a
  new opt-in `alloc-xthread = ["alloc"]` sub-feature. When a thread frees a
  block it does NOT own, it pushes it onto the owning heap's atomic Treiber
  stack via a `compare_exchange` loop (the Phase-7b linearization protocol,
  re-based onto the Phase 8/9 segment substrate). The owner drains the stack
  in bulk on its next operation and returns each block to its per-class
  `FreeList`. O(1) owner lookup via `segment_base_of(ptr)` -> segment header
  -> `owner_thread_free` pointer (a stable `*const AtomicPtr<u8>` stored in
  each segment's header, pointing to the owning heap's `Box`-allocated Treiber
  head). The `ThreadFreeStack` is pure safe composition over
  `core::sync::atomic::AtomicPtr` + the `Node` seam (one new
  `Node::deref_atomic_ptr` in the existing `node` unsafe seam; no new unsafe
  module). **Thread-death soundness via abandonment-leak:** under
  `alloc-xthread`, `Heap::drop` intentionally LEAKS its segments (via
  `ManuallyDrop<AllocCore>`) and the Treiber head (via
  `ManuallyDrop<ThreadFreeStack>`) so that late cross-thread `dealloc` calls
  from other threads never touch unmapped memory or a freed `Box` -- segments
  stay mapped, the `AtomicPtr` stays allocated. This is a BOUNDED leak on
  thread death (one heap per thread), acceptable for the target long-lived
  thread-pool workload. Full abandoned-heap adoption (reclaiming leaked
  segments) is a Phase 11 deliverable. **Default `alloc` (no `alloc-xthread`)
  is unchanged Phase 9:** the single-thread-owner allocator with no
  `ThreadFreeStack`, no owner stamping, and normal segment release on
  `Heap::drop` (sound: single owner, no cross-thread refs). **Large / unstamped
  cross-thread free:** under `alloc-xthread`, a cross-thread free of a large
  block (`SegmentKind::Large`) or a block in an unstamped segment
  (`owner_thread_free == null`) is a documented no-op -- the block is
  conservatively leaked until the owning heap drops (or until Phase 11
  adoption). This avoids mis-accounting and is sound. **Decommit (M6) is NOT
  delivered** -- the `os::decommit_pages` / `os::recommit_pages` seam landed in
  Phase 10 (ready to wire) but is not integrated into the heap path. M6 is a
  Phase 11 deliverable. The soak test (`tests/heap_soak.rs`) asserts bounded
  segment growth via free-list reuse, not via decommit. Verification: **loom**
  model-check (`tests/loom_thread_free.rs`, 2 pushers + 1 drainer,
  `preemption_bound = 3`) with a proven counterfactual -- the naive non-CAS
  push demonstrably loses blocks under loom (the
  `counterfactual_naive_push_loses_blocks` test `#[should_panic]`s).
  Cross-thread differential proptest (`tests/heap_cross_thread.rs`, 64 cases,
  multiple threads, pattern write+readback -- non-vacuous). Soak test
  (`tests/heap_soak.rs`) -- bounded segment usage under sustained churn.
  Miri-clean on the cross-thread atomic seam (`tests/heap_miri_xthread.rs`,
  2-thread alloc/free, with `-Zmiri-ignore-leaks` for the intentional
  abandonment-leak).
- **Phase 9 -- per-thread heap + intrusive free lists (the lock-free fast
  path)** (behind a new opt-in `alloc` feature = `["alloc-core"]`). Each
  thread owns a `Heap` with per-size-class intrusive free lists stored inside
  the freed blocks themselves (via the Phase 8 `node` seam -- zero metadata
  allocation). The hot path (`alloc_small` / `dealloc_small`) is a single
  pointer read/write -- no lock, no atomic, no `Vec`/`Box`/`std::alloc` (M5
  reentrancy-freedom upheld). On free-list drain, a batch refill carves
  blocks from the Phase 8 `AllocCore` substrate. TLS heap binding via
  `std::thread_local!` with lazy, allocation-free init (`with_heap`); heap
  released on thread exit. Large/huge allocations route through the Phase 8
  dedicated-segment path. No new `unsafe` module -- the heap is pure safe
  composition over the Phase 8 `os` + `node` seams. Cross-thread free is
  Phase 10. Differential proptest (M1--M4 through the heap, 64 cases),
  targeted unit tests (alignment, reuse, refill, realloc, churn, multi-thread
  isolation), miri-clean. Single-thread throughput bench vs mimalloc and the
  system allocator (`benches/heap_alloc.rs`, `docs/HEAP_BENCH.md`): the heap
  matches the system allocator but is ~7--12x slower than mimalloc on the hot
  path; the architecture is structurally correct (same design as mimalloc) and
  the constant-factor gap is implementation overhead targeted for Phase 11.
- **Phase 8 — segment substrate + self-hosted metadata (the Membrane
  Inversion)** (behind a new opt-in `alloc-core` feature). The foundation of a
  real general-purpose allocator: the safe slot-table discipline stops
  *consuming* `Vec<T>` and starts *governing* OS-backed, SEGMENT-aligned memory
  (default 4 MiB), with the allocator's own metadata **carved from the segments
  it manages** (no `Vec`/`HashSet`/`std::alloc` on any alloc path). `unsafe`
  stays confined to exactly two documented seams: `os` (the OS aperture —
  `VirtualAlloc`/`VirtualFree` on windows, `mmap`/`munmap` on unix, via an
  over-reserve+trim for SEGMENT alignment; replaces `std::alloc` entirely) and
  `node` (the intrusive free-list node r/w, generalising the `hand` discipline).
  Everything between — `SegmentTable` (self-hosted generational registry),
  `PageMap`/`BinTable` (per-segment page descriptors + per-class free bins), the
  primordial `bootstrap`, the ~40-class size scheme, and `AllocCore`'s
  single-threaded `alloc`/`dealloc`/`realloc`/`alloc_zeroed` — is pure safe
  integer arithmetic (the Cartographer). Invariants **M1–M8** documented
  (`docs/INVARIANTS.md`, spec in `docs/ALLOC_PLAN.md` §4) and encoded as a
  differential proptest (M1–M4 vs a reference model), targeted unit tests, and a
  **runtime reentrancy audit (M5)** — a counting global allocator proves the
  alloc path never recurses into `std::alloc`. The core is **miri-clean**:
  because miri cannot execute the raw OS FFI, the `os` aperture has a
  `#[cfg(miri)]`-only fallback to `std::alloc` (test instrumentation; the
  production aperture is unchanged and the M5 proof runs without miri). Single
  confined unsafe per seam; `forbid`/`deny(unsafe_code)` everywhere else.
  **Supersedes** the Phase-4 `byte_region.rs` `std::alloc` fallback and its
  `Vec`/`HashSet` metadata. Per-thread heaps (Phase 9), cross-thread free +
  decommit (Phase 10), and the `GlobalAlloc` face (Phase 11) build on this.
- Initial scaffold of the `sefer-alloc` crate.
- Single-threaded `Region<T>` — a thin typed membrane over the
  [`slotmap`](https://crates.io/crates/slotmap) crate (`insert` / `get` /
  `get_mut` / `remove` / `contains` / `iter` / `clear`, all `O(1)`), built under
  `#![forbid(unsafe_code)]`; `slotmap`'s audited `unsafe` owns the dense
  generational engine, including version-saturation slot retirement.
- Typed, copyable `Handle<T>` — a newtype over `slotmap::DefaultKey` with
  hand-written `Copy`/`Eq`/`Hash`/`Debug` impls that hold for every `T`.
- `SyncRegion<T>` — the always-shippable concurrent baseline: a
  `RwLock<Region<T>>` with a guard API plus one-shot convenience methods, with
  poison recovery, still `#![forbid(unsafe_code)]`.
- `LockFreeRegion<T>` (behind the opt-in `experimental` feature) — **lock-free
  reads** via `arc-swap` RCU with page-granularity copy-on-write: readers load
  an immutable snapshot and resolve handles without any lock; rare writers
  serialise, copy only the touched page, and publish atomically. Values live
  behind `Arc<T>`; reclamation is plain `Arc` refcounting. **Zero `unsafe` of
  our own** — the crate stays `#![forbid(unsafe_code)]` with the feature on.
- `EpochRegion<T>` (behind `experimental`) — the fixed-capacity epoch tier with
  O(1) per-slot writes: lock-free reads via a seqlock-validated
  `(generation, value)` publication protocol and `crossbeam-epoch` reclamation.
  Introduces the crate's **single confined `unsafe` organ** (`concurrent::hand`,
  `AtomicSlot<T>`); confinement is compiler-enforced (`#![deny(unsafe_code)]`
  crate-wide under the feature, lifted only in that one module). The publication
  protocol is **loom-model-checked**; live values are dropped on region drop
  (I5). miri cannot run the tier only because `crossbeam-epoch`'s global
  collector is not miri-clean upstream — our `unsafe` is not implicated.
- `ShardedRegion<T>` and `ShardedHandle<T>` (behind `experimental`, Phase 7a) —
  **N-way parallel writes** via the single-writer principle: a `Box<[EpochRegion]>`
  of shards plus a thread-local router that lazily binds each writer thread to one
  shard (atomic round-robin), so two writers in different shards never meet on a
  lock. Reads stay the untouched lock-free `EpochRegion` seqlock. **Pure safe
  composition — zero new `unsafe`**; the module compiles under the crate's
  unsafe-confinement. `ShardedHandle` carries the shard id so reads/removes route
  back to the owning shard. Honest 7a edge: a claimed shard is not released
  (fits a bounded pool of long-lived threads; the shard lifecycle + lock-free
  cross-thread remove land in 7b). A multi-shard differential proptest (I1–I4
  across shards) and a routed concurrent stress test guard it; a write-scaling
  bench (`benches/sharded_write.rs`) compares it to the `SyncRegion` / `Arc<Mutex>`
  baselines.
- **Phase 7b — lock-free cross-thread removal + shard lifecycle** (behind
  `experimental`). A non-owner thread can now `remove` a handle WITHOUT taking
  the owning shard's writer mutex: `AtomicSlot::try_evict_at` performs a
  generation **`compare_exchange`** as the single linearization point — exactly
  one thread wins per generation, so exactly one schedules `defer_destroy` and
  decrements the (now `AtomicUsize`) live count (no double-free, no
  lost-live-value). The freed index is enqueued to a per-shard remote-free queue
  the owner drains on its next op (free list stays owner-only). `EpochRegion`
  gains `remote_evict`; `ShardedRegion::remove` routes owner-path vs lock-free
  remote-path by the calling thread's shard. Shards are now **releasable**: a
  thread-local `Drop` guard frees the shard's `occupied` token on thread exit,
  so a dead thread's shard can be adopted by a new thread while its live slots
  stay resolvable (reads are ownership-free). The relaxed "any thread may evict"
  contract is **loom-model-checked** (`tests/loom_sharded.rs`, 1 owner + 1
  remote-remover + 1 reader, `preemption_bound = 3`) — verified to FAIL on the
  naive load-then-swap protocol. `unsafe` stays confined to `concurrent/hand.rs`.
- **Phase 7c — thread-per-core pinning** (behind a new opt-in `pinning` feature
  = `["experimental", "dep:core_affinity"]`). `ShardedRegion::bind_current_thread_to_shard`
  deterministically routes a thread to a specific shard (the auto round-robin
  claim cannot), and `PinnedRunner` spawns one worker per core, pins worker *i*
  to core *i* (via `core_affinity`, a safe wrapper — **zero new `unsafe`**), and
  binds it to shard *i* — so `shard == core` and the hot path has no lock and no
  cross-shard contention (also why it composes with `glommio`/`monoio`/`tokio`
  current-thread-per-core without "lock across `.await`"). `core_affinity` is an
  **optional** dependency: the default and `experimental` builds do not pull it.
  Pinning is best-effort (honoured per OS); the shard binding (the routing
  truth) always holds, so tests assert routing, not affinity. A `pinned_write`
  bench compares pinned vs unpinned with an honest, workload-dependent verdict.
- **Phase 7d — `ShardedByteArena`** (behind a new opt-in `byte-sharded` feature
  = `["byte"]`, research-flagged). N per-thread `ByteRegion` shards
  (`Box<[Mutex<ByteRegion>]>`) for parallel raw allocation: a thread binds to its
  own shard via a TLS round-robin router, so threads in different shards never
  contend on one lock. Cross-thread `dealloc`/`realloc` route to the owning shard
  via a scan over `ByteRegion::contains_ptr` (safe pointer-comparison, no
  dereference) — a pointer is never freed against the wrong shard. `prewarm()`
  carves a chunk per shard and touches its pages up front to remove cold-start
  latency (callable from a background thread; the arena is `Send + Sync`). The
  only added `unsafe` is a one-line `unsafe impl Send for ByteRegion` (the region
  owns all its memory; access is `Mutex`-serialised) — everything else is safe
  composition; `unsafe` stays confined to `src/byte/*`. Correctness (cross-thread
  free, concurrent per-shard churn, bounded chunk growth, realloc byte
  preservation) is covered by `tests/byte_sharded.rs` and is **miri-clean**.
  Honest verdict (`docs/BYTE_SHARDED_BENCH.md`): it parallelises across shards
  but is NOT a `mimalloc` competitor and never returns memory to the OS until
  drop — research, not production.
- `ByteRegion` and `ByteAllocator` (behind the research-flagged `byte` feature)
  — the descent to raw bytes: a size-classed free-list byte arena whose
  placement logic is pure safe integer arithmetic (the Cartographer), with the
  single irreducible `*mut u8` aperture confined and documented, plus an
  experimental `unsafe impl GlobalAlloc` delegating through a `Mutex`. The
  second confined-`unsafe` module; confinement stays compiler-enforced. The
  whole byte tier is **miri-clean**. Honest scope: it does not aim to beat the
  system allocator / `mimalloc` (see `docs/BYTE_BENCH.md`); resocks5's global
  allocator stays `mimalloc` regardless.
- Safety invariants I1–I5 documented (`docs/INVARIANTS.md`) and encoded as
  unit tests plus a proptest differential harness against a reference model
  (`tests/differential.rs`).
- Full detailed implementation plan — per-phase goals, deliverables, steps, and
  gates, plus dependency DAG, risk register, decisions log, and open questions
  (`docs/PLAN.md`) — alongside architecture notes (`docs/DESIGN.md`).
- Dual MIT / Apache-2.0 licensing; MSRV pinned to 1.88.
