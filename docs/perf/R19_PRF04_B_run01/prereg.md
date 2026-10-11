# R19 PRF04 phase 2b — measurement-only prepared infrastructure preregistration

**Status: UNMEASURED. Infrastructure only; no measured verdict or policy change.**
Phase 1 is historical A-only materiality evidence, not A/B promotion evidence.

## Owner decision: exact metric and observation requirements

Metric: **first allocating call, includes TLS bind/claim**: first alloc,
alloc_zeroed or realloc on each native thread. Preceding dealloc is not timed;
its separate pre_first_alloc_deallocs count is sample index 3. The binary itself
emits first_call_metric exactly `first allocating call, includes TLS bind/claim`
and the named pre_first_alloc_deallocs array equal to all sample index-3 values.
The validator requires both; the runner validates original binary JSON and never
synthesizes observer fields. Retention emits both fields as null.

(a) Exactly one sample per worker is hard-asserted. Const-initialized no-Drop
TLS marks the call before forwarding, with static preallocated atomic samples.
The wrapper performs no allocation, formatting, pointer inspection or panic.
(b) Diagnostic sum of first-call counts equals successful claim-counter delta.
(c) Live first-alloc START timestamp must be strictly after the main start
barrier release timestamp. Main crosses the start barrier, records release,
then Release-publishes the gate; workers Acquire-wait before workload. Bootstrap
allocation before closure is never retimed: it produces live_valid=false,
saved as invalidation, and KILL-1 INCONCLUSIVE. No retry or replacement.
(d) Wrapper and shared harness must be byte-identical A/B; hashes checked before
builds and each sample. Clean retention has a direct SeferAlloc global allocator.

Sample arrays are [count,start_ns,duration_ns,pre_first_alloc_deallocs,release_ns].
Timestamps share the process monotonic Instant epoch; no wall clock durations.
Worker sample storage is static, never allocated in the wrapper. Joins publish
completed samples to main. The same shared workload serves both examples via
`examples/r19_prf04_gate2b_shared/harness.rs`. This prefixed folder stays within
`examples/r19_prf04_*` scope; without a `main.rs` it adds no auto-discovered example.

## Intended arms, entry and schedule

A = main fresh-first plus this harness; B = A plus V4. Two explicit worktree
arguments are required by the interface; no fallback to the current
worktree or implicit arm. This delivery reads neither foreign worktree. Future
runner access is restricted to the two explicit user-supplied arm arguments.

Real `#[global_allocator] SeferAlloc::new()`, with main bound and LIVE before
baseline (count_before=1). Each process executes exactly one arm/cell/repetition.
Pure production release build: `production`. Separate diagnostic release build:
`production bench-internals alloc-stats internals`. No diagnostic timings enter
any verdict. Default features, locked dependencies, rustc/target/OS/CPU and
requested/resolved feature metadata must be pinned before measurement. Both
arms' commit and tree SHA plus production and diagnostic binary SHA256 must be
captured before the first sample, with timestamps and per-sample identity
matches. Missing identity is rejection, not a default. Persist only relative arm
labels A/B, never worktree or toolchain private paths. Git writes are prohibited
in this delivery; no measurement commit or tree is manufactured here.

Retention example: `examples/r19_prf04_gate2b.rs mode arm K W`.

- R-wave: K=2/8/32, W=16/128; five independent processes per arm/cell.
  Alternate arm order by repetition. Small payload exactly phase 1: 128 objects
  per size 64/256/1024/2048/4096 B, 958464 B (~1 MiB), plus Large 2 MiB;
  page touches, free all, two barriers retaining every worker before TLS exit.
  Spawn and native join, never scope. All joins precede the next wave.
- R-live: K=64, W=1. Start barrier plus timestamped Release/Acquire gate.
  Every observed first allocating call must start strictly after release.
  Bootstrap early allocation invalidates the axis; retain workers before exit.
- R-churn: 2000 sequential spawn → same alloc/free workload → native join pairs;
  no overlap between native lifetimes.
- Intended latency/cost control: at least 20 paired comparisons per R-live and
  R-churn axis, A/A controls and ABBA order, no retries or replaced failures.
  Observer first allocating call wave samples cover all K*W events per process. Pair ids and
  series/order must be explicit; synthetic schedules are not measured evidence.
- Only permitted smoke parameters: wave A 2 2, live A 4 1, churn A 1 20.

## Oracles and raw output

For every process: conflicts before/after=0, maintenance_running=false; service
never started. High-water count delta excludes the main heap:

- wave A: delta=1+W(K−1); wave B: delta≤K+1;
- wave B diagnostic: free_set+hint≥(W−1)K−1 and bump≤K+1;
- live both: delta=K, free_set=0;
- churn both: delta≤2.

