# Implementation plan — the "radical performance optimization" audit (2026-07-10)

**Source:** `docs/reviews/2026-07-10-radical-performance-optimization-audit.md`
(sections 3–13; written read-only against `e6b9b3a`, which is the current
`main` tip — verified: `git rev-parse HEAD` = `e6b9b3aa…`).

**Cross-referenced against (all read in full):**

- `docs/perf/PERF_PLAN_2026-07-10-post-review-action-plan.md` (groups G1–G11)
- `docs/perf/IAI_BASELINE.md` — specifically the G1 honest-reject section, the
  X4/X5/X6/E2 reject ledger, the five Post-PERF-PASS-N reference tables, and
  the "Session summary — five passes, eleven action groups" closer
- the five original reviews (`docs/reviews/2026-07-10-perf-*.md`)
- the actual source at every file:line the audit cites (verified below,
  per item — the audit's references were checked against `e6b9b3a`, not
  trusted)

This is a planning document. Nothing below has been implemented. Ratings use
this session's established idiom (HIGH/MEDIUM/LOW confidence, LOW/MEDIUM/HIGH
risk); no new rating scheme is invented. Wall-clock numbers are quoted with
the same honesty framing PASS-1..5 used: `sample_size(10)` on a noisy Windows
host — ratios and deterministic Ir are the signal, single ns figures are not.

---

## 1. Executive summary — the realistic ceiling after PASS-1..5

**Is the audit's own evidence solid?** Mostly yes, and it is appropriately
honest about it. Section 1 states outright that the host is noisy, that
`sample_size(10)` numbers are "ориентиры" (rough guides), and it explicitly
disclaims the mimalloc 256 B/1024 B steady-state figures as high-variance
("нельзя трактовать как точный коэффициент"). The claims it actually builds
on are the robust ones:

- **Steady-state churn is won.** Wall-clock agrees with the deterministic
  judge: `small_churn_16b` marginal cost is 124.2 Ir/op (Post-PERF-PASS-5
  table), and the wall-clock table shows Sefer at or ahead of mimalloc at
  every steady-state size. This is order-of-magnitude-stable evidence, not a
  noise artifact.
- **Cold/bulk tiny is the remaining scalar gap.** The ~2.3x wall-clock gap at
  16 B bulk (~39.5 vs ~17.1 ns/pair) is directionally consistent with the
  deterministic 68–70 Ir/op cold-vs-churn budget (192.6 vs 124.2 Ir/op) that
  the iai table pins exactly. The precise 2.3x ratio should be treated as
  ±noise; the existence and rough size of the gap should not.
- **One place the audit slightly overclaims:** it presents the 68–70 Ir/op
  delta as "реальный оптимизационный бюджет" for the tiny bulk path. Not all
  of that budget is removable — a large fraction is the refill/carve/flush
  machinery itself (already batch-optimized in W4/E1+E3), not overhead. The
  removable slices are (a) the free-side magazine scan in free-storms (the
  Э10 comment in `heap_core.rs` confirms `cnt` sits at `TCACHE_CAP=16` before
  every overflow in exactly this pattern — worth roughly 15–25 Ir/op of the
  budget, not all 68) and (b) per-call TLS/classify/route repetition, which
  only a batch API removes. Expectations below are set against that
  narrower honest budget.

**What is genuinely left after PASS-1..5** (each verified in source, §2):

1. **Registry bootstrap materialization** — deterministic, structural, and
   invisible to every current judge: `bootstrap.rs:541-555` writes
   `next_free` into all 4096 slots at a 7040 B stride (`heap_slot.rs`
   documents the align(64) stride), dirtying ~4096 distinct pages ≈ 16 MiB
   RSS, on a ~27.5 MiB reservation that (post PASS-1
   reserve-then-commit-exact) is fully commit-charged on Windows. First-touch
   latency + RSS defect, not a hot-path defect.
2. **Wide oscillating working sets** — PASS-3's own honest report already
   measured the residual: 173 (256 B) and 367 (1024 B) decommit calls remain
   because the pool is hard-capped at `POOL_MAX_SLOTS = 4`
   (`alloc_core.rs:94`, silent clamp at `:541`) while the public config
   advertises `.pool_segments(8)` (`small_segment_pool_config.rs:83`).
3. **Sustained cross-thread fan-in** — `heap_core.rs:1452/1458` discards
   `ring.push` overflow (`let _ =`), an unbounded logical leak under
   producer→consumer pressure. Correctness-flavored, invisible to every
   current bench.
4. **Cold/bulk tiny scalar path** — the narrower budget above; the two
   levers are the MagazineBitmap experiment and a batch API.

Realistic ceiling: double-digit percentages on those *specific shapes*
(startup RSS/latency, wide working sets, bulk tiny, fan-in robustness) — and
approximately **zero** further on steady-state churn, which every proposal
must treat as the won front with the established ±10 raw-Ir kill threshold
on the churn benches.

