# R24-2 — free path decomposed by magazine state: cheap push vs. overflow event

**Task #380 (R24-2), Round 24.** The mandatory measurement gate before any
free-path remediation. R24-1 (task #379, commit `14a86ce`,
`docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §9) corrected R23-3's
"74.70 Ir/free = own-thread body: M2 oracles + magazine push (fused), 80.8%"
headline: the bench arms free 64 DISTINCT pointers in one sequential pass,
hitting the magazine overflow arm (`cnt == TCACHE_CAP = 16`) six times — so
74.70 Ir/free is an AVERAGE over 58 non-overflow pushes AND 6 overflow events,
not an isolated push cost. This task decomposes that average into its two
magazine states (cheap non-overflow push vs. one overflow event), isolates the
overflow's one cleanly-separable sub-cost (the 8-block bitmap-clear pass), and
sweeps the batch size N to show how the overflow ratio amortizes.

**Date:** 2026-07-27. **Base revision measured:** `main` @
`14a86ce8540141c59f442265802c49f239cb3824` (working tree carrying only this
task's own additive edits at measurement time). **Platform measured:** WSL2
(Ubuntu, kernel `6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro
x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc
`1.98.0-nightly (bd08c9e71 2026-06-25)` — same toolchain/host as every other
`npm run iai` measurement in this doc tree (R22-15 through R24-1).

**Measurement only. No production behavior changed:** one new safe
`#[doc(hidden)]` hook (`dbg_overflow_bitmap_clear_pass`) that exposes an exact
production loop standalone, and seven new bench arms. No production call site
touched, no existing function body edited.

---

## 0. Headline: overflow is a batch phenomenon; ordinary interleaved hot free has NO overflow

| free-path state | isolated Ir (full free, SCOPE A) | isolated Ir (own-thread body, SCOPE B) | share |
|---|---:|---:|---:|
| **cheap non-overflow push** (M2 oracles + push, fused) | **43–44** | **~26** | 31% of the own-thread body |
| **one overflow event** (bitmap-clear + flush_class + compaction + push) | **571** | **~554** | **69% of the own-thread body** |
| overflow / cheap-push ratio | **12.9×** | **21.4×** | — |

The single most important finding: **R23-3's 74.70 Ir/free (the "80.8% of the
free path" headline) is ~69% overflow event + ~31% cheap push.** The overflow
arm — which fires only when 16+ distinct blocks are freed in one pass without
an intervening alloc — is the dominant cost of the BATCH-FREE workload
(`dealloc_free_only_16b`), NOT of an ordinary hot free.

Ordinary hot free (the interleaved alloc-then-immediately-free shape of
`small_churn_16b`, where each block is freed before the next is allocated and
the magazine never accumulates past `count == 1`) **never fires the overflow
arm at all.** Its free half is a pure cheap push: ~46.6 Ir/free (R23-2's
69.0 Ir/pair − R23-3's 22.38 Ir/alloc-hit), matching the isolated cheap push
(43–44 Ir) within refill amortization. The 92.50 Ir/free R23-3 reported is a
batch artifact (64-block batch-free-with-overflow), not "a real free".

See §4 for the full N-sweep (how the overflow ratio amortizes as batch size
grows), §5 for what could/could not be cleanly isolated, and §6 for the
prioritization implication for R24-3 (flush_magazine_class) vs. any future
cheap-push optimization.

---

## 1. Investigation performed first (per the task's instruction)

### 1.1 The magazine state machine and where overflow fires

Read `HeapCore::dealloc_own_thread_with_base` (`src/registry/heap_core_free.rs`)
in full — the cheap arm (lines 712-742) and the overflow arm (lines 744-816).
`cnt` is read at line 316 BEFORE the push; `TCACHE_CAP = 16`, `FLUSH_N = 8`
(`src/registry/tcache.rs:48,124`). Confirming R24-1 §9.3's arithmetic: freeing
N distinct blocks sequentially from `count == 0` fires overflow at frees #17,
#25, #33, #41, #49, #57 (6 events for N=64), leaving 58 cheap pushes.

### 1.2 Why the sweep uses an alloc-64 prefix (count → 0), NOT alloc-N-free-N

`refill_n_for_class` for the 16 B class is `TCACHE_CAP` (16)
(`src/registry/tcache.rs:107`), so `alloc k*16` leaves the magazine at exactly
`count == 0`. `alloc N` for N not a multiple of 16 leaves
`count == 16 - (N mod 16)` — so `alloc 8 free 8` would start the frees at
count 8 and overflow on the 8th free, and `alloc 17 free 17` gives TWO
overflows, not one. To make every sweep point's free loop start at count 0
(overflow fires predictably: 0 for N≤16, exactly 1 at N=17, 2 at N=32, 6 at
N=64), **every sweep arm allocates a FIXED 64 (= 4×16, count → 0) then frees
only the first N.** The shared alloc-64 prefix is byte-identical to the
existing `dealloc_prealloc_only_16b`, so `free_cost(N) = Ir(arm_N) − Ir(prefix)`
cancels it exactly. This is disclosed here rather than silently chosen, per
this project's "measured, not spun" convention (same norm as R23-3 §2's
self-caught methodology bugs).

### 1.3 What is and is NOT cleanly isolable within one overflow event

Read the overflow arm body (lines 744-816). It is three sequential pieces in
one straight-line block:

1. **The 8-block bitmap-clear pass** (lines 762-768): a `for` loop re-deriving
   `segment_base_of_ptr` + `SegmentMeta::new` + `clear_magazine` per flushed
   pointer. **Cleanly isolable** via a new safe hook that runs this exact loop
   standalone (`dbg_overflow_bitmap_clear_pass`, §3) — the loop is
   self-contained, needs no `HeapCore` mutable state, and `clear_magazine`'s
   cost is identical whether the bit was set or clear (read-word/clear-bit/
   write-back), so the hook measures the real per-iteration cost regardless of
   input bitmap state. This is R24-3's (flush_magazine_class) exact target.
2. **`flush_class` on 8 blocks** (lines 774-778): an `unsafe` call returning
   the 8 blocks to the substrate (`mark_free` + `dec_live` + decommit-check per
   block). **NOT cleanly isolable** without a hook that calls `flush_class`
   standalone — which production never does outside this overflow arm (it has
   its own R6-MS-3 unsafe contract), the exact Heisenberg risk the task brief
   warned against.
3. **The 8-pointer compaction shift** (lines 779-783) + **final push**
   (lines 800-802): inline sequential operations fused with `flush_class` in
   one straight-line block, no workload-level separation point.

So the overflow decomposes into: (1) the bitmap-clear pass [isolable, 84 Ir],
and (2) `flush_class` + compaction + final push [a single non-isolable
remainder, ~470 Ir, derived as overflow-body-minus-bitmap-clear]. See §5.1.

### 1.4 Two scopes — stated explicitly to avoid the confusion R23-3 hit

R23-3's "own-thread body" (74.70 Ir/free) subtracted the routing prefix
(`segment_base_of_ptr` + `contains_base`) — it was SCOPE B. The sweep arms here
go through the FULL `(*heap).dealloc` path (routing + contains_base +
own-thread body) — SCOPE A. This report gives BOTH for every quantity and
states which scope each number is, because the routing prefix (17.2 Ir/call,
inherited from R23-1/R23-3) is a constant offset between them:

- **SCOPE A (full free):** everything `(*heap).dealloc(ptr, layout)` does —
  `dealloc_routing` → `contains_base` (Tier-1) → `segment_base_of_ptr` →
  oracles → push/overflow.
- **SCOPE B (own-thread body):** SCOPE A minus the routing prefix (17.2 Ir:
  `contains_base` 8.17 + `segment_base_of_ptr` 9.03, R23-1/R23-3). This is
  R23-3's "own-thread body" scope — the oracles + push/overflow, fused.

---

## 2. Two isolation techniques used (and why each applies where)

This task needed BOTH techniques this project has established (R22-17/R23-1
shared-prefix subtraction; R23-2 N/2N) — the exact choice R23-3 made twice and
self-corrected (its §2.2). Per R23-3's own documented failure mode, N/2N is
valid ONLY when doubling the loop count doubles JUST the isolated quantity;
shared-prefix subtraction is the universal fallback.

- **Shared-prefix subtraction** for the cheap-push isolation (n8 → n9: one
  added cheap push at count 8) and the overflow isolation (n16 → n17: one
  added overflow free). Both pairs are byte-identical except for one added
  operation, so the Ir difference is exactly that operation's cost — the same
  pattern R22-17/R23-1/R23-3 established.
- **Shared-prefix subtraction against the existing prefix** for the N-sweep
  (`dealloc_prealloc_only_16b` = alloc-64-only = the shared prefix; each
  `free_cost(N) = Ir(arm_N) − Ir(prefix)`). NOT N/2N, because the quantity
  being swept (the overflow ratio) is non-linear in N (0 overflows for N≤16,
  then a step at N=17) — N/2N's linearity assumption does not hold here.
- **Direct-call hook** for the bitmap-clear sub-cost (the one place a new hook
  was genuinely necessary — the bitmap-clear loop is fused inside the overflow
  arm with no workload-level separation from `flush_class`).

No N/2N pair was used anywhere in this task — the non-linear overflow step
makes it invalid for every measurement here.

---

## 3. New measurement hook and bench arms

### 3.1 New hook (ONE — the minimum necessary)

**`HeapCore::dbg_overflow_bitmap_clear_pass`** (`src/registry/heap_core_diag.rs`)
— `#[doc(hidden)]`, `#[cfg(all(feature = "alloc-global", feature = "fastbin"))]`,
`#[inline(always)]`, **safe** (`&self`, no `unsafe` boundary — unlike
`dbg_dealloc_own_thread_with_base`/`dbg_push_to_ring`). Runs the EXACT
production 8-block bitmap-clear loop (`heap_core_free.rs:762-768`) verbatim:
`segment_base_of_ptr` + `SegmentMeta::new` + `clear_magazine` per pointer.
`#[inline(always)]` added per R23-3 §2.1's lesson (match production's
inline-ness; the production loop is inline within the `#[inline(always)]`
`dealloc_own_thread_with_base`). Measured: the arm's Ir was byte-identical
(7451) with and without the attribute — the optimizer had already elided the
single call boundary for this tight 8-iteration loop, so the attribute is
defensive, not load-bearing here (unlike R23-3's large-function case).

This is the ONLY new hook. No other measurement needed one — the cheap-push
and overflow isolations use pure shared-prefix subtraction (n8/n9, n16/n17),
and the N-sweep uses the existing `dealloc_prealloc_only_16b` prefix. This
matches R23-1/R23-2/R23-3's precedent of mostly not needing new hooks. **For
R24-6 tracking: this hook lives in the production unsafe-audit surface (its
`alloc-global + fastbin` gate is in `production`); R24-6 will move it to a
non-production feature.**

### 3.2 New bench arms (SEVEN)

| arm | isolates | technique | feature gate |
|---|---|---|---|
| `dealloc_free_only_16b_n1` | sweep N=1 (1 cheap push at count 0) | shared-prefix vs `dealloc_prealloc_only_16b` | `alloc-xthread` |
| `dealloc_free_only_16b_n8` | sweep N=8 (8 cheap pushes) + prefix for cheap-push pair & bitmap-clear arm | shared-prefix | `alloc-xthread` |
| `dealloc_free_only_16b_n9` | measurement-1 partner: n8 + 1 cheap push at count 8 | shared-prefix vs n8 | `alloc-xthread` |
| `dealloc_free_only_16b_n16` | sweep N=16 (16 cheap pushes) + prefix for overflow pair | shared-prefix | `alloc-xthread` |
| `dealloc_free_only_16b_n17` | measurement-2: n16 + 1 overflow event | shared-prefix vs n16 | `alloc-xthread` |
| `dealloc_free_only_16b_n32` | sweep N=32 (30 cheap + 2 overflow) | shared-prefix | `alloc-xthread` |
| `dealloc_overflow_bitmap_clear_only_16b` | measurement-3: bitmap-clear pass alone (n8 prefix + hook call) | shared-prefix vs n8 | `alloc-xthread + fastbin` |

All arms follow this file's existing conventions: `#[cfg(target_os = "linux")]`
(+ `alloc-xthread`/`fastbin` where the delegated mechanism requires it),
`black_box` on observables, doc comments explaining what each isolates and
how, registered in the `perf_gate` `library_benchmark_group!` list (43 benches
total, up from 36 before this task). The existing `dealloc_free_only_16b`
serves as the N=64 sweep point unchanged.

---

## 4. Results — real, deterministic `npm run iai` numbers (two independent runs, byte-identical `Ir`)

Raw evidence (both runs full stdout; truncation not needed — the suite is
fast):

- `docs/perf/_raw_r24_2_run1.log`
- `docs/perf/_raw_r24_2_run2.log`

Both runs: **43 benches, byte-identical `Ir` for every row including all 7 new
arms** (confirmed via a column diff of the extracted first-Ir-number across
both runs — zero differences). The 6 pre-existing reference arms reproduced
R23-3's published numbers EXACTLY (`small_churn_16b`=8051,
`dealloc_prealloc_only_16b`=7003, `dealloc_free_only_16b`=12923,
`dealloc_own_thread_body_only_16b`=12362,
`dealloc_contains_base_probe_only_16b`=8104,
`dealloc_segment_base_of_ptr_probe_only_16b`=7581) — confirming this run is on
the same toolchain/host and sound.

