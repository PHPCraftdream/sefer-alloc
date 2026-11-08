# J13b runtime learning — Task49 delivery manifest

Status: runtime source and activating tests authored; exact-source integration CI acceptance is pending. Class: `perf(opt-in)` (Auto behavior changes). No performance measurements or performance GO exist.

## Accepted prerequisites, not current integration receipts

- Task48 control sampling: `aacf1d097d9d9ab04f1b382c780602c713fbd8d0`, CI `42490064262` SUCCESS after source/log review and child cleanup.
- Task60 standalone packed cell: `91c875f36900ae5108f251e1ff0b81990736a02e`, CI `42500341404` SUCCESS; four native cell tests plus one forced-CAS rejection test, one actual-source Loom test, positive aarch64 executions, 213 semantic cases / 314 mutants / zero failures, twelve-commit trailer range with zero red. See `J13B_LEARNING_CELL_MANIFEST.md`.
- Those immutable receipts do not cover the new runtime composition. The local caller preserves its checkpoint separately; publication excludes that checkpoint rather than rewriting history.

## Shipping boundary in this candidate

`ThpRuntime` owns six process-wide estimator cells, classified by requested size, not usable span, business task or thread identity. Both Q31 components begin at one, as in the accepted pure estimator. The existing immutable process tuple supplies the gradient; no new setting, callback or publication rule is added.

The fresh eligible Auto path first chooses its existing no-THP controls, even at share zero or when advice budget is exhausted. A selected control never receives positive THP advice. Non-controls read one coherent estimator pair and apply the resolved gradient's bounded share through the existing prefix geometry and outstanding-charge ledger. This changes advice amount, never allocation size or mapping geometry.

Only the existing successful owner-terminal control sample feeds learning. A pure conversion rejects zero sampled pages or resident pages above sampled pages, widens the Q31 multiply to u128, and bounds the quotient by SCALE. Accumulated telemetry is never an estimator input. Failed premises, sampling errors, remote UNKNOWN, cache second lives and invalidated old-realloc tokens do not train. A genuinely fresh moving destination can independently obtain its own clean control; this does not revive the old token.

The accepted asymmetric EMA/anchor/floor/curve/gain math is unchanged. Pure math comments now point to the separate runtime adapter; no arithmetic was rewritten. Pure `ThpAdvice::share_for` remains a state-free mode baseline (Auto zero); production fresh Auto explicitly reads `ThpRuntime::learned_share` instead. Native activation tests must distinguish that connection from leaving the old zero seam in place.

## Bounded admission and diagnostics

The existing cell makes at most eight source-level strong-CAS calls per observation. Success commits both components together; exhaustion applies no update and is counted as dropped. This is not a wait-free or wall-clock bound: an atomic implementation can retry internally, and neither fairness nor unbiased admission is guaranteed. Group cells are logically separate, not promised cache-line-isolated. Control cohorts are global across eligible request sizes, not per group: a size group has no minimum observation cadence and can retain its optimistic prior under correlated size/ordinal patterns.

The previous eight control telemetry fields are preserved. New bench-internals/internals-only readback `dbg_thp_learning_telemetry()` reports rows `[e, m, target_share, applied, dropped]`. The first three derive from one pair and one captured gradient; applied/dropped are independent wrapping diagnostic loads. They neither drive learning nor form a coherent whole-array snapshot. Correct closure requires resolved configuration, quiescence and a supported non-wrapping observation interval; the real judge must not infer accepted advice from target share alone.

New applied/dropped writes reuse the existing diagnostic counter helper. Two new load sites have the explicit P27 diagnostic scope/rule and one positive plus two rejecting fixture cases. The generic counter scope excludes exactly those two named statics to avoid duplicate ownership. No existing P26 rule expectation or counter inventory is weakened.

## Observable acceptance oracles

