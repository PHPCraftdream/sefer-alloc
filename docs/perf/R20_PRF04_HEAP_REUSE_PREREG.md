# R20 PRF-04 heap-reuse Ir gate (P4-1) — preregistration

**Status: UNMEASURED. Candidate SHA: `TBD`.** Before candidate build, the orchestrator replaces `TBD` with the full 40-hex commit SHA, records branch and ancestry, and re-freezes the complete input set. No candidate build or result is claimed here.

## 1. Scope, invocation and frozen modes

Owner-frozen 2026-10-14. This deterministic Ir gate evaluates heap reuse at L1 (registry claim/release) and L2 (shipping `#[global_allocator]` target allocation). Primary claim: L2, always. `RELEASE_LAYER` is a frozen code constant shared by the judge and runner: `L2` or `L1`; the input must match it. L1 is only the predeclared release fallback. It excludes only `l2_wave_release` from release judgment; L1 wave release (14 Ir/exit) remains mandatory. Construction/start, L2 cold and L2 recycled plus all corresponding primary oracles remain mandatory under either constant. Fallback selection is bound by the retained, prerefreeze A-only trial-rehearsal bundle and the re-frozen constant, not runtime input flags or an invented signed helper.

The production workflow invokes the runner with only `--stage prepare|build|oracle|measure|rehearsal|cleanup` and `--out-dir PATH`. There is no candidate, policy, release, or collection-policy CLI. Source changes are allowed only before the orchestrator re-freezes the preregistered inputs. A rehearsal is A-only, needs no candidate, and can emit only `REHEARSAL:...`, never GO/NO-GO. An official prepare remains unusable while the candidate constant is `TBD`.

A and S are the source/build defaults FreshFirst and ReuseFirst respectively, resolved from source/configuration, not user flags. No policy API/flag is invented. Per-process configuration conflicts must be zero. Primary metric is Ir; native latency, throughput, RSS, cache footprint and contention are deferred.

## 2. Arms, identity and process cells

A1/A2 repeat one A binary per layer; S1/S2 repeat one S binary per layer. C0 is A source at a different equal-length one-letter worktree path and uses only registered `-C debuginfo=line-tables-only`. It controls build/layout noise. S is a descendant of A; no cherry-pick is manufactured. All stage manifests capture clean committed HEAD/tree, patch/untracked digests, source identity and capture time. Candidate ancestry and all thirteen frozen input paths are recorded. Builds capture executable byte SHA-256; the runner rechecks actual bytes before every oracle, measurement, and launch. Build and oracle receipts are linked by manifest hashes. Every process retains stdout, stderr, exit code, times, argv, cwd, and environment.

## 3. Fixed inventory and collection contract

L1 uses `benches/support/r20_prf04_heap_reuse.rs`, `benches/support/r20_prf04.rs` and `benches/r20_prf04_heap_reuse_iai.rs`: exactly start plus cold/recycled/wave/wave_release at N/2N/4N (13 rows). L2 uses the shipping global allocator benchmark and has exactly cold/recycled/wave_release at N/2N/4N (nine rows). N=16, 2N=32, 4N=64. L1 has 13 and L2 has 9 required rows per arm/process, except the explicitly omitted L2 wave-release family in frozen L1-release fallback.

Measured child functions must execute exactly N target incoming calls (including any registered setup operation), and each target frame's inclusive Ir must equal its complete child-thread Ir. L2 recycled setup releases one direct `HeapRegistry::dbg_claim_lease()` on the benchmark main thread before collection; it adds no child target call or guard record. Measured L2 recycled work is exactly N target incoming calls and N corresponding child-guard releases. Every child profile, PID, TID, part, argv, executable, index, toggle and effective setting is checked. Use the proven R20/R19 `profileFunctions`/`parseProfiles` logic, including inclusive frame and call-edge accounting. Positive actual target work and exact per-child counts are mandatory. L1 fallback retains mandatory construction/start and all primary L2 cold and recycled cell oracles/rows; only the L2 wave-release family is excluded from release judgment, while L1 wave release remains mandatory. The L2 recycled oracle always verifies target claim activation and recycled path; fallback does not skip it. Any runtime-prebind allocation yielding target claim delta zero invalidates target activation evidence.

The actual collected functions and registered N must agree. If iai's built-in warmup calls the cell outside measurement, use a custom harness/configuration that executes only the registered measured calls (or count it explicitly); do not accept N+1 actual incoming calls as N. Collection settings are harness configuration receipts, not user policy flags. Capture and compare effective runner arguments, source config and raw logs.

