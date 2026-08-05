# Sol-release + G1–G7 remediation wave (tasks #555–570) — independent READONLY review

**Date:** 2026-08-05
**Reviewer:** independent `@oh` session (readonly; the only working-tree change
is this file, which stays untracked per this project's established convention).
**Scope reviewed:** `c5db553..HEAD` (`HEAD` = `2a7f1e6e1e8f2834e02fefb26883b378c5371868`)
= **24 commits**, covering BOTH remediation waves:
the F1–F8 wave (tasks #547–552, 7 commits, all rebased) and the
Sol-F1–F7 + G1–G7 wave (tasks #555–570, 17 commits).
**Findings under re-verification:** Sol-F1…F7
(`docs/reviews/2026-08-05-sol-release-readonly-review.md`) and G1…G7 + G1-bonus
(`docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`) — 15 items.

**Method.** Every commit's `git show`/`git show --stat` read; all `src/`,
`Cargo.toml`, `.github/`, `scripts/` diffs read in full. Independent
re-derivations rather than re-assertions: the rebase's content-preservation
proved by comparing **tree SHAs** pre/post; the `internals` boundary probed by
building an **out-of-repo scratch crate** that depends on this crate by path
(no working-tree change here); the raw-log census re-run command-for-command;
the release.yml CHANGELOG guard simulated locally against the real file.
Executions run (all read-only w.r.t. the tree): `cargo fmt --all -- --check`,
`cargo test --features "production internals" --no-fail-fast`,
`cargo build --example sol_f1_dbg_carve_batch_negative_probe` both ways,
`cargo clippy --all-targets -- -D warnings`, `cargo check --all-features`,
`node scripts/verify-commit-prefixes.mjs` (both ranges),
`node scripts/verify-internals-negative-boundary.mjs`,
`node scripts/verify-gate-report.mjs`, and the two named `tests/` targets.

---

## 0. Verdict

**Twelve of the fifteen findings are genuinely closed. Two of the three P1
release blockers are NOT — and the wave surfaced, but did not file, two
compile failures that make `npm run check` impossible to pass at HEAD.**

The engineering quality inside each individual task is high and the zero-trust
discipline is visible: G1's rebase is provably content-preserving (tree SHAs
byte-identical), G3's census reproduces to the byte, Sol-F5/F6/F7's doc
rewrites are precise and honest about what is *not* guaranteed, Sol-F3's fix is
pinned by a real structural test, and Sol-F1 built a genuine compile-fail
oracle where the prior wave only had prose. Nothing I found is a soundness
defect in shipping code, and no new UB/UAF/data-race was introduced.

But the wave's own charter — release readiness — is not met:

- The tree **does not compile** in 2 of the 6 CI clippy configurations. Both
  failures were *discovered by this wave* (named verbatim in `dbb4016`'s
  commit body) and filed in **neither** open-items index, in direct violation
  of CLAUDE.md's "add it to the appropriate index in the same commit" rule.
- **Sol-F1 closed 3 of the 5 files** that carry crate-root-reachable
  `AllocCore::dbg_*` inherent methods. 17 such methods remain callable from a
  downstream crate under plain `production` with `internals` off — proven by
  compiling an external probe, not inferred.
- **Sol-F2's CHANGELOG consolidation did not move anything.** The
  `## [Unreleased]` header was deleted and the `## [0.3.0]` header left where
  it was, so ~4,970 lines (Rounds 34→12 and nine `BREAKING CHANGE` sections)
  now sit above the first `##` version header, belonging to no release
  section at all. The commit body's claim that content was "moved to the
  beginning of the 0.3.0 section" is contradicted by its own
  3-insertion/3-deletion diff.

| # | Sev | Finding | Anchor |
|---|-----|---------|--------|
| H1 | **P1** | `npm run check` step 1 and 2 of 6 CI clippy rows fail to compile at HEAD; both failures were discovered by this wave and indexed nowhere | `tests/r34_18_heap_core_stack_pressure_pin.rs:45`, `src/registry/heap_core.rs` |
| H2 | **P1** | Sol-F1 only partially closed — 17 `AllocCore::dbg_*` inherent methods still reachable without `internals` under plain `production` | `src/alloc_core/alloc_core_large_cache.rs`, `src/alloc_core/alloc_core_small_pool.rs` |
| H3 | **P1** | Sol-F2's consolidation is structurally wrong — ~4,970 lines of release notes now belong to no `##` version section; two contributor docs still point at the deleted `[Unreleased]` | `CHANGELOG.md:8`/`:4982`, `CONTRIBUTING.md:200`, `.github/PULL_REQUEST_TEMPLATE.md:33` |
| H4 | P2 | The G1 rebase orphaned 13 commit SHAs cited in 4 tracked files; the post-rebase closing commit added 6 more dangling citations | `CHANGELOG.md:45-61`, `docs/CORRECTNESS_OPEN_ITEMS.md:1253` |
| H5 | P2 | Sol-F5/F6 source docs say the residual is "tracked as a follow-up"; nothing tracks it — zero Sol-F entries exist in either index | `src/global/fallback.rs:400`, `src/alloc_core/remote_free_ring.rs:886` |
| H6 | P3 | `R34_MANIFEST.md` omits the 24 post-closing commits that `CHANGELOG.md` files under the same Round-34 heading — third recurrence of the same under-count | `docs/perf/round-manifests/R34_MANIFEST.md:6` |
| H7 | P3 | No `### BREAKING CHANGE` entry for the `internals` public-surface narrowing, against nine established precedents of exactly that shape | `CHANGELOG.md:2078`, `:2203` (precedents) |
| H8 | P4 | `dbb4016`'s `fix(perf):` prefix is inapt for a visibility/cfg change to non-perf-sensitive surface; its own predecessor `27879af` used `feat(api)` for the identical change class | `dbb4016` commit subject |

---

## 1. Sol-F1 — `AllocCore::dbg_*` behind `internals` — **PARTIALLY closed (H2)**

### What is correct

The gating in the three files the fix touched is correct and well-reasoned:

- `src/alloc_core/alloc_core_small_diag.rs:33` — the whole `impl AllocCore`
  block is `#[cfg(feature = "internals")]`. Every method in the file is a
  `dbg_*` hook; correct.
- `src/alloc_core/alloc_core_small_reclaim.rs` — split into an ungated block
  (`reclaim_offset`/`reclaim_offset_checked`, both `pub(crate) fn`, never
  externally reachable regardless of module visibility) and an
  `internals`-gated block for the five `dbg_*` hooks. The stated rationale
  ("`pub(crate)` items were never part of the semver boundary") is right.
- `src/alloc_core/alloc_core_core_diag.rs` — split into an ungated 3-method
  block and an `internals`-gated block for the other ~70.

**The three ungated methods are genuinely required.** Verified against
`src/global/sefer_alloc.rs`'s `AllocStats::stats()` (`:467-509`), which calls
unconditionally — no `internals` gate anywhere on the path:
`dbg_segments_reserved_total()` (`:496`), `dbg_segments_released_total()`
(`:497`), `dbg_foreign_or_unroutable_frees()` (`:507-508`). (`stats()` calls a
fourth, `dbg_decommit_count()` at `:475`, which lives in
`alloc_core_small_pool.rs` — a file the fix did not touch, consistent with it
staying ungated.)

**The oracle works, in both directions, as specified.** Run by me:

```
cargo build --features "alloc-core alloc-global alloc-decommit" \
  --example sol_f1_dbg_carve_batch_negative_probe
  → EXIT 101, error[E0599]: no method named `dbg_carve_batch` found for struct `AllocCore`

… same command + internals → EXIT 0
node scripts/verify-internals-negative-boundary.mjs → ALL GREEN (exit 0)
```

The script asserts the *specific* E0599 (not merely "some failure"), which is
the right design — a generic build failure would make the oracle vacuous.

### H2 [P1] — 17 `AllocCore::dbg_*` methods are still reachable without `internals`

The fix scoped itself to three files. `AllocCore`'s inherent `dbg_*` methods
live in **five**. An enumeration of `pub fn dbg_`/`pub unsafe fn dbg_` across
`src/alloc_core/` and of each one's own `#[cfg]`:

| File | `impl AllocCore` gate | `dbg_*` reachable under plain `production` |
|---|---|---|
| `alloc_core_core_diag.rs` | split, `internals` | 3 (all required by `stats()`) |
| `alloc_core_small_diag.rs` | `internals` | 0 |
| `alloc_core_small_reclaim.rs` | split, `internals` | 0 |
| **`alloc_core_large_cache.rs:60`** | **ungated** | **11** |
| **`alloc_core_small_pool.rs:118`** | **ungated** | **6** (1 required by `stats()`) |
| `alloc_core.rs:1108` | ungated | 3 more, gated on `numa-aware` |

`production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
"alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]`
(`Cargo.toml:407`) — so every method whose only gate is
`#[cfg(feature = "alloc-decommit")]` is on the public surface of a plain
`production` build.

**Proven, not inferred.** I built a scratch crate *outside* this repository
(`/tmp/solf1probe`, since removed) with
`sefer-alloc = { path = …, default-features = false, features = ["alloc-core",
"alloc-global", "alloc-decommit"] }` — i.e. `internals` off — whose `main`
takes each method as a function item:

```rust
let _a: fn(&AllocCore) -> usize            = AllocCore::dbg_pool_cap;
let _b: fn(&AllocCore) -> usize            = AllocCore::dbg_pooled_count;
let _c: fn(&mut AllocCore)                 = AllocCore::dbg_force_decay_tick;
let _d: fn(&mut AllocCore, u32, u64, usize)= AllocCore::dbg_set_decay_config;
let _e: fn(&mut AllocCore) -> usize        = AllocCore::dbg_drain_small_pool;
let _f: fn(&mut AllocCore, Option<usize>)  = AllocCore::dbg_set_large_cache_budget;
let _g: fn(&AllocCore) -> usize            = AllocCore::dbg_large_cache_used;
let _h: fn(&AllocCore) -> u64              = AllocCore::dbg_large_cache_hits;
let _i: fn() -> u64                        = AllocCore::dbg_decommit_count;
let _j: fn(&AllocCore, *mut u8) -> Option<u32>  = AllocCore::dbg_live_count_for;
let _k: fn(&AllocCore, *mut u8) -> Option<bool> = AllocCore::dbg_is_decommitted_for;
```

`cargo build` → **`Finished dev profile … EXIT 0`**. All eleven resolve. The
full residual list (17 under `production`, plus `dbg_large_cache_total_slots`,
`dbg_decay_config`, `dbg_large_cache_occupied_bits`,
`dbg_large_cache_slot_sizes`, `dbg_large_cache_budget`,
`dbg_large_cache_mode`), of which only `dbg_decommit_count` has a production
caller.

Three aggravating details:

1. **Four of them mutate allocator state** (`dbg_force_decay_tick`,
   `dbg_set_decay_config`, `dbg_drain_small_pool`,
   `dbg_set_large_cache_budget`). A downstream consumer cannot reach the global
   allocator's own `AllocCore` this way (that lives behind the registry, which
   *is* `internals`-gated), so this is an API/semver defect, not a proven
   soundness hole — exactly the framing the Sol review used for the original
   F1.
2. **Two are safe `pub fn`s taking a raw pointer that derive a segment base
   from it** — `dbg_live_count_for` and `dbg_is_decommitted_for`
   (`alloc_core_small_pool.rs:674`, `:755`) both call
   `os::segment_base_of_ptr(ptr)`. Unlike the R25-1 bug, they *do* validate
   (`self.table.contains_base_ro(base)` before any read), so I am **not**
   claiming a soundness hole — but this is precisely the shape CLAUDE.md's
   benchmark-hook rule says must not sit on a `production` build's safe public
   surface.
3. **The oracle cannot catch this.** `examples/sol_f1_dbg_carve_batch_negative_probe.rs`
   probes exactly one method and calls it "the representative probe method",
   while its own module doc states the general property "`AllocCore::dbg_*`
   diagnostic hooks do NOT resolve without `internals`" — which my probe
   disproves for 17 methods. This is the same "the guard cannot fail for the
   reason it was written" shape the F1-wave (task #547) fixed for the
   module-path half.

**And the documentation the original finding cited is still wrong.**
`Cargo.toml`'s `internals` doc (`:409-464`) was not touched by `dbb4016` (its
66-line `Cargo.toml` diff is entirely `required-features` additions plus the
new `[[example]]` entry). It still tells a `production` consumer it gets the
stable surface "— NOT three internal modules' worth of `dbg_*` hooks", and
"Only the MODULE PATH — and everything reachable solely through it (every
`dbg_*` hook …) — moves behind `internals`."

