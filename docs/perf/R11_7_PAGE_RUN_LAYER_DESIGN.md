> **DEFERRED — reassessed, no demonstrated production victim (2026-07-22,
> R12-13; wording corrected 2026-07-22, R13-4/task #274):** re-evaluated
> after `exact-span-large` (R12-3, `2593d30`) and `large-reserved-capacity`
> (R12-4, `fc155c9`) landed as opt-in features. Verdict: **NO-GO on
> implementing this design now** — R12-3, where its opt-in feature is
> enabled, closes the measured RSS/committed-bytes pain (up to 15.8x
> amplification down to ~1.00–1.05x) this design was ultimately justified
> by, and the remaining `SegmentTable`-slot/OS-reservation-syscall pressure
> this design's `PageRunTable` existed to avoid has no demonstrated victim
> anywhere in this codebase's tests/benches/examples (three of the four
> target classes route through the Small-class path instead of Large only
> when the opt-in `medium-classes-wide` feature is enabled, since
> `SMALL_MAX` = 1.75 MiB there). **Neither `exact-span-large` nor
> `medium-classes-wide` is part of `production`** — `medium-classes-wide`
> was separately NO-GO'd for `production` over a large realloc regression —
> so in `production`'s actual shipping composition, 1.25–1.75 MiB objects
> still route through Large with whole-`SEGMENT` rounding today. The
> correct status is therefore "deferred, no demonstrated production
> victim," not the earlier "superseded" wording, which read as though
> `production` itself had already solved the problem. See
> `docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md` for the full analysis. This
> design remains valid and reusable as a starting point if a real
> `MAX_SEGMENTS`-bound or reservation-syscall-bound many-live-medium-object
> workload is ever measured — nothing below this notice is changed or
> retracted.

# R11-7 — Page-run layer for the 1.25–2 MiB medium-classes-wide density gap: DESIGN-ONLY (no code change)

**Task:** #250 (R11-7) — design a genuinely new `SegmentKind` variant (the
"page-run layer") sized independently of the standard 4 MiB `SEGMENT`
constant, so the `medium-classes-wide` 1.25 / 1.5 / 1.75 / 2 MiB size-class
range gets real multi-block density instead of the near-1x packing it gets
today in a 4 MiB segment. This is the idea R10-4 §9 sketched in outline and
explicitly recommended as the real long-term fix for the density gap R8-9,
R9-4, and R10-4 all independently found.
**Outcome: DESIGN-ONLY.** No `src/`, `Cargo.toml`, or `tests/` file is
modified. The deliverable is this doc. §7 states the kill-gate table; §8
gives the verdict. Prototype code is authorized only on a separate, explicit
future request — this mirrors R10-4 and R11-3's two-stage discipline exactly
(design-doc stage first, mandatory review, prototype only later).
**Date:** 2026-07-21
**Base revision:** `main` @ `f0dd9a9` (R10-4 and R11-3 are this session's
structural precedents; R9-4 landed the `medium-classes-wide` substrate this
design extends).
**Platform:** Windows 10 Pro x86-64 (analysis host). The density arithmetic in
§2 is deterministic geometry (`floor(arena_size / block_size) - 1`, R9-4's
own formula, re-applied here to larger arena sizes), not timings — no
measurement is performed or required for this stage.

---

## 0. TL;DR — CONDITIONAL GO, but the addressing/directory redesign (§3) is
## substantially larger than a "bigger segment" framing suggests

The density win is real and re-verified against the actual `medium-classes-wide`
constants (§2): an 8 MiB page-run arena delivers density **5 / 4 / 3 / 3** for
the 1.25/1.5/1.75/2 MiB classes (vs today's 2/1/1/1 — the 2 MiB class does not
exist today; 1 is the density it WOULD get in a 4 MiB segment, exactly R9-4's
own finding for why it was excluded), and a 16 MiB arena
delivers **11/9/8/7** — matching R10-4 §9.1's illustrative table almost
exactly (recomputed here from the real `SEGMENT`/`SMALL_META_END` constants
rather than repeated uncritically; see §2.2 for the one place the real
arithmetic differs slightly from R10-4's rounded numbers).

**But this design is NOT "a bigger `SegmentHeader` with the same rest of the
substrate."** Systematically working through every mechanism that assumes
segment-uniformity (§4) surfaces that a page-run arena would need:

1. Its **own carve/refill/free-list logic**, forked from (not shared with)
   `carve_block`/`carve_batch`/`find_segment_with_free` — because those
   functions' geometry constants (`PAGES_PER_SEGMENT`, the page-map's fixed
   size, `SegmentBitmap::FOOTPRINT = SEGMENT / MIN_BLOCK / 8`) are baked to
   the GLOBAL `SEGMENT` constant, not parameterized per segment.
2. A **separate per-arena metadata region sized for the arena's own extent**,
   not the standard `small_meta_end()` — every bitmap in the substrate today
   (`AllocBitmap`, `MagazineBitmap`, `PageMap`) is `FOOTPRINT = SEGMENT /
   MIN_BLOCK / 8` or `PAGES_PER_SEGMENT = SEGMENT / PAGE`, a compile-time
   constant derived from the GLOBAL `SEGMENT`, not from "this segment's
   size." An 8/16 MiB arena reusing these unchanged would either under-size
   the bitmap (silent OOB) or waste 2×/4× the bitmap space it needs (if the
   global constant is left as an over-approximation, which does not work
   either — see §4.1 for why "just allocate `SEGMENT`-sized bitmaps and only
   use a prefix" does NOT work for `PageMap`/`AllocBitmap` without further
   change).
3. **Compatibility with `segment_base_of_ptr`'s O(1) masking IS achievable**
   for a power-of-two arena size, and this is the one piece of good news:
   §3.2 shows a `>SEGMENT`-sized, `SEGMENT`-aligned arena, ITSELF also a
   power of two and SELF-aligned, can still be found via a two-step
   resolution (`segment_base_of_ptr` finds the nearest `SEGMENT`-aligned
   candidate; a header-presence/backpointer check finds the TRUE page-run
   base if the candidate is an interior `SEGMENT`-slice of a larger arena).
   This is NOT the zero-change compatibility R10-4 §9's one-line framing
   ("the arena is still internally `block_size`-aligned, just bigger")
   implied — it requires a defined lookup protocol, detailed in §3.
4. The **segment directory and segment table both index by "one row per
   `SEGMENT`-sized slot"** (§3.3/§3.4) — a page-run arena spanning N
   `SEGMENT`-widths needs EITHER N directory/table slots pointing at
   fragments of the SAME arena (workable, but means "one segment" no longer
   implies "one table entry" anywhere else in the crate) OR a parallel,
   smaller table dedicated to page-run arenas (cleaner boundary, more new
   code). §3.4 recommends the parallel-table design and states why.

None of this is a NO-GO signal by itself — R10-4 and R9-4 both did work of
comparable shape (new bitmaps, new tables) and shipped. But the honest
finding this report must surface: **the design surface is closer in size to
"a second, smaller segment-table subsystem living alongside the existing
one" than to "extend `SegmentKind` with a fourth case and reuse everything
downstream."** §4's interaction table has 11 rows; only 2 are "reused as-is."
This is why the verdict (§8) is CONDITIONAL GO with a substantially larger
stage-2 estimate than R10-4/R11-3's designs, not a small bounded patch.

---

## 1. `SegmentKind` — the new variant and the exhaustive call-site inventory

### 1.1 Today's `SegmentKind`

`src/alloc_core/segment_header.rs:148-175`:

```text
pub(crate) enum SegmentKind {
    Primordial = 0,
    Small = 1,
    Large = 2,
    Unknown = 0xFF,   // L-5 (UBFIX-11): reject sentinel, never written
}
```