### 4.1 Raw Ir table (new arms + the rows they derive against)

| bench | raw Ir | role |
|---|---:|---|
| `dealloc_prealloc_only_16b` (shared prefix) | 7,003 | prefix (alloc-64, free 0) |
| `dealloc_free_only_16b_n1` (new) | 7,058 | sweep N=1 |
| `dealloc_free_only_16b_n8` (new) | 7,367 | sweep N=8 + cheap-push prefix |
| `dealloc_free_only_16b_n9` (new) | 7,410 | cheap-push partner |
| `dealloc_free_only_16b_n16` (new) | 7,711 | sweep N=16 + overflow prefix |
| `dealloc_free_only_16b_n17` (new) | 8,282 | overflow partner |
| `dealloc_free_only_16b_n32` (new) | 9,451 | sweep N=32 |
| `dealloc_free_only_16b` (N=64, existing) | 12,923 | sweep N=64 |
| `dealloc_overflow_bitmap_clear_only_16b` (new) | 7,451 | bitmap-clear arm |
| `dealloc_own_thread_body_only_16b` (R23-3) | 12,362 | reconciliation ref |
| `small_churn_16b` (existing, context) | 8,051 | interleaved ref |

### 4.2 Measurement 1 — cheap non-overflow push

| derivation | Ir | scope |
|---|---:|---|
| dedicated pair: `Ir(n9) − Ir(n8)` = 7410 − 7367 | **43** | A (full free, one push at count 8) |
| amortized: `free_cost(16)/16` = 708/16 | **44.25** | A (full free, steady) |
| minus routing prefix (17.2, R23-1/R23-3) | **~26** | B (oracle + push fused — R23-3's "cheap arm" scope) |

The dedicated pair (43) and the 16-push amortization (44.25) agree to within
1.25 Ir — the pair is the cleaner single-free isolation (shared-prefix, one
operation difference), the amortization confirms it is steady-state not a
first-iteration outlier. **Cheap non-overflow push ≈ 43–44 Ir/full-free (SCOPE
A), ≈ 26 Ir as the oracle+push body (SCOPE B).** (The N=1 point gives 55 Ir —
inflated by ~11 Ir of one-time loop/iterator setup; reported as an outlier,
not the steady cost.)

### 4.3 Measurement 2 — one overflow event

| derivation | Ir | scope |
|---|---:|---|
| dedicated pair: `Ir(n17) − Ir(n16)` = 8282 − 7711 | **571** | A (full overflow free) |
| minus routing prefix (17.2) | **~554** | B (overflow body) |
| overflow / cheap-push ratio: 571 / 44.25 | **12.9×** | A |

**One overflow event costs 571 Ir — 12.9× a cheap push.** This is the 17th
free alone (oracles + bitmap-clear-pass + flush_class + compaction + final
push), isolated by subtracting the 16-free arm (16 cheap pushes, no overflow).

### 4.4 Measurement 3 — overflow sub-costs (the one cleanly isolable piece)

| derivation | Ir | scope | isolable? |
|---|---:|---|---|
| bitmap-clear pass: `Ir(bitmap_clear) − Ir(n8)` = 7451 − 7367 | **84** | raw loop (hook) | **YES** (via `dbg_overflow_bitmap_clear_pass`) |
| flush_class(8) + compaction + final push (derived) | **~470** | B remainder | NO (fused, no separation point) |
| overflow body total | ~554 | B | — |

**The bitmap-clear pass (84 Ir) is the overflow's one cleanly-isolable
sub-cost** — 15.2% of the overflow body. It is isolated via the new hook
(n8 prefix + hook call vs n8 prefix alone). The remaining ~470 Ir
(flush_class on 8 blocks + 8-pointer compaction + final push) is a single
non-isolable remainder: these three operations run in one straight-line block
with no workload-level separation point, and isolating flush_class individually
would need a hook that calls it standalone — which production never does
outside this overflow arm (it has its own R6-MS-3 unsafe contract), the exact
Heisenberg risk the task brief warned against. See §5.1.

### 4.5 Measurement 4 — N-sweep (how the overflow ratio amortizes)

`free_cost(N) = Ir(arm_N) − Ir(prefix)`, starting every free loop at count 0:

| N | free_cost(N) | Ir/free | overflows | cheap pushes | reconstruction (cheap×44.25 + ovf×571) |
|---:|---:|---:|---:|---:|---|
| 1 | 55 | 55.00 | 0 | 1 | (+11 loop-setup; outlier, not steady) |
| 8 | 364 | 45.50 | 0 | 8 | 8×45.5 ✓ |
| 16 | 708 | 44.25 | 0 | 16 | 16×44.25 ✓ (pure-cheap floor) |
| 17 | 1,279 | 75.24 | 1 | 16 | 708 + 571 = 1279 ✓ |
| 32 | 2,448 | 76.50 | 2 | 30 | 30×44.25 + 2×571 = 2470 (0.9%) |
| 64 | 5,920 | 92.50 | 6 | 58 | 58×44.25 + 6×571 = 5992 (1.2%) |

The reconstruction (last column) matches the measured `free_cost(N)` within
~1% at every point — the cheap-push (44.25 Ir) + overflow (571 Ir) model
explains the sweep. The per-free cost is FLAT at ~44 Ir for N≤16 (pure cheap
push, zero overflow), then steps up at N=17 (first overflow) and climbs as the
overflow ratio grows: 44 → 75 → 77 → 93 Ir/free at N = 16 → 17 → 32 → 64.

**Overflow share of the N=64 batch:** 6 × 571 = 3426 Ir out of 5920 = **57.9%
overflow, 42.1% cheap push.** Six overflow events (9.4% of the 64 frees)
account for ~58% of the batch-free cost.

### 4.6 Measurement 5 — the interleaved comparison (what ordinary hot free actually costs)

`small_churn_16b` (existing) measures the FULL interleaved alloc+free pair.
R23-2's N/2N derived 69.0 Ir/pair; R23-3 isolated the alloc-side magazine-hit
pop at 22.38 Ir/alloc. So the interleaved **free half ≈ 69.0 − 22.38 =
46.6 Ir/free**.

This matches the isolated cheap push (43–44 Ir, §4.2) within ~2–3 Ir of refill
amortization (the alloc side occasionally refills the magazine). **Critically:
the interleaved shape never fires the overflow arm** — each block is freed
before the next is allocated, so the magazine never accumulates past count 1.
So **ordinary interleaved hot free ≈ 46.6 Ir, ALL cheap push, ZERO overflow.**

This is the framing correction R24-1 flagged as needed: the 92.50 Ir/free
"real free loop" (`dealloc_free_only_16b`) is a 64-block batch-free-with-
overflow workload, NOT the free half of the interleaved hot pair. The free
half of the hot pair is ~46.6 Ir — half the batch figure — and contains no
overflow cost at all.

### 4.7 Reconciliation with R23-3's 74.70 Ir/free

R23-3's 74.70 was the own-thread body (SCOPE B) averaged over the 64-block
batch (58 cheap + 6 overflow). This task's isolated SCOPE-B numbers:
- cheap push body ≈ 25.8 Ir (= 43 pair − 17.2 routing)
- overflow body ≈ 553.8 Ir (= 571 − 17.2 routing)

Reconstruction: (58 × 25.8 + 6 × 553.8) / 64 = **75.30 Ir/free**, vs R23-3's
measured 74.70 — **0.8% reconciliation** (within subtraction rounding). The
decomposition checks out: **within the own-thread body, overflow is 68.9% and
cheap push is 31.1%.** R23-3's "80.8% of the free path" was really "~69%
overflow + ~31% cheap push" inside the own-thread body, averaged over a batch.

---

## 5. What could NOT be cleanly isolated, and why

### 5.1 flush_class vs. compaction vs. final push (within one overflow event)

**Genuinely fused, not merely unmeasured** — the same category as R23-3's
M2-oracles-vs-push finding (§6.1 of that report). The three pieces run in one
straight-line block (lines 774-802) after the bitmap-clear pass: `flush_class`
(an `unsafe` call with the R6-MS-3 contract), the 8-pointer compaction shift,
and the final push (`mark_magazine` + slot write + count bump). Splitting
flush_class from compaction would need a hook that calls `flush_class`
standalone — but production never runs `flush_class` outside this exact
overflow context (it returns blocks to the substrate via `mark_free` +
`dec_live`; running it without the surrounding overflow's magazine-state
setup would be a different, invented mechanism). The bitmap-clear pass (84 Ir)
IS cleanly isolable because it is a self-contained loop needing no mutable
`HeapCore` state; the rest (~470 Ir) is reported as one non-isolable
remainder, derived as overflow-body-minus-bitmap-clear. This is the smallest
honestly-isolable decomposition — a partial, honestly-caveated split, not an
overreaching one.

### 5.2 The cheap push at exactly count 0 vs. count 8

The dedicated pair (n9 − n8) isolates a cheap push at count 8; the N=1 arm
isolates one at count 0 (but inflated by loop-setup). The steady amortized
cost is flat at ~44 Ir whether averaged over 8 or 16 pushes (45.5 vs 44.25) —
so the cheap-push cost is effectively count-independent in the 0–15 range, and
the count-8 figure (43 Ir) generalizes. No count-dependent split is reported
because none is measurable above the ~1 Ir noise floor.

### 5.3 The interleaved free half — directly measured only as a difference of two prior reports

46.6 Ir/free is R23-2's pair (69.0) minus R23-3's alloc-hit (22.38) — both
from PRIOR reports, not re-measured in this task. It matches the isolated
cheap push (43–44 Ir) within refill amortization, which is the confirmation
that matters, but a dedicated interleaved-free-only arm (via shared-prefix
subtraction against an interleaved alloc-only arm) was not built — scoped out,
since the cheap-push isolation (§4.2) already gives the same number more
directly.

---

## 6. Implication for prioritizing R24-3 vs. cheap-push optimization

**R24-3 (flush_magazine_class — the bitmap-clear-pass merge):** eliminates the
84-Ir bitmap-clear pass per overflow event by folding `clear_magazine` into
`flush_class`'s per-block loop. Bounded payoff:

- **Batch-free workloads (dealloc_free_only_16b shape):** 6 overflows × 84 Ir
  = 504 Ir saved out of 5920 = **8.5% of batch-free cost.**
- **Ordinary interleaved hot free (small_churn_16b shape):** **0%** — overflow
  never fires, so the bitmap-clear pass never runs. R24-3 does not speed up
  the ordinary hot free at all.

So R24-3 is a batch-free optimization (~8.5%), NOT an ordinary-hot-free
optimization. Whether that is worth pursuing depends on whether batch-free
(freeing many distinct blocks in one pass) is a real workload shape for this
allocator's users — a question this measurement task does NOT answer (it
measures costs, not workload frequencies).

