# 04 — Test infrastructure & seams: extraction candidates + in-place testability wins

Research lane: **test infrastructure & measurement (testability angle)**.
Read-only survey, 2026-07-16. Siblings cover data structures, concurrency, OS/platform.

The question answered here: *what test/measurement infrastructure could become
reusable community crates, and how could restructuring improve this project's
own testability?*

Inventory surveyed: `scripts/*.mjs` (12 runners), `benches/` (15 targets, incl.
the R6-OPT-A judge family), `tests/` (~190 files, incl. 3 differential-model
tests and 18 loom models), `fuzz/fuzz_targets/` (3 targets),
`crates/malloc-bench` (already-extracted single-file crate),
`examples/` (process-probe binaries), plus the confined-unsafe two-tier seam and
`#[doc(hidden)]` `dbg_*` accessor patterns from `CLAUDE.md` / README.

---

## C1. Allocator differential-testing harness (op-stream vs reference model)

**What it is / files.** Three near-identical implementations of the same idea —
apply a random op stream (`Alloc {size, align}` / `Dealloc(i)` /
`Realloc {i, new_size}` / `AllocZeroed`) to the allocator under test AND to a
trivial reference model (a `Vec<Live {ptr, size, align}>`), asserting the
M1–M4 oracles (validity/alignment, double-free-is-no-op, no byte overlap
between live blocks, fill-pattern write/read-back, zeroed contract, realloc
prefix preservation):

- `tests/alloc_core_differential.rs` (221 lines, proptest, 64 cases)
- `tests/heap_differential.rs` (183 lines, proptest — historically the `Heap`
  face, now a 1:1 `AllocCore` duplicate)
- `fuzz/fuzz_targets/global_alloc_ops.rs` (libFuzzer + `arbitrary`, the same
  model and the same oracles, third copy)
- `tests/differential.rs` is the same *pattern* one layer up (the `Region`
  container vs a `Vec` model, I1–I5 incl. drop-once accounting) — evidence the
  pattern generalizes beyond raw allocators.

**Coupling.** Remarkably low. The model only needs `alloc/dealloc/realloc/
alloc_zeroed` over `Layout` — i.e. exactly the `GlobalAlloc` surface. The only
sefer-specific parts are the size-strategy constants (`SMALL_MAX`-shaped
weights) and the `AllocCore::new()` constructor call.

**Extraction effort.** Low–medium (1–2 days). Define a driver generic over a
minimal `unsafe trait RawAllocator` (blanket-impl for any `GlobalAlloc`), move
the `Live` model + oracles + `Op` enum into a lib, ship two front-ends:
a proptest `Strategy<Vec<Op>>` and an `impl Arbitrary for OpStream` so the same
model powers both `cargo test` and `cargo fuzz`. Size-distribution knobs
(small-class cap, large-arm weight) become a `Config`.

**Testability gain — this project.** The three copies already drifted once (the
realloc-tail re-fill fix had to be mirrored from `heap_differential` into
`alloc_core_differential` by hand, per the comment at
`tests/alloc_core_differential.rs:186-192`). One shared model = one oracle to
harden, and every oracle improvement automatically reaches proptest, miri
(the bounded run), and libFuzzer. **Community:** anyone writing an allocator in
Rust rebuilds exactly this harness from scratch; nothing on crates.io offers a
ready "differential-test your `GlobalAlloc` against a model with UAF/overlap/
zeroed/realloc oracles" kit.

**Community value.** High. This is the correctness-side twin of
`malloc-bench-rs` (performance side). Pitch: *"proptest + cargo-fuzz your
`GlobalAlloc` in 10 lines — M1–M4 oracles included."*

**Suggested name/scope.** `globalalloc-model` (or `alloc-diff`): op-stream
generator, reference model, oracles; proptest + `arbitrary` front-ends;
zero non-dev dependencies.

---

## C2. `malloc-bench-rs` — the larson/mstress macro-benchmark harness

**What it is / files.** `crates/malloc-bench/src/lib.rs` (~600 lines, single-file
seam crate, publish-ready metadata already in `Cargo.toml`: description,
keywords, docs.rs link, MIT/Apache-2.0). Generic-over-`GlobalAlloc` MT harness:
larson (server churn with cross-thread mailbox handoff) + mstress (batch
fill/half-free/refill), deterministic xorshift64 seeds, barrier-aligned
steady-state timing, a carefully closed leak window on thread-finish skew,
`run()` + `sweep()` API.

