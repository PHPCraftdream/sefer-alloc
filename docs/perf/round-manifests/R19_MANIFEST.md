# Round 19 manifest — owner-decision work, performance gates PRF-01..04 and verification infrastructure

**Scope:** Round 19 starts after `31db7f17698459e9ce26c3f8701c314304b9c00b` (the commit that added the R18 manifest) and ends at the commit before the R19 closeout that adds this file. The commit table is derived by this command; no SHA is transcribed or inferred:

```sh
git log --reverse --format="%H|%cI|%s" 31db7f17698459e9ce26c3f8701c314304b9c00b..94c6d4e95d0b5a02e4f10cc3c35b9c0baf1e6916
```

70 commits. The closeout commit that records this file is intentionally outside the range (no self-reference).

## §1. Commits and classification

| # | SHA | Commit time (`%cI`) | Prefix (R30-12) | Subject | Category and R19 effect |
|---:|---|---|---|---|---|
| 1 | `97846068917ffff592656a61a9048bb6168527e5` | `2026-10-11T08:56:06+02:00` | `docs` | docs: record R18 solution research reports and the decision summary | docs |
| 2 | `fa5a86e7855225c6cbd7add293923118d1081cf1` | `2026-10-11T09:00:49+02:00` | `test` | test: bind the stats snapshot only where the assertions use it (r3a compiled-out test) | test |
| 3 | `782082ab48337b933be7e85ac6fd12f6e6c29dda` | `2026-10-11T09:06:44+02:00` | `docs` | docs: record the round-17 performance findings in the perf open-items index (items 85-88) | docs |
| 4 | `091ca50c616befbc532b5964caf1458784be0895` | `2026-10-11T09:36:03+02:00` | `ci` | ci: keep the weekly feature sweeps running and summarize failures | ci |
| 5 | `04a8b242103a441eab7840f58996036f84528fef` | `2026-10-11T09:45:12+02:00` | `docs` | docs: record the arbitrated owner-question decisions for the round-19 work | docs |
| 6 | `39ae36008db1eb363b793ee791ca83db0a902408` | `2026-10-11T09:57:04+02:00` | `checkpoint` | checkpoint: 2026-10-11-0955 | docs — session checkpoint |
| 7 | `52e806ea121e074e37f36702146651195fd68f7a` | `2026-10-11T10:02:28+02:00` | `ci` | ci: symbolize the fuzz job and suppress the registry route-array leak report | ci |
| 8 | `aa94ca1ea903b2495cd8a0712e89907b0967e9dc` | `2026-10-11T10:08:09+02:00` | `fix` | fix: abort on detected Small free-list corruption under hardened | fix — SHIPPED `fix`: hardened free-list corruption aborts; production does not validate links, so no production change |
| 9 | `2cb47cea6b61742645904a5247eeb9950867c804` | `2026-10-11T10:13:05+02:00` | `docs` | docs: record the fuzz LSan, production free-list validation and old abort-site cards (items 200-202) | docs |
| 10 | `c40c7adfd52387b90bef714d2dff7ef8e0a49784` | `2026-10-11T10:20:26+02:00` | `fix(perf)` | fix(perf): reject interior own-thread Large frees and reallocs | fix(perf) — SHIPPED `fix(perf)`: interior own-thread Large free/realloc rejected; Ir not measured, no speedup or neutrality claimed |
| 11 | `fb783d16eb2027d31ff3137f2d09ecbe7f840d27` | `2026-10-11T10:29:12+02:00` | `fix(perf)` | fix(perf): drop decay debt once a batch reaches headroom and reset it on full trim | fix(perf) — SHIPPED `fix(perf)`: large-cache decay debt dropped at headroom and reset by full trim; no speed claim |
| 12 | `042e88fb67ab48dd6bef7cf8bc416d8b0103ea54` | `2026-10-11T10:30:04+02:00` | `docs` | docs: sync the integration test file count after the SEC-03 and decay tests | docs |
| 13 | `19536b1925247cf598c837a1b2bf3dfbd14a01ea` | `2026-10-11T10:44:12+02:00` | `test(sidecar)` | test(sidecar): pin ClassLeaves publication and exercise the real spill/read race | test |
| 14 | `4146b7fd82a44ad5a8f272aeb1235961064fd873` | `2026-10-11T10:44:28+02:00` | `docs` | docs: sync the integration test file count after the ClassLeaves Miri test | docs |
| 15 | `5a6cfc3da0e482ae96689144cb54cca6cf9835bc` | `2026-10-11T10:54:15+02:00` | `test(global)` | test(global): Windows exit stranded-lock subprocess oracle (CON-01 phase A) | test |
| 16 | `fbbaeaae588904be04978d5e922ad5f1b8baf36b` | `2026-10-11T10:55:17+02:00` | `docs` | docs: sync the integration test file count after the CON-01 oracle tests | docs |
| 17 | `6a532896f6f649f3f8695d62b146fd613a20bb5e` | `2026-10-11T11:26:14+02:00` | `build(scripts)` | build(scripts): source-scope invalidation for evidence registry rows | build |
| 18 | `e959923ae498e745a9b909f7601c7ae5e2dce895` | `2026-10-11T11:58:27+02:00` | `test(registry)` | test(registry): bound RouteSlots arrays per registry slot (CI-2b) | test |
| 19 | `12057d98c0a45dccba0421570ed285ce392e4fa0` | `2026-10-11T11:59:26+02:00` | `docs` | docs: sync the integration test file count after the RouteSlots bound test | docs |
| 20 | `870ea4af6e6660e18c21388f82c67159101db2c2` | `2026-10-11T12:00:11+02:00` | `docs` | docs: update item 200 after the RouteSlots bound test landed | docs |
| 21 | `ccd4f1c06c635366a21eb946394f35edbc7d3aa1` | `2026-10-11T12:12:33+02:00` | `fix(global)` | fix(global): avoid stranded allocator waits during Windows process exit | fix — SHIPPED `fix(global)`: Windows process-exit waits give up; other targets unchanged |
| 22 | `9748f6c4e3815080793dff8a8e238b3e5db3b4e9` | `2026-10-11T12:20:27+02:00` | `docs(ci)` | docs(ci): record the Windows process-exit contract and run its tests on windows-latest | docs |
| 23 | `bfcbbfee3049b3359e3021436e21b6db719d5ddf` | `2026-10-11T12:29:02+02:00` | `docs(config)` | docs(config): cross-reference NUMA overrides and repair manifest file references | docs(config) |
| 24 | `f503a017023bb2f9c1d69b6b2e43dd12ac963aca` | `2026-10-11T12:34:31+02:00` | `test` | test: make the promoted-large batch-free test compile and record why no CI row builds it | test |
| 25 | `534f8c92fe3c02080149d74b3ec3c4505b02a8f0` | `2026-10-11T12:45:45+02:00` | `checkpoint` | checkpoint: 2026-10-11-1243 | docs — session checkpoint |
| 26 | `0a73ec397d3de28c6e8d120fd15ed260316a5035` | `2026-10-11T13:32:01+02:00` | `test(loom)` | test(loom): model the sidecar summary word (PRF-01 C-prime) | test |
| 27 | `363b7915b666a6b463d7e6162f644222ade4eab9` | `2026-10-11T13:32:01+02:00` | `docs` | docs: relax the last-access rule for the discovery summary | docs |
| 28 | `a7b5bc671aa65c91136f14465c08a1660fe1679d` | `2026-10-11T13:40:32+02:00` | `bench` | bench: PRF-04 registry claim wave harness and arm-A retention measurement | bench — judge/probe/gate report or harness only |
| 29 | `028bdde54e749b8eca9f5be4d9e876667ce12b4c` | `2026-10-11T14:17:51+02:00` | `build(scripts)` | build(scripts): drive Miri runs from one target manifest and verify its CI wiring | build |
| 30 | `1f9187e564bf184a1e572a9de80d36dcf7407844` | `2026-10-11T14:17:51+02:00` | `test(miri)` | test(miri): align the Miri regressions with the manifest and the remote-free contract | test |
| 31 | `491df5cddea7e12c8a30d581f4b27ea6dbe3f84a` | `2026-10-11T14:17:51+02:00` | `ci(miri)` | ci(miri): run the manifest GREEN targets and the static wiring check | ci |
| 32 | `d9ff73a41d6aa8af9b8e45e2be3c8a0c92777816` | `2026-10-11T14:21:47+02:00` | `docs(miri)` | docs(miri): describe the Miri target manifest | docs |
| 33 | `fc5ff7c9a32f01b6a0a0b1483fc31ad733d23d2e` | `2026-10-11T14:26:33+02:00` | `ci` | ci: raise the sentinel floor to 126 for the summary Loom model | ci |
| 34 | `6b5b06188f32d056bfe5d3c63168bf20d8bb684b` | `2026-10-11T15:30:32+02:00` | `test` | test: keep each magazine-bounds test on a fresh heap | test |
| 35 | `c4d4909778ee32690e500e67e35fadadbb4a9e54` | `2026-10-11T15:33:04+02:00` | `bench` | bench: PRF-02 own-free single-resolve gate, re-registration and rejected C-prime | bench — judge/probe/gate report or harness only |
| 36 | `7ce23bfc76bcee24fe23671b42c9468c66caae68` | `2026-10-11T15:33:04+02:00` | `perf(runtime)` | perf(runtime): resolve own-free roots once through OwnedBlock | perf(runtime) — SHIPPED `perf(runtime)`: own-free root resolved once (alloc-global + fastbin only); deterministic Ir −17 of 69 (push) and −14 of 84 per pair (churn); not wall-clock |
| 37 | `e0858d081b7aa39468f0b5a427ccd2b4eef80345` | `2026-10-11T16:03:39+02:00` | `bench` | bench: attribute the Large cache route share (PRF-03 step 1) | bench — judge/probe/gate report or harness only |
| 38 | `58a8fb88f4dd3471d18319f51cb5f45caadd2d44` | `2026-10-11T16:16:19+02:00` | `checkpoint` | checkpoint: 2026-10-11-1614 | docs — session checkpoint |
| 39 | `4cea763be826145149b814c7c79987516cc55801` | `2026-10-11T16:26:46+02:00` | `test(kani)` | test(kani): validate Large word transitions despite loose CAS result model | test |
| 40 | `b65e199dd93df8ce87208aa75bb836a6936f9483` | `2026-10-11T16:26:47+02:00` | `ci` | ci: drop duplicated Kani subset workflow | ci |
| 41 | `5dbf7a596ed7d4cb3b1ab4c54e35df5838afa5f4` | `2026-10-11T18:33:53+02:00` | `bench` | bench: PRF-04 B-vs-A thread-wave gate harness and preregistration | bench — judge/probe/gate report or harness only |
| 42 | `00b260025110d5ced0cbe10fcacb446e4fc8e949` | `2026-10-11T18:41:22+02:00` | `test` | test: realign two pin tests with the process-exit return and the single canonical lookup | test |
| 43 | `0895712a6fa2ebb346121aad4e07867be4e72cbb` | `2026-10-11T18:59:41+02:00` | `fix` | fix: gate OwnedBlock on alloc-global so the numa-aware clippy row is clean again | fix — `fix`: feature-gate repair for the `numa-aware` clippy row |
| 44 | `ba9fad18393cbbc82e95678497ae5d1083ee9bd2` | `2026-10-11T19:00:36+02:00` | `bench` | bench: preregister the Large route retain gate (PRF-03) | bench — judge/probe/gate report or harness only |
| 45 | `175e7398db175569a65a7c77ff20511bec8e592b` | `2026-10-11T19:31:49+02:00` | `bench` | bench: let the PRF-04 gate runner create the binary copy directory in a fresh worktree | bench — judge/probe/gate report or harness only |
| 46 | `a252a872046247dc26409f3e88a9f314487bfaf9` | `2026-10-11T20:16:27+02:00` | `bench` | bench: PRF-04 B-vs-A gate run01, benefit met and KILL-1 inconclusive on a loaded host | bench — judge/probe/gate report or harness only |
| 47 | `90b01b19b82f3b9d74dc3c235fc14282b9751a71` | `2026-10-11T20:43:24+02:00` | `checkpoint` | checkpoint: 2026-10-11-2042 | docs — session checkpoint |
| 48 | `09084fc3645c196aa48ea3c26e6428f45019f571` | `2026-10-11T21:23:40+02:00` | `bench` | bench: PRF-03 preregistration amendment A2, before any measured build | bench — judge/probe/gate report or harness only |
| 49 | `1d36b44a15c3f30230098b5f76aee5258c8bc209` | `2026-10-11T21:25:33+02:00` | `docs` | docs: record the arbiter's PRF-03 runner design decisions behind amendment A2 | docs |
| 50 | `6904dced074b38ef15e00249742d54330b1e9e40` | `2026-10-11T23:08:08+02:00` | `bench` | bench: PRF-03 retained-route gate runner, R2/R3 iai cells, oracle and native MT twin | bench — judge/probe/gate report or harness only |
| 51 | `df5b80434a886f0fcf403fc2827dbee45119627a` | `2026-10-11T23:44:47+02:00` | `bench` | bench: preregister the sidecar summary discovery gate (PRF-01); add growth16 and scan oracle counters | bench — judge/probe/gate report or harness only |
| 52 | `3b741caa1c6e6e9243d8a52e99501fc7deafb5c4` | `2026-10-12T00:10:22+02:00` | `bench` | bench: PRF-03 B-vs-A gate run01, R1 benefit met, R3 and control criteria failed, native axis inconclusive | bench — judge/probe/gate report or harness only |
| 53 | `7e7da281c2e378e1b32b04b3caa4cac0eed8eaf8` | `2026-10-12T00:19:13+02:00` | `bench` | bench: PRF-03 amendment A3 (harness confounder removal) and the reviewer decision after run01 | bench — judge/probe/gate report or harness only |
| 54 | `1ec93b3c2900bce0de59836964d83d382ab7ef31` | `2026-10-12T02:51:18+02:00` | `bench` | bench: PRF-01 preregistration Amendment A for hot-control geometry (before any gate build) | bench — judge/probe/gate report or harness only |
| 55 | `bba6a2d038bc855829c2da66710a2aee49e1cf09` | `2026-10-12T03:13:11+02:00` | `bench` | bench: PRF-03 B2-vs-A gate run02, path-shape confounder removed, R3 and the two-sided control rule still failed | bench — judge/probe/gate report or harness only |
| 56 | `e46b3d39408806f025b1c58925fdb4bbba728faa` | `2026-10-12T03:59:41+02:00` | `bench` | bench: PRF-04 B-vs-A gate run02, benefit reproduced and KILL-1 fired on a noise-dominated axis | bench — judge/probe/gate report or harness only |
| 57 | `ce6a9e69c88b2cd1d09c53052e9e4cf9d4b5e3a1` | `2026-10-12T04:14:49+02:00` | `bench` | bench: PRF-01 preregistration Amendment B for the producer boundary and the empty control (before any gate build) | bench — judge/probe/gate report or harness only |
| 58 | `2a0671d7030bbf9d4c555fad107b964262948c12` | `2026-10-12T05:38:13+02:00` | `bench` | bench: PRF-01 gate harness under Amendment B (producer anchor, oracle checkpoints, native fan-in twin, collecting runner) | bench — judge/probe/gate report or harness only |
| 59 | `0fffe021ae3667b6010f6fe3c1990b7c033c4317` | `2026-10-12T05:40:02+02:00` | `bench` | bench: PRF-01 A-only producer preflight b03, Amendment B coverage is achievable (diagnostic only, one Callgrind launch consumed) | bench — judge/probe/gate report or harness only |
| 60 | `801de6b6eb8bc0d53995ba8b87e86c325b9463c9` | `2026-10-12T06:59:35+02:00` | `bench` | bench: PRF-01 runner aggregation re-reads the runner-source JSON from the hash-chained private raw stdout | bench — judge/probe/gate report or harness only |
| 61 | `329b1b3c8778846b7eb6e1dc2b0385df3ce308a0` | `2026-10-12T06:59:35+02:00` | `bench` | bench: PRF-01 C-prime gate run01, benefit met but hot controls (+569 Ir vs T=10) and producer (+9.03 Ir per free vs +6) fail; machine verdict INCOMPLETE | bench — judge/probe/gate report or harness only |
| 62 | `0a5dc0365e6ffdeed562af7aefa8b713b1e6baf7` | `2026-10-12T07:20:32+02:00` | `fix` | fix: keep segment_table_impl.rs under the 1000-line cap by moving the bench-only route-slot observers | fix |
| 63 | `fa3bef7cbfa525233f80ce6e9edb93ba8005e907` | `2026-10-12T08:06:15+02:00` | `test` | test: let the CON-01 lock-count oracle account for exact-object-proto under --all-features | test |
| 64 | `fa9c76a8727a5dea763283c571e86e878f9f2e87` | `2026-10-12T08:11:55+02:00` | `test` | test: gate the PRF-02R realloc-tail test on the real promotion predicate | test |
| 65 | `82ccf3e33feffc419aaf573ed67ee164780194cd` | `2026-10-12T08:12:40+02:00` | `build` | build: rustfmt the CON-01 lock-count oracle edit | build |
| 66 | `8d769e93deab7807b9df2536ee9f3827453a0cee` | `2026-10-12T08:23:40+02:00` | `checkpoint` | checkpoint: 2026-10-12-0820 | docs — session checkpoint |
| 67 | `ac6089f7f16b40b659b1c25486484d63160fc603` | `2026-10-12T09:37:40+02:00` | `fix(global)` | fix(global): resolve the Windows shutdown query once so lock-wait loops stop entering the loader | fix — SHIPPED `fix(global)`: Windows shutdown query resolved once and cached (Windows only); fixes TLS destructors skipped at exit when a waiter died inside the loader lookup; no speed claim |
| 68 | `3f44fa0f43d2498542d109e80ef66c5e1bb0cf83` | `2026-10-12T11:06:30+02:00` | `test` | test: keep the PRF-02R promotion note outside the cfg attribute the gate verifier tokenizes | test |
| 69 | `02fef6bd2b245ccc660121aad2f8855a92766c61` | `2026-10-12T12:44:01+02:00` | `build(scripts)` | build(scripts): let the evidence judge take --source-scope so selftest fixtures ignore the repository scope file | build |
| 70 | `94c6d4e95d0b5a02e4f10cc3c35b9c0baf1e6916` | `2026-10-12T12:44:17+02:00` | `docs(evidence)` | docs(evidence): re-verify the nine rows made stale by the R19 test edits and scope them | docs |

