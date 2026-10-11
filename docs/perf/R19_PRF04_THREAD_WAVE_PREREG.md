# R19 PRF04 C1 — bench / measurement-only baseline preregistration (not measured)

Status: implementation/validation only. No measured verdict, no V4, no policy change.
The owner decision, question 3 in the 2026-10-11 decisions document, supersedes
the stale NOT-approved wording of perf item 87; the index is intentionally untouched.

## Fixed experiment

Only A, current fresh-first policy; K=32, W=1 and W=128, five fresh subprocesses
per cell: **ten primary samples total**. Order alternates short/long across
repetitions. Two additional diagnostic subprocesses (one per cell) are activation
checks, not primary samples. No warmup waves, retries, R-live, R-churn or B arm
in C1. A failed oracle stops the run; do not silently replace samples.

Entry is an actual `#[global_allocator] static GLOBAL: SeferAlloc`.
Each worker allocates 128 objects at each of 64, 256, 1024, 2048 and 4096 bytes
(958464 Small payload bytes), plus one 2 MiB Large object, touches each page,
frees all workload objects, then waits at a common barrier. Main samples memory
while all 32 threads remain alive, releases a second barrier, and joins every
native thread before spawning the next wave. The common barrier prevents early
TLS exit hints during another worker's claim. `maintenance_running()` must remain
false; the harness never requests startup.

Main binds and remains LIVE before taking the baseline: raw count_before=1.
Derived worker count increment is `1 + W*(32-1)`: 32 / 3969.
Raw count_after is **33 / 3970**, including main. These are code-derived
expectations, not measurements. Report before, after and delta; never relabel
3969 as the whole process count. Diagnostic successful-claim deltas are
`[hint, bump, cap_scan, recovery] = [W-1, 1+31W, 0, 0]`, summing to `32W`.
Counters increment only immediately before a successful lease return, excluding
lost CAS, chunk OOM and constructor failure; all added code is compiled out
without bench-internals. Bump is selector provenance, not merely first-claim state.

## Features, configuration and corrections to the research sketch

Primary: exact `production`, no bench-internals or alloc-stats. Diagnostic:
`production bench-internals alloc-stats internals`, separately built/copied/hashed.
`internals` is needed to access the existing registry and AllocCore observer surfaces;
it is not added to the primary build. The initial diagnostic compile without it
failed with private-module/missing-observer errors; no production exposure is widened.
Timing
fields are absent in diagnostic JSON and aggregation rejects diagnostic timings.
No production feature composition changes.

Requested config is `LargeCacheConfig::new()` (SeferAlloc::new). In diagnostics,
every successful lease compares the actual LIVE resolved policy via the existing
`live_config_matches` under its newly acquired exclusive authority, records
matches/mismatches, and asserts all `32W` worker leases match, mismatches=0.
All builds assert process config-conflicts before/after=0. Primary configuration
readback is validated by the matched diagnostic build/workload, not falsely
claimed as a primary observer. No new raw-pointer public hook is introduced.

**Research correction:** zero config conflicts alone is not resolved-value
readback. The new diagnostic match oracle supplies it. The existing
`AllocCore::dbg_routed_miss_scans()` counts only with alloc-stats; bench-internals
alone would return a misleading zero. Hence alloc-stats is diagnostic only;
primary emits null, not measured zero. Report the diagnostic routed scan count,
do not assert scans=0 without evidence. This is the existing negative-directory
scan observer, not the separate r18_sidecar_scan_bench custom-cfg observer.

**Research correction:** commit is not portable virtual address space. Use the
fallible proc-probe snapshot and keep RSS, optional commit_charge and virtual_size
separate. Missing commit_charge means the commit threshold is INCONCLUSIVE;
never substitute virtual_size or charged_or_reserved_bytes for commit. Peak fields
are maxima of barrier/idle snapshots, **not continuous OS peak**.

First-workload-alloc latency uses Instant around the first explicit worker Vec
allocation, not native-thread startup-to-first TLS bind: std thread setup may
already allocate before the closure. It is honestly labelled
`first_workload_alloc_ns`; it is descriptive only, **not valid KILL-3 first-bind
evidence**. The research's first-alloc timing aspect is halted pending a separately
validated thread-entry/first-bind observer; C1's retention decision does not use it.
Wall time includes spawn, work, barrier snapshots, native join and TLS teardown.

## Predetermined decision boundary

Compute arithmetic median of five process-idle commit-charge samples in each
cell, then `median(W128) - median(W1)` in bytes. Below 67108864 bytes permits
NULL/V0 without V4, after checking all oracles. At/above that floor permits only
consideration of C2: **not GO**. Missing commit data or failed identity/oracle
means INCONCLUSIVE, not NULL. RSS and incremental RSS are reported distinctly;
RSS cannot replace commit in this decision.

Future B promotion (not implemented/run): primary benefit K32/W128 median commit
improvement at least max(64 MiB, 25% of A); R-live p50 within 5% and paired-t
criterion; R-churn wall within max(2%, A/A noise); first-bind p99 B <= 1.10*A is
hard KILL-3. A/A noise wider than the threshold fails closed. No promotion based
on these ten A-only samples. NUMA has no separate policy; V6 remains a separate
future gate only if measured FREE retention still >=64 MiB after the V4 decision.

## Reproduction and artifacts

From this isolated repository root, after orchestrator approval:

```text
node scripts/r19_prf04_thread_wave.mjs --run docs/perf/R19_PRF04_A_run01
```

The runner makes no git writes and reads no other worktree. Before measurement
it saves a binary tracked diff against HEAD **plus added-file patches for all
untracked Rust/JS/TOML source and this preregistration**, hashes that immutable
patch, captures base SHA, rustc -vV, host target, release/default-feature metadata,
requested and resolved Cargo features, OS/CPU, and both copied binary SHA-256s.
Binary copies live only under `target/r19_prf04_<run-id>/`, after verifying the
existing target parent; identity names are repository-relative paths under target,
never private absolute paths. The docs run directory contains only the small
identity/raw JSON, source patch and derived CSV/Markdown tables, no executables.
It rejects flag/target overrides, source drift, private-path text, existing output
directories and artifact overwrites. Cargo is sequential, --locked,
RUSTC_WRAPPER empty, CARGO_BUILD_JOBS=3, target inside this worktree. Build errors
are not copied to artifacts because they can include private absolute paths.
Raw JSON is saved after each validated sample, before derived tables.

One script validates count/source/config arithmetic, exact cell sizes, immutable
identity and the median difference, then emits summary CSV and Markdown tables.
Statistic names accompany the computations. No handwritten measured tables.
Use `--aggregate` with an existing run directory only before derived files exist;
no-overwrite prevents silently rewriting published evidence.

Validation without allocator measurement:

```text
node scripts/r19_prf04_thread_wave.mjs --self-test
node scripts/verify-dbg-hook-safety.mjs
```

Self-test fixtures are synthetic and include invalid count, config, maintenance,
source-sum, resolved-default and diagnostic-timing rejection controls. Existing
selector and OOM tests are the narrow Rust regressions; new runtime wave tests
would duplicate the explicitly deferred measurement and are not added in C1.
No README, architecture, changelog, index, manifest or lockfile updates in C1.
