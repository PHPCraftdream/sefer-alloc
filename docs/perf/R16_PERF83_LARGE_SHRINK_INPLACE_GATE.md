# R16 item 83 — Large shrink-in-place: audit, preregistered gate and measured verdict

**Commit class (R30-12): `bench`. Verdict (computed by `scripts/r16_perf83_gate_table.mjs`): NO-GO. `git diff -- src` is empty; no runtime behavior changes ship.**
The candidate was implemented, frozen and measured, and its source was then restored to base. Sections 1–6 are the pre-measurement audit and preregistration, kept verbatim as written then: their statements about future, unimplemented or unverified state ("not implemented", "have not been verified by builds", "nothing beyond chunk1 has been delivered") are historical and are superseded by §7–§8. §5 is unchanged except for the amendments listed in §7, each recorded before the measurement it affects. §8 interprets the generated results.
Independent re-measurement by the integrating reviewer (separate A/B build from the frozen candidate patch `d2af1c71…`, forced rebuild, same bench binary layout): `realloc_large_shrink_8_to_6mib` −694401 Ir, `realloc_large_shrink_8_to_4p5mib` −521409 Ir, `realloc_grow` −14 Ir, all prefix and other Large controls 0 or ±3 Ir; these reproduce §8.1 exactly, so the NO-GO is not a measurement artifact.
Evidence labels: **CODE** = directly read source; **INFERRED** = consequence of that source, not runtime observation; **OBSERVED** = from a committed receipt/raw log.

## 1. Scope, references and identity

Read CLAUDE.md end-to-end (including structure, tests, performance evidence and all Active rules); perf OPEN_ITEMS item83 (:3043–3048); round-16 oxx review H1 (:99), W3 (:138, :209); R16_PERF84_FLUSH_ROOT_MASK_GATE.md; R12_02_MAGAZINE_MASK_GATE.md; scripts/r16_perf84_iai.mjs and r16_perf84_gate_table.mjs.
This is a single assigned item, not formation of a new round-wide queue. No index updates or round-wide closure claims. **HeapCore is the explicitly user-selected decision/promotion layer** for this gate. Real-global-allocator wall-clock and Vec/application throughput are not measured; their absence is not a blocker for this HeapCore gate.

Premeasurement code-audit identity, captured before writing this document:

| field | value |
|---|---|
| HEAD | `549d07b648ad48ee8e9d929bfec40157626a80b8` |
| HEAD tree | `146b0060ab3eed7100f66cf6164929b666381e58` |
| initial status | clean (`git status --short` printed nothing) |
| exact `git diff --ignore-cr-at-eol -- src` SHA-256 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (empty patch) |

This identifies today's unchanged source, NOT a future B candidate. Capture B's exact patch bytes/hash separately before any future measurement. No write-tree, index, branch, commit or worktree operation is permitted.

## 2. Frozen candidate (implemented, measured, reverted after NO-GO)

Let `old_eff=max(old_layout.size(), MIN_BLOCK)` and `new_eff=max(new_size, MIN_BLOCK)`.
Keep existing Large growth/equal behavior. A **strict shrink** can return the same allocation only when both layouts classify Large and:

```text
new_eff < old_eff
&& new_eff >= old_eff / 2 + old_eff % 2
&& ptr.addr() == canonical_base.addr() + existing_payload_offset
&& existing_payload_offset.checked_add(new_eff).is_some_and(|end| end <= existing_span_usable)
```

The division/remainder expression is the overflow-safe equivalent of `new_eff*2 >= old_eff`, including odd old sizes. The constant **2** bounds logical size retention from moderate shrink to at most twice the new effective request; it is not an empirically tuned optimum or a bound on total OS reservation including header/rounding.
On success use the existing field-specific `set_large_size_at(base, new_eff)` and increment `RELOC_INPLACE_LARGE_CALLS` exactly once under `alloc-stats`; return the allocator-derived pointer. Preserve payload offset, span_usable, reserved_capacity, reservation pair, large_align, bump, kind, route identity, terminal generation/owner state and live credit. No tail decommit, route re-registration or new unsafe surface. Below-half shrink and Large→Small shrink move; foreign path untouched.
Candidate scope is exclusively `src/alloc_core/alloc_core/mem/realloc_fastpath.rs`; no counter/accessor documentation edits in other src files are required or authorized. The existing counter is reused with the success semantics specified above. No policy substitution if this design fails.

## 3. Complete lexical consumer audit in src

Census command (read-only, includes comments as well as code):

```text
rg -n 'large_size|span_usable|reserved_capacity|payload_offset|payload_off|register_payload' src
rg -n 'large_size_at|set_large_size_at|span_usable_at|reserved_capacity_at|register_payload' src --glob '*.rs'
```

The tables inventory every executable accessor/registration occurrence and every field-consumer family found by that census. Comments and field declarations are separated from executable consumers; no cross-checkout source was consulted. Line numbers refer to the HEAD above.

### 3.1 Definitions, field views and exact callsites (CODE)