- `tests/j13b_learning_runtime.rs`: actual fresh owner controls drive a resolved custom gradient through sparse decline, zero target share with unchanged accepted-advice bytes, dense recovery while controls remain active, and renewed real advice. Every group transition is compared with the accepted pure estimator using the actual single-control resident/sample delta; other groups remain independent. A dedicated matrix row activates the Linux/Android body.
- `tests/j13b_thp_auto_seam.rs`: the former zero-seam oracle now checks real runtime admission. Two complete cohorts are held concurrently; the second begins with the entire advice budget already charged. It must still select and learn its clean control. Fourteen non-controls never train, charge never exceeds the budget, and checked terminal release returns it. Non-Linux/Android backends still advise/learn nothing.
- Existing installed-GlobalAlloc and concurrent-ordinal oracles now check learning activation and quiescent `applied + dropped == valid` closure. The actual captured-ticket proof remains intact; neither counter deltas nor pure replay replace it.
- Existing sampler-recovery, premise-recovery, remote-UNKNOWN, cache-reuse and OOM-shrink tests check exact estimator exclusions and exactly-once learning. The shared VM advice FI flag now targets the next actually selected ordinal in a single-threaded fresh cohort: otherwise a preceding non-control positive hint could consume the fault. Failure/recovery expectations and lifetime/layout contracts are not relaxed.
- Actual-source cell tests additionally bind residency conversion to zero/invalid/u64-boundary counts. The actual-source Loom model and deterministic forced-CAS rejection remain required, but neither is a whole-runtime composition proof. Native concurrent closure does not force runtime exhaustion; that limitation stays explicit in correctness item272.

## Preserved scope and remaining cost questions

Default Off, Always/Off behavior, requested minimum, advice-byte budget, checked release, header size/terminal pins, production feature composition, dependencies and versions are unchanged. The learning step adds no allocation header field. No small allocation/free path, foreign metadata mutation, scheduler hook, background service or pinning is added. New unsafe operations are confined to tests that touch live owned payload or free with the exact layout; production unsafe seams are unchanged.

The controls remain bounded residency observations, not useful-byte/RSS/physical-THP measurements. Remote/lifetime/window-correlation and admission-loss biases remain in correctness item272. Current-layout evidence item271 stays CONDITIONAL. Real runtime judge admission (#54), matched cost/benefit campaign (#50) and any default decision are outstanding. No default-path codegen, Ir, throughput or RSS result is inferred from correctness CI or a zero-cost scope SKIP.

## Verification and publication protocol

Allowed local checks only: fmt/rustfmt, cargo check/clippy, cargo-free structure/inventory/hook/evidence verifiers. Windows production/internals/bench/FI check and all-target Clippy passed, as did plain-production lib Clippy, all-features all-target Clippy and the cfg(loom) target type check. The targeted minimal-feature check passed with thirteen warnings about unchanged B4 imports/counters; it is not claimed warning-free. Linux/Android-only bodies were excluded on Windows, not executed or proven. The conversion/cell test bodies are platform-independent and were type-checked, not run. No local test, model, build, benchmark, workload or ghost-lint binary is run.

Cargo-free checks passed: ordering inventory 730 sites / 96 files / 27 protocols / 53 scopes, zero gaps/double claims/dead scopes/protocol errors; unchanged structure-debt ratchet; hook-safety scanner; 29-target Loom wiring. Invariant metadata has five blocks with zero errors/warnings; full protected-span/fixture execution remains CI-only. The strict evidence verifier returned exit0 INCOMPLETE for existing unfinished legacy rows, not full evidence PASS.

Before publication: personally review all source, tests, fixture mutations and staged changes; validate invariant metadata; preserve historical evidence requirements. The root target census is derived from the filesystem (533 test targets; 29 Loom-named models). New exact-source CI must positively execute the activating native/model/fixture/trailer targets and all selected PR checks before Task49 closes. Failed attempts remain failures and are fixed in the same workstream.

No source SHA or CI result is invented before publication. After committing, derive the published commit list with `git log --format='%H %s' 91c875f3..<published-head>` on the publication branch; bind every execution receipt to that exact head. No performance raw logs were produced.
