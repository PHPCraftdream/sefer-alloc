# `sefer-region` — code-quality / structure / API-ergonomics review

**Date:** 2026-08-11
**Scope:** `crates/region/` (package `sefer-region`) in full — `src/{lib,region,sync_region,handle}.rs`,
`tests/` (11 files), `benches/` (5 harnesses), `examples/contended_reads.rs`, `Cargo.toml`,
`README.md` — read as *code quality*: structure, duplication, dead weight, API coherence,
un-measured hot-path smells, and test/bench organization.
**Reviewed tree:** `main` @ `e4f98d3b1e6681e5788a9516e96f3afde63434ee` (= `origin/main`,
pushed), working tree clean w.r.t. `crates/region/` and `docs/`.
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0`, Windows 10 x86_64.
**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add`/`git commit`. Every command quoted below was actually run on this host.

**Deliberately out of scope (already measured, already decided — not re-opened here):** the
three structural perf levers closed by `docs/perf/R827_REGION_NEW_CONTENTION_GATE.md` and
`docs/perf/R828_STRUCTURAL_LEVERS_GATE.md` — `NEXT_REGION_ID` contention (~87 %, decomposed
into ~69 % contention × ~57 % CAS-loop), `DenseSlotMap` (9.45× iteration / 2.9× churn, DEFER),
the batch/guard API (9.15×, GO opt-in, unimplemented), and drop-outside-write-lock (DEFER).
Two findings below *touch* those reports (Q7 on how their numbers are quoted in shipping
rustdoc, Q22 on two mitigations R827 does not consider); both cite the existing reports
explicitly rather than presenting anything of theirs as new.

---

## Verdict up front

**The source is in good shape as code.** Four files, ~1 600 lines including doc comments,
zero `unsafe`, zero `TODO`/`FIXME`/`unimplemented!`/`dbg!` anywhere in `src/`, `tests/`,
`benches/` or `examples/`, no dead or unused items in `src/`, no half-wired feature, and no
structural damage from the twenty-plus rounds of patches that have landed on it. `Region`
and `SyncRegion` do **not** duplicate logic — `SyncRegion`'s one-shots are one-line
delegations, which is the right shape. The functions are short; the longest body in the crate
is `try_with_capacity` at ~15 statements, and most are 3–5 lines. What accumulated is not
tangled code, it is **duplicated prose** (the invariant list lives in four places, `clear`'s
partial-clear paragraph in two) and a **fallible-API family that was bolted on without a
consistency pass** (Q4, Q5, Q8, Q13).

