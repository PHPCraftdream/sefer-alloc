# `numa-shim` (`crates/numa`) — publish-readiness review

**Date:** 2026-08-06
**Scope:** read-only pre-release audit of the `numa-shim` workspace member ahead of
re-publishing it to crates.io and tagging the root `sefer-alloc` 0.3.0.
**Measured at:** `main` @ `2a1ca35ae7ba78c286beaf3acff4b8210fa9766f`, working tree
clean except for untracked `docs/reviews/*.md` + `.claude/` (no `crates/numa/` file
is dirty — `git status` shows no modification under that path).
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0 (c980f4866 2026-06-30)`,
host `x86_64-pc-windows-msvc`. MSRV re-verified separately on `1.88-x86_64-pc-windows-msvc`.
**Nothing was edited, committed, bumped, or published.** `cargo publish` was run
**only** with `--dry-run`.

---

## Verdict: **GO-WITH-FIXES**

The crate is in good shape: it builds, tests, and lints clean on every feature
combination; it has no TODO/FIXME/`unimplemented!`/stub platform branch; every
public item is documented and `#![deny(missing_docs)]` (`crates/numa/src/lib.rs:49`)
enforces it; the package tarball is minimal and leaks nothing. There is **no
blocking correctness defect** in the shipping code paths.

**Publish is blocked by exactly one hard ordering dependency** (`aligned-vmem 0.2.0`
must reach crates.io first — confirmed below, §3), which is already tracked as task
L2/#615 and is not a defect in this crate.

The **0.2.0-not-0.1.1 decision is confirmed correct, and 0.3.0 is NOT needed** — see
§6. Two of the post-publish commits are genuinely breaking under strict Cargo SemVer
rules, and under Cargo's 0.x rules a `0.1.0 → 0.2.0` bump is exactly the right
carrier for a breaking change. Nothing found upends the plan.

The "with fixes" are five small, cheap items that should land **before** the tag,
all of them documentation/metadata, none of them touching shipping logic:

| # | Fix | Severity | Cite |
|---|-----|----------|------|
| F1 | `mod platform` is defined **twice** on `macos` + `miri` → E0428 compile break | **Medium** (latent, real) | `src/lib.rs:763` vs `:788` |
| F2 | `categories` claims `no-std::no-alloc`, but the crate has no `#![no_std]` and links `std` | Low (metadata accuracy) | `Cargo.toml:13`; `src/lib.rs` (no `#![no_std]`) |
| F3 | No `[package.metadata.docs.rs]` → docs.rs will render the crate **without** `reserve_on_node`, with 4 broken intra-doc links | Low (docs.rs quality) | `Cargo.toml:1-13`; measured, §2 |
| F4 | Broken intra-doc link `[`aligned-vmem`]` fires even under `--all-features` | Low | `src/lib.rs:32` |
| F5 | README install snippet still says `numa-shim = "0.1"`; README's flagship example violates `bind_range`'s own documented safety contract | Low | `README.md:33`, `:36`, `:49-52` vs `src/lib.rs:186-191` |

---

## 1. Metadata completeness

`crates/numa/Cargo.toml` is well-formed and near-complete for crates.io.

**Present and correct:**

- `name` / `version` / `edition` / `rust-version` — `Cargo.toml:2-5`. MSRV `1.88`
  is **verified**, not just claimed: `cargo +1.88 check -p numa-shim --all-features`
  exits 0 (see §2).
- `license = "MIT OR Apache-2.0"` (`Cargo.toml:6`) with **both** license files
  actually present in the crate directory (`crates/numa/LICENSE-APACHE`,
  `crates/numa/LICENSE-MIT`) and both included in the package tarball (§3). Many
  workspace members get this wrong; this one does not.
- `description` (`Cargo.toml:7`) — 264 chars, well under the crates.io limit,
  accurate, and leads with the actual differentiator.
- `readme` / `repository` / `homepage` / `documentation` — `Cargo.toml:8-11`. The
  `homepage` correctly deep-links to `tree/main/crates/numa` rather than the repo
  root, which is the right call for a workspace member.
- `keywords` — 5 entries (`Cargo.toml:12`), exactly at the crates.io maximum.

**F2 — `categories` is inaccurate (`Cargo.toml:13`).** The list is
`["os", "memory-management", "no-std::no-alloc"]`. The first two are right. The
third is not: `crates/numa/src/lib.rs` contains **no `#![no_std]` attribute** (the
only inner attributes in the file are `#![allow(unsafe_code)]` at `:48` and
`#![deny(missing_docs)]` at `:49`), so the crate links `std` unconditionally. It
also *uses* `std` directly under the `mock` feature —
`std::thread_local!` at `src/lib.rs:96` and `Vec` at `:98`/`:105`. The category
slug is valid so crates.io will accept it, but it advertises a property the crate
does not have. Note this is **copied from the sibling crate**
(`crates/vmem/Cargo.toml:13` has the identical third category), so the same
question applies there — worth deciding once for both.

**Not blocking, but worth noting:** there is no `authors` field. That is fine
(optional since edition 2018, and crates.io shows the publishing account), just
flagging it as a deliberate-or-not choice.

