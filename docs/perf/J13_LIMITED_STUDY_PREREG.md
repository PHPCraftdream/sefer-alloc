# J13 limited Always/Off study — preregistration

**Registered:** 2026-11-10. **Status:** owner-authorized manual CI study; not yet measured. The dated amendment below supersedes the original operational prerequisites only.

## Dated premeasurement owner amendment — 2026-11-11

The owner approved measurement-first delivery on the green `25fcdfe08f44ae5e287e4abd2768cd8931b1dbf0` baseline (CI42919482576), with these narrow compatibility and smoke amendments approved before data. The owner answer and subsequent approval are recorded in the session; neither is measurement evidence.

- No new harness/controller/runner/benchmark implementation. Remove only the reducer's obsolete swap/pressure admission predicate: 470 helper + 48 judge + 21 parser + 101 runner + 259 reducer = **899/900**. Preserve all other host, identity, mechanism and statistic checks. The historical 900/900 tally below precedes this deletion.
- The former 1024-allocation/1024-free64-byte Small-Ir prerequisite is superseded: Small-path cost verification moves to #107 using existing IAI/zero-cost CI gates on the changed default binary. No new Small benchmark or unperformed zero-delta result is claimed. Correctness item264 remains OPEN.
- Actual-host preflight requires Linux x86_64, base pages4096 bytes, THP2097152 bytes, top-level THP `madvise`, and MemAvailable >=8 GiB. Missing facts fail admission. No host tuning. Record raw facts, not an earlier build-host admission. Host MemAvailable is the declared memory check, not a guarantee of cgroup headroom.
- Replace no-active-swap/pressure admission with registered per-cell A/A readability. An A/A interval containing 1 does not prove absence of host/quota bias. No swap/PSI/cgroup observer or continuous/shared controller is required.
- Keep one child at a time, <=2 actors, <=2 GiB live payload, existing<=120s direct-child timeout and a 30-minute CI measurement-step timeout including preflight/runner/report overhead. Build/component checks are outside it. Separate runner invocations are accepted. The prior 256 MiB hard cumulative-evidence bound is superseded, not enforced by child buffers; record artifact sizes and apply repository raw-storage policy.
- Smoke is cycle8 MiB and cache16 MiB, one ABBA plus one A/A block each (16 children), numbers discarded in advance. Strict reducer sample-count refusal is expected; no fabricated smoke CSV/CI. Full-shape synthetic fixtures cover rendering; registered data exercise actual measured generation.
- Before registered dispatch assess full 960-child feasibility within **20 minutes** from smoke timings, naming assumptions and unmeasured large-cell uncertainty. If it does not fit, stop and ask before dispatch. An unsupported estimate is not a pass. Hard step timeout remains30 minutes.
- If interrupted before all cells finish, the owner permits **one full repeat**. Retain both attempts and partial raw separately; never splice cells, selectively replace observations or run a third campaign. A campaign completing all measurement cells is final even if later report/upload fails: recover existing evidence, do not repeat payload.
- Cells, counts, touch kernels, configuration/mechanism evidence, estimator and raw-first reporting remain unchanged. Accepted memory cost up to2 MiB per touched2 MiB region is reported, not a veto; materially new regressions go to the owner. No new numeric product gate.
- Manual `workflow_dispatch` only; no automatic PR/push/schedule campaign. PR229 delivers actual raw/generated CSV/Markdown first, then the owner decides the default before #107. Auto/budget later; full builder/scalar paused. No local native execution, checkpoint publication or unknown-lock removal is authorized.

## Purpose and approved sequence

The current main baseline is `bad95849`: PRs #226 (VM foundation), #225 (runtime), #227 (landing-history repair), and #228 (closure) are present. Auto learning is accepted on exact `6c2ba419` (CI `42512485612`); the production default remains `Off`. Current `Always` is range advice, not historical J13's whole-mapping implementation. The full J13b child identity/backend remains unavailable (`identity_backend_missing`); `raw.v4` remains disabled and the archived builder remains untouched.

