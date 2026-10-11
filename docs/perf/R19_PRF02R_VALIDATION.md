# PRF02R Stage D — EXACT B validation

C-prime not started. Cargo runs strictly serial under WSL; background jobs awaited. No broad suite/IAI, installs, git writes, other worktrees, or Cargo.lock edits.

Environment: RUSTC_WRAPPER empty, CARGO_BUILD_JOBS=3, CARGO_TARGET_DIR=<this worktree>/target. Historical Stage D Miri receipts use preinstalled nightly 0.1.0 (bd08c9e718 2026-06-25), MIRIFLAGS=-Zmiri-strict-provenance -Zmiri-disable-isolation. These are not the independent top-level strict-provenance runs without disable-isolation. The requested independent top-level coverage is canonical lookup (2 tests) plus r6_heapcore_provenance (1 test), not realloc tail; its receipts are absent from this checkout, so those independent results are not verified here. Local scripts/miri.mjs and .cargo/config.toml inspected read-only. Historical Miri reports wrapper ignored and actual artifacts under target/miri.

## Commands and per-binary results

Each log below contains exact command, stdout/stderr and exit. All successful tests have 0 failed, 0 ignored, 0 filtered, except restored M5 exact filtering (1 filtered). No successful zero-test binary.

### M5-body-restored-green.log

Receipt: `target/r19-prf02r/stage-d-validation/M5-body-restored-green.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test heapcore_own_free_single_canonical_lookup own_free_resolves_once_and_parks_every_block -- --exact --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s

### M5-body.log

Receipt: `target/r19-prf02r/stage-d-validation/M5-body.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test heapcore_own_free_single_canonical_lookup own_free_resolves_once_and_parks_every_block -- --exact --test-threads=1 --nocapture
```
Exit: 101.
Behavioral RED: 1 selected test aborts (SIGABRT); no final libtest passed/failed/ignored summary. Not a compile failure or zero-test success.

### batch-exact.log

Receipt: `target/r19-prf02r/stage-d-validation/batch-exact.log`.

```sh
cargo test --locked --no-default-features --features 'alloc-global fastbin batch-api medium-classes internals bench-internals' --test regression_dealloc_batch_promoted_large_free -- --test-threads=1 --nocapture
```
Exit: 101.
BLOCKED compilation; no tests executed. Existing external/out-of-scope test not edited. Unsafe calls at regression_dealloc_batch_promoted_large_free.rs:88 and :105.

### Exact requested hardened row (completed on B before C measurement)

`cargo clippy --locked --all-targets --features 'hardened medium-classes internals' -- -Dwarnings`

Exit 0; completed serially on exact B. Receipt: `target/r19-prf02c/formatted-exact-B-clippy.log`. No extra production feature. The cancelled top-level run is not counted. The earlier production-containing superset failure below remains a separate row; existing batch E0133 remains unresolved. Full validation is not green. Our B Miri canonical lookup (2 tests) and realloc tail (1 test) completed green, not an outstanding independent run.

### clippy-hardened.log

Receipt: `target/r19-prf02r/stage-d-validation/clippy-hardened.log`.

```sh
cargo clippy --locked --all-targets --features 'production hardened medium-classes internals' -- -Dwarnings
```
Exit: 101.
FAIL compilation: existing test import sefer_alloc::global::tls_heap at r14_4_promotion_free_correctness.rs:92; bench-internals missing in requested hardened clippy row. Out-of-scope test not edited.

### clippy-production.log

Receipt: `target/r19-prf02r/stage-d-validation/clippy-production.log`.

```sh
cargo clippy --locked --all-targets --features 'production internals' -- -Dwarnings
```
Exit: 0.

### exact-b-identity.log

### feature-off.log

Receipt: `target/r19-prf02r/stage-d-validation/feature-off.log`.

```sh
cargo test --locked --features 'production internals bench-internals alloc-stats' --test r14_4_promotion_feature_off_non_disturbance -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

### hardened-r11_4_dealloc_batch_hardened_guards.log

Receipt: `target/r19-prf02r/stage-d-validation/hardened-r11_4_dealloc_batch_hardened_guards.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats hardened batch-api' --test r11_4_dealloc_batch_hardened_guards -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

### hardened-regression_hardened_interior_ptr.log

Receipt: `target/r19-prf02r/stage-d-validation/hardened-regression_hardened_interior_ptr.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats hardened' --test regression_hardened_interior_ptr -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

### hardened-regression_hardened_large_kind_own_free.log

