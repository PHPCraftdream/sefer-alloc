# Round 34 — independent READONLY review

**Date:** 2026-08-05
**Reviewer:** independent `@oh` session (readonly; no working-tree change other
than this file, which stays untracked per this project's established convention).
**Scope reviewed:** `0762772~1..8cb89ea` plus the closing commit `c5db553`
(`docs: extend Round 34 CHANGELOG with all 26 tasks + commit session
checkpoints`), i.e. the full round `40241b0..c5db553` = **43 commits**.
**Method:** `git log`/`git show`/`git ls-tree` on every commit in the range;
full diff reads on the four highest-risk tasks (R34-14, R34-15, R34-17,
R34-23) plus R34-3/R34-4/R34-16/R34-18/R34-24; independent re-derivation of
the load-bearing mechanisms (the `push_large_deferred_free` CAS contract, the
`SegmentHeader::large` constructor byte comparison, the
`realloc_inplace_fast_path_known_base` return-site enumeration); repo-wide
greps for stale claims, TODO/placeholder, and safe-`pub fn`+raw-pointer
hazards. No build or test was run (shared workspace).

---

## 0. Verdict

**The round is of high quality.** The engineering substance holds up under
independent re-derivation: R34-14 is a genuine, correctly-diagnosed
permanent-leak bug fix (verified end-to-end below, not taken on trust);
R34-15/R34-17 are careful, correctly-scoped hardening; R34-3's `internals`
boundary is a real semver boundary; the crossbeam-epoch bump is surgically
clean. Zero TODO/placeholder/half-wired code was found in `src/`. No new safe
`pub fn` accepting a raw pointer and touching allocator metadata was
introduced — CLAUDE.md's benchmark-hook rule is respected, and all six new
`dbg_*` hooks are correctly classified in `tests/dbg_hook_safety_tripwire.rs`.
The out-of-scope-commit pattern from R34-2's first attempt (`00a1c59`) did
**not** recur in any of the other 24 tasks.

**Eight findings**, none of them a correctness or soundness defect in shipping
code. Two are worth acting on (`[P2]`); six are documentation/process accuracy
(`[P3]`/`[P4]`). The dominant theme is *stale cross-references produced
within the round itself* — a later task invalidating an earlier task's cited
number or line, with no sweep. This is a milder instance of the same class the
round's own R34-21/R34-22 built structural pins for.

| # | Sev | Finding | Anchor |
|---|-----|---------|--------|
| F1 | **P2** | The `internals` boundary test is vacuous in every configuration actually run | `tests/r34_3_internals_boundary_api.rs:60` |
| F2 | **P2** | A live current-state card still cites the ~40× realloc figure R34-23 refuted this round | `docs/perf/OPEN_ITEMS.md:1188` |
| F3 | P3 | Correctness index item 15 still reads "BLOCKED on #524/R34-5" after #524 closed | `docs/CORRECTNESS_OPEN_ITEMS.md:1135` |
| F4 | P3 | R34-16's five-tripwire enumeration cites a line number R34-23 invalidated | `src/global/sefer_alloc.rs` (site 1) |
| F5 | P3 | The round manifest omits 4 commits and 2 tasks; CHANGELOG bills it as complete | `docs/perf/round-manifests/R34_MANIFEST.md:47` |
| F6 | P3 | CHANGELOG's R34-3 commit list omits `27879af`, the commit that creates the boundary | `CHANGELOG.md:17` |
| F7 | P3 | R34-24's own policy names a tier-2 violator but neither remediates nor indexes it | `CLAUDE.md` (artifact policy, point 1) |
| F8 | P4 | `docs/ALLOC_BENCH.md` still publishes the "physically impossible" 9.67 µs figure | `docs/ALLOC_BENCH.md:247` |

---

## 1. Did each landed commit do what the task claimed? — **YES**

I spot-checked the four tasks flagged as riskiest, re-deriving each claim from
source rather than from the commit message.

### R34-14 (`7ef5a46`) — "real permanent-leak bug fix" — **CONFIRMED, genuine**

This is the round's strongest claim and it survives independent scrutiny. The
full causal chain checks out:

1. `drain_large_deferred_free` (`src/alloc_core/deferred_large/drain.rs:47-77`)
   pops a base off the stack and calls `core.reclaim_large_segment(cur)` — it
   **never resets `deferred_next` back to `ABANDONED_TAIL`**. I read the whole
   function; there is no such store. So a drained segment carries either
   `DEFERRED_LARGE_TAIL` or a real address in its link word.
2. `push_large_deferred_free` (`src/alloc_core/deferred_large/push.rs:110-120`)
   claims the link word with `compare_exchange(ABANDONED_TAIL, next_link, …)`
   and **`return`s on failure** — the documented double-push guard.
3. Therefore a cache-hit reuse that carries `deferred_next` forward makes the
   next cross-thread free's claim CAS fail → the push is silently dropped →
   the segment is permanently leaked. Exactly as claimed.

The fix's three reset values are byte-identical to the `SegmentHeader::large`
constructor at `src/alloc_core/segment_header.rs:828-830` +
`owner_thread_free: core::ptr::null()`:
`pack_owner(OWNER_STATE_LIVE, OWNER_ID_NONE, 0)` / `ABANDONED_TAIL` / null.
The commit's "verified byte-for-byte" claim is accurate. The `[correctness
fix, P0]` CHANGELOG tag and the `fix(perf)` prefix are both correct under
R30-12 (invariant restored, no speedup claimed).

### R34-15 (`49929d0`) — OOM widening — **CONFIRMED, correctly scoped**

`Registry::slot_or_none` / `try_ensure_chunk` are added as genuinely fallible
siblings; `ensure_chunk`/`slot` keep `std::process::abort()` for the alloc
path, which the diff documents with an explicit rationale rather than leaving
implicit. All three free-path call sites in `heap_core_xthread.rs` fold `None`
into a *pre-existing* defensive branch, and `owner_slot_is_live`'s `None =>
true` matches the `idx >= MAX_HEAPS => true` default two lines above it — so
the OOM path cannot short-circuit RAD-4's spin. The F-3 residual is documented
at the call sites, not glossed. No scope creep.

### R34-17 (`c270b0c`) — RAII guards — **CONFIRMED**

`DrainHeadPublish`'s `Drop` is the sole `head` writer on the drain path;
`publish.h` is updated only after a slot is fully processed
(reclaimed + cleared + advanced), so the unwind path publishes exactly the
progress made — the "only real progress is published" claim holds. The
pre-fix code stored unconditionally after the loop, so the guard introduces no
behavioural change on the happy path. `src/global/mod.rs`'s +10 lines are
`pub use` re-exports only — **compliant** with CLAUDE.md's "`mod.rs` —
reexports only, no code" rule (I checked this specifically because a `+10` on
a `mod.rs` is a common violation shape).

### R34-23 (`19b1918`+`ba716a0`+`827a57a`) — realloc gate + README correction

The path-activation oracle is sound: I enumerated every `return` site in
`realloc_inplace_fast_path_known_base` and all five are counted (2× `Some`
Large, 1× `None` Large, 1× `Some` Small, 1× final `None`), so the documented
`inplace_large + inplace_small + decline == total` invariant genuinely holds
by construction rather than by assertion. Increments are `alloc-stats`-gated
and `alloc-stats` is **not** in `production` (`Cargo.toml:399`), so nothing
ships. README's live rows are correctly corrected (`README.md:918-919`,
`:1203-1209`); residual `~40×` strings in README are explicitly framed as the
historical claim being corrected. See F2/F8 for the two sites the sweep
missed outside README.

---

## 2. False claims leaked into CHANGELOG.md? — **THREE, all P3 (none of the R33-G1 severity)**

I verified the numeric claims that are cheapest to falsify. **These all check
out:**

- R34-3 "a mechanical, 107-file `tests/*.rs` cfg-gate update" — `b47cc6a`
  touches 108 `tests/` files, of which one is the new boundary test → 107
  re-gated. ✅
- R34-6 "44 hunks across 42 test files" — `7aeee2d`: 44 `@@` hunks, 42 files. ✅
- R34-18 `size_of::<HeapCore>() == 7576`, budget 8192, `Tcache` 6664 B —
  matches the const-assert block verbatim. ✅
- R34-13 `OWN_CACHE_SIZE = 16` — `src/alloc_core/segment_table.rs:180`. ✅
- R34-24 "recounted at 256/256, all under the ceiling; largest 145 KiB" —
  `ls docs/perf/_raw_*.log | wc -l` = 256; largest is
  `_raw_r34_10_sparse_decay_gate.log` at ~144 KiB. ✅
- R34-16's five tripwire *message strings* — all five present, at the claimed
  files (see F4 for the line-number caveat). ✅
- `docs/ARCHITECTURE.md:497` "244 files, as of task #537" vs actual
  `ls tests/*.rs | wc -l` = 244. ✅ (This one was touched by six separate
  commits this round and still landed correct.)

**Three inaccuracies found:**

### F6 [P3] — `CHANGELOG.md:17` omits the commit that creates the boundary

The R34-3 bullet ends "Commits `0762772`+`b47cc6a`+`f9ae91f`." It omits
**`27879af`** — the commit that actually adds the `internals` feature to
`Cargo.toml` and rewrites `src/lib.rs`'s three module declarations, i.e. the
entire substance of the task — and substitutes `f9ae91f`, an untagged
follow-up that only edits a doc count in `docs/ARCHITECTURE.md`. The manifest
gets this right (`R34_MANIFEST.md:119`: `27879af`, `b47cc6a`, `0762772`), so
the two artifacts disagree. A reader following the CHANGELOG's citation to
audit the boundary would never see the diff that creates it.

### F5 [P3] — the manifest is billed as complete but is not

`CHANGELOG.md:10` (Round 34 header) points readers to the manifest for "the
round's **full** commit-by-commit classification", and `CHANGELOG.md:39` says
it classifies "all **38** of this round's commits". Both overstate it:

- `git log --oneline 40241b0..8cb89ea | wc -l` = **42** (43 including
  `c5db553`). The manifest's §1 upper bound is `827a57a` (38 commits) — which
  the manifest itself states honestly ("HEAD-at-manifest-time"), so the defect
  is in the CHANGELOG's framing, not the manifest's own preamble.
- Missing from §1: `4ba188a`, `9b06b56`, `7758f7a`, `8cb89ea`.
- Missing from §2 entirely: **R34-25** and **R34-26** have no per-item
  verdict, so the artifact whose stated purpose is "a single per-round
  artifact that records … the final verdict per item" is silent on two of the
  round's 26 items.
- `R34_MANIFEST.md:122` attributes `a9edc87` to "**(untagged)**". It is
  R34-6/task #525 (the CHANGELOG has this right at `CHANGELOG.md:20`). The
  commit subject genuinely omits the task tag, but the manifest recorded the
  symptom instead of resolving it — which is precisely the lookup the manifest
  exists to provide.

Since R34-24 explicitly positions this manifest as "the reference example
future rounds should match", these gaps propagate as a template.

### F-third [P3, folded into F5's category] — the header names two reviews, the round used three

`CHANGELOG.md:10` reads "closed all 26 findings from **two** independent
readonly reviews (`…release-stabilization-audit.md`, … and
`…r32-r33-global-bench-readonly-review.md`)". But `4aaca52`'s own commit body
opens with "**Round-33 readonly review finding G1 (P2)**" — i.e. R34-1's
source is `docs/reviews/2026-08-03-round33-readonly-review.md`, a *third*
review the header does not name. R34-2's commit body likewise cites
"Round-32/33 review **+** 2026-08-04 audit/bench-review findings", and
`docs/CORRECTNESS_OPEN_ITEMS.md` item 20 is sourced to the Round-33 review as
well. The header also equates "26 findings" with the round's 26 *tasks*, which
is not a 1:1 mapping (R34-25/R34-26 are research design gates, not review
findings). Low impact, but it is a provenance claim in the same sentence that
asks the reader to trust the round's sourcing.

---

## 3. Is `docs/perf/OPEN_ITEMS.md` current? — **MOSTLY, with one live stale verdict**

Structurally the indexes are in good shape and were actively maintained
throughout the round (R34-2, R34-10, R34-11, R34-22, R34-23, R34-26 all
touched them; the correctness index carries clean `RESOLVED by R34-6 / R34-16
/ R34-18 / R34-19` trails). R34-2's own work — including the caught-and-fixed
`F10 → 7d55209` misattribution — is solid, and R34-24's new "current-state,
not archive" structural rule is a good addition. **Two currency defects:**

### F2 [P2] — `docs/perf/OPEN_ITEMS.md:1188` still asserts the refuted ~40×

Item `[L]`12's **"Current number/verdict"** card reads:

> optional, NOT a next step a round should plan around — the sub-16 KiB tail
> is already cheapest (**OPT-G in-place Large-grow is ~40× faster than
> mimalloc**), so marginal payoff is small even at a favorable hit rate.

R34-23 refuted exactly that figure **in this round** — the real ratio is
~1.8–2.1× (`README.md:918`, `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md:30`),
and the report calls the underlying 9.7 µs "physically impossible". This is
not a cosmetic stale string: the ratio is the card's stated *reason* for the
item's low-value verdict, and it is off by roughly 20×. Under CLAUDE.md's
"OPEN_ITEMS indexes are CURRENT-STATE, not archives" rule — added by this
same round (R34-24) — a current-state card must not assert a number the round
itself disproved.

Worth noting the sequencing: `ba716a0` (the correction) landed at ~23:0x, and
`8cb89ea` (R34-26) edited `docs/perf/OPEN_ITEMS.md` *after* it, so there was a
later touch of the very file that could have swept it.

### F3 [P3] — `docs/CORRECTNESS_OPEN_ITEMS.md:1135`: a blocker that unblocked mid-round

Item 15 (provenance asymmetry / audit F-2) ends:

> **BLOCKED on #524/R34-5** (the concurrent small-block ring miri test); only
> if that test flags under Stacked Borrows should the `atomic_ptr_ref`
> treatment be applied to `atomic_u32_at`.

Task #524 **completed this round** (`fd54ddc`, plus `b47a261`/`91ff1dd`
fixing the local wrappers). The new test is wired into the `miri-plain` job
(`.github/workflows/ci.yml:901`) whose `MIRIFLAGS` are
`-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5` — no
`-Zmiri-tree-borrows`, i.e. miri's default **Stacked Borrows** model is
active, which is precisely the condition the item names. So the item's own
decision rule has been answered (in the negative), yet the card still presents
#524 as pending. A fresh round reading this index at round start — the exact
use case CLAUDE.md mandates — would draw the wrong conclusion.

---

## 4. Is the crossbeam-epoch bump (task #523) clean? — **YES, exemplary**

`353cc05` touches exactly three files. `Cargo.lock`'s diff is **4 lines, one
package**: `crossbeam-epoch` `0.9.18 → 0.9.20` plus its checksum. No other
package version, no other checksum, no dependency-graph change, no transitive
churn — which is what `cargo update -p crossbeam-epoch --precise 0.9.20`
should produce and does. `deny.toml` replaces the `RUSTSEC-2026-0204` ignore
entry with a comment recording the bump and its task of origin (removing a
suppression rather than leaving it stale). `CHANGELOG.md` gains the Round 34
heading. No `src/` change, and `production` (`Cargo.toml:399`) contains no
`experimental`, so the advisory never reached a shipping build — as the commit
message claims. **No finding.**

---

## 5. Does the `internals` boundary (task #522) actually close the semver gap? — **YES structurally, but the guard is vacuous**

**The boundary itself is real.** `Cargo.toml:455` defines `internals = []`;
`Cargo.toml:399`'s `production` list does **not** contain it, so no
`--features production` consumer gets it implicitly. `src/lib.rs`'s mechanism
is correct: each of `alloc_core`/`global`/`registry` gets two `cfg`-disjoint
declarations — `pub mod X` under `all(<base>, internals)` and `pub(crate) mod
X` under `all(<base>, not(internals))` — so the module body always compiles
(keeping the crate-root `pub use` re-exports working) while only the
**external path** `sefer_alloc::alloc_core::*` is conditional. That is exactly
the right shape, and the ~50 `pub unsafe fn dbg_*` hooks come off the public
semver surface as claimed.

### F1 [P2] — but `tests/r34_3_internals_boundary_api.rs` never runs in the configuration it exists to test

The test file's own module doc is explicit about its intent (lines 16-22):

> This file is compiled under PLAIN `alloc-core`/`alloc-decommit`/
> `alloc-global` (**no `internals`**) … so a future change that accidentally
> moves one of these re-exports behind `internals` (or removes it) fails to
> COMPILE here.

It gates itself with `#![cfg(all(alloc-core, alloc-global, alloc-decommit))]`
(line 60-64) and deliberately does **not** exclude `internals`. I enumerated
every `cargo test` invocation in `.github/workflows/ci.yml` and every row in
`scripts/check-matrix.mjs`:

- Every row that enables `alloc-global` **also passes `internals`** (ci.yml
  lines 289, 301, 305, 313, 316, 334, 343, 357, 382, 407, 475, 516, 524, 587,
  594, 641, 645, 655, 671, 676).
- The rows *without* `internals` are: `cargo test --no-fail-fast`
  (`default = ["std"]`, line 107 — no `alloc-core`), `--features experimental`,
  `--features pinning`, `--features "alloc-core page-map-diag"` (no
  `alloc-global`), and `cargo test --release --features "alloc-global
  alloc-xthread"` (no `alloc-decommit`). **None satisfies the file's own
  `#![cfg]`**, so the file is compiled out in all of them.
- `--all-features` (line 421) compiles it, but turns `internals` **on**.
- `scripts/check-matrix.mjs`'s `clippy-production` row is now `libOnly: true`
  (`--lib`, not `--all-targets`), so it never compiles `tests/` at all.

Net: the file only ever compiles with `internals` **enabled**, where every
name it imports resolves whether or not the re-exports were moved behind the
feature. The guard **cannot fail for the reason it was written**, in any
configuration this repo actually executes. Its negative half is separately and
honestly disclosed as unguarded ("verified structurally and repeatedly by this
task's own zero-trust verification pass" — a one-time manual check, not a
standing gate), so the `internals` boundary currently has **no standing
automated protection at all**.

This is the same "pass by absence" class the round itself caught elsewhere
(R34-5's `miri.mjs`/`tsan.mjs` 0-test passes) and that R33-3 caught in a loom
counterfactual — which is why I rate it P2 despite the boundary being
correctly built. The cheapest fix is one dedicated CI row, e.g.
`cargo test --features "alloc-core alloc-global alloc-decommit" --test
r34_3_internals_boundary_api` (the file's own doc already names this
invocation; nothing runs it).

---

## 6. Did the "out-of-scope committed files" pattern recur? — **NO**

I listed every changed path for all 43 commits and compared each against its
subject line. The R34-2 incident (`b45b824` committing three review reports,
self-corrected by `00a1c59`) is the **only** instance. Specifically checked
and cleared:

- `b45b824`'s edit to `R32_3_…_summary.csv` — *in scope*, and explicitly
  justified in its own commit body (a `doc_commit` parent→landing correction,
  re-derived through the checked script).
- `a3831e5` (`.gitignore`/`Cargo.toml`/`package.json`), `fd54ddc`
  (`ci.yml`/`miri.mjs`/`ARCHITECTURE.md`), `0e29fc2` (`README.md` +
  `bench-table.mjs`) — all necessary consequences of their stated task.
- `9e70266` (R34-12) force-adds 3 `docs/perf/paired_ab_runs/*.json` (107/36/36
  KiB). This is **policy-compliant**: `4aaca52` re-documented the
  scratch-by-default + `git add -f`-when-cited exception for that directory
  earlier in the same round, and 36 such files are tracked repo-wide.
- The three untracked review reports plus `.claude/` are correctly left
  untracked at HEAD; `c5db553` explicitly excludes them.

### F7 [P3] — but a *new* artifact directory sidesteps both conventions

`ba716a0` created **`docs/perf/r34_23_runs/`**, which:

- is matched by **no `.gitignore` rule** (`git check-ignore` returns nothing),
  unlike `docs/perf/paired_ab_runs/`, so its contents were committed as
  ordinary tracked files rather than through the deliberate force-add
  citation gate; the same applies to `docs/perf/r34_12_run.json`; and
- contains `2026-08-04T22-03-44-381Z_direct_raw.json` at **258 KiB**, which
  exceeds the 200 KiB force-add ceiling R34-24 set later the same round.

To the round's credit, `9b06b56` **found and honestly documented this**,
amending CLAUDE.md to name `docs/perf/r34_23_runs/*.json` as "the first real
tier-2 case — see point 2 below". But point 2 mandates truncation **or**
gzipping, and neither was applied: the file remains committed verbatim at 258
KiB. And the known deviation is filed in **neither** open-items index (I
grepped both for `r34_23_runs` / `tier-2`), so under CLAUDE.md's own round-start
convention no future round inherits a trigger to close it. The policy is also
keyed on the `_raw_*.log` **filename glob**, which is precisely why its
compliance census (256/256) cannot see its own largest violator — a naming-based
blind spot demonstrated by a file from the same round.

---

## 7. General quality — **clean**

- **Benchmark-hook rule (CLAUDE.md, R25-1):** the round adds six safe `pub fn
  dbg_*` hooks — `dbg_reloc_inplace_large_count`, `dbg_reloc_inplace_small_count`,
  `dbg_reloc_fastpath_decline_count`, `dbg_set_inject_fallback_init_panic`,
  `dbg_init_state`, `dbg_panic_in_fallback_init_rolls_back`,
  `dbg_set_inject_chunk_oom`, `dbg_slot_or_none`. **None takes a raw pointer**,
  so none is the `unsafe fn`-required shape. All are classified in
  `tests/dbg_hook_safety_tripwire.rs` (`PURE_OBSERVERS` / `SAFE_MUTATORS`)
  with per-entry justifications in the same commits that add them — good
  discipline. *Nit, not a finding:* `dbg_slot_or_none` is filed under
  `PURE_OBSERVERS`, but calling it can materialise a registry chunk (an OS VM
  reservation side-effect). Benign and idempotent — a later `claim()` would
  materialise it anyway, as the code says — but "pure observer" slightly
  overstates it.
- **Counter cost:** all three R34-23 increments are `#[cfg(feature =
  "alloc-stats")]`, and `alloc-stats` is not in `production`. Nothing is added
  to the shipping realloc path. The always-compiled accessors follow the
  pre-existing `OPT_H_ATTEMPTS` convention in the same file, so this is not a
  new deviation.
- **TODO/placeholder/half-wired:** `grep` for `TODO|FIXME|XXX|unimplemented!|
  todo!()` across `src/` returns **zero** hits.
- **`mod.rs` rule:** the only `mod.rs` touched (`src/global/mod.rs`, +10) is
  `pub use` re-exports only. Compliant.
- **Commit-prefix taxonomy (R30-12):** correctly applied throughout, including
  the deliberate `fix(security)` choice for `353cc05` (justified in-body
  against all five perf slots) and `fix(perf)` rather than `perf(runtime)` on
  all six production-source commits. The "Runtime improvements this round: 0"
  header is accurate — `production`'s composition is unchanged.

### F4 [P3] — one `src/` doc line-number citation went stale inside the round

`src/global/sefer_alloc.rs`'s R34-16 module doc enumerates the five
release-surviving tripwires by `file:line`. Site 1 cites
`alloc_core/alloc_core.rs:2158`; the `assert!` is now at **2203–2205**. Cause:
`19b1918` (R34-23) added ~45 lines near `alloc_core.rs:411` *later the same
day*, shifting everything below. Sites 2–5
(`alloc_core_large_cache.rs:147/160/166/321`) are still byte-exact — I checked
all four.

`tests/no_panic_doc_accuracy.rs` pins the five **message strings** and their
occurrence counts (`assert_count`), not their line numbers, so this drift is
structurally undetectable by the very test written to prevent this doc from
regressing. Worth either dropping the line numbers from the doc (the file +
function name are unambiguous) or extending the pin to line positions.

### F8 [P4] — `docs/ALLOC_BENCH.md` still publishes the refuted figure

`docs/ALLOC_BENCH.md:247` carries:

> `realloc_grow_geometric` (64 B→4 MiB) | **9.67 µs** | 382.7 µs | 2.78 ms |
> **39.6× faster** | **288× faster**

with the surrounding prose "**The realloc rows are the news** — the X-arc
turned realloc from parity into a rout" and "SeferAlloc improved on ITSELF 33×
/ 1,850×". Line 248's `~1,500×` is likewise superseded (R34-23 re-measured
~3,350×, so that one is understated, not wrong in direction).

I weighed whether this is merely archival — the file is structured as dated
snapshots and does carry "do not compare" warnings for `Vec_push` and the
churn benches. It falls short of that defence on two counts: (a) the file's own
header states "Everything else (large alloc/free, **realloc**, cold-direct) is
methodology-unchanged across sections", which actively *invites* cross-section
comparison of exactly these rows; and (b) R34-23's finding is not "superseded
by a newer measurement" but "**physically impossible**" — i.e. wrong when
written. This project's own append-only-correction convention (used by R31-12,
R32-4, R32-10 §11.4 and R34-1 itself) would call for a dated note here. P4
because it is a historical log rather than a live claim surface.

---

## 8. What I checked and found clean (no finding)

Recorded so a later reader knows these were covered, not skipped:

- Every commit's file list vs. its subject (43 commits) — one known incident
  only (§6).
- `Cargo.lock` diff isolation for the dependency bump (§4).
- `production` feature composition unchanged across the round
  (`Cargo.toml:399` untouched by any commit in range) — the "Runtime
  improvements: 0" claim is sound.
- `src/lib.rs` two-declaration `internals` mechanism for correctness (§5).
- `SegmentHeader::large` constructor vs R34-14's three resets — byte-identical.
- `drain_large_deferred_free` and `push_large_deferred_free` full bodies —
  the leak mechanism is real.
- All five return sites of `realloc_inplace_fast_path_known_base` vs the
  oracle's documented sum invariant — holds.
- `RemoteFreeRing::drain` full body post-guard, including early-`break`
  interaction with the guard's `Drop` — correct.
- `tests/dbg_hook_safety_tripwire.rs` additions in all three commits that add
  hooks — complete.
- `docs/ARCHITECTURE.md:497` test-file count vs `ls tests/*.rs | wc -l` — 244
  = 244, despite six separate commits editing it.
- `docs/perf/OPEN_ITEMS.md` / `docs/CORRECTNESS_OPEN_ITEMS.md` are genuinely
  distinct files (identical 2019-line counts are coincidence; md5 differs).
- Repo-wide grep for surviving `~40×` / `~1,500×` / `9.7 µs` claims — two live
  sites found (F2, F8); all others are correction context or archive.
- `grep` for TODO/FIXME/placeholder in `src/` — zero.

---

## 9. Recommended follow-ups (suggested priority)

1. **F1** — add one CI/local row running the boundary test without
   `internals`, else the `internals` semver boundary has no standing guard.
2. **F2** — correct `docs/perf/OPEN_ITEMS.md:1188`'s ~40× to R34-23's
   ~1.8–2.1× and re-check whether item `[L]`12's low-value verdict still
   follows from the corrected ratio.
3. **F3** — resolve `docs/CORRECTNESS_OPEN_ITEMS.md` item 15 now that #524
   has answered its trigger.
4. **F5/F6** — extend the manifest to the round's real span (add the 4
   commits, R34-25/R34-26 verdicts, resolve `a9edc87` → R34-6) and fix
   `CHANGELOG.md:17`'s R34-3 commit list to include `27879af`.
5. **F7** — either bring `docs/perf/r34_23_runs/*.json` into tier-2
   compliance or file the deviation in an open-items index with a trigger;
   consider re-keying the policy on size/role rather than the `_raw_*.log`
   filename glob.
6. **F4/F8** — drop (or pin) the line numbers in `sefer_alloc.rs`'s tripwire
   enumeration; add a dated correction note to `docs/ALLOC_BENCH.md:247`.

---

*This report is a local, untracked artifact by convention — it is not intended
to be committed.*