The approved next delivery is a limited comparison of **current range `Always` against `Off`**, through the existing `examples/g1_big_judge.rs` with environment configuration. No new entrypoint, workflow or general builder. **Dated premeasurement owner decision (2026-11-10): `До900строк` — approve a cap of 900 combined executable-source lines, including both the harness and reducer. This supersedes the earlier approximately 500-line target.** The initial tally was 628 harness + 230 reducer = 858. After the runner fixes and actual producer-binding integration, the parent independently counted 470 helper + 48 judge + 21 parser + 101 runner + 260 reducer = **900/900**, with no remaining margin. The harness count includes added nonblank/noncomment source lines; the reducer count conservatively includes every physical line. This accounting is not minified and excludes no implementation logic; YAML, Cargo declarations, docs, and tests are outside the source count, not places to hide executable logic. This is a source-size decision only: it changes no workload statistics or product gate and grants no build, CI-dispatch, workload, or measurement authorization. The first implementation PR combines the initial figures with its docs, harness, and focused checks; Auto and budget work are later campaigns. The existing harness is not yet claimed to implement this contract.

Historical J13 campaign-2 formal RSS NO-GO and the separate owner exception remain distinct: the historical owner decision is git object `7ae557aa:docs/perf/J13_OWNER_DECISION.md` (file absent from HEAD). The old few-KiB RSS misses are **not** an automatic veto for current range `Always`; investigate materially new problems. Timing noise that leaves a speed claim unsupported does not reverse the approved direction. No new numeric product regression definition is approved: report findings for owner review before a default decision. Any new numeric product gate requires a dated amendment before measurement. `Off` remains the default; there is no J13b GO.

Fault counts and `AnonHugePages` corroborate activity only; they do not prove exact advice attribution, physical peak, or speed. Report approximate RSS/VmHWM, faults, live `AnonHugePages`, and allocator counters separately. Exact physical peaks, bucket attribution, and an exhaustive advice ledger are excluded; if an actually unsupported metric is claimed, mark only that claim INCONCLUSIVE, never substitute zero.

## Stage A — workloads and configuration

Use one measurement child and exactly one workload occurrence per cell. Compare `Off` with `Always` (requested-size minimum 8 MiB), both budget 0 (unlimited), `CAUTIOUS`, identical production/internals/bench-internals features, compiler and flags, and otherwise identical configuration. Cache is Off except for the cache-hit cell, where both arms use identical cache settings. Use an actual configured SeferAlloc `GlobalAlloc` for payload work through owned, non-inlined/window helpers where useful; process-global `System` handles judge bookkeeping. This is not AllocCore-only and does not claim to time every process-global allocation. All Stage A simple cells are COLD; no one-occurrence warm results are claimed. For `cache-16MiB`, the first life is COLD and the second is CACHE (an actual hit), not synthetic warm. Existing judge repetitions are 1 except for this explicit cache sequence.

Freeze and record source/tree, compiler, features, configuration, and binary-hash receipts from trusted CI before measurement. Record requested and resolved tuple, cache readback, and zero tuple conflicts. Do not fabricate a three-artifact manifest. The missing child identity/backend, disabled `raw.v4`, and archived builder stay untouched. Verify observed path behavior: `Off` and below-threshold requests produce no Large advice; fresh eligible `Always` requests accept correct-prefix advice; the actual cache hit receives no advice.

Match existing `g1_big_judge` touch kernels exactly: volatile byte writes every 4096 bytes plus the last byte for dense `cycle`/`many`; `grow` touches each newly exposed tail with that kernel; `edge` writes first and last payload bytes. Report **full allocation + touch + free** timing and full-child setup/probes/trim/cleanup timing as separate scopes, never mixed. Memory comparisons use the same regime.

| Cell (10 total) | Exact operation |
|---|---|
| `cycle-1MiB` | Dense cycle, 1 MiB. |
| `cycle-8MiB-minus-1` | Dense cycle, 8 MiB − 1 byte. |
| `cycle-8MiB` | Dense cycle, exactly 8 MiB. |
| `cycle-16MiB` | Dense cycle, 16 MiB. |
| `cycle-128MiB` | Dense cycle, 128 MiB. |
| `cycle-1024MiB` | Dense cycle, 1024 MiB (1 GiB). |
| `grow-1GiB` | Start at 1 MiB and touch it; then double the same object by realloc, touching each newly exposed tail through 1 GiB; free once. |
| `many-128x16MiB` | Allocate and immediately densely touch each of 128 × 16 MiB blocks, keeping all live; then free in reverse, matching the existing judge's order. |
| `edge-128MiB` | Allocate 128 MiB, write first and last payload bytes, free. |
| `cache-16MiB` | Cache enabled: fresh 16 MiB allocate/touch/free, then allocate/touch/free again, explicitly trim; prove first fresh and second an actual hit. |

