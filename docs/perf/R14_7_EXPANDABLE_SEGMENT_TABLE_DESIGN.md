# R14-7 — Expandable/chained `SegmentTable` beyond the raised `MAX_SEGMENTS`: DESIGN-ONLY (no code change)

**Task:** #292 (R14-7, P1/P2) — R13-8
(`docs/perf/R13_8_MEDIUM_WORKING_SET_JUDGE.md`) found a 100%-reproducible
capacity cliff: every Large allocation consumes exactly one `SegmentTable`
slot (`src/alloc_core/segment_table.rs`), independent of feature
combination and independent of `alloc-decommit` (whose free-list recycle
only helps once an object is *freed* — a static live working set never
benefits from it). With `MAX_SEGMENTS = 1024`, the usable ceiling for
simultaneously-live Large objects was exactly 1023. This task's brief asked
for three things in order: (1) document the limitation in the README now
— done, see the `## Honest limitations` section; (2) measure whether a
simple `MAX_SEGMENTS` raise is cheap; (3) raise it if cheap, otherwise write
this design doc instead. **Outcome: BOTH.** The raise (1024 → 4096, 4×) was
measured cheap on every axis this session could check (§1) and has been
landed in the same round — **this document is not a substitute for the
raise; it is the deliberately-deferred follow-on** for the residual
question the raise does not close: what happens when a workload needs more
than `MAX_SEGMENTS - 1` (now 4095) simultaneously-live Large objects. No
`src/`, `Cargo.toml`, or `tests/*.rs` behavior is added by this document
beyond the raise itself and its accompanying density-agnostic test
(`tests/r14_7_max_segments_ceiling.rs`); this doc's own proposal is
DESIGN-ONLY.

**Date:** 2026-07-23.
**Base revision:** `main` @ `a3434df` (R13-1..R13-11 landed; this session's
`git status` shows R14-1..R14-6 already staged/committed ahead of this
task in the same round).
**Platform:** Windows 10 Pro x86-64 (measurement host, same as R13-8's).

---

## 0. TL;DR

The `MAX_SEGMENTS` raise (1024 → 4096) landed this round buys **4× more
headroom for free** — idle-process RSS is statistically unchanged (2996–3308
KiB range across repeated runs, no directional shift attributable to the
larger table), the primordial segment's metadata footprint grows by only
~84 KiB inside a large, mostly-idle ~3.9 MiB fixed budget, and no scan path
degrades non-linearly (§1). That closes the *specific* 1023-object wall
R13-8 measured for a good while. **This document is about what comes after
that headroom is also exhausted** — a workload needing tens of thousands of
simultaneously-live Large objects, or an even larger raise that starts to
threaten the primordial segment's fixed 4 MiB budget. The recommended
future direction is a **two-level chained table** (§3): keep the existing
flat array/hash/free-list scheme as level 0 (capacity `MAX_SEGMENTS`,
unchanged addressing, zero regression risk for the common case), and add
level-1 **extension blocks** — each a small, separately-OS-reserved array
of slots, linked from a fixed-size pointer list living in the primordial
segment — allocated lazily only when level 0 fills. Lookup stays O(1)
amortized (few branches to find the right level, then direct index) and the
existing `segment_of(ptr) = ptr & ~(SEGMENT-1)` O(1) owner resolution is
completely unaffected (the table is a census structure, not on the address
resolution hot path). **This is explicitly evaluated jointly with the
page-run question per the fx-review note this task's brief cites**
(`docs/reviews/2026-07-23-r14-reviews-synthesis.md` §4, point on #292): §4
below compares the two capability axes — capacity ceiling AND the cold-carve
gap (2.4–2.7× behind `mimalloc` at 16 B/256 B) — because a page-run-style
arena layer would help BOTH while an expandable table only helps the first,
and the right investment depends on which axis a real future workload
actually needs.

---

## 1. Why the `MAX_SEGMENTS` raise (1024 → 4096) was judged cheap — the
## numbers behind the decision this doc follows on from

Measured directly this session (not estimated), reproducing R13-8's own
harness (`examples/r13_8_medium_working_set_judge.rs`) unchanged except for
the `MAX_SEGMENTS` constant:

### 1.1 Static / layout cost

The registry array, open-addressing hash table (`HASH_CAPACITY = 2 ×
MAX_SEGMENTS`), and free-list index stack are all carved from a
**fixed 4 MiB primordial segment** (`SEGMENT = 1 << 22`,
`src/alloc_core/os.rs:65`), guarded by a compile-time assert
(`Layout::primordial_meta_end() + PAGE <= SEGMENT`,
`src/alloc_core/segment_header.rs:1250`). Measured directly via
`SegmentLayout::PRIMORDIAL_META_END`:

| `MAX_SEGMENTS` | registry+hash+free-list footprint | `PRIMORDIAL_META_END` | headroom to `SEGMENT` (4096 KiB) |
|---:|---:|---:|---:|
| 1024 (old) | 32 KiB | 104 KiB | 3992 KiB |
| 4096 (new) | 112 KiB | 188 KiB | 3908 KiB |

Even under `--all-features` (`hardened`'s generation table adds ~256 KiB to
the primordial metadata region on top of the registry/hash/free-list),
headroom at `MAX_SEGMENTS = 4096` is still 3736 KiB. The +84 KiB the raise
costs is under 3% of the fixed 4 MiB budget in the worst measured case —
**this is the load-bearing number for "how much more room is left before a
future raise threatens the compile-time assert"**: roughly another **10×**
before the registry+hash+free-list alone would approach the primordial
segment's total capacity (order-of-magnitude, not a precise gate — `hardened`
and other future metadata growth eat into the same budget and were not
re-derived to a specific next ceiling here).

`size_of::<AllocCore>()` is **unchanged** (760 bytes) — the struct carries
only pointers into the primordial segment, not the array inline, so the
in-process struct size never depends on `MAX_SEGMENTS`.

### 1.2 Idle-process (bootstrap) RSS

Three repeated runs of the R13-8 harness's own idle-RSS probe (`proc_probe`
snapshot immediately after `AllocCore::new()`, before any user allocation),
`production + alloc-stats`, at `MAX_SEGMENTS = 4096`:

| Run | idle RSS (KiB) |
|---:|---:|
| 1 | 2996 |
| 2 | 2996 |
| 3 | 3004 |

R13-8's own baseline at `MAX_SEGMENTS = 1024` (`production` alone) measured
idle RSS = 2992 KiB. The three MAX_SEGMENTS=4096 runs (2996/2996/3004) sit
within a handful of KiB of that — **no directional shift attributable to
the 4× larger table**, consistent with the registry/hash/free-list array
being backed by demand-paged VA inside the primordial segment: the pages
holding slots 1024..4096 (never touched until something actually registers
there) are reserved but not committed/faulted at idle, so they cost
essentially nothing until used.

### 1.3 Scale-sweep — ceiling moved, no non-linear cost approaching it

Re-running R13-8's Part A/B (live-object scale sweep + exact ceiling probe)
at `MAX_SEGMENTS = 4096`, `production + alloc-stats`:

| target | achieved | alloc µs/op | dealloc µs/op |
|---:|---:|---:|---:|
| 256 | 256 | 16.4 | 84.5 |
| 512 | 512 | 22.3 | 91.6 |
| 1024 | 1024 | 18.8 | 89.2 |
| 2048 | 2048 | 18.3 | 91.0 |

(At `MAX_SEGMENTS = 1024` these same target rows hit the wall at 1023;
now they sail through — the ceiling genuinely moved.) Part B's exact
ceiling probe (`probe_target = MAX_SEGMENTS + 64`): `achieved = 4095`,
`stopped_by_null_alloc = true`, `table_count = 4096` at stop — the exact
same "-1 for the primordial slot" shape R13-8 found at the old ceiling,
now reproduced at the new one. Alloc/dealloc µs/op stays in the same
narrow band across every scale point — **no non-linear degradation
approaching the (now 4×-larger) ceiling**, matching R13-8's own headline
finding that the wall is binary, not gradual.

### 1.4 Scan-path audit — why the raise doesn't cost more on the hot path

