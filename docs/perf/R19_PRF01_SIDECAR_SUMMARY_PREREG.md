# R19 PRF01 sidecar summary C′ — phase 3 bench preregistration

Measurement-only protocol; NOT measured, NOT a summary implementation, NOT a
production acceptance receipt. Workload bodies are shared by thin iai wrappers
and one cfg-gated integration oracle test. Native installed-global example/runner
belongs to phase 5. No B/A samples or production summary algorithm are present.

Authority read in full: `docs/reviews/2026-10-11-solution-research-summary.md`,
`docs/reviews/2026-10-11-solution-research-scan-prf01.md`, Question 2 of
`docs/reviews/2026-10-11-owner-questions-decisions-ox.md`,
`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`, and
`docs/REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md`. C′ supersedes the research's C
recommendation: pending AcqRel publication, then summary load(Relaxed), conditional
fetch_or(Release), under the SAME retained RoutePin; discovery clears summary
before pending cuts. Strict trim and maintenance ignore summary. Reduced Loom
proof does not prove actual-type lifetime/provenance. This gate does not close
those correctness obligations or item 162 by itself. Existing open-item statuses
are left unchanged; preregistration does not close their acceptance criteria.

## Owner thresholds (verbatim from Question 2)

- Entry point: `SeferAlloc` `GlobalAlloc` → `HeapCore`, routed. Идентичность — measurement-commit SHA по R29-6 п.1.
- **Режим 1 (польза, Ir; судья детерминированный).**
  - `multiseg_cold_256k` и `seg_cycle_decommit_256k`: B/A ≤ 0,25 по Ir. Оценка исследования ≈0,07, порог с запасом.
  - Новые `growth16_{n,2n,4n}`: наклон `(Ir4n−Ir2n)/(Ir2n−Irn)` ≤ 2,5 у B.
  - Оракул: у B в чистом росте `pending_words_visited == 0` и `summary_words_visited > 0`; у A ≈ 319 063 / 1 004 503 (как R35). Число routed-сканов A == B (иначе режимы разные).
  - Контроли: hot-churn |ΔIr| ≤ T по схеме R16 perf83; mimalloc ΔIr = 0.
- **Режим 2 (цена продюсера; отдельно, не сводить с режимом 1).**
  - (i) iai «присоединённый продюсер освобождает N блоков»: ΔIr ≤ +6 Ir на удалённый Small free. Оракул: число summary-RMW и summary-load отдельными счётчиками, оба > 0 в сценарии с повторными free в одно слово.
  - (ii) нативный paired A/B через установленный `#[global_allocator]`, fan-in 1/2/4/8 продюсеров в один сегмент: p50 ≤ +5 %, p99 ≤ +10 %, с A/A-контролем. Если полоса A/A шире порога (как в item 81 на WSL2), ось (ii) = INCONCLUSIVE, и **GO не выдаётся** (fail-closed) до прогона на нешумящем хосте.
- `size_of::<SmallSidecar>()` растёт ровно на 512 Б (assert в скрипте).

## Frozen operational interpretation

A1/A2 use ONE A primary binary; B1/B2 ONE B primary binary. C0 is a different
primary binary built from the SAME A source, environment/toolchain/features.
EACH A1/A2 × B1/B2 comparison must pass, not averages or a selected pair.
Primary exact requested features: `production bench-internals internals`, no
oracle cfg, no alloc-stats. Native exact requested feature: `production`.
Retain Cargo resolved dependency-feature graphs, rustc -vV, target, flags,
profile, default-feature policy, OS/CPU, allocator and work units in environment
and build receipts. Same environment/toolchain/features across matched arms.