**Coupling.** None — it never mentions sefer. It is *already extracted*; it is
just not published.

**Extraction effort.** Near zero for the crate itself. The real remaining work
is inside this repo: `examples/malloc_macro.rs` carries an acknowledged
**second independent copy** of the same workload (module doc, "deliberate
duplication", task #28) because the crate API lacks a per-thread pin hook for
the `pinning`/`PinnedRunner` sweep and because the README MT-table numbers were
never re-plumbed. Add a `make_alloc`-style per-thread `on_thread_start(t)` hook
(or a `Runner` trait) and the example copy can be retired.

**Testability gain — this project.** Retiring the duplicate kills a documented
will-drift liability. **Community:** it is the pure-Rust answer to
`mimalloc-bench` (C-only) — its own README already pitches it that way.

**Community value.** High, and the cheapest win in this document: publish it.

**Suggested name/scope.** Keep `malloc-bench-rs`; add the pin hook + a tiny
CLI example printing a `sweep()` comparison table.

---

## C3. Criterion 3-arm comparison harness ("SeferAlloc vs mimalloc vs System")

**What it is / files.** `scripts/bench-table.mjs` (328 lines) +
`benches/global_alloc.rs`. Runs one criterion bench, parses stdout (time
triples, `change:` triples, verdicts), and always prints the SAME four
`{arm}×{size}` tables in the SAME unit (ns per op, scaled by the bench's
ops-per-iteration constant) with a vs-mimalloc ratio column, plus honest
UNSCALED sections for groups that have no per-op constant, plus a
run-over-run change appendix. Born from a real incident: a µs/batch vs ns/op
unit mixup that masqueraded as a 2× regression.

**Coupling.** Medium-high. The parser and table printer are generic; the
`SIZED_GROUPS` list, `OPS = 1024`, `SIZES`, `ARMS`, and the per-group prose
(including line-number references into the bench source) are hand-mirrored
copies of `benches/global_alloc.rs` constants — the classic drift-by-mirror
shape.

**Extraction effort.** Medium. As a community tool it needs a config manifest
(groups, arms, per-group scale, expected ids) instead of hardcoded constants.
Honest caveat: `critcmp` and `criterion-table` already cover baseline
comparison; the *novel* part here is only (a) per-group op-count normalization
to a single honest unit and (b) the cross-*arm* ratio column with an
expected-ids completeness gate. That is a thin layer — worth extracting only
as a small config-driven tool, not a framework.

**Testability gain — this project (bigger than the extraction).** Kill the
mirror: make `benches/global_alloc.rs` emit a machine-readable manifest
(e.g. one `MANIFEST group=global_alloc ops=1024 sizes=16,64,256,1024` line per
group, in the same spirit as the `RESULT key=value` protocol of C5) and have
the script consume it. Then a bench edit can never silently desynchronize the
table. **Community:** modest — a "compare N allocator arms honestly" preset.

**Community value.** Medium-low (overlap with critcmp). The incident-driven
*discipline* (fixed unit, fixed shape, completeness gate) is the valuable part
and is better told as documentation.

**Suggested name/scope.** `criterion-arms` (npm-free Node script or a small
Rust binary): config-driven arm×param tables + ratio column + expected-id gate.

---

## C4. iai instruction-count perf-gate helper (WSL judge + marginal Ir/op)

**What it is / files.** `benches/perf_gate_iai.rs` (the Linux-only
iai-callgrind gate, 12 benches, CI baseline via `actions/cache` +
`IAI_CALLGRIND_REGRESSION='Ir=10'`) and `scripts/iai.mjs` (452 lines): runs the
gate from a Windows dev host through WSL + Valgrind, encapsulating three
researched traps (Windows `sccache.exe` leaking via `RUSTC_WRAPPER` into WSL;
dedicated Linux target dir; runner-version pinning to the lib version), parses
Ir + cache-sim columns (L1/L2/RAM/Estimated Cycles), and computes a
**marginal Ir/op** column: subtract the process-bootstrap constant (proxied by
the `large_alloc_free_cycle` bench) and divide by each bench's op count, so
per-op regressions are comparable across benches whose raw sums are 58–90%
shared bootstrap.

**Coupling.** Medium. The WSL plumbing, runner install/pinning, and the
Ir/cache parser are fully generic. The `BENCH_OPS` map and the bootstrap-proxy
choice are project-specific and hand-mirror the bench source constants
(same drift shape as C3).

**Extraction effort.** Medium (2–3 days): config file for
`{bench_target, features, bench_ops, bootstrap_proxy}`; keep the
"pass = every requested bench produced an Ir" semantics.

**Testability gain — this project.** Encode op counts in bench names
(`cold_alloc_free_256x16b` already does — `256x` is parseable) or emit them
from the bench itself, and derive `BENCH_OPS` instead of mirroring it.
**Community:** two genuinely unserved pieces: (1) "run iai-callgrind from
Windows via WSL without re-learning the sccache/target-dir/runner-version
traps"; (2) the marginal-Ir/op decomposition — a documented, transferable
technique for any crate whose benches share a large one-time constant
(any allocator, any DB engine with a bootstrap, any JIT warmup).

**Community value.** Medium-high in a narrow niche (perf-gated crates with
Windows developers). The decomposition technique deserves a write-up
regardless of tooling.

**Suggested name/scope.** `iai-judge` (or `cargo-iai-wsl`): WSL bridge +
parser + config-driven marginal-per-op table; Ir stays the pass/fail metric,
cache columns best-effort.

---

## C5. Process-per-sample probe family (first-alloc RSS/commit, dealloc-only, paired A/B/B/A)

**What it is / files.** A three-tool family built on one protocol — a probe
binary prints machine-parseable `RESULT key=value` lines; a runner launches it
N times as **fresh processes** and aggregates min/median/max:

- `scripts/first-alloc-bench.mjs` + `examples/first_alloc_process.rs` — the
  first-touch RSS + **commit-charge** (a deliberately separate axis: Windows
  `PagefileUsage` vs `WorkingSetSize` — demand-zero commit is invisible to
  RSS) + first-alloc latency judge. Exists because criterion/iai amortize any
  once-per-process cost into invisibility; caught the ~16 MiB registry
  first-touch (RAD-1) and remains its regression guard.
- `scripts/dealloc-only-bench.mjs` + `examples/dealloc_only_unbound_thread.rs`
  — "a thread whose first allocator call is a foreign free" judge over a
  (B, T, mode) matrix; thread-local bindings persist for a thread's lifetime,
  so only a fresh process can test a never-bound thread.
- `scripts/paired-ab-runner.mjs` (408 lines) + `examples/paired_ab_{sefer,
  mimalloc,system}.rs` + `examples/_shared/paired_ab_workload.rs` — the
  general A/B/B/A process-level paired judge: alternating launch order that
  cancels monotonic host drift, hand-rolled paired t-test + sign test,
  same-vs-same control mode (`--arms sefer,sefer`, the honesty check),
  installed-allocator sanity gate (sefer's segment counter must be non-zero in
  the sefer arm, exactly 0 elsewhere), and full provenance JSON per run (raw
  samples, git hash, rustc version, CPU, power plan) into
  `docs/perf/paired_ab_runs/`. It is itself the "reusable version" of a
  one-off script the R5-R2 investigation wrote and threw away.

**Coupling.** Runners: low (the `RESULT` protocol, stats, A/B/B/A order and
provenance capture are generic; only default example names/features are
project-specific). Probe binaries: sefer-specific by nature, but the
RSS/commit reading code (`K32GetProcessMemoryInfo` / `/proc/self/statm`) is a
reusable building block.

**Extraction effort.** Medium (3–5 days for the family). Ship: (a) a tiny Rust
`proc-probe` lib — `emit("key", value)` + cross-platform RSS/commit/peak
readers; (b) a runner that takes `{binary, samples, metrics}` config;
(c) the paired A/B/B/A runner taking two arbitrary commands as arms.

**Testability gain — this project.** Mostly already realized (the family IS
the reusable form of prior one-offs). Remaining: `first-alloc`, `dealloc-only`
and `paired-ab`'s three probe binaries each hand-roll the same RSS/commit
readers — fold them into one shared `examples/_shared/` module (the
`paired_ab_workload.rs` precedent shows the pattern). **Community:** criterion
fundamentally cannot measure once-per-process effects (first-touch RSS, commit
charge, TLS-bind latency), and "paired process-level A/B with a t-test +
same-vs-same control" is what every "my allocator is faster" claim should be
backed by and almost never is.

**Community value.** High. Nothing on crates.io/npm packages this protocol;
it applies to any runtime with process-lifetime effects (allocators, JITs,
DB engines, CLI cold-start).

**Suggested name/scope.** `proc-probe` (Rust lib: RESULT protocol + memory
readers) + `paired-ab` (runner: A/B/B/A, t-test/sign-test, provenance JSON).

---

## C6. loom/miri/tsan/asan/fuzz runner conventions (the hardening-sweep harness)

**What it is / files.** `scripts/lib.mjs` (REPO_ROOT, `winToWsl`, teeing `run`,
output-scanning `verdict` that fails on TSan markers even when the exit code is
0) plus five runners: `loom.mjs` (per-test **feature-set map** mirroring the CI
matrix), `miri.mjs` (a strict-provenance MATRIX plus a separate
PLAIN-provenance matrix for the by-design exposed-provenance stacks, with
per-test MIRIFLAGS incl. an elevated preemption rate for one aliasing guard),
`tsan.mjs` / `asan.mjs` (the WSL `-Zbuild-std` recipes + the sccache-wrapper
scrub + stress-budget env), `fuzz.mjs` (WSL cargo-fuzz with the
CARGO_TARGET_DIR-leak trap), and `check-all.mjs` (the fail-fast pre-push gate).
Two hard-won guards recur across all of them: **hard-fail on 0 tests selected /
0 tests ran** (the stale-feature-name → silent-green class of bug, tasks #29,
#18, #204) and **verdict-by-output-scan, not exit code**.

**Coupling.** High in the matrices (test names × feature sets are the
project), low in the shell (lib.mjs + the WSL recipes + the guards are
generic).

**Extraction effort.** As a crate: not recommended — the value is 80%
convention, 20% code, and every project's matrix differs. As a template
(`cargo-harden` xtask skeleton or a documented `scripts/` starter kit):
low effort, real value.

**Testability gain — this project (the biggest in-place item in this lane).**
The feature matrices exist in TWO places that must be mirrored by hand:
`scripts/{loom,miri,tsan}.mjs` and `.github/workflows/ci.yml` — the comments
say "MUST mirror the ci.yml matrix" five times, and the loom map has already
gone stale once (the deleted `alloc` feature, task #204). Restructure: one
machine-readable matrix file (`scripts/matrix.json` or TOML) consumed by the
runners AND used to generate/validate the workflow (even a
`tests/no_stale_doc_references.rs`-style consistency test comparing the two
would eliminate the class). **Community:** the WSL traps and the
zero-runs-is-red guard are broadly applicable knowledge for any unsafe-heavy
crate.

**Community value.** Medium — as a *documented template/write-up*
("a five-sanitizer harness for unsafe Rust from a Windows dev host"), not as a
published crate.

**Suggested name/scope.** No crate. A `docs/` write-up + starter template;
in-repo, the single-source matrix.

---

## C7. Confined-unsafe two-tier seam + `#[doc(hidden)]` dbg-accessor pattern (technique, not code)

**What it is / files.** Two paired disciplines documented in `CLAUDE.md` /
README §"Where unsafe lives":

1. **Two-tier confined unsafe** — `#![forbid(unsafe_code)]` above; tier-1
   module-level `#![allow(unsafe_code)]` seams; tier-2 item-level allows with
   per-item `# Safety` docs; and crucially a **self-verifying inventory
   command** (the comment-proof anchored grep
   `^\s*#!?\[allow\(unsafe_code\)\]`) instead of a hardcoded count, so audits
   compare against a command's output and an uncovered `unsafe` token is a
   compile error in every feature configuration.
2. **`#[doc(hidden)]` test-hook forwarders** — 100+ `pub fn dbg_*` accessors,
   deliberately segregated into dedicated `*_diag.rs` / `*_pool.rs` files
   (`src/alloc_core/alloc_core_core_diag.rs`,
   `src/registry/heap_core_diag.rs`, `src/global/tls_heap.rs`,
   `src/registry/bootstrap.rs`, …), exposing otherwise-internal state
   (freelist heads, ring cursors, decommit counters, slot states, fault
   injection like `dbg_arm_commit_fail`) so ~190 integration tests in `tests/`
   can reach internals without `#[cfg(test)] mod tests` in `src/`. The
   accessors themselves are regression-tested (e.g.
   `tests/regression_r2_3_dbg_accessors_membership_guard.rs`) and there is a
   compiled-out guard for stats walks
   (`tests/regression_r3a_stats_walk_compiled_out.rs`).

**Coupling / extraction.** This is a *pattern*, and honestly too bespoke for a
crate. A proc-macro (`#[test_hook]`) would add a dependency to buy almost
nothing over `#[doc(hidden)] pub fn dbg_*` + convention. The one crate-shaped
piece: a tiny CI checker that runs the tier-inventory grep and diffs it against
a checked-in allowlist (a 50-line `xtask`; conceivable as `cargo-unsafe-seams`
but of marginal value over the grep itself).

**Testability gain — this project.** Already largely banked — this pattern is
*why* the tests/-only rule works at all. Two incremental ideas: (a) an
`xtask`/test that asserts every `dbg_*` accessor is referenced by at least one
file under `tests/` (dead-hook detection; hooks that nothing exercises are
silent API surface); (b) consider a `dbg-hooks` feature so `production`
release builds can prove the accessors compile out (extending the
`r3a_stats_walk_compiled_out` precedent) — weigh against the "features change
what you test" cost. **Community:** as a written-up technique
("test-only seams for `#![forbid(unsafe_code)]`-style crates: doc-hidden
forwarders + self-verifying unsafe inventory") this is genuinely useful and
rarely articulated.

**Community value.** Medium as documentation (blog/`docs/` chapter); near-zero
as a library.

**Suggested name/scope.** A pattern write-up (could live in this repo's docs
and be linked from the README); optional 50-line inventory-check xtask.

---

## Ranked shortlist

### Extract as a crate/tool (in order of value ÷ effort)

1. **Publish `malloc-bench-rs`** (`crates/malloc-bench`) — already extracted,
   publish-ready metadata; add the per-thread pin hook so
   `examples/malloc_macro.rs`'s duplicate can be retired. *Effort: days.
   Value: high (pure-Rust mimalloc-bench).*
2. **`globalalloc-model`** — the differential op-stream harness (C1), unifying
   the three in-repo copies as its first consumer. *Effort: 1–2 days.
   Value: high (correctness twin of #1; nothing comparable exists).*
3. **`proc-probe` + `paired-ab`** — the RESULT-protocol process-probe lib +
   the A/B/B/A paired-stats runner (C5). *Effort: 3–5 days. Value: high;
   measures what criterion structurally cannot.*
4. **`iai-judge`** — config-driven iai-callgrind WSL bridge with the marginal
   Ir/op decomposition (C4). *Effort: 2–3 days. Value: medium-high, narrow
   niche.*
5. **`criterion-arms`** (C3) — only if demand appears; substantial overlap
   with `critcmp`. *Value: medium-low.*

### Restructure in-place for this project's testability

1. **Single-source the sanitizer feature matrices** (C6): one
   `scripts/matrix.{json,toml}` consumed by `loom.mjs`/`miri.mjs`/`tsan.mjs`
   and validated against `.github/workflows/ci.yml` (or generating it) —
   eliminates the "MUST mirror" drift class that has already bitten twice.
2. **Unify the differential model** (C1): one shared model/oracle module for
   `tests/alloc_core_differential.rs`, `tests/heap_differential.rs`, and
   `fuzz/fuzz_targets/global_alloc_ops.rs` (this is also step 0 of extraction
   candidate #2).
3. **Kill the hand-mirrored constants** in `scripts/bench-table.mjs` and
   `scripts/iai.mjs`: have `benches/global_alloc.rs` /
   `benches/perf_gate_iai.rs` emit (or encode in names) their op counts and
   group manifests; scripts derive, never mirror.
4. **Retire `examples/malloc_macro.rs`'s duplicate workload** via the
   malloc-bench pin-hook API (closes the documented task-#28 drift risk).
5. **Dead-hook detection for `dbg_*` accessors** (C7): a consistency test that
   every doc-hidden test hook is exercised by at least one `tests/` file;
   shared RSS/commit readers for the three probe examples.

### Document as a technique (no code extraction)

- The **confined-unsafe two-tier seam + self-verifying inventory grep + doc-hidden
  dbg-accessor** pattern (C7), and the **hardening-harness conventions**
  (zero-runs-is-red, verdict-by-output-scan, the WSL sccache/target-dir traps)
  (C6) — a `docs/` chapter / blog write-up, linked from the README.
