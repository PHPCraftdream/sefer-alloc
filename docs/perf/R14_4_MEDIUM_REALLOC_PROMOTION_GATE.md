# R14-4 — Stage 2 Small/medium→Large realloc promotion: production A/B gate

**Task:** #289 (R14-4). **MEASUREMENT + IMPLEMENTATION, not a promotion
decision.** This document reports what was built and measured; the
GO/CONDITIONAL-GO/NO-GO line in §7 is a **recommendation**, not a decision.
Whether `medium-classes` (already opt-in, unaffected by this task) is
promoted into `production = [...]` in `Cargo.toml` remains a separate,
pre-existing open question this task does not resolve — this task's own
scope is narrower: implement Stage 2 of the promotion mechanism the design
doc (`docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`) approved,
gated behind the EXISTING `medium-classes` feature, and report honestly
whether it clears R10-2's realloc kill-gate. `Cargo.toml`'s
`production = [...]` list is untouched by this task (one additive
`[[example]]` entry only).

**Date:** 2026-07-23. **Base revision:** `main` @ `a3434df` (R13-2 landed;
this is the next P1 task in the R14 queue). **Platform measured:** Windows
10 Pro x86-64 (native) for wall-clock/RSS/commit; WSL2 (Ubuntu 24.04) +
Valgrind/Callgrind (available this session) for the deterministic
instruction-count (iai) axis. Single physical host — see §8.

---

## 0. Headline summary

