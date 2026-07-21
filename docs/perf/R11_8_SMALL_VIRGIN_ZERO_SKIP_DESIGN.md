# R11-8 — Small-path `alloc_zeroed` virgin-zero skip: DESIGN-ONLY (no code change)

**Task:** design (not prototype) a virgin-carve zero-skip for the SMALL
allocation path's `alloc_zeroed`, mirroring the shipped Large-path skip
(task #221/R8-8, miri fix R9-1), with a mandatory two-stage gate — design
doc first, review, prototype only on separate future authorization.
**Outcome:** **DESIGN-ONLY.** No file under `src/`, `Cargo.toml`, or `tests/`
is modified. This document is the deliverable.
**Date:** 2026-07-21.
**Base revision:** `main` @ `f0dd9a9` (HEAD at task start; `docs(checkpoints):
land session checkpoint files`).

---

## 0. This is substantially a re-verification of an existing design, not a fresh one

**Critical prior art, discovered by grep at the start of this task and read
in full before writing a single line here:**

- `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` (landed commit `7bdbc0f`,
  2026-07-20) — a design-only report that already covers this exact
  optimization end-to-end: precise invariant, per-segment `payload_virgin`
  bool, five risk areas each closed with file:line evidence, a miri gate
  mirroring R9-1, a staged Stage 0/1/2/3 implementation plan, and a full
  test plan. Its verdict: **DESIGN-ONLY, GO for staged implementation on a
  future task; NO-GO for rushing a prototype in that session.**
- `docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md` — an
  earlier, EARLIER **NO-GO** (P4(b)/S3 of the original perf plan) that R9-5
  explicitly rebuts point-by-point, crediting the rebuttal to a substrate
  change (R8-10, task #223, commit `852828e`, 2026-07-20) that removed the
  only production code path that could have made the optimization unsound
  on macOS/XNU/*BSD (`MADV_DONTNEED` laziness).
- `25ae4a5` (`perf(alloc-core): elide fresh-segment AllocBitmap virgin-init`)
  — a DIFFERENT, already-**shipped** optimization that looks superficially
  related (it also uses the word "virgin" and also elides an init pass on a
  freshly-reserved segment) but targets **allocator metadata**
  (`AllocBitmap::init_in_place`'s 32 KiB zero-write), not **user-observable
  payload**. It is not the P4(b)/R9-5/R11-8 optimization and this document
  does not touch it. Verified by reading the commit body and the current
  `AllocBitmap::init_in_place` call sites — unchanged from R9-5's account.

**What this document adds over R9-5, per the current task's explicit
requirements:**

1. Independent re-verification of every file:line claim against the
   CURRENT source tree (line numbers have drifted since 2026-07-20 —
   `alloc_core_small.rs` is now 2267 lines; R9-5 cited a ~2100-line
   version), not a transcription of R9-5's own analysis.
2. Confirmation that the **production gap R9-5 identified (§7 there) is
   still open today**: `HeapCore::alloc_zeroed`'s small arm
   (`src/registry/heap_core_alloc.rs:325-333`, current) still delegates to
   `self.alloc(layout)` + unconditional `Node::zero`, and does NOT consult
   `AllocCore::alloc_zeroed` or any virgin signal. Nothing has closed this
   gap since R9-5 was written — confirmed by `git log` on the intervening
   commit (`33581bd`, R11-1) touching `heap_core_alloc.rs` only in the
   `alloc_batch`/`dealloc_batch` surface, orthogonal to `alloc_zeroed`.
3. A **formally-stated predicate** (§2) in the exact form the task brief
   requests ("block at offset O is virgin at the moment of carve C iff...").
4. The **kill-gate table format** this session's R10-4/R11-3/R11-7 used
   (criterion → target → finding → PASS/FAIL → GO/CONDITIONAL GO/NO-GO),
   which R9-5 approximated but did not use verbatim.
5. A test plan restated against the CURRENT call graph (magazine plumbing,
   `refill_class_bump_impl`'s interleaved drain/carve — traced fresh in §5).
6. Explicit verification of every one of the orchestrator's five numbered
   hypotheses from the task brief (§6), each checked against source read
   this session, not assumed from R9-5's account (R9-5 predates this task
   and could not have anticipated its exact framing).

**Where this document's verdict differs from R9-5's:** it doesn't. Having
independently re-derived the same invariant from the same source files and
found no new production code that changes any of the five risk areas, this
document reaches the **same substantive conclusion** — design is sound,
verdict is DESIGN-ONLY / GO-for-staged-implementation, NOT GO for a
same-session prototype — via its own independent verification path, and
folds in the specific deliverables the R11-8 task brief asks for that R9-5's
framing did not produce in the same shape (formal predicate, kill-gate
table, per-hypothesis verification ledger).

---

## 1. Scope recap — the shipped Large-path precedent, read in full

`AllocCore::alloc_large` returns `(*mut u8, bool)`
(`src/alloc_core/alloc_core_large.rs:57`). The bool is `true` iff the
returned allocation lives in a genuinely fresh OS reservation the OS
zero-fills by construction (Windows `VirtualAlloc` MEM_COMMIT demand-zero;
Unix anonymous `mmap` zero-fill). `alloc_large_slow` (the only fresh-span
producer, `alloc_core_large.rs:268-361`) yields `cfg!(not(miri))` at its
sole return site (line 360): "R9-1: fresh means OS-zeroed only on real OS
backends; miri's `std::alloc` fallback does not zero, so the freshness
signal must be withheld there." A `large_cache` HIT (a reused,
previously-freed segment that may still hold the prior occupant's bytes,
`alloc_core_large.rs:127-250`) yields `false` unconditionally (line 249).
`AllocCore::alloc_zeroed`'s Large arm (`alloc_core.rs:952-960`) consults the
bool and skips `Node::zero` iff `!ptr.is_null() && !is_fresh` is false, i.e.
skips when fresh. `LARGE_ZERO_PASS_CALLS` (`alloc_core.rs:221`, gated
`alloc-stats`) counts every NON-skip, so a regression that reintroduces
unconditional zeroing is observable by a nonzero delta even when byte
content alone cannot distinguish "skipped because OS-zeroed" from "zeroed
redundantly" (the doc comment at `alloc_core.rs:207-210` states this
explicitly).

**Confirmed bug class from this exact mechanism's history**: R9-1
(`860d897`) fixed a real correctness gap — under miri,
`crates/vmem`'s aperture falls back to `std::alloc::alloc` (confirmed,
`crates/vmem/src/lib.rs:7-8`: "Under miri it falls back to `std::alloc` so
consumers stay miri-testable"; line 766-768: "Under miri, `reserve_aligned`
falls back to `std::alloc`, which does NOT [zero]... zero explicitly under
miri"), which does NOT zero — so a naively "fresh" reservation under miri
carries no zero guarantee, and treating it as fresh would return
uninitialized memory. This is the exact bug class this task's brief warns
about, and the fix pattern (`cfg!(not(miri))` gating the freshness bool at
its single production site) is the template this design reuses verbatim
for the Small path (§4).

**Why Large and Small differ structurally.** Large has exactly ONE block
per dedicated segment — segment-level freshness and block-level freshness
coincide. Small has MANY same-class blocks per 4 MiB segment, carved
incrementally by a monotonic per-segment bump cursor. A block at offset
`[old_bump, old_bump + block_size)` is carved exactly once; after a later
`dealloc`, it may return to the segment's free list and be re-served by
`pop_free` — bit-for-bit indistinguishable at the `alloc_zeroed` call site
from a virgin carve *unless something at the dispatch layer distinguishes
them*. This is the entire technical crux both R9-5 and this document
resolve (§2-§4) without adding per-block metadata.

---

## 2. The virgin invariant — formally stated

**Definition (virgin block, block-level).** A small block at segment
offset `O` (with `O` a whole multiple of `block_size(class_idx)` and
`O >= payload_start`) is **virgin at the moment of carve `C`** if and only
if BOTH of the following hold:

1. **Dispatch condition** — `C` is a bump-cursor advance
   (`AllocCore::carve_block` or `AllocCore::carve_batch`) that serves `O`
   for the FIRST time in this segment's current registered lifetime, i.e.
   `C` is not `pop_free` / `drain_freelist_batch` (a free-list pop, which by
   construction serves a block that was carved and issued at some earlier
   point — see the codebase's own invariant statement,
   `alloc_core_small.rs:1564-1566`: *"Every block that was ever carved has
   `off >= bump` ONLY after such a reset [decommit]... a live block in a
   committed segment always has `off < bump`"* — the CONTRAPOSITIVE of this
   is exactly the dispatch condition: a block reachable via `pop_free` has
   `off < bump` throughout, meaning it was carved at some `bump' <= off`
   strictly before the CURRENT carve, hence is not virgin).

2. **Lifetime condition** — the segment's `payload_virgin` state (defined
   below) is `true` at the moment of `C`, where:

   > `payload_virgin` is `true` for a segment iff every byte in
   > `[payload_start, current_bump)` at this instant was made readable
   > EITHER by the segment's own fresh OS reservation (`Segment::reserve` /
   > `numa::reserve_aligned_on_node`) OR by an incremental first-time
   > `os::commit_pages` grow-on-carve call (`alloc-lazy-commit` only), and
   > NO in-place decommit-then-recommit cycle has EVER occurred on this
   > segment's CURRENT registration (i.e. since its last
   > `reserve_small_segment` / bootstrap, this segment's slot has not been
   > through `decommit_empty_segment_impl(..., release_follows=false)`).

**Combined predicate** (the form the task brief asks for):

```text
is_virgin(segment, O, C) :=
       C ∈ {carve_block, carve_batch}                       (dispatch)
    ∧  O ∈ [aligned_bump_before_C, bump_after_C)             (this carve served O)
    ∧  segment.payload_virgin == true                        (lifetime)
    ∧  cfg!(not(miri))                                       (OS-zero guarantee)
```

All four conjuncts are independently necessary and jointly sufficient (§4
proves each cannot be bypassed by any live production code path). The
dispatch conjunct needs NO new metadata (it is already syntactically
distinguished — `alloc_small`'s three-branch dispatch, `pop_free` vs
`find_segment_with_free`+`pop_free` vs `carve_block_with_refill`,
`alloc_core_small.rs:103-222`). Only the lifetime conjunct requires ONE new
bit of per-segment state (§3). The miri conjunct is a compile-time `cfg!`,
zero runtime cost.

**Explicit non-virgin cases the predicate correctly excludes:**

- **Reused (freed-then-realloc'd) block**: served via `pop_free` →
  dispatch conjunct false → never virgin, regardless of `payload_virgin`.
- **Pooled-but-not-decommitted segment (R8-3/R8-10 precedent)**: R8-10
  (task #223, `alloc_core_small_pool.rs:258-265`) established pool
  admission "never decommits or resets metadata... free lists intact." A
  pooled segment's already-carved region (`[payload_start, bump)` as it
  stood at pool-entry) is reused via `find_segment_with_free`'s free-list
  path (`pop_free`/`drain_freelist_batch`) — dispatch conjunct false for
  every block in that region. `carve_block`/`carve_batch` NEVER runs on a
  pooled segment while it remains pooled — `reserve_small_segment`'s own
  doc (`alloc_core_small.rs:1624-1644`, current) states a pooled segment
  "is fully-carved (bump near `SEGMENT` end)... cannot serve as a FRESH
  carve target... drawn from via `find_segment_with_free`'s free-list
  reuse... NOT popped as `small_cur` here." So pooling can never mark a
  block virgin, independent of the lifetime bit's value — **exactly the
  case the task brief calls out as must-never-happen, confirmed structurally
  unreachable, not merely "the bit says false."**
- **Post-decommit-recommit block, IF that path is ever live**: lifetime
  conjunct set to `false` at the decommit site (§3 reset table) —
  conservative regardless of whether the in-place-decommit-then-reuse path
  is reachable in production today (§4.3 shows it currently is not, but the
  design does not rely on that non-reachability holding forever).

---

## 3. Tracking mechanism

### 3.1 What state is needed, and where

**One new per-segment field**, added to `SegmentHeader`
(`src/alloc_core/segment_header.rs`), alongside the existing owner-only
`bump: usize` (near line 315 per R9-5's citation; the header's `decommitted:
u32` field is at line 372 and `committed_payload_end: usize` at line 547 in
the CURRENT tree, confirmed by grep this session):

```text
payload_virgin: bool   // owner-only, single-writer (the segment's owning
                        // AllocCore's owning thread). true iff every byte
                        // in [payload_start, bump) was made readable by a
                        // fresh OS reserve or a first-time incremental
                        // commit on the segment's CURRENT registration —
                        // i.e. no in-place decommit-then-recommit cycle has
                        // occurred since the last reserve_small_segment /
                        // bootstrap for this slot.
```

**Is `bump`'s current value alone sufficient, without a separate bit (the
task brief's own question)?** No — `bump` tracks HOW FAR the segment has
been carved, not WHETHER that carved range is still OS-zero-backed. Two
segments can have an identical `bump` value while differing in virginity:
one fresh, one that went through decommit+recommit-in-place (a currently
dead production path, §4.3, but the field must not assume it stays dead).
`bump`'s monotonicity WITHIN one lifetime is what collapses the general
"virgin frontier: usize" idea (mentioned as a hedge in R9-5 §3 and in the
task brief's point 1) down to a single bool: since `bump` never rewinds
except at a full decommit-reset (`decommit_empty_segment_impl` sets
`meta.set_bump(payload_start)`, `alloc_core_small_pool.rs:641,696`), a carve
is ALWAYS past the previous carve's frontier, so "is this specific carve
past the frontier" is tautologically true for every carve in a lifetime —
the only question that remains is the single lifetime-wide "has this
registration ever been decommitted-in-place," which is exactly what the
bool encodes. A `usize` frontier would be strictly more general (it would
also correctly handle a hypothetical FUTURE substrate where bump could
retreat WITHOUT a full metadata reset) but buys nothing against the
substrate that exists today, at 8x the storage. **Verdict: bool, not
frontier**, matching R9-5's §3 conclusion, independently re-derived here.

### 3.2 Reset/update protocol at every relevant transition

| Event | `payload_virgin` after | Site (confirmed this session) | Why |
|---|---|---|---|
| Fresh `reserve_small_segment` (incl. primordial bootstrap) | `cfg!(not(miri))` | `alloc_core_small.rs:1623-1900ish` (current `reserve_small_segment`), `bootstrap.rs` primordial init | Genuinely fresh OS reservation — OS-zero-guaranteed on every REAL backend (`Segment::reserve`, `os.rs:146`, shares the exact `aligned_vmem` seam Large uses, confirmed §4 below); miri withheld identically to R9-1. |
| `carve_block` / `carve_batch` success | **unchanged** | `alloc_core_small.rs:1247-1357`, `1404-1499` | A carve does not decommit anything; it only reads the bit (to compute the per-call/per-batch virgin signal) and never writes it. |
| `pop_free` / `drain_freelist_batch` success | **unchanged** | `alloc_core_small.rs:1014-1109`, `1159-1240` | Free-list reuse doesn't touch lifetime state; served blocks are reported non-virgin via the DISPATCH conjunct (§2), not via this bit — the bit stays whatever it was for the SEGMENT (a future carve on the same segment still needs the correct lifetime answer). |
| `dealloc_small` / `reclaim_offset(_checked)` push | **unchanged** | `alloc_core_small.rs:1515-1617`, `alloc_core_small_reclaim.rs:68-330` | Neither own-thread nor cross-thread free touches `bump`, `committed_payload_end`, or (in this design) `payload_virgin` — see the codebase's own existing owner-only-field discipline note for `bump` (`alloc_core_small.rs:1250-1255`: "the Owner touches ONLY the `bump` field... `bump` is owner-only... a plain field write is race-free"); `payload_virgin` is owner-only by the identical argument (written only in `reserve_small_segment` and the decommit-reset path, both owner-thread; read only in `carve_block`/`carve_batch`, also owner-thread). |
| Pool admission (`release_or_pool_empty_segment`, pool branch) | **unchanged** | `alloc_core_small_pool.rs:234-285` (confirmed this session: "pool admission never decommits or resets metadata... free lists intact") | R8-10 pool admission is a pure linked-list splice (`pool_push_front`, sets only `pool_prev`/`pool_next`) — it never touches `bump`/payload/`payload_virgin`. (Moot anyway per §2's pooled-segment case: `carve_block` never runs on a pooled segment, so the bit is never consulted while pooled.) |
| Pool eviction → release (`release_empty_segment_now` / `decommit_empty_segment_for_release`, `release_follows=true`) | **irrelevant** — segment ceases to exist | `alloc_core_small_pool.rs:359-360`, `619-622` | Full `os::release_segment` (`MEM_RELEASE`/`munmap`) follows immediately; the registration is gone. A future reservation at any address is a FRESH `reserve_small_segment`, which resets the bit to `cfg!(not(miri))` from scratch — no special-case needed. |
| In-place decommit-and-retain (`decommit_empty_segment_impl(..., release_follows=false)`) | **`false`** | `alloc_core_small_pool.rs:629-730` | The ONLY path that can decommit a payload while leaving the segment registered. §4.3 shows this has ZERO production callers today (same grep result R9-5 found, independently re-run this session), but the design sets the bit defensively so a FUTURE change re-enabling this path (a new decommit policy) cannot silently regress the skip's soundness — the skip degrades to "always zero" automatically rather than becoming unsound. |

**Read site.** `carve_block`/`carve_batch` read `payload_virgin` ONCE per
carve call/batch and produce the per-block(-run) virgin signal
`segment.payload_virgin && cfg!(not(miri))`; `pop_free`/
`drain_freelist_batch` produce `false` unconditionally, never reading the
bit at all (the dispatch conjunct alone settles it). `alloc_small` (and, for
the production path, the magazine refill machinery — §5) must propagate
this signal alongside the pointer(s) it returns, since the existing
`*mut u8`-only return type carries no room for it — this is the actual
plumbing cost, not the bit itself.

**Storage cost.** One byte per 4 MiB segment (`1 byte / 4 MiB`, matching
R9-5's accounting) — negligible, and importantly requires NO per-block
metadata (resolving the original 2026-07-10 P4(b) NO-GO's reason #1
outright, §7).

---

## 4. Interaction inventory — every mechanism that could invalidate the invariant

Re-verified against the CURRENT source tree this session (not merely cited
from R9-5), covering the specific list the task brief demands: M2 guards,
magazine, BinTable free-list, M6 decommit/pool/recommit, cross-thread
frees, and the `hardened` generation-bump mechanism.

### 4.1 M2 double-free guard (own-thread and cross-thread)

`dealloc_small` (`alloc_core_small.rs:1515-1617`) and
`reclaim_offset`/`reclaim_offset_checked`
(`alloc_core_small_reclaim.rs:68-330`) both gate on the alloc-bitmap
`is_free` test before pushing — a block already free is a no-op, never
double-pushed. **Interaction with virginity: none.** M2 governs whether a
free succeeds; it never marks anything virgin, and a block that reaches
either free path has, by construction, already been carved (issued once —
otherwise there would be nothing to free), so it is already permanently
non-virgin per the dispatch conjunct. M2 firing or not firing cannot change
that.

### 4.2 Magazine push/pop — does it write the block body?

**Verified this session, not merely cited.** `refill_class_bump_impl`
(`alloc_core_small_magazine.rs:139-...`) fills its `out` buffer via TWO
possible producers in the SAME call, confirming the task brief's point 3
precisely:

1. `drain_freelist_batch(self.small_cur, class_idx, &mut out[filled..])`
   (line 187) and, on a miss, the same call against
   `find_segment_with_free`'s result (line 224) — reads existing free-list
   blocks. `drain_freelist_batch`'s own doc (`alloc_core_small.rs:1123-1130`)
   states explicitly: *"We never WRITE the block body on this path (pop
   doesn't)"*.
2. `carve_batch(class_idx, block_size, &mut out[filled..])` (line 240 and
   249) — fresh bump-carve. `carve_batch`'s own doc
   (`alloc_core_small.rs:1396-1398`) states: *"M2: carve NEVER touches the
   alloc bitmap (a bump-carved block is already bit0=allocated, the M2
   convention) — identical to `carve_block`."* — and neither `carve_block`
   nor `carve_batch` calls `Node::write_next` or any other body-write
   primitive anywhere in their bodies (confirmed by reading both functions
   in full, `alloc_core_small.rs:1247-1357` and `1404-1499`).

**So carve NEVER writes the block body — confirmed, not assumed.** The
magazine refill loop interleaves free-list-drained blocks
(`out[0..k]`, non-virgin) and freshly-carved blocks
(`out[k..filled]`, potentially virgin) in the SAME `out` slice across
possibly-multiple `while` iterations — this is the per-block distinction
the task brief's point 3 flags as subtle, and it IS per-slot-distinguishable
(each contiguous run within `out` comes from exactly one producer call, so
a virgin-tracking design can tag runs, not individual slots, at zero extra
per-block cost — see §5's magazine-plumbing note).

**Magazine POP** (the consumer side, `HeapCore::alloc`'s fastbin fast path)
is a pointer-array pop from `PerClass.slots` — reads a stored pointer, does
not touch the pointee's bytes at all. **Magazine PUSH** (a `dealloc` back
into the magazine) similarly stores the freed pointer into the slot array;
R11-1's own commit message (`33581bd`, confirmed this session) describes
the magazine-residency BITMAP bit being cleared/set — a per-segment
sidecar bitmap, disjoint from the block body — never the block's own bytes.
**Confirms task-brief point 5's claim: the magazine free path touches no
block body at all**, independently re-verified against `carve_block`/
`carve_batch`/`drain_freelist_batch`'s doc comments and bodies this session
(not merely re-cited from the prior R11-1 finding).

### 4.3 BinTable free-list (`dealloc_small` / `reclaim_offset`) — the "moot" argument, verified

`dealloc_small` (line 1591) and `reclaim_offset`/`reclaim_offset_checked`
(lines ~187, ~300+) DO call `Node::write_next(block_nn, old_head_ptr)` —
writing the intrusive free-list `next` pointer INTO the block's own body.
**Does this matter for virginity?** No, and the reasoning is now verified
rather than assumed: `write_next` on ANY of these three call sites (own-
thread free, own-thread ring-drain reclaim, checked ring-drain reclaim)
fires ONLY on a block that is being FREED — which requires the block to
have been carved and handed out at some earlier point (you cannot free
something never allocated; the M2 bitmap guard additionally requires the
block to currently read "allocated," i.e. previously carved via
`carve_block`/`carve_batch`, which set the bit as part of a carve). So
every `write_next` call targets an ALREADY non-virgin block (dispatch
conjunct already false for it) — the write happens strictly AFTER the
block has permanently left the virgin state (at its original carve), so it
can never cause a virgin block to become dirty-without-detection. **The
task brief's proposed "should be moot, but verify this reasoning
explicitly" is confirmed: moot.**

### 4.4 M6 decommit / pool / recommit machinery — the macOS crux, re-verified

**This is the load-bearing risk area.** Two sub-paths, both re-checked by
grep this session (not merely cited from R9-5):

**(a) Full release + fresh re-reserve.**
`release_or_pool_empty_segment`'s release branch (line 274,
`alloc_core_small_pool.rs`) → `release_empty_segment_now` →
`decommit_empty_segment_for_release` (line 620, hard-codes
`release_follows=true`) → `decommit_empty_segment_impl` takes the
`release_follows` fast path (lines 637-643): sets ONLY `bump` and
`decommitted`, then returns immediately — no payload state is meaningfully
"reset" because the whole reservation is released moments later
(`os::release_segment` → `MEM_RELEASE`/`munmap`) and the slot is `recycle`d
(NULLed). A future allocation is a genuinely fresh `reserve_small_segment`
— zero-guaranteed on every real OS backend including macOS fresh `mmap`
(the `MADV_DONTNEED` laziness exception is specifically decommit-then-REUSE,
not fresh reserve). `payload_virgin` resets to `cfg!(not(miri))` from
scratch. **Sound.**

**(b) Decommit-in-place + recommit-on-reuse — the dangerous state.**
`decommit_empty_segment_impl(_, _, release_follows=false)` is the only path
that could decommit a payload while the segment STAYS registered, creating
exactly the macOS `MADV_DONTNEED`-is-advisory-and-lazy danger the original
2026-07-10 NO-GO named. **Grep re-run this session, independent of R9-5's
result:**

```text
$ grep -rn "decommit_empty_segment_impl" src/alloc_core/
src/alloc_core/alloc_core_small_pool.rs:621:  Self::decommit_empty_segment_impl(meta, base, true);
src/alloc_core/alloc_core_small_pool.rs:631:  fn decommit_empty_segment_impl(meta: &mut SegmentMeta, base: *mut u8, release_follows: bool) {
```

**Zero production callers pass `release_follows=false`.** The ONLY call
site (line 621) hard-codes `true`. This confirms R9-5's finding (§4.3
there) is still accurate on the current tree: R8-10 (task #223, landed
2026-07-20, the day the deep-audit P2-3 flagged this exact risk as
unresolved) removed the B3 decommit-on-pool-admission design that used to
be the production caller of the `release_follows=false` leg. With that leg
dead, `is_decommitted()` is never `true` on a live registered small segment
in production, so `carve_block`'s/`carve_batch`'s `is_decommitted()`
recommit branches (`alloc_core_small.rs:1265-1305`, `1423-1442`) are also
currently-dead defensive code. **Because the dangerous path is
structurally unreachable in production today, no live block can ever
reach a decommit-then-recommit-in-place cycle, so no block can be
incorrectly marked virgin via this route.** The design still sets
`payload_virgin=false` at the `release_follows=false` site (§3 table) so a
FUTURE re-introduction of this leg (a new decommit policy) fails safe
(reverts to always-zero) rather than silently becoming unsound — this is
the honest hedge the prior NO-GO's reasoning demanded, kept even though the
path is dead today.

### 4.5 Cross-thread frees (`RemoteFreeRing` / `HeapOverflow`)

**Producer side** (`RemoteFreeRing::push`, `remote_free_ring.rs:752`):
signature is `push(&self, offset: u32) -> Result<(), PushOverflow>` —
confirmed this session by reading the signature directly: the ring carries
ONLY a packed `u32` (offset + class bits, optionally a generation byte
under `hardened`), never a pointer, never touches the block's bytes at all.
A cross-thread free can therefore never write into the freed block's body
on the producer side, by construction of the ring's own type.

**Consumer side** (`reclaim_offset`/`reclaim_offset_checked`, owner-thread,
during a ring drain): DOES call `write_next` into the block body (§4.3),
but — same argument as §4.3 — only on a block that is already non-virgin
(it was carved and issued before it could ever be freed, cross-thread or
not). **The cross-thread free path cannot elevate a non-virgin block to
virgin, and (being owner-thread-only for the `payload_virgin` bit's writes)
cannot race or corrupt the bit itself** — the field stays owner-only by the
same single-writer argument the codebase already documents for `bump`
(§4.2/§3.2 table).

`HeapOverflow` (the RAD-4b fallback path for when the inline ring
overflows, gated `alloc-xthread` without `fastbin`) was checked for the
same property: it is a directory of deferred frees keyed by pointer/offset,
drained the same way as the ring, through the same `reclaim_offset`
machinery — no separate body-write primitive. Same conclusion.

### 4.6 `hardened`'s generation-bump mechanism

**Verified this session: `bump_gen`
(`src/alloc_core/segment_header_gen_table.rs:98-110`) writes to
`Node::atomic_u8_at(base, Layout::gen_table_off() + idx)` — a per-segment
GENERATION TABLE that lives in segment METADATA (confirmed by its own doc
comment, lines 112-136: "lives in segment metadata, is NOT decommitted with
the payload"), not the block payload.** `gen_table_off()` is a fixed
metadata-region offset, structurally disjoint from `payload_start` (the
generation table occupies its own footprint within `[0, payload_start)`,
the same never-decommitted metadata region the bump/committed_payload_end
fields live in). **The gen-bump writes a byte the caller of `alloc_zeroed`
NEVER observes** — it is not part of the `size`-byte span
`Node::zero`/the virgin-skip operates over. So: `bump_gen` writes
header-adjacent metadata, a DISJOINT region from the block body the caller
would see as "zeroed content" — precisely the disambiguation the task
brief demanded a precise (not guessed) answer for. **No interaction with
the virgin invariant.**

`pop_free` calls `bump_gen` at line ~1098-1106 (current), gated
`#[cfg(feature = "hardened")]`, at the point a block is handed DIRECTLY to
a caller via the non-magazine substrate pop — i.e. only on the
ALREADY-non-virgin `pop_free` dispatch leg (never on `carve_block`/
`carve_batch`, whose doc explicitly says the bump is NOT bumped there;
"blocks pulled into the magazine are NOT bumped here — they are bumped on
their later magazine pop"). So even structurally, `bump_gen` never fires on
the carve (virgin-candidate) dispatch leg at all in the non-magazine path;
under `hardened` (which implies `fastbin`, `Cargo.toml:374`), blocks are
served through the magazine and this substrate `pop_free` call site is not
even reached for `HeapCore::alloc`'s production small-object path — moot
twice over.

---

## 5. Miri safety

Mirrors R9-1 exactly, independently re-verified this session:

- `crates/vmem/src/lib.rs:7-8`: "Under miri it falls back to `std::alloc` so
  consumers stay miri-testable." Line 766-768 (`leak_zeroed_pages`,
  the SAME helper Large's freshness doc cites): "Under miri, `reserve_aligned`
  falls back to `std::alloc`, which does NOT [zero]... zero explicitly under
  miri."
- **Small segments share the SAME miri-fallback aperture as Large — verified,
  not assumed.** `reserve_small_segment`'s non-numa-aware path
  (`alloc_core_small.rs:1713-1733`, current) calls `Segment::reserve(SEGMENT)`
  — the identical `os.rs:146` `Segment::reserve` function `alloc_large_slow`
  calls for its OS reservation (`alloc_core_large.rs:305`,
  `#[cfg(not(feature = "numa-aware"))]` arm). Both paths bottom out in the
  same `aligned_vmem` seam, which is the crate `crates/vmem` re-exports
  under the name used throughout `os.rs`. There is exactly ONE miri-fallback
  aperture for BOTH segment kinds, not two independently-gated ones — so the
  Large path's proven-correct gate transfers structurally, not by analogy.
- Design requirement (§3 table, row 1): `payload_virgin = cfg!(not(miri))`
  at fresh reserve. Combined predicate (§2) ANDs this with `cfg!(not(miri))`
  again at the read site as defense-in-depth (matching Large's pattern of
  gating at the single producer site) — under `cfg!(miri)` the combined
  signal is always `false`, so `alloc_zeroed`'s small arm always runs
  `Node::zero` under miri, preserving the contract there exactly as R9-1
  did for Large.
- Test requirement: mirror `tests/alloc_zeroed_fresh_large_skip.rs`'s
  per-platform delta-assertion pattern (real-OS: skip fires, delta 0;
  miri: skip does not fire, delta 1) verbatim — see §7.

**Not run this session** (per this task's design-only constraint and this
repo's own documented miri cost, ~17+ minutes/run) — verified by reading
the `cfg!(miri)` gates, which are compile-time and structurally identical
to R9-1's already-shipped, already-miri-validated Large-path gate.

---

## 6. Verification ledger — the orchestrator's five hypotheses, checked against source read this session

| # | Hypothesis (from task brief) | Verified? | Where checked |
|---|---|---|---|
| 1 | Bump only advances via `carve_block`/`carve_batch`, never rewinds except at M6 decommit-release (resets to `small_meta_end()`), and pooling does NOT reset bump | **Confirmed, with one refinement**: the "pooling does NOT reset bump" comment the brief quotes lives at `alloc_core_small_pool.rs` (the R8-10 admission-path doc, confirmed lines 258-265 this session: "pool admission never decommits or resets metadata"). The decommit-RELEASE path (`release_follows=true`) DOES reset `bump` (line 641, `meta.set_bump(payload_start)`) but this precedes an immediate full OS release — the segment ceases to exist, so "reset" is moot for virginity (a future reserve at any address is fresh, §4.4(a)). The decommit-RETAIN path (`release_follows=false`) also resets bump (line 696) but is the dead-in-production leg (§4.4(b)). | `alloc_core_small_pool.rs:234-730` |
| 2 | Every issued block (fresh carve OR magazine/free-list pop) originates from a prior bump advance; a block at offset `< bump` has been carved and issued at SOME point even if currently free-listed; a block at offset `>= bump` (current) has never been carved | **Confirmed exactly**, and is in fact the codebase's OWN stated invariant (`alloc_core_small.rs:1564-1566`, quoted verbatim in §2 above) — not merely the orchestrator's inference. | `alloc_core_small.rs:1558-1574` (the `dealloc_small` `off >= bump` guard's own doc comment) |
| 3 | The check is narrower than "is this a Small alloc" — it's "did THIS alloc_zeroed call resolve via a fresh carve"; the refill path (`refill_class_bump_impl`) may drain BOTH free-list blocks AND carve fresh ones in the SAME refill call, needing per-block (not per-call) distinction | **Confirmed exactly**, verified by reading `refill_class_bump_impl`'s full body this session (§4.2): the `while filled < want` loop alternates `drain_freelist_batch` (lines 187, 224) and `carve_batch` (lines 240, 249) — a single refill call CAN and typically DOES interleave both producers across its iterations. Per-RUN (not strictly per-block) granularity suffices, since `carve_batch`'s own bump-monotonicity makes every block within ONE `carve_batch` call share one virgin signal (§4 in R9-5, re-derived independently in §2's "lifetime condition" here) — so the correct granularity is "per contiguous producer-call span within `out`," not per-individual-block, which is slightly coarser than the brief's literal phrasing but equivalent in effect and cheaper to implement. | `alloc_core_small_magazine.rs:139-260` (`refill_class_bump_impl`) |
| 4 | Decommit+recommit must be paired with an actual OS-level decommit such that a subsequent recommit is genuinely OS-zero again; if any path resets bump WITHOUT a matching physical decommit, treating post-reset carves as virgin is unsound | **Confirmed as a real (but currently dead) hazard, correctly identified**: `decommit_empty_segment_for_release` (`release_follows=true`) is the ONE path that resets `bump` WITHOUT necessarily performing `os::decommit_pages` first — reading its body (lines 637-643) confirms it explicitly SKIPS the `os::decommit_pages` call ("the ONLY load-bearing action is resetting the bump cursor... the whole reservation is about to go back to the OS" — no physical decommit needed because `os::release_segment` supersedes it moments later). This is EXACTLY the brief's flagged hazard — but it is provably harmless BECAUSE the release follows immediately (§4.4(a)): there is no window where a stale "committed-but-logically-reset" segment could be carved into by a future `carve_block` (the segment is unregistered before any such carve could occur), so the virgin-invariant's dispatch+lifetime combination is never evaluated against this transient state at all. The retain-path (`release_follows=false`) DOES pair the reset with a real `os::decommit_pages` call (line 677/687) — but is dead in production (§4.4(b)). | `alloc_core_small_pool.rs:619-730` |
| 5 | The magazine push/pop path never writes to a block's body (making the "recycled blocks are never mistaken for virgin" argument moot); the BinTable free-list `next`-pointer write on push is moot because it only ever targets an already-carved (non-virgin) block | **Confirmed, independently re-verified against current source (not re-cited)**: §4.2 (magazine: no body writes anywhere in `carve_block`/`carve_batch`/`drain_freelist_batch`/magazine pop-push) and §4.3 (BinTable: `write_next` calls in `dealloc_small`/`reclaim_offset(_checked)` only ever target already-carved blocks, moot by construction). | `alloc_core_small.rs` (carve/pop docs), `alloc_core_small_reclaim.rs:68-330` |

**Overall: all five hypotheses held up under independent verification.**
Hypothesis 1 needed a minor refinement (distinguishing the release-path
reset, which is moot, from a hypothetical retain-path reset, which the
brief's own point 4 already anticipated and which §4.4(b)/§4.3 close). No
hypothesis was refuted; none required abandoning the design.

---

## 7. Test plan for a future stage-2 (implementation) session

New test file `tests/alloc_zeroed_virgin_small_skip.rs`
(`#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]`,
mirroring `tests/alloc_zeroed_fresh_large_skip.rs`'s structure — including
its file-wide `Mutex` serialization, since the new `SMALL_ZERO_PASS_CALLS`
counter would be process-wide like `LARGE_ZERO_PASS_CALLS`):

1. **`fresh_small_alloc_zeroed_is_all_zero_and_skips_zero_pass`** — the
   "skip genuinely fires" test the task brief mandates: allocate via
   `AllocCore::alloc_zeroed` on a genuinely fresh segment (virgin carve),
   assert (a) every byte reads zero AND (b) `SMALL_ZERO_PASS_CALLS` delta is
   0 under real OS / 1 under `cfg(miri)` — an OBSERVABLE signal that the
   explicit zero pass was actually skipped, not merely that the result
   happened to read zero (satisfying the brief's explicit requirement that
   this be distinguishable from "zeroed redundantly").

2. **`dirty_freed_reallocd_small_still_zeroes`** — **the mandatory
   counterfactual** the task brief specifies verbatim: `alloc` a block,
   write `0xAA` (or another non-zero poison pattern) to every byte,
   `dealloc` it, then `alloc_zeroed` the same size class REPEATEDLY until
   the free-list pop serves the SAME offset back (deterministic for a
   single-block free list — the very next `alloc_zeroed` call of the same
   class, since `pop_free` is LIFO), and assert every byte is genuinely
   zero AND the delta is 1 (skip must NOT fire — dispatch conjunct is
   false). This is the test that would go RED if the virgin predicate is
   wrong in the dangerous direction (leaking dirty memory) — the exact
   counterfactual R9-1's Large-path test already validates the SHAPE of
   (`tests/alloc_zeroed_fresh_large_skip.rs`'s dirty-cache-hit test,
   confirmed to exist by its citation in R9-5 §11.3 and the file's presence
   in the repo).

3. **`pooled_segment_alloc_zeroed_never_claims_virgin`** — drive a segment
   through empty → pool → reuse (the R8-10 path; `tests/small_segment_pool.rs`
   has scaffolding to build on), then `alloc_zeroed` from the pooled
   segment's free list: every served block must read all-zero AND
   `SMALL_ZERO_PASS_CALLS` must increment per block (never virgin) — this is
   the regression guard for §2's "pooled-but-not-decommitted" exclusion.

4. **`interleaved_virgin_and_reuse_within_one_refill`** — NEW relative to
   R9-5's test plan, added specifically because §4.2/§6-point-3 established
   that ONE `refill_class_bump_impl` call can interleave free-list-drained
   and freshly-carved blocks in the SAME `out` buffer: construct a scenario
   (free some blocks in the current segment, then request a refill larger
   than the free-list has) that forces exactly this interleaving, and assert
   every resulting `alloc_zeroed` (via whichever channel finally consumes
   the block — magazine hit in a stage-2 build) reads correctly zero
   regardless of which producer (drain vs carve) supplied it, with the
   `SMALL_ZERO_PASS_CALLS` delta matching exactly the count of
   drain-sourced blocks in the mix (not the carve-sourced ones).

5. **`decommit_retain_path_clears_virgin_bit`** — a test-only hook
   (`#[doc(hidden)]`, mirroring the established test-only-forwarder pattern)
   that forces the `release_follows=false` leg to run (currently
   unreachable via any production call site, §4.4(b)) and asserts
   `payload_virgin` reads `false` immediately after, and that a subsequent
   carve at the same offset does NOT skip the zero pass. This is the
   regression guard R9-5 flagged as "the correct guard, noted for Stage 1"
   but did not itself specify in test form — added here per this task's
   explicit "test plan for whatever future stage-2 session implements this"
   requirement.

6. **`miri_gated_small_virgin_skip_withheld`** — inline `cfg(miri)` assertion
   folded into test 1 (mirroring `tests/alloc_zeroed_fresh_large_skip.rs`'s
   own inline miri branch), with a `cfg(miri)` buffer-size shrink for the
   full-buffer byte-by-byte read-back check (miri's per-byte-tracked
   execution is slow at full block sizes).

**Non-negotiable acceptance bar for stage-2**: test 2 (the poison
counterfactual) and test 5 (the decommit-retain regression guard) must both
be green AND must be shown to go RED when the relevant guard is reverted
(the "counterfactual proof" discipline this session's CLAUDE.md mandates
for every phase) before any stage-2 code is considered for `production`.

---

## 8. Kill-gate / verdict

| # | Criterion | Target | Finding (this session, independently re-verified) | Verdict |
|---|---|---|---|---|
| K1 | Formal invariant stated at testable-predicate granularity | precise, block/carve-level | §2's `is_virgin(segment, O, C)` four-conjunct predicate, each conjunct independently justified against current source | **PASS** |
| K2 | Pooling (R8-3/R8-10): can a pooled/reused block be marked virgin? | provably no | Dispatch conjunct is false for every pool-reused block (served via `pop_free`); `carve_block`/`carve_batch` structurally never run on a pooled segment (§2, §4 R9-5-cross-check) | **PASS** |
| K3 | Lazy-commit incremental grow-on-carve: is every commit call site preceding a bump advance zero-guaranteed? | provably yes | Grow-on-carve is first-time `os::commit_pages` on never-before-committed pages (demand-zero on Windows, the only cfg this branch compiles under); recommit-on-`is_decommitted` is dead in production (§4.4) | **PASS** |
| K4 | Release-vs-decommit-in-place (macOS `MADV_DONTNEED` crux): can a decommit-reused block be marked virgin? | provably no | `decommit_empty_segment_impl(release_follows=false)` has ZERO production callers, re-confirmed by grep this session on the CURRENT tree (not merely re-cited); design sets the bit `false` defensively at that site regardless, so a future re-introduction fails safe | **PASS** |
| K5 | Batched carve (`carve_batch`): can a non-virgin block slip into a virgin-marked batch? | provably no | Bump monotonic within one lifetime → every block in one `carve_batch` run shares one lifetime-bit read; no intra-run transition possible (§2, §4.2) | **PASS** |
| K6 | Magazine refill interleaving (task-specific risk this session's brief raised): can a per-CALL virgin signal misattribute a drained (non-virgin) block as carved (virgin)? | provably no, with per-run (not per-call) granularity | `refill_class_bump_impl` traced line-by-line this session: `drain_freelist_batch` and `carve_batch` calls are separately identifiable producer spans within `out`; per-span tagging (not per-call) is required and sufficient (§4.2, §6 point 3) | **PASS** |
| K7 | Cross-thread frees (ring / `HeapOverflow`): can a remote free race, corrupt, or elevate the bit? | provably no | Ring `push` carries only a `u32` offset, never touches block bytes; `reclaim_offset(_checked)`'s `write_next` only ever targets already-non-virgin (previously-carved) blocks; bit is owner-only, same single-writer discipline as `bump` (§4.5) | **PASS** |
| K8 | `hardened` generation-bump: does it write into the caller-observable payload region, or disjoint metadata? | must be a precise answer, not a guess | `bump_gen` writes `Layout::gen_table_off() + idx` — a fixed metadata-region byte, structurally disjoint from `payload_start`-relative user bytes; additionally only reachable via the non-magazine `pop_free` dispatch leg, itself already non-virgin (§4.6) | **PASS** |
| K9 | Miri safety: does the design withhold the signal under `cfg!(miri)`, and does Small share Large's exact aperture? | yes, mirroring R9-1, verified not assumed | `reserve_small_segment` and `alloc_large_slow` both call the SAME `Segment::reserve` (`os.rs:146`) → same `aligned_vmem` miri-fallback aperture; combined predicate ANDs `cfg!(not(miri))` at both the bit-set site and the read site (§5) | **PASS** |
| K10 | Prior NO-GO history properly reconciled (2026-07-10 P4(b), 2026-07-19 deep-audit P2-3)? | yes, point-by-point, re-derived not merely cited | §0/§7-below: R9-5 already did this once; this document independently re-derives the SAME conclusions from source rather than trusting R9-5's account, and finds no drift | **PASS** |
| K11 | Production win reachable without a large additional surface? | assessed honestly | **NO** — `HeapCore::alloc_zeroed`'s small arm (`heap_core_alloc.rs:325-333`, reconfirmed current) still delegates to `self.alloc` + unconditional `Node::zero`, bypassing `AllocCore::alloc_zeroed` entirely. A substrate-only prototype (mirroring the Large path's test layer) is fully testable but would benefit ZERO production callers under `production`/`fastbin` builds — the magazine plumbing (§4.2's per-run tagging into `PerClass.slots`) is the real remaining surface, with its own open storage-design question (parallel `[bool; TCACHE_CAP]` vs a whole-`PerClass` short-circuit bit vs a stolen pointer tag bit — R9-5 §11 sketches three candidates, unresolved) | **CONDITIONAL — see verdict** |
| K12 | Win narrowness — does the skip help general churn or only a narrow pattern? | assessed honestly | Skip benefits ONLY the genuinely-first-touch, never-reused `alloc_zeroed` call (cold-start / calloc-burst / append-only-zeroed-buffer patterns). Steady-state churn (the pattern this session's other R11 perf work targets) reuses blocks via `pop_free`/magazine and gets **zero** benefit by the dispatch conjunct's own construction — this is not a flaw in the design, it is the design's honest scope, matching the ALREADY-shipped Large-path skip's identical narrowness (Large's skip has the same "only genuinely fresh, never-reused" scope, and it shipped anyway because large-object churn is common in the target workloads; small-object churn dominating the actual hot benchmarks makes the small-path ceiling narrower in relative terms) | **ACKNOWLEDGED, narrow** |

### Verdict: **DESIGN-ONLY. CONDITIONAL GO for staged implementation on a future task. NO-GO for a same-session (or immediate next-session) prototype.**

All eleven correctness/soundness criteria (K1-K10) **PASS** — the invariant
is sound, every risk area the task brief named is closed with current-tree
evidence, the miri gate transfers structurally from the proven Large-path
precedent, and the prior NO-GO history is honestly reconciled rather than
re-litigated. This mirrors R9-5's independent conclusion exactly, arrived
at via fresh verification rather than trust in that document.

The verdict is **CONDITIONAL, not unconditional GO**, for two reasons
or​thogonal to correctness (K11, K12), matching this session's established
honesty norm (R10-2's NO-GO, R11-7's downgrade):

1. **A substrate-only prototype (the cheapest thing to build) is
   production-inert.** `HeapCore::alloc_zeroed` — the ONLY path
   `SeferAlloc::alloc_zeroed`/`GlobalAlloc::alloc_zeroed` actually reaches
   under any realistic build (`production` implies `fastbin`) — does not
   call `AllocCore::alloc_zeroed` at all for small classes. Landing a
   substrate-only change would produce a fully green, fully tested feature
   that helps no real caller — a shape this session's CLAUDE.md phased-
   delivery discipline would flag as incomplete work, not a completed
   phase. The REAL implementation work is the magazine-plumbing surface
   (Stage 2 in R9-5's staging, unchanged by this document), which has its
   own genuinely open storage-design question not yet resolved by either
   report.
2. **The win is real but narrow** (K12) — a cold/calloc-first-touch ceiling
   analytically estimated (R9-5 §8, not re-derived here since no new
   measurement was performed this session either) at ~130 ns (4 KiB) to
   ~70-90 µs (1 MiB) per genuinely-virgin call, and exactly zero on the
   steady-state churn patterns this session's OTHER R11 work (R11-4
   batched dealloc, R11-6 NUMA directory, R11-7 medium page-run layer) is
   busy optimizing. It is not a wasted optimization, but it is not the
   highest-leverage remaining item either.

**What would flip this to unconditional GO:** (i) a dedicated cold-first-
touch `alloc_zeroed` criterion bench (R9-5 §11 Stage 0) proving the memset
cost is non-trivial on THIS host under THIS toolchain, replacing the
current analytical-only estimate with a measured one; (ii) a resolved
magazine-plumbing storage design (one of R9-5's three candidates, actually
chosen and justified) so a prototype would be production-relevant on the
first pass rather than requiring a second stage; (iii) the mandatory
poison-counterfactual test (§7 test 2 here) shown red-before/green-after on
the actual implementation, not merely specified.

---

## 9. Explicitly NOT done this session

- **No `src/` change.** No `payload_virgin` field added to `SegmentHeader`;
  no `carve_block`/`carve_batch`/`alloc_small`/`alloc_zeroed` signature
  touched; no `SMALL_ZERO_PASS_CALLS` counter added; no new feature flag
  added to `Cargo.toml`.
- **No `tests/` change.** §7's test plan is specified, ready to lift when
  a future stage-2 task lands the implementation; none of the six tests
  exist as files yet.
- **No `Cargo.toml` change.**
- **No miri run.** Verified by reading the `cfg!(miri)` gate structure
  (§5), which mirrors the already-shipped, already-validated Large-path
  gate exactly — no new miri-specific reasoning was needed or invented.
- **No benchmark run.** §8's win estimate is inherited from R9-5's
  analytical figures (memset-bandwidth-derived), not re-measured — Stage 0
  of R9-5's staged plan (a dedicated cold-`alloc_zeroed` criterion bench)
  remains the correct place to turn this into a measured number, and
  remains undone.
- **No commit, no push, no `git add`.** Per this task's explicit
  constraint — the orchestrator reviews any future diff.
