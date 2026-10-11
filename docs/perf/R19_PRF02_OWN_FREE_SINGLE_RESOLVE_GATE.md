# R19 PRF-02 — perf(runtime) candidate B NO-GO; fix(perf) C NO-GO

Base: `042e88fb67ab48dd6bef7cf8bc416d8b0103ea54`. Twelve sequential narrow
iai runs completed: A/B/C, two repeats, instrumented and pure-production axes.
All repeats matched absolute Instructions, including instrumented body diagnostics.
No threshold was changed after observation. Worktree remains C by owner direction;
this is not merge authorization. Unknown standalone parent-vs-fix fallback receipt
was NOT measured and must not be inferred from C-minus-B.

## Reproducible evidence

[Generated results](R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_results.md),
[CSV including raw controls, deltas, oracle and metadata](R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_summary.csv),
[original premeasurement identity](R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_identity.json).
The identity was copied unchanged from the manifest recorded BEFORE measurement,
not recreated from the final tree. It contains original source/fixture/harness
hashes and rustc, Linux target, CPU and OS identification. Two samples per arm/axis.

Raw receipts (all under 200 KiB, sanitized, first absolute Instructions column):

- [_raw_r19_prf02_A_instrumented_1.log](_raw_r19_prf02_A_instrumented_1.log), [_raw_r19_prf02_A_instrumented_2.log](_raw_r19_prf02_A_instrumented_2.log)
- [_raw_r19_prf02_B_instrumented_1.log](_raw_r19_prf02_B_instrumented_1.log), [_raw_r19_prf02_B_instrumented_2.log](_raw_r19_prf02_B_instrumented_2.log)
- [_raw_r19_prf02_C_instrumented_1.log](_raw_r19_prf02_C_instrumented_1.log), [_raw_r19_prf02_C_instrumented_2.log](_raw_r19_prf02_C_instrumented_2.log)
- [_raw_r19_prf02_A_pure_1.log](_raw_r19_prf02_A_pure_1.log), [_raw_r19_prf02_A_pure_2.log](_raw_r19_prf02_A_pure_2.log)
- [_raw_r19_prf02_B_pure_1.log](_raw_r19_prf02_B_pure_1.log), [_raw_r19_prf02_B_pure_2.log](_raw_r19_prf02_B_pure_2.log)
- [_raw_r19_prf02_C_pure_1.log](_raw_r19_prf02_C_pure_1.log), [_raw_r19_prf02_C_pure_2.log](_raw_r19_prf02_C_pure_2.log)
- Activation: [_raw_r19_prf02_A.oracle.log](_raw_r19_prf02_A.oracle.log), [_raw_r19_prf02_B.oracle.log](_raw_r19_prf02_B.oracle.log), [_raw_r19_prf02_C.oracle.log](_raw_r19_prf02_C.oracle.log).

Initial infrastructure failures preserved separately:
[_raw_r19_prf02_A.oracle_teardown_failure.log](_raw_r19_prf02_A.oracle_teardown_failure.log),
[_raw_r19_prf02_B.oracle_stale_build_failure.log](_raw_r19_prf02_B.oracle_stale_build_failure.log),
[_raw_r19_prf02_A.oracle_before_timestamp_fix.log](_raw_r19_prf02_A.oracle_before_timestamp_fix.log).
The activation lease was changed to deliberately stay LIVE like the fixture;
cp snapshots now refresh timestamps to prevent stale cargo fingerprints. Harness
identity was repinned before successful activation and all iai measurements.

## Protocol and unchanged thresholds

A = base runtime; B = OwnedBlock single lookup; C = B plus Unknown rejection
and split lower bound. Instrumented features: `production bench-internals internals`;
pure: `production internals`. Instrumented free uses HeapCore, churn SeferAlloc;
pure free/churn uses HeapCore. Compare only within an axis.

Shared iai filters are output-only: the task instrumented target therefore
extracts the thirteen exact original function bodies and lease helper into a
narrow group. Pure target has eleven required functions, including mimalloc,
Large and magazine controls. Shared scripts/iai.mjs and perf_gate_iai.rs unchanged.
Original fixture source hash is recorded; group/module identity differs, so
these are new A/B/C counts, not historical absolute-count claims.