**One finding is HIGH and lands before anything else: `main` is red right now.** The exact
command `.github/workflows/ci.yml:64` runs —
`cargo clippy -p sefer-region --no-default-features --all-targets -- -D warnings` — fails to
compile at `e4f98d3`, and has been failing since `1bfbb7e` (task #822), seven commits ago. It
is not caught by `cargo test -p sefer-region --no-default-features` (same job, line 70 —
`dead_code` is a warning there, not an error), it is not caught by
`cargo clippy … --all-features` (the row the #832 closing review's green table ran), and it is
not caught by `npm run check` (whose matrix contains no `sefer-region` row at all).

Twenty-three findings: **1 HIGH, 6 MEDIUM, 10 LOW, 6 INFO.** None is a memory-safety or
data-race defect; the crate's `#![forbid(unsafe_code)]` posture and its invariant enforcement
both hold up under this reading.

---

## What was verified green (so the negatives below are read in context)

| Check | Result |
|---|---|
| `cargo fmt --check -p sefer-region` | **PASS** — exit 0, no diff |
| `cargo clippy -p sefer-region --all-targets --all-features -- -D warnings` | **PASS** — clean, no diagnostics |
| `cargo clippy -p sefer-region --no-default-features --all-targets -- -D warnings` | **FAIL** — see **Q1** |
| `cargo test -p sefer-region --all-features` | **PASS** — 70 passed, 0 failed, 1 ignored across 12 binaries |
| `grep -rn "TODO\|FIXME\|XXX\|unimplemented!\|dbg!(" src/ tests/ benches/ examples/` | **no matches** |
| Dead/unused items in `src/` | **none.** Every `pub` item is reachable from the crate root; the only doc-hidden item (`dbg_try_mint_region_id`) is a sanctioned test-only forwarder with a live consumer (`tests/region_id_exhaustion.rs`, 7 tests) |
| `Region` ↔ `SyncRegion` logic duplication | **none.** All 8 `SyncRegion` one-shots are single-expression delegations through `read()`/`write()`; no algorithm is written twice |
| Prior review's fixed findings still fixed | **yes.** `grep -n '\bI6\b' crates/region/src/region.rs` now returns only the two lines of the genuine I6 bullet (F-A2 closed); the I6 bullet is present in both `lib.rs` and `region.rs` (F-C9 closed); `r828_batch_guard_probe.rs:106` reads "time per lookup (N = …)" (F-C4 closed) |

---

# Findings

## Q1 — HIGH — `main` is red: the crate's own release-gate clippy row does not compile, and has not for seven commits

`crates/region/tests/handle_static_asserts.rs:40` declares the helper

```rust
const fn assert_send_sync<T: Send + Sync>() {}
```

and **all four** of its use sites (`:63`, `:71`, `:75`, `:79`) carry `#[cfg(feature = "std")]`.
Under `--no-default-features` the helper therefore has zero users:

```
$ cargo clippy -p sefer-region --no-default-features --all-targets -- -D warnings
error: function `assert_send_sync` is never used
  --> crates\region\tests\handle_static_asserts.rs:40:10
   = note: `-D dead-code` implied by `-D warnings`
error: could not compile `sefer-region` (test "handle_static_asserts") due to 1 previous error
```

That is byte-for-byte the command in `.github/workflows/ci.yml:64`, inside the
`sefer-region package gates` job that task #793/F15 created specifically to gate this crate's
release. `HEAD` is `origin/main`, so this is the state of the published branch.

**Provenance.** `git log -S 'cfg(feature = "std")' -- crates/region/tests/handle_static_asserts.rs`
returns exactly one commit: `1bfbb7e` (F7, task #822). Before it, the four `const _` assertions
were ungated (`git show 99db640:crates/region/tests/handle_static_asserts.rs:64,71,74,77`) and
the helper always had users. `1bfbb7e` added the gates and orphaned the helper in the
`--no-default-features` configuration. Nothing since has touched it; `7c5f26e`'s removal of
`handle_layout_matches_expectations` is unrelated (that test used only `size_of`, never
`assert_send_sync`).

**Why three separate gates missed it:**

1. `cargo test -p sefer-region --no-default-features` (ci.yml:70, and the #832 closing
   review's own green table) compiles the same file but without `-D warnings` — `dead_code` is
   a warning, the binary builds, 50 tests pass.
2. `cargo clippy … --all-features` passes, because with `std` on the helper has four users.
   The closing review's green table lists only that row.
3. `npm run check` never runs it: `grep -n "sefer-region" scripts/check-matrix.mjs scripts/check-all.mjs`
   returns **nothing** — the local pre-push gate covers the root crate's six clippy rows only,
   so member-crate CI rows have no local mirror at all.

**Fix — and the good fix is not `#[allow(dead_code)]`.** Three of the four assertions do not
need `std` in the first place: `*const u32` is `!Send + !Sync` in `core`, and `NonSyncType`
can use `core::cell::Cell` instead of `std::cell::Cell`. Only the `Rc` case genuinely needs
`alloc`/`std`. Un-gating the two `core`-only assertions both fixes the red row and *restores*
Send/Sync tripwire coverage to the `no_std` configuration, where it currently does not run at
all. Silencing with `#[allow(dead_code)]` would leave that coverage hole in place.

**Also worth doing in the same pass:** add the two `sefer-region` clippy rows (and the sibling
member crates') to `scripts/check-matrix.mjs`, or accept explicitly that member-crate gates are
CI-only — the current state is the third variant, where nobody decided and nobody noticed for
seven commits. Per CLAUDE.md's own post-push rule ("confirm CI went green — do not assume
it"), this is the exact failure shape that rule exists to catch.

## Q2 — MEDIUM — `SyncRegion::read()`/`write()` clear the poison flag on **every** acquisition, not "after recovering from it"

`sync_region.rs:194-198` and `:208-212`:

```rust
pub fn read(&self) -> RwLockReadGuard<'_, Region<T>> {
    let guard = self.inner.read().unwrap_or_else(PoisonError::into_inner);
    self.inner.clear_poison();
    guard
}
```

`RwLock::clear_poison` is not free and not conditional. In this toolchain's std
(`library/std/src/sync/poison/rwlock.rs:619-621` → `library/std/src/sync/poison.rs:151-153`)
it is `self.failed.store(false, Ordering::Relaxed)` — an unconditional store to an `AtomicBool`
that sits between `RwLock`'s `sys::RwLock` word and its `UnsafeCell<T>`, i.e. almost certainly
on the same cache line as the lock word every reader already contends for. Every single
`read()` — and therefore every one-shot `get_cloned`/`contains`/`len`/`is_empty`, the crate's
hottest concurrent path and the one R828 §2 measured — now pays that store even though poison
is absent in every acquisition but the one following a writer panic.

The class's own doc says something narrower than what the code does
(`sync_region.rs:51-59`): "`read` and `write` … **clear the lock's poison flag immediately
after recovering from it**". The implementation clears it whether or not anything was
recovered.

**Checked, and it is not a correctness bug** (stated explicitly so the fix is not
over-scoped): the worry would be a thread clearing poison it never observed. It cannot happen.
`std` sets poison only when a `RwLockWriteGuard` is dropped during unwind, and a writer cannot
hold the write lock while any other thread holds a guard. So between `self.inner.read()`
returning `Ok` and `clear_poison()` executing, no new poison can appear; the store is either
clearing poison this very call recovered, or a no-op.

**Fix (behavior-identical, one branch):**

```rust
match self.inner.read() {
    Ok(g) => g,
    Err(p) => { let g = p.into_inner(); self.inner.clear_poison(); g }
}
```

This matches the documented policy exactly and removes the store from the uncontended path.
Not measured here — an honest A/B would reuse `benches/r828_batch_guard_probe.rs`'s
`concurrent_manual_guard` arm, which is exactly the shape that would show it if it shows
anywhere.

## Q3 — MEDIUM — values are enumerable, their handles are not: no handle-yielding iteration and no `retain`

`Region::iter`/`iter_mut` (`region.rs:513-527`) yield `&T`/`&mut T` only, wrapping
`slotmap`'s `values()`/`values_mut()`. There is no way, from outside the crate, to obtain the
`Handle<T>` of a value you are looking at. Consequences for a consumer:

- "scan the region and remove the entries matching a predicate" is impossible without keeping
  a parallel `Vec<Handle<T>>` maintained by hand at every `insert`/`remove` site — which is
  precisely the bookkeeping a handle store exists to remove.
- there is no `retain` either, although `slotmap::SlotMap::retain` exists upstream and would
  wrap trivially (it hands the closure `(DefaultKey, &mut V)`; the key would become a
  `Handle<T>` via the crate's own `Handle::from_key_and_region`).

This is *not* an encapsulation constraint. Yielding `Handle<T>` leaks no `slotmap` type — it is
this crate's own opaque type, and I7 makes handles minted from `self.region_id` correct by
construction. The `Iter`/`IterMut` wrappers already exist precisely so the public surface never
names a `slotmap` type (`region.rs:599-608`), so the pattern is established.

Suggested shape, additive and semver-safe: `iter_handles(&self) -> impl Iterator<Item = (Handle<T>, &T)>`
(or a named `HandleIter`, matching the existing wrapper convention) plus
`retain(&mut self, f: impl FnMut(Handle<T>, &mut T) -> bool)`. Worth deciding *now* rather
than after the 0.2.0 freeze, because a later addition of a handle-yielding iterator interacts
with any future `DenseRegion` handle-identity decision (`R828_STRUCTURAL_LEVERS_GATE.md` §1's
first open design question) — deciding the iteration surface first constrains that fork less
than deciding it afterwards.

## Q4 — MEDIUM — `try_new`/`try_with_capacity` return an error type two of whose three variants they cannot produce, and `TryReserveError` is not `#[non_exhaustive]`

`region.rs:265` / `:312` both return `Result<Self, TryReserveError>`, but `try_new` can only
ever fail with `RegionIdExhausted`. The type is wide enough that the crate has to *document its
way out of it* (`region.rs:45-49`):

> "Only ever returned by `Region::try_new`/`try_with_capacity` (constructors mint a new
> region_id); `Region::try_reserve` on an existing `Region` never produces this variant…"

— i.e. one variant is impossible-for-some-methods in one direction and the other two are
impossible-for-`try_new` in the other. A caller writing an exhaustive `match` on
`try_new`'s error gets two arms that can never execute, and rustdoc offers prose instead of
types to explain it. Three smaller points in the same area:

- **Not `#[non_exhaustive]`** (`region.rs:33-50`), and `CapacityExceeded` has public fields.
  Adding a variant later — e.g. if `try_insert` (Q13) lands, or if a future `slotmap` grows a
  new failure mode — is a breaking change. The same freeze-time decision was taken
  deliberately for other crates in this workspace (task #728 for `size-classes`' `Params`,
  #715 for `aligned-vmem`'s mock `Call`); `sefer-region` never made it.
- **The name collides with `std::collections::TryReserveError`**, which is the type a reader
  expects `try_reserve` to return and which carries no construction failures.
- `RegionIdExhaustedError` is already public with a `From` impl (`region.rs:78-82`), so
  `try_new() -> Result<Self, RegionIdExhaustedError>` costs nothing to express and makes the
  impossible arms unrepresentable.

**Recommended:** `try_new -> Result<Self, RegionIdExhaustedError>`; keep `TryReserveError` for
`try_with_capacity`/`try_reserve`; add `#[non_exhaustive]` to it. If the wide type is kept for
uniformity, that is defensible — but it should be a recorded decision, not the current
"documented around" state.

## Q5 — MEDIUM — `SyncRegion` did not get the fallible-constructor pass, and has no `try_read`/`try_write` next to its own async warning

`SyncRegion` exposes exactly `new`, `with_capacity`, `read`, `write`, `insert`, `remove`,
`contains`, `len`, `is_empty`, `clear`, `get_cloned`, `into_inner` (`sync_region.rs:139-309`).
Two gaps:

1. **No `try_new`/`try_with_capacity`.** F11/#825 added the fallible family to `Region` for
   exactly one documented reason — a 32-bit host where `region_id` exhaustion is *reachable,
   not theoretical* (`region.rs:210-217`) — and then left the concurrent type, which is the
   type a long-lived server actually uses, panic-only. A workaround exists
   (`Region::try_new().map(SyncRegion::from)`, via the `From` impl at `sync_region.rs:125-137`)
   and is mentioned nowhere; `SyncRegion::new`'s `# Panics` section (`:159-170`) just points at
   `Region::new`.
2. **No `try_read`/`try_write`.** The class doc spends 25 lines (`:95-120`) warning that these
   are blocking, that `tokio::time::timeout` cannot cancel a blocking acquisition, and that
   `spawn_blocking` does not make one cancellation-safe — and then offers no non-blocking
   entry point, though the `Debug` impl uses `self.inner.try_read()` internally
   (`:334`) so the capability is right there. `read()`/`write()` already commit to returning
   `std`'s concrete guard types as "a deliberate, stable API commitment" (`:190-193`), so
   `try_read -> Option<RwLockReadGuard<'_, Region<T>>>` adds no new type commitment.

Either add them, or state in the class doc why the asymmetry is intentional and name the
`try_new().map(SyncRegion::from)` route. Right now a reader cannot tell whether it is a
decision or an oversight.

## Q6 — MEDIUM — the I1–I7 invariant text exists in four hand-maintained copies, and drift between two of them was the last review's only pre-tag blocker

The same seven invariants are written out, in four different wordings, at:

| location | length |
|---|---|
| `crates/region/src/lib.rs:26-56` | 31 lines |
| `crates/region/src/region.rs:164-217` | 54 lines (I7 carries an extra ~15-line exhaustion discussion) |
| `crates/region/README.md` | full list |
| `docs/INVARIANTS.md:8-46` | the canonical list |

This is not a stylistic objection — it is the mechanism behind two of the nine findings the
#832 closing review filed three days of commits ago: **F-A2** (five stale `I6` references
surviving in `region.rs` after F2's renumbering — the *only* finding that review called
pre-tag-blocking, and four of the five were rustdoc on public constructors) and **F-C9** (both
`lib.rs` and `region.rs` advertised "I1–I7" while listing six). Both were copy-drift; both were
caught by a human re-reading, not by any check. A fifth copy is one round away from existing.

**Fix:** hoist the shared body into `crates/region/src/invariants.md` and pull it into both
rustdoc sites with `#![doc = include_str!("invariants.md")]` / `#[doc = include_str!(…)]`,
keeping only the genuinely `Region`-specific commentary (the exhaustion-bound paragraph) inline.
`docs/INVARIANTS.md` can then include or reference the same file instead of restating it. The
README copy is the one that legitimately stays separate (it is the crates.io landing page), so
the count goes 4 → 2 with one of the two being generated.

## Q7 — MEDIUM — shipping rustdoc and README publish contended-read multipliers that this round's own gate report says must not be treated as stable

`sync_region.rs:86-93` (rendered on docs.rs) states:

> "…resulting in a ~4× aggregate throughput loss going from 1 to 8 reader threads on a 16-CPU
> host. Batching multiple reads under one held `read` guard restores flat scaling at **~30× the
> one-shot aggregate at 8 threads**."

and `crates/region/README.md:208-225` publishes the table those figures come from (1 221 vs
38.7 ns/op, i.e. 31.6×), sourced from `examples/contended_reads.rs`.

`docs/perf/R828_STRUCTURAL_LEVERS_GATE.md` §2 measured the same question with the DCE bug fixed
and got **9.15×**, explicitly recorded the gap as unresolved (`:163`, "flagged as an open
discrepancy, not silently reconciled") and instructed (`:181`, `:268`) that the next work on
this question should re-measure "rather than reusing any of the three numbers now on record."
The shipping rustdoc quotes one of those three numbers, with no caveat and no cross-reference,
and the README table has no pointer to R828 either.

The two are not *arithmetically* contradictory (README measures one-shot-vs-batched at 8
readers; R828 measured one-shot-vs-batched at 1 reader plus batched at 8), which is exactly the
problem — a reader has two multipliers 3.5× apart for "how much does batching win", from two
harnesses that never reference each other. Compounding it, those two harnesses
(`examples/contended_reads.rs` and `benches/r828_batch_guard_probe.rs`) are independent
re-implementations of an overlapping measurement, sharing no code and no cross-link.

**Fix (docs-only, no re-measurement needed):** in `sync_region.rs`'s "Contended reads" section
and the README table, state the measurement's provenance and its regime (which harness, how
many readers, one-shot-vs-what) and link both to `R828_STRUCTURAL_LEVERS_GATE.md` §2 and its
open-discrepancy note. If the number is to keep shipping in rustdoc at all, it needs the same
regime labelling CLAUDE.md's cost-and-benefit rule already demands of gate reports.

## Q8 — LOW — `Region::new`'s panic message breaks the crate's own message convention, and calls exhaustion "overflow"

Three panicking wrappers, three different shapes:

| site | message |
|---|---|
| `region.rs:286` | `.expect("region_id overflow")` |
| `region.rs:359` | `panic!("Region::with_capacity: {e}")` |
| `region.rs:444` | `panic!("Region::reserve: {e}")` |

The convention (method name + the error's own `Display`) is not incidental — it is
*regression-tested*: `tests/coverage_gaps.rs:509-536` exists solely to assert that
`reserve`'s panic names `reserve` and not `with_capacity`, after task #825 found a real bug of
exactly that shape. `new` opts out of the convention and additionally mis-names the condition:
the counter does not overflow, it saturates to a permanent sentinel (`region.rs:107-117`), and
`RegionIdExhaustedError`'s own `Display` already says so correctly ("process-wide region_id
counter exhausted"). **Fix:** `Self::try_new().unwrap_or_else(|e| panic!("Region::new: {e}"))`.

## Q9 — LOW — a provably-dead guard with an 11-line comment, checking a quantity that is never used

`region.rs:322-332`:

```rust
// Defense-in-depth, not a guard that can currently fire: … [11 lines proving it cannot]
capacity.checked_add(1).ok_or(TryReserveError::Overflow)?;
let region_id = try_mint_region_id(&NEXT_REGION_ID)?;
```

The statement's value is discarded; `capacity + 1` is never used (the next line passes plain
`capacity` to `SlotMap::with_capacity`). `tests/coverage_gaps.rs:538-562` independently
documents that this guard is unreachable on both 32- and 64-bit targets. So the crate carries a
no-op statement plus eleven lines of comment explaining that it is a no-op, in the middle of a
constructor.

Not harmful — but if it is kept as a tripwire against a future `slotmap` domain change, a
`debug_assert!(capacity.checked_add(1).is_some())` says that more honestly than a discarded
`?`, and if it is deleted, the comment's reasoning is worth keeping as a one-line note on
`SLOTMAP_MAX_RESERVE`. Either is better than the present shape, which reads like leftover
scaffolding to anyone who has not read the test.

## Q10 — LOW — two different published domain limits for the same backing store

`try_with_capacity` rejects `capacity > 2^32 - 3` (`region.rs:315`); `try_reserve` rejects
`len() + additional > 2^32 - 2` (`region.rs:408`). Both are user-visible: in rustdoc
(`:293-294`, `:398-399`) and in `TryReserveError::CapacityExceeded`'s `Display`, which prints
whichever limit fired. The off-by-one is explained only as "one slot is reserved as a sentinel"
(`region.rs:39-41`) — an upstream implementation detail — so a user who reads both methods sees
`Region::with_capacity(n)` and `Region::new()` + `reserve(n)` admitting different maximum `n`
with no stated reason. Name the two constants in one place (they are currently two
independently-declared `const`s inside two different function bodies) and say in one sentence
why they differ, or unify on the stricter one.

## Q11 — LOW — `Error` impls are `std`-only, though the MSRV allows `core::error::Error`

`region.rs:29-30` and `:68-76` gate both `impl std::error::Error` on `#[cfg(feature = "std")]`.
The crate advertises `no_std + alloc` as a first-class configuration (`lib.rs:65-70`), and
`rust-version = "1.88"` (Cargo.toml:5) is well past `core::error::Error`'s stabilization in
1.81. A `no_std` consumer today gets two public error types with `Display` but no `Error` impl,
so they compose with nothing. Implementing `core::error::Error` unconditionally (and keeping
the `std` re-export path, which is the same trait) closes the gap with no MSRV cost and no API
break.

## Q12 — LOW — the I7 guard is written out four times; single-siting it would make the invariant enforceable by inspection

`get` (`:461-466`), `get_mut` (`:470-475`), `contains` (`:479-484`) and `remove` (`:490-495`)
each open with the same three lines:

```rust
if handle.region_id != self.region_id {
    return None; // or `false`
}
```

Four copies of the crate's single most load-bearing runtime check, in a crate whose I7 wording
is "**every** accessor … checks its `region_id` before touching the backing slotmap". A private
helper — `#[inline] fn owned_key(&self, h: Handle<T>) -> Option<slotmap::DefaultKey>` — makes
each accessor a one-liner (`self.inner.get(self.owned_key(handle)?)`) and makes "does every
accessor check?" answerable by grepping for one call. Zero behavioral change, zero codegen
change (private, generic, `#[inline]`). This is the one place in the crate where DRY is worth
more than the two lines it saves, because a *fifth* accessor added later (Q3's `iter_handles`,
a future `try_insert`) is exactly where a missed guard would land.

## Q13 — LOW — `insert` is the one operation that can actually hit the live-entry limit, and it is the one with no fallible form

After F11/#825 the crate has `try_new`, `try_with_capacity`, `try_reserve` — construction and
reservation. `insert` (`region.rs:454-456`) still panics "if the backing `slotmap` is full
(2^32 - 2 live entries)" with no `try_insert`. On the 32-bit hosts whose exhaustion the crate
documents as reachable, the live-entry ceiling is the *one* limit a real workload can approach
by simply doing its job, and it is the one the fallible family skipped. The check is trivial
(`self.inner.len() >= SLOTMAP_MAX_LIVE`) and would reuse the `TryReserveError` machinery — but
adding a variant later is a breaking change unless Q4's `#[non_exhaustive]` lands first, which
is why the two should be decided together.

## Q14 — LOW — five bench binaries re-implement the same statistics block, with one silent trap and one leftover from the last review

Every probe (`region_new_contention_gate.rs`, `r828_dense_iteration_probe.rs`,
`r828_batch_guard_probe.rs`, `r828_drop_outside_lock_probe.rs`) contains its own copy of:
collect raw samples → print `raw_csv,…` → `HashMap` group-by → `sort_by(partial_cmp.unwrap())`
→ mean → `values[values.len() / 2]` → `assert!(x.is_finite() && x > 0.0)` → format a ratio.
Three observations, in increasing order of consequence:

1. **`values[len / 2]` is the upper-middle element, not a median**, and is only correct because
   `SAMPLES = 5` is odd in all four probes. Anyone who bumps a probe to `SAMPLES = 4` or `6`
   silently starts publishing a labelled "median" that is not one — the exact class of defect
   CLAUDE.md's derived-numbers rule point 3 targets ("statistic names are printed by the code
   that computes them"), which R30-4 already caught once in `r29_3_decomposition_gate.rs`.
2. **`r828_drop_outside_lock_probe.rs:119` still computes `_blocked_median` and discards it**,
   while `R828_DROP_OUTSIDE_LOCK_summary.csv` publishes a `blocked_median_ns` column — F-C8's
   observation, verified still true at `e4f98d3`.
3. **The "derived … by a small script" claim in both reports remains unbacked.**
   `R828_STRUCTURAL_LEVERS_GATE.md:255` and `R827_REGION_NEW_CONTENTION_GATE.md:52` both cite a
   derivation script; `ls scripts/ | grep -iE 'r82|region'` returns nothing at HEAD. (Status
   note, not a new finding — F-C8 was filed and is one of the three the fix commit `a935e79`
   did not close.)

**Fix:** one `benches/common/stats.rs` pulled in with `#[path = "common/stats.rs"] mod stats;`
(the standard way to share code between bench binaries), exposing `mean`, `median` (a real one),
and a `print_raw_csv` helper — roughly 40 lines that delete ~150 duplicated ones and make (1)
and (2) impossible.

## Q15 — LOW — the contention gate dispatches arms by string with a catch-all, so a mislabelled row is one typo away

`benches/region_new_contention_gate.rs:61-76`:

```rust
for &arm_name in &["shared_atomic", "shared_fetch_add", "baseline_local_atomic"] {
    …
    let wall_ns = match arm_name {
        "shared_atomic"    => run_shared_atomic(thread_count),
        "shared_fetch_add" => run_shared_fetch_add(thread_count),
        _                  => run_baseline_local_atomic(thread_count),
    };
```

Add a fourth arm name to the array (or mistype an existing one) and the harness silently runs
`baseline_local_atomic` while labelling every emitted row — raw CSV, summary, and every ratio
derived from it — with the new name. There is no assertion anywhere that the arm executed is
the arm named. A three-variant enum, or a `[( &str, fn(usize) -> u64 ); 3]` table, makes the
mapping total and the mistake unrepresentable at zero cost.

This harness has been rebuilt twice for measurement-integrity reasons (`59c079c`, then
`a935e79`'s F-C6 third arm), and CLAUDE.md's R30-8 rule is precisely about proving that an arm
exercised the mechanism its label claims. A catch-all `_ =>` in the arm dispatcher is that rule's
failure mode built into the harness's control flow.

## Q16 — LOW — test files are named after the review process, not the behavior, and the shared fixture is copy-pasted

- `tests/coverage_gaps.rs` — **880 lines, 27 tests**, named after why it was written ("we had
  coverage gaps") rather than what it covers. It currently holds drop-once semantics for both
  `Region` and `SyncRegion`, `clear` happy paths, iterator behavior, `get_mut`, `Default`,
  the whole capacity API, and all six fallible-variant tests. Nothing about the name tells a
  future reader that the `try_reserve` tests live there.
- `tests/f14_api_ergonomics.rs` — 429 lines, named after finding ID "F14" from a review three
  rounds ago. `bench_ids_isolatable.rs` is likewise named after a bug report rather than a
  property, though it at least reads as one.
- `struct DropCounter` is defined twice, near-identically: `tests/coverage_gaps.rs:12-29` and
  `tests/clear_partial_under_panic.rs:18-46` (the latter adds panic-on-drop). There is no
  `tests/common/mod.rs`, the standard Rust mechanism for exactly this.

Suggested split, behavior-first: `drop_semantics.rs`, `clear.rs`, `capacity_api.rs`,
`fallible_api.rs`, `iterators.rs`, `debug_impls.rs`, `handle_ord.rs`, with `tests/common/mod.rs`
holding `DropCounter` and `catch_panic_message` (currently `coverage_gaps.rs:662`, private to
that file). This is pure organization — no test bodies need to change — but it is the
difference between a suite a maintainer can navigate and one where the answer to "is X tested?"
is `grep`.

## Q17 — LOW — `SyncRegion::clear`'s doc duplicates `Region::clear`'s partial-clear paragraph verbatim

`sync_region.rs:273-282` repeats, word for word, the ten-line partial-clear-under-panic
contract from `region.rs:538-545` (the "(1) no value is dropped twice, (2) no value is leaked …
(3) accounting remains correct" block, plus the `tests/clear_partial_under_panic.rs` pointer).
That paragraph has been rewritten twice already (tasks #787/F6 and #817/F4 both narrowed the
survivor claim); the next narrowing has to find both copies. Same class as Q6, smaller blast
radius: replace the body with "the partial-clear contract is [`Region::clear`]'s — see there"
plus the `SyncRegion`-specific sentence (the reentrancy pointer) that genuinely belongs here.

## Q18 — INFO — `iter`/`iter_mut` are the only non-mutating accessors without `#[must_use]`

`len`, `is_empty`, `capacity`, `get`, `get_mut`, `contains`, `insert`, `new`, `with_capacity`,
`into_inner` all carry `#[must_use]`; `iter` (`region.rs:513`) and `iter_mut` (`:523`) do not.
`Iterator`'s own `#[must_use]` does not help — it applies to `impl Iterator` return positions,
not to a concrete named type like `Iter<'_, T>` — so `region.iter();` as a statement compiles
silently today.

## Q19 — INFO — `Handle`'s constructor and `Debug` still use the old field order the fields were just reordered away from

`f044f86` reordered `Handle`'s fields to `(region_id, key)` specifically so that declaration
order matches `Ord`'s comparison order (`handle.rs:42-51` documents the reasoning at length).
Two sites in the same file kept the old order: `from_key_and_region(key, region_id)` and its
struct literal (`handle.rs:60-66`), and the `Debug` impl, which prints `key` then `region_id`
(`:133-140`). Cosmetic; mentioned only because it undercuts the invariant that commit
deliberately established, one file away from where it established it. (Note that changing
`Debug`'s order is not free — `tests/smoke.rs:40-53`'s `slot_index` parses `Debug` output,
though it searches for `DefaultKey(` rather than relying on field position, so it would survive.)

## Q20 — INFO — "poison is never observable" is contradicted by the `Debug` impl

`sync_region.rs:60-63` states that `SyncRegion` deliberately exposes no `is_poisoned()`
"because poisoned state is never observable for longer than the single access that first
recovers from it". The `Debug` impl (`:336-340`) renders `poisoned: true` via `try_read()`,
which — unlike `read()` — does *not* clear the flag. So poison is observable, without clearing,
through a public trait impl. Both behaviors are defensible; the sentence just needs "(except
through [`Debug`], which reports it without clearing)".

## Q21 — INFO — the `Region` ↔ `SyncRegion` conversion pair is asymmetric in style

`From<Region<T>> for SyncRegion<T>` exists (`sync_region.rs:125-137`), but the inverse is only
`into_inner` (`:151-155`). `impl<T> From<SyncRegion<T>> for Region<T> { fn from(sr) -> Self { sr.into_inner() } }`
is three lines and makes the pair discoverable from either direction (and `.into()`-usable in
generic code). Both directions are already documented as handle-preserving, so there is no
semantic obstacle.

## Q22 — INFO — two structural mitigations for R827's measured contention that neither R827 nor the design notes consider

Filed as an addition to `docs/perf/R827_REGION_NEW_CONTENTION_GATE.md` (whose own conclusion
says a future perf attempt "should not assume switching back to a shared `fetch_add` would
recover most of the gap"), not as a re-report of its measurement:

1. **Lazy minting.** `region_id` is minted eagerly in `try_new`/`try_with_capacity`
   (`region.rs:266`, `:333`), but nothing *needs* it until the first `insert` — the accessors
   only ever compare it, and a region with no handles outstanding can reject every handle it is
   shown. Storing `Option<NonZeroUsize>` and minting inside `insert` (which already takes
   `&mut self`) would remove the shared atomic from `Region::new()` entirely, i.e. from the
   whole workload R827 measured, at the cost of one perfectly-predicted branch per insert.
   Real costs, both semantic: the exhaustion panic moves from `new()` to `insert()` (a
   documented-contract change, and `insert`'s panic list currently names only the slotmap-full
   case), and `Debug` would have to render an unminted region. Worth a decision before the API
   freezes, since the panic contract is part of the public surface.
2. **Block reservation.** A thread-local cursor claiming `N` ids per shared RMW amortizes the
   contended operation `N`-fold. But it divides the 32-bit id budget by `N`, and this crate
   explicitly documents 32-bit exhaustion as *reachable rather than theoretical*
   (`region.rs:210-217`) — so on the one target where the contention fix would matter for a
   long-lived process, it is also the target where the budget cut bites. Probably a
   non-starter as stated; recorded so the next round does not re-derive it from scratch.

Neither is a recommendation to implement now. Both are cheap to measure with the existing
`benches/region_new_contention_gate.rs` (a fourth arm), if the question is ever reopened.

## Q23 — INFO — `docs/INVARIANTS.md`'s I6 citation points outside the published package

`docs/INVARIANTS.md:32-37` says I6 is "Verified in `tests/freelist_reuse.rs`" — that is the
**workspace-root** test (`D:\dev\rust\sefer-alloc\tests\freelist_reuse.rs`, which drives
`sefer_alloc::Region`, the re-export at `src/lib.rs:384`). It is real coverage, but it is not in
the `sefer-region` package, does not run under `cargo test -p sefer-region`, and does not ship
in the published tarball. The member's *own* I6 oracle —
`crates/region/tests/coverage_gaps.rs:451-489`, `region_reserve_reuses_freed_slots_on_churn` —
is cited nowhere. One line in each direction fixes it. (Same shape for I7, which
`INVARIANTS.md` attributes to the root `tests/region_invariants.rs` while
`crates/region/tests/smoke.rs:100-221` holds three member-local cross-region tests.)

---

## Checked and explicitly NOT findings

Recorded so a later reader does not re-litigate them:

- **Missing `#[inline]` on hot accessors.** Not a defect: every method on `Region<T>`/
  `SyncRegion<T>` is generic in `T`, so its MIR is exported and downstream monomorphization can
  inline it without the attribute. The one non-generic hot function in the crate,
  `try_mint_region_id` (`region.rs:104`), *does* carry `#[inline]` — which is load-bearing
  exactly because it is not generic and is called from generic code instantiated downstream.
- **No `DoubleEndedIterator` on `Iter`/`IterMut`.** Not a gap: `slotmap` 1.1.1 does not
  implement it upstream either (`slotmap-1.1.1/src/basic.rs:1277-1291` implements only
  `FusedIterator` and `ExactSizeIterator` for `Values`/`ValuesMut`), so there is nothing to
  forward.
- **`SLOTMAP_MAX_RESERVE`/`SLOTMAP_MAX_LIVE`'s `((1u64 << 32) - 3) as usize` on a 32-bit
  target.** Checked for truncation: `4_294_967_293` and `4_294_967_294` both fit in `u32`, so
  the `as usize` cast is lossless on the narrowest supported target.
- **`SyncRegion::remove`'s "guard released before the value is dropped" contract.** Correct as
  written: the temporary write guard in `self.write().remove(handle)` is dropped at the end of
  the tail expression, before the `Option<T>` reaches the caller. Backed by
  `tests/remove_guard_release.rs`.
- **The unconditional `clear_poison` as a *correctness* issue.** Analyzed in Q2 — it cannot
  clear poison that no thread observed, because no writer can poison while a guard is held.
  Q2 is a doc-fidelity + wasted-store finding only.
- **`Eq`/`Hash`/`Ord` field-order mismatch in `Handle`** (`eq` compares `key` first, `cmp`
  compares `region_id` first, `hash` feeds `key` first). All three are internally consistent
  and mutually compatible; the differing orders only affect short-circuit behavior, and `Ord`'s
  order is deliberately documented as an unspecified implementation detail (`handle.rs:111-117`).
- **`Region` not implementing `Clone` for `T: Clone`.** Correctly absent — a cloned region
  would need a fresh `region_id`, so no outstanding handle could address the copy. Not
  documented as a deliberate absence, but not wrong.

---

## Recommended order

1. **Q1** — red CI on `main`, and the fix (un-gating the two `core`-only assertions) *adds*
   `no_std` coverage rather than silencing a lint. Nothing else should land first.
2. **Q4 + Q13 + Q5** — one API-shape decision, taken together, before the 0.2.0 freeze:
   error-type width, `#[non_exhaustive]`, `try_insert`, `SyncRegion::try_*`. These are the only
   findings here that become expensive after a tag.
3. **Q3** — handle-yielding iteration / `retain`; also pre-freeze, and it constrains the future
   `DenseRegion` handle-identity fork less if decided now.
4. **Q2** — one-branch change, documented policy restored, store removed from the hottest path.
5. **Q6 + Q17** — de-duplicate the invariant list and the `clear` contract; this is the class
   that produced the last review's only pre-tag blocker.
6. **Q7** — docs-only regime/provenance labelling of the contended-read numbers.
7. **Q8, Q9, Q10, Q11, Q12** — small `src/` hygiene, one pass.
8. **Q14, Q15, Q16** — bench/test structure; no user-visible effect, but Q15 is a live
   measurement-integrity hazard in a harness that has already been rebuilt twice.
9. **Q18–Q23** — INFO cleanup and recorded decisions.

---

## API evolution deferral (2026-08-11)

The four pre-freeze API findings that this review explicitly marked as "resolve before
the API freezes" — **Q3** (handle-yielding iteration/retain), **Q4** (error-type width /
`#[non_exhaustive]`), **Q5** (fallible `SyncRegion` constructors), and **Q13** (try_insert) —
are **consciously NOT implemented** in this round.

This is not "forgotten" — it is a deliberate decision. Empirical verification confirms that
`sefer-region` is not used by the `sefer-alloc` allocator runtime itself (a `grep` search of
the main workspace's `src/` shows no direct calls to `Region`/`Handle`/`SyncRegion` on any hot
path — only re-exports in the workspace root crate's public surface). Without a demonstrated
external consumer, further API investment would be polishing an abstraction with no actual
use case. These findings remain on the record as future work to be revisited only if/when
a real consumer requests the features — not as a "missing" items that block the current
state.