Category counts (derived from the prefixes above): bench 20, build 4, ci 5, docs 19, docs(config) 1, fix 5, fix(perf) 2, perf(runtime) 1, test 13.

## §2. Net `production` default-feature impact

- **`production` feature composition: unchanged** — the six-entry `production` list in `Cargo.toml` is byte-identical at `31db7f17` and at `94c6d4e9`; `Cargo.lock` is unchanged; no dependency or project version was bumped; no new Cargo feature was added (only `check-cfg` entries for the measurement cfgs `r19_sidecar_summary_oracle` and `r19_prf01_producer_control`).
- **Runtime-changing commits in the range:** `7ce23bfc` (`perf(runtime)`, own-free `OwnedBlock`, `alloc-global` + `fastbin` builds), `fb783d16` and `c40c7adf` (`fix(perf)`), `aa94ca1e` (`fix`, `hardened` only), `ccd4f1c0` and `ac6089f7` (`fix(global)`, Windows process exit only). Everything else is bench, test, docs, ci or build-script work.
- No opt-in or measurement-only result is framed as a default speedup: the four PRF candidates other than PRF-02 were rejected and are **not** merged.

## §3. Measured deltas this round

| Item | Measurement | Result |
|---|---|---|
| PRF-02 own-free single resolve (`7ce23bfc`) | deterministic iai on `HeapCore`, two identical repeats per arm (`docs/perf/R19_PRF02R_REGATE.md`) | production push 69 → 52 Ir (−17 of 69), churn 84 → 70 Ir per pair (−14 of 84); mimalloc control delta 0; wall-clock not measured |
| PRF-03 retained Large route (candidates B, B2) | deterministic iai + native fan-in, two runs | R1 benefit met (−435 and −493 Ir) but R3 changed-payload (+151, +119 vs T = 12, 10) and the control rule failed; native axis INCONCLUSIVE |
| PRF-04 registry claim wave | barrier-wave commit charge, two runs | benefit reproduced (−99.2 % idle commit charge, 5420187648 B → 45465600 B) but KILL-1 fired on a noise-dominated axis |
| PRF-01 sidecar summary C′ | one full run, 24 cells × 5 labels + 108 native pairs | benefit met (0.113, 0.090, slope 2.144) but hot controls +569 Ir vs T = 10 and producer +9.03125 Ir per free vs ≤ 6 failed; judge machine verdict INCOMPLETE |