Receipt: `target/r19-prf02r/stage-d-validation/hardened-regression_hardened_large_kind_own_free.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats hardened' --test regression_hardened_large_kind_own_free -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

### hook-scanner-final.log

### hook-scanner.log

### miri-heapcore_own_free_single_canonical_lookup.log

Receipt: `target/r19-prf02r/stage-d-validation/miri-heapcore_own_free_single_canonical_lookup.log`.

```sh
cargo +nightly miri test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test heapcore_own_free_single_canonical_lookup -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 62.38s

### miri-r19_prf02r_realloc_tail.log

Receipt: `target/r19-prf02r/stage-d-validation/miri-r19_prf02r_realloc_tail.log`.

```sh
cargo +nightly miri test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r19_prf02r_realloc_tail -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 84.37s

### miri-r6_heapcore_provenance.log

Receipt: `target/r19-prf02r/stage-d-validation/miri-r6_heapcore_provenance.log`.

```sh
cargo +nightly miri test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r6_heapcore_provenance -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 143.32s

### miri-r6_own_provenance.log

Receipt: `target/r19-prf02r/stage-d-validation/miri-r6_own_provenance.log`.

```sh
cargo +nightly miri test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r6_own_provenance -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 47.98s

### prod-heapcore_own_free_single_canonical_lookup.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-heapcore_own_free_single_canonical_lookup.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test heapcore_own_free_single_canonical_lookup -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

### prod-r11_ph3a_reserve_promotion_fault.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-r11_ph3a_reserve_promotion_fault.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r11_ph3a_reserve_promotion_fault -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s

### prod-r14_4_promotion_free_correctness.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-r14_4_promotion_free_correctness.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r14_4_promotion_free_correctness -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

### prod-r14_4_promotion_move_leg_reduction.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-r14_4_promotion_move_leg_reduction.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r14_4_promotion_move_leg_reduction -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

### prod-r14_4_promotion_shrink_uses_move_leg.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-r14_4_promotion_shrink_uses_move_leg.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r14_4_promotion_shrink_uses_move_leg -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

### prod-r19_prf02r_realloc_tail.log

Receipt: `target/r19-prf02r/stage-d-validation/prod-r19_prf02r_realloc_tail.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r19_prf02r_realloc_tail -- --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

### realloc-tail-restored-green.log

Receipt: `target/r19-prf02r/stage-d-validation/realloc-tail-restored-green.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r19_prf02r_realloc_tail realloc_tail_and_inplace_activation -- --exact --test-threads=1 --nocapture
```
Exit: 0.
ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

### realloc-tail.log

Receipt: `target/r19-prf02r/stage-d-validation/realloc-tail.log`.

```sh
cargo test --locked --features 'production internals bench-internals medium-classes alloc-stats' --test r19_prf02r_realloc_tail realloc_tail_and_inplace_activation -- --exact --test-threads=1 --nocapture
```
Exit: 101.
Behavioral RED: 1 selected test aborts (SIGABRT); no final libtest passed/failed/ignored summary. Not a compile failure or zero-test success.

### rustfmt-2021.log

Receipt: `target/r19-prf02r/stage-d-validation/rustfmt-2021.log`.

```sh
rustfmt --edition 2021 --check src/alloc_core/alloc_core/mem/mem_impl.rs src/alloc_core/alloc_core/mem/mod.rs src/alloc_core/alloc_core/mod.rs src/alloc_core/mod.rs src/registry/heap_core/diag/diag_probes.rs src/registry/heap_core/free/dealloc_own_base.rs src/registry/heap_core/free/realloc.rs src/registry/heap_core_xthread/routing.rs src/alloc_core/alloc_core/mem/owned_block.rs tests/heapcore_own_free_single_canonical_lookup.rs tests/r19_prf02r_realloc_tail.rs benches/r19_prf02_instrumented_iai.rs benches/r19_prf02_production_iai.rs examples/r19_prf02_activation.rs
```
Exit: 0.

### rustfmt.log

Receipt: `target/r19-prf02r/stage-d-validation/rustfmt.log`.

```sh
rustfmt --edition 2024 --check src/alloc_core/alloc_core/mem/mem_impl.rs src/alloc_core/alloc_core/mem/mod.rs src/alloc_core/alloc_core/mod.rs src/alloc_core/mod.rs src/registry/heap_core/diag/diag_probes.rs src/registry/heap_core/free/dealloc_own_base.rs src/registry/heap_core/free/realloc.rs src/registry/heap_core_xthread/routing.rs src/alloc_core/alloc_core/mem/owned_block.rs tests/heapcore_own_free_single_canonical_lookup.rs tests/r19_prf02r_realloc_tail.rs benches/r19_prf02_instrumented_iai.rs benches/r19_prf02_production_iai.rs examples/r19_prf02_activation.rs
```
Exit: 1.

