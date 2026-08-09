# `racy-ptr-cell` — publish-readiness review (read-only)

**Date:** 2026-08-06
**Scope:** `crates/racy-ptr-cell` @ 0.1.0, standalone crates.io publish readiness
ahead of tagging root `sefer-alloc` 0.3.0.
**Measured tree:** `HEAD` = `2a1ca35ae7ba78c286beaf3acff4b8210fa9766f`
(read back from cargo's own
`D:/dev/rust/.cargo-target/package/racy-ptr-cell-0.1.0/.cargo_vcs_info.json`,
produced by the `cargo package` run in §4). Working tree dirty only in
untracked `docs/reviews/*` + `.claude/` — no tracked file under
`crates/racy-ptr-cell/` differs from `HEAD`.
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)`, host
`x86_64-pc-windows-msvc`. MSRV cross-check on `1.88-x86_64-pc-windows-msvc`.
**Nothing was edited, staged, committed, or published.** No real
`cargo publish` was run.

---

## Verdict: **GO-WITH-FIXES** — publish it, after 1 medium + 3 low fixes

Three separate questions were asked; they have three different answers, and it
matters that they are not collapsed:

- **"Is this worth publishing on its own merits?"** — Marginal. Honest read
  below in §1: the independent audience is small.
- **"Is it technically ready?"** — Yes. Everything is green (§3, §4), the
  metadata is complete with no placeholders (§2), doc coverage is 100% (§6),
  and there is zero incomplete work in the source (§5).
- **"Do we have a choice?"** — **No.** This is the decisive finding, and it
  overrides the first question: `publish = false` is **not a viable option**
  for this crate while `sefer-alloc` ships to crates.io in its current shape.
  See §1.1.

So: **publish `racy-ptr-cell` 0.1.0 before tagging `sefer-alloc-v0.3.0`**, with
the §5.1 fix applied first (it is a genuine, if narrow, invariant hole that
becomes a *public* invariant hole the moment the crate is on crates.io).

---

## 1. Should this be a standalone published crate?

### 1.1 The dependency graph has already decided this (decision-forcing)

Root `Cargo.toml:875`:

```toml
racy-ptr-cell = { path = "crates/racy-ptr-cell", version = "0.1", optional = true }
```

and `Cargo.toml:148`:

```toml
alloc-core = ["std", "dep:aligned-vmem", "dep:racy-ptr-cell", "dep:size-classes"]
```

with `alloc-global = ["alloc-core", ...]` (`Cargo.toml:179`) and
`production = ["alloc-global", ...]` (`Cargo.toml:407`).

A path dependency that also carries a `version` key is rewritten to a **registry**
dependency at publish time, and crates.io rejects an upload whose dependencies —
including *optional* ones, which are still resolved into the lockfile — do not
exist in the index. So:

- Marking `racy-ptr-cell` `publish = false` does **not** make the problem go
  away; it makes `cargo publish -p sefer-alloc` unresolvable.
- The only alternatives to publishing are (a) re-inlining the code back into
  `src/` and deleting the crate — undoing commit `63991cc`
  (`feat(racy-ptr-cell): extract lazy CAS-published pointer cell; unify 4 loom
  shadow models onto the real type`) and losing the real-type loom suite's
  crate-level isolation — or (b) shipping `sefer-alloc` without `alloc-core`,
  which is not a thing (`production` transitively requires it).

This is the same mechanism that already has an open task filed against it for a
*different* crate: **L2/#615, "cargo package fails now (aligned-vmem ^0.2
unresolvable)"** — a versioned path dep whose required version is not on the
registry blocks root packaging. `racy-ptr-cell` is the same class of blocker,
just not yet reached because L2 fails first. (I did **not** run
`cargo publish -p sefer-alloc --dry-run` to demonstrate it, because it would
abort on L2's `aligned-vmem ^0.2` before reaching `racy-ptr-cell` and would
prove nothing new.)

**Consequence for K3/#598:** for `racy-ptr-cell` specifically, "publish
standalone vs. mark `publish = false`" is not actually an open decision. The
real decision left is only *when* and *under what name*.

### 1.2 On its own merits: narrow, but genuinely non-trivial

Honest read of `crates/racy-ptr-cell/src/lib.rs:1-69` and `README.md:1-48`:

**For.** The crate occupies a real, articulable niche that `std::sync::OnceLock`
and `once_cell` do not fill, and the module doc states it precisely
(`src/lib.rs:27-49`): `no_std` + allocation-free + **safe inside a
`#[global_allocator]`** (no `std` sync primitive, so no reentrancy into the
allocator being bootstrapped) + **fallible init with rollback and loser
re-race** (`OnceLock::get_or_try_init` poisons on `Err`; this one rolls the
sentinel back to `null` so a later attempt can succeed). That last property is
not cosmetic — the anti-livelock rule it forces (losers spin
`while == INITIALIZING`, **not** `while != READY`, `src/lib.rs:16-22`,
`:323-347`) is exactly the kind of thing people get wrong, and the crate ships
an executable loom proof of it against the **real** type plus a
`#[should_panic]` counterfactual that fails without the correct rule
(`tests/loom_racy_ptr_cell.rs:404-495`). That verification story — real-type
loom + two non-vacuousness counterfactuals — is the crate's actual value-add
over the 40 lines of atomics someone would otherwise hand-roll.

