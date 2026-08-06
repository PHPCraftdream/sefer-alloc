# Round 34 remediation wave 6 manifest — commit classification & verdict

**Generated 2026-08-06 — task #650/P16's own closing pass.** See
`R34_REMEDIATION_1_MANIFEST.md` for the scope-redefinition rationale (this
file continues the same per-wave manifest series established there); "wave
6" here is the publish-readiness sweep for the six crates.io sub-crates,
landing directly on top of wave 5's (the release-readiness sprint's) closing
commit and following the identical convention.

This file covers **wave 6**: six independent `@oh` readonly review agents
(`docs/reviews/2026-08-06-{aligned-vmem,numa-shim,sefer-region,
racy-ptr-cell,size-classes,tagged-index-stack}-publish-readiness-review.md`,
plus their synthesis in
`docs/reviews/2026-08-06-publish-readiness-consolidated-summary.md`) audited
the six crates.io sub-crates the deferred K3/#598 publish-DAG task covers,
each returning **GO-WITH-FIXES**. Every finding was filed as a task
(#635-651, 17 tasks total) and closed via `@sh` delegation with full personal
zero-trust re-verification — continuing the "review → fix → review" chain
established across waves 3-5 (wave 5's own §2 is the direct precedent: each
wave's fix has repeatedly introduced at least one gap only an independent
read catches, which is why this wave's own closing task, #650, ran a final
audit pass rather than trusting the 14 individual fixes' own self-reported
verification — and that audit pass itself made one factual error, caught by
its own follow-up task's required pre-check; see §2 below).

## §1. Commit classification (verbatim from `git log`, FINAL)

Reproduce: `git log --reverse --format="%H %s" dc003c9~1..HEAD` (this table's
own upper bound is the wave's true closing SHA — this file's own last row
cannot literally cite its own commit SHA, the same self-referential-hash
problem prior waves' manifests documented: amending the file to embed its
own hash changes the hash it just embedded. Resolve the last row's SHA via
`git log -1 --format=%H -- <this file's path>`).

| # | SHA (full) | Commit prefix | Subject (truncated) | Task | Category |
|---|------------|---------------|---------------------|------|----------|
| 1 | `dc003c957b40baacaa147ff35e81884e27b0b1b4` | `fix` | numa: guard macOS platform stub with not(miri) | #635 | **fix — correctness + CI-coverage** (`crates/numa/src/lib.rs` cfg fix + new `numa-shim-macos-miri` CI job; not-yet-reachable-in-CI compile-time cfg path, no `production` behavior change) |
| 2 | `17f56935b97cf252a01a8e7ad17090fd8e7f2d1c` | `fix` | racy-ptr-cell: dbg_rollback_reenterable no longer clobbers a concurrent real winner | #636 | **fix — correctness** (`crates/racy-ptr-cell/src/lib.rs` + new loom test; `dbg_rollback_reenterable` is unconditionally `pub`, gated only by `#[doc(hidden)]` — `racy-ptr-cell` has no `bench-internals` Cargo feature at all — and allowlisted in `tests/dbg_hook_safety_tripwire.rs`'s `SAFE_MUTATORS`; no `production` runtime change) |
| 3 | `8529c0fc8a0cbe757ea075204a373dd53ea72ff8` | `fix` | size-classes: machine-check Params::extras preconditions | #637 | **fix — correctness** (const-eval asserts + `#[should_panic]` tests; sefer's own in-tree `EXTRAS` confirmed unaffected, no-op for `production`) |
| 4 | `d78625bdef57e8f5e35a815d91736774a541b900` | `fix` | tagged-index-stack: cap INDEX_BITS at 32 to close the TAIL-collision hole (F1) | #638 | **fix — correctness** (compile-time bound; sefer's own `INDEX_BITS=16` usage confirmed unaffected, no-op for `production`) |
| 5 | `6fc2f1b409acf619fe808abafa6fa7ba5065b521` | `build(ci)` | close 4 sub-crate CI coverage gaps (task #639, P5) | #639 | **build(ci) — workflow-config-only** (`.github/workflows/ci.yml` `test-workspace` job extended; no `src/` change) |
| 6 | `2a75d91f46defc60cc2881c7501b2e277ccdbdf3` | `build(ci)` | add release targets for the 3 unpublished sub-crates (K3/#598) | #648 | **build(ci) — workflow-config-only** (`.github/workflows/release.yml` tag patterns + dispatch options; infrastructure only, no crate published) |
| 7 | `ebe615dd53c982f6b0eb6b0dcdb15bd08bea81eb` | `docs` | vmem: fix three doc-accuracy overclaims (F1/F6/F10, task #640) | #640 | **docs-only** (`crates/vmem/Cargo.toml` + `src/lib.rs` doc comments; zero code-path change) |
| 8 | `b17ffabfdf68e37e9f7f255c46cad846745781c5` | `docs` | P7 — fix last surviving cross-region overclaim in sefer-region rustdoc | #641 | **docs-only + test** (`crates/region/src/lib.rs` doc fix + one new demonstration test in `tests/smoke.rs`) |
| 9 | `9ecada3d25bcbdf33e9b184c4233685e5b6a243f` | `docs` | racy-ptr-cell: three publish-readiness low-severity doc/discipline gaps (P8/#642) | #642 | **docs-only** (`crates/racy-ptr-cell/src/lib.rs` doc comments + `#![deny(missing_docs)]`; zero behavior change) |
| 10 | `300b41f97a0e7c85310e5ed53dcbf289414e779f` | `fix` | tagged-index-stack: document AtomicU64 portability limit + fix broken intra-doc link (F2/F3) | #643 | **fix — doc + real compile_error! guard** (a genuine, if small, behavior improvement: clearer failure message on an unsupported target, not just prose) |
| 11 | `7e1020f0f57fa8523e0830216c0aaa3c9cf88028` | `docs` | vmem,numa: add [package.metadata.docs.rs] to fix empty-default docs render (F3, task #644) | #644 | **docs-only** (`crates/vmem/Cargo.toml` + `crates/numa/Cargo.toml` metadata; zero `[features]` table change) |
| 12 | `19698da8f7ba331907b7ff99bcc4d879603417b2` | `docs` | vmem,numa: remove false no-std::no-alloc category claim (F5/F2, task #645) | #645 | **docs-only** (`categories` array correction on two `Cargo.toml` files; pure crates.io listing metadata) |
| 13 | `4c059fab167e37905290ecb646eff5e87ff7b844` | `fix` | vmem: resolve broken doc links + narrow mock dead-code allow (F4/F7/F8/F9, task #646) | #646 | **fix — doc links + lint-suppression narrowing** (`crates/vmem/src/lib.rs`, `src/fault_injection.rs`, root `Cargo.toml` feature-alias migration; the dead-code-allow narrowing is a real hygiene fix, bundled with doc-only F4/F7/F9) |
| 14 | `c8498cd2357403dbbbdb91f6dc39000ccdf25fc7` | `docs` | size-classes: correct two test-module docs overstating coverage | #647 | **docs-only** (`crates/size-classes/tests/{builder,proptest_builder}.rs` doc comments; zero test-body change) |
| 15 | `0a425196bac07b0bc373cdcedfc07dcd54e4955c` | `docs` | vmem,numa: close 2 leftover doc-accuracy gaps found during P16 audit | #650 | **docs-only** (found by the closing audit: a broken intra-doc link + a missed sibling copy of an already-corrected overclaim) |
| 16 | `7c8621f62356ead95baa5681a8913262157be4f8` | `docs` | P17 — pin #![deny(missing_docs)] on 3 crates already at 100% coverage | #651 | **docs-only** (one-line lint pin per crate, all three pre-confirmed at 100% coverage before the attribute was added; zero new doc content invented) |
| 17 | `03d624392b169ca300f1d56886626b6b6b515c63` | `docs` | CHANGELOG entry for the publish-readiness sweep (tasks #635-651) | — | **docs-only** (wave-closing CHANGELOG entry, inserted before `### BREAKING CHANGE` per the established heading-hierarchy lesson) |
| 18 | `9716792bd088fc2bf86ecd56cf44ed4fc8a84f5c` | `docs` | commit the checkpoint and all 7 publish-readiness review reports | — | **docs-only** (checkpoint + 7 review markdown files, committed per explicit standing user instruction) |
| 19 | *(this commit — see the self-referential-hash note above §1)* | `docs` | add wave-6 round manifest for the publish-readiness sweep | — | **docs-only** (this file's own finalization) |

### Aggregate counts (FINAL — 19 of 19 commits)

| Category | Count | Commits |
|----------|-------|---------|
| **fix — correctness** | 4 | dc003c9, 17f5693, 8529c0f, d78625b |
| **fix — doc + real behavior improvement** | 2 | 300b41f, 4c059fa |
| **build(ci) — workflow-config-only** | 2 | 6fc2f1b, 2a75d91 |
| **docs-only** | 11 | ebe615d, b17ffab, 9ecada3, 7e1020f, 19698da, c8498cd, 0a42519, 7c8621f, 03d6243, 9716792, *(this commit)* |

**Net default-feature impact:** one row (`4c059fa`, #13) edits the root
`Cargo.toml`'s `[features]` table — migrating two `production`-reachable
feature entries (`primordial-lazy-commit`, `small-segment-lazy-commit`) off
the deprecated `aligned-vmem/alloc-lazy-commit` alias onto its replacement
`aligned-vmem/lazy-commit` — verified inert because the removed alias name
has zero `cfg`-gated uses anywhere in `crates/vmem/src/` (it survives only
as a pure feature-name alias in `crates/vmem/Cargo.toml` for external
back-compat, `alloc-lazy-commit = ["lazy-commit"]`), confirmed by
`cargo build -p sefer-alloc --features production` staying green; no crate's
`[package] version` was touched anywhere in this wave. The four correctness
fixes (rows 1-4) all land on
crates not yet part of `sefer-alloc`'s own runtime dependency surface at the
versions currently in use — `numa-shim`'s macOS+miri cfg path was never
reachable in CI before this wave added the job that reaches it; the other
three are on crates.io-unpublished or about-to-be-republished crates whose
in-tree consumers (sefer's own `EXTRAS`, `INDEX_BITS=16` usage) were each
explicitly confirmed unaffected by the fix. `node
scripts/verify-commit-prefixes.mjs dc003c9~1..HEAD` reports PASS (8
direction-2 warnings on `docs`-prefixed commits — both scoped `docs(...):`
and bare `docs:` — touching non-`docs/` paths; the bare-`docs:` case (rows
8 and 19, `b17ffab`/`7c8621f`) only started being flagged after task #652
closed a scanner gap where bare `docs:` fell through to `'other'` and
skipped the direction-2 check entirely — every one of the 8 individually
re-verified via full diff read as a genuine false positive: pure
doc-comment/metadata/test-doc/lint-attribute changes with zero logic
touched, the same false-positive class this scanner has flagged
consistently since wave 5).

## §2. Zero-trust discovery: the closing audit itself contained one factual
error, caught by its own follow-up task rather than propagated

This wave closes 14 individual findings (rows 1-14) from six independent
crate-readiness reviews, then runs a dedicated closing audit (task #650,
row 15's origin) — the same "run a final gate that verifies the fixes
rather than trusting them" discipline wave 5's own manifest (§2) already
established as a repeated pattern across this session's whole review chain.

The audit found and fixed two genuine leftover gaps inline (row 15): a
broken intra-doc link in `numa-shim` and a missed sibling copy of an
already-corrected overclaim in `aligned-vmem` (task #640 had fixed the
"over-reserve + trim" overclaim in `Cargo.toml`'s `description` field, but
missed the identical claim duplicated in `reserve_aligned`'s own rustdoc and
the README's API table — the SAME class of "one commit fixes location A,
misses sibling location B" gap that recurred across CLAUDE.md's own
documented history of this project, e.g. the F3+F8 heading-hierarchy bug
recurring twice in wave 5).

More significantly, the audit's own report also contained a **factual
error**: it claimed `aligned-vmem` was one of four crates lacking
`#![deny(missing_docs)]`, when the crate had already carried that attribute
since its extraction (well before this wave started). This wrong claim was
carried into the filing of the follow-up task (#651/P17), which explicitly
instructed its executing agent to FIRST verify the premise before acting —
that pre-check (`grep -n "deny(missing_docs)"` across all four named
crates) caught the stale claim before any file was touched, correctly
narrowed the fix to the three crates that actually needed it, and the
orchestrator independently triple-confirmed the correction via a direct
`grep` before accepting it (documented in this session's own transcript,
not just the sub-agent's report).

This is not a new failure mode — it is the SAME "self-verification alone is
insufficient, only an independent second read catches what the first pass
missed" pattern this whole session's review chain has repeatedly
demonstrated (wave 3 §2, wave 4 §2, wave 5 §2, and now here) — but it is a
notable instance because the error originated in the AUDIT step itself, the
step specifically designed to catch other steps' errors, and was caught not
by a human but by a downstream task's own required pre-check plus the
orchestrator's habitual zero-trust re-verification. No finding in this wave
was accepted, and no task was marked complete, without the orchestrator
personally reading the full diff and independently re-running the claimed
verification commands — this discipline is what caught the audit's own
error before it could produce an incorrect fix.