## Mutants and restoration

M5-body adds only `let _second_lookup = self.core.canonical_block_of(block);` immediately after the owned root/block unpack. Counted invocation observed: hits=32, misses=0, lookups=32, tcache_count=16, versus B lookups=16. Lookup assertion at line42 fails 32 !=16, cargo exit101/SIGABRT. Restored exact test 1 passed.

Separate realloc move-tail mutant wraps only the move-tail free call and adds the same real canonical_block_of(block) immediately before it. Tail assertion at line72 fails Some(1) != Some(0), cargo exit101/SIGABRT. Restored exact test 1 passed; inplace=1 alloc=0 move=1 promote=2 tail=0 large_free=2.

Each source backup is external /tmp via cp only, restored via cp; cmp0 and SHA256 before/restored equality recorded in *-restore.json; backup paths and exact edits in *-mutation.json. Diffstat before/restored byte-equal: 9 files, 64 insertions, 48 deletions. All pinned source, Cargo.lock, Cargo.toml and identity hashes unchanged.

Initial mutant harness expected a final libtest FAILED summary and stopped after actual behavioral M5 SIGABRT; finally restored source. Corrected receipt acceptance to require the actual assertion panic, then reran both mutants serially and restored/green-checked both.

## Findings and tooling

rustfmt --edition 2021 --check passes across all 14 PRF02 Rust files; check-only preserves EXACT B. Initial erroneous edition2024 check reported formatting changes and was superseded by package edition2021 check. No formatting writes.

Actual hook scanner PASS: 208 reviewed safe, 33 reviewed unsafe, 102 genuinely bench-gated safe hooks.

Production clippy --all-targets -Dwarnings PASS. Exact hardened/medium/internals row separately completed with exit 0; the earlier production-containing superset failed on an existing test import and is not that exact row. Exact promoted batch regression remains blocked by existing E0133. Independent final clippy receipts for both requested exit-0 rows are absent from this checkout; do not relabel these historical receipts as independent final runs. Continue unblocked points completed; no GO and no C-prime authorization implied.

Move-leg reduction binary reports 5 passed, but its two promotion-off named tests return early when promotion is enabled; do not treat those as active off-feature coverage. Separate feature-off binary genuinely executes 2 tests without medium-classes. Hardened batch row adds batch-api because actual file gate requires it.

Final identity verification: all 10 B source hashes and 428 input hashes match; no mismatches. Identity file itself unchanged. See target/r19-prf02r/stage-d-validation/exact-b-identity.log.

## Doc-only closing audit

No cargo, measurement, git mutation, external writes, or pinned identity/preregistration/raw measurement edits performed during this audit. Only authorized Stage D scratch logs had the private worktree cwd replaced with `<repo>` (22 logs, 132 occurrences); command/result bytes otherwise preserved. Stage-B.diff needed no cwd replacement.

Historical promotion receipts above show 3 + 5 + 2 + realloc tail 1 = **11 passed, 0 failed**, all exit 0. Requested independent final promotion receipts are absent; this is not evidence of a new final run. The two promotion-off early returns remain qualified above.

Read-only hash verification: original source union = 159 source_inputs src entries + 10 disjoint arms.B entries = **169**, all match; R inputs = **428**, all match. C raw first absolute Instructions columns independently reproduce **198 markdown arithmetic rows**, including both repeat matches, without using result JSON. Cprime-B push is +11 on both axes; raw N/2N deltas are +706/+1410 pure and +704/+1408 instrumented: **NO-GO** remains.

Requested `docs/perf/_raw_r19_prf02r/c_*` directory/files do not exist here. Actual R/C evidence uses flat `_raw_r19_prf02r_*` and `_raw_r19_prf02c_*` logs. Private absolute-path scan of all 38 PRF02-family flat raw logs found no matches. Ignored R+C raw inventory: **20 files, 189302 bytes (<200 KiB)** (R: 12/100942; C: 8/88360). Historical original raw adds 18/134075; all-family total is 38/323377, not <200 KiB.

Grouped current git inventory (existing task changes included): Cargo.toml 1; src 9; benches 2; docs 19; examples 1; scripts 7; tests 2. Existing external batch E0133 remains unresolved; full validation is not green.
