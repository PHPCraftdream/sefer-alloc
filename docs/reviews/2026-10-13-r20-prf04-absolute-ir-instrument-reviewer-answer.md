# R20 PRF-04: reviewer answer on an absolute-Ir instrument for the selector (2026-10-13)

Asked of the rush reviewer role (read-only, prompt `.rush/stdin/review-abs-ir-instrument.prompt`, session
`review-abs-ir-instrument`) in answer to the open owner question "does the 5 % relative budget need an absolute
instrument?". The run ended with a stream error after section 5 had started (`exit_reason: error`); the text below
is the complete answer the session produced, verbatim, including where it stops.

---

## Verdict

**Yes—add an absolute-Ir guard, but use option (3): a two-part budget on the existing, real claim/release cells. Do not make a selector-only benchmark or function-level attribution the promotion gate.**

**Recommended policy: retain the one-sided 5% check and additionally require zero planned instruction tax beyond the registered deterministic-noise tolerance.** This is deliberately stricter than “allow a few tens of Ir.” The files provide no candidate-independent derivation of an acceptable 20–40 Ir tax; inventing one now would be a policy choice, not a consequence of determinism.

The existing run01 remains valid under its original preregistration. It does **not** establish compliance with this stronger policy. Applied descriptively to retained results, the recommended absolute guard would reject V4′ on cold and recycled windows—not invalidate the historical GO.

## 1. Why this instrument, rather than isolating the selector?

### What actually changes

**OBSERVED:** V4′ is not merely a replacement selection function. At commit `6951a9d0`, it adds:

- A bounded bitmap with `take`, `forget`, and `publish`: `src/registry/bootstrap/free_slot_hint.rs:39–89`.
- A free-set lookup between the reuse hint and saturation/bump checks: `src/registry/heap_registry/stack.rs:121–133`.
- Bitmap clearing **after the ownership CAS**, outside `pick_slot`: `src/registry/heap_registry/claim.rs:268–278`.
- Bitmap publication on ordinary lease release, maintenance release, and constructor-OOM return: `src/registry/heap_registry/claim.rs:496–499`, `:601–605`, `:662–667`.

These references are to the candidate revision. The reviewed `5d00d3ef..6951a9d0` diff contains these changes.

**INFERRED:** A selector-only check could pass while the shipped feature becomes more expensive through `forget` or `publish`. Those are part of the mechanism’s price, not incidental construction overhead. A claim+release slope is therefore a better primary cost guard than `pick_with_saturation` alone.

### Comparison and decision

| Instrument | Recommendation |
|---|---|
| **Selector-only iai cell** | **Not the promotion gate.** Useful later for diagnosing specific branches, but incomplete for this change. |
| **Function-level inclusive attribution** | **Diagnostic only.** Retained evidence does not establish a separately attributable selector or a complete matched A/B decomposition. |
| **Relative plus absolute whole-path bound** | **Adopt.** Measures the real entry point, includes ownership and release bookkeeping, needs no runtime refactor, and already has qualified geometry. |
| **Do nothing** | **Reject.** The existing budget deliberately permits a much larger tax than the owner’s excellence goal requires. |

**OBSERVED:** The actual measured entry point already matches the registry operation used by TLS binding, with the wrapper excluded explicitly: `docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md:24–45`. The repository requires both the exact shipping layer and per-arm activation evidence: `CLAUDE.md:855–859`, `:697–704`.

**INFERRED:** Keep that entry point. There is no need to shrink the measurement body to shrink the **budget**. A large deterministic total can resolve a small integer difference; the 161-Ir limitation is an acceptance-policy limitation, not an instruction-counter resolution limit.

### What a selector-only cell would really need

A defensible diagnostic cell would have to call the actual `HeapRegistry::pick_slot`, using the actual global registry and production callbacks—not the parameterized selector with dummy `reuse`, `scan`, or `bump` closures. The existing production connection is visible at `src/registry/heap_registry/claim.rs:391–413`; the parameterized model surface is at `src/registry/heap_registry/stack.rs:105–139`.

Its prebuilt states would need separate cases for valid hint, empty free set followed by bump, and multiple FREE slots with an exhausted hint. Construction would require a verified setup/toggle boundary or identical fixed setup cancelled by registered slopes. Repeated selection consumes hints and bitmap bits, so “call repeatedly on a fixed state” is not a stationary workload unless restoration is measured or independently excluded.

