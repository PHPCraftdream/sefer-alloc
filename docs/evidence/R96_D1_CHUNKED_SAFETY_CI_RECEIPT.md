# PR #96 — D1′ chunked-sidecar safety receipt

**Verdict: safety CI GREEN; performance NOT MEASURED.** No merge to `main` implied.

- PR: https://github.com/PHPCraftdream/sefer-alloc/pull/96
- CI run: https://github.com/PHPCraftdream/sefer-alloc/actions/runs/39869595774
- Tested head: `d031f03cdc14efdcc501840868491de7d8a9891e`
- Candidate tree: `5f89eee0eca1dad02ab7879360a97b1188559648`
- Run conclusion: `success` (GitHub API, 2026-10-23)

## Relevant proof jobs

- `miri (core)`, job `120140967198`: actual-source `chunked_sidecar_storage` passed all 3 tests in both Stacked Borrows and Tree Borrows. Covered unready backing, sparse/distant construction, pending preservation, retain clearing without reconstruction, and teardown.
- `loom (alloc-core alloc-xthread)`, job `120140967255`: the actual-source ChunkedBank models passed in release; the debug step first mutated the production ready load from Acquire to Relaxed, and the tracked Loom cell reported the expected causality violation. The CI-only script verified the required failure signature, restored the source byte-for-byte, then the restored debug model passed all 3 tests.
- `kani proofs`, job `120140967708`: all 21 harnesses completed, 0 failed, 1 unreachable. The five nested ChunkedBank proofs ran: offset/granule validation, word/backing bounds, issue/publish bit identity, scan-window validation, and exact range-mask membership. This does not prove concurrency or reclamation.
- `test windows (production)`, job `120140967696`: production suite and both dedicated rollback witnesses passed. The fastbin magazine-refill test and the non-fastbin scalar commit-refusal test each emitted their exact `... ok` sentinel.
- `test (core tiers)`, `test (hardened tier)`, `test (gated bodies + all-features)`, `test (feature isolation)`, `test (xthread progression)`, `test (x86_64-unknown-linux-gnu)`, `test (aarch64-unknown-linux-gnu)`, and `test macos (production)` all completed successfully.
- `check-matrix`, Clippy, rustfmt, docs consistency, evidence registry, and the remaining required CI groups all passed on this SHA.

The receipt is limited to the stated candidate, configurations, and CI run. It does not close the pre-existing unsafe residuals (including correctness item 164), does not establish performance benefit/non-regression, and does not authorize promotion to `production` or merge.
