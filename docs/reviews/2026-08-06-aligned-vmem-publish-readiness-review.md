# `aligned-vmem` 0.2.0 — publish-readiness review (read-only)

- **Date:** 2026-08-06
- **Crate:** `aligned-vmem` (`crates/vmem`), local version **0.2.0**
  (`crates/vmem/Cargo.toml:3`); crates.io holds **0.1.0** only (published
  2026-06-29).
- **Reviewed at:** `HEAD` = `2a1ca35ae7ba78c286beaf3acff4b8210fa9766f`;
  `git status --porcelain crates/vmem/` is **empty** (the crate's own subtree is
  clean at this SHA — the repo's untracked files all live outside `crates/vmem`).
- **Host:** Windows 10, `rustc 1.97.0 (2d8144b78 2026-07-07)`.
- **Scope:** read-only. No file was edited, no version bumped, no `cargo publish`
  (real) run, nothing committed.

---

## Verdict: **GO-WITH-FIXES**

Nothing found is a structural blocker. `aligned-vmem` is a **leaf** of the
publish DAG — `crates/vmem/Cargo.toml:71` has an empty `[dependencies]` section,
so it has no path dependency to unblock first, and `cargo package` completes
end-to-end today. Tests, clippy and `deny(missing_docs)` are all green. It can
be published as-is without breaking anything.

But five things should be fixed **before** the tarball is immutable on
crates.io, because three of them are wrong or missing *documentation* that
crates.io/docs.rs will render permanently for 0.2.0:

| # | Sev | Finding | Fix cost |
|---|-----|---------|----------|
| F1 | P1 | `huge-pages` doc claims macOS support that the code does not implement (silent no-op) | 3 comment lines |
| F2 | P1 | `huge-pages` has **zero** test / CI / consumer coverage anywhere in the repo | 1 test file, or drop the feature |
| F3 | P1 | No `[package.metadata.docs.rs]` → docs.rs will show the default-feature API only; every optional API the README advertises will be **absent** from docs.rs | 2 lines in `Cargo.toml` |
| F4 | P2 | 4 rustdoc broken-intra-doc-link warnings under `--all-features` | 4 doc-comment edits |
| F5 | P2 | `categories` claims `no-std::no-alloc`, but the crate requires `std` | 1 line |

F1/F3/F4/F5 are all one-line-ish and cheap. F2 is a judgement call (see §7).

---

## 1. Metadata completeness

`crates/vmem/Cargo.toml:1-12` — all of the recommended `[package]` keys are
present and non-empty:

| Key | Value | Assessment |
|---|---|---|
| `description` (`:7`) | long, accurate one-paragraph summary | OK, one nit — see below |
| `license` (`:6`) | `MIT OR Apache-2.0` | OK; both files present (`crates/vmem/LICENSE-MIT`, `crates/vmem/LICENSE-APACHE`) |
| `repository` (`:9`) | `https://github.com/PHPCraftdream/sefer-alloc` | OK |
| `homepage` (`:10`) | `.../tree/main/crates/vmem` | OK — points at the subdirectory, better than the bare repo |
| `documentation` (`:11`) | `https://docs.rs/aligned-vmem` | OK |
| `keywords` (`:12`) | `mmap`, `virtualalloc`, `memory`, `decommit`, `aligned` | OK (5/5, the crates.io max) |
| `categories` (`:13`) | `memory-management`, `os`, `no-std::no-alloc` | **third one is wrong — F5** |
| `readme` (`:8`) | `README.md` | OK, file exists |
| `rust-version` (`:5`) | `1.88` | Plausible: `usize::is_multiple_of` (`lib.rs:453`, `:520`, …) stabilised in 1.87. Not independently verified — no 1.88 toolchain on this host. |
| `[package.metadata.docs.rs]` | **absent** | **F3** |

### F5 — `no-std::no-alloc` is false

The crate is not `no_std` and cannot be. It uses `std` unconditionally in three
places:

- `crates/vmem/src/error.rs:96` — `impl std::error::Error for VmemError {}`
- `crates/vmem/src/error.rs:100,105` — `std::io::Error::last_os_error()`
- `crates/vmem/src/mock.rs:101` — `std::thread_local! { … }`

There is no `#![no_std]` attribute anywhere (`grep -n 'no_std' crates/vmem/src/`
returns nothing; the crate attrs are `lib.rs:75-90`). The category is a semantic
lie that crates.io will not catch (the slug itself is valid). It was inherited
verbatim from 0.1.0 (`git show d95ea7f:crates/vmem/Cargo.toml` line 13 has the
identical list), so this is pre-existing, not new — but 0.2.0 is the cheap
moment to drop it.

### F3 — no `[package.metadata.docs.rs]`

`crates/vmem/Cargo.toml` has no `[package.metadata.docs.rs]` block. Neither does
any other member (`grep -rn 'docs.rs' crates/*/Cargo.toml` returns only
`documentation = …` lines) — but the **root** crate does have one
(`Cargo.toml:26-27`, `features = ["production"]`), so the convention exists in
this repo and vmem is the outlier.

Consequence: docs.rs builds `aligned-vmem` with **default features**, and this
crate's default feature set is **empty** (`crates/vmem/Cargo.toml:15-70` — every
one of the six features is opt-in, none listed under a `default = [...]` key,
which does not exist). So docs.rs for 0.2.0 will not contain:

- `reserve_aligned_lazy` / `try_reserve_aligned_lazy` / `commit_range` /
  `try_commit_range` (`lib.rs:656-790`, gated `lazy-commit`)
- `reserve_aligned_huge` / `try_reserve_aligned_huge` (`lib.rs:792-821`, gated
  `huge-pages`)
- the whole `mock` module (`lib.rs:98-99`)
- the whole `fault_injection` module (`lib.rs:101-102`)
- the `bench-internals` counters and accessors (`lib.rs:143-205`)

`crates/vmem/README.md:52-62` advertises `lazy-commit`, `huge-pages`, `mock` and
`fault-injection` by name, and `README.md:47` lists `try_commit_range` in the API
table — a reader who clicks the docs.rs badge (`README.md:4`) will find none of
them. Suggested fix, mirroring the root crate's pattern:

```toml
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

(`all-features = true` is safe here — `--all-features` compiles and tests clean,
see §2 — though note it turns `mock` on, which replaces the syscall backend; if
that is undesirable for the rendered docs, list
`features = ["lazy-commit", "huge-pages", "fault-injection"]` explicitly instead.)

### README quality

`crates/vmem/README.md` is 97 lines and is **real, not a stub**: badges (`:3-5`),
a positioning paragraph (`:7-12`), an install snippet pinning `"0.2"` (`:14-17`)
that matches the local version, a working usage example (`:19-33`), a full API
table (`:35-49`), a feature list (`:51-62`), a backend summary (`:64-66`), an
explicit "why not `region`/`memmap2`/`mmap-rs`" differentiation section
(`:68-77`), the alignment contract (`:79-87`), a provenance/safety section
(`:89-93`), and licensing (`:95-97`). This is above the bar for a published
crate.

### Description accuracy — minor nit (P3)

`Cargo.toml:7` describes the technique as "(over-reserve + trim)". Since 0.1.0
that is no longer the *first* thing tried on Unix: `unix_reserve`
(`lib.rs:1123-1188`) tries an exact-size `mmap` fast path first
(`try_reserve_aligned_exact`, `lib.rs:1192`) and only falls through to
over-reserve + trim on an alignment miss (commit `bcf4d79`, "reserve-then-commit-exact
instead of over-commit-then-trim"). On Windows it over-reserves `size + align`
and never trims at all (`win_reserve_commit`, `lib.rs:891-960` — Windows cannot
partially release a `MEM_RESERVE` region; noted in `lib.rs:130-133`). The
description is not *false* (over-reserve + trim is still the fallback and still
the Windows shape minus the trim), just no longer the whole story. Not worth
blocking on.

---

## 2. Build / test / lint health, standalone

All three commands run from the repo root against the workspace member.

### `cargo test -p aligned-vmem --all-features` — **PASS**

```
tests/lazy_commit.rs   9 passed; 0 failed
tests/mock.rs          5 passed; 0 failed
tests/smoke.rs        10 passed; 0 failed
tests/fault_injection.rs  0 passed  <-- see note
lib (unit)             0 passed
Doc-tests              0 passed
```

24 tests, all green, exit code 0.

**Note (not a defect, but worth knowing):** `tests/fault_injection.rs` runs
**zero** tests under `--all-features`. This is deliberate and documented —
`crates/vmem/tests/fault_injection.rs:15-19` gates the whole file on
`#![cfg(all(feature = "fault-injection", feature = "lazy-commit", not(feature = "mock")))]`,
with a comment explaining that under `mock` the real `try_commit_range` is
replaced by the recording stub, so the tests "would produce a vacuous no-op
test, which would be worse than not running it". Correct reasoning; but it does
mean `--all-features` is *not* a superset run for this crate — a separate
`cargo test -p aligned-vmem --features "fault-injection lazy-commit"` invocation
is needed to actually exercise those 5 tests. Nothing in the repo does that:
`scripts/check-all.mjs` and `scripts/check-matrix.mjs` contain no `-p` flag and
never name `aligned-vmem` (so `npm run check` does not touch workspace members at
all), and CI's only crate-scoped run is default-features
(`.github/workflows/ci.yml:696`). See §7 F2.

