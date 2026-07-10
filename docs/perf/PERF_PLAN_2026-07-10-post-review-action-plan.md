# Perf action plan — synthesis of the five 2026-07-10 reviews

**Sources (read in full, reconciled here):**

- `docs/reviews/2026-07-10-perf-fastpath-review.md` (FP)
- `docs/reviews/2026-07-10-perf-churn-reuse-review.md` (CR)
- `docs/reviews/2026-07-10-perf-xthread-atomics-review.md` (XT)
- `docs/reviews/2026-07-10-perf-memory-layout-review.md` (ML)
- `docs/reviews/2026-07-10-perf-large-segment-review.md` (LS)

**Grounding (already-closed context, not re-litigated):**
`docs/checkpoints/2026-07-09-perf4-decommit-mechanism1-fix.md` (PERF-4
Mechanism 1 shipped; Mechanism 2 explicitly deferred),
`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md` (P4(b) NO-GO),
`docs/perf/IAI_BASELINE.md` (X4/X5/X6/E2 honest-rejects; judge blind spots).

This is a synthesis/planning document. Nothing below has been implemented.
Finding references use the shorthand above (e.g. FP1 = fastpath review
finding 1). Confidence/risk labels are the source reviews' own — none were
re-graded here.

---

## 1. Executive summary

The five reviews converge on two separate diagnoses, one per headline
symptom. **The 2x small-alloc cold/churn gap (16–256B vs mimalloc) is
structural instruction-and-cache-line work per op**, not atomics: the
xthread review's inventory confirms zero atomic RMWs/fences on the
own-thread fast path, and the fastpath review attributes the single largest
line-item (~25–35% of the gap, HIGH confidence) to the O(count) in-magazine
double-free scan that runs ~15 pointer compares per free in the benchmarked
steady state (FP1 = XT1, same root cause from two angles). The remainder is
spread across free-path cache-line breadth (FP2, ML1, ML5, ML6 — MEDIUM
confidence each) and a complete absence of `[profile.release]` tuning (ML3
— HIGH confidence the config is suboptimal, MEDIUM on magnitude). **The
1024B churn blow-up (224ns vs 47.5ns cold) is, with HIGH confidence and an
empirical bisect behind it, almost entirely NOT a reuse-path problem**: the
churn-reuse review measured the actual free-then-realloc loop at 29–30ns/op
(already beating mimalloc) and isolated the inflation to the zero-hysteresis
empty-small-segment decommit→release→re-reserve lifecycle (CR1), which the
large-segment review independently reached by structural analysis (LS1) —
both name the deferred PERF-4 "Mechanism 2" as the fix. Two cross-cutting
facts shape sequencing: the deterministic iai judge is provably blind to the
fault/syscall cost class that dominates the churn symptom (CR3), and the
Windows reservation primitive double-commits 8MiB per 4MiB segment (LS2),
which inflates the very cost Mechanism 2 would be judged against. Honesty
note: all ns figures are wall-clock on a noisy Windows host — the reviews
treat ratios and the bisect/probe evidence as the signal, and this plan
preserves that framing.

---

## 2. Deduplicated finding groups

Twenty-plus individual findings collapse into **11 distinct root causes**.
Merges are stated explicitly; "leave alone" verdicts are in §4.