## 4. Scenarios and path oracle

Cold: N sequential first allocations/claims, each in an independent ready holder; each must bind once. Recycled: one released warmup slot precedes N measured sequential claim+exit pairs, with setup outside the target collection. Wave: release 64 ready holders to FREE, then perform N sequential claims. Wave release begins with 64 LIVE holders, releases a wave and measures N exits only; there are no measured claims.

The L1 oracle records existing main claim-source array (length four), explicit missing/zero free-set observer for A, and cfg-gated free-set observer/popcount for S. No Cargo feature or new allocator API may be invented. L2 oracle uses real target allocations and actual counter snapshots immediately before/after each child target allocation. Snapshot code must not allocate; direct private TLS state is labeled unavailable. Oracle arrays are exactly four main sources plus explicit missing-free zero, as applicable; popcount is observer-only. Bind every oracle record to source, binary, environment and cell.

Expected: start bump/new=1; cold bump/new=N and no reuse; recycled hint/reclaim=N; wave A hint=1, bump/new=N-1, while S hint=1 and free=N-1; wave-release has no claim deltas, and S popcount=N-1 (A zero). A popcount observer may be absent; S observer absence is incomplete. Every S L2 cold/recycled child must demonstrate target claim delta one; aggregate N. L2 wave-release has no target claim.

## 5. Frozen budgets and verdict

Budgets: cold 4 Ir/op; recycled 4 Ir/claim+exit pair; wave release 14 Ir/exit; direct start `T_s+4`; L2 cold/recycled 7 Ir/op or pair. For every A1/A2 × S1/S2 pair and registered window require exact-integer `20*(wS-wA) <= wA` and `wS-wA <= L_w*P+T_s`. Negative deltas pass. Wave claim has only the relative rule and `4*wS <= wA`. Cold/wave windows are 0→N, N→2N, 2N→4N (P=16,16,32); recycled has N→2N (16) and mandatory 4N linearity; release has N→2N (16) and 2N→4N (32). Preserve all prior upper-window counts. `T_s=max(10, |A1-A2|, |C0-A1|)`; both differences must be <=10, and T_s never widens relative limits. Recycled and wave-release linearity are mandatory.

Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO. Missing identity/hash/profile/count/cell/oracle is INCOMPLETE. Wrong path, policy, activation, target count or profile is INVALID. Fixed-budget failure is NO-GO. Noise above ten is INCONCLUSIVE. Missing sibling is unjudgeable, never a pass. Only complete verified evidence can GO.

## 6. A-only rehearsal and fallback

Before any S build or collection, run a full A-only L2 trial-rehearsal. It validates L2 cold/recycled target activity regardless of release-symbol outcome and retains source identity, actual L2 binary SHA-256, resolved configuration, effective settings, positive target profiles and full receipt inventory. If exact release guard evidence is absent or uncollected, the orchestrator chooses `RELEASE_LAYER='L1'` before candidate build and refreezes both judge and runner constants. This retained bundle is the selection evidence; the judge verifies its prerefreeze binding and the constant/input match. L1 excludes only L2 wave-release judgment; it never skips L2 cold/recycled cells or oracles. Rehearsal has no candidate and never emits a gate verdict.

## 7. Frozen inputs, workflow and self-tests

The frozen inventory order is prereg, judge, runner, collector, profile helper, run self-test, control geometry rule, both benches, both support bodies, both oracles (13 files total); judge and runner lists must stay synchronized. This task does not create/update a `.sha256`; the orchestrator performs premeasurement re-freeze. Trial-rehearsal receipt bundle is retained separately and binds actual executable and config; no synthetic/signed pipeline helper is implied.

The Node self-test must run a lightweight injected stage adapter through production prepare/build/oracle/measure functions using fake Node executables only (no Cargo, rustc, Valgrind). It covers A-only rehearsal with no candidate, all stage receipt bindings, binary mutation refusal before launch, wrong manifest rejection, malformed profiles/wrong target count/extra work/recycled warmup count/release inactive rejection, frozen release-layer mismatch refusal, and verdict-free rehearsal. Judge fixtures exercise distinct L1/L2 contracts and observer absence without weakening required S observation. No fixture output is measurement evidence.

`--prepare`, `--build`, `--oracle`, `--measure`, `--rehearsal`, and `--cleanup` accept only stage and output-directory arguments. No cargo, build, measurement, hash-file creation or git writes are authorized by this restoration task.

