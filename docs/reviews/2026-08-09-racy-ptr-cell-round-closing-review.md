# `racy-ptr-cell` round-closing review (read-only, end-to-end)

**Date:** 2026-08-09
**Reviewed range:** `0db373d~1..HEAD`, HEAD = `2db9a1533a2101b37496d36e95d985ceebd89311` (`main`)
**Scope:** the 9-commit `racy-ptr-cell` `rust-intel` remediation round — 6 fix/test/doc commits
(`0db373d` #700, `048c657` #706, `9b98c7a` #707, `b65c51a` #708, `2e5fb72` #709, `2af6da3` #710),
the CHANGELOG commit (`11ca6ee` #741) and the two checkpoint-artifact commits (`968690e` #740,
`2db9a15` #742).
**Mode:** read-only. No repository file was modified except this report; `git status --porcelain`
was empty before writing it and is empty again after the scratch cleanup. Eight throwaway
counterfactual probes were built and run in a scratch cargo copy of `crates/racy-ptr-cell` under
`%TEMP%` (workspace-detached, deleted after use) to settle claims that could not be settled by
reading; their verbatim output is inlined below.

**Bottom line:** the round's *code and tests* are correct and — unusually — **every one of the five
counterfactuals that can be mechanically constructed actually reproduces**, independently verified
here rather than trusted from the commit messages. Task #700's happens-before fix is genuine (I
proved both the pre-fix vacuity and the post-fix detection), task #706's `RollbackGuard` is complete
and correctly scoped, task #707's `assert!` promotion is release-active and its test is release-
sensitive, and both of task #708's new tests die when their guard is removed. **No HIGH-severity
defect exists in the crate's source or test logic.**

The round's *delivery* has one real hole, and it is the mirror image of the finding the
`tagged-index-stack` closing review made one day earlier: **`cargo test -p racy-ptr-cell` runs in
zero CI configurations and zero local gate scripts.** Four of the six fixes this round landed are
regression tests that live in `tests/cell_unit.rs`, and not one of them has ever executed in CI —
including the `#707` `assert!` guard, whose regression test is *only* meaningful in a `--release`
run that also does not exist. That is finding **F1** and is the only high-severity item.

---

## 0. Current-state green check (re-run personally, not trusted from commit messages)

| Command | Result |
| --- | --- |
| `cargo test -p racy-ptr-cell` | **7 passed** (`cell_unit`), 0 failed; `loom_racy_ptr_cell` correctly compiles to 0 tests; **0 doctests** (CLAUDE.md's no-doctest rule holds) |
| `RUSTFLAGS="--cfg loom" cargo test -p racy-ptr-cell --test loom_racy_ptr_cell --release` | **7 passed**, 0 failed, in 0.38s |
| `cargo clippy -p racy-ptr-cell --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg loom" cargo clippy -p racy-ptr-cell --tests -- -D warnings` | clean |
| `cargo fmt -p racy-ptr-cell -- --check` | clean (exit 0) |
| `cargo doc -p racy-ptr-cell --no-deps` | **0 warnings** |
| `cargo build -p racy-ptr-cell --no-default-features --target thumbv7em-none-eabi` (CI's own step) | clean — the round's `RollbackGuard`/`assert!` additions did not break the `no_std` bare-metal build |
| `cargo test --features "production internals" --test dbg_hook_safety_tripwire` (root repo) | **7 passed** — `#710`'s `#[doc(hidden)]` removal did not invalidate the root repo's `dbg_*` allowlist tripwire |
| `MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test --test cell_unit` | **6 pass, 1 fails** — see **F2** |

All the numeric claims in the commit messages check out against the current tree.

---

## 1. Commit-by-commit: does each diff match its own message?

All nine commits were read line by line (`git show`) against their messages. **Nine of nine diffs
do what their messages say.** Specific spot-verifications that carried real risk of divergence:

- **`0db373d` (#700)** genuinely moves both marker checks inside the racing closures
  (`tests/loom_racy_ptr_cell.rs:137`, `:189`, `:212`) and genuinely deletes the two post-`join`
  reads. Independently counterfactual-verified in both directions — see §2.1.
- **`048c657` (#706)** lands `RollbackGuard` (`src/lib.rs:182-217`), arms it at `:373`, and defuses
  it on **both** non-unwinding exits (`:408` publish, `:422` OOM). Verified complete: the sentinel
  is held across caller code in exactly one place in the whole crate (`get_or_try_init`'s winner
  branch); `dbg_rollback_reenterable` holds it only across atomic ops that cannot panic, so there
  is no second unguarded window. Counterfactual reproduced — see §2.2.
- **`9b98c7a` (#707)** promotes `debug_assert!`→`assert!` at `src/lib.rs:396`. The check is
  `Self::is_ready(raw)`, and since `raw` comes from a `NonNull`, the only reachable failure is
  `addr == 1` — i.e. it is exactly and only the sentinel-collision guard it claims to be, with the
  `align_of >= 2` construction guard covering the complementary case. Counterfactual reproduced,
  and it is genuinely release-sensitive — see §2.3.
- **`b65c51a` (#708)** adds both tests; both counterfactuals reproduced — see §2.4. The rationale
  for using `default()` rather than `new()` for the align guard is independently confirmed correct:
  `static CELL: RacyPtrCell<u8> = RacyPtrCell::new();` really is a const-eval **compile** error
  (probed; verbatim output in §2.4), so the runtime arm is the only testable one without a banned
  `compile_fail` doctest.
- **`2e5fb72` (#709)** replaces `.addr()`/`as *mut Payload` with
  `expose_provenance()`/`with_exposed_provenance_mut` at `tests/loom_racy_ptr_cell.rs:428`, `:446`,
  `:495-497`. The fix is right and does not weaken the test it edits: with step 4's gating reverted
  in `src/lib.rs`, the post-`#709` test still fails, and for the right reason (§2.5). But the same
  defect class survives untouched one file over — **F2**.
- **`2af6da3` (#710)** removes both `#[doc(hidden)]` attributes, adds two `# Stability` sections,
  adds 10 `// SAFETY:` blocks (the message says 9 — **F7**), reworders the `heap_ptr` comment
  (`tests/loom_racy_ptr_cell.rs:619-624`), softens the module doc's counterfactual-fidelity claim
  (`:47-57`), and appends the review-doc resolution note. All present. One wrong line citation in
  the appended note — **F6**.
- **`11ca6ee` / `968690e` / `2db9a15`** are docs-only (CHANGELOG + two checkpoints) and touch
  nothing else.

**Out-of-scope edits, TODO/placeholder code, half-wired features: none found.** `git diff --stat
0db373d^ 2af6da3` touches exactly five files: the four crate files plus the one
`docs/reviews/2026-08-06-...md` resolution note the task explicitly called for. No Cargo.toml,
no `Cargo.lock`, no version bump, no new dependency. `grep -rn "TODO\|FIXME\|XXX\|unimplemented!\|
todo!\|HACK"` over the whole crate returns nothing. The only `#[allow]` in the crate outside the
crate-level `#![allow(unsafe_code)]` is one pre-existing `#[allow(clippy::never_loop)]`
(`tests/loom_racy_ptr_cell.rs:625`) whose justification comment is accurate.

## 2. Were the six fixes' counterfactuals real? (zero-trust re-verification)

### 2.1 `#700` — happens-before oracle — **CONFIRMED, in both directions**

The commit's claim has two halves and I tested both, because half the claim ("the fix works") is
worthless if the other half ("the old test was vacuous") was never true.

**(a) Post-fix, does the oracle actually detect a broken publish?** Flipped `src/lib.rs:405`
`Ordering::Release` → `Ordering::Relaxed` in a scratch copy, rebuilt, re-ran the loom suite:

```text
running 7 tests
test counterfactual_relaxed_publish_loses_happens_before - should panic ... ok
test real_exactly_once_three_threads ... FAILED
test real_exactly_once_two_threads ... FAILED
test counterfactual_spin_on_ready_livelocks_on_oom_rollback - should panic ... ok
test real_fast_path_reentry_same_pointer ... ok
test real_survives_oom_rollback_two_threads ... ok
test real_probe_rollback_does_not_clobber_concurrent_winner ... ok

---- real_exactly_once_two_threads stdout ----
thread 'real_exactly_once_two_threads' panicked at loom-0.7.2\src\rt\location.rs:115:9:
Causality violation: Concurrent load and mut accesses.

test result: FAILED. 5 passed; 2 failed
```

**(b) Pre-fix, was it really vacuous?** Restored `tests/loom_racy_ptr_cell.rs` to
`git show 0db373d^:...` (the pre-#700 file) while KEEPING the `Relaxed` publish:

```text
running 7 tests
test counterfactual_relaxed_publish_loses_happens_before - should panic ... ok
test counterfactual_spin_on_ready_livelocks_on_oom_rollback - should panic ... ok
test real_exactly_once_three_threads ... ok
test real_fast_path_reentry_same_pointer ... ok
test real_exactly_once_two_threads ... ok
test real_survives_oom_rollback_two_threads ... ok
test real_probe_rollback_does_not_clobber_concurrent_winner ... ok

test result: ok. 7 passed; 0 failed
```

Fully green on a broken publish. The audit's HIGH finding was real, the fix closes it, and the fix
is the *only* thing closing it — the two shipped `#[should_panic]` counterfactuals stayed green in
both configurations, which is expected (they are shadow models over their own `AtomicPtr`, not the
crate's) but worth stating: **they are not what protects `src/lib.rs:405`; these two real-type
tests are.**

One nuance the commit message and the test comments do not name — see **F9**: the failure is
loom's causality checker, not the test's own `assert_eq!`.

### 2.2 `#706` — `RollbackGuard` — **CONFIRMED, and the fix is complete**

Short-circuited the guard by constructing it already-defused (`defused: true` in
`RollbackGuard::new`, `src/lib.rs:192`) — the minimal change that removes the fix without removing
the type:

```text
running 7 tests
test panicking_init_rolls_back_and_subsequent_call_succeeds ... FAILED

thread '...' panicked at tests\cell_unit.rs:75:13: simulated init panic
thread '...' panicked at tests\cell_unit.rs:105:10:
get_or_try_init after a panicking init did not return within 5s -- the INITIALIZING sentinel
is stuck forever (task #706's livelock is back): Timeout

test result: FAILED. 6 passed; 1 failed; finished in 5.01s
```

**Completeness, reasoned through rather than assumed.** I looked specifically for a way to defeat
the guard:

- *A second unguarded sentinel-holding window?* None. `get_or_try_init` is the only place the
  sentinel is held across code the cell does not control. `dbg_rollback_reenterable` holds it
  across three atomic ops only.
- *An exit path that leaves the guard armed when it should not be?* No. Both `Some`/`None` arms
  defuse, and on the `Some` arm the `defuse()` is unconditionally sequenced after the publish
  `store` with nothing fallible in between.
- *An exit path that defuses when it should not?* No. The `assert!` at `:396` fires **before**
  `guard.defuse()` at `:408`, so a sentinel-returning init unwinds through an ARMED guard — which
  is exactly what `#707`'s test relies on and what the `#707` commit message claims.
- *`panic = "abort"` / `panic_immediate_abort`?* The guard silently does nothing, but the process
  is dying anyway; not a hole.
- *Ordering?* `Release` on the rollback store is the same ordering the explicit OOM path uses and
  pairs correctly with a re-racing thread's CAS `Acquire`. There is no partially-initialised state
  to publish (init never published), so nothing stronger is needed.

The one thing the fix does *not* have is a test for the multi-threaded half of its own stated defect
— see **F10**.

### 2.3 `#707` — `assert!` promotion — **CONFIRMED, and release-sensitive**

Reverted to `debug_assert!` and ran under `--release`:

```text
running 7 tests
test init_returning_the_sentinel_address_panics - should panic ... FAILED
---- init_returning_the_sentinel_address_panics stdout ----
note: test did not panic as expected at tests\cell_unit.rs:125:4
test result: FAILED. 6 passed; 1 failed
```

Under a plain debug `cargo test` the same reverted source stays **green** (the `debug_assert!`
still fires). So this test is a real regression guard **only in a release build** — which is the
entire point of the promotion, and which nothing in CI ever runs. That half of the gap is folded
into **F1**.

### 2.4 `#708` — two zero-coverage contracts — **BOTH CONFIRMED**

(1) Deleted both `align_of::<T>() >= 2` asserts from `src/lib.rs`:

```text
---- align_of_one_payload_panics_at_construction stdout ----
note: test did not panic as expected at tests\cell_unit.rs:166:4
test result: FAILED. 6 passed; 1 failed
```

(2) Stubbed `dbg_rollback_reenterable` to `return None` unconditionally:

```text
test dbg_rollback_reenterable_happy_path_and_not_applicable_arm ... FAILED
test result: FAILED. 6 passed; 1 failed
```

The test's own justification for choosing `default()` over `new()` was also independently checked
rather than taken on faith. Probe (`static CELL: RacyPtrCell<u8> = RacyPtrCell::new();` in a scratch
example):

```text
error[E0080]: evaluation panicked: RacyPtrCell<T> requires align_of::<T>() >= 2 so the
INITIALIZING sentinel (address 1) can never collide with a real published pointer
 --> examples\consteval_probe.rs:2:32
```

Confirmed: the `const fn` route really is a compile error, so `default()` really is the only
runtime-testable arm, exactly as the test comment and CHANGELOG bullet claim.

### 2.5 `#709` — provenance round-trip — **fix is correct, but incomplete across the round (F2)**

The edited test remains non-vacuous after the change. Reverting `src/lib.rs`'s step-4 gating to an
unconditional `store(null)` (i.e. reintroducing the `#636` clobber bug) still makes it fail, and
for the right reason:

```text
test real_probe_rollback_does_not_clobber_concurrent_winner ... FAILED
thread '...' panicked at tests\loom_racy_ptr_cell.rs:457:9:
assertion `left == right` failed: exactly ONE real caller must run init despite the concurrent
probe (got 2)
  left: 2
 right: 1
```

The API choice is right too (`expose_provenance` / `with_exposed_provenance_mut` are exactly the
documented pair for this situation, both stable since 1.84, comfortably inside the crate's
`rust-version = "1.88"`). But the identical pattern was introduced by this round's own `048c657`
into `tests/cell_unit.rs` and never revisited — **F2**.

### 2.6 `#710` — API posture + SAFETY tags — **decision is defensible; three residuals**

The `#[doc(hidden)]`-plus-"call this from your tests" contradiction is real and the resolution is a
legitimate way out. The tripwire in the root repo still passes, so the promotion did not break the
project's own `dbg_*` policing. Three residuals follow as **F4** (`dbg_is_ready` is a strict
duplicate of `get().is_some()`, and the recorded rationale never engages CLAUDE.md's own
benchmark-hook rule that the earlier review explicitly invoked), **F6** (wrong line citation in the
appended resolution note) and **F7** (SAFETY-tag count).

I independently re-swept every `unsafe` site in the crate. **Every `unsafe` block in `src/lib.rs`,
`tests/cell_unit.rs`, and `tests/loom_racy_ptr_cell.rs` is preceded by a substantive `// SAFETY:`
comment**, and each one names the actual invariant rather than filler — the three
`NonNull::new_unchecked` sites each cite the specific preceding non-null proof (`is_ready(p)`, or
the `a != 0` + fall-through-from-sentinel pair at `:446-450`), and every reclaim site names the
matching `Box::leak`/`Box::into_raw` and the exactly-once/after-join condition. The one site that is
not a `// SAFETY:` line is **F11** (informational).

## 3. Findings

### F1 — HIGH — `cargo test -p racy-ptr-cell` runs in ZERO CI configurations; four of this round's six fixes are regression tests that have never executed in CI

**`.github/workflows/ci.yml`** — the complete set of CI invocations of this crate is:

| Line | Invocation | What it covers |
| --- | --- | --- |
| `:765` | `cargo build -p racy-ptr-cell --no-default-features --target thumbv7em-none-eabi` | library only — `cargo build` compiles no test target |
| `:1143-1144` | `cargo test --release -p racy-ptr-cell --test loom_racy_ptr_cell` under `RUSTFLAGS: "--cfg loom"` | the loom suite only |

`tests/cell_unit.rs` is excluded from `:1143` **twice over** — by `--test loom_racy_ptr_cell`
target selection, and by its own `#![cfg(not(loom))]` gate (`:6`). There is no `cargo test
--workspace` step anywhere (`test-workspace` at `:683-799` enumerates crates explicitly and this
crate is not among the `cargo test` steps), and no local gate covers it either: `npm run check` →
`scripts/check-all.mjs` is root-crate-scoped, and `npm run loom` → `scripts/loom.mjs:29` runs only
`--test loom_racy_ptr_cell`.

**Consequence, concretely.** All seven `cell_unit.rs` tests have never run in CI, including the four
this round created:

- `panicking_init_rolls_back_and_subsequent_call_succeeds` (#706) — the ONLY test of the
  `RollbackGuard`. Delete the guard and CI stays fully green.
- `init_returning_the_sentinel_address_panics` (#707) — and this one is worse: even if the file
  were added to a debug `cargo test` step, it would still pass with `assert!` reverted to
  `debug_assert!` (§2.3). It needs a `--release` step specifically. Revert `src/lib.rs:396` today
  and *nothing anywhere* goes red.
- `align_of_one_payload_panics_at_construction` (#708) — delete both asserts, CI stays green.
- `dbg_rollback_reenterable_happy_path_and_not_applicable_arm` (#708) — stub the probe to `None`,
  CI stays green.

This is exactly the gap class task **#639** closed for `tagged-index-stack` (ci.yml `:709-724`
documents the identical situation verbatim: "the ONLY existing CI invocation of this crate anywhere
in this workflow is ... `--test loom_aba` ... which excludes both files") and that task **#772**'s
F4/F5 closed one day earlier by adding `cargo test -p tagged-index-stack --release` at `:735`.
`racy-ptr-cell` was left out of both passes, and this round then landed four safety-critical
regression tests into the uncovered file without noticing.

**Suggested close (for a follow-up task, not applied here):** add two steps to `test-workspace`,
next to the existing `racy-ptr-cell` bare-metal build at `:765` —
`cargo test -p racy-ptr-cell --no-fail-fast` and `cargo test -p racy-ptr-cell --release
--no-fail-fast`. The suite runs in well under a second; the `--release` step is not optional here,
it is the only thing that makes `#707`'s guard a checked invariant rather than a comment.

### F2 — MEDIUM — the exact provenance defect `#709` fixed is still live in `tests/cell_unit.rs`, introduced by this round's own `048c657`

**`crates/racy-ptr-cell/tests/cell_unit.rs:99`** (`.map(|p| p.as_ptr() as usize)`) and **`:111`**
(`NonNull::new(addr as *mut Payload)`), whose reconstructed pointer is then deallocated at
**`:120`** (`drop(Box::from_raw(p.as_ptr()))`).

This is the same `usize`-across-a-thread-boundary-then-`Box::from_raw` shape that task **#709**
(`2e5fb72`, one round-commit later) removed from `tests/loom_racy_ptr_cell.rs` for exactly this
reason, and that the crate's own module docs hold themselves to (`src/lib.rs:56-58`: the sentinel is
"Constructed via `core::ptr::without_provenance_mut` … strict-provenance-clean"). It was introduced
by **this round's** `048c657` (#706), which created the file's new test, and #709 two commits later
fixed only the sibling file.

**Verified, not inferred.** Under `-Zmiri-strict-provenance` the crate's other six tests pass and
this one hard-errors:

```text
test panicking_init_rolls_back_and_subsequent_call_succeeds ... error: unsupported operation:
integer-to-pointer casts and `ptr::with_exposed_provenance` are not supported with
`-Zmiri-strict-provenance`
   --> tests\cell_unit.rs:111:26
    |
111 |     let p = NonNull::new(addr as *mut Payload).unwrap();
    |                          ^^^^^^^^^^^^^^^^^^^^ unsupported operation occurred here
```

Under default (permissive) Miri it passes with a warning, because `p.as_ptr() as usize` at `:99` is
itself an *exposing* cast — so, unlike the loom-file case #709 fixed (which used `.addr()`, which
does **not** expose), this one is **not UB** under the exposed-provenance model. That is why this is
MEDIUM and not HIGH. It is nonetheless: (a) a violation of the discipline the same round declared
and the crate advertises; (b) a hard blocker for ever running this crate's tests under strict-
provenance Miri; and (c) unlike the loom file, this one **is** miri-runnable, so it is the cheaper
of the two to prove — the CHANGELOG's "#709 is **Not miri-verified**" caveat could have been
converted into a real miri check here for free.

**Suggested close:** `p.as_ptr().expose_provenance()` at `:99` and
`core::ptr::with_exposed_provenance_mut::<Payload>(addr)` at `:111`, mirroring `#709`'s own fix
verbatim; then the whole `cell_unit` suite passes under `-Zmiri-strict-provenance` (the other six
already do).

### F3 — MEDIUM — `get_or_try_init` gained a release-active panic and an unwind contract this round; neither appears in any user-facing doc

**`crates/racy-ptr-cell/src/lib.rs:310-338`** — `get_or_try_init`'s rustdoc has a "Contract" list
and no `# Panics` section, and did not gain one from either `#707` or `#706`. As shipped, a
downstream reader of docs.rs cannot learn either of the two facts this round created:

1. **It panics.** `assert!(Self::is_ready(raw), "RacyPtrCell: init returned the null/sentinel
   address")` (`:396`) is release-active as of `9b98c7a` and is reachable from **100% safe caller
   code** — the audit's own framing. It is documented only in an inline `//` comment (`:377-395`),
   invisible in rustdoc. `RacyPtrCell::new` has a `# Panics` section for its own weaker guard; the
   method with the newer, more reachable panic does not.
2. **What happens if `init` unwinds.** `#706`'s whole contribution is a *contract*: a panicking init
   propagates AND leaves the cell in `UNINIT`, so a later call may retry. That contract is written
   up beautifully — on `RollbackGuard` (`:168-181`), a **private** struct, which rustdoc does not
   render. `OnceLock::get_or_init` documents its poisoning behaviour; this cell's rollback-instead-
   of-poison behaviour on the *panic* path is a genuine differentiator (the crate-level docs and
   README already sell the rollback story for the `None`/OOM path) and it is nowhere a user can see
   it.

**Concrete scenario:** a `#[global_allocator]` bootstrap author — the crate's stated primary
audience, in the one context where an unexpected panic is least affordable — reads the full rustdoc
for `get_or_try_init`, concludes it cannot panic, and writes no `catch_unwind`/abort strategy around
it. Neither `#![deny(missing_docs)]` nor default clippy flags this (`clippy::missing_panics_doc` is
pedantic and not enabled). This is worth closing before the first publish, since it is a
documentation of *behaviour that changed this round*.

### F4 — MEDIUM-LOW — `#710` permanently semver-committed `dbg_is_ready`, which is a strict duplicate of `get().is_some()`, on a rationale that names a distinction from `get` that does not exist

**`crates/racy-ptr-cell/src/lib.rs:482-486`** vs **`:297-308`**. Textually:

```rust
pub fn get(&self) -> Option<NonNull<T>> {
    let p = self.ptr.load(Ordering::Acquire);
    if Self::is_ready(p) { Some(unsafe { NonNull::new_unchecked(p) }) } else { None }
}
pub fn dbg_is_ready(&self) -> bool {
    Self::is_ready(self.ptr.load(Ordering::Acquire))
}
```

`dbg_is_ready(&self)` is exactly `self.get().is_some()` — same load, same ordering, same predicate,
no capability `get` lacks. Its doc (`:464-469`) justifies its separate existence as letting a test
"assert lazy-materialisation ordering **without racing a concurrent init**" — but `get` does not
race a concurrent init either; it is the same single `Acquire` load with no CAS and no spin. The
stated distinction is not real.

The `# Stability` rationale that #710 attached to it (`:471-481`) is entirely inherited from
`dbg_rollback_reenterable`'s case — "it would advertise this function to downstream consumers' tests
while hiding it from the rustdoc". That argument is sound for `dbg_rollback_reenterable`, which
genuinely has no non-`dbg_` equivalent. It does not transfer to `dbg_is_ready`, whose downstream
consumers can simply call `get().is_some()`. The round therefore turned a redundant probe into a
permanent public-API commitment on a borrowed justification.

Two secondary observations on the same decision, neither by itself a finding:

- The recorded rejection of the feature-flag alternative (README `:62-68`, and the commit body)
  says feature-gating "would require restructuring the whole existing test suite behind an opt-in
  flag". That overstates it — `[[test]] required-features` exists, and the honest part of the cost
  is the one also stated ("a corresponding CI matrix addition"). Given **F1**, the CI matrix for
  this crate needs an addition regardless.
- Neither the README section, the commit message, nor the CHANGELOG bullet engages CLAUDE.md's
  benchmark-hook rule (2) — "any hook with no production caller MUST default to gating behind
  `bench-internals`" — even though the 2026-08-06 review's suggested fix 3 cited that rule by name
  as its argument. The decision to reject it is defensible (this hook takes no raw pointer, cannot
  cause UB, and the root repo's `tests/dbg_hook_safety_tripwire.rs` already allowlists both hooks
  with reviewed justifications — re-verified green above), but a publish-blocking API decision that
  overrides a standing project rule should say so and say why, not go unmentioned.

### F5 — MEDIUM — the round flagged open items but added nothing to `docs/CORRECTNESS_OPEN_ITEMS.md`

`git log --oneline -1 -- docs/CORRECTNESS_OPEN_ITEMS.md` is `57c4510`, which predates every commit
in this round; `docs/perf/OPEN_ITEMS.md` mentions this crate nowhere. Yet the round explicitly
flagged at least one open caveat and left one real gap:

- `CHANGELOG.md:216` states, in bold, that `#709` is "**Not miri-verified**". That is an
  acknowledged, deliberate coverage caveat on a soundness-adjacent fix. It exists in neither index.
- **F1**'s CI gap (`cargo test -p racy-ptr-cell` in zero configurations) exists in neither index,
  and the round's own four new tests landed into it.

CLAUDE.md's standing rule is explicit: "When a gate report / commit / review newly flags an open
item, add it to the appropriate index in the same commit", with `docs/CORRECTNESS_OPEN_ITEMS.md`
scoped to exactly "correctness bugs, flaky tests, and CI-coverage gaps flagged from ANY source
(commit messages, code comments, reviews)". The rule's own stated rationale applies literally here —
the in-session TaskList does not survive a session boundary, and this sweep is explicitly
multi-session (tasks #697-731 are queued for later rounds).

Mitigating, and stated for fairness: this is a sweep-wide pattern, not unique to this round — the
`tagged-index-stack` round immediately before it did the same, and its closing review's F4/F5 items
were filed as TaskList entries (#771/#772) rather than index entries. Rated MEDIUM as a process
finding rather than higher because the substantive items are all captured in this report and its
follow-up table.

### F6 — LOW — the appended resolution note cites a line range that is wrong at every revision

**`docs/reviews/2026-08-06-racy-ptr-cell-publish-readiness-review.md`** (the note appended by
`2af6da3`) states the fix "landed in `src/lib.rs:471-474` (`postcondition_holds` gates the restore
store)". Checked against three revisions:

| Revision | Actual location of the `postcondition_holds` gate | What is at `:471-474` |
| --- | --- | --- |
| `17f5693` (the fix commit) | `:444-446` | (file is shorter than 471 lines) |
| `2e5fb72` (the note's parent) | `:553-555` | tail of `dbg_is_ready` |
| `2af6da3` / current | `:578-580` | `dbg_is_ready`'s `# Stability` doc |

The citation is not merely stale-by-a-refactor; it does not correspond to the fix at any point in
the file's history. Consequence: a reader following the note to verify the closure lands in the
middle of an unrelated doc comment. The substance of the note is otherwise correct and independently
re-verified here — fix 1 did land, the regression test does exist and is non-vacuous (§2.5), and
fix 2's second half (the mirrored justification string in the root repo's
`tests/dbg_hook_safety_tripwire.rs:284-285`) was in fact corrected in `17f5693`, so the note's "no
further action needed" is substantively true.

### F7 — LOW — the SAFETY-tag count is 10, not 9, and the CHANGELOG's own breakdown is internally inconsistent

`git show 2af6da3 | grep -c '^+.*SAFETY'` → **10** added blocks, 0 removed: 1 in `src/lib.rs`
(`:302`), 4 in `tests/cell_unit.rs` (`:57`, `:117`, `:158`, `:219`), 5 in
`tests/loom_racy_ptr_cell.rs` (`:160`, `:228`, `:278`, `:356`, `:599`).

- The commit **subject** and the task title say "9".
- The commit **body**'s own enumeration reads "src/lib.rs:302 … 4 reclaim_payload call sites …
  plus one Box::from_raw call site … and 4 reclaim sites in tests/cell_unit.rs" — which sums to 10
  under a "9" headline.
- `CHANGELOG.md:217` says "all 9 … (1 in `src/lib.rs`, **8** across `tests/cell_unit.rs`/
  `tests/loom_racy_ptr_cell.rs`)" — the actual split is 1 + 9.

The "9" traces to the audit's own count at audit time
(`docs/reviews/2026-08-07-racy-ptr-cell-rust-intel-audit.md`: 1 + 2 + 6 = 9); by the time #710 ran,
`#706`/`#708` had added two more untagged sites and `#700` had already tagged one of the six. So
nothing was *missed* — the round closed a superset — but three published statements disagree with
each other and with the diff.

### F8 — LOW — the README still says the `#[should_panic]` counterfactuals run against the real type; `#710` corrected exactly this overclaim in the test module doc and did not propagate it

**`crates/racy-ptr-cell/README.md:37-40`**: *"Both rules are pinned by executable loom proofs that
run against the real `RacyPtrCell` type … including `#[should_panic]` counterfactuals that fail
without the correct code."*

The counterfactuals do **not** run against the real type. `tests/loom_racy_ptr_cell.rs:36-38` says so
directly ("Loom cannot rebuild the crate with a deliberately-broken ordering, so the two broken
protocols are transcribed here as `#[should_panic]` models"), and `#710` went further and softened
that same doc for counterfactual B's `AtomicU8` encoding (`:47-57`) — while leaving the README's
stronger version of the same claim untouched. `Cargo.toml:7`'s `description`, which ships immutably
to crates.io on first publish, has the milder phrasing ("loom proofs run against the real type, with
`#[should_panic]` counterfactuals") and is defensible as written.

**Important qualification — the substance survives, and I checked rather than assumed it.** Both
headline rules genuinely ARE pinned against the real type, by the non-counterfactual tests:

- *Rule 1 (Release publish)*: `real_exactly_once_two_threads` / `_three_threads` — §2.1(a).
- *Rule 2 (`spin while == INITIALIZING`)*: `real_survives_oom_rollback_two_threads`. Verified
  directly by a counterfactual nothing in this round exercised — rewriting the real loser rule at
  `src/lib.rs:439` from `a == SENTINEL_INITIALIZING` to `!Self::is_ready(p)` (i.e. spin while
  `!= READY`):

  ```text
  test real_survives_oom_rollback_two_threads ... FAILED
  thread '...' panicked at loom-0.7.2\src\rt\path.rs:175:9:
  Model exceeded maximum number of branches. This is often caused by an algorithm requiring the
  processor to make progress, e.g. spin locks.
  test result: FAILED. 6 passed; 1 failed
  ```

  Both rules are therefore load-bearing and proven against the real type. Only the README's
  attribution of that proof to the *counterfactuals* is loose.

### F9 — LOW — `#700`'s new oracle cannot fail on value grounds; the real detector is loom's causality checker, and neither the comment nor the assertion message says so

**`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:137-141`, `:189-193`, `:212-216`** — each new
check reads `init_marker` and asserts `== 0xDEAD_BEEF` with the message *"loser must see the fully
constructed pointee (Release/Acquire pair)"*.

In the real-type tests the payload is built as `AtomicU32::new(0xDEAD_BEEF)` inside
`make_payload()` (`:92-95`) — the marker is set at construction and `0xDEAD_BEEF` is the only value
ever stored. There is no zero-then-store sequence, so **no interleaving can make this `assert_eq!`
observe a different value**. What actually fires under a broken publish is loom's
`"Causality violation: Concurrent load and mut accesses"` (§2.1(a)) — the *access itself*, performed
at a point with no join-established happens-before, is the detector; the assertion is only the
vehicle that forces the cross-thread read to happen there.

This does not make the fix wrong — it is genuinely non-vacuous, proven both ways — but it makes the
shipped explanation inaccurate in a way that matters for maintenance: a future editor who "cleans
up" the assertion into something that does not dereference the pointer (e.g. comparing pointer
values instead) would silently destroy the detection while the test still reads as if it checks the
same thing. The shadow-model counterfactual's own doc (`:574-581`) *does* explain this correctly
("Loom flags the racing access before our own `assert_eq!` on the stale value can even run"); the
three real-type sites do not, and the CHANGELOG bullet (`:212`) reports the causality-violation
message without noting that the assertion is not what produced it.

### F10 — LOW — `#706`'s guard is tested single-threaded only; the concurrently-spinning-loser scenario its own doc names as the defect has no test

`RollbackGuard`'s doc (`src/lib.rs:168-181`) states the defect as *"every concurrent loser
busy-spins at 100% CPU indefinitely (they spin on `== INITIALIZING`, which never changes)"*. The one
test (`tests/cell_unit.rs:63-121`) exercises a strictly weaker property: a **subsequent** call on a
quiescent cell succeeds. A loser that was *already inside the spin loop* when the winner unwound is
never modelled, and the loom suite has no panicking-init scenario at all (loom + `catch_unwind` is
awkward, which is a fair reason it was skipped).

The two properties do coincide here — both reduce to "the cell left `INITIALIZING`" — so this is a
coverage note, not a hole in the fix. Concrete scenario where it would matter: a future change that
made the rollback conditional (e.g. only rolling back when no loser is waiting, or storing some
third state) would keep the single-threaded test green while reintroducing the spin.

### F11 — INFORMATIONAL — `reclaim_payload`'s safety justification is prose inside a `///` doc, not a `# Safety` section or a `// SAFETY:` line

**`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:98-103`** — `unsafe fn reclaim_payload` carries
*"/// Reclaim a leaked payload (loom leak-check hygiene). SAFETY: `p` came from `make_payload`'s
`Box::leak` …"*. The content is correct and sufficient; the form is neither of the two the crate's
own header promises (`src/lib.rs:85-86`: "Every `unsafe fn` / `unsafe impl` carries a `# Safety` /
`// SAFETY:` justification"). Notably, the mechanical check `2af6da3`'s message claims to have run
("a manual grep sweep confirms every `unsafe` block … is now preceded by a `// SAFETY:` comment")
would not have confirmed this one: the line immediately above `:101` is `/// after all threads
joined.`, which contains no `SAFETY` token. `clippy::missing_safety_doc` does not fire because the
item is not public. Cost to fix: promote the sentence to a `/// # Safety` section.

### F12 — INFORMATIONAL — the probe-vs-winner test's same-pointer oracle is conditionally skipped

**`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:471-476`** — `if seen.len() == 2 { assert_eq!(
seen[0], seen[1], …) }`. In this model `seen.len()` is **always** 2 by construction: both real
callers return `Some` on every legal schedule (`get_or_try_init` returns `None` only when the
winner's `init` returns `None`, which no closure here ever does), so the guard can never be false.
An unconditional `assert_eq!(seen.len(), 2)` before the comparison would be strictly stronger at
zero cost and would turn a silent skip into a signal if a future edit made a caller able to return
`None`. Same one-sidedness class as F7 in the `tagged-index-stack` closing review; flagged for
symmetry, not because it is currently wrong.

### F13 — INFORMATIONAL — "each of the 6 tasks individually zero-trust re-verified with a genuine counterfactual" is not literally true for two of them

**`CHANGELOG.md:210`**. Five of the six do have a real counterfactual and all five reproduce (§2.1-
2.5 — I re-ran every one). But `#709` has none by construction (its own bullet at `:216` says
"**Not miri-verified**", and its verification was "re-ran the loom suite, still green", which is a
no-regression check, not a counterfactual), and `#710` is a docs/attribute change for which no
counterfactual exists. The blanket "each of the 6" overstates by two. The individual bullets are
each honest about what they actually did; only the section header generalises.

---

## 4. Category-by-category verdicts on the questions asked

### 4.1 Are the six fixes correct, complete, and non-vacuous? — **yes, all six**

Summarised in §2. No fix is locally-plausible-but-incomplete; the one completeness question worth
asking (does `#706` leave a second unguarded window?) was chased explicitly and the answer is no.

### 4.2 Is there any new safe `pub fn` touching pointer metadata without `unsafe` + SAFETY doc? — **no**

The round added no new `pub fn` at all. It *un-hid* two existing ones. Neither accepts a raw
pointer; `dbg_is_ready` is a pure `Acquire` load; `dbg_rollback_reenterable` mutates only its own
`AtomicPtr` and cannot cause UB (its clobber hazard was closed in `17f5693` and is loom-pinned,
re-verified in §2.5). CLAUDE.md's benchmark-hook rule (1) — "safe `pub fn` that accepts a raw
pointer and touches allocator metadata" — does not engage. Rule (2)'s gating requirement is
satisfied through the root repo's reviewed allowlist (`tests/dbg_hook_safety_tripwire.rs:166`,
`:284`), which still passes; the fact that the decision to override it went unmentioned is F4's
second bullet.

### 4.3 Doc / README / CHANGELOG accuracy versus the code as it now stands

- **README** — the new "Test-probe API stability" section (`:46-73`) accurately describes what the
  code now does (both methods non-hidden, both with `# Stability` sections — verified present at
  `src/lib.rs:471-481` and `:526-536`). The pre-existing loom-proof paragraph is loose (**F8**).
- **`src/lib.rs`** — the crate header's `unsafe` inventory (`:71-86`) is accurate at current HEAD: 2
  `unsafe impl` + 3 `NonNull::new_unchecked` sites, no `unsafe fn`, matching the grep. The
  `# Panics` gap on `get_or_try_init` is **F3**.
- **CHANGELOG** — all six commit SHAs cited at `:212-217` are correct and in landing order, each
  bullet's substantive description matches its diff, and the "**Runtime improvements: 0**" header is
  honest under the R30-12 taxonomy (`#707` does change release-mode observable behaviour — panic
  instead of silent wedge — but the header's own hedge, "no shipping algorithm changed observable
  behavior on any *success* path", is precise, and publishing the sentinel was never a success path
  by the method's own documented contract). Defects: the SAFETY count (**F7**) and the
  "each of the 6 … counterfactual" generalisation (**F13**).
- **Commit prefixes** — `test(...)` ×3, `fix(...)` ×2, `docs(...)` ×1. Correct under R30-12: the two
  `fix(...)` commits genuinely changed shipping code for correctness with no speedup claimed, and
  the taxonomy's `perf(...)` family correctly does not appear.

### 4.4 The `#710` API-posture decision, judged on its merits

Splitting the two methods rather than treating them as one decision:

- **`dbg_rollback_reenterable` — the promotion is right.** It has no non-`dbg_` equivalent, its doc
  genuinely invites external use, and `#[doc(hidden)]` + "call this from your tests" is a real
  contradiction. Its safety story is sound (§4.2) and now loom-pinned.
- **`dbg_is_ready` — the promotion is the weak half.** F4.

The rejected alternative (feature-gating) was rejected on a partly-overstated cost, and the standing
project rule that the earlier review invoked went unaddressed. None of this is a defect in shipped
behaviour; it is a publish-blocking decision recorded with a thinner justification than its
significance warrants, and `Cargo.toml`'s `description`/the public surface are immutable per
published version.

### 4.5 What a pre-publish `/rust-intel` audit would still flag (task #659)

Ranked by what actually blocks a first publish:

1. **F1** — four safety-critical regression tests with zero CI coverage. Everything else on this
   list is cheaper to fix than this one is to leave.
2. **F3** — a published `get_or_try_init` whose rustdoc does not mention that it panics.
3. **F4** — a permanent public-API commitment (`dbg_is_ready`) that duplicates `get().is_some()`;
   the window to reconsider closes at first publish.
4. **F2** — strict-provenance uncleanliness in a crate that advertises strict-provenance discipline.
5. **F8** — a README claim that overstates what the counterfactuals prove.
6. Pre-existing, not a regression from this round: the crate name and the 383-character
   `description` are one-way doors already filed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 28; the
   `loom = "0.7"` triplication is task #711.
7. Also pre-existing: there is no plain multi-threaded (non-loom) stress test of `get_or_try_init`
   on real hardware. Low value given loom's coverage of the actual hazard, but typically named.

---

## 5. Categories with nothing to report

Stated explicitly rather than omitted:

- **Out-of-scope edits:** none. Every hunk in the six fix commits is inside
  `crates/racy-ptr-cell/`, except the one `docs/reviews/` resolution note the task called for.
- **TODO / placeholder / half-wired features:** none.
- **Version bumps / dependency changes:** none. `Cargo.toml` and `Cargo.lock` are untouched by the
  entire round.
- **New `unsafe` surface:** none. No new `unsafe fn`, no new `unsafe impl`, no new `unsafe` block in
  `src/`. The crate's `unsafe` inventory is unchanged from before the round.
- **SAFETY-tag substance:** all 10 added tags name a real invariant (the specific non-null proof, or
  the specific `Box::leak` they pair with and the exactly-once/after-join condition) — none is
  filler. Every `unsafe` block in `src/` and `tests/` is now covered; F11 is a form nit on the one
  `unsafe fn`.
- **`no_std` posture:** intact — the CI bare-metal cross-build was re-run here and is clean; the
  added `assert!`s carry static messages with no format arguments, so they lower to
  `core::panicking::panic` and pull in no formatting machinery.
- **Doctests:** zero, as CLAUDE.md requires. Both the README and `src/lib.rs` use ```text fences.
- **Root-repo consumers:** `src/registry/bootstrap.rs:747`/`:949` still compile and the root
  `dbg_hook_safety_tripwire` suite is green — the `#[doc(hidden)]` removal broke nothing upstream.
- **Clippy / fmt / rustdoc warnings:** clean in all four configurations re-run in §0.
- **Commit-message claims vs. diffs:** every self-reported counterfactual in every commit message
  was independently re-run here, not read. All five that are mechanically constructible reproduce.

---

## 6. Suggested follow-up tasks

| Priority | Finding | One-line action |
| --- | --- | --- |
| P1 | F1 | Add `cargo test -p racy-ptr-cell --no-fail-fast` **and** `cargo test -p racy-ptr-cell --release --no-fail-fast` to `ci.yml`'s `test-workspace` job (next to `:765`); the `--release` step is what makes `#707`'s `assert!` a checked invariant. |
| P1 | F3 | Add a `# Panics` section to `get_or_try_init` covering both the sentinel/null `assert!` and the unwind-propagates-and-rolls-back contract; the `RollbackGuard` doc's content is already written, it just lives on a private item. |
| P2 | F2 | Apply `#709`'s own fix to `tests/cell_unit.rs:99`/`:111` (`expose_provenance` / `with_exposed_provenance_mut`), then the whole `cell_unit` suite passes under `-Zmiri-strict-provenance`. |
| P2 | F4 | Decide `dbg_is_ready`'s fate before first publish: either drop it (callers use `get().is_some()`) or correct its doc to stop claiming a distinction from `get` that does not exist; record the CLAUDE.md benchmark-hook-rule override explicitly either way. |
| P2 | F5 | File F1 (and the `#709` "not miri-verified" caveat, which F2 closes) into `docs/CORRECTNESS_OPEN_ITEMS.md` so they survive a session boundary. |
| P3 | F8 | Reword `README.md:37-40` to attribute the real-type proof to the real-type tests and the counterfactuals to the shadow models — the §F8 evidence gives the exact true statement. |
| P3 | F6 + F7 | Correct the resolution note's `src/lib.rs:471-474` citation to `:578-580`; reconcile the 9-vs-10 SAFETY count in `CHANGELOG.md:217`. |
| P4 | F9 + F10 + F11 + F12 + F13 | Comment/oracle hygiene: explain that loom's causality checker (not the `assert_eq!`) is the detector at the three real-type sites; note the untested concurrent-loser arm; promote `reclaim_payload`'s prose to `# Safety`; make `assert_eq!(seen.len(), 2)` unconditional; soften the CHANGELOG's "each of the 6". |