---

## 2. Deduplicated evaluation of every audit item

The audit's sections 3–12 contain ~14 distinct proposals. They deduplicate
into **9 workstreams** (merge notes inline, following the original PERF_PLAN
synthesis pattern).

### E1. Registry bootstrap cost — audit §4 (P0 chunked registry) MERGED WITH §12.5 (startup benchmark) and §4's own "process-per-sample benchmark" list

**Merge note:** §4's fix, §4's verification list, and §12.5's "first-process
benchmark" are one workstream from two angles — you cannot judge the fix
without the harness, and the harness has no other consumer. Merged.

- **Overlap with PASS-1..5 / rejects:** none. The prior session's registry
  work (the `static` → lazy `AtomicPtr` bootstrap, documented in
  `bootstrap.rs`'s header) solved a *binary-size* problem (~22 MB of
  `.data`), explicitly not the RSS/commit/first-touch problem. No PASS
  touched registry materialization. Genuinely new.
- **Source verification:** accurate. `MAX_HEAPS = 4096` (`bootstrap.rs:161`);
  the init loop writes `next_free = NEXT_FREE_TAIL` for all 4096 slots
  (`bootstrap.rs:541-555`); `HeapSlot` stride is 7040 B post-PASS-4
  `repr(align(64))` (`heap_slot.rs:218-223` — the audit's "примерно 7040 B"
  is exact); 4096 × 7040 ≈ 27.5 MiB; each write lands on a distinct page
  (stride > 4 KiB), so ~16 MiB demand-zero RSS at first allocation. One
  stale detail: `bootstrap.rs`'s own prose still says "~22 MB" / "~5 KiB per
  slot" (pre-align numbers) — fix the comment while there.
- **Design notes (for the implementing phase, not decided here):**
  - The audit's `[AtomicPtr<Chunk>; 64]` × 64-slot chunks keeps slot
    addresses stable, the 4096 cap, and the 16-bit `TaggedPtr` index intact.
    Cold paths only: `claim`/`recycle`/`pick_slot`/stats aggregation
    (`heap_registry.rs:878/933` iterate `0..count` — becomes per-chunk) and
    abandoned adoption. The TLS-bound hot path caches `&HeapSlot` and never
    re-derives it — untouched, as the audit claims.
  - **Cheaper first step worth evaluating in-phase:** the eager `next_free`
    loop may be removable outright. `next_free` is only read for slots on
    the free stack, and a slot enters the stack via a push that writes
    `next_free` first; a slot minted by `count.fetch_add` never has it read
    before that. If a full audit of `next_free` reads confirms this, lazy
    init kills the 16 MiB first-touch RSS and most of the latency *without*
    restructuring — chunking then remains only for the Windows commit-charge
    (the full REGISTRY_SIZE is committed at reserve). Do the read-audit
    first; it may split this into a trivial win + a smaller follow-up.
- **Confidence:** HIGH (defect is deterministic and structural).
  **Risk:** MEDIUM — touches the `bootstrap.rs` unsafe seam and the
  claim/recycle Treiber protocol; NOT H1-adjacent, NOT decommit-adjacent.
- **Verification:** new process-per-sample harness (first-alloc latency;
  RSS/commit after 1/8/64 heaps; concurrent first-chunk race), full registry
  test suite + miri on registry tests, loom is not required (the chunk
  publication is a single CAS-publish idiom already used by `REGISTRY_PTR`).
  `npm run iai` must stay flat (registry bootstrap is outside every bench's
  measured window except via `SeferAlloc::new()` — watch
  `large_alloc_free_cycle` as the bootstrap proxy; it may legitimately DROP).

### E2. Scalable/honest small-segment pool — audit §6 (P1) MERGED WITH §12.2 (silent clamp) and the preset half of §9

**Merge note:** §6 proposes the intrusive pool + honest cap + presets; §12.2
is the same clamp complaint as code-quality; §9's `low-rss`/`balanced`/
`throughput` presets appear in both §6 and §9 — one retention-policy
configurability idea from two angles. Merged; the *feature split* half of §9
stays separate (E7).

- **Overlap with PASS-1..5:** builds directly ON PASS-3's Mechanism-2 pool —
  not a re-proposal. PASS-3's own honest report is the motivating evidence:
  the pool fully absorbed decommit churn at 64 B but left 173/367 decommit
  calls at 256 B/1024 B because demand exceeds the 4-segment cap. This is
  the session's own recorded unfinished business.
- **Source verification:** accurate. `POOL_MAX_SLOTS: usize = 4`
  (`alloc_core.rs:94`); silent clamp
  `core.pool_cap = by_segments.min(by_bytes).min(POOL_MAX_SLOTS)`
  (`alloc_core.rs:541`, repeated at `:619`); the public builder doc example
  literally shows `.pool_segments(8)` (`small_segment_pool_config.rs:83`)
  which the runtime silently reduces to 4. The OOM-path pool drain the audit
  mentions is indeed already in `e6b9b3a` (tip commit).
