# Security & Compliance Audit — sefer-alloc 0.3.0

Date: 2026-07-06. Scope: workspace root + `crates/{vmem,numa,region,malloc-bench}`,
dependency tree, CI workflows, licensing, disclosure process, X7 hardened-feature
documentation. Research-only pass; no source or test files were modified.

---

## 1. Security findings

### 1.1 `unsafe` confinement — README claim VERIFIED (with two README-table gaps noted in §1.2)

Method: `grep -rn "unsafe {" / "unsafe fn" / "unsafe impl"` over `src/` and
`crates/*/src`, cross-checked against the README section
"Where unsafe lives — the complete list" and the per-file
`#![allow(unsafe_code)]` / `#![forbid(unsafe_code)]` headers.

**Result: every `unsafe` block in the workspace sits inside a named seam file.**
No stray `unsafe` was found outside the seam list. `src/heap/`, `src/alloc_core/`
(other than `os.rs`/`node.rs`/`numa.rs`), `src/region*`, and `crates/region`
contain zero `unsafe` blocks (only doc-comment mentions).

Inventory (block counts from grep; `// SAFETY:` count per file alongside):

| Seam file | `unsafe {` blocks | `SAFETY` comments | Judgment |
|---|---|---|---|
| `src/alloc_core/node.rs` (lines 84–474, e.g. 84, 101, 122, 134, 145, 167–292, 313, 338, 362, 372, 410, 474) | 22 | 23 | Adequate — every block has a SAFETY comment; contracts (in-bounds offset, alignment, single-writer) are stated per function. |
| `src/alloc_core/os.rs` (160, 176 `unsafe impl Send`, 198, 227, 237) | 4 + 1 impl | 5 | Adequate — thin delegation to `aligned-vmem`; the `Send` impl at :176 carries rationale. |
| `src/alloc_core/numa.rs` (:63) | 1 | present | Adequate — forwards to `numa_shim::bind_range`; clippy `not_unsafe_ptr_arg_deref` allow is justified in a comment (:49–53). |
| `src/concurrent/hand.rs` (212, 368, 392; `unsafe impl Send/Sync` 429/439) | 3 + 2 impls | 13 | Adequate — heavily documented; module is deprecated/legacy (`experimental` only). |
| `src/global/sefer_alloc.rs` (339 `unsafe impl GlobalAlloc`; 341/355/360/376/381/387/392/403) | 4 blocks + trait impl | 6 | Adequate — the trait obligation itself is the seam; bodies delegate to safe `Heap` entry points. |
| `src/global/fallback.rs` (149, 200) | 2 | 5 | Adequate — `static mut MaybeUninit<HeapCore>` init protocol documented. |
| `src/global/tls_heap.rs` (234, 429) | 2 | 3 | Adequate. |
| `src/registry/bootstrap.rs` (286 `unsafe impl Sync`, 353, 475, 515, 572, 592) | 6 | 9 | Adequate — publication/acquire protocol documented at :142. |
| `src/registry/heap_registry.rs` (76, 88, 105, 136, 149, 164, 207, 212, 306, … incl. `unsafe fn recycle`/`abandon_segments`/`try_adopt`) | 15 | 19 | Adequate — pointer-handoff contracts stated on each `unsafe fn`. |
| `src/registry/heap_slot.rs` | Send/Sync impls | present | Adequate (listed in README seam table). |
| `crates/vmem/src/lib.rs` (199, 229, 273–391, 424–638 — the OS FFI: `VirtualAlloc`/`mmap`/`madvise`) | 21 | 29 | Adequate — the whole crate IS the aperture; SAFETY density exceeds block count. |
| `crates/numa/src/lib.rs` (174 `unsafe fn bind_range`, 249–683 — `mbind`/`GetNumaProcessorNodeEx` FFI) | 10 | 19 | Adequate. |
| `crates/malloc-bench/src` | confined helpers | per README | Not runtime-relevant (bench harness, not in the sefer-alloc dep tree). |

Confinement is compiler-enforced: the crate root is `#![forbid(unsafe_code)]`
(switching to `deny` + per-file `allow` when `alloc-core`/`experimental` features
are on), so a stray `unsafe` outside these files is a hard compile error. Verified
claim: TRUE.

