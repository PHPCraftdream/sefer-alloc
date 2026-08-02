# Speedup-opportunity survey — 2026-07-31

**Status: investigation in progress.** This file is written incrementally: each
section below is appended as soon as a concrete, source-grounded hypothesis is
formed. A prioritized summary / punch list is added at the top at the end.

**Scope / posture.** Read-only research. No `src/`, `benches/`, `examples/`,
`tests/`, or `Cargo.toml` changes; no commits. Every candidate below is scoped
as "what a FUTURE gate would have to prove", per `CLAUDE.md`'s evidence rules
(same-workload-regime cost/benefit, path-activation oracle, entry-point-layer
honesty, derived-not-hand-typed tables, immutable source identity).

**No new numbers are invented here.** Where a real measured number exists in
`docs/perf/`, it is cited by file. Where none exists, the section says so and
describes the cheapest first probe instead.

---

## Summary / prioritized punch list

**14 entries: 13 numbered findings (F1–F13) plus one sub-finding (F1b).**
Ranked below by *(potential win) × (how close it already is to
provable/shippable) × (how cheap the first proof step is)* — deliberately
NOT by raw expected win, because this project's binding constraint is
measurement capacity, not idea supply (see F3).

**Read this first — the one structural point.** Nine of the fourteen entries
are ideas; the tenth, **F3**, is the observation that this project's
deterministic judge (`Ir`) is structurally blind to most of what is left, and
that **six items now wait on one missing macro-benchmark** (OPEN_ITEMS `[L]`
item 34's four — X5/20, T10/22, R1/23, R15-1/9 — plus F1 and F2), with **F10
waiting on a seventh, related-but-distinct artifact** (many *threads* into one
ring, not many *segments*). If a round plans more than one task, building
measurement capacity is the highest-value thing on this page; F3's rank of 7
below reflects only its cost-of-first-proof, not its strategic priority, which
is #1.

**The ranking.**

| # | Finding | Potential win | Proof-readiness | Cheapest first step | Class |
|---|---|---|---|---|---|
| 1 | **F6** — `realloc` move leg re-derives `base` + re-runs `contains_base` it already proved | ~17–21 Ir/move-leg realloc (both components separately measured: 9.03 + 8.2/12.0) | **High** — judge already committed | `npm run iai` on `realloc_grow` after a ~4-line edit | shippable, cheap |
| 2 | **F7** — `alloc_zeroed`'s magazine hit stamps; `alloc`'s deliberately does not | ~12–18 Ir/hit, on a hit measured at 22.4 Ir total; **also a confound in the open R31-0 A/B** | **High** — `Ir`-visible; one line | enumerate magazine producers, then `npm run iai` | shippable, cheap + fixes an open item's evidence |
| 3 | **F4** — `PerClass` lacks `#[repr(C)]`, so the documented one-cache-line magazine is not in effect | 0-of-8 → 7-of-8 classes get `count`+`slots[0..6]` on one line | **High** — offsets verified empirically; fix is byte-identical in size | add `#[repr(C)]` + field order + an `offset_of!` const-assert | cheap to try, **benefit unproven** (expect 0 `Ir`) |
| 4 | **F1b** — one 2-bit-per-granule state word replacing two 32 KiB-apart bitmaps | one `locate` + one load answering both free-path oracles; RAD-5's own precedent was −52 Ir/op for the same *kind* of move | **Medium-high** — `Ir`-visible, judged by the existing churn benches | design note + audit of the bulk/word-level bitmap users | best win-per-risk of the structural changes |
| 5 | **F12** — large-cache HIT rewrites ~130 B of header; ~5 words actually changed | ~10–20% of a ~45 ns cache hit, at most | **High** — `Ir`-visible, `large_alloc_free_cycle` exists | add the `debug_assert_eq!` that would **falsify** it, run it | cheap to try, small win |
| 6 | **F9** — `Instant::now()` guard is a cliff keyed on `used > headroom`; the shipped low-headroom profiles cross it by design | anchored at **~105 ns/call** by task #95's own before/after, ×2 per large cycle | **Medium** — needs a confound-free arm (hit rate moves with headroom) | re-run an existing headroom A/B straddling the guard at fixed hit rate | measurement-first; **the doc fix is worth doing regardless** |
| 7 | **F3** — `Ir` is blind to what's left; 6 items on one missing macro-bench | unblocks 6 (7 with F10's sibling artifact) findings at once | **Low** — it *is* the missing proof | scope the ≥64-live-segment harness as its own task | **large potential, needs real infra first** |
| 8 | **F8** — large-cache scans walk 56 B/slot AoS to read 8 B; 7 lines (35 extended) → 1 (5) | 4 scans/cycle; ~21 line-touches → ~3 at base-8 | **Medium** — the occupancy-bitmask subset is clean; the two sidecars replicate a field | ship/measure the bitmask subset ALONE first | medium code, real maintenance-cost risk (X5's failure mode) |
| 9 | **F11** — Windows over-reserves 2× VA per segment, no aligned-reservation API; Unix fast path's hit rate unmeasured | Unix half **bounded at ≤1.0–1.3%** by R29-3; Windows half unmeasured, and Windows is item 16's own named trigger | **Low-medium** — no Windows perf artifact exists in the corpus | 2 `bench-internals` counters (hit/total) in `unix_reserve` — free, settles the Unix half outright | step 1 trivial; step 2 a full task; step 3 real engineering |
| 10 | **F2** — `OWN_CACHE_SIZE = 4` direct-mapped; a Large-heavy workload thrashes it by construction | Tier-1 hit 8.8% of a free's `Ir`; Tier-2 12.0 Ir/call — the thrashing case has **never** been measured | **Low** — R23-3 §1.3 already proved a portable workload **cannot** force Tier-2 | build the Tier-1 hit/miss counter (independently useful; closes item 1's own last open clause) | trivial change, **the judge is the hard part** |
| 11 | **F10** — every cross-thread ring push reads the consumer-dirtied `head` line; classic shadow-head is one step further than PERF-PASS-4 went | removes one cross-core-coherent line read per xthread free | **Low** — no producer/consumer harness exists, and `Ir` cannot see it | build the N-producer/1-consumer wall-clock harness | large potential, needs real infra first |
| 12 | **F1** — interleave the two bitmaps (pure-locality form) | cache/TLB only | **Low** — subsumed by F1b's strictly stronger form; blocked on F3's macro-bench | do F1b instead | superseded; revisit only if F1b is rejected |
| 13 | **F13** — negative results: over-alignment classification, TLS/registry binding, NUMA | — | n/a | one `cargo asm` check of the Windows TLS lowering | **don't revisit** (recorded so it isn't re-derived) |
| 14 | **F5** — clz `class_for` vs the 16 KiB `SIZE2CLASS` LUT | — | n/a | none | **confirmed dead, and deader** — recommend narrowing item 19's trigger wording |

**The three buckets, stated plainly.**

- **Cheap to try, benefit genuinely unproven:** F4 (expect a 0 `Ir` delta by
  construction — its case is "restore the documented intent at zero cost", not
  a promised number), F12 (small by its own anchor), F2's constant change
  (trivial edit, but its judge does not exist and R23-3 proved the obvious one
  cannot be built portably).
- **Real, measurable, and shippable this round:** F6, F7 — both have
  separately-measured component costs, an already-committed judge, and edits
  of a handful of lines. F7 additionally carries an obligation independent of
  whether anything ships: append a dated correction to R31-0 naming the ON/OFF
  arm asymmetry (its bias is *against* ON, so no published verdict flips).
  F1b is the largest such win but is a genuine medium-sized change.
- **Large potential, needs real infra investment first:** F3 (the ≥64-segment
  macro-bench), F10 (a multi-thread ring harness), F11 step 2 (the Windows
  equivalent of R29-3's decomposition — note this is item 16's *own* stated
  trigger (b), and OPEN_ITEMS `[L]` item 24's unexplained Windows wall-clock
  signal is the standing evidence that the Windows OS-interface layer is the
  single largest unmeasured surface in this codebase).
- **Confirmed dead, do not revisit:** F5 (the LUT — with a recommendation to
  *narrow* item 19's revisit trigger so a future round does not re-derive the
  density argument), and all three parts of F13.

**If only three things are done:** F6, then F7 (with its R31-0 addendum), then
scope F3's macro-bench as its own task. The first two are close to free and
give the round a real, defensible number; the third is what makes the next
five rounds possible.

**Two cross-cutting observations worth carrying forward.** (1) Findings F1,
F4, F8 and F12 are the same shape — this project has tuned its subsystems'
*policy* exhaustively (five open items on the large cache alone, all about
`headroom_bytes`/`budget_bytes`/`pool_segments`/slot counts) and has never once
audited their *bytes*: struct field order, array-of-structs vs
structure-of-arrays, and how much of a structure a hot loop actually needs.
That is an unworked seam, not four coincidences. (2) Findings F6, F7 and F12
are also one shape — **work that a previous optimization already made
unnecessary but never removed**: F6 is the Э9/P7.1 `_with_base` optimization
that was applied to the dealloc path and never back-ported to realloc; F7 is
the P4 stamp hoist applied to `alloc` and not to its `alloc_zeroed` twin; F12
is a full-struct write kept after the values it writes became carried-forward
copies. Each is a residue of a correct earlier decision, and each is cheap.


---

## Findings

### F1 — The two per-segment bitmaps are laid out exactly 32 KiB apart, so every own-thread small free touches at least 3 distinct metadata cache lines (NEW — not in either open-items index)

**What / where.**

- `src/alloc_core/segment_header_layout.rs:28-43` — `Layout::alloc_bitmap_off()`
  and `Layout::magazine_bitmap_off()`. The magazine bitmap is placed
  `align_up_const(alloc_bitmap_off() + AllocBitmap::FOOTPRINT, 8)`, i.e.
  immediately AFTER the whole alloc bitmap.
- `src/alloc_core/segment_bitmap.rs:50` —
  `FOOTPRINT = SEGMENT / MIN_BLOCK / 8` = `4 MiB / 16 / 8` = **32 KiB**, and
  both `AllocBitmap::FOOTPRINT` (`alloc_bitmap.rs:73`) and
  `MagazineBitmap::FOOTPRINT` (`magazine_bitmap.rs:93`) alias that same
  constant — identical geometry, identical index math
  (`segment_bitmap.rs:114`: `bit = off >> MIN_BLOCK_SHIFT`).
- Consumers on the hot free path:
  `src/registry/heap_core_free.rs:681-710` —
  `off`, `meta = SegmentMeta::new(base)`, then
  `meta.magazine_bitmap().is_in_magazine(off)` (line 683),
  `meta.bump_of()` (line 705, `alloc-decommit`),
  `meta.alloc_bitmap().is_free(off)` (line 708), then on the push arm
  `meta.magazine_bitmap().mark_magazine(off)` (line 722).

**Why it might be slow today.** Because the two bitmaps have identical
geometry and are stored back-to-back, the alloc bit and the magazine bit for
the SAME block offset are always exactly `FOOTPRINT` = 32 KiB apart in the
segment. They can therefore never share a cache line, never share a 4 KiB
page, and never share a TLB entry. One own-thread small free consequently
touches, at minimum:

1. the magazine-bitmap byte (`is_in_magazine`),
2. the segment-header line for `bump_of()` (a third, unrelated region at
   segment offset ~0),
3. the alloc-bitmap byte (`is_free`) — 32 KiB away from (1),
4. the magazine-bitmap byte again (`mark_magazine`, hot after (1)),
5. the `tcache.classes[c]` line.

