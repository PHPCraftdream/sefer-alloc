# Publish-readiness sweep — consolidated summary (7 crates)

**Date:** 2026-08-06
**Scope:** ahead of the deferred pre-release publish-DAG pass (tasks K3/#598,
K4/#599, K9/#604, L2/#615, L3/#616, L5/#618), seven independent `@oh`
readonly review agents each audited one workspace crate for crates.io
publish readiness. All seven measured at `main` @
`2a1ca35ae7ba78c286beaf3acff4b8210fa9766f` (or a commit within one hop of
it), all read-only — no file edited, no version bumped, no real `cargo
publish` run, nothing committed by any agent.

**Individual reports** (all uncommitted, this file included, pending
maintainer triage):
- `docs/reviews/2026-08-06-aligned-vmem-publish-readiness-review.md`
- `docs/reviews/2026-08-06-numa-shim-publish-readiness-review.md`
- `docs/reviews/2026-08-06-sefer-region-publish-readiness-review.md`
- `docs/reviews/2026-08-06-racy-ptr-cell-publish-readiness-review.md`
- `docs/reviews/2026-08-06-size-classes-publish-readiness-review.md`
- `docs/reviews/2026-08-06-tagged-index-stack-publish-readiness-review.md`

**Headline: all seven are GO-WITH-FIXES. Nothing found is a NO-GO.** No
crate has a structural blocker; every crate's build/test/clippy/doc/package
gates are green. The "with-fixes" list is real, small, and mostly
documentation/metadata — except for two items worth the maintainer's
particular attention: a latent macOS+miri compile break in `numa-shim`
(§2) and a reproducible silent-memory-corruption precondition gap in
`size-classes` (§5).

---

## 1. Verdict table

| Crate | Path | Published | Local | Verdict | Fix cost | DAG position |
|---|---|---|---|---|---|---|
| `aligned-vmem` | `crates/vmem` | 0.1.0 | 0.2.0 | GO-WITH-FIXES | 5 doc/metadata fixes (F1,F3,F4,F5 cheap; F2 a judgment call) | **leaf**, publish first |
| `numa-shim` | `crates/numa` | 0.1.0 | 0.1.0 (drifted) | GO-WITH-FIXES | 1 real compile-break fix + 4 doc/metadata | depends on `aligned-vmem 0.2` — publish **after** |
| `sefer-region` | `crates/region` | 0.1.0 | 0.1.0 (no code drift) | GO-WITH-FIXES | 1 doc-overclaim line, else optional (see below) | leaf, unconstrained |
| `racy-ptr-cell` | `crates/racy-ptr-cell` | unpublished | 0.1.0 | GO-WITH-FIXES | 1 medium (double-init hazard) + 3 low | leaf, **mandatory** (blocks 0.3.0) |
| `size-classes` | `crates/size-classes` | unpublished | 0.1.0 | GO-WITH-FIXES | 2 `const` asserts (~8 lines) | leaf, **mandatory** (blocks 0.3.0) |
| `tagged-index-stack` | `crates/tagged-index-stack` | unpublished | 0.1.0 | GO-WITH-FIXES | 4 small fixes (1 medium correctness contract, 1 medium portability doc, 2 low) | leaf, **mandatory** (blocks 0.3.0) |
| `malloc-bench-rs` | `crates/malloc-bench` | 0.1.0 | 0.1.0 | *not reviewed this batch* | — | standalone, not in `sefer-alloc`'s runtime dep tree |

`malloc-bench-rs` was out of scope for this batch (dev-tooling harness, not
part of the `sefer-alloc` runtime dependency graph) — flagging here only so
its absence from the table above is a deliberate scoping choice, not an
oversight.

---

## 2. `aligned-vmem` (crates/vmem) — GO-WITH-FIXES

Leaf of the DAG (empty `[dependencies]` in the packaged manifest) — **can
publish first, unilaterally**. All gates green (24 tests, clippy clean,
`deny(missing_docs)` clean, package/verify-build clean, 33 KiB compressed,
no leaked local paths/dev-docs).

**Findings:**
- **F1 (P1):** `huge-pages` feature's Cargo.toml comment + rustdoc claim
  macOS `VM_FLAGS_SUPERPAGE`/`MADV_HUGEPAGE` support; the actual non-Linux
  code path (`src/lib.rs:1407-1412`) is an empty no-op. README itself is
  accurate — only the Cargo.toml comment and rustdoc overclaim. 3-line fix.
- **F2 (P1, judgment call):** `huge-pages` has **zero** test/CI coverage
  anywhere — the Linux `MAP_HUGETLB` path has never been compiled by CI on
  any OS (crate-scoped CI job runs default-features only, which reduces 3
  of 4 test files to zero tests). Includes one PLAUSIBLE-not-CONFIRMED
  silent VA-leak hazard (`src/lib.rs:1173-1179`+`:1389-1392`, `munmap`
  return value discarded). Opt-in/off-by-default, so not a blocker, but
  publishing an advertised-but-never-compiled feature is a conscious risk.
- **F3 (P1):** no `[package.metadata.docs.rs]` → docs.rs will render 0.2.0
  with **empty default features**, omitting every optional API the README
  advertises (`lazy-commit`, `huge-pages`, `mock`, `fault-injection`,
  `bench-internals`). 2-line fix, root crate already has the pattern
  (`Cargo.toml:26-27`).
- **F4 (P2):** 4 broken/private rustdoc intra-doc-link warnings under
  `--all-features` (clean under default features — F3 currently hides
  this; fixing F3 without F4 would surface all four on the published docs).
- **F5 (P2):** `categories` claims `no-std::no-alloc`; crate uses `std`
  unconditionally (`error.rs:96,100,105`, `mock.rs:101`). Inherited from
  0.1.0, cheap to drop now.
- **F6 (P2):** `decommit`'s `# Safety` section omits that a pre-`recommit`
  write on Windows is a hard access violation — this exact divergence
  already crashed an in-repo consumer
  (`docs/CORRECTNESS_OPEN_ITEMS.md:370-397`).

**Semver:** 0.2.0 (minor) confirmed correct — real breaking change in
`recommit`'s signature (`()` → `#[must_use] bool`, commit `617518f`).
Everything else in the 16-commit delta is additive or a non-breaking
deprecation. Not 1.0-worthy yet.

**Perf:** nothing queued. The one candidate speedup (Windows
`VirtualAlloc2` aligned-reservation fast path) was already measured and
explicitly rejected (F11, `docs/perf/OPEN_ITEMS.md:2080` — avoidable share
4.3-4.8%, page faults dominate at ~95.4%). Freezing 0.2.0 now is
reasonable.

---

## 3. `numa-shim` (crates/numa) — GO-WITH-FIXES

**Not a DAG leaf** — depends on `aligned-vmem = { version = "0.2", path =
"../vmem" }` (`crates/numa/Cargo.toml`), so it must publish **after**
`aligned-vmem 0.2.0` lands. `cargo publish --dry-run` today fails exactly
as expected: `failed to select a version for the requirement 'aligned-vmem
= "^0.2"'`. Confirmed there is no feature-flag escape (`--no-default-features`
gives the identical error — `cargo publish` resolves the full manifest
regardless of selected features).

All health gates green (13/13 tests all-features, 9/9 under `mock`, 4/4
default, clippy clean, `deny(missing_docs)` clean by construction, package
tarball minimal/clean — **L5's tarball-leak concern does not apply here**).

**The one finding that matters most:**
- **F1 (Medium, real, previously unknown):** `src/lib.rs:763` is
  `#[cfg(target_os = "macos")]` with no `not(miri)` guard, unlike its three
  sibling `#[cfg]` blocks (`:259`, `:608`, `:812`). On macOS **under miri**,
  both it and the separate `#[cfg(miri)] mod platform` (`:788`) compile
  simultaneously → `error[E0428]: the name 'platform' is defined multiple
  times`. Reproduced via scratch repro (no native darwin toolchain
  available here to compile directly, but the cfg-overlap is unambiguous
  from source). Never caught because every miri CI job runs on
  `ubuntu-latest`, and the standalone `numa-shim-macos` CI job runs plain
  `cargo test` (no miri). One-word fix.

**Other findings:** `categories` again claims `no-std::no-alloc` falsely
(F2); no `[package.metadata.docs.rs]`, so docs.rs would omit
`reserve_on_node` and show 4 broken intra-doc links (F3).

**Also caught, outside the original brief:** root `Cargo.toml:896` still
pins `numa-shim = { …, version = "0.1", … }`. **This must move to `"0.2"`
in the same change as the numa-shim version bump**, or `sefer-alloc 0.3.0`
would build fine locally (path override masks it) while downstream
consumers silently resolve the stale published `numa-shim 0.1.0`.

**Semver — 0.2.0-not-0.1.1 plan reconfirmed, not upended.** Reading actual
diffs (not just commit subjects) found two genuinely breaking changes:
`#[non_exhaustive]` added to `MockCall` (commit `dbfeca3`) and the
`aligned-vmem` 0.1→0.2 public-dependency bump changing `Reservation`'s
identity in `reserve_on_node`'s signature (commit `4ec1516`). Under 0.x
rules, **0.2.0 is exactly right; 0.1.1 would have been actively wrong.**

**Correction to the original task brief:** commit `9b48844`'s subject
("cache current_node()...") is misleading for this crate specifically —
the actual `current_node()` caching landed on `AllocCore` in the **root**
crate, not in `crates/numa/`. This crate's delta from that commit is only a
16-line mock-recorder reentrancy fix. An external crates.io consumer of
`numa-shim` alone still pays the full uncached syscall path. Flagged as an
open question rather than a filed item, since a prior open-items pass
already closed NUMA perf as "out of scope for `production`" and explicitly
warned against re-litigating that ground — but that closure never assessed
the shim as a *standalone* artifact.

**Perf:** current state is a reasonable freeze point for 0.2.0 — remaining
speedup ideas (`getcpu(2)`, caching, loop bounding) are internal-only or
additive, ship cleanly as 0.2.1+, and keeping them out of 0.2.0 also avoids
tripping `docs/NUMA_RELEASE_GATE.md`'s multi-socket gate.

---

## 4. `sefer-region` (crates/region) — GO-WITH-FIXES

Leaf, unconstrained (only external dep is `slotmap`). **Confirmed zero
code drift since the 0.1.0 publish** — not assumed, independently verified
two ways: (1) comment-stripped diff of all four `src/` files against the
pre-drift commit reports IDENTICAL; (2) the actual published tarball was
downloaded from `static.crates.io` (sha256 matching the crates.io API
checksum) and diffed against the working tree — `src/handle.rs` and
`tests/smoke.rs` byte-identical, the three differing files differ **only**
in `///`/`//!` doc-comment lines.

**But the published artifacts themselves have drifted, and all three
corrections are of claims the maintainer's own commits already classified
as factually false:**

1. **Live crates.io category is wrong**: published listing shows
   `no-std::no-alloc`; local `Cargo.toml:13` already corrected to
   `no-std` (removed by commit `2e88262`, because the crate does `extern
   crate alloc`) — but that fix has never reached crates.io, since 0.1.0's
   listing is fixed at publish time.
2. **The published README asserts a safety guarantee the crate does not
   provide**: it claims the compiler rejects "cross-region handle
   confusion." The branding is `PhantomData<fn() -> T>`
   (`src/handle.rs:19`) — by value **type**, not by `Region` instance. A
   `Handle<T>` from one `Region<T>` is silently accepted by, and can
   corrupt/misresolve against, an unrelated `Region<T>`. Not memory-unsafe
   (no `unsafe` in this crate), but a real logic hazard the live crates.io
   page currently invites. Already fixed locally in `README.md:22-26`
   (commit `aab617a`).
3. docs.rs rustdoc mirrors the same stale claim.

**The one local fix still needed:** `crates/region/src/lib.rs:16` still
carries the exact "cross-region" overclaim that `aab617a` already fixed in
the README but missed in the crate-root rustdoc — the last surviving
instance. Publishing 0.1.1 without fixing this would put a corrected
crates.io front page next to a docs.rs page that contradicts it on the
crate's headline safety property.

**All gates green**: 5/5 tests (0 doctests), clippy clean, `cargo doc`
clean, `--no-default-features` `no_std` build passes, `-W missing_docs`
zero warnings (100% doc coverage), package clean (12 files, 38.8 KiB, no
leakage — L5/#618 does not apply to this crate).

**Recommendation:** publish **0.1.1** as a docs/metadata-only patch after
the one `lib.rs:16` fix — or, since the compiled artifact is genuinely
unchanged, staying at 0.1.0 is also a defensible choice (this is
explicitly left as an open question, not a forced call).

**Perf:** nothing owed — `docs/perf/OPEN_ITEMS.md:1527-1530` already
examined and ruled this crate out as a batch-API consumer by design. The
only noted-but-undone idea (`docs/BENCHMARKS.md:52-55`, a `DenseSlotMap`
type alias) is additive, non-breaking, and gated on a trigger that hasn't
fired.

---

## 5. `racy-ptr-cell` (crates/racy-ptr-cell) — GO-WITH-FIXES

Leaf, currently unpublished. **The publish decision is not actually
optional.** Root `Cargo.toml:875` declares `racy-ptr-cell = { path = …,
version = "0.1", optional = true }`, reached via `alloc-core` →
`alloc-global` → `production`. A versioned path dependency is rewritten to
a registry dependency at publish time, and crates.io rejects uploads whose
declared dependencies (including optional ones) aren't in the index —
`publish = false` does not sidestep this, it makes `cargo publish -p
sefer-alloc` unresolvable outright. Same mechanism as the already-tracked
L2/#615 `aligned-vmem` issue. So for K3/#598 the real remaining decision
is only *when*, not *whether*.

**On independent merits alone the case is marginal** (one type, three
methods, narrow audience) — but that's moot given the forced dependency
above.

**All gates green**: 3/3 tests, clippy clean, `cargo doc` clean, package
verifies (10 files, 67.3 KiB, no leakage), MSRV honest, builds on
`thumbv7em-none-eabi`, 100% doc coverage, zero TODO/FIXME/unimplemented,
complete metadata. `real-type loom` confirmed: 6/6 loom tests green.

**The one real finding (Medium):** `dbg_rollback_reenterable`
(`src/lib.rs:388-420`) — step 4's `store(null)` at `:417` is
**unconditional**. If step 3's CAS legitimately lost a race to another
thread, step 4 destroys that thread's sentinel mid-`init`, causing double
initialization and two different published pointers — breaking both
invariants the crate's own loom suite proves. The doc's parenthetical
(`:383-385`, "the entry CAS is the guard...touches nothing") overstates
what actually protects steps 2-4, and this overstated claim has already
propagated into `tests/dbg_hook_safety_tripwire.rs:287`. Fix is two lines
(make step 4 conditional on step 3's success) plus a doc correction. Low
severity today (bench-only, `#[doc(hidden)]`), but matters more once
published, since `#[doc(hidden)]` hides rustdoc, not the crate's semver
surface.

**Low findings:** `new()` panics at runtime (not compile time) for
misaligned `T`, undocumented (`:168-172`); two `unsafe {}` blocks
(`:276`, `:341`) lack the inline `// SAFETY:` comment the crate's own
top-level block promises every site has; no `#![deny(missing_docs)]` pin
despite currently-100% coverage; CI's `no_std` job only covers the root
crate, so this crate's `no_std` claim is itself unpinned.

**Perf:** zero hits in either open-items index for this crate specifically.

**Infra gap for K3:** `release.yml` has no `racy-ptr-cell-v*` tag pattern
or dispatch option — publishing by hand would bypass the K8/#603
CI-success guard. One naming note: the crate name `racy-` reads to a
newcomer as "has data races," the opposite of the guarantee it actually
provides — worth a second look before the name becomes permanent on
crates.io.

---

## 6. `size-classes` (crates/size-classes) — GO-WITH-FIXES

Leaf, currently unpublished, no path dependencies of its own. **Strongest
standalone-publish case of the three K3 crates** — no sefer-shaped concept
survives in its public API (the one coupling point, "what counts as
huge?", was deliberately generalized into `Params::huge_threshold`), and
its generality is exercised in its own tests by three materially different
size-class schemes. **Also a hard blocker for `sefer-alloc 0.3.0`
regardless of the standalone-value question**: root `Cargo.toml:883`
declares `version = "0.1"` on the path dependency, so `publish = false`
would require editing the root manifest too — strictly more work than just
publishing it.

**All gates green, first try, zero warnings**: 9/9 tests (independent
reference implementations cross-checked three ways — jump table vs. walk
vs. scan), clippy clean, `-D missing_docs` zero warnings (100% doc
coverage), verified `no_std` on real `thumbv7em-none-eabi`, package clean
(10 files, 17.1 KiB, both license files, no leakage, no path deps so
unaffected by L2/L5), complete metadata (all 11 `[package]` fields, valid
keywords/categories), zero TODO/FIXME/dead-code.

**The finding that drives GO-WITH-FIXES instead of plain GO — reproduced,
not theoretical:** `Params::extras` documents three preconditions in prose
(`lib.rs:65-69`) that are the caller's responsibility, but **none is
checked**. `build_table` only asserts power-of-two `min_block`,
`geo_count > 0`, and slice-length consistency (`lib.rs:104-114`). Both
violations were reproduced in a scratch crate outside the repo:

- `min_block=16, extras=&[100,200]` → `class_for(96, align=16)` returns
  block size **100** (`100 % 16 == 4`) — misaligned memory handed back for
  an `align=16` request, from a `#![forbid(unsafe_code)]` crate, because
  the fast lookup path (`lib.rs:360-362`) skips the divisibility check
  entirely and rests on an invariant that `extras` can silently violate.
- `extras=&[16,32]` overlapping the geometric run → two table indices
  become permanently unreachable, with no diagnostic anywhere.

Fix is ~8 lines of `const fn`-compatible asserts (a strictly-increasing
check over the merged table). **In-tree this is unreachable today**
(sefer's own `EXTRAS` are hand-verified and pinned by consumer tests), but
a third-party consumer has no such protection — and adding compile-time
preconditions *after* publication would break someone's existing build, so
this needs to land in the very first publish, not a follow-up patch.

**Perf/API-freeze: no reason to wait.** Both open-items indexes checked
end-to-end — `docs/perf/OPEN_ITEMS.md:1865-1888` (item 39/F13, dated
2026-08-03) already examined this crate's `class_for` hot path and
returned a deliberate **"THIN, not worth a round"** verdict; the next
candidate (a memo cache) was considered and rejected for adding a branch
to the hottest path. The prior optimization is marked KEPT and already
landed. Source untouched for three weeks across rounds 30-34 — the
const-table design is stable; freeze it.

**Secondary notes:** CI never builds this crate specifically on a `no_std`
target (only the root crate's default-features-off build is checked, and
this crate is gated behind `alloc-core`); `Params`' five public fields have
no `#[non_exhaustive]` escape hatch (a deliberate, documented tradeoff —
`#[non_exhaustive]` would forbid the `const` struct-literal construction
the design requires — so this locks all five fields for the life of
`0.1.x`, a decision to be conscious of, not a defect).

Publishing this closes one third of the `CORRECTNESS_OPEN_ITEMS.md` item
24 (S4) false "each is a real crates.io crate" README claim — the README
already renders a crates.io badge for this crate that currently resolves
to nothing. Name confirmed free on crates.io as of review date.

---

## 7. `tagged-index-stack` (crates/tagged-index-stack) — GO-WITH-FIXES

Leaf, currently unpublished, `#![forbid(unsafe_code)]`. **Best independent
standalone-publish case of the three K3 crates** per the reviewing agent —
an ABA-tagged Treiber free-index stack is a genuinely reusable lock-free
primitive well outside allocator contexts. **Also a hard blocker for
`sefer-alloc 0.3.0`**: root `Cargo.toml:892` declares `version = "0.1"` on
this path dependency, reached via `alloc-global`; `publish = false` would
force `alloc-global` — and therefore `production` — out of the published
`sefer-alloc` crate entirely. No version bump is needed for this one:
`0.1.0` is already exactly what the root manifest pins.

**All gates green**: 12/12 relevant tests, clippy clean, loom 4/4 pass
(both `#[should_panic]` counterfactuals fire correctly), `--no-default-features`
build clean, `-D missing_docs` clean, package clean (11 files, no
leakage), `cargo publish --dry-run` reaches "Uploading" before aborting
(the standard dry-run stop point — no real publish attempted). Metadata
complete (10/10 fields, both LICENSE files, `[lints] workspace = true`
correctly inlined into the packaged manifest, empty `[dependencies]` — no
transitive deps reach a downstream consumer).

**Four findings, all cheap, none a design problem:**
- **F1 (Medium, reproduced):** `push`'s documented contract (`index <
  INDEX_MASK`, `lib.rs:317-328`) is insufficient once `INDEX_BITS > 32`:
  `index == u32::MAX` equals the internal `TAIL` sentinel (`:129`), passes
  the existing `debug_assert`, and causes **silent, reproducible slot
  loss** via `pop`'s `next == TAIL` branch. Reproduced in a scratch
  consumer at `INDEX_BITS = 40` — a drain returned `[5]` instead of the
  expected `[5, 4294967295]`. Not memory-unsafe (the crate is
  `#![forbid(unsafe_code)]`), and unreachable at sefer's own in-tree
  `INDEX_BITS = 16`, but exactly the kind of width-dependent constraint
  the docs discuss adjacently and then omit. 1 line + 1 doc sentence.
- **F2 (Medium):** `no_std` + `no-std::no-alloc` category, but the
  internal head is `AtomicU64` — verified absent on `thumbv7em-none-eabi`,
  `thumbv6m-none-eabi`, `riscv32imc-unknown-none-elf`,
  `armv5te-unknown-linux-gnueabi`. Undocumented anywhere. A false
  portability promise to exactly the embedded audience this category
  targets.
- **F3 (Low):** one broken rustdoc intra-doc link (`lib.rs:53`) — every
  sibling reference nearby is correctly qualified, this one instance was
  missed.
- **F4 (Medium):** **12 of the crate's 16 tests are CI-dead.** The only CI
  invocation anywhere is `.github/workflows/ci.yml:952-953` (`-p
  tagged-index-stack --test loom_aba` under `RUSTFLAGS="--cfg loom"`);
  `stack_unit.rs` and `regression_counter_wrap.rs` are `#![cfg(not(loom))]`
  and no `ci.yml` step passes `--workspace`, so those two files' tests —
  three quarters of the suite by file count — have never run in CI.
  `scripts/check-all.mjs` has zero references to this crate either.

**Two corrections to the original task brief, both good news:**
- **Not a hot path.** `free_slots` (the sefer-internal consumer) is
  reached only via `pick_slot`, whose sole callers are
  `HeapRegistry::claim`/`claim_with_config` — once per thread on
  thread-heap acquisition, not on the `alloc`/`dealloc` fast path.
  Consistent with there being no perf item filed against it.
- **The crate has no `[features]` table at all** — loom is wired purely
  via `--cfg loom` + `[target.'cfg(loom)'.dependencies]`, which the
  reviewer confirms is the architecturally correct approach (no runtime
  cost, no accidental default-on).

**Perf:** open-items grep found only one mention — this crate ruled out as
a batch-API consumer candidate. No perf or correctness work is pending
against the actual code.

---

## 8. Cross-cutting themes

1. **All seven crates are structurally publishable today** — no crate
   needs an architectural change, only documentation/metadata fixes plus,
   in two cases, small precondition/cfg fixes.
2. **Semver plan for the two drifted crates is independently confirmed
   correct**, not just asserted: `aligned-vmem 0.2.0` and `numa-shim
   0.2.0` both have genuine breaking changes in their post-0.1.0 delta
   (verified from actual diffs, not commit subjects), so the minor bump is
   the right carrier under Cargo's 0.x rules — 0.1.x patches would have
   been wrong, 0.3.0/1.0.0 is not needed yet.
3. **`no-std::no-alloc` category is wrong on three of the seven crates**
   (`aligned-vmem`, `numa-shim` — both use `std` unconditionally in small
   spots — and it's also *silently unsound to claim without checking* on
   `tagged-index-stack`, which is genuinely `no_std` but needs
   `target_has_atomic="64"`, undocumented). Worth a single pass across all
   `crates/*/Cargo.toml` `categories` fields before any of them publish,
   rather than three separate fixes.
4. **CI does not compile every crate's own feature/target matrix
   standalone.** Recurring pattern: `aligned-vmem`'s `huge-pages` (never
   compiled by CI on any OS), `tagged-index-stack`'s non-loom tests (never
   run in CI at all), `size-classes`'s `no_std` build (only the root
   crate's default-features-off build is checked). None of this blocks
   publishing, but each is a coverage gap that becomes more consequential
   once a crate has independent downstream consumers who can hit a path
   the maintainer's own CI has never exercised.
5. **The one latent compile-break (numa-shim F1, macOS+miri `mod platform`
   duplicate)** is the most important single finding in this batch — real,
   reproducible from source inspection, and undetectable by this repo's
   current CI matrix (miri only runs on ubuntu). Should be fixed before
   any republish, independent of the version-bump question.
6. **The root `Cargo.toml`'s own dependency version pins must move in
   lockstep** with any sub-crate bump — confirmed necessary for
   `aligned-vmem` (`:866`, already `"0.2"`, correct) and **still needed**
   for `numa-shim` (`:896`, currently `"0.1"`, must become `"0.2"`). Missing
   this would let a local build succeed via the path override while
   downstream consumers silently resolve the stale published version —
   exactly the class of bug L3/#616 already flagged.
7. **No crate's package tarball leaks anything** — L5/#618's tarball-leak
   concern, filed against the root `sefer-alloc` package, does not
   reproduce in any of the six sub-crates reviewed here; each crate
   directory is too small/self-contained to have a `docs/`/`checkpoints/`
   subtree to leak in the first place.

---

## 9. Suggested action plan (maintainer decision, not yet actioned)

Given the DAG constraints established across all seven reports:

1. **`aligned-vmem`** — apply F1/F3/F4/F5/F6 doc fixes, publish `0.2.0`
   first (true leaf, no prerequisites).
2. **`numa-shim`** — fix F1 (macOS+miri compile break) first — independent
   of everything else and should not wait. Apply F2/F3 doc fixes. Bump
   root `Cargo.toml:896` to `"0.2"` in the same change. Publish `0.2.0`
   after step 1 lands (hard dependency on `aligned-vmem 0.2.0` existing on
   crates.io).
3. **`sefer-region`** — decide (open question, not forced): publish
   `0.1.1` with the one `lib.rs:16` doc fix, or leave at `0.1.0` since the
   compiled artifact is genuinely unchanged. Unconstrained by the other
   two — can happen any time.
4. **`racy-ptr-cell`, `size-classes`, `tagged-index-stack`** — all three
   are *mandatory*, not optional, for `sefer-alloc 0.3.0` to be publishable
   at all (each is a hard-pinned versioned path dependency reachable from
   `production`/`alloc-core`/`alloc-global`). Apply each crate's small fix
   list, add release-workflow tag patterns for the three (currently
   missing — the original K3 finding), then publish all three `0.1.0`
   (leaves, no DAG ordering constraint among themselves).
5. Only after 1-4 land: revisit K4/#599 (`cargo package --list` / `publish
   --dry-run` end-to-end against the real registry) and K9/#604 (full
   test/doc/package matrix for all workspace members) as the final
   pre-tag verification pass.

This plan is advisory — every version bump and publish action requires the
maintainer's explicit go-ahead per this repo's standing convention (no
autonomous version bumps).