**F3 — no `[package.metadata.docs.rs]` section.** `default = []`
(`Cargo.toml:16`), so docs.rs will build with **no features**, which means
`reserve_on_node` (`src/lib.rs:229-252`, gated `#[cfg(feature = "vmem-integration")]`)
will be **absent from the rendered docs entirely** — even though it is one of the
crate's two headline capabilities and the README documents it at length
(`README.md:57-70`). Measured directly: `cargo doc -p numa-shim --no-deps` (default
features) emits **4** `rustdoc::broken_intra_doc_links` warnings, three of which are
`unresolved link to 'reserve_on_node'` at `src/lib.rs:32`, `:36`, and `:177` — the
feature table, the platform matrix, and the `bind_range` doc that tells Windows
users to use `reserve_on_node` instead. Recommended fix is two lines:

```toml
[package.metadata.docs.rs]
features = ["vmem-integration"]
```

(`features` rather than `all-features = true`, so docs.rs does not also enable the
test-only `mock` backend and publish its recording internals as the crate's headline
API surface.)

### `crates/numa/README.md` — exists, and is genuinely good

108 lines. It has a platform matrix (`README.md:9-14`), a "why yet another NUMA
crate" section that states the actual syscall numbers (`README.md:16-27`), a
feature-flag section, a full public-API listing (`README.md:72-90`), a syscall-number
table (`README.md:92-100`), and an MSRV line (`README.md:102-104`). This is above
the median for a 0.x utility crate.

**F5 — two README issues.**

1. **Version strings are stale for the planned bump.** `README.md:33` says
   `numa-shim = "0.1"` and `README.md:36` says `version = "0.1"`. `git log
   --since=2026-06-29T17:36:48Z -- crates/numa/README.md` is **empty** — the README
   has not been touched since the 0.1.0 publish, so these lines will be wrong the
   moment 0.2.0 lands. Same for the crate-level doc if it grows an install snippet.

2. **The flagship usage example violates the safety contract of the function it
   demonstrates.** `README.md:49-52`:

   ```rust
   let mut buf = vec![0u8; 4096];
   let node = current_node().unwrap_or(0);
   // SAFETY: `buf` is a live allocation owned by this scope.
   unsafe { bind_range(buf.as_mut_ptr(), buf.len(), node) };
   ```

   `bind_range`'s own `# Safety` clause (`src/lib.rs:186-191`) says: *"`[base, base + len)`
   must be a valid **OS reservation** owned exclusively by the caller."* A
   `Vec<u8>` is a `std::alloc` allocation with alignment 1, not an OS reservation —
   it can (and normally does) sit inside a larger mapping shared with unrelated heap
   objects, and its base is essentially never page-aligned. On Linux, `mbind(2)`
   requires a page-aligned `addr` and returns `EINVAL` otherwise; the wrapper
   discards errors by design (`src/lib.rs:182-184`, `:528-530`), so the example is a
   **silent no-op on the one platform where `bind_range` does anything at all**,
   while simultaneously being the documented-forbidden usage. The same pattern is
   baked into `tests/smoke.rs:33-45` (`bind_range_on_owned_memory_does_not_panic`),
   though there it is defensible — that test's stated purpose is only "must not
   panic," which it does verify.

   Suggested fix: make the README example use `reserve_on_node` (a real OS
   reservation), or a page-aligned buffer, and keep the `Vec` version only as a
   "this will be rejected by the kernel" counter-example. This is a
   teach-the-wrong-thing issue on the crate's front page, not a soundness bug.

---

## 2. Build / test / lint health, standalone

All commands run from the repo root. All green.

| Command | Result |
|---|---|
| `cargo test -p numa-shim --all-features` | **PASS** — 13 passed / 0 failed (lib 0, `mock_dispatch` 7, `smoke` 6, doc-tests 0) |
| `cargo test -p numa-shim --features mock` | **PASS** — 9 passed / 0 failed (`mock_dispatch` 5, `smoke` 4) |
| `cargo test -p numa-shim` (default, no features) | **PASS** — 4 passed / 0 failed (`smoke` 4) |
| `cargo clippy -p numa-shim --all-features --all-targets -- -D warnings` | **PASS** — zero findings, zero warnings |
| `cargo doc -p numa-shim --all-features --no-deps` | **PASS (exit 0), 1 warning** — see F4 below |
| `cargo doc -p numa-shim --no-deps` (default features) | **PASS (exit 0), 4 warnings** — see F3 above |
| `cargo +1.88 check -p numa-shim --all-features` | **PASS** — MSRV claim at `Cargo.toml:5` holds |