**UPDATED VERDICT (R18-2, task #331, 2026-07-26) — see §7.1 for the full
current numbers.** The table below and this section's original
"~1,700–2,300× slower" framing were measured on code carrying a real 4 MiB
Large-segment leak (fixed by R17-4/task #321) and predate R18-3's (task #330)
`kind_at` narrowing; §7.1 explicitly says that old framing "must NOT be cited
as an argument against promoting `medium-classes` — it was confounded by the
leak." The CURRENT, re-verified numbers on post-R17-4/R18-3 code (`main` @
`912740f`) are ~1,180× (combo 1+2, `production,medium-classes`) / ~380×
(combo 3, with `large-cache-extended`) slower on the realloc sub-window —
still RED, still does NOT clear the 20% kill-gate — but the original §0
figures below overstate the gap because of the leak. The original table and
prose are preserved unchanged below as historical record (§7.2 keeps them
explicitly as "Original verdict at task time"); read §7.1 before drawing any
conclusion from this section alone.

| # | Measurement | Baseline | Treatment (Stage 2 promotion) | Verdict |
|---|---|---|---|---|
| 1 | R10-2 judge, realloc phase, 10 paired A/B/B/A rounds | `medium_off` ≈ 15.3–17.7 ms/20-round-block (≈29–42 µs/realloc) | `medium_on` (promotion active) ≈ 65.5–71.2 ms/20-round-block | mean Δ = **−69.1 ms** (A−B), t=−142.3, sign 10/10 A-faster → **medium_on still ~1,700–2,300× slower — does NOT clear the 20% kill-gate** |
| 2 | R10-2 judge, alloc phase | ≈15.3–17.7 ms | ≈15.9–17.1 ms (faster) | mean Δ = **+2.68 ms** (A−B, B faster), t=51.5, sign 10/10 B-faster → **unaffected by this task, still a clear win (~31×, matches R10-2)** |
| 3 | R10-2 judge, free phase | ≈15.3–17.7 ms | much faster | mean Δ = **+13.6 ms** (A−B, B faster), t=97.3, sign 10/10 B-faster → **unaffected, still a clear win (~211×, matches R10-2)** |
| 4 | iai, `realloc_grow` (64 B → 4 MiB, 16 doublings, single object, crosses the 256 KiB threshold at step 13) | 513,192 Ir (`production`) | 513,373 Ir (`production,medium-classes`, promotion active) | **+181 Ir, +0.035% — no measurable regression on the deterministic axis for a single-object growth-through-the-ladder scenario** |
| 5 | Unit tests (a)–(d), design doc §6 test plan | — | all pass, non-vacuous (red/green counterfactual confirmed) | **PASS** |
| 6 | Feature-OFF non-disturbance (`production`, no `medium-classes`) | — | full suite green, promotion code compiles out entirely | **PASS** |

**The core finding is a genuine split verdict.** The promotion mechanism
itself is implemented correctly (§2, §3 — verified independently, not just
by the implementing agent's claim) and does not regress the deterministic
single-object iai axis at all (#4). But the SAME mechanism, measured against
R10-2's EXACT original realloc-heavy workload (16 concurrently-live objects,
`LARGE_CACHE_SLOTS = 8`), does **not** close R10-2's ~2,111× regression —
it remains within the same order of magnitude (~1,700–2,300×). §5 explains
why: that specific workload's promoted working set (16 objects) oversubscribes
the 8-slot large-object cache, so roughly half of every round's promotions pay
a genuine fresh OS `VirtualAlloc` reservation instead of a cache hit — a cost
that swamps the ladder-walk copies the promotion was designed to eliminate.

---

## 1. What was implemented

`src/registry/heap_core_free.rs`, `HeapCore::realloc`'s own-segment branch:
a new step (2.5), inserted between the existing in-place attempt (OPT-F/OPT-G)
and the existing move leg, active only when:
- `medium-classes` is compiled in (`#[cfg(feature = "medium-classes")]`);
- the resize is a GROW (`new_size > old_layout.size()`);
- the block currently classifies Small/medium
  (`SizeClasses::class_for(old_layout.size().max(MIN_BLOCK), old_layout.align()).is_some()`);
- `new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256 KiB).

When all hold, a new private helper `HeapCore::try_promote_to_large` is
called: it bounds the read via `safe_payload_read_span` (R2-1 parity with
the existing move leg), runs the SAME ownership-hook bookkeeping
`HeapCore::alloc`'s Large branch performs (A1 deferred-large drain under
`alloc-xthread`, the `HeapOverflow` drain under `alloc-xthread` without
`fastbin`, `stamp_segment_owner` on the result), calls
`AllocCore::alloc_large(new_size, old_layout.align())` **directly** (not
through `self.alloc(padded_layout)` — see §1.1 for why that would be wrong),
copies the full old buffer (`Node::copy_nonoverlapping`, same span the
existing move leg copies on a grow), and frees the old block via
`self.dealloc`. On success the old block is already freed; on failure
(`alloc_large` returns null) the old block is left completely intact and the
caller falls through to the existing move leg unchanged.

No new bookkeeping was needed, confirming the design doc's §4.2 argument: a
promoted block is a genuine, ordinary Large-segment allocation from the
moment `alloc_large` returns it — `SegmentHeader::kind_at(base)` (the same
mechanism every other Large block's `dealloc`/realloc already uses) routes
it correctly with zero new tags, fields, or invariants.

### 1.1 A correctness point found and fixed during implementation

The design doc's §4.1 sketch left the exact promotion call open
("call `self.core.alloc_large(...)` directly"). The natural-looking
alternative — routing through the ordinary `self.alloc(padded_layout)`
entry point (reusing `HeapCore::alloc`'s existing stamping, matching the
existing move leg's own alloc call) — is **wrong** here: under
`medium-classes`, `SMALL_MAX` is 1 MiB, strictly above the 256 KiB
promotion threshold. A `padded_layout` sized to a threshold-crossing but
still-under-1-MiB `new_size` would simply reclassify back into a (larger)
medium class under `class_for`'s ordinary rules — defeating the entire
point of promoting to Large. The implementation therefore calls
`AllocCore::alloc_large` directly (forcing Large classification
unconditionally, bypassing `class_for` entirely) and manually replicates the
bookkeeping `HeapCore::alloc`'s Large branch performs, mirroring
`HeapCore::alloc_zeroed`'s own existing Large branch. This was caught by
test (a) (§4) genuinely failing before the fix (the block never promoted —
`class_for` claimed it back into a medium class every time) and passing
after — not discovered by inspection alone.

---

## 2. Pad-target decision — resolves the design doc's §4.4 open question

**Chosen: pad target = `new_size`, no artificial padding beyond the
caller's request.**

### 2.1 Reasoning

`AllocCore::alloc_large` (`src/alloc_core/alloc_core_large.rs`) rounds every
request up to a whole `SEGMENT` (4 MiB) multiple unless the opt-in
`exact-span-large` feature is enabled — and `production` does not include
`exact-span-large`. So under the mainline `production,medium-classes` build
this task ships against, ANY pad target at or below one `SEGMENT` (2 MiB
fixed, a 512 KiB floor, or plain `new_size` — every candidate in this
sweep's growth sequence tops out at 1 MiB) is moot: `alloc_large` rounds it
up to the same 4 MiB commit regardless. Padding therefore buys no headroom a
bare `new_size` doesn't already get for free from `alloc_large`'s own
rounding, and a caller whose growth pattern needs headroom beyond one
`SEGMENT` is exactly what the separate, already-existing, opt-in
`large-reserved-capacity` feature provides — orthogonal to this promotion,
not something this mechanism needs to duplicate.

### 2.2 Measurement — `examples/r14_4_pad_target_probe.rs`

A throwaway probe (not a shipping artifact) sweeps three candidates at the
256 KiB threshold — fixed 2 MiB, `max(new_size, 512 KiB)` floor, and plain
`new_size` — over an 8-object, 30-round, 8-step (64→1024 KiB) growth
sequence, freeing each round's objects before the next round starts.
**Independently re-run and re-verified by the reviewer (not just the
implementing agent), twice, reproducibly:**

| Mode | ns/growth-seq | `segments_reserved_total` (30 rounds × 8 obj) | `large_cache_hits` | `commit_after_kib` (steady state) |
|---|---:|---:|---:|---:|
| `fixed2mib` | ≈58,300–58,600 | 17 | 232 | ≈67,900 KiB |
| `floor512kib` | ≈257,000–332,000 | not separately captured | not separately captured | ≈1,020,000 KiB |
| `nopad` | ≈289,000–410,000 | 249 | 0 | ≈1,020,000 KiB |

**Correction to the implementing agent's original summary:** the agent's
draft reported `nopad`/`floor512kib` as the LOWER-commit options (~29.9 MiB)
and `fixed2mib` as materially higher (~67.9 MiB, "2.3× more RSS"). The
reviewer's independent re-run — reproduced twice, with `segments_reserved_total`/
`large_cache_hits` instrumentation added temporarily to confirm — found the
**opposite direction**: `fixed2mib` reserves only 17 distinct segments
across the whole 240-object run (232 large-cache hits — i.e. the SAME
committed segment is reused almost every round) and settles at ~68 MiB
steady-state commit, while `nopad`/`floor512kib` reserve up to 249 distinct
segments with **zero** large-cache hits and settle at ~1.0 GiB steady-state
commit — a 15× higher commit cost, not a 2.3× LOWER one.

**Root cause: RESOLVED by R17-4 (task #321).** The anomaly is NOT in the
`alloc_large` cache admission at all (the admission predicate compares
`usable` values that ARE identical 4 MiB across all three modes, exactly as
this task expected). It is on the DEALLOC side, in the fastbin magazine
dispatch (`HeapCore::dealloc_own_thread_with_base`,
`src/registry/heap_core_free.rs`): that dispatch keys on
`SizeClasses::class_for(layout.size())`, NOT on the segment's `kind`. Under
`medium-classes` (`SMALL_MAX` == 1 MiB), a Large segment can LEGITIMATELY be
freed with a layout whose size classifies small — R14-4's promotion diverts
a medium block to a 4 MiB Large segment at the 256 KiB threshold, and OPT-G
then grows it IN PLACE to any size ≤ `SMALL_MAX` while it stays a Large
segment, so its (contract-correct) dealloc layout classifies small. Before
the R17-4 fix such a free was misrouted into the small magazine path: the
Large segment never reached `AllocCore::dealloc`'s Large branch, was never
deposited into `large_cache` nor released, and LEAKED — so every subsequent
promotion reserved a fresh 4 MiB segment.

This maps 1:1 onto the three arms: `nopad`/`floor512kib` end the growth
sequence at a dealloc layout of 1024 KiB (== `SMALL_MAX`, classifies
`Some(54)` → magazine path → leak → 0 hits / 249 segments / ~1 GiB), while
`fixed2mib` ends at 2048 KiB (> `SMALL_MAX`, classifies `None` → fastbin
fall-through → substrate → Large branch → deposit → 232 hits / 17 segments /
~68 MiB). The "request size literally identical" hypothesis (this task's
best guess within its time budget) was a red herring — `alloc_large` rounds
to the same 4 MiB `usable` regardless, so the ALLOC side is symmetric; only
the DEALLOC routing fork on the layout size differs. Confirmed empirically
by instrumenting `alloc_large`'s OPT-E lookup and the magazine dispatch:
`nopad`'s 24 deallocs took `class_for(1048576) → Some(54) → magazine`, never
entering the Large branch (0 deposits); `fixed2mib`'s 24 deallocs took
`class_for(2097152) → None → core.dealloc → Large branch` (24 deposits
across 8 slots).

The fix routes Large segments to the kind-keyed Large dealloc
UNCONDITIONALLY (not gated on `hardened`, and not a no-op — the pre-existing
F7 guard framed this case as a caller contract violation and (hardened-only)
silently leaked it; that framing is false for the promotion+OPT-G case).
After the fix all three pad targets produce statistically indistinguishable
~68 MiB commit and ~35–40 µs/growth-seq — exactly what §2.1's SEGMENT-
rounding argument predicted. Pinned by
`tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs` (red/green
counterfactual: `large_cache_hits` delta 0 before the fix, > 0 after).

**This discrepancy does not change the pad-target decision.** The reasoning
in §2.1 (SEGMENT-rounding makes padding moot under `production`, and
`large-reserved-capacity` is the correct orthogonal lever for callers that
need more) holds independent of which of the three candidates has the
"true" lowest commit in this probe's specific harness shape — the shipping
mechanism does not pad at all (`nopad`'s exact behavior, i.e. the design's
actual implementation), so the shipping mechanism's real commit behavior is
whatever `nopad`'s numbers show, not `fixed2mib`'s. **Flagged as an open
methodology item for a follow-up task**, not resolved here — see §6.

---

## 3. Test plan — design doc §6, all five scenarios

All four `tests/` files below are new, in `tests/`, following the project's
`rNN_M_description.rs` naming convention, NOT inline (per CLAUDE.md). All
re-run independently by the reviewer (not just the implementing agent),
twice, including immediately after a red/green counterfactual — see §3.5.

### 3.1 (a) Move-leg reduction — `tests/r14_4_promotion_move_leg_reduction.rs`

Oracle: pointer identity. A block grown past the 256 KiB threshold, then
grown again within the promoted block's committed span, must return the
SAME pointer on the second grow (OPT-G in-place, no move) — a differing
pointer would mean it took another ladder-walk move-leg instead.

- `second_grow_past_threshold_hits_opt_g_no_move` — **PASS**
- `repeated_post_promotion_grows_all_hit_opt_g` (three consecutive
  post-promotion grows, all must stay in-place) — **PASS**

### 3.2 (b) Correct free after promotion — `tests/r14_4_promotion_free_correctness.rs`

A distinctive, position-dependent canary is written before promotion and
verified to survive the promotion copy; the promoted block is then freed and
checked for no leak (`segments_reserved_total`/`segments_released_total`
bounds) and no corruption of a subsequent, unrelated allocation.

- `canary_survives_promotion_and_free_leaves_no_leak` — **PASS**
- `repeated_promote_and_free_does_not_leak_unboundedly` (20 rounds,
  `reserved_delta <= 40` bound) — **PASS**

### 3.3 (c) Shrink after promotion — `tests/r14_4_promotion_shrink_uses_move_leg.rs`

Oracle: pointer CHANGES on a shrink back below the original medium range —
proving the existing Large→Small move-leg fires (this design adds no
in-place Large→Small shrink fast path, matching the design doc's explicit
non-goal).

- `shrink_below_original_medium_range_relocates_and_preserves_prefix` — **PASS**
- `shrink_back_into_medium_range_also_relocates` — **PASS**

### 3.4 (d) Feature-OFF non-disturbance — `tests/r14_4_promotion_feature_off_non_disturbance.rs`

Gated `#[cfg(not(feature = "medium-classes"))]` (the dual of the other three
files). Confirms that without `medium-classes`, growth across what would be
the medium range behaves exactly like ordinary, pre-existing Large realloc
(no promotion concept applies — there is nothing to promote FROM, since the
block is already Large before the grow).

- `growth_across_the_would_be_medium_threshold_is_ordinary_large_realloc` — **PASS**
- `small_to_large_growth_without_medium_classes_is_the_ordinary_move_leg` — **PASS**

### 3.5 Red/green counterfactual (non-vacuousness proof)

The reviewer personally disabled the promotion call site (renamed its
`#[cfg(feature = "medium-classes")]` guard to a nonexistent feature name,
forcing it to compile out) and re-ran test (a):

```text
test second_grow_past_threshold_hits_opt_g_no_move ... FAILED
test repeated_post_promotion_grows_all_hit_opt_g ... FAILED
  left: 0x238dd450000
 right: 0x238dd4c0000   (pointers differ — promotion did not fire, ladder-walk move-leg ran instead)
```

Both tests fail exactly as predicted without the promotion diversion,
confirming they are non-vacuous. The guard was restored and both tests
pass again (re-confirmed by the reviewer independently).

### 3.6 (e) R10-2 judge re-run — the real number

See §0 rows 1–3 and §5 for the full result and root-cause analysis. Does
**not** clear R10-2's 20% kill-gate for the exact realloc-heavy workload
R10-2 used.

---

## 4. Verification runs (all re-run and confirmed by the reviewer, not just claimed by the implementing agent)

- `cargo test --release --features "production medium-classes"` — full
  suite, **green** (re-confirmed twice by the reviewer; one interleaved run
  hit a `race_repro.rs` `STATUS_STACK_BUFFER_OVERRUN` — reproduced as an
  environmental flake from concurrent CPU contention in this shared
  workspace: `race_repro.rs` shares no code path with this task's change,
  and reran clean in isolation and in a full clean rerun, both independently
  confirmed by the reviewer).
- `cargo test --release --features production` (feature-OFF, test 3.4) —
  full suite, **green** (reviewer-confirmed, exit code 0).
- `cargo test --release --features "production medium-classes-wide"` —
  the three `medium-classes`-only test files, **green** (reviewer-confirmed;
  promotion is correctly gated on `medium-classes` alone, so behaves
  identically under the wide variant).
- `cargo clippy --all-targets --features "production medium-classes" -- -D warnings` — **clean** (reviewer-run).
- `cargo clippy --all-targets --features experimental -- -D warnings` — **clean** (reviewer-run).
- `cargo clippy --all-targets --all-features -- -D warnings` — **clean** (reviewer-run).
- `cargo fmt --check` — **clean** (reviewer-run, both before and after the
  temporary counterfactual edit/restore).
- `cargo test --release --features production --test no_stale_doc_references`
  — **green** (reviewer-run; required updating `README.md`'s tier-2 unsafe
  count 51→52 and `docs/ARCHITECTURE.md`'s test-file count 208→212 for the
  four new test files and one new `#[allow(unsafe_code)]` call-site block —
  both doc-drift fixes are in this task's diff).
- iai (`npm run iai` / `node scripts/iai.mjs`), `production` baseline vs
  `production,medium-classes` treatment, WSL2 + Valgrind/Callgrind
  (available and used this session, contrary to the implementing agent's
  belief that it was unavailable): **see §0 row 4 and §5.2** — `realloc_grow`
  moves 513,192 → 513,373 Ir (+0.035%), no other bench moves meaningfully.
  Raw logs: `docs/perf/_raw_r14_4_iai_baseline.log`,
  `docs/perf/_raw_r14_4_iai_medium.log`.

---

## 5. Why the R10-2 gate does not clear (root cause)

Confirmed independently by the reviewer (re-running `node
scripts/r10_2_medium_gate.mjs --pairs 10` personally, not trusting the
implementing agent's numbers alone):

```text
alloc:   mean Δ (A−B) = +2.683 ms   t=51.496   sign A-faster=0/10  B-faster=10/10  REAL
free:    mean Δ (A−B) = +13.606 ms  t=97.323   sign A-faster=0/10  B-faster=10/10  REAL
realloc: mean Δ (A−B) = -69.085 ms  t=-142.281 sign A-faster=10/10 B-faster=0/10   REAL
```

(A = `medium_off`/baseline `production`; B = `medium_on`/treatment
`production,medium-classes` with Stage-2 promotion active.)

The realloc phase's `medium_on` per-run RESULT lines show
`segments_reserved_total≈324` and `commit_after_kib≈1,330,000` (≈1.3 GiB) —
essentially the SAME leak-shaped signature the pad-target probe's `nopad`
mode showed in isolation (§2.2). R10-2's exact workload
(`examples/_shared/paired_ab_medium_workload.rs`) keeps `WS_LEN = 16` live
objects per round, each starting fresh at 256 KiB and promoting on its very
first grow step — but `LARGE_CACHE_SLOTS = 8`, so at most half the promoted
objects per round can be served from the cache; the rest pay a genuine OS
`VirtualAlloc` + first-touch commit for a fresh 4 MiB segment every single
round. For THIS workload's specific shape (16 objects, 8 cache slots,
promotion on essentially the first grow step), "1 promotion copy replacing
N ladder legs" is dominated by "roughly half of every round's promotions
cost a real OS reservation" — a cost on the same order as the ladder-walk
copies the promotion eliminated. This is an honest negative result for this
exact gate scenario, not a spun one.

**This interacts directly with task #290 (R14-5, large-cache-extended
hardening).** `docs/perf/R13_...` and the `large-cache-extended` feature
already materialize additional cache slots (8 → up to 40) under conditions
R14-5 is scoped to harden. A larger or adaptive large-object cache would
plausibly change this specific gate's verdict — but that is R14-5's scope,
not this task's, and this task does not speculate further on the number.

**The deterministic iai axis (§0 row 4) tells a different, narrower story**:
for a SINGLE object growing through the full ladder (no concurrent working
set, no cache pressure), the promotion adds no measurable instruction-count
cost. The two findings are not in conflict — they measure different things
(single-object growth-path efficiency vs. multi-object cache-pressure
wall-clock) — but the wall-clock number is the one that answers R10-2's
actual kill-gate question, and it is negative.

---

## 6. Open items for a follow-up task (not resolved here)

1. **The pad-target probe's commit-cost discrepancy (§2.2) — RESOLVED by
   R17-4 (task #321).** The mechanism causing `nopad`/`floor512kib` to
   reserve far more distinct segments (0 cache hits) than `fixed2mib` (232
   cache hits) despite all three requesting the same 4 MiB rounded `usable`
   span is now fully root-caused: a fastbin magazine dealloc-dispatch bug
   that keyed on `class_for(layout.size())` instead of segment `kind`, mis-
   routing the dealloc of a promoted-then-in-place-grown Large block whose
   dealloc layout classified small (≤ `SMALL_MAX`) into the small magazine
   path, leaking its 4 MiB segment every round. Fixed in
   `src/registry/heap_core_free.rs` (`dealloc_own_thread_with_base`),
   pinned by `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs`.
   See §2.2 (now closed) for the full trace. As predicted there, this does
   NOT change the pad-target decision (§2.1's SEGMENT-rounding argument is
   independent of the anomaly, and post-fix all three arms are
   indistinguishable).
2. **R14-5's large-cache hardening may flip this gate's verdict — RESOLVED
   (does NOT flip) by R18-2 (task #331).** The re-run was performed on
   2026-07-26 on post-R17-4/R18-3 code (`main` @ `912740f`), for three
   feature compositions: `production` (baseline), `production,medium-classes`,
   and `production,medium-classes,large-cache-extended`. The `large-cache-
   extended` arm (8→40 slots) does substantially HELP the realloc phase
   (~3.5×: 66 ms → 19 ms; cache-hit-rate proxy 46% → 94%) but does NOT bring
   it under the 20% kill-gate (still ~380× slower than the baseline's
   near-zero in-place Large realloc). Full numbers, the SD/mean-delta
   resolvability check, and the same-vs-same control are in §10; the refined
   verdict is in §7's R18-2 block. The root cause is now confirmed as
   structural promotion-copy cost (the 256 KiB memcpy per promoted object
   that dense packing forces on a cross-class realloc-grow), NOT the leak
   R17-4 fixed — the leak inflated COMMIT (1.3 GiB → 50 MiB, fixed), not
   TIME.

---

## 7. Verdict

### 7.1 R18-2 re-run (task #331, 2026-07-26) — CURRENT verdict

The original verdict below (§7.2) was measured on code carrying the real
4 MiB Large-segment leak that R17-4 (task #321, commit `1b761f4`) later found
and fixed, and before R18-3 (task #330, commit `912740f`) narrowed the
`kind_at` check. **That old "1,700–2,300× slower" framing must NOT be cited
as an argument against promoting `medium-classes` — it was confounded by the
leak.** R18-2 re-ran `scripts/r10_2_medium_gate.mjs --pairs 20` (the exact
original harness, zero source/script changes) on current `main` @ `912740f`
for three feature compositions, 20 A/B/B/A pairs (80 process launches) per
phase, 3 phases each. Full numbers + SD/mean-delta resolvability + the
same-vs-same control are in §10; headline:

| Arm B (treatment) vs Arm A=`production` | realloc mean Δ (A−B) | realloc per-op (B) | B/A ratio | `segments` (B) | `commit` (B) | SD/Δ | resolvable? | realloc kill-gate (20%) |
|---|---:|---:|---:|---:|---:|---:|:---:|:---:|
| `production,medium-classes` | **−66.06 ms** | 67.6 µs/realloc | ~1,180× | 172 | 49 MiB | 11.7% | YES | **FAIL (RED)** |
| `production,medium-classes,large-cache-extended` | **−19.38 ms** | 19.6 µs/realloc | ~380× | 20 | 81 MiB | 10.7% | YES | **FAIL (RED)** |
| (control: `production` vs `production`, same-vs-same) | +0.0006 ms | — | — | 329 | 34 MiB | 1229% | NO (expected; t=0.36≪crit, sign 8/12 → harness honesty PASS) | n/a |

(Baseline `production` realloc is ~0.056 ms / ~58 ns per realloc — an
in-place Large header update within the dedicated 4 MiB span, near-zero by
design, so the percentage frame is degenerate; see R10-2 §4.2. The absolute
per-op cost and the SD/Δ ratio are the honest frames. The alloc/free wins
are confirmed fully preserved: alloc Δ≈+3.3–3.7 ms, free Δ≈+14.7–14.9 ms,
B-faster 20/20 in every run — see §10.)

**CURRENT GATE: still RED on R10-2's realloc kill-gate, for BOTH
`production,medium-classes` and `production,medium-classes,large-cache-
extended`.** This is an honest negative result — the R17-4 leak fix and the
R18-3 `kind_at` narrowing did NOT flip the gate. What they DID change:

- **The leak is gone (CONFIRMED, OBSERVED).** `medium_on` commit dropped
  1,330,000 KiB (≈1.3 GiB, R14-4 §5) → 50,518 KiB (≈49 MiB); `segments`
  324 → 172. The dealloc-routing bug that made every promoted-then-freed
  Large segment leak is fixed; the cache now reuses spans as designed.
- **`large-cache-extended` materially helps but does not clear the gate
  (CONFIRMED, OBSERVED).** 40 slots cut the realloc gap ~3.5× (66→19 ms) and
  raised the cache-hit-rate proxy (segments 172→20 ⇒ ~46%→~94% of the 320
  realloc-phase promotions now reuse a cached span), at the cost of higher
  resident commit (50→81 MiB, the expected RSS-for-fewer-OS-round-trips
  trade-off). The residual ~19 ms is the genuine promotion `memcpy`
  (16 objects × 256 KiB × 20 rounds ≈ 80 MiB copied) plus per-promotion
  `alloc_large` bookkeeping even on a cache hit — structural to dense
  packing, not a bug.
- **The environment CAN resolve these effects (CONFIRMED).** SD/Δ is
  2.9–11.7% for every real phase (the host was under heavy load, 66–94%
  CPU, which inflated absolute times and SD, but every measured delta is
  8–34× its own SD). This is the opposite of the R17-7 situation a third
  review flagged (SD 7.7 ms > delta 1.2 ms); here no effect is sub-noise.

**Refined recommendation (not a decision):** the realloc concern for
`medium-classes` is **still open and still RED** even after the leak fix and
even with `large-cache-extended`. Do NOT promote `medium-classes` into
`production` on the strength of this gate. The remaining regression is the
structural memcpy cost of cross-class realloc-grow under dense packing
(R10-2 §5's own diagnosis); closing it would require one of R10-2 §5's
mitigations (in-place medium-class grow within a segment, or growth
headroom / over-allocation within the medium class) — none of which R17-4 or
R18-3 implemented. This task does **not** modify `Cargo.toml`'s
`production = [...]` list.

### 7.2 Original verdict at task time (2026-07-23, on pre-R17-4 leaky code)

- The Stage-2 promotion mechanism is implemented correctly (design doc's
  §4.2 "no new bookkeeping" claim verified for real, not just argued), all
  five design-doc test scenarios (a)–(e) pass with non-vacuous oracles
  (red/green counterfactual confirmed independently), feature-OFF
  non-disturbance is confirmed, and the deterministic iai axis shows no
  regression for the mechanism in isolation.
- **It does NOT clear R10-2's 20% realloc kill-gate for R10-2's own
  realloc-heavy workload** — `medium_on` (with promotion) remains
  ~1,700–2,300× slower than baseline on that exact scenario, independently
  reproduced by the reviewer. The root cause (§5) is cache-slot pressure
  from the specific 16-object/8-slot workload shape, not a flaw in the
  promotion logic itself.
- The alloc/free wins R10-2 originally found (~31×/~211×) are confirmed
  fully preserved (§0 rows 2–3, §5) — this task's change does not touch
  those paths at all.

**Recommendation (not a decision):** do NOT treat this task as having
resolved R10-2's realloc regression for `medium-classes` as a whole. The
promotion mechanism is sound infrastructure worth keeping (gated, as
implemented, behind the already-opt-in `medium-classes` feature — no
`production` change), but a user/orchestrator deciding whether to promote
`medium-classes` into `production` should treat R10-2's realloc concern as
**still open** pending R14-5's cache hardening and a re-run of this exact
gate against that hardened cache. This task does **not** modify
`Cargo.toml`'s `production = [...]` list.

---

## 8. Platform limitation

Single physical host — Windows 10 Pro x86-64 native for wall-clock/RSS/commit,
WSL2 (Ubuntu 24.04) + Valgrind/Callgrind on the same underlying CPU/memory
subsystem for the iai axis. No Linux-native, macOS-native, or multi-socket
NUMA hardware was available to this session (same structural limitation as
every prior R13/R14 gate report in this project).

---

## 9. Artifacts this task adds

- `src/registry/heap_core_free.rs` — `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
  const, the promotion call site (step 2.5 of `realloc`'s own-segment
  branch), and the `try_promote_to_large` private helper — all gated
  `#[cfg(feature = "medium-classes")]`.
- `tests/r14_4_promotion_move_leg_reduction.rs`,
  `tests/r14_4_promotion_free_correctness.rs`,
  `tests/r14_4_promotion_shrink_uses_move_leg.rs`,
  `tests/r14_4_promotion_feature_off_non_disturbance.rs` — new tests (§3).
- `examples/r14_4_pad_target_probe.rs` — throwaway pad-target sweep (§2.2),
  registered in `Cargo.toml` (`required-features = ["alloc-global"]`, one
  additive `[[example]]` entry, no `production` change).
- `docs/ARCHITECTURE.md`, `README.md` — doc-drift count fixes required to
  keep `no_stale_doc_references` green (test-file count 208→212, README
  tier-2 unsafe-site count 51→52).
- `docs/perf/_raw_r14_4_iai_baseline.log`,
  `docs/perf/_raw_r14_4_iai_medium.log` — raw iai logs backing §0 row 4/§5.2.
- This document.
- No `Cargo.toml` `production = [...]` edit; no other `src/` file touched.

---

## 10. R18-2 re-run detail (task #331, 2026-07-26)

This section is the full evidence backing the §7.1 verdict. It is a
MEASUREMENT-ONLY re-run on current `main` @ `912740f` (post-R17-4 leak fix
`1b761f4`, post-R18-3 `kind_at` narrowing `912740f`); **no `src/`, no
`Cargo.toml`, no script was modified** — `scripts/r10_2_medium_gate.mjs` was
invoked exactly as the original R10-2/R14-4 reports specify.

### 10.1 Environment and disclosure

- **Host / CPU:** 11th Gen Intel Core i7-11800H @ 2.30GHz, 8C/16T. Windows 10
  Pro x86-64, native. Power plan: Balanced. `rustc 1.97.0`.
- **Host CPU load during measurement: 66–94% (HIGH — shared dev-host).** This
  is the same structural condition prior rounds flagged (R17-7, R14-3 §2.4).
  It inflates absolute wall-times and the per-pair SD, but — as §10.4 shows —
  every measured delta is a large multiple of its own SD, so no effect here
  is sub-noise. The numbers are NOT clean-room; they are honest numbers from
  a loaded host, and the verdict rests on the delta/SD ratio, not on the
  absolute times.
- **Commit measured:** `main` @ `912740f` (clean working tree for the gate
  runs; the only later edits are this doc + the summary CSV + the
  force-added raw logs).

### 10.2 Three feature compositions (each vs `production` baseline = arm A)

The R10-2 harness pairs ONE baseline (`paired_ab_medium_off`, built
`--features production`) against ONE treatment (`paired_ab_medium_on`).
Three treatments were measured by rebuilding the ON arm with different
feature sets and re-invoking the gate with `--skip-build`:

| combo | arm A (baseline) | arm B (treatment) |
|---|---|---|
| 1+2 | `production` | `production,medium-classes` |
| 3 | `production` | `production,medium-classes,large-cache-extended` |

(Combo "1" and "2" in the task framing are the baseline arm A and the
medium-classes arm B of the same run — they share one A/B/B/A session.)

### 10.3 Results — per-arm means over 120 launches (20 pairs × 2 A-slots / 2 B-slots × 3 phases = 120 per arm)

Per-op conversions use the harness's fixed op counts: alloc/free phases do
16 × 20 = 320 ops each; the realloc phase does 16 objects × 3 grow-steps ×
20 rounds = 960 realloc-grows. `segments_reserved_total` and the commit/RSS
snapshots are emitted once per launch; `segments` is a deterministic
constant per arm (min == max across all 120 launches), which is itself a
signal that the allocator's reservation pattern is stable run-to-run.

| combo | arm | alloc (launch-mean) | free | realloc (launch-mean) | realloc per-op | segments | commit | rss |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1+2 | A `production` | 3.75 ms | 15.15 ms | 0.056 ms | ~58 ns | 329 | 33.7 MiB | 3.1 MiB |
| 1+2 | B `…,medium-classes` | 0.151 ms | 0.076 ms | 64.94 ms | ~67.6 µs | 172 | 49.3 MiB | 9.3 MiB |
| 3 | A `production` | 3.47 ms | 14.81 ms | 0.050 ms | ~52 ns | 329 | 33.7 MiB | 3.1 MiB |
| 3 | B `…,medium-classes,large-cache-extended` | 0.137 ms | 0.080 ms | 18.80 ms | ~19.6 µs | 20 | 81.4 MiB | 11.4 MiB |

**Full-round criterion time (sum of the three phase means) vs sub-window.**
Per CLAUDE.md's wall-clock-gate rule, both axes are reported for the same
harness: the **realloc phase is the sub-window** that decides the kill-gate
(the 20% threshold is on realloc specifically), and the **full round**
(alloc+free+realloc) is the net:

| combo | arm | full round (alloc+free+realloc) | net B vs A |
|---|---|---:|---|
| 1+2 | A `production` | 18.96 ms | — |
| 1+2 | B `…,medium-classes` | 65.17 ms | **B ~3.4× slower overall** (realloc dominates the alloc+free wins) |
| 3 | A `production` | 18.33 ms | — |
| 3 | B `…,medium-classes,large-cache-extended` | 19.02 ms | **B ~1.04× (≈ break-even overall)** — `large-cache-extended`'s realloc cut (~19 ms) is now comparable to the alloc+free savings (~18 ms) |

**Methodological caveat:** the full-round figures above are the ARITHMETIC
SUM of the three independently-paired phase means in §10.4 (alloc/free/
realloc), not a single paired sample of their own — there is no full-round
SD/t/sign-test in this measurement. A genuine full-round paired statistic
would require a 4th runner pass measuring one combined alloc+free+realloc
sequence per launch (rather than three separate phase timers) and running
its OWN paired A/B/B/A comparison. Not done here; flagged for a future
re-run if the full-round number is ever load-bearing for a decision — today
it is presented as context, not as the kill-gate criterion (the gate itself
is on the realloc sub-window alone, per this section's own framing above).

The combo-3 full-round near-break-even is notable but does NOT clear the
gate: the gate is on the realloc SUB-WINDOW, which is still ~380× (and the
net is only break-even for THIS specifically realloc-heavy 16-object
workload; a less realloc-heavy program would see only the alloc/free wins).

### 10.4 Paired statistics + SD/mean-delta resolvability (the R17-7 check)

The runner reports the paired t-test on the 20 block-paired deltas (A−B).
The SD/Δ column is the methodological check a third review
(`docs/reviews/2026-07-25-crush-review-r13-r17.md` §4.1) demanded after
R17-7: there, SD (7.7–8.2 ms) exceeded the mean delta (1.2–1.25 ms), so the
host could not resolve an effect that small. Here, for every REAL phase, SD
is 2.9–11.7% of |Δ| (Δ is 8–34× its own SD) → **the host DOES resolve every
effect**. The control row's "not resolvable" is the CORRECT outcome for a
same-vs-same run (no real effect exists; t ≈ 0).

| combo | phase | paired Δ (A−B) | SD | t | sign (A/B) | sig? | SD/Δ | resolvable? |
|---|---|---:|---:|---:|:---:|:---:|---:|:---:|
| 1+2 | alloc | +3.681 ms | 0.328 ms | 50.13 | 0 / 20 | REAL | 8.9% | YES |
| 1+2 | free | +14.891 ms | 0.433 ms | 153.77 | 0 / 20 | REAL | 2.9% | YES |
| 1+2 | realloc | **−66.055 ms** | 7.746 ms | −38.14 | 20 / 0 | REAL | 11.7% | YES |
| 3 | alloc | +3.262 ms | 0.120 ms | 122.05 | 0 / 20 | REAL | 3.7% | YES |
| 3 | free | +14.721 ms | 0.508 ms | 129.72 | 0 / 20 | REAL | 3.4% | YES |
| 3 | realloc | **−19.378 ms** | 2.066 ms | −41.94 | 20 / 0 | REAL | 10.7% | YES |
| control | realloc | +0.0006 ms | 0.0077 ms | 0.364 | 8 / 12 | NOT sig | 1229% | NO (expected; null result) |

### 10.5 Same-vs-same control (harness honesty)

`node scripts/paired-ab-runner.mjs --config docs/perf/paired_ab_runs/_r10_2_realloc.json --arms A,A --pairs 20`
(both arms = the `production` OFF exe, realloc phase): t=0.364 ≪ crit 2.101,
sign 8/12 (near-even). The harness has NO spurious self-difference on the
phase that decides the kill-gate.

### 10.6 Cache-hit-rate proxy (the metric the task asked for)

The R10-2 gate binaries do NOT emit `large_cache_hits` (the original gate
deliberately omitted `alloc-stats`, and R14-4 §2.2's explicit `large_cache_hits`
numbers came from a SEPARATE temporary-instrumented pad-target probe, not the
gate binaries). Per R14-4 §5's own diagnostic method, `segments_reserved_total`
IS the cache-miss proxy: each promotion that misses the large cache reserves
a fresh 4 MiB span; each hit reuses a cached span (no new reservation). The
realloc phase does exactly 16 promotions × 20 rounds = 320 `alloc_large`
calls under `medium-classes`; for arm B the alloc/free phases use the medium
path (no Large reservations), so `segments` ≈ fresh reservations among those
320:

- `production,medium-classes`: 172 fresh / 320 ⇒ **~46% hit rate** (≈ 8 of 16
  per round reuse a cached span — matches the 8-slot design limit exactly;
  R14-4 §5's "roughly half miss" prediction, now on non-leaky code).
- `production,medium-classes,large-cache-extended`: 20 fresh / 320 ⇒ **~94%
  hit rate** (40 slots catch almost every promotion). This is the
  `large-cache-extended` win, and it is what cut the realloc time ~3.5×.
- Both still leave a non-zero realloc cost (the promotion `memcpy`), so both
  still fail the 20% gate.

**Methodology note for a future re-run.** The proxy above is only necessary
because the gate binaries deliberately omit `alloc-stats` (its per-hit
increment is a hot-path cost the gate ships without). A future re-run should
build the ON arm with `--features "production,medium-classes,alloc-stats"`
(combo 3 adds `,large-cache-extended`) and read the REAL counter directly —
the public `AllocStats::large_cache_hits` field (`src/global/alloc_stats.rs`,
surfaced through the existing `#[doc(hidden)]` dbg accessor) reads `0`
without that feature and the true count with it. That replaces the
`segments_reserved_total` miss-inference with the exact counter the task
framing asked for, and removes the implicit "one `alloc_large` call == one
promoted object" coupling the proxy relies on (true for THIS workload's
shape, but a property a direct counter would not assume).

### 10.7 Why the gate is still RED (root cause, post-fix)

R14-4 §5 attributed the ~2,000× to "roughly half of every round's promotions
pay a genuine fresh OS `VirtualAlloc` instead of a cache hit" — i.e.
cache-slot pressure (16 promoted objects, 8 slots). That diagnosis was
CORRECT about the mechanism but was entangled with the R17-4 leak, which
made COMMIT unbounded (1.3 GiB). R17-4 fixed the COMMIT path (dealloc
routing) but did NOT change the TIME path: the per-promotion cost is still
(alloc_large — a fresh `VirtualAlloc` on a miss, or a cache lookup on a hit)
+ a 256 KiB `copy_nonoverlapping` of the preserved prefix. The leak only
added commit, not wall-clock. Hence post-fix: commit is bounded (49 MiB),
the time regression is essentially unchanged at ~1,180× (combo 1+2), and it
is now cleanly attributable to genuine promotion work, not a leak artifact.
The ratio dropped from R14-4's "~1,700–2,300×" to "~1,180×" NOT because
`medium_on` got faster — its absolute realloc cost is essentially unchanged
(~67.6 µs/op now vs ~72 µs/op in R14-4) — but because the BASELINE realloc
happened to measure slower under this session's heavier host load (~58 ns/op
now vs ~39 ns/op in R14-4; the baseline's near-zero cost is load-sensitive).
The load-invariant comparison is the absolute medium realloc per-op (~67 µs),
which R17-4 did not move. `large-cache-extended` removes the miss penalty (94% hits, 66→19 ms) but
cannot remove the structural memcpy — that requires R10-2 §5's mitigations
(in-place medium grow / growth headroom), which are out of scope for R17-4 /
R18-3 / R18-2.

### 10.8 Raw logs + machine-readable summary (committed via `git add -f`)

- `docs/perf/_raw_r18_2_combo12_off_vs_on.log` — combo 1+2 full gate run
  (truncated to the three `=== A vs B ===` summary blocks + build banner +
  one sample A/B/B/A block, per the R14-10 truncation precedent; full
  uncurated stdout reproducible via `node scripts/r10_2_medium_gate.mjs
  --pairs 20`).
- `docs/perf/_raw_r18_2_combo3_off_vs_onext.log` — combo 3 (with
  `large-cache-extended`), same truncation.
- `docs/perf/_raw_r18_2_control_off_vs_off.log` — same-vs-same control
  (untruncated; it is the harness-honesty evidence).
- `docs/perf/R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` — machine-readable
  companion (commit, features, per-phase means, paired Δ/SD/t/sign,
  SD/Δ ratio, resolvable flag, segments/commit/rss, hit-rate proxy).
- Provenance JSONs (one per phase, with every raw per-process sample + git
  commit + rustc + CPU + power plan) were written to
  `docs/perf/paired_ab_runs/2026-07-26T*.json` (gitignored by repo
  convention; reproducible by re-running the gate).