**INFERRED:** That is additional instrumentation and qualification work, while still omitting the post-CAS clearing and release publication. Refactoring production to make it independently callable is acceptable in principle, but unnecessary for this decision.

## 2. Concrete specification

### Entry point, cells, and controls

**Recommended specification:** retain all ten existing cells, unchanged:

| Family | Definition | Registered geometry |
|---|---|---|
| Start | Bootstrap plus one EMPTY first claim | Direct start cell |
| Cold | Claim and forget 16/32/64 leases | Existing scenario windows |
| Recycled | One fixed warmup claim+drop, then 16/32/64 claim+release pairs | N→2N slope plus 4N linearity qualification |
| Wave | Fixed warmup of 64 claims and releases, then 16/32/64 claims retained LIVE | Existing scenario windows |

Bodies: `benches/support/r20_prf04.rs:48–108`; iai entry functions: `benches/r20_prf04_claim_iai.rs:18–57`.

Keep:

- A1/A2 from one A binary, B1/B2 from one B binary.
- Same-source, distinct-binary C0.
- Separate counter-enabled oracle for every arm/family/size.
- `production internals` for measured cells, without `bench-internals`.

These controls are registered at `docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md:104–116`, `:169–186`, `:41–45`. No mimalloc analogue should be invented.

### Windows and construction treatment

Retain exactly:

- Cold and wave: `w0 = Ir(16)`, `w1 = Ir(32) − Ir(16)`, `w2 = Ir(64) − Ir(32)`.
- Recycled: `w = Ir(32) − Ir(16)`; require the existing 4N linearity test.
- Start: direct measured total, never an inferred intercept.

**OBSERVED:** This geometry is already registered at `docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md:122–130`. Cold non-affinity has retained A-only characterization; recycled is affine (`:53–64`).

**Important limitation:** `w0` contains fixed setup; it is not a pure per-operation observation. Recycled setup cancels in its slope. Construction remains independently judged by the direct start cell, as required by `docs/perf/R20_CONTROL_GEOMETRY_RULE.md:36–58`.

### Absolute budget

For every registered window and every B/A comparison, require **both**:

\[
20(w_B-w_A)\le w_A
\]

and

\[
w_B-w_A\le L_wP_F+T_s(w).
\]

Here:

- \(L_w\) is the registered window length: 16 or 32 operations.
- \(P_F=0\) Ir per operation: **no planned positive tax**.
- \(T_s(w)=\max(10,|w_{A1}-w_{A2}|,|w_{C0}-w_{A1}|)\), preserving the existing definition.
- Add an absolute qualification ceiling: both baseline differences must be **≤10 Ir per window**. Otherwise **INCONCLUSIVE**, not a larger budget.

Thus qualified measurements have an absolute window budget of **10 Ir**. The existing relative noise ceiling remains an additional requirement, not a replacement.

For construction, retain **`budget_K = T_s(start)`**, with the same new absolute qualification ceiling of 10 Ir. Do not derive construction from any slope.

### Why this constant is defensible—and what cannot be derived

**OBSERVED:** Retained A1/A2/C0 counts coincide across the cells: `docs/perf/R20_PRF04_CLAIM_PATH_RUN01/summary.csv:2–21`, `:42–51`. The rule nevertheless supplies a 10-Ir floor: `docs/perf/R20_CONTROL_GEOMETRY_RULE.md:31–35`.

**Recommended owner policy:** accept no planned instruction tax; retain the established 10-Ir tolerance rather than claim perfect reproducibility universally. Freeze this policy before new measurements.

**There is no pure derivation of an allowed “few tens of Ir” from those facts.** Noise determines distinguishability, not acceptable overhead. Likewise, counting added atomic operations does not determine their compiled Ir: indexing, branches, inlining, and cleanup matter. The change’s arithmetic establishes that claim and release costs must both be included; it does not supply a justified positive allowance.

If the owner wants to pay a positive tax, that allowance needs an explicit independent principle and code-generation evidence—not a rounded-up version of V4′’s observed +15. My recommendation is not to manufacture that allowance.

## 3. Retained attribution: what it actually allows

**OBSERVED:** The retained A-only profile reports:

- Inclusive `dbg_claim_lease`: 602,104 Ir.
- Self `dbg_claim_lease`: 8,299 Ir.
- Large construction descendants, including `calloc`/`memset`.

See `docs/perf/_raw_r20_prf04_cold_profile.log:19–35`.

It does **not** list a separately measured selector. The recycled self cost is also attributed to `dbg_claim_lease`, not selector-only logic (`:40–48`).