Source ABI is read via `&[u64]`, order `[hint,bump,cap_scan,recovery]` in A4;
B5 appends `free_set` fifth without shifting existing positions. JSON records
actual `claim_sources_len` and `missing_fifth_explicit_zero`, plus diagnostic
`free_set` explicitly equal to the fifth delta or 0 when missing. The validator
requires this equality; missing fifth is never an observed B free_set value.
Production `claim_sources`, `claim_sources_len`, `free_set` and the missing-fifth
flag are null (unavailable). B diagnostic requires `claim_sources_len=5`. Successful
claim sum=K*W; resolved default matches=K*W, mismatches=0, genuine lease-policy
comparison as phase 1. Routed negative-directory scans require alloc-stats and
are recorded only in diagnostic; production null is unavailable, not zero.

Raw JSON includes idle RSS/commit, before snapshots, sampled barrier/idle maxima,
segments_reserved_total delta, count/config fields, wall time including spawn,
work, snapshot barriers, joins and TLS teardown. Peak is sampled, not continuous
OS peak. Missing commit_charge stays unavailable; never replace it with VA/RSS.
Observer first_call_samples capture every worker. Retention samples have null
first_call_samples. Diagnostic wall is null; diagnostic performance is excluded.

## Owner thresholds — verbatim accepted corrections

Source: `docs/reviews/2026-10-11-owner-questions-decisions-ox.md`, Question 3.

- **Поправка 1: первичная ячейка benefit — K=32, W=128** (у A ожидается 3969 heap, ниже предела 4096). При нижней оценке ~50 КиБ на FREE heap (исследование §2.4) ячейка K=8, W=128 даёт 897 heap ≈ 45 МиБ — ниже пола 64 МиБ даже при реальном структурном дефекте. Значит, судить по ней — заранее закладывать NULL. K=8/W=128 и K=2 репортятся как вторичные. Порог BENEFIT прежний: `commit_idle(B) ≤ commit_idle(A) − max(64 MiB, 25 %·commit_idle(A))`, медиана из 5.
- **Поправка 2: p99 первого alloc в R-wave — жёсткий KILL-3** (B ≤ 1,10 × A), а не «вопрос владельцу». Порог, решение по которому откладывается до просмотра чисел, перестаёт быть предрегистрированным. Если реюз дорог из-за `trim_for_recycle` под новым владельцем (`src/registry/heap_registry/claim.rs:317`), это отдельная оптимизация, а не повод пропустить регресс.
- **Поправка 3:** KILL-1 (R-live, |Δp50| ≤ 5 %, paired t) и KILL-2 (R-churn, Δwall ≤ max(2 %, шум A/A)) — как предложено. Если полоса A/A на KILL-1 шире 5 % (ср. perf item 81, шум WSL2), ось INCONCLUSIVE, и GO не выдаётся до прогона на нешумящем хосте.

Fixed operational definitions:

- BENEFIT: arithmetic median of five process idle commit samples at **32/128**,
  B≤A−max(67108864 bytes,0.25*A). Secondary cells excluded from verdict.
  Savings percentage numerator A−B, denominator A.
- KILL-1: live |delta p50|≤5%, numerator p50B−p50A, denominator p50A;
  two-sided paired Student t p<0.05 with positive mean B−A is significant
  slowdown and kills even inside the magnitude limit. ≥20 matched pairs plus
  same-A controls. Exceeding the magnitude limit also kills on a valid axis.
- KILL-2: churn wall delta≤max(2%,AAnoise), numerator median wallB−median wallA,
  denominator median wallA. AAnoise=|median(A2)−median(A1)|/median(A1) over the
  40 same-A pairs (the verdict statistic applied to A against A). The bound widens to
  AAnoise when that is above 2%; noise never makes this axis inconclusive.
  Positive slowdown above the bound kills.
- KILL-3: hard wave32/128 **median of five per-run nearest-rank p99s**, each
  p99 from all K*W first allocating call events; B≤1.10*A. Do not pool processes before
  p99. Percentage numerator p99B−p99A, denominator p99A.
- AA wider than its threshold is INCONCLUSIVE, never widened into GO, on the benefit
  (max64MiB/25% A), live (5%) and p99 (10%) axes. R-churn is the owner's exception:
  its bound is max(2%,AAnoise) per the quoted correction 3.
  Zero/nonfinite denominators, missing pairs, counts, identity, commit or observer
  are INCONCLUSIVE/rejected; missing commit is **not NULL**.