Existing benefit names in `benches/perf_gate_iai.rs` are literal names:
`multiseg_cold_256k` uses 2 × 34 alloc/free of 258752 B @ align 8;
`seg_cycle_decommit_256k` uses 6 × 34 of that SAME Small size (not literal 256 KiB,
which would route Large). New growth names are `growth16_n`, `growth16_2n`,
`growth16_4n`, N=32768/65536/131072 allocations of 16 B, no frees before end.
B slopes are checked independently for B1 and B2; denominator MUST be positive.
A nonpositive denominator is a complete threshold failure, not a division by zero.
The owner specifies no lower bound on the numerator: a negative numerator is
reported and passes the literal upper bound. It is not silently rejected as a
new threshold; source/workload review must explain such a nonmonotone result.

Hot controls: `small_churn_16b`, `churn_256b`, `churn_write_256b`,
`aligned_churn_640b_a128`. T=max(10,|A1−A2|,|C0−A1|), per cell;
|B−A|≤T for every cross pair. All 13 existing mimalloc cells are required, with
exact zero delta for every cross pair (names frozen in judge inventory).
There is **no existing strict-trim iai cell in benches/perf_gate_iai.rs**.
`strictTrim` explicitly records no cell; no invented iai name or placeholder
Ir. Existing criterion lifecycle/global benches call diagnostic trim, but are
not a strict-trim iai cell and cannot be substituted. If a future reviewed freeze
adds an existing strict-trim cell, its one-sided delta≤T must be imposed before
measurement; this freeze does not silently add it.

New publication iai name: `r19_prf01_publication`, joined remote publication
of 34 × 258752-B blocks in excluded setup, then a 2×34 owner discovery window;
strict trim is outside that iai window in the integration oracle only.
discovery_records_reclaimed>0 and strict_trim_remaining equal ALL A/B/C0 rows.
strict_trim_remaining is the actual joined records consumed by the subsequent
strict trim (observed zero after all 34 discovery reclaims), not a fabricated
pending census or a ninth API counter. Nonpublication rows carry null + reason. New producer name:
`r19_prf01_joined_producer_same_word`: fixed N=4096 remote Small frees of 16 B,
joined producer, repeated frees in the same pending word, one segment, no owner
discovery during producer window. Setup excludes preallocation/geometry checks.
The producer wrapper selects `EntryPoint::Custom` on the exact intended child
symbol `<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window`, a private
`#[inline(never)]` function borrowing allocator/address slice/Layout. Spawn/join,
parent handoff and child Vec destruction are outside this counted function.
The emitted symbol spelling was observed with nm (see the corrected observations
below); collection activation (child Ir) is still NOT verified. No broad guessed
wildcard is used. Every (IrB−IrA)/4096≤6 remains
the owner threshold on this matched child-body boundary, not an algebraic
spawn/join-control subtraction. B summary_rmw>0 and
summary_load>0 separately; A APIs under oracle cfg return TRUE constant zero,
not compiled-out observer defaults misreported as observations.

Producer excluded setup allocates a bounded8192 live 16-B candidate set, resolves
each through safe numeric-key RouteDirectory::lookup, checks Small/Primordial
kind and payload inclusion, and groups by pinned route incarnation. It selects
4096 from ONE actual descriptor and locally frees every other candidate before
the counted child function. If no descriptor has4096 candidates, setup fails;
N is never reduced and address/4MiB is not treated as route identity. Test-only
setup diagnostics use the existing unsafe owned-reservation observer under its
live-owner contract to print each group's canonical root, address/offset min/max
and allocation count. No new pointer hook or shipping route behavior is added.
Repeated pending-word traffic is checked on the selected live addresses.
The bound8192 is a setup selection limit, NOT a proven promise that every
recycled owner provides a4096 group; independent Cargo smoke remains necessary.