- **Design notes:** fixed honest cap via intrusive owner-only links through
  small-segment headers first (O(1) admit/remove/evict, `AllocCore` keeps
  only head/tail/count/byte-budget — no per-slot array growth multiplied by
  MAX_HEAPS); *adaptive* cap is a stretch goal to be justified by the sweep
  harness, not landed on faith. Header link fields must re-run the PASS-5
  layout const-asserts. Defaults stay 4 / 16 MiB (behavior-preserving);
  raising defaults is a policy question (§5 Q2).
- **Confidence:** HIGH that a larger cap absorbs the measured residual (the
  mechanism already demonstrably works at 64 B). **Risk:** MEDIUM — segment
  lifecycle / Mechanism-2 / D1 invariants; `regression_c3_unbounded_recycle`
  and the PASS-3 pool suite are the guards; not H1-adjacent (owner-only).
- **Verification:** `pool_cap_sweep` (working_set_cycle parameterized by cap
  0/1/4/8/16) — the audit's stage-A item 2; existing `working_set_cycle`
  with `decommit_calls` deltas; full decommit test set + miri on the
  targeted lifecycle tests; `npm run iai` flat on all 12 (pool bookkeeping
  is off the magazine hot path).

### E3. Overflow-safe cross-thread free — audit §7 (P1), two-stage

- **Overlap with PASS-1..5:** none. PASS-4 touched the ring (empty-guard +
  cursor-block false sharing) but never the producer-side overflow drop.
- **Source verification:** accurate. `RING_CAP = 256`
  (`remote_free_ring.rs:166`); both push sites discard the result —
  `let _ = ring.push(packed);` (`heap_core.rs:1452` hardened, `:1458`
  non-hardened); the ring's own docs call overflow "a bounded leak (sound)"
  per-event (`remote_free_ring.rs:162`) — the audit's point is that
  *repeated* overflow is an unbounded cumulative leak and pins `live_count`
  above zero, blocking decommit/recycle. Correct.
- **Assessment:** this is as much a robustness/correctness item as a perf
  item, and the only P0/P1 item touching the H1-adjacent MPSC family — the
  mechanism that produced this codebase's one confirmed UB bug. It must be
  split:
  - **E3a — non-dropping overflow fallback** (heap-level MPSC overflow
    stack, or bounded retry + fallback): closes the leak with the smallest
    protocol delta. The ring protocol itself stays untouched; the fallback
    is a new, separate Treiber-style structure (the codebase already has two
    proven instances of this idiom: `abandoned_segs`, A1 deferred-large).
  - **E3b — dirty-segment queue**: the perf half (owner drains only dirty
    segments; removes O(n segments) ring polling on refill miss). Higher
    risk (empty→queued flag races, slot recycle/adoption, delayed
    publication) — the audit itself calls the protocol high-risk and
    mandates loom. Gate on E3a landing plus fan-in harness evidence that
    polling actually costs something.
- **Confidence:** HIGH on the defect's existence; MEDIUM on the perf upside
  (no current bench exhibits it). **Risk:** E3a MEDIUM, E3b HIGH
  (H1-adjacent both; loom + miri mandatory).
- **Verification:** new `remote_fanin` harness (audit stage-A item 4) that
  FIRST reproduces the leak on current main (ring occupancy, overflow count
  — the process-wide D2 counter already exists at `remote_free_ring.rs:136`
  — reclaimed vs attempted, RSS growth) so the fix has a red→green
  counterfactual; loom tests for the fallback push/drain interleavings;
  miri on xthread tests; `npm run iai` flat (single-threaded benches never
  overflow).

### E4. Exact O(1) magazine membership (MagazineBitmap) — audit §3 (P0) — a bounded GO/NO-GO experiment, not a committed win