**The larger overflow cost is flush_class + compaction (~470 Ir/overflow =
82.4% of the overflow body, 84.8% of the non-bitmap-clear remainder), which
R24-3's bitmap-clear merge does NOT directly target.** If batch-free speedup
is the goal, flush_class itself (8 blocks × ~53 Ir/block, dominated by
`mark_free` + `dec_live` + decommit-check per block) is the larger lever
inside the overflow arm — but it is NOT cleanly isolable from compaction
without a flush_class-standalone hook (§5.1), so a follow-up measurement
would be needed before optimizing it.

**Cheap-push optimization:** the cheap push is already small (43–44 Ir
full-free, ~26 Ir as the oracle+push body). The routing prefix (17.2 Ir =
37% of the full cheap free) is the larger single chunk — already isolated by
R22-17/R23-1 and gated by real ownership-safety constraints (R22-17 §4.2).
The oracle+push body itself (~26 Ir) is the M2 double-free oracles fused with
the slot write, per R23-3 §1.2 not further separable. There is no obvious
large win on the cheap push.

**Recommendation:** R24-3 is a legitimate ~8.5% batch-free optimization (the
bitmap-clear merge is clean and its target is now precisely measured at 84 Ir);
it is NOT an ordinary-hot-free optimization, and should not be prioritized on
that basis. The overflow's larger cost (flush_class) would need its own
isolation measurement before any optimization attempt. This report recommends
NO remediation — only the measurement, per this project's "measure first,
remediate as a separate task" convention.

