# CI long-job input collection (discovery stage)

This directory holds gzip-compressed normalized GitHub Actions job snapshots and compact collection metadata. Existing full snapshots: `43704208363.json.gz`, `43781371392.json.gz`, `43851422289.json.gz`; ordinary push comparison: `43831250776.json.gz`. Added snapshots: `43886332211.json.gz`, `43582569269.json.gz`, `43410623227.json.gz`. `collection.json` records the bounded run-list provenance, API totals, request ledger, candidate classification, and exclusions; it is metadata, not a jobs snapshot.

## Discovery and classification

One workflow-runs request used `GET /repos/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml/runs?per_page=100`. GitHub reported `total_count: 1432`, returned 100 newest records. Selection is based on schedule events (full regular CI tier), or a standalone `CI: full` trailer in the full commit message; no run was selected because it succeeded. Records below are newest first among the six full-tier candidates with saved job snapshots:

| run | created (UTC) | event / trigger | conclusion | long jobs >600s | interpretation |
|---|---|---|---|---:|---|
| 43886332211 | 2026-11-16 14:29:53 | schedule | failure | 13 (12 with a runner) | full-tier attempt; retain observed jobs, censor failed jobs; one cancelled macOS job had runner_id 0 and no steps |
| 43851422289 | 2026-11-16 10:43:41 | schedule | success | 12 | completed full nightly; four expected weekly/dispatch-only skips |
| 43781371392 | 2026-11-16 00:36:29 | pull_request; standalone `CI: full` trailer | success | 11 | completed full PR; four expected weekly/dispatch-only skips |
| 43704208363 | 2026-11-15 10:28:04 | schedule | success | 11 | completed full nightly; four expected weekly/dispatch-only skips |
| 43582569269 | 2026-11-14 09:52:11 | schedule | failure | 10 | full-tier attempt; multiple ordinary jobs failed, retain as censored evidence |
| 43410623227 | 2026-11-13 10:38:25 | schedule | failure | 8 | full-tier attempt; multiple ordinary jobs failed, retain as censored evidence |

These are six full-selected/attempted runs, not six successful full runs. Failure does not erase job timing or make the run disappear from the discovery set. Failed steps are right-censored observations for successful-duration analysis; do not describe these three failures as completed successful executions. Run `43831250776` is a successful ordinary push comparison (0 jobs >600s), not a full-tier candidate.

### Newly fetched candidates: incomplete/failed groups

- `43886332211`: two weekly jobs failed: `fuzz targets run (weekly, 10 min/target)` (job 133365374209) and `cargo-hack feature-powerset (depth 2, check-only, weekly)` (133365374290). `numa-shim real macOS kernel (unsupported-platform contract)` (133365637356) was cancelled with runner_id 0 / empty runner name and no steps; its 1,013-second queue/job interval is not runner execution and is excluded from executed-long-job counts. The arm64 weak-memory job (133365376138) was skipped without a runner. No `annotations_url` was supplied by the jobs endpoint, so no more specific cause is asserted.
- `43582569269`: 11 failed jobs, including xthread progression, hardened tier, macOS production, check-matrix, aarch64/x86_64 Linux, feature isolation, ghost build, Windows production, gated bodies/all-features, and clippy. All are retained; failed step names/conclusions appear in the snapshot. Four weekly/arm64 optional jobs were skipped without runners.
- `43410623227`: 11 failed jobs, including macOS production, xthread progression, gated bodies/all-features, check-matrix, feature isolation, clippy, ghost build, Windows production, hardened tier, and aarch64/x86_64 Linux. All are retained; failed step names/conclusions appear in the snapshot. Four weekly/arm64 optional jobs were skipped without runners.

For the two older failed schedules, the skipped groups are the expected weekly-only/arm64 jobs, not additional unexplained omissions. No annotations were present in any of the three new jobs API payloads. Job and step conclusions, timestamps, runner fields, and the actual failed/cancelled job/step records are retained in the normalized gzip snapshots.