**Doc-test count is 0** in every configuration, consistent with the project's
"No doctests" rule — `src/lib.rs:17` uses a ```` ```text ```` fence and `:26` points
at `tests/smoke.rs` for the runnable form (this was done by `30b7082`, T1b).

**The `mock` feature specifically works.** `docs/NUMA_TESTING_OPTIONS.md` exists and
describes it as "Phase 1" (`Cargo.toml:18-20` cites that doc). All 5
`mock_dispatch` tests pass under `--features mock` alone, covering the four
dispatch behaviours the mock exists to prove: scripted return value
(`current_node_records_scripted_value`), default (`current_node_default_zero`),
argument recording (`bind_range_records_args`), and both short-circuits
(`bind_range_no_node_short_circuits`, `bind_range_zero_len_short_circuits`). Under
`--features "mock vmem-integration"` two more fire
(`reserve_on_node_chains_and_records`, `reserve_on_node_no_node_skips_bind`). CI
already runs exactly these combinations per-PR on three OSes
(`.github/workflows/ci.yml:1291-1334`: `numa-shim-mock` on Linux,
`numa-shim-windows` on `windows-latest` including the **non-mock** real
`VirtualAllocExNuma` path, and `numa-shim-macos`).

**F4 — one doc warning survives even under `--all-features`:**

```
warning: unresolved link to `aligned-vmem`
  --> crates\numa\src\lib.rs:32:70
```

`src/lib.rs:32` writes `[`aligned-vmem`]` with a hyphen; rustdoc resolves paths, and
the crate's Rust-side name is `aligned_vmem` (underscore) — so this can never
resolve. Either underscore it (`[`aligned_vmem`]`, which resolves under
`vmem-integration`) or make it a plain code span. One-character fix; it is the
**only** warning in an otherwise fully clean `--all-features` doc build.

---

## 3. Packaging

**`cargo package -p numa-shim --list` SUCCEEDS.** The task brief predicted this
would fail; that prediction is **refuted for `--list`** and **confirmed for
`publish`**. `--list` does not consult the registry for path-overridden
dependencies, so it completes and prints:

```
.cargo_vcs_info.json
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/lib.rs
tests/mock_dispatch.rs
tests/smoke.rs
```

Ten files. **The package content is clean** — both licenses, the README, one source
file, two test files. Nothing leaks: no internal review docs, no local absolute
paths, no stray artifacts. This is worth stating explicitly because it is the exact
opposite of the root crate's situation (task L5/#618, "package tarball leaks
internal review docs and local paths") — **that concern does not apply to
`numa-shim`.** `cargo package -p numa-shim --list --no-default-features` produces a
byte-identical list.

**`cargo publish -p numa-shim --dry-run --allow-dirty` FAILS**, exactly as predicted,
and the failure is the registry-resolution one:

```
warning: crate numa-shim@0.1.0 already exists on crates.io index
   Packaging numa-shim v0.1.0 (D:\dev\rust\sefer-alloc\crates\numa)
    Updating crates.io index
error: failed to prepare local package for uploading

Caused by:
  failed to select a version for the requirement `aligned-vmem = "^0.2"`
  candidate versions found which didn't match: 0.1.0
  location searched: crates.io index
  required by package `numa-shim v0.1.0 (D:\dev\rust\sefer-alloc\crates\numa)`
```

(exit 101). Two things this output settles:

1. **The ordering blocker is real and confirmed.** `crates/numa/Cargo.toml:24` requires
   `aligned-vmem = "0.2"`; crates.io only has `aligned-vmem 0.1.0`.
   `aligned-vmem 0.2.0` **must** publish first. Matches task L2/#615 and the
   documented DAG in `.github/workflows/release.yml:36-38`.

2. **`--no-default-features` does NOT work around it.** I tested this explicitly as
   the brief asked. `cargo publish -p numa-shim --dry-run --allow-dirty
   --no-default-features` produces the **identical** error. This is expected and
   worth writing down so nobody re-tries it: `cargo publish` resolves the *manifest's*
   dependency table, and an `optional = true` dependency is still a manifest entry
   that must resolve against the registry regardless of which features are selected
   for the verification build. There is no feature-flag escape hatch — the only two
   options are (a) publish `aligned-vmem 0.2.0` first (correct), or (b) drop the
   `vmem-integration` feature from the release (not recommended; the root crate
   depends on it — `Cargo.toml:896` pins
   `numa-shim = { path = "crates/numa", version = "0.1", features = ["vmem-integration"], optional = true }`).

3. The `warning: crate numa-shim@0.1.0 already exists on crates.io index` line
   independently confirms the version bump is mandatory, not optional.

**Coupled action item the brief did not mention.** The root `Cargo.toml:896` pins
`numa-shim = { ..., version = "0.1", ... }`. When `numa-shim` becomes `0.2.0`, that
line **must** be updated to `version = "0.2"` in the same change — otherwise
publishing `sefer-alloc 0.3.0` will either fail resolution or (worse) silently
resolve consumers to the stale `numa-shim 0.1.0` from crates.io while the local
workspace builds against the path override, producing a build that works locally
and breaks for every downstream user. Same applies to the `aligned-vmem` pin.

---

## 4. Completeness scan

**Clean.** `grep -rnE "TODO|FIXME|XXX|unimplemented!|todo!|HACK" crates/numa/src/
crates/numa/tests/` returns **zero matches**. There is no placeholder, no stub, no
half-wired branch.

**All five platform branches are fully written, not stubs:**

| Branch | Location | Status |
|---|---|---|
| Linux (non-miri) | `src/lib.rs:259-503` | Full impl: `sched_getcpu` + sysfs cpumap parser + `mbind(2)` |
| Linux mbind, x86_64/aarch64 | `src/lib.rs:514-539` | Full impl (`SYS_MBIND` 237/235, `src/lib.rs:560`/`:565`) |
| Linux mbind, other arch | `src/lib.rs:542-550` | **Intentional documented no-op**, not a stub — README.md:97-99 states the policy ("the syscall number is unknown; contributions welcome") |
| Windows (non-miri) | `src/lib.rs:608-760` | Full impl: `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` + `VirtualAllocExNuma` |
| macOS | `src/lib.rs:763-785` | **Intentional documented no-op** — macOS has no public NUMA API (`src/lib.rs:9`) |
| miri | `src/lib.rs:788-809` | **Intentional documented no-op** |
| fallback (other) | `src/lib.rs:812-833` | **Intentional documented no-op** |

Each no-op carries a doc comment stating *why* it is a no-op, and the crate-level
platform matrix (`src/lib.rs:36-43`) plus the README table (`README.md:9-14`)
document the behaviour for consumers. This is the right shape for a portability
shim — a silent no-op that a caller cannot discover would be the defect; these are
all announced.

### F1 — `mod platform` is defined twice on macOS under miri (real, latent)

The one genuine code defect found.

- `src/lib.rs:763` — `#[cfg(target_os = "macos")] mod platform { ... }`
- `src/lib.rs:788` — `#[cfg(miri)] mod platform { ... }`

