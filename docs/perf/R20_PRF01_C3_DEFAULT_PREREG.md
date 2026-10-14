# R20 PRF-01 C‴ default-path cost preregistration

**Status: PREREGISTERED CONTRACT; candidate source absent.** No C‴ candidate commit or candidate Ir exists for this protocol. This document registers the deciding default-path gate; it is not a report of an official run, implementation, or result. The default-path change is eligible to merge only on GO under the deterministic rules below. Native latency/contention remains deferred and is not a GO prerequisite under the owner decision recorded here.

This is adapted from `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md`, retaining the v2 measurement workflow and applicable control-geometry/noise rules while replacing C″/flag assumptions with the owner-directed C‴ default design. It registers the gate; it does not amend v2 or claim implementation of candidate code or future C3-specific harness components.

## Authority and decision

- C‴ design: `docs/reviews/2026-10-13-r20-prf01-remote-free-discovery-flag-design.md`, §§4–6; owner contract `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` §13.
- Open item: `docs/perf/OPEN_ITEMS.md` item 82. P1-3 is the prerequisite strict-trim-from-payload-start change and Gate S. This is P1-4 preregistration. P1-5 supplies the candidate; P1-6 runs this deciding gate. Do not substitute a candidate source or build before the freeze/refreeze protocol.
- Owner decision: if the C‴ price gate passes, the C‴ summary-hint protocol is default behavior with no user flag. `RemoteFreeDiscovery`/`FullScan` flagging is fallback only if the producer-price gate fails; route tags and config surface are not part of this default-path candidate.
- Entry point is `SeferAlloc` as `GlobalAlloc` → routed `HeapCore`. The measured layer is why this gate can decide the shipping default; no direct `AllocCore` substitute. Remote producers free on a joined second thread through `GlobalAlloc::dealloc` → `publish_foreign`.
- C‴ protocol, as proposed (not asserted implemented): summary in dead prefix `pending[0..64)` of the same pinned sidecar; P1 class acquire/reject-zero; P2 terminal `pending[w].fetch_or(..., AcqRel)` with unused result; P3 summary relaxed load and cold Release RMW only if bit clear; P4 drop pin. P2 is the linearization point; producer's sole allowed subsequent metadata access is the summary word in that pinned sidecar. Owner discovery swaps summary words then pending words and consumes all selected cuts. Strict trim, bounded maintenance and OOM rescue use fixed payload scans and never read the prefix. Contract §13 is gated on cost GO; this preregistration does not assert proof or implementation of its correctness obligations.

## Identities and arms

Five arms, 185 rows (37 cells × 5 arms): A1/A2 are identical A binary; B1/B2 identical candidate B binary; C0 is A source under the registered distinct path label/build control. A is `merge-base(candidate, origin/main)` and the run is dispatched only after P1-3 is present. Capture source identities before builds; preserve clean-tree, frozen-hash, binary-pin and per-stage identity binding rules of the v2 workflow. P1-4 harness files are not present in the A base: record the exact immutable A base SHA and separately capture an immutable hash/receipt for the precise preregistered harness overlay before any build. The overlay is restricted to frozen harness/test/oracle inputs; no candidate `src/` drift or unregistered files. C0 is an A-path control, not candidate code. B is the candidate ref/SHA supplied to the dispatch; **the candidate ref/SHA is TBD and must be rejected until preregistration is explicitly re-frozen with that immutable ref and identity**. No reconstructed identity or mutable branch name may substitute.

A1/A2 and B1/B2 each reuse one binary. Full A/B/C0 worktree paths are equal length with one-letter labels. The C0 registered build difference is `-C debuginfo=line-tables-only`; it changes no source. Do not add rehearsal identities to official evidence. Per-stage identity revalidation and manifest/build/oracle binding remain as in v2.

## Noise and control geometry (frozen before data)

For every per-cell raw quantity and every hot-family window, define `T_s = max(10, |A1 − A2|, |C0 − A1|)` on that same quantity. Noise ceiling: each `|A1 − A2|` and `|C0 − A1|` must be ≤ `max(10, floor(A1/200))`, with the absolute difference numerator and A1 quantity denominator. Above ceiling is INCONCLUSIVE; neither ceiling widens a budget or T_s. No threshold may be raised or selected after data.

Hot controls use N/2N/4N with N=64, 2N=128, 4N=256; each arm's linearity is `|(Ir(4N)-Ir(2N)) - 2*(Ir(2N)-Ir(N))| ≤ T_s`. For each family, two-sided slope agreement requires `|s_B-s_A1| ≤ T_s` and `|s_B-s_A2| ≤ T_s`, where `s_X=Ir_X(2N)-Ir_X(N)`. A failed expected-affinity check is INVALID. No 3a non-affinity exception is registered. A pure affine offset is not itself nonlinear; the separate slope bound catches slope shifts.