Oracle: separate compile-time build under `cfg(r19_sidecar_summary_oracle)`.
The existing `r18_sidecar_scan_bench` remains unchanged: its frozen runtime
TLS mode can replace production swap by prefilter, so extending/reusing it would
not guarantee primary-equivalent A. Cargo check-cfg adds only the new cfg name,
not a Cargo feature or composition change. The new observer counts actual scan
construction, aggregates visited/exchanged/nonempty/successful-reclaim counts
locally and updates relaxed process totals once per completed discovery scan.
Eight API slots are ordered pending_words_visited,pending_words_exchanged,
nonempty_cuts,discovery_records_reclaimed,routed_scans,summary_words_visited,
summary_rmw,summary_load. A summary slots are true constant zero because A
has no summary operations; other counters are actual observations. Safe read-only
`HeapCore::dbg_prf01_sidecar_oracle` is in PURE_OBSERVERS. No resets or runtime
mode; production branch and publication orderings are unchanged. Do not combine
the r18 and r19 cfgs: r18 changes the algorithm being observed.
Feature-exact retained receipt links identical source/workload/config to primary
and oracle binary hashes, shows cfg-only differences and reviews every measured
path; it cannot replace actual activation observations. Every benefit row has
routed_scans>0 equal ALL five arms. Freeze A pending count tolerance NOW, before
counts: inclusive ±floor(1% × expected) around 319063/1004503 respectively.
Other A growth rows require positive pending visits and exchanges=visits.
B pure growth pending visits=0 and summary_words_visited>0. Pure-growth nonempty
cuts/reclaimed records=0 are real oracle observations, not assumed zeros.
General counts satisfy nonempty_cuts≤pending_words_exchanged≤pending_words_visited.
Compiled-out observations are null WITH nonempty per-field reasons. Controls
have no paired oracle; all nine oracle fields null. Required activation fields
cannot be null; strict_trim_remaining is only observed by publication's external
strict-trim check, and is null + reason elsewhere.

## Native protocol only (phase 5 runner, not implemented here)

Installed `#[global_allocator] SeferAlloc`, fan-in n=1/2/4/8 into ONE segment.
4096 fixed 16-B remote frees/process; retain per-process ns/op, p50Ns, p99Ns.
Nine fresh processes EACH A / independent same-A AA / B per n = 108 total.
No retries, replacement processes, pooled latency quantiles or timing oracles.
For each n and repetition 1..9: odd repetitions A,AA,B; even B,AA,A.
Zero-based position/index; one-based repetition. Judge validates complete exact
schedule in array order. runIds globally unique including paired oracle runs;
positive OS pid per process (OS pid reuse is allowed, runId reuse is not).
For each n/metric use median of nine per-process metrics, never pooling.
Noise=|median(AA)−median(A)|/median(A). p50 noise>0.05 or p99 noise>0.10 =>
INCONCLUSIVE; otherwise (median(B)−median(A))/median(A)≤0.05/0.10 respectively.
nsPerOp is descriptive, no invented owner threshold. AA is independent fresh
processes from same A binary, not recycled A samples.

## Immutable identities and retained proof

Capture source identity BEFORE build/sample, separately pin A/B at top level;
measurement commits are required and A is ancestor of B. This worker does not
create them. Retain commit chain plus raw ancestry proof receipt, source patch
and untracked manifest exact bytes. baseHead is measurementCommit; source fields
remain explicit even for clean committed trees (hash empty patch/manifest bytes,
never placeholder 0). Hash all source/build inputs, including untracked files and
ignored inputs actually used. identitySha256 hashes canonical retained source
manifest; receipt specifies encoding and independently reviews source identity.
No identity reconstructed after measurement. Source capturedAt < buildStartedAt
< startedAt; binary digest is captured after build and before sampling. Retain
binary capture/copy timestamps in build receipts; source capture is necessarily
before binary existence. Hashes prove byte identities only, not ancestry or
truth of labels; human review of retained commit chain, source mapping, cfg
linkage and raw outputs is still mandatory. Independently frozen top-level pins
must be reviewed before samples: deriving pins from rows would accept wholesale
arm/binary substitution. Changing pins together with all evidence requires a
new independent identity review, not an assertion that hashes prove intent.
All binary identities are independently pinned and checked against provenance;
only C0 differs from A primary by requirement. Different feature/cfg builds may
be byte-identical; global uniqueness of seven binary digests is NOT required.

## Version1 full schema

