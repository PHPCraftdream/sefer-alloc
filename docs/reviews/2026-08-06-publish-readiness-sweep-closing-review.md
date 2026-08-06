# Closing readonly review — publish-readiness sweep (wave 6, `dc003c9~1..HEAD`, 19 commits)

**Reviewer:** independent readonly agent (claude-opus-5, effort=high), 2026-08-06.
**Scope:** the 19 commits `dc003c9..831a617`, their claims, `CHANGELOG.md`'s new
`#### Publish-readiness sweep` subsection, `docs/checkpoints/2026-08-06-1145.md`,
and `docs/perf/round-manifests/R34_REMEDIATION_6_MANIFEST.md`.
**Constraints honoured:** no file edited outside this report, nothing staged or
committed, no `cargo publish` in any form (not even `--dry-run`).
**Out of scope by instruction (and confirmed NOT flagged below):** the crates.io
publish DAG execution, version bumps (incl. `numa-shim` 0.1.0→0.2.0), root pin
sync — tasks K3/#598 and #649, deliberately gated on a user go-ahead.

---

## Verdict summary

**The wave's self-reported state is substantially accurate.** All 19 commit
diffs were read in full; every commit's diff does what its subject and body
claim, the four "reproduced correctness bug" fixes are genuinely fixed (two
independently re-verified below, one by an executed counterfactual and one by an
executed compile-fail probe in a scratch consumer crate), the CHANGELOG's new
subsection is correctly parented (the `### BREAKING CHANGE` heading-hierarchy
bug did **not** recur), no `[package] version` was touched anywhere, no
`TODO`/`FIXME`/placeholder was left in source, no new `pub fn` touching
allocator metadata was introduced, and all six sub-crates pass `cargo test
--all-features` and `cargo clippy --all-features --all-targets -- -D warnings`
cleanly when run fresh right now.

**However, the wave's own bookkeeping is not clean.** Twelve findings follow.
The two most consequential are process/accuracy defects, not code defects:

- the round manifest's headline "Net default-feature impact" paragraph states a
  fact that is **false** — one commit *does* edit the root `Cargo.toml`'s
  `[features]` section, in a way `production` transitively reaches (the effect is
  inert, and the manifest's own row 13 admits the edit, so the manifest
  contradicts itself); and
- `scripts/verify-commit-prefixes.mjs` has a **structural blind spot** that made
  its "PASS with exactly 6 warnings" an incomplete account of this very wave —
  two more commits of exactly the shape direction-2 exists to surface escaped it
  silently, because the classifier has no bare-`docs:` arm.

Neither invalidates the wave's technical work. Both are exactly the "a gate that
looks like it validates but structurally can't" and "a summary artifact that
overclaims relative to its own commits" classes this project's CLAUDE.md
documents as recurring.

---

## Findings

### P2-1 — Manifest's "Net default-feature impact" paragraph is factually false, and contradicts the manifest's own row 13

`docs/perf/round-manifests/R34_REMEDIATION_6_MANIFEST.md:66-68` states:

> **Net default-feature impact:** `production`'s feature composition is
> **UNCHANGED** across all 19 commits — **no row touches the root
> `Cargo.toml`'s `[features]` section**, and no crate's `[package] version` was
> touched anywhere in this wave.

The second clause is true (independently verified: `git log -p dc003c9~1..HEAD
-- '*Cargo.toml' | grep -E '^[+-]version'` is empty). **The emphasised clause is
false.** Commit `4c059fa` edits the root `Cargo.toml` at `Cargo.toml:770` and
`Cargo.toml:790`, both inside the `[features]` table:

```
 primordial-lazy-commit = [ "alloc-core", -"aligned-vmem/alloc-lazy-commit", +"aligned-vmem/lazy-commit", ... ]
 small-segment-lazy-commit = [ "alloc-core", -"aligned-vmem/alloc-lazy-commit", +"aligned-vmem/lazy-commit", ... ]
```

`primordial-lazy-commit` is part of `production` (per its own comment at
`Cargo.toml:763`), so `production`'s *resolved* feature set for the
`aligned-vmem` package genuinely changed by one feature name.