A `grep -rln MAX_SEGMENTS src/` audit (this session) found every
`O(table size)`-shaped walk in the codebase and classified each:

- **`SegmentTable::bases()`** (`src/alloc_core/segment_table.rs:541`) —
  the only genuine `O(count)` iterator. Its two call sites are
  `AllocCore::drop` (`src/alloc_core/alloc_core.rs:1929`, one-time process
  teardown, not a per-op cost) and the defensive `contains_base` fallback
  inside `find_segment_with_free`'s slow path (bounded — see next point).
- **`find_segment_with_free_impl`** (`src/alloc_core/alloc_core_small.rs:480`)
  — the actual hot alloc-miss path. Under `production`'s default
  `alloc-segment-directory` feature, this is **directory-accelerated**: a
  per-class bitmap query, not a linear scan over the table. The directory
  materializes once `table.count() >= DIRECTORY_MATERIALIZE_THRESHOLD`
  (= 32, `src/alloc_core/segment_directory.rs:117`) and is treated as
  **authoritative on a miss** (R8-2/task #215) — no scan at all on the
  common miss path. A full linear-scan fallback runs only (a) before the
  directory materializes (first 32 segments) or (b) once every
  `DIRECTORY_MISS_FULL_SCAN_PERIOD` (= 64, `segment_directory.rs:160`)
  misses, as a bounded periodic self-heal — i.e. the true `O(table size)`
  scan is already rate-limited to roughly 1-in-64 misses in steady state,
  independent of `MAX_SEGMENTS`, before and after this raise.
- Every other `MAX_SEGMENTS`-adjacent site found by the grep
  (`REGISTRY_FOOTPRINT`/`HASH_FOOTPRINT`/`FREE_LIST_FOOTPRINT` in
  `segment_table.rs`, the layout-offset chain in
  `segment_header_layout.rs`, `dbg_max_segments()`/`dbg_table_count()` in
  `alloc_core_core_diag.rs`) is either a compile-time constant, an O(1)
  accessor, or `#[doc(hidden)]` test-only surface — none scale their
  runtime cost with `MAX_SEGMENTS` beyond the fixed layout-offset
  arithmetic already covered in §1.1.

**Conclusion of §1**: the raise's only real cost is the +84 KiB metadata
footprint (well inside budget) and it does not touch any hot-path
`O(table size)` behavior because `production`'s directory feature already
bounds that independent of table size. This is why the raise, not a design
doc, was the right move for THIS round's headroom — and why this document
exists for the case that headroom is exhausted too.

---

## 2. What the raise does NOT fix — the residual question this document
## addresses

1. **A workload needing more than `MAX_SEGMENTS - 1` (4095)
   simultaneously-live Large objects still hits the identical wall shape**
   R13-8 documented — just at a 4×-higher count. Nothing about the flat
   array/hash/free-list *design* changed; only the constant did. If a
   future measured workload needs, say, 50,000 simultaneously-live Large
   objects, another 4× raise (to 16,384) is not obviously still "cheap":
   §1.1's headroom arithmetic gives roughly one more order of magnitude
   before the fixed-4-MiB-primordial-segment budget becomes the binding
   constraint (not a precise number — see §1.1's own caveat), and every
   further raise trades a little more of the shared primordial metadata
   budget that also hosts small-segment bookkeeping,
   `alloc-segment-directory`'s sidecar, the NUMA directory, etc. (each
   feature's own metadata region). A repeated pattern of "measure, raise,
   repeat" is a legitimate strategy for a while, but it has a visible
   asymptote; a structurally unbounded design is the honest answer once a
   workload's true requirement is unknown or open-ended.
2. **The flat array is not adaptive.** Every process pays the full
   `MAX_SEGMENTS`-sized registry+hash+free-list layout in its primordial
   segment regardless of whether it ever uses more than a handful of Large
   objects — §1.2 showed this costs ~nothing at idle today (demand-paged,
   untouched), but a design that only allocates capacity when actually
   needed is a strictly better invariant to hold as the base constant grows
   across future rounds.
3. **This is exactly the residual gap `R12_13_PAGE_RUN_LAYER_DEFERRED.md`
   already named** (§3 point (b) there: "`SegmentTable`-slot /
   OS-reservation-syscall pressure at high live-object counts") — that
   document's own verdict was NO-GO on implementing page-run *at that time*
   specifically because no workload in this repository demonstrated the
   need. R13-8 is the first concrete demonstration in this codebase's own
   tests/benches/examples of hitting that exact pressure point (§0 there),
   which is why this task exists. The verdict below is NOT "implement
   page-run now" — it is "here is the target design for WHEN a workload
   exceeds the raised ceiling," matching this project's consistent pattern
   (R9-4, R10-4, R11-3, R12-3, R12-13) of gating heavyweight subsystems on
   measured pain, not hypothetical pain.

---

## 3. Design sketch — a two-level chained `SegmentTable`

### 3.1 Level 0 (unchanged) + level-1 extension blocks

Keep everything in `src/alloc_core/segment_table.rs` exactly as it is today
for the first `MAX_SEGMENTS` slots (level 0): same flat array, same
open-addressing hash table, same free-list stack, same O(1)
`register`/`unregister`/`recycle`/`contains_base`. This is the
zero-regression-risk part of the design — every existing test, every
existing perf characteristic for a workload that never exceeds
`MAX_SEGMENTS` live segments, is untouched.

Add a **fixed-size list of extension-block pointers** (say 32 slots — the
number of times the table can be doubled again before hitting some sane
hard ceiling, e.g. `2^32 * MAX_SEGMENTS` if every extension slot were used,
which is already an absurd amount of address space) carved into the
primordial segment right after the existing free-list-top counter. Each
entry is `null_mut()` until materialized. Each extension block, when
materialized, is a **separately OS-reserved region** (via the same
`os::reserve_aligned`/`vmem` seam the primordial segment itself uses,
NOT carved from the primordial segment) holding its own flat array +
open-addressing hash table + free-list, sized identically to level 0
(`MAX_SEGMENTS` slots) or some other tuned constant — the exact per-block
size is a tuning knob, not a structural decision.

### 3.2 `register` when level 0 is full

```text
fn register(&mut self, base) -> Option<SegmentId> {
    if let Some(id) = self.level0.register(base) {
        return Some(id.tag_level(0));
    }
    // Level 0 full: walk materialized extension blocks in order, register
    // into the first one with room; materialize a fresh extension block
    // (one more OS reservation) only if every existing one is also full.
    for (i, block) in self.extensions.iter_mut().enumerate() {
        if let Some(block) = block {
            if let Some(id) = block.register(base) {
                return Some(id.tag_level(i + 1));
            }
        }
    }
    if self.extensions.len() < MAX_EXTENSIONS {
        let mut fresh = ExtensionBlock::reserve()?;
        let id = fresh.register(base)?;
        self.extensions.push(Some(fresh));
        return Some(id.tag_level(self.extensions.len()));
    }
    None // truly exhausted (all levels full)
}
```

A `SegmentId` becomes a tagged index: a couple of high bits select the
level (0 = the flat array, 1..N = which extension block), the rest is the
existing within-block index — this is a **compatible, non-breaking widening
of the existing `u32 segment_id`** field already stored in every
`SegmentHeader` (`src/alloc_core/segment_header.rs`, `segment_id_at`); the
O(1) `unregister`/`recycle` paths that already read `segment_id` directly
out of the header (task #135's optimization) keep working unchanged — they
just decode the level tag first, then dispatch to the right block's slot
array by the same O(1) arithmetic level 0 already uses today.

### 3.3 What stays O(1), what doesn't

- **`segment_of(ptr) = ptr & ~(SEGMENT-1)`** (`src/alloc_core::os`) is
  **completely unaffected** — this is pure address arithmetic, never
  touches the table at all. This is the actual hot-path owner resolution
  used by every `dealloc`/`realloc` call; the table (census structure) is
  only consulted for the defensive `contains_base` foreign-pointer check
  and the alloc-miss `find_segment_with_free` scan (§1.4) — neither is on
  the address-resolution critical path.
- **`unregister`/`recycle`** stay O(1): decode the level tag from
  `segment_id` (already read in one header load), index directly into the
  right block's slot array — same cost shape as today, plus one branch to
  pick the level.
- **`register`** stays O(1) in the common case (level 0 or an already-
  materialized extension block has room) and pays exactly one extra OS
  reservation syscall only on the rare event of needing a brand new
  extension block — an event that, by construction, happens at most
  `MAX_EXTENSIONS` times over the life of a process (bounded, not
  per-allocation).
- **`bases()`** (used only by `Drop` and the bounded directory self-heal
  scan, §1.4) becomes a chain of `count()`-bounded walks across
  materialized blocks instead of one — still linear in the number of live
  segments, same complexity class as today, just spread across blocks.
- **`contains_base`'s own-segment cache** (the `OWN_CACHE_SIZE`-entry
  direct-mapped cache, `segment_table.rs:90-98`) needs its `cache_index`
  derivation extended to fold in the level tag (or simply size the cache
  per-level) — a small, mechanical change to the existing invariant
  documented in `own_cache_clear`'s doc comment, not a new hazard class.

### 3.4 Materialization cost / when this pays for itself

Each extension block is a genuinely new OS `reserve_aligned` call — the
same cost class as reserving any Large segment today (one syscall, no
commit until slots are actually used, matching §1.2's demand-paged
argument for why the flat table's own idle cost is near zero). This design
is strictly BETTER than "just keep raising `MAX_SEGMENTS`" for a workload
whose true requirement is unknown or highly variable across processes,
because a process that only ever uses 500 live Large objects pays for
level 0 only (unchanged from today), while a process that genuinely needs
50,000 pays the extra reservation cost only for the blocks it actually
materializes — the flat-raise strategy makes EVERY process pay the larger
primordial-segment metadata footprint (§1.1) whether or not it needs the
headroom.

---

## 4. Joint evaluation against page-run — capacity ceiling AND the
## cold-carve gap (fx-review note, task #292)

The fx synthesis review
(`docs/reviews/2026-07-23-r14-reviews-synthesis.md` §4, the item
attached to #292) flagged that an earlier draft of this task's framing
risked evaluating the expandable-table idea in isolation from the
still-open page-run question — the two should be compared on BOTH axes a
future reader might care about, not just the capacity ceiling this task's
brief led with:

| Axis | Expandable/chained `SegmentTable` (this doc) | Page-run layer (`R11_7_PAGE_RUN_LAYER_DESIGN.md`) |
|---|---|---|
| **Capacity ceiling** (R13-8's problem) | **Directly solves it.** Structurally unbounded live-segment count (bounded only by `MAX_EXTENSIONS × block size`, a very large practical ceiling), replacing "raise a constant" with "grow lazily." | **Solves it as a side effect**, not its primary design goal — R11-7's own `PageRunTable` sketch was explicitly designed NOT to consume one `SegmentTable` slot per object (§2.6 there), so a page-run arena holding many same-class medium objects amortizes the slot cost across the arena instead of paying one slot per object. For the SPECIFIC 260 KiB–2 MiB range R13-8 measured, this only helps once `medium-classes-wide` is enabled (`R12_13...md` §3 point 1) — under `production`'s actual shipping composition today, page-run would need to also handle the plain Large path this task's ceiling is about. |
| **Cold-carve gap** (16 B/256 B, 2.4–2.7× behind `mimalloc`, README `## Performance` §"Cold first-touch") | **Does not help at all.** This design only changes the census/bookkeeping structure for Large-path segments; it has zero interaction with the Small-class cold-carve path (page-map writes, page faults on genuinely fresh pages — see `docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`) that the 16 B/256 B gap actually lives in. | **Not evaluated for this either** — R11-7's design targets 1.25–2 MiB medium-classes-wide density, not the 16–256 B cold path; the two gaps are in genuinely different code paths (Large-segment census vs. Small-class per-block carve) and neither existing design doc claims to close the cold-carve gap. The already-tried, already-rejected `alloc-runfreelist` experiment (`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`) specifically targeted this gap and regressed every iai bench it touched — it was removed entirely (R6-CQ-4). |
| **Implementation cost** | Smaller — extends one existing module (`segment_table.rs`) plus a tagged-index widening that stays compatible with the existing `segment_id` header field; no new `SegmentKind`, no directory/bitmap format change. | Large — R11-7's own estimate: "six of eleven cross-cutting mechanisms need genuinely new, parallel code," "closer to a second subsystem" (quoted in `R12_13...md` §4). |
| **Demonstrated victim today** | **Yes — R13-8, reproducibly, in this codebase's own examples**, motivating this task directly. | **No** — `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §3 point 2 remains accurate: no test/bench/example in this repository exercises "many-thousands-of-simultaneously-live 1.25–2.0 MiB objects" specifically (as opposed to the plain-Large-path pressure this task's expandable-table design targets). |

**Conclusion of §4**: the two designs are NOT substitutes for each other —
they solve different problems that happen to share a related-sounding name
("segment table capacity" vs. "arena density"). The expandable table is the
right next step **specifically for the capacity-ceiling axis**, because it
has a demonstrated victim (R13-8), a bounded implementation cost, and zero
interaction risk with the cold-carve gap (it doesn't touch that path at
all, for better or worse). The cold-carve gap remains **entirely
unaddressed by either design** and would need its own investigation (the
`alloc-runfreelist` experiment already tried and failed one angle on it;
`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md` is the live tracking
doc for that separate problem). Page-run's own capacity-adjacent side
benefit (§2.6 of R11-7) only closes the specific slice of the ceiling
problem that would exist under `medium-classes-wide` — which is not part
of `production` today — so it is not a substitute for this document's
narrower, cheaper, more directly-targeted fix for the ceiling `production`
users actually hit.

---

## 5. Kill-gate / verdict

**Verdict for THIS session: raise `MAX_SEGMENTS` (done, §1), write this
design doc (done), implement nothing further.** The chained-table design
above is CONDITIONAL — implementation is authorized only when one of the
following becomes true, mirroring this project's established gating
discipline (R9-4, R10-4, R11-3, R11-7, R12-13):

1. A real workload (test, bench, example, or a user report) demonstrates a
   need for more than `MAX_SEGMENTS - 1` (4095) simultaneously-live Large
   objects, OR
2. A future round's `MAX_SEGMENTS` raise stops being "cheap" by this
   document's own §1 criteria (idle RSS shift, primordial-segment headroom
   shrinking toward the compile-time assert, or a scan path that stops
   being directory-bounded), OR
3. A future round separately decides to pursue page-run for the density/
   cold-carve reasons in §4, at which point this document's tagged-`SegmentId`
   widening should be re-evaluated alongside whatever `PageRunTable` design
   that round produces, since both touch the same `segment_id` header field
   and ideally should not be designed twice independently.

---

## 6. Caveats

- No code, test, or benchmark for the chained-table design itself was
  written or run this session — §1's numbers are real measurements of the
  ALREADY-LANDED `MAX_SEGMENTS` raise (1024 → 4096); §3's design is a sketch,
  not a validated prototype, matching this project's two-stage discipline
  (design first, mandatory review, prototype only on separate authorization
  — R10-4/R11-3/R11-7's own precedent).
- §1.1's "roughly one more order of magnitude before the primordial
  segment's fixed budget becomes binding" is an order-of-magnitude
  estimate from the measured headroom numbers, not a precise future gate —
  a future raise attempt should re-measure `PRIMORDIAL_META_END` directly
  rather than trust this extrapolation.
- Single-host measurement (Windows 10 Pro x86-64); no Linux/macOS/NUMA
  cross-check performed this session for the raise itself (same
  limitation R13-6/R13-8 already documented for this feature family).
- The tagged-`SegmentId` widening sketch (§3.2) assumes the existing `u32
  segment_id` header field has enough spare bits for a level tag at
  `MAX_EXTENSIONS`-scale — this was not verified against the actual bit
  layout in `src/alloc_core/segment_header.rs` this session; a real
  implementation attempt must re-derive the exact bit budget before
  committing to this encoding.