The macOS gate has **no `not(miri)` guard**, unlike its two siblings:

- `src/lib.rs:259` — `#[cfg(all(target_os = "linux", not(miri)))]` ✔
- `src/lib.rs:608` — `#[cfg(all(windows, not(miri)))]` ✔
- `src/lib.rs:812` — `#[cfg(not(any(target_os = "linux", windows, target_os = "macos", miri,)))]` ✔
- `src/lib.rs:763` — `#[cfg(target_os = "macos")]` ✘ **missing `not(miri)`**

So on `x86_64-apple-darwin` / `aarch64-apple-darwin` **under miri**, both cfgs
evaluate true and the crate fails to compile with `error[E0428]: the name 'platform'
is defined multiple times`.

**Verified, not assumed.** I could not compile the real crate for a darwin target
(`x86_64-apple-darwin` std is not installed on this host — `error[E0463]: can't find
crate for 'std'`), so I reproduced the exact cfg shape in a scratch file outside the
repo:

```
error[E0428]: the name `platform` is defined multiple times
 --> ...t.rs:5:1
  |
3 | mod platform { pub fn f() -> u32 { 0 } }
  | ------------ previous definition of the module `platform` here
4 | #[cfg(miri)]
5 | mod platform { pub fn f() -> u32 { 1 } }
  | ^^^^^^^^^^^^ `platform` redefined here
```

**Why it has never been caught:** every miri job in `.github/workflows/ci.yml`
(`miri-core`, `miri-alloc-core`, `miri-fastbin`, …, from line 749 onward) runs on
`ubuntu-latest`, and the `numa-shim-macos` job
(`.github/workflows/ci.yml:1322-1334`) runs plain `cargo test`, never `cargo miri
test`. macOS-under-miri is therefore a cell no CI job occupies.

**Why it matters for a published crate:** the crate's own docs promise miri support
in three places — `src/lib.rs:9` ("macOS / miri: no-op"), `src/lib.rs:42` (miri row
in the platform matrix), `README.md:13` (miri row in the README table). A downstream
consumer on an Apple-Silicon dev machine who has `numa-shim` anywhere in the
dependency graph and runs `cargo miri test` gets a hard compile error, against a
crate that advertises miri as supported. This is the single most likely
"first bug report after publishing" candidate in the crate.

**Fix:** one word — change `src/lib.rs:763` to `#[cfg(all(target_os = "macos", not(miri)))]`.
Zero behavioural change on any currently-tested configuration (the miri stub at
`:788` already provides the identical `NO_NODE` / no-op semantics the macOS stub
provides). Cheap enough to land before the tag; if it slips, it is a clean 0.2.1.

### Dead code

Twelve `#[cfg_attr(feature = "mock", allow(dead_code))]` attributes exist
(`src/lib.rs:264, 520, 547, 554, 559, 564, 582, 613, 764, 789, 813`). These are
**not** suppressed real dead code — they are the deliberate, narrowly-targeted
outcome of `c5469f5`, which chose per-symbol `cfg_attr` gates over a blanket
module-level `allow` precisely so a genuinely-new dead-code regression in the
non-mock build would still be caught. The attributes are only active when `mock` is
on, i.e. only in the configuration where the platform modules really are bypassed by
construction (`src/lib.rs:152-157`, `:196-208`, `:232-247`). Verified correct: the
`--all-features` clippy run (which enables `mock`) is clean, and so are the
default-feature and `mock`-only test builds.

---

## 5. Public API doc coverage

**Complete, and mechanically enforced.** `#![deny(missing_docs)]` at
`src/lib.rs:49` means an undocumented public item is a hard compile error, and the
crate compiles in every configuration tested in §2 — so coverage is 100% by
construction, not by inspection.

