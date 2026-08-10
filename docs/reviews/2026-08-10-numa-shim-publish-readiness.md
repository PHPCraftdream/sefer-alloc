# numa-shim — publish-readiness investigation (task #657)

Investigation only — no code changes, no version bump, no `cargo publish`.
Per task #657's own scope: report findings, let the maintainer decide.

## 1. Blocking finding: `cargo publish --dry-run -p numa-shim` FAILS TODAY

```
$ cargo publish --dry-run -p numa-shim
error: failed to prepare local package for uploading

Caused by:
  failed to select a version for the requirement `aligned-vmem = "^0.2"`
  candidate versions found which didn't match: 0.1.0
  location searched: crates.io index
  required by package `numa-shim v0.1.0 (D:\dev\rust\sefer-alloc\crates\numa)`
```

`crates/numa/Cargo.toml`'s optional dependency reads
`aligned-vmem = { version = "0.2", path = "../vmem", optional = true }`
(bumped in commit `4ec1516`, 2026-07-17, when `aligned-vmem` itself moved to
0.2 locally). But `aligned-vmem` 0.2.0 has never been published — it is
still 0.1.0 on crates.io (this is exactly task #658's own open item: "local
already bumped, crates.io still shows 0.1.0"). A packaged `numa-shim` build
resolves dependencies against the crates.io index, not the local path, so
publishing `numa-shim` today would try to pull `aligned-vmem ^0.2` from
crates.io and fail to resolve — Cargo's dependency resolution does not fall
back to the `path` reference for a published-crate build.

**Consequence: task #658 (publish `aligned-vmem` 0.2.0) is a hard
prerequisite for task #657's actual publish step, even though it was not
listed in #657's `blockedBy`.** The local-only `path = "../vmem"` masks this
during workspace development (`cargo build`/`cargo test` in this repo never
hit the crates.io resolution path), which is why it went unnoticed until
this dry-run.

## 2. Commit count since the 2026-06-29 publish: 22, not 11

The task's own description cited "11 commits... since the 2026-06-29
publish date" — that count predates this round's numa-shim rust-intel
remediation (tasks #697, #720-727, #778), which landed after task #657 was
originally filed. `git log --oneline --since="2026-06-30" -- crates/numa`
returns 22 commits as of this investigation. Classified below.

### Real behavioral/correctness fixes never published (user-facing on 0.1.0)

| Commit | What it fixed |
|---|---|
| `f97bf1d` | **HIGH** — `current_node()`'s `OnceLock` topology cache deadlocks/UBs under `sefer-alloc-as-global` + `numa-aware` (task #777) |
| `b275480` | `mbind` `maxnode` off-by-one silently drops CPU node 63's bit — real ABI divergence from the syscall's documented contract (task #697) |
| `3899ab9` | A single 256-byte cpumap read was treated as complete — silent truncation, wrong-node answers on hosts with >900 CPUs (task #720) |
| `2efa70f` | Windows path committed `size + align` instead of `size`, doubling commit charge (task #724) |
| `dc003c9` | macOS + miri: duplicate-definition `E0428` compile error (already known per this task's own description — "never published") |
| `7ea2798` | Pre-0.1.0-era bundle: fallback livelock (F1), warnings-clean restore (F3), flaky bulk-bypass test (F4), miri align tests (F5) |
| `9b48844` → `2cdb765` → `f97bf1d` | `current_node()` gained a syscall-avoiding cache (perf), which then needed a deadlock/UB fix — net: real behavior change across three commits, not one |

### API-surface / semver-relevant changes

| Commit | What changed |
|---|---|
| `53b3ca2` | Publish-surface decisions (task #726): mock thread-locals visibility, `MockCall` variant-level `#[non_exhaustive]`, `CALLS` `Vec` bound, mock/real backend unification behavior |
| `69045e3` | 4 doc/code semantics divergences closed (task #722) — behavior clarified/changed to match intended contract in some cases |
| `f989bed` | `bind_range`'s `# Safety` contract scoped to when it actually applies (a documentation correction, but one that narrows a previously-overbroad safety claim) |

### Internal-only (tests, CI, docs, hygiene — no publish-relevant behavior change)

`94c4a74`, `c5e013b`, `0a42519`, `19698da`, `7e1020f`, `dbfeca3`, `30b7082`,
`c5469f5`, `d542f51`, `fd2a3bb` (the F2-F13 round-closing bundle — mixed,
mostly docs/tests/CI; no new behavioral fix beyond what's already listed
above), and `4ec1516`'s one-line touch to `crates/numa/Cargo.toml` (just the
`aligned-vmem` dependency version bump discussed in §1 — not a numa-shim
behavior change).

## 3. Recommended semver bump: **at least 0.2.0 (minor), not a patch**

Two independent reasons converge on minor, not patch:

1. `f97bf1d`'s deadlock/UB fix and `b275480`/`3899ab9`'s correctness fixes
   are real behavioral changes to already-published 0.1.0 functions — under
   strict semver these could be argued as patch-eligible (bugfixes), but
   the combination with reason 2 below tips it to minor.
2. `53b3ca2`'s publish-surface decisions include a `#[non_exhaustive]`
   addition to `MockCall` — adding `#[non_exhaustive]` to a previously
   exhaustive-matchable enum is a real API-surface change (existing
   exhaustive `match` call sites in downstream code would newly need a
   wildcard arm), which is minor-bump territory, not patch.

This mirrors `aligned-vmem`'s own precedent (task #658: 0.1→0.2 for a
comparable mix of new API + behavior changes). **Not applying the bump here
— per this project's standing "never bump versions without explicit
request" rule.**

## 4. Packaging and metadata — clean

`cargo package --list -p numa-shim` succeeds and lists exactly the expected
files (`Cargo.toml`, `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`,
`src/lib.rs`, three test files). `crates/numa/Cargo.toml`'s metadata block
(`description`, `readme`, `repository`, `homepage`, `documentation`,
`keywords`, `categories`) is present and accurate — no gaps found. The
`[package.metadata.docs.rs]` `vmem-integration`-only feature list (task
#644) and the `mock` feature's extensive non-additive-hazard doc comment
(task #726) are both already in place and current.

## Summary

- **Blocking**: `aligned-vmem` 0.2.0 must publish to crates.io before
  `numa-shim` can be republished at all (dependency resolution failure,
  reproduced above) — this makes task #658 a hard prerequisite of #657,
  not just a sibling task.
- **Recommended bump**: 0.1.0 → 0.2.0 (minor), given both real
  behavioral fixes (including one HIGH-severity deadlock/UB) and a
  `#[non_exhaustive]` API-surface addition since the last publish.
- **Packaging/metadata**: clean, no changes needed.
- Version bump and `cargo publish` are NOT performed here — task #657's
  own scope reserves the actual publish/republish decision for the
  maintainer, and the standing project rule against unrequested version
  bumps applies regardless.