---

## 7. Verification performed

- **Read the mechanism FIRST** (§1): the cheap arm, the overflow arm, the
  `refill_n_for_class` refill geometry, why alloc-N-free-N does not start the
  frees at count 0.
- **Chose the isolation technique per-quantity** (§2): shared-prefix for
  cheap-push and overflow (valid — one operation difference); NOT N/2N for the
  sweep (the overflow step is non-linear in N); direct-call hook for the
  bitmap-clear sub-cost (the one place it was genuinely necessary).
- **Two independent `npm run iai` runs** (43 benches each, `--features
  production`, the CI default) — byte-identical `Ir` for every bench including
  all 7 new arms, confirmed via a column diff.
- **Reference arms reproduced R23-3 exactly** (§4) — 6 pre-existing arms
  byte-identical to R23-3's published numbers, confirming same toolchain/host.
- **Reconciliation cross-check** (§4.7): the isolated cheap-push (25.8 Ir) +
  overflow (553.8 Ir) SCOPE-B numbers reconstruct R23-3's 74.70 Ir/free to
  within 0.8% — the decomposition is consistent with the prior measurement.
- **N-sweep reconstruction** (§4.5): the cheap-push + overflow model matches
  measured `free_cost(N)` within ~1% at N = 17, 32, 64.