Inspected anyway; the surface is small and every item's doc is substantive rather
than a placeholder:

| Item | Location | Doc quality |
|---|---|---|
| crate root | `src/lib.rs:1-43` | Strong — selling point, per-platform mechanism, feature table, full platform matrix |
| `NO_NODE` | `src/lib.rs:51-57` | Explains the sentinel *and* why `current_node` returns `Option` instead |
| `current_node` | `src/lib.rs:139-151` | Enumerates all three `None` cases + the single-node `Some(0)` case |
| `bind_range` | `src/lib.rs:169-192` | Per-platform behaviour + explicit `# Safety` contract + explicit error-swallowing policy |
| `reserve_on_node` | `src/lib.rs:211-231` | Per-platform behaviour, `None` conditions, `NO_NODE` behaviour |
| `mock` module | `src/lib.rs:59-66` | Purpose + feature gate |
| `MockCall` + all 3 variants + all 7 fields | `src/lib.rs:70-94` | Every field individually documented |
| `mock::CALLS`, `mock::CURRENT_NODE_SLOT` | `src/lib.rs:96-101` | Documented |
| `mock::drain`, `mock::set_current_node` | `src/lib.rs:103-111` | Documented |

Two structural notes, neither blocking:

- **The crate is a single `lib.rs` with several public items.** This is the
  sanctioned exception #3 in `CLAUDE.md` ("single-file seam crates in `crates/`"),
  which names `crates/numa/src/lib.rs` explicitly. No violation.
- **`mock::CALLS` and `mock::CURRENT_NODE_SLOT` are `pub static` thread-locals
  exposing raw `RefCell<Vec<MockCall>>` / `RefCell<u32>` (`src/lib.rs:96-101`).**
  A downstream `mock` user can hold a `borrow()` across a call into `current_node()`
  and observe the reentrancy behaviour `9b48844` introduced (silently dropped log
  entry). That is a fragile public surface for a published crate. Not a fix for this
  release — the accessors `drain()`/`set_current_node()` already cover the intended
  use, so the statics could be made private in a future 0.3.0 — just noting it as
  known API debt.

---

## 6. Semver sanity — **the 0.2.0 plan is confirmed correct; 0.3.0 is not needed**

I read all seven post-publish diffs with `git show <sha> -- crates/numa/`, not the
subjects. The commit subjects are, in two cases, **actively misleading about what
happened inside this crate**.

Complete change set since `0.1.0` (published 2026-06-29T17:36:48Z), oldest first:

| SHA | Subject | What it *actually* did to `crates/numa/` | Semver class |
|---|---|---|---|
| `7ea2798` | `fix(0.3.0-dev): F1 fallback livelock…` | **1 line.** `size % PAGE != 0` → `!size.is_multiple_of(PAGE)` in the Windows `reserve_aligned_numa` guard (`src/lib.rs:680`) | **None** — semantically identical, clippy-driven |
| `d542f51` | `ci: fix clippy --all-features on numa-shim linux platform module` | **3 lines.** `for i in 0..digits` → `.iter().take(digits)` in `format_sysfs_path`; one `///` → `//` on an `extern "C"` block | **None** — identical output, no API |
| `c5469f5` | `fix(clippy): resolve numa mock dead-code/needless_return…` | **22 lines.** Three `return x;` → `x` tail expressions; eleven `#[cfg_attr(feature="mock", allow(dead_code))]` added | **None** — pure lint hygiene |
| `30b7082` | `test(docs): T1b — migrate all rustdoc doctests to regular tests` | **3 lines.** ```` ```rust ```` → ```` ```text ```` on the crate-level usage example + a pointer to `tests/smoke.rs` (`src/lib.rs:17`, `:26`) | **None** — doc-only |
| `4ec1516` | `feat(vmem): aligned-vmem 0.2 …` | **1 line, `Cargo.toml` only.** `aligned-vmem = "0.1"` → `"0.2"` (`Cargo.toml:24`) | **BREAKING** (see below) |
| `dbfeca3` | `chore: hygiene grab-bag …` | **1 line.** `#[non_exhaustive]` added to `MockCall` (`src/lib.rs:71`) | **BREAKING** (see below) |
| `9b48844` | `perf(numa): cache current_node() to cut per-call syscall overhead (R11-5)` | **16 lines, and NOT the caching.** Only the mock recorder: `CALLS.with(\|c\| c.borrow_mut().push(call))` → `try_with` + `try_borrow_mut` (`src/lib.rs:130-136`) + a 10-line rationale comment | **None** (mock-only behaviour change) |

Nothing else. `crates/numa/tests/` and `crates/numa/README.md` are **byte-unchanged
since publish** (`git log --since=… -- crates/numa/tests/ crates/numa/README.md`
returns empty).

### The two genuinely breaking changes