`kind_at` (`segment_header_views.rs:22-56`) strictly decodes the raw byte:
`0 → Primordial, 1 → Small, 2 → Large, anything else → Unknown`. This is the
exact mechanism to extend: a new discriminant value, say `PageRun = 3`, is a
one-line addition to the enum and a one-line addition to `kind_at`'s match
arm (`3 => SegmentKind::PageRun`). The `Unknown` reject-sentinel machinery
(every caller tests a SPECIFIC expected kind via `==`/`matches!`, never an
exhaustive catch-all except `AllocCore::dealloc`'s own match) means adding a
new legitimate discriminant is exactly as low-risk at the ENUM level as
`Unknown`'s own design intended — see §1.3 for why the CALL SITES are where
the real work is, not the enum.

### 1.2 Full inventory of `SegmentKind::` sites

Exhaustive `grep -rn "SegmentKind::" src/alloc_core/ src/registry/` — 44
matches across 17 files. Grouped by call-site SHAPE, each classified: does it
need new page-run-aware logic (N), can it safely no-op/ignore (I), or does it
need to explicitly reject (R)?

| # | File:line | Pattern | Classification | Reasoning |
|---|---|---|---|---|
| 1 | `alloc_core.rs:1070` | `SegmentKind::Large => { .. }` (dealloc's Large-free arm, whole-segment release/cache) | **N** | A page-run arena is not "one allocation per segment" like Large — it hosts many blocks via a BinTable, closer to Small. Must NOT fall into this arm (would release/cache the whole multi-block arena on a single block's free). |
| 2 | `alloc_core.rs:1244` | `SegmentKind::Small \| SegmentKind::Primordial => { .. }` (dealloc's small-free arm: derive class, `dealloc_small`) | **N** | `dealloc_small(base, ptr, class_idx)` assumes `base` is a `SEGMENT`-sized small segment (its BinTable/bitmap geometry is derived from the standard layout). A page-run arena's BinTable/bitmap live at DIFFERENT offsets (§4.1) — cannot literally join this arm; needs its own arm calling page-run-specific free logic that shares the SAME conceptual shape (derive class from Layout, push to a free list, decrement live-count) but different offset arithmetic. |
| 3 | `alloc_core.rs:1276` | `SegmentKind::Unknown => {}` (dealloc's reject arm) | **I** (pattern precedent only) | Not itself changed; establishes the precedent a corrupted/garbled `kind` byte is contained, not guessed — the same posture the new arm must NOT weaken. |
| 4 | `alloc_core.rs:1439` | `SegmentHeader::kind_at(base) == SegmentKind::Large` (in `safe_payload_read_span`, decides the committed-span upper bound: `span_usable_at` for Large, else the LITERAL `os::SEGMENT` constant) | **R (must reject / must not silently fall to the `else` branch)** | **This is the single most dangerous latent site if page-run is added carelessly.** The `else` branch is NOT "not Large" — it is "assumed exactly `SEGMENT` bytes, fully committed." A page-run arena is `N × SEGMENT` bytes; if its `kind_at` returns anything that lands in this `else` arm, `safe_payload_read_span` UNDER-reports the readable span for `N > 1` (harmless — merely conservative, clamps reads) but that undersizing masks a design smell: any function using this pattern (`if Large {A} else {SEGMENT}`) needs a THIRD arm for page-run, using the arena's own real span (stored in the header, exactly like `Large`'s `span_usable`), not the fixed constant. |
| 5 | `alloc_core.rs:1562` | `if kind == SegmentKind::Large` (OPT-G in-place-grow eligibility inside `realloc_inplace_fast_path_known_base`) | **I, PROVISIONALLY** — see §1.4 | A page-run block growing within its own class doesn't need OPT-G (it isn't Large); page-run→page-run growth crossing INTO Large is the realloc-promotion question R11-3 explored for medium-classes and is explicitly OUT OF SCOPE here (§5.3). |
| 6 | `alloc_core.rs:1578` | `matches!(kind, SegmentKind::Small \| SegmentKind::Primordial)` (OPT-F same-class in-place eligibility) | **N** | A page-run block reallocated within the SAME page-run class should also get an OPT-F-shaped no-op fast path (trivial, same reasoning as Small) — but the guard must explicitly include the new kind, not silently fall through as "false" (a correctness NO-OP today only because the new kind does not exist yet; once it does, OMITTING it here is a missed optimization, not a bug, but MUST be a deliberate choice documented at the site). |
| 7 | `alloc_core_core_diag.rs:92` | `SegmentKind::Small \| SegmentKind::Primordial` (dbg helper, likely a "is this a small-shaped segment" predicate) | **N** | Diagnostic surface should learn the new kind so debug tooling doesn't silently misreport page-run segments as "neither." |
| 8 | `alloc_core_core_diag.rs:219, 261` | doc comments referencing `Unknown` | **I** | Comment-only; no logic change, but doc text should note the new variant exists alongside `Unknown` in the discriminant space. |
| 9 | `alloc_core_core_diag.rs:297-300` | `SegmentKind::Primordial => 0, Small => 1, Large => 2, Unknown => 3` (numeric dbg encoding for a diagnostic surface) | **N** | Needs a 5th arm (`PageRun => 4`) — this is an EXHAUSTIVE match (no catch-all), so the compiler forces this site to be updated the moment the enum grows; cannot be silently skipped. |
| 10 | `alloc_core_small.rs:679, 931, 2178` | `SegmentKind::Small \| SegmentKind::Primordial` (various small-path checks — carve/refill inner logic, drain-related) | **N or I depending on exact site** — see §1.4 note | These are INSIDE the small-segment carve/refill/drain machinery itself; whether a page-run arena's own carve logic REUSES these functions (requiring the match arm to grow) or gets PARALLEL functions (requiring no change here, but a mirror file) is exactly §4's central design fork. §4.1 recommends parallel functions — under that recommendation these sites are **I** (untouched; the new kind never reaches them because page-run has its own carve module). |
| 11 | `alloc_core_small.rs:1550` | `if kind == SegmentKind::Primordial { .. } else { .. }` (computing `payload_start`: primordial has the registry after metadata, small does not) | **I** (under the parallel-carve recommendation) | Same reasoning as #10 — page-run's own carve module computes its OWN `payload_start` from its OWN layout constants (§4.1), never calling into this function. |
| 12 | `alloc_core_small_diag.rs:217, 306` | Small/Primordial checks, Primordial-specific dbg | **I** (parallel module) | Diagnostic mirrors of #10/#11; a page-run diagnostic surface is new, separate code (§6), not a change to these. |
| 13 | `alloc_core_small_magazine.rs:518` | `if kind == SegmentKind::Primordial { .. }` (magazine refill's `payload_start` computation, same shape as #11) | **I** (parallel module) | Magazine refill is a SMALL-segment-only optimization (per-thread tcache backed by the small free-list machinery). Whether page-run arenas get a magazine tier at all is a stage-2 tuning question (§5.2), not decided here; if NOT, this site is untouched by construction. |
| 14 | `alloc_core_small_pool.rs:97, 152, 511, 592` | `matches!(SegmentHeader::kind_at(base), SegmentKind::Small)` / `Small \| Primordial` (the M6 empty-segment hysteresis pool: admission, pop, decay) | **R (must explicitly exclude, at least for stage 2)** | §4.4 argues page-run arenas should NOT join the existing small-segment pool in stage 2 — the pool's `pool_next`/`pool_prev` intrusive links live in the STANDARD `SegmentHeader` at fixed offsets that assume a `SEGMENT`-sized reclaim unit; more importantly, pooling policy (how many empty page-run arenas to keep warm) is a SEPARATE tuning question from the small-segment pool's, and mixing them risks one policy's hysteresis starving the other. These four sites should gain an explicit `Small` (not `Small \| PageRun`) — i.e., stay exactly as they are today; a page-run arena is simply never admitted, which happens automatically as long as the pool admission check is `== Small` and page-run's `kind_at` never returns `Small`. **No code change needed here IF page-run stays outside the pool by construction** — but this row is flagged **R** because a careless future edit that widens these matches to include a new kind by pattern-matching habit (`Small \| Primordial \| PageRun`) would be the wrong default; the doc comment at each site should say so explicitly. |
| 15 | `alloc_core_small_reclaim.rs:100, 117, 230, 250, 454` | `matches!(kind, Small \| Primordial)` guard + `Primordial`-specific `payload_start` (the cross-thread reclaim guard chain — G1-class sites from R10-4 §3.2) | **R (reject at guard, or N with a parallel reclaim path)** | **This is the cross-thread correctness-sensitive surface** — the exact guard chain R10-4 spent its whole report on for the ALIGNMENT question. For page-run, the alignment invariant `off % block_size == 0` is UNCHANGED (§2.3: page-run does NOT change carve alignment, only arena size) — so IF a page-run arena's cross-thread free routes through this SAME reclaim function, the guard is still sound for it, PROVIDED `payload_start`/`bump` bounds are read from the RIGHT geometry. Given §4.1's parallel-carve-module recommendation, the SAFEST stage-2 choice is: the reclaim guard's `matches!(kind, Small \| Primordial)` check REJECTS a page-run `kind` (since page-run is a new discriminant, it fails this match automatically — no code change needed for the reject to happen; this is the SAME "grows the enum, old code that pattern-matches ONLY the old kinds correctly excludes the new one" property `Unknown` already established) and a SEPARATE reclaim function (mirroring this one) is written for page-run's own ring geometry. This is listed **R** at the EXISTING site (correctly, automatically, for free) and **N** for the NEW parallel function this task's stage 2 would add. |
| 16 | `bootstrap.rs:69-70, 247` | Comments + `hdr.kind = SegmentKind::Primordial` (bootstrap's primordial-segment construction) | **I** | The primordial segment itself is never a page-run arena; untouched. |
| 17 | `deferred_large/*.rs` (2 comment references) | Doc comments only | **I** | Comment-only. |
| 18 | `segment_directory.rs:323` | `matches!(SegmentHeader::kind_at(base), Small \| Primordial)` (directory rebuild: skip non-Small/Primordial, i.e. skip Large) | **R (must also skip page-run, for stage 2) — see §3.3** | The EXISTING flat directory (`class_nonempty_by_node`) is a per-`SMALL_CLASS_COUNT`, per-segment-table-slot bitmap. §3.3 concludes a page-run arena needs ITS OWN directory-equivalent (not necessarily a bitmap, given the tiny class count and block count involved), so this rebuild loop should CONTINUE to skip page-run arenas exactly as it does Large today — automatic, since `kind_at` returning `PageRun` already fails the `Small \| Primordial` match. No code change needed for the skip; flagged **R** to make the "this is deliberate, not an oversight" reasoning explicit for a future reader. |
| 19 | `segment_header.rs:620, 679` (`SegmentHeader::small`/`::large` constructors) | Sets `kind: SegmentKind::Small` / `::Large` | **N** | A THIRD constructor, `SegmentHeader::page_run(..)`, is needed, setting `kind: SegmentKind::PageRun` and whatever page-run-specific fields the header design (§4.1) adds. |
| 20 | `segment_header_layout.rs:180` (comment) | "`dec_live_and_maybe_decommit` guards on `SegmentKind::Small`" | **I** (see row 14) | Confirms decommit is ALSO `Small`-only today; a page-run arena's decommit policy (if any) is a stage-2 tuning question, deliberately deferred (§5.2/§8). |
| 21 | `segment_header_views.rs:22-56` (`kind_at`'s decode match) | `0 → Primordial, 1 → Small, 2 → Large, _ → Unknown` | **N** | The one-line addition described in §1.1: `3 => SegmentKind::PageRun`. |
| 22 | `heap_core_dealloc_batch.rs:59, 233` | `SegmentHeader::kind_at(base) == SegmentKind::Large` (batched dealloc's per-segment grouping: decides "does this segment need whole-span release" vs "batch into the small free list") | **N** | Same class of gap as row 1: batched dealloc needs a THIRD branch for page-run blocks, grouping them for a batched page-run-specific free, not the Large whole-span release. |
| 23 | `heap_core_free.rs:158` | `SegmentHeader::kind_at(base) == SegmentKind::Large` (registry-level free routing) | **N** | Same shape as row 1/22, at the registry layer instead of `alloc_core`. |
| 24 | `heap_core_xthread.rs:769` | `SegmentHeader::kind_at(base) == SegmentKind::Large` (cross-thread free routing: Large gets deferred-large-stack treatment, else gets `RemoteFreeRing`) | **R, with a THIRD path — see §4.3** | A page-run block's cross-thread free is NEITHER the Large deferred-stack protocol NOR (without further design) the EXISTING per-`SEGMENT`-sized `RemoteFreeRing`, because that ring's offset encoding assumes `off < SEGMENT` (§4.3 — this is a hard correctness constraint, not a convenience). A page-run arena needs its OWN ring sized for its OWN larger offset range, i.e. `off < N × SEGMENT`, which needs a wider offset field than the ring's current packed `u32` (`(off, class_idx)` — `off` already consumes up to 22 bits for `SEGMENT = 4 MiB`; `N × SEGMENT` for `N ∈ {2, 4}` needs 23–24 bits, which the EXISTING packing scheme has headroom for only if the class-index field shrinks, or a new packing is defined — see §4.3 for the concrete bit-budget check). |

### 1.3 Summary of the inventory

```text
Category N (needs new page-run-aware logic):  1, 2, 4, 6, 9, 15(new fn), 19, 22, 23, 24   (10 sites/new-fns)
Category I (safe no-op / automatically excluded by construction): 3, 5, 7, 8, 10, 11, 12, 13, 16, 17, 20   (11 sites)
Category R (must explicitly reject, or already rejects "for free"): 14, 15(existing), 18, 24   (4 sites, 1 overlaps N)
```

The **N-category count (10)** is the honest measure of "how much of the
substrate must learn about the new kind" — this is larger than R10-4's
4-guard-site inventory (that report's whole correctness surface was 4 sites;
this one touches roughly 2.5× as many call sites PLUS an entirely new ring
protocol, §4.3). This is the quantitative basis for §0's "closer to a second
subsystem" framing.

### 1.4 A note on the parallel-module recommendation (rows 10-13)

Rows 10-13 are marked **I** contingent on choosing PARALLEL carve/refill/
magazine functions for page-run over EXTENDING the existing small-segment
functions with more `match` arms. This recommendation is made here (not
deferred to §4) because it is the single design choice that determines
whether the N-count in §1.3 is 10 or closer to 20: extending
`carve_block`/`carve_batch`/`find_segment_with_free`/the magazine refill path
in place would require every one of THOSE functions to branch on arena size
(not just kind) at multiple points (page-map indexing, bitmap indexing,
bump-cursor bounds), multiplying the correctness surface inside
ALREADY-hot, already-audited functions. A parallel module
(`alloc_core_page_run.rs`, one file per CLAUDE.md's file-per-export
discipline, likely split further per the existing `alloc_core_small_*.rs`
convention) keeps the existing small-segment code path's byte-for-byte
behavior provably unchanged (the same "feature-OFF/kind-absent build is
byte-identical" discipline R9-4/R10-4 both required) and confines all new
geometry arithmetic to new, freshly-written, freshly-tested code. This is the
same trade-off R10-4 §4.3 made choosing "Oracle B: scoped to wide classes
only" over a change that touched the general guard.

---

## 2. Arena sizing — recomputed density, real constants

### 2.1 Real constants (re-read this session, not assumed from R10-4)

```text
SEGMENT              = 4,194,304 B  (src/alloc_core/os.rs:65, 1 << 22)
SEGMENT_SHIFT         = 22           (src/alloc_core/segment_table.rs:105)
MIN_BLOCK             = 16 B         (src/alloc_core/size_classes.rs:62)
small_meta_end()      = 73,728 B (72 KiB) — non-hardened default build
                        (page-aligned past header+page-map+bin-table+alloc-
                        bitmap+magazine-bitmap+remote-ring, all sized off the
                        GLOBAL SEGMENT/PAGE constants today — §4.1 explains
                        why a page-run arena's metadata region is NOT simply
                        this same 72 KiB)
```

Wide-medium class sizes, confirmed from `src/alloc_core/size_classes.rs:130-132`:

```text
1.25 MiB = 1,310,720 B
1.50 MiB = 1,572,864 B
1.75 MiB = 1,835,008 B
2.00 MiB = 2,097,152 B   (NOT currently a class — R9-4 explicitly left it out
                          as "out of scope," see size_classes.rs:39-41's doc
                          comment; this design treats "add the 2 MiB class"
                          as part of the SAME page-run-layer proposal, since
                          the whole point of the arena redesign is to make 2
                          MiB viable — see §2.3)
```

### 2.2 Density formula, re-applied to larger arena sizes

R9-4 §2.2's empirical density formula (confirmed by that report's own carve
test): for a class whose `block_size > small_meta_end`, the carve aligns the
first block to `block_size` itself (not to `small_meta_end`), so:

```text
density(arena_size, block_size) = floor(arena_size / block_size) - 1
```

This formula is UNCHANGED by making the arena bigger — the "－1" tax is a
structural property of block-size-aligned carving in ANY arena size, not a
4-MiB-specific artifact. A page-run arena does NOT change carve alignment
(§1.4's parallel-module recommendation keeps `align_up(bump, block_size)`
identical to today's small-segment carve — this design deliberately does NOT
combine with R10-4's `class_align` alignment change; see §2.4 for why they
are independent and non-competing optimizations, correcting R10-4 §9's
framing of them as alternatives).

**Own page-run metadata overhead is assumed equal to `small_meta_end` (72 KiB)
for this first-pass calculation** — see §4.1 for why the REAL page-run
metadata region is likely SMALLER (fewer classes to carry in its own
BinTable-equivalent) but this is not yet a finalized design, so 72 KiB is
used as a conservative (slightly pessimistic) placeholder; the density
numbers below would only IMPROVE with a smaller real metadata region.

### 2.3 Density table — recomputed for real arena size candidates

| Arena size | 1.25 MiB density | 1.5 MiB density | 1.75 MiB density | 2.0 MiB density |
|---|---:|---:|---:|---:|
| 4 MiB (today, `medium-classes-wide`) | 2 | 1 | 1 | 1 (not a class today — R9-4 §2.3 computed exactly this value, `floor(4Mi/2Mi)-1=1`, i.e. NO density win over Large's 1x, and explicitly excluded 2 MiB from the class table on that basis, not because the geometric density is zero) |
| 8 MiB (2×`SEGMENT`) | `floor(8388608/1310720)-1 = 5` | `floor(8388608/1572864)-1 = 4` | `floor(8388608/1835008)-1 = 3` | `floor(8388608/2097152)-1 = 3` |
| 16 MiB (4×`SEGMENT`) | `floor(16777216/1310720)-1 = 11` | `floor(16777216/1572864)-1 = 9` | `floor(16777216/1835008)-1 = 8` | `floor(16777216/2097152)-1 = 7` |
| 32 MiB (8×`SEGMENT`) | `floor(33554432/1310720)-1 = 24` | `floor(33554432/1572864)-1 = 20` | `floor(33554432/1835008)-1 = 17` | `floor(33554432/2097152)-1 = 15` |

Verification of the 8 MiB / 2.0 MiB cell (the one value that differs from
R10-4 §9.1's table, which only covered 1.25/1.5/1.75 MiB): `8,388,608 /
2,097,152 = 4` exactly (2 MiB divides 8 MiB evenly since both are powers of
two), so `floor(...) - 1 = 3`. This confirms 8 MiB is enough to make even the
previously-excluded 2 MiB class viable (density 3, vs today's 0/unavailable).

R10-4 §9.1's 8/16 MiB numbers for 1.25/1.5/1.75 MiB (5/4/3 and 11/9/8) are
CONFIRMED exact by this recomputation from the real constants — no
correction needed there. The one addition this report makes is the 2.0 MiB
row (R10-4 did not compute it because 2 MiB is not yet a class at all).

### 2.4 `class_align` (R10-4) and page-run arena sizing are independent,
### non-competing, and could in principle STACK

R10-4 §9 framed its own `class_align` alignment change and the page-run layer
as ALTERNATIVES ("the page-run layer is a strictly superior solution").
Re-examining this after actually designing the page-run layer: they solve
the SAME symptom (low density) via genuinely DIFFERENT, additive mechanisms
— `class_align` reduces the ALIGNMENT TAX inside a FIXED arena size;
page-run increases the ARENA SIZE, which reduces the tax's PROPORTION of the
total. Nothing about this design requires `class_align`'s alignment change,
and nothing about `class_align` requires a bigger arena. If BOTH were
adopted (not proposed here — out of scope), the page-run arena's own carve
would use `class_align` internally too, multiplying the density gain
further (e.g. 16 MiB + `class_align` for 1.25 MiB: `origin =
align_up(72Ki, 256Ki) = 262144`, `floor((16777216-262144)/1310720) = 12`, one
more than the 11 this report computes without `class_align`). This report
does not pursue that combination — it would compound §1's already-larger
correctness surface with R10-4's own oracle-redesign correctness surface,
and neither report's scope asked for the union. Flagged here only to correct
R10-4's own "superior solution, not both" framing with the more precise
"independent axes, stacking is possible but not evaluated."

### 2.5 Recommended sizing: ONE fixed arena size, 8 MiB, covering the whole
### 1.25–2.0 MiB range

**Recommendation: a single fixed page-run arena size of 8 MiB (2×`SEGMENT`),
NOT per-class-size-tier arenas, NOT the 16 MiB option.**

Reasoning:

- **8 MiB already delivers a real, multi-block density win for every class
  in the target range** (5/4/3/3 vs today's 2/1/1/1) — a 2.5×-to-3×
  density improvement depending on class, without needing the largest
  candidate size.
- **16 MiB's marginal gain over 8 MiB (11/9/8/7 vs 5/4/3/3, roughly 2× more
  blocks per arena) trades against 2× the per-arena commit cost** for a
  workload that only ever populates ONE or TWO blocks of a given class in a
  given arena — the exact over-commit concern R11-3 §2.6 flagged for its
  OWN (unrelated) Large-promotion design applies here too: a bigger arena
  is a bigger unit of commit, and this design has NOT measured real-world
  medium-classes-wide allocation VOLUME to justify optimizing for maximum
  density over minimum per-arena commit. Recommending the SMALLER of the
  two viable candidates is the conservative default absent that data.
- **A single arena size (not per-class-tier arenas) keeps §1's inventory
  and §3's addressing design to ONE new geometry, not four.** Per-class-tier
  arena sizing (e.g., a dedicated smaller arena for 1.25 MiB, a larger one
  for 2.0 MiB) would let each class hit ITS OWN density sweet spot, but
  quadruples the number of new constants, new metadata layouts, and new
  address-range checks this design must reason about, for a density gain
  that (per §2.3) is already "good enough" (3×+ density) at a single 8 MiB
  size across the whole range. This is the same "don't multiply
  configuration surface for a marginal gain" reasoning R9-4 recommendation
  #3 applied when it suggested dropping 1.5/1.75 MiB from the wide list
  rather than adding per-class tuning.
- **8 MiB = 2×`SEGMENT` keeps the arena a clean small multiple of `SEGMENT`,
  which is the property §3.2's addressing scheme needs** (a power-of-two
  multiple of `SEGMENT`, so the arena itself is also `SEGMENT`-aligned AND
  self-aligned at its own larger size). 16 MiB = 4×`SEGMENT` would work
  identically for §3.2's addressing scheme (also a power of two), so this is
  not a tie-breaker between 8 and 16 — the commit-cost argument above is.

### 2.6 Interaction with `MAX_SEGMENTS` / segment-table capacity math

`MAX_SEGMENTS = 1024` (`segment_table.rs:64`) bounds the NUMBER of
`SEGMENT`-sized table slots. An 8 MiB page-run arena, under §3.4's
recommended PARALLEL smaller table (not the existing `SegmentTable`), does
NOT consume a `SegmentTable` slot at all — it lives in its own table with
its own, much smaller capacity constant (§3.4 proposes `MAX_PAGE_RUN_ARENAS`,
sized far below 1024, since each arena hosts MANY blocks, unlike a Large
segment's one-allocation-per-slot). This is the key argument FOR the
parallel-table design over "let each 8 MiB arena occupy 2 SegmentTable slots
pointing at the same base" (the alternative considered and rejected in
§3.4): the existing `MAX_SEGMENTS` capacity budget is sized for
one-slot-per-distinct-allocation-unit (worst case: `MAX_SEGMENTS` live Large
allocations); a page-run arena hosting `~5` blocks does not want to spend 2
OF THAT BUDGET's precious 1024 slots on itself when a dedicated,
independently-sized table costs nothing from that budget.

---

## 3. Directory/table interaction — the crux

### 3.1 Restating the tension precisely

`segment_base_of_ptr` (`os.rs:95-122`) is:

```text
pub(crate) fn segment_base_of_ptr(ptr: *mut u8) -> *mut u8 {
    ptr.map_addr(|a| a & !(SEGMENT - 1))
}
```

This masks the low 22 bits (`SEGMENT_SHIFT`) unconditionally. For a pointer
INTO an 8 MiB page-run arena, this function returns the nearest
`SEGMENT`-aligned (4 MiB) boundary AT OR BELOW the pointer — which is either
the arena's TRUE base (if the pointer falls in the arena's first 4 MiB half)
or a point 4 MiB INTO the arena (if the pointer falls in the second half) —
NEITHER of which is reliably "the arena's base" without further work.

### 3.2 Can it stay compatible with the masking scheme? Yes, via a two-step
### resolution — NOT a zero-change compatibility

Because 8 MiB (and 16 MiB) are themselves powers of two AND multiples of
`SEGMENT`, a page-run arena reserved via `vmem::reserve_aligned(8 MiB,
SEGMENT)` (the SAME primitive `Segment::reserve` already calls internally
for oversized requests — `os.rs:150-152`, confirmed this session: `let
n_segments = len.div_ceil(SEGMENT); let usable = n_segments * SEGMENT; let
reservation = vmem::reserve_aligned(usable, SEGMENT)?;` — this exact
machinery ALREADY exists and is exercised today by every multi-segment Large
allocation) is guaranteed `SEGMENT`-aligned, but is NOT guaranteed
**8-MiB-aligned** unless reserved with `align = 8 MiB` explicitly. **The
recommended fix reserves page-run arenas with `vmem::reserve_aligned(8 MiB, 8
MiB)`** (align = the ARENA size, not `SEGMENT`) — this makes the arena base
ITSELF `8 MiB`-aligned, a strictly stronger property than `SEGMENT`-aligned.

Given THAT stronger alignment, the two-step resolution is:

```text
// SKETCH — illustrative only, NOT applied this session.
fn page_run_base_of_ptr(ptr: *mut u8) -> Option<*mut u8> {
    // Step 1: mask to the PAGE_RUN_ARENA_SIZE boundary (not SEGMENT).
    let candidate = ptr.map_addr(|a| a & !(PAGE_RUN_ARENA_SIZE - 1));
    // Step 2: the header at `candidate` must claim kind == PageRun and
    // magic == SEGMENT_MAGIC (the SAME sanity check every other kind uses).
    // A foreign / non-page-run pointer's masked candidate will fail this
    // check (kind_at returns something else, or the page is unmapped) —
    // exactly the existing `contains_base` foreign-pointer defence's shape.
    if SegmentHeader::kind_at(candidate) == SegmentKind::PageRun {
        Some(candidate)
    } else {
        None
    }
}
```

This is a SEPARATE masking function (a new `PAGE_RUN_ARENA_SIZE` constant,
distinct from `SEGMENT`), not a change to `segment_base_of_ptr` itself. The
crucial consequence: **every code path that resolves "which segment owns
this pointer" needs to know WHICH masking scheme to try.** For the
dealloc/realloc hot path (§3.5) this means: try `segment_base_of_ptr` (the
existing, `SEGMENT`-granularity mask) first as today; if `contains_base`
(the standard table) misses AND `page_run_base_of_ptr`'s candidate can be
computed, THEN check the page-run table. This is a real, measurable
BRANCH added to the dealloc hot path for every free — quantified in §5.1.

**This is the sense in which R10-4 §9's one-line claim ("the arena is still
internally block_size-aligned, just bigger... zero guard-invariant
changes") is TRUE for the CARVE alignment invariant (§2.2 confirms this) but
INCOMPLETE for the ADDRESS RESOLUTION invariant** — `segment_base_of_ptr`'s
O(1) masking is exactly calibrated to `SEGMENT`, and a bigger arena, even a
power-of-two one, needs a distinguishable SECOND masking constant and a
disambiguation step. This is the report's single most important
correction to R10-4 §9's framing.

### 3.3 What breaks for `SegmentDirectory` specifically

`SegmentDirectory::class_nonempty_by_node` (`segment_directory.rs:226`) is
indexed `[node_bucket][class_idx][word]` where `word = slot_idx / 64` and
`slot_idx` is a **`SegmentTable` slot index** (`0..MAX_SEGMENTS`). Two
concrete problems:

1. **A page-run arena, under the §3.4 parallel-table recommendation, has NO
   `SegmentTable` slot at all** — so it cannot be represented in THIS
   bitmap by construction (there is no `slot_idx` to set a bit at). This is
   not a bug to fix; it is the direct consequence of choosing a parallel
   table. The existing directory's rebuild loop (`segment_directory.rs:314-342`)
   already SKIPS non-Small/Primordial kinds (`Large` today; `PageRun`
   automatically once the new kind exists, per row 18 of §1.2's inventory)
   — so the existing directory is UNCHANGED and simply never learns about
   page-run arenas. Correct and free.
2. **A page-run arena needs its OWN, separate "which class has free blocks
   in which arena" index** if the free-list-miss fallback (walking every
   live page-run arena linearly) is judged too slow once there are more
   than a handful of arenas live. Given §2.6's point that page-run arenas
   are a MUCH smaller population than `SEGMENT`-sized segments (5-ish
   blocks each; a workload would need to be allocating enormous volumes of
   1.25-2 MiB objects to have many arenas live simultaneously — each 8 MiB
   arena holds ~3-5 blocks, so 100 live 1.5 MiB objects need ~25 arenas,
   NOT 100), the `DIRECTORY_MATERIALIZE_THRESHOLD = 32` precedent
   (`segment_directory.rs:92`, "below this count, linear scan is cheap
   enough") suggests a page-run directory may not be NEEDED at all for
   realistic populations — a linear scan over (likely single-digit to
   low-double-digit) live page-run arenas is probably fine without a
   dedicated bitmap. This is a stage-2 tuning decision, not resolved here;
   flagged as an explicit open question in §8.

### 3.4 What breaks for `SegmentTable`, and the parallel-table recommendation

`SegmentTable` (`segment_table.rs`) is a fixed `MAX_SEGMENTS = 1024`-slot
array of `*mut u8` bases, each slot representing exactly ONE `SEGMENT`-sized
(or, for Large, `N×SEGMENT`-sized but STILL single-allocation) span.
`bases()`/`base_at()`/`contains_base()`/`register()`/`unregister()`/
`recycle()` all key by this ONE-SLOT-PER-SPAN model, and `contains_base`'s
O(1) hash (`hash_index`) is keyed by `(base >> SEGMENT_SHIFT)` — i.e., it
ALREADY assumes a candidate base found via `segment_base_of_ptr`'s masking,
so it is naturally `SEGMENT`-granularity keyed.

**Two candidate designs, evaluated:**

**Design A — reuse `SegmentTable`, register a page-run arena at N
`SEGMENT`-granularity slots (one "true" slot + (N-1) "alias" slots pointing
at the same base, or at sentinel values meaning "interior of the arena
registered at slot X").** Rejected: this pollutes the `bases()`/`drop`
walk (which must now special-case "this is an alias slot, don't double-free
it"), consumes N of the precious 1024-slot budget per arena (§2.6), and
means `contains_base`'s hash keying (by `base >> SEGMENT_SHIFT`) needs EVERY
alias address registered too (so a pointer anywhere in the arena's 8 MiB
resolves via `segment_base_of_ptr`'s existing granularity) — multiplying the
correctness surface of an already-heavily-audited hash/backward-shift-delete
structure (`segment_table.rs:713-827`'s extensive correctness commentary)
for a case its original design never anticipated.

**Design B (RECOMMENDED) — a separate, small, parallel `PageRunTable`,
structurally identical in SHAPE to `SegmentTable` (fixed array + O(1) hash +
free-list-of-recycled-slots, the same self-hosted-in-primordial-segment
discipline) but sized for a MUCH smaller capacity** (e.g.
`MAX_PAGE_RUN_ARENAS = 64` — generously above the "25 arenas for 100 live
1.5 MiB objects" estimate in §3.3, while costing only `64 * 8 = 512 B` for
the slots array, negligible next to `SegmentTable`'s existing `1024 * 8 = 8
KiB`), **keyed by `(base >> PAGE_RUN_ARENA_SHIFT)` instead of
`SEGMENT_SHIFT`** — a distinct hash space, no collision risk with the
existing table, no consumption of the `MAX_SEGMENTS` budget. Dealloc/realloc
routing (§3.5) tries `SegmentTable::contains_base` first (today's path,
UNCHANGED — the common case, zero added cost for non-page-run frees once
the branch is structured as "miss falls through," see §5.1), and ONLY on a
miss tries `PageRunTable::contains_base` using the SEPARATE
`page_run_base_of_ptr` masking from §3.2. This is a strictly ADDITIVE
change: the existing `SegmentTable`'s hash, capacity, and correctness
properties are completely undisturbed (the same "byte-identical
feature-OFF build" discipline every prior R9/R10/R11 code-change task
required).

### 3.5 Cost of the two-table dealloc/realloc routing (qualitative here;
### quantified in §5.1)

Every dealloc/realloc call that is NOT for a page-run block pays exactly one
extra branch: `if !self.table.contains_base(base) { try page_run_base_of_ptr
+ page_run_table.contains_base }`. Because `contains_base` is already O(1)
(hash + own-cache fast path, `segment_table.rs:454-468`), and the `else`
branch (the page-run attempt) is only reached on the EXISTING miss path
(today: "foreign pointer, no-op" — `alloc_core.rs:1052-1063`), this is a
**zero-cost addition on the hit path** (the overwhelming majority of frees:
Small/Primordial/Large, all hit `SegmentTable::contains_base` on the first
try) and a **small added cost only on what is TODAY the "foreign pointer"
slow path** (rare — a genuine foreign pointer, or now also a page-run
pointer). This is a favorable cost profile: the hot path is untouched.

---

## 4. Interaction with every prior-session mechanism assuming segment-uniformity

Mirroring R10-4 §4's interaction-table format.

| # | Mechanism | Reused as-is / Needs adaptation / Needs parallel mechanism | Reasoning |
|---|---|---|---|
| 1 | **M2 alloc-bitmap** (`AllocBitmap`, double-free guard, `FOOTPRINT = SEGMENT / MIN_BLOCK / 8`) | **Needs parallel mechanism** | The footprint constant is derived from the GLOBAL `SEGMENT`, not "this segment's size" — reusing it unchanged for an 8 MiB arena would allocate a bitmap sized for only the FIRST 4 MiB (silently under-covering the second half — an OOB / undersized-bitmap bug if used naively) OR (if the type were parameterized) require threading an arena-size parameter through `SegmentBitmap`'s currently-`const` FOOTPRINT, which is used in const-context offset arithmetic elsewhere (`segment_header_layout.rs`) — not a trivial parameterization. **Recommendation:** page-run gets its OWN bitmap type (or a `SegmentBitmap`-family sibling constructed with an arena-specific `FOOTPRINT = PAGE_RUN_ARENA_SIZE / MIN_BLOCK / 8`), living in the page-run arena's own metadata region (§4.1). |
| 2 | **M2 magazine-residency bitmap** (`MagazineBitmap`) | **Needs parallel mechanism** — same reasoning as row 1 | Whether page-run blocks even GET a magazine tier at all (row 13 of §1.2) is undecided; if yes, it needs the same arena-sized-footprint treatment as `AllocBitmap`. |
| 3 | **`PageMap`** (per-page "first class wins" descriptor, `FOOTPRINT = PAGES_PER_SEGMENT * 1 = SEGMENT / PAGE`) | **Needs parallel mechanism, OR can be omitted entirely** | Same footprint-constant problem as rows 1-2. BUT: `PageMap` is explicitly documented as "NOT load-bearing for class routing" (`segment_header.rs:736-741` — "do NOT derive block classes from it... No production dealloc path derives a freed block's class from PageMap"). Given a page-run arena hosts at most ~5 blocks total (§2.3), a full page-granularity map (2048 entries for an 8 MiB arena at 4 KiB pages) is disproportionate bookkeeping for so few blocks. **Recommendation: page-run arenas SKIP PageMap entirely** — it is diagnostic-only substrate elsewhere and can stay that way, or be omitted, for page-run (an explicit scope reduction vs. the standard small-segment layout, justified by the tiny block count). |
| 4 | **`RemoteFreeRing` / cross-thread free** | **Needs a parallel mechanism with a wider offset field** — see §4.3 below | The ring's packed `u32` entry (`pack_entry(off, class_idx)`, `remote_free_ring.rs:180,261`) documents `off < SEGMENT` as the reason `u32::MAX` is an unambiguous sentinel. An 8 MiB arena's offsets range up to `8,388,608`, needing 23 bits (vs `SEGMENT`'s 22) — see §4.3 for the exact bit-budget accounting. |
| 5 | **`HeapOverflow`** (the per-heap MPSC overflow sidecar) | **Needs adaptation** | `heap_overflow.rs:324-325` documents relying on "every segment is a `SEGMENT`-aligned OS reservation... so a real base's low 22 bits are all zero" as part of its packed-word sentinel-safety argument (the SAME class of assumption as the ring). A page-run arena's base is `PAGE_RUN_ARENA_SIZE`-aligned (stronger than `SEGMENT`-aligned per §3.2), so its low 22 bits ARE still zero (an 8/16/32 MiB alignment is a superset of 4 MiB alignment) — this specific invariant actually HOLDS for page-run bases without change. What needs adaptation is the PAYLOAD `(offset, class)` word this sidecar carries, which is the SAME `RemoteFreeRing`-packed word from row 4 — so `HeapOverflow`'s adaptation is entirely INHERITED from whatever row 4 decides, not an independent design point. |
| 6 | **R8-1/R8-2/R9-8 segment directory machinery** (`SegmentDirectory`, materialize threshold, per-class miss-streak rescan) | **Reused as-is (by not participating)** | §3.3 already covers this: the existing directory's rebuild loop skips non-Small/Primordial kinds automatically; page-run needs, at most, its OWN much-simpler directory-equivalent (open question, §3.3 point 2) but the EXISTING one is provably unaffected. |
| 7 | **R10-6/R11-6 NUMA node-indexed directory** (`class_nonempty_by_node`, `node_bucket`) | **Needs adaptation IF page-run wants NUMA routing, else reused as-is (by not participating)** | Same "automatically skipped" argument as row 6 for the case page-run does NOT get NUMA-aware placement in stage 2. IF a future stage wants NUMA-local page-run arenas (arguably MORE important for 1.25-2 MiB objects than for tiny ones, since they are large enough to matter for locality), the arena's header would need a `node_id` field (row in §4.1's header design) and reservation would need to route through the SAME `numa::alloc_on_node`-family calls `reserve_small_segment` uses under `numa-aware` — mechanically straightforward ADAPTATION (copy the pattern), not a new invention, but explicitly deferred to stage 2 tuning (§5.2) since it compounds an already-large design with a second opt-in feature's interaction. |
| 8 | **`alloc-decommit`'s large-cache** (`alloc_core_large_cache.rs`, `LARGE_CACHE_SLOTS`, decay/eviction) | **Not applicable / explicitly excluded** | The large-cache caches WHOLE Large segments (one-allocation-per-segment) keyed by size-fit. A page-run arena is Small-shaped (many blocks, BinTable-style), not Large-shaped — it does not fit this cache's model at all. Confirmed by row 1/4/22/23 of §1.2: page-run must NOT fall into any `kind == Large` branch, and the large-cache is exclusively reached from those branches. |
| 9 | **`alloc-decommit`'s empty-segment pool/release machinery** (`alloc_core_small_pool.rs`, `pool_next`/`pool_prev`, `release_or_pool_empty_segment`) | **Needs a parallel mechanism, explicitly NOT the existing pool — see §4.4** | Row 14 of §1.2 already flags this: the existing pool's admission check is `kind == Small` exactly (not `Small \| Primordial`, and would need to explicitly NOT become `Small \| PageRun` either). §4.4 elaborates why page-run needs its OWN, separately-tuned empty-arena policy rather than sharing the small-segment pool's hysteresis knobs. |
| 10 | **M2 double-free guard's `is_free`/`mark_free`/`mark_alloc` semantics** (the CONCEPT, not the `AllocBitmap` TYPE — already covered in row 1) | **Concept reused, mechanism parallel** | The semantic discipline ("owner-only, single-writer, bit=1 means free") transfers unchanged to a page-run-sized sibling bitmap — only the FOOTPRINT constant and the type differ (row 1). |
| 11 | **`SegmentHeader`'s owner-state / cross-thread-free-routing fields** (`owner_thread_free`, `owner_state`, `magic`) | **Reused as-is, same struct** | A page-run arena's header is a NEW struct/layout for ITS OWN metadata (§4.1), but there is no reason it cannot embed the SAME `owner_thread_free: *const AtomicPtr<u8>` / `owner_state: u64` / `magic: u32` fields at the START of its own header, in the same physical shape — cross-thread ownership resolution (`owner_state_atomic`, `magic_at`, the whole R6-MS-5 atomic-field-read discipline) is ORTHOGONAL to segment SIZE; it is about WHO owns this span and IS it really ours, both of which apply identically to a bigger arena. Recommended: the page-run header's hot set intentionally MIRRORS `SegmentHeader`'s field layout for these specific fields, so the field-specific accessors (`kind_at`, `magic_at`, `owner_thread_free_at`, `owner_state_atomic`) can be REUSED VERBATIM (same `offset_of!`-based accessors work on any `#[repr(C)]` struct with `kind`/`magic`/`owner_thread_free`/`owner_state` at the SAME relative offsets) rather than duplicated. This is the ONE piece of the existing substrate that transfers with genuinely ZERO new code, if the page-run header is deliberately designed to share this field prefix. |

### Summary of §4's table

```text
Reused as-is (rows 6, 11):                                    2 / 11
Reused as-is by NOT participating / automatic exclusion (rows 6, 7, 8): counted above + 8 = 3 unique
Needs adaptation (rows 5, 7-conditional):                     1-2 / 11
Needs a parallel mechanism (rows 1, 2, 3, 4, 9, 10):           6 / 11
```

Six of eleven mechanisms need genuinely NEW, parallel code. This is the
quantitative confirmation of §0's "second subsystem" framing.

---

## 5. `RemoteFreeRing` bit-budget — the one place a wider offset genuinely
## does not fit today's packing without a format change

### 5.1 The exact bit accounting

`pack_entry` (`remote_free_ring.rs:251-267`, the non-hardened packing):
packs `(off: u32, class_idx: u32)` into one `u32`. `SEGMENT = 4 MiB` needs
22 bits for `off` (`0..4,194,304`); `SMALL_CLASS_COUNT` (58 under
`medium-classes-wide`) needs 6 bits for `class_idx` (`0..64`) — 22 + 6 = 28
bits, with 4 bits of headroom in the `u32` (matching R9-4 §"Conditions"
item 1's note that the hardened ring's class field has "4 headroom values
left" after growing to 58 classes — the SAME headroom this design would
also consume, from a different angle).

An 8 MiB page-run arena's offsets need `log2(8,388,608) = 23` bits — **one
MORE bit than the non-hardened ring's CURRENT 22-bit off field uses**, but
the combined `23 + 6 = 29` bits still fits comfortably inside a `u32` (3
bits of headroom remain, vs today's 4 — the non-hardened ring format has
room). The **hardened** ring's `pack_entry_hardened` (`remote_free_ring.rs:433-452`)
packs `(gen: u8, class_idx, off)` and additionally asserts
`off.is_multiple_of(MIN_BLOCK)` and reduces `off` to `off16 = off /
MIN_BLOCK` internally (per the doc at line 419-431) — the exact bit budget
here needs a closer read than this report's scope requires to state a firm
number, but the STRUCTURAL point stands: this is a FORMAT change to the ring
entry encoding, which is either (a) a NEW, separate ring TYPE for page-run
arenas with its own wider packing (recommended — mirrors the whole
"parallel mechanism" pattern this design keeps landing on) or (b) a
delicate reduction of the existing ring's bit budget for ALL classes
(rejected — touches the correctness-audited existing ring for every
non-page-run class too, the opposite of the "existing behavior byte-
identical" discipline this design otherwise maintains everywhere else).

**Recommendation: a page-run arena gets its own `PageRunFreeRing` type**,
structurally similar to `RemoteFreeRing` (same MPSC CAS-reserve protocol,
same cursor-block cache-line layout) but with a WIDER packed-offset field
(23-24 bits, since even a future 16/32 MiB arena stays under 25 bits) sized
for its own arena, and a NARROWER class field (page-run only ever needs to
encode the ~4-5 wide-medium classes it hosts, needing 3 bits, not 6) —
trading unused class-field headroom for the extra offset bit the bigger
arena needs. This is a NEW, small, self-contained module — not a change to
the existing `remote_free_ring.rs`.

### 5.2 Perf overhead of the new dealloc-routing branch (§3.5), quantified

The added branch (`SegmentTable::contains_base` miss → try
`page_run_base_of_ptr` → `PageRunTable::contains_base`) costs, on the
EXISTING today's-foreign-pointer-or-hit-elsewhere path: one additional
pointer mask (`&`, ~1 cycle) + one additional O(1) hash probe
(`PageRunTable::contains_base`, same shape as the existing
`SegmentTable::contains_base` — dominated by an L1/L2 cache-line load, ~4-10
cycles) if the FIRST table misses. For the overwhelming majority of frees
(anything Small/Primordial/Large), the first `contains_base` call HITS and
this new branch is never reached — **zero added cost on the dominant hot
path**, matching §3.5's qualitative claim with a cycle-level estimate. This
is an ANALYTICAL estimate (mirroring R10-4 §5's own analytical-not-measured
convention for a design-stage report — no wall-clock measurement is
performed or claimed here).

---

## 6. Feature gating

### 6.1 Recommendation: a NEW feature, `medium-page-run`, requiring
### `medium-classes-wide`

Unlike R11-3's decision (gate behind the EXISTING `medium-classes` feature,
because that design reused existing Large-segment machinery with ZERO new
metadata/invariants), THIS design's own §1/§4 findings — a new `SegmentKind`,
a new parallel segment table, a new parallel free ring, new bitmaps, new
carve/refill logic — are the OPPOSITE profile: substantial new metadata, a
new correctness surface touching the cross-thread free path (§4.3/§5.1),
and (§2.3) a plausible EXTENSION of the class list itself (adding the
previously-out-of-scope 2.0 MiB class). This is structurally the SAME
profile R10-4 §8.2 used to justify ITS OWN new dedicated feature
(`wide-class-align`) over folding into `medium-classes-wide` directly: "the
reclaim path is the §13 corruption defence; a wrong design here risks
metadata corruption in the cross-thread free path" — R10-4's own reasoning
for needing a distinct opt-in gate applies here at least as strongly, since
this design touches MORE of that surface (§1.2's 10 N-category sites vs
R10-4's 4 guard sites).

`medium-page-run = ["medium-classes-wide"]` (requires, does not merely
imply-through — the page-run layer only makes sense once the wide classes
exist to route into it) — additive, NOT part of `production` or any default
bundle, matching every precedent (`medium-classes`, `medium-classes-wide`,
the hypothetical `wide-class-align`). `--all-features` would pull it in for
the CI feature-matrix, so its correctness is continuously checked even
though it ships opt-in.

### 6.2 Whether page-run REPLACES or SUPPLEMENTS the existing fixed
### 1.25/1.5/1.75 MiB classes under `medium-classes-wide`

**Recommendation: supplements, does not replace, and is a SEPARATE decision
axis from `medium-classes-wide` itself.** A user who wants the wide classes
but NOT the page-run layer's extra metadata/correctness-surface cost keeps
today's behavior exactly (2/1/1 density, `medium-classes-wide` alone). A
user who ALSO enables `medium-page-run` gets the SAME class boundaries
(1.25/1.5/1.75, plus the newly-viable 2.0 MiB) but requests in that range
route to a page-run arena instead of a standard 4 MiB segment — this is a
ROUTING change (which arena kind serves the class), not a class-table
change, so `SMALL_CLASS_COUNT`/`SIZE_CLASS_TABLE` stay EXACTLY as
`medium-classes-wide` already defines them (no change to `size_classes.rs`'s
existing table beyond, potentially, ADDING the 2.0 MiB entry — itself a
`medium-page-run`-gated addition, since a 2.0 MiB class is pointless
without page-run to back it, per R9-4 §2.3's explicit "2 MiB out of scope"
finding, whose reasoning (`floor(4Mi/2Mi)-1 = 1`, i.e. NO density win in a
4 MiB segment) is EXACTLY resolved by page-run and nothing else).

---

## 7. Kill-gate / verdict

| # | Criterion | Target | Finding (this report) | Verdict |
|---|---|---|---|---|
| K1 | Is the density gain real and recomputed from actual constants (not repeated uncritically from R10-4's illustrative table)? | exact arithmetic from real `SEGMENT`/class-size constants | §2.3: 8 MiB → 5/4/3/3, 16 MiB → 11/9/8/7 (1.25/1.5/1.75/2.0 MiB). R10-4's 1.25-1.75 MiB numbers confirmed exact; the 2.0 MiB row is new (R10-4 never computed it). | **PASS** |
| K2 | Is every `SegmentKind::` call site inventoried and classified? | exhaustive grep, every site classified N/I/R | §1.2: 44 raw matches across 17 files, grouped into 24 distinct call-site rows, each classified. 10 need new logic, 11 are automatically safe, 4 must explicitly (or automatically) reject. | **PASS** |
| K3 | Is `segment_base_of_ptr`'s O(1) masking compatible with a bigger arena? | state honestly, don't hand-wave | §3.2: compatible via a SEPARATE masking constant + a two-step disambiguation (mask, then verify `kind_at`), NOT zero-change compatibility as R10-4 §9's one-liner implied. This is stated as a correction to that framing, not a discovery that breaks the design. | **PASS (with correction)** |
| K4 | Does the segment directory / segment table interaction have a concrete, workable resolution? | not hand-waved; a specific design chosen with reasoning | §3.3/§3.4: existing directory/table are UNTOUCHED (page-run arenas are automatically excluded by kind); a NEW parallel `PageRunTable` (Design B) is recommended over aliasing into the existing table (Design A, explicitly rejected with reasoning). | **PASS** |
| K5 | Is the interaction with every prior-session segment-uniformity mechanism (M2 bitmaps, RemoteFreeRing/HeapOverflow, directory, NUMA directory, large-cache, pool) systematically checked? | R10-4 §4-style interaction table, every row reasoned | §4: 11-row table. 2 reused as-is, up to 2 need adaptation, 6 need a parallel mechanism, plus explicit "not applicable" for the large-cache. | **PASS** |
| K6 | Is the cross-thread free ring's offset-encoding constraint (the hardest correctness-adjacent finding) surfaced and resolved? | concrete bit-budget check, not assumed to "just work" | §5.1: 8 MiB needs 23 offset bits vs `SEGMENT`'s 22; the non-hardened ring's `u32` packing has 3 bits of headroom left after accounting for `class_idx` — fits, but is TIGHT, and the hardened ring's packing needs its own closer read (explicitly flagged as not fully resolved in this report). A dedicated `PageRunFreeRing` type is recommended over touching the existing ring's format. | **PASS (with one flagged incompleteness — the hardened-ring bit budget)** |
| K7 | Is the feature-gating decision justified against BOTH precedents (R10-4's new-gate reasoning AND R11-3's reused-gate reasoning), not just picked by pattern-matching the most recent one? | explicit comparison | §6.1: this design's correctness-surface profile (new SegmentKind, new tables, new ring format) matches R10-4's "needs its own gate" profile, not R11-3's "reuses existing machinery, folds into existing gate" profile — the comparison is made explicitly, not just asserted. | **PASS** |
| K8 | Is the true size of the design surface stated honestly (not undersold as "just a bigger segment")? | explicit, not spun | §0/§1.3/§4 summary: 10 of 24 call-site rows need new logic; 6 of 11 cross-cutting mechanisms need a parallel mechanism. Stated plainly as "closer to a second subsystem" rather than minimized. | **PASS** |
| K9 | Are open questions that this report deliberately does NOT resolve stated explicitly, not silently assumed? | explicit "not decided here" callouts | §3.3 point 2 (does page-run need its own directory), §4 row 7 (NUMA-aware page-run), §4 row 9 / §4.4 (empty-arena pool policy), §5.1 (hardened-ring exact bit accounting), §6.2 (2.0 MiB class addition timing) — five explicit open questions, each flagged where it arises. | **PASS** |

### Verdict

**CONDITIONAL GO — ready for a future stage-2 prototype session, but the
stage-2 estimate should be scoped as SUBSTANTIALLY LARGER than R10-4's or
R11-3's own stage-2 estimates, not as a small bounded patch.**

The density win is real, re-verified against actual constants, and larger
than the alternative (`class_align`) this session's OTHER design doc
explored. The addressing/directory question (§3), initially flagged in the
task brief as "likely the single hardest open question... work through it
explicitly," IS tractable — §3.2's two-step masking and §3.4's parallel
`PageRunTable` are concrete, implementable designs, not hand-waves. This is
the basis for GO rather than NO-GO.

The CONDITIONAL qualifier, and the explicit warning against under-scoping
stage 2, rests on §0/§1.3/§4's quantitative finding: **six of eleven
cross-cutting mechanisms need genuinely new, parallel code** (a new
per-arena bitmap pair, a new free ring with a wider offset field, a new
segment table, new carve/refill/free logic, a new empty-arena pool policy),
and **ten of twenty-four `SegmentKind`-dispatch call sites need new logic**.
This is roughly 2-3× the correctness-surface size of R10-4's alignment-oracle
design (which itself only reached CONDITIONAL GO) and R11-3's realloc-
promotion design (which reused existing Large-segment machinery with ZERO
new metadata). A stage-2 session that budgets "a bounded patch, a few
hundred lines" against this design will under-deliver; the honest estimate,
based on the parallel-module count above (a new `SegmentKind` variant, a new
`PageRunTable`, a new `PageRunFreeRing`, new per-arena bitmaps, new carve/
refill logic, new dealloc/realloc routing branches, plus the test suite
each of these needs under this project's "every phase delivered with
tests" rule) is **closer to a multi-phase mini-project inside stage 2** than
a single-session prototype — comparable in total scope to the ORIGINAL
`medium-classes`/`medium-classes-wide` substrate build-out (R6/R9-4
combined), not to a single R10/R11-class task.

---

## 8. What stage 2 would need to resolve BEFORE writing code (for a future
## session's own design-review step, not resolved here)

1. **The page-run directory question (§3.3 point 2):** measure or reason
   more precisely about REALISTIC live-arena counts for target workloads
   before deciding whether a dedicated directory bitmap is needed, or a
   linear scan over live page-run arenas (likely single/low-double-digit
   count) suffices — mirroring the `DIRECTORY_MATERIALIZE_THRESHOLD = 32`
   precedent's own data-driven threshold choice (`R7_DIRECTORY_BASELINE.md`).
2. **The hardened-ring bit-budget** (§5.1) needs a full, careful read of
   `pack_entry_hardened`'s exact field-width accounting (the `off16`
   reduction, the `gen` field width) before committing to a specific
   `PageRunFreeRing` packing scheme under `hardened + medium-page-run`.
3. **Whether page-run arenas get a magazine tier at all** (§1.2 row 13,
   §4 row 2) — a real tuning/complexity trade-off, not yet resolved.
4. **The empty-arena pool/decommit policy** (§4 row 9, §4.4) — needs its
   own hysteresis tuning, independent of the existing small-segment pool's
   `pool_cap`/decay knobs, calibrated to the very different population size
   (single-digit live arenas, not hundreds of small segments).
5. **Whether the 2.0 MiB class ships alongside page-run** (§6.2) — logically
   clean to bundle (page-run is the ONLY mechanism that makes 2.0 MiB
   viable at all, per R9-4's own finding), but is an independent
   size-class-table change that could, in principle, be deferred to a
   follow-up if stage 2's own scope needs trimming.
6. **A pad-target / arena-reuse-across-sizes question this report did NOT
   examine:** once an 8 MiB arena has hosted, say, three 1.25 MiB blocks and
   they are all freed, does the arena's remaining capacity get reused for a
   DIFFERENT wide-medium class (mixed-class page-run arenas, mirroring the
   standard small segment's "mixed-class, shared bump cursor" model per
   `segment_header.rs:181-191`), or is each page-run arena single-class
   (simpler bookkeeping, more fragmentation risk if class demand is
   uneven)? This report's §2 density arithmetic implicitly assumes
   single-class arenas (matching R9-4's own density methodology for the
   standard segment); stage 2 should decide this explicitly rather than
   inherit the assumption silently.

None of these six questions change the §7 verdict — they are the natural
next layer of design work a CONDITIONAL GO defers to the stage that has
license to write code and can validate assumptions empirically (density
measurement, directory-population measurement) the way R9-4's own carve
tests did for the original wide-class prototype.

---

## 9. Caveats

- **No code was written or run this session.** Every density number in §2
  is deterministic geometry (`floor(arena_size / block_size) - 1`), computed
  by hand from real constants read from `src/alloc_core/os.rs`,
  `src/alloc_core/size_classes.rs`, and `src/alloc_core/segment_table.rs`
  this session — not estimated, but also not empirically confirmed by a
  carve test (that confirmation is stage 2's job, mirroring R9-4 §5's own
  test suite for the ORIGINAL wide-class density claim).
- **The bit-budget check in §5.1 is the least complete part of this
  report.** The non-hardened ring's headroom (3 bits after an 8 MiB arena)
  is confirmed arithmetically; the HARDENED ring's exact accounting was
  read at a summary level (`remote_free_ring.rs:419-452`'s doc comments)
  but not derived bit-by-bit the way the non-hardened case was — flagged
  explicitly in §8 point 2 as unresolved, not silently assumed to work.
- **No wall-clock measurement was performed or is claimed.** §5.2's
  dealloc-routing-branch cost estimate is analytical (instruction-count
  reasoning), mirroring R10-4 §5's own analytical-estimate convention for a
  design-stage report — this is consistent with this project's practice of
  NOT measuring at the design stage, only at the prototype/validation stage.
- **This report deliberately does NOT combine with R10-4's `class_align`
  alignment change** (§2.4 explains why they are independent, not
  competing, axes) — evaluating the STACKED combination is out of scope for
  both reports.
- **No `src/`, `Cargo.toml`, or `tests/` file is modified.** This is a
  documentation-only deliverable, confirmed by construction (every code
  block in this report is fenced and marked SKETCH / illustrative, never
  applied).
- **Single analysis session, single host, no independent second-reader
  verification of the source-reading this report is based on** beyond the
  author's own re-reading of every cited file this session (not carried
  over from R10-4/R9-4's own citations without re-verification — every
  `SegmentKind::` call site in §1.2, every constant in §2.1, and the
  `Segment::reserve`/`vmem::reserve_aligned` machinery cited in §3.2 were
  freshly greped/read this session, not assumed from the precedent
  reports' own citations).