- **`cargo check --bench perf_gate_iai --features production`** (WSL2, the
  platform this bench compiles its real body under) — clean, after one fix
  (the bitmap-clear arm's raw-pointer deref needed an `unsafe` block, matching
  every other `(*heap).method()` call in this file).
- **`production`'s feature composition confirmed unchanged**:
  `grep -n "^production = " Cargo.toml` returns
  `["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
  "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]`,
  byte-identical to pre-task. `git status --short` confirms `Cargo.toml` is
  not in this task's diff.
- **No production behavior changed**: the one new `src/` item is a
  `#[doc(hidden)]` safe thin wrapper running an EXISTING production loop
  verbatim — no production call site touched, no existing function body edited.
- **clippy was NOT run under WSL** (deferred to the reviewing session's own
  `npm run check` pass, per this project's pre-push convention; same caveat as
  R23-3 §8).

---

## Files touched

- `src/registry/heap_core_diag.rs` — added `HeapCore::dbg_overflow_bitmap_clear_pass`
  (measurement-only, safe, `#[doc(hidden)]`, `#[cfg(all(feature = "alloc-global",
  feature = "fastbin"))]`, `#[inline(always)]`). **One new hook — the only one
  this task added (for R24-6 tracking).**
- `benches/perf_gate_iai.rs` — added `dealloc_free_only_16b_n1`, `_n8`, `_n9`,
  `_n16`, `_n17`, `_n32`, `dealloc_overflow_bitmap_clear_only_16b`; registered
  all seven in the `perf_gate` `library_benchmark_group!` list (43 benches
  total, up from 36). Zero changes to any pre-existing bench fn's body.