**Why the conclusion nevertheless survives (verified, not assumed):**
`crates/vmem/Cargo.toml:42` defines `alloc-lazy-commit = ["lazy-commit"]` as a
pure alias, and `grep -rn "alloc-lazy-commit\|alloc_lazy_commit" crates/vmem/`
finds it **only** in `Cargo.toml` comments and `README.md` — there is no
`cfg(feature = "alloc-lazy-commit")` anywhere in `crates/vmem/src/`, so no
compiled code changes. `cargo build -p sefer-alloc --features production` is
green (exit 0, run fresh for this review).

The manifest's own **row 13** (`:49`) *does* disclose the change ("root
`Cargo.toml` feature-alias migration"), as does `CHANGELOG.md:127` ("the root
`Cargo.toml`'s own two remaining consumers of a deprecated feature alias
migrated to the current name"). So this is an internal contradiction inside one
file, in precisely the summary line CLAUDE.md's R34-24 manifest rule says a
reviewer is supposed to be able to trust without opening every commit. The
correct wording is "one row edits the root `[features]` table, verified inert
because the removed name is a no-cfg alias."

**Commit:** `4c059fa`. **Files:** `docs/perf/round-manifests/R34_REMEDIATION_6_MANIFEST.md:67`,
`Cargo.toml:770`, `Cargo.toml:790`.

---

### P2-2 — `verify-commit-prefixes.mjs` direction-2 is structurally blind to bare `docs:` subjects; the manifest's "6 warnings, all re-verified" therefore understates the candidate set

`scripts/verify-commit-prefixes.mjs:201` defines
`BENCH_OR_DOCS_RE = /^(bench|docs)\(([^)]*)\)!?:/` — **scoped only**. Line 227
adds `BENCH_BARE_RE = /^bench:/` so a bare `bench:` still classifies as `bench`
and gets its path check. **There is no `DOCS_BARE_RE`.** A subject beginning
`docs:` (no scope) therefore falls through `classifySubject` to `'other'`
(`:228`) and is never path-checked at all (`main`'s direction-2 branch at `:295`
only fires for `bench` / `docs-config` / `docs-other`).

This is not hypothetical — it fired twice in this wave:

| commit | subject | paths outside `docs|examples|benches|tests|scripts/` | warned? |
|---|---|---|---|---|
| `b17ffab` | `docs: P7 — fix last surviving cross-region overclaim…` | `crates/region/src/lib.rs` (+ a **new test** in `crates/region/tests/smoke.rs`) | **no** |
| `7c8621f` | `docs: P17 — pin #![deny(missing_docs)] on 3 crates…` | `crates/region/src/lib.rs`, `crates/size-classes/src/lib.rs`, `crates/tagged-index-stack/src/lib.rs` | **no** |

Both are in fact non-behavioral (I read both diffs in full — a doc comment plus a
demonstration test in one, a single lint attribute per crate in the other), so
there is **no harm this wave**. The defect is the gate: manifest `:76-81` reports
"PASS (6 direction-2 warnings … every one individually re-verified)" as if 6 were
the full candidate population. It is 6 of 8; the scanner cannot see the other 2
by construction, and a future bare-`docs:` commit hiding a real change would pass
silently. Note also that `7c8621f` is the *stronger* of the two candidates: a
crate-level `#![deny(missing_docs)]` is a compile-gating attribute, not a doc
comment, which is exactly the judgement call direction-2 exists to route to a
human.

**Files:** `scripts/verify-commit-prefixes.mjs:201,227,228`,
`docs/perf/round-manifests/R34_REMEDIATION_6_MANIFEST.md:76-81`.

---

### P3-3 — `2a75d91` invalidated a live open-items item and did not update it in the same commit

`docs/CORRECTNESS_OPEN_ITEMS.md:1404` is item 24, `Status: OPEN`. Its
current-state card at `:1414-1418` cites, as its evidence:

> Four more … have no tag pattern or `workflow_dispatch` dropdown option in
> `.github/workflows/release.yml` either (**that file lists exactly 5 crates**:
> `aligned-vmem`, `sefer-region`, `malloc-bench-rs`, `numa-shim`, `sefer-alloc`).

Commit `2a75d91` (this wave) made that false: `.github/workflows/release.yml:66-73`
and `:80-88` now list **8** crates, having added `racy-ptr-cell`,
`size-classes`, and `tagged-index-stack` to both the tag patterns and the
dispatch dropdown. Item 24 was not touched — no commit in `dc003c9~1..HEAD`
touches `docs/CORRECTNESS_OPEN_ITEMS.md` or `docs/perf/OPEN_ITEMS.md` at all
(verified: `git log --oneline dc003c9~1..HEAD -- docs/CORRECTNESS_OPEN_ITEMS.md
docs/perf/OPEN_ITEMS.md` is empty).

CLAUDE.md's "OPEN_ITEMS indexes are CURRENT-STATE, not archives" rule requires
the card to be refreshed by the commit that changes the fact it cites, in the
same commit. Item 24's *headline* (README's "all 11 are real crates.io crates"
claim) is still genuinely open — only its cited sub-evidence went stale — so this
is a card-refresh, not a closure.

---

### P3-4 — Four open/deferred items this wave itself flagged went into neither open-items index

CLAUDE.md: "When a gate report / commit / review newly flags an open item, add it
to the appropriate index in the same commit … the in-session TaskList does not
survive a session boundary." All four below live **only** in a commit body or a
source comment:

1. **No compile-fail harness for `INDEX_BITS > 32`.**
   `crates/tagged-index-stack/tests/stack_unit.rs:137-144` records, verbatim, "a
   known, honestly-recorded coverage gap, not a silent omission" — but records it
   nowhere an index reader will see it. (Commit `d78625b`.) This is a CI-coverage
   gap, squarely inside `docs/CORRECTNESS_OPEN_ITEMS.md`'s stated scope.
2. **The macOS+miri fix is unverified until CI runs it.** `dc003c9`'s own body
   states the local verification "does NOT exercise the macOS `not(miri)` arm or
   the macOS+miri crossing itself — that verification depends on the new
   `numa-shim-macos-miri` CI job actually running on `macos-latest`." That is a
   pending verification trigger with no index entry. (I verified the fix is
   structurally correct by cfg analysis below — see "spot-checks that passed" —
   but the empirical confirmation is still owed.)
3. **`compile_error!` cascade left deliberately unfixed.** `300b41f`'s body: "the
   plain E0432 still cascades after it … judged too intrusive for the benefit."
   A conscious, defensible call — but an unrecorded one.
4. **Two one-way-door publish decisions surfaced and dropped.** `9ecada3`'s body
   raises the `racy-ptr-cell` crate name reading as "has data races" (the opposite
   of its guarantee) and its 383-character `description`, both "one-way doors once
   first published," and explicitly defers them to the maintainer. Neither is in
   an index, nor in the deferred-publish task's own scope note.

---

### P3-5 — `test-workspace`'s own header comment now contradicts the four steps `6fc2f1b` added below it

`.github/workflows/ci.yml:684-690` still reads:

> the workspace member crates' own test suites (aligned-vmem … plus sefer-region
> and malloc-bench-rs) never ran in CI. Run each member's tests explicitly.
> **Default features only** …