The run01 evidence inventory contains totals, receipts, oracle records, and summaries—not a committed matched set of raw per-function Callgrind profiles. The runner saves textual iai output and extracts total Instructions: `scripts/r20_prf04_claim_path_run.mjs:345–359`, `:374–394`.

**Conclusion:** retained evidence supports construction dominance and whole-path deltas. It does not support a confirmatory absolute selector-attribution verdict. Inclusive claim cost includes construction; self cost excludes callees but includes other inlined work. Neither is synonymous with “selector.”

## 4. Preregistration/judge changes and run01 status

**Recommended changes:**

1. Create a separately versioned, frozen supplemental preregistration; leave the original protocol intact.
2. Register \(P_F=0\), window lengths, the absolute rule, and the 10-Ir absolute noise ceiling.
3. State explicitly that this is a **whole claim/release cost guard**, not selector isolation.
4. Add exact-integer absolute checks to the judge’s existing per-window/per-pair loop (`scripts/r20_prf04_claim_path_gate.mjs:180–191`).
5. Report absolute excess, window length, and tolerance separately from percentages.
6. Add counterfactual tests for an absolute-only failure inside 5%, equality at the boundary, negative deltas, excessive baseline noise, and every mandatory window.
7. Freeze the complete supplemental dependency set and retain the new CI run’s evidence.

**OBSERVED:** Original run01 passes its frozen checks (`docs/perf/R20_PRF04_CLAIM_PATH_GATE.md:105–110`). Retroactive threshold changes are prohibited (`docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md:188–200`).

**Decision:** no rerun is needed to preserve that historical result. A new CI measurement is required to report a result under the supplemental protocol. However, another run cannot undo the fact that V4′’s results were already seen; disclose that this is a prospective policy applied to a known candidate, not a candidate-blind original preregistration.

Descriptively, retained cold-window excesses of 128/128/256 and recycled excess of 240 exceed the proposed 10-Ir bound (`docs/perf/R20_PRF04_CLAIM_PATH_GATE.md:58–64`). A fresh run must not be used to relabel those observations as previously unknown.

## 5. Risks and failure modes

- **Not selector isolation.** Whole-path savings can offset selector overhead, especially in wave. This guard must not support a “selector is free” claim.
- **Wave publication cost remains masked.** Fixed warmup releases cancel in later slopes; the report already identifies release-only wave cost as unmeasured (`docs/perf/R20_PRF04_CLAIM_PATH_GATE.md:98–103`, `:133–134`).


---

## Orchestrator disposition (2026-10-13)

1. **Accepted as the design recommendation:** an absolute guard on the EXISTING whole claim/release cells (option 3),
   not a selector-only gate and not function-level attribution. It measures the shipped entry point, includes
   the post-CAS clearing and release publication that a selector-only cell would miss, and needs no new
   instrumentation or runtime refactor.
2. **Not implemented here, and not decided by the orchestrator:** the constant. `P_F = 0` ("no planned positive
   instruction tax") is an owner policy, not a derivation; the reviewer states there is no candidate-independent
   derivation of an allowed few tens of Ir. Applied descriptively to the retained run01 deltas it would reject
   V4-prime on the cold windows (+8 Ir per claim = 128/128/256 over the 16/16/32-claim windows) and on the recycled
   window (+15 Ir per pair = 240), while the same candidate removes 91.40 % of the per-claim cost in the wave family
   and its extra heaps. So the choice is between (a) a candidate-blind zero-tax policy under which V4-prime would
   need a leaner implementation, (b) an explicit owner-set positive allowance (needs its own principle and
   code-generation evidence, not a rounded-up +15), or (c) keeping run01 as the sole preregistered verdict.
3. **Run01 stays valid** under its own frozen preregistration (the reviewer agrees); a supplemental protocol would be
   prospective and must disclose that V4-prime results were already seen. A fresh CI run would be needed only to
   report under that supplemental protocol.
4. **Next steps if the owner picks (a) or (b):** a separately versioned supplemental preregistration (window lengths,
   `P_F`, the 10-Ir absolute noise ceiling), exact-integer absolute checks in the judge loop
   (`scripts/r20_prf04_claim_path_gate.mjs` around lines 180-191) with counterfactual tests for an absolute-only
   failure inside 5 %, boundary equality, negative deltas and excessive baseline noise, then one new CI run. Without
   an owner choice nothing in the gate changes.
