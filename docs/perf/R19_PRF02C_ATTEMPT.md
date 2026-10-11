# R19 PRF02C — preparation / safety review / exact recipe

Status: completed single approved eight-run attempt; derived verdict NO-GO. Exact B restored. Existing batch E0133 and earlier production-containing hardened clippy superset failure remain owner-owned; the exact requested hardened row passed. Full B validation is not green.

## Source review

Original C backup reviewed read-only; preserved unchanged. Its raw kind decode/split is NOT reused. Its test inferred the original byte from the decoded tag and used catch_unwind; neither is retained. Reviewed Cprime uses captured RAW original byte and an exclusive-borrow RAII guard; restoration happens before assertions and teardown. No bogus claim that a corrupted free meets the ordinary dealloc contract: Cprime explicitly documents the narrow typed-valid Unknown own-thread probe exception.

Production diff: only three shared guard branches widen Large to Large|Unknown and reject Unknown through one cold, non-inlined counter/no-op helper. No geometry or bitmap access occurs in the Unknown arm. F7(A)/(B) preserve Large behavior. `BlockKind::of(base, Some(c))`, BlockKind implementation, complete payload_start expression, OwnedBlock, routing and all remaining B production logic remain byte-identical.

Instrumentation diff: HeapCore unsafe + internals/bench-internals/fastbin-gated stamp forwarder returns the previous RAW byte; reviewed unsafe inventory + required bench gate; precise header/dealloc probe contracts; separate Unknown regression. Existing AllocCore stamp safety contract is extended only for the reviewed own-thread 0xff probe. Existing safety and geometry obligations otherwise remain unchanged. There is no new raw decode or Small/Large split.

Source snapshot patch is captured before any measurement under target/r19-prf02c/Cprime.patch. B/Cprime source maps and patch hash are in the separate C identity/snapshot documents. All source snapshots are external only. B strict-provenance Miri completed green: canonical lookup 2 tests plus realloc-tail 1 test (our completed B validation, not an outstanding independent run).

CLAUDE rule3 decision: NO-GO means revert ALL Cprime production/hook/inventory/contract/test changes in this task; no independent contract-regression retention claimed. The driver restores exact B in finally on success or failure. External snapshots survive. No tuning/retries.

## Preparation receipts

- B restored/unchanged: byte comparisons PASS for all 13 present snapshot entries; Cprime-only regression remains absent from B.
- Immutable R verification: all 10 B source hashes and 428 pinned input hashes match.
- Driver and judge no-Cargo self-tests PASS; driver syntax check PASS.
- Historical pre-format C patch SHA256: `93f3aeba15f9366bb64a056b66b564cf39bfaf829c2af0b2722c34e643ad33ec`.
- Historical pre-format C prereg SHA256: `4a6d53cd1ad9fe75c9c441d651d04db138d439d06c6a2be72046d99d03a69404`.
- Historical pre-format C identity SHA256: `7d0cd0bd00d74544a13fe0e80c5e9729ee9d26535226d4ae6514dda3aa21ba21`.
- External source-only store: external-store (local-only; supplied by operator).
- Original C backup remains untouched: external-store original-C backup (local-only; supplied by operator).
- Validation document edits are path-prefix replacement and removal of four empty Receipt lines only; R identity/prereg/scripts untouched.

## Mandatory pre-run review findings

- Removed private external-store paths; store identity is local-only and operator-supplied, not pinned documentation.
- Driver installation must use external `cp` followed by `cmp` and fresh timestamps, including finally restoration.
- Judge must emit Markdown and CSV from its exact JSON numeric rows, preserving numerator and denominator for every ratio; after measurement the ATTEMPT must contain that exact verdict, never a hand-built table.
- Repin C script identity and preregistration before correctness execution; no measurement has occurred. Immutable R inputs remain unchanged.
- Review actual installed runtime, hook, safety and test code before Cargo. Compilation failure or safety contradiction terminates validation without workaround or another attempt.

## Formatting-only authorization before first measurement

Applied exactly four rustfmt --edition 2021 hunks through editing tools after reviewing rustfmt's diff: hook cfg wrapping; regression crate cfg wrapping; RestoreKind initializer expansion; counter assert expansion. No semantic edits; shared guard, BlockKind::of and payload_start unchanged. C snapshot, source patch, identity and preregistration are repinned before measurement. Re-run all three Unknown branches and strict-provenance Miri, scanner and formatting check; restore B via cp/cmp and verify all 428 inputs. Original-C external backup preserved. Exactly one approved performance attempt, no retries.

From this repository root, with completed B Miri and explicit approval:

1. `node scripts/r19_prf02c_iai.mjs --self-test`
2. `node scripts/r19_prf02c_gate_table.mjs --self-test`
3. For correctness validation, install only the pinned Cprime files from the external snapshot, preserving a finally restoration of every B source map entry (delete the Cprime-only test on restore). Run serially with RUSTC_WRAPPER empty, CARGO_BUILD_JOBS=3, CARGO_TARGET_DIR="$PWD/target"; no lockfile edits:
   - `cargo test --locked --features 'production internals bench-internals' --test regression_magazine_free_unknown_kind -- --test-threads=1 --nocapture`
   - same binary with `production internals bench-internals medium-classes` (F7 A)
   - same binary with `production internals bench-internals hardened` (F7 B)
   - `cargo +nightly miri test --locked --features 'production internals bench-internals' --test regression_magazine_free_unknown_kind -- --test-threads=1 --nocapture`, MIRIFLAGS='-Zmiri-strict-provenance -Zmiri-disable-isolation'.
   - Negative counterfactual: replace ONLY the shared guard file with pinned B, keep reviewed probe instrumentation/contracts. Exact same test must fail `Unknown entered magazine`; never double-free an accepted counterfactual block; RAII still restores true kind. Restore Cprime guard then validate green; restore whole B in finally. This is a correctness mutant, not an extra performance arm.
   - Run hook scanner on the installed Cprime inventory before approving measurement.