- `docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md` — this report.
- `docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE_summary.csv` — companion
  machine-readable summary.
- `docs/perf/_raw_r24_2_run1.log` / `_raw_r24_2_run2.log` — full raw
  `npm run iai` stdout for the two independent, byte-identical-`Ir` runs cited
  in §4. `git add -f` needed (`.gitignore` excludes `docs/perf/_raw_*.log` by
  default, R13-10/task #280).
- `docs/perf/OPEN_ITEMS.md` — item 1 gets a "DONE (task #380, R24-2)" note.
- `Cargo.toml` — **untouched** (confirmed in §7).

**Files needing `git add -f`** (gitignored by `.gitignore`, `/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r24_2_run1.log`
- `docs/perf/_raw_r24_2_run2.log`

---

## Post-publication note (R27-10, task #428)

The `dealloc_overflow_bitmap_clear_only_16b` bench arm (§3.3 / §4.3's 84-Ir
isolation) and the `HeapCore::dbg_overflow_bitmap_clear_pass` measurement
hook it called (`src/registry/heap_core_diag.rs`) were **removed** in R27-10
(task #428). The bitmap-clear optimization region this hook was built to
isolate accumulated four consecutive NO-GOs (R24-3, R24-4, R25-3, R26-7), so
the hook and its single consumer were deleted rather than retained as dead
infrastructure whose only caller left a temporary magazine-state invariant
broken on return (see `docs/reviews/2026-07-28-r26-readonly-review.md` P2).
The 84-Ir / 7,451 figures above remain valid historical measurements at this
report's measurement commit; to reproduce, check out that commit (the hook +
arm are preserved in git history). See `docs/perf/OPEN_ITEMS.md` item 1.