- **Overlap with the G1 reject — the key question.** The G1 reject
  (IAI_BASELINE.md) closed with: "the shape to try is NOT a simple bit
  redefinition but a design that (a) audits every `mark_alloc`/`mark_free`
  call site … and (b) resolves whether `is_in_magazine` in
  `reclaim_offset_checked` becomes provably redundant or must be kept."
  The audit's separate-bitmap design is a genuine answer to that, not
  repackaging:
  - Blocker (a) — `mark_alloc` semantics inversion at the four
    freelist-drain sites and `carve_batch`'s deliberate leave-unset
    optimization — is sidestepped entirely: `AllocBitmap` keeps its exact
    current semantics at every call site; the new bitmap is orthogonal
    state (set on magazine push, cleared on pop).
  - Blocker (b) — the H1-adjacent `is_in_magazine` question — gets a
    concrete resolution: the predicate is NOT removed (the reject's fear);
    it becomes an O(1) owner-read bitmap probe with identical semantics.
    The ring protocol itself is untouched; only the owner-side predicate
    implementation changes. Still needs the full #164/R1 regression suite
    re-run (`drain_resident_xthread_double_free_no_corruption`,
    `refill_window_does_not_double_issue_in_out_buffer_resident_block`,
    the M2 counterfactual trio) — but this is test-gated MEDIUM, not the
    reject's unresolved-design blocker.
  - It is also not the X4-B bloom shape: X4-B's closing note dismissed
    "per-slot bits keyed by magazine slot" as "just the scan again" — this
    is per-*block-offset* bits, O(1) probe by offset, no scan. New shape.
- **Source verification:** accurate. The scan is the Э10 branchless chunked
  loop at `heap_core.rs:1026-1044`; the "cnt is 1–3 in churn, pins at 16 in
  free-storms" characterization matches the code's own comments (`:940`,
  `:1007`). The 32 KiB / 0.78% memory cost is right (matches `AllocBitmap`'s
  own geometry). PASS-2's virgin-init skip must be *extended* to the new
  bitmap (and the poison-counterfactual test extended with it) — the audit
  says this; it is load-bearing, since otherwise the fix re-adds the 32 KiB
  zero-init PASS-2 just removed.
- **Honest expected value — lower than the audit's P0 billing.** The win
  side collects only where the scan is long (bulk/free-storm free path,
  `cnt≈16` → roughly 15–25 Ir/op of the 68 Ir/op cold budget, plus the
  `reclaim_offset_checked` scan on xthread drains). The cost side is a
  metadata store on *every* magazine push AND pop — the won front, where the
  kill threshold is ±10 raw Ir per churn bench (≈0.16 Ir/op at 64 pairs).
  Two stores + index math per op-pair will plausibly exceed that unless the
  bitmap line stays L1-resident (it may: 16 B-granule offsets within one
  working set share lines). This is exactly the X4-B arithmetic that killed
  the bloom. The experiment is worth running because the shape is new and
  the cold/bulk upside is real, but the prior probability of a churn-clean
  GO is moderate at best — plan for a NO-GO ledger entry as a fully
  acceptable outcome (the project has five precedents).
- **Confidence:** MEDIUM. **Risk:** MEDIUM-HIGH (M2/D1 invariant chain +
  segment-metadata layout shift: `small_meta_end`, decommit-reset zeroing,
  hardened gen-table offsets, PASS-5 layout asserts).
- **Verification:** `npm run iai` marginal Ir/op vs the Post-PERF-PASS-5
  table (churn = kill gate; cold/recycle = win gate) AND criterion 16/64/256
  B churn + bulk patterns; M2 counterfactual suite proven non-vacuous
  (re-break, observe red, restore); miri on double-free/decommit tests;
  xthread regression suite for the predicate swap.
- **The audit's cheaper alternative** (`trusted-fast` feature disabling the
  M2 scan on the `GlobalAlloc` path) is a *policy* item — technically
  trivial, but it weakens a documented defence-in-depth guarantee. Escalated
  to §5 (Q4), not scheduled.

### E5. Batch API — audit §5 (P0/P1) — human-gated public surface

- **Overlap with PASS-1..5:** none. W4's `carve_batch`/E3 batching and the
  refill/flush primitives are internal; no public batch surface exists or
  was ever proposed. The audit is correct that the internals are ready
  (`refill_class_bump` `alloc_core.rs:2507`, `flush_class`, `carve_batch`
  via `dbg_carve_batch` at `:2051`).
- **Assessment:** highest *ceiling* of any item for workloads that adopt it
  (the per-call TLS-resolve/classify/route/M2 repetition is exactly what
  scalar `GlobalAlloc` cannot amortize), and zero benefit for everyone else.
  It is new public API: `unsafe fn alloc_batch/dealloc_batch` on
  `SeferAlloc` (or a safe `Pool<T>` wrapper), with a new unsafe contract
  (duplicate detection within a batch, layout uniformity, feature-gating,
  one-file-one-export placement). That is a product/design decision — §5
  Q1 — and per this repo's conventions it is not decided unilaterally here.
  Scheduled as a gated phase with the design questions enumerated.
- **Confidence:** HIGH on mechanism (primitives proven), MEDIUM on adoption
  value. **Risk:** MEDIUM — new public unsafe contract; internals reuse
  proven code; no protocol changes.
- **Verification:** new `batch_alloc_free` criterion bench (scalar vs batch,
  16/64/256/1024 B — audit stage-A item 5); the scalar side must reproduce
  the current ~39.5 ns/16 B bulk figure as its baseline; M2-analog
  duplicate-in-batch tests; miri.

### E6. Hybrid per-class segment index — audit §8 (P2) — conditional, correctly gated

- **Overlap with rejects:** this is the X5 territory. The X5 honest-reject
  explicitly reserved re-opening "at ≥64 segments with a bench that models
  it" and noted the cheapest bitmap variant is correctness-proven and
  recoverable from the ledger. The audit complies with the gate (threshold
  activation at count ≥ 64, keep the scan below it) and honestly says
  current benches (≤3 segments) cannot judge it. Not a duplicate — but not
  actionable until the harness exists and shows a cost.