## Frozen 37-cell inventory

Names below are exact cell names, individually enumerated; no brace expansion or external-inventory substitution defines this inventory. The 13 mimalloc cells are unchanged from v2. Their target/source implementation status is not implied by this list.

1. `multiseg_cold_256k`
2. `seg_cycle_decommit_256k`
3. `growth16_n`
4. `growth16_2n`
5. `growth16_4n`
6. `r20_prf01_small_churn_16b_n`
7. `r20_prf01_small_churn_16b_2n`
8. `r20_prf01_small_churn_16b_4n`
9. `r20_prf01_churn_256b_n`
10. `r20_prf01_churn_256b_2n`
11. `r20_prf01_churn_256b_4n`
12. `r20_prf01_churn_write_256b_n`
13. `r20_prf01_churn_write_256b_2n`
14. `r20_prf01_churn_write_256b_4n`
15. `r20_prf01_aligned_churn_640b_a128_n`
16. `r20_prf01_aligned_churn_640b_a128_2n`
17. `r20_prf01_aligned_churn_640b_a128_4n`
18. `r20_prf01_start_one_alloc_free_16b`
19. `mimalloc_small_churn_16b`
20. `mimalloc_small_churn_16b_2n`
21. `mimalloc_churn_256b`
22. `mimalloc_cold_alloc_free_256x16b`
23. `mimalloc_cold_alloc_free_256x16b_2n`
24. `mimalloc_cold_alloc_free_256x16b_4n`
25. `mimalloc_cold_alloc_only_256x16b`
26. `mimalloc_cold_alloc_only_256x16b_2n`
27. `mimalloc_cold_alloc_only_256x16b_4n`
28. `mimalloc_cold_alloc_free_256x64b`
29. `mimalloc_recycle_alloc_free_256x16b`
30. `mimalloc_recycle_alloc_free_256x64b`
31. `mimalloc_bootstrap_proxy`
32. `r19_prf01_publication`
33. `r19_prf01_joined_producer_same_word`
34. `r20_prf01_c3_producer_distinct_words`
35. `r20_prf01_c3_remote_large_free`
36. `r20_prf01_strict_trim_payload`
37. `r20_prf01_bounded_maintenance`

The support inventory assigns 20 cells to iai (5 benefit + 13 mimalloc + publication + same-word producer), 13 hot/construction cells to controls, two new producer cells to C3, and two trim cells to the trim target. Same-word producer remains the existing `perf_gate_iai` producer-control cell. Future C3 candidate and trim sources are external prerequisites where absent. `r19_sidecar_summary_oracle` retains all eight R19 oracle counters in their established order: `pending_words_visited`, `pending_words_exchanged`, `nonempty_cuts`, `discovery_records_reclaimed`, `routed_scans`, `summary_words_visited`, `summary_hint_rmw`, `summary_hint_loads`. The candidate-only C3 oracle is selected by `cfg(r20_prf01_c3_oracle)`; it supplements, and does not replace, the R19 oracle. Its CLI is `--c3-json <cell> <processId>`. It is an oracle only, never a measurement path; rehearsals are main-only and not measurement or performance evidence.
## Thresholds and required observations

All limits are fixed before candidate data. Any deterministic failure is NO-GO (or INVALID for malformed/unqualified evidence); no automatic GO from unexplained gains.