**Minor doc drift (low):** the README "internal seams" table
(`README.md:344–357`) says "eight" active seams under `production`, and lists
`os.rs`, `node.rs`, `numa.rs`, `sefer_alloc.rs`, `tls_heap.rs`, `fallback.rs`,
`bootstrap.rs`, `heap_slot.rs`, `heap_registry.rs`, `hand.rs`. The set found by
grep matches this table exactly — no unlisted seam exists. No action needed beyond
keeping the table in sync (its stated source of truth,
`grep -rln 'allow(unsafe_code)'`, still agrees).

### 1.2 Dependency tree — CLEAN, no slopsquatting indicators

Runtime dependencies (default build): `sefer-region` (path, workspace) →
`slotmap 1.1.1` (+ build-dep `version_check 0.9.5`). That is the ENTIRE runtime
tree. Optional features add: `arc-swap 1`, `crossbeam-epoch 0.9`,
`core_affinity 0.8`, `aligned-vmem` (path), `numa-shim` (path). All are
well-known, widely used crates with correct canonical names.

Dev-dependency tree (criterion, proptest, tokio, mimalloc, loom, iai-callgrind
and transitive deps) was reviewed. Two names checked specifically as potential
slopsquats:

- `zmij 1.0.21` — pulled by `serde_json 1.0.150` (dtolnay's own float/int
  formatting crate that replaced itoa/ryu in recent serde_json). Legitimate;
  dev-only (via criterion). Not a squat.
- `shlex 2.0.1` — pulled by `cc` via `libmimalloc-sys` (mimalloc is dev-only,
  used for benchmarking). Canonical crate. Not a squat.

No dependency with a bad security reputation, unmaintained status flag, or
name-similarity risk was found. Verdict: clean.

### 1.3 `cargo audit` — NOT RUN (tool not installed)

`cargo audit` is not installed on this machine (`error: no such command: audit`).
Per instructions, it was not installed. NOT VERIFIED: RustSec advisory status of
the dev-dependency tree. Given the runtime tree is effectively `slotmap` only,
residual risk is low, but a CI `cargo audit` / `cargo deny` job would close this
gap permanently (none exists today — see §1.6).

### 1.4 Hardcoded secrets — NONE FOUND

Grepped for API keys, tokens, passwords, private-key blocks, and provider-specific
token prefixes (`ghp_`, `AKIA…`, crates.io `cio…`, `-----BEGIN`) across all
tracked file types. All keyword hits are documentation/comments (e.g.
`release.yml`'s instructions for setting the `CARGO_REGISTRY_TOKEN` repository
secret via `gh secret set` — the secret itself is referenced only as
`${{ secrets.CARGO_REGISTRY_TOKEN }}`, scoped as a step-level env var of the
publish step only, which is correct practice). No credential material in the repo.

### 1.5 X7 hardened-feature 1/256 wrap residual — documented internally, NOT user-facing (MEDIUM, doc gap)

The accepted 1/256 generation-wrap residual (an 8-bit per-granule gen counter:
256 re-issues of one block before a lazy drain can realign a stale ring note)
is well documented in:

- `docs/design/X7_GENERATIONAL_RING_PLAN.md:73,123,142,150` (internal design doc);
- `docs/DURABILITY.md:38,68` (full inventory row: "accepted residual by design",
  with the §2.5 rationale for not widening to u64 and the pinning tests
  `tests/regression_gen_wrap_boundary.rs`, `tests/regression_gen_table_layout.rs`);
- `CHANGELOG.md:160–174` ("The only remaining leak is the **1/256 wrap**").

However, the **README** — the primary user-facing surface for a security-conscious
consumer choosing `--features hardened` — does not mention it anywhere:

- The README feature-table row for `hardened` (`README.md:769`) describes only the
  interior-pointer free guard and does not mention the X7 generational ring at all
  (the row appears to predate the X7 arc).
- The README "documented residual" paragraph (`README.md:702–708`) describes the
  pre-X7 ring↔magazine cross-thread double-free residual and points to
  `docs/FASTBIN_DESIGN.md`, but does not state the post-X7 status: closed under
  `hardened` EXCEPT the 1/256 wrap.