| file:line | responsibility / implication |
|---|---|
| `src/alloc_core/segment/segment_header/segment_header_impl.rs:358–387,471–519` | large_size is logical current size; payload_offset is payload start; span_usable is physical committed span; reserved_capacity is reserved VA, >= span. Never derive physical span from logical size. |
| same file `:615–638,692–724` | Small constructor zeros these fields; Large constructor stamps size, payload_offset=`bump-size`, span and capacity. |
| `src/alloc_core/segment/segment_header/segment_header_views.rs:59–61` | only executable definition of large_size_at; field-specific load, feature-gated promoted-Large layout check. |
| same file `:114–116,129–131,225–227,253–255` | definitions of span_usable_at, set_large_size_at, reserved_capacity_at and set_span_usable_at; loads/stores are field-specific Node operations. |
| `src/registry/heap_core/free/dealloc_own_base.rs:242–243` | sole executable large_size_at consumer: hardened promoted-Large check requires current clamped layout size and alignment. Stale large_size after shrink would reject the legitimate new-layout free in this branch. |
| `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:134` | safe_payload_read_span consumes span_usable_at; read bound is physical span minus actual payload offset, not logical size. |
| same file `:292–299` | reads payload_offset, verifies payload-start pointer, checks checked_add against span_usable_at and writes logical size on committed-span success. |
| same file `:316` | second set_large_size_at call after successful reserved-capacity growth. |
| same file `:515,519,540` | sole reserved_capacity_at consumer; second grow span read; sole set_span_usable_at call after commit succeeds. Failure leaves header unchanged (:533–538). |
| `src/alloc_core/large/alloc_core_large.rs:422` | third executable set_large_size_at call, cache hit; changes logical size only. |
| `src/alloc_core/segment/segment_table/segment_table_impl.rs:382,385–405` | register wrapper forwards base as payload; register_payload definition keys by masked payload, prepares route with base/len/kind/payload. |
| `src/alloc_core/large/alloc_core_large.rs:451–455,658–662` | the two direct Large register_payload calls: cached and fresh. Both use reserved capacity as route length and actual payload as key. |
| `src/alloc_core/segment/segment_table/segment_table_impl.rs:510–512` | unregister reconstructs payload key using header.payload_offset; shrink must not alter offset. |

All other occurrences of those accessor names in src are explanatory comments, not additional calls. There is no shrink setter for reserved_capacity or payload offset.

### 3.2 Allocation, cache keys, biased alignment (CODE)

| file:line | finding |
|---|---|
| `src/alloc_core/large/alloc_core_large.rs:143–159,192–207` | header/padding plus size uses checked_add; production whole-SEGMENT rounding differs from opt-in exact-page span. A literal 8 MiB request needs MORE than 8 MiB physical span because metadata precedes payload. |
| same file `:216,238–244` | cache compatibility is align<SEGMENT and physical slot.usable_size between requested usable and usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR); best-fit smallest span. No large_size cache key. |
| same file `:254–255,297–298` | remove hit's full physical usable bytes from accounting, including generation-exhaustion fallback. |
| same file `:382–398,422–430` | cache hit verifies physical geometry/reservation against slot; rewrites logical size, align, bump and payload offset; physical span/capacity remain carried forward. |
| same file `:451–484` | register slot capacity/actual payload, finish terminal reuse, return freshness=false. |
| same file `:517–520,558–563,630–653` | biased reserve for align>=SEGMENT; non-NUMA capacity target may exceed committed usable; NUMA capacity=usable. |
| same file `:678–699` | fresh header records committed usable and reserved capacity separately; freshness is cfg!(not(miri)). |
| `src/alloc_core/platform/os.rs:178–210` | biased root is derived from OS reservation and aligned payload, not the masked payload address. Raw length/root-offset arithmetic checked; commit useful window at actual root. Extra initial page noted by H3 is outside this candidate. |
| `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:275–283` | resolves masked payload key to canonical metadata root and reconstructs allocator-provenance pointer; this is mandatory for biased roots. |

**INFERRED:** shrinking only logical size cannot change cache fit or registration identity. For biased align>=SEGMENT the allocation remains uncached on free; no new masking shortcut is permitted. No evidence here establishes a runtime win or RSS neutrality.

### 3.3 Deallocation, lifecycle, zeroing, accounting and decay (CODE)

| file:line | finding |
|---|---|
| `src/alloc_core/alloc_core/mem/mem_impl.rs:224–269,293–315` | own dealloc canonicalizes, routes by actual kind, claims Large terminal credit and snapshots physical span/capacity. Layout does not determine release size. |
| same file `:363–365,439–452,474–487` | cache admission excludes over-segment alignment, checks physical budget; slot stores reservation pair/span/capacity; accounting adds physical span; direct release uses reservation pair. |
| `src/alloc_core/large/alloc_core_large.rs:728–755,767–773,810–821` | cross-thread owner reclaim claims live state, unregisters, admits by physical span/alignment/budget and carries capacity into slot; full physical bytes charged. |
| `src/alloc_core/alloc_core/alloc_core_impl.rs:176–194` | CachedLarge fields are reservation, reservation_len, base, usable_size and reserved_capacity. Logical size absent. |
| `src/alloc_core/large/alloc_core_large_cache.rs:211–215,627–639` | budget-infeasible check uses physical usable_size; decay excess is large_cache_used_bytes minus headroom, then evict_at_least. |
| `src/alloc_core/large/alloc_core_large_cache_eviction.rs:34–54,198–215,231–245` | FIFO sequence chooses victim; full victim.usable_size subtracted; release uses victim reservation pair; logical shrink does not reduce accounting. |
| same file `:340–344`; `src/alloc_core/large/alloc_core_large_cache.rs:396` | base/extended slot-size diagnostics expose usable_size, not logical size. |
| `src/alloc_core/alloc_core/lifecycle.rs:60–70,470–479,510–539` | cached drop releases reservation pair after terminal credit transition; live Large drop reads header reservation pair and claims/releases live credit. No recomputation from logical size, span or capacity. |
| `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:851–866,883–888` | reconciliation counts active committed bytes from hdr.span_usable and reserved bytes from reservation_len; cached bytes from usable_size/reservation_len. |
| `src/alloc_core/alloc_core/mem/mem_impl.rs:135–136` | alloc_zeroed consults alloc_large freshness; cached bytes explicitly zeroed, fresh OS span skip only. Shrink does not create a fresh allocation. |
| `src/registry/heap_core/alloc/hot.rs:726–739` | HeapCore alloc_zeroed also calls core.alloc_large and zeroes only non-fresh returns; the fresh skip is not inferred solely from the lower-layer implementation. |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:27–41,213–263` | geometry observer reads payload offset/capacity; logical-size/span/capacity diagnostics use canonical lookup and field-specific reads. |
| `src/alloc_core/segment/segment_header/layout_asserts.rs:123–165` | exhaustive constructor-field inventory, not a runtime consumer. |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs:25` | payload_off mentioned only to distinguish gen-table offset semantics. |
| `src/global/sefer_alloc/mod.rs:152`; `src/registry/heap_core/free/dealloc.rs:148` | contract documentation references payload offset/current logical layout; no additional executable consumers. |