All receipts: `{path,sha256}` with retained repository-relative nonprivate path,
64 lowercase hex digest. No machine-specific absolute paths in artifacts.
Integers are finite safe nonnegative; positive Ir/timings/workCount/pid.
Features are exact sets without duplicates. Source equality includes timestamps.

| Scope | Fields / constraints |
|---|---|
| top | version=1; hashes=[{path,sha256}] raw document/script order; environmentSha256; environmentReceipt; inventoryReceipt; featureExactReceipt; identityReviewReceipt; entry; inventory exact judge list; strictTrim {cell:null,reason}; identities; ancestry; sizeOf; iai; native; schedule |
| identities.A/B | label A/B independently; measurementCommit (40 hex); source; environmentSha256; binaries {iai,oracle,native}; buildStartedAt; buildReceipt; features exact primary; nativeFeatures production; primaryCfg []; oracleCfg [r19_sidecar_summary_oracle]; toolchain {rustc,target,profile,flags} identical |
| identities.C0 | source exactly A source; binarySha256 distinct from A primary (only required binary inequality); buildStartedAt; buildReceipt |
| source | baseHead,patchSha256,untrackedSha256,identitySha256,capturedAt |
| ancestry | result='A ancestor B'; chain unique 40-hex commits starting A ending B; receipt retained proof/parent-chain review |
| sizeOf | A/B actual size_of SmallSidecar integers; receipt; B−A exactly512 |
| iai | name exact inventory; arm A1/A2/B1/B2/C0 exactly once/name; environmentSha256; binarySha256; source; startedAt; features; receipt; config; Ir; oracle; producer scenario literal as judge |
| config | requested/resolved objects exactly equal; maintenance false, workCount positive, segmentCount positive, blockBytes positive, cachePolicy nonempty; configConflictsDelta=0; processBoundary='isolated-process'; runId; pid |
| oracle | pending_words_visited,pending_words_exchanged,nonempty_cuts,discovery_records_reclaimed,routed_scans,summary_words_visited,summary_rmw,summary_load,strict_trim_remaining; unobservedReasons; linkage |
| oracle.linkage | required paired rows: binarySha256 pinned oracle; primaryBinarySha256 pinned primary/C0; source; environmentSha256; features primary; cfg oracle cfg; startedAt; config matching row; receipt; featureExactReceipt matching top digest. Controls null linkage plus all counter null reasons |
| native | same provenance/config fields; n∈1,2,4,8; arm A/AA/B; repetition,position,index matching schedule; entry='installed #[global_allocator] SeferAlloc'; nsPerOp,p50Ns,p99Ns positive; p99≥p50 |
| schedule | 108 exact {n,arm,repetition,position,index} jobs, as protocol above |

Configuration readback MUST be observed where SeferAlloc is applicable.
Mimalloc rows have requested=null,resolved=null,configConflictsDelta=null and a
nonempty notApplicableReason; runId/pid/processBoundary remain mandatory.
No fake SeferAlloc config observation is requested for mimalloc.
If unavailable in SeferAlloc primary, retain an
actual same-source/claim-path probe receipt with copied resolved values and
review feature-exact linkage; do not turn absent readback into fabricated values.
This schema's config fields are evidence assertions, not auto-proven by equality.

## Judge, derived output, fail closed, freeze

`node scripts/r19_prf01_sidecar_summary_gate.mjs --input FILE` reads JSON only,
emits derived tables with actual numerators/denominators, arithmetic assertions,
mode1 / mode2Iai / mode2Native verdicts and global verdict. Missing/malformed/no
data => INCOMPLETE nonzero; valid complete threshold failures => NO-GO nonzero;
wide AA band => INCONCLUSIVE nonzero (takes global precedence); GO only every
required axis. Required identity/oracle/config failures are INCOMPLETE, not GO.
No artifact writes or subprocesses. --selftest emits only 'synthetic selftest
only', with counted checks and zero measurements; synthetic GO is never evidence.
--hashes prints raw SHA256 document then script. --check-hashes verifies the
companion exactly. Companion excludes itself; no self-hash. Input hashes must
match recomputation; edits require explicit re-freeze BEFORE any measurement.

