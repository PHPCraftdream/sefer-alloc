# R19 PRF02C — C-prime preregistration

Preparation only; no Cargo, Miri, IAI or measurement authorized by this document.
B regate is GO. Existing batch E0133 and hardened clippy blockers remain owner-owned and untouched.

Exactly one candidate: B plus Unknown rejection in the shared own-free guard.
Production uses `matches!(&kind, BlockKind::Large | BlockKind::Unknown)` with a cold `reject_unknown` before any Large geometry. F7(A)/(B) reject Unknown identically.
`BlockKind::of(base, Some(c))` and the complete payload_start expression remain byte-identical to B. No raw decode, split or tuning.

One attempt, two axes (pure / instrumented), arms B / Cprime, two identical repeats per arm and axis. Run order per axis: B1, Cprime1, B2, Cprime2. First absolute Instructions column only.
Push = n9-n8; Cprime-B must lie in [-3,+1] inclusive on BOTH axes.
Raw small churn N and 2N deltas each <= +10 on BOTH axes.
Original controls unchanged: mimalloc N/2N delta exactly 0; Large delta <= +10 one-sided; magazine (hit-prefill) delta within +/-10. All raw counts including diagnostics must repeat exactly. Free average, churn slope /64 and body diagnostic /64 are reported without additional thresholds.
Missing/failed receipts, source/input/identity/prereg drift, repeat mismatch or any threshold failure => NO-GO; no retries, selective reruns or tuning.

Safety review precedes snapshots. The corruption probe uses typed-valid Unknown 0xff only, exclusive owned live header access, no allocation/reallocation or teardown while stamped, RAII restoration of the captured true byte before assertions/teardown. Allocation remains live only on rejection. Negative counterfactual is exact B guard with the same instrumentation: magazine acceptance must make the rejection assertion fail; restoration still precedes teardown. This counterfactual is correctness evidence, not a performance arm.

CLAUDE hook rule 3 decision, same task: on NO-GO revert ALL Cprime source changes including hook, inventory row, header/dealloc contract extension and regression. No independent justification claimed for retaining experiment instrumentation. External source snapshots remain preserved. GO is not permission to land without correctness approval.

Before the attempt capture B-to-Cprime source patch and SHA256; pin source maps, all R inputs, C scripts/prereg and generated harnesses. External storage contains source backups ONLY. Tree stays exact B throughout preparation; driver temporarily installs Cprime and restores B in finally, comparing bytes and hashes. Immutable R prereg/identity/scripts remain unchanged.

Execution explicitly approved after formatting-only rustfmt --edition 2021 edits (hook cfg and regression cfg/initializer/assert wrapping only), before first measurement; same candidate semantics, not a second candidate or workaround. B Miri completed green (canonical lookup 2 tests plus realloc-tail 1). Cargo access remains serial. RUSTC_WRAPPER empty; CARGO_BUILD_JOBS=3; CARGO_TARGET_DIR=<repo>/target; cargo --locked; preinstalled matching IAI runner only, no installation, other checkout, git writes or lockfile edits.
