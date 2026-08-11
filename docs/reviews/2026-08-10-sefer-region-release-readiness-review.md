# `sefer-region` — release-readiness review (post-F2 / post-F14)

**Date:** 2026-08-10
**Scope:** `crates/region/` (package `sefer-region`), read-only.
**Reviewed tree:** local `main` @ `90afbee` (working tree clean w.r.t. `crates/region/`;
`git diff --stat -- crates/region` empty). The two commits under primary review —
`9741388` (F2, task #802) and `c077fd2` (F14 remainder, task #803) — are committed
locally but **not yet pushed** (`origin/main` = `e7c13b2`), so no CI run has ever
executed against them.
**Nature:** research only. No file in `crates/region/`, no `Cargo.toml`, no version,
and no git state was modified. Every empirical claim below was reproduced on this
host; commands are quoted inline so each is re-runnable.

**Verdict up front: NOT READY.** One reproduced build break (F1) makes the crate's
own advertised `no_std` target fail to compile on `main`, and the release-gating CI
job that was built to catch exactly this class of problem was empirically confirmed
to pass anyway (F3). Details and the full finding list follow.

---

## What was verified green (so the negatives below are read in context)

| Check | Result |
|---|---|
| `cargo test -p sefer-region --all-features` | **PASS** — 41 tests across 9 binaries, 0 failed, 1 `#[ignore]`d (`captrack_probe`, documented reason) |
| `cargo test -p sefer-region --no-default-features` | **PASS** — 25 tests, 0 failed |
| `cargo clippy -p sefer-region --all-features --all-targets -- -D warnings` | **PASS** — clean |
| `grep -rn 'unsafe' crates/region/{src,tests,benches,examples}` | **zero `unsafe` tokens anywhere**, including tests/benches; `#![forbid(unsafe_code)]` (`src/lib.rs:66`) + `[lints.rust] unsafe_code = "forbid"` (`Cargo.toml:27-28`) both present. Nothing to report. |
| slotmap type leakage into the public API | **none found.** `Handle::key` / `Handle::region_id` are `pub(crate)` (`handle.rs:26-27`); `Iter::inner` / `IterMut::inner` are private (`region.rs:346`, `:389`); `Region::inner` private (`region.rs:87`). No `pub fn` in `region.rs` / `sync_region.rs` / `lib.rs` names a `slotmap::` path in argument or return position. The F14 newtype wrapping did what it claimed. |
| `region_id` check applied to **every** public method taking a `Handle<T>` | **complete.** `Region::{get, get_mut, contains, remove}` are the only four (`region.rs:208`, `:217`, `:226`, `:237`); `SyncRegion::{remove, contains, get_cloned}` delegate through them. Verified empirically — see the probe output in F5. |
| `Handle<T>` layout / niche | **as documented.** Probe printed `size=16 align=8 optsize=16`; pinned by `const _` asserts at `tests/handle_static_asserts.rs:67`, `:73`. |
| `Ord` / `Eq` consistency for `Handle<T>` | **consistent as written.** `cmp` returns `Equal` iff `key` and `region_id` both match, which is exactly `PartialEq::eq` (`handle.rs:51-54` vs `:76-83`). The *test* for it is weak — see F13. |
| `SyncRegion::into_inner` under poison | **correct.** `unwrap_or_else(PoisonError::into_inner)` (`sync_region.rs:104-108`), same policy as `read`/`write`; covered by `f14_api_ergonomics.rs:29-47`. `region_id` travels with the moved `Region`, verified by `:13-25`. |
| `IntoIterator for &mut Region<T>` aliasing | **sound.** Delegates to `slotmap::basic::ValuesMut`, which is `slotmap`'s own checked `&mut` iterator; the newtype adds no `unsafe` and no extra lifetime plumbing (`region.rs:329-336`, `:388-410`). |
| `Region`/`Handle` identity under `mem::swap` / move | **sound.** `region_id` lives *inside* `Region` next to the slotmap (`region.rs:86-88`), so identity travels with the data on any move/swap; there is no path that duplicates a `region_id` (no `Clone for Region`). |

---

# Findings

## F1 — HIGH — `main` is red: `Region`'s new `AtomicU64` breaks the crate's own advertised bare-metal `no_std` build

**Summary.** Commit `9741388` added `use core::sync::atomic::{AtomicU64, Ordering};` to
`Region`, which does not exist on any target without 64-bit atomics — including
`thumbv7em-none-eabi`, the exact target this repo's CI pins as sefer-region's `no_std`
proof.

**Citation.** `crates/region/src/region.rs:5` (`use core::sync::atomic::{AtomicU64, Ordering};`),
`crates/region/src/region.rs:7` (`static NEXT_REGION_ID: AtomicU64`),
against `.github/workflows/ci.yml:888`.

**Failure scenario (reproduced, not hypothetical).**

```
$ rustc --print cfg --target thumbv7em-none-eabi | grep atomic
target_has_atomic="16"
target_has_atomic="32"
target_has_atomic="8"
target_has_atomic="ptr"          # <- no "64"

$ cargo build -p sefer-region --no-default-features --target thumbv7em-none-eabi
error[E0432]: unresolved import `core::sync::atomic::AtomicU64`
 --> crates\region\src\region.rs:5:26
  |
5 | use core::sync::atomic::{AtomicU64, Ordering};
  |                          ^^^^^^^^^ no `AtomicU64` in `sync::atomic`
error: could not compile `sefer-region` (lib) due to 1 previous error
```

That command is **verbatim** the CI step at `.github/workflows/ci.yml:888`, whose own
inline comment says *"Verified locally: compiles clean."* — true when written (task
#667), false since `9741388`. It has not gone red yet only because `9741388`/`c077fd2`
are unpushed.

**Why this was missed.** `9741388`'s commit message records the no_std verification as
`cargo test -p sefer-region --no-default-features` — a **host x86_64** run, where
`AtomicU64` exists. `--no-default-features` alone does not exercise a target lacking
64-bit atomics; only the cross-build does, and it was not re-run.

**Aggravating factor — the repo already knew.** `.github/workflows/ci.yml:869-871`,
written for `tagged-index-stack` in task #772, states in so many words:
*"it can't use `thumbv7em-none-eabi` like the two crates above (that target has no
64-bit atomics, and this crate's head is a single `AtomicU64` behind a
`compile_error!` guard requiring `target_has_atomic = \"64\"`)"*. The identical
constraint was then introduced into `sefer-region` two rounds later without the
guard, the target swap, or the doc caveat.

**Blast radius beyond CI.** The failure invalidates live published claims:
- `crates/region/Cargo.toml:12` `keywords = [..., "no-std"]` and `:13`
  `categories = [..., "no-std"]`;
- `crates/region/Cargo.toml:7` description — *"no_std + alloc capable"*;
- `crates/region/src/lib.rs:58-63` §"`no_std` support";
- `crates/region/README.md:36` and `:121-125` (feature table).

All of these are now false for every Cortex-M target (`thumbv6m`, `thumbv7m`,
`thumbv7em`, `thumbv8m.base`), `riscv32imc`/`imac`, `msp430`, and `avr` — i.e. most
of the audience the `no-std` keyword is aimed at. Before `9741388` the crate genuinely
built on all of them.

**Recommended fix (described, not applied).** Three viable routes, in my order of
preference:
1. **Widen the counter to `AtomicUsize` + `NonZeroUsize`.** Available wherever
   `target_has_atomic = "ptr"` (i.e. everywhere this crate previously built), keeps
   `Handle<T>` at 16 bytes on 64-bit and *shrinks* it to 12 on 32-bit, and preserves
   the `Option<Handle<T>>` niche. Costs: the exhaustion bound becomes 2^32 `Region`
   constructions on a 32-bit host (see F11 — that bound must then be documented, not
   just asserted), and the `handle_static_asserts.rs` constants become
   pointer-width-dependent.
2. **`#[cfg(target_has_atomic = "64")]` with an `AtomicU32`/`NonZeroU32` fallback.**
   Keeps the 64-bit bound on 64-bit hosts, but makes `size_of::<Handle<T>>()`
   target-dependent (12 vs 16) — a documented-layout wrinkle the crate currently
   promises away, and two code paths to test instead of one.
3. **Follow the in-repo `tagged-index-stack` precedent:** keep `AtomicU64`, add a
   `compile_error!` guarded on `target_has_atomic = "64"` so the failure is a named
   reason rather than an E0432, swap the CI target from `thumbv7em-none-eabi` to
   `x86_64-unknown-none` (which has 64-bit atomics and no std, exactly as
   `ci.yml:874-880` already reasons for `tagged-index-stack`), and **narrow the
   `no-std` claims in README / lib.rs / Cargo.toml keywords+categories+description
   to say "no_std + alloc on targets with 64-bit atomics."** This is the smallest
   diff but it is a genuine, user-visible reduction in advertised portability, so it
   is a maintainer decision, not a mechanical fix.

**Severity rationale (HIGH).** It is a reproduced compile failure on `main` against a
target the project itself pins in CI, and it silently falsifies four separate
published portability claims including two crates.io metadata fields.

---

## F2 — HIGH — the manifest still says `version = "0.1.0"`, which is already published on crates.io; the release cannot proceed and the root crate's pin has not been prepared

**Summary.** `sefer-region` 0.1.0 is live on crates.io and un-yanked. `9741388` is a
`feat!` breaking change targeting 0.2.0, but the manifest was never bumped, so the
"republish" this review is gating is currently impossible.

**Citations.** `crates/region/Cargo.toml:3` (`version = "0.1.0"`);
`crates/region/README.md:45` (`sefer-region = "0.1"` in Quick start) and `:124`;
root `Cargo.toml:851` (`sefer-region = { path = "crates/region", version = "0.1", … }`);
root `src/lib.rs:379`, `:382` (re-exports `Handle`, `Region`, `SyncRegion`).

**Evidence that 0.1.0 is published.**

```
$ curl -s https://index.crates.io/se/fe/sefer-region
{"name":"sefer-region","vers":"0.1.0",…,"yanked":false,"rust_version":"1.88",
 "pubtime":"2026-06-29T17:31:34Z"}
```

**Failure scenario.** `cargo publish -p sefer-region` (or pushing tag
`sefer-region-v*`, which `.github/workflows/release.yml:68,83` turns into a publish)
is rejected by the registry with *"crate version `0.1.0` is already uploaded"*. If the
tag is `sefer-region-v0.2.0` while the manifest says `0.1.0`, the release workflow's
tag/version consistency step fails instead.

**The part that is easy to miss.** This is not only a sefer-region problem:
- Root `Cargo.toml:851` pins `version = "0.1"`. A `0.2.0` sefer-region does **not**
  satisfy that requirement, so the root crate stops building the moment the bump
  lands, and cannot be published until the pin moves to `"0.2"`.
- `sefer-alloc` **re-exports** `Handle`/`Region`/`SyncRegion` at its own crate root
  (`src/lib.rs:379,382`). F2's behavioral break therefore breaks the *published
  `sefer-alloc` API too* — any downstream user of `sefer_alloc::Handle` sees the same
  semantic change. `sefer-alloc`'s own version bump/CHANGELOG must account for it, and
  CI runs `cargo semver-checks` for `sefer-region` **only** (`ci.yml:90`), not for the
  re-exporting parent.

**Recommended fix.** Bump `crates/region/Cargo.toml` to `0.2.0`; in the *same* commit
update root `Cargo.toml:851` to `version = "0.2"`, `crates/region/README.md:45`/`:124`
to `"0.2"`, and decide+record `sefer-alloc`'s own required bump. Note this is already
tracked as tasks #785 / #801 (Stage E) — it is listed here because it is the single
largest release-readiness item and because the root-crate ripple (`Cargo.toml:851`,
`sefer-alloc`'s own semver) is not called out in either task's description.

**Severity rationale (HIGH).** It blocks the stated goal outright, and it has a
cross-crate consequence (`sefer-alloc`'s published API) that no current gate covers.

---

## F3 — MEDIUM — the `cargo-semver-checks` release gate passes on the breaking release, and is structurally incapable of catching it

**Summary.** CI's semver gate was verified to run **green** against a 0.1.0-vs-0.1.0
comparison of a breaking change. Its inline comment claims a strength it does not have.

**Citation.** `.github/workflows/ci.yml:85-90`, whose comment reads *"A real failure
here is a real gate -- no `|| true` swallowing it."*

**Evidence (run on this tree, after `cargo install cargo-semver-checks --locked`).**

```
$ cargo semver-checks check-release --package sefer-region
    Checking sefer-region v0.1.0 -> v0.1.0 (no change; assume minor)
     Checked [0.022s] 196 checks: 196 pass, 58 skip
     Summary no semver update required
```

**Failure scenario.** The release's defining change — a `Handle<T>` minted by one
`Region<T>` now resolves to `None` in another instead of aliasing that instance's value
— is *purely behavioral*. `cargo-semver-checks` reasons over rustdoc JSON: no public
item was removed, no signature narrowed, no `repr` removed relative to the **published**
0.1.0 (which had no `repr` attribute at all — verified against the extracted
`sefer-region-0.1.0.crate`). So the tool correctly reports "no semver update required"
for the most consequential breaking change this crate will ever ship. It additionally
does not object to current-version == baseline-version; it prints
`(no change; assume minor)` and proceeds.

**Recommended fix.** Two independent, cheap steps:
1. Reword the CI comment so it states what the gate actually covers (rustdoc-visible
   API shape) and what it does not (runtime semantics, and "version already published").
2. Add a genuine publish-collision guard alongside it — e.g. a step that queries
   `https://index.crates.io/se/fe/sefer-region` and fails if the manifest version is
   already present in the index. That one check would have caught F2 mechanically.

**Severity rationale (MEDIUM).** No user-facing defect, but it is an *actively
misleading* gate: it produced a green signal on precisely the release it was installed
to protect, and the surrounding comment invites the reader to trust it more than is
warranted.

---

## F4 — MEDIUM — `SyncRegion`'s `Debug` reports a poisoned-but-unlocked lock as `"<locked>"`, and its justifying comment misdescribes `std`

**Summary.** After any writer panic, `format!("{:?}", sync_region)` permanently shows
`SyncRegion { inner: "<locked>" }` even though the lock is free — hiding the data and
hiding the poison. This is the one place the crate does *not* apply its documented
recover-from-poison policy.

**Citation.** `crates/region/src/sync_region.rs:255-268`, specifically the comment at
`:257-259` (*"Pattern borrowed from std::sync::RwLock's own Debug impl: try_read()
first, and fall back to a '<locked>' placeholder if the lock is currently held by
another thread"*) and the catch-all `Err(_) =>` arm at `:263`.

**Failure scenario (reproduced).** Scratch binary (built outside the repo, path-dep on
`crates/region`), poisoning the lock in a spawned thread then printing both this crate's
type and a bare `std::sync::RwLock` for comparison:

```
(a) poisoned+unlocked Debug         = SyncRegion { inner: "<locked>" }
(a) std RwLock<i32> poisoned Debug  = RwLock { data: 1, poisoned: true, .. }
```

`RwLock::try_read` returns `Err(TryLockError::Poisoned(_))` on a poisoned lock
regardless of whether it is held, so `Err(_)` swallows the poisoned case into the
would-block case. `std`'s own `Debug` (which the comment claims to mirror) matches
`TryLockError::Poisoned` separately and prints the recovered data plus
`poisoned: true`. The comment is therefore factually wrong about the pattern it names.

Concrete user impact: the `SyncRegion` doc block at `sync_region.rs:27-46` promises
*"the `SyncRegion` remains usable"* after poison and every other method
(`read`/`write`/`into_inner`) honours that; a developer debugging the aftermath of a
writer panic — the exact moment `Debug` matters most — is instead shown a state that
says "someone else holds the lock", which is false, and is given no hint that poison
occurred at all.

**Coverage.** `tests/f14_api_ergonomics.rs:86-109` covers only the genuinely-held case
(a sleeping thread holding `write()`); no test covers the poisoned-but-free case, so a
fix has no regression guard today either.

**Recommended fix.** Match `TryLockError::Poisoned(e)` explicitly and render
`e.into_inner()` (the recovered `&Region<T>`) plus an explicit `poisoned: true` field,
reserving `"<locked>"` for `TryLockError::WouldBlock`; render the placeholder with
`format_args!` rather than a `&str` so it prints unquoted, as `std` does. Add a test
mirroring the probe above.

**Severity rationale (MEDIUM).** Diagnostic-only (no data loss, no unsoundness), but it
silently misreports state in the failure mode the type's own documentation spends a
whole section on, and it is guarded by a comment asserting the opposite of the truth.

---

## F5 — MEDIUM — I6 is only 3/4 covered: `get_mut`'s cross-region check and *all* of `SyncRegion`'s cross-instance behavior have no test

**Summary.** The release's headline invariant is tested for `get`/`remove`/`contains` on
`Region` and nowhere else. Deleting the `region_id` guard from `Region::get_mut` leaves
the entire test suite green, and `SyncRegion`'s compliance rests on a delegation claim
asserted only in a commit message.

**Citations.** Guard sites with no test: `crates/region/src/region.rs:217-219`
(`get_mut`); `crates/region/src/sync_region.rs:171` (`remove`), `:186` (`contains`),
`:241` (`get_cloned`). The only I6 test is
`crates/region/tests/smoke.rs:98-142`, which exercises `get`, `remove`, `contains` on
`Region` only. `region.rs:47-55`'s own I6 text names `get_mut` explicitly as one of the
four checked accessors.

**Failure scenario.** Two ways this bites:
1. *Silent regression.* Remove lines `region.rs:217-219`. `cargo test -p sefer-region
   --all-features` still passes 41/41. A `Handle<Foo>` from region A then hands out a
   `&mut Foo` into region B's value with the colliding key — the exact
   value-substitution bug F2 was created to eliminate, restored on the one accessor
   that hands out *mutable* access.
2. *Untested delegation claim.* `9741388`'s message states *"`SyncRegion<T>` needed no
   changes … so the `region_id` check applies automatically."* True today (verified
   below), but nothing pins it. Any future `SyncRegion` method that touches
   `self.inner`'s slotmap without going through `Region`'s accessors — e.g. a
   `get_many`/batch-read convenience API, which is exactly what design note #798
   proposes for 0.2 — would reintroduce the hole with a green suite.

**Verified current behavior is correct** (so this is a coverage defect, not a live bug).
Scratch probe output:

```
(d) a.get_mut(hb) is_none = true
(e) sb.get_cloned(x)=None  sb.contains(x)=false  sb.remove(x)=None  sa.get_cloned(y)=None
```

**Recommended fix.** Extend `smoke.rs::region_handle_from_different_instance_is_rejected`
with the `get_mut` arm in both directions, and add a
`sync_region_handle_from_different_instance_is_rejected` covering
`get_cloned`/`contains`/`remove` plus the guard-held forms (`sr.read().get(h_other)`,
`sr.write().get_mut(h_other)`). Also worth adding: a handle from a **dropped** `Region`
must be rejected by a subsequently-created one — the practically most important I6
scenario (use-after-free-shaped handle reuse), currently untested.

**Severity rationale (MEDIUM).** No defect ships today, but the crate's newest and
most-advertised invariant has a concrete, one-line-deletion counterfactual that the
whole suite fails to notice, on the accessor with the widest consequence.

---

## F6 — MEDIUM — every published performance number predates the F2 change, and no benchmark isolates the cost F2 added

**Summary.** `README.md`'s perf tables and its "Wrapper overhead: measured, not
assumed" section were measured before `9741388` added a `region_id` comparison to every
lookup and an atomic RMW to every `Region::new()`. Neither was re-measured, and no
workload targets either.

**Citations.** `crates/region/README.md:127-155` (the main table), `:201-221`
(§"Wrapper overhead: measured, not assumed", which concludes *"**No measurable wrapper
overhead was found.**"*); `crates/region/benches/region_bench.rs` (no workload for a
rejected cross-region handle; `Region::new()`'s new atomic is inside `st/insert`'s
`bench_batched` fixture at `:52` and is never exercised concurrently).

**Evidence of staleness.** `git show 9741388 -- crates/region/README.md` touches only
the §"Why?" prose; `git log --oneline -- crates/region/README.md` shows the perf tables
unchanged since `d9094ea`, i.e. one commit *before* F2 landed.

**Failure scenario.** A reader comparing `st/get_hit` 5.07 ns against `raw/get_hit`
4.76 ns concludes the typed membrane costs nothing. Both numbers were produced by a
`get` that did *not* contain `if handle.region_id != self.region_id { return None; }`.
The A/B is no longer an A/B of the shipping code, and the section's whole premise —
*"Investigated so this stays a checked fact rather than an assumption"* — is exactly
what has silently lapsed. Additionally, `Region::new()` is now a contended global
atomic RMW (`region.rs:95`, `:130`): a workload creating many short-lived `Region`s
across threads pays a shared-cache-line cost that did not exist in 0.1.0 and that no
benchmark measures.

**Recommended fix.** Re-run `cargo bench -p sefer-region --bench region_bench` on the
post-F2 tree and refresh both the main table and the wrapper-overhead A/B, citing the
new measurement SHA. Add two workloads while there: `st/get_wrong_region` (a handle
minted by a second `Region`, so the cost of the *rejecting* path is visible next to
`st/get_hit`/`st/get_stale`), and a multi-threaded `Region::new()` workload so the new
global-counter contention has a number rather than an assumption. Note: this review
deliberately did **not** run the bench itself — `bench-scale-tool`'s `save_manifest`
writes the workspace-level `bench-iters.txt`, which would mutate a tracked file in a
workspace other agents are concurrently editing.

**Severity rationale (MEDIUM).** No correctness impact, but the crate's crates.io
landing page publishes measured numbers as current for code that no longer exists, and
the section making that claim explicitly frames itself as a guard against unchecked
assumptions.

---

## F7 — MEDIUM — README's Invariants list stops at I5; I6 — the reason this release is breaking — is missing from the crate's landing page (and from the miri-gated and fuzz oracles)

**Summary.** `lib.rs` and `region.rs` both gained an I6 in `9741388`. The README, the
root workspace's miri-covered invariant test, and the libFuzzer target all still
enumerate I1–I5 only.

**Citations.**
- `crates/region/README.md:64-81` — bullets I1…I5, no I6 (compare
  `crates/region/src/lib.rs:43-49` and `crates/region/src/region.rs:47-55`, which both
  have it).
- `tests/region_invariants.rs:3-4` (root) — *"These encode invariants I1–I5"*; this is
  the file CI runs under miri (`.github/workflows/ci.yml:980-981`), so **I6 is never
  exercised under miri**.
- `fuzz/fuzz_targets/region_ops.rs:4-15` — enumerates I1–I5 only, so I6 is outside the
  fuzz oracle too.

**Failure scenario.** A prospective user evaluating the crate on crates.io reads the
Invariants section as the crate's contract. They will not find the guarantee that
motivated a breaking major bump, and — because `README.md:19-30`'s §"Why?" describes
`region_id` as a *mechanism* rather than an *invariant* — they have no numbered,
citable statement of it to depend on. Meanwhile the two automated oracles that would
catch an I6 regression under adversarial conditions (miri, libFuzzer) do not model it
at all.

**Recommended fix.** Add the I6 bullet to `README.md`'s list, mirroring
`lib.rs:43-49`'s wording. Extend `tests/region_invariants.rs` with an I6 case (it uses
`sefer_alloc::Region`, the re-export, so this simultaneously proves the re-export path
honours it under miri) and add a two-`Region` arm to `fuzz/fuzz_targets/region_ops.rs`'s
op stream.

**Severity rationale (MEDIUM).** The landing-page contract under-states the release's
one substantive guarantee, and the crate's two strongest automated oracles have a
documented hole exactly where the newest code is.

---

## F8 — MEDIUM — `region.rs`'s "Generation saturation" advice is stale post-F2 and now names the wrong hazard

**Summary.** The closing paragraph of `Region`'s struct doc tells the reader to guard
against "cross-region handle reuse" — a hazard the same commit eliminated — and does
not name the hazard the paragraph is actually about.

**Citation.** `crates/region/src/region.rs:82-84`:

> *"Applications that need a stronger guarantee (e.g. to reuse handles without ever
> risking alias) must add their own wrapper layer that tracks generation wrap or
> otherwise avoids **cross-region handle reuse**."*

**Failure scenario.** A reader who has just read I6 four lines earlier ("a mismatch is
treated exactly like a stale handle") reaches this sentence and concludes either (a)
that I6 is unreliable and they must hand-roll their own cross-region guard — wasted
work on a solved problem — or (b) that the crate's docs contradict themselves. The real
residual risk the §"Generation saturation" section is describing is **same-region**
generation wrap after ~2^31 occupy/free cycles of one slot; that is what a wrapper layer
would actually need to track, and the sentence never says so.

This is the precise stale-doc class that `9741388`'s own zero-trust review caught in
three other places (`lib.rs`, `region.rs`'s Invariants block, `README.md` §"Why?") —
this fourth instance, further down the same file, was not caught.

**Recommended fix.** Replace "cross-region handle reuse" with an explicit statement of
the same-region wrap hazard, e.g. *"…must add their own wrapper layer that tracks
generation wrap on a hot slot; cross-**instance** confusion is already handled by I6 and
needs no wrapper."*

**Severity rationale (MEDIUM).** A live rustdoc paragraph, on the crate's central type,
that directs the reader to defend against a threat the crate removed while omitting the
one that remains.

---

## F9 — MEDIUM — the root `sefer-alloc` crate's front-page docs still describe the pre-F2 `Handle<T>`, and still call slotmap "audited" after `sefer-region` explicitly retracted that claim

**Summary.** Two live-doc inaccuracies on the crate that re-exports `Handle`/`Region`.

**Citations.**
- `src/lib.rs:14-16` — *"`[Handle<T>]` is a **newtype over** a `DefaultKey` plus
  `PhantomData<fn() -> T>`"*. False since `9741388`: `Handle<T>` is a `#[repr(C)]`
  three-field struct (`crates/region/src/handle.rs:22-29`) of 16 bytes, not a newtype
  of 8. `src/lib.rs:379,382` re-exports exactly this type, so this is the first
  description a `sefer-alloc` user reads of it.
- `src/lib.rs:17` — *"`slotmap`'s **audited** `unsafe` owns the generational layout"*,
  duplicated at `src/lib.rs:155`, `README.md:359`, `README.md:600`,
  `docs/ARCHITECTURE.md:71`, `docs/ARCHITECTURE.md:146`. This directly contradicts
  `crates/region/README.md:250-256`, which task #795/F23 rewrote to say *"No
  version-scoped audit record for `slotmap` is tracked by this project"*. The F23
  checkpoint records the fix as landing in "README.md ×2, src/lib.rs"; the grep above
  shows six surviving instances, so that task is not actually closed.

**Failure scenario.** A safety-conscious evaluator reads `sefer-alloc`'s README/rustdoc,
sees "audited", and treats the dependency as having a review record that does not exist
— while the sub-crate's own README, one directory down, tells them the opposite. For the
`Handle` description, anyone sizing a handle array from the root crate's doc budgets 8
bytes per handle and is off by 2×.

**Recommended fix.** Update `src/lib.rs:14-16` to describe the current three-field shape
and cross-reference I6; sweep the six "audited" sites to the qualified wording already
adopted in `crates/region/README.md:250-256` (or, per this repo's non-retroactive
convention, mark the `docs/*.md` historical ones STALE and fix only the live
`src/lib.rs` / `README.md` ones).

**Severity rationale (MEDIUM).** Two independent false claims on a published crate's
primary documentation, one of which a prior task believed it had already closed.

---

## F10 — MEDIUM — `CHANGELOG.md` has no entry for `#802` or `#803`

**Summary.** The two commits under review are absent from the changelog. Its most
recent `sefer-region` section (line ~303, tasks #769-770) predates them, and line 127
still states the *pre-F2* behavior in the present tense.

**Citations.** `CHANGELOG.md` — last `sefer-region` heading at `:303`
(*"round-closing-review follow-ups (2026-08-08/09, tasks #769-770)"*); `:127` still
reads *"a `Handle<T>` from one `Region<T>` is silently accepted by an unrelated
`Region<T>` of the same type"*; `:177` still describes `#[repr(transparent)]` and an
"8 bytes" layout guarantee as current.

**Failure scenario.** A user upgrading `sefer-alloc` or `sefer-region` reads the
changelog to find out what broke. They learn nothing about the identity model change,
nothing about `Handle<T>` doubling in size, and — from `:127` and `:177` — are actively
told the *old* behavior and the *old* layout. Under this project's own convention
(CLAUDE.md, "Post-work: update CHANGELOG.md with the round"), the round is not complete
without it.

**Recommended fix.** Add a `sefer-region` round section covering `9741388` + `c077fd2`,
tagged per the R30-12 taxonomy (`feat!` / `feat`), stating: the identity-model break and
its user-visible symptom, the 8→16 byte `Handle<T>` change, the `repr(transparent)` →
`repr(C)` change, the new public items (`Iter`, `IterMut`, `SyncRegion::into_inner`,
`Debug`, `IntoIterator`, `Ord`), and the target version. Lines `:127`/`:177` are
historical narrative for their own rounds and should be left as-is per the
non-retroactive convention — but only once a *current* section exists to supersede them.

**Severity rationale (MEDIUM).** Pure documentation, but it is the one artifact a
downstream upgrader consults for a breaking change, and today it tells them the
opposite of the truth.

---

## F11 — LOW — `Region::new`/`with_capacity`/`Default` can panic, undocumented; and I6 is silently bounded at 2^64 `Region` constructions while I2/I3's 2^31 bound is documented meticulously

**Citations.** `crates/region/src/region.rs:95-96` and `:130-131`
(`NonZeroU64::new(NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed)).expect("region_id overflow")`);
`crates/region/src/lib.rs:43-49` (I6 stated unconditionally: *"Two `Region<T>`s can
**never** alias each other's values"*).

**Failure scenario.** Two distinct, both practically unreachable but asymmetrically
documented:
1. The `2^64`-th `Region::new()` in a process observes `0` from `fetch_add` and panics
   with `"region_id overflow"`. Neither `new`, `with_capacity`, nor the `Default` impl
   carries a `# Panics` section (contrast `with_capacity`'s meticulous one at `:103-114`
   for capacity), so `Region::default()` is a documented-infallible constructor that can
   in fact abort the program.
2. Worse for the contract: the counter *keeps incrementing past the panic*, so
   constructions `2^64 + 1, +2, …` receive `region_id` 1, 2, 3 — colliding with the
   earliest `Region`s in the process. Any handle held across that boundary re-acquires
   the pre-F2 aliasing hazard. I6's "never" is therefore a 2^64 bound, not an absolute,
   and the crate says nothing about it — while spending three paragraphs
   (`region.rs:57-84`) on I2/I3's analogous 2^31 bound.

Both are far beyond any real workload (2^64 constructions at 1 ns each ≈ 584 years), so
this is a documentation-honesty item, not a hazard.

**Recommended fix.** Add a one-line `# Panics` to `new`/`with_capacity`, and one
sentence to I6 stating the 2^64-constructions bound in the same honest register the
crate already uses for I2/I3. If F1 is fixed via route 1 (`AtomicUsize`), this becomes
2^32 on 32-bit hosts — *reachable* for a long-lived server minting a `Region` per
request — and then documenting it is no longer optional.

**Severity rationale (LOW).** Unreachable in practice; flagged because the crate's own
standard for disclosing wrap bounds is visibly higher everywhere else.

---

## F12 — LOW — `f14_api_ergonomics.rs:149` pins `slotmap`'s unspecified iteration order — the exact false-red class task #694 fixed elsewhere

**Citation.** `crates/region/tests/f14_api_ergonomics.rs:149`:
`assert_eq!(region.iter().cloned().collect::<Vec<_>>(), vec![2, 4, 6]);`

**Failure scenario.** `Region::iter`'s own doc (`region.rs:243-244`) says *"The order is
unspecified and changes as elements are removed."* A `slotmap` 1.x patch that changes
iteration order — entirely within its rights — turns this test red for a reason that is
not a regression in `sefer-region`. This is precisely the hazard task #694 spent a round
removing from `clear_partial_under_panic.rs`; `c077fd2` reintroduced one instance of it
in a new file. (`coverage_gaps.rs:339-343` gets this right — it uses
`values.contains(&&20)` etc.)

**Recommended fix.** Compare as a multiset: sort both sides, or assert
`values.contains(…)` per element plus `len() == 3`, matching `coverage_gaps.rs`'s
established pattern in the same crate.

**Severity rationale (LOW).** No shipping defect; a latent false-red on a dependency
bump, in a class the project has already declared out of bounds once.

---

## F13 — LOW — `handle_ord_handles_from_different_regions` asserts almost nothing; a real `Ord`/`Eq` inconsistency would go unnoticed

**Citation.** `crates/region/tests/f14_api_ergonomics.rs:186-202` — the only cross-region
ordering test. It asserts `assert_ne!(h_a, h_b)`, `partial_cmp(...).is_some()`, and
"`cmp()` doesn't panic". It never asserts *what* the ordering is.

**Failure scenario (concrete counterfactual).** Change `handle.rs:78-81` so `cmp`
returns `Ordering::Equal` when the `key`s match, dropping the `region_id` tiebreak:

- `handle_ord_handles_from_different_regions` — still passes (`is_some()` holds,
  `assert_ne!` uses `PartialEq`, which is untouched).
- `handle_ord_consistent_within_same_region` (`:169-183`) — still passes; its
  `handles.dedup()` uses `PartialEq`, not `Ord`, and all three handles are from one
  region anyway.
- `handle_ord_total_order` (`:216-240`) — still passes; single region only.

So the whole suite stays green with `cmp(a,b) == Equal` while `a == b` is `false` — a
straight violation of `Ord`'s documented requirement that it agree with `Eq`, and a real
bug for any user putting handles from two regions into a `BTreeMap`/`BTreeSet` (the
second insert would silently overwrite the first). The probe confirms the *current* code
is right — `ha=Handle { key: DefaultKey(1v1), region_id: 2 }`,
`hb=Handle { key: DefaultKey(1v1), region_id: 3 }`, `ha < hb == true` — but nothing
pins it.

**Recommended fix.** Assert the documented order explicitly for the interesting case
(same `key`, different `region_id`): both `h_a < h_b || h_b < h_a`, `h_a.cmp(&h_b) !=
Equal`, and antisymmetry `h_a.cmp(&h_b) == h_b.cmp(&h_a).reverse()`. Add a
`BTreeSet<Handle<T>>` case with two colliding-key cross-region handles asserting
`len() == 2` — the direct analogue of the existing `HashSet` case at
`smoke.rs:179-191`.

**Severity rationale (LOW).** Code is correct today; the test that guards it cannot
fail for the failure mode it exists to guard.

---

## F14 — LOW — the README's `SyncRegion` example produces two compiler warnings when copied verbatim, and the mirror test silently diverges to hide it

**Citations.** `crates/region/README.md:106-110` (`w.insert(1u32);` / `w.insert(2u32);`)
against `crates/region/src/region.rs:199` (`#[must_use]` on `Region::insert`);
`crates/region/tests/readme_examples.rs:46-47`, which writes `let _ = w.insert(1u32);`
and notes only that "sr2 from the original example was unused".

**Failure scenario (reproduced).** Copying the README block into a fresh crate:

```
warning: unused return value of `Region::<T>::insert` that must be used
  --> src\main.rs:10:9   |  w.insert(1u32);
warning: unused return value of `Region::<T>::insert` that must be used
  --> src\main.rs:11:9   |  w.insert(2u32);
```

A new user's first contact with the crate is two warnings on the documented
quick-start. `readme_examples.rs` exists precisely to catch README/API drift, but by
adding `let _ =` it diverges from the text it claims to mirror, so it can never catch
this.

**Recommended fix.** Change the README to `let _ = w.insert(1u32);` (or bind the
handles, which is more instructive), and make `readme_examples.rs` byte-identical to the
snippet so its drift-detection is real.

**Severity rationale (LOW).** Cosmetic for the user; notable because the dedicated
anti-drift test was modified in a way that defeats its own purpose.

---

## F15 — LOW — `#[must_use]` asymmetry between `Region::insert` and `SyncRegion::insert`

**Citations.** `crates/region/src/region.rs:199` (`#[must_use]`) vs
`crates/region/src/sync_region.rs:161` (none). `must_use` is present on
`SyncRegion::{contains, len, is_empty, with_capacity, new, into_inner}` — `insert` is
the sole omission.

**Failure scenario (reproduced).** `sr3.insert(5);` on a `SyncRegion<u32>` compiles
silently with no diagnostic, while the identical mistake through a write guard warns.
Dropping the returned handle strands the value identically in both cases — it is
unreachable until `clear()` or drop — so the guidance should be identical. Note the
asymmetry is *new*: 0.1.0 (verified against the extracted `.crate`) had `#[must_use]` on
neither; task-#669-era work added it to `Region::insert` only.

**Recommended fix.** Add `#[must_use]` to `SyncRegion::insert`. This is not a semver
break (adding `must_use` only adds a lint), so it is free to do before or after 0.2.0 —
but doing it now keeps the F14 doc-symmetry pass (task #689) honest.

**Severity rationale (LOW).** A missing lint, not a defect; cited because the crate has
an explicit `Region`↔`SyncRegion` symmetry convention that this violates.

---

## F16 — LOW — `Region::with_capacity`'s `checked_add(1)` guard is unreachable on every supported target, and the comment justifying it states a false identity

**Citations.** `crates/region/src/region.rs:119-128`;
`crates/region/tests/coverage_gaps.rs:513-520`.

**Failure scenario.** `with_capacity` rejects `capacity > SLOTMAP_MAX_RESERVE`
(= 2^32 − 3) *first*, then evaluates `capacity.checked_add(1).expect("…capacity
overflow")` as a discarded statement. On 64-bit, every `capacity` reaching that line is
≤ 2^32 − 3, so `checked_add(1)` is always `Some`; on 32-bit, `SLOTMAP_MAX_RESERVE` is
`usize::MAX − 2`, so the same holds. The guard cannot fire on any target this crate
supports.

`coverage_gaps.rs:517-519` justifies keeping it with: *"SLOTMAP_MAX_RESERVE IS
usize::MAX - 3, so the two guards' domains abut with no gap"*. That is wrong twice:
`SLOTMAP_MAX_RESERVE` is 2^32 − 3, which is nowhere near `usize::MAX` on 64-bit, and on
32-bit it equals `usize::MAX − 2`, not `− 3`. The stated reason for retaining the code
is therefore not a fact.

Relatedly, `with_capacity`'s `# Panics` doc (`region.rs:103-114`) opens with *"Panics if
`capacity == usize::MAX` (the underlying `slotmap` reserves one extra slot…)"* — the
unreachable guard — and only then mentions the 2^32 − 3 domain check that is actually
what fires. A reader matching on the panic message will match the wrong one.

**Recommended fix.** Either delete the `checked_add` statement and the paragraph
justifying it, or keep it as explicit defense-in-depth with a corrected comment
(`debug_assert!`-style, stating plainly that it is unreachable under the current
`SLOTMAP_MAX_RESERVE` and exists only to survive a future change to that constant).
Reorder `# Panics` so the guard that actually fires for `usize::MAX` is listed first.

**Severity rationale (LOW).** Dead code plus an incorrect explanatory comment and a
misordered panic contract; no runtime consequence.

---

## F17 — LOW — two stale doc/comment references introduced by `9741388`/`c077fd2`

**(a) `smoke.rs`'s `slot_index` helper documents a pre-F2 `Debug` rendering.**
`crates/region/tests/smoke.rs:33-34` says the output is
`Handle { key: DefaultKey(1v1) }`. Actual, reproduced:
`Handle { key: DefaultKey(1v1), region_id: 2 }`. The parser itself still works (it keys
off `DefaultKey(` and `v`), so this is comment-only — but it is the sole external
description of `Handle`'s `Debug` shape, and it is wrong.

**(b) `readme_examples.rs`'s cited line ranges are off by ~5-7 lines.**
`crates/region/tests/readme_examples.rs:7` says *"Mirrors README.md:43-57"* (actual
fence content: 49-62) and `:25` says *"Mirrors README.md:86-106"* (actual: 92-113).
`9741388` added 4 lines to §"Why?" above both blocks and nothing re-anchored the
citations. The file's whole purpose is anti-drift; a reader following the citation lands
mid-`Cargo.toml`-snippet.

**Recommended fix.** Update both comments. For (b), consider citing the section heading
(`§Quick start`, `§SyncRegion`) rather than line numbers, which cannot go stale.

**Severity rationale (LOW).** Comment-only, but both live in the crate's
drift-detection tests, where a stale reference is exactly the thing they exist to
prevent.

---

## F18 — LOW / design decision — `Region`'s `Debug` embeds a process-global `region_id`, making its output non-reproducible

**Citation.** `crates/region/src/region.rs:303-311`.

**Failure scenario (reproduced).** In one probe run, two freshly-created `Region<u8>`s
printed as `Region { region_id: 6, … }` and `Region { region_id: 7, … }` — the values
depend on how many `Region`/`SyncRegion` instances the process happened to construct
first, including inside unrelated tests running in the same binary. A downstream user
writing an `insta`/snapshot assertion over `format!("{:?}", region)`, or a log-diffing
test, gets a value that changes with unrelated code and with test-execution order.

Note the crate's *own* test (`f14_api_ergonomics.rs:52-63`) already dodges this by
asserting substring presence rather than content — a tacit acknowledgement.

**Recommended fix.** A judgment call, not a bug: either (a) omit `region_id` from
`Debug` (it is an internal identity, and `len`/`capacity` are the structurally useful
fields), or (b) keep it and add one sentence to the impl's rustdoc stating the output is
not stable across runs. (b) is cheaper and preserves the debugging value for
cross-region confusion — which is exactly what `region_id` in `Debug` is good for.

**Severity rationale (LOW).** No defect; a foreseeable downstream-flakiness source worth
one sentence of documentation before the API is frozen at 0.2.0.

---

## F19 — LOW / semver decision, cheapest to make now — `Handle`'s `Ord` orders `key` before `region_id`

**Citation.** `crates/region/src/handle.rs:64-83`.

**Observation.** Sorting a `Vec<Handle<T>>` drawn from two `Region`s interleaves them
(handles with equal `key` from different regions sit adjacent), rather than grouping by
region. A user cannot take a `BTreeMap<Handle<T>, V>` and range-scan the entries
belonging to one `Region`. Ordering `region_id` first would give that for free at
identical cost, and is the ordering most users would guess from the type's semantics.

Also: the justifying comment at `:64-65` — *"This matches the `Hash` impl (which also
hashes key first)"* — is a non-sequitur. `Hash` field order has no bearing on `Ord`
consistency; the property that actually matters (`cmp == Equal` ⟺ `eq == true`) holds
for either field order.

**Why "now".** Changing the order later is not a *compile*-breaking change, so
`cargo-semver-checks` will never flag it — but it silently reorders every user's
`BTreeMap`/`sort()` output, which is the worst kind of change to ship quietly. Deciding
before 0.2.0 costs nothing.

**Recommended fix.** Either flip to `region_id`-then-`key` and say so in the doc, or
keep the current order and replace the `Hash` non-sequitur with the real rationale
(and an explicit note that handles from different regions interleave under `Ord`).

**Severity rationale (LOW).** No defect; a one-line decision that is free today and
awkward after publication.

---

## F20 — INFO — investigated, nothing found

Recorded so a later round does not re-spend budget on these.

1. **`unsafe` anywhere in the crate.** Zero tokens in `src/`, `tests/`, `benches/`,
   `examples/`. Both the crate attribute (`src/lib.rs:66`) and the package lint
   (`Cargo.toml:27-28`) are present. Clean.
2. **`slotmap` types leaking into public signatures.** None — see the green table at
   the top. The `Iter`/`IterMut` newtypes added by `c077fd2` genuinely close the leak
   the commit message describes.
3. **`region_id` check completeness.** All four `Region` accessors taking a
   `Handle<T>` check it; there is no fifth. `SyncRegion` has no method that reaches the
   slotmap except through them.
4. **`Handle<T>` size/niche claims.** `size_of::<Handle<u8>>() == 16`,
   `size_of::<Option<Handle<u8>>>() == 16`, `align == 8` — asserted at compile time
   (`handle_static_asserts.rs:67`, `:73`) and reproduced at runtime. `#[repr(C)]` with
   `DefaultKey` (8 B, align 4) followed by `NonZeroU64` (align 8) yields zero padding;
   the niche survives `repr(C)`.
5. **`IterMut` aliasing.** No hazard: it wraps `slotmap::basic::ValuesMut` with no
   added `unsafe` and no lifetime widening.
6. **`DoubleEndedIterator` on `Iter`/`IterMut`.** Not implemented — but
   `slotmap-1.1.1/src/basic.rs:1219-1289` shows `Values`/`ValuesMut` do not implement it
   either, so nothing was lost by the newtype wrapping. Not a defect.
7. **`SyncRegion::remove` releasing the guard before the caller drops `T`.** Correct —
   the guard is a tail-expression temporary, dropped before the function returns; pinned
   by `tests/remove_guard_release.rs`, which would hang rather than fail on regression.
8. **`Region::clear()` invalidating handles.** Correct — `SlotMap::clear` →
   `drain()`, whose `Drop` calls `remove_from_slot` per slot and bumps each generation
   (`slotmap-1.1.1/src/basic.rs:615-642`). Covered by
   `coverage_gaps.rs::region_clear_happy_path`.
9. **`Region::reserve`'s overflow guard vs slotmap's internals.** slotmap's own
   `reserve` computes `self.len() + additional` unchecked; the crate's `checked_add`
   at `region.rs:180-184` is genuinely load-bearing (unlike `with_capacity`'s — F16)
   and is non-vacuously tested at `coverage_gaps.rs:492-506`.
10. **Identity under move/swap.** `mem::swap`ping two `Region<T>`s moves each
    `region_id` with its own slotmap, so no handle is ever orphaned or re-pointed. No
    aliasing path exists.
11. **`--no-default-features` host testing is not `no_std` testing.** `ci.yml:70`'s
    `cargo test -p sefer-region --no-default-features` still links `std` for the
    integration-test binaries; the only real `no_std` proof is the cross-build at
    `ci.yml:888` (which is what F1 breaks). Adequate as-is — the crate has no
    `std`-specific logic in that configuration — but the step's comment should not be
    read as `no_std` *test* coverage.
12. **`bench_ids_isolatable.rs`'s substring check.** Verified: the 14 current workload
    ids (`st/*`, `sync/*`, `raw/*`) have no substring collisions, and the `.bench(` /
    `.bench_batched(` markers do not alias each other. Working as intended.

---

# Release-readiness verdict

## **NOT READY**

Two blockers, in order:

1. **F1 (HIGH) — `main` does not compile for the crate's own advertised bare-metal
   `no_std` target.** Reproduced with the exact command CI runs. This is not a
   documentation issue: a real, currently-supported configuration went from building to
   failing in `9741388`, and four separate published portability claims (README, crate
   description, `keywords`, `categories`) became false at the same moment. It must be
   fixed *or* the claims must be narrowed before anything is published; both routes are
   maintainer decisions with real trade-offs (see F1's three options).
2. **F2 (HIGH) — the version was never bumped and 0.1.0 is already live**, so the
   publish is mechanically impossible, and the root crate's `version = "0.1"` pin plus
   `sefer-alloc`'s own re-export semver have not been prepared. Tracked as #785/#801,
   but the root-crate ripple is not captured in either task.

Compounding both: **F3** shows the release-gating CI job built to catch exactly this
class of problem returns green (`196 pass, 0 fail, "no semver update required"`) on a
0.1.0→0.1.0 comparison of a breaking change — so the automation currently in place
would not have stopped either blocker.

**What is genuinely good.** The F2 identity redesign itself is *correct*: every accessor
that can touch the slotmap checks `region_id`, the check is complete (no fifth
accessor), identity travels with the data under move/swap, the `NonZeroU64` niche claim
holds exactly as documented, `Ord`/`Eq`/`Hash` are mutually consistent, and the F14
`Iter`/`IterMut` newtypes genuinely close the slotmap-type leak they were introduced
for. The crate is memory-safe with zero `unsafe` of its own, all 41 tests pass in both
feature configurations, and clippy is clean at `-D warnings`. I found **no live
correctness bug in the shipping logic** — every finding above is a build-configuration
break, a release-process gap, a documentation inaccuracy, or a test that cannot fail
for the thing it guards.

**Path to READY**, in dependency order:

| # | Item | Blocking? |
|---|---|---|
| 1 | F1 — decide and apply one of the three atomic-width routes; re-run the cross-build | **yes** |
| 2 | F2 — bump to 0.2.0 + update root `Cargo.toml:851` + decide `sefer-alloc`'s own bump | **yes** |
| 3 | F5 — add the `get_mut` and `SyncRegion` I6 tests (cheap, and closes the counterfactual) | strongly recommended |
| 4 | F7 — add I6 to README, `tests/region_invariants.rs` (miri), and the fuzz oracle | strongly recommended |
| 5 | F6 — re-measure the perf tables post-F2; add `st/get_wrong_region` | strongly recommended |
| 6 | F8, F9, F10 — the three stale-doc / missing-CHANGELOG items | strongly recommended |
| 7 | F4 — fix `SyncRegion`'s poisoned-`Debug` + its misleading comment | recommended |
| 8 | F19, F18 — the two "decide before the API freezes" items | recommended (free now, awkward later) |
| 9 | F3, F11–F17 | non-blocking cleanup |

Items 3-6 are not strictly publish-blocking, but shipping a *breaking* release whose
headline invariant is 3/4-tested, absent from the landing page, absent from the miri and
fuzz oracles, and absent from the CHANGELOG would repeat the pattern this crate's own
audit history has already corrected twice.

---

## Reproduction notes

Every empirical claim above came from one of:

- `cargo test -p sefer-region --all-features` / `--no-default-features` (this tree)
- `cargo clippy -p sefer-region --all-features --all-targets -- -D warnings`
- `cargo build -p sefer-region --no-default-features --target thumbv7em-none-eabi`
  (**F1**)
- `rustc --print cfg --target thumbv7em-none-eabi | grep atomic` (**F1**)
- `cargo semver-checks check-release --package sefer-region` after
  `cargo install cargo-semver-checks --locked` (**F3**)
- `curl -s https://index.crates.io/se/fe/sefer-region` (**F2**)
- the published 0.1.0 tarball, extracted for API/attribute comparison
  (`curl -sL https://static.crates.io/crates/sefer-region/sefer-region-0.1.0.crate`)
  (**F2, F3, F15**)
- a throwaway scratch binary **outside the repository** with a path dependency on
  `crates/region`, built with a private `CARGO_TARGET_DIR`, exercising: poisoned-lock
  `Debug` vs `std`'s, cross-region `get_mut`/`SyncRegion` rejection, `Handle` size/align,
  cross-region `Ord`, `Region` `Debug` reproducibility, and the README snippet's
  `must_use` warnings (**F4, F5, F13, F14, F15, F17a, F18**)

No file under `crates/region/`, no `Cargo.toml`, no version, and no git ref was
modified. The benchmark suite was deliberately **not** run: `bench-scale-tool`'s
`save_manifest` writes the workspace-level `bench-iters.txt`, a tracked file, in a
workspace other agents are concurrently editing.