**Recommendation:** update the README `hardened` row and the residual paragraph to
(a) mention the X7 stamp/compare defense as part of `hardened`, and (b) state the
1/256 wrap explicitly as the accepted probabilistic limit, linking to
`docs/DURABILITY.md`. For an allocator marketing "double-free = no-op, never UB"
guarantees, a consumer evaluating the hardened tier should not have to read
CHANGELOG/internal design docs to learn the defense is probabilistic at the
1/256 boundary. Severity: documentation gap, not a code defect — the residual
only matters for programs already committing double-free UB, and the design
trade-off (not doubling the ring footprint) is sound and test-pinned.

### 1.6 CI workflows — mostly sound; three low-severity notes

Reviewed `.github/workflows/{ci.yml,release.yml,perf-gate.yml}`.

Good: no secrets echoed to logs; `CARGO_REGISTRY_TOKEN` confined to the single
`cargo publish` step's env; release workflow has a tag↔Cargo.toml version guard
and a pre-publish test gate; publish concurrency serialised; fuzz targets get a
build-only bit-rot gate; miri/loom/MSRV/no_std jobs present.

1. **No `permissions:` block in any workflow (LOW).** All three workflows run
   with the repository-default `GITHUB_TOKEN` permissions. If the repo default
   is the legacy read/write, every job (including third-party actions) gets a
   write-capable token it does not need. Add `permissions: contents: read` at
   workflow level (release.yml needs nothing more either — publishing uses the
   crates.io token, not GITHUB_TOKEN).
2. **Actions pinned by tag, not SHA (LOW).** `actions/checkout@v5`,
   `dtolnay/rust-toolchain@stable|nightly|1.88`, `taiki-e/install-action@v2`.
   All are reputable maintainers; tag-pinning is common practice, but for a
   published allocator crate with a release workflow, SHA-pinning (at least in
   `release.yml`, the workflow with the publish token) would remove the
   tag-rewrite supply-chain vector. Note `dtolnay/rust-toolchain@stable` is a
   moving branch by design — acceptable, but worth a conscious decision.
3. **No `cargo audit`/`cargo deny` job (LOW).** See §1.3. The release "Future
   improvements" note about migrating to crates.io trusted publishing (OIDC,
   removing the long-lived token) is a good roadmap item — endorse it.

No secret exposure, no `pull_request_target` misuse, no script-injection-prone
`${{ }}` interpolation of untrusted input into `run:` bodies (tag/label names are
interpolated, but only on `push`/`schedule`/label events from collaborators;
the `github.ref_name` interpolations in release.yml are single-quoted; residual
risk is negligible for a repo where tag pushes require write access).

---

## 2. Compliance findings

### 2.1 License declaration vs files — CONSISTENT

