# globalalloc-model: publication preparation and self-review

Reviewer: Sol-codex. Started 2026-09-07 08:34 local.
Baseline: `a2f887e5439780f8dc3d3867360b806716de8f69`.
Scope: this crate, its publication artifacts and directly related CI.

## Initial review

Existing reviews and preparation are present; the latest ox run was a narrow
confirmation. This pass reads all eight source modules, the six integration
test targets, Cargo metadata, README, CHANGELOG and crate-specific CI.
Resolved dependencies: proptest 1.11.0 and arbitrary 1.4.2. Declared MSRV: 1.85.

Confirmed findings to fix:

- **P2: arbitrary size distribution.** Saturating two `u32` weights loses
  probability mass. With both weights `u32::MAX`, `raw % total` never selects
  the large arm. Deriving size magnitude from the same `u32` quotient also
  makes large configured size values unreachable. Independent entropy and a
  full-width weight sum are required.
- **P2: destructive operations hide corruption.** A later allocation can
  damage a still-live block; freeing it before the final sweep silently drops
  the evidence. Shrinking realloc can discard a damaged suffix. Check a block
  before deallocation and before realloc, and check survivors before each
  teardown deallocation.
- **P2: 32-bit tests fail to compile.** `oracle_negative.rs` contains
  `1usize << 63` and hard-coded 64-bit panic operands. Express the boundary
  using `usize::BITS`, retain diagnostic assertions on both widths and add CI.
- **P3: reproducible proptest seeds break the entropy test.** The test uses
  two default runners, so a valid `PROPTEST_RNG_SEED` makes them identical.
  Explicitly select random seeds only for this configuration test.
- **P3: RawAllocator initialization contract.** A blanket GlobalAlloc cannot
  unconditionally initialize realloc's prefix if the old bytes were not
  initialized. State the corresponding caller precondition; `drive` meets it
  by filling each allocation.
- **P3: inaccurate public limitations.** Uninitialized reads are UB natively
  as well as under Miri; native detection is not guaranteed. Using the tested
  allocator as the global allocator does not inherently cause recursion.
  Explain actual reentrancy conditions, unsupported invalid allocators and
  the deliberate lack of generic cleanup after an oracle panic.
- **P3: package and usage coverage.** Add feature-specific dependency examples,
  isolated extracted-package tests, 32-bit tests and docs.rs configuration
  validation. Existing Miri and bare-metal CI remain relevant coverage.

Reference: [Rust GlobalAlloc contract](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html).
The local resolved proptest source is the reference for RNG configuration and
weighted-union behavior. No new dependencies or performance claims are needed.

## Workflow

Reviews are performed by the main agent. Sol/high workers implement bounded
fixes in three explicitly created, detached Git worktrees. Their patches must
be inspected and integrated before final tests and the second self-review.
No publication or push is part of this task.

## Acceptance and second self-review

Completed 2026-09-07, approximately 09:01 +02:00. Verdict: **GO** for the
inspected working tree; remaining findings: **P0=0, P1=0, P2=0, P3=0**.
This verdict includes the implementation changes in the working tree; it does
not describe the unchanged baseline commit alone.

All seven initial findings are closed. The main reviewer inspected all worker
diffs, integrated them using patches, and re-read the resulting contracts,
generator branches, lifecycle checks and CI. Further acceptance refinements:

- Zero-weight proptest arms are structurally absent, including during shrinking.
- The i686 CI job now runs every test target; it no longer excludes the
  negative-oracle target that contained the former 64-bit literal.
- `RawAllocator` expressly requires valid provenance, a single allocated
  object for each extent, initialized byte lifetime and no racing accesses.
- `drive`'s own rustdoc now matches the README's UB, reentrancy and panic-leak
  limitations; no native UB detection guarantee remains.
- The arena fault implementation checks the entire returned realloc extent,
  rather than merely the copied prefix.

### Verified behavior