**INFERRED:** no tail decommit means both committed span and cache charge can remain higher after shrink. Existing release/terminal-credit protocol needs no semantic change; correctness tests must still prove cache reuse, current-layout free and teardown.

### 3.4 Realloc routing and oracle (CODE)

`src/registry/heap_core/free/realloc.rs:137–203` canonicalizes own pointer and delegates to try_realloc_inplace_known_base; `:347–348,403–407` copies min(old,new) then frees old own allocation. Foreign branch `:414–434` validates route and alloc/copy/free: leave unchanged.
`src/alloc_core/alloc_core/mem/mem_impl.rs:644–688` shares fast path for standalone core, then physical read-span validation and move. Candidate therefore also affects this lower layer, though judge must call **`unsafe { (*heap).realloc(ptr, old_layout, new_size) }`**, never bypass through AllocCore.
`src/alloc_core/alloc_core/mem/realloc_fastpath.rs:291` currently allows growth/equal only. Large branch has **no new-layout class_for test** (contrast Small same-class checks :336–337).
`src/alloc_core/alloc_core/counters.rs:211–238` and `alloc_core_core_diag/perf_diag.rs:95–119` provide process-wide Large/Small/decline oracle. Increments are alloc-stats-only. Decline is not a successful-move counter: early validation failures can occur without a counted decline or completed move. Count moves independently via successful returned-address inequality while old object is live.

## 4. Blockers and scope discrepancies

1. **CODE blocker for a naive one-condition edit:** widening :291 alone would allow Large→Small shrink, contrary to frozen candidate. Add explicit Large classification for both old/new layouts on the strict-shrink branch; do not change existing growth/equal decisions. Correctness must exercise exact-half, half-minus-one, odd sizes and cross-class shrink.
2. **Selected layer:** item83/review mentions real #[global_allocator], but the user explicitly selected HeapCore for this gate and its promotion decision. Real-global wall-clock is not measured; this is a scope disclosure, not a blocker or an additional promotion requirement.
3. **Runner reuse limitation:** perf84 script pins a different BASE (:12), different owned files (:15), calls forbidden `git ls-files` (:26) and includes a task84 oracle. Do not execute/copy it unchanged. Future driver must enumerate local inputs using filesystem traversal, reject symlinks/escapes and use only allowed git reads. Its no-overwrite snapshots/manifest verification and checked-table design are precedents, not current receipts.
4. **RSS risk:** logical half-size rule does not bound physical retained bytes to 2x. Production header/SEGMENT rounding and old-span caching can change peak/residency. This is a mandatory measured kill gate, not grounds to silently add tail decommit or retune the factor.
5. Tool availability, frozen snapshot build portability and compiler/Valgrind identity have not been verified by builds. Missing tools or inability to obtain C0/activation/RSS is BLOCKED, not GO and not permission to substitute wall clock/noise estimates.

## 5. Exact preregistration — immutable before all measurements

### 5.1 Arms, feature sets, fixtures and deterministic gate

Performance/RSS features: `production bench-internals internals`, **without alloc-stats**. Activation-only rebuild adds `alloc-stats`; instrumented results must never supply Ir/RSS verdict numbers.
A = unchanged pinned src plus common chunk2 harness. C0 = identical A input bytes at a different nested path. B = A inputs plus audited candidate. All snapshots under `<repo>/target/r16-perf83/{A,C0,B}`, each with own `$PWD/target`. No parent checkout or external target. Frozen snapshots must never be overwritten.

New Ir fixtures: **N=1** HeapCore-owned live object, align16, full touch of the old logical payload, one realloc, full touch of the resulting new logical payload, black_box of the pointer/result and free using NEW layout. Full touch means deterministic writes across the logical payload, identical in A/B; no extra full-byte preservation scan in measured benches. Byte preservation is checked by activation/correctness separately. Same heap setup, touch schedule and teardown routine in both arms. No artificial work to magnify savings. Sizes are exact binary bytes (MiB=1,048,576):

| proposed fixture name | old → new bytes | expected A / B activation |
|---|---|---|
| `realloc_large_shrink_8_to_6mib` | 8388608 → 6291456 | move / in-place |
| `realloc_large_shrink_8_to_4p5mib` | 8388608 → 4718592 | move / in-place |
| `realloc_large_shrink_8_to_3mib` | 8388608 → 3145728 | move / move |
| `realloc_large_grow_6_to_8mib` | 6291456 → 8388608 | move / move |
| `realloc_large_equal_8mib` | 8388608 → 8388608 | in-place / in-place |

Premeasurement activation correction (chunk2a): Windows A was observed to move for 6→8 MiB, independently reproduced by the operator. Linux activation remains to be verified. **CODE/INFERRED explanation:** the 6 MiB request plus metadata rounds to an 8 MiB committed span, but its saved positive payload_offset plus an 8 MiB new payload exceeds that span (`realloc_fastpath.rs:292–298`); production has no extra reserved-capacity growth feature. Thus growth A/B expects move/move; this corrects activation expectations only, not sizes, candidate policy, Ir/RSS metric gates or thresholds. Equal8 remains in-place.

Mandatory prefix pairs (these are preregistered new fixture names, not claims that functions already exist):