Zero doctests, consistent with CLAUDE.md's "No doctests" rule — `lib.rs:39-56`
uses a ` ```text ` fence and `lib.rs:58` points at `tests/smoke.rs` for the
runnable form.

### `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` — **PASS**

Exit code 0, no diagnostics. Also re-ran **without** `--all-features`
(`cargo clippy -p aligned-vmem --all-targets -- -D warnings`) — also clean.

Caveat, P3: `lib.rs:81` is a crate-wide
`#![cfg_attr(feature = "mock", allow(dead_code))]`. Under `--all-features`
(`mock` on) `dead_code` is suppressed for the *entire crate*, so the
`--all-features` clippy row is weaker than it appears — it cannot report an
unused item anywhere. The default-feature row above compensates. A tighter fix
would move the `allow` onto the specific `*_impl` helpers it is meant for
(`lib.rs:78-80` explains the intent) rather than the crate root.

### `cargo doc -p aligned-vmem --all-features --no-deps` — **PASS with 4 warnings (F4)**

Exit code 0 (rustdoc lints are `warn`-by-default, not denied), but four
diagnostics:

1. `crates/vmem/src/fault_injection.rs:8` — unresolved link to `try_commit_range`
   (the item is at the crate root, not in scope inside the `fault_injection`
   module; needs `crate::try_commit_range`, which the sibling module already does
   correctly at `error.rs:3-4`).
2. `crates/vmem/src/lib.rs:146` — unresolved link to `try_reserve_aligned_exact`
   (that fn is **private**, `lib.rs:1192`, and Unix-only, so it can never resolve
   from a public doc — should be plain backticks, not an intra-doc link).
3. `crates/vmem/src/lib.rs:153` — same, second occurrence.
4. `crates/vmem/src/lib.rs:161` — "public documentation for
   `WINDOWS_RESERVE_COMMIT_CALLS` links to private item `win_reserve_commit`"
   (`win_reserve_commit` is private, `lib.rs:891`).

`cargo doc -p aligned-vmem --no-deps` (default features) is **clean, 0
warnings** — all four live in `bench-internals`- / `fault-injection`-gated doc
comments. So, ironically, F3 currently *hides* F4 on docs.rs; fixing F3 without
fixing F4 would surface all four as unrendered links on the published docs.
Fix both together.

---

## 3. Packaging

### `cargo package -p aligned-vmem --list` — 15 files, all legitimate

```
.cargo_vcs_info.json   Cargo.lock            Cargo.toml
Cargo.toml.orig        LICENSE-APACHE        LICENSE-MIT
README.md              src/error.rs          src/fault_injection.rs
src/lib.rs             src/mock.rs           tests/fault_injection.rs
tests/lazy_commit.rs   tests/mock.rs         tests/smoke.rs
```

Nothing that should not ship. Specifically checked and **absent**: internal
review docs, `docs/checkpoints/`, `docs/perf/_raw_*.log`, `scripts/`,
`package.json`, `.github/`, benchmark fixtures. This is because the crate
directory itself contains only those files — `crates/vmem/` has no `docs/`,
`benches/`, or `examples/` subtree to leak, so no `exclude` key is needed (unlike
the root crate, which needs `Cargo.toml:20-25`).

A scan for leaked absolute local paths (`[A-Za-z]:[\\/]`, `/home/`, `/Users/`)
across `crates/vmem/src`, `crates/vmem/tests`, `README.md` and `Cargo.toml`
returned only `https://` URLs — **no local path leakage**.

### `cargo package -p aligned-vmem --allow-dirty` — **PASS**

```
Packaging aligned-vmem v0.2.0
Packaged 15 files, 120.0KiB (32.8KiB compressed)
Verifying aligned-vmem v0.2.0
Compiling aligned-vmem v0.2.0 (…/package/aligned-vmem-0.2.0)
Finished `dev` profile in 11.75s
```

Exit code 0 — the tarball builds standalone from an extracted copy. 33 KiB
compressed is a healthy size.

Two confirmations from the generated artifacts:

- The normalised `Cargo.toml` in the tarball has **no `[dependencies]` section
  at all** and all six features intact
  (`alloc-lazy-commit`, `bench-internals`, `fault-injection`, `huge-pages`,
  `lazy-commit`, `mock`). Nothing to rewrite from `path` to registry — this crate
  is a true leaf. **This directly de-risks tasks K3/K4/L2**: `aligned-vmem` can be
  published first, unilaterally, with no other crate needing to move.
