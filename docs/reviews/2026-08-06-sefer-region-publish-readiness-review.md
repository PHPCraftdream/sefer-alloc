# `sefer-region` — publish-readiness review (read-only)

**Date:** 2026-08-06
**Scope:** `crates/region` (crate `sefer-region`), ahead of tagging root `sefer-alloc` 0.3.0
**Mode:** read-only investigation. No file outside this report was modified; no version
bumped; no commit made; `cargo publish` was **not** run (only `cargo package`, which does
not contact the publish endpoint).
**Reviewed tree:** `main` @ `ba18071` (`git rev-parse HEAD`), working tree dirty only with
untracked `docs/reviews/*` + `.claude/` (unrelated to `crates/region`; `git status
--porcelain -- crates/region/` is empty — the crate itself is clean).

---

## Verdict: **GO-WITH-FIXES**

Two-part verdict, because the premise in the task brief ("no real drift → nothing to
publish") is **half right and half wrong**, and the half that is wrong is the decision-relevant
half:

- **Compiled code: exact parity with crates.io 0.1.0. Confirmed, not assumed.** I downloaded
  the actual published tarball (`sha256
  b017b215913f35e944b248a361221826ccbee6c0655b3dd9fb96bf28ef43f47c`, matching the checksum the
  crates.io API reports for 0.1.0) and diffed it against the working tree. `src/handle.rs` and
  `tests/smoke.rs` are **byte-identical**; the three files that differ (`src/lib.rs`,
  `src/region.rs`, `src/sync_region.rs`) differ **only in `///` / `//!` lines** — proven
  independently by a comment-stripped diff against the pre-drift commit, which reports
  `IDENTICAL` for all four source files. There is no logic, no whitespace-adjacent
  behavior change, no signature change, no feature change. **A version bump is not required by
  any code change.** On that axis alone the answer is "already at parity, nothing to do."

- **Published artifacts: three real, user-visible drifts — and all three are corrections of
  claims the maintainer's own commits classify as factually false.** The crates.io front page
  and the docs.rs rustdoc for 0.1.0 currently serve doc text that `2e88262` and `aab617a`
  explicitly identified as wrong, and the published crates.io category
  (`no-std::no-alloc`) is wrong for a crate that does `extern crate alloc`
  (`crates/region/src/lib.rs:47`). crates.io does not permit re-publishing 0.1.0, so the only
  route to correcting the live page is a **0.1.1 patch release**.

**Recommendation:** publish **0.1.1** as a docs/metadata-only patch, after applying the one
local fix in §6 (`crates/region/src/lib.rs:16` still carries the exact overclaim `aab617a`
corrected in the README but missed in the rustdoc). Publishing 0.1.1 *without* that one-line
fix would ship a corrected README next to a still-wrong rustdoc, which is worse than either
state alone.

If the maintainer decides the doc drift is not worth a release, **"leave at 0.1.0" is also a
defensible GO** — nothing here is a soundness or availability problem, and the crate builds,
tests, lints, docs and packages cleanly exactly as it stands. That is a judgment call and is
listed as open question Q1.

---

## 1. Metadata completeness — **PASS**

`crates/region/Cargo.toml` `[package]` (lines 1–14) has every field crates.io and docs.rs
care about:

| Field | Line | Value / note |
|---|---|---|
| `name` | `Cargo.toml:2` | `sefer-region` |
| `version` | `Cargo.toml:3` | `0.1.0` — matches crates.io exactly (verified via API) |
| `edition` | `Cargo.toml:4` | `2021` |
| `rust-version` | `Cargo.toml:5` | `1.88` — consistent with the root crate (`Cargo.toml:7`) and all 10 sibling members |
| `license` | `Cargo.toml:6` | `MIT OR Apache-2.0`; both texts present (`crates/region/LICENSE-MIT`, `crates/region/LICENSE-APACHE`) and both ship in the tarball |
| `description` | `Cargo.toml:7` | present, specific, states the standalone-use case |
| `readme` | `Cargo.toml:8` | `README.md`, exists, ships |
| `repository` / `homepage` / `documentation` | `Cargo.toml:9-11` | all three present; `homepage` correctly deep-links to `tree/main/crates/region` |
| `keywords` | `Cargo.toml:12` | 5 (the crates.io maximum), all relevant |
| `categories` | `Cargo.toml:13` | 3, all valid slugs — **but see §4, this line is the metadata drift** |

`crates/region/README.md` (128 lines) is genuinely good for a standalone consumer: badges
(`README.md:3-5`), a one-paragraph positioning statement (`README.md:7-15`), a "why not just
slotmap" section (`README.md:17-26`), a compile-shaped quick start (`README.md:36-58`), the
I1–I5 invariant list (`README.md:60-73`), a separate `SyncRegion` section with a
multi-op-transaction example (`README.md:75-100`), a feature-flag table (`README.md:102-112`),
a safety statement (`README.md:114-119`), and dual-license text (`README.md:121-128`). I
traced the `SyncRegion` example's arithmetic by hand (`README.md:87-99`: insert 42 → len 1,
then two inserts under one write guard → `assert_eq!(sr.len(), 3)`) and it is correct against
the real API in `crates/region/src/sync_region.rs`.