| work fixture | prefix fixture |
|---|---|
| `realloc_large_shrink_8_to_6mib` | `realloc_large_shrink_8_to_6mib_prefix` |
| `realloc_large_shrink_8_to_4p5mib` | `realloc_large_shrink_8_to_4p5mib_prefix` |
| `realloc_large_shrink_8_to_3mib` | `realloc_large_shrink_8_to_3mib_prefix` |
| `realloc_large_grow_6_to_8mib` | `realloc_large_grow_6_to_8mib_prefix` |
| `realloc_large_equal_8mib` | `realloc_large_equal_8mib_prefix` |

Each prefix uses exactly its work partner's HeapCore construction, old-layout allocation and full old-payload touch setup, black_box and teardown routine. Prefix omits realloc and the resulting new-payload touch; it frees its still-live old allocation using OLD layout, while work frees the returned allocation using NEW layout. This contract-required layout difference is disclosed, not concealed as identical allocator free work. No alternate allocation/cache priming or extra verification loop in either member.
For each arm/row print `paired_ir=Ir(work)-Ir(prefix)` and `paired_delta=paired_ir_B-paired_ir_A`, with operands visible. These are explanatory paired results, not replacement primary gates: the two moderate-shrink **raw work** savings must still strictly exceed T. Every prefix is a mandatory control requiring **abs(B1.Ir-A1.Ir)<=T**. No additional paired threshold is imposed; no paired result can rescue a failed raw-work or prefix gate.

Existing **all Large/realloc rows** in benches/perf_gate_iai.rs, frozen as mandatory controls: `large_alloc_free_cycle` (:2226), `large_cache_prefill_only_4mib` (:2280), `large_cache_hit_only_4mib` (:2316), `large_cache_free_slot_search_prefill_only` (:2392), `large_cache_free_slot_search_cycle_only` (:2431), `realloc_grow` (:2487), and the small realloc-burst row `dealloc_realloc_burst_1088_16b_n17` (:1732). Missing/feature-skipped fixture is not a pass. No reduction after seeing numbers.

Run order: A1, A2, C0, B1, B2, after all identities and builds are frozen. Two independent runs each A/B; one C0. Every row must have identical A1=A2 and B1=B2 numeric columns: Ir, L1, L2, RAM, Total read+write, Estimated Cycles. No averaging, retries chosen for favorable values or repeat-only substitute for missing C0.
Define `delta(row)=B1.Ir-A1.Ir`, `C0delta(row)=C0.Ir-A1.Ir` and **T=max(10 Ir, max over all registered rows abs(C0delta))**.
Both 8→6 and 8→4.5 must improve **strictly more than T** (`A1.Ir-B1.Ir > T`). Every other registered row requires **abs(delta)<=T**, including all five prefixes, 8→3, 6→8, equal and all existing Large/realloc controls. Large-target savings cannot compensate any failed control. Report every cache-model column/delta and C0 envelope as secondary signals, not real CPU cycles or latency. No native timing speed claim is preregistered.

### 5.2 Activation and correctness/counterfactual gate

Fresh process per arm/activation scenario; serial tests, counter deltas around only the single realloc. Without env the integration test runs the meaningful below-half 8→3 control with ARM=A; explicitly supplied ARM/SCENARIO values are strictly validated. The later driver must explicitly run all five scenarios per arm, not treat the default as full coverage. A/C0 moderate shrink: Large delta0, Small delta0, decline delta1, successful move1. B moderate shrink: Large delta1, Small delta0, decline delta0, successful move0. Below-half and growth6→8 controls: Large delta0, Small delta0, move1/decline1 in both; equal8: Large delta1, Small delta0, decline0/move0 in both. Assert non-null, exact classification, actual alignment, full prefix preservation and new-layout deallocation. Record requested/resolved configuration, conflict counter if exposed, process/arm/feature identity and observed pointer/counter outcomes. Never call counters proof of runtime activation before running them.

Chunk3 tests in tests/ must also cover: half exact and half-minus-one; odd old_eff ceiling; Large→Small must move; shrink→regrow within physical span; biased align=SEGMENT and 2*SEGMENT with canonical geometry stable; hardened current logical-size/align free (with promoted-Large reachable feature composition); ordinary cache deposit/reuse physical charge unchanged and alloc_zeroed on recycled span truly zero; owner/foreign frees and foreign realloc unchanged; terminal credits/cache decay/trim/drop release; allocation-failure old block remains live; existing OOB/foreign-layout defenses. Opt-in exact-span-large and large-reserved-capacity coverage is correctness only, not substitute performance features.

Nonvacuity requirements: revert only shrink eligibility (A counterfactual) must fail B same-pointer/counter tests; omit logical-size update must fail header/current-layout-free oracle; relax half boundary must fail below-half move assertion; omit increment must fail delta1. Separate mutants, no simultaneous attribution. Use `cp file file.bak`, mutate, run targeted test, `cp file.bak file`, `rm file.bak`, then exact diff/hash verification against pre-mutant B. Restore on failure too. Never git restore/checkout. No weakened existing test or assertion.

### 5.3 Native RSS/commit gate in the same workload regime

Freeze **N=1 live workload object**, **9 fresh processes per arm per scenario**, align16, one HeapCore per process and no previous arm/config in that process. The primary RSS scenarios are exactly the five Ir work scenarios: **8→6, 8→4.5, 8→3, 6→8 and equal8 MiB**, using the byte sizes in §5.1. Each process measures baseline resident bytes at the beginning, then uses the same old-layout allocation/full old-payload touch, single HeapCore realloc, full new-payload touch and teardown routine as its Ir work fixture. Primary RSS is absolute resident bytes immediately after that single realloc and full new-payload touch, with the one resulting allocation still live. Byte-preservation scans belong to activation/correctness, not an extra measurement workload. Per scenario take the median of the nine process primary RSS values. Require **median_RSS_B <= median_RSS_A + 0.01*median_RSS_A** in each of the five scenarios; the denominator is A's absolute median resident bytes, not a baseline-subtracted delta.