Immediately below (`:697-745`), `6fc2f1b` added `cargo test -p
tagged-index-stack`, `cargo test -p aligned-vmem **--all-features**`, and two
`thumbv7em-none-eabi` builds for `size-classes`/`racy-ptr-cell`. The job now
covers six crates, one of them with all features on. This is the identical
stale-header-vs-actual-steps drift this project already fixed once as S9/#632
(`check-all.mjs`'s header not matching its own steps array).

---

### P3-6 — `racy-ptr-cell` has no `bench-internals` feature, yet two commits/artifacts describe its `dbg_*` hooks as "bench-internals-only"

`crates/racy-ptr-cell/Cargo.toml` has **no `[features]` table at all** (verified
by reading the file). `dbg_is_ready` (`crates/racy-ptr-cell/src/lib.rs:388`) and
`dbg_rollback_reenterable` (`:431`) are therefore unconditionally `pub`, gated by
nothing but `#[doc(hidden)]`. Yet:

- `17f5693`'s body: "This hook is `#[doc(hidden)]`, **bench-internals-only test
  infrastructure**";
- manifest row 2 (`R34_REMEDIATION_6_MANIFEST.md:38`): "**bench-internals-only
  hook**, `#[doc(hidden)]`".

**This is a description error, not a policy breach** — I checked: the hook is
explicitly allowlisted under `SAFE_MUTATORS` in
`tests/dbg_hook_safety_tripwire.rs:286` with the (newly corrected, accurate)
invariant justification, which is exactly rule (b) of that tripwire's sanctioned
paths for an ungated safe mutator. I ran the tripwire fresh: 7/7 green.

The residual substantive point, which none of the six reviews nor the closing
audit surfaced: on a crate about to receive its **first** crates.io publish, both
`dbg_*` fns become part of `0.1.0`'s compile-visible public API. `#[doc(hidden)]`
hides them from rustdoc but is not a semver exemption a compiler enforces — a
downstream user can call `dbg_rollback_reenterable` on a live cell. Post-`17f5693`
that is memory-safe and cannot break exactly-once init (verified below), so the
impact is low; but "which symbols am I committing to at 0.1.0" is precisely a
publish-readiness question, and it was not asked.

---

### P4-7 — `_CHECK_BITS` is enforced only on paths that instantiate `pack`, so the doc's flat "compile-time guard" overstates slightly

`crates/tagged-index-stack/src/lib.rs:179-195` documents `_CHECK_BITS` as
"Compile-time guard: `INDEX_BITS` must be in `1..=32`". It is forced by exactly
one reference, `let () = Self::_CHECK_BITS;` at `:212`, inside `pack`.

Verified empirically in a throwaway consumer crate (built against the local path
dep, then deleted):

- `TaggedIndexStack::<33>::new()` + `push`/`pop` → **hard compile error**,
  `error[E0080]: evaluation panicked: INDEX_BITS must be in 1..=32 …`, with rustc
  naming the chain `TaggedIndexStack::new` → `TaggedIndex::<33>::empty` (`:232`)
  → `pack` (`:212`). The F1 hazard is genuinely closed structurally.
- `TaggedIndex::<40>::unpack(word)` + `TaggedIndex::<40>::INDEX_MASK`, with `pack`
  never instantiated → **compiles clean**.

Since the corruption scenario requires a `TaggedIndexStack`, and every stack
construction goes through `pack`, the security-relevant hole is fully closed.
The doc sentence is a hair stronger than the mechanism.

---

### P4-8 — Checkpoint's commit count and commit range disagree with each other

`docs/checkpoints/2026-08-06-1145.md:13`: "Total: **19 commits** landed this wave
(`dc003c9` **through `7c8621f`**)". `dc003c9..7c8621f` inclusive is **16**
commits (I counted them from `git log --reverse --oneline`). The checkpoint's own
line 19 says the CHANGELOG entry, the `.md` commits, and the closing review were
still ahead — i.e. 16 was the count at write time and 19 is the final count. The
sentence fuses the final count onto the then-current range.

---

### P4-9 — CHANGELOG miscounts the `aligned-vmem` over-reserve overclaim copies

`CHANGELOG.md:127`: "**a fourth copy** of the same over-reserve overclaim (in
`reserve_aligned`'s own rustdoc **and** the README table) was missed by that fix".
Per the diffs: `ebe615d` fixed one location (`crates/vmem/Cargo.toml`
`description`); `0a42519` fixed **two** (`crates/vmem/src/lib.rs:448` and
`crates/vmem/README.md:40`). So they are the 2nd and 3rd copies, not "a fourth
copy", and the singular does not match the two locations the parenthetical names.
(`0a42519`'s own commit body gets this right: "missed **two** other copies".)

---

### P4-10 — "12 of its 16 tests" cannot also include the F1 regression test

`CHANGELOG.md:126` and `6fc2f1b`'s body both say `tagged-index-stack`'s CI
"excluded 12 of its 16 tests (both non-loom test files, **including the new F1
regression above**)". Counted at `6fc2f1b`'s own tree: `stack_unit.rs` 9 +
`regression_counter_wrap.rs` 4 + `loom_aba.rs` 4 = **17** tests, of which **13**
are non-loom. "12 of 16" is the pre-`d78625b` count, from before the F1 test
existed — so it cannot be the count that includes it. Off by one on both
numerator and denominator.

---

### P4-11 — Test comment in `extras_overlapping_geometric_run_panics` names the wrong table maximum

`crates/size-classes/tests/builder.rs:218-222` asserts `l ==
size2class_len(200, MIN_BLOCK)` with the message `"sanity: expected max 200"`.
The actual `table[N-1]` for that scheme is **192** (geometric run
`16,32,48,64,80,112,144,192` merged with `extras=[16,32]`, as the test's own
comment at `:180` correctly states). The assert passes only because
`size2class_len` floor-divides: `192/16+1 == 200/16+1 == 13`. The test is valid
and non-vacuous (verified: it runs and passes, and its `#[should_panic(expected =
"strictly increasing")]` fires from `build_size2class`); the sanity message is
just wrong about which number it is checking.

---

### P4-12 — `#![deny(missing_docs)]` on four about-to-be-published library crates is a downstream-build-breakage footgun worth a conscious pre-publish call

`7c8621f` added `#![deny(missing_docs)]` to `crates/region/src/lib.rs:52`,
`crates/size-classes/src/lib.rs:44`, `crates/tagged-index-stack/src/lib.rs:126`;
`9ecada3` added it to `crates/racy-ptr-cell/src/lib.rs:88`. All four were
verified at 100% coverage first, and all four compile clean today — this is not a
defect now. But `deny` (as opposed to `warn` in the lib plus `-D warnings` in
CI) in a **published** crate means a future rustc release that widens
`missing_docs` turns downstream `cargo build` of that pinned version red, with no
recourse for the consumer. The ecosystem convention is lib-`warn` + CI-`deny`.
Advisory only; raising it because these four crates are days from a first/next
publish and this is a one-way door for already-published versions.

---

## Spot-checks that passed cleanly (no finding)

These were run or read for this review and produced nothing to report:

1. **All 19 commit diffs read in full** (`git show <sha>` for each). Every diff
   matches its subject and body. No out-of-scope edit found in any of them: each
   commit touches only the files its message names, and the two commits whose
   scope is easiest to overrun (`4c059fa`, 3 files across two crates; `6fc2f1b`,
   one workflow) stay inside their stated scope.

2. **Commit-prefix honesty, per commit.** No `docs`-prefixed commit hides a
   behavior change. Independently confirmed all 6 direction-2 warnings by reading
   the diff content, not the manifest's claim: `ebe615d` (Cargo.toml
   `description` + feature comment + two rustdoc blocks), `9ecada3` (three
   `///`/`//` additions + one lint attribute), `7e1020f` (a new
   `[package.metadata.docs.rs]` block on two manifests — zero `[features]`
   change), `19698da` (one `categories` array element removed per manifest),
   `c8498cd` (two `//!` module-doc blocks, zero test bodies touched), `0a42519`
   (three single-line doc/README edits). Also checked the two `build(ci)` commits
   claim nothing beyond workflow config — correct, `6fc2f1b` and `2a75d91` touch
   only `.github/workflows/*.yml`. `300b41f` and `4c059fa` correctly use `fix`
   rather than `docs` because each does contain a real (small) non-doc change
   (a `compile_error!` guard; a lint-suppression narrowing plus the root
   feature-alias migration).