**Against.** The API is one type and three methods (§6). The payload must be
leaked for the process lifetime (`src/lib.rs:14-15`, `:61-69`) and `T` must
have `align_of >= 2` (`src/lib.rs:179-183`). Realistic consumers are allocator
authors, bare-metal bootstraps, and runtime bootstraps — a small population.
Search-discoverability is also weak: the name reads as a *warning* rather than
a guarantee (see §2.3).

**Net:** on merits alone I would call it "publish later, if anyone asks."
Combined with §1.1 it is "publish now." The two are not in conflict — §1.1
simply dominates.

---

## 2. Metadata completeness

`crates/racy-ptr-cell/Cargo.toml` — **complete, no placeholders, no TODOs.**

| field | line | value / verdict |
|---|---|---|
| `name` | `:2` | `racy-ptr-cell` — **available**, see §2.3 |
| `version` | `:3` | `0.1.0` — first publish |
| `edition` | `:4` | `2021` |
| `rust-version` | `:5` | `1.88` — verified buildable, §3.4 |
| `license` | `:6` | `MIT OR Apache-2.0`; both files present (`LICENSE-MIT`, `LICENSE-APACHE`) and both are in the tarball (§4) |
| `description` | `:7` | present, **383 chars** — see §2.2 |
| `readme` | `:8` | `README.md` — file exists, 1972 bytes |
| `repository` | `:9` | root repo URL — correct |
| `homepage` | `:10` | points at `tree/main/crates/racy-ptr-cell` — good, sub-crate-specific |
| `documentation` | `:11` | `https://docs.rs/racy-ptr-cell` |
| `keywords` | `:12` | 5/5 used, all valid (`oncelock`, `lock-free`, `allocator`, `cas`, `no-std`) |
| `categories` | `:13` | `memory-management`, `concurrency`, `no-std::no-alloc` — all real crates.io slugs; `cargo package` raised no warning |
| `publish` | — | absent → defaults to publishing. Deliberate or not, this is the state that matches §1.1 |
| `authors` | — | absent. Optional since edition 2018; not a defect |

### 2.1 Not present, and mostly fine

- **No `[package.metadata.docs.rs]`.** Correct today: the crate has **zero**
  Cargo features, so `--all-features` and default are identical and docs.rs
  needs no hint. **If** the §5.1 fix introduces an `internals` feature, add
  `[package.metadata.docs.rs] all-features = true` in the same change.
- **No `include`/`exclude`.** Not needed — the tarball is already minimal
  (§4).