## Ordering, receipts, and statistics

Host facts: Linux x86_64, 4 KiB base pages, 2 MiB THP, system mode `madvise`; read and record, do not change host policy. Boundary oracle: 8 MiB−1 is not advised; exactly 8 MiB fresh receives eligible advice. Cache oracle: first allocation fresh, second an actual hit, zero new advice on hit; absent evidence makes that claim unreadable.

Run the existing paired runner per cell: `node scripts/paired-ab-runner.mjs --config <config> --arms A,B --pairs 12`, where A=Off and B=Always; add `--arms A,A` for Off/Off control. This is 12 ABBA blocks/cell (48 launches) plus 12 A/A blocks (48), 96/cell and 960 total. Preserve raw records. The runner's printed block mean/raw difference-t summary is not adopted. Use the checked compact reducer: raw minimum, minimum of exactly two observations per arm per block, `d=ln(Always/Off)`, `exp(mean(d))`, 95% t interval with 11 df and critical value 2.201. A/A interval contains 1 for claim readability, not a product veto. Freeze cold/warm labels; no tuning or reruns. Report medians for faults, RSS, and `AnonHugePages` separately.

Historical J13 references (cite original J13 preregistration §§4 and 6 and J13b preregistration §8; references are context, not new product gates):

- **T0 mechanism corroboration:** dense 128 MiB, 1 GiB, and 1 GiB-grow: median Always `AnonHugePages` ≥80% of block and median faults ≤20% of Off. Missing corroboration makes only that mechanism claim unreadable.
- **T1 speed reference:** dense 1 GiB cycle Always/Off upper 95% CI <0.85 supports that speed claim. If noisy, report unsupported; noise alone does not reverse direction.
- **T2 RSS references (report only):** cycle 128 MiB/1 GiB: Off +2 MiB; grow 1 GiB: Off +16 MiB; many 128 × 16 MiB: Off +256 MiB; edge 128 MiB: Off +4 MiB. Not general bounds or vetoes. The 16 MiB cycle was in original J13; its Off +2 MiB is reference only.
- **Edge/control references:** edge median cold time ≤2 ms is historical context, not a new gate. A/A readability and the historical 1 MiB fault difference ≤5% are descriptive only.
- For applicable cells ≥16 MiB, an interval wholly above 1.10 is an **investigation trigger**, not a kill/veto; edge retains its absolute-time reference. Investigate materially new regressions and obtain owner disposition before any default decision.

The original 64-byte Small-Ir prerequisite is superseded by the dated amendment: #107 owns changed-binary Small-path cost verification through existing gates. No standalone 1024-allocation result is claimed by this Large study.

## Operational bounds and later registrations

Current operational enforcement is the dated amendment above: one child at a time, at most two actors, ≤2 GiB payload, actual-host preflight with ≥8 GiB MemAvailable, ≤120s direct-child timeout and 30-minute measurement-step timeout. The20-minute feasibility check is a pre-dispatch owner gate. Swap/pressure admission and the cumulative256 MiB hard bound are superseded. A failed enforced bound makes the affected attempt INCONCLUSIVE, not product failure; preserve both attempts if the one full repeat is used. Do not tune THP/sysfs. Report actual observed scopes separately; unavailable exact physical, bucket, or exhaustive-ledger metrics do not block this limited comparison.

The following Auto and budget paragraphs preserve later obligations; they are **not** prerequisites for the limited first study. Their own campaign needs separate registration. Retain six request groups `[0,1)`, `[1,8)`, `[8,32)`, `[32,128)`, `[128,512)`, `[512,∞)` MiB. Auto uses globally eligible, aligned cohorts of exactly eight, with exactly one control in each cohort (not one control total). For each selected control, sample at most 16 windows × 2 MiB; include only clean owner-first-life evidence, excluding remote UNKNOWN and cache/realloc-invalid evidence.