Verdict precedence: valid any KILL → NO-GO; otherwise missing/noise/invalid
observer/invalid oracles → INCONCLUSIVE; otherwise oracles OK and benefit absent
→ NULL; otherwise all kills passed, benefit and oracles OK → GO. Diagnostic
activity validates mechanisms but diagnostic performance never supplies verdict.
There is no measured verdict in this delivery.

## Deterministic complete schedule and checked aggregation

The runner accepts --run outdir --arm-a wt --arm-b wt. It is not launched by
this implementation session. Existing paths reject; all artifacts use exclusive
creation. Sequential --locked release builds, RUSTC_WRAPPER empty, jobs=3,
each supplied worktree's target. Both examples are built under production and
production bench-internals alloc-stats internals. No git writes.

Exactly 492 subprocess slots, in the following fixed order:
1. Five repetitions; K ascending 2/8/32, W ascending 16/128; A,B for odd
   repetitions, B,A for even; retention then observer for each arm: 120 slots.
2. Five AA-wave blocks at 32/128: retention A,A then observer A,A: 20 slots.
3. Live then churn: twenty blocks each, ABBA then AAAA: 320 slots.
   Pair positions (0,1) and (3,2): forty paired comparisons per mode.
4. Diagnostics: six wave cells then live then churn; A then B; retention then
   observer: 32 slots. Diagnostic performance never enters decisions.

AA controls cover commit, per-run wave p99, live p50 and churn wall. Noise is
the verdict statistic applied to same-A pairs: |median(A2)−median(A1)|/median(A1)
(numerator median(A2)−median(A1), denominator median(A1)); live uses the 40
same-A pairs, wave p99 the five per-run p99 pairs.
Commit AA compares each absolute byte difference with max(67108864,.25*A1).
Wave AA >10% and live AA >5% invalidate their axes, never widen GO.
KILL-1 keeps the owner's positive statistically significant paired-t slowdown
interpretation even within 5%. KILL-2's bound is max(2%,AAnoise) (quoted
correction 3). Missing commit is INCONCLUSIVE, not NULL.

Commit SHA, HEAD tree SHA (base, not a manufactured working tree), tracked patch
plus untracked relevant Rust/JS/TOML/prereg sources, patch SHA256, byte source
hashes, four binary hashes per arm, rustc/target/cargo, sanitized resolved feature
metadata and OS/CPU are captured before first sample. The patch records working
tree differences; no git write-tree. Before each sample and after completion,
source identity, binary hashes and prereg hash must still match. Root runner and
harness hashes are also checked per sample, even when root is outside both arms.
Before the first sample the exact prereg bytes are saved as relative `prereg.md`;
aggregation requires a lowercase 64-hex identity.prereg_sha256 and verifies the
snapshot bytes against it, never trusting identity alone. Artifacts contain
only arm labels, relative source names and SHA identities, never private paths.

Raw JSON is saved after each validated process; live early allocation is instead
saved as an invalidation record, not substituted. Oracle/identity/config/count/
maintenance rejection stops the runner. Missing slots cannot aggregate. One pure
aggregateRows function validates exact ordered slot identity/cardinalities,
computes all axes, and derives CSV, Markdown and verdict JSON with arithmetic
assertions. The filesystem adapter checks preserved patch and prereg snapshot
hashes and refuses
summary/report/verdict overwrite. Secondary cells and diagnostic timings excluded.

Self-tests use complete synthetic 492-slot fixtures without repository artifacts;
known median, nearest-rank, paired-t and SHA answers; count/config/maintenance/
source-length/free_set rejection; observer a/b/c controls, exact metric-label and
named/index-3 counter mismatch rejection, retention null requirements; prereg
snapshot tampering and malformed/missing hash rejection; all four AA noise
axes; source/wrapper/binary/identity drift; missing primary commit; each kill;
verdict precedence and complete aggregate output. No new Rust test targets.

Validation status is recorded by the implementation response, not fabricated here.
No full gate, full tests, npm benchmarks or powerset runs. No allocator source,
lock/version/protected docs or git changes. Only example Cargo blocks are changed.

## Amendment A1 (orchestrator, before any measurement)

Review of the delivered runner found three deviations or gaps, fixed before any
sample was taken; no owner threshold changed.

1. KILL-2 used a fail-closed 2% bound; the owner's correction 3 says max(2%,AAnoise). Restored.
2. AA noise was the maximum single-pair deviation, far stricter than the statistic it
   guards (a median of 40). It is now the same statistic applied to A against A.
3. The runner now requires clean arm worktrees (identity is the commit), A an ancestor of B,
   identical rustc/cargo/target and resolved features across arms; a fresh worktree no longer needs a pre-existing target.
   Tables additionally carry bound, pairs, mean delta, t and p for the live/churn axes.