- **No `CHANGELOG.md` for this crate.** `release.yml`'s changelog guard
  explicitly skips member crates and says so
  (`.github/workflows/release.yml:213-216`, "None of the member crates ... has
  a per-crate changelog yet, so for them this guard is skipped with an explicit
  reason"). Consistent with L4/#617; not a blocker, but a first publish is the
  natural moment to reconsider.

### 2.2 `description` is 383 characters (nit)

Under crates.io's 1000-char limit, so it will upload. But crates.io search
results and `cargo search` truncate hard, and the current text front-loads a
parenthetical state machine before it says what the thing is. The first ~90
chars a user actually sees are:

> `Allocation-free, no_std lazy CAS-published pointer cell (UNINIT -> INITIALIZING -> READY over…`

Suggest keeping the full text but ensuring the **first sentence stands alone**
as the elevator pitch, with the loom/counterfactual detail moved to the README
(where it already is, `README.md:37-44`).

### 2.3 The name is permanent and reads backwards (nit, but decide now)

Independently confirmed the name is free:

```
$ curl -s https://crates.io/api/v1/crates/racy-ptr-cell
{"errors":[{"detail":"crate `racy-ptr-cell` does not exist"}]}
```

"Racy" here means *"raced upon, and correct under racing"* — but a first-time
reader on crates.io, with no module doc in view, will read "racy" as
"has data races", i.e. the exact opposite of the crate's guarantee. crates.io
names cannot be renamed or reclaimed (only yanked), so this is a one-way door.
Alternatives that read the way the crate behaves: `retry-once-ptr`,
`fallible-ptr-once`, `cas-ptr-cell`. **Not a blocker** — just the last
cheap moment to change it.

---

## 3. Build / test / lint health (standalone, from repo root)

All four gates run and green.

### 3.1 `cargo test -p racy-ptr-cell --all-features`

```
     Running unittests src\lib.rs        → 0 passed  (no in-src tests, per CLAUDE.md)
     Running tests\cell_unit.rs          → 3 passed; 0 failed
         get_is_none_until_initialised ... ok
         init_runs_once_then_fast_path ... ok
         oom_rolls_back_and_retry_succeeds ... ok
     Running tests\loom_racy_ptr_cell.rs → 0 passed  (file is #![cfg(loom)]:51 — vacuous without the cfg, by design)
   Doc-tests racy_ptr_cell              → 0 passed  (CLAUDE.md's "No doctests" rule is honored)
```

Note `--all-features` is a no-op here: the crate declares no `[features]`
table at all.

### 3.2 `cargo clippy -p racy-ptr-cell --all-features --all-targets -- -D warnings`

```
    Checking racy-ptr-cell v0.1.0
    Finished `dev` profile in 23.44s
```

Clean — zero warnings. (Note this runs the default lint set; the crate inherits
only `[workspace.lints.rust] unexpected_cfgs` from root `Cargo.toml:89-93` via
`crates/racy-ptr-cell/Cargo.toml:15-18`.)

### 3.3 `cargo doc -p racy-ptr-cell --all-features --no-deps`

```
 Documenting racy-ptr-cell v0.1.0
    Finished; Generated .../doc/racy_ptr_cell/index.html
```

Clean — no broken intra-doc links, no warnings.

### 3.4 MSRV and `no_std` (extra checks, not requested)

```
$ cargo +1.88 build -p racy-ptr-cell
    Finished `dev` profile in 2.01s                          ← rust-version:5 is honest
$ cargo build -p racy-ptr-cell --target thumbv7em-none-eabi
    Finished `dev` profile in 1.04s                          ← the no_std claim is real on a bare-metal target
```

Both pass. Worth recording because CI's `no_std` job only builds the **root**
crate for `thumbv7em-none-eabi` (`.github/workflows/ci.yml:711-725`) — the
`no_std` claim in this crate's own description/README
(`Cargo.toml:7`, `README.md:8-9`) is **not** currently pinned by any CI job.
Adding `-p racy-ptr-cell` to that job is a one-line hardening.

### 3.5 loom — real-type suite, 6/6 green including both counterfactuals

The crate's own README gives the invocation (`README.md:42-44`); CI runs the
same thing on ubuntu (`.github/workflows/ci.yml:956-959`). Reproduced here on
Windows, in an isolated target dir so as not to invalidate the shared build
cache:

```
$ RUSTFLAGS="--cfg loom" CARGO_TARGET_DIR=... cargo test --release -p racy-ptr-cell --test loom_racy_ptr_cell

running 6 tests
test counterfactual_relaxed_publish_loses_happens_before - should panic ... ok
test counterfactual_spin_on_ready_livelocks_on_oom_rollback - should panic ... ok
test real_exactly_once_three_threads ... ok
test real_exactly_once_two_threads ... ok
test real_fast_path_reentry_same_pointer ... ok
test real_survives_oom_rollback_two_threads ... ok

test result: ok. 6 passed; 0 failed; ... finished in 0.16s
```

This is the strongest single piece of evidence in the crate's favor, and it is
worth being explicit about *why*:

- The four real-type models (`tests/loom_racy_ptr_cell.rs:96`, `:141`, `:187`,
  `:241`) run against the shipped `RacyPtrCell`, not a transcription — the
  crate aliases its atomics to `loom::sync::atomic` under `--cfg loom`
  (`src/lib.rs:98-101`), which is why the `loom` dep is a
  `[target.'cfg(loom)'.dependencies]` **library** dep and not a dev-dep
  (`Cargo.toml:27-28`, explained at `:21-25`).
- The two `#[should_panic]` counterfactuals
  (`tests/loom_racy_ptr_cell.rs:383-402`, `:469-495`) are the
  non-vacuousness proof the repo's own zero-trust convention demands: they fail
  without the correct `Release` publish / `== INITIALIZING` spin rule. Both
  panicked as required.

The `#![cfg(loom)]` at `tests/loom_racy_ptr_cell.rs:51` means the suite builds
**empty and passes vacuously** without `RUSTFLAGS="--cfg loom"` — the exact
trap already filed and fixed for `CONTRIBUTING.md` as S3/#626. It is
correctly documented here at `:45-49` and correctly wired in CI.

---

## 4. Packaging

`cargo package -p racy-ptr-cell --list --allow-dirty`:

```
.cargo_vcs_info.json
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/lib.rs
tests/cell_unit.rs
tests/loom_racy_ptr_cell.rs
```

`cargo package -p racy-ptr-cell --allow-dirty` (verification build, **no
publish**):

```
   Packaging racy-ptr-cell v0.1.0
    Updating crates.io index
    Packaged 10 files, 67.3KiB (20.3KiB compressed)
   Verifying racy-ptr-cell v0.1.0
   Compiling racy-ptr-cell v0.1.0 (.../package/racy-ptr-cell-0.1.0)
    Finished `dev` profile in 11.75s
```

**Clean.** Specific things checked, because they are open items elsewhere:

- **No leak of `docs/`, review docs, or local absolute paths** — contrast
  L5/#615's finding against the *root* package. Ten files, all intentional.
  The largest is `LICENSE-APACHE` (11,354 B); `src/lib.rs` and the two test
  files are the rest.
- **The vendored `Cargo.lock` is trimmed to this crate's own graph** — 261
  lines / 6,902 B, not the full workspace lock.
- **`[lints] workspace = true` normalizes correctly** — the generated manifest
  inlines `[lints.rust.unexpected_cfgs] check-cfg = ["cfg(loom)", "cfg(kani)"]`,
  so the published crate does not depend on the workspace being present.
  (`kani` is root-only and harmlessly unused here, as the comment at
  `Cargo.toml:16-17` says.)
- **`.cargo_vcs_info.json` records only `sha1` + `path_in_vcs`** — no local
  filesystem path leaked.
- **`cfg(loom)` target dep survives normalization** as
  `[target."cfg(loom)".dependencies.loom] version = "0.7"`. This is correct
  and intended: a normal downstream build pulls zero non-std deps; only a
  downstream user who themselves sets `RUSTFLAGS="--cfg loom"` resolves `loom`.

### 4.1 Missing release infrastructure (K3/#598's actual deliverable)

`.github/workflows/release.yml` has **no** publish path for this crate:

- No `racy-ptr-cell-v*` tag pattern — the list is exactly
  `aligned-vmem-v*`, `sefer-region-v*`, `malloc-bench-rs-v*`, `numa-shim-v*`,
  `sefer-alloc-v*` (`:51-55`).
- No `racy-ptr-cell` option in the `workflow_dispatch` crate dropdown — same
  five (`:63-67`).

Publishing therefore requires editing `release.yml` in two places (or a
one-off manual `cargo publish`, which bypasses the K8/#603 "bind non-dry
publish to full CI success" guard and should not be done).

---

## 5. Completeness scan

```
$ grep -rnE "TODO|FIXME|unimplemented!|todo!|XXX|HACK" crates/racy-ptr-cell/
(no matches)
```

Zero incomplete markers across the whole crate — source, tests, README.

**`unsafe` accounting**, against CLAUDE.md's own self-verifying command
(`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'`): exactly **one** tier-1 match,
`crates/racy-ptr-cell/src/lib.rs:87`. It carries a 17-line justification block
(`:71-86`) naming the single reason to hold `unsafe` and enumerating the two
audited kinds. There are **no** `unsafe fn` in the crate. The four `unsafe`
sites are:

| site | line | justification present? |
|---|---|---|
| `unsafe impl<T> Send` | `:163` | yes — `// SAFETY:` at `:158-162`, plus the 18-line rationale at `:146-157` |
| `unsafe impl<T> Sync` | `:165` | yes — `// SAFETY: see the Send impl above.` at `:164` |
| `NonNull::new_unchecked` in `get` | `:233` | yes — `// is_ready proved p non-null.` at `:232` |
| `NonNull::new_unchecked` fast path | `:276` | covered by the same `is_ready` guard at `:275`; **no inline `// SAFETY:` comment** |
| `NonNull::new_unchecked` loser path | `:341` | guarded by `a != 0` at `:339`; **no inline `// SAFETY:` comment** |

Both un-commented sites are provably guarded one line above, so this is a
**documentation-consistency nit**, not a soundness gap — but the crate-level
block at `:85-86` asserts "Every `unsafe fn` / `unsafe impl` carries a
`# Safety` / `// SAFETY:` justification", and two `unsafe {}` blocks do not.
For a crate whose selling point is auditability, close the gap.

No dead code (clippy `-D warnings --all-targets` is clean, §3.2; every private
helper — `spin_hint`, `sentinel`, `is_ready` — has a live caller).

### 5.1 **[MEDIUM] `dbg_rollback_reenterable` can clobber a concurrent winner's sentinel, and its doc says it cannot**

`src/lib.rs:388-420`. This is the one real finding.

The probe is a four-step sequence:

1. `:391-398` — `CAS(null → sentinel)`; `.ok()?` returns `None` if the cell was
   not `UNINIT`.
2. `:402` — `store(null, Release)` — the rollback under test.
3. `:406-414` — `CAS(null → sentinel)` again; result is the returned verdict.
4. `:417` — `store(null, Release)` — "restore to null, exactly as observed on
   entry". **This store is unconditional.**

Step 4 executing when step 3 *failed* is the bug. Concretely:

- Cell is `UNINIT`. Probe thread **P** wins step 1; cell = sentinel.
- Thread **A** calls `get_or_try_init` (`:268`): fast path misses, CAS at
  `:280` fails, A enters the loser spin at `:331` and observes sentinel →
  spins.
- P executes step 2 (`:402`); cell = null.
- A observes `a == 0` at `:339-346`, breaks out, loops to the top, and its CAS
  at `:280` **succeeds**. Cell = sentinel, owned by A. A begins running the
  caller's `init` closure.
- P executes step 3: CAS fails (cell is A's sentinel) → `postcondition_holds
  = false`.
- P executes step 4 **anyway**: `store(null)` — **A's sentinel is destroyed
  while A is still inside `init`.**
- Thread **B** now wins `CAS(null → sentinel)` and runs `init` a **second
  time**. Both A and B publish. Whichever stores last wins; the other's
  allocation is leaked, and B and A have observed **two different pointers**.

That breaks both headline guarantees the loom suite proves — "exactly-once
init" and "same pointer for all observers" (`tests/loom_racy_ptr_cell.rs:17-20`).
It is not UB by itself (the function is safe and only touches its own atomic),
but for this crate's actual consumers it is worse than a leak: in
`sefer-alloc`'s use, two `PerClassDirty`/registry-chunk materialisations for
one slot is a correctness break in the consumer.

The reason this is a **medium** and not a **high** is that the crate documents
a precondition — `:381-383`, "callers MUST pick a cell no other thread is
concurrently initialising". But the very next clause overstates what enforces
it (`:383-385`):

> *(the entry CAS is the guard: if the cell is not `UNINIT`, the probe returns
> `None` and touches nothing)*

The entry CAS guards only the **instant of step 1**. It does nothing about
steps 2-4, which is exactly where the window is. A reader who trusts that
parenthetical will conclude the probe is concurrency-safe. It is not.

The same overstatement has already propagated into the root crate's own
audit ledger — `tests/dbg_hook_safety_tripwire.rs:286-287` justifies this hook
with:

> `"entry CAS proves the cell UNINIT before touching it; restores original state before returning"`

Both halves are true only in the uncontended case, which is the case the
justification was supposed to be arguing about.

**Why this matters more once published:** internally, the only caller is
`src/registry/bootstrap.rs:949`, forwarding from a `#[doc(hidden)] pub fn`
(`:938`) that is exercised by controlled tests on a caller-chosen idle chunk
index — the precondition genuinely holds. On crates.io, `dbg_rollback_reenterable`
is a `pub` safe fn on a 0.1.0 public API. `#[doc(hidden)]` (`:386`) hides it
from rustdoc; it does not remove it from the callable or the semver surface.

**Suggested fixes, cheapest first (all are semver-clean at 0.1.0):**

1. **Make step 4 conditional** — only `store(null)` if step 3's CAS succeeded.
   If step 3 failed, someone else legitimately owns the cell and the probe must
   not touch it; return `None` ("not applicable") rather than `Some(false)`,
   since a concurrent owner is not evidence that rollback is broken. Two-line
   change, removes the clobber entirely.
2. **Fix the doc parenthetical** at `:383-385` to say what the entry CAS
   actually guarantees (a point-in-time check, not mutual exclusion), and fix
   the mirrored claim at `tests/dbg_hook_safety_tripwire.rs:287`.
3. **Gate both `dbg_*` hooks behind an off-by-default feature** (e.g.
   `internals`), matching CLAUDE.md's own benchmark-hook rule ("Any hook with
   no production caller MUST default to gating behind the `bench-internals`
   feature ... otherwise the hook's `#[cfg]` is satisfied by `production`
   alone and silently widens the safe public surface"). The crate currently
   has **no** `[features]` table, so this is additive and cheap — but it means
   `sefer-alloc`'s `bootstrap.rs:949` forwarder must enable that feature, and
   docs.rs needs `all-features = true` (§2.1). Do this only if the maintainer
   wants the public surface minimal on day one; fix 1+2 alone closes the
   defect.

**Resolution note (appended, task #710, 2026-08-09):** this finding is
CLOSED — fix 1 above (conditional step 4) landed in `src/lib.rs`
(`postcondition_holds` gates the restore store at what is `:604-607` as of
the task #774 round-closing-review correction, 2026-08-09 — the original
citation here, `:471-474`, was already stale by the time it was written
and never corresponded to the gate at any revision; see
`docs/reviews/2026-08-09-racy-ptr-cell-round-closing-review.md` §F6.
Line numbers drift as the file is edited — grep `postcondition_holds` for
the current location rather than trusting either citation going forward),
matching this section's own suggestion exactly, and a real-type loom
regression test
(`tests/loom_racy_ptr_cell.rs`, `real_probe_rollback_does_not_clobber_concurrent_winner`)
now proves the clobber cannot recur. The rust-intel audit that queued
this task independently confirmed this was already fixed by the time it
ran (`docs/reviews/2026-08-07-racy-ptr-cell-rust-intel-audit.md`'s "NO
CODE CHANGE NEEDED" note) and flagged only that this section's own
snapshot — describing the pre-fix behavior as still live — could
mislead a reader who does not cross-reference the newer audit. This note
closes that gap; no further action needed. Suggested fix 3 (feature-gate
the `dbg_*` hooks) was independently evaluated and REJECTED — see task
#710's commit for the chosen alternative (promote both hooks to
documented, non-`#[doc(hidden)]` public API instead) and the crate
README's "Test-probe API stability" section for the full rationale.

### 5.2 [LOW] `new()` is a runtime panic, not a compile error, when not const-evaluated

`src/lib.rs:179-183` (and the loom twin at `:194-197`) asserts
`align_of::<T>() >= 2`. In the documented usage — `static CELL: RacyPtrCell<T>
= RacyPtrCell::new();` (`README.md:18`) — this is a const-eval **compile
error**, which is the intended behavior. But `RacyPtrCell::<u8>::new()` called
in a normal fn compiles fine and **panics at runtime**, and
`impl Default` (`:423-427`) inherits that with no warning of its own. The
constraint is described in the crate docs (`:55-59`, `:124-125`) but `new`'s
own doc comment (`:168-172`) has no `# Panics` section. Add one; it is the
first thing a `no_std`/`#[global_allocator]` consumer will trip over, in the
one context where a panic is least affordable.

---

## 6. Public API doc coverage — 100%

```
$ cargo +nightly rustdoc -p racy-ptr-cell -- -Z unstable-options --show-coverage
+-------------------------------------+------------+------------+------------+------------+
| File                                | Documented | Percentage |   Examples | Percentage |
+-------------------------------------+------------+------------+------------+------------+
| crates\racy-ptr-cell\src\lib.rs     |          5 |     100.0% |          0 |       0.0% |
+-------------------------------------+------------+------------+------------+------------+
```

The whole public surface, each with a substantive doc comment (not a
one-liner):

| item | line | notes |
|---|---|---|
| crate module doc | `:1-69` | state machine, the two rules, `OnceLock` comparison, sentinel encoding, ownership |
| `struct RacyPtrCell<T>` | `:136` | doc at `:128-135`; both fields documented too |
| `RacyPtrCell::new` | `:175` / `:193` | `const` on normal builds, non-`const` under loom, both documented; missing `# Panics` — §5.2 |
| `RacyPtrCell::get` | `:229` | ordering contract stated |
| `RacyPtrCell::get_or_try_init` | `:268` | 25-line contract: fast path / winner / loser / reentrancy |
| `RacyPtrCell::dbg_is_ready` | `:361` | `#[doc(hidden)]` |
| `RacyPtrCell::dbg_rollback_reenterable` | `:388` | `#[doc(hidden)]`; doc is wrong in one clause — §5.1 |
| `impl Default` | `:423` | trait impl; `fn default` undocumented, which is conventional |

**0% examples is correct and deliberate**, not a gap: CLAUDE.md forbids
runnable doctests in `src/**/*.rs`. The illustrative snippets use
non-executed ` ```text ` fences (`src/lib.rs:6-10`, `README.md:17-24`) exactly
as the rule requires, and the executable versions live in `tests/`.

One caveat: nothing **pins** the 100%. The crate has no
`#![deny(missing_docs)]` / `#![warn(missing_docs)]`, and the inherited
workspace lint table (root `Cargo.toml:89-93`) only sets `unexpected_cfgs`. For
a published crate, `#![deny(missing_docs)]` at `src/lib.rs:87`-ish costs
nothing today (coverage is already 100%) and prevents regression.

---

## 7. Performance angle — nothing known-but-undone

```
$ grep -n -i "racy\|RacyPtrCell" docs/perf/OPEN_ITEMS.md docs/perf/OPEN_ITEMS_ARCHIVE.md
(no matches)
```

**Zero** hits in either perf index. No perf-gate report, no deferred
optimization, no measurement debt targets this crate.

`docs/CORRECTNESS_OPEN_ITEMS.md` has two relevant hits, neither a defect in
this crate:

- **`:1156`** — inside **item 17** ("Five tier-1 `unsafe` seams have no miri,
  no loom, and no kani harness", task **K11/#606**). The mention is that
  `alloc_core::dirty_by_class`'s `loom_class_aware_dirty` model uses
  hand-rolled `loom::sync` atomics rather than the real
  `PerClassDirty`/`RacyPtrCell` types. That is a gap in **sefer-alloc's**
  in-tree loom coverage of the sidecar deref, not in `racy-ptr-cell` — whose
  own protocol *is* covered by a real-type suite (§3.5).
- **`:1412`** — inside **item 24** (task **S4/#627**), the README's false
  "each is a real crates.io crate someone can `cargo add` on its own" claim
  (`README.md:515`) with badges for all eleven members (`README.md:549` is
  this crate's row). Publishing resolves it for this crate; not publishing
  requires the README edit item 24 describes.

Performance-wise there is nothing to measure: the cell is one `AtomicPtr`, the
fast path is a single `Acquire` load (`:230`, `:274`), and the crate adds no
allocation and no dependency to a normal build.

---

## Open questions for the maintainer

**Top recommendation, unambiguous: PUBLISH NOW — `racy-ptr-cell` 0.1.0, before
tagging `sefer-alloc-v0.3.0`, after applying §5.1 fixes 1+2.**

Not because the crate has a large independent audience (§1.2 says it does not),
but because `sefer-alloc` 0.3.0 cannot be published to crates.io while a
versioned path dep of its `production` feature chain is absent from the
registry (§1.1). "Keep internal / `publish = false`" is not on the table
without re-inlining commit `63991cc`.

Ordered questions:

1. **§5.1 — fix before or after publishing?** My read: **before**. The
   two-line conditional-store fix plus the doc correction turn a
   publicly-callable safe fn that can break the crate's headline invariant into
   one that cannot. Post-publish it is still fixable (0.1.1, no semver break),
   but it would ship a known hole in the 0.1.0 that docs.rs will render
   forever. Also decide whether you additionally want fix 3 (feature-gating
   the two `dbg_*` hooks) — that one has a real cost: `sefer-alloc`'s
   `bootstrap.rs:949` forwarder must enable the new feature.

2. **§2.3 — keep the name `racy-ptr-cell`?** Last cheap moment. crates.io names
   are permanent. "Racy" reads to a newcomer as "has data races", the opposite
   of what the crate guarantees.

3. **§4.1 — how does it get published?** `release.yml` needs a
   `racy-ptr-cell-v*` tag pattern (`:51-55`) and a dropdown option (`:63-67`).
   This is K3/#598's actual work item. Doing it by hand instead bypasses
   K8/#603's "non-dry publish must clear full CI on the same SHA" guard —
   don't.

4. **Does this crate get its own `CHANGELOG.md`?** `release.yml:213-216`
   currently skips the changelog guard for every member crate, by explicit
   decision (L4/#617). A first publish is the natural moment to either commit
   to per-crate changelogs or record that member crates deliberately have
   none.

5. **Should `#![deny(missing_docs)]` be added (§6) and should CI's `no_std` job
   cover `-p racy-ptr-cell` (§3.4)?** Both are one-liners; both pin a property
   the crate currently advertises but does not enforce. The `no_std` one is the
   more valuable of the two — the description at `Cargo.toml:7` sells
   "Allocation-free, no_std" and nothing in CI would catch it regressing.

6. **Publish order.** `racy-ptr-cell` is a leaf (no path deps), so it can go
   first, independent of L2/#615 (`aligned-vmem ^0.2`) and L3/#616. It does not
   block on, and is not blocked by, anything except `sefer-alloc` itself, which
   blocks on it.