4. Windows/WSL driver, explicit reviewed approval: `R19_PRF02C_APPROVED=yes node scripts/r19_prf02c_iai.mjs --run <external-source-store>`. The actual external store is supplied by the operator; source-only snapshots are already prepared. Driver enforces one attempt via a directory marker and serial order pure B1/Cprime1/B2/Cprime2 then instrumented B1/Cprime1/B2/Cprime2; finally restores B, checks byte comparisons, source hashes and all R inputs. Generated harness uses root target, --locked, three build jobs and preinstalled IAI runner; installation forbidden.
5. `node scripts/r19_prf02c_gate_table.mjs --derive docs/perf` creates the immutable C result, not R outputs. Two repeats must match all absolute instruction counts; both-axis push inclusive [-3,+1], raw churn N/2N <=+10, original controls. Failed/missing receipts => NO-GO, not fabricated data.

## Correctness receipts (no measurement)

- Exact Cprime installed by external cp/cmp with fresh timestamps. Reviewed actual shared guard, hook forwarder, raw stamp implementation, dealloc exception and regression code; no candidate source changes or workarounds applied.
- Production, F7A medium-classes, F7B hardened: each ran exactly 1 test, 1 passed, 0 failed. Strict-provenance Miri: exactly 1 test, 1 passed, 0 failed. Total initial green executions: 4, not four distinct tests.
- Exact B guard only, reviewed Cprime instrumentation retained: all three feature combinations compiled and reached the behavioral assertion `Unknown entered magazine`, left 16 / right 15. Each ran 1 test, assertion failed, exit 101 with SIGABRT (no completed libtest summary). No compile-error false red and no double-free call after acceptance.
- Restored Cprime green after each mutant: each ran 1 test, 1 passed, 0 failed. Total green test executions including these confirmations: 7. Total behavioral-red mutant executions: 3.
- Every mutant used an external cp/cmp guard backup; finally restoration cmp and Cprime source hashes passed. Restore diffstat: 0 files changed per mutant. The production mutant's first evidence parser incorrectly required a completed `test result: FAILED` summary, despite the assertion-triggered abort; corrected only the local evidence parser, not candidate source, and continued the remaining mutants without rerunning that mutant.
- Hook scanner PASS: 208 reviewed safe, 34 reviewed unsafe, 102 genuinely bench-gated safe hooks.
- Initial rustfmt --edition 2021 --check FAILED: queries.rs cfg formatting and regression cfg/struct/assert formatting. Subsequently corrected by authorized formatting-only edits before the first measurement; repinned identity and snapshots, same candidate semantics.
- Full B restored in finally after each correctness stage via cp/cmp; all B source hashes and all 428 pinned R input hashes verified. Cprime-only test deleted. No IAI, git writes, lockfile edits or out-of-scope fixes.
- Initial sanitized raw correctness logs are under target/r19-prf02c/correctness-*.log; post-format receipts are under target/r19-prf02c/formatted-*.log. The checked judge emits JSON, Markdown and CSV from the same persisted numeric rows and writes its exact verdict into this ATTEMPT.

The preparation self-tests do not execute Cargo. Correctness receipts above are separate from the no-Cargo preparation checks; they are not performance evidence.

## Measured judge verdict

Exact verdict: **NO-GO**. Numeric rows and all ratio numerators/denominators are derived from R19_PRF02C_RESULT.json in R19_PRF02C_RESULT.md and R19_PRF02C_RESULT_summary.csv.

## Final execution and restoration receipts

- Formatted C patch SHA256: `da629b08a216895144b57285c69424f113baae0c32329da04b6606969d9a986c`. C source maps, patch, identity and preregistration repinned before measurement; immutable throughout the attempt.
- Post-format Unknown production/F7A/F7B and strict-provenance Miri: each 1 passed, 0 failed (four executions). Formatting PASS; C hook scanner PASS (208 safe / 34 unsafe / 102 bench-gated safe).
- Exact B requested clippy hardened medium-classes internals, without extra production: exit 0. Earlier superset failure and batch E0133 remain separate unresolved external blockers; no full-validation-green claim, no fixes. Cancelled top-level clippy not counted.
- Exactly eight successful serial performance receipts, two axes and two repeats per arm. Every absolute instruction count repeated exactly; no retries or second attempt. Checked script generated RESULT JSON/MD/summary CSV and the exact NO-GO verdict above.
- Final driver restoration: all 13 present B snapshot entries cp/cmp and all B source hashes PASS; all 428 pinned R inputs PASS. Cprime-only regression deleted. All Cprime production, hook, inventory and contract changes absent from tree under rule 3; external B/Cprime and original-C backup preserved. Final B hook scanner PASS (208 safe / 33 unsafe / 102 bench-gated safe).