**No `crates/region/CHANGELOG.md` exists** (no member crate has one). This is **not** a
release blocker: `.github/workflows/release.yml:193` ff. — the "CHANGELOG must be
consolidated before publish" guard — short-circuits with an explicit skip for any crate whose
name is not `sefer-alloc` (`release.yml:224-234`, task L4/#617). A `sefer-region-v0.1.1` tag
would pass that guard by design.

## 2. Build / test / lint health, standalone — **ALL GREEN**

All run from repo root against `main` @ `ba18071`.

| Command | Result |
|---|---|
| `cargo test -p sefer-region --all-features` | **PASS** — 5/5 tests green (`region_insert_get_remove_roundtrip`, `region_stale_handle_returns_none`, `region_len_is_empty_track_live`, `sync_region_basic`, `sync_region_poison_recovery`), 0 failed, **0 doctests** (compliant with CLAUDE.md's no-doctests rule) |
| `cargo clippy -p sefer-region --all-features --all-targets -- -D warnings` | **PASS** — clean, zero warnings |
| `cargo doc -p sefer-region --all-features --no-deps` | **PASS** — zero rustdoc warnings, zero broken intra-doc links |
| `cargo build -p sefer-region --no-default-features` (extra: the `no_std + alloc` claim) | **PASS** — the `no_std` claim at `README.md:31` / `src/lib.rs:38-42` is real, not aspirational |
| `RUSTFLAGS="-W missing_docs" cargo build -p sefer-region --all-features` (extra) | **PASS** — **0 warnings**, i.e. 100% public-API doc coverage (see §6) |

The 5 tests are thin but not vacuous: `region_stale_handle_returns_none` is the I3/no-ABA
oracle and `sync_region_poison_recovery` exercises the documented
`PoisonError::into_inner` policy (`crates/region/src/sync_region.rs:56,64`). Coverage of I5
(drop-once) is documented (`src/region.rs:30-32`) but has no dedicated drop-counter test —
noted as Q3, not a blocker (the invariant is delegated wholly to `slotmap`, which owns the
storage).

## 3. Packaging — **CLEAN, nothing objectionable ships**

`cargo package -p sefer-region --list` → exactly **12 files**:

```
.cargo_vcs_info.json   Cargo.lock   Cargo.toml   Cargo.toml.orig
LICENSE-APACHE   LICENSE-MIT   README.md
src/handle.rs   src/lib.rs   src/region.rs   src/sync_region.rs
tests/smoke.rs
```

`cargo package -p sefer-region --allow-dirty` → **`Packaged 12 files, 38.8KiB (12.7KiB
compressed)`**, then `Verifying sefer-region v0.1.0` → **compiled clean**. No `--no-verify`
was needed.

Nothing that shouldn't ship is present. Specifically:

- **No leaked internal docs and no local absolute paths** — this crate is *not* affected by
  the root-crate packaging leak tracked as task L5/#618. That finding is about the root
  `sefer-alloc` tarball; `crates/region` has no `docs/`, no `benches/`, no scratch files in
  its directory, so the default `cargo package` inclusion rules already produce a minimal
  tarball. No `exclude`/`include` key is needed in `crates/region/Cargo.toml`.
- **No path dependencies** — `slotmap = { version = "1", default-features = false }`
  (`Cargo.toml:20`) is the only dependency, an external crates.io crate. Confirms the brief's
  claim: this crate is a DAG leaf and is **fully independent of the aligned-vmem / numa-shim
  ordering question** (tasks L2/#615, L3/#616). It can be published at any point in the
  release sequence, including first or last.
- **Root-manifest compatibility is already satisfied** — the root declares
  `sefer-region = { path = "crates/region", version = "0.1", default-features = false }`
  (`Cargo.toml:845`). `^0.1` is satisfied by the already-published 0.1.0 *and* by a
  hypothetical 0.1.1, so **no root manifest edit is required either way**. This is the exact
  opposite of L2/#615's `aligned-vmem ^0.2` situation, where the requirement is unresolvable
  against what is published.

## 4. Drift confirmation — **CONFIRMED doc/metadata-only, with three published-artifact drifts**

Only **three commits in the entire repository history** have ever touched `crates/region/`
(`git log --format='%h %ad %s' --date=short -- crates/region/`):

| Commit | Date | Nature |
|---|---|---|
| `3394677` | 2026-06-28 | `feat(workspace): extract sefer-region crate …` — the extraction; **this is the published content** |
| `2e88262` | 2026-07-12 | `docs: T7 — doc/metadata accuracy pass` — post-publish |
| `aab617a` | 2026-07-13 | `docs: R4-4 — sync architecture docs …` — post-publish |

The published tarball's `.cargo_vcs_info.json` records `sha1
d95ea7f069436414c3e210f5632055b75542f614` (`d95ea7f`, 2026-06-29, *"fix(vmem): MAP_ANON differs
across BSD (0x1000) vs Linux (0x20)"* — a commit that does not touch `crates/region`, so the
region content at that SHA is `3394677`'s). `git merge-base --is-ancestor d95ea7f HEAD` →
**yes**, so the publish point is reachable from `main`.

### 4a. Zero compiled-logic change — verified two independent ways

**Method 1 — comment-stripped diff of the git tree.** For each of the four source files,
`git show 3394677:crates/region/src/<f>` with all `///`, `//!`, `//` and blank lines removed,
diffed against the same transform of the working-tree file:

```
IDENTICAL (code, comments stripped): src/handle.rs
IDENTICAL (code, comments stripped): src/lib.rs
IDENTICAL (code, comments stripped): src/region.rs
IDENTICAL (code, comments stripped): src/sync_region.rs
tests/smoke.rs byte-identical to published state
```

**Method 2 — diff against the real published artifact.** Downloaded
`https://static.crates.io/crates/sefer-region/sefer-region-0.1.0.crate` (sha256
`b017b215913f35e9…f47c`, which matches the checksum the crates.io API reports for 0.1.0, so
this is provably the bytes users receive) and diffed the extracted tree against
`crates/region/`:

- `src/handle.rs` — **SAME** (byte-identical)
- `tests/smoke.rs` — **SAME** (byte-identical)
- `src/lib.rs` — differs, `//!` lines only (module doc, lines 3–9)
- `src/region.rs` — differs, `///` lines only (type doc lines 7–16; `iter`/`iter_mut` docs
  lines 120–133)
- `src/sync_region.rs` — differs, **one word** in a `///` line: "dense store" →
  "slot store" (`src/sync_region.rs:26`)
- `README.md` — differs (see 4c)
- `Cargo.toml.orig` vs `Cargo.toml` — differs, **one line** (see 4b)

**I found no non-doc-comment diff of any kind.** No whitespace-adjacent logic, no reordered
items, no attribute change, no `Cargo.lock`-driven dependency change. The brief's "both
commits are doc-only" claim is independently confirmed.

### 4b. Drift #1 — published crates.io **category is wrong**

Verified directly against the crates.io API, not inferred from the diff:

```
cats: ['data-structures', 'memory-management', 'no-std::no-alloc']
```

Local is `["data-structures", "memory-management", "no-std"]` (`crates/region/Cargo.toml:13`).
`2e88262`'s own commit body states the reason for the change: the crate does `extern crate
alloc` (`crates/region/src/lib.rs:47`) and `slotmap` allocates, so `no-std::no-alloc` is a
factually false category claim. **The live crates.io listing still carries it**, meaning the
crate is currently discoverable in — and mis-advertised to — the wrong `no-alloc` audience.
Category metadata is frozen per-version; only a new version corrects it.

### 4c. Drift #2 — published README carries two claims the maintainer classified as false

The crates.io front page for 0.1.0 currently renders:

1. *"values live in slotmap's dense, cache-friendly, always-compact backing store"* — false.
   `slotmap::SlotMap` leaves tombstone holes on removal; `crates/region/src/region.rs:120-124`
   and `docs/BENCHMARKS.md:52-55` both document the non-dense reality (~30% slower iteration
   than `DenseSlotMap`). Corrected locally at `crates/region/README.md:9-13`.
2. *"the compiler rejects **cross-region** handle confusion at the type level"* — false, and
   this is the one I'd weight highest. Branding is `PhantomData<fn() -> T>`
   (`crates/region/src/handle.rs:19`), i.e. by **value type**, not by `Region` *instance*. A
   `Handle<T>` minted by one `Region<T>` is accepted by a different `Region<T>` of the same
   type and — because `slotmap::DefaultKey`s are per-map index+generation pairs with no map
   identity — can **silently resolve to, or `remove`, an unrelated value** in that other
   instance. Corrected locally at `crates/region/README.md:22-26`, which now states the
   cross-*type* guarantee precisely and discloses the cross-instance hazard in full.

Point 2 is a *safety-adjacent misstatement on a live public page*: a reader of the published
README could reasonably build a design on a guarantee the crate does not provide. It is not a
soundness bug in the crate (no `unsafe`, no UB — a mis-keyed lookup returns a wrong-but-valid
`&T` or `None`), but it is a logic hazard the current published text actively invites and the
current local text explicitly warns about.

### 4d. Drift #3 — published docs.rs rustdoc carries the same stale claims

`src/lib.rs:3-9`, `src/region.rs:7-16`, `src/region.rs:120-133` and `src/sync_region.rs:26` on
docs.rs still show the pre-correction text. Same root cause, same remedy.

**Bottom line for §4:** there is genuinely *nothing to publish for behavioral reasons*, and
genuinely *something to publish for documentation-accuracy reasons*. Both statements are true
simultaneously, and the second is the one that argues for a 0.1.1.

## 5. Completeness scan — **CLEAN**

`grep -rnE 'TODO|FIXME|unimplemented!|todo!|XXX|HACK|allow\(dead_code\)|#\[allow'` over
`crates/region/src/` and `crates/region/tests/` → **zero matches**. No placeholders, no
half-wired features, no suppressed lints of any kind (not even a single `#[allow]`), no dead
code. The whole crate is 395 lines of `src` + 120 lines of tests.

`#![forbid(unsafe_code)]` at `crates/region/src/lib.rs:45` — the crate contributes zero
`unsafe`, matching the README's differentiator claim (`README.md:114-119`) and keeping it
outside the tier-1/tier-2 `unsafe`-seam inventory entirely.

## 6. Public API doc coverage — **EXCELLENT (100%), with one stale sentence**

`RUSTFLAGS="-W missing_docs" cargo build -p sefer-region --all-features` emits **0 warnings**.
Every public item is documented, and the docs go well past name-restating:

- `Region<T>` (`src/region.rs:5-41`) carries the full I1–I5 invariant list *and* an explicit
  "Generation saturation" section (`src/region.rs:33-41`) covering the classic
  generational-arena ABA caveat and stating it is `slotmap`'s responsibility, not
  hand-rolled here.
- Per-method complexity is stated rather than hand-waved (`src/region.rs:14-16`: lookup /
  insert / remove `O(1)`; iteration and `clear` linear; `reserve` may reallocate) — this was
  itself a correction landed by `aab617a`.
- `reserve` documents its panic condition (`src/region.rs:80-85`).
- `SyncRegion<T>` (`src/sync_region.rs:7-30`) documents the poisoning-recovery policy and
  *why* recovery rather than propagation is sound, and `len` explicitly notes it is a
  momentary snapshot under concurrency (`src/sync_region.rs:91-92`) — exactly the caveat a
  standalone user needs.
- `Handle<T>` (`src/handle.rs:7-14`) is accurate: it says *"a `Handle<A>` cannot be passed to
  a `Region<B>`"*, which is the correct cross-**type** framing.

### Finding — the one fix that makes this GO-WITH-FIXES rather than plain GO

**`crates/region/src/lib.rs:16` still reads:**

```
//! wraps it in `Handle<T>` — a `PhantomData<fn() -> T>`-branded key — so the
//! compiler rejects cross-region handle confusion at the type level.
```

This is the *identical overclaim* that `aab617a` diagnosed and fixed in
`crates/region/README.md:22-26`, but the same commit did **not** carry the fix into the crate
root rustdoc. `grep -n "cross-region" crates/region/src/ crates/region/README.md` returns
exactly one hit — `src/lib.rs:16` — confirming this is the last surviving instance.

Consequence if 0.1.1 ships as-is: the crates.io front page would say "cross-**type**, and note
the cross-instance hazard", while the docs.rs landing page one click away would still say
"cross-region", i.e. the two published surfaces would *contradict each other* on the crate's
headline safety property. Recommended fix is one sentence, mirroring the already-approved
README wording. **Severity: low (docs-only), priority: high-if-publishing (it is the specific
defect the release would exist to fix).**

### Secondary, optional

`src/lib.rs:6`, `src/region.rs:10` and `src/region.rs:126` all cite **`docs/BENCHMARKS.md`**
as the evidence for the SlotMap-vs-DenseSlotMap trade-off. That file lives at repo root
(`docs/BENCHMARKS.md`, 59 lines) and is **not in the package tarball** (see §3's 12-file
list), so a docs.rs reader sees a reference to a path they cannot resolve. Cheap fix if
0.1.1 happens: make it a repository URL. **Severity: cosmetic.**

## 7. Performance angle — **no outstanding perf work; one dormant, trigger-gated idea**

Searched `docs/perf/OPEN_ITEMS.md`, `docs/CORRECTNESS_OPEN_ITEMS.md` and
`docs/perf/OPEN_ITEMS_ARCHIVE.md` for `sefer-region|crates/region|slotmap|Handle<T>|SyncRegion`.
Three hits total, **none of them an open perf item against this crate**:

- `docs/perf/OPEN_ITEMS.md:1527-1530` — the batch-API consumer-scouting item explicitly reads
  `region` and concludes it is *"a `slotmap`-backed handle store (`slotmap::SlotMap` does its
  own internal storage management, never calls into `SeferAlloc::alloc_batch`)"*, i.e. the
  crate was examined as a candidate consumer and **ruled out by design**, not deferred.
- `docs/perf/OPEN_ITEMS.md:1592` — `crates/region/src/region.rs` listed only as a file *read*
  during that reconfirmation pass.
- `docs/CORRECTNESS_OPEN_ITEMS.md:1417` — `sefer-region` appears only in item 24's list of the
  5 crates that *do* have release.yml targets (a publish-DAG completeness item about the
  *other* crates; `sefer-region` is on the good side of it).

`grep` over `crates/region/src/` for perf-shaped notes (`TODO`, `FIXME`, `perf`, `slow`,
`optimi*`) → nothing actionable.

**The one noted-but-not-done idea** lives in `docs/BENCHMARKS.md:52-55`:

> `DenseSlotMap` remains a documented option for **iteration-bound** consumers (frequent full
> sweeps, rare lookups); the wrapper could expose it behind a type alias if such a consumer
> appears.

Assessment: this is a **trigger-gated, additive, non-breaking** idea, not a deferred
obligation. Its trigger ("such a consumer appears") has **not fired** — there is no
iteration-bound consumer of `Region<T>` anywhere in this tree, and the current backing choice
is empirically justified rather than assumed (`docs/BENCHMARKS.md:36-50`: SlotMap wins lookup
46.8 µs vs DenseSlotMap 54.4 µs and HashMap 244 µs, and wins churn 10.7 ns vs 30.3 ns, losing
only iteration by ~30%, the colder axis for the target read-mostly workloads). If ever
implemented it would be a *new type alias*, purely additive, requiring at most a minor bump —
so it does not gate 0.1.1 and does not argue for holding the release.

**Direct answer to the maintainer's question:** there is **no performance reason** to hold
`sefer-region` back, and no performance work owed. The crate is at exact behavioral parity
with crates.io 0.1.0 and its backing-container choice is benchmarked, documented, and
unchallenged. The *only* reason to cut a 0.1.1 is documentation/metadata accuracy — which is
a real reason (§4b–4d), but a discretionary one.

---

## Summary table

| # | Area | Result |
|---|---|---|
| 1 | Metadata completeness | **PASS** — all fields present; no per-crate CHANGELOG needed (guard skips members, `release.yml:224-234`) |
| 2 | `cargo test` / `clippy` / `doc` (+ `no_std`, `missing_docs`) | **PASS** — all green, 0 warnings, 0 doctests |
| 3 | `cargo package --list` / `--allow-dirty` | **PASS** — 12 files, 38.8 KiB, verify-build clean, no leaks, no path deps |
| 4 | Drift vs published 0.1.0 | **Code: ZERO drift** (verified against the real tarball). **Docs/metadata: 3 drifts** — category, README ×2 claims, rustdoc |
| 5 | Completeness scan | **PASS** — 0 TODO/FIXME/`todo!`/`unimplemented!`/`#[allow]`/dead code |
| 6 | Public API doc coverage | **PASS (100%)** — one stale sentence at `src/lib.rs:16`; one cosmetic dangling `docs/BENCHMARKS.md` ref |
| 7 | Performance | **Nothing owed.** One dormant, trigger-not-fired, additive idea (`docs/BENCHMARKS.md:52-55`) |

**Suggested 0.1.1 content, if the maintainer takes the GO-WITH-FIXES path** (all
documentation; zero code):
1. Fix `crates/region/src/lib.rs:16` — "cross-region" → cross-**type** + cross-instance
   disclosure, mirroring `README.md:22-26`. *(required — this is the point of the release)*
2. Bump `crates/region/Cargo.toml:3` to `0.1.1`. Root `Cargo.toml:845`'s `version = "0.1"`
   already accepts it; **no root manifest edit needed**.
3. Optional: turn the three `docs/BENCHMARKS.md` references into repository URLs.
4. Tag `sefer-region-v0.1.1` — `.github/workflows/release.yml:52` (tag pattern) and `:64`
   (`workflow_dispatch` option) already wire this crate; the CHANGELOG guard skips it by
   design.

Commit prefix under CLAUDE.md's R30-12 taxonomy would be **`docs(config)`** — an existing
documented surface corrected, zero code change, zero `production` composition impact.

---

## Open questions for the maintainer

- **Q1 — Is the doc drift worth a 0.1.1 at all?** This is the only real decision in this
  review, and it is a judgment call I should not make unilaterally. The case *for*: the live
  crates.io page asserts a handle-isolation guarantee the crate does not provide (§4c point
  2), and advertises a `no-std::no-alloc` category that is factually wrong (§4b) — both
  already diagnosed as false by your own commits, both only fixable by a new version. The
  case *against*: zero behavioral change, and a patch release consumes release bandwidth
  during a 0.3.0 push. My read is that §4c point 2 alone justifies it, but "stay at 0.1.0 and
  fold the doc corrections into whatever the next substantive `sefer-region` release is" is a
  legitimate GO.

- **Q2 — Publish ordering.** `sefer-region` has no path dependencies (`Cargo.toml:20`), so it
  is unconstrained by the aligned-vmem / numa-shim ordering question (L2/#615, L3/#616). If
  0.1.1 happens, it can go first (lowest-risk rehearsal of the release.yml member-crate path,
  which has never been exercised — there is no `sefer-region-v*` tag in the repo; `git tag -l`
  shows only `sefer-alloc-v0.1.0/0.2.0/0.2.1`, so 0.1.0 was published outside this workflow)
  or last. Do you want it used as the canary for `release.yml`'s member-crate branch?

- **Q3 — I5 (drop-once) test coverage.** `src/region.rs:30-32` documents drop-once as an
  upheld invariant, but no test asserts it (a `Drop`-counting newtype over
  insert/remove/`clear`/region-drop would be ~25 lines in `tests/smoke.rs`). The invariant is
  delegated wholly to `slotmap`, so this is arguably testing a dependency — I did **not**
  treat it as a publish blocker. Worth adding for the standalone-consumer story, or leave it?

- **Q4 — the un-tagged 0.1.0.** The published 0.1.0 has no git tag; it is recoverable only via
  the tarball's `.cargo_vcs_info.json` (`d95ea7f`). Should a retroactive `sefer-region-v0.1.0`
  tag be placed on `d95ea7f` for traceability before 0.3.0? (Read-only task — I did not create
  it. Note that tagging `d95ea7f` would *not* trigger a re-publish, since 0.1.0 already exists
  on crates.io and the publish step would fail closed; but confirm that against
  `release.yml`'s own guards before doing it.)