Cache-Off phases retain 16 MiB sparse at 5%→dense full, plus mixed recovery using eight dense 16 MiB blocks and eight sparse 64 MiB blocks at 2%, with actual sparse-to-dense recovery in the same group. For each phase/group, target 96 valid controls (cap 1536 eligible fresh allocations per phase/group); valid occupancy classes are sparse <0.1 and dense >0.9, with attained sparse/dense shares <0.5/≥0.9, respectively. Static-control equivalence requires the 95% CI wholly within ±5%. Equal-budget policy overhead ceiling is 1.20, separate from the sparse-RSS reference Off + 25% × (Always−Off). The 128 MiB free-time delta Auto−Always with cache Off is ≤0.5 ms; churn and churn-small obligations are deferred separately. These are original campaign obligations, not new Stage A gates.

Budget follow-up uses a 256 MiB budget and a requested live volume of 2 GiB (8 × 256 MiB), with sparse and dense 128 × 16 MiB regimes evaluated separately. Accepted/advised-byte peak must be 90–100% of the 256 MiB budget under original T0, distinct from strict charge ≤ budget. Post-free outstanding advice equals the advice held by cached mappings; require zero after their confirmed terminal release, not at every free. Retain cache carry and confirmed terminal release; same-regime RSS reference Off + budget +256 KiB. Dense benefit upper CI <1 is the original B reference, not a Stage A veto. No cross-regime Pareto claim. No full D / B / L / C / S / GO campaign is authorized here.

## Scope limits

This registration does not establish historical whole-mapping identity, complete J13/J13b acceptance, exact physical backing/peak, bucket-attributed RSS, or a default change. Full builder identity/adapters/admission remain for a later full J13b campaign only, not shipping or this narrow study. Preserve historical NO-GO and owner exception as distinct facts; no default GO is inferred.

## 2026-11-12 tail-trim registration

**Registered before any measurement of the new source.** This section registers a rerun of the same ten-cell Always/Off campaign on a source that trims the advised range to the 2 MiB floor of the requested block's end (payload offset plus requested size) instead of the 2 MiB floor of the SEGMENT-rounded usable span (owner-approved default-Always follow-up, package "tail trim"). Nothing below is a result; every figure is a hypothesis to be confirmed or refuted by the rerun.

**Scope of the rerun.** The same ten cells, the same two arms (A=Off, B=Always; both budget 0, requested-size minimum 8 MiB, `CAUTIOUS`), the same paired runner, pair counts, A/A controls, host preflight, operational bounds, receipts and statistics as the sections above, through the existing `examples/g1_big_judge.rs`. The only change is the source under test. The measured source commit and its trusted-CI build receipt are recorded before measurement; a rerun on any other source is not this registration.

**Hypotheses (not results).**

- `AnonHugePages` under Always: 8 MiB block 10 MiB to 8 MiB; 16 MiB block 18 MiB to 16 MiB; edge 4 MiB to 2 MiB. Off stays 0.
- Cycle peak RSS, Always minus Off: about 2.03 MiB to about 0.
- No dense Always/Off wall-clock ratio is worse than the corresponding ratio in the committed report (`docs/perf/j13_limited_20261111/J13_LIMITED_ALWAYS_OFF.md`).
- Edge cost: about half of the committed edge Always/Off ratio (one fewer advised 2 MiB region is faulted).
- `grow-1GiB` is checked specifically: its Always/Off ratio, faults, peak RSS and `AnonHugePages` are reported as numbers. No fix for it belongs to this package.

**Stop rule.** If any dense Always/Off ratio is worse than the committed report's, stop and bring the report to the owner before any further step.

**Side item.** The second-life CACHE payload ratio (+2.3 percent in the committed report) is a read-only investigation: no code, default or harness change is made for it here.

This registration changes no harness, runner, reducer or gate and makes no default, dependency or version change. Fault counts and `AnonHugePages` still corroborate activation only; they do not prove exact attribution, physical peak or speed.