push=n9−n8; free-average=(n16−prefix)/16; churn=(2N−N)/64.
B−A instrumented push and churn ≤−8 Ir; pure push ≤−6 Ir.
C−B instrumented push in [−3,+1]. Raw churn kill-gate +10 Ir.
Mimalloc ΔIr=0; Large ±10; magazine-hit-minus-prefill ±10.
Body diagnostic=(body−segment-base-probe)/64, no threshold.
Activation observes only sixteen frees after prealloc64: A32/B/C16 lookups,
tcache16, routed-scan delta0. All successful oracle processes exited zero.

B meets own-free benefit thresholds but fails Large controls; C fails push
and churn kill gates. Detailed numbers and verdicts come only from the checked
single derivation script, not hand-transcribed tables.

Switching uses external source backups (not checkouts/build trees), cp/cmp only,
and finally restores C. Linux builds use worktree-local `target/r19-linux` to
separate object formats from Windows target; this is the necessary documented
technical deviation from literal target-root, not a policy override.

## Immutable source identities and final formatting

Runtime B patch SHA-256: `1730350cd2f8120f61043a37154b8abab88a045ec329064c8a3b9a3dac4307df`.
Runtime C patch SHA-256: `d5481e312e4fe95251ae272a3f918761b418f461b522fba1156076c755be7279`.
Measured harness patch SHA-256: `b672719e9cf51f7faa508ff674cf25e8af212a34cd80aa8134b9adb26d098a62`.
Measured instrumented fixture SHA-256: `6b2ab0afd7758a0c818d85337b63762fd86fa245b248712ec337d8f64b659f82`.

The measured patch and exact fixture backup remain preserved externally.
After measurement, rustfmt-only fixture delta SHA-256:
`9289b0652bf2204f27d933d98c2971537abe37beeb907c8b985a11bcc1789a70`.
Final formatted fixture SHA-256:
`e60fc65043669ab3100a5ef6d2cac4a5c29304fc378a20923d018aaaa40b1db7`.
Final formatted fixture was NOT remeasured; do not claim final-tree/measurement
byte identity. The derivation script subsequently added raw control/oracle rows
and persisted metadata input; thresholds and observed input counts unchanged.

## Remaining limitations

Historical `54fc2a21` full-source replacement exceeds permitted runtime scope:
BLOCKED, no verdict, never represented by a lower-bound-only current-tree revert.
M1–M5 red, restored cmp0; M6 uncovered because registry cannot construct the
private witness and the selected tests do not directly cover that hook mutant.
No performance tuning or third Large lookup removal followed NO-GO.

## Final validation receipts

Windows environment for every cargo command: `RUSTC_WRAPPER=`,
`CARGO_BUILD_JOBS=3`, `CARGO_TARGET_DIR="$PWD/target"`, `--locked`.
Affected test matrix features: `production bench-internals internals hardened
medium-classes alloc-stats batch-api`. Fourteen targets, 33 tests passed:
lookup2, Unknown1, bounds2, metadata-region1, provenance2, canonical-cache2,
Tier1-counters2, promoted-Large-routing1, batch-hardened2, hardened-Large4,
hardened-interior2, strict-kind3, kind-aware-entries7, free-budget2.

The separate `--no-default-features` target with `alloc-global fastbin batch-api
medium-classes internals bench-internals` FAILED compilation:
`regression_dealloc_batch_promoted_large_free` calls unsafe realloc and
unsafe dealloc_batch without unsafe blocks (E0133, lines 88/105). The test
was not edited and was not counted as covered.

Clippy `--all-targets -- -D warnings`: `production` FAILED on the unrelated
example `sol_f1_dbg_carve_batch_negative_probe` missing dbg_carve_batch (E0599).
`production internals` PASSED; `hardened medium-classes internals` PASSED.
Sequential shell final exit0 does not erase the two earlier failed commands.

Final rustfmt edition2021 check on all fifteen changed Rust files PASSED;
hook scanner and git diff check PASSED. Scoped stub/private-path scan had no
matches. Final runtime C hash verification PASSED. No git writes.

Earlier narrow WSL Miri command (not rerun in this final chunk):
`cargo +nightly miri test --locked --features "production bench-internals
internals" --test r6_heapcore_provenance --test
heapcore_own_free_single_canonical_lookup -- --nocapture`, same worktree with
`RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target"`.
Three tests passed on miri `0.1.0 (bd08c9e718 2026-06-25)`; this is a prior
receipt, not an independent final Miri rerun.
