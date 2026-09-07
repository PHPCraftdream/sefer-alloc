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

## Acceptance

Pending: integrated code review, regression and package checks, final P0-P3 audit.