- Root `Cargo.toml:8`: `license = "MIT OR Apache-2.0"`. Both `LICENSE-MIT` and
  `LICENSE-APACHE` exist at the repo root (MIT text verified: "Copyright (c) 2026
  sefer-alloc contributors").
- All four sub-crates (`crates/vmem`, `crates/numa`, `crates/region`,
  `crates/malloc-bench`) declare `MIT OR Apache-2.0` and ship their own
  LICENSE-MIT/LICENSE-APACHE pairs (verified for vmem and region; numa and
  malloc-bench declare the same field). Consistent.

### 2.2 Dependency licenses — NO COPYLEFT FOUND

`cargo license`/`cargo deny` are not installed, so this was assessed from the
`cargo tree` inventory against the crates' known published license fields
(NOT machine-verified — stated explicitly per audit rules):

- Runtime: `slotmap` (MIT), `version_check` (MIT/Apache-2.0). Compatible.
- Optional: `arc-swap` (MIT/Apache-2.0), `crossbeam-epoch` (MIT/Apache-2.0),
  `core_affinity` (MIT). Compatible.
- Dev-only tree (criterion/proptest/tokio/mimalloc/loom/etc. and transitives,
  including `zerocopy` (BSD-2-Clause OR Apache-2.0 OR MIT), `half`, `zmij`,
  `windows-sys` (MIT/Apache-2.0)): all permissive. Dev-dependencies do not
  affect downstream license obligations of the published crate in any case.
- No GPL/LGPL/MPL/SSPL or otherwise copyleft crate appears anywhere in
  `cargo tree`. `mimalloc` (MIT) wraps C code but is dev-only (bench baseline),
  consistent with the crate's "100% Rust, no C/C++" runtime claim.

Verdict: compatible. Recommendation: add a `cargo deny check licenses` CI job to
make this machine-checked.

### 2.3 SECURITY.md — EXISTS but SHIPPED WITH PLACEHOLDERS (MEDIUM, compliance)

`SECURITY.md` exists and describes a proper process (private GitHub Security
Advisories preferred, e-mail fallback, no-public-issue policy). Two concrete
defects:

1. **Placeholder contact e-mail still present** (`SECURITY.md` fallback section:
   the literal string `REPLACE_WITH_REAL_EMAIL`, plus the top-of-file
   `<!-- PLACEHOLDER: ... -->` comment). The repo is public and the crate is on
   crates.io at 0.3.0 — the fallback disclosure channel is non-functional.
   Fix: insert the real maintainer address (or delete the e-mail fallback and
   rely solely on GitHub Security Advisories).
2. **Stale supported-versions table**: it declares "0.1.x (current) — Yes" while
   the published crate is 0.3.0. As written it literally promises patches only
   for 0.1.x. Fix: update to 0.3.x (or "latest 0.x release").

### 2.4 MSRV — CONSISTENT and CI-ENFORCED

- `Cargo.toml:7`: `rust-version = "1.88"`.
- `README.md:11` (badge "MSRV: 1.88") and `README.md:885–888` ("## MSRV —
  **1.88.**"): consistent.
- Enforced in CI: `ci.yml` job `msrv` ("MSRV 1.88 (cargo check)") pins
  `dtolnay/rust-toolchain@1.88` and runs `cargo check --all-features`. This is a
  build-only check (tests run on stable), which is the standard and sufficient
  MSRV evidence. VERIFIED consistent and enforced.

### 2.5 Ancillary compliance files

`CHANGELOG.md`, `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md` all present. The
published tarball excludes CI/scripts/checkpoints via `Cargo.toml` `exclude`
(good hygiene: internal session checkpoints under `docs/checkpoints/` — which do
mention workflow-secret setup commands, though no actual secret values — never
reach crates.io).

---

## 3. Summary / severity table

| # | Severity | Area | Finding | Location |
|---|---|---|---|---|
| 1 | Medium (compliance) | Disclosure | SECURITY.md ships with `REPLACE_WITH_REAL_EMAIL` placeholder — fallback channel non-functional | `SECURITY.md` |
| 2 | Medium (compliance) | Disclosure | SECURITY.md supported-versions table says 0.1.x; crate is 0.3.0 | `SECURITY.md` |
| 3 | Medium (doc gap) | Hardened tier | 1/256 gen-wrap residual documented only in CHANGELOG + internal docs (DURABILITY.md, X7 plan), absent from README's `hardened` row and residual paragraph | `README.md:702–708, 769` |
| 4 | Low | CI | No `permissions:` block in any workflow — default GITHUB_TOKEN scope | `.github/workflows/*.yml` |
| 5 | Low | CI / supply chain | Actions pinned by tag, not SHA (notably in `release.yml`, the token-bearing workflow) | `.github/workflows/release.yml` |
| 6 | Low | CI | No `cargo audit`/`cargo deny` job; RustSec status of dev-deps NOT VERIFIED locally (tool absent) | CI |
| 7 | Info | Unsafe audit | All `unsafe` confined to the named seams; SAFETY comments present at ≥1 per block in every seam file — README claim VERIFIED | §1.1 table |
| 8 | Info | Dependencies | Runtime tree is `slotmap` + path crates only; `zmij`/`shlex` checked and legitimate; no slopsquatting, no copyleft | §1.2, §2.2 |
| 9 | Info | Secrets | No hardcoded credentials anywhere in the repo | §1.4 |
| 10 | Info | MSRV | 1.88 consistent across Cargo.toml/README and enforced by a pinned CI job | §2.4 |

Overall posture: strong for a project of this size. The three medium items are
documentation/process fixes, not code defects; the code-level unsafe-confinement
and verification claims checked out exactly as advertised.
