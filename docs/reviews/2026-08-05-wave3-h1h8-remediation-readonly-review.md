# Independent READONLY review — wave 3 (H1–H8) release-readiness remediation

- **Scope:** `2a7f1e6..HEAD` (14 commits, `85dacfc` at review time), i.e. tasks
  #571–#578 plus the three fallout-fix commits (`e886ea4`, `0d23e7f`,
  `2f16ba6`) and the two closing doc commits (`b57f988`, `85dacfc`).
- **Source review being closed:** `docs/reviews/2026-08-05-sol-remediation-readonly-review.md`
  (findings H1–H8).
- **Posture:** zero-trust. Every commit diff read in full; every claimed
  verification re-run locally; two external probe crates built outside the repo
  to test the `internals` boundary from a genuine downstream position; a full
  reachability sweep of every commit SHA cited in every tracked `*.md`.
- **Nothing in the repository was modified by this review** (this file is an
  untracked local artifact, per project convention).

**Verdict: 10 findings — 1×P1, 3×P2, 3×P3, 3×P4.** The P1 is a genuine,
reproducible **CI-red at `HEAD`**: two live `ci.yml` commands do not compile.
It is pre-existing (introduced in Round 34 by R34-18's size pin) but it sits
squarely inside H1's remit and is *directly contradicted by H1's own written
analysis*, which wave 3 then closed as fully resolved. Everything else is
documentation/structural drift, three items of which are recurrences of defect
classes the same wave or the immediately prior wave had just fixed.

---

## What I verified GREEN (so the findings below are read in proportion)

| Check | Result |
|---|---|
| `cargo check --features "production internals"` | clean (exit 0) |
| `cargo check --features "production"` (no `internals`) | clean |
| `cargo test --features "production internals" --no-fail-fast` | clean (exit 0, 0 failures) |
| `cargo clippy --all-targets --features "production internals" -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo check --examples --benches --all-features` | clean (exit 0) — H2's `required-features` sweep in `e886ea4` is complete for this axis |
| `cargo check --example r31_3_large_cache_extended_narrow_off --features "alloc-global internals"` | clean (its own `required-features`, honoured) |
| `cargo check --example r31_3_large_cache_extended_narrow_on --features "alloc-global alloc-decommit large-cache-extended internals"` | clean |
| `node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` | 36 files, 128 methods, 124 gated, 4 allowlisted, **0 violations** |
| `node scripts/verify-commit-prefixes.mjs 2a7f1e6..HEAD` | PASS (14 commits, all wave-3 prefixes valid under R30-12) |
| **Full `npm run check`** | Reproduces **exactly** the single failure the session claims — `verify-commit-prefixes` red on the pre-existing `43115cf`/`5c1142f`. All 6 clippy rows, all 4 test combos (including `--all-features`), both `internals` boundary oracles, `verify-perf-gate-stubs`, `verify-gate-report` are green. **The session's claim is accurate.** |
| H3 (CHANGELOG structure) | `grep -n "^## \[" CHANGELOG.md` → `8: [0.3.0] (unreleased)`, `5975: [0.2.1]`, `6029: [0.2.0]`, `6119: [0.1.0]`. No orphaned `##`-level content. **Genuinely fixed** (but see F3 for a `###`-level break introduced in the same wave). |
| H8 (documentation-only decision record) | `git show --stat 800ee86` → 1 file, `docs/CORRECTNESS_OPEN_ITEMS.md`, +39/-0. **No code changed, no rebase performed.** Confirmed. |
| H2's `dbg_decommit_count` allowlist exemption | Verified real: `src/global/sefer_alloc.rs:475` `decommit_calls: crate::alloc_core::AllocCore::dbg_decommit_count()`. Justified. |
| H2 gating, from a genuine downstream position | External probe crate, `sefer-alloc = { features = ["production"] }` (no `internals`): `ac.dbg_large_cache_used()` → **E0599, method not found**. The gating genuinely works at the crate boundary, not just structurally. |
| Flaky test predates this session | `git log -- tests/segment_table_contains_base_tier1_counters.rs` → last touched by `7aeee2d` (Round 34 rustfmt drift). Claim confirmed. `TEST_LOCK` fix matches the established pattern; no assertion logic changed. |
| SHA-pair claims in H4 | All four rebase pairs verified by tree-object identity: `73817ee`/`5e75032`, `a4dc38e`/`ff496c6`, `d46c349`/`a7d7395`, `5710a6e`/`358be4e` — identical trees, replacements reachable from `HEAD`. Correct as far as they go (see F2). |
| H6 manifest tables | `git log --reverse c5db553..4f45eee` → exactly the 7 rows of `R34_REMEDIATION_1_MANIFEST.md`, in order. `git log --reverse 4f45eee..2a7f1e6` → exactly the 17 rows of `R34_REMEDIATION_2_MANIFEST.md`, in order. `40241b0..c5db553` = 43 (Round 34 proper). **Waves 1 and 2 are exact** (see F7 for wave 3's own). |
| TODO/placeholder/half-wired scan | `git diff 2a7f1e6..HEAD \| grep -E "^\+.*(TODO\|FIXME\|XXX\|unimplemented!\|todo!\|placeholder)"` → **no hits**. |
| Out-of-scope edits | None found. Every `src/` hunk in the range is either a `#[cfg]` addition, a doc comment, or the two-line H5 cross-references in `remote_free_ring.rs` / `fallback.rs`. |
| `no dbg_* leak via other crate-root re-exports` | Enumerated `src/lib.rs`'s `pub use` list; the only re-exported types carrying `pub fn dbg_*` are `AllocCore` (fully handled by H2) and `SeferAlloc` (see F5). |

---

## F1 [P1] — the tree does not compile under `production medium-classes`; **two live `ci.yml` commands are red at `HEAD`**

**Where:** `src/registry/heap_core.rs:596` (`const _: () = assert!(size_of::<HeapCore>() <= 8192);`),
gated by the `#[cfg(not(any(...)))]` H1 added in `8b9ed10` and re-documented in `0d23e7f`.
Failing CI steps: `.github/workflows/ci.yml:590-592` and `:596-599`, in the
`test-feature-isolation` job (`ci.yml:428`).

**Reproduced by me, verbatim, at `HEAD` (`85dacfc`):**

```
$ cargo test --features "production medium-classes internals" \
      --test r14_4_promotion_move_leg_reduction --no-fail-fast
error[E0080]: evaluation panicked: assertion failed: size_of::<HeapCore>() <= 8192
   --> src\registry\heap_core.rs:596:15
error: could not compile `sefer-alloc` (lib) due to 1 previous error
```

I swept **every** distinct `--features "…"` string that appears in `ci.yml`
(37 sets) with `cargo check`. Exactly four fail, all the same assert:

```
FAIL: production medium-classes
FAIL: production medium-classes internals                  <-- ci.yml:592, live `run:`
FAIL: production medium-classes exact-span-large
FAIL: production medium-classes exact-span-large internals <-- ci.yml:599, live `run:`
```

Measured size (obtained by running the runtime pin under
`production medium-classes bench-internals internals`, where H1's own exclusion
disables the assert):

```
size_of::<HeapCore>() = 8408 bytes (budget 8192, headroom -216 B)
```

**What's wrong.** H1's commit body (`8b9ed10`) states its whole rationale as:

> "The 8192-byte budget was set based on `production` (the maximum SHIPPING
> composition), not `--all-features` (which includes experimental, test-only,
> and benchmark features like `experimental`/`pinning` that no real deployment
> would use)."

That premise is provably false. `medium-classes` is a **shipping opt-in**, not
an experimental/test/bench feature: it has four dedicated CI rows
(`clippy --features "hardened medium-classes internals"`,
`test --features "hardened medium-classes internals"`, plus the two failing
`production medium-classes …` rows), and its own `Cargo.toml` documentation.
H1 diagnosed the `--all-features` symptom, excluded the four features that
`--all-features` happens to add, and never asked the general question "which
*other* feature combinations exceed the budget" — so the fix is scoped to the
one combination that happened to be reported.

This is **pre-existing** (the pin landed in Round 34, R34-18; H1 only *narrowed*
when it fires, which can never make it fire more). But H1's stated task was
"file + fix the compile failures breaking the gate", the review that produced
H1 framed the finding as "the tree does not compile in 2 of 6 enforced
configurations", and wave 3 closed H1 as fully resolved with an explicit "no
open-items index entry is needed". Two *more* enforced configurations were left
red, unfiled, and contradicted by the fix's own written analysis.

It is invisible to `npm run check` because `PER_PR_ROWS` contains no
`production medium-classes` row — the local gate and `ci.yml`'s
`test-feature-isolation` job are **not** the same matrix, and only the clippy
subset is generated from the shared source of truth.

**Suggested fix (pick one, plus the guard):**
1. Raise the budget to cover the largest genuinely-shipping composition
   (`production medium-classes` = 8408 B → e.g. 8704 with the R34-18-mandated
   stack-pressure comment recording *why*), keeping the four experimental
   exclusions; **or**
2. add `medium-classes` / `medium-classes-wide` to the assert's
   `#[cfg(not(any(...)))]` list *and* to the mirrored list in
   `tests/r34_18_heap_core_stack_pressure_pin.rs`, and file an open item that
   the pin no longer covers a shipping opt-in (weaker — it silently drops
   coverage for a configuration users can actually deploy).
3. **Either way, add the guard:** a check that the feature sets `ci.yml`
   actually invokes all compile. The cheapest form is a script that greps
   `--features "…"` out of `ci.yml` and `cargo check`s each (that is literally
   how I found this, and it runs in a couple of minutes warm) — or extend
   `PER_PR_ROWS` to include the `test-feature-isolation` sets. Without it,
   `npm run check` will keep certifying green while `ci.yml` is red, which is
   exactly the failure mode `docs/CORRECTNESS_OPEN_ITEMS.md` item 11 records.

Also correct the now-false sentence in `src/registry/heap_core.rs`'s comment
block ("among those, the maximum is `production` + `internals` + `numa-aware`
at 7592 B, every smaller composition strictly below") — `production
medium-classes` is 8408 B and is *not* excluded by the cfg above it.

---

## F2 [P2] — H4 closed 4 of the 13 orphaned SHAs the review enumerated; 8 stale citations survive (6 of them bare)

**Where:** commit `6e5c067`; `CHANGELOG.md:87,97,98,106,109,111,112`;
`docs/perf/OPEN_ITEMS_ARCHIVE.md:1220,1222`.

I ran an independent reachability sweep before reading the source review:
extracted every backticked 7–40 hex token from every tracked `*.md`
(880 candidates), kept those that `git cat-file -t` resolves to a commit, and
tested each with `git merge-base --is-ancestor <sha> HEAD`. Results relevant to
G1's rebase:

| SHA cited | Reachable from HEAD? | Live successor (tree-identical) | Where cited | Handled by H4? |
|---|---|---|---|---|
| `73817ee` | NO | `5e75032` | CHANGELOG (annotated ✔), CORRECTNESS_OPEN_ITEMS (annotated ✔), **OPEN_ITEMS_ARCHIVE.md:1222 (bare)** | partly |
| `d46c349` | NO | `a7d7395` | CHANGELOG (annotated ✔), **OPEN_ITEMS_ARCHIVE.md:1220 (bare)** | partly |
| `a4dc38e` | NO | `ff496c6` | CHANGELOG (annotated ✔) | ✔ |
| `5710a6e` | NO | `358be4e` | CHANGELOG + R34_MANIFEST (annotated ✔) | ✔ |
| `7faa377` | NO | `55f8317` | **CHANGELOG.md:97 — bare** | ✗ |
| `e496d8b` | NO | `80463d2` | **CHANGELOG.md:87, :98 — bare** | ✗ |
| `9296adb` | NO | `dbb4016` | **CHANGELOG.md:106 — bare** (CORRECTNESS_OPEN_ITEMS.md:2260 is annotated ✔) | ✗ |
| `2f70081` | NO | `6190526` | **CHANGELOG.md:109 — bare** | ✗ |
| `04ba0f8` | NO | `1f1015a` | **CHANGELOG.md:111 — bare** | ✗ |
| `2e1ef90` | NO | `45c45be` | **CHANGELOG.md:112 — bare** | ✗ |
| `15a1ef6` | NO | `d17eec3` | CHANGELOG.md:108 — annotated "(post-rebase SHA `d17eec3`)" ✔ | pre-existing ✔ |
| `4d52cfb` | YES | — | correctly unaffected | ✔ |

The source review (`…sol-remediation-readonly-review.md:555-593`) listed
**thirteen** orphaned SHAs *with the exact replacement mapping*, including every
one of the six H4 missed. H4's commit body instead says it checked "5 candidate
short-SHAs" and "found 10 hits across 3 files", and claims:

> "not 4 — `docs/perf/OPEN_ITEMS.md` had none, contrary to the review's file
> list; `docs/perf/round-manifests/R34_MANIFEST.md` had the 10th"

The review's file list names `docs/perf/OPEN_ITEMS_ARCHIVE.md`, not
`docs/perf/OPEN_ITEMS.md`. H4 substituted the sibling filename, "disproved" the
review against the wrong file, and consequently never opened the archive — where
two bare stale citations still sit at `:1220` and `:1222`.

Net: the finding H4 was created to close is ~40 % closed, and the closing
commit's own body asserts a completeness it did not have. This is the *same*
"a later task invalidates an earlier artifact's cited identifier, with no sweep"
class the review named as "the wave's own charter defect recurring for the third
time".

**Suggested fix:** apply the review's own mapping table to the six remaining
CHANGELOG citations and the two archive citations; then add a mechanical guard —
the reachability sweep above is ~15 lines of Node and would make this class
impossible to miss again (a `verify-cited-shas.mjs` step in `check-all.mjs`,
allowing an explicit "originally `X`" annotation form as the escape hatch the
docs already use).

---

## F3 [P2] — H7's `### BREAKING CHANGE` heading was inserted mid-list and orphaned ~40 Round-34 bullets plus all three wave subsections out of the `### Round 34` section

**Where:** commit `5c17cc3`; `CHANGELOG.md:19`.

Current heading structure of the `[0.3.0]` section:

```
   8: ## [0.3.0] (unreleased)
  10: ### Round 34 — 26 tasks addressing findings from three independent readonly reviews …
  14: #### Measurement, correctness & tooling
  19: ### BREAKING CHANGE — `AllocCore`'s `dbg_*` diagnostic surface narrowed behind `internals`
  91: #### Post-closing independent review remediation (2026-08-05, tasks #547-552)
 102: #### Release-readiness remediation (2026-08-05, tasks #555-570)
 119: #### Release-readiness remediation follow-up (2026-08-05, tasks #571-578)
```

H7 inserted the `###` heading immediately after R34-3's bullet — i.e. *inside*
the `#### Measurement, correctness & tooling` bullet list, after only two of its
~27 bullets. Because `###` outranks `####`, this **terminates both** the
`#### Measurement…` subsection *and* the `### Round 34` section. Every remaining
Round-34 bullet (R34-4 … R34-27, lines 65–90) and all three remediation-wave
subsections (`:91`, `:102`, `:119`) now render as content of
**"BREAKING CHANGE — AllocCore's dbg_* diagnostic surface narrowed"**, not of
Round 34.

All nine pre-existing `### BREAKING CHANGE` precedents in this file
(`:2088, :2144, :2190, :2239, :2269, :2298, :2361, :2392, :2431`) sit *after* a
completed body of content, in a contiguous block — none is spliced into a live
bullet list. H7's commit message says it "read 3 of the 9 existing precedents…
to match their established intro-paragraph/**Why.**/**What changed:**/
**Migration.** structure" — it matched the *body* format and missed the
*placement* convention.

This is the same heading-structure defect class H3 fixed in this very wave, one
level down. H3's own structural verification (`grep -n "^## \["`) is `##`-only
by construction, so it could not have caught it.

**Suggested fix:** move the `### BREAKING CHANGE` block to *after* the Round-34
bullet list (before `#### Post-closing independent review remediation`, or into
the existing contiguous BREAKING-CHANGE block), or demote it to `####` if it is
meant to live inside the Round-34 section. Then extend H3's verification to
check `###`/`####` nesting, not just `##`.

---

## F4 [P2] — 36+ test files call now-`internals`-gated `AllocCore::dbg_*` without `internals` in their own `#![cfg]`, so they hard-fail instead of being cfg'd out

**Where:** consequence of `dbb4016` (Sol-F1, wave 2) and `25d6ac4` (H2, wave 3).

R34-3 (`b47cc6a`) established the invariant that a `tests/*.rs` file reaching
`internals`-gated surface carries `feature = "internals"` in its own crate-level
`#![cfg(...)]`, so that in a no-`internals` build it is **skipped**, not broken —
that is what the 107-file mechanical sweep was for. Sol-F1 and H2 then gated 124
`AllocCore::dbg_*` methods without re-running that sweep. H2 updated 19 test
files; a static sweep (parse each test's balanced `#![cfg(...)]`, cross-check
against the 124 gated method names) finds **39 more** that still lack it
(a handful, e.g. `tests/dbg_hook_safety_tripwire.rs`, are text-mention-only
false positives — the compiler-verified core is ≥36).

Compiler-confirmed at `HEAD`:

```
$ cargo test --no-run --features "production"       # no `internals`
error[E0599]: no method named `dbg_pool_cap` found for struct `AllocCore` …
error[E0599]: no method named `dbg_layout_class_for` found …
error[E0599]: no method named `dbg_push_to_ring` found …
… (20+ distinct methods)
error: could not compile `sefer-alloc` (test "regression_batch_flush")
error: could not compile `sefer-alloc` (test "regression_own_thread_large_no_leak")
error: could not compile `sefer-alloc` (test "segment_directory_a3")
```

Most of the 39 were already broken by wave 2 (`alloc_core_core_diag.rs` /
`alloc_core_small_diag.rs` / `alloc_core_small_reclaim.rs` methods). **Wave 3
newly broke at least two** whose only gated calls come from H2's own
`alloc_core_small_pool.rs`:

- `tests/r30_1_decomp_full_cycle_cursor_safety.rs`
  (`#![cfg(all(alloc-core, alloc-xthread, alloc-decommit, bench-internals))]`;
  calls `dbg_pool_cap`, `dbg_pooled_count`, `dbg_decomp_full_cycle`,
  `dbg_decomp_reserve_and_keep`, `dbg_decomp_release` — real call sites at
  `:72,:82,:85,:182,:187`)
- `tests/r31_15_reserved_small_segment_cross_core_release.rs`
  (real call sites at `:96,:105,:119,:126`)

**Severity note (why P2, not P1):** no *current* CI row or `check-all.mjs` step
hits it — every test-running invocation in `ci.yml` now appends `internals`.
So this is not CI-red today. It is (a) a broken developer command that
CLAUDE.md's own workflow suggests (`cargo test` under `production`), (b) a
violated structural invariant, and (c) a landmine: adding one no-`internals`
test row later (which is exactly what F1 above recommends) turns it red.

Also note H1 fixed **precisely one instance of this class**
(`tests/r34_18_heap_core_stack_pressure_pin.rs`, by adding `internals` to its
`#![cfg]`) without asking whether the class had other members.

**Suggested fix:** re-run R34-3's mechanical sweep over the current gated-method
set (the list is already computable — `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`
enumerates it), adding `feature = "internals"` to each affected test's
`#![cfg]`; then extend that same script with a second assertion: *no
`tests/*.rs` calls a gated `dbg_*` without declaring `internals`*. That closes
the class the way H2's script closed its own.

---

## F5 [P3] — `SeferAlloc::dbg_trim_current_thread` is still an ungated `pub` `dbg_*` hook reachable from a plain `--features production` downstream build

**Where:** `src/global/sefer_alloc.rs:616`.

Verified from a genuine downstream position (external probe crate,
`sefer-alloc = { default-features = false, features = ["production"] }`, no
`internals`):

```rust
let a = sefer_alloc::SeferAlloc::new();
a.dbg_trim_current_thread();                       // compiles
let _ = sefer_alloc::AllocCore::dbg_decommit_count();   // compiles (allowlisted, intended)
```
→ `Finished dev profile … in 8.53s`

`SeferAlloc` is re-exported at the crate root unconditionally on `alloc-global`
(`src/lib.rs:411`); `dbg_trim_current_thread` carries only `#[doc(hidden)]` —
no `#[cfg]` at all. Its five siblings in the same file
(`dbg_drain_current_thread_rings`, `dbg_current_large_cache_*`) all correctly
carry `feature = "internals"` in their cfg after H2.

This is **exactly the shape** of the defect Sol-F1/H2 exist to close, one type
up the hierarchy, and:
- `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` cannot see it — its
  declared scope is `impl AllocCore` blocks in `src/alloc_core/*.rs` only;
- H7's new `### BREAKING CHANGE` entry says the diagnostic surface is now
  narrowed behind `internals`, without qualifying that the `SeferAlloc`-level
  hooks are only *partly* so;
- CLAUDE.md's benchmark-hook rule 2 ("any hook with no production caller MUST
  default to gating behind `bench-internals`") is not satisfied — its only
  caller is `benches/global_alloc.rs`.

It is a *safe* `&self` method (no raw pointer parameter), so this is a
public-surface/semver concern, not a soundness one — hence P3. It is
pre-existing, not introduced by wave 3.

**Suggested fix:** add `#[cfg(all(feature = "bench-internals", feature = "internals"))]`
(and `internals` to `benches/global_alloc.rs`'s `required-features`), or, if the
hook must stay ungated for bench reasons, record it as an explicit documented
exception the way `dbg_push_to_ring` already is (README §"Where unsafe lives"),
and widen the exhaustive script's scope to `SeferAlloc`/any crate-root-re-exported
type so the boundary check is actually exhaustive rather than
`AllocCore`-shaped.

---

## F6 [P3] — the two new `docs/CORRECTNESS_OPEN_ITEMS.md` "Recently resolved" entries reuse item numbers 17 and 18, which are live OPEN items

**Where:** commits `800ee86` (item 17) and `2f16ba6` (item 18);
`docs/CORRECTNESS_OPEN_ITEMS.md:2257` and `:2296` vs the open list at `:1137`
and `:1154`.

```
OPEN     17 (:1137) — "Five tier-1 `unsafe` seams have no miri/loom/kani harness" (R34-2/#521)
RESOLVED 17 (:2257) — "H8 — dbb4016's fix(perf) prefix … left as-is"           (#578)

OPEN     18 (:1154) — "kani proves only the smallest seam …"                    (R34-2/#521)
RESOLVED 18 (:2296) — "Flaky test — repeated_same_segment_frees…"               (wave 3)
```

Both `800ee86`'s commit body, `2f16ba6`'s commit body, and `CHANGELOG.md`'s
wave-3 section refer to "`docs/CORRECTNESS_OPEN_ITEMS.md` item 17" / "item 18"
with no disambiguation — pointing, as written, at the wrong entries. H5's own
new items (22, 23) correctly took the next free numbers in the open list, so the
convention was understood in the same wave; the next free numbers for these two
were 24 and 25.

This is the item-number-collision class the *immediately preceding wave* fixed
twice (G6/task #560 and G6-followup/task #570, both in `docs/perf/OPEN_ITEMS.md`),
now recurring in the sibling index. (Numbers 12–16 already collide across the two
lists pre-existing; wave 3 extended the collision to 17–18.)

**Suggested fix:** renumber the two new resolved entries to 24/25 (or introduce
an explicit `R-` prefix for the resolved list and renumber it once), and update
the two commit-message references indirectly via the CHANGELOG bullet, which is
the citation a future reader will actually follow.

---

## F7 [P3] — `R34_REMEDIATION_3_MANIFEST.md` covers 11 of wave 3's 14 commits; its own residual note predicts 1 more, but 3 landed

**Where:** `docs/perf/round-manifests/R34_REMEDIATION_3_MANIFEST.md` (commits
`db63aed` + `28663e4`).

```
$ git log --oneline 2a7f1e6..HEAD | wc -l
14
manifest §1 rows: 11 (8b9ed10 … 2f16ba6)
missing: 28663e4 (the manifest's own extension commit), b57f988 (CHANGELOG), 85dacfc (checkpoints)
```

Arithmetic cross-check: `40241b0..HEAD` = **81** = 43 (Round 34 proper) + 7
(wave 1) + 17 (wave 2) + 14 (wave 3). Waves 1 and 2 are exact and *do* include
their own closing commits (`4f45eee`, `2a7f1e6`); wave 3's does not, so the
three manifests are mutually inconsistent in convention.

The file is honest about being incomplete ("One more commit … will still land
after this file's own edit"), but that prediction is wrong by two, and
CLAUDE.md's R34-24 rule requires the manifest to record *every* commit in the
round. H6's whole thesis was that "the recurring under-count pattern" is now
"closed structurally"; the very first manifest produced under the new scheme
under-counts.

**Suggested fix:** the wave-closing commit (`b57f988`/`85dacfc`) should have
carried the final three rows — the natural convention is "the wave's last
commit updates the manifest, listing itself" (which is what waves 1 and 2 did,
since they were written retroactively). Add the three rows; state the convention
explicitly in `R34_REMEDIATION_1_MANIFEST.md`'s header so wave 4 does not
repeat it.

---

## F8 [P4] — "3 of 9 files" / "across 9 files" is wrong; there are 6

**Where:** `CHANGELOG.md:119` (wave-3 intro paragraph, "Sol-F1's `internals`-gating
fix covered only 3 of 9 files"), `CHANGELOG.md`'s H2 bullet (same phrase), and
H7's `### BREAKING CHANGE` entry (`CHANGELOG.md:~40`, "~125 `AllocCore::dbg_*`
inherent methods across 9 files").

Ground truth (derived, not counted by hand — same walker the committed verify
script uses):

```
alloc_core.rs                  3
alloc_core_core_diag.rs       73
alloc_core_large_cache.rs     15
alloc_core_small_diag.rs      14
alloc_core_small_pool.rs      19
alloc_core_small_reclaim.rs    4
                    files: 6   total: 128   (124 gated + 4 allowlisted)
```

Sol-F1 covered 3 of **6**; H2 covered the other 3. "~125" is a fair rounding of
124 gated; "9 files" is not a rounding of anything. (It may be 6 + the 3
delegation files `heap_core_diag.rs`/`heap_core.rs`/`sefer_alloc.rs`, but those
hold `HeapCore`/`SeferAlloc` wrappers, not `AllocCore` inherent methods.)

**Suggested fix:** replace "9 files" with "6 files" in all three places, or
state "6 `src/alloc_core/*.rs` files + 3 delegation files" if the larger number
was intended.

---

## F9 [P4] — `scripts/check-all.mjs` still says "the 5 `clippy` rows" / "clippy x5"; there are 6 — and `25d6ac4` edited that exact line

**Where:** `scripts/check-all.mjs:19`, `:106`, `:224`.

`PER_PR_ROWS` currently resolves to 8 rows: **6** clippy + 1 test + 1 check
(verified by importing `scripts/check-matrix.mjs` and counting). The header
comment (`:19` "2-6. the 5 `clippy` rows"), the inline comment (`:106` "the 5
PER_PR_ROWS clippy rows (default / experimental / --all-features / hardened
medium-classes / production)" — a 5-name list missing `production internals`),
and the runtime banner (`:224` "clippy x5 [generated], test x4") are all stale
since R34-3 added the 6th row.

`25d6ac4` modified line 224 (to insert the new verify step's name into the
banner) and left "clippy x5" untouched — and the *immediately preceding wave*
spent a whole commit (`eb66af6`) fixing this exact stale "5 clippy rows" count
in `ci.yml`, without checking the sibling file.

**Suggested fix:** make the banner derive the counts
(`` `clippy x${clippyRows.length}` ``) rather than hardcode them, and fix the two
prose comments.

---

## F10 [P4] — the new exhaustive verify script accepts a plain `//` comment as an `internals` gate

**Where:** `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`,
`precedingBlockIsInternalsGated()`.

The walker accepts `#[`, `///`, `//!`, and `//` lines as part of the preceding
block, and sets `gated = true` for **any** of them containing the literal
`feature = "internals"`. A doc comment such as
`/// Requires `#[cfg(feature = "internals")]`.` above an ungated method makes
the oracle report it as gated.

I checked whether any method currently relies on this: I re-ran the same walker
with the gate condition restricted to lines starting with `#[` — **zero**
methods change classification, so the committed 124/4/0 result is correct today.
This is a latent weakening of the oracle, not a present false pass.

Two smaller scope notes on the same script:
- `readdirSync(ALLOC_CORE_DIR)` is non-recursive, so `src/alloc_core/deferred_large/`
  is never scanned. Verified currently harmless (`grep` finds no `pub fn dbg_*`
  there, and every `impl AllocCore` block in `src/` lives in a top-level
  `src/alloc_core/*.rs` file), but a future `impl AllocCore` in a subdirectory
  would be invisible.
- Scope is `AllocCore` only — see F5 for the concrete method this misses.

**Suggested fix:** `if (trimmed.startsWith('#[') && trimmed.includes('feature = "internals"')) gated = true;`
and a recursive directory walk.

---

## Appendix — commands used for the independent verification

```bash
# reachability sweep behind F2
git ls-files '*.md' | xargs grep -ohE '`[0-9a-f]{7,40}`' | tr -d '`' | sort -u \
  | while read s; do t=$(git cat-file -t "$s" 2>/dev/null) || continue; [ "$t" = commit ] || continue;
      git merge-base --is-ancestor "$s" HEAD 2>/dev/null || echo "ORPHAN $s"; done

# rebase-pair tree identity
git rev-parse 73817ee^{tree} 5e75032^{tree}   # → identical (and for the other 3 pairs)

# F1: every ci.yml feature set
grep -ohE -- '--features "[^"]+"' .github/workflows/ci.yml | sed 's/--features //; s/"//g' | sort -u \
  | while read -r fs; do cargo check --features "$fs" 2>&1 | grep -q '^error' && echo "FAIL: [$fs]"; done

# F1: exact failing CI command
cargo test --features "production medium-classes internals" \
  --test r14_4_promotion_move_leg_reduction --no-fail-fast

# F1: measured size where the assert is disabled
cargo test --features "production medium-classes bench-internals internals" \
  --test r34_18_heap_core_stack_pressure_pin -- --nocapture

# F4: no-internals test compile
cargo test --no-run --features "production"

# F5 / H2 positive+negative boundary: external probe crate outside the repo
#   sefer-alloc = { path = "…", default-features = false, features = ["production"] }
#   ac.dbg_large_cache_used()        → E0599 (correctly hidden)
#   a.dbg_trim_current_thread()      → compiles (F5)

# full local gate
npm run check    # → single failure: verify-commit-prefixes on 43115cf / 5c1142f (pre-existing)
```
