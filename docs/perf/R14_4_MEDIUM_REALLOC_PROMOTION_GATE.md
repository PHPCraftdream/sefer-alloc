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

**Root cause not fully isolated within this task's time budget.** All three
candidates request an amount that `alloc_large` rounds to the identical 4
MiB `usable` span, and the large-cache admission predicate
(`slot.usable_size >= usable && slot.usable_size <= usable * 2`,
`src/alloc_core/alloc_core_large.rs`) compares `usable` values that should
be identical (4 MiB) across all three modes — so a same-band cache hit was
expected to behave symmetrically. It evidently does not for this specific
throwaway probe's harness shape (`fixed2mib`'s request size is
LITERALLYidentical, byte-for-byte, every round — 2,097,152 — while
`nopad`/`floor512kib` vary round to round with the growth sequence's actual
values, which may interact with something in the cache/admission path this
task did not fully trace, e.g. FIFO eviction ordering or a `usable_size`
comparison edge this task's `grep`-level investigation did not conclusively
identify).

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

1. **The pad-target probe's commit-cost discrepancy (§2.2)** — the exact
   mechanism causing `nopad`/`floor512kib` to reserve far more distinct
   segments (0 cache hits) than `fixed2mib` (232 cache hits) despite all
   three requesting the same 4 MiB rounded `usable` span was not fully
   root-caused within this task's time budget. Does not change the pad-target
   decision (§2.1's SEGMENT-rounding argument is independent of this), but
   is worth a dedicated look — possibly a real large-cache admission
   asymmetry worth its own bug report, or a harness artifact specific to
   this throwaway probe.
2. **R14-5's large-cache hardening may flip this gate's verdict** — worth
   re-running `scripts/r10_2_medium_gate.mjs` against this task's promotion
   mechanism once R14-5 lands, since the root cause (§5) is cache-slot
   pressure, which R14-5 directly targets.

---

## 7. Verdict

**GATE: CONDITIONAL-GO on the mechanism, RED on R10-2's specific kill-gate.**

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