Separately preregister a **20-cycle 8↔6 MiB diagnostic peak gate**, also **N=1 live workload object** and **9 fresh processes per arm**. Measure baseline at process measurement start before setup; allocate/touch8, then repeat shrink-to6/full-touch6 and grow-to8/full-touch8 twenty times. Use the same touch/realloc mechanism as the primary fixtures, no unrelated load. Capture RSS at setup and after every shrink/grow, then final-free and teardown. Read Linux **VmHWM** after cycle20 and before final-free: it is a **process lifetime peak including startup/setup**, NOT a resettable cycle-window peak. This disclosed lifetime high-water metric is the registered peak of the cyclic probe; no resettable-window API or true-window requirement applies. Define `cycle_peak_arm=max(VmHWM across its nine processes)` and require **cycle_peak_B <= cycle_peak_A + 0.01*cycle_peak_A** (denominator: A's absolute maximum lifetime peak). This separate diagnostic gate is mandatory, but does not replace the same-regime five single-realloc primary RSS gates.

For each scenario and the cyclic probe run pairs sample1 A,B; sample2 B,A; alternate through9. RSS observer must be out of judged allocator process or use a non-Sefer system allocation path. Record baseline RSS/VmHWM, absolute primary RSS, lifetime peak and final-free/teardown separately; do not relabel lifetime peak as an isolated cycle-window peak. No forced cache drain, decay clock mutation, trim, sleeps or arm-specific configuration changes to rescue RSS. Record requested/resolved normal cache/decay settings. No baseline subtraction or pooling scenarios to rescue a failure. Commit/reserved bytes are supplementary actual OS/allocator diagnostics, distinctly labelled from RSS; neither replaces RSS. CPU/OS/page-size/compiler identity and process exit/assertion status belong to every receipt.

### 5.4 Identity, build receipts and derived artifacts

Before any Ir/RSS measurement, capture HEAD/tree, exact `git diff --ignore-cr-at-eol -- src` bytes+SHA256, common bench/activation/probe/script hashes, sorted input path+SHA256 manifest and manifest hash per arm. Include Cargo.toml, existing Cargo.lock, local seam crates, config files and every build input; do not generate/change dependency versions or lockfile. Prove A=C0 manifest byte identity and B differences restricted to audited candidate files. Capture actual build commands, environment, features/profile/target triple, rustc/cargo/runner/Valgrind versions, start/end/exit and executable SHA256 for performance, RSS and activation binaries. Each run references its manifest and binary hash; rebuilding requires a new receipt and hash verification, not silently reusing stale shared artifacts.

Build/test command envelope for later chunks only:

```text
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo test --locked --features "production bench-internals internals alloc-stats" --test <actual-chunk2-or-3-test> -- --test-threads=1
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo bench --locked --no-run --bench perf_gate_iai --features "production bench-internals internals"
```

These are preregistered command shapes, NOT executed commands or existing new test names. Direct formatting of actually edited Rust files: `rustfmt --edition 2021 --check <files>`. All unsafe remains in existing seam style with SAFETY contracts; mod.rs reexports only, one export responsibility, no doctests or new production measurement hooks.

Future checked script must derive Markdown tables, summary CSV and verdict from structured per-sample raw artifacts written first, verify hashes/repeats/row completeness/RSS sample count and arithmetic, and offer read-only --check. Ratios print numerator/denominator; statistics are labelled at computation. Sanitize all machine-private path prefixes to `<repo>`/`<target>`; each cited raw excerpt <200KiB, truncation markers/reproduction command/full sanitized hash retained. Preserve complete relevant evidence, do not hand-transcribe headline values. No summary numbers or raw measurement logs are owed by this code-only preregistration yet.

## 6. Decision rules and chunk boundaries

GO requires all frozen identity, activation, correctness/counterfactual, repeat, Ir benefit/control and RSS gates together. Identity/missing-observer/missing-data failure = BLOCKED/INVALID, never numerical GO. Verified activation with savings not strictly above T and no kill-gate failure = NULL; any control/RSS/correctness failure = NO-GO. Report every failure without changing factor, workloads, feature set or tolerances after seeing data. On NO-GO/NULL restore owned src using `cp` pre-candidate backup, not git restore, remove backups only after verification; confirm exact pre-candidate src diff/hash. Keep valid harness/report evidence distinct from rejected runtime candidate and review unused hooks in same task.

Chunk2: bench/activation/scripts and frozen A/C0 preparation only. Chunk3: candidate/correctness. Chunk4: RSS probe and authorized measurements. Chunk5: checked tables/verdict. Nothing beyond chunk1 has been delivered here. No commits/staging/stash/reset/restore/rebase/merge/push/checkout/write-tree/index/branch writes; no Cargo/versions/README/CHANGELOG/open-index edits; no subagents. Stop/report any non-build/non-iai anomaly lasting >10min; explicitly long builds/iai require declared receipts, not silent indefinite runs.

## 7. Amendments made before the affected measurement, and the execution log

1. **Growth control expectation (§5.1 table, §5.2).** Preregistration predicted `6→8 MiB` in-place in both arms. Before any Ir/RSS run, the A-arm activation oracle was OBSERVED to move the block: `moved=true`, `decline_delta=1`. INFERRED from CODE: payload_offset plus 8 MiB exceeds the 8 MiB physical span of a rounded 6 MiB allocation. The expectation was changed to move/move. The metric gate (|ΔIr|≤T, RSS ≤ A+1% A) was not changed.
2. **Measurement-rebuild guard (`scripts/r16_perf83_iai.mjs --run`).** The first A1 attempt was rejected by the driver itself, because `cargo bench` re-printed `Compiling` on the drvfs snapshot. That attempt is preserved, unused, under `target/r16-perf83/invalidated_A1_attempt1/`. The guard now checks two things: that exactly the compile-receipt binary path ran (`Running benches/perf_gate_iai.rs (target/release/deps/perf_gate_iai-6a994302539a9d7d)`), and that this binary's SHA-256 is unchanged after the run. The judge checks the same. OBSERVED: the binary hash is identical before and after every run (A `027dd149…`, C0 `a8492c1c…`, B `8dae55b9…`). A2 rebuilt nothing, yet its six numeric columns equal A1 in every row; B1=B2 likewise.
3. **Identity bundle storage.** The identity bundle is about 1.34 MB uncompressed, because it holds every receipt and build log. Under the CLAUDE.md tier-2 rule (200 KiB–2 MiB, option b) it is committed gzip-compressed as `R16_PERF83_LARGE_SHRINK_INPLACE_GATE_identity.json.gz`. The summary CSV keeps per-run A1/C0/B1 iai metrics (A2/B2 equality is asserted by the script) and per-sample RSS/peak values; per-sample baseline, final-free and teardown values remain in `_raw_r16_perf83_rss.log`.
4. RSS stale partial receipts: the first RSS compile attempt left only a start/tools receipt and no binary. Those receipts and its empty `target/rss` were deleted before the receipted `--rss-compile A`/`B`. No RSS data existed at that point.

5. **Post-measurement lint-only edit (integration).** Linux `clippy -D warnings` rejected `examples/r16_perf83_large_shrink_rss.rs` with `needless_late_init` (invisible to the Windows clippy). The `final_size` binding was rewritten from a late-initialized `let` to `let final_size = if …`, with identical control flow and values. The RSS binary hashes in the identity bundle therefore refer to the pre-edit source; no measurement was repeated, and the edit cannot change any number (same branches, same calls, same order).

Execution (all receipts under `target/r16-perf83`, imported by `--capture`): `--prepare`, `--prepare-b`, `--verify`, `--compile A|C0|B` (each log contains `Compiling sefer-alloc`), `--activation A|C0|B` (5 separate processes each), `--rss-compile A|B`, `--run A1`, `A2`, `C0`, `B1`, `B2`, `--rss-run` (108 fresh processes), then `scripts/r16_perf83_gate_table.mjs --capture`, `--update-report`, `--check`.

Committed evidence: raw iai excerpts `docs/perf/_raw_r16_perf83_A1.log`, `_raw_r16_perf83_A2.log`, `_raw_r16_perf83_C0.log`, `_raw_r16_perf83_B1.log`, `_raw_r16_perf83_B2.log` (11332 B each, selected rows verbatim, full-log SHA-256 inside), `_raw_r16_perf83_rss.log` (108 NDJSON rows), `R16_PERF83_LARGE_SHRINK_INPLACE_GATE_summary.csv`, `R16_PERF83_LARGE_SHRINK_INPLACE_GATE_identity.json.gz`.

Identity (OBSERVED): base `549d07b648ad48ee8e9d929bfec40157626a80b8`, tree `146b0060ab3eed7100f66cf6164929b666381e58`. Candidate patch SHA-256 (`git diff --ignore-cr-at-eol -- src`) is `d2af1c71315bc72e6d78a006e7bce69d33e76d803fe8128acca5cde86ab4bef3`; the patch bytes are in the identity bundle as `candidate.patch`. A/C0 input manifest SHA-256 is `111c2784…0f20` (identical by assertion), and B is `c89095be…ffaf`, which differs from A only in `realloc_fastpath.rs`.

## 8. Results and verdict

Entry point: `HeapCore::realloc` through `(*heap)` on a `HeapRegistry` lease (bench helper `r16_perf83_round`, RSS probe, activation oracle). This layer is the one `GlobalAlloc::realloc` forwards to via `try_realloc_inplace_known_base`, and it is the layer the user chose. Features: `production bench-internals internals`, plus `alloc-stats` for the activation build only.

Path activation (OBSERVED; separate process per arm and scenario; `RELOC_*` deltas around the single realloc):
- A and C0: 8→6 and 8→4.5 moved (`decline_delta=1`); 8→3 and 6→8 moved; equal8 stayed in place (`inplace_large_delta=1`).
- B: 8→6 and 8→4.5 stayed in place (`moved=false`, `inplace_large_delta=1`, `decline_delta=0`); 8→3 and 6→8 moved; equal8 stayed in place.

RSS configuration (R26-4): requested = default `HeapRegistry::dbg_claim_lease` config. Resolved = decay 1000 bp / 1000 ms, headroom 268435456 B, large-cache budget `null`, 8 slots, pool_cap 4, `config_conflicts_delta=0`. The resolved config is identical across all 108 processes. Process boundary: subprocess per sample and arm.

Correctness while B was in the tree:
- New correctness test, 16 tests on the AllocCore and HeapCore layers: 16/16 under `production internals alloc-stats bench-internals batch-api` and 16/16 under `hardened production internals alloc-stats bench-internals`.
- 116 related test binaries (grep `realloc|reserved_capacity|large_cache|alloc_large|RELOC_INPLACE|dbg_reloc`, `--no-fail-fast`): 338 passed, 2 failed.
- The 2 failures are `no_stale_doc_references::{architecture_test_file_count_matches_reality, verification_inventory_matches_docs}`. They count `tests/*.rs` files against README/ARCHITECTURE; this is index work outside this task's write scope.
- Mutants of B (each restored by `cp`; the patch hash was re-verified as `d2af1c71…` afterwards):
  - (a) the threshold accepts any shrink: `below_half::alloc_core` fails, and activation B `8→3` fails.
  - (b) in-place shrink disabled: `biased::alloc_core` fails, and activation B `8→6` fails.
  - (c) `set_large_size_at` skipped on shrink: `biased`, `boundaries`, `moderate_shrink` and `regrow` fail on the `alloc_core` layer (4/16).

**Why NO-GO.** The two target rows pass far above T:
- 8→6: A1−B1 = 694401 Ir (B/A 14729652/15424053).
- 8→4.5: 521409 Ir (13156788/13678197).

All five prefix controls give Δ=0, and every new Large control is within T. The RSS axis passes everywhere and is strongly favourable to B:
- 8→6 median 10555392/16855040 B.
- 8→4.5 median 10547200/15282176 B.
- 20-cycle 8↔6 VmHWM maximum 10563584/16863232 B.

The preregistered existing control `realloc_grow` (64 B→4 MiB through `GlobalAlloc::realloc`) measured Δ=−14 Ir (582733−582747) against T=max(10, max |C0−A1|=12)=12, so it fails |Δ|≤T. A1=A2 and B1=B2 show repeatability within each binary; they do not establish that the cross-binary −14 Ir delta is signal. The same-source control C0−A1 is +12 Ir, comparable in magnitude to B1−A1=−14 Ir, so the observed delta cannot be attributed to the candidate code shape. The preregistered two-sided rule is unchanged and the computed verdict remains NO-GO.

**Not done as a result.** The src change and its correctness test (`tests/r16_perf83_large_shrink_inplace.rs`) were removed. The candidate patch remains reproducible from `candidate.patch` in the identity bundle. The bench rows, activation oracle (its default no-env run is the base-valid 8→3 control), RSS probe, scripts, CSV and raw logs are kept. A follow-up needs a fresh preregistration with a control strategy justified before measurement, including enough same-source control samples and/or a control built from the candidate source to resolve the observed ±12/−14 Ir scale. Do not infer a code-shape effect from the existing control delta. Whether to do a follow-up is the orchestrator's decision.

Not measured: wall-clock/native cycles (Ir and EstCycles are Callgrind model outputs only); a real `#[global_allocator]`; `Vec` throughput; Windows or other OSes (all measurements ran Linux under WSL2); huge pages; `numa-aware`; `exact-span-large`/`large-reserved-capacity` performance; multi-threaded or foreign-realloc workloads; committed or reserved bytes as a separate gate.

### 8.1 Generated tables (do not edit; `node scripts/r16_perf83_gate_table.mjs --check`)

<!-- r16_perf83:tables:begin -->
| bench | A1 Ir | A2 Ir | C0 Ir | B1 Ir | B2 Ir | B−A Ir | B/A Ir (num/den) | A/B/C0 L1 | A/B/C0 L2 | A/B/C0 RAM | A/B/C0 Total | A/B/C0 EstCycles | B/A EstCycles (num/den) |
|---|---:|---:|---:|---:|---:|---:|---|---|---|---|---|---|---|
| realloc_large_shrink_8_to_6mib | 15424053 | 15424053 | 15424041 | 14729652 | 14729652 | -694401 | 0.954979 (14729652/15424053) | 30116275/29224516/30116257 | 49138/98031/49137 | 378321/132673/378320 | 30543734/29455220/30543714 | 43603200/34358226/43603142 | 0.787975 (34358226/43603200) |
| realloc_large_shrink_8_to_6mib_prefix | 8437961 | 8437961 | 8437961 | 8437961 | 8437961 | 0 | 1.000000 (8437961/8437961) | 16739620/16739622/16739620 | 163/163/163 | 132201/132199/132201 | 16871984/16871984/16871984 | 21367470/21367402/21367470 | 0.999997 (21367402/21367470) |
| realloc_large_shrink_8_to_4p5mib | 13678197 | 13678197 | 13678185 | 13156788 | 13156788 | -521409 | 0.961880 (13156788/13678197) | 26772978/26103363/26772960 | 73713/73561/73712 | 280019/132568/280018 | 27126710/26309492/27126690 | 36942208/31111048/36942150 | 0.842155 (31111048/36942208) |
| realloc_large_shrink_8_to_4p5mib_prefix | 8437961 | 8437961 | 8437961 | 8437961 | 8437961 | 0 | 1.000000 (8437961/8437961) | 16739620/16739622/16739620 | 163/163/163 | 132201/132199/132201 | 16871984/16871984/16871984 | 21367470/21367402/21367470 | 0.999997 (21367402/21367470) |
| realloc_large_shrink_8_to_3mib | 11932341 | 11932341 | 11932329 | 11932344 | 11932344 | 3 | 1.000000 (11932344/11932341) | 23429683/23429694/23429665 | 57478/57477/57476 | 222525/222524/222525 | 23709686/23709695/23709666 | 31505448/31505419/31505420 | 0.999999 (31505419/31505448) |
| realloc_large_shrink_8_to_3mib_prefix | 8437961 | 8437961 | 8437961 | 8437961 | 8437961 | 0 | 1.000000 (8437961/8437961) | 16739620/16739622/16739620 | 163/163/163 | 132201/132199/132201 | 16871984/16871984/16871984 | 21367470/21367402/21367470 | 0.999997 (21367402/21367470) |
| realloc_large_grow_6_to_8mib | 15424062 | 15424062 | 15424050 | 15424059 | 15424059 | -3 | 1.000000 (15424059/15424062) | 30116286/30116290/30116268 | 81936/81937/81934 | 345524/345521/345524 | 30543746/30543748/30543726 | 42619306/42619210/42619278 | 0.999998 (42619210/42619306) |
| realloc_large_grow_6_to_8mib_prefix | 6340809 | 6340809 | 6340809 | 6340809 | 6340809 | 0 | 1.000000 (6340809/6340809) | 12578084/12578086/12578084 | 210/210/210 | 99386/99384/99386 | 12677680/12677680/12677680 | 16057644/16057576/16057644 | 0.999996 (16057576/16057644) |
| realloc_large_equal_8mib | 16826787 | 16826787 | 16826787 | 16826784 | 16826784 | -3 | 1.000000 (16826784/16826787) | 33386029/33386029/33386029 | 130661/130661/130661 | 132811/132809/132811 | 33649501/33649499/33649501 | 38687719/38687649/38687719 | 0.999998 (38687649/38687719) |
| realloc_large_equal_8mib_prefix | 8437973 | 8437973 | 8437961 | 8437973 | 8437973 | 0 | 1.000000 (8437973/8437973) | 16739639/16739641/16739620 | 165/165/163 | 132200/132198/132201 | 16872004/16872004/16871984 | 21367464/21367396/21367470 | 0.999997 (21367396/21367464) |
| large_alloc_free_cycle | 49809 | 49809 | 49809 | 49809 | 49809 | 0 | 1.000000 (49809/49809) | 94126/94125/94126 | 184/186/184 | 1108/1107/1108 | 95418/95418/95418 | 133826/133800/133826 | 0.999806 (133800/133826) |
| large_cache_prefill_only_4mib | 58797 | 58797 | 58809 | 58797 | 58797 | 0 | 1.000000 (58797/58797) | 107065/107067/107084 | 168/168/169 | 1097/1095/1097 | 108330/108330/108350 | 146300/146232/146324 | 0.999535 (146232/146300) |
| large_cache_hit_only_4mib | 60100 | 60100 | 60100 | 60100 | 60100 | 0 | 1.000000 (60100/60100) | 108921/108923/108921 | 168/168/168 | 1113/1111/1113 | 110202/110202/110202 | 148716/148648/148716 | 0.999543 (148648/148716) |
| large_cache_free_slot_search_prefill_only | 63607 | 63607 | 63607 | 63607 | 63607 | 0 | 1.000000 (63607/63607) | 113350/113353/113350 | 170/169/170 | 1222/1220/1222 | 114742/114742/114742 | 156970/156898/156970 | 0.999541 (156898/156970) |
| large_cache_free_slot_search_cycle_only | 75140 | 75140 | 75140 | 75140 | 75140 | 0 | 1.000000 (75140/75140) | 129682/129685/129682 | 170/169/170 | 1243/1241/1243 | 131095/131095/131095 | 174037/173965/174037 | 0.999586 (173965/174037) |
| realloc_grow | 582747 | 582747 | 582759 | 582733 | 582733 | -14 | 0.999976 (582733/582747) | 1195393/1195459/1195410 | 4412/4423/4413 | 70961/70961/70963 | 1270766/1270843/1270786 | 3701088/3701209/3701180 | 1.000033 (3701209/3701088) |
| dealloc_realloc_burst_1088_16b_n17 | 344181 | 344181 | 344181 | 344181 | 344181 | 0 | 1.000000 (344181/344181) | 491110/491111/491110 | 173/172/173 | 1253/1253/1253 | 492536/492536/492536 | 535830/535826/535830 | 0.999993 (535826/535830) |

T=max(10 Ir,max absolute C0−A1 across all 17 rows)=12 Ir. All six numeric columns must repeat exactly.

| work | A work−prefix (operands) | B work−prefix (operands) | paired B−A (operands) |
|---|---|---|---|
| realloc_large_shrink_8_to_6mib | 6986092 (15424053−8437961) | 6291691 (14729652−8437961) | -694401 (6291691−6986092) |
| realloc_large_shrink_8_to_4p5mib | 5240236 (13678197−8437961) | 4718827 (13156788−8437961) | -521409 (4718827−5240236) |
| realloc_large_shrink_8_to_3mib | 3494380 (11932341−8437961) | 3494383 (11932344−8437961) | 3 (3494383−3494380) |
| realloc_large_grow_6_to_8mib | 9083253 (15424062−6340809) | 9083250 (15424059−6340809) | -3 (9083250−9083253) |
| realloc_large_equal_8mib | 8388814 (16826787−8437973) | 8388811 (16826784−8437973) | -3 (8388811−8388814) |

| RSS scenario | statistic | A bytes | B bytes | B/A (num/den) | allowed B bytes (A+1% A) |
|---|---|---:|---:|---|---:|
| realloc_large_shrink_8_to_6mib | median absolute post-realloc RSS (9 processes) | 16855040 | 10555392 | 0.626245 (10555392/16855040) | 17023590.4 |
| realloc_large_shrink_8_to_4p5mib | median absolute post-realloc RSS (9 processes) | 15282176 | 10547200 | 0.690163 (10547200/15282176) | 15434997.76 |
| realloc_large_shrink_8_to_3mib | median absolute post-realloc RSS (9 processes) | 13697024 | 13701120 | 1.000299 (13701120/13697024) | 13833994.24 |
| realloc_large_grow_6_to_8mib | median absolute post-realloc RSS (9 processes) | 16850944 | 16850944 | 1.000000 (16850944/16850944) | 17019453.44 |
| realloc_large_equal_8mib | median absolute post-realloc RSS (9 processes) | 10547200 | 10547200 | 1.000000 (10547200/10547200) | 10652672 |
| cycles8to6 | max lifetime VmHWM including setup (9 processes, 20 cycles) | 16863232 | 10563584 | 0.626427 (10563584/16863232) | 17031864.32 |

Computed gate: NO-GO. Failures: Ir gate realloc_grow.
Paired values are explanatory, not replacement gates. Cache-model columns are secondary, not native cycles/wall-clock. RSS uses absolute A denominators, not baseline-subtracted deltas.
<!-- r16_perf83:tables:end -->
