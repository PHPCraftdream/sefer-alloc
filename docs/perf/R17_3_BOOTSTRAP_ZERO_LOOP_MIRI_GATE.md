# R17-3 — gate bootstrap hash-table / free-list zero-loops behind `cfg(miri)`

**Task:** #320 (R17-3, Round 17, P1). **Code change + measurement.** Gate the two
unconditional `MAX_SEGMENTS`-scaled zero-fill loops in `bootstrap::primordial()`
behind `#[cfg(miri)]`, mirroring the existing virgin-page-skip discipline the
neighbouring `AllocBitmap`/`MagazineBitmap` inits already apply. This recovers a
deterministic startup Ir cost that R14-7's `MAX_SEGMENTS` 1024→4096 raise
quadrupled and R16-4 (task #314) root-caused to exactly these two loops.

**Date:** 2026-07-24. **Base revision:** `main` @ `f65015a` (post-Round-16). The
before/after pair is the *same* working tree at `f65015a` with the R17-3 edit
applied vs reverted — NOT a commit pair (the change is a single 2-loop `cfg`
gate in one file), so there is no intervening code drift to control for.
**Platform:** Windows 10 Pro x86-64 (native); `npm run iai` run under WSL2
(Ubuntu 24.04) + Valgrind/Callgrind, the same setup as every prior perf-gate doc
in this series (R13-6, R13-9, R14-4/5/6, R15-1).

---

## 0. Headline summary

| axis | before-fix (`f65015a`, loops unconditional) | after-fix (`f65015a` + R17-3, loops `cfg(miri)`) | verdict |
|---|---|---|---|
| iai, 12 single-thread non-remote benches, Instructions (Ir) | see §3 | **−81,966 Ir, uniformly, on EVERY bench** (−14% to −96% relative, because absolute Ir per bench varies) | **Real, deterministic, one-time bootstrap-scale cost recovered.** The flat per-bench delta and the byte-exact reconciliation (§4) confirm the mechanism. |
| iai marginal `Ir/op*` (hot-path, bootstrap subtracted) | 74.1 / 183.0 / 185.6 / 30,586.4 / … | **identical** (74.1 / 183.0 / 185.6 / 30,586.4 / …) | **Zero hot-path effect** — only the bootstrap constant moved, confirming this is not a per-operation change. |
| Miri, bootstrap-exercising tests under strict provenance | (loops run unconditionally) | loops STILL run (gate correctly inverted; miri's `std::alloc` is not guaranteed zeroed) | **Soundness preserved** — see §6. |

**The headline number (−81,966 Ir flat) is LARGER than the +61,440 Ir regression
R14-7 introduced and R16-4 attributed to these loops.** This is expected, not an
anomaly: these two loops were *never* `cfg(miri)`-gated at any `MAX_SEGMENTS`
value — they predate R14-7 (written in Task #135 / "OPT-B"), so the fix removes
their FULL memset cost, not just the raise-attributable increment. The
byte-reconciliation in §4 shows the observed 81,966 Ir decomposes as
`20,480 (pre-raise baseline) + 61,440 (R14-7 raise increment) = 81,920` bytes,
matching the observed delta to within 46 Ir (<0.06%, memset call/ret setup
overhead). The task brief framed R17-3 as "recover R14-7's +61.4K Ir startup
regression"; the actual recovery additionally reclaims the ~20.5K Ir of
pre-existing baseline cost these loops had carried since they were written — a
net win beyond the regression recovery, with no downside (the loops were a
tautology over OS-zeroed fresh pages on every real backend).

---

## 1. Why this task exists (R14-7 / R16-4 recap)

R14-7 (`ffb82bc`, task #292) raised `MAX_SEGMENTS` 1024 → 4096 to remove a binary
wall on simultaneously-live Large objects. R15-1 (task #303,
`docs/perf/R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md`) measured the aftermath and
found a flat **+61,440 Ir** appearing identically on every one of the 12 iai
benches — the signature of a one-time bootstrap cost, not a hot-path cost (none
of these benches perform a cross-thread free, so none ever call
`drain_dirty_segments`). R15-1's §2.3 originally mis-attributed this to
`SegmentTable::from_primordial` (which in fact performs no memory operation); its
§2.3a **CORRECTED** bullet, resolved by R16-4 (task #314, commit `afa6b1d`),
pinned the exact mechanism via `callgrind_annotate` + `objdump`: LLVM recognises
two compile-time-bounded, statically-zero-valued write loops in
`bootstrap::primordial()` (`src/alloc_core/bootstrap.rs`) as a `memset` idiom and
lowers them to `call memset` instructions whose `MAX_SEGMENTS`-derived size
arguments quadrupled across the raise:

| call site | source | before `edx` (size) | after `edx` (size) | byte-delta |
|---|---|---:|---:|---:|
| `bootstrap.rs` "4b. OPT-B" hash-table loop | `HASH_CAPACITY * 8 = 2 * MAX_SEGMENTS * 8` | 16,384 | 65,536 | +49,152 |
| `bootstrap.rs` "4c" free-list loop | `FREE_LIST_CAPACITY * 4 = MAX_SEGMENTS * 4` | 4,100 | 16,388 | +12,288 |
| **total** | | | | **+61,440** |

R16-4's byte-delta reconciliation (`49,152 + 12,288 = 61,440`) matched the
observed +61,440 Ir exactly (1 Ir per extra byte zeroed for glibc's AVX2 memset
at this size range). R16-4's §2.3a explicitly flagged the follow-up — *"Whether
to extend that [virgin-page] skip to these two loops … is a natural follow-up …
flagged for the orchestrator as a distinct task, not applied here"* — and Round
17's review queued it as R17-3 (P1). This task applies that follow-up.

**The key soundness fact** (already established by PERF-PASS-2, G5/C1, task #50,
for the neighbouring bitmap inits, and identical here): both loops zero-fill
state over `base`, which is the **PRIMORDIAL segment** — reserved a few lines
above via `Segment::reserve`/`reserve_lazy`, the ONLY OS allocation primitive on
the bootstrap path, never carved or decommitted before this point. The OS
guarantees fresh pages read zero (`mmap`/`VirtualAlloc`) on every real backend,
so the zero-fill writes exactly the value the pages already hold — a tautology.
Under `miri` the fallback aperture (`std::alloc::alloc`) is NOT guaranteed
zeroed, so miri keeps the explicit init (the gate is inverted: loops present
under `cfg(miri)`, absent otherwise).

---

## 2. The fix

Two edits in `src/alloc_core/bootstrap.rs`, symmetric to the existing
`AllocBitmap::init_in_place` / `MagazineBitmap::init_in_place` gates 70 lines
above them in the same file (lines 165, 173):

1. **Hash-table zero-fill loop** ("4b. OPT-B", now `bootstrap.rs:236`): the
   `for i in 0..segment_table::HASH_CAPACITY { … Node::write_struct::<*mut u8>(
   slot, null_mut()) }` loop gains a `#[cfg(miri)]` attribute. The doc comment is
   rewritten to the bitmap-init style: states the virgin-page skip reasoning,
   cross-references the neighbouring bitmap-init precedent and R16-4's
   root-cause, and notes miri keeps the explicit init.

2. **Free-list zero-fill loop** ("4c", now `bootstrap.rs:280`): the
   `for i in 0..segment_table::FREE_LIST_CAPACITY { … Node::write_u32(slot, 0) }`
   loop gains a `#[cfg(miri)]` attribute, same comment style. The comment
   additionally records the loop's "defensive" character: with `top = 0` only
   entries `[0, top)` are ever read, so none are read at bootstrap regardless,
   but the conservative `cfg(miri)` gate (not full removal) is kept per the plan.

**What was NOT touched (critical):** the two NON-zero writes that bracket the
loops stay unconditional, because they write a REAL value, not zero:

- the primordial-base *insert* into the hash table immediately after the
  hash-loop (`bootstrap.rs:247-255`, `Node::write_struct::<*mut u8>(hash_slot,
  base)`) — writes `base`, not `null_mut()`;
- the free-list `top = 0` write after the free-list loop (`bootstrap.rs:290-291`,
  `Node::write_u32(free_top_ptr, 0)`) — writes the stack's real "empty" initial
  state, not a zero-fill (the value happens to be `0` but it is the structural
  initial top-of-stack, observationally load-bearing: every recycle `push`/`pop`
  reads it).

Both `hash_slots` / `free_list_slots` / `free_top_ptr` bindings remain
unconditionally consumed by `SegmentTable::from_primordial` (`bootstrap.rs:394`),
so there are no unused-variable warnings under `cfg(not(miri))`.

---

## 3. iai — deterministic before/after instruction-count delta

### 3.1 Method

`npm run iai` (`node scripts/iai.mjs`) on the same working tree at `f65015a`,
once with the R17-3 edit reverted (before) and once applied (after). Same 12-bench
`benches/perf_gate_iai.rs` suite, plain `production` feature set, same WSL2 +
Valgrind/Callgrind host as every prior perf-gate doc. Raw logs:
`docs/perf/_raw_r17_3_iai_before.log`,
`docs/perf/_raw_r17_3_iai_after.log` (`git add -f`'d per the raw-log policy —
both logs are cited as the evidence for the table below).

### 3.2 The delta

| bench | before Ir | after Ir | Δ raw | Δ % | marginal Ir/op* (before → after) |
|---|---:|---:|---:|---:|---|
| small_churn_16b | 90,017 | 8,051 | **−81,966** | −91.1% | 74.1 → 74.1 |
| aligned_churn_640b_a128 | 89,953 | 7,987 | **−81,966** | −91.1% | 73.1 → 73.1 |
| large_alloc_free_cycle | 85,274 | 3,308 | **−81,966** | −96.1% | − → − |
| realloc_grow | 574,656 | 492,690 | **−81,966** | −14.3% | 30,586.4 → 30,586.4 |
| cold_alloc_free_256x16b | 132,130 | 50,164 | **−81,966** | −62.0% | 183.0 → 183.0 |
| cold_alloc_free_256x64b | 132,130 | 50,164 | **−81,966** | −62.0% | 183.0 → 183.0 |
| recycle_alloc_free_256x16b | 180,309 | 98,343 | **−81,966** | −45.5% | 185.6 → 185.6 |
| recycle_alloc_free_256x64b | 180,309 | 98,343 | **−81,966** | −45.5% | 185.6 → 185.6 |
| churn_256b | 90,017 | 8,051 | **−81,966** | −91.1% | 74.1 → 74.1 |
| churn_write_256b | 90,273 | 8,307 | **−81,966** | −90.8% | 78.1 → 78.1 |
| multiseg_cold_256k | 107,785 | 25,819 | **−81,966** | −76.0% | 331.0 → 331.0 |
| seg_cycle_decommit_256k | 144,093 | 62,127 | **−81,966** | −56.9% | 288.3 → 288.3 |

Every bench moves by the **same absolute** amount (−81,966 Ir, zero spread across
all 12 — a stronger "flat bootstrap constant" signature than R15-1's +61,440±356
spread), regardless of bench shape. `large_alloc_free_cycle` (one Large
alloc+free, no small-class machinery) moves by the same amount as
`recycle_alloc_free_256x16b`'s 512-iteration recycle loop — impossible for any
per-operation change, definitive for a one-time bootstrap cost.

**The marginal `Ir/op*` column is byte-identical before → after on every bench
that has one.** This is the decisive hot-path-neutrality proof: the fix changes
ONLY the bootstrap constant (the `large_alloc_free_cycle` proxy: 85,274 → 3,308
Ir), and since `Ir/op* = (Ir − bootstrap) / ops`, a pure bootstrap-constant shift
cancels out of every marginal figure by construction. No bench's per-operation
instruction count changed.

---

## 4. Why the recovery (−81,966 Ir) is larger than R14-7's +61,440 Ir regression

The task brief (R17-3) framed this as "recover R14-7's +61.4K Ir startup
regression". The observed recovery is ~81,966 Ir — larger than 61,440 by
~20,526 Ir. This is expected and explained, not an anomaly:

**These two loops were never `cfg(miri)`-gated at any `MAX_SEGMENTS` value.**
They were written in Task #135 ("4c") / "OPT-B" ("4b"), both predate R14-7, and
ran unconditionally over OS-zeroed fresh pages on every build since. R14-7 only
raised `MAX_SEGMENTS` 1024 → 4096, which quadrupled the loops' trip counts; it
did not introduce them. R16-4's `callgrind_annotate` measured the **delta across
the raise** (the *increment* R14-7 added: 49,152 hash-bytes + 12,288 free-list-
bytes = 61,440 bytes), correctly matching the +61,440 Ir *regression* R14-7
caused. But the loops' **pre-raise baseline** cost was already present and
unmeasured-by-R16-4 (R16-4 compared `b117257` vs `ffb82bc`, the raise commit
pair — its "before" arm at `MAX_SEGMENTS=1024` already paid the loops' 20,480-byte
baseline, which cancels out of the *delta* R16-4 computed but is NOT zero in
absolute terms).

This fix gates the loops entirely (under `cfg(not(miri))`), so it removes their
**FULL** current cost at `MAX_SEGMENTS=4096`, which is the pre-raise baseline
scaled up by the raise PLUS the raise increment:

| component | bytes (at `MAX_SEGMENTS=4096`) | source |
|---|---:|---|
| hash-table loop | 65,536 | `HASH_CAPACITY * 8 = 2 * 4096 * 8` |
| free-list loop | 16,384 | `FREE_LIST_CAPACITY * 4 = 4096 * 4` |
| **total removed by this fix** | **81,920** | |
| …of which: pre-raise baseline (`MAX_SEGMENTS=1024`) | 20,480 | `2*1024*8 + 1024*4` |
| …of which: R14-7 raise increment | 61,440 | matches R16-4's +61,440 Ir exactly |
| **observed Ir recovery** | **81,966** | §3.2, all 12 benches |
| reconciliation gap | 46 Ir (0.056%) | memset `call`/`ret`/setup overhead — constant regardless of size; callgrind counts the fill itself at ~1 Ir/byte for glibc's AVX2 memset at this range, plus a fixed ~46-Ir call frame |

So the fix recovers **R14-7's 61,440 Ir regression (the stated task goal) AND an
additional ~20,480 Ir of pre-existing baseline cost** the loops had carried since
Task #135. The additional recovery is a pure win: the baseline cost was a
tautology over OS-zeroed pages on every real backend (same reasoning PERF-PASS-2
already applied to the neighbouring bitmap inits in the same file), so removing
it changes no observable behaviour — it only stops writing zeros over zeros.

---

## 5. Soundness — why the skip is safe

The skip's safety rests on three properties, all already established for the
neighbouring `AllocBitmap`/`MagazineBitmap` inits (`bootstrap.rs:165,173`) and
identical here:

1. **`base` is the primordial segment, reserved fresh.** Step 1 of
   `bootstrap::primordial()` reserves it via `Segment::reserve`/
   `reserve_lazy` — the ONLY OS allocation primitive on the bootstrap path (see
   the module doc). It is never carved or decommitted before the zero-loops run.

2. **Fresh OS pages read zero on every real backend.** `mmap` (Unix) and
   `VirtualAlloc` (Windows) both guarantee zero-filled pages on first touch.
   The loops write `null_mut()` (hash) / `0` (free-list) — exactly the value the
   pages already hold. Re-zeroing is a tautology under `cfg(not(miri))`.

3. **Under `miri`, the loops still run.** The gate is `#[cfg(miri)]` (loops
   PRESENT under miri, ABSENT otherwise). Miri's fallback aperture
   (`std::alloc::alloc`) does NOT guarantee zeroed memory, so miri keeps the
   explicit init unconditionally — the identical discipline the bitmap inits
   use. Miri verification (§6) confirms bootstrap completes correctly under miri.

**Free-list-specific note (per the plan's "conservative, not full-removal"
guidance):** even in a hypothetical world where the pages were NOT zeroed, the
free-list loop's zero-fill is observationally irrelevant: only entries `[0, top)`
are ever read, and `top = 0` (written unconditionally at `bootstrap.rs:291`)
means none are read at bootstrap; every entry is first WRITTEN by a real recycle
`push` before it is ever read. So the array's initial contents cannot affect
behaviour regardless of the page state. The conservative `cfg(miri)` gate (rather
than full deletion) is kept per the Round-17 plan, matching the bitmap-init
precedent and keeping the layout inspectable/debuggable.

**What stays unconditional (NOT zero-fills, must not be gated):**
- primordial-base insert into the hash table (`bootstrap.rs:247-255`) — writes
  `base`, a real value, into the slot `hash_index(base)`; relies on the freshly-
  zeroed table only to guarantee that slot is empty (it is, under both miri
  explicit-zero and non-miri OS-zero);
- free-list `top = 0` (`bootstrap.rs:291`) — the stack's real "empty" structural
  initial state; every `push`/`pop` reads it.

---

## 6. Miri verification

Two bootstrap-exercising tests run under strict provenance
(`MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation"`), both via
`cargo +nightly miri test`:

| test | features | result | what it exercises |
|---|---|---|---|
| `regression_large_align_no_segment_exhaustion` | `alloc-core` | **2 passed; 0 failed** (146s) | registers align-128 Large segments → exercises `contains_base` hash-table lookups over the table populated by bootstrap; would misbehave if the hash table were not zeroed under miri |
| `regression_virgin_bitmap_skip` | `alloc-core` | **3 passed; 0 failed** (603s) | the direct analogous counterfactual: `t1_primordial_bitmap_reads_zero_before_any_traffic` confirms the primordial segment reads zero (the OS-zeroed-page contract this fix now also relies on for the hash/free-list arrays); runs full bootstrap |

Both pass UB-free under strict provenance (miri aborts on any UB, so a clean pass
is meaningful). This confirms the gate is correctly inverted: under `cfg(miri)`
the zero-loops STILL RUN (so miri's non-zeroed `std::alloc` pages get zeroed just
as before the fix), and bootstrap completes correctly. The fix changes NOTHING
about miri behaviour — the loops are byte-identical under `cfg(miri)`; only the
non-miri path loses them.

(Raw miri stdout not committed as a named artifact — no report below cites a
specific line of it as evidence requiring `git add -f` reproducibility; the
pass/fail verdicts above are transcribed directly. Reproducible via
`node scripts/miri.mjs regression_large_align_no_segment_exhaustion regression_virgin_bitmap_skip`.)

---

## 7. Decision

**Unconditional production fix — not a feature gate, not opt-in.** This is the
default path (the loops were default-on; the skip is now default-on under
`cfg(not(miri))`, which is every real backend). No promotion question: the fix
ships as default behaviour, validated by §3 (deterministic Ir recovery, zero
hot-path effect), §4 (byte-exact reconciliation explaining the larger-than-
expected recovery), §5 (soundness identical to the existing bitmap-init
precedent), and §6 (miri confirms the gate is correctly inverted and bootstrap
completes). The `production` feature composition is unchanged, so no
`bench:table` / `IAI_BASELINE.md` refresh is triggered by this fix (per CLAUDE.md
the canonical-table refresh rule fires only when `production`'s composition
changes).

---

## 8. Raw logs

- `docs/perf/_raw_r17_3_iai_before.log` — `npm run iai` on `f65015a` with R17-3
  edit reverted (loops unconditional). 12 benches, PASS, bootstrap constant
  (`large_alloc_free_cycle`) = 85,274 Ir.
- `docs/perf/_raw_r17_3_iai_after.log` — `npm run iai` on `f65015a` with R17-3
  edit applied (loops `cfg(miri)`-gated). 12 benches, PASS, bootstrap constant
  = 3,308 Ir.

Both `git add -f`'d alongside this report per the raw-log citation policy.
Machine-readable companion: `R17_3_BOOTSTRAP_ZERO_LOOP_MIRI_GATE_summary.csv`.