3. **Correctness bug #2 (`racy-ptr-cell`) — fix verified AND counterfactually
   proven, by me, not trusted.** Read the full `dbg_rollback_reenterable` body
   (`crates/racy-ptr-cell/src/lib.rs:429-470`) plus `get_or_try_init`
   (`:287-374`). The fix is complete by construction: the only unconditional
   store the probe still performs is step 2 (sentinel→null), which executes while
   the probe provably owns the cell (it won step 1's CAS and no other thread can
   take a cell holding the sentinel), and step 4's store is now gated on step 3's
   CAS having re-won. Every path where a real caller owns the cell — mid-`init`
   (sentinel) or published (real pointer) — makes step 3's `CAS(null→sentinel)`
   fail, so the probe returns `None` and touches nothing.
   Ran the suite as instructed: `RUSTFLAGS="--cfg loom" cargo test -p
   racy-ptr-cell --release --test loom_racy_ptr_cell` → **7/7 pass**, including
   `real_probe_rollback_does_not_clobber_concurrent_winner`.
   **Counterfactual executed independently:** copied the crate to a scratch
   directory outside the repo, reverted step 4 to the pre-fix unconditional
   `store(null)`, re-ran the same loom command → that one test **FAILS**:
   `assertion left == right failed: exactly ONE real caller must run init despite
   the concurrent probe (got 2) / left: 2 / right: 1`, all other 6 still pass.
   The test is genuinely non-vacuous and the commit's claim is truthful. Scratch
   copy deleted; the repo tree was never modified.