The corresponding alloc hit (`heap_core_alloc.rs:232-236`) touches (1) and (5)
again. For comparison, mimalloc's free is `block->next = page->local_free;
page->local_free = block; page->used--` — it touches the block itself (already
warm, the caller just used it) plus the one `mi_page_t` line. sefer-alloc
deliberately does NOT write the block body (`heap_core_free.rs:620-622`
explicitly claims this as a structural advantage on cold working sets), but it
pays for that by touching **two 32 KiB-separated metadata regions plus the
header** instead.

**Rough magnitude — honestly, unknown; no existing number isolates it.** The
existing measurement (`docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`,
OPEN_ITEMS `[L]` item 17) gives **12.19 Ir/hit** for the alloc-side
`clear_magazine` block = **54.5% of a magazine hit** — but `Ir` is an
INSTRUCTION count and is structurally blind to this hypothesis: interleaving
the bitmaps changes zero instructions, only which lines they land on. The
right instrument is the D1/LL miss + `Estimated Cycles` axis that
`iai-callgrind` already reports (X6's own reject note in
`docs/perf/IAI_BASELINE.md` cites "RAM hits unchanged (±4)", so the counter
exists and is already read in this project).

**Status.** BRAND NEW. Neither `docs/perf/OPEN_ITEMS.md` nor
`docs/CORRECTNESS_OPEN_ITEMS.md` contains any item about bitmap *placement*.
`[L]` item 21 (G1) proposed *merging* magazine residency INTO `AllocBitmap`
(a semantic change, rejected because it inverts load-bearing
`mark_alloc`/`mark_free` call-site invariants). This is the strictly weaker,
purely mechanical sibling G1 never considered: keep two independent bits with
unchanged semantics, just **interleave their storage** so that the alloc word
and the magazine word covering the same granule range sit in the same cache
line (e.g. alternate `u64` words: `[alloc[0], mag[0], alloc[1], mag[1], ...]`,
so one 64-byte line covers 4 alloc words + 4 magazine words = 256 granules =
4 KiB of payload, for both bitmaps at once). Total footprint is unchanged
(64 KiB); only `SegmentBitmap::locate` and the two `_off()` functions change.

**Blocker / caveat (be honest about this one).** The measured win is a
cache-locality win, and **this project's current bench set is structurally
unable to show it**: `multiseg_cold_256k` spans only 3 segments, and
OPEN_ITEMS `[L]` item 34 already records "the missing artifact: a realistic
≥64-live-segment / long-lived-process macro-benchmark" as a standing
structural blocker that has independently killed four prior findings
(X5/item 20, T10/item 22, R1/item 23, R15-1/item 9). This candidate is the
FIFTH item to bottleneck on the same missing artifact, which is itself a
signal about where the highest-leverage project investment is (see F9 below).

**What would be needed to capture it.**
1. A design note establishing that `SegmentBitmap::locate` is the single
   choke point for both bitmaps (it is — `segment_bitmap.rs:110-115`), so an
   interleaved addressing scheme is a one-function change plus the two
   `_off()` constants; audit the bulk/word-level users
   (`AllocBitmap` clear-pass, `init_in_place`, segment reset) which currently
   assume contiguity.
2. A judge that can SEE the difference: per CLAUDE.md's path-activation-oracle
   rule, it needs (a) a working set large enough that the metadata regions do
   not both stay L1-resident, and (b) an oracle proving the arm actually took
   the own-thread magazine free path (`dbg_small_zero_pass_count` is the
   wrong counter here; the natural one is a free-path counter that does not
   yet exist).
3. Cost side, same regime: `init_in_place` zeroing cost and any bulk-clear
   pass must be measured under the interleaved layout too, not just the
   probe path.

Scope: **medium local change** (one `locate` + two offset constants + audit of
bulk users), **large measurement cost** (needs the missing macro-bench).

#### F1b — the stronger form: a single 2-bits-per-granule state word, which is ALSO an `Ir` win (so it is NOT blocked on the macro-bench)

Re-reading the free path after writing F1, the interleaving idea has a
strictly better variant that does not depend on cache effects at all.
`src/registry/heap_core_free.rs:681-710` currently does:

```text
let off  = (ptr - base) as u32;              // once
let meta = SegmentMeta::new(base);
meta.magazine_bitmap().is_in_magazine(off)   // base+MAG_OFF   -> locate(off) -> load -> test
meta.bump_of()                               // base+0
meta.alloc_bitmap().is_free(off)             // base+ALLOC_OFF -> locate(off) -> load -> test
meta.magazine_bitmap().mark_magazine(off)    // base+MAG_OFF   -> locate(off) -> load|store
```

`SegmentBitmap::locate` (`src/alloc_core/segment_bitmap.rs:110-118`) is the
SAME pure function of `off` in all three calls, applied to three different
region bases. On the fast (legit-free) path **both oracles always run** — a
live block is in neither, so neither can short-circuit the other. The two
loads therefore always both execute, always from two lines 32 KiB apart, with
the byte-index/bit-index arithmetic recomputed each time.

Replacing the two 1-bit-per-granule bitmaps with ONE **2-bit-per-granule**
word array (bit pair = `{allocated?, in-magazine?}`) gives:

- **identical total footprint** — 2 × 32 KiB today vs 1 × 64 KiB;
- **one `locate`, one load** answering BOTH oracles (a straight instruction
  saving, visible in `Ir`, unlike F1's pure-locality form);
- **one cache line** instead of two on the free path, and the alloc-hit
  `clear_magazine` now lands on a line the free path already warms.

Crucially, this is NOT what `[L]` item 21 (G1, 2026-07-10) rejected. G1
proposed *redefining* `AllocBitmap`'s single bit to also mean magazine
residency; its blocker (quoted in the item) was that this "requires
*inverting* existing load-bearing optimizations at multiple call sites" —
`refill_class_bump_impl`'s `mark_alloc` premise, `carve_batch`'s leave-unset,
and `reclaim_offset_checked`'s separate `is_in_magazine` scan. A 2-bit
widening changes **none** of those semantics: `mark_alloc`/`mark_free` keep
operating on their own bit, `is_in_magazine`/`mark_magazine`/`clear_magazine`
keep operating on theirs; only the ADDRESSING is shared. G1's blocker is a
semantics blocker, and this variant has no semantics change to block on.

Note also that this region is not exhausted the way `[L]` item 17 implies:
RAD-5's own GO entry (`docs/perf/IAI_BASELINE.md`, "RAD-5 GO (2026-07-11/12)")
shows the bitmap earned **−3,262…−3,327 raw Ir on all four churn benches**
(−52 Ir/op) by replacing a variable-trip-count scan with a straight-line
probe. Its own stated mechanism — "replacing it with the bitmap's
unconditional straight-line probe let the compiler generate a tighter fast
path" — argues that going from *two* straight-line probes to *one* is the
same kind of move again, not a diminishing one.

**Risk to check before building this.** `AllocBitmap` and `MagazineBitmap`
also have **bulk** users where a 2-bit interleave costs more than it saves:
`init_in_place` (`segment_bitmap.rs:73`, the zero-fill at segment bootstrap —
unaffected, same total bytes), the virgin-init-skip elision
(`tests/regression_virgin_bitmap_skip.rs` pins this), any word-at-a-time
clear pass, and — most importantly — `flush_class`/`flush_run`'s per-block
`mark_free` (`R28_1_FLUSH_CLASS_ISOLATION_GATE.md` measures this at 449 Ir /
8 blocks), where the free-path's magazine-clear loop
(`heap_core_free.rs:762-768`) and `flush_class`'s `mark_free` currently touch
two separate regions per block and would newly touch one — a probable second
win, but it must be measured, not assumed.

**Judge.** Unlike F1, this one CAN be judged by the existing instrument:
`npm run iai` on the four churn benches plus `cold_alloc_free_256x16b` /
`recycle_alloc_free_256x16b`, against the same ±10 raw-Ir churn kill gate
RAD-5 itself was measured under (`IAI_BASELINE.md`'s own table format). No
new macro-benchmark is required for the primary verdict; the macro-bench only
adds the locality upside on top.

Scope: **medium** — new addressing in `SegmentBitmap::locate` (or a new
`DualBitmap` type), two `Layout::*_off()` constants collapse to one, every
call site keeps its current name and meaning. Real but bounded; the
correctness surface is already pinned by existing counterfactual tests
(`in_magazine_double_free_is_noop`,
`refill_window_does_not_double_issue_in_out_buffer_resident_block`,
`drain_resident_xthread_double_free_no_corruption`,
`realloc_path_drain_respects_magazine`) which RAD-5's own verification
section confirms are non-vacuous.

---

### F2 — `OWN_CACHE_SIZE = 4`: the free path's Tier-1 ownership cache is a 4-entry direct-mapped array that a Large-heavy workload must thrash by construction (NEW as a *tuning* candidate)

**What / where.**

- `src/alloc_core/segment_table.rs:140` — `pub(crate) const OWN_CACHE_SIZE: usize = 4;`
- `:181` / `:233` — `own_cache: [*mut u8; OWN_CACHE_SIZE]`, per `SegmentTable`
  (i.e. per heap).
- `:497-510` — `contains_base`: Tier-1 is
  `own_cache[(base >> SEGMENT_SHIFT) & 3] == base`; on a miss it falls
  through to `hash_contains` (Tier-2, `:934-956`, 8192-slot open-addressing
  linear probe with backward-shift deletion).
- Call site: `src/registry/heap_core_xthread.rs:756-787` — `dealloc_routing`
  calls `contains_base(base)` on **every single free**, before anything else.

**Why it might be slow today.** `cache_index(base) = (base >> 22) & 3`
addresses 4 buckets. A `SegmentKind::Large` allocation owns its own
4 MiB-aligned segment (`AllocCore::alloc_large`,
`src/alloc_core/alloc_core_large.rs`), so a workload with N concurrently-live
Large objects has N distinct hot bases going through this cache. At N > 4 the
cache is thrashing by construction, and — because it is DIRECT-mapped, not
associative — even N == 4 only works if the OS handed out four bases whose
bits 22-23 happen to differ. Every miss becomes an 8192-slot (64 KiB) hash
table probe, whose first probe step is very likely an L2/LLC access on a cold
segment.

**Rough magnitude — partially measured, and the measured part understates it.**
- Tier-1 HIT cost: **8.8% of a real free's `Ir`** (523/5,920 over 64 calls
  ≈ 8.2 Ir/call) — `docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7
  (revised from the original 18.6% headline; OPEN_ITEMS `[A]` item 1).
