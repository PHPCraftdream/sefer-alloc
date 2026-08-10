# aligned-vmem — publish-readiness investigation (task #658)

Investigation only — no code changes, no version bump (already at 0.2.0
locally), no `cargo publish`. Per task #658's own scope: report findings,
let the maintainer decide. This is the most release-critical of the six
crate-publish tasks: **`sefer-alloc` itself cannot be published today**
without this one landing, since root `Cargo.toml` already requires
`aligned-vmem = { path = "crates/vmem", version = "0.2", optional = true }`
(line 860) and no `aligned-vmem` 0.2.x exists on crates.io yet.

## 1. Why the local version is already 0.2.0

Commit `4ec1516` (2026-07-17, "feat(vmem): aligned-vmem 0.2 — OS page_size,
try_* Result API, lazy-commit/MADV_FREE/huge-pages, mock+fault-injection,
leak_zeroed_pages") performed the bump. Its own message states the rationale
explicitly and states the bump was "explicitly approved by the user" at the
time:

- Real `page_size()` via `sysconf(_SC_PAGESIZE)`/`GetSystemInfo`
  (correctness fix for macOS's 16 KiB pages — previously assumed 4 KiB).
- New fallible `try_*` API returning `Result<_, VmemError>` (new
  `src/error.rs`) alongside the existing infallible surface.
- `alloc-lazy-commit` feature renamed to `lazy-commit` — **the old name was
  kept as an alias for one release**, per the commit message.
- New `decommit_lazy` (`MADV_FREE`/`MADV_FREE_REUSABLE`/eager-on-Windows).
- New optional `huge-pages` feature.
- New `mock` feature (call-log recording + fault injection).
- New `leak_zeroed_pages(size)` helper.

This is a real, substantial API-surface expansion (not just internal
churn) — a minor bump (0.1→0.2) is the correct call, already made, and
does not need revisiting.

## 2. 30 commits since the 2026-06-29 0.1.0 publish (task's own count was 20)

The task description's "20 commits" predates this round's aligned-vmem
rust-intel remediation (tasks #699, #712-719, #776), which landed after
the task was filed. `git log --oneline --since="2026-06-30" --
crates/vmem` returns 30 commits. The 0.2.0 API surface described in §1
(commit `4ec1516`) already accounts for the largest single behavioral
jump; the remainder splits into:

### Real correctness fixes on top of the already-bumped 0.2.0 surface

| Commit | What it fixed |
|---|---|
| `54089fa` | `recommit`/`commit_range` clamped a contract-VIOLATION case to the SUCCESS sentinel — already crashed an in-repo consumer (task #712, MEDIUM) |
| `131355a` | `VmemError` captured `errno` AFTER intervening cleanup FFI could clobber it — now captured immediately (task #713, MEDIUM) |
| `2e7f4f5` | `_SC_PAGESIZE` wrong on all four BSDs; hugetlb requests with a misaligned size silently leaked instead of being rejected (task #714, two MEDIUM findings) |
| `94aef18` | Two native over-reserve paths used exposed-address `as`-cast round-trips instead of strict provenance (task #717) |
| `81ecfe3` | Huge-pages mock coverage gap + a miri-UB test assertion (task #716) |
| `b8b70fb` | Two real data-race hazards in `fault_injection`'s atomics (task #718, then #775 fixed the regression test's own inability to fail against the pre-fix race) |
| `617518f` | Windows recommit made fallible — honest OOM instead of an access-violation crash (pre-0.2, but still since the 0.1.0 publish date) |
| `bcf4d79` | Reserve-then-commit-exact instead of over-commit-then-trim (behavior change, pre-0.2) |

### API-surface / publish-blocking decisions

| Commit | What changed |
|---|---|
| `e5f6700` | Two publish-blocking API questions decided for `mock` (task #715): `Call` variants gained variant-level `#[non_exhaustive]`; the mock/real backend feature-unification hazard documented |
| `4c059fa` | Narrowed `mock`'s dead-code `#[allow]`, fixed broken doc links (task #646) |

### Internal-only (docs, tests, CI, hygiene, formatting)

`c43cd58`, `289ade3`, `55e71b0`, `0a42519`, `19698da`, `7e1020f`, `ebe615d`,
`f6c3a61` (a new bench, not a behavior change), `f97cf1f`, `dbfeca3`,
`b37ef98`, `ffd3215`, `d3a0162`, `30b7082`, `06c04ba`, `95adec0`, and the
pre-0.2 commits `e5310a0`/`7b4acb3` (both already folded into what `4ec1516`
shipped as 0.2.0's baseline).

**None of these post-`4ec1516` commits changes the 0.2.0 target version
recommendation** — they're bugfixes and hardening on top of the surface
`4ec1516` already bumped to 0.2.0 for, not new API-surface growth that
would argue for 0.3.0. 0.2.0 remains the correct target.

## 3. Packaging — clean

`cargo package --list -p aligned-vmem` lists exactly the expected 16
files (`Cargo.toml`, both `LICENSE-*`, `README.md`, `src/{lib,error,
fault_injection,mock}.rs`, 5 test files). `cargo publish --dry-run -p
aligned-vmem` succeeds end-to-end: packages, verifies (compiles the
packaged tarball in isolation), and reaches the upload step before
aborting on the dry-run flag as expected — no dependency-resolution
failure (unlike numa-shim's, see the companion report
`docs/reviews/2026-08-10-numa-shim-publish-readiness.md` §1), since
`aligned-vmem` itself has zero crate dependencies.

## 4. Metadata — clean; one real gap found

`crates/vmem/Cargo.toml`'s metadata block (`description`, `readme`,
`repository`, `homepage`, `documentation`, `keywords`, `categories`) is
present, accurate, and already reflects the 0.2.0 feature set (the
`[package.metadata.docs.rs]` list explicitly includes `lazy-commit`,
`huge-pages`, `fault-injection` — task #644's fix).

**Gap: `crates/vmem/README.md` has no migration/changelog section for
the 0.1→0.2 jump.** The commit message for `4ec1516` explicitly notes the
old `alloc-lazy-commit` feature name "kept one release" as a
compatibility accommodation — implying the author anticipated real
downstream consumers upgrading from 0.1.0. But nothing in the published
README tells such a consumer what changed or that the old feature name
is deprecated-but-still-working. The root `CHANGELOG.md` (line ~3713)
does document the 0.2 release in the workspace's own internal changelog,
but that file is not part of what ships to crates.io consumers reading
the crate's own README/docs.rs page. This is a documentation gap, not a
packaging or code defect — flagged for the maintainer's judgment on
whether to add a short "Upgrading from 0.1" section before publishing.

## Summary

- **Not blocking** (unlike numa-shim): packaging, dependency resolution,
  and `cargo publish --dry-run` all succeed today at 0.2.0.
- **Version**: 0.2.0 is already correct and well-justified; no further
  bump needed before publishing.
- **Real gap**: no 0.1→0.2 migration note in the published-facing
  README, despite a deliberate one-release compatibility alias
  (`alloc-lazy-commit`) suggesting the author expected real upgraders.
  Worth adding before publish, at the maintainer's discretion.
- `cargo publish` is NOT performed here — the actual publish action
  remains the user's explicit call, per this task's own scope and the
  project's standing rule.