- `.cargo_vcs_info.json` records `{"git":{"sha1":"2a1ca35…"},"path_in_vcs":"crates/vmem"}`
  with **no `"dirty": true`** flag — i.e. `--allow-dirty` was not actually needed
  here; the crate subtree is clean. A real publish from this SHA would not need
  the flag.

---

## 4. Completeness scan

`grep -rnE 'TODO|FIXME|unimplemented!|todo!\(|XXX|HACK|allow\(dead_code\)|unreachable!'`
over `crates/vmem/src/` and `crates/vmem/Cargo.toml`:

- **Zero** `TODO`, `FIXME`, `XXX`, `HACK`, `unimplemented!`, `todo!()`,
  `unreachable!`.
- Three `allow(dead_code)`, all with an in-place justifying comment:
  - `lib.rs:81` — crate-wide under `mock` (`lib.rs:77-80` explains: the real
    `*_impl` helpers become legitimately unused when the recording backend
    replaces them). See the P3 nit in §2.
  - `lib.rs:87-90` — the `fault-injection`-without-`lazy-commit` combination
    (`lib.rs:82-86` explains the hook is compiled-but-unreachable there).
  - `lib.rs:1478` — on `enum DecommitKind`, "unused under the `mock` feature".

One `#[deprecated]`, `lib.rs:315-317`, on `Reservation::is_empty` — with a full
explanatory doc block (`lib.rs:305-314`) saying the method is always `false` for
any valid handle. **P3 nit:** the attribute has a `note` but no
`since = "0.2.0"`, so rustdoc renders "Deprecated" with no version. Cheap to add.

**No half-wired features found in `src/`.** The one genuinely under-finished
thing is not in the source but in the surrounding test/CI/doc envelope — see F1
and F2 below.

### F1 (P1) — `huge-pages` documents macOS behaviour it does not implement

Two places claim macOS large-page support:

- `crates/vmem/Cargo.toml:31` — "macOS `VM_FLAGS_SUPERPAGE` best-effort via
  `MADV_HUGEPAGE`"
- `crates/vmem/src/lib.rs:780-781` — "macOS best-effort `MADV_HUGEPAGE`"

(`crates/vmem/README.md:54-55` is **accurate** — it names only "`MAP_HUGETLB` /
`MEM_LARGE_PAGES`, best-effort with fallback" and makes no macOS claim. Only the
Cargo.toml comment and the rustdoc overclaim.)

The code does neither. `libc_madvise_hugepage` has two definitions:

- `lib.rs:1400-1405`, `cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))`
  — issues the real `madvise(MADV_HUGEPAGE)`.
- `lib.rs:1407-1412`, `cfg(all(unix, not(miri), not(target_os = "linux"), feature = "huge-pages"))`
  — **an empty no-op body**, comment: "Non-Linux Unix: no transparent-huge-page
  madvise".

And `MAP_HUGETLB` is only OR-ed in under `target_os = "linux"`
(`lib.rs:1367-1369`). `VM_FLAGS_SUPERPAGE` appears **nowhere** in the source
(`grep` returns only the Cargo.toml comment). So on macOS,
`reserve_aligned_huge` is byte-for-byte identical to `reserve_aligned` — a
silent no-op wearing a feature name. Fix is documentation-only: say "Linux
`MAP_HUGETLB` + `MADV_HUGEPAGE`, Windows `MEM_LARGE_PAGES`; **no-op on macOS and
other Unix**".

Two related P3 observations in the same code, neither a correctness bug:

- `lib.rs:1180-1184`: after the over-reserve path falls back from a failed
  hugetlb `mmap` to an ordinary one (`lib.rs:1139-1148`), the local `huge` flag
  is **not** cleared, so `libc_madvise_hugepage` is still applied to the
  ordinary mapping. Harmless (THP hint on a normal mapping is exactly what you'd
  want anyway), but the variable no longer means what its name says.
- The documented "never fails purely because huge pages are unavailable"
  guarantee (`lib.rs:784-787`, `Cargo.toml:33-35`) **is** genuinely implemented
  on both platforms: Unix retries `libc_mmap(over, false)` at `lib.rs:1139-1148`;
  Windows retries the commit without `MEM_LARGE_PAGES` at `lib.rs:948-966`. Claim
  verified, not overclaimed.

---

## 5. Public API doc coverage

**The crate already sets `#![deny(missing_docs)]` — `crates/vmem/src/lib.rs:76`**
(and has since 0.1.0: `git show d95ea7f:crates/vmem/src/lib.rs` line 53). It is
**clean**: `cargo doc --all-features` produced zero `missing_docs` diagnostics,
and the crate compiles under `-D warnings`, so every public item in every feature
configuration is documented by construction. `#![warn(missing_docs)]` would add
nothing — the stronger lint is already on.