Selftests include complete synthetic GO, boundary/over comparisons, all cross
arms, each hot/mimalloc cell, slope/positive denominator, null/absent counters,
activation/count tolerance/publication checks, independent summary RMW/load,
source/binary substitutions and swapped labels, schema/hash/environment/feature/
config/time/duplicates, sizeof and schedule/missing process, each native n and
p50/p99 AA boundary/over plus B limit boundary/over. Count real rule mutations,
not merely malformed-JSON checks.

Raw tier policy: <200KiB verbatim; 200KiB–2MiB gzip OR explicit truncated excerpt
with markers and reproduction command; >2MiB content hash, minimal excerpt,
external content-addressed full artifact plus reproduction command. Summary
CSV/JSON, commands and immutable identities ALWAYS retained irrespective of tier.
No measured report or raw data exists in this phase.

### Producer child-thread Ir coverage: required receipt, not inferred from oracle

Pinned macro0.5.1 source `lib_bench.rs` renders setup as the argument expression
before entering the inner noinline wrapper in the SAME profiled process. Pinned
runner0.14.2 `runner/lib_bench.rs` selects only the custom toggle when
EntryPoint::Custom is specified; `runner/callgrind/args.rs` sets
separate_threads=true (unlike standalone Callgrind's default no). The documented
API is `EntryPoint::Custom(String)` and LibraryBenchmarkConfig::entry_point.
Simply adding collect-atstart=yes would retain the default parent toggle and
would not establish the requested excluded setup/child boundary. Producer now
uses the named child custom boundary instead. Installed emitted symbol must be
verified by nm/demangling before samples; exact-name failure is INCOMPLETE, not
permission to broaden the match. Collect-jumps is unrelated to thread activation.

References inspected (documentation/source, not measured receipts):
https://docs.rs/iai-callgrind/0.14.2/iai_callgrind/enum.EntryPoint.html,
https://docs.rs/crate/iai-callgrind-macros/0.5.1/source/src/lib_bench.rs,
https://docs.rs/crate/iai-callgrind-runner/0.14.2/source/src/runner/lib_bench.rs,
https://docs.rs/crate/iai-callgrind-runner/0.14.2/source/src/runner/callgrind/args.rs,
and https://valgrind.org/docs/manual/cl-manual.html.

Every producer iai row REQUIRES `producerIrReceipt` with this checked schema:

| Fields | Rules |
|---|---|
| binarySha256,source,environmentSha256,features,cfg | exactly primary row identity/features; cfg=[] |
| receipt,symbolReceipt,profileReceipt | retained {path,sha256} evidence |
| symbol,toggleCollect | exact intended child symbol above; installed nm evidence mandatory |
| runnerVersion,valgrindVersion,generatedArguments | runner0.14.2; actual Valgrind version and complete generated argv retained |
| collectAtStart,defaultToggle,entryPoint,separateThreads,aggregation | false,false,Custom,true,'sum of all thread profile Ir' |
| childFreeCount,childIr,totalIr,startedAt | count4096, positive child Ir, totalIr=row.Ir, valid post-source-capture time |
| control | receipt; workload='child remote frees removed'; independently retained source/binary identity (binary differs); same environment; childIr=0; totalIr differs; deltaIr=primary total−control total positive; parentSetupUnchanged=true; collectionUnchanged=true |

Removed-workload control changes only the child frees; owner cleanup must still
retire its allocations OUTSIDE the counted boundary. Profile proves executed
remote-free/pending publication work on the child; native joined completion4096
alone does NOT establish Ir coverage. Judge rejects missing/substituted/zero
coverage or non-distinguishing control. Hashes and boolean assertions are not a
proof of receipt contents: independent raw-profile/symbol/source review remains
mandatory. No real producerIrReceipt, child profile, or negative-control run has
been obtained; Mode2 evidence remains INCOMPLETE and synthetic GO is not real GO.
No broad toggle or unmeasured cancellation claim is accepted.

### Corrected fresh-process observation (no algorithm-regression conclusion)

Earlier Windows debug and WSL release smoke used sequential owner threads in
ONE process: cycles1066458/335 exceeded frozen1004503/275, while multiseg
matched319063/99. Publication was358773 in all-tests versus355593 narrow.
Those were invalid freshness assumptions, not accepted activation references.
The orchestrator subsequently reported fresh-scenario subprocess smoke PASS3/0:
multiseg319063/99; cycles1004503/275; publication355593/92, nonempty34,
reclaimed34, remaining0; producer trim consumption4096; sizeof41984. Frozen1%
assertions are unchanged and pass under the correct process boundary. This
refutes the prior alleged algorithm regression: the earlier mismatch was a
shared-process/reused-registry sampling artifact. No owner cycle-reference
amendment or tolerance change is needed on this evidence.

R35 source8d08825a9a92192763bdc12e1bfb0f9c25af6003 used production
bench-internals internals, r18_sidecar_scan_bench and separate iai children;
its cycles275/1004503 reference remains frozen. Corrected smoke here is an
orchestrator-supplied observation excerpt, not a worker-run full gate or source/
binary prebuild acceptance receipt. The counts are reproducible with
`tests/r19_prf01_sidecar_oracle.rs` (frozen 1 % assertions); no raw log or
summary file is retained for them because they are not the basis of any verdict.

The orchestrator's nm observation is exactly
`<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window`.
Custom symbol, receipt validator and fixture are corrected to this observed
spelling (including angle brackets). The next binary must be rebuilt from this
source. Child Ir/negative-control capture remains unverified; no producer GO.
Primary-build equivalence. Every `src/` hunk of this phase sits under
`cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))` (plus the
check-cfg name in `Cargo.toml`), so primary builds compile none of it. Byte
equivalence of two SEPARATELY BUILT bench binaries is not claimed and is not
needed: one single-profile micro comparison (`multiseg_cold_256k`, base commit
versus this tree) differed by −797 Ir (4026902 vs 4026105) with exclusive-cost
differences in try_carve_batch, try_drain_freelist_batch, the cell wrapper,
RouteDirectory::register and finish_bind. The cause is not established; separate
binaries differ in code layout and route registration is address-dependent. The
gate bounds exactly this layout noise per cell with C0 (T = max(10, |A1−A2|,
|C0−A1|)), and both gate arms carry this cfg-off code. No profile, summary file
or baseline refresh is retained for this micro comparison. No performance GO or
C′ B/A result follows.

Final subprocess-boundary mutant reruns: reclaimed aggregation disabled and
routed construction disabled each produced child0passed/1failed and parent
0passed/1failed, exit101. Clean restored publication produced child1passed/0failed
and parent1passed/0failed, exit0, with355593 visits/exchanges,34 nonempty/reclaimed,
92 scans and trimremaining0. Child stderr is not forwarded, so these reruns prove
failure propagation without claiming the hidden child assertion text was captured.
Both restores compared byte-identically to original oracle SHA256
6cf64e22a20380502c61486f9e683020b2fd098a2e2a14b273b81eff51b42dfc.
Child Ir/negative-control capture remains unverified; no producer GO.



## Wired inventory and validation surface

`benches/support/r19_prf01.rs` owns the workload bodies as one responsibility;
`benches/perf_gate_iai.rs` has five thin wrappers and ONE contiguous five-line
member insertion immediately after `seg_cycle_decommit_256k` in the existing
`perf_gate` group: **102 members = 97 original + 5 new**. There is no second
group; `main!(library_benchmark_groups = perf_gate)` is unchanged.
The judge's 24-cell inventory is a decision subset, not the full suite: five
benefits, four hot controls, thirteen mimalloc, publication, producer. All 24
are actual wired iai names; publication is not an oracle-only invented cell.
Existing stubs guard retains the original perf_gate group parser and checks the
exact contiguous five names after seg_cycle, each occurring once, a single group,
and unchanged main wiring. Setup attributes are transparent to cfg parsing. No baseline numbers or canonical README/architecture counts are
refreshed by preregistration; no new top-level bench file was added.
`iai.mjs` BENCH_OPS now explicitly lists growth32768/65536/131072 complete
alloc/free pairs; producer/publication are null because their excluded setup
makes subtraction of a full bootstrap proxy invalid. The PRF01 judge uses
raw child Ir/free, not that descriptive marginal column. No baseline values
are inserted. Root inventory is386 test files,87 examples,26 benches;
README's385 test-file tokens are a known scope blocker for
verification_inventory_matches_docs, not bypassed or edited here.

`tests/r19_prf01_sidecar_oracle.rs` has THREE named cfg-gated integration tests:
`growth_oracles`, `publication_oracle`, `producer_oracle`. Parent tests invoke
current_exe with --exact and SEFER_PRF01_SCENARIO; growth launches TWO fresh
children (one per geometry), publication and producer one each: FOUR scenario
processes total. Each child runs exactly one scenario with explicit owner/producer
joins; parent checks child success and forwards counts. System remains global.
No mutex or shared-registry freshness assumption substitutes for process isolation.
Real GlobalAlloc multiseg
2×34, cycles6×34, publication and joined producer. It prints snapshot deltas,
checks frozen inclusive1% counts, actual successful discovery reclaims=34, one
strict trim with zero remaining publication records, and actual strict trim
consumption of4096 producer records (so constant-zero A summary alone is not
accepted as activation proof). No growth timing is run by this integration test.
`HeapCore::dbg_prf01_small_sidecar_size()` prints actual
`size_of::<SmallSidecar>()` in the oracle binary, through a safe PURE_OBSERVERS
entry; this is logical layout, not RSS and not a fabricated before/after value.
The multiseg test-only body and root diagnostic loop require
all(test,r19_sidecar_summary_oracle), so a normal bench compiled with cfg(test)
does not include unused oracle-only helpers. Source identity must be re-captured
after these changes; no already-built executable is relabelled as this source.
Only A exists now; the future B receipt still must assert exact+512.
Primary/oracle linkage and process isolation still require future retained
receipts; test output is not a measurement-commit identity or GO.

Dependencies already pinned in Cargo.lock: iai-callgrind0.14.2,
mimalloc0.1.52; no new dependencies. Oracle process bounded work ensures counters
cannot overflow in the registered experiment; process-wide deltas require no
concurrent unrelated SeferAlloc workers, maintenance remains off.

Validation commands from repository root (orchestrator runs Cargo):

```text
node scripts/r19_prf01_sidecar_summary_gate.mjs --selftest
node scripts/r19_prf01_sidecar_summary_gate.mjs --check-hashes
node scripts/verify-perf-gate-stubs.mjs
node scripts/verify-dbg-hook-safety.mjs
cargo check --locked --bench perf_gate_iai --features "production bench-internals internals"
cargo check --locked --lib --features "alloc-global bench-internals internals"
RUSTFLAGS="--cfg r19_sidecar_summary_oracle" cargo test --locked --test r19_prf01_sidecar_oracle --features "production bench-internals internals" -- --nocapture
RUSTFLAGS="--cfg r19_sidecar_summary_oracle" cargo check --locked --lib --features "alloc-global bench-internals internals"
```

On non-Linux the iai main is inactive, so actual thin-wrapper macro compilation
must also be checked on Linux before claiming build coverage. No Cargo build or
runtime-test success is claimed by this preregistration. Counterfactuals to run
locally on this same session's resumed corrections: suppress constructed scan
increment (routed_scans>0 fails), suppress visited aggregation (1% fails), suppress
successful discovery-reclaim increment (34 fails), suppress real remote free
(strict-trim4096 fails). Do not alter shipping publication/summary algorithm to
manufacture a benchmark result. Mixed R18/R19 cfg compilation is rejected rather
than measuring a different algorithm. No B/A sample or p3b baseline refresh is
a prerequisite for the source-only inventory guard.