## Long jobs (>600 seconds)

Durations are completed_at minus started_at. IDs below are GitHub job IDs; `>600s` is strict. Cancelled jobs without a runner are listed separately rather than counted as executed long jobs.

| run | count | job IDs and names |
|---|---:|---|
| 43886332211 | 13 (12 executed) | 133365374209 fuzz targets run (weekly, 10 min/target); 133365635759 check-matrix (manifest-driven, non-clippy rows); 133365636081 test windows (production); 133365636120 test (xthread progression); 133365636247 test (gated bodies + all-features); 133365636628 miri (fastbin); 133365636679 kani proofs; 133365636691 test (hardened tier); 133365636764 miri (core); 133365636928 miri-plain; 133365636939 miri (core, pin cache TreeBorrows); 133365637282 test (aarch64-unknown-linux-gnu); 133365637356 numa-shim real macOS kernel (unsupported-platform contract; cancelled, no runner) |
| 43851422289 | 12 | 133244747299 miri (core); 133244747380 test (xthread progression); 133244747420 test (gated bodies + all-features); 133244747477 test windows (production); 133244747486 test (hardened tier); 133244747496 check-matrix (manifest-driven, non-clippy rows); 133244747615 miri (core, pin cache TreeBorrows); 133244747662 kani proofs; 133244747713 miri-plain; 133244747810 test (aarch64-unknown-linux-gnu); 133244747871 miri (fastbin); 133244747902 test (feature isolation) |
| 43781371392 | 11 | 133019282343 test (xthread progression); 133019282569 test (hardened tier); 133019282593 test windows (production); 133019282627 check-matrix (manifest-driven, non-clippy rows); 133019282646 test (gated bodies + all-features); 133019282675 miri (fastbin); 133019282698 test (aarch64-unknown-linux-gnu); 133019282717 miri (core, pin cache TreeBorrows); 133019282727 miri (core); 133019282758 miri-plain; 133019282782 kani proofs |
| 43704208363 | 11 | 132795418733 miri (core); 132795418746 test (xthread progression); 132795418762 miri-plain; 132795418785 test (hardened tier); 132795418889 test (gated bodies + all-features); 132795418911 miri (fastbin); 132795418938 check-matrix (manifest-driven, non-clippy rows); 132795418958 test (aarch64-unknown-linux-gnu); 132795418960 miri (core, pin cache TreeBorrows); 132795419078 test windows (production); 132795419134 kani proofs |
| 43582569269 | 10 | 132410259960 kani proofs; 132410260009 test (xthread progression); 132410260031 test (hardened tier); 132410260038 miri (core); 132410260048 miri (core, pin cache TreeBorrows); 132410260141 miri (fastbin); 132410260146 check-matrix (manifest-driven, non-clippy rows); 132410260356 test (feature isolation); 132410260379 test (gated bodies + all-features); 132410260402 miri-plain |
| 43410623227 | 8 | 131835906890 test (xthread progression); 131835907176 check-matrix (manifest-driven, non-clippy rows); 131835907208 miri (core); 131835907233 miri (core, pin cache TreeBorrows); 131835907334 miri (fastbin); 131835907493 test (hardened tier); 131835907570 kani proofs; 131835907640 miri-plain |
| 43831250776 | 0 | none |

## Sequential job-log collection and phase derivation

The log stage used the remaining 25 `gh` invocations exactly once and sequentially. Together with the 4 discovery calls and the previously fetched sample log (5 calls already used), the request ledger is exactly 30/30. Target coverage is 26 logs (25 new plus sample `43410623227/131835907208`): 23 of 25 new log responses succeeded (22,113,467 raw bytes) and two timed out without an HTTP response; no retries were made. Including the earlier sample, 24/26 targets have raw logs. The failed requests are `132410259960` (kani proofs, run 43582569269) and `132795419134` (kani proofs, run 43704208363); both are retained as failed-fetch records with job metadata and no invented phases.

