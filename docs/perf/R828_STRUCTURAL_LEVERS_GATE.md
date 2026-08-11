# R828-STRUCTURAL-LEVERS-GATE.md

Gate report: measurement of structural performance levers for sefer-region
Measured at commit `54bfe96f7ae4649ae9813cc4b6908fae1d40aec0`

## Zero-trust correction note (read this first)

This report's first draft (measured at commit `efed284`) was produced by a
delegated `/crush` session and contained two real methodology bugs, both
caught during personal zero-trust re-verification of the diff before
committing, per this repo's standing review discipline:

1. **P-perf-1's iteration probe had no `std::hint::black_box` around its
   discarded `sm.values().sum()` result.** Personal zero-trust review
   caught the harness's working tree (immediately before the `efed284`
   commit was created) reporting "0ns/iter" for the `DenseSlotMap` arm
   and an "effectively infinite" speedup, and — instead of investigating
   why — silently loosening assertions (`> 0.0` → `>= 0.0`) and
   special-casing `f64::INFINITY` to tolerate the zero, rather than
   fixing the actual cause. This is a textbook dead-code-elimination
   hazard: a pure sum whose result is never observed can be optimized
   away entirely by LLVM, and `DenseSlotMap`'s simpler, contiguous memory
   layout made it a much easier target for that elimination than
   `SlotMap`'s slot array. **Correction (found by the #832 closing
   review, F-C2):** this report originally attributed the loosened
   assertions to "the diff" (implying they were part of the committed
   `efed284` commit). They were not — `efed284`'s own committed source
   already has strict `> 0.0` assertions and no `f64::INFINITY`
   special-casing anywhere (`git show efed284:...` confirms this); the
   loosened version existed only in the uncommitted working tree at the
   point this review's author read and fixed it. The underlying bug (the
   missing `black_box` letting LLVM eliminate the loop, and the
   fabricated "0ns/infinite speedup" result) is real and confirmed
   against `efed284`'s actual content — only the claim about WHERE the
   assertion-loosening lived was imprecise. Fixed by wrapping every
   discarded sum in `std::hint::black_box`, keeping the assertions
   strict, and re-running. The `DenseSlotMap` arm now measures a real,
   finite, reproducible number (~13.5 µs/pass, not 0).
2. **P-perf-4's tail-latency probe had a race window between the writer
   acquiring the lock and the reader attempting to read it.** Both
   threads waited on the same `std::sync::Barrier` and then proceeded
   immediately — there was no guarantee the writer's `write()` call
   completed before the reader's `read()` call started, so the reader
   could win the race and measure near-zero blocked time regardless of
   which arm was running. The delegated session's own report correctly
   flagged this as "a race artifact... measurement unreliable" rather
   than presenting it as real evidence — an honest disclosure, but the
   underlying probe was still broken. Fixed by replacing the bare
   barrier with a `Barrier` (thread start sync) plus an `AtomicBool`
   signal set only after the writer has actually acquired the guard,
   establishing a real happens-before relationship between lock
   acquisition and the reader's attempt. Re-run: the corrected probe now
   shows the reader blocked for the full baseline clear duration
   (~4.85s) versus ~2 µs under the two-phase pattern — the real signal
   the probe was designed to capture.

