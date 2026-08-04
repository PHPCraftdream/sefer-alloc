# Round 34 manifest — commit classification & per-item verdict

**Generated.** The commit table in §1 is reproduced verbatim from
`git log --reverse --format="%H|%cI|%s" 40241b0..827a57a` (the exclusive
lower bound is Round 33's closing checkpoint `40241b0`; the inclusive upper
bound is Round 34's HEAD-at-manifest-time `827a57a`). No commit SHA was
hand-transcribed — every 40-hex SHA below is byte-identical to `git log`
output. This manifest is the FIRST real instance of the per-round manifest
artifact required by CLAUDE.md's "Round manifest" rule (added R34-24/task
#543); it doubles as the reference example future rounds should match.

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

**Round span.** 2026-08-04T11:43+02:00 (first commit) → 2026-08-05T00:37+02:00
(last commit). 38 commits, 1 calendar day.

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
catch-up loop is the closest to a behavioral change — it prevents an unbounded
spin — but adds no new allocation path and claims no speedup).

---

## §1. Commit classification (verbatim from `git log`)

Reproduce: `git log --reverse --format="%H %s" 40241b0..827a57a`

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
| 13 | `a9edc87300ca540f5fe7f5318268ca290b5ce81f` | `fix(perf)` | promote RemoteFreeRing cached_head from Relaxed to Acquire/Release | (untagged) | **fix(perf) — production-source** (`src/alloc_core/remote_free_ring.rs`; `alloc-xthread` in `production`) |
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

### Aggregate counts

| Category | Count | Commits |
|----------|-------|---------|
| **fix(perf) — production-source** (touches production-reachable `src/`) | 6 | a9edc87, 73dceca, 7ef5a46, 49929d0, c270b0c, 3281ebc |
| **opt-in-source** (non-default feature code changed) | 0 | — |
| **bench-only** (judge/probe/gate-report/harness; no shipping code) | 8 | a3831e5, b23d7c5, 0e29fc2, 94e133a, a100647, 9e70266, 19b1918, ba716a0 |
| **docs-only** (no runtime code; includes comment-only `src/` edits) | 10 | f9ae91f, 4aaca52, b45b824, 00a1c59, d4c4f8b, 1c686f8, e550006, ae9c9c3, b229187, 827a57a |
| **feat(api) — build-gating** (visibility reorg, no behavior change) | 3 | 27879af, b47cc6a, 0762772 |
| **test-only / test-fix** (new tests or test infrastructure) | 6 | fd54ddc, b47a261, 91ff1dd, 7aeee2d, 0047cf2, ae11073 |
| **fix(data) — CSV correction** | 2 | 5c1142f, 43115cf |
| **dependency-security** | 1 | 353cc05 |
| **ci/build tooling** | 2 | db7b30f, 36b4b3e |
| **Total** | **38** | |

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
| **(untagged)** | `a9edc87` | fix(perf) | **SHIPPED** — RemoteFreeRing `cached_head` promoted from Relaxed to Acquire/Release (memory-ordering correctness hardening in production-reachable `alloc-xthread` path) |
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
| **R34-24** (#543) | (this commit) | docs(process) | **RULES** — CLAUDE.md amendments: artifact storage policy (hybrid), round-manifest rule, OPEN_ITEMS structural rule, landing-SHA wording |

---

## §3. Measured deltas (wall-clock / Ir / RSS)

Round 34 produced no `perf(runtime)` or `perf(opt-in)` commits, so there are
no measured speedup deltas to report. The measurement work that did land is
bench-only or correctness-verification:

- **R34-10** (`_raw_r34_10_sparse_decay_gate.log`, 145 KiB): sparse-decay
  accumulation — confirmed the stride-retention bound does NOT hold (finding,
  not a speedup). No production code changed in response to this finding
  alone; R34-11's bounded catch-up loop is the structural response.
- **R34-11** (`_raw_r34_11_catch_up_decay_gate.log`): catch-up decay gate —
  confirmed sparse gap reduced + R32-8 benefit preserved (GO for the
  production-source fix `73dceca`). No speedup claimed; the fix prevents an
  unbounded spin and preserves R32-8's existing decay-throttle benefit.
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

---

## §4. Reproduction

```bash
# Regenerate the §1 commit table (byte-identical SHAs):
git log --reverse --format="%H %s" 40241b0..827a57a

# Verify the round boundary:
git log --oneline -1 40241b0   # → "docs: commit Round 33 session checkpoint"
git log --oneline -1 827a57a   # → "docs: complete the R34-23 realloc README correction…"

# Count commits in the round:
git log --oneline 40241b0..827a57a | wc -l   # → 38
```