**(a) `#[non_exhaustive]` on `MockCall` (`dbfeca3`, `src/lib.rs:71`).** Per the Cargo
Book's SemVer Compatibility reference, *adding* `#[non_exhaustive]` to an existing
public enum is classified **Major** — it breaks any downstream exhaustive `match`.
`MockCall` is public API (`src/lib.rs:73`) whenever the `mock` feature is on. The
mitigating facts: `mock` is documented test-only (`Cargo.toml:18-20`,
`src/lib.rs:59-65`), and `dbfeca3`'s own commit message records that it verified zero
exhaustive matches exist anywhere in the workspace. But a crates.io consumer is not
the workspace, and 0.1.0 has been public for five weeks.

**(b) The public dependency bumped a 0.x-major (`4ec1516`, `Cargo.toml:24`).**
`reserve_on_node` returns `Option<aligned_vmem::Reservation>` (`src/lib.rs:231`) —
`aligned_vmem::Reservation` is a **re-exposed type from a different crate** in
`numa-shim`'s public signature. Moving from `aligned-vmem 0.1` to `0.2` changes the
identity of that type. A downstream crate that depends on `aligned-vmem = "0.1"`
*and* `numa-shim` with `vmem-integration` would get two distinct `aligned-vmem`
copies in the graph and a type mismatch at the boundary. This is the textbook
public-dependency-major-bump break.

### Why this **confirms** rather than upends the 0.2.0 plan

Under Cargo's 0.x semver rules, for `0.MINOR.PATCH` with `MINOR > 0` the **minor
position is the compatibility-breaking position** — `^0.1` does not match `0.2.0`.
So:

- **`0.1.1` would have been wrong**, and not merely conservatively wrong — it would
  have silently flowed both breaking changes above into every existing
  `sefer-alloc 0.2.1` build via its `numa-shim = "^0.1"` pin, which is precisely the
  hazard the already-made decision was guarding against. The pre-existing constraint
  was correct **and** independently justified by the actual diff content, not only by
  the caution argument.
- **`0.2.0` is exactly right.** Under 0.x rules a minor bump *is* the breaking-change
  carrier. `0.2.0` correctly signals both breaks.
- **`0.3.0` is not needed.** There is no second, distinct breaking change requiring
  another compatibility boundary — one bump carries both, and there is no
  already-published `0.2.x` for these to break.

### One observable-behaviour change worth naming explicitly

`9b48844`'s `borrow_mut()` → `try_borrow_mut()` (`src/lib.rs:130-136`) **is** an
observable behaviour change: the recorder used to *panic* on reentrant invocation and
now silently drops the reentrant log entry. It is confined to the `mock` feature, the
returned node value is unaffected (it comes from `current_node_slot()`,
`src/lib.rs:114-116`), and it is unambiguously a bug fix — a recording mock should not
panic. It is nonetheless a semantic change a `mock`-feature consumer could notice, so
it belongs in the 0.2.0 release notes rather than being folded into "internal only."

### The commit-subject discrepancy — worth reading twice

**`9b48844`'s subject says the exact opposite of what it did to this crate.** The
subject is `perf(numa): cache current_node() to cut per-call syscall overhead`, but
the entire caching implementation landed in the **root** crate — a
`cached_numa_node: Option<u32>` field on `AllocCore` with invalidation at
`HeapRegistry::claim`, as that commit's own body describes. Its `crates/numa/`
delta is **only** the mock-recorder reentrancy fix. Confirmed by reading the diff:
`git show --stat 9b48844 -- crates/numa/` reports `1 file changed, 16 insertions(+),
1 deletion(-)`, all of it inside `pub mod mock`. There is no cache anywhere in
`crates/numa/src/lib.rs`. This matters for §7 and is exactly the failure mode
`CLAUDE.md`'s R30-12 commit-prefix taxonomy exists to prevent, seen from the other
direction: a `perf(...)`-prefixed commit whose effect on *this* crate is
measurement-infrastructure-only.

Similarly, `7ea2798`'s subject (`F1 fallback livelock, F3 warnings-clean…`) describes
root-crate work; its `crates/numa/` contribution is a single clippy-driven
`is_multiple_of` rewrite.

**Net for this crate:** four of seven commits are pure lint/doc hygiene with zero
semantic effect; one is a mock-only bug fix; two are breaking, and both are correctly
carried by a 0.2.0 bump.

---

## 7. Performance angle

**Recommendation: now is a reasonable point to freeze 0.2.0. Do not gate the release
on further perf work.** But there is a finding here the maintainer will want to know
about, because the premise of the question is inverted.

### The `9b48844` optimization is NOT in this crate

Per §6: the caching landed on `AllocCore` in the root crate, not in `numa-shim`.
`crates/numa/src/lib.rs` contains **no cache of any kind**. Every call to
`numa_shim::current_node()` from an external crates.io consumer still executes the
full uncached path:

`current_node()` (`src/lib.rs:151`) → `platform::current_node_impl()` (`:268`) →
`sched_getcpu()` (`:271`) → `cpu_to_numa_node(cpu)` (`:309`) → a loop over
`0u32..64` (`:311`) calling `node_contains_cpu` (`:326`) → `format_sysfs_path`
(`:334`) + `read_cpumap_contains_cpu` (`:373`), each iteration doing a full
`open` / `read(256)` / `close` triple (`:376`, `:383`, `:385`).