### G1: Magazine double-free oracle — O(count) scan per free
**Merges:** FP1 + XT1. XT1 says verbatim it is "covered in depth by
[FP] finding 1 — same root cause, different angle of arrival": FP saw it as
instruction count, XT as memory-touch breadth (~4 dependent, potentially
cold cache lines per free). One mechanism, one fix.
Every own-thread small free scans `tcache.slots[c][0..cnt]` for the pointer
(`src/registry/heap_core.rs:1005-1044`); in the benchmarked cold/churn
pattern `count[c]` oscillates 15↔16, so the "cnt is 1–3" comment is wrong
for this workload. Fix direction: fold magazine membership into the
per-segment `AllocBitmap` already loaded on this path (bit 1 = "not owned
by user"), collapsing the oracle to the existing `is_free(off)` test and
potentially subsuming the decommit-reset `bump_of()` guard
(`heap_core.rs:1065-1068`). FP: HIGH confidence, ~25–35% of the 2x gap,
MEDIUM risk (M2/D1 invariant chain).

### G2: Zero-hysteresis empty-small-segment release → Mechanism-2 pool
**Merges:** CR1 + LS1 (independent convergence on the same fix) + CR2
(first-touch faults on the free path via `flush_run`'s `write_next` — its
recurrence disappears once G2 keeps pages warm) + CR5 (the `production`
bundling dimension, carried into §5 as a policy question) + LS6
(`MADV_FREE`, conditional companion that the keep-registered variant makes
unnecessary).
The moment a non-current Small segment's `live_count` hits 0, the whole
4MiB reservation is released (`src/alloc_core/alloc_core.rs:1070`, `:1121`,
`:3447-3449`, `:2836-2841` → `segment_table.rs:381` → `os::release_segment`).
Oscillating working sets pay release + re-reserve + metadata re-init + a
full set of demand-zero faults per oscillation; cost scales with block size
(16B never leaves the primordial segment). CR1: HIGH confidence (feature
bisect: `fastbin` without `alloc-decommit` → 44.5ns/op flat; isolated probe
reproduced 20 decommit/release/re-reserve cycles and 715ns teardown frees
at 1024B). This is exactly the "Mechanism 2 (hysteresis)" the PERF-4
checkpoint deferred. Two design variants exist: the checkpoint's sketch
(pool of decommitted segments, recommit on reuse) vs LS1's cheaper
**keep-registered-as-is** variant (leave the emptied segment in the table —
its freelists are fully populated, reuse via `find_segment_with_free` is
zero extra work; release from the existing decay tick or beyond a retention
budget, mirroring `large_cache` at `alloc_core.rs:73`). CR is explicit that
the pool must keep pages **committed** — decommit-then-cache re-faults on
touch, which is most of the measured cost. MEDIUM risk (both reviews).

### G3: Judge/bench blind spots for the fault/syscall cost class
**Merges:** CR3 + CR4. Tooling, not allocator code.
(a) iai/callgrind Ir is structurally blind to page faults and syscalls —
`recycle` vs `cold` Ir are near-identical (205.3 vs 204.5 Ir/op) while
wall-clock diverges 4.7x at 1024B; the PERF-4 checkpoint's instruction to
gate Mechanism 2 on `npm run iai` is therefore unfulfillable as written.
Fix: add a `working_set_cycle` criterion bench (multi-working-set
batched-teardown shape) as the canonical Mechanism-2 judge, optionally
recording `dbg_decommit_count`/`dbg_segments_released_total` deltas.
(b) `benches/global_alloc.rs:118-125`: at 1024B under decommit the timed
teardown is ~85% of the reported "churn" number (~183µs of ~208µs), far
beyond the documented ~20% skew (task #24). Fix: make teardown genuinely
untimed via a drop-guard, AND keep one intentionally-timed variant as the
Mechanism-2 signal. Both HIGH confidence, LOW risk.
Note: this is a *criterion/wall-clock* harness, deliberately NOT the
fault-axis CI judge idea that review F10 already declined (see §4) — no CI
gating, just a local measurement instrument.

### G4: OS reservation primitive — per-platform syscall waste
**Merges:** LS2 (Windows) + LS4 (Unix). Same function
(`crates/vmem/src/lib.rs`, `reserve_aligned_raw`), same defect family
(over-reserve + trim), independent per-platform fixes.
Windows (`lib.rs:341-378`): `MEM_RESERVE|MEM_COMMIT` of size+align commits
8MiB per 4MiB segment, then decommit-trims — a transient 2x commit-charge
spike, page-table population for pages discarded microseconds later, 3
syscalls. Fix: reserve-only, then commit just the aligned span (2 syscalls,
zero over-commit); release path untouched. LS2: HIGH confidence, LOW risk —
LS's single highest-confidence recommendation.
Unix (`lib.rs:473-509`): try exact-size `mmap` first, fall back to
over-reserve on misalignment (mimalloc's trick). MEDIUM confidence, LOW
risk. Caveat both: invisible to the iai judge (Ir doesn't price syscalls;
iai runs the Linux path under WSL) — wall-clock validation only.

### G5: Fresh-segment metadata init dead work (32KiB bitmap memset)
**Source:** LS3 (no merge partner, but a hard design interaction with G1 —
see sequencing).
Every fresh small segment zeroes a 32KiB `AllocBitmap` + 1024 PageMap bytes
(`alloc_core.rs:4109-4116`, `bootstrap.rs:74-80`, `alloc_bitmap.rs:80-86`)
on memory the OS guarantees is zero — a tautology that eagerly dirties 8
metadata pages. Fix: skip `init_in_place` on the virgin-reserve call sites
under `cfg(not(miri))`; optionally flip `PageClass::Free` to 0. LS3
explicitly distinguishes this from the rejected P4(b) (segment-level exact
virgin signal, metadata not user payload, decommit-reuse path excluded by
construction). HIGH confidence on soundness, MEDIUM on magnitude, MEDIUM
risk (needs a poison counterfactual + call-site audit).
**Interaction warning:** G1's bonus step ("segment reset sets payload bits
to 1 instead of 0") inverts the virgin bitmap state; if G1 adopts it, a
fresh segment's correct bitmap state is no longer all-zeros and G5's
tautology argument breaks (skip would become a bug, or init becomes
memset(0xFF)). G1 and G5 must be designed together.

### G6: No `[profile.release]`/`[profile.bench]` tuning at all
**Source:** ML3.
`Cargo.toml:442-463` declines a profile section only for debug-info
reasons; benches run at 16 codegen units, no LTO, unwind landing pads —
while mimalloc's C core is compiled at full optimization by its build
script. Fix: `lto = "thin"` (or fat) + `codegen-units = 1`, optionally
`[profile.bench] panic = "abort"`. HIGH confidence config is suboptimal,
MEDIUM confidence on magnitude, effectively ZERO risk. ML calls it "the one
lever that applies to every instruction of the measured benchmark and costs
nothing to try first." Known consequence: iai baselines shift → re-pin per
the IAI_BASELINE.md regeneration rule.

### G7: Owner-private hot-structure layout
**Merges:** ML1 (SegmentHeader hot fields straddle the exact 64B split at
offset 64 — reorder so the small-segment per-op set fits bytes 0..64) + FP2
(restructure `Tcache` so `count` and top-of-stack share a line —
`tcache.rs:105-112`) + ML5 (`count: [u16;49]` → `u8`, fits one line —
subsumed by FP2's restructure) + ML6 (`AllocCore` cold `large_cache` placed
ahead of hot `table.own_cache` — fold into the same hot-header pass).
Merged because all four are owner-private, single-threaded cache-footprint
edits with no cross-thread protocol dimension, best done and measured as
one layout pass. Confidence HIGH-MEDIUM (ML1) down to LOW (ML6); risk LOW
throughout (`repr(C)` + `offset_of!` accessors keep it deterministic; ML
notes `size_of` for `SegmentHeader` stays 104 so downstream offsets are
byte-identical). ML's own tempering: the magazine-hit path already touches
only ~4 hot lines — layout alone won't close 2x.

### G8: Cross-thread false sharing (physical residue of the H1 hoist)
**Merges:** ML2 + ML4. Both are false-sharing partitions that only
materialize under cross-thread traffic — irrelevant to the single-threaded
benchmarks that triggered this investigation, which is why they are grouped
apart from G7 despite also being "layout".
ML2: the cache line at `HeapSlot` offset 6976..7040 holds the owner's
`last_stamped_segment`/`id` AND the remote-CASed `thread_free` AND the next
slot's `state`/`generation` (stride 7024 not a 64-multiple). Fix: 64B-align
the remote-access fields into a sub-struct + `#[repr(align(64))]` on
`HeapSlot` (stride 7040, +64KiB registry-wide). HIGH confidence on
existence, MEDIUM on real-world impact, LOW risk.
ML4: `RemoteFreeRing` `head`+`tail`+`overflow`+slots[0..12] share one line
(`remote_free_ring.rs:394-412`); widen `CURSOR_BLOCK` 16→128. Same
confidence/risk profile.

### G9: Unconditional ring-drain on every refill free-list miss
**Source:** XT2 (XT's one actionable finding).
`find_segment_with_free_impl` (`alloc_core.rs:2808-2842`) unconditionally
runs `RemoteFreeRing::drain` per owned segment — including an unconditional
`head.store(h, Release)` that dirties the cursor line even in a process
that never does one cross-thread free. `is_empty()`
(`remote_free_ring.rs:663`) exists for exactly this and is dead code. Fix:
Relaxed `tail` load vs owner-cached `head` guard before draining. HIGH
confidence, MEDIUM risk — same MPSC protocol family as the H1 bug; XT
requires a loom test for the empty-check-vs-concurrent-push race and the
slot re-claim boundary. Expected win: moderate on cold-storm/churn, ~zero
on magazine-hit steady state.

### G10: Small code-motion and classification cleanups
**Merges:** FP3 (outline the fully-inlined cross-thread dealloc tail into
`#[cold] #[inline(never)]` — `heap_core.rs:1285-1433`) + FP4 (collapse the
dominant `align <= 16 && size <= SMALL_MAX` alloc classification to a
single guarded LUT index — `heap_core.rs:576-603`,
`size_classes.rs:161-182`) + FP5/XT5 (duplicate `classify` on the Large leg
— FP5 says not worth changing standalone; XT5 says trivial impact; folded
here as a rider on FP4's call-site shrink).
Merged as "low-risk instruction-level trims to the same two call sites",
each individually small (FP4: ~5–10 Ir/alloc). MEDIUM confidence (FP3,
FP4), LOW / LOW-MEDIUM risk.

### G11: Large-cache first-fit → best-fit
**Source:** LS5. Scan all 8 slots for the smallest compatible
`usable_size` (`alloc_core.rs:3514-3524`) instead of taking the first ≤2x
match; avoids both the wasted-RSS hit and the subsequent forced miss. HIGH
confidence on correctness, MEDIUM-LOW on measured impact (needs a
mixed-size large workload to demonstrate), LOW risk.

---

## 3. Prioritized action plan

Ordering = (expected impact × review-stated confidence) / review-stated
risk, with tooling prerequisites pulled forward. Tiers, not a strict serial
queue — Tier A items are independent of each other.

### Tier A — cheap, near-zero-risk, do first (they re-baseline everything else)

**A1. G6 — add `[profile.release]` tuning** (`Cargo.toml`)
- Do: `lto = "thin"`, `codegen-units = 1`; optionally `[profile.bench]
  panic = "abort"`. One config edit, no code.
- Impact: unquantified by the review (MEDIUM confidence on magnitude), but
  it applies to every instruction of every measured benchmark.
- Risk: effectively zero (ML3). No unsafe, no semantics.
- Verify: `npm run bench:table` + `npm run iai` before/after; **re-pin the
  iai reference table** (IAI_BASELINE.md's own rule: regenerate before
  diffing new work). CI perf-gate thresholds may need re-pinning.
- Sequencing: FIRST — every later Ir/wall-clock delta should be measured
  on the tuned profile, or it will be conflated with this change.

**A2. G3 — bench harness fixes** (`benches/global_alloc.rs`, new criterion
bench)
- Do: (a) untimed teardown via drop-guard in the existing churn bench;
  (b) new `working_set_cycle` criterion bench (multi-working-set
  batched-teardown shape from CR's probe) with
  `dbg_segments_released_total` delta reporting — the designated
  Mechanism-2 judge. Fast criterion profile per repo convention
  (`sample_size(10)`, short warm-up/measurement).
- Impact: no allocator speedup; unblocks honest judging of G2 and stops
  the published churn table reporting ~85% teardown at 1024B.
- Risk: LOW (bench-only).
- Verify: the new bench reproduces the 1024B anomaly on current main
  (~200+ns/op with nonzero release-counter deltas) and the fixed churn
  bench converges toward the ~30ns/op reuse-path figure.
- Sequencing: before G2's go/no-go measurement. Independent of A1.

**A3. G4 — fix `reserve_aligned_raw` per platform** (`crates/vmem/src/lib.rs`)
- Do: Windows two-step reserve-then-commit-subrange (`:341-378`); Unix
  exact-size-mmap-first with over-reserve fallback (`:473-509`).
- Impact: per segment reservation — Windows: removes a guaranteed 2x
  transient commit + ~4MiB of discarded page-table population + 1 syscall;
  Unix: 3 syscalls → 1 in the common case. Compounds with everything that
  reserves segments.
- Risk: LOW (LS2 HIGH confidence / LS4 MEDIUM; fallback preserves current
  behavior exactly; release path untouched). Touches the vmem unsafe seam
  but only the call pattern, not the safety contract.
- Verify: wall-clock only — `segment_decommit_cycle` criterion bench and
  `npm run bench:table`; iai is blind to this (LS2 caveat). vmem crate's
  own tests.
- Sequencing: before G2's measurement — G4 shrinks the miss-path cost that
  G2's pool avoids, so landing G4 first prevents overstating G2's win.

### Tier B — the two big levers (independent of each other; can run in parallel)

**B1. G2 — Mechanism-2 hysteresis pool for empty small segments**
- Do: design-first (the PERF-4 checkpoint mandates a judge-measured design
  cycle). Preferred variant per LS1: keep the emptied segment registered
  as-is (freelists intact, pages committed), track "empty since tick T" /
  "N empty retained", release from the existing decay tick
  (`maybe_decay_large_cache` pattern, `alloc_core.rs:73`, `:3772`) or
  beyond budget. Wire at the three recycle-on-empty sites
  (`alloc_core.rs:1070`, `:1121`, `:3447-3449`, ring-drain `:2836-2841`).
  CR's hard constraint: pages stay COMMITTED (no decommit-then-cache).
- Impact: CR1's measured upper bound (`fastbin` bisect) — 1024B churn
  224.3ns → ~45ns/op, i.e. from 2.6x slower than mimalloc to ~2x faster.
  Wall-clock numbers, noisy host; the bisect/probe evidence is the signal.
  Also dissolves CR2's fault-on-free recurrence for free.
- Risk: MEDIUM (both CR1 and LS1). Touches segment lifecycle + decommit
  invariants (D1, stale-ring guards); no new unsafe expected; NOT
  H1-adjacent (owner-only lifecycle). `regression_c3_unbounded_recycle`
  guards the unbounded-retention failure mode.
- Verify: the A2 `working_set_cycle` bench + release-counter deltas is the
  judge — explicitly NOT `npm run iai` alone (CR3: Ir is blind here). Also:
  full decommit test set (`decommit_soak`, `decommit_miri_cycle`,
  `decommit_stale_ring`, `regression_c3_unbounded_recycle`), miri on the
  targeted decommit/reuse tests, proptest ~64 cases on the reuse invariant.
  `seg_cycle_decommit_256k` iai bench must not regress (it still measures
  the release path when the pool overflows).
- Sequencing: after A2 (judge exists) and A3 (fair baseline). Policy
  questions in §5 need answers before defaults are chosen, but the
  mechanism + opt-out knob can be built ahead of that call.

**B2. G1 — fold the magazine double-free oracle into the AllocBitmap**
- Do: redefine bitmap bit 1 as "block not owned by the user" (set on
  magazine push AND freelist membership, clear on pop/carve); the M2
  double-free oracle becomes the `is_free(off)` test already on the path.
  Touches `dealloc_own_thread_with_base` (`heap_core.rs:1005-1077`),
  `refill_class_bump`, `flush_class`, decommit-reset semantics.
  **Design jointly with G5** (see interaction warning in §2/G5): decide
  the virgin-segment bitmap polarity once, for both changes.
- Impact: FP1's estimate — ~25–35% of the 2x small-alloc gap; the single
  largest line-item separating ~131 Ir/op from mimalloc's ~40–50. Bonus:
  may subsume the `bump_of()` stale-free guard (one fewer header line per
  free — also addresses part of XT1's line-breadth framing).
- Risk: MEDIUM (FP1) — the M2/D1 invariant chain is the project's core
  guarantee. Stays in safe code (bitmap ops via the node seam), owner-only
  (no cross-thread protocol change, not H1-adjacent).
- Verify: counterfactual double-free regression tests re-run and proven
  non-vacuous (would fail without the guard); M2 proptest ~64 cases; miri
  on the targeted double-free/decommit tests; `npm run iai`
  (`small_churn_16b`, `cold_alloc_free_256x16b` marginal Ir/op are the
  judges — this one IS Ir-visible, unlike G2). Watch the X4-B precedent:
  churn is the won front, ±10 Ir kill threshold on the hot benches.
- Sequencing: independent of B1. Measure after A1's baseline re-pin.

### Tier C — measured follow-ons

**C1. G5 — fresh-segment metadata init elision** (after/with B2's polarity
decision)
- Do: skip `AllocBitmap::init_in_place` + shrink PageMap init on the
  virgin-reserve sites only (`alloc_core.rs:4109-4116`,
  `bootstrap.rs:74-80`), `cfg(not(miri))` (miri's alloc fallback is
  uninitialized — keep explicit zeroing there).
- Impact: a few thousand Ir per fresh segment off `cold_*`,
  `multiseg_cold_256k`, `seg_cycle_decommit_256k`; 8 fewer dirty pages per
  segment (LS3; IAI_BASELINE.md names the 32KiB bitmap-init as a dominant
  bootstrap component). Note B1 reduces fresh-reserve frequency, so C1's
  steady-state impact shrinks once B1 lands — it remains a cold-start win.
- Risk: MEDIUM — needs the poison-then-assert counterfactual and an audit
  that no third path calls `init_in_place` on dirty memory.
- Verify: `npm run iai` (directly visible); poison counterfactual test;
  full decommit-reuse test set unchanged.

**C2. G9 — ring-drain empty-guard** (loom-gated)
- Do: wire the dead `is_empty()` logic as a pre-drain guard in
  `find_segment_with_free_impl` (`alloc_core.rs:2808-2842`): Relaxed `tail`
  load vs owner-cached `head`; skip drain (including the Release
  head-store) when empty.
- Impact: XT2 — "moderate on cold-storm/churn benches, ~zero on
  magazine-hit steady state"; removes ~3 atomics × N segments per refill
  miss in single-threaded processes.
- Risk: MEDIUM and treated with H1-grade suspicion despite touching only
  the consumer side: same MPSC protocol family. XT's requirement: loom test
  covering empty-check-vs-concurrent-push and the slot re-claim boundary,
  and re-establishing the single-consumer cached-head argument across slot
  release→claim. Miri on the targeted xthread tests.
- Verify: loom + miri first, then `npm run iai` on cold/recycle benches.

**C3. G7 — owner-private layout pass** (one batch: SegmentHeader reorder,
Tcache restructure + u8 counts, AllocCore hot-header fold)
- Impact: modest — ML's own tempering applies (won't close 2x alone); FP2
  removes ~1–2 lines per free.
- Risk: LOW — `repr(C)`/`offset_of!` determinism; re-run the const layout
  asserts in `segment_header.rs` and any header-byte-snapshot tests.
- Verify: `npm run iai` EstCycles columns (the cache-aware judge added in
  X3) + `npm run bench:table`. Measure the batch as one delta.
- Sequencing: after B2 — G1 changes which lines the free path touches, so
  the layout pass should be laid out against the post-G1 hot set.

### Tier D — principled but low-urgency (no current bench demonstrates the win)

**D1. G8 — false-sharing partition** (`HeapSlot` align(64) split; ring
`CURSOR_BLOCK` 16→128). LOW risk, but ML2's own caveat: impact only under
cross-thread Large-free / sustained remote-free traffic — invisible to
every current bench. Do when a multi-threaded bench exists to judge it, or
accept it as an unmeasured hygiene fix with the layout const-asserts as the
only gate. Re-pin any test hardcoding `FOOTPRINT`/meta offsets; verify the
registry-footprint const still fits the primordial segment.

**D2. G10 — code-motion trims** (FP3 outline foreign tail; FP4
classification collapse; Large-leg duplicate-classify rider). LOW risk,
small individually; judge with iai marginal Ir/op, respect the ±10 Ir
churn kill threshold. Natural to fold into B2's touched call sites rather
than as standalone commits.

**D3. G11 — large-cache best-fit.** LOW risk, O(8) scan on a cold path;
LS5 says impact needs a mixed-size large workload to demonstrate — write
that micro-bench first or land it as unmeasured hygiene with correctness
tests only.

### Dependency summary

```
A1 (profile) ──> re-pin iai baseline ──> all later Ir judgments
A2 (bench harness) ──┐
A3 (vmem reserve) ───┴──> B1 (Mechanism-2 pool) ──> C1 impact shrinks (still worth cold-start)
B2 (bitmap oracle) ──┬──> C1 (init elision: shared bitmap-polarity design)
                     └──> C3 (layout pass targets the post-B2 hot set)
C2, D1–D3: independent; C2 gated on loom/miri, D1 on a future MT bench.
```

---

## 4. Explicitly out of scope / already rejected (do not re-propose)

- **P4(b) `alloc_zeroed` virgin-skip** — NO-GO 2026-07-10
  (`docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md`, plan
  P4(b)). LS3/G5 is a *different* question (segment-level exact virgin
  signal, metadata not payload) and does not reopen it.
- **X4** — `TCACHE_CAP=32` and the bloom-gated M2 scan: both measured and
  rejected (IAI_BASELINE.md §X4; bloom lost on the won churn front).
  G1/B2 is not the bloom — it removes the scan rather than gating it —
  but the X4-B ±10 Ir churn kill threshold applies to its judging.
- **X5** — per-class segment queues / free-class bitmap: honest-reject at
  n=3 segments; gated on a ≥64-segment bench that doesn't exist
  (IAI_BASELINE.md §X5; LS explicitly did not re-propose).
- **X6** — clz `class_for` vs the SIZE2CLASS LUT: rejected (10/11 EstCycles
  regressions).
- **E2** — `REFILL_N` const LUT: rejected (regressed vs inlined udiv).
- **Fault-axis CI judge** — declined (review F10, per LS's ruled-out list).
  G3/A2 is a local criterion harness, not a CI gate — it does not reverse
  that decision.
- **PERF-3.5 (run-encoded freelist)** — prior NO-GO stands; CR2's explicit
  instruction: do NOT re-litigate until G2 lands and a fault-aware harness
  (A2) exists to render a non-blind verdict.
- **Leave-alone verdicts from the reviews:** XT3 (the Large-path
  `drain_large_deferred_free` Acquire load is the minimal form and is
  literally the H1 word — leave alone); XT4 (`dealloc_routing` ownership
  test is near-optimal; the mimalloc-style header-compare alternative is
  unsound for the foreign/unmapped case); FP6 and ML7 (TLS resolution,
  `contains_base`, `Node`, `AllocBitmap` structure, `SegmentTable` field
  order — confirmed clean/non-issues).
- **The "500x decommit gap"** — invalidated bench artifact (PERF-4
  checkpoint correction); on the corrected small-decommit cycle Sefer is
  ~4x FASTER than mimalloc. G2's motivation rests on CR's new bisect/probe
  evidence, not on the retracted figure.

---

## 5. Open questions for a human

1. **G2 default policy:** once the hysteresis pool exists, should it be
   on-by-default in `production` (CR5 frames this as a product decision)?
   `production` currently buys RSS-friendliness at a fault-storm price on
   oscillating workloads (the shamir-db 15–18% signal). CR's suggestion —
   defaults of ~4–8 segments / 16–32MiB budget with a documented knob where
   0 = current behavior — needs an owner's yes/no on default-on and on the
   budget numbers. Related: how much committed memory may an idle process
   retain before the decay tick releases it (RSS-vs-latency tradeoff)?
2. **G2 design variant ratification:** keep-registered-as-is (LS1, cheaper,
   no recommit machinery) vs the checkpoint's decommit-then-pool sketch.
   The reviews' technical evidence favors keep-registered (committed pages
   are the point), but the checkpoint explicitly reserved this for a
   design cycle — confirm the variant before implementation, since LS6
   (`MADV_FREE`) only matters under the other variant.
3. **G3/CR4 reporting policy:** after the teardown fix, the canonical
   `npm run bench:table` churn numbers will drop at larger sizes for
   bench-methodology reasons, not allocator changes. Publish both
   (reuse-path number + lifecycle-inclusive number), or switch the headline
   table? This changes the numbers README/CHANGELOG cite — an honesty-of-
   reporting call, per the project's own bench-table provenance rule.
4. **G1 + hardened tier:** B2 redefines a bitmap bit's meaning that the
   `hardened` gen-table guard sits next to. The reviews did not analyze the
   hardened interaction — flag for the design review: does the bit-1
   redefinition change any hardened-tier assumption, and is a hardened iai
   cost re-publication needed after B2?
5. **G6 scope:** profile settings in a library's Cargo.toml only affect
   this workspace's own binaries/benches, not consumers. Should README
   additionally document recommended profile settings for consumers, or is
   that overreach? (Minor, but it touches published guidance.)