**Fix shape:** gate `alloc_core_large_cache.rs`'s and
`alloc_core_small_pool.rs`'s `dbg_*` methods the same way (a split `impl`
block, or `#[cfg(all(feature = "alloc-decommit", feature = "internals"))]`
per method), extend the probe example to name one method from *each* file
(so the oracle's coverage matches its claim), and correct the `Cargo.toml`
prose.

---

## 2. Sol-F2 — version/CHANGELOG reconciliation — **structurally NOT closed (H3)**

### What is correct

- `git tag -l` → `sefer-alloc-v0.1.0`, `sefer-alloc-v0.2.0`,
  `sefer-alloc-v0.2.1`. **No 0.3.0 tag** — the premise ("0.3.0 is unreleased")
  is verified, and the decision to drop the false `- 2026-07-04` date is right.
- `Cargo.toml:3` = `version = "0.3.0"`, consistent.
- The new `release.yml` guard's **logic is sound and its version plumbing is
  correct**. `steps.version.outputs.version` comes from `cargo pkgid` with the
  two-shape strip (`${PKGID##*#}` then `${AFTER_HASH##*@}`, `release.yml:143-145`)
  — the same value the pre-existing tag-match guard uses, so the two guards
  cannot disagree about what "the version" is. Simulated locally against the
  real file with `VERSION=0.3.0`: `MATCH_COUNT=1`, `(unreleased)` marker
  present → **the guard would correctly FAIL a publish today**.
- `9c5ea64` removing the fabricated `issues/564` URL is a genuine self-caught
  fabrication fix and is the right call.

### H3 [P1] — the consolidation moved nothing; ~4,970 lines now belong to no version section

`8edd8c8`'s entire diff is **3 insertions, 3 deletions**:

```diff
-## [Unreleased]
-
 ### Round 34 — 26 tasks …
@@ -4963,7 +4961,9 @@
-## [0.3.0] - 2026-07-04
+
+## [0.3.0] (unreleased)
+
```

That deletes the top-level header and renames the one 4,970 lines lower. It
does **not** move any content. Present state:

- `CHANGELOG.md:8` — `### Round 34 …` (a `###` with no parent `##`).
- Everything from `:8` to `:4981` — Rounds 34, 33, 31, 32, 30 … 12, plus
  **nine `### BREAKING CHANGE — …` sections** (`:2022`, `:2078`, `:2124`,
  `:2173`, `:2203`, `:2232`, `:2295`, `:2326`, `:2365`) — sits directly under
  the document preamble, inside no release section.
- `CHANGELOG.md:4982` — `## [0.3.0] (unreleased)`, whose body opens
  "0.3.0 is the first `0.3.x` release … It bundles four workstreams …": the
  *old* P0–P7 arc only. None of Round 34 is in it.
- The first `##` header a Keep-a-Changelog parser (or a human scrolling from
  the top) meets is 4,981 lines in.

The commit body asserts the opposite — "Moving all Round 34 +
review-remediation content (previously under [Unreleased]) to the beginning of
the 0.3.0 section" and "Preserving all existing 0.3.0 content … without loss"
— and offers "verified by diff: 3 insertions, 3 deletions, all header/blank-line
changes" as the *evidence*. That number is itself the disproof: moving ~4,970
lines cannot be a 3-line diff.

**Collateral, mechanically checkable:**

- `CONTRIBUTING.md:199-200` still instructs contributors to update
  "`CHANGELOG.md` (under `[Unreleased]`)".
- `.github/PULL_REQUEST_TEMPLATE.md:33` still has the checkbox
  "`CHANGELOG.md` updated under `[Unreleased]`".

Neither header exists any more. Both files were untouched by the wave.

**The new guard does not detect any of this.** It checks (a) exactly one
`^## \[VERSION\]` header exists and (b) that header lacks `(unreleased)`. A
release cut today by adding a date to `:4982` would pass both checks while
shipping every Round-34 release note outside the released section. That is
narrower than the original Sol-F2 recommendation, which explicitly asked for
"`[Unreleased]` before tag publish must be empty or explicitly allowed".

**Fix shape:** rename `CHANGELOG.md:8`'s deleted header back in as
`## [0.3.0] (unreleased)` at line 8 and merge the lower section's body under
it (deleting the duplicate header at `:4982`), then update `CONTRIBUTING.md`
and the PR template, and extend the guard with a "no `###` content above the
first `##`" structural assertion.

Cosmetic sibling: `:4980-4984` now carries a double blank line on both sides of
the renamed header — leftover from the same edit.

---

## 3. Sol-F3 — `Cargo.toml`'s panic/abort contract — **CLOSED, correct**

`Cargo.toml:164-179`'s new wording is accurate against both cited facts:

> Ordinary failure paths are no-op, not abort: checked branches (OOM, a foreign
> pointer, a layout we refuse to serve) return null / are a safe no-op, never
> panic. **This is NOT a blanket never-panic/never-abort guarantee for the whole
> entry-point surface** — see `src/global/sefer_alloc.rs`'s module doc for the
> five release-surviving invariant tripwires … and `src/registry/bootstrap.rs`'s
> `Registry::ensure_chunk` for the direct `std::process::abort()` on alloc-path
> chunk-materialisation OOM (the free path is fallible instead, via
> `slot_or_none`/`try_ensure_chunk`, R34-15).

It names both exceptions the review named, scopes the free-path improvement
correctly (it does *not* claim the alloc path became fallible), and does not
overclaim.

**Ran the pin:**
`cargo test --features "production internals" --test no_stale_doc_references
cargo_toml_alloc_global_panic_contract_is_accurate` → **1 passed**.

---

## 4. Sol-F4 — R34-11 release narrative — **CLOSED, correct**

The rewritten `CHANGELOG.md:23` bullet and `R34_MANIFEST.md:52-58`/`:179-190`
now separate the three claims explicitly, exactly as the review asked:
observable retention policy changed (final gap 3→1 in the measured sparse arm);
peak unchanged at 4; ≥3-segment persistence 72.5% (29/40); R32-8's stride
speedup preserved but *unchanged code*; no new throughput speedup measured
(the gate's own §4 says the catch-up body is never reached in that regime).
"Unbounded spin" is retracted, with the retracted phrase quoted at its point of
removal so the correction is traceable — the right treatment.

One incidental improvement I checked rather than assumed: the manifest's raw-log
filename was silently corrected from `_raw_r34_11_catch_up_decay_gate.log` to
`_raw_r34_11_catchup_decay_gate.log`. `ls docs/perf/` confirms the latter is the
file that exists. Correct.

---

## 5. Sol-F5 / Sol-F6 / Sol-F7 — residual-hardening docs — **CLOSED as written (but see H5)**

**Sol-F5** (`src/alloc_core/remote_free_ring.rs:848-888`, plus a cross-reference
on `drain` at `:1284-1292`) states both halves precisely: what IS guaranteed
(no loss of already-fully-processed prior elements) and what is NOT (no
exactly-once for the element in flight at panic time, because the loop order is
`reclaim → clear → advance`). It correctly labels the "no production reclaim
closure currently panics after mutating" claim as an observation about the code
as written, not a structural guarantee, and correctly scopes the two-phase
protocol out. It also names the practical boundary (the replay window is
unobservable through `GlobalAlloc`, which aborts on escaping unwind).

**Sol-F6** (`src/global/fallback.rs:368-402`) narrows the claim to "no permanent
`INITIALIZING` livelock" and states plainly that `Drop`-of-an-already-written
`HeapCore` is NOT guaranteed, why the window is currently unreachable
(`bind_thread_free` is a plain field assignment; the injection panic fires
before `HeapCore::new`), and that it is not structurally closed.

**Sol-F7** (`src/alloc_core/remote_free_ring.rs:229-240`) adds the one explicit
sentence naming the compound nature (memory model **plus** a bounded-staleness
scheduler assumption) and states that a "formally verified" claim omitting it is
incomplete; the same caveat was swept into `docs/perf/OPEN_ITEMS.md`'s two bare
"formally verified" sites and a new `S11` append-only section in
`R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md`. The sweep claim ("none in `src/`")
matches what I found.

All three are honest and do not overstate. The problem is what happens next:

### H5 [P2] — "tracked as a follow-up" tracks nothing

`src/global/fallback.rs:400-402` ends:

> Closing it structurally (making the guard aware of whether `HeapCore` was
> written, so an armed unwind after that point drops the stale value or poisons
> the slot instead of just rolling back) is **tracked as a follow-up**, not done
> here.

`git grep` over `docs/CORRECTNESS_OPEN_ITEMS.md` and `docs/perf/OPEN_ITEMS.md`
for `InitStateGuard`, `DrainHeadPublish`, `Sol-F`, and every task id `#563`,
`#565`–`#569` returns **zero hits** (the single `exactly-once` hit at
`CORRECTNESS_OPEN_ITEMS.md:2075` is an unrelated loom-model invariant list).
So the statement is false as written, and the Sol-F5 residual (which its own
doc calls "out of scope … see the review finding" — a *deliberately
uncommitted* file) has no durable record either.

This is the exact failure mode CLAUDE.md's indexing rule exists for: "the
in-session TaskList does not survive a session boundary, so a fresh session
inherits no memory of prior rounds' flagged-open items — these indexes do."
Both residuals — plus Sol-F1's (H2) — belong in
`docs/CORRECTNESS_OPEN_ITEMS.md` as `[T]` cards.

---

## 6. G1 (rebase) — **CLOSED, and verified NOT a false PASS**

```
node scripts/verify-commit-prefixes.mjs c5db553..HEAD
  → linted 24 commit(s); 3 WARNING(s) (direction 2); PASS; exit 0
```

The three warnings are benign and each is a known-legitimate shape: `eb66af6`
(ci.yml comment-only), `d17eec3` (Cargo.toml comment-only), `358be4e`
(`.gitignore`) — all verified by reading their diffs.

**Confirming it is not a false PASS.** The rebase rewrote six commits'
messages/SHAs. I compared **tree objects**, which cannot be faked by a message
edit:

| pre-rebase | post-rebase | tree SHA |
|---|---|---|
| `73817ee` | `5e75032` | `92e9228df608244f141c79916f05be5bcd1e2ba9` |
| `7faa377` | `55f8317` | `dd03d93652a5e5c321ed9d14a87d4d880de50606` |
| `e496d8b` | `80463d2` | `7e4448e87d04840b9d89d501d742c9080378a00b` |
| `5710a6e` | `358be4e` | `79d0ba32058cbe8eee374c714c0c9db4976409c4` |
| `d46c349` | `a7d7395` | `84b80526adfb9e316b45f9ce1651ff6fb3364752` |
| `4623dc3` | `4f45eee` | `dcb3e417dd52344a7f2b5feff39c9cc6b099d488` |

Byte-identical in all six. Additionally the whole-branch tip:
`git rev-parse 2e1ef90^{tree}` (pre-rebase tip) `= 45c45be^{tree}` (post-rebase)
`= 5c2c484b289193f198afb5c15e39ac1fdd15a5d4`. **Zero content moved** — the
CHANGELOG's own claim, independently confirmed rather than trusted.

I also read three mid-range commits' diffs in full alongside their (possibly
rewritten) messages — `5e75032` (4 lines in `OPEN_ITEMS.md`, message now
`docs(config):`), `a7d7395` (F4+F8, message now states sites 2–5's *doc prose*
is edited by this commit while their *source* is untouched — G5's fix, and it
is now accurate against the diff), `80463d2` (manifest span + CHANGELOG
attribution). All three messages describe their diffs correctly.

**Residual, correctly deferred:** the default range `@{u}..HEAD` (125 commits)
still **exits 1** with 2 FAILUREs — `43115cf` and `5c1142f`, the pre-existing
Round-34 pair. `scripts/check-all.mjs:197-199` runs the verifier on that default
range, so `npm run check` **step 14 is still red**. That is the documented
G1-bonus disposition (§9 below), not a new defect — but it is worth stating
plainly that "G1 closed" does not mean "`npm run check` is green".

---

## 7. G2 / G3 / G4 / G6 / G6-followup — **all CLOSED, independently verified**

**G2.** `CHANGELOG.md:36` now reads "classifying all 38 of this round's commits
(38 at the time this bullet was written; extended to the full 43-commit round
span by task #550/commit `e496d8b` — see …)". The append-a-parenthetical
treatment (rather than silently rewriting a historical bullet) is the right one
per this project's own convention. (The cited SHA is now dangling — see H4.)

**G3.** `R34_MANIFEST.md` §3.1 "Raw-log census". I re-ran both commands rather
than reading the numbers:

```
git log --name-only --format="COMMIT:%H" 40241b0..c5db553 -- 'docs/perf/_raw_*.log' …
  → 5 files: 145348 + 49999 + 91281 + 1076 + 1297 = 289,001 B   ✓ matches
find docs/perf -name '_raw_*.log' -type f | wc -l        → 256   ✓ matches
… -printf "%s\n" | awk '{s+=$1} END {print s}'    → 5,428,319 B  ✓ matches
find docs/perf -name '_raw_*.log' -size +200k | wc -l    → 0     ✓ matches the rule's own census
```

Every number reproduces exactly. The added note distinguishing
`docs/perf/r34_23_runs/` as a different artifact class is a good catch.

**G4.** `OPEN_ITEMS_ARCHIVE.md:1194-1225` gains a dated `⚠ CORRECTION`
blockquote beside (not instead of) the stale ~40× text — the append-only
treatment F8 established, applied consistently. It correctly states all three
sub-problems the finding named (wrong figure, dead README pointers, the card's
own back-link), gives the corrected `~1.8×`/`~2.1×` with both sources, and
explicitly notes the item's *decision* is unchanged — a genuine re-derivation,
not a number swap.

**G6 + G6-followup.** The requested check:

```
sed -n '87,2018p' docs/perf/OPEN_ITEMS.md | grep -oE "^[0-9]+\. " | sed 's/\. $//' \
  | sort -n | uniq -d     → (empty)
```

**Zero duplicates.** I widened the check to the whole file as a cross-check:
`1`, `2`, `3`, `34` appear twice, but each pair is cross-*section* (the file's
own "How to use" instruction list at `:16-26`; the "Recently resolved"
section's independent numbering at `:2019+`; the 14-finding cross-reference
list at `:2057+`). No collision inside the open-items tiers. Clean.

**G5** (commit-message wording) was folded into the rebase — verified in §6.
**G7** was correctly accepted as an existing pattern: `scripts/check-matrix.mjs:162`'s
note is self-contained enough to carry its meaning without the cited file, a real
precedent exists (`tests/regression_xthread_small_ring_miri.rs:3`), and the
alternative — committing review reports — would break a deliberate convention.
Accepting it was the right call.

---

## 8. G1-bonus — **CLOSED, documented honestly**

`docs/CORRECTNESS_OPEN_ITEMS.md:1195-1268` (item 21) is a model current-state
card: `Status: OPEN — not fixed`, the verifier output quoted verbatim over the
Round-34-scoped range, both SHAs independently re-confirmed via `git show
--stat` rather than on the script's word, the correct prefix argued from
CLAUDE.md's own taxonomy text (`docs(config):`), an explicit "why not fixed
here", a concrete reopening trigger, and a correction of the Round-34 closing
review's "correctly applied throughout" claim. It even states plainly that
`npm run check`'s verifier step is red whenever a range including them is
linted — which I independently confirmed (exit 1 on `@{u}..HEAD`).

One staleness point, folded into H4: the card was written *before* the G1
rebase and still describes that rebase as hypothetical ("when a rebase touching
this era of history happens for another reason (e.g. task #555/G1's own
`73817ee` rebase, if it is ever extended this far back…)") and refers to "the
still-open G1 disposition for `73817ee`". G1 is now closed and the rebase has
happened. Per CLAUDE.md's "OPEN_ITEMS indexes are CURRENT-STATE" rule the card
needs a one-line status refresh.

---

## 9. Wave-level checks

### H1 [P1] — the tree does not compile in 2 of 6 CI clippy configurations

Both reproduced by me at `HEAD`:

```
cargo clippy --all-targets -- -D warnings                     → EXIT 101
  error[E0432]: unresolved import `sefer_alloc::registry`
  error: could not compile `sefer-alloc` (test "r34_18_heap_core_stack_pressure_pin")

cargo check --all-features                                    → EXIT 101
  error[E0080]: evaluation panicked: assertion failed: size_of::<HeapCore>() <= 8192
  error: could not compile `sefer-alloc` (lib)
```

These are `PER_PR_ROWS` entries `clippy-default` (`features: ''`, no
`libOnly`, so `rowToCargoArgs` emits `--all-targets`) and `clippy-all-features`
— **CI clippy matrix entries 1 and 3**, and `npm run check`'s **first**
step. `npm run check` therefore cannot get past step 1 today.

- Cause 1: `tests/r34_18_heap_core_stack_pressure_pin.rs:45` does
  `use sefer_alloc::registry::HeapCore;` with **no `#![cfg(…)]` gate at all**,
  while R34-3 made `mod registry` `pub(crate)` without `internals`.
- Cause 2: `--all-features` unions in feature-gated `HeapCore` fields that push
  `size_of::<HeapCore>()` past R34-18's own 8,192-byte pin.

Both are genuinely **pre-existing** (introduced inside Round 34 by `27879af`
and `3281ebc`) — confirmed: `git log c5db553..HEAD --
tests/r34_18_heap_core_stack_pressure_pin.rs src/registry/heap_core.rs
src/lib.rs` is empty, so this wave touched none of the three files.

What makes this a finding *against this wave* is the disposition, not the
authorship. `dbb4016`'s own commit body names both, verbatim, as "Known
pre-existing, out-of-scope gaps discovered during verification" — and neither
appears anywhere in `docs/CORRECTNESS_OPEN_ITEMS.md` or
`docs/perf/OPEN_ITEMS.md` (`git grep` for `r34_18`, `stack_pressure`, `8192`
in the correctness index returns only R34-18's own historical "Recently
resolved" narrative at `:2022-2032`, which describes the pin as *working*).
CLAUDE.md is explicit: "When a gate report / commit / review newly flags an
open item, add it to the appropriate index in the same commit." That did not
happen — and this is the same repository whose `docs/CORRECTNESS_OPEN_ITEMS.md`
item 11 records `main` sitting red on clippy rows for up to 70 commits for
exactly this reason.

A third gap named in the same paragraph —
`examples/r31_3_large_cache_extended_narrow_{off,on}.rs` broken under their own
`required-features` (missing `large-cache-extended`) — is likewise unfiled, and
is *not* covered by item 11's closed bug list (that one was a different missing
feature, `alloc-decommit`, fixed by R33-1).

For a wave whose stated purpose is release readiness, "the tree does not build
in 2 of 6 enforced configurations" outranks all three original P1 blockers, and
it is not recorded anywhere a future session would find it.

### H4 [P2] — the rebase orphaned 13 cited SHAs; the closing commit added 6 more

Every pre-rebase SHA is now unreachable from `HEAD` (they survive only as
unreferenced objects in *this* clone's object store — a `git gc`, or any fresh
clone, loses them):

```
73817ee 7faa377 e496d8b 5710a6e d46c349 4623dc3 8e615a1
9296adb 15a1ef6 2f70081 a4dc38e 04ba0f8 2e1ef90
→ all: NOT-on-HEAD (git merge-base --is-ancestor fails for each)
```

`git grep` finds them in **four tracked files** (excluding `docs/checkpoints/`):

- `CHANGELOG.md` — 12 lines. `:36` (`e496d8b`), `:45-49` (the whole F2–F8
  bullet list), `:55` `9296adb`, `:57` `15a1ef6`, `:58` `2f70081`,
  `:59` `a4dc38e`, `:60` `04ba0f8`, `:61` `2e1ef90`.
- `docs/CORRECTNESS_OPEN_ITEMS.md:1253`, `:1256`, `:1261`, `:1267` — `73817ee`.
- `docs/perf/OPEN_ITEMS_ARCHIVE.md:1220`, `:1222` — `d46c349`, `73817ee`.
- `docs/perf/round-manifests/R34_MANIFEST.md:256` — `5710a6e`.

The closing commit `2a7f1e6` was authored **after** the rebase and still
introduced six of them (`9296adb`, `15a1ef6`, `2f70081`, `a4dc38e`, `04ba0f8`,
`2e1ef90` — confirmed by grepping its own diff). It handles two of the six
correctly ("Commit `9296adb` (post-rebase SHA `dbb4016`)"), which shows the
hazard was known — and then omits the same parenthetical for the other four
(Sol-F4/F5/F6/F7), leaving **11 of 13** citations with no live counterpart
anywhere.

This is the wave's own charter defect recurring inside the wave for the third
time in two waves: a later task invalidating an earlier artifact's cited
identifier, with no sweep. Cheapest fix: one pass replacing each dangling SHA
with its live successor (`73817ee`→`5e75032`, `7faa377`→`55f8317`,
`e496d8b`→`80463d2`, `5710a6e`→`358be4e`, `d46c349`→`a7d7395`,
`4623dc3`→`4f45eee`, `9296adb`→`dbb4016`, `15a1ef6`→`d17eec3`,
`2f70081`→`6190526`, `a4dc38e`→`ff496c6`, `04ba0f8`→`1f1015a`,
`2e1ef90`→`45c45be`), each verifiable by `git merge-base --is-ancestor`.

### H6 [P3] — R34_MANIFEST under-counts the round again, now by 24

`R34_MANIFEST.md:6` fixes its span at `40241b0..c5db553` (43 commits) and never
mentions either remediation wave. But `CHANGELOG.md` files both waves as
`####` subsections **under the same `### Round 34` heading** (`:40` "Post-closing
independent review remediation (2026-08-05, tasks #547-552)" and `:51`
"Release-readiness remediation (2026-08-05, tasks #555-570)"). So the artifact
CLAUDE.md requires to classify "every commit in the round" now omits 24 of
Round 34's 67 commits — the third instance of this exact under-count (38 → 43 →
67), in the file explicitly positioned as "the reference example future rounds
should match". Either extend the span, or state in the manifest that the
post-closing waves are deliberately out of scope and why.

Related, smaller: `CHANGELOG.md:8`'s heading still reads "Round 34 — 26 tasks"
while the section now covers 26 + 6 + 16 = 48 tasks.

### H7 [P3] — no BREAKING CHANGE entry for the `internals` narrowing

`CHANGELOG.md` has nine `### BREAKING CHANGE — …` sections, including three of
precisely this shape: "`AllocCore`/`HeapCore::dbg_push_to_ring` narrowed to
`unsafe fn`" (`:2078`), "public raw-memory test hooks narrowed to `unsafe fn`"
(`:2203`), "registry control-plane fields narrowed to `pub(crate)`" (`:2173`).
R34-3 removed three `pub mod` paths from the surface and `dbb4016` removed ~74
`pub` inherent methods from `AllocCore` under `production`; neither has such a
heading (grep for `BREAKING` + `internals` → nothing). 0.3.0 is unreleased and
0.2.1 is live, so nothing is *shipped* wrongly — but a release-readiness wave
that spent a P1 on CHANGELOG accuracy should not leave the round's largest
public-surface removal filed only as `[correctness fix, P1]`.

### H8 [P4] — `dbb4016`'s prefix

`fix(perf):` per R30-12 is for changes that "live in the SAME hot-path /
measurement-sensitive code the `perf(...)` family already tracks". `dbb4016`
changes `#[cfg]` gates on diagnostic hooks and `required-features` — not
perf-sensitive code — and is a public-API narrowing. Its own direct predecessor
for the identical change class, `27879af`, used `feat(api):`. The lint cannot
catch this (it only checks the docs-only-vs-code axis, and `dbb4016` does touch
`src/`, so it passes). Minor, and noted only so the taxonomy stays honest.

Smaller nit in the same register: `5e75032`'s `docs(config):` — R30-12 defines
that slot as "an existing tuning/config option was documented"; re-deriving an
`OPEN_ITEMS.md` verdict is a plain `docs:`. Both prefixes pass the lint; only
one matches the rule's text.

### Everything else I checked, and found clean

- **`cargo fmt --all -- --check`** → exit 0, clean.
- **`cargo test --features "production internals" --no-fail-fast`** → exit 0;
  `246` `test result: ok` lines, **`0`** `test result: FAILED` lines.
  (Note: this is genuinely green. My *first* attempt piped through `tail`, whose
  exit code would have masked a failure — re-run without the pipe to be sure.)
- **`node scripts/verify-gate-report.mjs`** → PASS, 104 reports scanned (64
  pre-existing WARN-only notices, unchanged by this wave).
- **`node scripts/verify-internals-negative-boundary.mjs`** → ALL GREEN.
- **Working tree** — `git status --short` shows only `.claude/` and seven
  untracked `docs/reviews/*.md` reports (three pre-dating this session, four
  from it). **No debris**: no scratch files, no stray logs, no accidentally
  committed `target/` or run artifacts. The two `.gz`/`.gitignore` changes from
  the F7 task are in-scope and correct.
- **No TODO/FIXME/placeholder** introduced anywhere in the 24 commits.
- **No new safe `pub fn` taking a raw pointer** was *added* by this wave —
  CLAUDE.md's benchmark-hook rule is engaged only in the residual sense noted
  in H2 point 2 (two pre-existing, validated hooks that H2's fix would move
  behind `internals` anyway).
- **`eb66af6`** ("5 clippy rows" → 6) is correct: `PER_PR_ROWS` has exactly six
  `kind: 'clippy'` entries, and the commit correctly leaves `ci.yml:189`'s
  explicitly-historical "at the time this job was added" snapshot alone.
- **`a6484ca`/`9c5ea64`** — `build:` prefix, outside the R30-12 five, correctly
  self-justified in both bodies.
- **`2a7f1e6`** committed exactly two files (`CHANGELOG.md` + one checkpoint);
  the four review reports and `.claude/` are correctly excluded per convention.

---

## 10. Status table

| Finding | Status |
|---|---|
| Sol-F1 (P1, `internals` hides `dbg_*`) | **PARTIAL** — 3 of 5 files gated; 17 methods still reachable (H2) |
| Sol-F2 (P1, version/CHANGELOG) | **PARTIAL** — date/tag/guard correct; consolidation structurally wrong (H3) |
| Sol-F3 (P1, panic/abort contract) | **CLOSED** — reworded + pinned, test run by me |
| Sol-F4 (P2, R34-11 narrative) | **CLOSED** |
| Sol-F5 (P2, `DrainHeadPublish`) | **CLOSED** as doc work; residual untracked (H5) |
| Sol-F6 (P2, `InitStateGuard`) | **CLOSED** as doc work; residual untracked (H5) |
| Sol-F7 (P3, cached-head assumption) | **CLOSED** |
| G1 (P2, prefix lint / rebase) | **CLOSED** — PASS verified non-false via tree SHAs |
| G2 (P3, "38 commits") | **CLOSED** |
| G3 (P3, raw-log census) | **CLOSED** — numbers reproduced exactly |
| G4 (P4, archive ~40×) | **CLOSED** |
| G5 (P4, "left untouched") | **CLOSED** via rebase |
| G6 + followup (P4, item numbers) | **CLOSED** — 0 duplicates |
| G7 (P4, untracked citation) | **ACCEPTED** — correctly, with precedent |
| G1-bonus (process) | **CLOSED** (filed) — card needs a post-rebase status refresh (H4) |

---

## 11. Recommended follow-ups (suggested priority)

1. **H1** — file both compile failures in `docs/CORRECTNESS_OPEN_ITEMS.md` as
   `[A]` cards *and* fix them (a `#![cfg(all(feature = "internals", …))]` on
   `tests/r34_18_heap_core_stack_pressure_pin.rs`; a decision on the 8 KiB pin
   under `--all-features`). Nothing else on this list matters until
   `npm run check` can reach step 2.
2. **H3** — move the `## [0.3.0] (unreleased)` header to line 8 and merge the
   duplicate section; update `CONTRIBUTING.md:200` and
   `.github/PULL_REQUEST_TEMPLATE.md:33`; add a structural assertion to the
   release guard.
3. **H2** — gate `alloc_core_large_cache.rs`'s and `alloc_core_small_pool.rs`'s
   `dbg_*` methods, extend the probe example to cover one method per file, and
   correct `Cargo.toml:409-464`'s prose.
4. **H4** — one sweep replacing 13 dangling SHAs with their live successors;
   refresh `CORRECTNESS_OPEN_ITEMS.md` item 21's card to post-rebase reality.
5. **H5** — file the Sol-F5/F6 residuals (and H2's) as `[T]` cards, so
   `fallback.rs`'s "tracked as a follow-up" becomes true.
6. **H6/H7/H8** — manifest span (or an explicit scope note), a `BREAKING
   CHANGE` heading for the `internals` narrowing, and the prefix nits.

---

*This report is a local, untracked artifact by convention — it is not intended
to be committed.*