- Discovery benefit: `multiseg_cold_256k` and `seg_cycle_decommit_256k` B/A ≤ 0.25 (exact integer comparison `4*IrB ≤ IrA`); `growth16` slope of B ≤ 2.5 (exact `2*num ≤ 5*den`, denominator > 0).
- Publication, same regime: `r19_prf01_publication` net B ≤ A.
- Existing same-word producer: `r19_prf01_joined_producer_same_word`, 4096 frees, `(workloadIrB-workloadIrA)/4096 ≤ +6 Ir/free` (budget unchanged). Use the matched child-body `producer_free_window` boundary and its registered producer receipt/control subtraction; do not compute the budget from overall cell Ir totals.
- Distinct-word producer: `r20_prf01_c3_producer_distinct_words`, exactly 4096 distinct remote 1 KiB blocks routed through the canonical index; `(workloadIrB-workloadIrA)/4096 ≤ +10 Ir/free`. Use the matched child-body producer boundary and registered control subtraction, not overall cell Ir totals. The one test-set load and exactly one RMW per newly nonempty pending word must be checked against observed counters; budget is frozen, not re-derived from observed work.
- Remote Large free: exactly `SegmentLayout::SMALL_MAX + 1` bytes, verified to route as Large; absolute `|IrB-IrA| ≤ T_s` (noise-only tolerance).
- Construction `r20_prf01_start_one_alloc_free_16b`: one `SeferAlloc::new()` and exactly one 16-byte alloc/free registration; one-sided net work `IrB-IrA ≤ 0 + T_s`. If gain is less than `-T_s`, retain and require a receipt-backed functional decomposition showing the removed pass; absent evidence is INCOMPLETE, false/malformed evidence INVALID. No hot-family intercept.
- Hot families: all four listed families at N/2N/4N; two-sided slope neutrality and per-arm affinity against the frozen `T_s` as specified above.
- Strict trim and bounded maintenance: 20 remote frees of 32-byte allocations; bounded maintenance budget is one work unit. Each absolute B-vs-A difference ≤ that cell's `T_s`; activation/counters must establish the measured intended fixed-scan behavior. They never read summary prefix.
- `sizeof(SmallSidecar)` delta exactly 0; expected absolute value `41984` in both arms.
- All 13 mimalloc cells: ΔIr = 0 exactly for all cross pairs; `strictTrim` records no cell. The C3 oracle is N/A for mimalloc; the actual mimalloc benchmark remains measured.

Oracle keys are required observations, not prose-only expectations. Common counters: `pending_words_visited`, `pending_words_exchanged`, `nonempty_cuts`, `discovery_records_reclaimed`, `routed_scans`, `summary_words_visited`, `summary_hint_loads`, `summary_hint_rmw`, `strict_trim_remaining`. C3 oracle required fields: `pending_words_visited`, `summary_words_visited`, `summary_hint_loads`, `summary_hint_rmw`, `discovery_records_reclaimed`, `quiescent_summary_missing_bits`, `sizeof_small_sidecar`. `HeapCore::dbg_prf01_c3_oracle(&self) -> [u64; 7]` order: `pending_words_visited`, `summary_words_visited`, `summary_hint_loads`, `summary_hint_rmw`, `discovery_records_reclaimed`, `quiescent_summary_missing_bits`, `sizeof_small_sidecar`; the separate method `dbg_prf01_c3_quiescent_summary_missing_bits(&self)` is the gauge. Capture gauge after producer join and before cleanup. The C3 oracle required fields are candidate-only observations; official R19 runs retain the full eight-counter R19 oracle and do not require C3 measurements. Official R20 B runs require actual candidate counters; rehearsal output validates compatibility only and is never measurement or candidate-performance evidence. In main A/C0/rehearsal B, the four dynamic C3 fields `summary_words_visited`, `summary_hint_loads`, `summary_hint_rmw`, and `quiescent_summary_missing_bits` are null with reasons; `sizeof_small_sidecar` is the actual statically measured size on main. B must show discovery pending-word visits substantially below A with summary words visited > 0. `summary_hint_loads` equals remote frees; `summary_hint_rmw` equals actual newly-set transitions (distinct-word workload expects one per newly nonempty word). In same-word producer, loads equal frees while RMW is one transition per word, not every free. Quiescent check after producer join and before discovery: `quiescent_summary_missing_bits == 0` on B. Publication reclaim count is equal A/B. Strict trim remaining is zero after completion.

The C3 summary counters are absent on A and may be absent/null/zero there, explicitly allowed by the support contract; do not fabricate A-side dynamic samples for absent instrumentation. B counters must come from actual candidate-source instrumentation and the candidate oracle, never injected constants/source-derived zeroes. R19's old `summary_load` expectation is not inherited: keys are `summary_hint_loads` and `summary_hint_rmw` as defined in C3 support. The exact existing hot-helper implementation must be used; do not substitute an equivalent or broaden the measured path.

Discovery oracle also requires the v2 invariants where applicable: B `pending_words_visited` far below A; B `summary_words_visited > 0`; `routed_scans > 0` and equal over comparable arms; in pure growth `nonempty_cuts` and `discovery_records_reclaimed` are zero; count monotonicity `nonempty_cuts ≤ pending_words_exchanged ≤ pending_words_visited`. A baseline pending counts retain v2 registered values/tolerance only where the exact same cells and instrumentation apply; no invented numeric acceptance value for newly introduced cells. P1-5 must establish the candidate actual-source proof/activation obligations, not merely emulate the counters. Correctness obligations include the item-82 actual-type Loom refinement and negative controls, Miri paused-producer lifetime check, and quiescent oracle; these are implementation/acceptance prerequisites, not claimed completed by this cost gate.

## Workflow, receipts, freeze, and verdict