## §4. Final verdict per item

| Item | Verdict | Evidence |
|---|---|---|
| PRF-02 / perf item 85 | **SHIPPED** | `7ce23bfc`; `docs/perf/R19_PRF02R_REGATE.md` |
| PRF-03 / perf item 86 | **NO-GO** (both candidates; branches kept, not merged) | `docs/perf/R19_PRF03_B_VS_A_GATE.md`, `R19_PRF03_B2_VS_A_GATE.md` |
| PRF-04 / perf item 87 | **NO-GO** (V4 kept, not merged) | `docs/perf/R19_PRF04_B_VS_A_GATE_RUN02.md` |
| PRF-01 / perf item 82 | **NO-GO** by the frozen Ir criteria; machine verdict INCOMPLETE | `docs/perf/R19_PRF01_C_PRIME_GATE_RUN01.md`, `R19_PRF01_PRODUCER_PREFLIGHT_B03.md` |
| LOG-01 (large-cache decay debt) | **SHIPPED** `fix(perf)`, no speed claim | `fb783d16` |
| SEC-03 (interior Large free/realloc) | **SHIPPED** `fix(perf)`, Ir not measured | `c40c7adf` |
| Hardened free-list corruption | **SHIPPED** `fix` (hardened only) | `aa94ca1e` |
| CON-01 Windows process exit | **SHIPPED** `fix(global)` + subprocess oracle tests + CI row | `5a6cfc3d`, `ccd4f1c0`, `9748f6c4` |
| Verification/CI infrastructure (Miri manifest, Kani, Loom summary model, evidence scope, RouteSlots bound, weekly sweeps) | **INFRASTRUCTURE** | commits tagged `test`/`ci`/`build` above |
| Open HUMAN questions | PRF-03 (new R3/control protocol), PRF-04 (new KILL-1 estimator), PRF-01 (redesign vs new owner budgets) | the three NO-GO cards in `docs/perf/OPEN_ITEMS.md` |