- Tier-2 (hash probe) cost: **12.00 Ir/call = 13.0%** of that same workload's
  free total — `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §5 table row
  `dealloc_hash_contains_only_probe_16b` (8,349 Ir / 64 calls), measured via
  the purpose-built `SegmentTable::dbg_hash_contains_only` hook.
- **Both figures are `Ir` only.** R22-17 §1.2 states explicitly that its
  number is a *conservative lower bound* precisely because its workload never
  leaves Tier-1, and R23-3 §1.3/§6.2 states that a portable workload
  **cannot** be built to force Tier-2 (the index depends on OS-assigned
  addresses). So the actual cost of Tier-1 thrashing on a Large-heavy
  workload has never been measured at all, in `Ir` OR in cycles.

**Status.** RE-ASSESSMENT with a NEW twist. OPEN_ITEMS `[A]` item 1 covers
`contains_base`'s cost and is marked "ANSWERED, not open … region judged
likely exhausted for further micro-optimization at the per-block-cost scope",
BUT its own Current-state card leaves this exact door open, verbatim:
*"Separately, Tier-2-hash-probe-heavy workloads might show `contains_base` >
8.8% (open, not a proven floor)."* Every remediation attempt logged under that
item (R24-3, R24-4, R25-3, R26-7, R28-1) targeted the **magazine-overflow**
mechanic, not the routing prefix, and — critically — **no round has ever tried
the single cheapest knob in the whole region: raising `OWN_CACHE_SIZE`.**
A grep of `src/`, `docs/perf/*.md` and `docs/*.md` finds `OWN_CACHE_SIZE`
mentioned only as a *caveat* in five places; it is never itself the subject of
an experiment.

**What would be needed to capture it.** This is the cheapest live experiment
in this whole survey:
1. `OWN_CACHE_SIZE` 4 → 16 (or 32) is a **one-constant change**. It must stay a
   power of two (the mask at `:561` requires it). Cost: 96 (or 224) extra bytes
   per `SegmentTable`, i.e. per heap — negligible next to the 64 KiB hash table
   already sitting beside it, and the array is `null`-initialised at `:233`
   with no other structural coupling.
2. The judge, however, is the hard part, and R23-3 §1.3 already documented
   exactly why: **the workload cannot portably force Tier-2**, so an `Ir` gate
   on the existing bench set will show a flat 0 delta and "prove" nothing. The
   honest instrument is the existing `dbg_hash_contains_only` hook plus a
   *counter* of Tier-1 hit/miss (which does not exist yet — this would be the
   path-activation oracle CLAUDE.md's R30-8 rule requires), driven by a
   multi-segment workload; the metric is the hit-rate, not the wall clock.
3. Only with a measured Tier-1 miss rate > 0 on a realistic shape does a
   wall-clock/cycles A/B become meaningful.

Scope: **trivial code change, real measurement design work.** The measurement
design (a Tier-1 hit/miss counter) is independently useful — it converts
R22-17's and R23-3's "we cannot force Tier-2, so we don't know" into a number,
and it is the missing evidence for OPEN_ITEMS item 1's own last open clause.

---

### F3 — Meta-finding: the project's deterministic judge (`Ir`) is structurally blind to the class of win that is left, and six separate items are now blocked on the same missing macro-benchmark

**What / where.** This is not a code site; it is a pattern across the corpus.

- `npm run iai` / `benches/perf_gate_iai.rs` is the project's designated
  deterministic judge (CLAUDE.md, "Before every push"), and the kill-thresholds
  quoted in reject after reject are raw **`Ir`** thresholds: "±10 raw Ir,
  X4-B precedent" (OPEN_ITEMS `[L]` items 20/22/23), "far past the ±10
  hot-path kill threshold" (item 18).
- The benched shapes span **at most 3 segments** (`multiseg_cold_256k`; stated
  in `[L]` item 20's own mechanism note: *"at n=3 segments the maintenance RMW
  … is a net cost"*), and the churn shapes live **entirely in the primordial
  segment** (`R22_17_…GATE.md` §1.2).

**Why this matters for finding future speedups.** Every remaining structural
difference between this allocator and mimalloc that I can identify in the hot
path is a **memory-hierarchy** difference, not an instruction-count
difference:

| Mechanism | sefer-alloc | mimalloc | Visible in `Ir`? |
|---|---|---|---|
| own-thread free ownership check | `contains_base` — 4-entry cache, else 64 KiB hash probe (F2) | `_mi_ptr_segment(p)->thread_id` — one header load | Only the hit case |
| free-path double-free oracles | 2 bitmaps, 32 KiB apart, + header `bump_of` (F1) | none (block body write) | No |
| alloc-hit bookkeeping | `clear_magazine` RMW into segment metadata | `page->free = block->next` (block already warm) | Partly (12.19 Ir, item 17) |
| size→class | 16 KiB `SIZE2CLASS` LUT | small bin LUT | No (X6 saw "RAM hits unchanged (±4)") |

`Ir` counts instructions; it cannot distinguish a load that hits L1 from one
that misses to DRAM. `iai-callgrind` DOES report `Estimated Cycles` and RAM
hits (X6's reject note cites them — `docs/perf/IAI_BASELINE.md`), so the
counter exists — but a cache-simulation metric is only meaningful if the
benchmark's working set is large enough to miss, and this project's benchmarks
are deliberately tiny (CLAUDE.md, "Speed: short scenario by default":
`sample_size(10)`, "the entire suite in a few seconds").

**Status.** PARTIAL RE-STATEMENT of OPEN_ITEMS `[L]` item 34 ("The missing
artifact: a realistic ≥64-live-segment / long-lived-process macro-benchmark"),
which already records that FOUR items (X5/20, T10/22, R1/23, R15-1/9) are
blocked on it. What is new here is the count and the framing:

1. **F1 and F2 above both bottleneck on the same artifact**, taking it to
   **six** blocked items, not four.
2. The item-34 entry frames the artifact as needed for the *segment-scan*
   family of ideas specifically. It is actually needed for a broader class:
   **anything whose benefit is a cache-line or TLB effect rather than an
   instruction count.** That includes bitmap co-location (F1), ownership-cache
   sizing (F2), and — see F5 below — the `PerClass` magazine's own layout.
3. Item 34's status is `[L]` low-priority. On the evidence of the last ~10
   rounds (five consecutive NO-GO/exhausted verdicts under item 1, four
   independent rejects citing "n=3 segments" as the reason), **the marginal
   value of another `Ir`-judged micro-experiment is close to zero, and the
   marginal value of the missing macro-benchmark is the highest in the
   backlog.** That is a re-prioritisation recommendation, not a new mechanism.

**What would be needed.** Not a gate — a *harness*. Concretely: a long-lived,
multi-thread, ≥64-live-segment workload with a mixed Small/Large size
distribution and a steady-state (not burst) shape, run under
(a) `iai-callgrind` for `Estimated Cycles`/RAM-hits and (b) the existing
`scripts/paired-ab-runner.mjs` process-level judge for wall clock. Per
CLAUDE.md's R30-8 rule it must carry a path-activation oracle per arm; the
counters that would serve are mostly missing today and are listed per-finding
above.

---

### F4 — `PerClass` is missing `#[repr(C)]`, so rustc reorders `count` to offset 128 and the documented "one cache line" magazine optimization is NOT actually in effect (NEW — verified empirically, zero-cost fix)

**What / where.** `src/registry/tcache.rs:163-210`:

```rust
#[derive(Clone, Copy)]
pub(crate) struct PerClass {
    pub(crate) count: u8,
    pub(crate) slots: [*mut u8; TCACHE_CAP],   // 16 pointers = 128 B
    #[cfg(feature = "virgin-zero-skip")]
    pub(crate) virgin_mask: u16,
}
```

Its own doc comment (`tcache.rs:140-162`) states the intent explicitly:

> "PERF-PASS-5 (G7/FP2, task #53): one size class's magazine — `count` and
> `slots` bundled together so a magazine push/pop touches ONE cache line
> instead of two. … Grouping `count` and `slots` into one `PerClass` struct …
> puts a class's depth counter directly adjacent to (in front of) its own
> pointer stack, so both live in the same 8-byte-aligned region and — for the
> common case where a hit/push touches only the top few slots — the SAME
> 64-byte cache line."

**Why it might be slow today.** The struct has **no `#[repr(C)]`**, so its
field order is `repr(Rust)` — unspecified, and rustc's actual layout algorithm
sorts fields by descending alignment, which puts the 8-aligned `slots` array
FIRST and the 1-byte `count` LAST. I verified this rather than assuming it
(scratch `rustc -O` probe outside the repo, using `core::mem::offset_of!`):

| variant | `size_of` | `align_of` | `offset_of(slots)` | `offset_of(count)` | `offset_of(virgin_mask)` |
|---|---:|---:|---:|---:|---:|
| production (no `virgin-zero-skip`) | 136 | 8 | **0** | **128** | — |
| with `virgin-zero-skip` | 136 | 8 | **0** | **130** | 128 |

Array stride is 136 in both cases.

So under `production`, for class `c` the magazine hit at
`heap_core_alloc.rs:161-206` reads `count` at byte `c*136 + 128` and
`slots[count-1]` at byte `c*136 + 8*(count-1)`. For the common shallow
magazine (`count` 1-3, which R22-17 §1.2 and the `Э6` comment at
`heap_core_free.rs:601-602` both state is the churn-workload reality: *"in
churn cnt is 1–3"*), those two addresses are **112-128 bytes apart — always a
different 64-byte cache line.** The optimization the doc comment describes is
therefore not in effect; what task #53 actually achieved was moving `count`
from ~6 KiB away to 128 bytes away (still a separate line, but same page).

**Rough magnitude — no existing number; but the fix is free, which changes the
cost/benefit.** No gate report measures `PerClass` field offsets. What makes
this worth doing anyway is that the fix costs **nothing**:

- Adding `#[repr(C)]` and declaring `count, virgin_mask, slots` in that order
  gives `count` at 0, `virgin_mask` at 2, `slots` at 8 — total size **136 with
  or without `virgin-zero-skip`** (verified by the same arithmetic: 1 + 2 + 5
  pad + 128), i.e. **byte-identical size and stride to today** in the
  production configuration, and 8 bytes *smaller* than today's 136+pad in the
  `virgin-zero-skip` configuration would otherwise need.
- With `count` at struct offset 0 and stride 136, `count` and `slots[0..6]`
  land in the same 64-byte line for **7 of every 8 classes** (the line offset
  of class `c` is `(c*136) mod 64 = (c*8) mod 64`; only `c ≡ 7 (mod 8)` puts
  `slots[0]` past the line boundary). Today it is **0 of 8**.

**Status.** BRAND NEW. `PerClass`, `repr(C)`, and field ordering appear
nowhere in `docs/perf/OPEN_ITEMS.md` or `docs/CORRECTNESS_OPEN_ITEMS.md`. This
is not a new mechanism — it is a **latent regression against an
already-decided, already-documented optimization** (task #53 / PERF-PASS-5
G7/FP2), of exactly the class an independent review would call a
"documentation says X, code does Y" defect. Arguably it belongs in
`docs/CORRECTNESS_OPEN_ITEMS.md` as a doc-vs-code divergence as much as in the
perf index.

**What would be needed to capture it.**
1. A structural regression test asserting the offsets (`offset_of!(PerClass,
   count) == 0` and `offset_of!(PerClass, slots) == 8`), so the layout claim
   in the doc comment becomes enforced rather than aspirational. This crate
   already uses exactly this pattern for layout invariants — see the
   `const _: () = assert!(TCACHE_CAP <= 16, …)` at `tcache.rs:71-74`, which
   pins a different `PerClass` invariant the same way.
2. `npm run iai` on the four churn benches for the kill gate. Expect **0 `Ir`
   delta** (same instructions, different addresses) — so per CLAUDE.md's
   derived-not-hand-typed rule the report must be explicit that its verdict
   rests on `Estimated Cycles` / D1-miss deltas, not `Ir`, and must state
   honestly that the churn benches are L1-resident and may show nothing. The
   real justification here is "restore the documented intent at zero cost",
   not a promised number.
3. Note the interaction with F1b: if `PerClass` is touched anyway, consider
   whether the top-of-stack access pattern (pop from `slots[count-1]`) argues
   for a downward-growing stack so that the hot end is adjacent to `count` at
   ALL depths, not just shallow ones. That IS a semantic change and needs its
   own justification — flagged, not recommended.

Scope: **one attribute + one field reorder + one const-assert.** The smallest
change in this survey by a wide margin.

---

### F5 — The 16 KiB `SIZE2CLASS` LUT is NOT the cache problem X6's revisit trigger implies (re-assessment: looks dead, and here is why)

**What / where.** `src/alloc_core/size_classes.rs:171-183` —
`S2C_LEN = size2class_len(SMALL_MAX, MIN_BLOCK)`, a `[u8; ~16192]` static
(`SMALL_MAX` ≈ 253 KiB / `MIN_BLOCK` 16 B ≈ 16.2 K entries ≈ **15.8 KiB**).
Indexed at `crates/size-classes/src/lib.rs:359`:
`self.size2class[(need - 1) >> self.min_block_shift]`.

**The known item.** OPEN_ITEMS `[L]` item 19 (X6, 2026-07-05) rejected
replacing the LUT with a clz-based computation: churn `Ir` 0 delta (the
compiler const-evals `class_for` for the benches' fixed sizes),
`realloc_grow` **+658 Ir**, Estimated Cycles regressed on 10/11 benches. Its
revisit trigger is: *"a REAL-application cache profile (not microbenches)
showing SIZE2CLASS lines contending."*

**Fresh re-assessment — the trigger is unlikely to ever fire, for a reason
X6's own text hints at but does not draw out.** The index is
`(size-1) >> 4`, which is **dense from zero**. That means the hot region of
the table is determined by the workload's size distribution, and real
allocation size distributions are overwhelmingly small:

- sizes ≤ 1 KiB → indices 0..63 → **exactly one 64-byte cache line**;
- sizes ≤ 4 KiB → indices 0..255 → 4 cache lines;
- sizes ≤ 64 KiB → indices 0..4095 → 64 cache lines.

So the "16 KiB footprint" framing overstates the risk substantially: a
workload would have to be dominated by *large, widely-scattered* small-class
sizes (tens of KiB, spread across many distinct 1 KiB-wide index bands) before
the LUT's tail is touched at all — and such a workload is by construction
allocation-rate-limited elsewhere (each op moves ≥16 KiB of payload, so one
extra L2 hit on a class lookup is noise). X6's observation "RAM hits unchanged
(±4), so the LUT's 16 KiB footprint never surfaced as misses" is therefore not
just a microbenchmark artifact — it is the expected result for any
small-object-dominated workload too.

**The one variant X6 did NOT try, and why I still do not recommend it.**
mimalloc's real design is a hybrid: a direct-indexed
`mi_heap_t::pages_free_direct[]` for the small range plus a clz-based
`_mi_bin()` above it — i.e. keep the LUT for small sizes and compute for large
ones. X6 measured only the *all-clz* replacement, which is why `realloc_grow`
(a dynamically-sized path) regressed +658 Ir. A hybrid would leave the hot
small path at exactly one indexed load (0 `Ir` delta by construction) and
shrink the table by ~15.75 KiB. But per the density argument above, the
15.75 KiB it removes is the part **that was never hot**, so the expected win is
approximately zero — and it adds a branch on `size` to the hottest path in the
allocator, which is exactly the shape the X4-B "won-front" rule rejects.

**Status.** RE-ASSESSMENT of `[L]` item 19 (X6). Verdict: **still dead, and
now deader** — I would go further than item 19 and say its revisit trigger
should be narrowed from "a real-application cache profile" to "a real
application whose size distribution is dominated by scattered ≥16 KiB
small-class sizes", which is a much rarer thing than the current wording
suggests and is worth one line in the item so a future round does not spend
time re-deriving this.

**What would be needed.** Nothing. Recommend narrowing item 19's trigger
wording only.

---

### F6 — `realloc`'s move leg re-derives `base` and re-runs the full `contains_base` ownership probe it *already performed 170 lines earlier* in the same function (NEW — the cheapest Ir-visible win in this survey)

**What / where.** `src/registry/heap_core_free.rs`, inside
`HeapCore::realloc`'s own-segment branch:

- `:917` — `let base = os::segment_base_of_ptr(ptr);`
- `:922` — `if self.core.contains_base(base) { … }` — the ownership probe.
  Everything below runs with `base` in hand and `contains_base(base) == true`
  already **proven**.
- `:1085` — move leg allocates: `let new_ptr = self.alloc(new_layout);`
- `:1096` — move leg frees the old block: `unsafe { self.dealloc(ptr, old_layout) };`

That last call goes to `HeapCore::dealloc` (`:239`) → under `alloc-xthread`,
`self.dealloc_routing(ptr, layout)` (`src/registry/heap_core_xthread.rs:755`),
which **starts by recomputing exactly the two things `realloc` already has**:

- `heap_core_xthread.rs:756` — `let base = os::segment_base_of_ptr(ptr);`
- `:776` — `if self.core.contains_base(base) { … }`
- `:783` — `self.dealloc_own_thread_with_base(ptr, layout, base);`

`dealloc_own_thread_with_base` is `pub(super)` and is **defined in
`heap_core_free.rs` itself** (`:298`) — the very file `realloc` lives in — and
its entire reason for existing (the Э9/P7.1 comment at `:285-295`) is
"take a pre-computed `base` so the caller that already has one does not
recompute it." `realloc`'s move leg is precisely such a caller, and it is the
one call site that does *not* use it.

The same pattern repeats a second time at `:1360`, in
`try_promote_to_large` (`self.dealloc(ptr, old_layout)` with `base` already a
function *parameter*, `:1278`).

**Why it might be slow today.** Both recomputed operations have already been
independently isolated and priced by this project's own gates:

- `os::segment_base_of_ptr` — **~9.03 Ir**, the isolated figure R23-1
  measured and R29-10 reproduced byte-identical (OPEN_ITEMS `[L]` item 17's
  own current-state card cites it twice).
- `contains_base` — **8.2 Ir/call** on a Tier-1 hit (523 Ir / 64 calls,
  `docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7); **12.00
  Ir/call** if it degrades to the Tier-2 hash probe
  (`docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §5, row
  `dealloc_hash_contains_only_probe_16b`).

So the redundancy is worth roughly **17–21 Ir per move-leg realloc**, plus a
second, unnecessary read of the Tier-1 `own_cache` line (and, on a Tier-1
miss, a 64 KiB-hash-table probe whose first step is likely an L2/LLC access —
see F2). Note the failure mode is *worse* than the naive estimate in exactly
the shape realloc creates: `self.alloc(new_layout)` at `:1085` runs
**between** the two probes and may itself register a fresh segment, so the
second `contains_base` is not even guaranteed to still be the Tier-1 hit the
first one was.

There is also a smaller, likely-already-free sibling: `realloc`'s
foreign-pointer leg at `:1123` calls `os::segment_base_of_ptr(ptr)` a second
time in the same function body (the `:917` binding is scoped inside the
`#[cfg(feature = "alloc-global")]` block). `segment_base_of_ptr` is a pure
`map_addr`, so LLVM almost certainly CSEs this one — flagged for completeness,
not as a claim.

**Rough magnitude — the component costs are measured (above); the composite is
not.** No gate report measures `realloc`'s move leg's internal breakdown. What
makes this candidate unusual is that the *judge already exists and is already
wired*: `benches/perf_gate_iai.rs::realloc_grow` (`:2032`) drives 16 geometric
doublings 64 B → 4 MiB through the real `#[global_allocator]` `SeferAlloc`
face, and most of those doublings cross a size class and therefore take the
move leg. This is the same bench R13-6 and R14-6 both used as their decisive
instrument (`+102.3%` / `+52.7%` and the 2x→4x growth-factor decision,
respectively), so its sensitivity to this exact path is established, not
hoped for.

**Status.** BRAND NEW. Neither index contains any item about `realloc`'s
internal redundancy. `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` and
OPEN_ITEMS `[L]` item 12 concern *avoiding* the move leg (in-place grow);
`[D]` item 6 (R22-16 promotion remap) concerns making the move leg's
**memcpy** cheaper. Nobody has looked at the move leg's own **bookkeeping**
prologue/epilogue. Note this is the mirror image of the Э9/P7.1 optimization
(task #160) that already fixed this exact redundancy on the *dealloc* path —
the realloc path was simply never revisited when that landed.

**What would be needed to capture it.**
1. **The correctness argument, stated explicitly** (this is the real work, not
   the edit): is `contains_base(base)` still true at `:1096`, after
   `self.alloc(new_layout)` ran? Argument: the old block is still LIVE at that
   point (it has not been freed yet), so its segment's live count is > 0, so
   no path inside `alloc` — `find_segment_with_free`'s ring drain →
   `release_or_pool_empty_segment`, `drain_small_pool`, `evict_*` — can
   unregister it (they all require an empty segment). A gate must verify this
   by enumerating every unregister call site reachable from `HeapCore::alloc`,
   not assert it. The same argument covers `try_promote_to_large`'s `:1360`.
2. Replace `self.dealloc(ptr, old_layout)` with a direct
   `self.dealloc_own_thread_with_base(ptr, old_layout, base)` under
   `all(alloc-global, fastbin)` and `self.dealloc_own_thread(ptr, old_layout)`
   otherwise — mirroring `dealloc_routing`'s own `#[cfg]` split at
   `heap_core_xthread.rs:782-785` verbatim so the two can be diffed by eye.
3. Judge: `npm run iai` on `realloc_grow` (primary, expect a **negative** raw-Ir
   delta) plus the four churn benches as the standing ±10 raw-Ir kill gate
   (they do not realloc, so they must be flat — a non-zero delta there means
   the change perturbed codegen, which is itself the result).
   Per CLAUDE.md's R30-8 rule the report needs a path-activation oracle
   proving the arm actually took the **move leg** rather than the in-place
   OPT-F/OPT-G short-circuit; the natural one is a
   `bench-internals`-gated move-leg counter, which does not exist yet.
   Per CLAUDE.md's entry-point rule, note explicitly that this bench drives
   `SeferAlloc::realloc` (the real `#[global_allocator]` face) → `HeapCore::realloc`
   — the layer the change is at — not a bare `AllocCore`.
4. Counterfactual: `tests/` already pins realloc's cross-thread routing
   behaviour; confirm at least one test would go RED if the direct call were
   given the *wrong* base (e.g. by asserting the freed block reappears in this
   heap's own magazine/free list, not another heap's ring).

Scope: **~4 lines changed + one enumeration argument**, judged by an
already-committed bench. Highest ratio of (measurable win) to (cost of first
proof) in this survey.

---

### F7 — `alloc_zeroed`'s magazine-hit path pays a `stamp_segment_owner` that plain `alloc`'s magazine-hit path deliberately does NOT — an asymmetry that is both a free win AND a confound inside the currently-open R31-0 measurement (NEW)

**What / where.** Two structurally identical magazine-hit arms in
`src/registry/heap_core_alloc.rs`, one of which stamps and one of which does
not:

- **Plain `alloc`'s hit arm** (`:156-255`). Its own comment at `:165-167` is
  explicit: *"P4: NO stamp here — the block's source segment was already
  stamped during the refill that originally pulled it. The OPT-C cache
  guarantees the segment header still carries our ownership."* No
  `stamp_segment_owner` call anywhere in the arm.
- **`alloc_small_zeroed_via_magazine`'s hit arm** (`:337-375`, the path
  `alloc_zeroed` takes for a small class under `virgin-zero-skip` + `fastbin`).
  Same pop, same `virgin_mask` maintenance, same `clear_magazine` — and then
  `:373` **`self.stamp_segment_owner(issued);`** before returning.

The P4 justification applies to BOTH arms verbatim: both are fed by a refill
(`refill_magazine_slow` `:718-732` / `refill_magazine_slow_virgin` `:443-453`)
whose stamp-dedupe loops are line-for-line identical and stamp every distinct
source segment before any block lands in the magazine. There is no stated
reason for the difference; `alloc_small_zeroed_via_magazine`'s own doc comment
(`:326-330`) claims the opposite intent — that the plain `alloc` path was kept
a *separate* call site precisely so it "pays NOT ONE EXTRA INSTRUCTION" — with
no note that the new sibling took on an extra one of its own.

**Why it might be slow today.** `stamp_segment_owner`
(`src/registry/heap_core_ownership.rs:129-159`) is `#[inline(always)]` but not
free even on its OPT-C cache hit:

1. `os::segment_base_of_ptr(ptr)` (`:132`) — **~9.03 Ir isolated** (R23-1,
   reproduced byte-identical by R29-10; cited in OPEN_ITEMS `[L]` item 17).
   Note `alloc_small_zeroed_via_magazine` **already computed this exact value**
   28 lines earlier at `:358` for `clear_magazine`, and does not pass it in
   (`stamp_segment_owner` takes `ptr`, not `base` — there is no
   `_with_base` sibling).
2. `base == self.last_stamped_segment` + `!is_null()` compare (`:143`).
3. `SegmentMeta::new(base).owner_state_atomic()` + a **Relaxed atomic load of
   the segment header word** (`:147-148`) — a load of a *segment metadata*
   line, i.e. a fourth distinct metadata region touched by this path on top of
   the three F1 already enumerates (magazine bitmap, alloc bitmap, header
   `bump_of`).
4. `unpack_owner_id(cur) == self.id` (`:149`).

So roughly **12–18 Ir plus one extra metadata cache line** per
`alloc_zeroed` magazine hit, entirely redundant under the arm's own P4
argument.

**Rough magnitude — no gate isolates it, but its two largest components are
separately measured** (the ~9.03 Ir `segment_base_of_ptr`, above; and R29-10's
12.19 Ir/hit for the structurally-comparable `segment_base_of_ptr` +
`SegmentMeta::new` + bitmap-RMW block that runs in the *same* arm, of which
~9.03 Ir was the base derivation and ~3.16 Ir the metadata RMW residual —
`docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`). That gate
measured a magazine hit at 22.4 Ir/op total (reproducing R23-3 exactly), so a
12–18 Ir addition on the `alloc_zeroed` variant of that same hit is
**plausibly a ~50-80% surcharge on the hit itself** — which would make this,
if it reproduces, one of the larger single-site findings in this survey, not a
rounding error.

**Status.** BRAND NEW, and it lands directly inside an item that is **open
right now**: OPEN_ITEMS `[D]` item 25 (`virgin-zero-skip`, REOPENED by R31-0 /
task #471). Two distinct consequences, and the second one is the more
important:

1. **As a speedup:** removing the stamp is a candidate improvement to the
   `virgin-zero-skip` ON path — i.e. to the exact feature whose promotion
   decision item 25 is currently holding open pending user sign-off.
2. **As a measurement confound in R31-0 itself:** R31-0's A/B compares an ON
   binary against an OFF binary through `HeapCore::alloc_zeroed`. Under
   `not(virgin-zero-skip)` that call routes to `self.alloc(layout)`
   (`heap_core_alloc.rs:585-595`) → the **no-stamp** hit arm; under
   `virgin-zero-skip` it routes to `alloc_small_zeroed_via_magazine` → the
   **stamping** hit arm. The ON arm therefore carries a per-hit cost that has
   nothing to do with skipping `Node::zero`. **This does not threaten R31-0's
   headline finding** — the confound penalises the ON arm, and ON still
   measured −89% to −98.6% on `notouch`, so the reported win is if anything an
   understatement — but it IS a real, unstated asymmetry between the two arms
   of a published A/B, of exactly the class CLAUDE.md's R26-4 / R30-8 /
   entry-point rule family exists to catch (here: same layer, same config,
   same code path label — but *not the same instruction sequence around the
   thing under test*). It is also a plausible partial explanation for the
   `onebyte`/`full` categories' sign-inconsistent results that R31-0 reports
   as unresolved noise: in those categories the zeroing saving is small, so a
   fixed ~12-18 Ir/hit ON-side surcharge is proportionally much larger.

**What would be needed to capture it.**
1. Decide whether the stamp is genuinely removable. The honest possibility
   that it is NOT: `alloc_small_zeroed_via_magazine` is reached from
   `alloc_zeroed`, which — unlike `alloc` — is *also* the entry point a
   `virgin-zero-skip`-without-`fastbin` build routes through
   `AllocCore::alloc_small_with_virgin` + an explicit stamp
   (`heap_core_alloc.rs:573-575`). If the `fastbin` arm's stamp was copied
   from that sibling by symmetry rather than by necessity, it is removable;
   if some `alloc_zeroed`-only caller can reach a magazine block whose segment
   was never stamped, it is not. Resolve by enumeration, not assumption — the
   only producers of magazine entries are the two refill functions and the
   free path's push (`heap_core_free.rs:722-724`), all of which run on blocks
   from already-stamped segments.
2. Judge: `npm run iai`. This one IS `Ir`-visible (instructions removed, not
   just relocated), unlike F1/F4 — the gate is a raw-Ir delta on the
   `virgin-zero-skip`-gated iai benches (`benches/perf_gate_iai.rs` already
   carries `#[cfg]`-gated `virgin-zero-skip` stubs at `:1268`), with the four
   churn benches as the ±10 raw-Ir kill gate (plain `alloc` is untouched, so
   they must be exactly flat).
3. Path-activation oracle (CLAUDE.md R30-8): the arm must prove it took the
   magazine **HIT** arm, not the refill miss — `dbg_tcache_virgin_mask` and
   the existing `SMALL_ZERO_PASS_CALLS`/`tcache_hits` counters can serve;
   a hit-vs-miss ratio must be reported per arm, since a hit-starved workload
   would show nothing whatever the change.
4. If confirmed, append a dated correction to
   `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` naming the asymmetry and
   the direction of its bias (against ON), per this project's append-only
   correction convention — regardless of whether the stamp is then removed.

Scope: **one line deleted** if removable; the real work is the enumeration in
step 1 and the R31-0 addendum in step 4.

---

### F8 — every large-cache scan walks a 56-byte-per-slot array-of-structs to read one 8-byte field, so a best-fit lookup touches 7 cache lines (35 with `large-cache-extended`) instead of 1 (5) — a structural sibling of F1, on a different data structure (NEW)

**What / where.**

- `src/alloc_core/alloc_core.rs:187-215` — `struct CachedLarge` has six
  8-byte fields (`reservation`, `reservation_len`, `base`, `usable_size`,
  `reserved_capacity`, `seq`). Verified layout (scratch `rustc -O` probe
  outside the repo, `size_of`/`align_of`): `CachedLarge` = **48 B**,
  `Option<CachedLarge>` = **56 B** (no niche — every field is a raw pointer,
  `usize`, or `u64`, none of which has one).
- `src/alloc_core/alloc_core.rs:95` — `LARGE_CACHE_SLOTS = 8`;
  `src/alloc_core/large_cache_extended.rs:112` —
  `LARGE_CACHE_EXTENDED_SLOTS = 32`. So the storage the scans walk is
  **448 B = 7 cache lines** (base) and **2,240 B = 35 cache lines**
  (base + extension), computed from the measured 56 B stride.
- The four scans, all `O(scan_bound)` and all reading full `Option<CachedLarge>`
  slots through `large_cache_slot_get` (`alloc_core_large_cache.rs:67`):
  1. **Best-fit lookup**, `alloc_core_large.rs:217-227` — runs on **every
     `alloc_large`**, reads only `slot.usable_size`.
  2. **Free-slot search**, `alloc_core_large_cache.rs:203-206`
     (`self.large_cache.iter().position(|s| s.is_none())`) — runs on every
     large-dealloc admission attempt, reads only the discriminant.
  3. **FIFO-oldest search**, `:492-497` (`oldest_occupied_slot`) — runs on
     every eviction, reads only `c.seq`.
  4. **Budget/eviction retry loop**, `alloc_core_large.rs:590-604` and its
     mirror in `AllocCore::dealloc`'s Large branch — re-runs (2) and (3) once
     per eviction iteration.

**Why it might be slow today.** Every one of those four scans is a
**structure-of-arrays problem solved as array-of-structs**. Scan (1) needs
`usable_size` and nothing else; an `[usize; 40]` sidecar of just that field is
320 B = **5 cache lines**, and the base-8 case collapses to **64 B = exactly
1 line**. Scan (3) needs `seq` only — a second `[u64; 40]` is another 5 lines,
1 for the base case. Scan (2) needs only occupancy — a single `u64` bitmask
makes it `trailing_ones()`, i.e. **zero lines and zero branches**, replacing a
linear `position()` walk entirely.

So the current shape touches, per large alloc/free **cycle** at base-8:
7 lines (best-fit) + up to 7 (free-slot) + up to 7 (oldest) ≈ **21 line
touches**, against a bitmask-plus-two-sidecars shape's **~3**. With the
extension materialised the same comparison is ~105 vs ~11.

**Rough magnitude — no existing number isolates the SCAN's own cost, and the
one adjacent measurement points the other way, which is why this needs care.**
OPEN_ITEMS `[L]` item 7 / `[D]` item 30 cover the O(8)-vs-O(40) *bound*
question, and R31-3 (task #466,
`docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md`) measured it
directly: the extended (40-slot) cache came out **FASTER at every N**
(t=7.1-17.8, sign 19-20/20), mechanistically attributed to the base cache's
own FIFO-eviction-and-refill cost during materialisation, "not the wider scan
bound itself." Read carefully, that is not evidence the scan is cheap — it is
evidence the scan's cost was *dominated* by an eviction cost the wider cache
avoids. This finding is the orthogonal axis R31-3 explicitly did not measure:
make the scan itself 7x narrower **without** changing the slot count, so the
two effects are separable for the first time.

**Status.** BRAND NEW as a *layout* candidate. `docs/perf/OPEN_ITEMS.md` has
five separate large-cache items (`[A]` 13, `[D]` 30, `[L]` 7, `[L]` 27, plus
`[D]` 29's dependency on 28) and `docs/CORRECTNESS_OPEN_ITEMS.md` none — every
one of them is about **policy** (`headroom_bytes`, `budget_bytes`,
`pool_segments`, slot count, retention, hit rate). Not one is about how the
cache's storage is laid out or how its scans read it. This is the same class
of finding as F1 (bitmap placement) and F4 (`PerClass` field order): the
project has extensively tuned this subsystem's *policy* and never once
inspected its *bytes*.

**What would be needed to capture it.**
1. Note the honest asymmetry between the three sub-changes: (2)'s occupancy
   bitmask is a pure win with no duplication hazard (the discriminant is
   derivable, not duplicated); (1) and (3) introduce a **replicated field**
   (`usable_size`/`seq` living in two places), which is a real correctness
   surface — every `large_cache_slot_set` / `large_cache_slot_take` /
   `evict_*` site must maintain the sidecar in lockstep, exactly the
   maintenance-cost failure mode that killed X5 (`[L]` item 20: "at n=3 the
   maintenance RMW on every transition is a net cost"). The bitmask alone may
   be the whole shippable subset; a gate should measure it separately from the
   sidecars rather than bundling all three.
2. Judge: this is **partly `Ir`-visible** (the bitmask removes a loop; the
   sidecars do not change instruction counts much but change which lines are
   read), so the report must state per CLAUDE.md's derived-not-hand-typed rule
   which axis carries its verdict — `Ir` for (2), `Estimated Cycles`/D1 misses
   for (1)/(3). `benches/perf_gate_iai.rs`'s `large_alloc_free_cycle`
   (`:2013`) and `seg_cycle_decommit_256k` are the existing instruments.
3. Path-activation oracle (CLAUDE.md R30-8): per arm, prove the workload
   actually **populated** the cache before scanning it — the existing
   `AllocCore`/`HeapCore::dbg_large_cache_hits` and
   `dbg_large_cache_total_slots` (`alloc_core_large_cache.rs:308`) counters
   give admissions and materialisation state; R29-13 §1.3's
   `used_post_teardown_max > 0` and R30-6 §1.3's `admissions_ok`+`hits_ok`
   assertions are the precedent pattern to copy verbatim. A scan over an
   **empty** cache touches nothing and would show a flat 0 — the single most
   likely way to accidentally publish a null result here.
4. Same-regime discipline (CLAUDE.md's R30-6 rule): the benefit is largest
   when the cache is FULL, and the extension only materialises when the base 8
   are full — so cost and benefit must be measured in one arm that actually
   crosses that boundary, not by combining a base-8 latency number with an
   extension-40 footprint number.

Scope: **medium** — (2) is a `u64` field plus three maintenance sites; (1)/(3)
are two parallel arrays plus the same three sites, with a real replication
invariant to pin by test.

---

### F9 — the large-cache decay tick's `Instant::now()` fast-path exit is a CLIFF keyed on `used > headroom`, and the profiles R30-7/R31-9 just shipped are designed to put workloads on the wrong side of it (NEW — a cost of a shipped feature that nobody has priced)

**What / where.**
`src/alloc_core/alloc_core_large_cache.rs:320-356`, `maybe_decay_large_cache`:

```text
if self.large_cache_used_bytes <= self.decay_config.headroom_bytes {
    return;                              // <- the whole optimization
}
let now = std::time::Instant::now();     // <- everything past here pays it
```

Its own comment (`:321-330`) states the cost being avoided and the workload it
was validated against, verbatim: *"avoid `Instant::now()` (a
`QueryPerformanceCounter` syscall on Windows, ~50-100 ns) … This covers the
dominant benchmark workload (alloc+free cycle with one cached span at ~4-16
MiB, **far below the 256 MiB default headroom**) and restores the ~45 ns
cache-hit timing that the unconditional clock read had regressed to ~150 ns.
See task #95."*

Two call sites, both unconditional on their respective paths:
- `src/alloc_core/alloc_core_large.rs:140` — top of **every** `alloc_large`.
- `src/alloc_core/alloc_core.rs:1467` — the Large branch of **every**
  `AllocCore::dealloc`.
- (plus `alloc_core_large.rs:566`, `reclaim_large_segment`, on the
  cross-thread reclaim path.)

So a steady-state large alloc/free cycle calls it **twice**.

**Why it might be slow today.** The guard is not "is there work to do
*now*" — it is "**is the cache above its headroom at all**", which is a
persistent, workload-shaped state, not a transient one. Once
`large_cache_used_bytes` sits above `headroom_bytes`, *every* large alloc and
*every* large free pays a full wall-clock read, forever, even though
`run_decay_step` will do nothing on all but one call per `decay_interval`
(default 1000 ms). The clock read is the expensive part and it is on the
outside of the interval check, not the inside.

This matters *now* specifically because of what shipped in Round 30/31:

- `src/alloc_core/profile.rs:287-291` — `LargeCachePolicy::LowHeadroom` sets
  `headroom_bytes = 16 MiB`; `LargeCachePolicy::Trimmed64MiB` sets **64 MiB**;
  `Default` keeps 256 MiB.
- The entire measured *point* of those two variants (R30-6, R29-13, R31-1) is
  to let a heap whose working set is tens-to-hundreds of MiB retain **less**
  — i.e. to be **above** its headroom during normal operation. R31-1
  (`R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md`) explicitly measured
  128 MiB and 288 MiB bursts against 64 MiB headroom: those arms sit above the
  guard by 2x-4.5x for the whole burst.

So the guard silently holds for the `production` default and silently **fails
for both non-default profiles under exactly the workloads they were designed
for**. Nothing in `profile.rs`'s (long, careful) doc comments mentions this;
the two policies are documented purely as an RSS-floor-vs-hit-rate trade.

**Rough magnitude — one anchored estimate exists, from this file's own
history.** Task #95's note above records the real before/after it caused when
the clock read WAS unconditional: **~45 ns → ~150 ns** on a large-cache hit,
i.e. the clock read cost roughly **~105 ns per call** on that machine, or
**~2.3x the entire hit**. Two calls per alloc/free cycle. That is the
*measured* magnitude of the thing the guard removes — and it is exactly what a
sub-headroom-configured workload pays back. Note the specific Windows
relevance: `Instant::now()` is `QueryPerformanceCounter`, and OPEN_ITEMS
`[L]` item 24 (R5-R2b) already records an unexplained **Windows-native
wall-clock churn effect that `Ir` cannot see** and that this project has never
had the tooling to chase. A per-op `QueryPerformanceCounter` is precisely the
shape of cost that is invisible to `Ir` (it is a handful of instructions plus
an opaque call) and visible to a wall clock — this candidate is a concrete,
checkable hypothesis in that otherwise-empty space, not a new mystery.

**Status.** BRAND NEW. `docs/perf/OPEN_ITEMS.md`'s five large-cache items
(`[A]` 13, `[D]` 30, `[L]` 7, `[L]` 27, `[D]` 29) are all about policy VALUES;
`docs/CORRECTNESS_OPEN_ITEMS.md` has nothing. The decay tick's own cost has
never been measured at all — R29-13 §"Current number/verdict" reasons about
`maybe_decay_large_cache`'s *first-call timer-priming rule* and about how
often a decay tick FIRES, but never about what the check itself costs when it
does not fire. Item 31's P2-5 and item 27's narrowing both examined
`Trimmed64MiB`'s hit-rate honesty; neither considered that the same setting
also switches on a per-op clock read.

**What would be needed to capture it.**
1. **Cheapest first probe, no code change at all:** re-run an existing
   large-path A/B at two `headroom_bytes` values chosen to straddle the guard
   for the SAME working set (e.g. working set ~128 MiB, headroom 256 MiB
   [guard holds] vs 64 MiB [guard fails]), and read the latency delta. The
   harness already exists — `examples/r30_6_large_cache_headroom_ab_gate.rs`
   and R31-1's crossing-regime example sweep exactly this knob through the
   real `#[global_allocator]` via `scripts/paired-ab-runner.mjs`. **Careful
   confound (this is the whole design problem):** the two arms differ in
   *both* the clock-read frequency AND the hit rate (R31-1 measured a real
   12.5-point hit-rate loss at that boundary), so a naive A/B cannot
   attribute the delta. The arm must therefore either (a) hold hit rate fixed
   and vary only the guard (e.g. two runs at the same headroom, one with the
   clock read forced on via a `bench-internals` switch), or (b) report the
   `dbg_large_cache_hits` delta alongside the latency so the reader can see
   the hit rate did not move. Per CLAUDE.md's same-regime rule, (a) is the
   honest design and (b) is only a check on it.
2. **If confirmed, the fix is structural, not a tuning knob:** move the
   interval check *ahead* of the clock read by keeping a cheap monotonic
   op-counter (e.g. "only consult the clock every Nth large op past the
   headroom"), or cache the last `Instant` and only re-read when a coarse
   counter says the interval could plausibly have elapsed. Both preserve the
   1000 ms decay semantics approximately; a gate must state which semantic it
   is trading (decay granularity) and pin it by test, since
   `dbg_force_decay_tick` (`:426-446`) and R29-13's forced-convergence
   measurement both depend on the current shape.
3. **Regardless of whether the fix ships:** add the cost to
   `LargeCachePolicy::LowHeadroom`'s and `::Trimmed64MiB`'s doc comments. They
   are the shipped, user-facing surface, they already carry a carefully
   maintained regime caveat (R31-9/task #473), and "this policy also enables a
   per-large-op wall-clock read" is the same class of disclosure.
4. Path-activation oracle (R30-8): a `bench-internals` counter of
   "`maybe_decay_large_cache` calls that passed the guard" — trivially addable
   next to the existing `DECOMMIT_CALLS` static — is the exact instrument, and
   it is also what proves arm (a) above actually differed.

Scope: **measurement first, ~10 lines if it confirms.** The doc-comment fix
(step 3) is worth doing on its own even if the perf finding turns out small.

---

### F10 — every cross-thread free reads the ring's consumer-written `head` cache line, so PERF-PASS-4's own cache-line split guarantees a 2-line, cross-core-coherent push instead of a 1-line one (NEW — the classic "shadow head" the design stopped one step short of)

**What / where.** `src/alloc_core/remote_free_ring.rs`:

- `:551` `HEAD_OFF = 0`, `:557` `TAIL_OFF = 64`, `:563` `OVERFLOW_OFF = 68`,
  `:569` `SLOTS_OFF = 128`. PERF-PASS-4 (G8/ML4, task #52) deliberately widened
  the cursor block from 16 B to 128 B so `head` (consumer-only writes) and
  `tail`/`overflow` (producer-touched) sit on **separate 64-byte lines** —
  see the extensive rationale at `:522-540`.
- `RemoteFreeRing::push` (`:752-787`), the producer side, on **every**
  cross-thread free:
  ```text
  let t = self.tail().load(Relaxed);        // line @64  (producer line)
  let h = self.head().load(Acquire);        // line @0   (CONSUMER line)  <-- this one
  if t.wrapping_sub(h) >= RING_CAP { … }    // RING_CAP = 256 (:194)
  self.tail().compare_exchange_weak(t, t+1, AcqRel, Relaxed)   // line @64
  self.slot(t).store(offset, Release);      // line @128+
  ```
- `try_push_uncounted` (`:812-838`) has the byte-identical shape.
- `drain` (`:856-907`), the consumer side, ends with
  `self.head().store(h, Release)` (`:905`) — i.e. it **dirties line @0 on
  every drain**.
- Call sites: `src/registry/heap_core_xthread.rs:998-1005` →
  `push_with_overflow_retry` (`:1216`) → `ring.push(packed)` (`:1221`).

**Why it might be slow today.** The `head.load(Acquire)` at `:759` exists for
exactly one purpose: the full-check. It is a read of a line the **consumer
Release-stores on every drain**. So on the canonical cross-thread shape this
whole subsystem exists for — producer thread frees, owner thread drains — the
producer's line @0 read is invalidated by the owner's drain and must be
re-fetched from the owner's cache (a cross-core coherence miss, tens to ~100+
cycles on a modern multi-socket or even cross-CCX box), while the producer's
line @64 CAS is separately contended among producers. PERF-PASS-4 correctly
identified that these two lines must not be the same line — and then left the
producer reading both of them anyway. The split removed *false* sharing and
left the *true* sharing in place.

The standard fix (DPDK's rte_ring, folly's `MPMCQueue`, Vyukov's own bounded
queue notes) is a **shadow/cached head**: keep a `cached_head: AtomicU32`
replica **in the producer line** (there are 56 unused padding bytes at
offsets 72..128 — see the layout diagram at `:47-51`), and:

```text
let t = tail.load(Relaxed);
let ch = cached_head.load(Relaxed);          // same line as tail — free
if t.wrapping_sub(ch) >= RING_CAP {          // only NOW consult the real head
    let h = head.load(Acquire);
    cached_head.store(h, Relaxed);
    if t.wrapping_sub(h) >= RING_CAP { …overflow… }
}
```

With `RING_CAP = 256` and the module's own statement that *"the owner drains on
every alloc, so the ring rarely fills under normal churn"* (`:150-151`), the
refresh branch is taken essentially never — so the common push drops from
**2 lines (one of them cross-core-dirty) to 1**.

Soundness sketch (to be proven, not assumed, by whoever builds this): the
shadow is a pure **hint that can only be STALE-LOW** (`head` is monotonic and
only ever advances, and a stale-low `ch` can only make the queue look *more*
full than it is). A stale shadow therefore causes at worst one extra real
`head.load(Acquire)` — never a missed overflow, never a lost entry, never a
slot reuse before drain. The `Acquire` on the real `head` load, which is what
actually establishes the "a drained slot is observable" edge documented at
`:76-79`, is preserved exactly, on the only branch that consumes it.

**Rough magnitude — honestly, none exists.** No gate report in
`docs/perf/` measures cross-thread free push cost at all: the corpus's
cross-thread work is R6-OPT-P0-1 (avoiding a bind/spinlock for a bind-less
dealloc), R6-OPT-P0-4 (the overflow-first retry policy), and R12-6's dedup
buffer — all about *routing decisions* and *overflow handling*, never about
the push's own memory traffic. And the benched shapes cannot show it: this is
a **cross-core coherence** effect, and `benches/perf_gate_iai.rs` runs
single-threaded under Callgrind (which models a cache hierarchy but not
inter-core invalidation traffic at all). This is therefore a **seventh** item
bottlenecked on missing multi-threaded macro-measurement — related to, but
NOT the same as, OPEN_ITEMS `[L]` item 34's ≥64-live-segment blocker: item 34
needs many SEGMENTS; this needs many THREADS actively producing into one
segment's ring while its owner drains. Worth naming as a distinct missing
artifact rather than folding into item 34 (see F3's re-prioritisation
argument, which this strengthens).

**Status.** BRAND NEW. Neither index mentions `RemoteFreeRing`'s push cost.
PERF-PASS-4 (task #52) is the closest prior art and is recorded only in the
source comment at `:522-540`, not in `OPEN_ITEMS.md` at all.

**What would be needed to capture it.**
1. A producer/consumer harness that does not yet exist: N producer threads
   freeing blocks allocated by one owner thread, owner draining, measured in
   **wall clock** via `scripts/paired-ab-runner.mjs` (the project's
   process-level judge) — `Ir` is structurally blind here, so the report must
   say so up front per CLAUDE.md's derived-not-hand-typed rule.
2. Path-activation oracle (R30-8): prove the arm actually pushed to the RING
   and did not divert. The counters already exist and are already read by
   `SeferAlloc::stats()`: `DBG_RING_OVERFLOW` (`:176`) must be **0** (a
   non-zero overflow count means the arm was measuring the retry/overflow tier,
   not the fast push), and `ring_overflows` / `large_xthread_reclaimed` give
   the routing split. A per-arm push count would need a new
   `bench-internals` counter.
3. Cost side, same regime: the shadow adds a `Relaxed` store to the producer
   line on the refresh path. Prove that a ring under genuine pressure (the
   near-full regime where the refresh fires every push) is not made *worse* —
   this is the exact same "maintenance RMW dominates at small N" failure mode
   that killed X5 (`[L]` item 20), and it must be measured in the SAME arm
   that measures the benefit, not a separate one (CLAUDE.md's R30-6 rule).
4. Correctness: `tests/remote_ring_unit.rs` already builds a ring over a plain
   `Box<[u8]>` and asserts `reclaimed + overflowed == pushed`
   (`RemoteFreeRing::over_test_buffer`, `:633`) — the ideal counterfactual
   harness, and a stale-shadow bug would break that invariant, so the existing
   test is non-vacuous for this change. Also re-run the `dbg_set_cursors`
   (`:684`) `u32::MAX → 0` wrap test: the shadow is a wrapping counter too and
   the wrap is where a naive `<`-comparison implementation would break.

Scope: **small-to-medium code change (one `u32` in existing padding + ~8 lines
in two push functions), large measurement investment** — the harness does not
exist and is the real cost.

---

### F11 — every Windows segment reservation over-reserves `size + align` (2× the VA for a 4 MiB segment) and never trims it, because the backend has no aligned-reservation fast path at all — unlike the Unix backend, which has one whose hit rate has never been measured (NEW; the Windows half sits on OPEN_ITEMS item 16's own named-but-unexplored trigger)

**What / where.** `crates/vmem/src/lib.rs`, the two backends of
`reserve_aligned_raw`:

- **Windows** (`:789-866`). `reserve_aligned_raw` → `win_reserve_commit`,
  which unconditionally does `let over = size.checked_add(align)?;` (`:807`)
  then `VirtualAlloc(NULL, over, MEM_RESERVE, …)` (`:812`), finds the aligned
  base inside it, commits only `commit_len` (`:837-844`), and returns
  `(base, region, over)` — **the full `over`-byte reservation is kept**.
  `release_reservation` (`:869-874`) later frees the whole region with
  `VirtualFree(region, 0, MEM_RELEASE)`. Windows cannot partially release a
  `MEM_RESERVE` region, so there is no trim step and none is attempted.
  With `SEGMENT = 4 MiB` (`src/alloc_core/os.rs`) used as BOTH size and align
  for a small segment, `over = 8 MiB` — **every 4 MiB segment permanently
  reserves 8 MiB of address space.**
- **Unix** (`:1020-1093`). `unix_reserve` FIRST tries
  `try_reserve_aligned_exact` (`:1098-1120`): one `mmap` of exactly `size`,
  then `if !region_addr.is_multiple_of(align) { munmap; return None }`. On
  failure it falls through to the over-reserve path: `mmap(size + align)` +
  `munmap(head)` + `munmap(tail)`.

**Why it might be slow today.**

*Windows (the substantive half).* Two costs, one certain and one to be
measured:
1. **Syscalls per segment: 2, unconditionally** — one `VirtualAlloc`
   `MEM_RESERVE` + one `VirtualAlloc` `MEM_COMMIT`. Windows 10 / Server 2016+
   has `VirtualAlloc2` with a `MEM_EXTENDED_PARAMETER` of type
   `MemExtendedParameterAddressRequirements` carrying a
   `MEM_ADDRESS_REQUIREMENTS { Alignment }` field, which returns an
   alignment-satisfying reservation directly — collapsing reserve+commit for
   an aligned span into **one** call with **no over-reservation**. This is,
   to my knowledge, exactly what mimalloc's Windows primitive does (its
   `win_virtual_alloc` tries `VirtualAlloc2` and falls back to
   over-allocate-and-retry) — **worth verifying against mimalloc's actual
   source before citing it as precedent**, since this survey is read-only and
   I did not check it.
2. **2× virtual-address amplification, permanently.** At the `MAX_SEGMENTS`
   ceiling (4095) that is ~32 GiB of reserved VA instead of ~16 GiB. On 64-bit
   this is not an OOM risk, but it is not free either: every reservation is a
   VAD-tree node with a range twice as wide, and `large-reserved-capacity`
   multiplies the base figure further (its own `LARGE_RESERVED_CAP_BYTES` is
   64 MiB, `src/alloc_core/alloc_core_large.rs:42` → 128 MiB reserved per
   qualifying Large segment). Whether that costs measurable **time** is
   exactly the unknown.

*Unix (the half I would NOT pursue — and here is why, from this project's own
data).* The exact-mmap fast path is a **coin flip that is a net syscall LOSS
below a 50% hit rate**: a hit costs 1 syscall, a miss costs 1 (`mmap`) + 1
(`munmap`) + the fallback's 1 + up to 2 = **5**, versus 3 for going straight to
over-reserve. So the fast path pays off only if it hits more than half the
time, and its hit rate is entirely at the kernel's discretion: for 4 MiB
alignment on Linux, `thp_get_unmapped_area` aligns large anonymous mappings to
2 MiB when THP is enabled (giving roughly even odds), and without THP the odds
are ~1/1024. **Nothing anywhere counts this.** BUT — and this is the honest
bound —
`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` (OPEN_ITEMS `[L]`
item 16) already decomposed the Linux segment-lifecycle cycle and found the
entire avoidable (non-page-fault) share is **1.0–1.3%**, with **98.7–99.0%**
irreducible page-fault cost. A syscall-count change lives inside that 1.0–1.3%.
So the Unix half's ceiling is already measured and it is small.

That asymmetry is the finding's actual point: item 16's own "Next trigger"
names the untested case verbatim — *"revisit ONLY if … (b) the OS-backend
changes to one where recommit is a real separate syscall (Windows
`MEM_DECOMMIT`+`MEM_COMMIT`, where the VMA-teardown-vs-page-walk trade-off may
differ)"*. Windows is that backend, it is this project's own development
platform (`env`: win32 / Windows 10 Pro), and **no Windows equivalent of
R29-3's decomposition has ever been run.**

**Rough magnitude — none for Windows; bounded at ~1.0–1.3% for Linux (R29-3).**
There is a second, suggestive data point worth naming precisely because it is
unexplained: OPEN_ITEMS `[L]` item 24 (R5-R2b) records a **real, statistically
solid ~14–29% wall-clock churn slowdown measured on native Windows** whose
`Ir` counterpart went the *opposite* direction (−20.6%), and whose own closing
note lists the remaining candidate causes as *"real page-fault/`VirtualAlloc`/
decommit costs, TLB behavior, ASLR/base-address-dependent cache
conflicts"* — explicitly flagged as needing "Windows-native tooling (ETW / a
Windows perf-counter harness) this project does not currently have wired up."
**I am not claiming this finding explains that** — item 24's workload is
small-object churn, which reserves few segments. I am pointing out that the
Windows OS-interface layer is the single largest unmeasured surface in this
codebase, and that item 24 is the standing evidence that something real lives
there.

**Status.** BRAND NEW as a code finding (nothing in either index mentions
`vmem`'s reservation strategy, `VirtualAlloc2`, or the exact-mmap fast path's
hit rate); the *Windows-is-unmeasured* framing is a RE-STATEMENT of item 16's
trigger (b) and item 24's closing note, connected to a concrete mechanism for
the first time.

**What would be needed to capture it.**
1. **Cheapest first step, zero risk, answers the Unix question outright:** two
   counters (hit / total) in `unix_reserve` around
   `try_reserve_aligned_exact`, plus a Windows-side count of reserve+commit
   pairs. `crates/vmem` already has `SEGMENTS_RESERVED_TOTAL`/`..._RELEASED_TOTAL`
   plumbed through `src/alloc_core/os.rs:374-386` and surfaced in
   `AllocStats` — the same pattern extends in a few lines. Per CLAUDE.md's
   benchmark-hook rule these must be `bench-internals`-gated. This turns
   "the fast path might be a net loss" from speculation into a number, for
   free.
2. **The Windows decomposition R29-3 never got:** port
   `examples/r29_3_decomposition_gate.rs`'s methodology to the Windows
   backend (reserve / commit / decommit / release, timed separately, with the
   page-fault share separated out as R29-3 did on Linux). This is the
   genuinely valuable artifact and it is a task in its own right, not a
   sub-step. It also directly serves item 16's trigger (b) and would be the
   first Windows-native perf artifact in the corpus.
3. Only if (2) shows the reservation path is material: prototype
   `VirtualAlloc2`. **Non-trivial for this crate specifically** — `vmem`'s
   module doc (`:786-787`) states it declares raw bindings locally with *no*
   `winapi`/`windows-sys` dependency, relying on "std always links kernel32",
   and `VirtualAlloc2` is **not exported from kernel32.dll** (it lives in
   `KernelBase.dll` / `api-ms-win-core-memory-l1-1-6.dll`). A dependency-free
   implementation therefore needs runtime `GetProcAddress` resolution plus a
   fallback to today's over-reserve path on older systems — real work, and the
   reason this is step 3 and not step 1.
4. Immutable source identity BEFORE measurement, and a `platform=` field in
   the summary CSV that actually says Windows — the corpus's existing
   `_summary.csv` files are all Linux/WSL, and a Windows row must be
   distinguishable at a glance.

Scope: **step 1 is trivial and independently useful; step 2 is a full task;
step 3 is a real engineering change with a portability tail.**

**RESOLVED (2026-08-03, R32-13/task #504).** Steps 1 and 2 both shipped and
measured. Step 1: the Unix hit/total and Windows call-count counters exist
(`aligned_vmem::UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS`/`WINDOWS_RESERVE_COMMIT_CALLS`,
`bench-internals`-gated) — this Windows-only dev machine cannot exercise the
Unix side, but the counter is proven correctly wired (stays 0 on Windows, as
expected). Step 2: the Windows-native decomposition found the reservation
path's avoidable share is **4.3-4.8% (median 4.60%)**, well under the 20%
materiality threshold — larger than Linux's 1.0-1.3% (R29-3) but still small
in absolute terms, and page-fault cost still dominates (~95.4%). Step 3
(`VirtualAlloc2`) was explicitly declined: the evidence does not justify it.
A genuinely new finding along the way: on Windows, `VirtualAlloc(MEM_COMMIT)`
costs ~2x MORE than `VirtualAlloc(MEM_RESERVE)`, the opposite of the naive
expectation. See `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md`
for the full report and `docs/perf/OPEN_ITEMS.md` items 16/24 for the
cross-referenced index entries.

---

### F12 — the large-cache HIT path rewrites the entire ~130-byte `SegmentHeader` when only ~5 words actually changed, and 4 of the rewritten fields are copied back byte-identical from the cache entry (NEW; small, but `Ir`-visible and judged by an already-committed bench)

**What / where.** `src/alloc_core/alloc_core_large.rs:306-344`, the
`large_cache` hit arm of `alloc_large` — the "fast" path whose entire purpose
is to avoid an OS round-trip:

```text
let bump = hdr_aligned + align_up(size, align);
let hdr = SegmentHeader::large(
    u32::MAX,                 // placeholder, patched after register()
    size, align,              // <- genuinely new
    slot.usable_size,         // <- carried forward from the cache entry
    slot.reserved_capacity,   // <- carried forward
    bump,                     // <- genuinely new
    slot.reservation,         // <- carried forward
    slot.reservation_len,     // <- carried forward
);
Node::write_struct(slot.base as *mut SegmentHeader, hdr);   // :324 — FULL struct
… register() …
Node::write_u32(… segment_id …, id);                        // :344 — 1 word patch
```

Four of the eight constructor arguments (`usable_size`, `reserved_capacity`,
`reservation`, `reservation_len`) are, by this code's own comments at
`:310-313`, *"carried forward from the CACHED slot's own … the true physical
span of the segment being reused — NOT recomputed"* — i.e. they are **the same
values already sitting in that header in memory**, because the header was
written with them when the segment was created and nothing has changed them
since. The only fields that genuinely differ from what is already there are
`magic` (deliberately zeroed to 0 on deposit — `alloc_core.rs`'s Large-dealloc
branch and `alloc_core_large.rs:622-624`), `large_size`, `large_align`, `bump`,
and `segment_id` (already patched separately at `:344`).

`SegmentHeader` is ~130 bytes — the source's own dated notes track it growing
104 → 120 (`segment_header.rs:286-300`, RAD-3/E2) → 128 (`:546`) → larger
(`:604`), and the only compile-time pin is `assert!(size_of::<SegmentHeader>()
<= PAGE)` (`:1288`), so the exact current value is not asserted anywhere and
would need a probe to state precisely. Either way it spans **at least 2, more
likely 3, 64-byte cache lines**.

**Why it might be slow today.** A `write_struct` of ~130 bytes is ~16 stores
across 3 lines, and every one of those lines needs a read-for-ownership if it
is not already in the store buffer / L1 — on a large-cache hit the segment's
header line is by construction **cold** (the segment has been sitting unused in
the cache, possibly for many milliseconds, and on Unix its pages may have been
`MADV_DONTNEED`-ed). A targeted write of `{magic, large_size, large_align,
bump}` is ~4-5 stores on (probably) 1-2 lines.

Two secondary points in the same arm: the hit is preceded by the O(N) best-fit
scan F8 covers, and by the `Instant::now()` F9 covers — so the three findings
compose on the same path rather than competing.

**Rough magnitude — small, and I want to be honest about that.** The
only anchor available is the task-#95 note quoted in F9: a large-cache hit
timed at **~45 ns**. ~130 bytes of cold-line stores is plausibly single-digit
nanoseconds of that, i.e. **~10-20% of the hit at best**, and possibly much
less if the lines are warm. What makes it worth listing despite the small size
is the cost of proving it: the change is **`Ir`-visible** (fewer store
instructions, not merely relocated ones), and
`benches/perf_gate_iai.rs::large_alloc_free_cycle` (`:2013`) already exercises
exactly this alloc→cache-deposit→alloc-hit cycle under the real
`#[global_allocator]`. So the first proof step is a single `npm run iai` run
with no new harness at all.

**Status.** BRAND NEW. Not in either index. Adjacent prior work exists and
should be read first, because it constrains the fix: **UBFIX-6 (M-2)** — the
long comment at `:268-305` — deliberately REORDERED this arm so the full
header write happens while `slot.base` is still **unregistered**, closing a
data race against remote defensive readers (`magic_at`/`kind_at`/
`large_size_at`/`span_usable_at`). A field-wise write is sound under exactly
the same argument (nothing can address the segment yet), but the argument must
be restated, not assumed — this is a site where a previous round already found
and fixed a real race.

**What would be needed to capture it.**
1. Establish the current `size_of::<SegmentHeader>()` empirically (a
   `const _: () = assert!(size_of::<SegmentHeader>() == N)` pin is arguably
   worth adding on its own — the file's own three dated "this field grew the
   header from X to Y" notes show it has drifted three times with nothing
   catching it).
2. Replace the full `write_struct` with targeted field writes for
   `{magic, large_size, large_align, bump}` (the `SegmentHeader::*_at` /
   `set_*_at` accessors already exist for several of these), restating the
   UBFIX-6 unregistered-window argument for the new shape. **Verify the
   carried-forward premise by assertion, not by reading**: add a
   `debug_assert_eq!` that the in-memory `span_usable`/`reserved_capacity`/
   `reservation`/`reservation_len` already equal the cache entry's — if that
   ever fires, this finding is wrong and the full write is load-bearing.
   That debug assert is the cheapest possible falsification of the whole idea
   and should be run FIRST, before any edit.
3. Judge: `npm run iai` on `large_alloc_free_cycle` and
   `seg_cycle_decommit_256k`; the four churn benches as the ±10 raw-Ir kill
   gate (untouched path, must be flat).
4. Path-activation oracle (R30-8): the arm must prove it actually took the
   **cache-hit** arm and not `alloc_large_slow` — `dbg_large_cache_hits`
   already exists and R29-13 §1.3 / R30-6 §1.3 are the precedent for
   hard-asserting a non-zero hit count per arm before trusting its timing.

Scope: **small** — one call site, ~5 lines, plus a restated soundness
paragraph and one new debug assert.

---

### F13 — three areas checked and found thin: over-alignment classification (already optimized once, T10/perf#9), TLS/registry binding on the ordinary path (already minimal), and NUMA (not in `production`). Recorded as a NEGATIVE result so a future round does not re-derive it

**What / where, and the verdict for each.**

**(a) `Layout` alignment > `MIN_BLOCK` on the classification hot path —
already-worked ground, verdict THIN.**
`crates/size-classes/src/lib.rs:353-384` (`class_for`) and
`src/alloc_core/size_classes.rs:74` (`SMALL_ALIGN_MAX = MIN_BLOCK` = 16). Any
request with `align > 16` — which includes very common shapes like
crossbeam/tokio's 64- and 128-byte cache-padded types — misses the O(1) fast
path (`:360`) and enters the divisibility walk (`:368-383`), on **both** alloc
(`heap_core_alloc.rs:79`) and free (`heap_core_free.rs:315`). That sounds
promising until you check the history: OPEN_ITEMS `[L]` item 22's own text
records that T10's KEPT sub-finding (perf#9) is exactly *"`class_for`
align>16 jump-ahead walk over `SIZE2CLASS`"* — this walk has **already been
optimized once**, from a step-by-1 scan to the current bitmask-round-up jump,
and is correctness-pinned by
`tests/size_classes_slow_path_equivalence.rs`. The remaining walk is typically
1-2 iterations (each: one table load, one `block & (align-1)` test, one
`size2class` lookup), and the obvious next move — a 1-entry
`(size, align) → class` memo — adds a branch to the hottest path in the
allocator, which is precisely the shape X4-B's won-front rule (`[L]` item 18)
rejects. Also note C1 (0.3.0) already captured the LARGE win in this area:
before it, `align > 16` requests bypassed the magazine entirely on every
alloc AND free (see `heap_core_alloc.rs:137-154`'s comment). **Verdict: not
worth a round.** The one thing that WOULD change this is a measured real
workload dominated by over-aligned allocations, which would be a
workload-shape finding, not a code finding.

**(b) TLS binding / `HeapRegistry` on the ordinary alloc/free path — verdict
ALREADY MINIMAL.** `src/global/tls_heap.rs`'s three resolvers
(`current_for_alloc` `:394`, `current_for_alloc_with_config` `:520`,
`current_for_dealloc` `:489`) each reduce to **one `LOCAL.try_with` load plus
one unsigned compare**: the Э2 (task #145) trick at `:396-406` collapses the
two sentinels (`null` = 0, `TORN` = `usize::MAX`) into a single
`p.addr().wrapping_sub(1) < usize::MAX - 1` branch, so the hot arm is one
compare, not two. `LOCAL` is a `const`-initialised `Cell<*mut HeapCore>` with
**no `Drop`** (`:137`), which is the configuration where std's `thread_local!`
lowers to a direct `#[thread_local]` static access with no lazy-init check and
a `try_with` whose `Err` arm is statically dead. `HeapRegistry` is not touched
at all after the first bind — `bind_slow`/`claim` are `#[cold]` and run once
per thread (`:542-560`). R6-OPT-P0-1 already removed the one remaining real
cost here (a bind-less thread's `dealloc` used to claim a whole registry slot
and commit a 4 MiB primordial segment just to free one foreign pointer —
`:443-486`). **I found nothing further to take.** The one thing I could NOT
verify read-only, and which a future round could cheaply check, is whether the
Windows-MSVC lowering of `LOCAL.try_with` is genuinely the direct
`#[thread_local]` form or carries a per-access indirection — a `cargo asm` /
disassembly check of `SeferAlloc::alloc`'s prologue on the `x86_64-pc-windows-msvc`
target would settle it in minutes and is worth doing once, given item 24's
standing unexplained Windows wall-clock signal (see F11).

**(c) NUMA — verdict OUT OF SCOPE for `production`.** `crates/numa/` exists
(833 lines) with `src/alloc_core/numa.rs` (125 lines) as the in-crate seam, but
`numa-aware` is **not part of `production`**, so every NUMA-touching site
(`alloc_core_large.rs:382-407`, `:487-488`, `:349-353`) compiles out of the
shipped configuration entirely. Within the feature, the one plausibly-hot cost
— `numa::current_node()` per large allocation — is already cached with a
bounded refresh period (`AllocCore::current_node_cached`,
`src/alloc_core/alloc_core.rs:1172-1187`, R11-5/R12-5) and invalidated at
`claim` (`:1206-1211`). OPEN_ITEMS' "Recently resolved" trail also records
R10-6/R11-6's `class_nonempty_by_node` work as closed and **re-verified
still-closed by R25-9 against a stale re-flag** — i.e. this area has already
had one round wasted on re-raising a settled item, which is precisely the
outcome this negative-result entry exists to prevent a third time.

**Status.** DEAD / negative result, recorded deliberately. CLAUDE.md's
open-items convention is built around the observation that un-recorded
conclusions get re-derived (item 34 consolidates four independently-filed
items that all waited on one missing artifact; R25-9 re-verified an item a
review had wrongly re-flagged). A survey that lists only its positive findings
invites the next round to re-walk (a), (b) and (c) from scratch.

**What would be needed.** Nothing, except the one cheap check named in (b)
(disassemble the Windows TLS access once). If a future round disagrees with
(a), the trigger to state is a **measured** workload whose allocation mix is
dominated by `align > 16` requests — not a fresh code reading.

---