4. **Correctness bug #4 (`tagged-index-stack`) — fix verified by an executed
   compile-fail probe.** See P4-7 above for the exact rustc output. `INDEX_BITS =
   33` is a hard `E0080` through `TaggedIndexStack::new` → `empty` → `pack` →
   `_CHECK_BITS`. Also read `push`'s guard: at the new maximum width 32,
   `INDEX_MASK == u32::MAX == TAIL`, so the pre-existing `index < INDEX_MASK`
   debug-assert already excludes `TAIL` — no separate runtime check is needed, as
   the commit claims. `width_32_index_mask_equals_tail_and_is_rejected` runs and
   passes.

5. **Correctness bug #1 (`numa-shim`) — verified structurally.** Enumerated every
   `mod platform` cfg after the fix: `all(linux, not(miri))` `:259`,
   `all(windows, not(miri))` `:608`, `all(macos, not(miri))` `:773`, `miri`
   `:798`, `not(any(linux, windows, macos, miri))` `:822`. These five are
   provably pairwise-disjoint **and** exhaustive over the (OS × miri) space — the
   E0428 duplicate cannot recur, and no configuration is left with zero `platform`
   modules. (Empirical macOS+miri confirmation still owed — see P3-4 item 2.)

6. **Correctness bug #3 (`size-classes`) — verified by reading and running.**
   Both new asserts are const-eval `assert!`s inside `const fn`s on the only two
   construction chokepoints. Confirmed the two checks are genuinely
   non-redundant, as the commit argues: `extras=[100,200]` with `min_block=16`
   *does* stay strictly increasing after the merge, so `build_size2class`'s
   monotonicity check alone would not catch it — the `& mask == 0` check in
   `build_table` is independently required. Both new `#[should_panic]` tests
   appear in `--list` and pass (`2 passed`). No behavior change for sefer's own
   in-tree `EXTRAS`: `cargo build -p sefer-alloc --features production` green.

7. **CHANGELOG heading hierarchy — the recurring bug did NOT recur.** `grep -n
   "^## \|^### \|^#### " CHANGELOG.md` gives, in order: `:8 ## [0.3.0]`, `:10 ###
   Round 34`, `:14 …:108` a run of `####` siblings, `:121 #### Publish-readiness
   sweep`, `:130 ### BREAKING CHANGE`. The new `####` subsection is at 121 and
   `### BREAKING CHANGE` is at 130 — after **all** new wave-6 bullets (`:123-128`),
   not before. Correctly parented under `### Round 34`; nothing is re-parented.