Spot-checking quality rather than presence, the docs are unusually thorough for a
0.x crate:

- Crate-level module doc `lib.rs:1-73` covering positioning, the fallible-vs-infallible
  API split, the alignment contract, and the `PAGE`-vs-`page_size()` distinction.
- Every `unsafe fn` carries a real `# Safety` section, not a placeholder — e.g.
  `Reservation::from_raw_parts` (`lib.rs:371-392`) enumerates all five parameters'
  obligations plus the double-release hazard plus a Windows-specific
  `MEM_RESERVE | MEM_COMMIT` requirement.
- `release` (`lib.rs:488`), `decommit` (`lib.rs:519`), `decommit_lazy`
  (`lib.rs:553`), `recommit` (`lib.rs:590`), `try_recommit` (`lib.rs:602`),
  `commit_range` (`lib.rs:657`), `try_commit_range` (`lib.rs:669`) — all documented.
- The `error` (`error.rs:1-7`), `mock` (`mock.rs:1-…`) and `fault_injection`
  (`fault_injection.rs:1-30`) module docs each explain not just *what* but *why
  this and not the other one* — `fault_injection.rs:3-13` in particular spells
  out precisely how it differs from `mock`.

One **P2 documentation-hardening suggestion**, prompted by a real in-repo
consumer bug: `decommit`'s doc (`lib.rs:507-510`) says "Re-access after decommit
produces fresh zero-filled pages (after [`recommit`] on Windows; implicitly on
Unix)". That is technically correct but understates the consequence — on Windows
`MEM_DECOMMIT` (`decommit_pages_impl`, `lib.rs:970-979`) genuinely unmaps the
pages, so a write before `recommit` is a **hard access violation**, whereas on
Linux `MADV_DONTNEED` silently re-faults a zero page. This exact divergence is a
tracked open item in this repo — `docs/CORRECTNESS_OPEN_ITEMS.md:370-397` records
`examples/r29_3_decomposition_gate.rs` crashing with `STATUS_ACCESS_VIOLATION` on
native Windows because it assumed the Linux semantics. Note that the `# Safety`
block on `decommit` (`lib.rs:515-518`) says nothing about this at all. If the
crate's own sibling tripped on it, an external consumer will too. Suggested:
promote the divergence into the `# Safety` section in an explicit
"on Windows, writing into the range before `recommit` is UB / an access
violation" sentence.

---

## 6. Semver sanity — 0.2.0 (minor) is **correct**

16 commits touched `crates/vmem/` since the 0.1.0 publish
(`git log --oneline --since=2026-06-29T17:31:18Z -- crates/vmem/`).

**There is a genuine breaking change, so this cannot be a 0.1.x release.** The
signature of a public `unsafe fn` changed:

- 0.1.0 (`git show d95ea7f:crates/vmem/src/lib.rs:315`):
  `pub unsafe fn recommit(base: *mut u8, start: usize, end: usize)` → `()`
- 0.2.0 (`crates/vmem/src/lib.rs:590`):
  `#[must_use] pub unsafe fn recommit(base: *mut u8, start: usize, end: usize) -> bool`