| Requirement | Evidence |
| --- | --- |
| Weighted arms and full-width size reach | Deterministic arbitrary tests exercise both endpoints, zero/max weights, >u32 sizes without allocating them, all variants and the 2048-op cap |
| Stable shrinking constraints | `disabled_small_arm_stays_disabled_while_shrinking` walks the value tree and rejects sizes from a disabled arm |
| Reproducible property tests | Ordinary tests honor `PROPTEST_RNG_SEED=42`; only the entropy configuration test explicitly chooses random seeding |
| Detection before destructive operations | Three arena-backed tests catch corruption before dealloc, before shrink, and between teardown frees; all three failed against the old driver in the worker's negative control |
| Pointer safety | All six integration targets passed strict-provenance Miri; only the intentional no-op-deallocation fixture uses `-Zmiri-ignore-leaks` |
| 32-bit portability | Full all-feature suite executed on `i686-pc-windows-msvc`: 50 tests; bounds and panic operands use target width |
| Feature isolation | Default, proptest-only, arbitrary-only and all-feature tests passed; all-feature debug/release suites have 51 tests on x64 |
| Declared MSRV | All-feature library check passed on Rust 1.85 |
| no_std | Bare-metal `thumbv7em-none-eabi` proptest-feature build passed; existing CI retains default and proptest bare-metal builds |
| Packaged consumer | Cargo packaged and verified 22 files; externally extracted default/all-feature tests, Clippy and rustdoc passed; a separate consumer ran with default and all features |
| Actual final package bytes | After the final arena-bound refinement, the extracted all-feature suite was rebuilt in a fresh target directory and all 51 tests passed |
| docs.rs configuration | Nightly documentation with `RUSTDOCFLAGS="--cfg docsrs -D warnings"` passed |
| CI syntax and formatting | `actionlint .github/workflows/globalalloc-model.yml`, crate fmt and `git diff --check` passed |

The local final archive is `target/package/globalalloc-model-0.1.0.crate`,
SHA-256 `3af8a2c43948535725d4c9a974a179befa1ae41c54d2f9ebdfc1f9c486b4942b`.
It was produced with `--allow-dirty` to verify the prepared, uncommitted changes.
This is a local package validation, not a registry publication receipt.

The new workflow provides Windows/Linux feature matrices, full Linux i686
execution, extracted-package tests and an external consumer, MSRV and docs.rs
checks. Existing broad CI supplies Miri, no_std and release coverage. The new
remote workflow has not been pushed or executed during this task; local Windows
i686 execution is not being reported as a Linux run.

### Unsafe review notes

- `src/raw_allocator.rs`: the unsafe trait and blanket forwarding impl now
  share caller preconditions, including initialized old realloc prefix. Every
  forwarding block has a local safety explanation.
- `src/drive.rs`: raw writes/reads stay within accepted extents; overlap and
  alignment checks precede accesses to new results. Added pre-free/pre-realloc
  reads use still-live, previously filled blocks. Teardown rechecks each block
  before freeing it. Miri confirms the exercised paths.
- `src/double_free_ok.rs`: constructing double-free consent remains unsafe;
  the API and prohibition on using it with blanket GlobalAlloc impls are retained.
- The test arena owns its backing allocation and reclaims it on unwind;
  injected corruption changes initialized bytes inside that allocation.

### Deliberate limits

Invalid extent/provenance/initialization from an allocator is outside the safe
oracle domain. Transient corruption repaired between observations and equal
fill-byte collisions can escape detection. Oracle panics may leak tested
allocations, so repeated failing runs need arena/process-level reclamation.
These limits are stated in public docs, not counted as newly discovered defects.

Fuzz bytes are not a stable serialized format; the new decoder may reinterpret
an existing corpus. The public `Op` sequence remains available for exact replay.
No performance improvement is claimed. No versions were changed, and the main
agent created no commits or pushes for the implementation. Three accepted
auxiliary worktrees were removed after their changes were integrated.
