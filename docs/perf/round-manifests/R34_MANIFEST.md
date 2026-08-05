# Round 34 manifest — commit classification & per-item verdict

**Generated (extended 2026-08-05 — task #550, review findings F5+F6 from
`docs/reviews/2026-08-05-round34-readonly-review.md`).** The commit table in
§1 is reproduced verbatim from
`git log --reverse --format="%H|%cI|%s" 40241b0..c5db553` (the exclusive
lower bound is Round 33's closing checkpoint `40241b0`; the inclusive upper
bound is Round 34's actual closing commit `c5db553`, R34-27/task #546). No
commit SHA was hand-transcribed — every 40-hex SHA below is byte-identical
to `git log` output. This manifest is the FIRST real instance of the
per-round manifest artifact required by CLAUDE.md's "Round manifest" rule
(added R34-24/task #543); it doubles as the reference example future rounds
should match. **This revision corrects an under-count in the original
version**: the manifest as first written (commit `4ba188a`) stopped at
`827a57a` (38 commits) because that was HEAD at the time R34-24 wrote it —
but the round continued for 5 more commits after that (`4ba188a`, `9b06b56`,
`7758f7a`, `8cb89ea`, `c5db553`), which were never folded back in. The full
round span is **42 commits** in `40241b0..8cb89ea` (the last work commit) or
**43 commits** including the closing checkpoint commit `c5db553` — both
counts independently re-verified via
`git log --oneline 40241b0..8cb89ea | wc -l` (→ 42) and
`git log --oneline 40241b0..c5db553 | wc -l` (→ 43) before this revision was
written.

**Purpose.** A single per-round artifact that records, in one
machine-checkable place: (1) every commit in the round, classified by its
R30-12 taxonomy category; (2) the round's net `production` default-feature
impact; (3) measured wall-clock/Ir/RSS deltas; and (4) the final verdict per
item. This makes the R30-12 commit-prefix taxonomy **self-checking at the
round level** — a reader (or reviewer) can scan the "net default-feature
impact" line and the production-source commit list to confirm that no opt-in
or measurement-only result is being framed as a default speedup, without
opening every commit body or cross-referencing the CHANGELOG header. This is
exactly the failure mode an independent bench review flagged: "opt-in results
must not be presented as acceleration of ordinary `--features production`."

**Round span.** 2026-08-04T11:43+02:00 (first commit) → 2026-08-05T01:50+02:00
(last commit, closing checkpoint `c5db553`). 43 commits (42 work/process
commits + 1 closing checkpoint), 1 calendar day (2026-08-04 into
2026-08-05).

**Net default-feature impact.** `production`'s feature composition is
**UNCHANGED** — confirmed by R34-21/task #540 (`ae9c9c3`,
`docs(config): sync production bundle + lib.rs seam inventory with reality`),
which re-verified the `production` Cargo.toml feature list against the actual
`src/` surface and found them in sync. A new `internals` Cargo feature was
ADDED (R34-3/task #522) for build-gating the crate's internals-reaching
`src/` surface, but it is NOT part of `production`'s default bundle. Six
`fix(perf)` commits touch production-reachable `src/` code (memory-ordering
hardening, bounded-loop addition, field-reset correctness, OOM-handling
widening, panic-safety RAII guards, struct-size compile-time pin) — all are
correctness/consistency fixes per R30-12's `fix(perf)` definition: no measured
speedup is claimed, and no production default's observable *algorithm*
changed (only its internal correctness/consistency did; R34-11's bounded
catch-up loop is the closest to a behavioral change — it is a hard-capped
8-iteration loop, never a spin-loop, that changes observable large-cache
retention: final gap 3→1 segment in the measured sparse arm, peak unchanged
at 4 segments — but adds no new allocation path and claims no new speedup).

---

## §1. Commit classification (verbatim from `git log`)

Reproduce: `git log --reverse --format="%H %s" 40241b0..c5db553`

| # | SHA (full) | Commit prefix | Subject (truncated) | R34 task | Category |
|---|------------|---------------|---------------------|----------|----------|
| 1 | `27879af97fbea3f10c88a0274fe776578acb75c3` | `feat(api)` | gate alloc_core/global/registry behind `internals` | R34-3 #522 | **feat(api) — build-gating** (visibility reorg, no `src/` behavior change) |
| 2 | `b47cc6aa62c46b85ed871952b961054d8c39aa1a` | `feat(api)` | gate internals-reaching tests behind `internals` | R34-3 #522 | **feat(api) — build-gating** |
| 3 | `0762772509e5570c27ebd0f7cb2f5a681b7a65e7` | `feat(api)` | sync CI/check-matrix/release gates with `internals` | R34-3 #522 | **feat(api) — tooling sync** |
| 4 | `f9ae91ffa13eaebae62be238bc206f3a24cb22a1` | `docs(process)` | fix stale test-file count after R34-3 | — | **docs-only** |
| 5 | `4aaca52319738d2d28904251b0cbbf5facc6b739` | `docs(process)` | correct b3b18bb's false "paired_ab_runs never committed" framing | R34-1 #520 | **docs-only** |
| 6 | `b45b8248f1d10ef887a25d85569c120c682b4deb` | `docs(process)` | index Round-32/33 review + audit findings into open-items indexes | R34-2 #521 | **docs-only** |
| 7 | `00a1c594088ae5f00039f5069365b798fe938f57` | `docs(process)` | untrack readonly review files committed out-of-scope by R34-2 | — | **docs-only** |
| 8 | `353cc05050bbdbb97d39161fc8c5de83bbf7d518` | `fix(security)` | close RUSTSEC-2026-0204 by bumping crossbeam-epoch 0.9.18→0.9.20 | R34-4 #523 | **dependency-security** (Cargo.lock + deny.toml; no `src/` change) |
| 9 | `fd54ddc9ccb44fe6cd82b5f27df6dc32a106b56e` | `test(miri)` | add concurrent multi-producer SMALL-block RemoteFreeRing push/drain | R34-5 #524 | **test-only** |
| 10 | `b47a2611dd626523ea27ff39e3cef801c2cf6c55` | `fix(test)` | scripts/miri.mjs's local matrix silently omitted the `internals` feature | — | **test-tooling-fix** |
| 11 | `91ff1ddc67103b91bce2a8ffbc8b44fe0a4cba6a` | `fix(test)` | scripts/tsan.mjs's local passes silently omitted the `internals` feature | — | **test-tooling-fix** |
| 12 | `d4c4f8b042c84e1058306c3ba589090060c43778` | `docs` | commit Round-33-closing session checkpoint | — | **docs-only** (checkpoint) |
| 13 | `a9edc87300ca540f5fe7f5318268ca290b5ce81f` | `fix(perf)` | promote RemoteFreeRing cached_head from Relaxed to Acquire/Release | R34-6 #525 | **fix(perf) — production-source** (`src/alloc_core/remote_free_ring.rs`; `alloc-xthread` in `production`) |
| 14 | `7aeee2d3f626f767668a2720ead83cf5d7c04b24` | `fix(test)` | rustfmt drift left by R34-3's internals cfg-gate mechanical edit | — | **test-formatting-fix** |
| 15 | `a3831e5116cca281f3a447c882539539066dca35` | `bench` | R34-7 causal subprocess comparative harness (MVP) | R34-7 | **bench-only** |
| 16 | `b23d7c5aad784c12867c224e6cad3c453480b0ca` | `bench` | R34-8 control-arm drift guard for bench-table run-over-run appendix | R34-8 | **bench-only** |
| 17 | `0e29fc2007fd5d3330216e0950ac234860e0624d` | `bench` | R34-9 correct global_alloc bench labels/units to match what they measure | R34-9 | **bench-only** |
| 18 | `94e133ace444c142b8d7cc98ccbfb859035e036f` | `bench` | R34-10 sparse-decay accumulation gate (stride retention bound does NOT hold) | R34-10 | **bench-only** |
| 19 | `5c1142f5b128ef1134e397abd5139327f3135b86` | `fix(perf)` | correct R34-10 CSV's base_commit off-by-one (parent→landing SHA) | R34-10 | **fix(data) — CSV correction** |
| 20 | `73dcecab4268e2025a247985127b08b527a47212` | `fix(perf)` | add bounded catch-up loop to maybe_decay_large_cache | R34-11 | **fix(perf) — production-source** (`src/alloc_core/alloc_core_large_cache.rs`; `alloc-decommit` in `production`) |
| 21 | `a100647a73085df15287739c4f1833652fba633c` | `bench` | R34-11 catch-up decay gate (sparse gap reduced + R32-8 benefit preserved) | R34-11 | **bench-only** |
| 22 | `43115cf77290875933564040810f7f50707a9b5a` | `fix(perf)` | correct R34-11 CSV's base_commit off-by-one (parent→landing SHA) | R34-11 | **fix(data) — CSV correction** |
| 23 | `9e702668dab0a0d11adc807defb3d47394334806` | `bench` | R34-12 clean A/B re-gate of RemoteFreeRing shadow-head | R34-12 #531 | **bench-only** |
| 24 | `1c686f89ccd4ad6e14469e32bf7f677922acc22b` | `docs` | keep OWN_CACHE_SIZE=16; fix stale "(4)"/"& 3" doc comments to parametric OWN_CACHE_SIZE | R34-13 #532 | **docs-only** (`src/` comment-only) |
| 25 | `7ef5a465cc23e20c518f9163520640aebc7a7ee0` | `fix(perf)` | reset owner/deferred fields on large-cache hit; re-derive targeted-write safety arg | R34-14 #533 | **fix(perf) — production-source** (`src/alloc_core/alloc_core_large.rs` + `segment_header.rs`; `alloc-decommit` in `production`) |
| 26 | `49929d064a3c789a37961c13e3b04e454aee3f55` | `fix(perf)` | widen free-path chunk-materialisation OOM from abort to graceful return | R34-15 #534 | **fix(perf) — production-source** (`src/registry/bootstrap.rs` + `heap_core_xthread.rs`; production core) |
| 27 | `e55000684c14d108fbad169e6558defbcda5d2aa` | `docs(global)` | correct "No-panic" contract — enumerate 5 invariant tripwires + panic=abort guarantee | R34-16 #535 | **docs-only** (`src/lib.rs` comment) |
| 28 | `c270b0c1be0fc585bd8b31ee6ea011b570896712` | `fix(perf)` | add RAII unwind guards to RemoteFreeRing::drain head-publish and fallback heap_ptr init-state | R34-17 #536 | **fix(perf) — production-source** (`src/alloc_core/remote_free_ring.rs` + `src/global/fallback.rs`; production core) |
| 29 | `3281ebc0a682ad6e1b177aed00c0d76285ec23c3` | `fix(perf)` | pin size_of::\<HeapCore\>() <= 8 KiB against silent stack-pressure growth | R34-18 #537 | **fix(perf) — production-source** (`src/registry/heap_core.rs`; compile-time const-assert pin only) |
| 30 | `0047cf26d4b2e323b2c05e4f1df0277866e70d67` | `test(loom)` | pin F10 shadow fast path over a recycled slot with concurrent drain | R34-19 #538 | **test-only** |
| 31 | `ae110737eed444d048e51d8288780fc416206b72` | `fix(test)` | restore RingModelShadow2::full_check clobbered by a review-time race | R34-19 #538 | **test-fix** |
| 32 | `db7b30f9a79a05bdd4f0b93f1ebd61e720bfdce3` | `build(ci)` | add ASan gate (per-PR) + scheduled fuzz-run gate (weekly) | R34-20 #539 | **ci-tooling** |
| 33 | `36b4b3ec031235452c9df861555ea18b75108ffe` | `fix(build)` | sync fuzz/Cargo.lock with the workspace's tagged-index-stack dependency | R34-20 #539 | **build-fix** |
| 34 | `ae9c9c3636ec6b08cc0f8c4af0124cef0944a9d9` | `docs(config)` | sync `production` bundle + lib.rs seam inventory with reality | R34-21 #540 | **docs-only** (confirms `production` unchanged) |
| 35 | `b2291876163c9a253340788a6a67d6d256e1109d` | `fix(perf)` | pin PerClass magazine layout prose to its const-asserts + sweep bench-review doc drift | R34-22 #541 | **docs/test** (`src/` comment-only + drift-detection test pin) |
| 36 | `19b191878e0fb5959acac26a8c553eb58536bd6d` | `bench` | build R34-23 direct-realloc + real-Vec gate harnesses with path-activation oracle | R34-23 #542 | **bench-only** |
| 37 | `ba716a0d3a1092b400514c059fe157dd99c61836` | `bench` | R34-23 gate report — correct README realloc_grow_geometric (~40×→~2×), confirm neighbour_pressure (~3350×), NO-GO | R34-23 #542 | **bench-only** |
| 38 | `827a57abdb24ab5bef205d3897b801c41eab2f60` | `docs` | complete the R34-23 realloc README correction — two more stale ~40×/~1,500× sites | R34-23 #542 | **docs-only** |
| 39 | `4ba188a960e87e072e5f06ea8e9988f90cfd874d` | `docs(process)` | artifact storage policy + round-manifest artifact + OPEN_ITEMS structural rule + landing-SHA wording | R34-24 #543 | **docs-only** (CLAUDE.md process amendments; first version of this manifest) |
| 40 | `9b06b566ff114ef301d6b753dd9670036c96fd86` | `docs` | fix a false-precision count in R34-24's own artifact-storage-policy amendment | R34-24 #543 | **docs-only** (self-correction) |
| 41 | `7758f7ad9f53b4445980516f4a2568148f13bdfe` | `docs(process)` | R34-25 small-magazine provenance design — feasibility study, NEED-MORE-RESEARCH lean NO-GO | R34-25 #544 | **docs-only** (design-only research; no `src/`/`Cargo.toml`/tests/benches change) |
| 42 | `8cb89eadde54829220f5ed48859faee916914f0a` | `docs(process)` | R34-26 page-run layer design gate — in-place-grow angle, NEED-MORE-DATA lean NO-GO, no real consumer found | R34-26 #545 | **docs-only** (design-only research; no `src/`/`Cargo.toml` change; `docs/perf/OPEN_ITEMS.md` item 3 updated) |
| 43 | `c5db55330a84789fcbfb2b041aaff65f397844c5` | `docs` | extend Round 34 CHANGELOG with all 26 tasks + commit session checkpoints | R34-27 #546 | **docs-only** (round-closing CHANGELOG completion; not itself a numbered R34-N work item) |

### Aggregate counts

| Category | Count | Commits |
|----------|-------|---------|
| **fix(perf) — production-source** (touches production-reachable `src/`) | 6 | a9edc87, 73dceca, 7ef5a46, 49929d0, c270b0c, 3281ebc |
| **opt-in-source** (non-default feature code changed) | 0 | — |
| **bench-only** (judge/probe/gate-report/harness; no shipping code) | 8 | a3831e5, b23d7c5, 0e29fc2, 94e133a, a100647, 9e70266, 19b1918, ba716a0 |
| **docs-only** (no runtime code; includes comment-only `src/` edits) | 15 | f9ae91f, 4aaca52, b45b824, 00a1c59, d4c4f8b, 1c686f8, e550006, ae9c9c3, b229187, 827a57a, 4ba188a, 9b06b56, 7758f7a, 8cb89ea, c5db553 |
| **feat(api) — build-gating** (visibility reorg, no behavior change) | 3 | 27879af, b47cc6a, 0762772 |
| **test-only / test-fix** (new tests or test infrastructure) | 6 | fd54ddc, b47a261, 91ff1dd, 7aeee2d, 0047cf2, ae11073 |
| **fix(data) — CSV correction** | 2 | 5c1142f, 43115cf |
| **dependency-security** | 1 | 353cc05 |
| **ci/build tooling** | 2 | db7b30f, 36b4b3e |
| **Total** | **43** | |

**Self-check (R30-12 taxonomy).** The CHANGELOG header says "Runtime
improvements this round: 0" — this manifest confirms it: zero `perf(runtime)`
commits, zero `perf(opt-in)` commits, and the 6 production-source commits all
carry `fix(perf)` (correctness/consistency, no measured speedup claimed). No
opt-in or measurement-only result is framed as a default speedup.

---

## §2. Per-item verdicts

| Item | Commits | Category | Verdict / outcome |
|------|---------|----------|-------------------|
| **R34-1** (#520) | `4aaca52` | docs(process) | **CORRECTION** — corrected R33-5's `b3b18bb` false "paired_ab_runs never committed" framing (12 prior commits existed; 33 files remain force-tracked) |
| **R34-2** (#521) | `b45b824`, `00a1c59` | docs(process) | **INDEXED** — Round-32/33 review + 2026-08-04 audit/bench-review findings indexed into `OPEN_ITEMS.md` + `CORRECTNESS_OPEN_ITEMS.md`; out-of-scope readonly review files untracked |
| **R34-3** (#522) | `27879af`, `b47cc6a`, `0762772` | feat(api) | **SHIPPED** — new `internals` Cargo feature gates the crate's internals-reaching `src/` surface (visibility reorg, no behavior change); CI/check-matrix/release gates synced |
| **R34-4** (#523) | `353cc05` | fix(security) | **CLOSED** — RUSTSEC-2026-0204 closed by bumping crossbeam-epoch 0.9.18→0.9.20; `deny.toml` ignore entry removed |
| **R34-5** (#524) | `fd54ddc` | test(miri) | **ADDED** — concurrent multi-producer SMALL-block RemoteFreeRing push/drain miri coverage |
| **R34-6** (#525) | `a9edc87`, `7aeee2d` | fix(perf) + fix(test) | **SHIPPED** — RemoteFreeRing `cached_head` promoted from Relaxed to Acquire/Release, closing R32-11's F-1 ordering-proof gap (memory-ordering correctness hardening in production-reachable `alloc-xthread` path); a separate rustfmt-drift cleanup left by R34-3's mechanical cfg-gate edit caught and fixed in the same task (`7aeee2d`) |
| **R34-7** | `a3831e5` | bench | **INFRASTRUCTURE** — causal subprocess comparative harness MVP (no verdict; tool deliverable for future rounds) |
| **R34-8** | `b23d7c5` | bench | **INFRASTRUCTURE** — control-arm drift guard for bench-table run-over-run appendix |
| **R34-9** | `0e29fc2` | bench | **CORRECTION** — corrected `global_alloc` bench labels/units to match what they measure |
| **R34-10** | `94e133a`, `5c1142f` | bench + CSV fix | **FINDING** — sparse-decay stride retention bound does NOT hold (large-cache entries CAN accumulate beyond the assumed bound); no production change; CSV base_commit corrected (parent→landing SHA) |
| **R34-11** | `73dceca`, `a100647`, `43115cf` | fix(perf) + bench + CSV fix | **GO** — added bounded catch-up loop to `maybe_decay_large_cache` (production-reachable `alloc-decommit`); re-gate confirmed sparse gap reduced + R32-8 benefit preserved; CSV base_commit corrected |
| **R34-12** (#531) | `9e70266` | bench | **RE-VERIFIED** — clean A/B re-gate of RemoteFreeRing shadow-head (confirms R32-11's result) |
| **R34-13** (#532) | `1c686f8` | docs | **CORRECTION** — kept `OWN_CACHE_SIZE=16`; fixed stale `"(4)"`/`"& 3"` doc comments to parametric `OWN_CACHE_SIZE` |
| **R34-14** (#533) | `7ef5a46` | fix(perf) | **SHIPPED** — reset owner/deferred fields on large-cache hit; field-classification pinned; targeted-write safety arg re-derived (production-reachable `alloc-decommit`) |
| **R34-15** (#534) | `49929d0` | fix(perf) | **SHIPPED** — widened free-path chunk-materialisation OOM from abort to graceful return (robustness fix in production free path) |
| **R34-16** (#535) | `e550006` | docs(global) | **CORRECTION** — corrected "No-panic" contract: enumerated 5 invariant tripwires + panic=abort guarantee |
| **R34-17** (#536) | `c270b0c` | fix(perf) | **SHIPPED** — RAII unwind guards for `RemoteFreeRing::drain` head-publish + fallback `heap_ptr` init-state (panic-safety in production core) |
| **R34-18** (#537) | `3281ebc` | fix(perf) | **SHIPPED** — `size_of::<HeapCore>() <= 8 KiB` compile-time pin against silent stack-pressure growth (const-assert only; no runtime change) |
| **R34-19** (#538) | `0047cf2`, `ae11073` | test(loom) + test-fix | **SHIPPED** — loom test pinning F10 shadow fast path over a recycled slot with concurrent drain; restored `RingModelShadow2::full_check` clobbered by a review-time race |
| **R34-20** (#539) | `db7b30f`, `36b4b3e` | build(ci) + build-fix | **SHIPPED** — ASan gate (per-PR) + scheduled fuzz-run gate (weekly) added to CI; `fuzz/Cargo.lock` synced |
| **R34-21** (#540) | `ae9c9c3` | docs(config) | **VERIFIED** — `production` bundle + lib.rs seam inventory synced with reality; confirms `production` composition unchanged |
| **R34-22** (#541) | `b229187` | fix(perf) → docs/test | **CORRECTION** — PerClass magazine layout prose pinned to its const-asserts; bench-review doc-drift swept; new drift-detection test pin |
| **R34-23** (#542) | `19b1918`, `ba716a0`, `827a57a` | bench + bench + docs | **NO-GO** — NO-GO for `large-reserved-capacity` on geometric realloc; corrected README `realloc_grow_geometric` (~40×→~2×); confirmed `neighbour_pressure` (~3350×) |
| **R34-24** (#543) | `4ba188a`, `9b06b56` | docs(process) | **RULES** — CLAUDE.md amendments: artifact storage policy (hybrid), round-manifest rule, OPEN_ITEMS structural rule, landing-SHA wording; introduced this manifest's first version (`4ba188a`); self-corrected a false-precision raw-log count in the same amendment's own text (`9b06b56`) |
| **R34-25** (#544) | `7758f7a` | docs(process) | **NEED-MORE-RESEARCH, lean NO-GO** — design-only feasibility study of a small-magazine provenance scheme for the 16/64 B bulk-burst gap; the headline lever (caching the segment base) is very likely net-negative on first principles (the prior "9.03 Ir" attribution to `segment_base_of_ptr` was a measurement-probe artifact, not the real inlined ~1 Ir cost); the one sound lever (skip-clear for fresh-carve blocks) is cold-path-only and does not touch the steady-state recycled-hit gap; no prototype built; recommends a code-free disassembly check as the next step. No `src/`/`Cargo.toml`/tests/benches change. |
| **R34-26** (#545) | `8cb89ea` | docs(process) | **NEED-MORE-DATA, lean NO-GO** — design-gate study of a page-run layer (8-16 MiB arena, buddy/run bitmap) with in-place adjacent-run grow for the 256 KiB-2 MiB range; confirms the architecture COULD support in-place grow and would address the prior `medium-classes` NO-GO's root cause, but the mandatory precondition — a real consumer in that size range — is NOT met (every workload touching it is a synthetic adversarial harness; R29-5 found promotion is 0.054% of allocations); no prototype built. `docs/perf/OPEN_ITEMS.md` item 3 updated with the in-place-grow angle and a realloc-WIN (not merely parity) promotion criterion. No `src/`/`Cargo.toml` change. |
| **R34-27** (#546) | `c5db553` | docs | **CLOSING** — not a numbered review-remediation or research task; extends the Round 34 CHANGELOG section with dense per-task bullets for all 26 work items (R34-1 through R34-26) and commits the round's closing session checkpoints, per the established babygoal closing-sequence pattern (mirrors R33-14). Found and closed a framing gap while writing it: R34-2 had never actually been mentioned in `CHANGELOG.md` at all, despite the heading's original text claiming it was "already inline-referenced under Round 33's R33-5 entry" (true only for R34-1, not R34-2). |

---

## §3. Measured deltas (wall-clock / Ir / RSS)

Round 34 produced no `perf(runtime)` or `perf(opt-in)` commits, so there are
no measured speedup deltas to report. The measurement work that did land is
bench-only or correctness-verification:

- **R34-10** (`_raw_r34_10_sparse_decay_gate.log`, 145 KiB): sparse-decay
  accumulation — confirmed the stride-retention bound does NOT hold (finding,
  not a speedup). No production code changed in response to this finding
  alone; R34-11's bounded catch-up loop is the structural response.
- **R34-11** (`_raw_r34_11_catchup_decay_gate.log`): catch-up decay gate —
  observable retention policy changed (production-source fix `73dceca`):
  final gap reduced 3→1 segment in the measured sparse arm (events=1/interval);
  peak gap unchanged at 4 segments (stride-bound); ≥3-segment persistence
  29/40 = 72.5% (down from 95.0%, still the majority of the run). The catch-up
  loop is a hard-capped 8-iteration `for` loop, not a spin-loop of any kind —
  "prevents an unbounded spin" mischaracterized the mechanism and is retracted.
  R32-8's existing stride=64 throughput speedup is preserved (re-confirmed at
  ~67.6% vs. R32-8's original ~61%) but is UNCHANGED code, not a new result of
  this task; no new throughput speedup is measured or claimed — the gate's own
  §4 states the catch-up loop's body is never reached in the throughput
  regime. See `docs/perf/R34_11_CATCHUP_DECAY_GATE.md` §3-5.
- **R34-12** (`_raw_r34_12_paired_ab_full.log`, 91 KiB): shadow-head A/B
  clean re-gate — re-verifies R32-11's favorable-regime result (not a new
  speedup; a re-confirmation).
- **R34-23** (gate report `docs/perf/R34_23_*`): realloc gate — NO-GO for
  `large-reserved-capacity` on geometric realloc; corrected README
  `realloc_grow_geometric` (~40×→~2×). A measurement that narrowed an
  over-stated existing claim, not a speedup.

**Standing kill-gate (±10 raw-Ir churn).** No `perf(runtime)` commit this
round, so no kill-gate run was required. The 6 production-source commits
carry `fix(perf)` (correctness/consistency) and do not claim speedups.

### §3.1. Raw-log census (CLAUDE.md R34-24 mandated element)

CLAUDE.md's round-manifest rule states: "The manifest also records the
count and total size of raw-log files committed that round, making
aggregate `docs/perf/` growth visible per-round." Added 2026-08-05 (task
#557, independent review `docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`
finding G3) — this element was missing from both the original and the
F5-extended version of this manifest.

**Round-scoped count (the 5 `_raw_*.log` files this round's commits
actually added):**

```bash
git log --name-only --format="COMMIT:%H" 40241b0..c5db553 -- 'docs/perf/_raw_*.log' \
  | grep -E '^docs/perf/_raw_.*\.log$' | sort -u
```

→ 5 files: `_raw_r34_10_sparse_decay_gate.log` (145,348 B),
`_raw_r34_11_catchup_decay_gate.log` (49,999 B),
`_raw_r34_12_paired_ab_full.log` (91,281 B),
`_raw_r34_23_criterion_reverification.log` (1,076 B),
`_raw_r34_23_lrc_hypothesis_ab.log` (1,297 B) — **total 289,001 bytes
(≈282.2 KiB)**, individual sizes from `du -b <file>` for each of the 5
paths above, summed.

**Repo-wide count (all `_raw_*.log` files committed to date, any round —
the artifact-storage-policy rule's own reference command):**

```bash
find docs/perf -name '_raw_*.log' -type f | wc -l
```

→ **256** files, **5,428,319 bytes (≈5,301.1 KiB ≈ 5.18 MiB)** total
(`find docs/perf -name '_raw_*.log' -type f -exec du -b {} + | awk
'{sum+=$1} END {print sum}'`). All 256 are individually under the 200 KiB
tier-1 ceiling (the largest, `_raw_r34_10_sparse_decay_gate.log`, is
145,348 B ≈ 142 KiB) — consistent with the artifact-storage-policy rule's
own tier-1 census as of R34-24.

**Scope note.** The round-scoped figure (5 files / 289,001 B) is the one
this manifest's own R30-12 taxonomy applies to (i.e. what THIS round
added); the repo-wide figure (256 files / 5,428,319 B) is cited alongside
it because CLAUDE.md's artifact-storage-policy tier boundaries are defined
against individual file size, not a round-scoped aggregate, so the
repo-wide count is the more directly comparable number against that
rule's own "as of R34-24, ALL 256 committed `_raw_*.log` files fall under
this ceiling" statement. Both counts are independently re-derivable from
the two commands above; neither number is hand-transcribed.

**A different artifact class, explicitly excluded from both counts above
(the same naming-based blind spot this finding's review flagged as the
theme of finding G3):** `docs/perf/r34_23_runs/` holds two files —
`2026-08-04T22-03-44-381Z_direct_raw.json.gz` (8,674 B, tier-2
gzip-compressed per task #551/`358be4e`, reworded from `5710a6e` by the
later G1 rebase, task #555) and
`2026-08-04T22-03-52-053Z_vec_raw.json` (69,045 B, under the tier-1
ceiling uncompressed). Neither matches the `_raw_*.log` glob (they are
`.json`/`.json.gz`, not `.log`, and live under a differently-named
subdirectory) — this is precisely the naming-based gap the artifact-storage-
policy rule's tier system did not originally anticipate: a citable raw
artifact that is a raw JSON dump rather than a `_raw_*.log` text file.
task #551 already resolved this specific file's tier-2 sizing question
(258 KiB uncompressed → 8,674 B gzip-compressed, under the 200 KiB tier-1
ceiling for the compressed form), and `docs/perf/OPEN_ITEMS.md` carries the
still-open general gap (a THIRD tier-2/tier-3-sized raw artifact outside
the `_raw_*.log` convention would reopen it) as its own tracked item — this
census does not duplicate that item, only cross-references it so a reader
scanning this manifest's raw-log numbers does not mistake the
`_raw_*.log`-scoped 256/5 counts above for a total inclusive of
`r34_23_runs/`.

---

## §4. Reproduction

```bash
# Regenerate the §1 commit table (byte-identical SHAs):
git log --reverse --format="%H %s" 40241b0..c5db553

# Verify the round boundary:
git log --oneline -1 40241b0   # → "docs: commit Round 33 session checkpoint"
git log --oneline -1 c5db553   # → "docs: extend Round 34 CHANGELOG with all 26 tasks + commit session checkpoints…"

# Count commits in the round (last WORK commit, excludes the closing checkpoint):
git log --oneline 40241b0..8cb89ea | wc -l   # → 42

# Count commits in the round (full span, includes the closing checkpoint c5db553):
git log --oneline 40241b0..c5db553 | wc -l   # → 43
```

**Note on the original 38-commit undercount.** This manifest's first version
(commit `4ba188a`, R34-24/task #543) reproduced its own commit table as of
its own HEAD at write-time — but a per-round manifest is, by construction,
written *during* the round it describes, before the round's last commits
exist. `4ba188a` itself, its self-correction `9b06b56`, and the round's two
closing research tasks `7758f7a`/`8cb89ea` plus the closing checkpoint
`c5db553` all postdate the original table's own upper bound (`827a57a`) and
were therefore structurally unable to appear in it. This is not a
transcription error to avoid next time so much as a boundary-condition this
convention should flag explicitly going forward: a round manifest MAY need a
follow-up extension pass once the round's true closing commit is known (task
#550, independent readonly review finding F5,
`docs/reviews/2026-08-05-round34-readonly-review.md`).