## §5. Raw artifacts committed this round

- `docs/perf/_raw_*.log` files added in the range: **60 files, 366541 bytes** (largest 12967 bytes; every file is below the 200 KiB tier-1 ceiling).
- Retained gate directories are cited by hash and not committed when large (PRF-01 run01: about 122 MB, `docs/perf/R19_PRF01_C_PRIME_RUN01/MANIFEST.txt`). Compressed judge inputs are committed per gate (`input.json.gz`).

## §6. Verification at closeout

- `npm run check` (68 steps: rustfmt, 7 generated clippy rows, 8 test steps, aligned-vmem rows, feature-matrix rows, the node verifiers, strict evidence registry with its selftest, and `iai` with 106 benches producing Ir): **ALL GREEN** on commit `94c6d4e9` plus the uncommitted closeout docs (CHANGELOG, correctness index cards, this manifest). Sixth full run; the log is a local temp file and is not committed.
- What the earlier runs found and the round fixed in place before this one: run 1 — the 1000-line cap (`0a5dc036`), the CON-01 lock-count oracle under `exact-object-proto` (`fa3bef7c`), the PRF-02R test gate (`fa9c76a8`, `82ccf3e3`); run 4 — a 5% flake of `con01_exit_stranded_lock` whose cause was the Windows shutdown query re-entering the loader (`ac6089f7`; counterfactual 141 of 400 lost destructors before, 0 of 600 after); run 5 — a comment inside a `#![cfg]` that the gate verifier tokenizes (`3f44fa0f`) and nine evidence rows made stale by this round's own test edits (`02fef6bd`, `94c6d4e9`: Loom 7/7, Miri x4, two pin baselines, three caught mutants).
- Strict evidence registry with the gate's baseline (`31db7f17`): exit 0, verdict INCOMPLETE (unpublished spike rows and pending required cells, unchanged from R18). Registry selftest: 107 of 107.
- Not run locally and therefore **unconfirmed until CI runs after a push**: miri, loom, TSan, multi-arch, no_std, MSRV, the `windows-latest` CON-01 row. Nothing has been pushed. Correctness cards 107, 188, 190, 195 stay FIXED IN TREE, not resolved, until that CI run exists.