So on Linux a single `current_node()` call costs **up to 64 × (open + read + close)
= up to 192 syscalls**, plus `sched_getcpu`. The worst case is not theoretical: it is
hit whenever `sched_getcpu` returns a CPU that appears in no node's cpumap, in which
case the loop runs all 64 iterations and then returns `0` anyway (`:316-319`).

The maintainer has **already measured** what caching is worth: `9b48844`'s commit
body records **~230 ns uncached vs ~985 ps cached (~233×)** per call, and ~227 µs vs
~573 ns for a 1024-call batch (~396×) — and that was on **Windows**, where the
uncached path is merely two Win32 calls (`src/lib.rs:625`, `:631`). The commit body
explicitly notes the Linux sysfs-loop case is "more dramatic" and that no number is
claimed for it. So the benefit is proven for the cheap platform and reasoned to be
larger for the expensive one.

**The consequence for publishing:** `sefer-alloc` gets the fast path because the
cache lives in *its* `AllocCore`. Every other crates.io consumer of `numa-shim 0.2.0`
gets the slow path and has no way to obtain the fast one short of reimplementing the
cache themselves. That is a real quality-of-published-artifact gap, and it is
invisible if you read only the commit subject.

### Noted-but-not-done opportunities

I grepped both open-items indexes as asked.

- **`docs/perf/OPEN_ITEMS.md:1922-1932`** — item 39/F13 sub-verdict (c), **"NUMA —
  verdict OUT OF SCOPE for `production`."** Reasoning: `numa-aware` is not part of
  `production`, so every NUMA site compiles out of the shipped configuration, and
  "the one plausibly-hot cost (`numa::current_node()` per large allocation) is
  already cached … (`AllocCore::current_node_cached`, R11-5/R12-5)." Note the framing:
  it is scoped to *`sefer-alloc`'s* consumption of the shim, and it is correct on
  those terms. It does **not** assess the shim as a standalone published artifact —
  which is the gap above. The entry further warns this area has "already had one round
  wasted on re-raising a settled item" and exists specifically to prevent a third
  pass, so this should be treated as a *scope clarification*, not a reopening.
- **`docs/perf/OPEN_ITEMS.md:2036`, `:2045`, `:2082`** — R10-6/R11-6
  (`class_nonempty_by_node`) closed and re-verified still-closed by R25-9; F13
  recorded as a negative result. Nothing actionable.