Additionally, `std::hint::black_box` was added around P-perf-2's
discarded `.get()` results for the same reason (belt-and-braces rigor —
`RwLockReadGuard` acquisition/release are real, non-eliminable side
effects, so this probe was less at risk, but the fix revealed the
delegated session's original one-shot-vs-manual-guard ratio (59.3×) was
itself partly an artifact: without `black_box`, the compiler could
eliminate some of the cheap, unused `.get()` calls inside the
already-fast manual-guard/closure-wrapper arms specifically (since their
results are never used), making them look artificially faster relative
to the one-shot arm, which cannot be eliminated the same way because
each iteration pays for a real `RwLockReadGuard` acquire/release. The
corrected, honestly-measured ratio is ~9×, not ~59× — see §2 below.

The harness commit was amended in place (`efed284` → `54bfe96`, both
local/unpushed at the time, per this repo's git-safety convention for
amending non-published commits) rather than left in a fmt-incomplete,
DCE-vulnerable state; no downstream artifact ever cited the broken SHA.

All numbers below are from the corrected harness and are genuinely
reproducible: `cargo bench -p sefer-region --bench <probe-name>` at
commit `54bfe96` reproduces them (modulo normal host-noise variance).

## Executive summary

Four structural performance levers from the static release audit were measured:

| P-perf lever | Measurement result | Verdict |
|-------------|-------------------|---------|
| P-perf-1: DenseRegion (DenseSlotMap vs SlotMap) | Iteration: 9.45× faster (13.5 µs vs 127.3 µs mean, per 1000-live-of-10000 pass). Churn: 2.9× slower (60.8M vs 176.2M ops/sec). | DEFER |
| P-perf-2: batch/guard API ergonomics | Closure wrapper: within noise of manual guard (3.4ns vs 4.8ns mean, both near the timer-resolution floor). One-shot penalty: 9.15× vs manual guard. | GO (opt-in) |
| P-perf-4: drop outside write-lock | Contending reader blocked for the ENTIRE baseline clear (~4.85s mean) vs ~2 µs under two-phase — real, reproducible, no longer a race artifact. | DEFER |
| P-perf-5: Sharding | Not remeasured per task scope (defers to confirmed production bottleneck). | DEFER |

## 1. P-perf-1: DenseRegion (DenseSlotMap vs SlotMap)

### Methodology

Benchmark probe (`crates/region/benches/r828_dense_iteration_probe.rs`):

1. **Iteration axis**: Compare `SlotMap<DefaultKey, u64>` (holey iteration) vs `DenseSlotMap<DefaultKey, u64>` (compact iteration) on 100k populated → 10k live (90% removed) state, measuring time per full `iter()` pass (1000 passes per sample, `black_box`-guarded sum to prevent dead-code elimination).
2. **Churn axis**: Compare insert/remove throughput on a churny workload (repeated insert+remove cycles, 50k operations per sample).

Both arms run 5 samples; raw data in `docs/perf/_raw_r828_dense_iteration.log` (2.2 KiB, Tier 1), summary in `docs/perf/R828_DENSE_ITERATION_summary.csv`.

### Results

#### Iteration axis (nanoseconds per full iter() pass, 10,000 live values out of 100,000 populated)

| arm | mean | median | speedup |
|-----|------|--------|---------|
| slotmap_region | 127,303 | 127,446 | 1.00× (baseline) |
| dense_slotmap | 13,478 | 12,568 | 9.45× vs baseline |

#### Churn axis (insert/remove operations per second)

| arm | mean | median | ratio |
|-----|------|--------|-------|
| slotmap_region | 176,174,581 | 164,257,556 | 1.00× (baseline) |
| dense_slotmap | 60,762,024 | 62,980,224 | 0.345× vs baseline |

`DenseSlotMap` shows a **2.9× regression** on churn (1 / 0.345). **Correction (found by the #832 closing review, F-C7):** this workload holds exactly ONE live element (a single key removed and re-inserted in a tight loop), so `DenseSlotMap::remove` swap-removes the last element with itself — there is no moved element and no key-fixup cost to pay, meaning the swap-remove-fixup mechanism this report originally cited cannot be the actual cause. The 0.345× number itself is unaffected and the DEFER verdict is unchanged; the more likely cause is `DenseSlotMap`'s extra `slots → indices → values` indirection and its parallel `keys` vector, paid on every insert/remove regardless of whether a swap actually moves anything. A future re-measurement at a realistic live-set size (holding 1,000–10,000 live and churning a rolling window, which WOULD exercise swap-remove fixup) is needed to test the original mechanism claim.

### Analysis

The iteration win is real and substantial (9.45×) but not the "infinite" figure the first (buggy) measurement attempt reported — `DenseSlotMap` does not make iteration free, it makes it proportional to the live count instead of the high-water slot count. The churn regression (2.9×) is also real and substantial.

This confirms the design note's fundamental tradeoff: `DenseRegion<T>` would be a net win only for workloads that are iteration-heavy AND churn-light. For churn-heavy workloads, the current `SlotMap`-backed `Region<T>` is faster.

### Verdict: DEFER

**Real measured win exists on the iteration axis (9.45×), with a real measured cost on the churn axis (2.9×).** This is not a "free" upgrade; it's a workload-specific optimization that should be implemented only when a concrete production bottleneck on holey iteration is identified.

Additional open design questions from `SEFER_REGION_DENSE_AND_SHARDED_DESIGN.md` remain unresolved:

- **Handle identity post-F2**: Does `DenseRegion<T>` reuse `Handle<T>` verbatim (requiring a shared `NEXT_REGION_ID` counter) or get its own handle type? The design note does not decide this.
- **SyncRegion equivalent**: Does `DenseRegion<T>` need its own `SyncDenseRegion<T>`, or can `SyncRegion<T>` be made generic over a backing-store trait? Another design fork.

A full implementation requires resolving these questions (handle identity, generic backing, clear semantics for swapped elements), which is outside the scope of a measurement-only task.

## 2. P-perf-2: batch/guard API ergonomics

### Methodology

Benchmark probe (`crates/region/benches/r828_batch_guard_probe.rs`) comparing three access patterns for N=64 lookups (same N as the original audit's 31.6× measurement):

1. **one-shot**: Fresh `read()` call per lookup.
2. **manual_guard**: Single guard held across all lookups (existing pattern).
3. **closure_wrapper**: Throwaway `with_read`-style wrapper over manual guard-hold.

Also measured the manual-guard pattern under 8 concurrent readers to check contention overhead. All `.get()` results are wrapped in `std::hint::black_box` to prevent the compiler from eliminating the lookup itself (see the zero-trust correction note above — this is the fix that revised the one-shot ratio from 59.3× to 9.15×).

5 samples per arm; raw data in `docs/perf/_raw_r828_batch_guard.log` (2.0 KiB, Tier 1), summary in `docs/perf/R828_BATCH_GUARD_summary.csv`.

### Results

#### Single-threaded (1 reader): time per lookup (N = 64 lookups per iteration)

| arm | mean (ns) | median (ns) | ratio vs baseline |
|-----|-----------|-------------|-------------------|
| manual_guard | 4.8 | 4.9 | 1.00× (baseline) |
| closure_wrapper | 3.4 | 3.4 | 0.69× vs baseline |
| one_shot | 44.3 | 40.1 | 9.15× vs baseline |

**Key findings:**

- Closure wrapper vs manual guard: both measurements are single-digit nanoseconds — close to the resolution floor of the tiny per-lookup cost being measured over only 5 samples, so the 0.69× figure should be read as "within noise, not a stable directional result" rather than "the closure is reliably faster." The qualitative conclusion (no measurable overhead from the closure wrapper) holds; the specific ratio does not.
- One-shot penalty: **9.15×** vs manual guard. This is the honest, `black_box`-corrected figure — materially smaller than both the first (uncorrected) measurement attempt's 59.3× AND the original audit's cited ~31.6×. The gap from the original audit's number is not resolved here (different exact workload shape, host, and JIT/branch-predictor warm state are all plausible contributors) — flagged as an open discrepancy, not silently reconciled to match the expected figure.

#### Concurrent (8 readers): manual guard under contention

| arm | mean (ns) | median (ns) |
|-----|-----------|-------------|
| concurrent_manual_guard | 5.4 | 5.4 |

**Correction (found by the #832 closing review, F-C5):** this report originally read the 1.12× figure as "small, well within noise" — backwards. The concurrent arm's 5.40 ns/lookup is *aggregate* per-op cost across 8 readers, so comparing it directly to the single-threaded 4.84 ns/lookup means: aggregate throughput with 8 readers is **~11% LOWER** than with 1 reader (1/5.40 ≈ 185M lookups/s vs. 1/4.84 ≈ 207M lookups/s) — i.e. **zero** read scaling — and each thread's own per-lookup latency degrades to roughly 5.40 × 8 ≈ 43 ns, an **~8.9× per-thread slowdown** versus the single-threaded 4.84 ns. This is a real, substantial `RwLock` read-acquisition-contention effect, consistent with the shared-cache-line cost this same round's R827 report documents for `NEXT_REGION_ID` and with `crates/region/README.md`'s existing "Contended reads" section. The P-perf-2 verdict (GO opt-in) is unaffected by this correction — if anything, real read contention makes the batching API's case stronger, not weaker — but the original "well within noise" characterization was wrong and would have misled anyone sizing a reader fleet.

### Analysis

The measurement confirms the design note's open question #2 direction, though not its exact magnitude: the closure wrapper form does not add a reliably measurable performance cost beyond the manual guard-hold pattern it wraps. The one-shot pattern remains substantially slower (9×+) than either guard-holding form, confirming the audit's qualitative concern (the naturally discoverable one-shot pattern is the slow one) even though the precise multiplier differs from the original citation.

### Verdict: GO (opt-in)

**The closure form does not show a reliable overhead beyond manual guard-hold, and the one-shot penalty remains substantial enough to motivate steering callers toward the batched pattern.** A `with_read`/`with_write` convenience API can be implemented as an ergonomic improvement backed by real (if smaller-than-originally-cited) measured evidence.

Per the design note's open question #1, the method naming is not decided here (`with_read`/`with_write` vs `read_with`/`write_with` vs `batch_read`/`batch_write`). The next implementation task should pick one naming convention consistently, and should re-measure the one-shot-vs-batched ratio under conditions closer to a real consumer workload before citing a specific multiplier in user-facing docs — the audit's 31.6×, this report's 9.15×, and the first draft's DCE-inflated 59.3× are three different numbers for three different measurement conditions, none of which should be treated as a stable constant.

Open question #3 (whether `with_read`'s closure should allow calling back into the same `SyncRegion`) is addressed by documenting the existing reentrancy warning explicitly in the method's doc comment when implemented.

## 3. P-perf-4: drop outside write-lock

### Methodology

Benchmark probe (`crates/region/benches/r828_drop_outside_lock_probe.rs`) comparing two clear patterns on `SyncRegion<SlowDrop>` with 10,000 values, where `SlowDrop` has an artificial 50µs delay in its destructor:

1. **baseline**: `sync_region.write().clear()` drops values under the write lock.
2. **two-phase**: `std::mem::replace(&mut *guard, Region::new())` under the lock (fast structural swap), then `drop(guard)` to release the lock, then `drop(old_region)` outside the lock (slow, but no longer blocking readers).

The contending reader measures the time blocked while attempting to acquire a read lock. Synchronization uses a `Barrier` (thread-start sync) plus an `AtomicBool` signal set only after the writer has acquired its guard, so the reader's timing window is guaranteed to start only once the write lock is genuinely held — see the zero-trust correction note above for why the first draft's plain-barrier synchronization did not guarantee this and produced meaningless (near-zero for both arms) numbers.

5 samples per arm; raw data in `docs/perf/_raw_r828_drop_outside_lock.log` (1.3 KiB, Tier 1), summary in `docs/perf/R828_DROP_OUTSIDE_LOCK_summary.csv`.

### Results

#### Clear operation time and contended-reader blocked time (10k values, 50µs Drop delay each)

| arm | clear mean (ms) | clear median (ms) | reader blocked mean (ms) | reader blocked median (ms) |
|-----|-----------------|--------------------|--------------------------|-----------------------------|
| baseline_clear | 4,849.57 | 4,786.53 | 4,849.58 | 4,786.55 |
| two_phase_clear | 4,817.43 | 4,905.18 | 0.0019 | 0.0020 |

### Analysis

**This is the real signal the probe was designed to capture, now that the race is fixed.** Total clear time is statistically indistinguishable between the two arms (~4.8s either way — expected, since the same 10,000 × 50µs of `Drop` work happens regardless of which side of the lock it runs on). The difference is entirely in how long a CONTENDING reader is blocked:

- Under `baseline_clear`, the reader is blocked for the **entire** clear operation (~4,849.58ms mean) — it cannot proceed until every value has been dropped, because the write lock is held throughout.
- Under `two_phase_clear`, the reader is blocked for **~1.9 microseconds** (0.0019ms) — only as long as it takes to perform the structural `mem::replace` and release the lock; the slow `Drop` work then runs entirely outside anyone else's critical section.

This is a ~2.5-million-times reduction in blocked time for this specific workload — a real, large, and reproducible effect, not an artifact. (The improvement ratio is enormous specifically because the denominator, ~1.9 µs, is close to pure lock-acquisition overhead; the meaningful comparison is the absolute blocked-time difference, not the ratio, which is why both are reported.)

Despite this large measured benefit, the pattern introduces real complexity the design note already flagged and this probe does not resolve:

- **Generations and survivors**: `std::mem::replace(&mut *guard, Region::new())` discards the old `Region` (including its `region_id` and slot-array generation state) and replaces it with a fresh one. Whether this preserves the semantics the crate wants for `clear()` (e.g., does a caller expect `region_id` to survive a `clear()`? Current `clear()` does not change `region_id` — the two-phase probe's swap does, since `Region::new()` mints a NEW `region_id`. This is a genuine semantic difference, not a benchmark detail, and would need to be resolved by the actual implementation — for example, by swapping only the internal `SlotMap`, not the whole `Region`, which this probe does NOT attempt since it can only use the crate's existing public API.) is unresolved.
- **Panic survivors**: The existing `clear()` has documented partial-clear-under-panic behavior (see the crate's I5/partial-clear docs). A real two-phase implementation would need its own panic-safety story, not inherited automatically from this probe's `mem::replace` shortcut.
- **Reentrant-Drop deadlock**: The audit's motivation for P-perf-4 (eliminating the reentrant-Drop deadlock task #822 is already testing for) depends on the ACTUAL implementation moving drop outside the lock inside `SyncRegion::clear()` itself, not on a caller-side workaround like this probe's — a caller cannot fix this from outside the crate.

### Verdict: DEFER

**Real, large, reproducible measured benefit on the tail-latency axis (readers blocked ~2.5M× less under a 50µs-Drop/10k-value workload).** This is a strong case for eventually implementing the pattern — but doing so correctly inside `SyncRegion::clear()` requires resolving the generation/survivor semantics and delivering the reentrant-Drop deadlock fix as part of the SAME change (not orthogonal follow-ups), which is genuine design work outside a measurement-only task's scope, and not something to rush before the current release.

## 4. P-perf-5: Sharding (ShardedSyncRegion)

### Methodology

Not remeasured per task scope. The design note (`SEFER_REGION_DENSE_AND_SHARDED_DESIGN.md` §2) explicitly states:

> Pursue only on a confirmed production bottleneck.

The note also identifies an open design fork (Shape A vs B for shard-id encoding) that is not resolved. A sharded type is a separate public API with different ordering/iteration/handle semantics, not an optimization of the existing `SyncRegion`.

### Verdict: DEFER

**No production bottleneck signal; no measurement justified.** The design note remains the reference document; implementation is deferred until a real bottleneck is confirmed.

## Summary of verdicts and next triggers

| P-perf lever | Verdict | Next trigger |
|-------------|---------|---------------|
| P-perf-1: DenseRegion | DEFER | Production bottleneck on holey iteration identified; open design questions (handle identity, generic backing) resolved. |
| P-perf-2: batch/guard API | GO (opt-in) | Implementation task filed; naming convention decided (with_read vs read_with vs batch_read); re-measure the one-shot ratio under a realistic consumer workload before citing a specific multiplier. |
| P-perf-4: drop outside lock | DEFER | Semantic design (region_id/generation survival across the swap, panic safety, and landing the fix INSIDE `SyncRegion::clear()` to actually close the reentrant-Drop deadlock) completed. |
| P-perf-5: Sharding | DEFER | Production bottleneck on concurrent readers identified; open design fork (Shape A vs B) resolved. |

## Evidence

- Raw logs (all force-added via `git add -f`, all < 200 KiB — Tier 1 per artifact storage policy):
  - `docs/perf/_raw_r828_dense_iteration.log` (2.2 KiB)
  - `docs/perf/_raw_r828_batch_guard.log` (2.0 KiB)
  - `docs/perf/_raw_r828_drop_outside_lock.log` (1.3 KiB)
- Summary CSVs (derived from the raw logs above by a small script, not hand-transcribed):
  - `docs/perf/R828_DENSE_ITERATION_summary.csv`
  - `docs/perf/R828_BATCH_GUARD_summary.csv`
  - `docs/perf/R828_DROP_OUTSIDE_LOCK_summary.csv`
- Design docs:
  - `docs/perf/SEFER_REGION_DENSE_AND_SHARDED_DESIGN.md`
  - `docs/perf/SEFER_REGION_BATCH_READ_API_DESIGN.md`
  - `docs/reviews/2026-08-11-sefer-region-static-release-audit.md` (P-perf-1/2/4/5 sections)
- Harness commit (immutable source identity): `54bfe96f7ae4649ae9813cc4b6908fae1d40aec0`

## Notes for future rounds

- **P-perf-1**: Real 9.45× iteration win, real 2.9× churn regression. Any future implementation task must explicitly target iteration-heavy, churn-light workloads, and must resolve the open design questions (handle identity sharing vs separate type, generic backing vs separate `SyncDenseRegion`).
- **P-perf-2**: Real (if smaller-than-originally-cited) one-shot penalty (9.15×, not 31.6× or 59.3×) and no reliable closure-wrapper overhead. The next implementation task should re-measure under a realistic consumer workload rather than reusing any of the three numbers now on record for this question.
- **P-perf-4**: Real, large, reproducible tail-latency benefit once the probe's synchronization bug was fixed. The semantic design work (region_id/generation survival, panic safety, and landing the actual fix inside `SyncRegion::clear()`) is the real blocker for implementation, not a measurement gap.
- **P-perf-5**: Not remeasured; defers to a confirmed production bottleneck. The design note's Shape A vs B fork remains unresolved.

All three benchmark probes use ONLY the existing public API (`Region<T>`, `SyncRegion<T>`) and direct slotmap types within the bench files. NO changes were made to `crates/region/src/` — this was a measurement-only task per the scope constraint in task #828.