- **Source verification:** `find_segment_with_free_impl` is the O(n)
  index-driven walk (`alloc_core.rs:3194-3260` — the audit's description
  matches; note the line numbers drifted from the pre-PASS numbers the old
  plan used, the audit's characterization is current).
- **Confidence:** LOW until measured (X5's mechanism analysis — the header
  line is already hot at small n — stands). **Risk:** MEDIUM (transition
  bookkeeping on the dealloc slow path; X5 measured exactly that cost).
- **Verification:** new `multiseg_refill` harness at 64/128/512 segments
  FIRST; only if it shows material scan cost does the index get built.

### E7. Split caching from decommit policy (features/presets) — audit §9 (P2) — human-gated

- **Overlap:** none landed; the prior plan's §5 Q1 (Mechanism-2 default
  policy) is adjacent but narrower. Verified: the large cache is genuinely
  compiled under `alloc-decommit` (`alloc_core.rs:55,72` — "The cache is
  ONLY active under `alloc-decommit`"), so a user cannot have
  large-cache-without-small-decommit today. The audit's gap is real.
- **Assessment:** mechanically LOW-MEDIUM risk cfg refactor, but it changes
  the public feature matrix (`production`'s meaning, CI matrix, README's
  feature inventory, `npm run check`'s three matrix entries). Feature-set
  design is a product decision — §5 Q3.
- **Verification:** CI matrix build of every new combination; behavior
  parity tests (`production` must remain byte-identical by default);
  `npm run iai` flat.

### E8. Over-aligned classification LUT — audit §10 (P3) — defer, per the audit's own numbers

- **Overlap with X6:** adjacent but distinct — X6 rejected clz for the *seed*
  lookup; this targets the `align > 16` forward walk
  (`size_classes.rs:171-181`, verified: 0–3 iterations typical). Not
  re-litigating X6.
- **Assessment:** the audit's own evidence kills its urgency:
  `aligned_churn_640b_a128` = 124.4 vs 124.2 Ir/op plain churn. There is no
  measured cost to remove. Park until a real workload profile shows heavy
  align-32/64/128/4096 traffic. No phase allocated.

### E9. Region/containers + code-quality — audit §11 + §12.1/12.3/12.4

- **§11 `DenseRegion<T>`:** verified claims match the existing measurements
  (iteration ~30% faster, lookup ~16% slower, churn ~3x slower — default
  must not change). A new backend type is product surface — §5 Q5. No
  overlap with any PASS. LOW risk, LOW urgency; demand-gated.
- **§11 `SyncRegion` guard-batching:** a documentation/API-usage note, not
  allocator work. Fold into E5's design discussion if `Pool<T>` happens.
- **§12.1 split `alloc_core.rs`:** verified — 4875 lines. Aligns with the
  repo's one-file-one-export convention; PASS-1..5 kept touching this file
  and every future phase here (E2, E4) does too. A mechanical
  re-export-preserving split *reduces* zero-trust review cost for
  everything after it, at the price of one iai re-pin (binary layout
  shifts) and one large-but-mechanical review. Scheduled early.
- **§12.3 stale `rss_probe` prose:** verified stale — the process-wide
  overflow counter exists (`remote_free_ring.rs:136`, task D2) while
  `examples/rss_probe.rs:18-22,538-540` still says it needs adding.
  Trivial doc fix; fold into Phase 0.
- **§12.4 publish reuse-path + lifecycle-inclusive churn numbers:** merges
  with the prior plan's open question 3 (reporting policy) — same call,
  still unanswered. §5 Q6.

---

## 3. Prioritized, phased implementation plan

Ordering = (impact × confidence) / risk, with judges pulled forward, exactly
as PASS-1..5 sequenced. Each phase is one coherent, independently-committable
unit with its own tests (repo phase rules apply: tests in `tests/`, zero-trust
review, `npm run check` before any push, iai re-pin when binary layout
shifts). Phases 1–5 need no human policy input; 6–8 are gated.

### Phase 0 — judges first (bench/docs only, no allocator code)

- **Do:** (a) `first_alloc_process` harness — process-per-sample first-alloc
  latency + RSS/commit-charge probe (Criterion cannot measure this in-process;
  a small runner script per sample, like `scripts/iai.mjs`'s pattern);
  (b) `pool_cap_sweep` — `working_set_cycle` parameterized over pool cap
  0/1/4/8/16 with `decommit_calls` deltas; (c) `remote_fanin` — MT
  producer→consumer harness recording ring occupancy, the D2 overflow
  counter, reclaimed-vs-attempted, RSS — must demonstrate the overflow leak
  RED on current main; (d) fix the stale `rss_probe` prose and the stale
  "~22 MB / ~5 KiB" comment in `bootstrap.rs`.
- **Files:** `benches/` + `scripts/` + `examples/rss_probe.rs` +
  `src/registry/bootstrap.rs` (comment only).
- **Impact:** none directly; unblocks honest judging of Phases 1, 3, 4 (the
  audit's stage A, minus `multiseg_refill` and `batch_alloc_free`, which
  move into the phases that consume them).
- **Risk:** LOW (bench/docs only). Fast-profile per repo convention
  (`sample_size(10)`, short times).
- **Verify:** the fan-in harness reproduces overflow on main
  (counterfactual for Phase 4); the first-alloc harness reproduces the
  ~16 MiB RSS / commit spike on main (counterfactual for Phase 1).
- **Depends on:** nothing.

### Phase 1 — registry bootstrap cost (E1) — the top pick

- **Do:** step 1: audit all `next_free` reads; if the eager init loop is
  removable (lazy init at free-stack push), land that alone — it kills the
  4096-page first-touch. Step 2: chunked registry
  (`[AtomicPtr<Chunk>; 64]` × 64 `HeapSlot`s, CAS-publish on first touch,
  stable addresses, 4096 cap and TaggedPtr encoding unchanged) to fix the
  Windows commit-charge and make RSS scale with the thread high-water mark.
  Steps are separately committable.
- **Files:** `src/registry/bootstrap.rs`, `src/registry/heap_registry.rs`
  (slot accessor + count/free-stack/abandoned iteration), possibly
  `src/registry/heap_slot.rs` (no layout change intended).
- **Expected impact (audit's numbers, verified plausible):** first chunk
  ~440 KiB vs 27.5 MiB reservation; ~64x less bootstrap init work; RSS
  proportional to live heaps. Invisible to iai (may drop the bootstrap
  proxy Ir — a bonus, not the judge).
- **Risk:** MEDIUM — unsafe bootstrap seam + Treiber claim/recycle; no hot
  path, no H1, no decommit involvement. Why top-ranked: highest
  confidence-to-risk ratio of any code change (deterministic structural
  defect, bounded blast radius, judge built in Phase 0).
- **Verify:** `first_alloc_process` before/after; full registry suite +
  miri (registry tests are miri-covered); cross-chunk claim/recycle/adopt
  tests (new); `npm run iai` — no regression on all 12.
- **Depends on:** Phase 0(a).

### Phase 2 — split `alloc_core.rs` (E9/§12.1) — maintainability enabler

- **Do:** mechanical split into small-path / small-pool / large-path /
  large-cache modules, `mod.rs` reexports only (repo convention), zero
  behavior change. Deliberately BEFORE Phases 3 and 5, which edit exactly
  these regions — smaller reviewable diffs afterward.
- **Files:** `src/alloc_core/alloc_core.rs` → several files + `mod.rs`.
- **Impact:** none at runtime (must be provably none).
- **Risk:** LOW mechanically; one iai re-pin (binary layout shifts ≈ the
  PASS-3 noise-band precedent, every bench moved ≤55 Ir).
- **Verify:** full `cargo test --features production` + the other two
  matrix entries; `npm run iai` re-pin with a "layout-shift only" honesty
  note; `npm run check`.
- **Depends on:** nothing (can run parallel to Phase 1; commit after it to
  keep bisection clean).

### Phase 3 — scalable/honest small-segment pool (E2)

- **Do:** intrusive owner-only pool links in small-segment headers; O(1)
  admit/reuse-remove/evict; real configurable cap (honor
  `.pool_segments(8/16/64)` bounded by byte budget); REMOVE the silent
  clamp (either honor or hard-error — resolved cap observable either way);
  defaults unchanged (4 / 16 MiB). Adaptive cap only if `pool_cap_sweep`
  data motivates it — separate commit, same phase at most.
- **Files:** `src/alloc_core/` (pool module post-split),
  `src/alloc_core/segment_header.rs` (link fields — re-run PASS-5 layout
  asserts), `small_segment_pool_config.rs`.
- **Expected impact:** eliminate the remaining 173/367 decommit calls in
  `working_set_cycle` at 256 B/1024 B when the user raises the cap; PASS-3
  measured −16.3%/−13.8% wall-clock from absorbing only *part* of the
  churn, so full absorption plausibly exceeds that on those shapes — but do
  not promise beyond what the sweep shows; RSS trade is explicit and
  user-opted.
- **Risk:** MEDIUM — Mechanism-2/D1 lifecycle invariants;
  `regression_c3_unbounded_recycle` + PASS-3 pool suite + decommit set are
  the guards; owner-only (not H1-adjacent).
- **Verify:** `pool_cap_sweep` + `working_set_cycle` with decommit-counter
  deltas (wall-clock judge — iai is blind here, per the CR3 precedent);
  full decommit/miri set; `npm run iai` flat on all 12.
- **Depends on:** Phase 0(b); Phase 2 (file split) preferred first.

### Phase 4 — non-dropping cross-thread overflow fallback (E3a)

- **Do:** heap-level overflow path for `ring.push` failure (Treiber MPSC
  stack per heap, the `abandoned_segs`/A1 idiom) so no freed block is ever
  lost; owner drains fallback after rings. Do NOT build the dirty-segment
  queue here (that is Phase 7).
- **Files:** `src/registry/heap_core.rs` (`:1452/:1458` push sites),
  `src/alloc_core/remote_free_ring.rs` or a new seam file for the fallback
  stack, drain sites in `alloc_core`.
- **Expected impact:** correctness/robustness (unbounded logical leak +
  blocked decommit under fan-in → gone); perf-neutral by design on
  non-overflow workloads.
- **Risk:** HIGH-adjacent handled as MEDIUM by construction: the ring
  protocol is untouched; the new stack is a separate, well-precedented
  structure. Loom still mandatory (push-vs-drain, slot recycle boundary),
  as is miri on xthread tests — H1-family discipline.
- **Verify:** `remote_fanin` red→green (Phase 0(c) counterfactual); loom;
  miri; `npm run iai` flat (single-threaded benches never overflow); full
  xthread regression suite.
- **Depends on:** Phase 0(c). Independent of Phases 1–3.

### Phase 5 — MagazineBitmap GO/NO-GO experiment (E4)

- **Do:** second per-segment bitmap (mark on magazine push, clear on pop;
  flush = clear+`mark_free`; refill = existing `mark_alloc` +
  `mark_magazine`; direct-substrate untouched); replace both O(count) scans
  (own-thread free oracle at `heap_core.rs:1026-1044` and
  `reclaim_offset_checked`'s predicate) with the O(1) probe. EXTEND the
  PASS-2 virgin-init skip + poison counterfactual to the new bitmap.
  Segment-layout offsets re-audited (meta end, decommit reset, hardened gen
  table).
- **Expected impact:** cold/bulk 16–64 B free-storm path — honestly, a
  slice of the 68 Ir/op budget (est. 15–25 Ir/op), NOT the audit's implied
  full budget; plus O(1) xthread reclaim.
- **Risk:** MEDIUM-HIGH (M2/D1 chain, layout shift, hot-path store).
  **GO/NO-GO:** GO only if cold/recycle marginal Ir/op improves
  meaningfully AND every churn bench stays within ±10 raw Ir (the X4-B
  kill threshold). A NO-GO gets a full reject-with-numbers ledger entry in
  IAI_BASELINE.md per project precedent — that outcome is planned for and
  acceptable.
- **Verify:** `npm run iai` marginal columns vs Post-PERF-PASS-5; criterion
  churn+bulk at 16/64/256 B; M2 counterfactual trio re-broken/re-proven;
  xthread suite (#164/R1 tests) for the predicate swap; miri.
- **Depends on:** Phase 2 (touches the split files); independent of 1/3/4.
  Sequenced after the certain wins because its expected value is a
  coin-flip experiment.

### Phase 6 — batch API (E5) — GATED on §5 Q1

- **Do (if approved):** `unsafe alloc_batch`/`dealloc_batch` (and/or safe
  `Pool<T>`) over the existing refill/flush/carve primitives; batch-level
  duplicate check; group by segment/class before flush. New
  `batch_alloc_free` bench built first (scalar baseline red line ≈ current
  bulk numbers).
- **Risk:** MEDIUM (new public unsafe contract; proven internals).
- **Verify:** new bench scalar-vs-batch; duplicate-in-batch M2 tests; miri;
  `npm run iai` flat (scalar path untouched).
- **Depends on:** human decision; Phase 2 helpful.

### Phase 7 — dirty-segment queue (E3b) — GATED on Phase 4 + fan-in evidence

- **Do (if `remote_fanin` shows drain-polling cost at real segment counts):**
  producer sets a once-per-empty→non-empty queued flag publishing the
  segment base; owner drains only dirty segments. Loom coverage for
  flag-clear races, adoption, delayed publication — the audit's own list.
- **Risk:** HIGH (new MPSC protocol surface — full H1-grade discipline).
- **Depends on:** Phase 4; the `remote_fanin` + (if built) `multiseg_refill`
  evidence. Note overlap: this also removes part of E6's motivation for
  xthread-driven scans — evaluate E6 only after this.

### Phase 8 — conditional bucket (all gated on evidence or policy)

- **E7 feature/preset split** — gated on §5 Q3 (product call).
- **E6 hybrid per-class index** — gated on a `multiseg_refill` harness
  (64–512 segments) demonstrating real scan cost; respects the X5 reject.
- **E8 over-aligned LUT** — gated on a real workload profile (audit's own
  124.4 vs 124.2 evidence says: not now).
- **E9 `DenseRegion<T>`** — gated on §5 Q5 (product demand).

### Dependency sketch

```
Phase 0 (judges + doc fixes)
  ├─(a)→ Phase 1 (registry)          — independent of everything else
  ├─(b)→ Phase 3 (pool)              — after Phase 2 preferably
  └─(c)→ Phase 4 (overflow fallback) → Phase 7 (dirty queue, gated)
Phase 2 (alloc_core split) → Phase 3, Phase 5 (smaller diffs there)
Phase 5 (MagazineBitmap GO/NO-GO)   — after 2; outcome may be NO-GO ledger
Phase 6 (batch API)                  — gated on Q1
Phase 8 (conditional bucket)         — gated per item
Every phase: npm run check + iai re-pin when binary layout shifts.
```

---

## 4. Explicitly out of scope / already covered (do not re-propose)

Verified against IAI_BASELINE.md and git history (`50c07b0`…`e6b9b3a`); the
audit's own §2 list checks out and is restated here with provenance:

- **LTO/codegen tuning, bench teardown fix + `working_set_cycle`, vmem
  reserve-then-commit-exact / exact-mmap-first** — PASS-1 (`50c07b0`,
  `6d83442`, `bcf4d79`). Done.
- **Virgin `AllocBitmap` init skip, foreign-dealloc outlining** — PASS-2
  (`25ae4a5`, `3dcb2e4`). Done. (Phase 5 must extend, not reopen, this.)
- **Mechanism-2 hysteresis pool + large-cache best-fit** — PASS-3
  (`0be4823`). Done; Phase 3 extends its cap, which PASS-3's own report
  flagged as the residual — an extension, not a re-proposal.
- **Ring-drain empty-guard + false-sharing partitions** — PASS-4
  (`7cdb26d`, `021f654`). Done.
- **`SegmentHeader`/`Tcache` reorder; `AllocCore` reorder measured as no-op**
  — PASS-5 (`ca9e70a`, `a329b35`, `1fc6dd3`). Done.
- **G1 bit-redefinition of `AllocBitmap`** — honest-reject 2026-07-10; only
  the separate-bitmap shape (Phase 5) may proceed, under the reject's own
  stated conditions.
- **`TCACHE_CAP=32`; bloom-gated M2 scan** — X4 rejects (won-front rule).
- **Per-class segment bitmap at small n** — X5 reject; re-open ONLY behind a
  ≥64-segment bench (Phase 8/E6 honors the gate).
- **clz `class_for`** — X6 reject (10/11 EstCycles regressions).
- **`REFILL_N` LUT (E2), `REFILL_BATCH` > 31** — rejected.
- **`alloc_zeroed` virgin-payload skip** — P4(b) NO-GO stands.
- **Run-encoded freelist** — prior NO-GO stands (CR2's re-litigation
  precondition — G2 landed + fault-aware harness — is now met, but nothing
  in this audit re-proposes it; leave closed until someone brings a new
  design).
- **"500x decommit gap"** — retracted bench artifact; no proposal may cite
  it.

---

## 5. Open questions for a human (policy/product — not decided here)

1. **Batch API surface (Phase 6 gate):** should `SeferAlloc` grow public
   `unsafe alloc_batch`/`dealloc_batch`, a safe `Pool<T>` type, both, or
   neither? Feature-gated (`experimental` first?) or straight to stable
   surface? This sets a public unsafe contract the project must honor
   long-term; it also implicitly positions the crate for DBMS/ECS/arena
   consumers. Pure product call.
2. **Pool defaults and presets (Phase 3):** with an honest cap available,
   do defaults stay at 4 / 16 MiB (behavior-preserving, recommended for the
   phase itself), and do we ship named presets (`low-rss` / `balanced` /
   `throughput`)? Same RSS-vs-latency ownership question the prior plan's
   Q1 left open — it is still unanswered and now blocks only the *defaults*,
   not the mechanism.
3. **Feature split (Phase 8/E7):** splitting `alloc-large-cache` /
   `alloc-small-decommit` / `alloc-small-pool` out of `alloc-decommit`
   changes what `production` means, the CI matrix, and README guidance.
   Worth the surface-area growth? (Cargo features are additive; existing
   users of `production` must see zero change by default.)
4. **`trusted-fast` mode (audit §3's alternative):** a feature that disables
   the M2 in-magazine scan on the `GlobalAlloc` path — contractually
   defensible (double-free is already caller UB) but it retires a documented
   defence-in-depth guarantee for that configuration. Explicit opt-in
   separate from `production`, or not offered at all?
5. **`DenseRegion<T>`:** is there a consumer for a sweep-optimized region
   backend, or is this speculative surface? (Default `Region<T>` must not
   change either way — the audit and the measurements agree.)
6. **Churn reporting policy (audit §12.4 = prior plan Q3, still open):**
   publish both reuse-path and lifecycle-inclusive churn numbers in
   `bench:table`/README? This changes cited headline numbers for
   methodology reasons — the same honesty-of-reporting call flagged last
   time, still unowned.