That is commit `617518f` ("make Windows recommit fallible, honest OOM instead of
AV crash"). A `()` → `bool` return-type change plus a new `#[must_use]` breaks
any caller that used it in statement position under `-D warnings`. Under Cargo's
0.x semver rules (`0.MINOR.PATCH`, where MINOR is the compatibility axis),
breaking → **minor bump**. `0.2.0` is right.

Everything else in the delta is additive or non-breaking and would not on its own
have forced the bump:

- **Additive:** the whole `try_*` `Result` API + `VmemError` (`4ec1516`),
  `page_size()`, `decommit_lazy`, `leak_zeroed_pages`, `reserve_aligned_lazy` /
  `commit_range` behind `lazy-commit` (`e5310a0`), `reserve_aligned_huge` behind
  `huge-pages`, the `mock` module, the `fault_injection` module (`9d6c9f4`), the
  `bench-internals` counters (`f6c3a61`).
- **Non-breaking deprecation:** `Reservation::is_empty` deprecated, not removed
  (`7b4acb3`, `lib.rs:315`).
- **Feature rename handled correctly:** `alloc-lazy-commit` → `lazy-commit`, with
  the old name retained as a forwarding alias
  (`crates/vmem/Cargo.toml:22-27`) so downstream pins keep building. Textbook.
- **Internal:** `bcf4d79` (exact-size mmap fast path), `d95ea7f`/`b37ef98`
  (platform-constant + clippy fixes), `f97cf1f` (rustfmt).

**Not 1.0.0-worthy yet, in my read** — and I'd argue against it: `huge-pages` is
untested (F2), the `no-std::no-alloc` category needs correcting (F5), and
`Reservation::is_empty` is deprecated-but-present. A 1.0 should ship after the
deprecation is actually removed and the untested feature is either covered or
dropped, both of which are themselves breaking-ish. 0.2.0 now, 1.0.0 later, is
the right shape.

**P3, cross-repo consistency nit:** the root crate still consumes the crate via
the *deprecated alias* — `Cargo.toml:770` and `Cargo.toml:790` both list
`"aligned-vmem/alloc-lazy-commit"`, while `Cargo.toml:365` uses the new
`"aligned-vmem/lazy-commit"`. Since the alias is documented as kept "for one
release" (`crates/vmem/Cargo.toml:24-27`), the in-repo consumer should migrate to
the new name now, or the alias will have to survive past its stated sunset.

---

## 7. Performance angle — **no queued speedup justifies holding the publish**

I read both open-items indexes end-to-end for vmem-relevant entries and grepped
`crates/vmem/src/` for perf-flavoured comments.

### The one real perf item is **closed, with a rejection verdict**

`docs/perf/OPEN_ITEMS.md:2080` (the Round-32 closure table):

> | F11 | Windows segment reservation over-reserves 2× VA, no aligned-reservation
> fast path; Unix fast-path hit rate unmeasured | **Rejected-with-evidence (step 3
> declined)** — Unix/Windows counters shipped; first Windows-native reserve/commit
> decomposition found the avoidable share **4.3-4.8%** (well under materiality),
> page-fault cost still dominant (~95.4%); `VirtualAlloc2` explicitly declined |
> #504 | `f6c3a61e…` |

Corroborated at `docs/perf/OPEN_ITEMS.md:1335-1352`, which explicitly states the
finding is "recorded here only as the first quantified data point on that
surface, not a resolution", and at `:1918-1921`, which notes F11 "found the
reservation-path share of that signal small — 4.3-4.8%".

Translation for this decision: **the only known optimisation candidate inside
this crate — a Windows `VirtualAlloc2` aligned-reservation fast path that would
remove the 2× VA over-reservation and one of the two syscalls — was measured,
found to be worth at most ~4.3-4.8% of a fresh-segment cycle (page faults are
~95.4%), and explicitly declined.** It is not queued work. There is no pending
change that would alter `aligned-vmem`'s public API or its observable behaviour.

What actually *did* land from that investigation is already in the tree and
already in this version: the `bench-internals` counters
(`UNIX_EXACT_RESERVE_ATTEMPTS` / `_HITS` / `WINDOWS_RESERVE_COMMIT_CALLS`,
`lib.rs:143-205`, commit `f6c3a61`), which are observation-only, opt-in, and
compiled out by default (`lib.rs:1197`, `:1213`, `:948`, `:957` are the only
increment sites, all `#[cfg(feature = "bench-internals")]`).

### Other index hits — neither is a vmem defect

- `docs/CORRECTNESS_OPEN_ITEMS.md:370-397` — the native-Windows
  `STATUS_ACCESS_VIOLATION` in `examples/r29_3_decomposition_gate.rs`. The root
  cause is correctly attributed to a **consumer** assuming Linux `MADV_DONTNEED`
  semantics for Windows `MEM_DECOMMIT`; the fix named there is in the example, not
  in vmem. It does motivate the §5 doc-hardening suggestion, but not a code change
  or a publish hold.
- `docs/CORRECTNESS_OPEN_ITEMS.md:1417` — `aligned-vmem` merely named in the list
  of crates that `release.yml` knows how to publish. Not a defect.

### Source-level perf comments

No `TODO(perf)`-style markers anywhere in `crates/vmem/src/` (§4). The perf-relevant
prose that exists (`lib.rs:118-141`) is the *rationale* for the shipped counters and
records the F11 conclusion, not a promise of future work.

### F2 (P1) — the real gap here is coverage, not speed

`huge-pages` is referenced **nowhere** outside `crates/vmem/src/lib.rs` and its own
Cargo.toml stanza. Verified:

```
grep -rn 'huge-pages|huge_pages|reserve_aligned_huge' \
     .github/workflows/ci.yml Cargo.toml crates/vmem/tests/ tests/
→ (no matches)
```

- No test in `crates/vmem/tests/` touches `reserve_aligned_huge`.
- The crate-scoped CI job runs it with no features. `.github/workflows/ci.yml:683-696`
  (`test-workspace`) runs
  `cargo test -p aligned-vmem -p sefer-region -p malloc-bench-rs --no-fail-fast` on
  `ubuntu-latest` with **default features only** — and this crate's defaults are
  empty. Since `tests/lazy_commit.rs:7`, `tests/mock.rs:5` and
  `tests/fault_injection.rs:15-19` are each `#![cfg(feature = …)]`-gated on a vmem
  feature, that job compiles all four test files to **zero tests each** and
  effectively runs only `tests/smoke.rs`'s 10 tests. The crate's own
  feature-gated suite (9 + 5 + 5 tests) has never run in CI on any platform.
- The root crate never forwards it. Root `Cargo.toml` forwards
  `aligned-vmem/lazy-commit` (`:365`), `aligned-vmem/bench-internals` (`:573`),
  `aligned-vmem/alloc-lazy-commit` and `aligned-vmem/fault-injection`
  (`:770-771`, `:790-791`) — but **never** `aligned-vmem/huge-pages` and never
  `aligned-vmem/mock` (`grep -rn 'aligned-vmem/mock|aligned-vmem/huge' Cargo.toml
  crates/*/Cargo.toml` matches only a prose comment at `Cargo.toml:717`).

To be precise about what CI *does* cover: `aligned-vmem` **is** compiled as a
dependency on Linux with `lazy-commit` + `fault-injection` + `bench-internals`
turned on, via the root crate's `--all-features` rows
(`.github/workflows/ci.yml:111-112` clippy, `:424-426` test) and the MSRV job
(`:698-710`, `cargo check --all-features` on 1.88), and with `lazy-commit` on real
Windows via `.github/workflows/ci.yml:660`
(`production exact-span-large large-reserved-capacity internals`). The gap is
narrower than "no features are ever compiled" — it is precisely **`huge-pages`
and `mock`**, the two features no consumer forwards.

For `mock` that is tolerable: it has 5 dedicated tests
(`crates/vmem/tests/mock.rs`) that pass locally, and it is a test-only backend
that cannot affect a production consumer. For `huge-pages` it is not: the Linux
`MAP_HUGETLB` path (`lib.rs:1367-1369`) and both `libc_madvise_hugepage` bodies
(`lib.rs:1400-1412`) have **never been compiled by CI on any OS, and have no test
anywhere**. The only compilation they have ever received is a developer's local
`--all-features` build — which on this Windows host exercises only the
`MEM_LARGE_PAGES` branch (`lib.rs:1019-1025`), never the Linux one.

**One specific unverified hazard I want to name, marked PLAUSIBLE not CONFIRMED**
(I have no Linux host here and did not run it): in the over-reserve fallback
(`lib.rs:1173-1179`), the head and tail are trimmed with `libc_munmap` at
offsets derived from the caller's `align`. Under a successful `MAP_HUGETLB`
mapping, Linux requires `munmap` ranges to be huge-page-granular; a caller
passing `align` smaller than the huge page size (the API's documented floor is
`PAGE` = 4 KiB, `lib.rs:111`) would get `EINVAL` on the trims. `libc_munmap`
discards the return value (`lib.rs:1389-1392`, `let _ = munmap(...)`), so the
failure would be **silent** and the head/tail would stay mapped — an address-space
leak, not memory-unsafety, and only reachable when hugetlb pages are actually
configured on the machine. This is exactly the class of thing a single Linux
integration test would settle in minutes.

**Recommendation (maintainer's call):** either (a) add one `huge-pages` smoke test
plus a CI row that compiles the crate's own features on Linux/macOS, or (b) mark
`huge-pages` as experimental in `Cargo.toml:30-36` and the README, or (c) drop the
feature from 0.2.0 and reintroduce it when covered. Publishing an advertised,
never-CI-compiled feature is the single weakest part of this crate's release
posture — but it is opt-in, off by default, and cannot affect a consumer who does
not enable it, which is why this is GO-WITH-FIXES and not NO-GO.

---

## Summary of findings

| # | Sev | File:line | Finding |
|---|-----|-----------|---------|
| F1 | P1 | `crates/vmem/Cargo.toml:31`, `src/lib.rs:780-781` | `huge-pages` documents macOS `VM_FLAGS_SUPERPAGE`/`MADV_HUGEPAGE`; the non-Linux body (`src/lib.rs:1407-1412`) is an empty no-op and `VM_FLAGS_SUPERPAGE` appears nowhere in the source (README is accurate here) |
| F2 | P1 | `.github/workflows/ci.yml:696`; no matches in `crates/vmem/tests/` | `huge-pages` has zero test, zero CI, zero consumer coverage; the Linux `MAP_HUGETLB` path has never been compiled by CI. The crate-scoped CI job also compiles 3 of its 4 test files to zero tests (default features). Includes a PLAUSIBLE silent-VA-leak hazard at `src/lib.rs:1173-1179` + `:1389-1392` |
| F3 | P1 | `crates/vmem/Cargo.toml` (absent), cf. root `Cargo.toml:26-27` | No `[package.metadata.docs.rs]`; defaults are empty, so docs.rs 0.2.0 will omit every optional API the README advertises |
| F4 | P2 | `src/fault_injection.rs:8`, `src/lib.rs:146`, `:153`, `:161` | 4 rustdoc broken/private intra-doc link warnings under `--all-features` (clean under default features) |
| F5 | P2 | `crates/vmem/Cargo.toml:13` | `no-std::no-alloc` category is false — `std` used at `src/error.rs:96,100,105` and `src/mock.rs:101`; no `#![no_std]` anywhere |
| F6 | P2 | `src/lib.rs:507-518` | `decommit`'s `# Safety` omits that a pre-`recommit` write on Windows is a hard AV; this exact divergence already burned an in-repo consumer (`docs/CORRECTNESS_OPEN_ITEMS.md:370-397`) |
| F7 | P3 | `src/lib.rs:315-317` | `#[deprecated]` on `Reservation::is_empty` has no `since = "0.2.0"` |
| F8 | P3 | `src/lib.rs:81` | Crate-wide `allow(dead_code)` under `mock` makes the `--all-features` clippy row unable to report unused items |
| F9 | P3 | root `Cargo.toml:770`, `:790` | Root crate still consumes the deprecated `aligned-vmem/alloc-lazy-commit` alias while `:365` uses the new name |
| F10 | P3 | `crates/vmem/Cargo.toml:7` | `description` says "(over-reserve + trim)"; Unix now tries exact-size mmap first (`src/lib.rs:1192`) and Windows never trims (`src/lib.rs:891-960`) |
| F11 | P3 | `crates/vmem/tests/fault_injection.rs:15-19` | `--all-features` is not a superset run — the 5 fault-injection tests are skipped when `mock` is on (deliberate and documented, but means a second invocation is needed for full coverage) |

---

## Open questions for the maintainer

1. **F2 — what happens to `huge-pages` for 0.2.0?** Three options: cover it (a
   Linux smoke test + a CI row that compiles this crate's own feature set on
   Linux/macOS), label it experimental, or drop it from the release. Publishing an
   advertised feature that CI has never compiled is a deliberate risk acceptance,
   and it should be a conscious one. My preference: label experimental now, cover
   it before 1.0.
2. **F3 — `all-features = true` or an explicit list for docs.rs?**
   `all-features = true` turns `mock` on, which replaces the syscall backend; the
   rendered docs would still be correct (the `mock` gating is invisible in the
   signatures) but the `mock` module would appear as public API on docs.rs. If
   that is unwanted, `features = ["lazy-commit", "huge-pages", "fault-injection"]`
   is the narrower choice. Which?
3. **F5 — is `no-std::no-alloc` intentional aspiration or an oversight?** If there
   is a plan to make the core `no_std` (it is close — only `error.rs`'s
   `std::error::Error`/`std::io::Error` and `mock.rs`'s `thread_local!` stand in
   the way), the category could stay with a tracked item; otherwise it should be
   dropped from `Cargo.toml:13`. Note it shipped in 0.1.0 too, so removing it is a
   (cosmetic) change in listed metadata.
4. **F9 — should the `alloc-lazy-commit` alias sunset in 0.3.0?** If yes, the root
   crate's own two references (`Cargo.toml:770`, `:790`) should migrate in this
   release so the alias has no in-repo consumer left when it goes.
5. **MSRV 1.88 is only partially gated.** `.github/workflows/ci.yml:698-710` runs
   `cargo check --all-features` on the pinned 1.88 toolchain at the **repo root**,
   which does compile `aligned-vmem` as a dependency with `lazy-commit` +
   `fault-injection` + `bench-internals`. It never compiles `huge-pages` or
   `mock`, and never checks the crate standalone (`-p aligned-vmem
   --all-features`), so `rust-version = "1.88"` on `crates/vmem/Cargo.toml:5` is a
   partially-unchecked promise for a crate about to be consumed independently.
   Worth adding `cargo check -p aligned-vmem --all-features` to that job — one
   line, and it closes F2's compile axis at the same time.
6. **Publish ordering confirmation.** `aligned-vmem` is a true leaf (no
   `[dependencies]`, packaged manifest confirms nothing to rewrite), so it can go
   first with no prerequisites. Confirm the intended order is
   `aligned-vmem 0.2.0` → `numa-shim` → … → `sefer-alloc 0.3.0`, matching
   `docs/plans/2026-08-05-release-execution-map.md:189`.