- **`docs/CORRECTNESS_OPEN_ITEMS.md`** — one `numa` hit, at `:1418`, and it is not
  about NUMA at all: it is item 24 (task #627/S4), the README "all 11 members are real
  crates.io crates" claim, which merely *lists* `numa-shim` among the five crates that
  do have release targets in `.github/workflows/release.yml`. **No correctness item is
  open against `numa-shim`.**

So: **no index has a pending NUMA perf item.** The opportunities below are ones I
identified by reading `crates/numa/src/`, not ones already filed.

### Concrete speedups available (all internal-only)

1. **Use `getcpu(2)` instead of `sched_getcpu` + the sysfs scan.** This is the big
   one. Linux's `getcpu(2)` returns the CPU **and the NUMA node** in a single call,
   and is vDSO-accelerated (`sched_getcpu` is itself implemented on top of it in
   glibc, discarding the node). Adopting it would replace the entire
   `cpu_to_numa_node` → 64 × `node_contains_cpu` → `open`/`read`/`close` machinery
   (`src/lib.rs:309-390`, ~80 lines) with one call. It fits the crate's existing
   design perfectly — the raw-`syscall(2)` pattern with a per-arch constant is
   already established for `mbind` (`src/lib.rs:558-565`, `:573-575`), so this is the
   same technique applied to a second syscall, with no new dependency and no change to
   the "zero C libraries" selling point.
2. **Bound the node loop by reading `/sys/devices/system/node/online` once.** Much
   smaller win than (1) and strictly inferior to it, but it is the minimal-diff option
   if the raw-syscall route is unwanted: it caps the 64-iteration worst case at the
   real node count.
3. **Cache the result inside the shim.** The proven-233× change, currently available
   only to `sefer-alloc`. This one needs a *policy* decision (process-wide vs
   thread-local; refresh interval vs never; how a caller invalidates after a thread
   migrates or a `sched_setaffinity`) and would likely add a small amount of public
   API (e.g. an explicit `invalidate()` or a `current_node_cached()`). Adding public
   API is **minor-additive**, so it fits a future `0.2.x`/`0.3.0` cleanly.

### Why none of these should block 0.2.0

Options (1) and (2) are **purely internal**: same signature, same semantics
(`current_node() -> Option<u32>` with the same documented `None` cases,
`src/lib.rs:139-149`), no observable contract change. They can ship as **`0.2.1`**
whenever, with no coordination cost and no impact on the `sefer-alloc 0.3.0` tag.
Option (3) is minor-additive and fits a later release. Meanwhile the release *is*
blocked on `aligned-vmem 0.2.0` (§3) and the crate is otherwise clean, so adding a
perf task to the critical path buys nothing.

**One caveat on (1) if it is ever taken:** it changes which kernel interface reports
the node, so it should not land in the same release as a version bump that consumers
are already being asked to absorb a breaking change for. Keeping it in `0.2.1`,
behind the existing three-OS CI matrix plus the `numa-real-kernel` scheduled job
(`.github/workflows/ci.yml:1336+`), is the low-risk sequencing. Per
`docs/NUMA_RELEASE_GATE.md`'s own "When to run" clause — *"Before any release tagged
`0.x.y` whose diff touches `crates/numa/**`"* — a `getcpu(2)` change **would** trip
that gate and owe a multi-socket run, whereas the current 0.2.0 content (four
lint/doc commits + one mock fix + two metadata-level breaks, §6) is exactly the
"skip" case that clause carves out. That asymmetry is itself a good reason to keep
them in separate releases.

---

## Summary of pre-tag checklist

**Blocking (external, already tracked):**
1. Publish `aligned-vmem 0.2.0` to crates.io first (§3). Task L2/#615.
2. Bump `crates/numa/Cargo.toml:3` to `0.2.0`.
3. Update the root `Cargo.toml:896` pin `numa-shim = { … version = "0.1" … }` → `"0.2"` in the same change (§3).

**Recommended before the tag (cheap, no logic touched):**
4. **F1** — add `not(miri)` to `src/lib.rs:763` (one word; fixes a hard compile break on macOS + miri).
5. **F5** — `README.md:33`, `:36` → `"0.2"`; and fix the `bind_range` example at `README.md:49-52`.
6. **F3** — add `[package.metadata.docs.rs] features = ["vmem-integration"]`.
7. **F4** — `src/lib.rs:32`: `[`aligned-vmem`]` → `[`aligned_vmem`]`.
8. **F2** — drop or justify the `no-std::no-alloc` category at `Cargo.toml:13`.

**Release notes should mention:** the two breaking changes (§6a `#[non_exhaustive]`
on `MockCall`; §6b the `aligned-vmem` 0.1→0.2 public-dependency bump) and the
`mock`-only reentrancy behaviour change.

**Deliberately not recommended:** any perf work before the tag (§7).

---

## Open questions for the maintainer

1. **Is the `no-std::no-alloc` category intentional aspiration or an error?** The
   crate has no `#![no_std]` and uses `std::thread_local!`/`Vec` under `mock`
   (`src/lib.rs:96-105`). `crates/vmem/Cargo.toml:13` carries the identical category,
   so this is one decision covering both crates: **(a)** drop the category from both,
   or **(b)** actually make the non-`mock` core `no_std` (plausible — outside the mock
   module the crate appears to use only `core::ffi` and `Option`) and keep the claim
   honest. I did not attempt (b); it needs a real `--no-default-features` `no_std`
   build to confirm, and it would be a feature change, not a release fix.

2. **Should F1 (macOS + miri) block the tag, or ship as 0.2.1?** It is a one-word
   fix with zero behavioural effect on any currently-tested configuration, so my
   recommendation is to land it before tagging. But it is a genuinely new code change
   during a release freeze, so it is the maintainer's call. If it ships as 0.2.1
   instead, the crate-level docs at `src/lib.rs:9`/`:42` and `README.md:13` currently
   promise miri support that macOS users will not get — consider a one-line known-issue
   note in the release notes if so.

3. **Is `mock` intended to remain a published, non-additive feature?** Cargo requires
   features to be additive, and `mock` is not: it *replaces* the real platform backend
   (`src/lib.rs:152-157`, `:196-208`, `:232-247`). If any crate in a dependency graph
   enables `numa-shim/mock`, feature unification silently disables real NUMA for
   **every** consumer in that build — including `sefer-alloc`, whose
   `numa-aware-mock = ["numa-aware", "numa-shim/mock"]` (root `Cargo.toml:704`) is
   documented TEST-ONLY but is nonetheless a real edge in the public feature graph.
   This is not a regression (0.1.0 already shipped it) and I am **not** proposing a
   change now. But if it is to remain published, its docs should state the hazard
   explicitly; the alternative is to make it a `cfg`-driven rather than feature-driven
   seam in a future 0.3.0.

4. **Does the perf gap in §7 warrant an entry in `docs/perf/OPEN_ITEMS.md`?**
   `OPEN_ITEMS.md:1922-1932` closed the NUMA area as "OUT OF SCOPE for `production`,"
   which is correct *for `sefer-alloc`* but does not cover "`numa-shim` as a
   standalone published crate ships an uncached `current_node()` whose cached form is
   already measured at ~233× faster." That entry also explicitly warns against a third
   pass over this ground, so I have deliberately **not** filed anything — flagging the
   scope distinction for a human call rather than reopening a settled item unilaterally.

5. **Confirm the `aligned-vmem` version pin policy across the workspace.** §3's
   coupled action item (root `Cargo.toml:896`) is the kind of edit that is easy to miss
   and fails *silently for downstream users only* — local builds keep working via the
   path override while published consumers resolve the stale version. Worth a
   deliberate grep for every `version = "0.1"` / `version = "0.2"` path-dependency pin
   in the root manifest as part of the release, not just the `numa-shim` one.