Reuse the v2 staged workflow and artifact schema. Retain the common input fields and per-row `name`, `arm`, `config`, `oracle`, identity/linkage and binary pins; preserve source/build/oracle/measure receipts, manifest/build/oracle bindings, per-process IDs, raw logs/profiles, producer boundary receipts, receipt hashes, redaction, final-input-derived summary, and post-judge acceptance verifier. Extend only for C3 fields through the separate C3 driver; do not imply the v2 judge accepts the 37-cell inventory unchanged. Explicit owner dispatch; prepare/build/oracle/measure, no official rehearsal or slop. A rehearsal uses main only (no candidate): it validates judge/workflow compatibility and can never establish candidate performance or GO evidence. A construction gain receipt must identify retained profile evidence and decomposition. Before input is written and no candidate Ir exists, dispatch failure does not consume the gate; once official input exists, result stands. No run is official until identity, receipt validation and preregistration freeze all pass.

Freeze the exact `FROZEN_FILES` list from `scripts/r20_prf01_c3_support.mjs` (25 paths): `docs/perf/R20_PRF01_C3_DEFAULT_PREREG.md`, `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`, `docs/reviews/2026-10-13-r20-prf01-remote-free-discovery-flag-design.md`, `docs/perf/R20_CONTROL_GEOMETRY_RULE.md`, `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md`, `Cargo.toml`, `benches/r20_prf01_c3_cells_iai.rs`, `benches/r20_prf01_controls_iai.rs`, `benches/r20_prf01_trim_iai.rs`, `benches/support/r20_prf01_controls.rs`, `benches/support/r19_prf01.rs`, `benches/perf_gate_iai.rs`, `scripts/r20_prf01_c3_gate.mjs`, `scripts/r20_prf01_c3_run.mjs`, `scripts/r20_prf01_c3_verify_artifacts.mjs`, `scripts/r20_prf01_sidecar_summary_gate.mjs`, `scripts/r20_prf01_sidecar_summary_run.mjs`, `scripts/r20_prf01_verify_artifacts.mjs`, `scripts/r20_prf01_c3_support.mjs`, `scripts/r20_prf01_c3_fixture.mjs`, `scripts/r20_prf01_c3_runner_support.mjs`, `tests/r19_prf01_sidecar_oracle.rs`, `examples/r19_prf01_oracle.rs`, `examples/r20_prf01_c3_oracle.rs`, `tests/r9_bounded_background_maintenance.rs`. C3 candidate/trim sources absent from this worktree are external prerequisites; the support module explicitly permits the trim source to be missing pending prerequisite. The exact listed paths and A base identity plus the restricted harness-overlay identity must be frozen before any candidate build/measurement; candidate SHA is TBD and requires explicit pre-measurement refreeze. No companion `.sha256` is created by this task. Any later change requires pre-measurement re-freeze.

Native Mode 2 latency/contention is DEFERRED with exact method description: fixed remote-free batch on a quiet bare-metal host; per-process p50/p99 batch times divided by batch size. With every deterministic axis passing and native deferred, the owner-approved verdict is **GO (deterministic; native deferred)** and merge of the default-path change is permitted. This is intentionally different from v2's `INCONCLUSIVE (native deferred)` behavior. Any deterministic failure remains NO-GO/INVALID; incomplete rows/receipts are INCOMPLETE. Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO. No rehearsal verdict is gate evidence.

## Re-freeze log

No C3 candidate build or official measurement exists under this preregistration. Re-freeze entries begin empty. Any rehearsal runs are harness validation only and are never gate data; they cannot establish GO/NO-GO or candidate performance. Append only dated, pre-measurement amendments and official dispatch records; do not describe troubleshooting as partial official measurement. Runtime candidate source remains absent until P1-5 supplies it.

## Not covered

Native latency/contention and multi-producer fan-in beyond deterministic Ir; RSS, cache footprint, wall-clock latency; completion of the correctness proof/acceptance obligations. This document makes no claim that candidate C‴ is implemented or that the gated contract has taken effect.

- **2026-10-14 — pre-measurement re-freeze 1 (P1-6 dispatch binding).** B candidate = `1c99c81501d614cf6e5576947d339e0b2f9e195d` (draft PR #29 head: the P1-5 C‴ candidate merged with `main`). A = `merge-base(B, origin/main)` = `02806ba73e0716d0989844c41ea7a3300c51570d` (`main` with P1-3 and this harness, PR #27). The runner binds B through the frozen `CANDIDATE` constant in `scripts/r20_prf01_c3_support.mjs` (no dispatch argument), and `docs/perf/R20_PRF01_C3_DEFAULT_PREREG.sha256` freezes the 25 `FROZEN_FILES` at this commit. No candidate build or measurement precedes this entry; rehearsals 1–16 were main-only harness validation.