The gzip schema encodes aggregate action phases and elapsed bounds, aggregate group occurrences and bounds, commands with compile/execution sums and counts (not per-test timing rows), and up to 50 slow exact Cargo targets per job with a remainder count/sum. Command and target attribution flags preserve known, ambiguous, and unavailable cases; per-target totals include only a proven single Cargo command with one explicit target, while multi-command and multi-target timing stays in phase/command aggregates and is not duplicated across targets. Parsed jobs' maximum unknown partition is 7.454 seconds (sample `131835907208`), next 7.167 seconds (`132795419078`); all 24 available per-job phase partitions reconcile to API duration. The two timed-out jobs have no phase derivation. Cache phases were not observed in parsed logs (zero classified seconds), which does not imply cache work is absent.

| representative job family | run/job | setup | toolchain | build | execution | other | unknown |
|---|---|---:|---:|---:|---:|---:|---:|
| test (xthread progression) | 43410623227/131835906890 | 3.589 | 8.955 | 217.301 | 742.743 | 9.601 | 3.811 |
| check-matrix | 43704208363/132795418938 | 4.763 | 10.275 | 313.972 | 1476.508 | 22.035 | 2.447 |
| miri (core) | 43704208363/132795418733 | 3.511 | 19.544 | 28.665 | 1270.997 | 2.291 | 5.992 |
| miri (core, pin cache TreeBorrows) | 43704208363/132795418960 | 4.213 | 29.127 | 21.861 | 2097.167 | 1.026 | 1.606 |
| miri (fastbin) | 43704208363/132795418911 | 3.924 | 26.986 | 27.212 | 1979.347 | 5.757 | 1.774 |
| test (hardened tier) | 43410623227/131835907493 | 2.276 | 8.236 | 165.319 | 650.587 | 8.315 | 2.267 |
| kani proofs | 43410623227/131835907570 | 6.801 | 16.930 | 3.482 | 1200.775 | 4.823 | 5.189 |
| miri-plain | 43704208363/132795418762 | 3.802 | 23.050 | 19.753 | 1214.649 | 1.918 | 1.828 |
| test (feature isolation) | 43582569269/132410260356 | 3.060 | 10.817 | 132.479 | 463.776 | 3.293 | 3.575 |
| test (gated bodies + all-features) | 43582569269/132410260379 | 3.980 | 8.458 | 154.620 | 493.757 | 4.925 | 3.260 |
| test (aarch64-unknown-linux-gnu) | 43704208363/132795418958 | 3.675 | 39.287 | 241.406 | 1444.522 | 10.517 | 2.593 |
| test windows (production) | 43704208363/132795419078 | 7.892 | 17.864 | 302.113 | 520.554 | 17.410 | 7.167 |

Seconds; cacheRestore and cacheSave are zero for parsed logs. The three other >10-minute jobs in run `43704208363`—`132795418746` (xthread progression), `132795418785` (hardened tier), `132795418889` (gated bodies + all-features)—are an explicit coverage gap because the remaining request budget could not cover every long job. The budget was not exceeded to fetch them.

The parser classifies build time only from anchored Cargo package status lines (`Compiling <crate> v…` / `Checking <crate> v…`); Kani `Checking harness …`, `VERIFICATION:`, `Verification Time:`, propositional-reduction, and object-bit verifier messages are execution. A `Finished dev` line ends the Cargo build interval; it does not assign the subsequent Kani proof to build. Per-target execution totals are emitted only when one Cargo command is proven in its Run group and exactly one explicit target is present; multi-command and multi-target cases remain ambiguous and are excluded from target totals.


The input directory, including snapshots, README, collection ledger, and compact derivation, remains under the 204,800-byte cap. Runner metadata and API snapshots remain as described above. No cargo commands or CI jobs were run.