8. **CHANGELOG prose vs. cited commits — spot-checked five bullets** (more than
   the three requested), all of which hold up apart from the two miscounts filed
   as P4-9/P4-10: the `racy-ptr-cell` clobber description matches `17f5693`'s
   actual diff and my own counterfactual; the `tagged-index-stack` "capped at
   `1..=32` … since `push` takes a `u32` anyway" matches `d78625b`; the
   `sefer-region` `PhantomData<fn() -> T>` explanation matches `b17ffab` and the
   crate source; the `no-std::no-alloc` removal claim matches `19698da` exactly
   (both `categories` arrays, and `tagged-index-stack`'s genuinely-accurate claim
   really was left alone — verified in its `Cargo.toml`); the root-`Cargo.toml`
   feature-alias sentence matches `4c059fa`. The `#### Runtime improvements: 0`
   framing is accurate.

9. **Checkpoint & manifest claims cross-checked against `git log`.** The
   manifest's 19-row commit table matches `git log --reverse --format="%H %s"
   dc003c9~1..HEAD` row for row, SHA for SHA, including the self-referential
   last-row note. Its aggregate counts (4 + 2 + 2 + 11 = 19) add up and the named
   SHAs in each bucket are correct. Task-number attributions (#635-#651) match
   each commit's own body. Only the "Net default-feature impact" paragraph (P2-1)
   and the checkpoint's commit count (P4-8) are wrong.

10. **`cargo build -p sefer-alloc --features production`** — exit 0, clean.

11. **All six sub-crates, fresh right now:** `cargo test -p <crate>
    --all-features` and `cargo clippy -p <crate> --all-features --all-targets --
    -D warnings` for `aligned-vmem`, `numa-shim`, `sefer-region`,
    `racy-ptr-cell`, `size-classes`, `tagged-index-stack` — **all tests pass, all
    clippy runs emit zero output** (24 / 13 / 6 / 3 / 11 / 13 tests
    respectively). No warning, no failure, in any of the twelve invocations.

12. **`tests/dbg_hook_safety_tripwire.rs`** (modified by `17f5693`) — 7/7 pass
    under the feature set its own header prescribes, including
    `safe_dbg_hooks_match_reviewed_allowlist`.

13. **Hygiene sweeps, all clean:** no `TODO`/`FIXME`/`XXX`/`todo!`/`unimplemented!`
    added to any source file in the range (the only matches are inside the
    committed review markdown, where they are quoted grep *results*); no new
    `pub fn` or `pub unsafe fn` added anywhere under `src/` or `crates/` in the
    whole range (`git diff dc003c9~1..HEAD --unified=0 | grep '^\+.*pub .*fn '`
    is empty) — so CLAUDE.md's benchmark-hook rule has no new surface to police;
    no `[package] version` line added or removed in any manifest; no crate
    published, and `release.yml`'s new entries are tag patterns and dropdown
    options only, with no publish step invoked.

---

## Recommended dispositions

| # | Severity | Action |
|---|---|---|
| P2-1 | P2 | Correct the manifest's "Net default-feature impact" paragraph to state the root `[features]` edit and why it is inert. Doc-only fix. |
| P2-2 | P2 | Add a `DOCS_BARE_RE = /^docs:/` arm to `classifySubject` (symmetric with the existing `BENCH_BARE_RE`), then re-run over this range and re-verify the 2 newly-surfaced commits. Small, mechanical, closes a real gate hole. |
| P3-3 | P3 | Refresh item 24's current-state card in `docs/CORRECTNESS_OPEN_ITEMS.md` (release.yml now lists 8 crates); the item's headline stays OPEN. |
| P3-4 | P3 | File the four flagged-but-unindexed items into `docs/CORRECTNESS_OPEN_ITEMS.md`. |
| P3-5 | P3 | Update the `test-workspace` job header comment to match its six actual steps. |
| P3-6 | P3 | Decide, before `racy-ptr-cell 0.1.0` ships, whether both `dbg_*` fns should be in the published API; at minimum correct the "bench-internals-only" wording in the manifest. |
| P4-7…P4-12 | P4 | Copy-edit / advisory; batch into a single doc-accuracy commit or fold into the pre-publish pass. |

No P0 or P1 finding. Nothing in this review blocks the deferred publish DAG on
technical grounds; P3-6 and P4-12 are the two items worth resolving *before* the
first publishes rather than after, because both are one-way doors.
